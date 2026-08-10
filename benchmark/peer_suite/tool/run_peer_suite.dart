// ignore_for_file: avoid_relative_lib_imports

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../lib/peer_suite.dart';

const _quillTarget = 'lib/competitor_profile_harness.dart';
const _quillExecutable =
    'build/macos/Build/Products/Profile/'
    'flark_peer_benchmark.app/Contents/MacOS/flark_peer_benchmark';
const _superEditorTarget = 'lib/competitor_profile_harness.dart';

Future<void> main(List<String> arguments) async {
  final options = _Options.parse(arguments);
  final script = File.fromUri(Platform.script).absolute;
  final suiteRoot = script.parent.parent;
  final repositoryRoot = suiteRoot.parent.parent;
  final startedAtUtc = DateTime.now().toUtc();
  final runId = 'peer-suite-${startedAtUtc.microsecondsSinceEpoch}-p$pid';
  final output = Directory(
    options.output ??
        '${suiteRoot.path}/results/'
            '${startedAtUtc.toIso8601String().replaceAll(':', '-')}',
  ).absolute;
  output.createSync(recursive: true);
  final logs = Directory('${output.path}/logs')..createSync(recursive: true);
  final processResults = Directory('${output.path}/process-results')
    ..createSync(recursive: true);
  final exports = Directory('${output.path}/exports')
    ..createSync(recursive: true);

  final plan = PeerSuitePlan.protocol();
  final planFile = File('${output.path}/plan.json');
  await _writeJson(planFile, plan.toJson());
  final commands = <Map<String, Object?>>[];

  if (options.dryRun) {
    final assessment = const PeerSuiteValidator().validate(
      plan: plan,
      processes: const [],
      runGroups: const [],
      exclusiveMachineAttested: false,
      dryRun: true,
    );
    final receipt = <String, Object?>{
      'schemaVersion': peerSuiteSchemaVersion,
      'receiptId': '$runId-dry-run-nonclaim',
      'suiteId': peerSuiteId,
      'protocolId': peerSuiteProtocolId,
      'mode': 'dry-run-nonclaim',
      'startedAtUtc': startedAtUtc.toIso8601String(),
      'finishedAtUtc': DateTime.now().toUtc().toIso8601String(),
      'exactArgv': [Platform.resolvedExecutable, script.path, ...arguments],
      'cwd': Directory.current.absolute.path,
      'plan': {
        'path': planFile.path,
        'sha256': sha256File(planFile),
        'canonicalSha256': plan.sha256,
        'processCount': plan.entries.length,
      },
      'runGroups': const <Object?>[],
      'processes': const <Object?>[],
      ...assessment.toJson(),
    };
    final receiptFile = File('${output.path}/receipt.json');
    await _writeJson(receiptFile, receipt);
    stdout.writeln('PEER_SUITE_DRY_RUN_RECEIPT=${receiptFile.path}');
    stdout.writeln('PEER_SUITE_PLAN_SHA256=${plan.sha256}');
    return;
  }

  final flutter = await _resolveFlutter(options.flutter);
  final quillRoot = Directory('${repositoryRoot.path}/benchmark/peer');
  final superEditorRoot = Directory(
    '${repositoryRoot.path}/benchmark/peer_supereditor',
  );

  if (!options.skipBuild) {
    await _requiredCommand(
      executable: flutter,
      arguments: const ['pub', 'get'],
      cwd: quillRoot.path,
      label: 'quill-pub-get',
      logs: logs,
      commands: commands,
    );
    await _requiredCommand(
      executable: flutter,
      arguments: const [
        'build',
        'macos',
        '--profile',
        '-t',
        _quillTarget,
        '--dart-define=COMPETITOR_PROTOCOL_ID=$peerSuiteProtocolId',
      ],
      cwd: quillRoot.path,
      label: 'quill-profile-build',
      logs: logs,
      commands: commands,
    );
    await _requiredCommand(
      executable: flutter,
      arguments: const ['pub', 'get'],
      cwd: superEditorRoot.path,
      label: 'super-editor-pub-get',
      logs: logs,
      commands: commands,
    );
    await _requiredCommand(
      executable: flutter,
      arguments: const [
        'build',
        'macos',
        '--profile',
        '-t',
        _superEditorTarget,
        '--dart-define=COMPETITOR_PROTOCOL_ID=$peerSuiteProtocolId',
      ],
      cwd: superEditorRoot.path,
      label: 'super-editor-profile-build',
      logs: logs,
      commands: commands,
    );
  }

  final quillExecutable = File('${quillRoot.path}/$_quillExecutable');
  if (!quillExecutable.existsSync()) {
    throw StateError(
      'Missing Quill profile executable: ${quillExecutable.path}',
    );
  }
  final superEditorDriver = File(
    '${superEditorRoot.path}/tool/run_macos_profile.dart',
  );
  if (!superEditorDriver.existsSync()) {
    throw StateError('Missing SuperEditor driver: ${superEditorDriver.path}');
  }

  final processes = <PeerProcessEvidence>[];
  final groups = <RunGroupEvidence>[];
  final stateFile = File('${output.path}/suite-state.json');
  for (var groupIndex = 0; groupIndex < 3; groupIndex += 1) {
    final groupEntries = plan.entries
        .where((entry) => entry.groupIndex == groupIndex)
        .toList(growable: false);
    final idleStartedAtUtc = DateTime.now().toUtc();
    stdout.writeln(
      'PEER_SUITE_IDLE_START group=$groupIndex '
      'seconds=${options.idleSeconds} '
      'utc=${idleStartedAtUtc.toIso8601String()}',
    );
    await Future<void>.delayed(Duration(seconds: options.idleSeconds));
    final idleFinishedAtUtc = DateTime.now().toUtc();
    DateTime? firstStarted;
    DateTime? lastFinished;

    for (final entry in groupEntries) {
      stdout.writeln(
        '[${entry.orderSlot + 1}/${plan.entries.length}] '
        '${entry.peer} ${entry.workload} ${entry.targetBytes} '
        '${entry.location} r${entry.replicate}',
      );
      final evidence = entry.peer == 'flutter_quill'
          ? await _runQuill(
              entry: entry,
              executable: quillExecutable,
              quillRoot: quillRoot,
              logs: logs,
              processResults: processResults,
              exports: exports,
            )
          : await _runSuperEditor(
              entry: entry,
              flutter: flutter,
              driver: superEditorDriver,
              packageRoot: superEditorRoot,
              output: output,
              logs: logs,
            );
      processes.add(evidence);
      firstStarted ??= evidence.startedAtUtc;
      lastFinished = evidence.finishedAtUtc;
      await _writeJson(stateFile, {
        'schemaVersion': peerSuiteSchemaVersion,
        'receiptKind': 'incomplete-peer-suite-state',
        'claimEligible': false,
        'runId': runId,
        'planPath': planFile.path,
        'planSha256': sha256File(planFile),
        'completedProcessCount': processes.length,
        'processes': processes.map((value) => value.toJson()).toList(),
        'note':
            'An interrupted state is never aggregate evidence; restart the '
            'full suite so every group receives an uncontaminated idle period.',
      });
    }
    groups.add(
      RunGroupEvidence(
        groupIndex: groupIndex,
        idleStartedAtUtc: idleStartedAtUtc,
        idleFinishedAtUtc: idleFinishedAtUtc,
        firstProcessStartedAtUtc: firstStarted!,
        lastProcessFinishedAtUtc: lastFinished!,
      ),
    );
  }

  final assessment = const PeerSuiteValidator().validate(
    plan: plan,
    processes: processes,
    runGroups: groups,
    exclusiveMachineAttested: options.exclusiveMachineAttested,
    dryRun: false,
  );
  final provenance = await _coordinatorProvenance(
    repositoryRoot: repositoryRoot,
    script: script,
    suiteRoot: suiteRoot,
  );
  final receipt = <String, Object?>{
    'schemaVersion': peerSuiteSchemaVersion,
    'receiptId': runId,
    'suiteId': peerSuiteId,
    'protocolId': peerSuiteProtocolId,
    'mode': 'full-profile-protocol',
    'startedAtUtc': startedAtUtc.toIso8601String(),
    'finishedAtUtc': DateTime.now().toUtc().toIso8601String(),
    'exactArgv': [Platform.resolvedExecutable, script.path, ...arguments],
    'cwd': Directory.current.absolute.path,
    'exclusiveMachineAttested': options.exclusiveMachineAttested,
    'idleSecondsBeforeEachRunGroup': options.idleSeconds,
    'plan': {
      'path': planFile.path,
      'sha256': sha256File(planFile),
      'canonicalSha256': plan.sha256,
      'processCount': plan.entries.length,
    },
    'runGroups': groups.map((value) => value.toJson()).toList(),
    'processes': processes.map((value) => value.toJson()).toList(),
    'commands': commands,
    'provenance': provenance,
    ...assessment.toJson(),
  };
  final receiptFile = File('${output.path}/receipt.json');
  await _writeJson(receiptFile, receipt);
  stdout.writeln('PEER_SUITE_RECEIPT=${receiptFile.path}');
  if (!assessment.completionEnvelopeEligible) exitCode = 1;
}

Future<PeerProcessEvidence> _runQuill({
  required PeerSuiteEntry entry,
  required File executable,
  required Directory quillRoot,
  required Directory logs,
  required Directory processResults,
  required Directory exports,
}) async {
  final resultPath = '${processResults.path}/${entry.id}.json';
  final exportPath = '${exports.path}/${entry.id}.final-source.txt';
  final environment = <String, String>{
    'COMPETITOR_SCENARIO': entry.workload,
    'COMPETITOR_TARGET_BYTES': '${entry.targetBytes}',
    'COMPETITOR_LOCATION': entry.location,
    'COMPETITOR_RUN_INDEX': '${entry.replicate}',
    'COMPETITOR_ORDER_INDEX': '${entry.orderSlot}',
    'COMPETITOR_PROCESS_RUN_ID': entry.id,
    'COMPETITOR_OUTPUT_PATH': resultPath,
    'COMPETITOR_EXPORT_PATH': exportPath,
  };
  final result = await _runProcess(
    executable: executable.path,
    arguments: const [],
    cwd: quillRoot.path,
    environment: environment,
    stdoutPath: '${logs.path}/${entry.id}.stdout.log',
    stderrPath: '${logs.path}/${entry.id}.stderr.log',
    timeout: _processTimeout(entry.workload),
  );
  return result.toEvidence(entry: entry, resultPath: resultPath);
}

Future<PeerProcessEvidence> _runSuperEditor({
  required PeerSuiteEntry entry,
  required String flutter,
  required File driver,
  required Directory packageRoot,
  required Directory output,
  required Directory logs,
}) async {
  final peerOutput = Directory('${output.path}/super-editor')
    ..createSync(recursive: true);
  final arguments = <String>[
    driver.path,
    '--flutter=$flutter',
    '--no-build',
    '--workload=${entry.workload}',
    '--bytes=${entry.targetBytes}',
    '--location=${entry.location}',
    '--run-id=${entry.id}',
    '--run-group-id=group-${entry.groupIndex}',
    '--order-slot=${entry.orderSlot}',
    '--idle-seconds=0',
    '--output=${peerOutput.path}',
  ];
  final result = await _runProcess(
    executable: Platform.resolvedExecutable,
    arguments: arguments,
    cwd: packageRoot.path,
    environment: const {},
    stdoutPath: '${logs.path}/${entry.id}.stdout.log',
    stderrPath: '${logs.path}/${entry.id}.stderr.log',
    timeout: _processTimeout(entry.workload),
  );
  final resultPath = '${peerOutput.path}/${entry.id}/${entry.id}.result.json';
  return result.toEvidence(entry: entry, resultPath: resultPath);
}

Duration _processTimeout(String workload) => switch (workload) {
  'cold-open' => const Duration(seconds: 75),
  'sustained-typing' => const Duration(minutes: 2),
  'local-insert-delete' => const Duration(hours: 4),
  'paste-32kib' => const Duration(hours: 1),
  _ => throw StateError('No timeout for $workload'),
};

Future<_ProcessResult> _runProcess({
  required String executable,
  required List<String> arguments,
  required String cwd,
  required Map<String, String> environment,
  required String stdoutPath,
  required String stderrPath,
  required Duration timeout,
}) async {
  final startedAtUtc = DateTime.now().toUtc();
  final process = await Process.start(
    executable,
    arguments,
    workingDirectory: cwd,
    environment: environment,
    includeParentEnvironment: true,
  );
  final stdoutFile = File(stdoutPath);
  final stderrFile = File(stderrPath);
  final stdoutSink = stdoutFile.openWrite();
  final stderrSink = stderrFile.openWrite();
  final stdoutDone = process.stdout.pipe(stdoutSink);
  final stderrDone = process.stderr.pipe(stderrSink);
  var timedOut = false;
  late int code;
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
  return _ProcessResult(
    processId: process.pid,
    startedAtUtc: startedAtUtc,
    finishedAtUtc: DateTime.now().toUtc(),
    exitCode: code,
    timedOut: timedOut,
    argv: [executable, ...arguments],
    cwd: cwd,
    environmentOverrides: Map.unmodifiable(environment),
    stdoutPath: stdoutPath,
    stderrPath: stderrPath,
  );
}

Future<void> _requiredCommand({
  required String executable,
  required List<String> arguments,
  required String cwd,
  required String label,
  required Directory logs,
  required List<Map<String, Object?>> commands,
}) async {
  final startedAtUtc = DateTime.now().toUtc();
  final result = await Process.run(
    executable,
    arguments,
    workingDirectory: cwd,
  );
  final stdoutFile = File('${logs.path}/$label.stdout.log');
  final stderrFile = File('${logs.path}/$label.stderr.log');
  await stdoutFile.writeAsString('${result.stdout}', flush: true);
  await stderrFile.writeAsString('${result.stderr}', flush: true);
  final command = <String, Object?>{
    'argv': [executable, ...arguments],
    'cwd': cwd,
    'startedAtUtc': startedAtUtc.toIso8601String(),
    'finishedAtUtc': DateTime.now().toUtc().toIso8601String(),
    'exitCode': result.exitCode,
    'stdout': {'path': stdoutFile.path, 'sha256': sha256File(stdoutFile)},
    'stderr': {'path': stderrFile.path, 'sha256': sha256File(stderrFile)},
  };
  commands.add(command);
  if (result.exitCode != 0) {
    throw ProcessException(
      executable,
      arguments,
      '$label failed; see ${stderrFile.path}',
      result.exitCode,
    );
  }
}

Future<Map<String, Object?>> _coordinatorProvenance({
  required Directory repositoryRoot,
  required File script,
  required Directory suiteRoot,
}) async {
  Future<Map<String, Object?>> capture(
    String executable,
    List<String> args, {
    bool retainOutput = false,
  }) async {
    final result = await Process.run(
      executable,
      args,
      workingDirectory: repositoryRoot.path,
    );
    final output = '${result.stdout}${result.stderr}';
    return {
      'argv': [executable, ...args],
      'exitCode': result.exitCode,
      'outputBytes': utf8.encode(output).length,
      'sha256': sha256Text(output),
      if (retainOutput) 'stdout': '${result.stdout}'.trim(),
      if (retainOutput) 'stderr': '${result.stderr}'.trim(),
    };
  }

  final library = File('${suiteRoot.path}/lib/peer_suite.dart');
  return {
    'coordinatorSource': {'path': script.path, 'sha256': sha256File(script)},
    'validatorSource': {'path': library.path, 'sha256': sha256File(library)},
    'gitHead': await capture('git', ['rev-parse', 'HEAD'], retainOutput: true),
    'gitStatus': await capture('git', ['status', '--porcelain=v1']),
    'gitBinaryDiff': await capture('git', ['diff', '--binary', 'HEAD']),
    'inheritedEnvironmentPersisted': false,
    'environmentNote':
        'Only exact argv and explicit COMPETITOR_* values are retained; '
        'ambient environment values are excluded to avoid secret capture.',
  };
}

Future<String> _resolveFlutter(String? explicit) async {
  if (explicit != null) return File(explicit).absolute.path;
  final environment = Platform.environment['FLUTTER_BIN'];
  if (environment != null && environment.isNotEmpty) {
    return File(environment).absolute.path;
  }
  final which = await Process.run('/usr/bin/which', ['flutter']);
  if (which.exitCode == 0 && '${which.stdout}'.trim().isNotEmpty) {
    return '${which.stdout}'.trim();
  }
  throw StateError('Pass --flutter=/absolute/path/to/flutter');
}

Future<void> _writeJson(File file, Object? value) async {
  await file.writeAsString(
    '${const JsonEncoder.withIndent('  ').convert(value)}\n',
    flush: true,
  );
}

final class _ProcessResult {
  const _ProcessResult({
    required this.processId,
    required this.startedAtUtc,
    required this.finishedAtUtc,
    required this.exitCode,
    required this.timedOut,
    required this.argv,
    required this.cwd,
    required this.environmentOverrides,
    required this.stdoutPath,
    required this.stderrPath,
  });

  final int processId;
  final DateTime startedAtUtc;
  final DateTime finishedAtUtc;
  final int exitCode;
  final bool timedOut;
  final List<String> argv;
  final String cwd;
  final Map<String, String> environmentOverrides;
  final String stdoutPath;
  final String stderrPath;

  PeerProcessEvidence toEvidence({
    required PeerSuiteEntry entry,
    required String resultPath,
  }) {
    final result = File(resultPath);
    final stdoutFile = File(stdoutPath);
    final stderrFile = File(stderrPath);
    return PeerProcessEvidence(
      evidenceId: '${entry.id}-outer-p$processId',
      planEntryId: entry.id,
      processId: processId,
      startedAtUtc: startedAtUtc,
      finishedAtUtc: finishedAtUtc,
      exitCode: exitCode,
      timedOut: timedOut,
      argv: argv,
      cwd: cwd,
      environmentOverrides: environmentOverrides,
      resultPath: resultPath,
      resultSha256: result.existsSync() ? sha256File(result) : null,
      stdoutPath: stdoutPath,
      stdoutSha256: stdoutFile.existsSync() ? sha256File(stdoutFile) : null,
      stderrPath: stderrPath,
      stderrSha256: stderrFile.existsSync() ? sha256File(stderrFile) : null,
    );
  }
}

final class _Options {
  const _Options({
    required this.dryRun,
    required this.skipBuild,
    required this.exclusiveMachineAttested,
    required this.idleSeconds,
    required this.flutter,
    required this.output,
  });

  factory _Options.parse(List<String> arguments) {
    var dryRun = false;
    var execute = false;
    var skipBuild = false;
    var exclusive = false;
    var idleSeconds = protocolIdleSeconds;
    String? flutter;
    String? output;
    for (final argument in arguments) {
      if (argument == '--dry-run') {
        dryRun = true;
      } else if (argument == '--execute') {
        execute = true;
      } else if (argument == '--skip-build') {
        skipBuild = true;
      } else if (argument == '--exclusive-machine-attested') {
        exclusive = true;
      } else if (argument.startsWith('--idle-seconds=')) {
        idleSeconds = int.parse(argument.substring('--idle-seconds='.length));
      } else if (argument.startsWith('--flutter=')) {
        flutter = argument.substring('--flutter='.length);
      } else if (argument.startsWith('--output=')) {
        output = argument.substring('--output='.length);
      } else {
        throw FormatException('Unknown argument: $argument');
      }
    }
    if (dryRun == execute) {
      throw const FormatException(
        'Pass exactly one of --dry-run or --execute.',
      );
    }
    if (execute && !exclusive) {
      throw const FormatException(
        '--execute requires --exclusive-machine-attested.',
      );
    }
    if (execute && idleSeconds < protocolIdleSeconds) {
      throw FormatException(
        '--execute requires at least $protocolIdleSeconds idle seconds per group.',
      );
    }
    return _Options(
      dryRun: dryRun,
      skipBuild: skipBuild,
      exclusiveMachineAttested: exclusive,
      idleSeconds: idleSeconds,
      flutter: flutter,
      output: output,
    );
  }

  final bool dryRun;
  final bool skipBuild;
  final bool exclusiveMachineAttested;
  final int idleSeconds;
  final String? flutter;
  final String? output;
}
