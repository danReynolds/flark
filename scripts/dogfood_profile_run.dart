// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:crypto/crypto.dart';

import '../packages/flark/example/lib/dogfood_documents.dart';
import '../packages/flark/test/support/macos_native_canary_driver.dart';
import 'dogfood_fixture_identity.dart';
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
    required this.display,
    required this.run,
  });

  final int initialSourceBytes;
  final Map<String, Object?> display;
  final Map<String, Object?> run;
}

Future<void> main(List<String> arguments) async {
  if (arguments.length == 1 && arguments.single == '--list') {
    for (final entry in requiredDogfoodCells.entries) {
      stdout.writeln('${entry.key}\t${entry.value.runs}');
    }
    return;
  }
  if (arguments.length != 6) {
    stderr.writeln(
      'usage: dart run scripts/dogfood_profile_run.dart '
      '<cell-id> <run-index> <app-executable> <embedded-abi> '
      '<bundle-manifest.json> <output.json>',
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
    final appExecutable = File(arguments[2]).absolute;
    final embeddedAbi = File(arguments[3]).absolute;
    final manifest = File(arguments[4]).absolute;
    final result = await _runCell(
      cellId: cellId,
      runIndex: runIndex,
      denominator: denominator,
      appExecutable: appExecutable,
      embeddedAbi: embeddedAbi,
    );
    final manifestValue = jsonDecode(await manifest.readAsString());
    if (manifestValue is! Map<String, Object?> ||
        manifestValue['schema'] != 'dogfood_bundle_manifest_v1' ||
        manifestValue['sha256'] is! String) {
      throw const FormatException('Invalid dogfood bundle manifest.');
    }
    final fixture = dogfoodFixtureIdentity(cellId);
    if (fixture['sourceBytes'] != result.initialSourceBytes) {
      throw StateError('$cellId source size disagrees with its frozen fixture');
    }
    final output = File(arguments[5]);
    await output.parent.create(recursive: true);
    await output.writeAsString(
      '${jsonEncode({
        'id': cellId,
        'sourceBytes': result.initialSourceBytes,
        'warmupsPerRun': denominator.warmups,
        'samplesPerRun': denominator.samples,
        'runCount': denominator.runs,
        'cadenceHz': denominator.cadenceHz,
        'binding': {
          'candidateCommit': await _git(const ['rev-parse', 'HEAD']),
          'candidateTree': await _git(const ['rev-parse', 'HEAD^{tree}']),
          'bundleManifestSha256': manifestValue['sha256'],
          'mainExecutable': await _fileIdentity(appExecutable),
          'embeddedAbi': await _fileIdentity(embeddedAbi),
        },
        'fixture': fixture,
        'display': result.display,
        'run': result.run,
      })}\n',
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

Future<Map<String, Object>> _fileIdentity(File file) async => {
  'path': file.absolute.path,
  'bytes': await file.length(),
  'sha256': (await sha256.bind(file.openRead()).first).toString(),
};

Future<String> _git(List<String> arguments) async {
  final repository = File.fromUri(Platform.script).parent.parent;
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
  final targetPreset = _presetFor(cellId);
  final initialPreset = cellId.endsWith('-journey')
      ? DogfoodDocumentPreset.productTour
      : targetPreset;
  final initialSource = buildDogfoodDocument(initialPreset);
  final driver = MacosNativeCanaryDriver(
    appExecutable: appExecutable.path,
    libraryPath: embeddedAbi.path,
    actuatorScript: File(
      'packages/flark/tool/live_editor_macos_canary.swift',
    ).absolute.path,
    initialPresetName: initialPreset.name,
  );
  try {
    await driver.start();
    final launched = await driver.prepareObservationWindow(
      windowWidth: _windowWidth,
      windowHeight: _windowHeight,
    );
    if (launched.source != initialSource) {
      throw StateError('$cellId opened a source different from its preset');
    }
    if (cellId.endsWith('-journey')) {
      final targetSource = buildDogfoodDocument(targetPreset);
      final run = await _largePresetJourneyRun(
        driver: driver,
        launched: launched,
        preset: targetPreset,
        source: targetSource,
        cellId: cellId,
        runIndex: runIndex,
        denominator: denominator,
      );
      return _ProfileRunResult(
        initialSourceBytes: utf8.encode(targetSource).length,
        display: _profileDisplay(launched.display),
        run: run,
      );
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
        display: _profileDisplay(launched.display),
        run: run,
      );
    }
    if (cellId == 'lifecycle-same-process' ||
        cellId == 'lifecycle-fresh-process') {
      final run = await _lifecycleRun(
        driver: driver,
        launched: launched,
        source: initialSource,
        runIndex: runIndex,
        denominator: denominator,
      );
      return _ProfileRunResult(
        initialSourceBytes: utf8.encode(initialSource).length,
        display: _profileDisplay(launched.display),
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
      display: _profileDisplay(settled.display),
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

Map<String, Object?> _profileDisplay(Map<String, Object?> appDisplay) => {
  'refreshHz': appDisplay['refreshHz'],
  'framePeriodMicros': 1000000 / (appDisplay['refreshHz']! as num),
  'widthLogical': _windowWidth,
  'heightLogical': _windowHeight,
  'devicePixelRatio': appDisplay['devicePixelRatio'],
};

DogfoodDocumentPreset _presetFor(String cellId) {
  if (cellId.startsWith('product-tour') || cellId.startsWith('lifecycle-')) {
    return DogfoodDocumentPreset.productTour;
  }
  if (cellId.startsWith('ordinary-1m')) {
    return DogfoodDocumentPreset.prose1MiB;
  }
  return switch (cellId) {
    'dense-blocks-1m-journey' => DogfoodDocumentPreset.denseBlocks1MiB,
    'ordinary-5m-journey' => DogfoodDocumentPreset.prose5MiB,
    'giant-line-5m-journey' => DogfoodDocumentPreset.giantLine5MiB,
    'ordinary-10m-journey' => DogfoodDocumentPreset.prose10MiB,
    _ => throw UnsupportedError('$cellId has no implemented app preset'),
  };
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
    openSnapshot: launched,
  );
}

Future<Map<String, Object?>> _largePresetJourneyRun({
  required MacosNativeCanaryDriver driver,
  required MacosNativeCanarySnapshot launched,
  required DogfoodDocumentPreset preset,
  required String source,
  required String cellId,
  required int runIndex,
  required DogfoodCellDenominator denominator,
}) async {
  final opened = await driver.selectPreset(preset.name);
  if (opened.source != source || opened.sourceGeneration != 0) {
    throw StateError('$cellId did not open its exact target preset');
  }
  final accepted = opened.openAcceptedEpochMicros;
  if (accepted == null) {
    throw StateError('$cellId did not record preset-selection acceptance');
  }
  final caret = switch (preset) {
    DogfoodDocumentPreset.denseBlocks1MiB =>
      source.indexOf('Short bounded paragraph 000001.') +
          'Short bounded'.length,
    DogfoodDocumentPreset.giantLine5MiB =>
      source.indexOf('giant-word') + 'giant-word'.length,
    DogfoodDocumentPreset.prose5MiB || DogfoodDocumentPreset.prose10MiB =>
      source.indexOf('This is ordinary prose for') + 'This is ordinary'.length,
    _ => throw UnsupportedError('$cellId has no large-preset anchor'),
  };
  final baseline = await driver.activateAtUtf16(
    caret,
    windowWidth: _windowWidth,
    windowHeight: _windowHeight,
    retainObservations: true,
  );
  final expected = _pasteUndoExpectations(
    source: source,
    caret: caret,
    paste: 'x',
    firstGeneration: baseline.sourceGeneration + 1,
    count: 1,
  );
  await driver.typeText('x');
  await driver.settle();
  await driver.pressKey('undo');
  await driver.settle();
  await driver.scrollBy(_windowHeight * 2 + 1);
  final away = await driver.settle();
  if ((away.scrollOffset - baseline.scrollOffset).abs() < _windowHeight * 2) {
    throw StateError('$cellId did not traverse two viewport heights');
  }
  await driver.scrollBy(-(_windowHeight * 2 + 1));
  final settled = await driver.settle();
  final finalGeneration = expected.single.finalGeneration;
  if (settled.source != finalGeneration.source ||
      settled.selectionBaseUtf16 != finalGeneration.selectionBase ||
      settled.selectionExtentUtf16 != finalGeneration.selectionExtent ||
      settled.scrollOffset != baseline.scrollOffset) {
    throw StateError('$cellId did not complete its edit/undo/scroll journey');
  }
  final closed = await driver.closeSession();
  return _buildMeasuredRun(
    runIndex: runIndex,
    denominator: denominator,
    baseline: launched,
    settled: settled,
    closed: closed,
    expected: expected,
    openAcceptedMicros: accepted,
    openKind: 'presetSelection',
    openSnapshot: opened,
  );
}

Future<Map<String, Object?>> _lifecycleRun({
  required MacosNativeCanaryDriver driver,
  required MacosNativeCanarySnapshot launched,
  required String source,
  required int runIndex,
  required DogfoodCellDenominator denominator,
}) async {
  final fragments = <Map<String, Object?>>[];
  var opened = launched;
  for (
    var sessionOrdinal = 0;
    sessionOrdinal < denominator.samples;
    sessionOrdinal += 1
  ) {
    if (sessionOrdinal > 0) {
      opened = await driver.reset(
        id: 'lifecycle-$runIndex-$sessionOrdinal',
        source: source,
      );
    }
    if (opened.source != source || opened.sourceGeneration != 0) {
      throw StateError(
        'lifecycle session $sessionOrdinal did not open the pristine preset',
      );
    }
    final marker = 'locally.';
    final caret = source.indexOf(marker) + marker.length;
    final baseline = await driver.activateAtUtf16(
      caret,
      windowWidth: _windowWidth,
      windowHeight: _windowHeight,
    );
    final expected = _pasteUndoExpectations(
      source: source,
      caret: caret,
      paste: 'x',
      firstGeneration: baseline.sourceGeneration + 1,
      count: 1,
    );
    await driver.typeText('x');
    await driver.settle();
    await driver.pressKey('undo');
    final settled = await driver.settle();
    final finalGeneration = expected.single.finalGeneration;
    if (settled.source != finalGeneration.source ||
        settled.selectionBaseUtf16 != finalGeneration.selectionBase ||
        settled.selectionExtentUtf16 != finalGeneration.selectionExtent) {
      throw StateError(
        'lifecycle session $sessionOrdinal did not undo to the pristine source',
      );
    }
    final closed = await driver.closeSession();
    fragments.add(
      _buildMeasuredRun(
        runIndex: runIndex,
        denominator: denominator,
        baseline: baseline,
        settled: settled,
        closed: closed,
        expected: expected,
        sessionOrdinal: sessionOrdinal,
      ),
    );
  }
  return _mergeLifecycleFragments(
    runIndex: runIndex,
    denominator: denominator,
    fragments: fragments,
  );
}

Map<String, Object?> _mergeLifecycleFragments({
  required int runIndex,
  required DogfoodCellDenominator denominator,
  required List<Map<String, Object?>> fragments,
}) {
  if (fragments.isEmpty) {
    throw StateError('lifecycle run has no sessions');
  }
  final processIds = fragments.map((fragment) => fragment['processId']).toSet();
  if (processIds.length != 1) {
    throw StateError('one lifecycle run crossed process identities');
  }
  final warmups = <Map<String, Object?>>[];
  final samples = <Map<String, Object?>>[];
  final frames = <Map<String, Object?>>[];
  final inputs = <Map<String, Object?>>[];
  final paints = <Map<String, Object?>>[];
  final engines = <Map<String, Object?>>[];
  var frameOffset = 0;
  for (
    var sessionOrdinal = 0;
    sessionOrdinal < fragments.length;
    sessionOrdinal += 1
  ) {
    final fragment = fragments[sessionOrdinal];
    final fragmentFrames = (fragment['frames']! as List)
        .cast<Map<String, Object?>>();
    for (final frame in fragmentFrames) {
      frames.add({
        ...frame,
        'ordinal': (frame['ordinal']! as int) + frameOffset,
      });
    }
    for (final paint
        in (fragment['paintObservations']! as List)
            .cast<Map<String, Object?>>()) {
      final ordinal = paint['frameOrdinal'];
      paints.add({
        ...paint,
        if (ordinal is int) 'frameOrdinal': ordinal + frameOffset,
      });
    }
    inputs.addAll(
      (fragment['inputObservations']! as List).cast<Map<String, Object?>>(),
    );
    engines.addAll(
      (fragment['engineObservations']! as List).cast<Map<String, Object?>>(),
    );
    for (final sample
        in (fragment['samples']! as List).cast<Map<String, Object?>>()) {
      samples.add({
        ...sample,
        'index': sessionOrdinal,
        'startFrameOrdinal':
            (sample['startFrameOrdinal']! as int) + frameOffset,
        'endFrameOrdinal': (sample['endFrameOrdinal']! as int) + frameOffset,
        'provingFrameOrdinal':
            (sample['provingFrameOrdinal']! as int) + frameOffset,
      });
    }
    frameOffset += fragmentFrames.length;
  }
  final firstMemory = (fragments.first['memory']! as List)
      .cast<Map<String, Object?>>();
  final lastMemory = (fragments.last['memory']! as List)
      .cast<Map<String, Object?>>();
  final peak = fragments
      .expand(
        (fragment) =>
            (fragment['memory']! as List).cast<Map<String, Object?>>(),
      )
      .where((sample) => sample['stage'] == 'peak')
      .reduce(
        (left, right) =>
            (left['rssBytes']! as int) >= (right['rssBytes']! as int)
            ? left
            : right,
      );
  return {
    'run': runIndex,
    'processId': fragments.first['processId'],
    'freshProcess': denominator.processRule == DogfoodProcessRule.freshEveryRun,
    'openObservation': null,
    'warmups': warmups,
    'samples': samples,
    'frames': frames,
    'inputObservations': inputs,
    'paintObservations': paints,
    'engineObservations': engines,
    'memory': [
      firstMemory.firstWhere((sample) => sample['stage'] == 'baseline'),
      peak,
      lastMemory.firstWhere((sample) => sample['stage'] == 'close'),
      lastMemory.firstWhere((sample) => sample['stage'] == 'postClose'),
    ],
  };
}

Map<String, Object?> _buildMeasuredRun({
  required int runIndex,
  required DogfoodCellDenominator denominator,
  required MacosNativeCanarySnapshot baseline,
  required MacosNativeCanarySnapshot settled,
  required Map<String, Object?> closed,
  required List<_ExpectedSample> expected,
  int sessionOrdinal = 0,
  int? openAcceptedMicros,
  String openKind = 'processLaunch',
  MacosNativeCanarySnapshot? openSnapshot,
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
          sessionOrdinal: sessionOrdinal,
          acceptedMicros: openAcceptedMicros,
          editorSyncMicros: 0,
        ),
      );
      engineObservations.add({
        'sessionOrdinal': sessionOrdinal,
        'sourceGeneration': 0,
        'nativeFfiMicros': 0,
      });
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
        sessionOrdinal: sessionOrdinal,
        acceptedMicros: input.$1,
        editorSyncMicros: input.$2,
      ),
    );
    final performance = performanceByGeneration[generation];
    if (performance == null) {
      throw StateError('generation $generation has no engine receipt');
    }
    engineObservations.add({
      'sessionOrdinal': sessionOrdinal,
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
      'sessionOrdinal': sessionOrdinal,
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
      'activeRowVisible': paint['activeRowVisible']! as bool,
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

  Map<String, Object?>? openObservation;
  if (openAcceptedMicros != null) {
    final snapshot = openSnapshot;
    if (snapshot == null || snapshot.sourceGeneration != 0) {
      throw StateError('open measurement has no generation-zero snapshot');
    }
    final candidates = <Map<String, Object?>>[];
    for (final rawPaint in snapshot.paintReceipts) {
      if (rawPaint['sourceGeneration'] != 0 ||
          rawPaint['semanticsCurrent'] != true ||
          rawPaint['activeNeutralRowCount'] != 0 ||
          rawPaint['activeRowVisible'] != true ||
          (rawPaint['paintEpochMicros']! as int) < openAcceptedMicros) {
        continue;
      }
      final frameOrdinal = _nearestFrameOrdinal(
        rawPaint['frameStampMicros']! as int,
        frameOrdinalByStamp,
        framePeriodMicros: framePeriodMicros,
      );
      if (frameOrdinal == null) continue;
      final visibleStart = rawPaint['visibleUtf16Start']! as int;
      final visibleLength = rawPaint['visibleUtf16Length']! as int;
      candidates.add({
        'timestampMicros': rawPaint['paintEpochMicros'],
        'frameOrdinal': frameOrdinal,
        'visibleSourceSha256': rawPaint['visibleSourceSha256'],
        'expectedVisibleSourceSha256': _sha(
          _utf16Slice(snapshot.source, visibleStart, visibleLength),
        ),
        'canonicalSelectionBaseUtf16': rawPaint['canonicalSelectionBaseUtf16'],
        'canonicalSelectionExtentUtf16':
            rawPaint['canonicalSelectionExtentUtf16'],
        'caretSourceUtf16': rawPaint['caretSourceUtf16'],
        'caretDisplayUtf16': rawPaint['caretDisplayUtf16'],
      });
    }
    if (candidates.isEmpty) {
      throw StateError('open measurement has no certified proving paint');
    }
    final paint = candidates.first;
    frames[paint['frameOrdinal']! as int]['editorAttributed'] = true;
    openObservation = {
      'kind': openKind,
      'acceptedMicros': openAcceptedMicros,
      'paintMicros': paint['timestampMicros'],
      'openToEditableMicros':
          (paint['timestampMicros']! as int) - openAcceptedMicros,
      'sourceGeneration': 0,
      'sourceSha256': _sha(snapshot.source),
      'frameOrdinal': paint['frameOrdinal'],
      'visibleSourceSha256': paint['visibleSourceSha256'],
      'expectedVisibleSourceSha256': paint['expectedVisibleSourceSha256'],
      'canonicalSelectionBaseUtf16': paint['canonicalSelectionBaseUtf16'],
      'canonicalSelectionExtentUtf16': paint['canonicalSelectionExtentUtf16'],
      'expectedSelectionBaseUtf16': snapshot.selectionBaseUtf16,
      'expectedSelectionExtentUtf16': snapshot.selectionExtentUtf16,
      'caretSourceUtf16': paint['caretSourceUtf16'],
      'caretDisplayUtf16': paint['caretDisplayUtf16'],
      'semanticsCurrent': true,
      'activeNeutralRowCount': 0,
    };
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
        'sessionOrdinal': sessionOrdinal,
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
      'sessionOrdinal': sessionOrdinal,
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
      'rawProjectionFrames': finalPaints
          .where((paint) => (paint['activeNeutralRowCount']! as int) > 0)
          .length,
      'sourceIdentityMatched': finalPaints.every(
        (paint) =>
            paint['visibleSourceSha256'] ==
            paint['expectedVisibleSourceSha256'],
      ),
      'caretIdentityMatched': finalPaints.every(
        (paint) => paint['activeRowVisible'] == true
            ? paint['caretSourceUtf16'] == finalGeneration.selectionExtent &&
                  paint['caretDisplayUtf16'] != null
            : paint['caretSourceUtf16'] == null &&
                  paint['caretDisplayUtf16'] == null,
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
      if (denominator.requiresLiveStateZero)
        'globalLiveState': closed['globalLiveState'],
    };
    samples.add(sample);
  }

  return {
    'run': runIndex,
    'processId': '${settled.appProcessId}',
    'freshProcess': denominator.processRule == DogfoodProcessRule.freshEveryRun,
    'openObservation': openObservation,
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
  required int sessionOrdinal,
  required int acceptedMicros,
  required int editorSyncMicros,
}) => {
  'sessionOrdinal': sessionOrdinal,
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
