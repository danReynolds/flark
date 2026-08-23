// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:crypto/crypto.dart';

import '../packages/flark/example/lib/dogfood_documents.dart';
import '../packages/flark/test/support/macos_native_canary_driver.dart';
import 'verify_v4_dogfood_receipt.dart';

const _profileCadence = Duration(microseconds: 16667);
const _windowWidth = 1569;
const _windowHeight = 906;

final class _ExpectedGeneration {
  const _ExpectedGeneration({
    required this.generation,
    required this.source,
    required this.selectionBase,
    required this.selectionExtent,
  });

  final int generation;
  final String source;
  final int selectionBase;
  final int selectionExtent;
}

final class _ExpectedSample {
  const _ExpectedSample({
    required this.index,
    required this.generations,
    required this.scheduledMicros,
  });

  final int index;
  final List<_ExpectedGeneration> generations;
  final int? scheduledMicros;

  _ExpectedGeneration get finalGeneration => generations.last;
}

final class _ProfileRunResult {
  const _ProfileRunResult({
    required this.initialSourceBytes,
    required this.run,
  });

  final int initialSourceBytes;
  final Map<String, Object?> run;
}

Future<void> main(List<String> arguments) async {
  if (arguments.length != 5) {
    stderr.writeln(
      'usage: dart run scripts/dogfood_profile_run.dart '
      '<cell-id> <run-index> <app-executable> <embedded-abi> <output.json>',
    );
    exitCode = 64;
    return;
  }
  final cellId = arguments[0];
  final runIndex = int.tryParse(arguments[1]);
  if (runIndex == null || runIndex < 0) {
    stderr.writeln('dogfood-profile-run: run index must be nonnegative');
    exitCode = 64;
    return;
  }
  final denominator = requiredDogfoodCells[cellId];
  if (denominator == null) {
    stderr.writeln('dogfood-profile-run: unknown or disabled cell $cellId');
    exitCode = 64;
    return;
  }
  if (runIndex >= denominator.runs) {
    stderr.writeln(
      'dogfood-profile-run: run $runIndex is outside ${denominator.runs}',
    );
    exitCode = 64;
    return;
  }
  try {
    final result = await _runCell(
      cellId: cellId,
      runIndex: runIndex,
      denominator: denominator,
      appExecutable: File(arguments[2]).absolute,
      embeddedAbi: File(arguments[3]).absolute,
    );
    final output = File(arguments[4]);
    await output.parent.create(recursive: true);
    await output.writeAsString(
      '${jsonEncode({'id': cellId, 'sourceBytes': result.initialSourceBytes, 'warmupsPerRun': denominator.warmups, 'samplesPerRun': denominator.samples, 'runCount': denominator.runs, 'cadenceHz': denominator.cadenceHz, 'run': result.run})}\n',
      flush: true,
    );
    stdout.writeln(
      'dogfood-profile-run: PASS cell=$cellId run=$runIndex '
      'process=${result.run['processId']}',
    );
  } on Object catch (error, stackTrace) {
    stderr.writeln('dogfood-profile-run: FAIL $error');
    stderr.writeln(stackTrace);
    exitCode = 1;
  }
}

Future<_ProfileRunResult> _runCell({
  required String cellId,
  required int runIndex,
  required DogfoodCellDenominator denominator,
  required File appExecutable,
  required File embeddedAbi,
}) async {
  if (!appExecutable.existsSync() || !embeddedAbi.existsSync()) {
    throw StateError('profile app executable or embedded ABI is missing');
  }
  final preset = _presetFor(cellId);
  final initialSource = buildDogfoodDocument(preset);
  final driver = MacosNativeCanaryDriver(
    appExecutable: appExecutable.path,
    libraryPath: embeddedAbi.path,
    actuatorScript: File(
      'packages/flark/tool/live_editor_macos_canary.swift',
    ).absolute.path,
    initialPresetName: preset.name,
  );
  try {
    final launched = await driver.start();
    if (launched.source != initialSource) {
      throw StateError('$cellId opened a source different from its preset');
    }
    if (cellId == 'product-tour-cold-launch') {
      final run = await _coldLaunchRun(
        driver: driver,
        launched: launched,
        source: initialSource,
        runIndex: runIndex,
      );
      return _ProfileRunResult(
        initialSourceBytes: utf8.encode(initialSource).length,
        run: run,
      );
    }

    final total = denominator.warmups + denominator.samples;
    late MacosNativeCanarySnapshot baseline;
    late List<_ExpectedSample> expected;
    switch (cellId) {
      case 'product-tour-typing' || 'ordinary-1m-typing':
        final offset = cellId.startsWith('product-tour')
            ? initialSource.indexOf('This')
            : initialSource.indexOf('This is ordinary') + 'This '.length;
        baseline = await driver.activateAtUtf16(
          offset,
          windowWidth: _windowWidth,
          windowHeight: _windowHeight,
        );
        expected = _insertionExpectations(
          source: baseline.source,
          offset: offset,
          firstGeneration: baseline.sourceGeneration + 1,
          count: total,
        );
        await driver.typeText(
          _alternatingText(total),
          cadence: _profileCadence,
        );
      case 'product-tour-inline-typing' || 'ordinary-1m-inline-typing':
        final anchor = cellId.startsWith('product-tour') ? 'Rust' : 'Flark';
        final offset = initialSource.indexOf(anchor) + 2;
        baseline = await driver.activateAtUtf16(
          offset,
          windowWidth: _windowWidth,
          windowHeight: _windowHeight,
        );
        expected = _insertionExpectations(
          source: baseline.source,
          offset: offset,
          firstGeneration: baseline.sourceGeneration + 1,
          count: total,
        );
        await driver.typeText(
          _alternatingText(total),
          cadence: _profileCadence,
        );
      case 'product-tour-deletion' || 'ordinary-1m-deletion':
        final offset = cellId.startsWith('product-tour')
            ? initialSource.indexOf('This intentionally') + 'This '.length
            : initialSource.indexOf('This is ordinary') + 'This '.length;
        await driver.activateAtUtf16(
          offset,
          windowWidth: _windowWidth,
          windowHeight: _windowHeight,
        );
        final seed = List.filled(total, 'z').join();
        await driver.typeText(seed);
        final prepared = await driver.settle();
        final caret = offset + seed.length;
        baseline = await driver.activateAtUtf16(
          caret,
          windowWidth: _windowWidth,
          windowHeight: _windowHeight,
        );
        expected = _backspaceExpectations(
          source: prepared.source,
          caret: caret,
          firstGeneration: baseline.sourceGeneration + 1,
          count: total,
        );
        await driver.repeatKey(
          'backspace',
          count: total,
          cadence: _profileCadence,
        );
      case 'product-tour-structural-burst' || 'ordinary-1m-structural-burst':
        final marker = cellId.startsWith('product-tour')
            ? 'locally.'
            : 'parser catches up.';
        final offset = initialSource.indexOf(marker) + marker.length;
        baseline = await driver.activateAtUtf16(
          offset,
          windowWidth: _windowWidth,
          windowHeight: _windowHeight,
        );
        expected = _structuralExpectations(
          source: baseline.source,
          caret: offset,
          firstGeneration: baseline.sourceGeneration + 1,
          count: total,
        );
        await driver.typeStructuralBursts(
          count: total,
          cadence: _profileCadence,
        );
      case 'ordinary-1m-paste-32kib':
        final offset =
            initialSource.indexOf('This is ordinary') +
            'This is ordinary'.length;
        baseline = await driver.activateAtUtf16(
          offset,
          windowWidth: _windowWidth,
          windowHeight: _windowHeight,
        );
        final paste = List.filled(32 * 1024, 'p').join();
        expected = _pasteUndoExpectations(
          source: baseline.source,
          caret: offset,
          paste: paste,
          firstGeneration: baseline.sourceGeneration + 1,
          count: total,
        );
        for (var operation = 0; operation < total; operation += 1) {
          await driver.pasteText(paste);
          await driver.settle();
          await driver.pressKey('undo');
          await driver.settle();
        }
      default:
        throw UnsupportedError(
          '$cellId needs its history/scale journey telemetry before D0',
        );
    }
    final settled = await driver.settle();
    if (settled.source != expected.last.finalGeneration.source ||
        settled.selectionBaseUtf16 !=
            expected.last.finalGeneration.selectionBase ||
        settled.selectionExtentUtf16 !=
            expected.last.finalGeneration.selectionExtent) {
      throw StateError('$cellId did not reach its deterministic final state');
    }
    final closed = await driver.closeSession();
    return _ProfileRunResult(
      initialSourceBytes: utf8.encode(initialSource).length,
      run: _buildMeasuredRun(
        runIndex: runIndex,
        denominator: denominator,
        baseline: baseline,
        settled: settled,
        closed: closed,
        expected: expected,
      ),
    );
  } finally {
    await driver.close();
  }
}

DogfoodDocumentPreset _presetFor(String cellId) {
  if (cellId.startsWith('product-tour')) {
    return DogfoodDocumentPreset.productTour;
  }
  if (cellId.startsWith('ordinary-1m')) {
    return DogfoodDocumentPreset.prose1MiB;
  }
  throw UnsupportedError('$cellId has no implemented app preset');
}

List<_ExpectedSample> _insertionExpectations({
  required String source,
  required int offset,
  required int firstGeneration,
  required int count,
}) {
  final result = <_ExpectedSample>[];
  var current = source;
  var caret = offset;
  for (var index = 0; index < count; index += 1) {
    final inserted = index.isEven ? 'x' : 'y';
    current = current.replaceRange(caret, caret, inserted);
    caret += inserted.length;
    result.add(
      _ExpectedSample(
        index: index,
        generations: [
          _ExpectedGeneration(
            generation: firstGeneration + index,
            source: current,
            selectionBase: caret,
            selectionExtent: caret,
          ),
        ],
        scheduledMicros: index * _profileCadence.inMicroseconds,
      ),
    );
  }
  return result;
}

List<_ExpectedSample> _backspaceExpectations({
  required String source,
  required int caret,
  required int firstGeneration,
  required int count,
}) {
  final result = <_ExpectedSample>[];
  var current = source;
  var currentCaret = caret;
  for (var index = 0; index < count; index += 1) {
    current = current.replaceRange(currentCaret - 1, currentCaret, '');
    currentCaret -= 1;
    result.add(
      _ExpectedSample(
        index: index,
        generations: [
          _ExpectedGeneration(
            generation: firstGeneration + index,
            source: current,
            selectionBase: currentCaret,
            selectionExtent: currentCaret,
          ),
        ],
        scheduledMicros: index * _profileCadence.inMicroseconds,
      ),
    );
  }
  return result;
}

List<_ExpectedSample> _structuralExpectations({
  required String source,
  required int caret,
  required int firstGeneration,
  required int count,
}) {
  final result = <_ExpectedSample>[];
  var current = source;
  var currentCaret = caret;
  var generation = firstGeneration;
  for (var index = 0; index < count; index += 1) {
    current = current.replaceRange(currentCaret, currentCaret, '\n');
    currentCaret += 1;
    final structural = _ExpectedGeneration(
      generation: generation++,
      source: current,
      selectionBase: currentCaret,
      selectionExtent: currentCaret,
    );
    current = current.replaceRange(currentCaret, currentCaret, 'x');
    currentCaret += 1;
    final successor = _ExpectedGeneration(
      generation: generation++,
      source: current,
      selectionBase: currentCaret,
      selectionExtent: currentCaret,
    );
    result.add(
      _ExpectedSample(
        index: index,
        generations: [structural, successor],
        scheduledMicros: index * _profileCadence.inMicroseconds,
      ),
    );
  }
  return result;
}

List<_ExpectedSample> _pasteUndoExpectations({
  required String source,
  required int caret,
  required String paste,
  required int firstGeneration,
  required int count,
}) => [
  for (var index = 0; index < count; index += 1)
    _ExpectedSample(
      index: index,
      generations: [
        _ExpectedGeneration(
          generation: firstGeneration + index * 2,
          source: source.replaceRange(caret, caret, paste),
          selectionBase: caret + paste.length,
          selectionExtent: caret + paste.length,
        ),
        _ExpectedGeneration(
          generation: firstGeneration + index * 2 + 1,
          source: source,
          selectionBase: caret,
          selectionExtent: caret,
        ),
      ],
      scheduledMicros: null,
    ),
];

String _alternatingText(int count) =>
    List.generate(count, (index) => index.isEven ? 'x' : 'y').join();

Future<Map<String, Object?>> _coldLaunchRun({
  required MacosNativeCanaryDriver driver,
  required MacosNativeCanarySnapshot launched,
  required String source,
  required int runIndex,
}) async {
  final expected = _ExpectedSample(
    index: 0,
    generations: [
      _ExpectedGeneration(
        generation: launched.sourceGeneration,
        source: source,
        selectionBase: launched.selectionBaseUtf16,
        selectionExtent: launched.selectionExtentUtf16,
      ),
    ],
    scheduledMicros: null,
  );
  final launchEpoch =
      launched.processLaunchEpochMicros ??
      (throw StateError('cold launch is missing its process epoch'));
  final closed = await driver.closeSession();
  return _buildMeasuredRun(
    runIndex: runIndex,
    denominator: requiredDogfoodCells['product-tour-cold-launch']!,
    baseline: launched,
    settled: launched,
    closed: closed,
    expected: [expected],
    openAcceptedMicros: launchEpoch,
  );
}

Map<String, Object?> _buildMeasuredRun({
  required int runIndex,
  required DogfoodCellDenominator denominator,
  required MacosNativeCanarySnapshot baseline,
  required MacosNativeCanarySnapshot settled,
  required Map<String, Object?> closed,
  required List<_ExpectedSample> expected,
  int? openAcceptedMicros,
}) {
  final expectedByGeneration = <int, _ExpectedGeneration>{
    for (final sample in expected)
      for (final generation in sample.generations)
        generation.generation: generation,
  };
  final ordinaryInputs = _ordinaryInputEvents(settled.inputEvents);
  final semanticInputs = _semanticInputEvents(
    settled.inputEvents,
    settled.semanticEditPerformanceReceipts,
  );
  final historyInputs = _historyInputEvents(
    settled.inputEvents,
    settled.sourceEditPerformanceReceipts,
  );
  final performanceByGeneration = <int, Map<String, Object?>>{
    for (final receipt in settled.sourceEditPerformanceReceipts)
      receipt['sourceGeneration']! as int: receipt,
    for (final receipt in settled.semanticEditPerformanceReceipts)
      receipt['sourceGeneration']! as int: receipt,
  };
  final inputObservations = <Map<String, Object?>>[];
  final engineObservations = <Map<String, Object?>>[];
  for (final entry in expectedByGeneration.entries) {
    final generation = entry.key;
    final expectation = entry.value;
    if (generation == 0 && openAcceptedMicros != null) {
      inputObservations.add(
        _inputObservation(
          expectation,
          acceptedMicros: openAcceptedMicros,
          editorSyncMicros: 0,
        ),
      );
      engineObservations.add({'sourceGeneration': 0, 'nativeFfiMicros': 0});
      continue;
    }
    final input =
        ordinaryInputs[generation] ??
        semanticInputs[generation] ??
        historyInputs[generation];
    if (input == null) {
      throw StateError('generation $generation has no acceptance event');
    }
    inputObservations.add(
      _inputObservation(
        expectation,
        acceptedMicros: input.$1,
        editorSyncMicros: input.$2,
      ),
    );
    final performance = performanceByGeneration[generation];
    if (performance == null) {
      throw StateError('generation $generation has no engine receipt');
    }
    engineObservations.add({
      'sourceGeneration': generation,
      'nativeFfiMicros': performance['nativeFfiMicros']! as int,
    });
  }
  inputObservations.sort(
    (left, right) => (left['sourceGeneration']! as int).compareTo(
      right['sourceGeneration']! as int,
    ),
  );
  engineObservations.sort(
    (left, right) => (left['sourceGeneration']! as int).compareTo(
      right['sourceGeneration']! as int,
    ),
  );

  final timingByStamp = <int, Map<String, Object?>>{};
  for (final timing in settled.frameTimingReceipts) {
    timingByStamp[timing['vsyncStartMicros']! as int] = timing;
  }
  final orderedTimings = timingByStamp.values.toList()
    ..sort(
      (left, right) => (left['vsyncStartMicros']! as int).compareTo(
        right['vsyncStartMicros']! as int,
      ),
    );
  final frameOrdinalByStamp = <int, int>{};
  final frames = <Map<String, Object?>>[];
  for (var index = 0; index < orderedTimings.length; index += 1) {
    final timing = orderedTimings[index];
    final stamp = timing['vsyncStartMicros']! as int;
    frameOrdinalByStamp[stamp] = index;
    frames.add({
      'ordinal': index,
      'vsyncMicros': stamp,
      'buildMicros': timing['buildMicros']! as int,
      'rasterMicros': timing['rasterMicros']! as int,
      'editorSyncMicros': 0,
      'editorAttributed': false,
      'missed':
          (timing['totalSpanMicros']! as int) >=
          (1000000 / (settled.display['refreshHz']! as num)).round(),
    });
  }

  final paintObservations = <Map<String, Object?>>[];
  final framePeriodMicros = (1000000 / (settled.display['refreshHz']! as num))
      .round();
  for (final paint in settled.paintReceipts) {
    final generation = paint['sourceGeneration']! as int;
    final expectation = expectedByGeneration[generation];
    if (expectation == null) continue;
    final stamp = paint['frameStampMicros']! as int;
    final frameOrdinal = _nearestFrameOrdinal(
      stamp,
      frameOrdinalByStamp,
      framePeriodMicros: framePeriodMicros,
    );
    final nearestFrameDistanceMicros = frameOrdinalByStamp.keys.isEmpty
        ? null
        : frameOrdinalByStamp.keys
              .map((candidate) => (candidate + framePeriodMicros - stamp).abs())
              .reduce(math.min);
    if (frameOrdinal != null) {
      frames[frameOrdinal]['editorAttributed'] = true;
    }
    final visibleStart = paint['visibleUtf16Start']! as int;
    final visibleLength = paint['visibleUtf16Length']! as int;
    final expectedVisible = _utf16Slice(
      expectation.source,
      visibleStart,
      visibleLength,
    );
    paintObservations.add({
      'timestampMicros': paint['paintEpochMicros']! as int,
      'frameOrdinal': frameOrdinal,
      'sourceGeneration': generation,
      'visibleSourceSha256': paint['visibleSourceSha256']! as String,
      'expectedVisibleSourceSha256': _sha(expectedVisible),
      'canonicalSelectionBaseUtf16':
          paint['canonicalSelectionBaseUtf16']! as int,
      'canonicalSelectionExtentUtf16':
          paint['canonicalSelectionExtentUtf16']! as int,
      'expectedSelectionBaseUtf16': expectation.selectionBase,
      'expectedSelectionExtentUtf16': expectation.selectionExtent,
      'caretSourceUtf16': paint['caretSourceUtf16'] as int?,
      'caretDisplayUtf16': paint['caretDisplayUtf16'] as int?,
      'semanticsCurrent': paint['semanticsCurrent']! as bool,
      'activeNeutralRowCount': paint['activeNeutralRowCount']! as int,
      '_nearestFrameDistanceMicros': nearestFrameDistanceMicros,
    });
  }
  paintObservations.sort(
    (left, right) => (left['timestampMicros']! as int).compareTo(
      right['timestampMicros']! as int,
    ),
  );

  for (final input in inputObservations) {
    final generation = input['sourceGeneration']! as int;
    final accepted = input['acceptedMicros']! as int;
    final candidatePaints = paintObservations
        .where(
          (paint) =>
              paint['sourceGeneration'] == generation &&
              (paint['timestampMicros']! as int) >= accepted &&
              paint['frameOrdinal'] is int,
        )
        .toList();
    if (candidatePaints.isEmpty) continue;
    final frameOrdinal = candidatePaints.first['frameOrdinal']! as int;
    frames[frameOrdinal]['editorSyncMicros'] =
        (frames[frameOrdinal]['editorSyncMicros']! as int) +
        (input['editorSyncMicros']! as int);
  }

  final warmups = <Map<String, Object?>>[];
  final samples = <Map<String, Object?>>[];
  for (var operation = 0; operation < expected.length; operation += 1) {
    final expectation = expected[operation];
    final declared = expectation.generations
        .map((generation) => generation.generation)
        .toList(growable: false);
    final accepted = declared
        .map(
          (generation) =>
              inputObservations.firstWhere(
                    (input) => input['sourceGeneration'] == generation,
                  )['acceptedMicros']!
                  as int,
        )
        .reduce(math.min);
    final finalGeneration = expectation.finalGeneration;
    final engineMicros = declared
        .map(
          (generation) =>
              engineObservations.firstWhere(
                    (engine) => engine['sourceGeneration'] == generation,
                  )['nativeFfiMicros']!
                  as int,
        )
        .reduce(math.max);
    if (operation < denominator.warmups) {
      warmups.add({
        'index': operation,
        'acceptedMicros': accepted,
        'acceptedSourceGenerations': declared,
        'sourceGeneration': finalGeneration.generation,
        'sourceSha256': _sha(finalGeneration.source),
        'canonicalSelectionBaseUtf16': finalGeneration.selectionBase,
        'canonicalSelectionExtentUtf16': finalGeneration.selectionExtent,
        'engineMicros': engineMicros,
      });
      continue;
    }
    final finalPaints = paintObservations
        .where(
          (paint) => paint['sourceGeneration'] == finalGeneration.generation,
        )
        .toList();
    if (finalPaints.isEmpty) {
      throw StateError(
        'generation ${finalGeneration.generation} never painted',
      );
    }
    final firstPaint = finalPaints.first;
    final provingPaint = finalPaints.cast<Map<String, Object?>>().firstWhere(
      (paint) => paint['frameOrdinal'] is int,
      orElse: () {
        final distances =
            finalPaints
                .map((paint) => paint['_nearestFrameDistanceMicros'])
                .whereType<int>()
                .toList()
              ..sort();
        throw StateError(
          'generation ${finalGeneration.generation} has no proving '
          'FrameTiming; nearest=${distances.isEmpty ? 'none' : distances.first}',
        );
      },
    );
    final currentPaint = finalPaints.cast<Map<String, Object?>>().where(
      (paint) => paint['semanticsCurrent'] == true,
    );
    final sample = <String, Object?>{
      'index': operation - denominator.warmups,
      'scheduledMicros': expectation.scheduledMicros,
      'acceptedMicros': accepted,
      'sourcePaintMicros': firstPaint['timestampMicros'],
      'caretPaintMicros': firstPaint['timestampMicros'],
      'selectionPaintMicros': firstPaint['timestampMicros'],
      'acceptedSourceGenerations': declared,
      'sourceGeneration': finalGeneration.generation,
      'paintedSourceGeneration': firstPaint['sourceGeneration'],
      'sourceSha256': _sha(finalGeneration.source),
      'visibleSourceSha256': firstPaint['visibleSourceSha256'],
      'canonicalSelectionBaseUtf16': finalGeneration.selectionBase,
      'canonicalSelectionExtentUtf16': finalGeneration.selectionExtent,
      'paintedCaretSourceUtf16': firstPaint['caretSourceUtf16'],
      'startFrameOrdinal': paintObservations
          .where((paint) => declared.contains(paint['sourceGeneration']))
          .where((paint) => paint['frameOrdinal'] is int)
          .map((paint) => paint['frameOrdinal']! as int)
          .reduce(math.min),
      'endFrameOrdinal': paintObservations
          .where((paint) => declared.contains(paint['sourceGeneration']))
          .where((paint) => paint['frameOrdinal'] is int)
          .map((paint) => paint['frameOrdinal']! as int)
          .reduce(math.max),
      'provingFrameOrdinal': provingPaint['frameOrdinal'],
      'engineMicros': engineMicros,
      'visibleCertificationMicros': currentPaint.isEmpty
          ? 0
          : (currentPaint.first['timestampMicros']! as int) - accepted,
      'openToEditableMicros': openAcceptedMicros == null
          ? null
          : (firstPaint['timestampMicros']! as int) - openAcceptedMicros,
      'rawProjectionFrames': finalPaints
          .where((paint) => (paint['activeNeutralRowCount']! as int) > 0)
          .length,
      'sourceIdentityMatched': finalPaints.every(
        (paint) =>
            paint['visibleSourceSha256'] ==
            paint['expectedVisibleSourceSha256'],
      ),
      'caretIdentityMatched': finalPaints.every(
        (paint) =>
            paint['caretSourceUtf16'] == finalGeneration.selectionExtent &&
            paint['caretDisplayUtf16'] != null,
      ),
      'selectionIdentityMatched': finalPaints.every(
        (paint) =>
            paint['canonicalSelectionBaseUtf16'] ==
                finalGeneration.selectionBase &&
            paint['canonicalSelectionExtentUtf16'] ==
                finalGeneration.selectionExtent,
      ),
      'faulted': settled.faulted,
      'resyncCount': settled.resyncCount,
    };
    samples.add(sample);
  }

  return {
    'run': runIndex,
    'processId': '${settled.appProcessId}',
    'freshProcess': denominator.processRule == DogfoodProcessRule.freshEveryRun,
    'warmups': warmups,
    'samples': samples,
    'frames': frames,
    'inputObservations': inputObservations,
    'paintObservations': [
      for (final paint in paintObservations)
        {
          for (final entry in paint.entries)
            if (!entry.key.startsWith('_')) entry.key: entry.value,
        },
    ],
    'engineObservations': engineObservations,
    'memory': [
      {
        'stage': 'baseline',
        'timestampMicros': baseline.receiptEpochMicros,
        'rssBytes': baseline.currentRssBytes,
      },
      {
        'stage': 'peak',
        'timestampMicros': closed['closeRequestedEpochMicros'],
        'rssBytes': math.max(
          settled.maximumRssBytes,
          closed['closeRequestedMaximumRssBytes']! as int,
        ),
      },
      {
        'stage': 'close',
        'timestampMicros': closed['closeRequestedEpochMicros'],
        'rssBytes': closed['closeRequestedRssBytes'],
      },
      {
        'stage': 'postClose',
        'timestampMicros': closed['postCloseEpochMicros'],
        'rssBytes': closed['postCloseRssBytes'],
      },
    ],
  };
}

Map<int, (int, int)> _ordinaryInputEvents(List<String> events) {
  final result = <int, (int, int)>{};
  final pattern = RegExp(
    r'^(\d+):accepted-(?:deltas|full-value):generation=(\d+):elapsedMicros=(\d+)$',
  );
  for (final event in events) {
    final match = pattern.firstMatch(event);
    if (match == null) continue;
    final generation = int.parse(match.group(2)!);
    if (generation == 0) continue;
    result[generation] = (
      int.parse(match.group(1)!),
      int.parse(match.group(3)!),
    );
  }
  return result;
}

Map<int, (int, int)> _semanticInputEvents(
  List<String> events,
  List<Map<String, Object?>> receipts,
) {
  final actionEpochs = <int>[];
  final pattern = RegExp(r'^(\d+):action:');
  for (final event in events) {
    final match = pattern.firstMatch(event);
    if (match != null) actionEpochs.add(int.parse(match.group(1)!));
  }
  final orderedReceipts = [...receipts]
    ..sort(
      (left, right) => (left['sourceGeneration']! as int).compareTo(
        right['sourceGeneration']! as int,
      ),
    );
  if (actionEpochs.length != orderedReceipts.length) {
    throw StateError(
      'semantic action events (${actionEpochs.length}) do not match receipts '
      '(${orderedReceipts.length})',
    );
  }
  return {
    for (var index = 0; index < orderedReceipts.length; index += 1)
      orderedReceipts[index]['sourceGeneration']! as int: (
        actionEpochs[index],
        orderedReceipts[index]['platformCallbackMicros']! as int,
      ),
  };
}

Map<int, (int, int)> _historyInputEvents(
  List<String> events,
  List<Map<String, Object?>> receipts,
) {
  final shortcuts = <(int, String)>[];
  final pattern = RegExp(r'^(\d+):shortcut:(undo|redo)$');
  for (final event in events) {
    final match = pattern.firstMatch(event);
    if (match == null) continue;
    shortcuts.add((int.parse(match.group(1)!), match.group(2)!));
  }
  final historyReceipts =
      receipts
          .where(
            (receipt) => receipt['kind'] == 'undo' || receipt['kind'] == 'redo',
          )
          .toList()
        ..sort(
          (left, right) => (left['sourceGeneration']! as int).compareTo(
            right['sourceGeneration']! as int,
          ),
        );
  if (shortcuts.length != historyReceipts.length) {
    throw StateError(
      'history shortcuts (${shortcuts.length}) do not match receipts '
      '(${historyReceipts.length})',
    );
  }
  for (var index = 0; index < historyReceipts.length; index += 1) {
    if (historyReceipts[index]['kind'] != shortcuts[index].$2) {
      throw StateError(
        'history shortcut ${shortcuts[index].$2} does not match '
        'receipt ${historyReceipts[index]['kind']}',
      );
    }
  }
  return {
    for (var index = 0; index < historyReceipts.length; index += 1)
      historyReceipts[index]['sourceGeneration']! as int: (
        shortcuts[index].$1,
        0,
      ),
  };
}

Map<String, Object?> _inputObservation(
  _ExpectedGeneration expectation, {
  required int acceptedMicros,
  required int editorSyncMicros,
}) => {
  'sourceGeneration': expectation.generation,
  'acceptedMicros': acceptedMicros,
  'editorSyncMicros': editorSyncMicros,
  'sourceSha256': _sha(expectation.source),
  'canonicalSelectionBaseUtf16': expectation.selectionBase,
  'canonicalSelectionExtentUtf16': expectation.selectionExtent,
};

String _utf16Slice(String source, int start, int length) {
  final units = source.codeUnits;
  final end = start + length;
  if (start < 0 || end < start || end > units.length) {
    throw RangeError.range(end, start, units.length, 'visible UTF-16 end');
  }
  return String.fromCharCodes(units.sublist(start, end));
}

String _sha(String source) => sha256.convert(utf8.encode(source)).toString();

int? _nearestFrameOrdinal(
  int paintTargetStamp,
  Map<int, int> ordinalsByVsyncStart, {
  required int framePeriodMicros,
}) {
  int? bestOrdinal;
  // Flutter's onBeginFrame receives the engine's frame target time, which is
  // what currentSystemFrameTimeStamp exposes during paint. FrameTiming records
  // the same frame's vsync start. On macOS the target is one nominal display
  // period after that start; both values are independently microsecond-rounded.
  // A 32 us tolerance is still over 250x narrower than one 120 Hz frame.
  var bestDistance = 33;
  for (final entry in ordinalsByVsyncStart.entries) {
    final distance = (entry.key + framePeriodMicros - paintTargetStamp).abs();
    if (distance < bestDistance) {
      bestDistance = distance;
      bestOrdinal = entry.value;
    }
  }
  return bestOrdinal;
}
