// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:crypto/crypto.dart';

import '../packages/flark_flutter/example/lib/dogfood_documents.dart';
import 'dogfood_bundle_manifest.dart';
import 'dogfood_fixture_identity.dart';

const _schema = 'dogfood_performance_v1';
const _microsPerSecond = 1000000;
const _maxVisibilityMicros = 16000;
const _maxEngineP99Micros = 4000;
const _maxFlutterP99Micros = 8000;
const _maxEditorSpanMicros = 16000;
const _maxOpenMicros = 200000;
const _maxCertificationMicros = 500000;
const _maxRetainedRssBytes = 16 * 1024 * 1024;
const _largeSourceBytes = 1024 * 1024;
final _commitPattern = RegExp(r'^[0-9a-f]{40}$');
final _shaPattern = RegExp(r'^[0-9a-f]{64}$');
typedef _SourceKey = ({int sessionOrdinal, int sourceGeneration});
typedef _StructuralExpected = ({String sourceSha256, int caret});
final _structuralExpectedCache = <String, List<_StructuralExpected>>{};
final _structuralInitialSourceCache = <String, String>{};
final _openSourceCache = <String, ({String source, String sha256})>{};
const _enabledPresetIds = <String>{
  'productTour',
  'prose1MiB',
  'prose5MiB',
  'prose10MiB',
  'giantLine5MiB',
  'denseBlocks1MiB',
};

enum DogfoodProcessRule { any, freshEveryRun, oneSharedProcess }

final class DogfoodCellDenominator {
  const DogfoodCellDenominator({
    required this.warmups,
    required this.samples,
    required this.runs,
    required this.cadenceHz,
    this.processRule = DogfoodProcessRule.any,
    this.requiresLiveStateZero = false,
    this.requiresOpen = false,
    this.requiresInput = true,
  });

  final int warmups;
  final int samples;
  final int runs;
  final num cadenceHz;
  final DogfoodProcessRule processRule;
  final bool requiresLiveStateZero;
  final bool requiresOpen;
  final bool requiresInput;
}

const requiredDogfoodCells = <String, DogfoodCellDenominator>{
  'product-tour-cold-launch': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 5,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresOpen: true,
    requiresInput: false,
  ),
  'product-tour-typing': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 60,
  ),
  'product-tour-inline-typing': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 60,
  ),
  'product-tour-deletion': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 60,
  ),
  'product-tour-structural-burst': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 30,
  ),
  'ordinary-1m-typing': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 60,
  ),
  'ordinary-1m-inline-typing': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 60,
  ),
  'ordinary-1m-deletion': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 60,
  ),
  'ordinary-1m-structural-burst': DogfoodCellDenominator(
    warmups: 20,
    samples: 120,
    runs: 3,
    cadenceHz: 30,
  ),
  'ordinary-1m-paste-32kib': DogfoodCellDenominator(
    warmups: 2,
    samples: 10,
    runs: 3,
    cadenceHz: 0,
  ),
  'dense-blocks-1m-journey': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 5,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresOpen: true,
  ),
  'ordinary-5m-journey': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 5,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresOpen: true,
  ),
  'giant-line-5m-journey': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 5,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresOpen: true,
  ),
  'ordinary-10m-journey': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 5,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresOpen: true,
  ),
  'lifecycle-same-process': DogfoodCellDenominator(
    warmups: 0,
    samples: 100,
    runs: 1,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.oneSharedProcess,
    requiresLiveStateZero: true,
  ),
  'lifecycle-fresh-process': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 10,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresLiveStateZero: true,
  ),
};

const streamedDogfoodCell = <String, DogfoodCellDenominator>{
  'streamed-10m-journey': DogfoodCellDenominator(
    warmups: 0,
    samples: 1,
    runs: 5,
    cadenceHz: 0,
    processRule: DogfoodProcessRule.freshEveryRun,
    requiresOpen: true,
  ),
};

final class DogfoodReceiptValidation {
  const DogfoodReceiptValidation({
    required this.blockers,
    required this.metrics,
  });

  final List<String> blockers;
  final Map<String, Object> metrics;

  bool get passed => blockers.isEmpty;
}

Future<DogfoodReceiptValidation> verifyDogfoodPerformanceReceipt(
  Map<String, Object?> receipt, {
  Directory? repository,
  bool verifyArtifactFiles = true,
}) => _evaluateDogfoodPerformanceReceipt(
  receipt,
  repository: repository,
  verifyArtifactFiles: verifyArtifactFiles,
  verifyAssessment: true,
);

Future<DogfoodReceiptValidation> evaluateDogfoodPerformanceReceipt(
  Map<String, Object?> receipt, {
  Directory? repository,
  bool verifyArtifactFiles = true,
}) => _evaluateDogfoodPerformanceReceipt(
  receipt,
  repository: repository,
  verifyArtifactFiles: verifyArtifactFiles,
  verifyAssessment: false,
);

Future<Map<String, Object?>> sealDogfoodPerformanceReceipt(
  Map<String, Object?> receipt, {
  Directory? repository,
  bool verifyArtifactFiles = true,
}) async {
  final result = await evaluateDogfoodPerformanceReceipt(
    receipt,
    repository: repository,
    verifyArtifactFiles: verifyArtifactFiles,
  );
  return <String, Object?>{
    ...receipt,
    'assessment': <String, Object?>{
      'result': result.passed ? 'PASS' : 'FAIL',
      'blockers': result.blockers,
      'metrics': result.metrics,
    },
  };
}

Future<DogfoodReceiptValidation> _evaluateDogfoodPerformanceReceipt(
  Map<String, Object?> receipt, {
  required Directory? repository,
  required bool verifyArtifactFiles,
  required bool verifyAssessment,
}) async {
  final blockers = <String>[];
  final metricValues = _metricBuckets();
  final cellMetricValues = <String, Map<String, List<int>>>{};

  if (receipt['schema'] != _schema || receipt['schemaVersion'] != 1) {
    blockers.add('receipt must declare $_schema schemaVersion=1');
  }

  final candidate = _map(receipt['candidate'], 'candidate', blockers);
  final configuration = _map(
    receipt['configuration'],
    'configuration',
    blockers,
  );
  final artifacts = _map(receipt['artifacts'], 'artifacts', blockers);
  final host = _map(receipt['host'], 'host', blockers);
  final display = _map(receipt['display'], 'display', blockers);
  final assessment = verifyAssessment
      ? _map(receipt['assessment'], 'assessment', blockers)
      : const <String, Object?>{};

  for (final name in const ['commit', 'tree']) {
    final value = candidate[name];
    if (value is! String || !_commitPattern.hasMatch(value)) {
      blockers.add('candidate.$name must be a lowercase 40-digit git id');
    }
  }
  for (final name in const [
    'hostname',
    'operatingSystem',
    'architecture',
    'cpu',
    'flutterVersion',
    'dartVersion',
    'rustcVersion',
    'cargoVersion',
    'xcodeVersion',
  ]) {
    if (host[name] is! String || (host[name]! as String).isEmpty) {
      blockers.add('host.$name must be nonempty');
    }
  }
  if (host['logicalCores'] is! int || (host['logicalCores']! as int) < 1) {
    blockers.add('host.logicalCores must be positive');
  }
  if (host['physicalMemoryBytes'] is! int ||
      (host['physicalMemoryBytes']! as int) < 1) {
    blockers.add('host.physicalMemoryBytes must be positive');
  }

  final refreshHz = _number(
    display['refreshHz'],
    'display.refreshHz',
    blockers,
  );
  final framePeriod = _number(
    display['framePeriodMicros'],
    'display.framePeriodMicros',
    blockers,
  );
  if (refreshHz > 0 && framePeriod > 0) {
    final derived = _microsPerSecond / refreshHz;
    if ((derived - framePeriod).abs() > 1) {
      blockers.add(
        'display frame period $framePeriod does not match refresh $refreshHz',
      );
    }
  }
  if (display['widthLogical'] != 1569 || display['heightLogical'] != 906) {
    blockers.add('display must use the frozen 1569x906 logical geometry');
  }
  final fragmentIdentities = _list(
    artifacts['profileFragments'],
    'artifacts.profileFragments',
    blockers,
  );
  if (fragmentIdentities.isEmpty) {
    blockers.add('artifacts.profileFragments must not be empty');
  }

  if (verifyArtifactFiles) {
    await _verifyFileIdentity(
      configuration['ledger'],
      'configuration.ledger',
      blockers,
    );
    for (final name in const [
      'appBundleManifest',
      'mainExecutable',
      'embeddedAbi',
      'profileHarness',
    ]) {
      await _verifyFileIdentity(artifacts[name], 'artifacts.$name', blockers);
    }
    for (var index = 0; index < fragmentIdentities.length; index += 1) {
      await _verifyFileIdentity(
        fragmentIdentities[index],
        'artifacts.profileFragments[$index]',
        blockers,
      );
    }
    await _verifyBundleBinding(artifacts, blockers);
  }

  if (repository != null) {
    final head = await _git(repository, const ['rev-parse', 'HEAD'], blockers);
    final tree = await _git(repository, const [
      'rev-parse',
      'HEAD^{tree}',
    ], blockers);
    final status = await _git(repository, const [
      'status',
      '--porcelain',
    ], blockers);
    if (candidate['commit'] != head || candidate['tree'] != tree) {
      blockers.add('candidate commit/tree does not match the repository');
    }
    if (candidate['clean'] != true || status.isNotEmpty) {
      blockers.add('candidate worktree is not clean');
    }
  } else if (candidate['clean'] != true) {
    blockers.add('candidate.clean must be true');
  }

  final streamed = configuration['streamedOpeningEnabled'] == true;
  final configuredPresets = _list(
    configuration['enabledPresetIds'],
    'configuration.enabledPresetIds',
    blockers,
  ).whereType<String>().toSet();
  final expectedPresets = <String>{
    ..._enabledPresetIds,
    if (streamed) 'streamed10MiB',
  };
  if (!configuredPresets.containsAll(expectedPresets) ||
      !expectedPresets.containsAll(configuredPresets)) {
    blockers.add('enabled preset ids do not match the frozen D0 menu');
  }
  final expected = <String, DogfoodCellDenominator>{
    ...requiredDogfoodCells,
    if (streamed) ...streamedDogfoodCell,
  };
  final cells = _list(receipt['cells'], 'cells', blockers);
  final byId = <String, Map<String, Object?>>{};
  for (final value in cells) {
    final cell = _map(value, 'cells[]', blockers);
    final id = cell['id'];
    if (id is! String || id.isEmpty) {
      blockers.add('each cell must have a nonempty id');
      continue;
    }
    if (byId.containsKey(id)) blockers.add('duplicate cell $id');
    byId[id] = cell;
  }
  final actualIds = byId.keys.toSet();
  final expectedIds = expected.keys.toSet();
  for (final missing in expectedIds.difference(actualIds)) {
    blockers.add('missing required cell $missing');
  }
  for (final extra in actualIds.difference(expectedIds)) {
    blockers.add('unexpected cell $extra');
  }
  if (!streamed && actualIds.contains('streamed-10m-journey')) {
    blockers.add('streamed cell is present while the feature is disabled');
  }

  for (final entry in expected.entries) {
    final cell = byId[entry.key];
    if (cell == null) continue;
    final cellMetrics = _validateCell(
      entry.key,
      cell,
      entry.value,
      framePeriod: framePeriod,
      blockers: blockers,
    );
    cellMetricValues[entry.key] = cellMetrics;
    for (final metric in metricValues.entries) {
      metric.value.addAll(cellMetrics[metric.key]!);
    }
  }
  if (verifyArtifactFiles) {
    await _verifyProfileFragmentBinding(
      receipt,
      byId,
      fragmentIdentities,
      blockers,
    );
  }

  if (_percentile(metricValues['engineMicros']!, 99) > _maxEngineP99Micros) {
    blockers.add('Rust engine aggregate p99 exceeded 4 ms');
  }
  if (_percentile(metricValues['flutterFrameMicros']!, 99) >
      _maxFlutterP99Micros) {
    blockers.add('Flutter frame work aggregate p99 exceeded 8 ms');
  }

  final metrics = <String, Object>{
    for (final entry in metricValues.entries)
      entry.key: _distribution(entry.value),
    for (final cell in cellMetricValues.entries)
      for (final metric in cell.value.entries)
        'cell[${cell.key}].${metric.key}': _distribution(metric.value),
  };
  if (verifyAssessment) {
    final expectedResult = blockers.isEmpty ? 'PASS' : 'FAIL';
    if (assessment['result'] != expectedResult) {
      blockers.add(
        'assessment.result=${assessment['result']} but replay computed '
        '$expectedResult',
      );
    }
    final computedBeforeAssessment = [...blockers]..sort();
    final claimedBlockers = _list(
      assessment['blockers'],
      'assessment.blockers',
      blockers,
    ).whereType<String>().toList()..sort();
    if (!_sameJson(claimedBlockers, computedBeforeAssessment)) {
      blockers.add('assessment.blockers does not match replayed blockers');
    }
    if (!_sameJson(assessment['metrics'], metrics)) {
      blockers.add('assessment.metrics does not match replayed metrics');
    }
  }

  return DogfoodReceiptValidation(
    blockers: List.unmodifiable(blockers),
    metrics: Map.unmodifiable(metrics),
  );
}

Map<String, List<int>> _metricBuckets() => <String, List<int>>{
  'sourceToPaintMicros': [],
  'engineMicros': [],
  'flutterFrameMicros': [],
  'editorSpanMicros': [],
  'visibleCertificationMicros': [],
  'openToEditableMicros': [],
  'peakRssDeltaBytes': [],
  'retainedRssDeltaBytes': [],
};

Future<void> _verifyBundleBinding(
  Map<String, Object?> artifacts,
  List<String> blockers,
) async {
  try {
    final manifestIdentity = (artifacts['appBundleManifest']! as Map)
        .cast<String, Object?>();
    final mainIdentity = (artifacts['mainExecutable']! as Map)
        .cast<String, Object?>();
    final abiIdentity = (artifacts['embeddedAbi']! as Map)
        .cast<String, Object?>();
    final manifestFile = File(manifestIdentity['path']! as String);
    final decoded = jsonDecode(await manifestFile.readAsString());
    if (decoded is! Map<String, Object?> || decoded['bundlePath'] is! String) {
      throw const FormatException('manifest has no bundlePath');
    }
    final bundle = Directory(decoded['bundlePath']! as String);
    final verified = await verifyDogfoodBundleManifest(bundle, manifestFile);
    for (final entry in <(Map<String, Object?>, String)>[
      (mainIdentity, 'main executable'),
      (abiIdentity, 'embedded ABI'),
    ]) {
      final file = File(entry.$1['path']! as String);
      final manifestEntry = dogfoodBundleEntryForFile(verified, bundle, file);
      if (manifestEntry.bytes != entry.$1['bytes'] ||
          manifestEntry.sha256 != entry.$1['sha256']) {
        blockers.add('${entry.$2} disagrees with the app bundle manifest');
      }
    }
  } on Object catch (error) {
    blockers.add('app bundle artifact binding failed: $error');
  }
}

Future<void> _verifyProfileFragmentBinding(
  Map<String, Object?> receipt,
  Map<String, Map<String, Object?>> cells,
  List<Object?> identities,
  List<String> blockers,
) async {
  try {
    final candidate = (receipt['candidate']! as Map).cast<String, Object?>();
    final artifacts = (receipt['artifacts']! as Map).cast<String, Object?>();
    final host = (receipt['host']! as Map).cast<String, Object?>();
    final display = (receipt['display']! as Map).cast<String, Object?>();
    final manifestIdentity = (artifacts['appBundleManifest']! as Map)
        .cast<String, Object?>();
    final manifest =
        jsonDecode(
              await File(manifestIdentity['path']! as String).readAsString(),
            )
            as Map;
    final expectedBinding = <String, Object?>{
      'candidateCommit': candidate['commit'],
      'candidateTree': candidate['tree'],
      'bundleManifestSha256': manifest['sha256'],
      'mainExecutable': artifacts['mainExecutable'],
      'embeddedAbi': artifacts['embeddedAbi'],
      'measurementHost': {
        for (final name in const [
          'hostname',
          'operatingSystem',
          'architecture',
          'logicalCores',
          'physicalMemoryBytes',
        ])
          name: host[name],
      },
    };
    final expectedRuns = <String, Map<String, Object?>>{};
    for (final entry in cells.entries) {
      final runs = (entry.value['runs']! as List).cast<Map>();
      for (final rawRun in runs) {
        final run = rawRun.cast<String, Object?>();
        expectedRuns['${entry.key}/${run['run']}'] = run;
      }
    }
    final observed = <String>{};
    for (final rawIdentity in identities) {
      final identity = (rawIdentity! as Map).cast<String, Object?>();
      final decoded = jsonDecode(
        await File(identity['path']! as String).readAsString(),
      );
      if (decoded is! Map) {
        blockers.add('profile fragment is not an object: ${identity['path']}');
        continue;
      }
      final fragment = decoded.cast<String, Object?>();
      final id = fragment['id'];
      final run = fragment['run'];
      final runIndex = run is Map ? run['run'] : null;
      final key = '$id/$runIndex';
      final cell = id is String ? cells[id] : null;
      if (cell == null ||
          run is! Map ||
          runIndex is! int ||
          !observed.add(key)) {
        blockers.add(
          'profile fragment has an unexpected or duplicate run: $key',
        );
        continue;
      }
      if (!_sameJson(fragment['binding'], expectedBinding) ||
          !_sameJson(fragment['display'], display) ||
          !_sameJson(fragment['fixture'], cell['fixture']) ||
          fragment['sourceBytes'] != cell['sourceBytes'] ||
          fragment['warmupsPerRun'] != cell['warmupsPerRun'] ||
          fragment['samplesPerRun'] != cell['samplesPerRun'] ||
          fragment['runCount'] != cell['runCount'] ||
          fragment['cadenceHz'] != cell['cadenceHz'] ||
          !_sameJson(run, expectedRuns[key])) {
        blockers.add('profile fragment does not replay into cell run $key');
      }
    }
    if (!observed.containsAll(expectedRuns.keys) ||
        !expectedRuns.keys.toSet().containsAll(observed)) {
      blockers.add('profile fragments do not exactly cover final receipt runs');
    }
  } on Object catch (error) {
    blockers.add('profile fragment binding failed: $error');
  }
}

Map<String, List<int>> _validateCell(
  String id,
  Map<String, Object?> cell,
  DogfoodCellDenominator denominator, {
  required num framePeriod,
  required List<String> blockers,
}) {
  final metricValues = _metricBuckets();
  final structuralBurstMetricValues = id.endsWith('-structural-burst')
      ? _metricBuckets()
      : null;
  final prefix = 'cell[$id]';
  final sourceBytes = _integer(
    cell['sourceBytes'],
    '$prefix.sourceBytes',
    blockers,
  );
  final fixture = _map(cell['fixture'], '$prefix.fixture', blockers);
  final expectedFixture = dogfoodFixtureIdentity(id);
  if (!_sameJson(fixture, expectedFixture) ||
      fixture['sourceBytes'] != sourceBytes) {
    blockers.add('$prefix fixture identity does not match the frozen preset');
  }
  if (cell['warmupsPerRun'] != denominator.warmups ||
      cell['samplesPerRun'] != denominator.samples ||
      cell['runCount'] != denominator.runs ||
      cell['cadenceHz'] != denominator.cadenceHz) {
    blockers.add('$prefix denominator does not match the frozen D0 matrix');
  }
  final runs = _list(cell['runs'], '$prefix.runs', blockers);
  if (runs.length != denominator.runs) {
    blockers.add(
      '$prefix expected ${denominator.runs} runs, got ${runs.length}',
    );
  }
  final processIds = <String>{};
  for (var runIndex = 0; runIndex < runs.length; runIndex += 1) {
    final run = _map(runs[runIndex], '$prefix.runs[$runIndex]', blockers);
    final runPrefix = '$prefix.run[$runIndex]';
    if (run['run'] != runIndex) {
      blockers.add('$runPrefix has wrong run ordinal');
    }
    final processId = run['processId'];
    if (processId is! String || processId.isEmpty) {
      blockers.add('$runPrefix has no process identity');
    } else {
      processIds.add(processId);
    }
    if (run['faulted'] != false || run['resyncCount'] != 0) {
      blockers.add('$runPrefix faulted or resynchronized');
    }
    if (denominator.processRule == DogfoodProcessRule.freshEveryRun &&
        run['freshProcess'] != true) {
      blockers.add('$runPrefix must use a fresh process');
    }
    if (denominator.processRule == DogfoodProcessRule.oneSharedProcess &&
        run['freshProcess'] != false) {
      blockers.add('$runPrefix must use the warmed shared process');
    }

    final warmups = _list(run['warmups'], '$runPrefix.warmups', blockers);
    final samples = _list(run['samples'], '$runPrefix.samples', blockers);
    if (warmups.length != denominator.warmups) {
      blockers.add('$runPrefix expected ${denominator.warmups} warmups');
    }
    if (samples.length != denominator.samples) {
      blockers.add('$runPrefix expected ${denominator.samples} samples');
    }
    final frames = _frames(run['frames'], runPrefix, blockers);
    final measuredFrameOrdinals = <int>{};
    final inputObservations = _observationsByGeneration(
      run['inputObservations'],
      '$runPrefix.inputObservations',
      blockers,
      allowZero: denominator.requiresOpen || !denominator.requiresInput,
    );
    final engineObservations = _observationsByGeneration(
      run['engineObservations'],
      '$runPrefix.engineObservations',
      blockers,
      allowZero: denominator.requiresOpen || !denominator.requiresInput,
    );
    final paintObservations = _paintObservationsByGeneration(
      run['paintObservations'],
      '$runPrefix.paintObservations',
      blockers,
      allowZero: denominator.requiresOpen || !denominator.requiresInput,
    );
    final runSessionIdentities = _validateRunSessionIdentities(
      frames: frames,
      inputs: inputObservations,
      paints: paintObservations,
      engines: engineObservations,
      prefix: runPrefix,
      blockers: blockers,
    );
    _validateOpenObservation(
      run['openObservation'],
      id: id,
      required: denominator.requiresOpen,
      frames: frames,
      rawInputs: inputObservations,
      rawPaints: paintObservations,
      runSessionIdentities: runSessionIdentities,
      framePeriod: framePeriod,
      measuredFrameOrdinals: measuredFrameOrdinals,
      prefix: '$runPrefix.openObservation',
      blockers: blockers,
      metricValues: metricValues,
    );
    final declared = <Map<String, Object?>>[
      for (var index = 0; index < warmups.length; index += 1)
        _map(warmups[index], '$runPrefix.warmups[$index]', blockers),
      for (var index = 0; index < samples.length; index += 1)
        _map(samples[index], '$runPrefix.samples[$index]', blockers),
    ];
    final declaredGenerations = <_SourceKey>{};
    for (var index = 0; index < declared.length; index += 1) {
      final sessionOrdinal = _integer(
        declared[index]['sessionOrdinal'],
        '$runPrefix.declared[$index].sessionOrdinal',
        blockers,
      );
      for (final generation in _acceptedGenerations(
        declared[index],
        '$runPrefix.declared[$index]',
        blockers,
        allowZero: !denominator.requiresInput,
      )) {
        if (!declaredGenerations.add((
          sessionOrdinal: sessionOrdinal,
          sourceGeneration: generation,
        ))) {
          blockers.add('$runPrefix declared source generations must be unique');
        }
      }
    }
    if (denominator.requiresOpen) {
      declaredGenerations.add((sessionOrdinal: 0, sourceGeneration: 0));
    }
    final measuredGenerations = {
      for (final sample in declared)
        if (sample['sourceGeneration'] case final int generation)
          (
            sessionOrdinal: sample['sessionOrdinal']! as int,
            sourceGeneration: generation,
          ),
    };
    for (final entry in inputObservations.entries) {
      _validateOperationTiming(
        entry.value,
        required:
            id == 'ordinary-1m-paste-32kib' &&
            measuredGenerations.contains(entry.key),
        prefix:
            '$runPrefix.input[${entry.key.sessionOrdinal}:'
            '${entry.key.sourceGeneration}]',
        blockers: blockers,
      );
    }
    for (final entry in <String, Set<_SourceKey>>{
      'input': inputObservations.keys.toSet(),
      'engine': engineObservations.keys.toSet(),
    }.entries) {
      if (!entry.value.containsAll(declaredGenerations) ||
          !declaredGenerations.containsAll(entry.value)) {
        blockers.add(
          '$runPrefix ${entry.key} observations do not exactly cover the '
          'declared generations',
        );
      }
    }
    final observedPaintGenerations = paintObservations.keys.toSet();
    if (!declaredGenerations.containsAll(observedPaintGenerations)) {
      blockers.add(
        '$runPrefix paint observations contain an undeclared generation',
      );
    }
    for (var index = 0; index < samples.length; index += 1) {
      final sample = declared[warmups.length + index];
      final finalGeneration = sample['sourceGeneration'];
      final sessionOrdinal = sample['sessionOrdinal'];
      final disposition = sample['visibilityDisposition'];
      final supersededBy = sample['supersededBySourceGeneration'];
      final paintedGeneration = disposition == 'superseded-before-frame'
          ? supersededBy
          : finalGeneration;
      if (finalGeneration is! int ||
          sessionOrdinal is! int ||
          paintedGeneration is! int ||
          !paintObservations.containsKey((
            sessionOrdinal: sessionOrdinal,
            sourceGeneration: paintedGeneration,
          ))) {
        blockers.add('$runPrefix sample[$index] has no final-generation paint');
      }
    }
    for (var warmupIndex = 0; warmupIndex < warmups.length; warmupIndex += 1) {
      final warmup = declared[warmupIndex];
      final generations = _acceptedGenerations(
        warmup,
        '$runPrefix.warmup[$warmupIndex]',
        blockers,
        allowZero: !denominator.requiresInput,
      );
      final sessionOrdinal = warmup['sessionOrdinal']! as int;
      _validateWarmup(
        warmup,
        warmupIndex,
        rawInputs: [
          for (final generation in generations)
            inputObservations[(
              sessionOrdinal: sessionOrdinal,
              sourceGeneration: generation,
            )],
        ],
        rawEngines: [
          for (final generation in generations)
            engineObservations[(
              sessionOrdinal: sessionOrdinal,
              sourceGeneration: generation,
            )],
        ],
        requiresInput: denominator.requiresInput,
        prefix: '$runPrefix.warmup[$warmupIndex]',
        blockers: blockers,
      );
    }
    if (!id.endsWith('-structural-burst')) {
      _validateCadence(samples, denominator.cadenceHz, runPrefix, blockers);
    }
    for (var sampleIndex = 0; sampleIndex < samples.length; sampleIndex += 1) {
      final declaredIndex = warmups.length + sampleIndex;
      final sample = declared[declaredIndex];
      final generations = _acceptedGenerations(
        sample,
        '$runPrefix.sample[$sampleIndex]',
        blockers,
        allowZero: !denominator.requiresInput,
      );
      final sessionOrdinal = sample['sessionOrdinal']! as int;
      final supersededBy =
          sample['visibilityDisposition'] == 'superseded-before-frame'
          ? sample['supersededBySourceGeneration'] as int?
          : null;
      _validateSample(
        sample,
        sampleIndex,
        frames,
        rawInputs: [
          for (final generation in generations)
            inputObservations[(
              sessionOrdinal: sessionOrdinal,
              sourceGeneration: generation,
            )],
        ],
        rawEngines: [
          for (final generation in generations)
            engineObservations[(
              sessionOrdinal: sessionOrdinal,
              sourceGeneration: generation,
            )],
        ],
        rawPaints: [
          for (final generation in generations)
            ...paintObservations[(
                  sessionOrdinal: sessionOrdinal,
                  sourceGeneration: generation,
                )] ??
                const [],
          if (supersededBy != null)
            ...paintObservations[(
                  sessionOrdinal: sessionOrdinal,
                  sourceGeneration: supersededBy,
                )] ??
                const [],
        ],
        supersedingInput: supersededBy == null
            ? null
            : inputObservations[(
                sessionOrdinal: sessionOrdinal,
                sourceGeneration: supersededBy,
              )],
        nextAcceptedMicros: _nextAcceptedMicros(declared, declaredIndex),
        paintIntervalEndMicros: _nextAcceptedMicros(
          declared,
          declaredIndex + (supersededBy == null ? 0 : 1),
        ),
        collectMetrics: true,
        requiresInput: denominator.requiresInput,
        allowIntermediatePaintCoalescing: id.endsWith('-structural-burst'),
        measuredFrameOrdinals: measuredFrameOrdinals,
        framePeriod: framePeriod,
        requiresLiveStateZero: denominator.requiresLiveStateZero,
        prefix: '$runPrefix.sample[$sampleIndex]',
        blockers: blockers,
        metricValues: metricValues,
      );
    }
    if (id.endsWith('-structural-burst')) {
      _validateStructuralEvidence(
        id: id,
        runIndex: runIndex,
        run: run,
        denominator: denominator,
        framePeriod: framePeriod,
        prefix: runPrefix,
        blockers: blockers,
        burstMetricValues: structuralBurstMetricValues!,
      );
    }
    _recordMeasuredFrames(
      frames,
      measuredFrameOrdinals,
      runPrefix,
      blockers,
      metricValues,
    );
    _validateMemory(
      run['memory'],
      sourceBytes,
      runPrefix,
      blockers,
      metricValues,
    );
  }
  if (denominator.processRule == DogfoodProcessRule.freshEveryRun &&
      processIds.length != runs.length) {
    blockers.add('$prefix fresh runs did not use distinct process identities');
  }
  if (denominator.processRule == DogfoodProcessRule.oneSharedProcess &&
      processIds.length != 1) {
    blockers.add('$prefix must use exactly one warmed process');
  }
  if (_percentile(metricValues['engineMicros']!, 99) > _maxEngineP99Micros) {
    blockers.add('$prefix Rust engine p99 exceeded 4 ms');
  }
  if (_percentile(metricValues['flutterFrameMicros']!, 99) >
      _maxFlutterP99Micros) {
    blockers.add('$prefix Flutter frame work p99 exceeded 8 ms');
  }
  if (structuralBurstMetricValues != null) {
    if (_percentile(structuralBurstMetricValues['engineMicros']!, 99) >
        _maxEngineP99Micros) {
      blockers.add('$prefix structural burst Rust engine p99 exceeded 4 ms');
    }
    if (_percentile(structuralBurstMetricValues['flutterFrameMicros']!, 99) >
        _maxFlutterP99Micros) {
      blockers.add(
        '$prefix structural burst Flutter frame work p99 exceeded 8 ms',
      );
    }
    for (final entry in structuralBurstMetricValues.entries) {
      metricValues[entry.key]!.addAll(entry.value);
    }
  }
  return metricValues;
}

void _validateOperationTiming(
  Map<String, Object?> input, {
  required bool required,
  required String prefix,
  required List<String> blockers,
}) {
  final kind = input['operationTimingKind'];
  final event = input['operationTimingEvent'];
  if (kind == null && event == null) {
    if (required) blockers.add('$prefix has no platform paste timing');
    return;
  }
  if (kind != 'platform-paste' || event is! String) {
    blockers.add('$prefix has invalid platform operation timing');
    return;
  }
  final match = RegExp(
    r'^\d+:completed-paste:generation=(\d+)'
    r':acceptedAtEpochMicros=(\d+):elapsedMicros=(\d+)$',
  ).firstMatch(event);
  if (match == null ||
      int.tryParse(match.group(1)!) != input['sourceGeneration'] ||
      int.tryParse(match.group(2)!) != input['acceptedMicros'] ||
      int.tryParse(match.group(3)!) != input['editorSyncMicros']) {
    blockers.add('$prefix platform paste timing does not replay');
  }
}

void _validateStructuralEvidence({
  required String id,
  required int runIndex,
  required Map<String, Object?> run,
  required DogfoodCellDenominator denominator,
  required num framePeriod,
  required String prefix,
  required List<String> blockers,
  required Map<String, List<int>> burstMetricValues,
}) {
  if (run['structuralEvidenceVersion'] != 1) {
    blockers.add('$prefix has no structural evidence version 1');
  }
  _validateStructuralPhase(
    id: id,
    value: run,
    denominator: denominator,
    expectedPhase: 'latency',
    runIndex: runIndex,
    requireEveryGenerationPaint: false,
    enforceCadence: false,
    collectFrameMetrics: false,
    framePeriod: framePeriod,
    prefix: '$prefix.structuralLatency',
    blockers: blockers,
    metricValues: _metricBuckets(),
  );
  _validateStructuralPhase(
    id: id,
    value: run['structuralBurst'],
    denominator: denominator,
    expectedPhase: 'burst',
    runIndex: runIndex,
    requireEveryGenerationPaint: false,
    enforceCadence: true,
    collectFrameMetrics: true,
    framePeriod: framePeriod,
    prefix: '$prefix.structuralBurst',
    blockers: blockers,
    metricValues: burstMetricValues,
  );
  final control = run['structuralPerEditControl'];
  if (runIndex == 0) {
    _validateStructuralPhase(
      id: id,
      value: control,
      denominator: denominator,
      expectedPhase: 'perEditControl',
      runIndex: runIndex,
      requireEveryGenerationPaint: true,
      enforceCadence: false,
      collectFrameMetrics: false,
      framePeriod: framePeriod,
      prefix: '$prefix.structuralPerEditControl',
      blockers: blockers,
      metricValues: _metricBuckets(),
    );
  } else if (control != null) {
    blockers.add('$prefix may carry a per-edit control only in run zero');
  }
  final phases = <Map<String, Object?>>[
    run,
    _map(run['structuralBurst'], '$prefix.structuralBurst', blockers),
    if (runIndex == 0)
      _map(
        run['structuralPerEditControl'],
        '$prefix.structuralPerEditControl',
        blockers,
      ),
  ];
  final identities = phases
      .map((phase) => phase['structuralSessionIdentity'])
      .whereType<String>()
      .toSet();
  if (identities.length != phases.length) {
    blockers.add('$prefix structural phases did not use distinct sessions');
  }
  int? previousSequenceEnd;
  int? previousAppSequenceEnd;
  for (var index = 0; index < phases.length; index += 1) {
    final start = phases[index]['structuralActuatorSequenceStart'];
    final end = phases[index]['structuralActuatorSequenceEnd'];
    if (start is! int ||
        end is! int ||
        end <= start ||
        (previousSequenceEnd == null && start != 3) ||
        (previousSequenceEnd != null && start != previousSequenceEnd + 2)) {
      blockers.add(
        '$prefix structural phases do not have ordered actuator ranges',
      );
    }
    if (end is int) previousSequenceEnd = end;
    final setup = phases[index]['structuralSetupAcknowledgements'];
    final app = phases[index]['structuralAppAcknowledgements'];
    if (setup is List && setup.length == 2 && app is List && app.isNotEmpty) {
      final reset = setup.first;
      final terminal = app.last;
      final resetSequence = reset is Map ? reset['appCommandSequence'] : null;
      final terminalSequence = terminal is Map
          ? terminal['appCommandSequence']
          : null;
      if (resetSequence is! int ||
          terminalSequence is! int ||
          (previousAppSequenceEnd == null && resetSequence != 2) ||
          (previousAppSequenceEnd != null &&
              resetSequence != previousAppSequenceEnd + 1)) {
        blockers.add(
          '$prefix structural phases do not have contiguous app commands',
        );
      }
      if (terminalSequence is int) previousAppSequenceEnd = terminalSequence;
    }
  }
}

Set<String> _validateRunSessionIdentities({
  required Map<int, Map<String, Object?>> frames,
  required Map<_SourceKey, Map<String, Object?>> inputs,
  required Map<_SourceKey, List<Map<String, Object?>>> paints,
  required Map<_SourceKey, Map<String, Object?>> engines,
  required String prefix,
  required List<String> blockers,
}) {
  final identities = <String>{};
  final identityBySession = <int, String>{};
  var invalid = false;
  final sourceKeys = <_SourceKey>{
    ...inputs.keys,
    ...paints.keys,
    ...engines.keys,
  };
  for (final key in sourceKeys) {
    final keyIdentities = <String>{};
    final observations = <Map<String, Object?>>[
      ?inputs[key],
      ...?paints[key],
      ?engines[key],
    ];
    for (final observation in observations) {
      final identity = observation['measurementSessionIdentity'];
      if (identity is! String || identity.isEmpty) {
        invalid = true;
      } else {
        keyIdentities.add(identity);
        identities.add(identity);
      }
    }
    if (keyIdentities.length != 1) invalid = true;
    if (keyIdentities.length == 1) {
      final identity = keyIdentities.single;
      final prior = identityBySession[key.sessionOrdinal];
      if (prior != null && prior != identity) {
        invalid = true;
      } else {
        identityBySession[key.sessionOrdinal] = identity;
      }
    }
  }
  if (identityBySession.values.toSet().length != identityBySession.length) {
    invalid = true;
  }
  for (final observation in frames.values) {
    final identity = observation['measurementSessionIdentity'];
    final sessionOrdinal = observation['sessionOrdinal'];
    if (identity is! String ||
        identity.isEmpty ||
        sessionOrdinal is! int ||
        identityBySession[sessionOrdinal] != identity) {
      invalid = true;
    }
  }
  if (invalid || identities.isEmpty) {
    blockers.add(
      '$prefix raw observations do not preserve app-authored sessions',
    );
  }
  return identities;
}

void _validateStructuralAcknowledgements({
  required Map<String, Object?> phase,
  required Object? sessionIdentity,
  required List<Object?> commandTranscript,
  required Object? sequenceStart,
  required Object? sequenceEnd,
  required String prefix,
  required List<String> blockers,
}) {
  final setup = _list(
    phase['structuralSetupAcknowledgements'],
    '$prefix.structuralSetupAcknowledgements',
    blockers,
  );
  final app = _list(
    phase['structuralAppAcknowledgements'],
    '$prefix.structuralAppAcknowledgements',
    blockers,
  );
  if (sequenceStart is! int || sequenceEnd is! int) return;
  final expectedSetup = <({int actuatorSequence, String operation})>[
    (actuatorSequence: sequenceStart - 1, operation: 'reset'),
    (actuatorSequence: sequenceStart, operation: 'activateAtUtf16'),
  ];
  final expectedApp = <({int actuatorSequence, String operation})>[];
  for (var index = 0; index < commandTranscript.length; index += 1) {
    final command = commandTranscript[index];
    if (command is! String) continue;
    final operation = command.split(':').first;
    if (operation == 'settle' || operation == 'closeSession') {
      expectedApp.add((
        actuatorSequence: sequenceStart + index + 1,
        operation: operation,
      ));
    }
  }
  if (setup.length != expectedSetup.length ||
      app.length != expectedApp.length) {
    blockers.add('$prefix app acknowledgements do not replay');
    return;
  }
  final setupMaps = <Map<String, Object?>>[];
  for (var index = 0; index < setup.length; index += 1) {
    final acknowledgement = _map(
      setup[index],
      '$prefix.setup[$index]',
      blockers,
    );
    setupMaps.add(acknowledgement);
    if (acknowledgement['actuatorSequence'] !=
            expectedSetup[index].actuatorSequence ||
        acknowledgement['operation'] != expectedSetup[index].operation ||
        acknowledgement['canaryId'] != sessionIdentity) {
      blockers.add('$prefix setup app acknowledgement $index is invalid');
    }
  }
  final resetAppSequence = setupMaps[0]['appCommandSequence'];
  final activationAppSequence = setupMaps[1]['appCommandSequence'];
  if (resetAppSequence is! int ||
      activationAppSequence is! int ||
      activationAppSequence != resetAppSequence + 2) {
    blockers.add('$prefix setup app command sequence does not replay');
    return;
  }
  var expectedAppSequence = activationAppSequence;
  var acknowledgementIndex = 0;
  for (var index = 0; index < commandTranscript.length; index += 1) {
    final command = commandTranscript[index];
    if (command is! String) continue;
    final operation = command.split(':').first;
    switch (operation) {
      case 'typeStructuralBursts':
      case 'pressKey':
      case 'typeText':
        // The actuator asks the app to settle once to prove the selection
        // before dispatching these platform inputs.
        expectedAppSequence += 1;
      case 'settle':
      case 'closeSession':
        expectedAppSequence += 1;
        final acknowledgement = _map(
          app[acknowledgementIndex],
          '$prefix.measurement[$acknowledgementIndex]',
          blockers,
        );
        final expected = expectedApp[acknowledgementIndex];
        if (acknowledgement['actuatorSequence'] != expected.actuatorSequence ||
            acknowledgement['operation'] != expected.operation ||
            acknowledgement['canaryId'] != sessionIdentity ||
            acknowledgement['appCommandSequence'] != expectedAppSequence) {
          blockers.add(
            '$prefix measurement app acknowledgement '
            '$acknowledgementIndex is invalid',
          );
        }
        acknowledgementIndex += 1;
      default:
        blockers.add('$prefix contains an unsupported actuator transcript');
    }
  }
  if (expectedApp.isEmpty || expectedApp.last.actuatorSequence != sequenceEnd) {
    blockers.add('$prefix final app acknowledgement does not close its range');
  }
}

bool _paintClockReplays(
  Map<String, Object?> paint,
  Map<String, Object?>? frame, {
  required int framePeriodMicros,
}) {
  if (frame == null) return false;
  final timestamp = paint['timestampMicros'];
  final paintMonotonic = paint['paintMonotonicMicros'];
  final paintBefore = paint['paintEpochBeforeMicros'];
  final paintAfter = paint['paintEpochAfterMicros'];
  final frameBefore = frame['clockAnchorEpochBeforeMicros'];
  final frameAfter = frame['clockAnchorEpochAfterMicros'];
  final frameMonotonic = frame['clockAnchorMonotonicMicros'];
  final frameVsync = frame['monotonicVsyncMicros'];
  final buildStart = frame['buildStartMonotonicMicros'];
  final buildFinish = frame['buildFinishMonotonicMicros'];
  if (timestamp is! int ||
      paintMonotonic is! int ||
      paintBefore is! int ||
      paintAfter is! int ||
      frameBefore is! int ||
      frameAfter is! int ||
      frameMonotonic is! int ||
      frameVsync is! int ||
      buildStart is! int ||
      buildFinish is! int ||
      buildFinish < buildStart ||
      paintAfter < paintBefore ||
      paintAfter - paintBefore >= 1000 ||
      timestamp != paintBefore + ((paintAfter - paintBefore) ~/ 2)) {
    return false;
  }
  final mappedBefore = frameBefore + paintMonotonic - frameMonotonic;
  final mappedAfter = frameAfter + paintMonotonic - frameMonotonic;
  if (mappedAfter < paintBefore || mappedBefore > paintAfter) return false;
  return paintMonotonic >= buildStart && paintMonotonic <= buildFinish;
}

({Set<String> keys, int start, int end, bool coversBounds})?
_paintFragmentLedger(Object? value) {
  if (value is! List || value.isEmpty) return null;
  final keys = <String>{};
  int? sourceStart;
  int? sourceEnd;
  int? previousStart;
  int? coveredEnd;
  var coversBounds = true;
  for (final entry in value) {
    if (entry is! Map) return null;
    final ordinal = entry['ordinal'];
    final fragmentStart = entry['fragmentStart'];
    final fragmentEnd = entry['fragmentEnd'];
    final start = entry['sourceUtf16Start'];
    final end = entry['sourceUtf16End'];
    if (ordinal is! int ||
        fragmentStart is! int ||
        fragmentEnd is! int ||
        start is! int ||
        end is! int ||
        fragmentStart < 0 ||
        fragmentEnd < fragmentStart ||
        start < 0 ||
        end < start) {
      return null;
    }
    final key = '$ordinal:$fragmentStart:$fragmentEnd:$start:$end';
    if (!keys.add(key)) return null;
    if (previousStart != null &&
        (start < previousStart || start > coveredEnd! + 1)) {
      coversBounds = false;
    }
    previousStart = start;
    coveredEnd = coveredEnd == null ? end : math.max(coveredEnd, end);
    sourceStart = sourceStart == null ? start : math.min(sourceStart, start);
    sourceEnd = sourceEnd == null ? end : math.max(sourceEnd, end);
  }
  return (
    keys: keys,
    start: sourceStart!,
    end: sourceEnd!,
    coversBounds: coversBounds,
  );
}

bool _paintSurfaceReplays(Map<String, Object?> paint, Object? expectedExtent) {
  final rowCount = paint['paintedRowCount'];
  final requiredVisibleFragmentCount = paint['requiredVisibleFragmentCount'];
  final laidOutVisiblePlusOverscanFragmentCount =
      paint['laidOutVisiblePlusOverscanFragmentCount'];
  final visibleStart = paint['visibleUtf16Start'];
  final visibleLength = paint['visibleUtf16Length'];
  final paintedStart = paint['paintedSourceUtf16Start'];
  final paintedEnd = paint['paintedSourceUtf16End'];
  final readyStart = paint['visiblePlusOverscanUtf16Start'];
  final readyEnd = paint['visiblePlusOverscanUtf16End'];
  final requiredLedger = _paintFragmentLedger(
    paint['requiredVisibleFragments'],
  );
  final readyLedger = _paintFragmentLedger(
    paint['laidOutVisiblePlusOverscanFragments'],
  );
  final paintedLedger = _paintFragmentLedger(paint['paintedFragments']);
  return paint['completeVisibleSurface'] == true &&
      paint['completeVisiblePlusOverscanSurface'] == true &&
      rowCount is int &&
      rowCount > 0 &&
      requiredVisibleFragmentCount is int &&
      requiredVisibleFragmentCount > 0 &&
      rowCount == requiredVisibleFragmentCount &&
      laidOutVisiblePlusOverscanFragmentCount is int &&
      laidOutVisiblePlusOverscanFragmentCount >= requiredVisibleFragmentCount &&
      requiredLedger != null &&
      readyLedger != null &&
      paintedLedger != null &&
      requiredLedger.coversBounds &&
      readyLedger.coversBounds &&
      paintedLedger.coversBounds &&
      requiredLedger.keys.length == requiredVisibleFragmentCount &&
      paintedLedger.keys.length == rowCount &&
      readyLedger.keys.length == laidOutVisiblePlusOverscanFragmentCount &&
      requiredLedger.keys.length == paintedLedger.keys.length &&
      requiredLedger.keys.containsAll(paintedLedger.keys) &&
      readyLedger.keys.containsAll(requiredLedger.keys) &&
      visibleStart is int &&
      visibleLength is int &&
      visibleLength > 0 &&
      paintedStart is int &&
      paintedEnd is int &&
      paintedEnd > paintedStart &&
      readyStart is int &&
      readyEnd is int &&
      readyEnd > readyStart &&
      readyStart >= visibleStart &&
      readyEnd <= visibleStart + visibleLength &&
      readyStart <= requiredLedger.start &&
      readyEnd >= requiredLedger.end &&
      paintedStart >= readyStart &&
      paintedEnd <= readyEnd &&
      paintedLedger.start == paintedStart &&
      paintedLedger.end == paintedEnd &&
      readyLedger.start == readyStart &&
      readyLedger.end == readyEnd &&
      paint['visiblePlusOverscanSourceSha256'] ==
          paint['expectedVisiblePlusOverscanSourceSha256'] &&
      paint['visibleSourceSha256'] == paint['expectedVisibleSourceSha256'] &&
      expectedExtent is int &&
      paintedStart <= expectedExtent &&
      expectedExtent <= paintedEnd;
}

void _validateStructuralPhase({
  required String id,
  required Object? value,
  required DogfoodCellDenominator denominator,
  required String expectedPhase,
  required int runIndex,
  required bool requireEveryGenerationPaint,
  required bool enforceCadence,
  required bool collectFrameMetrics,
  required num framePeriod,
  required String prefix,
  required List<String> blockers,
  required Map<String, List<int>> metricValues,
}) {
  final phase = _map(value, prefix, blockers);
  final pairCount = denominator.warmups + denominator.samples;
  if (phase['run'] != runIndex || phase['structuralPhase'] != expectedPhase) {
    blockers.add('$prefix has the wrong structural phase identity');
  }
  final sessionIdentity = phase['structuralSessionIdentity'];
  if (sessionIdentity is! String || sessionIdentity.isEmpty) {
    blockers.add('$prefix has no structural session identity');
  }
  if (!_sameJson(
    phase['structuralCommandTranscript'],
    _expectedStructuralTranscript(expectedPhase, pairCount),
  )) {
    blockers.add('$prefix actuator command transcript does not match');
  }
  final commandTranscript = _list(
    phase['structuralCommandTranscript'],
    '$prefix.structuralCommandTranscript',
    blockers,
  );
  final sequenceStart = phase['structuralActuatorSequenceStart'];
  final sequenceEnd = phase['structuralActuatorSequenceEnd'];
  if (sequenceStart is! int ||
      sequenceEnd is! int ||
      sequenceEnd - sequenceStart != commandTranscript.length) {
    blockers.add('$prefix actuator acknowledgement range does not replay');
  }
  _validateStructuralAcknowledgements(
    phase: phase,
    sessionIdentity: sessionIdentity,
    commandTranscript: commandTranscript,
    sequenceStart: sequenceStart,
    sequenceEnd: sequenceEnd,
    prefix: prefix,
    blockers: blockers,
  );
  if (phase['faulted'] != false || phase['resyncCount'] != 0) {
    blockers.add('$prefix faulted or resynchronized');
  }
  final frames = _frames(phase['frames'], prefix, blockers);
  final inputs = _observationsByGeneration(
    phase['inputObservations'],
    '$prefix.inputObservations',
    blockers,
  );
  final engines = _observationsByGeneration(
    phase['engineObservations'],
    '$prefix.engineObservations',
    blockers,
  );
  final paints = _paintObservationsByGeneration(
    phase['paintObservations'],
    '$prefix.paintObservations',
    blockers,
  );
  final generationCount = pairCount * 2;
  for (final entry in <String, Iterable<Map<String, Object?>>>{
    'frame': frames.values,
    'input': inputs.values,
    'engine': engines.values,
    'paint': paints.values.expand((values) => values),
  }.entries) {
    if (entry.value.any(
      (observation) =>
          observation['measurementSessionIdentity'] != sessionIdentity,
    )) {
      blockers.add(
        '$prefix ${entry.key} observations escaped their app-echoed session',
      );
    }
  }
  final expectedKeys = <_SourceKey>{
    for (var generation = 1; generation <= generationCount; generation += 1)
      (sessionOrdinal: 0, sourceGeneration: generation),
  };
  for (final entry in {'input': inputs.keys, 'engine': engines.keys}.entries) {
    if (!entry.value.toSet().containsAll(expectedKeys) ||
        !expectedKeys.containsAll(entry.value)) {
      blockers.add(
        '$prefix ${entry.key} observations do not exactly cover all '
        '$generationCount structural generations',
      );
    }
  }
  if (!expectedKeys.containsAll(paints.keys) ||
      (requireEveryGenerationPaint &&
          !paints.keys.toSet().containsAll(expectedKeys))) {
    blockers.add(
      '$prefix paint observations do not ${requireEveryGenerationPaint ? 'exactly cover' : 'form a subset of'} all structural generations',
    );
  }
  _validateStructuralSummaryShape(
    phase,
    phaseKind: expectedPhase,
    denominator: denominator,
    generationCount: generationCount,
    inputs: inputs,
    prefix: prefix,
    blockers: blockers,
  );

  final preset = id.startsWith('product-tour')
      ? DogfoodDocumentPreset.productTour
      : DogfoodDocumentPreset.prose1MiB;
  final initialSource = _structuralInitialSourceCache.putIfAbsent(
    id,
    () => buildDogfoodDocument(preset),
  );
  final marker = id.startsWith('product-tour')
      ? 'locally.'
      : 'parser catches up.';
  final initialCaret = initialSource.indexOf(marker) + marker.length;
  final expectedSequence = _structuralExpectedSequence(
    cacheKey: '$id/$pairCount',
    source: initialSource,
    caret: initialCaret,
    pairCount: pairCount,
  );
  final firstMeasuredGeneration = denominator.warmups * 2 + 1;
  int? firstPairAccepted;
  int? previousAccepted;
  for (var pair = 0; pair < pairCount; pair += 1) {
    final returnGeneration = pair * 2 + 1;
    final successorGeneration = returnGeneration + 1;
    final returnExpected = expectedSequence[returnGeneration - 1];
    final returnInput =
        inputs[(sessionOrdinal: 0, sourceGeneration: returnGeneration)];
    _validateStructuralInput(
      returnInput,
      generation: returnGeneration,
      expectedSourceSha256: returnExpected.sourceSha256,
      expectedCaret: returnExpected.caret,
      previousAccepted: previousAccepted,
      prefix: '$prefix.pair[$pair].return',
      blockers: blockers,
    );
    final returnAccepted = returnInput?['acceptedMicros'];
    if (returnAccepted is int) {
      firstPairAccepted ??= returnAccepted;
      previousAccepted = returnAccepted;
      if (enforceCadence) {
        final expected =
            firstPairAccepted + pair * _structuralPairCadenceMicros;
        if ((returnAccepted - expected).abs() > 1000) {
          blockers.add('$prefix pair $pair was not accepted on schedule');
        }
      }
    }
    final successorExpected = expectedSequence[successorGeneration - 1];
    final successorInput =
        inputs[(sessionOrdinal: 0, sourceGeneration: successorGeneration)];
    _validateStructuralInput(
      successorInput,
      generation: successorGeneration,
      expectedSourceSha256: successorExpected.sourceSha256,
      expectedCaret: successorExpected.caret,
      previousAccepted: previousAccepted,
      prefix: '$prefix.pair[$pair].successor',
      blockers: blockers,
    );
    final successorAccepted = successorInput?['acceptedMicros'];
    if (returnAccepted is int && successorAccepted is int) {
      final delay = successorAccepted - returnAccepted;
      if (delay < 0 || delay > _structuralImmediateSuccessorMicros) {
        blockers.add('$prefix pair $pair successor was not immediate');
      }
      final successorPaints =
          paints[(sessionOrdinal: 0, sourceGeneration: successorGeneration)];
      if (successorPaints == null || successorPaints.isEmpty) {
        blockers.add('$prefix pair $pair successor never painted');
      } else {
        final firstSuccessorFrame = _firstFrameBuildAtOrAfter(
          frames,
          successorAccepted,
        );
        final orderedSuccessorPaints = [...successorPaints]
          ..sort(
            (left, right) => (left['timestampMicros']! as int).compareTo(
              right['timestampMicros']! as int,
            ),
          );
        final firstSuccessorPaint = orderedSuccessorPaints.first;
        final successorVisibility =
            (firstSuccessorPaint['timestampMicros']! as int) -
            successorAccepted;
        final visibilityBudget = math.min(
          _maxVisibilityMicros,
          framePeriod.round(),
        );
        if (firstSuccessorFrame == null ||
            !successorPaints.any(
              (paint) => paint['frameOrdinal'] == firstSuccessorFrame,
            )) {
          blockers.add('$prefix pair $pair successor missed its next frame');
        }
        if (successorVisibility < 0 || successorVisibility > visibilityBudget) {
          blockers.add(
            '$prefix pair $pair successor exceeded the visibility budget',
          );
        }
        if (collectFrameMetrics &&
            successorGeneration >= firstMeasuredGeneration) {
          metricValues['sourceToPaintMicros']!.add(successorVisibility);
        }
      }
      final returnPaints =
          paints[(sessionOrdinal: 0, sourceGeneration: returnGeneration)];
      if (returnPaints != null && returnPaints.isNotEmpty) {
        final firstReturnFrame = _firstFrameBuildAtOrAfter(
          frames,
          returnAccepted,
        );
        if (firstReturnFrame == null ||
            !returnPaints.any(
              (paint) => paint['frameOrdinal'] == firstReturnFrame,
            ) ||
            returnPaints.any(
              (paint) =>
                  paint['timestampMicros'] is! int ||
                  (paint['timestampMicros']! as int) >= successorAccepted,
            )) {
          blockers.add(
            '$prefix pair $pair Return did not paint on its first opportunity before the successor',
          );
        }
      } else if (!requireEveryGenerationPaint) {
        final hadFrameOpportunity = frames.values.any((frame) {
          final buildStart = _frameBuildStartEpochMicros(frame);
          return buildStart >= returnAccepted && buildStart < successorAccepted;
        });
        if (hadFrameOpportunity) {
          blockers.add(
            '$prefix pair $pair coalesced Return despite a frame opportunity',
          );
        }
      }
      previousAccepted = successorAccepted;
    }
  }

  for (final entry in engines.entries) {
    final nativeMicros = _integer(
      entry.value['nativeFfiMicros'],
      '$prefix.engine.nativeFfiMicros',
      blockers,
    );
    if (collectFrameMetrics &&
        entry.key.sourceGeneration >= firstMeasuredGeneration) {
      metricValues['engineMicros']!.add(nativeMicros);
    }
  }
  for (final entry in paints.entries) {
    final input = inputs[entry.key];
    if (input == null) continue;
    final nextInput =
        inputs[(
          sessionOrdinal: entry.key.sessionOrdinal,
          sourceGeneration: entry.key.sourceGeneration + 1,
        )];
    final nextAcceptedMicros = nextInput?['acceptedMicros'];
    final expectedSourceHash = input['sourceSha256'];
    final expectedBase = input['canonicalSelectionBaseUtf16'];
    final expectedExtent = input['canonicalSelectionExtentUtf16'];
    for (final paint in entry.value) {
      final active = paint['activeRowVisible'] == true;
      final frameOrdinal = paint['frameOrdinal'];
      final frameStamp = paint['frameStampMicros'];
      final visibleStart = paint['visibleUtf16Start'];
      final visibleLength = paint['visibleUtf16Length'];
      String? expectedVisibleHash;
      if (visibleStart is int && visibleLength is int) {
        try {
          expectedVisibleHash = sha256
              .convert(
                utf8.encode(
                  _structuralVisibleSlice(
                    source: initialSource,
                    caret: initialCaret,
                    generation: entry.key.sourceGeneration,
                    start: visibleStart,
                    length: visibleLength,
                  ),
                ),
              )
              .toString();
        } on RangeError {
          expectedVisibleHash = null;
        }
      }
      if (paint['timestampMicros'] is! int ||
          (paint['timestampMicros']! as int) <
              (input['acceptedMicros']! as int) ||
          (nextAcceptedMicros is int &&
              (paint['timestampMicros']! as int) >= nextAcceptedMicros) ||
          expectedVisibleHash == null ||
          visibleLength is! int ||
          visibleLength <= 0 ||
          paint['completeVisibleSurface'] != true ||
          !active ||
          !_paintSurfaceReplays(paint, expectedExtent) ||
          paint['visibleSourceSha256'] != expectedVisibleHash ||
          paint['expectedVisibleSourceSha256'] != expectedVisibleHash ||
          paint['canonicalSelectionBaseUtf16'] != expectedBase ||
          paint['canonicalSelectionExtentUtf16'] != expectedExtent ||
          (active &&
              (expectedExtent is! int ||
                  visibleStart is! int ||
                  expectedExtent < visibleStart ||
                  expectedExtent > visibleStart + visibleLength ||
                  paint['caretSourceUtf16'] != expectedExtent ||
                  paint['caretDisplayUtf16'] == null)) ||
          (paint['activeNeutralRowCount'] as int? ?? 1) != 0 ||
          frameOrdinal is! int ||
          !frames.containsKey(frameOrdinal) ||
          frames[frameOrdinal]?['sessionOrdinal'] != paint['sessionOrdinal'] ||
          frames[frameOrdinal]?['measurementSessionIdentity'] !=
              paint['measurementSessionIdentity'] ||
          frameStamp is! int ||
          _frameOrdinalForPaint(
                paint: paint,
                frames: frames,
                framePeriodMicros: framePeriod.round(),
              ) !=
              frameOrdinal ||
          !_paintClockReplays(
            paint,
            frames[frameOrdinal],
            framePeriodMicros: framePeriod.round(),
          ) ||
          expectedSourceHash is! String) {
        blockers.add(
          '$prefix generation ${entry.key.sourceGeneration} has a stale or raw paint',
        );
      }
    }
  }

  final expectedSyncByFrame = <int, int>{};
  final orderedInputs = inputs.values.toList()
    ..sort(
      (left, right) => (left['sourceGeneration']! as int).compareTo(
        right['sourceGeneration']! as int,
      ),
    );
  for (final input in orderedInputs) {
    final accepted = input['acceptedMicros']! as int;
    final frameOrdinal = _firstFrameBuildAtOrAfter(frames, accepted);
    if (frameOrdinal == null) {
      blockers.add(
        '$prefix generation ${input['sourceGeneration']} has no following frame',
      );
      continue;
    }
    expectedSyncByFrame.update(
      frameOrdinal,
      (value) => value + (input['editorSyncMicros']! as int),
      ifAbsent: () => input['editorSyncMicros']! as int,
    );
  }
  for (final entry in frames.entries) {
    final expectedSync = expectedSyncByFrame[entry.key] ?? 0;
    final frameSync = entry.value['editorSyncMicros'];
    if (frameSync != expectedSync) {
      blockers.add(
        '$prefix frame ${entry.key} does not exactly attribute coalesced '
        'synchronous editor work',
      );
    }
  }

  final terminalKey = (sessionOrdinal: 0, sourceGeneration: generationCount);
  final terminalPaints = paints[terminalKey] ?? const [];
  final terminalCurrent =
      terminalPaints
          .where(
            (paint) =>
                paint['semanticsCurrent'] == true &&
                paint['frameOrdinal'] is int,
          )
          .toList()
        ..sort(
          (left, right) => (left['timestampMicros']! as int).compareTo(
            right['timestampMicros']! as int,
          ),
        );
  if (terminalCurrent.isEmpty) {
    blockers.add('$prefix terminal generation did not paint and certify');
    return;
  }
  final terminalAccepted = inputs[terminalKey]?['acceptedMicros'];
  final terminalTimestamp = terminalCurrent.first['timestampMicros'];
  if (terminalAccepted is! int || terminalTimestamp is! int) {
    blockers.add('$prefix cannot bind terminal certification latency');
  } else {
    final certification = terminalTimestamp - terminalAccepted;
    if (certification < 0 || certification >= _maxCertificationMicros) {
      blockers.add('$prefix terminal certification exceeded 500 ms');
    }
    if (collectFrameMetrics) {
      metricValues['visibleCertificationMicros']!.add(certification);
    }
  }
  if (collectFrameMetrics) {
    final firstAccepted =
        inputs[(
          sessionOrdinal: 0,
          sourceGeneration: firstMeasuredGeneration,
        )]?['acceptedMicros'];
    final terminalFrame = terminalCurrent.first['frameOrdinal'];
    if (firstAccepted is! int || terminalFrame is! int) {
      blockers.add('$prefix cannot bind its complete burst frame interval');
      return;
    }
    final firstFrame = _firstFrameBuildAtOrAfter(frames, firstAccepted);
    if (firstFrame == null) {
      blockers.add('$prefix has no frame after burst acceptance');
      return;
    }
    _recordMeasuredFrames(
      frames,
      {
        for (var ordinal = firstFrame; ordinal <= terminalFrame; ordinal += 1)
          ordinal,
      },
      prefix,
      blockers,
      metricValues,
    );
  }
}

const _structuralPairCadenceMicros = 33333;
const _structuralImmediateSuccessorMicros = 30000;

List<String> _expectedStructuralTranscript(String phase, int pairCount) {
  final result = <String>[];
  switch (phase) {
    case 'latency':
      for (var index = 0; index < pairCount; index += 1) {
        result
          ..add('typeStructuralBursts:1:0')
          ..add('settle');
      }
      break;
    case 'burst':
      result.add(
        'typeStructuralBursts:$pairCount:$_structuralPairCadenceMicros',
      );
      break;
    case 'perEditControl':
      for (var index = 0; index < pairCount; index += 1) {
        result
          ..add('pressKey:enter')
          ..add('settle')
          ..add('typeText:x:0')
          ..add('settle');
      }
      break;
    default:
      return const [];
  }
  return result
    ..add('settle')
    ..add('closeSession');
}

void _validateStructuralSummaryShape(
  Map<String, Object?> phase, {
  required String phaseKind,
  required DogfoodCellDenominator denominator,
  required int generationCount,
  required Map<_SourceKey, Map<String, Object?>> inputs,
  required String prefix,
  required List<String> blockers,
}) {
  final warmups = _list(phase['warmups'], '$prefix.warmups', blockers);
  final samples = _list(phase['samples'], '$prefix.samples', blockers);
  if (phaseKind == 'burst') {
    if (warmups.isNotEmpty || samples.length != 1) {
      blockers.add('$prefix burst summary must be one aggregate sample');
      return;
    }
    final sample = _map(samples.single, '$prefix.samples[0]', blockers);
    final generations = _acceptedGenerations(
      sample,
      '$prefix.samples[0]',
      blockers,
    );
    final expected = [
      for (var generation = 1; generation <= generationCount; generation += 1)
        generation,
    ];
    if (sample['index'] != 0 ||
        sample['sourceGeneration'] != generationCount ||
        !_sameJson(generations, expected) ||
        sample['scheduleAcceptedMicros'] !=
            inputs[(
              sessionOrdinal: 0,
              sourceGeneration: 1,
            )]?['acceptedMicros'] ||
        sample['acceptedMicros'] !=
            inputs[(
              sessionOrdinal: 0,
              sourceGeneration: generationCount,
            )]?['acceptedMicros']) {
      blockers.add('$prefix burst summary does not cover the exact sequence');
    }
    if (sample['faulted'] != false || sample['resyncCount'] != 0) {
      blockers.add('$prefix burst sample faulted or resynchronized');
    }
    return;
  }
  if (warmups.length != denominator.warmups ||
      samples.length != denominator.samples) {
    blockers.add('$prefix does not preserve the frozen pair denominator');
    return;
  }
  for (
    var operation = 0;
    operation < denominator.warmups + denominator.samples;
    operation += 1
  ) {
    final isWarmup = operation < denominator.warmups;
    final index = isWarmup ? operation : operation - denominator.warmups;
    final collection = isWarmup ? warmups : samples;
    final summary = _map(
      collection[index],
      '$prefix.${isWarmup ? 'warmups' : 'samples'}[$index]',
      blockers,
    );
    final expectedGenerations = [operation * 2 + 1, operation * 2 + 2];
    final generations = _acceptedGenerations(
      summary,
      '$prefix.summary[$operation]',
      blockers,
    );
    final returnInput =
        inputs[(
          sessionOrdinal: 0,
          sourceGeneration: expectedGenerations.first,
        )];
    final successorInput =
        inputs[(sessionOrdinal: 0, sourceGeneration: expectedGenerations.last)];
    if (summary['index'] != index ||
        summary['sourceGeneration'] != expectedGenerations.last ||
        !_sameJson(generations, expectedGenerations) ||
        summary['scheduleAcceptedMicros'] != returnInput?['acceptedMicros'] ||
        summary['acceptedMicros'] != successorInput?['acceptedMicros'] ||
        summary['sourceSha256'] != successorInput?['sourceSha256'] ||
        summary['canonicalSelectionBaseUtf16'] !=
            successorInput?['canonicalSelectionBaseUtf16'] ||
        summary['canonicalSelectionExtentUtf16'] !=
            successorInput?['canonicalSelectionExtentUtf16']) {
      blockers.add(
        '$prefix summary $operation does not bind one exact Return+x pair',
      );
    }
    if (!isWarmup &&
        (summary['faulted'] != false || summary['resyncCount'] != 0)) {
      blockers.add('$prefix summary $operation faulted or resynchronized');
    }
  }
}

String _structuralVisibleSlice({
  required String source,
  required int caret,
  required int generation,
  required int start,
  required int length,
}) {
  final completedPairs = generation ~/ 2;
  final insertion =
      '${List.filled(completedPairs, '\n\nx').join()}'
      '${generation.isOdd ? '\n\n' : ''}';
  final resultLength = source.length + insertion.length;
  final end = start + length;
  if (start < 0 || length < 0 || end > resultLength) {
    throw RangeError.range(end, start, resultLength);
  }
  final buffer = StringBuffer();
  void appendOverlap(
    int segmentStart,
    int segmentEnd,
    String segment,
    int segmentSourceStart,
  ) {
    final overlapStart = math.max(start, segmentStart);
    final overlapEnd = math.min(end, segmentEnd);
    if (overlapStart >= overlapEnd) return;
    buffer.write(
      segment.substring(
        segmentSourceStart + overlapStart - segmentStart,
        segmentSourceStart + overlapEnd - segmentStart,
      ),
    );
  }

  appendOverlap(0, caret, source, 0);
  appendOverlap(caret, caret + insertion.length, insertion, 0);
  appendOverlap(caret + insertion.length, resultLength, source, caret);
  return buffer.toString();
}

void _validateStructuralInput(
  Map<String, Object?>? input, {
  required int generation,
  required String expectedSourceSha256,
  required int expectedCaret,
  required int? previousAccepted,
  required String prefix,
  required List<String> blockers,
}) {
  if (input == null) {
    blockers.add('$prefix is missing');
    return;
  }
  final accepted = input['acceptedMicros'];
  if (input['sourceGeneration'] != generation ||
      input['sourceSha256'] != expectedSourceSha256 ||
      input['canonicalSelectionBaseUtf16'] != expectedCaret ||
      input['canonicalSelectionExtentUtf16'] != expectedCaret ||
      accepted is! int ||
      (previousAccepted != null && accepted < previousAccepted)) {
    blockers.add('$prefix does not match the exact parser-authored transition');
  }
}

List<_StructuralExpected> _structuralExpectedSequence({
  required String cacheKey,
  required String source,
  required int caret,
  required int pairCount,
}) => _structuralExpectedCache.putIfAbsent(cacheKey, () {
  var current = source;
  var currentCaret = caret;
  final result = <_StructuralExpected>[];
  for (var pair = 0; pair < pairCount; pair += 1) {
    current = current.replaceRange(currentCaret, currentCaret, '\n\n');
    currentCaret += 2;
    result.add((
      sourceSha256: sha256.convert(utf8.encode(current)).toString(),
      caret: currentCaret,
    ));
    current = current.replaceRange(currentCaret, currentCaret, 'x');
    currentCaret += 1;
    result.add((
      sourceSha256: sha256.convert(utf8.encode(current)).toString(),
      caret: currentCaret,
    ));
  }
  return result;
});

int? _firstFrameBuildAtOrAfter(
  Map<int, Map<String, Object?>> frames,
  int acceptedMicros,
) {
  final candidates =
      frames.entries
          .where(
            (entry) =>
                _frameBuildStartEpochMicros(entry.value) >= acceptedMicros,
          )
          .toList()
        ..sort(
          (left, right) => _frameBuildStartEpochMicros(
            left.value,
          ).compareTo(_frameBuildStartEpochMicros(right.value)),
        );
  return candidates.isEmpty ? null : candidates.first.key;
}

void _validateOpenObservation(
  Object? value, {
  required String id,
  required bool required,
  required Map<int, Map<String, Object?>> frames,
  required Map<_SourceKey, Map<String, Object?>> rawInputs,
  required Map<_SourceKey, List<Map<String, Object?>>> rawPaints,
  required Set<String> runSessionIdentities,
  required num framePeriod,
  required Set<int> measuredFrameOrdinals,
  required String prefix,
  required List<String> blockers,
  required Map<String, List<int>> metricValues,
}) {
  if (!required) {
    if (value != null) blockers.add('$prefix is not declared by this cell');
    return;
  }
  if (value == null) {
    blockers.add('$prefix is required');
    return;
  }
  final observation = _map(value, prefix, blockers);
  final openSessionIdentity = observation['measurementSessionIdentity'];
  if (openSessionIdentity is! String ||
      !runSessionIdentities.contains(openSessionIdentity)) {
    blockers.add('$prefix escaped its app-authored measurement session');
  }
  final expectedKind = id == 'product-tour-cold-launch'
      ? 'processLaunch'
      : 'presetSelection';
  if (observation['kind'] != expectedKind) {
    blockers.add('$prefix must measure $expectedKind');
  }
  final accepted = _integer(
    observation['acceptedMicros'],
    '$prefix.acceptedMicros',
    blockers,
  );
  final paint = _integer(
    observation['paintMicros'],
    '$prefix.paintMicros',
    blockers,
  );
  final elapsed = _integer(
    observation['openToEditableMicros'],
    '$prefix.openToEditableMicros',
    blockers,
  );
  if (paint < accepted || paint - accepted != elapsed) {
    blockers.add('$prefix timing does not replay');
  }
  metricValues['openToEditableMicros']!.add(elapsed);
  if (elapsed >= _maxOpenMicros) {
    blockers.add('$prefix open exceeded 200 ms');
  }
  if (observation['sourceGeneration'] != 0 ||
      observation['canonicalSelectionBaseUtf16'] !=
          observation['expectedSelectionBaseUtf16'] ||
      observation['canonicalSelectionExtentUtf16'] !=
          observation['expectedSelectionExtentUtf16'] ||
      observation['semanticsCurrent'] != true ||
      observation['activeNeutralRowCount'] != 0) {
    blockers.add('$prefix is not an exact certified initial paint');
  }
  final visibleStart = observation['visibleUtf16Start'];
  final visibleLength = observation['visibleUtf16Length'];
  final openSource = _openSourceCache.putIfAbsent(id, () {
    final source = buildDogfoodDocument(_openPresetForCell(id));
    return (
      source: source,
      sha256: sha256.convert(utf8.encode(source)).toString(),
    );
  });
  final expectedSource = openSource.source;
  String? expectedVisibleHash;
  if (visibleStart is int && visibleLength is int) {
    try {
      expectedVisibleHash = sha256
          .convert(
            utf8.encode(
              expectedSource.substring(
                visibleStart,
                visibleStart + visibleLength,
              ),
            ),
          )
          .toString();
    } on RangeError {
      expectedVisibleHash = null;
    }
  }
  final expectedBase = observation['expectedSelectionBaseUtf16'];
  final expectedExtent = observation['expectedSelectionExtentUtf16'];
  if (observation['sourceSha256'] != openSource.sha256 ||
      expectedVisibleHash == null ||
      visibleLength is! int ||
      visibleLength <= 0 ||
      !_paintSurfaceReplays(observation, expectedExtent) ||
      observation['visibleSourceSha256'] != expectedVisibleHash ||
      observation['expectedVisibleSourceSha256'] != expectedVisibleHash) {
    blockers.add('$prefix source identity does not match its frozen preset');
  }
  if (expectedBase == expectedExtent &&
      (expectedExtent is! int ||
          visibleStart is! int ||
          visibleLength is! int ||
          expectedExtent < visibleStart ||
          expectedExtent > visibleStart + visibleLength ||
          observation['caretSourceUtf16'] != expectedExtent ||
          observation['caretDisplayUtf16'] == null)) {
    blockers.add('$prefix has no identity-preserving caret');
  }
  final frameOrdinal = _integer(
    observation['frameOrdinal'],
    '$prefix.frameOrdinal',
    blockers,
  );
  final openKey = (sessionOrdinal: 0, sourceGeneration: 0);
  final rawInput = rawInputs[openKey];
  if (rawInput == null ||
      rawInput['measurementSessionIdentity'] != openSessionIdentity ||
      rawInput['acceptedMicros'] != accepted ||
      rawInput['sourceSha256'] != openSource.sha256 ||
      rawInput['canonicalSelectionBaseUtf16'] != expectedBase ||
      rawInput['canonicalSelectionExtentUtf16'] != expectedExtent) {
    blockers.add('$prefix does not match its raw generation-zero acceptance');
  }
  final qualifyingPaints = [...rawPaints[openKey] ?? const []]
    ..removeWhere(
      (candidate) =>
          candidate['timestampMicros'] is! int ||
          (candidate['timestampMicros']! as int) < accepted ||
          candidate['semanticsCurrent'] != true ||
          candidate['activeNeutralRowCount'] != 0 ||
          candidate['activeRowVisible'] != true ||
          !_paintSurfaceReplays(
            candidate,
            observation['expectedSelectionExtentUtf16'],
          ) ||
          (candidate['visibleUtf16Length'] as int? ?? 0) <= 0,
    )
    ..sort(
      (left, right) => (left['timestampMicros']! as int).compareTo(
        right['timestampMicros']! as int,
      ),
    );
  if (qualifyingPaints.isEmpty) {
    blockers.add('$prefix has no raw generation-zero proving paint');
  } else {
    final rawPaint = qualifyingPaints.first;
    final replayedFrame = _frameOrdinalForPaint(
      paint: rawPaint,
      frames: frames,
      framePeriodMicros: framePeriod.round(),
    );
    final replayedFrameValue = replayedFrame == null
        ? null
        : frames[replayedFrame];
    if (rawPaint['timestampMicros'] != paint ||
        rawPaint['measurementSessionIdentity'] != openSessionIdentity ||
        replayedFrameValue?['measurementSessionIdentity'] !=
            openSessionIdentity ||
        rawPaint['frameOrdinal'] != frameOrdinal ||
        replayedFrame != frameOrdinal ||
        !_paintClockReplays(
          rawPaint,
          replayedFrameValue,
          framePeriodMicros: framePeriod.round(),
        ) ||
        rawPaint['visibleUtf16Start'] != visibleStart ||
        rawPaint['visibleUtf16Length'] != visibleLength ||
        rawPaint['visibleSourceSha256'] != expectedVisibleHash ||
        rawPaint['canonicalSelectionBaseUtf16'] != expectedBase ||
        rawPaint['canonicalSelectionExtentUtf16'] != expectedExtent ||
        rawPaint['caretSourceUtf16'] != observation['caretSourceUtf16'] ||
        rawPaint['caretDisplayUtf16'] != observation['caretDisplayUtf16'] ||
        rawPaint['paintMonotonicMicros'] !=
            observation['paintMonotonicMicros'] ||
        rawPaint['paintEpochBeforeMicros'] !=
            observation['paintEpochBeforeMicros'] ||
        rawPaint['paintEpochAfterMicros'] !=
            observation['paintEpochAfterMicros'] ||
        rawPaint['paintedRowCount'] != observation['paintedRowCount'] ||
        rawPaint['requiredVisibleFragmentCount'] !=
            observation['requiredVisibleFragmentCount'] ||
        rawPaint['laidOutVisiblePlusOverscanFragmentCount'] !=
            observation['laidOutVisiblePlusOverscanFragmentCount'] ||
        jsonEncode(rawPaint['requiredVisibleFragments']) !=
            jsonEncode(observation['requiredVisibleFragments']) ||
        jsonEncode(rawPaint['laidOutVisiblePlusOverscanFragments']) !=
            jsonEncode(observation['laidOutVisiblePlusOverscanFragments']) ||
        jsonEncode(rawPaint['paintedFragments']) !=
            jsonEncode(observation['paintedFragments']) ||
        rawPaint['paintedSourceUtf16Start'] !=
            observation['paintedSourceUtf16Start'] ||
        rawPaint['paintedSourceUtf16End'] !=
            observation['paintedSourceUtf16End'] ||
        rawPaint['completeVisiblePlusOverscanSurface'] !=
            observation['completeVisiblePlusOverscanSurface'] ||
        rawPaint['visiblePlusOverscanUtf16Start'] !=
            observation['visiblePlusOverscanUtf16Start'] ||
        rawPaint['visiblePlusOverscanUtf16End'] !=
            observation['visiblePlusOverscanUtf16End'] ||
        rawPaint['visiblePlusOverscanSourceSha256'] !=
            observation['visiblePlusOverscanSourceSha256'] ||
        rawPaint['expectedVisiblePlusOverscanSourceSha256'] !=
            observation['expectedVisiblePlusOverscanSourceSha256']) {
      blockers.add('$prefix does not replay its earliest raw proving paint');
    }
  }
  if (!frames.containsKey(frameOrdinal)) {
    blockers.add('$prefix references missing frame $frameOrdinal');
  } else {
    final firstFrame = _firstFrameBuildAtOrAfter(frames, accepted);
    if (firstFrame == null || firstFrame > frameOrdinal) {
      blockers.add('$prefix cannot bind the complete cold-open frame interval');
    } else {
      measuredFrameOrdinals.addAll([
        for (var ordinal = firstFrame; ordinal <= frameOrdinal; ordinal += 1)
          ordinal,
      ]);
    }
  }
}

DogfoodDocumentPreset _openPresetForCell(String id) => switch (id) {
  'product-tour-cold-launch' => DogfoodDocumentPreset.productTour,
  'dense-blocks-1m-journey' => DogfoodDocumentPreset.denseBlocks1MiB,
  'ordinary-5m-journey' => DogfoodDocumentPreset.prose5MiB,
  'giant-line-5m-journey' => DogfoodDocumentPreset.giantLine5MiB,
  'ordinary-10m-journey' ||
  'streamed-10m-journey' => DogfoodDocumentPreset.prose10MiB,
  _ => throw StateError('$id has no frozen open preset'),
};

void _validateWarmup(
  Map<String, Object?> warmup,
  int expectedIndex, {
  required List<Map<String, Object?>?> rawInputs,
  required List<Map<String, Object?>?> rawEngines,
  required bool requiresInput,
  required String prefix,
  required List<String> blockers,
}) {
  if (warmup['index'] != expectedIndex) {
    blockers.add('$prefix has wrong index');
  }
  final generation = _integer(
    warmup['sourceGeneration'],
    '$prefix.sourceGeneration',
    blockers,
  );
  final acceptedGenerations = _acceptedGenerations(
    warmup,
    prefix,
    blockers,
    allowZero: !requiresInput,
  );
  final accepted = _integer(
    warmup['acceptedMicros'],
    '$prefix.acceptedMicros',
    blockers,
  );
  final presentInputs = rawInputs.whereType<Map<String, Object?>>().toList()
    ..sort(
      (left, right) => (left['sourceGeneration']! as int).compareTo(
        right['sourceGeneration']! as int,
      ),
    );
  if (presentInputs.length != acceptedGenerations.length) {
    blockers.add('$prefix does not have one raw input per accepted generation');
  }
  final summaryInputs = presentInputs.where(
    (input) => input['sourceGeneration'] == generation,
  );
  final summaryInput = summaryInputs.length == 1 ? summaryInputs.single : null;
  if (presentInputs.isEmpty || summaryInput == null) {
    blockers.add('$prefix has no raw input observation');
  } else if (summaryInput['acceptedMicros'] != accepted ||
      summaryInput['sourceSha256'] != warmup['sourceSha256'] ||
      summaryInput['canonicalSelectionBaseUtf16'] !=
          warmup['canonicalSelectionBaseUtf16'] ||
      summaryInput['canonicalSelectionExtentUtf16'] !=
          warmup['canonicalSelectionExtentUtf16']) {
    blockers.add('$prefix does not match its raw input observations');
  }
  final presentEngines = rawEngines.whereType<Map<String, Object?>>().toList();
  if (presentEngines.length != acceptedGenerations.length) {
    blockers.add('$prefix does not have one engine receipt per generation');
  }
  for (var index = 0; index < presentEngines.length; index += 1) {
    _integer(
      presentEngines[index]['nativeFfiMicros'],
      '$prefix.rawEngine[$index].nativeFfiMicros',
      blockers,
    );
  }
  final engineMicros = _integer(
    warmup['engineMicros'],
    '$prefix.engineMicros',
    blockers,
  );
  final summaryEngines = presentEngines.where(
    (engine) => engine['sourceGeneration'] == generation,
  );
  if (summaryEngines.length != 1 ||
      summaryEngines.single['nativeFfiMicros'] != engineMicros) {
    blockers.add('$prefix engine timing does not replay');
  }
}

int? _nextAcceptedMicros(List<Map<String, Object?>> declared, int index) =>
    index + 1 >= declared.length
    ? null
    : (declared[index + 1]['scheduleAcceptedMicros'] ??
              declared[index + 1]['acceptedMicros'])
          as int?;

List<int> _acceptedGenerations(
  Map<String, Object?> sample,
  String prefix,
  List<String> blockers, {
  bool allowZero = false,
}) {
  final values = _list(
    sample['acceptedSourceGenerations'],
    '$prefix.acceptedSourceGenerations',
    blockers,
  );
  final result = <int>[];
  var previous = -1;
  for (var index = 0; index < values.length; index += 1) {
    final generation = _integer(
      values[index],
      '$prefix.acceptedSourceGenerations[$index]',
      blockers,
    );
    if ((!allowZero && generation == 0) || generation <= previous) {
      blockers.add('$prefix accepted source generations are not increasing');
    }
    result.add(generation);
    previous = generation;
  }
  if (result.isEmpty || !result.contains(sample['sourceGeneration'])) {
    blockers.add(
      '$prefix measured source generation is not in its accepted sequence',
    );
  }
  return result;
}

Map<_SourceKey, Map<String, Object?>> _observationsByGeneration(
  Object? value,
  String prefix,
  List<String> blockers, {
  bool allowZero = false,
}) {
  final values = _list(value, prefix, blockers);
  final result = <_SourceKey, Map<String, Object?>>{};
  for (var index = 0; index < values.length; index += 1) {
    final observation = _map(values[index], '$prefix[$index]', blockers);
    final generation = _integer(
      observation['sourceGeneration'],
      '$prefix[$index].sourceGeneration',
      blockers,
    );
    final sessionOrdinal = _integer(
      observation['sessionOrdinal'],
      '$prefix[$index].sessionOrdinal',
      blockers,
    );
    final key = (sessionOrdinal: sessionOrdinal, sourceGeneration: generation);
    if ((!allowZero && generation == 0) || result.containsKey(key)) {
      blockers.add('$prefix has a duplicate or zero source generation');
      continue;
    }
    result[key] = observation;
  }
  return result;
}

Map<_SourceKey, List<Map<String, Object?>>> _paintObservationsByGeneration(
  Object? value,
  String prefix,
  List<String> blockers, {
  bool allowZero = false,
}) {
  final values = _list(value, prefix, blockers);
  final result = <_SourceKey, List<Map<String, Object?>>>{};
  var previousTimestamp = -1;
  for (var index = 0; index < values.length; index += 1) {
    final observation = _map(values[index], '$prefix[$index]', blockers);
    final generation = _integer(
      observation['sourceGeneration'],
      '$prefix[$index].sourceGeneration',
      blockers,
    );
    final sessionOrdinal = _integer(
      observation['sessionOrdinal'],
      '$prefix[$index].sessionOrdinal',
      blockers,
    );
    final timestamp = _integer(
      observation['timestampMicros'],
      '$prefix[$index].timestampMicros',
      blockers,
    );
    if (!allowZero && generation == 0) {
      blockers.add('$prefix[$index] has a zero source generation');
      continue;
    }
    if (timestamp < previousTimestamp) {
      blockers.add('$prefix paint timestamps are not ordered');
    }
    previousTimestamp = timestamp;
    result
        .putIfAbsent((
          sessionOrdinal: sessionOrdinal,
          sourceGeneration: generation,
        ), () => [])
        .add(observation);
  }
  return result;
}

Map<int, Map<String, Object?>> _frames(
  Object? value,
  String prefix,
  List<String> blockers,
) {
  final values = _list(value, '$prefix.frames', blockers);
  final result = <int, Map<String, Object?>>{};
  var previousVsync = -1;
  var previousMonotonicVsync = -1;
  for (var index = 0; index < values.length; index += 1) {
    final frame = _map(values[index], '$prefix.frames[$index]', blockers);
    final ordinal = _integer(
      frame['ordinal'],
      '$prefix.frames[$index].ordinal',
      blockers,
    );
    if (ordinal != index) {
      blockers.add('$prefix frame ordinals are not contiguous');
    }
    if (result.containsKey(ordinal)) {
      blockers.add('$prefix has duplicate frame $ordinal');
    }
    result[ordinal] = frame;
    final vsync = _integer(frame['vsyncMicros'], 'frame.vsyncMicros', blockers);
    if (vsync <= previousVsync) {
      blockers.add('$prefix frame vsync timestamps are not increasing');
    }
    previousVsync = vsync;
    final monotonicVsync = _integer(
      frame['monotonicVsyncMicros'],
      'frame.monotonicVsyncMicros',
      blockers,
    );
    if (monotonicVsync <= previousMonotonicVsync) {
      blockers.add('$prefix frame monotonic timestamps are not increasing');
    }
    previousMonotonicVsync = monotonicVsync;
    final buildStart = _integer(
      frame['buildStartMonotonicMicros'],
      'frame.buildStartMonotonicMicros',
      blockers,
    );
    final buildFinish = _integer(
      frame['buildFinishMonotonicMicros'],
      'frame.buildFinishMonotonicMicros',
      blockers,
    );
    final build = _integer(frame['buildMicros'], 'frame.buildMicros', blockers);
    if (buildStart < monotonicVsync ||
        buildFinish < buildStart ||
        buildFinish - buildStart != build) {
      blockers.add('$prefix frame $ordinal has an invalid build interval');
    }
    final anchorBefore = _integer(
      frame['clockAnchorEpochBeforeMicros'],
      'frame.clockAnchorEpochBeforeMicros',
      blockers,
    );
    final anchorAfter = _integer(
      frame['clockAnchorEpochAfterMicros'],
      'frame.clockAnchorEpochAfterMicros',
      blockers,
    );
    final anchorMonotonic = _integer(
      frame['clockAnchorMonotonicMicros'],
      'frame.clockAnchorMonotonicMicros',
      blockers,
    );
    if (anchorAfter < anchorBefore || anchorAfter - anchorBefore >= 1000) {
      blockers.add('$prefix frame clock anchor is not a tight bracket');
    }
    final mappedBefore = anchorBefore + monotonicVsync - anchorMonotonic;
    final mappedAfter = anchorAfter + monotonicVsync - anchorMonotonic;
    if (vsync < mappedBefore || vsync > mappedAfter) {
      blockers.add('$prefix frame epoch timestamp does not replay its clock');
    }
    final raster = _integer(
      frame['rasterMicros'],
      'frame.rasterMicros',
      blockers,
    );
    final sync = _integer(
      frame['editorSyncMicros'],
      'frame.editorSyncMicros',
      blockers,
    );
    // Parsing the complete frame stream is separate from selecting the exact
    // measured subset. Warmup frames must remain in the raw receipt but must
    // not affect D0 percentiles.
    if (build < 0 || raster < 0 || sync < 0) {
      blockers.add('$prefix frame $ordinal contains a negative duration');
    }
  }
  return result;
}

void _recordMeasuredFrames(
  Map<int, Map<String, Object?>> frames,
  Set<int> ordinals,
  String prefix,
  List<String> blockers,
  Map<String, List<int>> metricValues,
) {
  final ordered = ordinals.toList()..sort();
  for (final ordinal in ordered) {
    final frame = frames[ordinal];
    if (frame == null) continue;
    if (frame['editorAttributed'] != true) {
      blockers.add('$prefix frame $ordinal was not editor-attributed');
      continue;
    }
    final build = frame['buildMicros']! as int;
    final raster = frame['rasterMicros']! as int;
    final sync = frame['editorSyncMicros']! as int;
    metricValues['flutterFrameMicros']!.add(build + raster);
    metricValues['editorSpanMicros']!.add(build + raster + sync);
    if (frame['missed'] == true) {
      blockers.add('$prefix frame $ordinal missed');
    }
    if (build + raster + sync >= _maxEditorSpanMicros) {
      blockers.add('$prefix frame $ordinal exceeded the editor span budget');
    }
  }
}

int _frameBuildStartEpochMicros(Map<String, Object?> frame) {
  final epochBefore = frame['clockAnchorEpochBeforeMicros']! as int;
  final epochAfter = frame['clockAnchorEpochAfterMicros']! as int;
  final anchorMonotonic = frame['clockAnchorMonotonicMicros']! as int;
  final buildStartMonotonic = frame['buildStartMonotonicMicros']! as int;
  return epochBefore +
      ((epochAfter - epochBefore) ~/ 2) +
      buildStartMonotonic -
      anchorMonotonic;
}

void _validateSample(
  Map<String, Object?> sample,
  int expectedIndex,
  Map<int, Map<String, Object?>> frames, {
  required List<Map<String, Object?>?> rawInputs,
  required List<Map<String, Object?>?> rawEngines,
  required List<Map<String, Object?>> rawPaints,
  required Map<String, Object?>? supersedingInput,
  required int? nextAcceptedMicros,
  required int? paintIntervalEndMicros,
  required bool collectMetrics,
  required bool requiresInput,
  required bool allowIntermediatePaintCoalescing,
  required Set<int> measuredFrameOrdinals,
  required num framePeriod,
  required bool requiresLiveStateZero,
  required String prefix,
  required List<String> blockers,
  required Map<String, List<int>> metricValues,
}) {
  if (sample['index'] != expectedIndex) blockers.add('$prefix has wrong index');
  final generation = _integer(
    sample['sourceGeneration'],
    '$prefix.sourceGeneration',
    blockers,
  );
  final visibilityDisposition = sample['visibilityDisposition'];
  final supersededByValue = sample['supersededBySourceGeneration'];
  final supersededBeforeFrame =
      visibilityDisposition == 'superseded-before-frame';
  if (visibilityDisposition != 'painted' && !supersededBeforeFrame) {
    blockers.add('$prefix has an invalid visibility disposition');
  }
  if ((!supersededBeforeFrame && supersededByValue != null) ||
      (supersededBeforeFrame && supersededByValue is! int)) {
    blockers.add('$prefix has an invalid superseding generation');
  }
  final paintedGeneration = supersededBeforeFrame && supersededByValue is int
      ? supersededByValue
      : generation;
  final acceptedGenerations = _acceptedGenerations(
    sample,
    prefix,
    blockers,
    allowZero: !requiresInput,
  );
  final accepted = _integer(
    sample['acceptedMicros'],
    '$prefix.acceptedMicros',
    blockers,
  );
  int visibilityAccepted = accepted;
  if (supersededBeforeFrame) {
    if (!requiresInput ||
        acceptedGenerations.length != 1 ||
        paintedGeneration != generation + 1 ||
        supersedingInput == null ||
        supersedingInput['sourceGeneration'] != paintedGeneration ||
        supersedingInput['acceptedMicros'] != nextAcceptedMicros) {
      blockers.add('$prefix has invalid before-frame supersession lineage');
    } else {
      visibilityAccepted = _integer(
        supersedingInput['acceptedMicros'],
        '$prefix.supersedingInput.acceptedMicros',
        blockers,
      );
      final followingFrames =
          frames.values
              .where((frame) => _frameBuildStartEpochMicros(frame) >= accepted)
              .toList()
            ..sort(
              (left, right) => _frameBuildStartEpochMicros(
                left,
              ).compareTo(_frameBuildStartEpochMicros(right)),
            );
      if (followingFrames.isEmpty ||
          visibilityAccepted >=
              _frameBuildStartEpochMicros(followingFrames.first)) {
        blockers.add('$prefix was not superseded before its first frame');
      }
    }
  }
  final sourcePaint = _integer(
    sample['sourcePaintMicros'],
    '$prefix.sourcePaintMicros',
    blockers,
  );
  final caretPaint = _integer(
    sample['caretPaintMicros'],
    '$prefix.caretPaintMicros',
    blockers,
  );
  final selectionPaint = _integer(
    sample['selectionPaintMicros'],
    '$prefix.selectionPaintMicros',
    blockers,
  );
  final visibility =
      math.max(sourcePaint, math.max(caretPaint, selectionPaint)) -
      visibilityAccepted;
  if (collectMetrics && requiresInput && !supersededBeforeFrame) {
    metricValues['sourceToPaintMicros']!.add(visibility);
  }
  final visibilityBudget = math.min(_maxVisibilityMicros, framePeriod.round());
  if (sourcePaint < visibilityAccepted ||
      caretPaint < visibilityAccepted ||
      selectionPaint < visibilityAccepted ||
      (requiresInput && visibility > visibilityBudget)) {
    blockers.add(
      '$prefix did not paint source/caret/selection by the next frame',
    );
  }
  if (sample['paintedSourceGeneration'] != paintedGeneration ||
      sample['sourceIdentityMatched'] != true ||
      sample['caretIdentityMatched'] != true ||
      sample['selectionIdentityMatched'] != true) {
    blockers.add('$prefix painted a torn source/selection generation');
  }
  for (final name in const ['sourceSha256', 'visibleSourceSha256']) {
    final value = sample[name];
    if (value is! String || !_shaPattern.hasMatch(value)) {
      blockers.add('$prefix.$name must be a lowercase SHA-256');
    }
  }
  final presentInputs = rawInputs.whereType<Map<String, Object?>>().toList();
  if (presentInputs.length != acceptedGenerations.length) {
    blockers.add('$prefix does not have one raw input per accepted generation');
  }
  presentInputs.sort(
    (left, right) => (left['sourceGeneration']! as int).compareTo(
      right['sourceGeneration']! as int,
    ),
  );
  for (var index = 0; index < presentInputs.length; index += 1) {
    _integer(
      presentInputs[index]['editorSyncMicros'],
      '$prefix.rawInput[$index].editorSyncMicros',
      blockers,
    );
  }
  final summaryInputs = presentInputs.where(
    (input) => input['sourceGeneration'] == generation,
  );
  final summaryInput = summaryInputs.length == 1 ? summaryInputs.single : null;
  if (presentInputs.isEmpty || summaryInput == null) {
    blockers.add('$prefix has no raw input observation');
  } else if (summaryInput['acceptedMicros'] != accepted ||
      summaryInput['sourceSha256'] != sample['sourceSha256'] ||
      summaryInput['canonicalSelectionBaseUtf16'] !=
          sample['canonicalSelectionBaseUtf16'] ||
      summaryInput['canonicalSelectionExtentUtf16'] !=
          sample['canonicalSelectionExtentUtf16']) {
    blockers.add('$prefix does not match its raw input observations');
  }

  final orderedPaints = [...rawPaints]
    ..sort(
      (left, right) => (left['timestampMicros']! as int).compareTo(
        right['timestampMicros']! as int,
      ),
    );
  if (orderedPaints.isEmpty) {
    blockers.add('$prefix has no raw paint observation');
  } else {
    if (supersededBeforeFrame &&
        orderedPaints.any((paint) => paint['sourceGeneration'] == generation)) {
      blockers.add('$prefix claims supersession for a generation that painted');
    }
    for (final acceptedGeneration in acceptedGenerations) {
      if (supersededBeforeFrame && acceptedGeneration == generation) {
        continue;
      }
      if (allowIntermediatePaintCoalescing &&
          acceptedGeneration != generation) {
        continue;
      }
      final generationPaints = orderedPaints.where(
        (paint) => paint['sourceGeneration'] == acceptedGeneration,
      );
      if (generationPaints.isEmpty) {
        blockers.add(
          '$prefix accepted generation $acceptedGeneration never painted',
        );
      } else if (!generationPaints.any(
        (paint) => paint['frameOrdinal'] is int,
      )) {
        blockers.add(
          '$prefix accepted generation $acceptedGeneration has no '
          'FrameTiming join',
        );
      }
    }
    final finalPaints = orderedPaints
        .where((paint) => paint['sourceGeneration'] == paintedGeneration)
        .toList();
    if (finalPaints.isEmpty) {
      blockers.add('$prefix has no final-generation paint observation');
      return;
    }
    final firstPaint = finalPaints.first;
    final joinedFinalPaint = finalPaints.cast<Map<String, Object?>>().where(
      (paint) => paint['frameOrdinal'] is int,
    );
    if (joinedFinalPaint.isEmpty) {
      blockers.add('$prefix has no proving paint joined to FrameTiming');
      return;
    }
    final provingPaint = joinedFinalPaint.first;
    final firstTimestamp = _integer(
      firstPaint['timestampMicros'],
      '$prefix.rawPaint.timestampMicros',
      blockers,
    );
    final firstFrame = _integer(
      provingPaint['frameOrdinal'],
      '$prefix.rawPaint.frameOrdinal',
      blockers,
    );
    int? certificationTimestamp;
    var rawProjectionFrames = 0;
    final acceptanceByGeneration = <int, int>{
      for (final input in presentInputs)
        input['sourceGeneration']! as int: input['acceptedMicros']! as int,
      if (supersedingInput != null)
        supersedingInput['sourceGeneration']! as int:
            supersedingInput['acceptedMicros']! as int,
    };
    for (
      var paintIndex = 0;
      paintIndex < orderedPaints.length;
      paintIndex += 1
    ) {
      final paint = orderedPaints[paintIndex];
      final timestamp = _integer(
        paint['timestampMicros'],
        '$prefix.rawPaint[$paintIndex].timestampMicros',
        blockers,
      );
      final frameValue = paint['frameOrdinal'];
      final paintAccepted = acceptanceByGeneration[paint['sourceGeneration']];
      if (paintAccepted == null ||
          timestamp < paintAccepted ||
          (paintIntervalEndMicros != null &&
              timestamp >= paintIntervalEndMicros)) {
        blockers.add(
          '$prefix contains a paint outside its acceptance interval',
        );
      }
      if (frameValue != null) {
        final frameOrdinal = _integer(
          frameValue,
          '$prefix.rawPaint[$paintIndex].frameOrdinal',
          blockers,
        );
        if (!frames.containsKey(frameOrdinal)) {
          blockers.add(
            '$prefix raw paint references missing frame $frameOrdinal',
          );
        }
        final joinedFrame = frames[frameOrdinal];
        if (joinedFrame?['sessionOrdinal'] != paint['sessionOrdinal'] ||
            joinedFrame?['measurementSessionIdentity'] !=
                paint['measurementSessionIdentity']) {
          blockers.add(
            '$prefix raw paint $paintIndex joined a foreign-session frame',
          );
        }
        _integer(
          paint['frameStampMicros'],
          '$prefix.rawPaint[$paintIndex].frameStampMicros',
          blockers,
        );
        final expectedFrame = _frameOrdinalForPaint(
          paint: paint,
          frames: frames,
          framePeriodMicros: framePeriod.round(),
        );
        if (expectedFrame != frameOrdinal) {
          blockers.add(
            '$prefix raw paint $paintIndex does not replay its FrameTiming join',
          );
        }
        if (!_paintClockReplays(
          paint,
          frames[frameOrdinal],
          framePeriodMicros: framePeriod.round(),
        )) {
          blockers.add(
            '$prefix raw paint $paintIndex does not replay its paint clock',
          );
        }
        if (collectMetrics && requiresInput) {
          measuredFrameOrdinals.add(frameOrdinal);
        }
      }
      final expectedBase = paint['expectedSelectionBaseUtf16'];
      final expectedExtent = paint['expectedSelectionExtentUtf16'];
      final collapsed = expectedBase == expectedExtent;
      final activeRowVisible = paint['activeRowVisible'] == true;
      final visibleStart = paint['visibleUtf16Start'];
      final visibleLength = paint['visibleUtf16Length'];
      if ((!acceptedGenerations.contains(paint['sourceGeneration']) &&
              paint['sourceGeneration'] != paintedGeneration) ||
          visibleStart is! int ||
          visibleLength is! int ||
          visibleLength <= 0 ||
          paint['completeVisibleSurface'] != true ||
          paint['visibleSourceSha256'] !=
              paint['expectedVisibleSourceSha256'] ||
          paint['canonicalSelectionBaseUtf16'] != expectedBase ||
          paint['canonicalSelectionExtentUtf16'] != expectedExtent ||
          (collapsed &&
              activeRowVisible &&
              (expectedExtent is! int ||
                  expectedExtent < visibleStart ||
                  expectedExtent > visibleStart + visibleLength ||
                  paint['caretSourceUtf16'] != expectedExtent ||
                  paint['caretDisplayUtf16'] == null)) ||
          (collapsed &&
              activeRowVisible &&
              !_paintSurfaceReplays(paint, expectedExtent)) ||
          !activeRowVisible) {
        blockers.add('$prefix raw paint $paintIndex is torn or stale');
      }
      final neutral = _integer(
        paint['activeNeutralRowCount'],
        '$prefix.rawPaint[$paintIndex].activeNeutralRowCount',
        blockers,
      );
      if (paint['sourceGeneration'] == paintedGeneration && neutral > 0) {
        rawProjectionFrames += 1;
      }
      if (paint['sourceGeneration'] == paintedGeneration &&
          paint['semanticsCurrent'] == true) {
        certificationTimestamp ??= timestamp;
      }
    }
    if (firstTimestamp != sourcePaint ||
        firstTimestamp != caretPaint ||
        firstTimestamp != selectionPaint ||
        firstFrame != sample['provingFrameOrdinal'] ||
        firstPaint['sourceGeneration'] != sample['paintedSourceGeneration'] ||
        firstPaint['visibleSourceSha256'] != sample['visibleSourceSha256'] ||
        firstPaint['caretSourceUtf16'] != sample['paintedCaretSourceUtf16']) {
      blockers.add('$prefix summary does not replay from its raw paints');
    }
    if (rawProjectionFrames != sample['rawProjectionFrames']) {
      blockers.add('$prefix raw projection count does not replay');
    }
    if (certificationTimestamp == null) {
      blockers.add('$prefix has no current-semantics paint');
    } else if (certificationTimestamp - visibilityAccepted !=
        sample['visibleCertificationMicros']) {
      blockers.add('$prefix certification time does not replay');
    }
  }
  final start = _integer(
    sample['startFrameOrdinal'],
    '$prefix.startFrameOrdinal',
    blockers,
  );
  final end = _integer(
    sample['endFrameOrdinal'],
    '$prefix.endFrameOrdinal',
    blockers,
  );
  final proving = _integer(
    sample['provingFrameOrdinal'],
    '$prefix.provingFrameOrdinal',
    blockers,
  );
  final expectedStartFrames =
      frames.values
          .where((frame) => _frameBuildStartEpochMicros(frame) >= accepted)
          .toList()
        ..sort(
          (left, right) => _frameBuildStartEpochMicros(
            left,
          ).compareTo(_frameBuildStartEpochMicros(right)),
        );
  if (expectedStartFrames.isEmpty) {
    blockers.add('$prefix acceptance has no following frame interval');
  } else if (start != expectedStartFrames.first['ordinal']) {
    blockers.add('$prefix frame interval does not begin at acceptance');
  }
  if (proving != start) {
    blockers.add('$prefix visible result missed its first frame opportunity');
  }
  if (start > proving || proving > end) {
    blockers.add('$prefix proving frame is outside its interval');
  }
  for (var ordinal = start; ordinal <= end; ordinal += 1) {
    if (!frames.containsKey(ordinal)) {
      blockers.add('$prefix is missing frame $ordinal');
    }
    if (collectMetrics && requiresInput) {
      measuredFrameOrdinals.add(ordinal);
    }
  }
  if (frames[proving] == null) {
    blockers.add('$prefix proving frame does not exist');
  } else if (summaryInput != null) {
    final rawEditorSync = _integer(
      summaryInput['editorSyncMicros'],
      '$prefix.rawInput.editorSyncMicros',
      blockers,
    );
    final frameEditorSync = _integer(
      frames[proving]!['editorSyncMicros'],
      '$prefix.provingFrame.editorSyncMicros',
      blockers,
    );
    if (frameEditorSync < rawEditorSync) {
      blockers.add('$prefix proving frame omits synchronous editor work');
    }
  }
  final engineMicros = _integer(
    sample['engineMicros'],
    '$prefix.engineMicros',
    blockers,
  );
  final certification = _integer(
    sample['visibleCertificationMicros'],
    '$prefix.visibleCertificationMicros',
    blockers,
  );
  final presentEngines = rawEngines.whereType<Map<String, Object?>>().toList();
  if (presentEngines.length != acceptedGenerations.length) {
    blockers.add('$prefix does not have one engine receipt per generation');
  }
  final rawNativeFfiMicros = <int>[];
  for (var index = 0; index < presentEngines.length; index += 1) {
    rawNativeFfiMicros.add(
      _integer(
        presentEngines[index]['nativeFfiMicros'],
        '$prefix.rawEngine[$index].nativeFfiMicros',
        blockers,
      ),
    );
  }
  final summaryEngines = presentEngines.where(
    (engine) => engine['sourceGeneration'] == generation,
  );
  if (summaryEngines.length != 1 ||
      summaryEngines.single['nativeFfiMicros'] != engineMicros) {
    blockers.add('$prefix engine timing does not replay');
  }
  if (collectMetrics && requiresInput) {
    metricValues['engineMicros']!.addAll(rawNativeFfiMicros);
    metricValues['visibleCertificationMicros']!.add(certification);
  }
  if (certification >= _maxCertificationMicros) {
    blockers.add('$prefix certification exceeded 500 ms');
  }
  if (sample['rawProjectionFrames'] != 0) {
    blockers.add('$prefix painted an undeclared raw projection');
  }
  if (sample['faulted'] != false || sample['resyncCount'] != 0) {
    blockers.add('$prefix faulted or resynchronized');
  }
  if (requiresLiveStateZero) {
    final live = _map(
      sample['globalLiveState'],
      '$prefix.globalLiveState',
      blockers,
    );
    for (final name in const [
      'sessions',
      'transactions',
      'continuations',
      'anchors',
      'historyTokens',
    ]) {
      if (live[name] != 0) blockers.add('$prefix leaked native $name');
    }
  }
}

int? _frameOrdinalForPaint({
  required Map<String, Object?> paint,
  required Map<int, Map<String, Object?>> frames,
  required int framePeriodMicros,
}) {
  final paintMonotonic = paint['paintMonotonicMicros'];
  final frameStamp = paint['frameStampMicros'];
  if (paintMonotonic is! int || frameStamp is! int) return null;
  final containing = <MapEntry<int, Map<String, Object?>>>[];
  for (final entry in frames.entries) {
    final start = entry.value['buildStartMonotonicMicros'];
    final finish = entry.value['buildFinishMonotonicMicros'];
    if (start is! int || finish is! int) continue;
    if (paintMonotonic >= start && paintMonotonic <= finish) {
      containing.add(entry);
    }
  }
  final ranked = <({int distance, int ordinal})>[];
  for (final entry in containing) {
    final vsync = entry.value['monotonicVsyncMicros'];
    if (vsync is! int) continue;
    final distance = (vsync + framePeriodMicros - frameStamp).abs();
    if (distance <= framePeriodMicros ~/ 8) {
      ranked.add((distance: distance, ordinal: entry.key));
    }
  }
  ranked.sort((left, right) => left.distance.compareTo(right.distance));
  if (ranked.isEmpty ||
      (ranked.length > 1 && ranked[0].distance == ranked[1].distance)) {
    return null;
  }
  return ranked.first.ordinal;
}

void _validateCadence(
  List<Object?> samples,
  num cadenceHz,
  String prefix,
  List<String> blockers,
) {
  if (cadenceHz == 0 || samples.length < 2) return;
  final first = _map(samples.first, '$prefix.samples[0]', blockers);
  final firstScheduled = first['scheduledMicros'];
  final firstAccepted = first['scheduleAcceptedMicros'];
  if (firstScheduled is! int || firstAccepted is! int) {
    blockers.add('$prefix cadence samples require scheduledMicros');
    return;
  }
  final acceptanceOffset = firstAccepted - firstScheduled;
  for (var index = 0; index < samples.length; index += 1) {
    final sample = _map(samples[index], '$prefix.samples[$index]', blockers);
    final scheduled = sample['scheduledMicros'];
    final accepted = sample['scheduleAcceptedMicros'];
    final expected =
        firstScheduled + (index * _microsPerSecond / cadenceHz).round();
    if (scheduled is! int || (scheduled - expected).abs() > 1) {
      blockers.add(
        '$prefix sample $index violates the ${cadenceHz}Hz schedule',
      );
    }
    if (scheduled is int &&
        (accepted is! int ||
            (accepted - scheduled - acceptanceOffset).abs() > 1000)) {
      blockers.add('$prefix sample $index was not accepted on schedule');
    }
  }
}

void _validateMemory(
  Object? value,
  int sourceBytes,
  String prefix,
  List<String> blockers,
  Map<String, List<int>> metricValues,
) {
  final values = _list(value, '$prefix.memory', blockers);
  final stages = <String, Map<String, Object?>>{};
  for (final value in values) {
    final sample = _map(value, '$prefix.memory[]', blockers);
    final stage = sample['stage'];
    if (stage is! String || stages.containsKey(stage)) {
      blockers.add('$prefix memory stages must be unique');
      continue;
    }
    stages[stage] = sample;
  }
  const required = ['baseline', 'peak', 'close', 'postClose'];
  if (!stages.keys.toSet().containsAll(required)) {
    blockers.add('$prefix memory receipt is incomplete');
    return;
  }
  var priorTimestamp = -1;
  for (final stage in required) {
    final timestamp = _integer(
      stages[stage]!['timestampMicros'],
      '$prefix.memory.$stage.timestamp',
      blockers,
    );
    if (timestamp < priorTimestamp) {
      blockers.add('$prefix memory stages are out of order');
    }
    priorTimestamp = timestamp;
  }
  final baseline = _integer(
    stages['baseline']!['rssBytes'],
    '$prefix.memory.baseline.rss',
    blockers,
  );
  final peak = _integer(
    stages['peak']!['rssBytes'],
    '$prefix.memory.peak.rss',
    blockers,
  );
  final close = _integer(
    stages['close']!['rssBytes'],
    '$prefix.memory.close.rss',
    blockers,
  );
  final postClose = _integer(
    stages['postClose']!['rssBytes'],
    '$prefix.memory.postClose.rss',
    blockers,
  );
  if (peak < baseline || peak < close) {
    blockers.add('$prefix active-run peak RSS is lower than an active stage');
  }
  final peakDelta = peak - baseline;
  final retainedDelta = math.max(0, postClose - baseline);
  metricValues['peakRssDeltaBytes']!.add(peakDelta);
  metricValues['retainedRssDeltaBytes']!.add(retainedDelta);
  if (sourceBytes >= _largeSourceBytes) {
    final peakBudget = math.max(64 * 1024 * 1024, sourceBytes * 8);
    if (peakDelta > peakBudget) {
      blockers.add('$prefix peak RSS exceeded $peakBudget bytes');
    }
    if (retainedDelta > _maxRetainedRssBytes) {
      blockers.add('$prefix retained RSS exceeded 16 MiB');
    }
  }
}

Map<String, Object> _distribution(List<int> values) {
  final sorted = [...values]..sort();
  int percentile(int value) {
    if (sorted.isEmpty) return 0;
    final index = ((sorted.length * value + 99) ~/ 100 - 1).clamp(
      0,
      sorted.length - 1,
    );
    return sorted[index];
  }

  return {
    'sampleCount': sorted.length,
    'p50': percentile(50),
    'p90': percentile(90),
    'p99': percentile(99),
    'max': sorted.isEmpty ? 0 : sorted.last,
  };
}

int _percentile(List<int> values, int percentile) {
  if (values.isEmpty) return 0;
  final sorted = [...values]..sort();
  final index = ((sorted.length * percentile + 99) ~/ 100 - 1).clamp(
    0,
    sorted.length - 1,
  );
  return sorted[index];
}

Map<String, Object?> _map(Object? value, String path, List<String> blockers) {
  if (value is Map<String, Object?>) return value;
  blockers.add('$path must be an object');
  return <String, Object?>{};
}

List<Object?> _list(Object? value, String path, List<String> blockers) {
  if (value is List<Object?>) return value;
  blockers.add('$path must be an array');
  return const [];
}

int _integer(Object? value, String path, List<String> blockers) {
  if (value is int && value >= 0) return value;
  blockers.add('$path must be a nonnegative integer');
  return 0;
}

num _number(Object? value, String path, List<String> blockers) {
  if (value is num && value > 0) return value;
  blockers.add('$path must be positive');
  return 0;
}

Future<void> _verifyFileIdentity(
  Object? value,
  String path,
  List<String> blockers,
) async {
  final identity = _map(value, path, blockers);
  final filePath = identity['path'];
  if (filePath is! String || filePath.isEmpty) {
    blockers.add('$path.path must be nonempty');
    return;
  }
  final file = File(filePath);
  if (!await file.exists()) {
    blockers.add('$path file does not exist: $filePath');
    return;
  }
  final bytes = await file.length();
  final digest = (await sha256.bind(file.openRead()).first).toString();
  if (identity['bytes'] != bytes || identity['sha256'] != digest) {
    blockers.add('$path identity does not match $filePath');
  }
}

Future<String> _git(
  Directory repository,
  List<String> arguments,
  List<String> blockers,
) async {
  final result = await Process.run(
    'git',
    arguments,
    workingDirectory: repository.path,
  );
  if (result.exitCode != 0) {
    blockers.add('git ${arguments.join(' ')} failed');
    return '';
  }
  return (result.stdout as String).trim();
}

bool _sameJson(Object? left, Object? right) =>
    jsonEncode(left) == jsonEncode(right);

Future<void> main(List<String> arguments) async {
  if (arguments.length != 2) {
    stderr.writeln(
      'usage: dart run scripts/verify_v4_dogfood_receipt.dart '
      '<repository> <dogfood-performance.json>',
    );
    exitCode = 64;
    return;
  }
  try {
    final decoded = jsonDecode(await File(arguments[1]).readAsString());
    if (decoded is! Map<String, Object?>) {
      throw const FormatException('receipt root must be an object');
    }
    final result = await verifyDogfoodPerformanceReceipt(
      decoded,
      repository: Directory(arguments[0]),
    );
    if (!result.passed) {
      for (final blocker in result.blockers) {
        stderr.writeln('dogfood-performance: $blocker');
      }
      throw StateError('${result.blockers.length} receipt blocker(s)');
    }
    stdout.writeln('dogfood-performance: PASS ${jsonEncode(result.metrics)}');
  } on Object catch (error) {
    stderr.writeln('dogfood-performance: FAIL $error');
    exitCode = 1;
  }
}
