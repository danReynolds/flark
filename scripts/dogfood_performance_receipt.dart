// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

import 'dogfood_bundle_manifest.dart';
import 'dogfood_fixture_identity.dart';
import 'verify_v4_dogfood_receipt.dart';

final class DogfoodProfileAssembly {
  const DogfoodProfileAssembly({
    required this.binding,
    required this.display,
    required this.cells,
  });

  final Map<String, Object?> binding;
  final Map<String, Object?> display;
  final List<Map<String, Object?>> cells;
}

DogfoodProfileAssembly assembleDogfoodProfileFragments(
  List<Map<String, Object?>> fragments, {
  bool streamed = false,
}) {
  final expected = <String, DogfoodCellDenominator>{
    ...requiredDogfoodCells,
    if (streamed) ...streamedDogfoodCell,
  };
  final byCell = <String, List<Map<String, Object?>>>{};
  Map<String, Object?>? binding;
  Map<String, Object?>? display;
  for (final fragment in fragments) {
    final id = fragment['id'];
    if (id is! String || !expected.containsKey(id)) {
      throw FormatException('unexpected profile fragment id $id');
    }
    final fragmentDisplay = (fragment['display']! as Map)
        .cast<String, Object?>();
    final fragmentBinding = (fragment['binding']! as Map)
        .cast<String, Object?>();
    final fixture = (fragment['fixture']! as Map).cast<String, Object?>();
    final expectedFixture = dogfoodFixtureIdentity(id);
    if (jsonEncode(fixture) != jsonEncode(expectedFixture) ||
        fragment['sourceBytes'] != expectedFixture['sourceBytes']) {
      throw StateError('$id fragment disagrees with its frozen fixture');
    }
    if (binding == null) {
      binding = fragmentBinding;
    } else if (jsonEncode(binding) != jsonEncode(fragmentBinding)) {
      throw StateError('profile fragments disagree on candidate artifacts');
    }
    if (display == null) {
      display = fragmentDisplay;
    } else if (jsonEncode(display) != jsonEncode(fragmentDisplay)) {
      throw StateError('profile fragments disagree on display provenance');
    }
    byCell.putIfAbsent(id, () => []).add(fragment);
  }
  final cells = <Map<String, Object?>>[];
  for (final entry in expected.entries) {
    final values = byCell[entry.key] ?? const [];
    if (values.length != entry.value.runs) {
      throw StateError(
        '${entry.key} expected ${entry.value.runs} fragments, '
        'found ${values.length}',
      );
    }
    values.sort(
      (left, right) => ((left['run']! as Map)['run']! as int).compareTo(
        (right['run']! as Map)['run']! as int,
      ),
    );
    final first = values.first;
    for (final value in values) {
      for (final name in const [
        'sourceBytes',
        'warmupsPerRun',
        'samplesPerRun',
        'runCount',
        'cadenceHz',
      ]) {
        if (value[name] != first[name]) {
          throw StateError('${entry.key} fragments disagree on $name');
        }
      }
    }
    cells.add({
      'id': entry.key,
      'sourceBytes': first['sourceBytes'],
      'warmupsPerRun': first['warmupsPerRun'],
      'samplesPerRun': first['samplesPerRun'],
      'runCount': first['runCount'],
      'cadenceHz': first['cadenceHz'],
      'fixture': first['fixture'],
      'runs': [for (final value in values) value['run']],
    });
  }
  if (binding == null || display == null) {
    throw StateError('profile receipt has no binding or display');
  }
  return DogfoodProfileAssembly(
    binding: binding,
    display: display,
    cells: cells,
  );
}

Future<void> main(List<String> arguments) async {
  if (arguments.length != 7 && arguments.length != 8) {
    stderr.writeln(
      'usage: dart run scripts/dogfood_performance_receipt.dart '
      '<repository> <app-bundle> <bundle-manifest.json> <main-executable> '
      '<embedded-abi> <fragments-directory> <output.json> [--streamed]',
    );
    exitCode = 64;
    return;
  }
  final streamed = arguments.length == 8 && arguments[7] == '--streamed';
  if (arguments.length == 8 && !streamed) {
    stderr.writeln(
      'dogfood-performance-receipt: unknown option ${arguments[7]}',
    );
    exitCode = 64;
    return;
  }
  try {
    final repository = Directory(arguments[0]).absolute;
    final appBundle = Directory(arguments[1]).absolute;
    final bundleManifest = File(arguments[2]).absolute;
    final mainExecutable = File(arguments[3]).absolute;
    final embeddedAbi = File(arguments[4]).absolute;
    final fragmentsDirectory = Directory(arguments[5]).absolute;
    final output = File(arguments[6]).absolute;
    for (final entity in <FileSystemEntity>[
      repository,
      appBundle,
      bundleManifest,
      mainExecutable,
      embeddedAbi,
      fragmentsDirectory,
    ]) {
      if (!await entity.exists()) {
        throw ArgumentError.value(entity.path, 'path', 'does not exist');
      }
    }
    final status = await _git(repository, const ['status', '--porcelain']);
    if (status.isNotEmpty) {
      throw StateError('performance receipt requires a clean worktree');
    }
    final verifiedManifest = await verifyDogfoodBundleManifest(
      appBundle,
      bundleManifest,
    );
    final mainEntry = dogfoodBundleEntryForFile(
      verifiedManifest,
      appBundle,
      mainExecutable,
    );
    final abiEntry = dogfoodBundleEntryForFile(
      verifiedManifest,
      appBundle,
      embeddedAbi,
    );
    final fragments = <Map<String, Object?>>[];
    await for (final entity in fragmentsDirectory.list()) {
      if (entity is! File || !entity.path.endsWith('.json')) continue;
      final value = jsonDecode(await entity.readAsString());
      if (value is! Map) {
        throw FormatException(
          'profile fragment is not an object: ${entity.path}',
        );
      }
      fragments.add(value.cast<String, Object?>());
    }
    final assembly = assembleDogfoodProfileFragments(
      fragments,
      streamed: streamed,
    );
    final head = await _git(repository, const ['rev-parse', 'HEAD']);
    final tree = await _git(repository, const ['rev-parse', 'HEAD^{tree}']);
    final mainIdentity = await _fileIdentity(mainExecutable);
    final abiIdentity = await _fileIdentity(embeddedAbi);
    final expectedBinding = <String, Object?>{
      'candidateCommit': head,
      'candidateTree': tree,
      'bundleManifestSha256': verifiedManifest.sha256,
      'mainExecutable': mainIdentity,
      'embeddedAbi': abiIdentity,
    };
    if (jsonEncode(assembly.binding) != jsonEncode(expectedBinding)) {
      throw StateError(
        'profile fragments are not bound to the current candidate app',
      );
    }
    if (mainEntry.bytes != mainIdentity['bytes'] ||
        mainEntry.sha256 != mainIdentity['sha256'] ||
        abiEntry.bytes != abiIdentity['bytes'] ||
        abiEntry.sha256 != abiIdentity['sha256']) {
      throw StateError('profile artifacts disagree with the bundle manifest');
    }
    final ledger = File(
      '${repository.path}/docs/testing/dogfood_scenario_v1.md',
    );
    final harness = File('${repository.path}/scripts/dogfood_profile_run.dart');
    final raw = <String, Object?>{
      'schema': 'dogfood_performance_v1',
      'schemaVersion': 1,
      'candidate': {'commit': head, 'tree': tree, 'clean': true},
      'configuration': {
        'ledger': await _fileIdentity(ledger),
        'streamedOpeningEnabled': streamed,
        'enabledPresetIds': [
          'productTour',
          'prose1MiB',
          'prose5MiB',
          'prose10MiB',
          'giantLine5MiB',
          'denseBlocks1MiB',
          if (streamed) 'streamed10MiB',
        ],
      },
      'artifacts': {
        'appBundleManifest': await _fileIdentity(bundleManifest),
        'mainExecutable': mainIdentity,
        'embeddedAbi': abiIdentity,
        'profileHarness': await _fileIdentity(harness),
      },
      'host': await _hostIdentity(),
      'display': assembly.display,
      'cells': assembly.cells,
    };
    final sealed = await sealDogfoodPerformanceReceipt(
      raw,
      repository: repository,
    );
    await output.parent.create(recursive: true);
    await output.writeAsString('${jsonEncode(sealed)}\n', flush: true);
    final assessment = (sealed['assessment']! as Map).cast<String, Object?>();
    if (assessment['result'] != 'PASS') {
      for (final blocker in (assessment['blockers']! as List)) {
        stderr.writeln('dogfood-performance-receipt: BLOCKER $blocker');
      }
      exitCode = 1;
      return;
    }
    stdout.writeln(
      'dogfood-performance-receipt: PASS commit=$head output=${output.path}',
    );
  } on Object catch (error, stackTrace) {
    stderr.writeln('dogfood-performance-receipt: FAIL $error');
    stderr.writeln(stackTrace);
    exitCode = 1;
  }
}

Future<Map<String, Object>> _hostIdentity() async {
  final physicalMemory = int.tryParse(
    await _command('sysctl', const ['-n', 'hw.memsize']),
  );
  return {
    'hostname': Platform.localHostname,
    'operatingSystem': Platform.operatingSystemVersion,
    'architecture': await _command('uname', const ['-m']),
    'cpu': await _command('sysctl', const ['-n', 'machdep.cpu.brand_string']),
    'logicalCores': Platform.numberOfProcessors,
    'physicalMemoryBytes': physicalMemory ?? 1,
    'flutterVersion': await _command('flutter', const ['--version']),
    'dartVersion': await _command('dart', const ['--version']),
    'rustcVersion': await _command('rustc', const ['--version']),
    'cargoVersion': await _command('cargo', const ['--version']),
    'xcodeVersion': await _command('xcodebuild', const ['-version']),
  };
}

Future<Map<String, Object>> _fileIdentity(File file) async {
  if (!await file.exists()) {
    throw ArgumentError.value(file.path, 'file', 'does not exist');
  }
  return {
    'path': file.absolute.path,
    'bytes': await file.length(),
    'sha256': (await sha256.bind(file.openRead()).first).toString(),
  };
}

Future<String> _git(Directory repository, List<String> arguments) =>
    _command('git', arguments, workingDirectory: repository.path);

Future<String> _command(
  String executable,
  List<String> arguments, {
  String? workingDirectory,
}) async {
  final result = await Process.run(
    executable,
    arguments,
    workingDirectory: workingDirectory,
  );
  if (result.exitCode != 0) {
    throw StateError(
      '$executable ${arguments.join(' ')} failed: '
      '${(result.stderr as String).trim()}',
    );
  }
  final output = (result.stdout as String).trim();
  if (output.isNotEmpty) return output.replaceAll(RegExp(r'\s+'), ' ');
  final error = (result.stderr as String).trim();
  if (error.isNotEmpty) return error.replaceAll(RegExp(r'\s+'), ' ');
  throw StateError('$executable ${arguments.join(' ')} returned no identity');
}
