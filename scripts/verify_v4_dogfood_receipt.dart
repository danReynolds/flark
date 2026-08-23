import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:crypto/crypto.dart';

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
    cadenceHz: 60,
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
    cadenceHz: 60,
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
  final metricValues = <String, List<int>>{
    'sourceToPaintMicros': [],
    'engineMicros': [],
    'flutterFrameMicros': [],
    'editorSpanMicros': [],
    'visibleCertificationMicros': [],
    'openToEditableMicros': [],
    'peakRssDeltaBytes': [],
    'retainedRssDeltaBytes': [],
  };

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
    _validateCell(
      entry.key,
      cell,
      entry.value,
      framePeriod: framePeriod,
      blockers: blockers,
      metricValues: metricValues,
    );
  }

  if (_percentile(metricValues['engineMicros']!, 99) > _maxEngineP99Micros) {
    blockers.add('Rust engine p99 exceeded 4 ms');
  }
  if (_percentile(metricValues['flutterFrameMicros']!, 99) >
      _maxFlutterP99Micros) {
    blockers.add('Flutter frame work p99 exceeded 8 ms');
  }

  final metrics = <String, Object>{
    for (final entry in metricValues.entries)
      entry.key: _distribution(entry.value),
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

void _validateCell(
  String id,
  Map<String, Object?> cell,
  DogfoodCellDenominator denominator, {
  required num framePeriod,
  required List<String> blockers,
  required Map<String, List<int>> metricValues,
}) {
  final prefix = 'cell[$id]';
  final sourceBytes = _integer(
    cell['sourceBytes'],
    '$prefix.sourceBytes',
    blockers,
  );
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
      allowZero: !denominator.requiresInput,
    );
    final engineObservations = _observationsByGeneration(
      run['engineObservations'],
      '$runPrefix.engineObservations',
      blockers,
      allowZero: !denominator.requiresInput,
    );
    final paintObservations = _paintObservationsByGeneration(
      run['paintObservations'],
      '$runPrefix.paintObservations',
      blockers,
      allowZero: !denominator.requiresInput,
    );
    final declared = <Map<String, Object?>>[
      for (var index = 0; index < warmups.length; index += 1)
        _map(warmups[index], '$runPrefix.warmups[$index]', blockers),
      for (var index = 0; index < samples.length; index += 1)
        _map(samples[index], '$runPrefix.samples[$index]', blockers),
    ];
    final declaredGenerations = <int>{};
    for (var index = 0; index < declared.length; index += 1) {
      for (final generation in _acceptedGenerations(
        declared[index],
        '$runPrefix.declared[$index]',
        blockers,
        allowZero: !denominator.requiresInput,
      )) {
        if (!declaredGenerations.add(generation)) {
          blockers.add('$runPrefix declared source generations must be unique');
        }
      }
    }
    for (final entry in <String, Set<int>>{
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
    final paintedGenerations = paintObservations.keys.toSet();
    if (!declaredGenerations.containsAll(paintedGenerations)) {
      blockers.add('$runPrefix paints contain an undeclared source generation');
    }
    for (var index = 0; index < declared.length; index += 1) {
      final finalGeneration = declared[index]['sourceGeneration'];
      if (finalGeneration is! int ||
          !paintObservations.containsKey(finalGeneration)) {
        blockers.add(
          '$runPrefix declared[$index] has no final-generation paint',
        );
      }
    }
    for (var warmupIndex = 0; warmupIndex < warmups.length; warmupIndex += 1) {
      final sample = declared[warmupIndex];
      final generations = _acceptedGenerations(
        sample,
        '$runPrefix.warmup[$warmupIndex]',
        blockers,
        allowZero: !denominator.requiresInput,
      );
      _validateSample(
        sample,
        warmupIndex,
        frames,
        rawInputs: [
          for (final generation in generations) inputObservations[generation],
        ],
        rawEngines: [
          for (final generation in generations) engineObservations[generation],
        ],
        rawPaints: [
          for (final generation in generations)
            ...paintObservations[generation] ?? const [],
        ],
        nextAcceptedMicros: _nextAcceptedMicros(declared, warmupIndex),
        collectMetrics: false,
        requiresInput: denominator.requiresInput,
        measuredFrameOrdinals: measuredFrameOrdinals,
        framePeriod: framePeriod,
        requiresLiveStateZero: false,
        requiresOpen: false,
        prefix: '$runPrefix.warmup[$warmupIndex]',
        blockers: blockers,
        metricValues: metricValues,
      );
    }
    _validateCadence(samples, denominator.cadenceHz, runPrefix, blockers);
    for (var sampleIndex = 0; sampleIndex < samples.length; sampleIndex += 1) {
      final declaredIndex = warmups.length + sampleIndex;
      final sample = declared[declaredIndex];
      final generations = _acceptedGenerations(
        sample,
        '$runPrefix.sample[$sampleIndex]',
        blockers,
        allowZero: !denominator.requiresInput,
      );
      _validateSample(
        sample,
        sampleIndex,
        frames,
        rawInputs: [
          for (final generation in generations) inputObservations[generation],
        ],
        rawEngines: [
          for (final generation in generations) engineObservations[generation],
        ],
        rawPaints: [
          for (final generation in generations)
            ...paintObservations[generation] ?? const [],
        ],
        nextAcceptedMicros: _nextAcceptedMicros(declared, declaredIndex),
        collectMetrics: true,
        requiresInput: denominator.requiresInput,
        measuredFrameOrdinals: measuredFrameOrdinals,
        framePeriod: framePeriod,
        requiresLiveStateZero: denominator.requiresLiveStateZero,
        requiresOpen: denominator.requiresOpen,
        prefix: '$runPrefix.sample[$sampleIndex]',
        blockers: blockers,
        metricValues: metricValues,
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
}

int? _nextAcceptedMicros(List<Map<String, Object?>> declared, int index) =>
    index + 1 >= declared.length
    ? null
    : declared[index + 1]['acceptedMicros'] as int?;

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
  if (result.isEmpty || sample['sourceGeneration'] != result.last) {
    blockers.add('$prefix final source generation does not match its sequence');
  }
  return result;
}

Map<int, Map<String, Object?>> _observationsByGeneration(
  Object? value,
  String prefix,
  List<String> blockers, {
  bool allowZero = false,
}) {
  final values = _list(value, prefix, blockers);
  final result = <int, Map<String, Object?>>{};
  for (var index = 0; index < values.length; index += 1) {
    final observation = _map(values[index], '$prefix[$index]', blockers);
    final generation = _integer(
      observation['sourceGeneration'],
      '$prefix[$index].sourceGeneration',
      blockers,
    );
    if ((!allowZero && generation == 0) || result.containsKey(generation)) {
      blockers.add('$prefix has a duplicate or zero source generation');
      continue;
    }
    result[generation] = observation;
  }
  return result;
}

Map<int, List<Map<String, Object?>>> _paintObservationsByGeneration(
  Object? value,
  String prefix,
  List<String> blockers, {
  bool allowZero = false,
}) {
  final values = _list(value, prefix, blockers);
  final result = <int, List<Map<String, Object?>>>{};
  var previousTimestamp = -1;
  for (var index = 0; index < values.length; index += 1) {
    final observation = _map(values[index], '$prefix[$index]', blockers);
    final generation = _integer(
      observation['sourceGeneration'],
      '$prefix[$index].sourceGeneration',
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
    result.putIfAbsent(generation, () => []).add(observation);
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
    final build = _integer(frame['buildMicros'], 'frame.buildMicros', blockers);
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
    if (frame == null || frame['editorAttributed'] != true) continue;
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

void _validateSample(
  Map<String, Object?> sample,
  int expectedIndex,
  Map<int, Map<String, Object?>> frames, {
  required List<Map<String, Object?>?> rawInputs,
  required List<Map<String, Object?>?> rawEngines,
  required List<Map<String, Object?>> rawPaints,
  required int? nextAcceptedMicros,
  required bool collectMetrics,
  required bool requiresInput,
  required Set<int> measuredFrameOrdinals,
  required num framePeriod,
  required bool requiresLiveStateZero,
  required bool requiresOpen,
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
      math.max(sourcePaint, math.max(caretPaint, selectionPaint)) - accepted;
  if (collectMetrics && requiresInput) {
    metricValues['sourceToPaintMicros']!.add(visibility);
  }
  final visibilityBudget = math.min(_maxVisibilityMicros, framePeriod.round());
  if (sourcePaint < accepted ||
      caretPaint < accepted ||
      selectionPaint < accepted ||
      (requiresInput && visibility > visibilityBudget)) {
    blockers.add(
      '$prefix did not paint source/caret/selection by the next frame',
    );
  }
  if (sample['sourceGeneration'] != sample['paintedSourceGeneration'] ||
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
  final firstInput = presentInputs.isEmpty ? null : presentInputs.first;
  final finalInput = presentInputs.isEmpty ? null : presentInputs.last;
  if (firstInput == null || finalInput == null) {
    blockers.add('$prefix has no raw input observation');
  } else if (firstInput['acceptedMicros'] != accepted ||
      finalInput['sourceGeneration'] != generation ||
      finalInput['sourceSha256'] != sample['sourceSha256'] ||
      finalInput['canonicalSelectionBaseUtf16'] !=
          sample['canonicalSelectionBaseUtf16'] ||
      finalInput['canonicalSelectionExtentUtf16'] !=
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
    final finalPaints = orderedPaints
        .where((paint) => paint['sourceGeneration'] == generation)
        .toList();
    if (finalPaints.isEmpty) {
      blockers.add('$prefix has no final-generation paint observation');
      return;
    }
    final firstPaint = finalPaints.first;
    final firstTimestamp = _integer(
      firstPaint['timestampMicros'],
      '$prefix.rawPaint.timestampMicros',
      blockers,
    );
    final firstFrame = _integer(
      firstPaint['frameOrdinal'],
      '$prefix.rawPaint.frameOrdinal',
      blockers,
    );
    int? certificationTimestamp;
    var rawProjectionFrames = 0;
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
      final frameOrdinal = _integer(
        paint['frameOrdinal'],
        '$prefix.rawPaint[$paintIndex].frameOrdinal',
        blockers,
      );
      if (timestamp < accepted ||
          (nextAcceptedMicros != null && timestamp >= nextAcceptedMicros)) {
        blockers.add(
          '$prefix contains a paint outside its acceptance interval',
        );
      }
      if (!frames.containsKey(frameOrdinal)) {
        blockers.add(
          '$prefix raw paint references missing frame $frameOrdinal',
        );
      }
      if (collectMetrics && requiresInput) {
        measuredFrameOrdinals.add(frameOrdinal);
      }
      final expectedBase = paint['expectedSelectionBaseUtf16'];
      final expectedExtent = paint['expectedSelectionExtentUtf16'];
      final collapsed = expectedBase == expectedExtent;
      if (!acceptedGenerations.contains(paint['sourceGeneration']) ||
          paint['visibleSourceSha256'] !=
              paint['expectedVisibleSourceSha256'] ||
          paint['canonicalSelectionBaseUtf16'] != expectedBase ||
          paint['canonicalSelectionExtentUtf16'] != expectedExtent ||
          (collapsed &&
              (paint['caretSourceUtf16'] != expectedExtent ||
                  paint['caretDisplayUtf16'] == null))) {
        blockers.add('$prefix raw paint $paintIndex is torn or stale');
      }
      final neutral = _integer(
        paint['activeNeutralRowCount'],
        '$prefix.rawPaint[$paintIndex].activeNeutralRowCount',
        blockers,
      );
      if (paint['sourceGeneration'] == generation && neutral > 0) {
        rawProjectionFrames += 1;
      }
      if (paint['sourceGeneration'] == generation &&
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
    } else if (certificationTimestamp - accepted !=
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
  if (start > proving || proving > end) {
    blockers.add('$prefix proving frame is outside its interval');
  }
  for (var ordinal = start; ordinal <= end; ordinal += 1) {
    if (!frames.containsKey(ordinal)) {
      blockers.add('$prefix is missing frame $ordinal');
    }
  }
  if (frames[proving] == null) {
    blockers.add('$prefix proving frame does not exist');
  } else if (finalInput != null) {
    final rawEditorSync = _integer(
      finalInput['editorSyncMicros'],
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
  if (rawNativeFfiMicros.isEmpty ||
      rawNativeFfiMicros.reduce(math.max) != engineMicros) {
    blockers.add('$prefix engine timing does not replay');
  }
  if (collectMetrics && requiresInput) {
    metricValues['engineMicros']!.addAll(rawNativeFfiMicros);
    metricValues['visibleCertificationMicros']!.add(certification);
  }
  if (certification >= _maxCertificationMicros) {
    blockers.add('$prefix certification exceeded 500 ms');
  }
  final open = sample['openToEditableMicros'];
  if (requiresOpen && open == null) {
    blockers.add('$prefix requires an open-to-editable measurement');
  }
  if (open != null) {
    final openMicros = _integer(open, '$prefix.openToEditableMicros', blockers);
    metricValues['openToEditableMicros']!.add(openMicros);
    if (openMicros >= _maxOpenMicros) {
      blockers.add('$prefix open exceeded 200 ms');
    }
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

void _validateCadence(
  List<Object?> samples,
  num cadenceHz,
  String prefix,
  List<String> blockers,
) {
  if (cadenceHz == 0 || samples.length < 2) return;
  final first = _map(samples.first, '$prefix.samples[0]', blockers);
  final firstScheduled = first['scheduledMicros'];
  final firstAccepted = first['acceptedMicros'];
  if (firstScheduled is! int || firstAccepted is! int) {
    blockers.add('$prefix cadence samples require scheduledMicros');
    return;
  }
  final acceptanceOffset = firstAccepted - firstScheduled;
  for (var index = 0; index < samples.length; index += 1) {
    final sample = _map(samples[index], '$prefix.samples[$index]', blockers);
    final scheduled = sample['scheduledMicros'];
    final accepted = sample['acceptedMicros'];
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
  if (peak < baseline || peak < close || peak < postClose) {
    blockers.add('$prefix peak RSS is lower than another memory stage');
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
