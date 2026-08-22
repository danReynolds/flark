import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

Future<void> main(List<String> arguments) async {
  final options = _Options.parse(arguments);
  final script = File.fromUri(Platform.script).absolute;
  final packageRoot = script.parent.parent;
  final outputRoot = Directory(
    options.value('output') ?? '${packageRoot.path}/results',
  ).absolute;
  outputRoot.createSync(recursive: true);

  final workload = options.value('workload') ?? 'cold-open';
  final targetBytes = int.parse(options.value('bytes') ?? '1048576');
  final location =
      options.value('location') ??
      (workload == 'sustained-typing' ? 'end' : 'middle');
  final defaults = switch (workload) {
    'cold-open' => (warmups: 0, samples: 1, cadence: 0),
    'sustained-typing' => (warmups: 20, samples: 200, cadence: 100),
    'local-insert-delete' => (warmups: 10, samples: 100, cadence: 0),
    'paste-32kib' => (warmups: 2, samples: 20, cadence: 0),
    _ => throw ArgumentError.value(workload, '--workload'),
  };
  final warmups = int.parse(options.value('warmups') ?? '${defaults.warmups}');
  final samples = int.parse(options.value('samples') ?? '${defaults.samples}');
  final cadenceMillis = int.parse(
    options.value('cadence-ms') ?? '${defaults.cadence}',
  );
  final pasteBytes = int.parse(options.value('paste-bytes') ?? '32768');
  final progressTimeoutSeconds = int.parse(
    options.value('timeout-seconds') ?? '60',
  );
  final appTimeoutSeconds = int.parse(
    options.value('app-timeout-seconds') ?? '$progressTimeoutSeconds',
  );
  final idleSeconds = int.parse(options.value('idle-seconds') ?? '0');
  final runId =
      options.value('run-id') ??
      'supereditor-$workload-$targetBytes-$location-'
          '${DateTime.now().toUtc().microsecondsSinceEpoch}';
  final runDirectory = Directory('${outputRoot.path}/$runId')
    ..createSync(recursive: true);
  final flutter = await _resolveFlutter(options.value('flutter'));

  if (!options.flag('no-build')) {
    await _runInherited(flutter, [
      'build',
      'macos',
      '--profile',
      '-t',
      'lib/competitor_profile_harness.dart',
      '--dart-define=COMPETITOR_PROTOCOL_ID=m0-mac-competitor-profile-v1',
    ], workingDirectory: packageRoot.path);
  }

  final app = Directory(
    '${packageRoot.path}/build/macos/Build/Products/Profile/'
    'flark_peer_supereditor.app',
  );
  final executable = File('${app.path}/Contents/MacOS/flark_peer_supereditor');
  if (!executable.existsSync()) {
    throw StateError('Profile executable does not exist: ${executable.path}');
  }

  final host = await _hostProvenance();
  final build = await _buildProvenance(
    packageRoot: packageRoot,
    flutter: flutter,
    app: app,
    runDirectory: runDirectory,
  );
  final startedAtUtc = DateTime.now().toUtc();
  final invocation = <String, Object?>{
    'driverArgv': [
      Platform.executable,
      Platform.script.toFilePath(),
      ...arguments,
    ],
    'childArgv': [executable.path],
    'cwd': packageRoot.path,
    'runId': runId,
    'runGroupId': options.value('run-group-id'),
    'orderSlot': options.value('order-slot'),
    'progressTimeoutSeconds': progressTimeoutSeconds,
    'appTimeoutSeconds': appTimeoutSeconds,
    'idleSeconds': idleSeconds,
    'startedAtUtc': startedAtUtc.toIso8601String(),
  };

  final environment = Map<String, String>.from(Platform.environment)
    ..addAll({
      'COMPETITOR_WORKLOAD': workload,
      'COMPETITOR_LOCATION': location,
      'COMPETITOR_SIZE_BYTES': '$targetBytes',
      'COMPETITOR_WARMUP_COUNT': '$warmups',
      'COMPETITOR_SAMPLE_COUNT': '$samples',
      'COMPETITOR_CADENCE_MILLIS': '$cadenceMillis',
      'COMPETITOR_PASTE_BYTES': '$pasteBytes',
      'COMPETITOR_TIMEOUT_SECONDS': '$appTimeoutSeconds',
      'COMPETITOR_RUN_ID': runId,
      'COMPETITOR_OUTPUT_DIRECTORY': runDirectory.path,
      'COMPETITOR_HOST_PROVENANCE_JSON': jsonEncode(host),
      'COMPETITOR_BUILD_PROVENANCE_JSON': jsonEncode(build),
      'COMPETITOR_INVOCATION_JSON': jsonEncode(invocation),
    });

  final idleStartedAtUtc = DateTime.now().toUtc();
  if (idleSeconds > 0) {
    stdout.writeln(
      'COMPETITOR_IDLE_START seconds=$idleSeconds '
      'utc=${idleStartedAtUtc.toIso8601String()}',
    );
    await Future<void>.delayed(Duration(seconds: idleSeconds));
  }
  final idleFinishedAtUtc = DateTime.now().toUtc();
  final processLaunchRequestedAtUtc = DateTime.now().toUtc();
  invocation['processLaunchRequestedAtUtc'] = processLaunchRequestedAtUtc
      .toIso8601String();
  environment['COMPETITOR_INVOCATION_JSON'] = jsonEncode(invocation);

  final stdoutFile = File('${runDirectory.path}/$runId.stdout.log');
  final stderrFile = File('${runDirectory.path}/$runId.stderr.log');
  final stdoutSink = stdoutFile.openWrite();
  final stderrSink = stderrFile.openWrite();
  final stdoutText = StringBuffer();
  var lastProgress = DateTime.now();
  var watchdogTimedOut = false;

  final process = await Process.start(
    executable.path,
    const [],
    workingDirectory: packageRoot.path,
    environment: environment,
  );
  final stdoutDone = process.stdout.listen((bytes) {
    lastProgress = DateTime.now();
    stdoutSink.add(bytes);
    stdout.add(bytes);
    stdoutText.write(utf8.decode(bytes, allowMalformed: true));
  }).asFuture<void>();
  final stderrDone = process.stderr.listen((bytes) {
    lastProgress = DateTime.now();
    stderrSink.add(bytes);
    stderr.add(bytes);
  }).asFuture<void>();

  int? processExitCode;
  unawaited(process.exitCode.then((value) => processExitCode = value));
  while (processExitCode == null) {
    await Future<void>.delayed(const Duration(milliseconds: 250));
    if (DateTime.now().difference(lastProgress).inSeconds >=
        progressTimeoutSeconds) {
      watchdogTimedOut = true;
      process.kill(ProcessSignal.sigterm);
      await Future<void>.delayed(const Duration(seconds: 2));
      if (processExitCode == null) process.kill(ProcessSignal.sigkill);
      processExitCode = await process.exitCode;
      break;
    }
  }
  await Future.wait([stdoutDone, stderrDone]);
  await stdoutSink.flush();
  await stderrSink.flush();
  await stdoutSink.close();
  await stderrSink.close();

  final marker = RegExp(
    r'COMPETITOR_RESULT_JSON=([^\r\n]+)',
  ).allMatches(stdoutText.toString()).lastOrNull?.group(1);
  final finishedAtUtc = DateTime.now().toUtc();
  final driverEvidence = <String, Object?>{
    'schemaVersion': 1,
    'watchdogKind': 'no-output-progress',
    'watchdogTimedOut': watchdogTimedOut,
    'progressTimeoutSeconds': progressTimeoutSeconds,
    'processExitCode': processExitCode,
    'processId': process.pid,
    'processLaunchRequestedAtUtc': processLaunchRequestedAtUtc
        .toIso8601String(),
    'startedAtUtc': startedAtUtc.toIso8601String(),
    'finishedAtUtc': finishedAtUtc.toIso8601String(),
    'stdout': {
      'path': stdoutFile.path,
      'sha256': await _sha256File(stdoutFile),
    },
    'stderr': {
      'path': stderrFile.path,
      'sha256': await _sha256File(stderrFile),
    },
    'host': host,
    'build': build,
    'invocation': invocation,
    'runControl': {
      'idleSecondsRequested': idleSeconds,
      'idleSecondsObserved': idleFinishedAtUtc
          .difference(idleStartedAtUtc)
          .inSeconds,
      'idleStartedAtUtc': idleStartedAtUtc.toIso8601String(),
      'idleFinishedAtUtc': idleFinishedAtUtc.toIso8601String(),
      'runGroupId': options.value('run-group-id'),
      'orderSlot': options.value('order-slot'),
      'exclusiveMachineUse': 'coordinator-attested-by-nonempty-run-group-id',
    },
  };

  File finalReceipt;
  if (marker != null && File(marker).existsSync()) {
    finalReceipt = await _retainAppReceipt(
      sourceReceipt: File(marker),
      runDirectory: runDirectory,
      runId: runId,
      driverEvidence: driverEvidence,
    );
  } else {
    finalReceipt = File('${runDirectory.path}/$runId.result.json');
    await finalReceipt.writeAsString(
      const JsonEncoder.withIndent('  ').convert({
        'schemaVersion': 1,
        'receiptKind': 'competitor-profile-driver-failure',
        'peer': 'super_editor',
        'config': {
          'protocolId': 'm0-mac-competitor-profile-v1',
          'workload': workload,
          'location': location,
          'targetBytes': targetBytes,
          'warmupCount': warmups,
          'sampleCount': samples,
          'cadenceMillis': cadenceMillis,
          'pasteBytes': pasteBytes,
          'appTimeoutSeconds': appTimeoutSeconds,
          'runId': runId,
        },
        'completion': watchdogTimedOut ? 'timeout' : 'failed-before-receipt',
        'protocolConformant': false,
        'claimEligible': false,
        'fidelity': {
          'pass': false,
          'reason': 'No final export receipt was produced.',
        },
        'driver': driverEvidence,
      }),
      flush: true,
    );
  }

  stdout.writeln('COMPETITOR_RETAINED_RESULT=${finalReceipt.path}');
  if (watchdogTimedOut || processExitCode != 0) exitCode = 2;
}

Future<File> _retainAppReceipt({
  required File sourceReceipt,
  required Directory runDirectory,
  required String runId,
  required Map<String, Object?> driverEvidence,
}) async {
  final sourceDirectory = sourceReceipt.parent;
  final retainedDirectory = Directory('${runDirectory.path}/artifacts')
    ..createSync(recursive: true);
  for (final entity in sourceDirectory.listSync(recursive: true)) {
    if (entity is! File) continue;
    final relative = entity.path.substring(sourceDirectory.path.length + 1);
    final destination = File('${retainedDirectory.path}/$relative');
    destination.parent.createSync(recursive: true);
    await entity.openRead().pipe(destination.openWrite());
  }

  final retainedSource = File(
    '${retainedDirectory.path}/${sourceReceipt.uri.pathSegments.last}',
  );
  final receipt = (jsonDecode(await retainedSource.readAsString()) as Map)
      .cast<String, Object?>();
  final artifacts = (receipt['artifacts'] as Map).cast<String, Object?>();
  artifacts['artifactDirectory'] = retainedDirectory.path;
  for (final key in ['rawTimeline', 'finalExport']) {
    final value = artifacts[key];
    if (value is! Map) continue;
    final artifact = value.cast<String, Object?>();
    final oldPath = artifact['path'];
    if (oldPath is String) {
      artifact['path'] =
          '${retainedDirectory.path}/${File(oldPath).uri.pathSegments.last}';
    }
  }
  artifacts['stdout'] = (driverEvidence['stdout'] as Map)
      .cast<String, Object?>();
  artifacts['stderr'] = (driverEvidence['stderr'] as Map)
      .cast<String, Object?>();
  receipt['driver'] = driverEvidence;
  final host = (driverEvidence['host'] as Map).cast<String, Object?>();
  final power = (host['power'] as Map).cast<String, Object?>();
  final runControl = (driverEvidence['runControl'] as Map)
      .cast<String, Object?>();
  final localChecksPass =
      receipt['completion'] == 'complete' &&
      receipt['protocolConformant'] == true &&
      driverEvidence['watchdogTimedOut'] == false &&
      (driverEvidence['build'] as Map)['dependencyTreeClean'] == true &&
      power['pluggedIntoAc'] == true &&
      power['lowPowerModeOff'] == true &&
      (runControl['idleSecondsObserved'] as int) >= 300 &&
      '${runControl['runGroupId'] ?? ''}'.isNotEmpty &&
      '${runControl['orderSlot'] ?? ''}'.isNotEmpty;
  // This process cannot prove the suite-wide Latin-square ordering, peer
  // rotation, or exclusive-machine interval. A locally successful sample is
  // therefore never itself claim-eligible; the M0 suite coordinator must
  // promote a complete run group after validating those controls.
  receipt['claimEligible'] = false;
  receipt['claimEligibility'] = {
    'localChecksPass': localChecksPass,
    'requiresSuiteCoordinator': true,
    'reason':
        'A peer-local process cannot attest suite-wide rotation and exclusive '
        'machine controls.',
  };

  final finalReceipt = File('${runDirectory.path}/$runId.result.json');
  await finalReceipt.writeAsString(
    const JsonEncoder.withIndent('  ').convert(receipt),
    flush: true,
  );
  return finalReceipt;
}

Future<Map<String, Object?>> _hostProvenance() async {
  final pmsetBattery = await _capture('/usr/bin/pmset', ['-g', 'batt']);
  final pmsetCustom = await _capture('/usr/bin/pmset', ['-g', 'custom']);
  final pmsetThermal = await _capture('/usr/bin/pmset', ['-g', 'therm']);
  final batteryText = '${pmsetBattery['stdout']}';
  final customText = '${pmsetCustom['stdout']}';
  return {
    'osProductVersion': await _captureValue('/usr/bin/sw_vers', [
      '-productVersion',
    ]),
    'osBuildVersion': await _captureValue('/usr/bin/sw_vers', [
      '-buildVersion',
    ]),
    'architecture': await _captureValue('/usr/bin/uname', ['-m']),
    'machineModel': await _captureValue('/usr/sbin/sysctl', ['-n', 'hw.model']),
    'cpu': await _captureValue('/usr/sbin/sysctl', [
      '-n',
      'machdep.cpu.brand_string',
    ]),
    'physicalCores': int.tryParse(
      await _captureValue('/usr/sbin/sysctl', ['-n', 'hw.physicalcpu']),
    ),
    'logicalCores': int.tryParse(
      await _captureValue('/usr/sbin/sysctl', ['-n', 'hw.logicalcpu']),
    ),
    'memoryBytes': int.tryParse(
      await _captureValue('/usr/sbin/sysctl', ['-n', 'hw.memsize']),
    ),
    'xcode': await _capture('/usr/bin/xcodebuild', ['-version']),
    'power': {
      'battery': pmsetBattery,
      'custom': pmsetCustom,
      'pluggedIntoAc': batteryText.contains('AC Power'),
      'lowPowerModeOff': RegExp(
        r'lowpowermode\s+0',
        caseSensitive: false,
      ).hasMatch(customText),
    },
    'thermal': pmsetThermal,
    'omittedIdentifiers': ['serialNumber', 'hardwareUuid'],
  };
}

Future<Map<String, Object?>> _buildProvenance({
  required Directory packageRoot,
  required String flutter,
  required Directory app,
  required Directory runDirectory,
}) async {
  final flutterVersion = await _capture(flutter, ['--version', '--machine']);
  final packageConfig =
      jsonDecode(
            await File(
              '${packageRoot.path}/.dart_tool/package_config.json',
            ).readAsString(),
          )
          as Map<String, Object?>;
  final packages = packageConfig['packages'] as List<Object?>;
  final superEditor = packages.cast<Map<String, Object?>>().firstWhere(
    (package) => package['name'] == 'super_editor',
  );
  final dependencyRoot = Directory.fromUri(
    Uri.parse(superEditor['rootUri']! as String),
  );
  final gitStatus = await _capture('/usr/bin/git', [
    '-C',
    dependencyRoot.path,
    'status',
    '--porcelain=v1',
  ]);
  final dependencyTreeClean = '${gitStatus['stdout']}'.trim().isEmpty;
  Map<String, Object?>? compatibilityPatch;
  if (!dependencyTreeClean) {
    final diff = await Process.run('/usr/bin/git', [
      '-C',
      dependencyRoot.path,
      'diff',
      '--binary',
    ]);
    final patch = File('${runDirectory.path}/super_editor_compatibility.patch');
    await patch.writeAsString('${diff.stdout}', flush: true);
    compatibilityPatch = {
      'path': patch.path,
      'sha256': await _sha256File(patch),
    };
  }

  Future<Map<String, Object?>> hash(String relative) async {
    final file = File('${packageRoot.path}/$relative');
    return {
      'path': relative,
      'sha256': await _sha256File(file),
      'bytes': file.lengthSync(),
    };
  }

  return {
    'flutter': flutterVersion,
    'dartRuntime': Platform.version,
    'profileApplication': {
      'path': app.path,
      'treeSha256': await _sha256Tree(app),
    },
    'runner': await hash('lib/competitor_profile_harness.dart'),
    'nativeInputBridge': await hash('macos/Runner/MainFlutterWindow.swift'),
    'fixtureGenerator': await hash('lib/src/competitor_fixture.dart'),
    'pubspecLock': await hash('pubspec.lock'),
    'resolvedDependencies': await hash(
      'evidence/resolved_dependencies_compact.txt',
    ),
    'dependencySourceRevision': await _captureValue('/usr/bin/git', [
      '-C',
      dependencyRoot.path,
      'rev-parse',
      'HEAD',
    ]),
    'dependencySourceTree': await _captureValue('/usr/bin/git', [
      '-C',
      dependencyRoot.path,
      'rev-parse',
      'HEAD^{tree}',
    ]),
    'dependencyTreeClean': dependencyTreeClean,
    'dependencyGitStatus': gitStatus,
    'compatibilityPatch': compatibilityPatch,
  };
}

Future<String> _resolveFlutter(String? explicit) async {
  if (explicit != null) return File(explicit).absolute.path;
  final environmentValue = Platform.environment['FLUTTER_BIN'];
  if (environmentValue != null) return File(environmentValue).absolute.path;
  final which = await Process.run('/usr/bin/which', ['flutter']);
  if (which.exitCode == 0 && '${which.stdout}'.trim().isNotEmpty) {
    return '${which.stdout}'.trim();
  }
  throw StateError('Pass --flutter=/absolute/path/to/flutter');
}

Future<void> _runInherited(
  String executable,
  List<String> arguments, {
  required String workingDirectory,
}) async {
  final process = await Process.start(
    executable,
    arguments,
    workingDirectory: workingDirectory,
    mode: ProcessStartMode.inheritStdio,
  );
  final code = await process.exitCode;
  if (code != 0) {
    throw ProcessException(executable, arguments, 'Exited $code', code);
  }
}

Future<Map<String, Object?>> _capture(
  String executable,
  List<String> arguments,
) async {
  try {
    final result = await Process.run(executable, arguments);
    return {
      'argv': [executable, ...arguments],
      'exitCode': result.exitCode,
      'stdout': '${result.stdout}'.trim(),
      'stderr': '${result.stderr}'.trim(),
    };
  } catch (error) {
    return {
      'argv': [executable, ...arguments],
      'exitCode': null,
      'error': '$error',
    };
  }
}

Future<String> _captureValue(String executable, List<String> arguments) async {
  final result = await _capture(executable, arguments);
  return '${result['stdout'] ?? ''}'.trim();
}

Future<String> _sha256File(File file) async {
  return (await sha256.bind(file.openRead()).first).toString();
}

Future<String> _sha256Tree(Directory directory) async {
  final files =
      directory
          .listSync(recursive: true, followLinks: false)
          .whereType<File>()
          .toList()
        ..sort((left, right) => left.path.compareTo(right.path));
  final manifest = StringBuffer();
  for (final file in files) {
    final relative = file.path.substring(directory.path.length + 1);
    manifest
      ..write(relative)
      ..write('\t')
      ..write(file.lengthSync())
      ..write('\t')
      ..writeln(await _sha256File(file));
  }
  return sha256.convert(utf8.encode(manifest.toString())).toString();
}

final class _Options {
  _Options(this.values, this.flags);

  factory _Options.parse(List<String> arguments) {
    final values = <String, String>{};
    final flags = <String>{};
    for (final argument in arguments) {
      if (!argument.startsWith('--')) {
        throw ArgumentError.value(argument, 'argument', 'expected --key=value');
      }
      final body = argument.substring(2);
      final equals = body.indexOf('=');
      if (equals < 0) {
        flags.add(body);
      } else {
        values[body.substring(0, equals)] = body.substring(equals + 1);
      }
    }
    return _Options(values, flags);
  }

  final Map<String, String> values;
  final Set<String> flags;

  String? value(String key) => values[key];
  bool flag(String key) => flags.contains(key);
}

extension<T> on Iterable<T> {
  T? get lastOrNull {
    if (isEmpty) return null;
    return last;
  }
}
