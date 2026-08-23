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
      3045,
    );
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
      final samples = <Map<String, Object?>>[];
      final frames = <Map<String, Object?>>[];
      for (var sample = 0; sample < denominator.samples; sample += 1) {
        final scheduled = denominator.cadenceHz == 0
            ? null
            : (sample * 1000000 / denominator.cadenceHz).round();
        final accepted = 1000000 + (scheduled ?? sample * 20000);
        frames.add({
          'ordinal': sample,
          'vsyncMicros': accepted + 500,
          'buildMicros': 1000,
          'rasterMicros': 1000,
          'editorSyncMicros': 100,
          'editorAttributed': true,
          'missed': false,
        });
        samples.add({
          'index': sample,
          'scheduledMicros': scheduled,
          'acceptedMicros': accepted,
          'sourcePaintMicros': accepted + 500,
          'caretPaintMicros': accepted + 500,
          'selectionPaintMicros': accepted + 500,
          'sourceGeneration': sample + 1,
          'paintedSourceGeneration': sample + 1,
          'sourceSha256': _hash('a'),
          'visibleSourceSha256': _hash('b'),
          'canonicalSelectionBaseUtf16': sample + 1,
          'canonicalSelectionExtentUtf16': sample + 1,
          'paintedCaretSourceUtf16': sample + 1,
          'startFrameOrdinal': sample,
          'endFrameOrdinal': sample,
          'provingFrameOrdinal': sample,
          'engineMicros': 100,
          'visibleCertificationMicros': 5000,
          'openToEditableMicros': denominator.requiresOpen ? 10000 : null,
          'rawProjectionFrames': 0,
          'sourceIdentityMatched': true,
          'caretIdentityMatched': true,
          'selectionIdentityMatched': true,
          'faulted': false,
          'resyncCount': 0,
          if (denominator.requiresLiveStateZero)
            'globalLiveState': {
              'sessions': 0,
              'transactions': 0,
              'continuations': 0,
              'anchors': 0,
              'historyTokens': 0,
            },
        });
      }
      runs.add({
        'run': run,
        'processId':
            denominator.processRule == DogfoodProcessRule.oneSharedProcess
            ? 'shared'
            : '${entry.key}-$run',
        'freshProcess':
            denominator.processRule == DogfoodProcessRule.freshEveryRun,
        'warmups': List.generate(
          denominator.warmups,
          (index) => samples.isEmpty
              ? _warmup(index)
              : {...samples.first, 'index': index},
        ),
        'samples': samples,
        'frames': frames,
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

Map<String, Object?> _warmup(int index) => {
  'index': index,
  'acceptedMicros': 0,
  'sourcePaintMicros': 0,
  'caretPaintMicros': 0,
  'selectionPaintMicros': 0,
  'sourceGeneration': 0,
  'paintedSourceGeneration': 0,
  'sourceSha256': _hash('a'),
  'visibleSourceSha256': _hash('b'),
  'canonicalSelectionBaseUtf16': 0,
  'canonicalSelectionExtentUtf16': 0,
  'paintedCaretSourceUtf16': 0,
  'startFrameOrdinal': 0,
  'endFrameOrdinal': 0,
  'provingFrameOrdinal': 0,
  'engineMicros': 0,
  'visibleCertificationMicros': 0,
  'rawProjectionFrames': 0,
  'sourceIdentityMatched': true,
  'caretIdentityMatched': true,
  'selectionIdentityMatched': true,
  'faulted': false,
  'resyncCount': 0,
};

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
  final run = (_cells(receipt).first['runs']! as List).first as Map;
  return (run['samples']! as List).first as Map<String, Object?>;
}
