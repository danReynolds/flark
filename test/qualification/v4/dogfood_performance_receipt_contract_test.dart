// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import '../../../scripts/verify_v4_dogfood_receipt.dart';

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
      _validRawReceipt(),
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
    expect((result.metrics['flutterFrameMicros']! as Map)['sampleCount'], 3760);
    expect((result.metrics['engineMicros']! as Map)['sampleCount'], 3760);
    expect((result.metrics['openToEditableMicros']! as Map)['sampleCount'], 25);
  });

  test('replay fails closed on timing and denominator tampering', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      _validRawReceipt(),
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
      _validRawReceipt(),
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
    final coldSample = (coldRun['samples']! as List).first as Map;
    coldSample['openToEditableMicros'] = null;
    final openResult = await verifyDogfoodPerformanceReceipt(
      missingOpen,
      verifyArtifactFiles: false,
    );
    expect(
      openResult.blockers.join('\n'),
      contains('requires an open-to-editable measurement'),
    );
  });

  test('replay rejects raw paint, input, and engine disagreement', () async {
    final sealed = await sealDogfoodPerformanceReceipt(
      _validRawReceipt(),
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
}

Map<String, Object?> _validRawReceipt() {
  final cells = <Map<String, Object?>>[];
  for (final entry in requiredDogfoodCells.entries) {
    final denominator = entry.value;
    final sourceBytes = switch (entry.key) {
      String id when id.contains('10m') => 10 * 1024 * 1024,
      String id when id.contains('5m') => 5 * 1024 * 1024,
      String id when id.contains('1m') => 1024 * 1024,
      _ => 4096,
    };
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
      for (var warmup = 0; warmup < denominator.warmups; warmup += 1) {
        final accepted = 500000 + warmup * 20000;
        final finalGeneration = sourceGeneration + (structural ? 1 : 0);
        final finalFrame = frameOrdinal + (structural ? 1 : 0);
        warmups.add(
          _sample(
            index: warmup,
            accepted: accepted,
            acceptedSourceGenerations: [
              sourceGeneration,
              if (structural) finalGeneration,
            ],
            sourceGeneration: finalGeneration,
            frameOrdinal: finalFrame,
            requiresOpen: false,
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
        final scheduled = denominator.cadenceHz == 0
            ? null
            : (sample * 1000000 / denominator.cadenceHz).round();
        final accepted = 1000000 + (scheduled ?? sample * 20000);
        final finalGeneration = sourceGeneration + (structural ? 1 : 0);
        final finalFrame = frameOrdinal + (structural ? 1 : 0);
        samples.add(
          _sample(
            index: sample,
            scheduled: scheduled,
            accepted: accepted,
            acceptedSourceGenerations: [
              sourceGeneration,
              if (structural) finalGeneration,
            ],
            sourceGeneration: finalGeneration,
            frameOrdinal: finalFrame,
            requiresOpen: denominator.requiresOpen,
            requiresLiveStateZero: denominator.requiresLiveStateZero,
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
      runs.add({
        'run': run,
        'processId':
            denominator.processRule == DogfoodProcessRule.oneSharedProcess
            ? 'shared'
            : '${entry.key}-$run',
        'freshProcess':
            denominator.processRule == DogfoodProcessRule.freshEveryRun,
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
  required int accepted,
  required List<int> acceptedSourceGenerations,
  required int sourceGeneration,
  required int frameOrdinal,
  required bool requiresOpen,
  int? scheduled,
  bool requiresLiveStateZero = false,
}) => {
  'index': index,
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
  'openToEditableMicros': requiresOpen ? 10000 : null,
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
  required int frameOrdinal,
  bool semanticsCurrent = true,
  int activeNeutralRowCount = 0,
}) {
  inputObservations.add({
    'sourceGeneration': sourceGeneration,
    'acceptedMicros': accepted,
    'editorSyncMicros': 100,
    'sourceSha256': _hash('a'),
    'canonicalSelectionBaseUtf16': sourceGeneration,
    'canonicalSelectionExtentUtf16': sourceGeneration,
  });
  paintObservations.add({
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
  });
  engineObservations.add({
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
