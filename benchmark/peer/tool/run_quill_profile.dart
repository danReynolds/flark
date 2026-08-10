import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

import 'package:flark_peer_benchmark/profile_evidence.dart';
import 'package:flark_peer_benchmark/profile_fixture.dart';

const _profileTarget = 'lib/competitor_profile_harness.dart';
const _profileExecutable =
    'build/macos/Build/Products/Profile/'
    'flark_peer_benchmark.app/Contents/MacOS/flark_peer_benchmark';

Future<void> main(List<String> arguments) async {
  final options = _Options.parse(arguments);
  final packageRoot = Directory.current.absolute;
  final startedUtc = DateTime.now().toUtc();
  final invocationId = 'quill-${startedUtc.microsecondsSinceEpoch}-p$pid';
  final output = Directory(
    options.outputPath ??
        '${packageRoot.path}/artifacts/'
            'quill-profile-${startedUtc.toIso8601String().replaceAll(':', '-')}',
  );
  await output.create(recursive: true);
  final logs = Directory('${output.path}/logs')..createSync(recursive: true);
  final processes = Directory('${output.path}/process-results')
    ..createSync(recursive: true);
  final exports = Directory('${output.path}/exports')
    ..createSync(recursive: true);

  final commands = <Map<String, Object?>>[];
  if (!options.skipBuild) {
    await _runRequired(
      executable: 'flutter',
      arguments: const ['pub', 'get'],
      cwd: packageRoot.path,
      stdoutPath: '${logs.path}/flutter-pub-get.stdout.log',
      stderrPath: '${logs.path}/flutter-pub-get.stderr.log',
      commands: commands,
    );
    await _runRequired(
      executable: 'flutter',
      arguments: const [
        'build',
        'macos',
        '--profile',
        '-t',
        _profileTarget,
        '--dart-define=COMPETITOR_PROTOCOL_ID=$competitorProtocolId',
      ],
      cwd: packageRoot.path,
      stdoutPath: '${logs.path}/flutter-build.stdout.log',
      stderrPath: '${logs.path}/flutter-build.stderr.log',
      commands: commands,
    );
  }
  final executable = File('${packageRoot.path}/$_profileExecutable');
  if (!await executable.exists()) {
    throw StateError('Profile executable does not exist: ${executable.path}');
  }

  final deps = await _runRequired(
    executable: 'flutter',
    arguments: const ['pub', 'deps', '--style=compact'],
    cwd: packageRoot.path,
    stdoutPath: '${output.path}/flutter-pub-deps.txt',
    stderrPath: '${logs.path}/flutter-pub-deps.stderr.log',
    commands: commands,
  );
  final provenance = await _collectProvenance(
    packageRoot: packageRoot,
    output: output,
    executable: executable,
    depsExitCode: deps,
    commands: commands,
  );

  final plan = options.smoke ? _smokePlan() : _protocolPlan();
  final results = <Map<String, Object?>>[];
  for (var orderIndex = 0; orderIndex < plan.length; orderIndex += 1) {
    final run = plan[orderIndex];
    final id = run.id(orderIndex);
    final processRunId = '$invocationId-$id';
    stdout.writeln(
      '[$id] ${run.scenario} ${run.targetBytes} bytes ${run.location}',
    );
    final resultPath = '${processes.path}/$processRunId.json';
    final exportPath = exportArtifactPath(
      exportDirectory: exports.path,
      processRunId: processRunId,
    );
    final environment = <String, String>{
      'COMPETITOR_SCENARIO': run.scenario,
      'COMPETITOR_TARGET_BYTES': '${run.targetBytes}',
      'COMPETITOR_LOCATION': run.location,
      'COMPETITOR_RUN_INDEX': '${run.runIndex}',
      'COMPETITOR_ORDER_INDEX': '$orderIndex',
      'COMPETITOR_PROCESS_RUN_ID': processRunId,
      'COMPETITOR_OUTPUT_PATH': resultPath,
      'COMPETITOR_EXPORT_PATH': exportPath,
      if (options.smoke) ...const {
        'COMPETITOR_NONCLAIM_RUN': '1',
        'COMPETITOR_TYPING_WARMUPS': '1',
        'COMPETITOR_TYPING_SAMPLES': '3',
        'COMPETITOR_LOCAL_WARMUP_PAIRS': '1',
        'COMPETITOR_LOCAL_SAMPLE_PAIRS': '2',
        'COMPETITOR_PASTE_WARMUPS': '1',
        'COMPETITOR_PASTE_SAMPLES': '2',
        'COMPETITOR_INPUT_TIMEOUT_SECONDS': '5',
      },
    };
    final processResult = await _runProfileProcess(
      executable: executable.path,
      cwd: packageRoot.path,
      environment: environment,
      stdoutPath: '${logs.path}/$processRunId.stdout.log',
      stderrPath: '${logs.path}/$processRunId.stderr.log',
      timeout: run.processTimeout(smoke: options.smoke),
    );
    Map<String, Object?>? payload;
    final resultFile = File(resultPath);
    if (await resultFile.exists()) {
      payload = (jsonDecode(await resultFile.readAsString()) as Map).map(
        (key, value) => MapEntry('$key', value),
      );
    }
    results.add(<String, Object?>{
      'id': id,
      'processRunId': processRunId,
      'argv': <String>[executable.path],
      'cwd': packageRoot.path,
      'environmentOverrides': environment,
      'startedUtc': processResult.startedUtc.toIso8601String(),
      'completedUtc': processResult.completedUtc.toIso8601String(),
      'exitCode': processResult.exitCode,
      'timedOut': processResult.timedOut,
      'processTimeoutSeconds': run
          .processTimeout(smoke: options.smoke)
          .inSeconds,
      'stdoutPath': processResult.stdoutPath,
      'stdoutSha256': await _sha256File(File(processResult.stdoutPath)),
      'stderrPath': processResult.stderrPath,
      'stderrSha256': await _sha256File(File(processResult.stderrPath)),
      'resultPath': resultPath,
      'resultSha256': await resultFile.exists()
          ? await _sha256File(resultFile)
          : null,
      'exportPath': exportPath,
      'exportSha256': await File(exportPath).exists()
          ? await _sha256File(File(exportPath))
          : null,
      'payload': payload,
    });
  }

  final failures = results
      .where(
        (result) =>
            result['exitCode'] != 0 ||
            result['timedOut'] == true ||
            result['payload'] == null,
      )
      .toList();
  final processCompletionEligibleCount = results.where((result) {
    final payload = result['payload'];
    return payload is Map && payload['completionEnvelopeEligible'] == true;
  }).length;
  final completionEnvelope = evaluateAggregateCompletionEnvelope(
    protocolInvocation: !options.smoke,
    plannedProcessCount: plan.length,
    completedProcessCount: results
        .where((result) => result['payload'] != null)
        .length,
    eligibleProcessCount: processCompletionEligibleCount,
    failedProcessCount: failures.length,
  );
  final performanceClaim = localPerformanceClaimEligibility(scope: 'aggregate');
  final receipt = <String, Object?>{
    'schemaVersion': 1,
    'receiptId': options.smoke
        ? 'm0-quill-profile-smoke-nonclaim'
        : 'm0-quill-mac-competitor-profile-v1',
    'peer': 'flutter_quill',
    'protocolId': competitorProtocolId,
    'invocationId': invocationId,
    'mode': options.smoke ? 'nonclaim-smoke' : 'profile-protocol',
    'completionEnvelopeEligible': completionEnvelope.eligible,
    'completionEnvelopeBlockers': completionEnvelope.blockers,
    'performanceClaimEligible': performanceClaim.eligible,
    'performanceClaimBlockers': performanceClaim.blockers,
    'cohortPerformanceEligibility': const <String, Object?>{
      'assessed': false,
      'eligible': null,
      'reason': 'This receipt contains Quill only, not the two-peer cohort.',
    },
    // Backward-compatible alias for performance-claim eligibility only.
    'claimEligible': false,
    'claimBlockers': performanceClaim.blockers,
    'startedUtc': startedUtc.toIso8601String(),
    'completedUtc': DateTime.now().toUtc().toIso8601String(),
    'runCount': results.length,
    'failureCount': failures.length,
    'processCompletionEnvelopeEligibleCount': processCompletionEligibleCount,
    'runPlan': plan.map((run) => run.toJson()).toList(),
    'provenance': provenance,
    'commands': commands,
    'results': results,
  };
  final receiptFile = File('${output.path}/receipt.json');
  await receiptFile.writeAsString(
    '${const JsonEncoder.withIndent('  ').convert(receipt)}\n',
    flush: true,
  );
  stdout.writeln('Receipt: ${receiptFile.path}');
  if (failures.isNotEmpty || (!options.smoke && !completionEnvelope.eligible)) {
    exitCode = 1;
  }
}

List<_Run> _smokePlan() => const [
  _Run(scenario: 'cold-open', targetBytes: 1024),
  _Run(scenario: 'sustained-typing', targetBytes: 1024),
  _Run(scenario: 'local-insert-delete', targetBytes: 1024),
  _Run(scenario: 'paste-32kib', targetBytes: 1024),
];

List<_Run> _protocolPlan() {
  const sizes = [1048576, 5242880, 10485760];
  final runs = <_Run>[];
  for (var replicate = 0; replicate < 30; replicate += 1) {
    for (var offset = 0; offset < sizes.length; offset += 1) {
      final size = sizes[(replicate + offset) % sizes.length];
      runs.add(
        _Run(scenario: 'cold-open', targetBytes: size, runIndex: replicate),
      );
    }
  }
  for (var replicate = 0; replicate < 3; replicate += 1) {
    for (var offset = 0; offset < sizes.length; offset += 1) {
      final size = sizes[(replicate + offset) % sizes.length];
      runs.add(
        _Run(
          scenario: 'sustained-typing',
          targetBytes: size,
          runIndex: replicate,
        ),
      );
    }
  }
  for (final scenario in const ['local-insert-delete', 'paste-32kib']) {
    for (var locationIndex = 0; locationIndex < 3; locationIndex += 1) {
      final location = const ['start', 'middle', 'end'][locationIndex];
      for (var offset = 0; offset < sizes.length; offset += 1) {
        final size = sizes[(locationIndex + offset) % sizes.length];
        runs.add(
          _Run(
            scenario: scenario,
            targetBytes: size,
            location: location,
            runIndex: locationIndex,
          ),
        );
      }
    }
  }
  return runs;
}

final class _Run {
  const _Run({
    required this.scenario,
    required this.targetBytes,
    this.location = 'start',
    this.runIndex = 0,
  });

  final String scenario;
  final int targetBytes;
  final String location;
  final int runIndex;

  String id(int orderIndex) =>
      '${orderIndex.toString().padLeft(3, '0')}-$scenario-'
      '${targetBytes}b-$location-r$runIndex';

  Map<String, Object?> toJson() => <String, Object?>{
    'scenario': scenario,
    'targetBytes': targetBytes,
    'location': location,
    'runIndex': runIndex,
  };

  Duration processTimeout({required bool smoke}) {
    if (smoke) return const Duration(seconds: 15);
    return switch (scenario) {
      'cold-open' => const Duration(seconds: 75),
      'sustained-typing' => const Duration(minutes: 2),
      // Each sequential measured action has its own 60-second liveness
      // timeout. The process envelope must not invalidate individually-live
      // but very slow peer results.
      'local-insert-delete' => const Duration(hours: 4),
      'paste-32kib' => const Duration(hours: 1),
      _ => throw StateError('No process timeout for $scenario'),
    };
  }
}

final class _Options {
  const _Options({
    required this.smoke,
    required this.skipBuild,
    required this.outputPath,
  });

  factory _Options.parse(List<String> arguments) {
    var smoke = false;
    var skipBuild = false;
    String? outputPath;
    for (final argument in arguments) {
      if (argument == '--smoke') {
        smoke = true;
      } else if (argument == '--skip-build') {
        skipBuild = true;
      } else if (argument.startsWith('--output=')) {
        outputPath = argument.substring('--output='.length);
      } else {
        throw FormatException('Unknown argument: $argument');
      }
    }
    return _Options(smoke: smoke, skipBuild: skipBuild, outputPath: outputPath);
  }

  final bool smoke;
  final bool skipBuild;
  final String? outputPath;
}

final class _ProcessReceipt {
  const _ProcessReceipt({
    required this.startedUtc,
    required this.completedUtc,
    required this.exitCode,
    required this.timedOut,
    required this.stdoutPath,
    required this.stderrPath,
  });

  final DateTime startedUtc;
  final DateTime completedUtc;
  final int exitCode;
  final bool timedOut;
  final String stdoutPath;
  final String stderrPath;
}

Future<_ProcessReceipt> _runProfileProcess({
  required String executable,
  required String cwd,
  required Map<String, String> environment,
  required String stdoutPath,
  required String stderrPath,
  required Duration timeout,
}) async {
  final startedUtc = DateTime.now().toUtc();
  final process = await Process.start(
    executable,
    const [],
    workingDirectory: cwd,
    environment: environment,
    includeParentEnvironment: true,
  );
  final stdoutFile = File(stdoutPath).openWrite();
  final stderrFile = File(stderrPath).openWrite();
  final stdoutDone = process.stdout.pipe(stdoutFile);
  final stderrDone = process.stderr.pipe(stderrFile);
  var timedOut = false;
  int code;
  try {
    code = await process.exitCode.timeout(timeout);
  } on TimeoutException {
    timedOut = true;
    process.kill(ProcessSignal.sigterm);
    try {
      code = await process.exitCode.timeout(const Duration(seconds: 5));
    } on TimeoutException {
      process.kill(ProcessSignal.sigkill);
      code = await process.exitCode;
    }
  }
  await Future.wait([stdoutDone, stderrDone]);
  return _ProcessReceipt(
    startedUtc: startedUtc,
    completedUtc: DateTime.now().toUtc(),
    exitCode: code,
    timedOut: timedOut,
    stdoutPath: stdoutPath,
    stderrPath: stderrPath,
  );
}

Future<int> _runRequired({
  required String executable,
  required List<String> arguments,
  required String cwd,
  required String stdoutPath,
  required String stderrPath,
  required List<Map<String, Object?>> commands,
}) async {
  final started = DateTime.now().toUtc();
  final result = await Process.run(
    executable,
    arguments,
    workingDirectory: cwd,
  );
  await File(stdoutPath).writeAsString('${result.stdout}', flush: true);
  await File(stderrPath).writeAsString('${result.stderr}', flush: true);
  commands.add(<String, Object?>{
    'argv': [executable, ...arguments],
    'cwd': cwd,
    'startedUtc': started.toIso8601String(),
    'completedUtc': DateTime.now().toUtc().toIso8601String(),
    'exitCode': result.exitCode,
    'stdoutPath': stdoutPath,
    'stderrPath': stderrPath,
  });
  if (result.exitCode != 0) {
    throw ProcessException(
      executable,
      arguments,
      'Command failed; see $stderrPath',
      result.exitCode,
    );
  }
  return result.exitCode;
}

Future<Map<String, Object?>> _collectProvenance({
  required Directory packageRoot,
  required Directory output,
  required File executable,
  required int depsExitCode,
  required List<Map<String, Object?>> commands,
}) async {
  Future<Map<String, Object?>> command(
    String executable,
    List<String> arguments,
    String label,
  ) async {
    final result = await Process.run(
      executable,
      arguments,
      workingDirectory: packageRoot.path,
    );
    final path = '${output.path}/$label.txt';
    await File(
      path,
    ).writeAsString('${result.stdout}${result.stderr}', flush: true);
    commands.add(<String, Object?>{
      'argv': [executable, ...arguments],
      'cwd': packageRoot.path,
      'exitCode': result.exitCode,
      'stdoutAndStderrPath': path,
    });
    return <String, Object?>{
      'exitCode': result.exitCode,
      'path': path,
      'sha256': await _sha256File(File(path)),
    };
  }

  final appDirectory = executable.parent.parent.parent;
  final profileTree = await _hashTree(appDirectory);
  final environmentNames = Platform.environment.keys.toList()..sort();
  return <String, Object?>{
    'profileExecutablePath': executable.path,
    'profileExecutableSha256': await _sha256File(executable),
    'profileApplicationPath': appDirectory.path,
    'profileApplicationTreeSha256': profileTree['treeSha256'],
    'profileApplicationFiles': profileTree['files'],
    'runnerSha256': await _sha256File(
      File('${packageRoot.path}/lib/competitor_profile_harness.dart'),
    ),
    'orchestratorSha256': await _sha256File(
      File('${packageRoot.path}/tool/run_quill_profile.dart'),
    ),
    'pubspecSha256': await _sha256File(
      File('${packageRoot.path}/pubspec.yaml'),
    ),
    'lockfileSha256': await _sha256File(
      File('${packageRoot.path}/pubspec.lock'),
    ),
    'pubDepsExitCode': depsExitCode,
    'flutterVersion': await command('flutter', [
      '--version',
      '--machine',
    ], 'flutter-version'),
    'xcodeVersion': await command('xcodebuild', ['-version'], 'xcode-version'),
    'powerState': await command('pmset', ['-g', 'batt'], 'power-state'),
    'thermalState': await command('pmset', ['-g', 'therm'], 'thermal-state'),
    'gitHead': await command('git', ['rev-parse', 'HEAD'], 'git-head'),
    'gitStatus': await command('git', ['status', '--short'], 'git-status'),
    'inheritedEnvironmentValuesPersisted': false,
    'inheritedEnvironmentNameSetSha256': sha256
        .convert(utf8.encode(environmentNames.join('\n')))
        .toString(),
    'hostEnvironmentCaveat':
        'Only per-process COMPETITOR_* overrides are persisted; inherited '
        'values are omitted to avoid recording secrets.',
  };
}

Future<Map<String, Object?>> _hashTree(Directory root) async {
  final files = await root
      .list(recursive: true, followLinks: false)
      .where((entity) => entity is File)
      .cast<File>()
      .toList();
  files.sort((left, right) => left.path.compareTo(right.path));
  final manifest = <Map<String, Object?>>[];
  for (final file in files) {
    final relative = file.path.substring(root.path.length + 1);
    manifest.add(<String, Object?>{
      'path': relative,
      'bytes': await file.length(),
      'sha256': await _sha256File(file),
    });
  }
  final canonical = manifest
      .map((entry) => '${entry['path']}\t${entry['bytes']}\t${entry['sha256']}')
      .join('\n');
  return <String, Object?>{
    'treeSha256': sha256.convert(utf8.encode(canonical)).toString(),
    'files': manifest,
  };
}

Future<String> _sha256File(File file) async {
  final digest = await sha256.bind(file.openRead()).first;
  return '$digest';
}
