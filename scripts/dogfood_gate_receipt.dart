import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

const _actualPaintFiles = {
  'test/north_star_paint_matrix_test.dart',
  'test/inline_dependency_island_paint_acceptance_test.dart',
};
const _allowedEnvironmentKeys = {
  'CARGO_HOME',
  'DEVELOPER_DIR',
  'FLUTTER_ROOT',
  'HOME',
  'LANG',
  'LC_ALL',
  'LC_CTYPE',
  'LOGNAME',
  'PATH',
  'PUB_CACHE',
  'RUSTUP_HOME',
  'SDKROOT',
  'SHELL',
  'TERM',
  'TMPDIR',
  'USER',
  'XDG_CACHE_HOME',
  'XDG_CONFIG_HOME',
};

final class _DogfoodGateExecution {
  const _DogfoodGateExecution({
    required this.processId,
    required this.startedEpochMicros,
    required this.finishedEpochMicros,
    required this.exitCode,
  });

  final int processId;
  final int startedEpochMicros;
  final int finishedEpochMicros;
  final int exitCode;

  Map<String, Object> toJson() => {
    'processId': processId,
    'startedEpochMicros': startedEpochMicros,
    'finishedEpochMicros': finishedEpochMicros,
    'exitCode': exitCode,
  };
}

Future<Map<String, Object?>> runDogfoodGate({
  required Directory repository,
  required String lane,
  required File log,
  File? embeddedAbi,
}) async {
  final recipe = await _gateRecipe(repository, lane, embeddedAbi: embeddedAbi);
  final head = await _git(repository, const ['rev-parse', 'HEAD']);
  final tree = await _git(repository, const ['rev-parse', 'HEAD^{tree}']);
  if ((await _git(repository, const ['status', '--porcelain'])).isNotEmpty) {
    throw StateError('Gate receipt requires a clean worktree.');
  }
  await log.parent.create(recursive: true);
  final execution = await _execute(
    recipe.command,
    recipe.workingDirectory,
    recipe.processEnvironment,
    log,
  );
  if (execution.exitCode != 0) {
    throw StateError('$lane gate process exited ${execution.exitCode}.');
  }
  if ((await _git(repository, const ['rev-parse', 'HEAD'])) != head ||
      (await _git(repository, const ['rev-parse', 'HEAD^{tree}'])) != tree ||
      (await _git(repository, const ['status', '--porcelain'])).isNotEmpty) {
    throw StateError(
      '$lane gate changed or left the candidate worktree dirty.',
    );
  }
  final proof = await _replayLogProof(lane, log);
  return {
    'schema': 'dogfood_gate_receipt_v1',
    'lane': lane,
    'candidate': {'commit': head, 'tree': tree, 'clean': true},
    'command': recipe.command,
    'workingDirectory': recipe.workingDirectory.absolute.path,
    'environment': recipe.environmentEvidence,
    'execution': execution.toJson(),
    'toolchain': recipe.toolchain,
    'runnerArtifacts': recipe.runnerArtifacts,
    'log': await _fileIdentity(log),
    if (recipe.embeddedAbi != null) 'embeddedAbi': recipe.embeddedAbi,
    'proof': proof,
    'assessment': {'result': 'PASS'},
  };
}

Future<void> verifyDogfoodGateReceipt(
  Map<String, Object?> receipt, {
  required Directory repository,
}) async {
  if (receipt['schema'] != 'dogfood_gate_receipt_v1' ||
      (receipt['assessment'] as Map?)?['result'] != 'PASS') {
    throw StateError('Gate receipt is not a D0 PASS receipt.');
  }
  final lane = receipt['lane'];
  if (lane is! String) throw StateError('Gate receipt has no lane.');
  final embeddedAbi = receipt['embeddedAbi'] is Map
      ? (receipt['embeddedAbi']! as Map).cast<String, Object?>()
      : null;
  final candidate = (receipt['candidate'] as Map).cast<String, Object?>();
  if (candidate['commit'] !=
          await _git(repository, const ['rev-parse', 'HEAD']) ||
      candidate['tree'] !=
          await _git(repository, const ['rev-parse', 'HEAD^{tree}']) ||
      candidate['clean'] != true ||
      (await _git(repository, const ['status', '--porcelain'])).isNotEmpty) {
    throw StateError('Gate receipt candidate is not the clean current tree.');
  }
  await _verifyFrozenRecipe(
    receipt,
    repository: repository,
    lane: lane,
    embeddedAbi: embeddedAbi,
  );
  final execution = (receipt['execution'] as Map).cast<String, Object?>();
  if (execution['processId'] is! int ||
      (execution['processId']! as int) < 1 ||
      execution['startedEpochMicros'] is! int ||
      execution['finishedEpochMicros'] is! int ||
      (execution['finishedEpochMicros']! as int) <
          (execution['startedEpochMicros']! as int) ||
      execution['exitCode'] != 0) {
    throw StateError('Gate receipt has no successful observed process run.');
  }
  final logIdentity = (receipt['log'] as Map).cast<String, Object?>();
  await _verifyFileIdentity(logIdentity);
  final replayedProof = await _replayLogProof(
    lane,
    File(logIdentity['path']! as String),
  );
  if (jsonEncode(receipt['proof']) != jsonEncode(replayedProof)) {
    throw StateError(
      'Gate receipt proof does not replay from its machine log.',
    );
  }
}

Future<_DogfoodGateExecution> _execute(
  List<String> command,
  Directory workingDirectory,
  Map<String, String> environment,
  File log,
) async {
  final started = DateTime.now().microsecondsSinceEpoch;
  final process = await Process.start(
    command.first,
    command.sublist(1),
    workingDirectory: workingDirectory.path,
    environment: environment,
    includeParentEnvironment: false,
  );
  final sink = log.openWrite(mode: FileMode.write);
  final stdoutDone = process.stdout.transform(utf8.decoder).forEach((chunk) {
    sink.write(chunk);
    stdout.write(chunk);
  });
  final stderrDone = process.stderr.transform(utf8.decoder).forEach((chunk) {
    sink.write(chunk);
    stderr.write(chunk);
  });
  final exitCode = await process.exitCode;
  await Future.wait([stdoutDone, stderrDone]);
  await sink.flush();
  await sink.close();
  return _DogfoodGateExecution(
    processId: process.pid,
    startedEpochMicros: started,
    finishedEpochMicros: DateTime.now().microsecondsSinceEpoch,
    exitCode: exitCode,
  );
}

Future<_GateRecipe> _gateRecipe(
  Directory repository,
  String lane, {
  required File? embeddedAbi,
}) async {
  final toolNames = switch (lane) {
    'default' => const ['bash', 'cargo', 'dart', 'flutter'],
    'stress' => const ['bash', 'cargo'],
    'actual-paint' => const ['flutter'],
    _ => throw ArgumentError.value(lane, 'lane', 'unknown D0 gate lane'),
  };
  final processEnvironment = dogfoodGateProcessEnvironment(
    Platform.environment,
    overrides: lane == 'actual-paint'
        ? {'FLARK_V4_LIBRARY_PATH': embeddedAbi?.absolute.path ?? ''}
        : const {},
  );
  final resolvedTools = <String, File>{};
  for (final name in toolNames) {
    resolvedTools[name] = await _resolveExecutable(name, processEnvironment);
  }
  final arguments = switch (lane) {
    'default' => const ['scripts/verify_v4.sh'],
    'stress' => const ['scripts/verify_v4_certification_stress.sh'],
    'actual-paint' => const [
      'test',
      'test/north_star_paint_matrix_test.dart',
      'test/inline_dependency_island_paint_acceptance_test.dart',
      '--machine',
      '--concurrency=1',
    ],
    _ => throw ArgumentError.value(lane, 'lane', 'unknown D0 gate lane'),
  };
  final workingDirectory = lane == 'actual-paint'
      ? Directory('${repository.path}/packages/flark')
      : repository;
  final runnerArtifacts = <Map<String, Object>>[];
  Map<String, Object>? abiIdentity;
  if (lane == 'actual-paint') {
    if (embeddedAbi == null || !await embeddedAbi.exists()) {
      throw StateError('Actual-paint gate requires its exact native ABI.');
    }
    abiIdentity = await _fileIdentity(embeddedAbi);
    for (final relative in _actualPaintFiles) {
      runnerArtifacts.add(
        await _fileIdentity(File('${workingDirectory.path}/$relative')),
      );
    }
  } else {
    final script = lane == 'default'
        ? 'scripts/verify_v4.sh'
        : 'scripts/verify_v4_certification_stress.sh';
    runnerArtifacts.add(
      await _fileIdentity(File('${repository.path}/$script')),
    );
  }
  final toolchain = <Map<String, Object>>[];
  for (final name in toolNames) {
    toolchain.add({'name': name, ...await _fileIdentity(resolvedTools[name]!)});
  }
  final primary = resolvedTools[toolNames.first]!;
  return _GateRecipe(
    command: [primary.absolute.path, ...arguments],
    workingDirectory: workingDirectory,
    processEnvironment: processEnvironment,
    environmentEvidence: _environmentEvidence(
      processEnvironment,
      overrides: lane == 'actual-paint'
          ? {'FLARK_V4_LIBRARY_PATH': embeddedAbi!.absolute.path}
          : const {},
    ),
    toolchain: toolchain,
    runnerArtifacts: runnerArtifacts,
    embeddedAbi: abiIdentity,
  );
}

Map<String, String> dogfoodGateProcessEnvironment(
  Map<String, String> parent, {
  Map<String, String> overrides = const {},
}) {
  final result = <String, String>{
    for (final entry in parent.entries)
      if (_allowedEnvironmentKeys.contains(entry.key)) entry.key: entry.value,
  };
  result.addAll(overrides);
  return result;
}

Future<File> _resolveExecutable(
  String name,
  Map<String, String> environment,
) async {
  final candidates = <String>[];
  for (final directory in (environment['PATH'] ?? '').split(':')) {
    if (directory.isNotEmpty) candidates.add('$directory/$name');
  }
  for (final candidate in candidates) {
    final file = File(candidate).absolute;
    if (!await file.exists()) continue;
    return File(await file.resolveSymbolicLinks());
  }
  throw StateError('D0 gate cannot resolve executable $name.');
}

Map<String, Object> _environmentEvidence(
  Map<String, String> environment, {
  required Map<String, String> overrides,
}) {
  final keys = environment.keys.toList()..sort();
  final canonical = keys.map((key) => '$key=${environment[key]}').join('\n');
  return {
    'allowKeys': _allowedEnvironmentKeys.toList()..sort(),
    'overrides': overrides,
    'effective': {for (final key in keys) key: environment[key]!},
    'effectiveSha256': sha256.convert(utf8.encode(canonical)).toString(),
  };
}

Future<void> _verifyFrozenRecipe(
  Map<String, Object?> receipt, {
  required Directory repository,
  required String lane,
  required Map<String, Object?>? embeddedAbi,
}) async {
  final expectedArguments = switch (lane) {
    'default' => const ['scripts/verify_v4.sh'],
    'stress' => const ['scripts/verify_v4_certification_stress.sh'],
    'actual-paint' => const [
      'test',
      'test/north_star_paint_matrix_test.dart',
      'test/inline_dependency_island_paint_acceptance_test.dart',
      '--machine',
      '--concurrency=1',
    ],
    _ => throw StateError('Unknown gate lane $lane.'),
  };
  final expectedToolNames = switch (lane) {
    'default' => const ['bash', 'cargo', 'dart', 'flutter'],
    'stress' => const ['bash', 'cargo'],
    'actual-paint' => const ['flutter'],
    _ => throw StateError('Unknown gate lane $lane.'),
  };
  final expectedWorkingDirectory = lane == 'actual-paint'
      ? Directory('${repository.path}/packages/flark').absolute.path
      : repository.absolute.path;
  final command = (receipt['command'] as List?)?.cast<String>();
  if (command == null ||
      command.length != expectedArguments.length + 1 ||
      !File(command.first).isAbsolute ||
      jsonEncode(command.sublist(1)) != jsonEncode(expectedArguments) ||
      receipt['workingDirectory'] != expectedWorkingDirectory) {
    throw StateError('Gate receipt does not match its frozen command recipe.');
  }

  final environment = (receipt['environment'] as Map?)?.cast<String, Object?>();
  if (lane == 'actual-paint' && embeddedAbi == null) {
    throw StateError('Actual-paint receipt has no embedded ABI identity.');
  }
  final expectedOverrides = <String, String>{
    if (lane == 'actual-paint')
      'FLARK_V4_LIBRARY_PATH': embeddedAbi!['path']! as String,
  };
  final expectedEffectiveEnvironment = dogfoodGateProcessEnvironment(
    Platform.environment,
    overrides: expectedOverrides,
  );
  final digest = environment?['effectiveSha256'];
  final effective = environment?['effective'];
  final effectiveKeys = expectedEffectiveEnvironment.keys.toList()..sort();
  final effectiveCanonical = effectiveKeys
      .map((key) => '$key=${expectedEffectiveEnvironment[key]}')
      .join('\n');
  if (environment == null ||
      jsonEncode(environment['allowKeys']) !=
          jsonEncode(_allowedEnvironmentKeys.toList()..sort()) ||
      jsonEncode(environment['overrides']) != jsonEncode(expectedOverrides) ||
      jsonEncode(effective) !=
          jsonEncode({
            for (final key in effectiveKeys)
              key: expectedEffectiveEnvironment[key]!,
          }) ||
      digest is! String ||
      digest != sha256.convert(utf8.encode(effectiveCanonical)).toString()) {
    throw StateError('Gate receipt has no controlled execution environment.');
  }

  final toolchain = (receipt['toolchain'] as List?)?.cast<Map>();
  if (toolchain == null || toolchain.length != expectedToolNames.length) {
    throw StateError('Gate receipt has no complete toolchain identity.');
  }
  for (var index = 0; index < toolchain.length; index += 1) {
    final identity = toolchain[index].cast<String, Object?>();
    if (identity['name'] != expectedToolNames[index]) {
      throw StateError('Gate receipt toolchain order/name changed.');
    }
    final resolved = await _resolveExecutable(
      expectedToolNames[index],
      expectedEffectiveEnvironment,
    );
    if (identity['path'] != resolved.absolute.path) {
      throw StateError(
        'Gate receipt tool is not PATH-resolved: ${expectedToolNames[index]}.',
      );
    }
    await _verifyFileIdentity(identity);
  }
  if (toolchain.first['path'] != command.first) {
    throw StateError('Gate command is not its identity-bound primary tool.');
  }

  final expectedArtifacts = <Map<String, Object>>[];
  if (lane == 'actual-paint') {
    if (embeddedAbi == null) {
      throw StateError('Actual-paint receipt has no embedded ABI identity.');
    }
    await _verifyFileIdentity(embeddedAbi);
    for (final relative in _actualPaintFiles) {
      expectedArtifacts.add(
        await _fileIdentity(File('$expectedWorkingDirectory/$relative')),
      );
    }
  } else {
    final script = lane == 'default'
        ? 'scripts/verify_v4.sh'
        : 'scripts/verify_v4_certification_stress.sh';
    expectedArtifacts.add(
      await _fileIdentity(File('${repository.path}/$script')),
    );
  }
  if (jsonEncode(receipt['runnerArtifacts']) != jsonEncode(expectedArtifacts)) {
    throw StateError(
      'Gate runner artifacts do not match the current candidate.',
    );
  }
}

Future<Map<String, Object?>> _replayLogProof(String lane, File log) async {
  final lines = await log.readAsLines();
  switch (lane) {
    case 'default':
      _requireTerminalLines(lines, const [
        'verify_v4: active rust + dart + flutter v4 suites executed and passed.',
        'verify_v4: run scripts/verify_v4_certification_stress.sh for slow stress lanes.',
      ]);
      return {'kind': 'script-exit', 'exitCode': 0};
    case 'stress':
      _requireTerminalLines(lines, const [
        'verify_v4_certification_stress: full payload-budget stress passed.',
        'verify_v4_certification_stress: historical M0 receipt drift remains a separate audit.',
      ]);
      return {'kind': 'script-exit', 'exitCode': 0};
    case 'actual-paint':
      return _verifyMachineSuite(lines);
  }
  throw StateError('Unknown gate lane $lane.');
}

void _requireTerminalLines(List<String> lines, List<String> expected) {
  final nonempty = lines.where((line) => line.isNotEmpty).toList();
  final terminalStart = nonempty.length - expected.length;
  final matches =
      terminalStart >= 0 &&
      Iterable<int>.generate(
        expected.length,
      ).every((index) => nonempty[terminalStart + index] == expected[index]);
  if (!matches) {
    throw StateError(
      'Gate log does not end with its exact success protocol lines.',
    );
  }
}

Map<String, Object?> _verifyMachineSuite(Iterable<String> lines) {
  final starts = <int, ({String name, String url})>{};
  final completions = <int, ({String result, bool skipped})>{};
  bool? runnerSucceeded;
  for (final line in lines) {
    Object? decoded;
    try {
      decoded = jsonDecode(line);
    } on FormatException {
      continue;
    }
    if (decoded is! Map) continue;
    switch (decoded['type']) {
      case 'testStart':
        final test = decoded['test'];
        final sourceUrl = test is Map && test['root_url'] is String
            ? test['root_url']
            : test is Map
            ? test['url']
            : null;
        if (test is Map &&
            test['id'] is int &&
            test['name'] is String &&
            sourceUrl is String) {
          starts[test['id']! as int] = (
            name: test['name']! as String,
            url: sourceUrl,
          );
        }
      case 'testDone':
        if (decoded['testID'] is int &&
            decoded['result'] is String &&
            decoded['skipped'] is bool) {
          completions[decoded['testID']! as int] = (
            result: decoded['result']! as String,
            skipped: decoded['skipped']! as bool,
          );
        }
      case 'done':
        if (decoded['success'] is bool) {
          runnerSucceeded = decoded['success']! as bool;
        }
    }
  }
  if (runnerSucceeded != true || starts.isEmpty) {
    throw StateError(
      'Actual-paint machine runner did not complete successfully.',
    );
  }
  final observedFiles = <String>{};
  for (final entry in starts.entries) {
    final completion = completions[entry.key];
    if (completion == null ||
        completion.result != 'success' ||
        completion.skipped) {
      throw StateError('Actual-paint test did not pass: ${entry.value.name}');
    }
    for (final file in _actualPaintFiles) {
      if (entry.value.url.endsWith(file)) observedFiles.add(file);
    }
  }
  if (!observedFiles.containsAll(_actualPaintFiles)) {
    throw StateError('Actual-paint machine log omitted a required test file.');
  }
  return {
    'kind': 'flutter-machine-suite',
    'runnerSucceeded': true,
    'testCount': starts.length,
    'skipped': 0,
    'files': observedFiles.toList()..sort(),
  };
}

Future<void> _verifyFileIdentity(Map<String, Object?> identity) async {
  final actual = await _fileIdentity(File(identity['path']! as String));
  if (actual['bytes'] != identity['bytes'] ||
      actual['sha256'] != identity['sha256']) {
    throw StateError('Gate evidence artifact changed: ${identity['path']}');
  }
}

Future<Map<String, Object>> _fileIdentity(File file) async {
  if (!await file.exists()) {
    throw StateError('Gate evidence artifact does not exist: ${file.path}');
  }
  return {
    'path': file.absolute.path,
    'bytes': await file.length(),
    'sha256': (await sha256.bind(file.openRead()).first).toString(),
  };
}

Future<String> _git(Directory repository, List<String> arguments) async {
  final result = await Process.run(
    'git',
    arguments,
    workingDirectory: repository.path,
  );
  if (result.exitCode != 0) {
    throw StateError(
      'git ${arguments.join(' ')} failed: ${(result.stderr as String).trim()}',
    );
  }
  return (result.stdout as String).trim();
}

final class _GateRecipe {
  const _GateRecipe({
    required this.command,
    required this.workingDirectory,
    required this.processEnvironment,
    required this.environmentEvidence,
    required this.toolchain,
    required this.runnerArtifacts,
    required this.embeddedAbi,
  });

  final List<String> command;
  final Directory workingDirectory;
  final Map<String, String> processEnvironment;
  final Map<String, Object> environmentEvidence;
  final List<Map<String, Object>> toolchain;
  final List<Map<String, Object>> runnerArtifacts;
  final Map<String, Object>? embeddedAbi;
}

Future<void> main(List<String> arguments) async {
  if (arguments.length != 4 && arguments.length != 5) {
    stderr.writeln(
      'usage: dart run scripts/dogfood_gate_receipt.dart '
      '<repository> <default|stress|actual-paint> <log> <output> [embedded-abi]',
    );
    exitCode = 64;
    return;
  }
  try {
    final receipt = await runDogfoodGate(
      repository: Directory(arguments[0]).absolute,
      lane: arguments[1],
      log: File(arguments[2]).absolute,
      embeddedAbi: arguments.length == 5 ? File(arguments[4]).absolute : null,
    );
    final output = File(arguments[3]).absolute;
    await output.parent.create(recursive: true);
    await output.writeAsString('${jsonEncode(receipt)}\n', flush: true);
    stdout.writeln('dogfood-gate-receipt: PASS lane=${arguments[1]}');
  } on Object catch (error) {
    stderr.writeln('dogfood-gate-receipt: FAIL $error');
    exitCode = 1;
  }
}
