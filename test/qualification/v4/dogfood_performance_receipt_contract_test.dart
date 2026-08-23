// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import '../../../scripts/dogfood_fixture_identity.dart';
import '../../../scripts/verify_v4_dogfood_receipt.dart';
import '../../../scripts/dogfood_performance_receipt.dart';

void main() {
  test('schema and replay freeze the complete D0 denominator', () async {
    final schema =
        jsonDecode(
              File(
                'docs/testing/dogfood_performance_v1.schema.json',
              ).readAsStringSync(),
            )
            as Map<String, Object?>;
    expect(schema[r'$schema'], 'https://json-schema.org/draft/2020-12/schema');
    expect(schema['additionalProperties'], isFalse);

    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final result = await verifyDogfoodPerformanceReceipt(
      sealed,
      verifyArtifactFiles: false,
    );
    expect(result.blockers, isEmpty);
    expect((sealed['assessment']! as Map)['result'], 'PASS');
    expect(
      (result.metrics['sourceToPaintMicros']! as Map)['sampleCount'],
      3040,
    );
    expect((result.metrics['flutterFrameMicros']! as Map)['sampleCount'], 3875);
    expect((result.metrics['engineMicros']! as Map)['sampleCount'], 3870);
    expect((result.metrics['openToEditableMicros']! as Map)['sampleCount'], 25);
  });

  test('replay fails closed on timing and denominator tampering', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final timingTampered = _copy(sealed);
    final sample = _firstSample(timingTampered);
    sample['sourcePaintMicros'] = (sample['acceptedMicros']! as int) + 20000;
    final timingResult = await verifyDogfoodPerformanceReceipt(
      timingTampered,
      verifyArtifactFiles: false,
    );
    expect(
      timingResult.blockers.join('\n'),
      contains('did not paint source/caret/selection by the next frame'),
    );
    expect(
      timingResult.blockers.join('\n'),
      contains('assessment.metrics does not match replayed metrics'),
    );

    final denominatorTampered = _copy(sealed);
    _cells(denominatorTampered).first['samplesPerRun'] = 119;
    final denominatorResult = await verifyDogfoodPerformanceReceipt(
      denominatorTampered,
      verifyArtifactFiles: false,
    );
    expect(
      denominatorResult.blockers.join('\n'),
      contains('denominator does not match the frozen D0 matrix'),
    );
  });

  test('replay rejects a forged assessment and missing open proof', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final forged = _copy(sealed);
    final metrics = (forged['assessment']! as Map)['metrics']! as Map;
    (metrics['engineMicros']! as Map)['p99'] = 0;
    final forgedResult = await verifyDogfoodPerformanceReceipt(
      forged,
      verifyArtifactFiles: false,
    );
    expect(
      forgedResult.blockers,
      contains('assessment.metrics does not match replayed metrics'),
    );

    final missingOpen = _copy(sealed);
    final cold = _cells(
      missingOpen,
    ).firstWhere((cell) => cell['id'] == 'product-tour-cold-launch');
    final coldRun = (cold['runs']! as List).first as Map;
    coldRun['openObservation'] = null;
    final openResult = await verifyDogfoodPerformanceReceipt(
      missingOpen,
      verifyArtifactFiles: false,
    );
    expect(
      openResult.blockers.join('\n'),
      contains('openObservation is required'),
    );
  });

  test('replay rejects raw paint, input, and engine disagreement', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );

    final paintTampered = _copy(sealed);
    final paintRun = _firstRun(paintTampered);
    final paint = (paintRun['paintObservations']! as List).first as Map;
    paint['expectedVisibleSourceSha256'] = _hash('d');
    final paintResult = await verifyDogfoodPerformanceReceipt(
      paintTampered,
      verifyArtifactFiles: false,
    );
    expect(paintResult.blockers.join('\n'), contains('raw paint 0 is torn'));

    final inputTampered = _copy(sealed);
    final inputRun = _firstRun(inputTampered);
    final input = (inputRun['inputObservations']! as List).first as Map;
    input['sourceSha256'] = _hash('d');
    final inputResult = await verifyDogfoodPerformanceReceipt(
      inputTampered,
      verifyArtifactFiles: false,
    );
    expect(
      inputResult.blockers.join('\n'),
      contains('does not match its raw input observations'),
    );

    final engineTampered = _copy(sealed);
    final engineRun = _firstRun(engineTampered);
    final engine = (engineRun['engineObservations']! as List).first as Map;
    engine['nativeFfiMicros'] = 101;
    final engineResult = await verifyDogfoodPerformanceReceipt(
      engineTampered,
      verifyArtifactFiles: false,
    );
    expect(
      engineResult.blockers.join('\n'),
      contains('engine timing does not replay'),
    );
  });

  test(
    'budgets are enforced per workload rather than diluted globally',
    () async {
      final sealed = await sealDogfoodPerformanceReceipt(
        validRawDogfoodPerformanceReceiptForTest(),
        verifyArtifactFiles: false,
      );
      final tampered = _copy(sealed);
      final cell = _cells(
        tampered,
      ).firstWhere((value) => value['id'] == 'product-tour-typing');
      final run = ((cell['runs']! as List).first as Map)
          .cast<String, Object?>();
      final engines = (run['engineObservations']! as List).cast<Map>();
      final samples = (run['samples']! as List).cast<Map>();
      for (var index = 0; index < 4; index += 1) {
        engines[20 + index]['nativeFfiMicros'] = 5000;
        samples[index]['engineMicros'] = 5000;
      }
      final result = await verifyDogfoodPerformanceReceipt(
        tampered,
        verifyArtifactFiles: false,
      );
      expect(
        result.blockers,
        contains('cell[product-tour-typing] Rust engine p99 exceeded 4 ms'),
      );
      expect(
        result.blockers,
        isNot(contains('Rust engine aggregate p99 exceeded 4 ms')),
      );
    },
  );

  test('every accepted generation and intervening frame is evidence', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final missingPaint = _copy(sealed);
    final structural = _cells(
      missingPaint,
    ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
    final run = ((structural['runs']! as List).first as Map)
        .cast<String, Object?>();
    final sample = ((run['samples']! as List).first as Map)
        .cast<String, Object?>();
    final firstGeneration =
        (sample['acceptedSourceGenerations']! as List).first as int;
    (run['paintObservations']! as List).removeWhere(
      (value) => (value as Map)['sourceGeneration'] == firstGeneration,
    );
    final missingResult = await verifyDogfoodPerformanceReceipt(
      missingPaint,
      verifyArtifactFiles: false,
    );
    expect(
      missingResult.blockers.join('\n'),
      contains('paint observations do not exactly cover'),
    );

    final missedFrame = _copy(sealed);
    final missedStructural = _cells(
      missedFrame,
    ).firstWhere((value) => value['id'] == 'product-tour-structural-burst');
    final missedRun = ((missedStructural['runs']! as List).first as Map)
        .cast<String, Object?>();
    final missedSample = ((missedRun['samples']! as List).first as Map)
        .cast<String, Object?>();
    final start = missedSample['startFrameOrdinal']! as int;
    ((missedRun['frames']! as List)[start] as Map)['missed'] = true;
    final missedResult = await verifyDogfoodPerformanceReceipt(
      missedFrame,
      verifyArtifactFiles: false,
    );
    expect(missedResult.blockers.join('\n'), contains('frame $start missed'));
  });

  test('lifecycle replay keys restarted generations by session', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final lifecycle = _cells(
      sealed,
    ).firstWhere((cell) => cell['id'] == 'lifecycle-same-process');
    final run = (lifecycle['runs']! as List).single as Map;
    final samples = (run['samples']! as List).cast<Map>();
    expect(samples[0]['acceptedSourceGenerations'], [1, 2]);
    expect(samples[1]['acceptedSourceGenerations'], [1, 2]);
    expect(samples[0]['sessionOrdinal'], 0);
    expect(samples[1]['sessionOrdinal'], 1);

    final conflated = _copy(sealed);
    final conflatedLifecycle = _cells(
      conflated,
    ).firstWhere((cell) => cell['id'] == 'lifecycle-same-process');
    final conflatedRun = (conflatedLifecycle['runs']! as List).single as Map;
    for (final key in const [
      'samples',
      'inputObservations',
      'paintObservations',
      'engineObservations',
    ]) {
      for (final value in (conflatedRun[key]! as List).cast<Map>()) {
        if (value['sessionOrdinal'] == 1) value['sessionOrdinal'] = 0;
      }
    }
    final result = await verifyDogfoodPerformanceReceipt(
      conflated,
      verifyArtifactFiles: false,
    );
    expect(
      result.blockers.join('\n'),
      contains('declared source generations must be unique'),
    );
  });

  test('open replay rejects hidden work and a torn first paint', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      validRawDogfoodPerformanceReceiptForTest(),
      verifyArtifactFiles: false,
    );
    final hiddenWork = _copy(sealed);
    final run =
        (_cells(hiddenWork).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final open = run['openObservation']! as Map;
    open['openToEditableMicros'] = 1;
    final hiddenResult = await verifyDogfoodPerformanceReceipt(
      hiddenWork,
      verifyArtifactFiles: false,
    );
    expect(
      hiddenResult.blockers.join('\n'),
      contains('timing does not replay'),
    );

    final torn = _copy(sealed);
    final tornRun =
        (_cells(torn).firstWhere(
                      (cell) => cell['id'] == 'ordinary-5m-journey',
                    )['runs']!
                    as List)
                .first
            as Map;
    final tornOpen = tornRun['openObservation']! as Map;
    tornOpen['expectedVisibleSourceSha256'] = _hash('d');
    final tornResult = await verifyDogfoodPerformanceReceipt(
      torn,
      verifyArtifactFiles: false,
    );
    expect(
      tornResult.blockers.join('\n'),
      contains('is not an exact certified initial paint'),
    );
  });

  test('fragment assembly is complete, ordered, and display-bound', () {
    final raw = validRawDogfoodPerformanceReceiptForTest();
    final display = (raw['display']! as Map).cast<String, Object?>();
    final fragments = <Map<String, Object?>>[];
    final binding = {
      'candidateCommit': _hash40('a'),
      'candidateTree': _hash40('b'),
      'bundleManifestSha256': _hash('c'),
      'mainExecutable': _identity('app'),
      'embeddedAbi': _identity('abi'),
    };
    for (final cell in _cells(raw).reversed) {
      for (final run in ((cell['runs']! as List).reversed)) {
        final fixture = dogfoodFixtureIdentity(cell['id']! as String);
        fragments.add({
          'id': cell['id'],
          'sourceBytes': fixture['sourceBytes'],
          'warmupsPerRun': cell['warmupsPerRun'],
          'samplesPerRun': cell['samplesPerRun'],
          'runCount': cell['runCount'],
          'cadenceHz': cell['cadenceHz'],
          'binding': binding,
          'fixture': fixture,
          'display': display,
          'run': run,
        });
      }
    }
    final assembly = assembleDogfoodProfileFragments(fragments);
    expect(assembly.cells.map((cell) => cell['id']), requiredDogfoodCells.keys);
    for (final cell in assembly.cells) {
      final runs = (cell['runs']! as List).cast<Map>();
      expect(
        runs.map((run) => run['run']),
        List<int>.generate(runs.length, (index) => index),
      );
    }

    final mismatched = _copy(fragments.first);
    mismatched['display'] = {...display, 'refreshHz': 120};
    expect(
      () => assembleDogfoodProfileFragments([mismatched, ...fragments.skip(1)]),
      throwsStateError,
    );

    final wrongFixture = _copy(fragments.first);
    (wrongFixture['fixture']! as Map)['sourceBytes'] = 1;
    expect(
      () =>
          assembleDogfoodProfileFragments([wrongFixture, ...fragments.skip(1)]),
      throwsStateError,
    );

    final wrongCandidate = _copy(fragments.first);
    (wrongCandidate['binding']! as Map)['candidateCommit'] = _hash40('d');
    expect(
      () => assembleDogfoodProfileFragments([
        wrongCandidate,
        ...fragments.skip(1),
      ]),
      throwsStateError,
    );
  });
}

Map<String, Object?> validRawDogfoodPerformanceReceiptForTest() {
  final cells = <Map<String, Object?>>[];
  for (final entry in requiredDogfoodCells.entries) {
    final denominator = entry.value;
    final fixture = dogfoodFixtureIdentity(entry.key);
    final sourceBytes = fixture['sourceBytes']! as int;
    final runs = <Map<String, Object?>>[];
    for (var run = 0; run < denominator.runs; run += 1) {
      final warmups = <Map<String, Object?>>[];
      final samples = <Map<String, Object?>>[];
      final frames = <Map<String, Object?>>[];
      final inputObservations = <Map<String, Object?>>[];
      final paintObservations = <Map<String, Object?>>[];
      final engineObservations = <Map<String, Object?>>[];
      var frameOrdinal = 0;
      var sourceGeneration = denominator.requiresInput ? 1 : 0;
      final structural = entry.key.endsWith('structural-burst');
      final lifecycle = entry.key.startsWith('lifecycle-');
      for (var warmup = 0; warmup < denominator.warmups; warmup += 1) {
        final accepted = 500000 + warmup * 20000;
        final finalGeneration = sourceGeneration + (structural ? 1 : 0);
        final finalFrame = frameOrdinal + (structural ? 1 : 0);
        warmups.add(
          _warmupFromSample(
            _sample(
              index: warmup,
              accepted: accepted,
              acceptedSourceGenerations: [
                sourceGeneration,
                if (structural) finalGeneration,
              ],
              sourceGeneration: finalGeneration,
              frameOrdinal: finalFrame,
            ),
          ),
        );
        if (structural) {
          _addRawObservations(
            inputObservations: inputObservations,
            paintObservations: paintObservations,
            engineObservations: engineObservations,
            accepted: accepted,
            paintTimestamp: accepted + 50,
            sourceGeneration: sourceGeneration,
            frameOrdinal: frameOrdinal,
            semanticsCurrent: false,
            activeNeutralRowCount: 1,
          );
          frames.add(_frame(frameOrdinal, accepted, paintDelayMicros: 50));
          frameOrdinal += 1;
          sourceGeneration += 1;
        }
        _addRawObservations(
          inputObservations: inputObservations,
          paintObservations: paintObservations,
          engineObservations: engineObservations,
          accepted: accepted + (structural ? 100 : 0),
          paintTimestamp: accepted + 500,
          sourceGeneration: sourceGeneration,
          frameOrdinal: frameOrdinal,
        );
        frames.add(_frame(frameOrdinal, accepted));
        frameOrdinal += 1;
        sourceGeneration += 1;
      }
      for (var sample = 0; sample < denominator.samples; sample += 1) {
        final sessionOrdinal = lifecycle && denominator.samples > 1
            ? sample
            : 0;
        if (lifecycle) sourceGeneration = 1;
        final scheduled = denominator.cadenceHz == 0
            ? null
            : (sample * 1000000 / denominator.cadenceHz).round();
        final accepted = 1000000 + (scheduled ?? sample * 20000);
        final multiGeneration = structural || lifecycle;
        final finalGeneration = sourceGeneration + (multiGeneration ? 1 : 0);
        final finalFrame = frameOrdinal + (multiGeneration ? 1 : 0);
        samples.add(
          _sample(
            index: sample,
            sessionOrdinal: sessionOrdinal,
            scheduled: scheduled,
            accepted: accepted,
            acceptedSourceGenerations: [
              sourceGeneration,
              if (multiGeneration) finalGeneration,
            ],
            sourceGeneration: finalGeneration,
            frameOrdinal: finalFrame,
            requiresLiveStateZero: denominator.requiresLiveStateZero,
          ),
        );
        if (multiGeneration) {
          _addRawObservations(
            inputObservations: inputObservations,
            paintObservations: paintObservations,
            engineObservations: engineObservations,
            accepted: accepted,
            paintTimestamp: accepted + 50,
            sourceGeneration: sourceGeneration,
            sessionOrdinal: sessionOrdinal,
            frameOrdinal: frameOrdinal,
            semanticsCurrent: false,
            activeNeutralRowCount: 1,
          );
          frames.add(_frame(frameOrdinal, accepted, paintDelayMicros: 50));
          frameOrdinal += 1;
          sourceGeneration += 1;
        }
        _addRawObservations(
          inputObservations: inputObservations,
          paintObservations: paintObservations,
          engineObservations: engineObservations,
          accepted: accepted + (structural ? 100 : 0),
          paintTimestamp: accepted + 500,
          sourceGeneration: sourceGeneration,
          sessionOrdinal: sessionOrdinal,
          frameOrdinal: frameOrdinal,
        );
        frames.add(_frame(frameOrdinal, accepted));
        frameOrdinal += 1;
        sourceGeneration += 1;
      }
      runs.add({
        'run': run,
        'processId':
            denominator.processRule == DogfoodProcessRule.oneSharedProcess
            ? 'shared'
            : '${entry.key}-$run',
        'freshProcess':
            denominator.processRule == DogfoodProcessRule.freshEveryRun,
        'openObservation': denominator.requiresOpen
            ? {
                'kind': entry.key == 'product-tour-cold-launch'
                    ? 'processLaunch'
                    : 'presetSelection',
                'acceptedMicros': 1000000,
                'paintMicros': 1000500,
                'openToEditableMicros': 500,
                'sourceGeneration': 0,
                'sourceSha256': _hash('a'),
                'frameOrdinal': 0,
                'visibleSourceSha256': _hash('b'),
                'expectedVisibleSourceSha256': _hash('b'),
                'canonicalSelectionBaseUtf16': 0,
                'canonicalSelectionExtentUtf16': 0,
                'expectedSelectionBaseUtf16': 0,
                'expectedSelectionExtentUtf16': 0,
                'caretSourceUtf16': 0,
                'caretDisplayUtf16': 0,
                'semanticsCurrent': true,
                'activeNeutralRowCount': 0,
              }
            : null,
        'warmups': warmups,
        'samples': samples,
        'frames': frames,
        'inputObservations': inputObservations,
        'paintObservations': paintObservations,
        'engineObservations': engineObservations,
        'memory': const [
          {'stage': 'baseline', 'timestampMicros': 1, 'rssBytes': 100000000},
          {'stage': 'peak', 'timestampMicros': 2, 'rssBytes': 110000000},
          {'stage': 'close', 'timestampMicros': 3, 'rssBytes': 105000000},
          {'stage': 'postClose', 'timestampMicros': 4, 'rssBytes': 102000000},
        ],
      });
    }
    cells.add({
      'id': entry.key,
      'sourceBytes': sourceBytes,
      'warmupsPerRun': denominator.warmups,
      'samplesPerRun': denominator.samples,
      'runCount': denominator.runs,
      'cadenceHz': denominator.cadenceHz,
      'fixture': fixture,
      'runs': runs,
    });
  }
  return {
    'schema': 'dogfood_performance_v1',
    'schemaVersion': 1,
    'candidate': {
      'commit': List.filled(40, 'a').join(),
      'tree': List.filled(40, 'b').join(),
      'clean': true,
    },
    'configuration': {
      'ledger': _identity('ledger'),
      'streamedOpeningEnabled': false,
      'enabledPresetIds': const [
        'productTour',
        'prose1MiB',
        'prose5MiB',
        'prose10MiB',
        'giantLine5MiB',
        'denseBlocks1MiB',
      ],
    },
    'artifacts': {
      'appBundleManifest': _identity('manifest'),
      'mainExecutable': _identity('app'),
      'embeddedAbi': _identity('abi'),
      'profileHarness': _identity('harness'),
    },
    'host': {
      'hostname': 'benchmark-mac',
      'operatingSystem': 'macOS',
      'architecture': 'arm64',
      'cpu': 'Apple',
      'logicalCores': 8,
      'physicalMemoryBytes': 16000000000,
      'flutterVersion': 'test',
      'dartVersion': 'test',
      'rustcVersion': 'test',
      'cargoVersion': 'test',
      'xcodeVersion': 'test',
    },
    'display': {
      'refreshHz': 60,
      'framePeriodMicros': 1000000 / 60,
      'widthLogical': 1569,
      'heightLogical': 906,
      'devicePixelRatio': 2,
    },
    'cells': cells,
  };
}

Map<String, Object?> _sample({
  required int index,
  int sessionOrdinal = 0,
  required int accepted,
  required List<int> acceptedSourceGenerations,
  required int sourceGeneration,
  required int frameOrdinal,
  int? scheduled,
  bool requiresLiveStateZero = false,
}) => {
  'index': index,
  'sessionOrdinal': sessionOrdinal,
  'scheduledMicros': scheduled,
  'acceptedMicros': accepted,
  'sourcePaintMicros': accepted + 500,
  'caretPaintMicros': accepted + 500,
  'selectionPaintMicros': accepted + 500,
  'acceptedSourceGenerations': acceptedSourceGenerations,
  'sourceGeneration': sourceGeneration,
  'paintedSourceGeneration': sourceGeneration,
  'sourceSha256': _hash('a'),
  'visibleSourceSha256': _hash('b'),
  'canonicalSelectionBaseUtf16': sourceGeneration,
  'canonicalSelectionExtentUtf16': sourceGeneration,
  'paintedCaretSourceUtf16': sourceGeneration,
  'startFrameOrdinal': frameOrdinal,
  'endFrameOrdinal': frameOrdinal,
  'provingFrameOrdinal': frameOrdinal,
  'engineMicros': 100,
  'visibleCertificationMicros': 500,
  'rawProjectionFrames': 0,
  'sourceIdentityMatched': true,
  'caretIdentityMatched': true,
  'selectionIdentityMatched': true,
  'faulted': false,
  'resyncCount': 0,
  if (requiresLiveStateZero)
    'globalLiveState': {
      'sessions': 0,
      'transactions': 0,
      'continuations': 0,
      'anchors': 0,
      'historyTokens': 0,
    },
};

Map<String, Object?> _warmupFromSample(Map<String, Object?> sample) => {
  for (final name in const [
    'index',
    'sessionOrdinal',
    'acceptedMicros',
    'acceptedSourceGenerations',
    'sourceGeneration',
    'sourceSha256',
    'canonicalSelectionBaseUtf16',
    'canonicalSelectionExtentUtf16',
    'engineMicros',
  ])
    name: sample[name],
};

Map<String, Object?> _frame(
  int ordinal,
  int accepted, {
  int paintDelayMicros = 500,
}) => {
  'ordinal': ordinal,
  'vsyncMicros': accepted + paintDelayMicros,
  'buildMicros': 1000,
  'rasterMicros': 1000,
  'editorSyncMicros': 100,
  'editorAttributed': true,
  'missed': false,
};

void _addRawObservations({
  required List<Map<String, Object?>> inputObservations,
  required List<Map<String, Object?>> paintObservations,
  required List<Map<String, Object?>> engineObservations,
  required int accepted,
  required int paintTimestamp,
  required int sourceGeneration,
  int sessionOrdinal = 0,
  required int frameOrdinal,
  bool semanticsCurrent = true,
  int activeNeutralRowCount = 0,
}) {
  inputObservations.add({
    'sessionOrdinal': sessionOrdinal,
    'sourceGeneration': sourceGeneration,
    'acceptedMicros': accepted,
    'editorSyncMicros': 100,
    'sourceSha256': _hash('a'),
    'canonicalSelectionBaseUtf16': sourceGeneration,
    'canonicalSelectionExtentUtf16': sourceGeneration,
  });
  paintObservations.add({
    'sessionOrdinal': sessionOrdinal,
    'timestampMicros': paintTimestamp,
    'frameOrdinal': frameOrdinal,
    'sourceGeneration': sourceGeneration,
    'visibleSourceSha256': _hash('b'),
    'expectedVisibleSourceSha256': _hash('b'),
    'canonicalSelectionBaseUtf16': sourceGeneration,
    'canonicalSelectionExtentUtf16': sourceGeneration,
    'expectedSelectionBaseUtf16': sourceGeneration,
    'expectedSelectionExtentUtf16': sourceGeneration,
    'caretSourceUtf16': sourceGeneration,
    'caretDisplayUtf16': 1,
    'semanticsCurrent': semanticsCurrent,
    'activeNeutralRowCount': activeNeutralRowCount,
    'activeRowVisible': true,
  });
  engineObservations.add({
    'sessionOrdinal': sessionOrdinal,
    'sourceGeneration': sourceGeneration,
    'nativeFfiMicros': 100,
  });
}

Map<String, Object> _identity(String path) => {
  'path': path,
  'bytes': 1,
  'sha256': _hash('c'),
};

String _hash(String character) => List.filled(64, character).join();

String _hash40(String character) => List.filled(40, character).join();

Map<String, Object?> _copy(Map<String, Object?> value) =>
    jsonDecode(jsonEncode(value)) as Map<String, Object?>;

List<Map<String, Object?>> _cells(Map<String, Object?> receipt) =>
    (receipt['cells']! as List).cast<Map<String, Object?>>();

Map<String, Object?> _firstSample(Map<String, Object?> receipt) {
  final cell = _cells(
    receipt,
  ).firstWhere((candidate) => candidate['id'] == 'product-tour-typing');
  final run = ((cell['runs']! as List).first as Map).cast<String, Object?>();
  return (run['samples']! as List).first as Map<String, Object?>;
}

Map<String, Object?> _firstRun(Map<String, Object?> receipt) =>
    ((_cells(receipt).first['runs']! as List).first as Map)
        .cast<String, Object?>();
