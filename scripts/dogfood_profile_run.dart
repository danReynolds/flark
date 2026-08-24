// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:crypto/crypto.dart';

import '../packages/flark/example/lib/dogfood_documents.dart';
import '../packages/flark/test/support/macos_native_canary_driver.dart';
import 'dogfood_fixture_identity.dart';
import 'dogfood_host_identity.dart';
import 'verify_v4_dogfood_receipt.dart';

const _profileCadence = Duration(microseconds: 16667);
const _structuralBurstCadence = Duration(microseconds: 33333);
const _windowWidth = 1569;
const _windowHeight = 906;
const _structuralBurstEvidenceDenominator = DogfoodCellDenominator(
  warmups: 0,
  samples: 1,
  runs: 3,
  cadenceHz: 0,
);

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
    this.measuredGenerationIndex,
  });

  final int index;
  final List<_ExpectedGeneration> generations;
  final int? scheduledMicros;
  final int? measuredGenerationIndex;

  _ExpectedGeneration get finalGeneration => generations.last;
  _ExpectedGeneration get measuredGeneration =>
      generations[measuredGenerationIndex ?? generations.length - 1];
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
          'measurementHost': await dogfoodMeasurementHostIdentity(),
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
    initialWindowWidth: _windowWidth,
    initialWindowHeight: _windowHeight,
  );
  try {
    final launched = cellId == 'product-tour-cold-launch'
        ? await driver.start()
        : await driver.prepareObservationWindow(
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
    if (cellId.endsWith('-structural-burst')) {
      final run = await _structuralCellRun(
        driver: driver,
        source: initialSource,
        cellId: cellId,
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
        final productTour = cellId.startsWith('product-tour');
        final offset = productTour
            ? initialSource.indexOf('This intentionally') + 'This '.length
            : initialSource.indexOf('This is ordinary') + 'This '.length;
        if (productTour) {
          // The frozen deletion target intentionally lives in the wrapped
          // paragraph below the first viewport. Setup wheel input is excluded
          // when activateAtUtf16 resets the measured observation window.
          await driver.scrollBy(_windowHeight);
          await driver.settle();
        }
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
    _requireExpectedTerminalState(
      cellId: cellId,
      baseline: baseline,
      settled: settled,
      expected: expected,
    );
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

enum _StructuralPhase { latency, burst, perEditControl }

Future<Map<String, Object?>> _structuralCellRun({
  required MacosNativeCanaryDriver driver,
  required String source,
  required String cellId,
  required int runIndex,
  required DogfoodCellDenominator denominator,
}) async {
  final marker = cellId.startsWith('product-tour')
      ? 'locally.'
      : 'parser catches up.';
  final offset = source.indexOf(marker) + marker.length;
  if (offset < marker.length) {
    throw StateError('$cellId is missing its frozen structural anchor');
  }
  final total = denominator.warmups + denominator.samples;

  Future<Map<String, Object?>> runPhase(_StructuralPhase phase) async {
    final setupAcknowledgementStart = driver.appAcknowledgementCount;
    final opened = await driver.reset(
      id: '$cellId-${phase.name}-$runIndex',
      source: source,
    );
    if (opened.source != source || opened.sourceGeneration != 0) {
      throw StateError('$cellId ${phase.name} did not open its exact fixture');
    }
    final baseline = await driver.activateAtUtf16(
      offset,
      windowWidth: _windowWidth,
      windowHeight: _windowHeight,
    );
    final setupAcknowledgements = driver.appAcknowledgementsSince(
      setupAcknowledgementStart,
    );
    if (setupAcknowledgements.length != 2 ||
        setupAcknowledgements[0]['operation'] != 'reset' ||
        setupAcknowledgements[1]['operation'] != 'activateAtUtf16' ||
        setupAcknowledgements.any(
          (acknowledgement) => acknowledgement['canaryId'] != baseline.canaryId,
        )) {
      throw StateError(
        '$cellId ${phase.name} did not bind reset and activation to one app session',
      );
    }
    final commandSequenceStart = driver.commandSequence;
    final measurementAcknowledgementStart = driver.appAcknowledgementCount;
    final expected = _structuralExpectations(
      source: baseline.source,
      caret: offset,
      firstGeneration: baseline.sourceGeneration + 1,
      count: total,
      scheduledCadence: phase == _StructuralPhase.burst
          ? _structuralBurstCadence
          : null,
    );
    final transcript = <String>[];
    switch (phase) {
      case _StructuralPhase.latency:
        for (var index = 0; index < total; index += 1) {
          transcript.add('typeStructuralBursts:1:0');
          await driver.typeStructuralBursts(count: 1, cadence: Duration.zero);
          transcript.add('settle');
          await driver.settle();
        }
      case _StructuralPhase.burst:
        transcript.add(
          'typeStructuralBursts:$total:'
          '${_structuralBurstCadence.inMicroseconds}',
        );
        await driver.typeStructuralBursts(
          count: total,
          cadence: _structuralBurstCadence,
        );
      case _StructuralPhase.perEditControl:
        for (var index = 0; index < total; index += 1) {
          transcript.add('pressKey:enter');
          await driver.pressKey('enter');
          transcript.add('settle');
          await driver.settle();
          transcript.add('typeText:x:0');
          await driver.typeText('x', cadence: Duration.zero);
          transcript.add('settle');
          await driver.settle();
        }
    }
    transcript.add('settle');
    final settled = await driver.settle();
    _requireExpectedTerminalState(
      cellId: '$cellId ${phase.name}',
      baseline: baseline,
      settled: settled,
      expected: expected,
    );
    transcript.add('closeSession');
    final closed = await driver.closeSession();
    final commandSequenceEnd = driver.commandSequence;
    final appAcknowledgements = driver.appAcknowledgementsSince(
      measurementAcknowledgementStart,
    );
    Map<String, Object?> bindPhase(Map<String, Object?> measured) => {
      ...measured,
      'structuralPhase': phase.name,
      'structuralSessionIdentity': baseline.canaryId,
      'structuralActuatorSequenceStart': commandSequenceStart,
      'structuralActuatorSequenceEnd': commandSequenceEnd,
      'structuralCommandTranscript': transcript,
      'structuralSetupAcknowledgements': setupAcknowledgements,
      'structuralAppAcknowledgements': appAcknowledgements,
    };
    if (phase == _StructuralPhase.burst) {
      final allGenerations = [
        for (final sample in expected) ...sample.generations,
      ];
      return bindPhase(
        _buildMeasuredRun(
          runIndex: runIndex,
          denominator: _structuralBurstEvidenceDenominator,
          baseline: baseline,
          settled: settled,
          closed: closed,
          expected: [
            _ExpectedSample(
              index: 0,
              generations: allGenerations,
              scheduledMicros: null,
            ),
          ],
        ),
      );
    }
    return bindPhase(
      _buildMeasuredRun(
        runIndex: runIndex,
        denominator: denominator,
        baseline: baseline,
        settled: settled,
        closed: closed,
        expected: expected,
      ),
    );
  }

  final latency = await runPhase(_StructuralPhase.latency);
  final burst = await runPhase(_StructuralPhase.burst);
  final control = runIndex == 0
      ? await runPhase(_StructuralPhase.perEditControl)
      : null;
  return {
    ...latency,
    'structuralEvidenceVersion': 1,
    'structuralBurst': burst,
    'structuralPerEditControl': ?control,
  };
}

void _requireExpectedTerminalState({
  required String cellId,
  required MacosNativeCanarySnapshot baseline,
  required MacosNativeCanarySnapshot settled,
  required List<_ExpectedSample> expected,
}) {
  if (settled.faulted ||
      settled.lastError != null ||
      settled.resyncCount != baseline.resyncCount) {
    throw StateError(
      '$cellId became unhealthy: faulted=${settled.faulted} '
      'lastError=${settled.lastError} '
      'resyncCount=${baseline.resyncCount}->${settled.resyncCount} '
      'lastResyncReason=${settled.lastResyncReason}',
    );
  }
  final finalGeneration = expected.last.finalGeneration;
  if (settled.source != finalGeneration.source ||
      settled.selectionBaseUtf16 != finalGeneration.selectionBase ||
      settled.selectionExtentUtf16 != finalGeneration.selectionExtent) {
    throw StateError(
      '$cellId did not reach its deterministic final state: '
      'sourceGeneration=${settled.sourceGeneration} '
      'actualSourceLength=${settled.source.length} '
      'expectedSourceLength=${finalGeneration.source.length} '
      'actualSourceSha256=${_sha(settled.source)} '
      'expectedSourceSha256=${_sha(finalGeneration.source)} '
      'actualSelection=${settled.selectionBaseUtf16}..'
      '${settled.selectionExtentUtf16} '
      'expectedSelection=${finalGeneration.selectionBase}..'
      '${finalGeneration.selectionExtent} '
      'resyncCount=${settled.resyncCount} '
      'lastResyncReason=${settled.lastResyncReason} '
      'faulted=${settled.faulted} lastError=${settled.lastError}',
    );
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
  required Duration? scheduledCadence,
}) {
  final result = <_ExpectedSample>[];
  var current = source;
  var currentCaret = caret;
  var generation = firstGeneration;
  for (var index = 0; index < count; index += 1) {
    // The parser-authored paragraph-break action commits the blank-line
    // separator required to split one Markdown paragraph into two blocks.
    current = current.replaceRange(currentCaret, currentCaret, '\n\n');
    currentCaret += 2;
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
        scheduledMicros: scheduledCadence == null
            ? null
            : index * scheduledCadence.inMicroseconds,
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
      measuredGenerationIndex: 0,
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
  if (preset == DogfoodDocumentPreset.giantLine5MiB) {
    await driver.pressKey('right');
    final navigated = await driver.settle();
    if (navigated.source != source ||
        navigated.selectionBaseUtf16 != caret + 1 ||
        navigated.selectionExtentUtf16 != caret + 1) {
      throw StateError('$cellId did not navigate its giant physical line');
    }
    await driver.activateAtUtf16(
      caret,
      windowWidth: _windowWidth,
      windowHeight: _windowHeight,
      retainObservations: true,
    );
  }
  final firstEdit = _pasteUndoExpectations(
    source: source,
    caret: caret,
    paste: 'x',
    firstGeneration: baseline.sourceGeneration + 1,
    count: 1,
  ).single;
  await driver.typeText('x');
  await driver.settle();
  await driver.pressKey('undo');
  await driver.settle();
  var expected = [firstEdit];
  if (preset == DogfoodDocumentPreset.prose10MiB) {
    final styledCaret = source.indexOf('**Flark**') + '**Fla'.length;
    if (styledCaret < '**Fla'.length) {
      throw StateError('$cellId is missing its frozen inline-style anchor');
    }
    await driver.activateAtUtf16(
      styledCaret,
      windowWidth: _windowWidth,
      windowHeight: _windowHeight,
      retainObservations: true,
    );
    final styledEdit = _pasteUndoExpectations(
      source: source,
      caret: styledCaret,
      paste: 'y',
      firstGeneration: baseline.sourceGeneration + 3,
      count: 1,
    ).single;
    await driver.typeText('y');
    await driver.settle();
    await driver.pressKey('undo');
    await driver.settle();
    expected = [
      _ExpectedSample(
        index: 0,
        generations: [...firstEdit.generations, ...styledEdit.generations],
        measuredGenerationIndex: 2,
        scheduledMicros: null,
      ),
    ];
  }
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
    'faulted': fragments.any((fragment) => fragment['faulted'] == true),
    'resyncCount': fragments
        .map((fragment) => fragment['resyncCount']! as int)
        .fold<int>(0, (current, value) => math.max(current, value)),
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
  final openMeasurementSnapshot = openSnapshot ?? baseline;
  final expectedByGeneration = <int, _ExpectedGeneration>{
    for (final sample in expected)
      for (final generation in sample.generations)
        generation.generation: generation,
  };
  if (openAcceptedMicros != null && !expectedByGeneration.containsKey(0)) {
    expectedByGeneration[0] = _ExpectedGeneration(
      generation: 0,
      source: openMeasurementSnapshot.source,
      selectionBase: openMeasurementSnapshot.selectionBaseUtf16,
      selectionExtent: openMeasurementSnapshot.selectionExtentUtf16,
    );
  }
  final performanceByGeneration = <int, Map<String, Object?>>{
    for (final receipt in settled.sourceEditPerformanceReceipts)
      receipt['sourceGeneration']! as int: receipt,
    for (final receipt in settled.semanticEditPerformanceReceipts)
      receipt['sourceGeneration']! as int: receipt,
  };
  final operationTimingByGeneration = _operationTimingsByGeneration(
    settled.inputEvents,
  );
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
          measurementSessionIdentity: openMeasurementSnapshot.canaryId,
          acceptedMicros: openAcceptedMicros,
          editorSyncMicros: 0,
        ),
      );
      engineObservations.add({
        'sessionOrdinal': sessionOrdinal,
        'measurementSessionIdentity': openMeasurementSnapshot.canaryId,
        'sourceGeneration': 0,
        'nativeFfiMicros': 0,
      });
      continue;
    }
    final performance = performanceByGeneration[generation];
    if (performance == null) {
      throw StateError('generation $generation has no acceptance receipt');
    }
    final operationTiming = operationTimingByGeneration[generation];
    final acceptedMicros =
        operationTiming?['_acceptedMicros'] ??
        performance['acceptedAtEpochMicros'];
    final editorSyncMicros =
        operationTiming?['_editorSyncMicros'] ??
        performance['editorSyncMicros'] ??
        performance['platformCallbackMicros'];
    if (acceptedMicros is! int ||
        acceptedMicros <= 0 ||
        editorSyncMicros is! int ||
        editorSyncMicros < 0) {
      throw StateError(
        'generation $generation has an invalid acceptance clock',
      );
    }
    final performanceSessionIdentity = performance['canaryId'];
    if (performanceSessionIdentity is! String ||
        performanceSessionIdentity.isEmpty) {
      throw StateError(
        'generation $generation has no app-authored session identity',
      );
    }
    inputObservations.add(
      _inputObservation(
        expectation,
        sessionOrdinal: sessionOrdinal,
        measurementSessionIdentity: performanceSessionIdentity,
        acceptedMicros: acceptedMicros,
        editorSyncMicros: editorSyncMicros,
        operationTiming: operationTiming,
      ),
    );
    engineObservations.add({
      'sessionOrdinal': sessionOrdinal,
      'measurementSessionIdentity': performanceSessionIdentity,
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
  final clockAnchor = settled.frameClockAnchor;
  final anchorEpochBefore = clockAnchor['epochBeforeMicros']!;
  final anchorEpochAfter = clockAnchor['epochAfterMicros']!;
  final anchorMonotonic = clockAnchor['monotonicMicros']!;
  final anchorEpochMidpoint =
      anchorEpochBefore + ((anchorEpochAfter - anchorEpochBefore) ~/ 2);
  for (var index = 0; index < orderedTimings.length; index += 1) {
    final timing = orderedTimings[index];
    final stamp = timing['vsyncStartMicros']! as int;
    frameOrdinalByStamp[stamp] = index;
    frames.add({
      'ordinal': index,
      'sessionOrdinal': sessionOrdinal,
      'measurementSessionIdentity': timing['canaryId'],
      'vsyncMicros': anchorEpochMidpoint + stamp - anchorMonotonic,
      'monotonicVsyncMicros': stamp,
      'buildStartMonotonicMicros': timing['buildStartMicros'],
      'buildFinishMonotonicMicros': timing['buildFinishMicros'],
      'clockAnchorEpochBeforeMicros': anchorEpochBefore,
      'clockAnchorEpochAfterMicros': anchorEpochAfter,
      'clockAnchorMonotonicMicros': anchorMonotonic,
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
    final frameOrdinal = _frameOrdinalForPaint(
      paintMonotonicMicros: paint['paintMonotonicMicros']! as int,
      frameStampMicros: stamp,
      frames: frames,
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
      'measurementSessionIdentity': paint['canaryId'],
      'timestampMicros': paint['paintEpochMicros']! as int,
      'paintMonotonicMicros': paint['paintMonotonicMicros']! as int,
      'paintEpochBeforeMicros': paint['paintEpochBeforeMicros']! as int,
      'paintEpochAfterMicros': paint['paintEpochAfterMicros']! as int,
      'frameStampMicros': stamp,
      'frameOrdinal': frameOrdinal,
      'sourceGeneration': generation,
      'visibleUtf16Start': visibleStart,
      'visibleUtf16Length': visibleLength,
      'completeVisibleSurface': paint['completeVisibleSurface'] == true,
      'completeVisiblePlusOverscanSurface':
          paint['completeVisiblePlusOverscanSurface'] == true,
      'requiredVisibleFragmentCount':
          paint['requiredVisibleFragmentCount']! as int,
      'laidOutVisiblePlusOverscanFragmentCount':
          paint['laidOutVisiblePlusOverscanFragmentCount']! as int,
      'requiredVisibleFragments': paint['requiredVisibleFragments']! as List,
      'laidOutVisiblePlusOverscanFragments':
          paint['laidOutVisiblePlusOverscanFragments']! as List,
      'paintedFragments': paint['paintedFragments']! as List,
      'paintedRowCount': paint['paintedRowCount']! as int,
      'paintedSourceUtf16Start': paint['paintedSourceUtf16Start'] as int?,
      'paintedSourceUtf16End': paint['paintedSourceUtf16End'] as int?,
      'visiblePlusOverscanUtf16Start':
          paint['visiblePlusOverscanUtf16Start'] as int?,
      'visiblePlusOverscanUtf16End':
          paint['visiblePlusOverscanUtf16End'] as int?,
      'visiblePlusOverscanSourceSha256':
          paint['visiblePlusOverscanSourceSha256'] as String?,
      'expectedVisiblePlusOverscanSourceSha256':
          _expectedVisiblePlusOverscanSourceSha256(
            expectation.source,
            paint['visiblePlusOverscanUtf16Start'] as int?,
            paint['visiblePlusOverscanUtf16End'] as int?,
          ),
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
    final accepted = input['acceptedMicros']! as int;
    // A burst may accept a newer exact generation before Flutter has another
    // frame opportunity. Attribute every callback to the first following real
    // frame independently of whether that generation itself painted; using a
    // paint join here could hide synchronous work when Flutter legitimately
    // coalesces an intermediate publication.
    final frameOrdinal = _firstFrameCoveringAcceptance(frames, accepted);
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
      final frameOrdinal = _frameOrdinalForPaint(
        paintMonotonicMicros: rawPaint['paintMonotonicMicros']! as int,
        frameStampMicros: rawPaint['frameStampMicros']! as int,
        frames: frames,
        framePeriodMicros: framePeriodMicros,
      );
      if (frameOrdinal == null) continue;
      final visibleStart = rawPaint['visibleUtf16Start']! as int;
      final visibleLength = rawPaint['visibleUtf16Length']! as int;
      candidates.add({
        'measurementSessionIdentity': rawPaint['canaryId'],
        'timestampMicros': rawPaint['paintEpochMicros'],
        'paintMonotonicMicros': rawPaint['paintMonotonicMicros'],
        'paintEpochBeforeMicros': rawPaint['paintEpochBeforeMicros'],
        'paintEpochAfterMicros': rawPaint['paintEpochAfterMicros'],
        'frameOrdinal': frameOrdinal,
        'visibleSourceSha256': rawPaint['visibleSourceSha256'],
        'expectedVisibleSourceSha256': _sha(
          _utf16Slice(snapshot.source, visibleStart, visibleLength),
        ),
        'visibleUtf16Start': visibleStart,
        'visibleUtf16Length': visibleLength,
        'completeVisibleSurface': rawPaint['completeVisibleSurface'],
        'completeVisiblePlusOverscanSurface':
            rawPaint['completeVisiblePlusOverscanSurface'],
        'requiredVisibleFragmentCount':
            rawPaint['requiredVisibleFragmentCount'],
        'laidOutVisiblePlusOverscanFragmentCount':
            rawPaint['laidOutVisiblePlusOverscanFragmentCount'],
        'requiredVisibleFragments': rawPaint['requiredVisibleFragments'],
        'laidOutVisiblePlusOverscanFragments':
            rawPaint['laidOutVisiblePlusOverscanFragments'],
        'paintedFragments': rawPaint['paintedFragments'],
        'paintedRowCount': rawPaint['paintedRowCount'],
        'paintedSourceUtf16Start': rawPaint['paintedSourceUtf16Start'],
        'paintedSourceUtf16End': rawPaint['paintedSourceUtf16End'],
        'visiblePlusOverscanUtf16Start':
            rawPaint['visiblePlusOverscanUtf16Start'],
        'visiblePlusOverscanUtf16End': rawPaint['visiblePlusOverscanUtf16End'],
        'visiblePlusOverscanSourceSha256':
            rawPaint['visiblePlusOverscanSourceSha256'],
        'expectedVisiblePlusOverscanSourceSha256':
            _expectedVisiblePlusOverscanSourceSha256(
              snapshot.source,
              rawPaint['visiblePlusOverscanUtf16Start'] as int?,
              rawPaint['visiblePlusOverscanUtf16End'] as int?,
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
      'measurementSessionIdentity': paint['measurementSessionIdentity'],
      'acceptedMicros': openAcceptedMicros,
      'paintMicros': paint['timestampMicros'],
      'paintMonotonicMicros': paint['paintMonotonicMicros'],
      'paintEpochBeforeMicros': paint['paintEpochBeforeMicros'],
      'paintEpochAfterMicros': paint['paintEpochAfterMicros'],
      'openToEditableMicros':
          (paint['timestampMicros']! as int) - openAcceptedMicros,
      'sourceGeneration': 0,
      'sourceSha256': _sha(snapshot.source),
      'frameOrdinal': paint['frameOrdinal'],
      'visibleSourceSha256': paint['visibleSourceSha256'],
      'expectedVisibleSourceSha256': paint['expectedVisibleSourceSha256'],
      'visibleUtf16Start': paint['visibleUtf16Start'],
      'visibleUtf16Length': paint['visibleUtf16Length'],
      'completeVisibleSurface': paint['completeVisibleSurface'],
      'completeVisiblePlusOverscanSurface':
          paint['completeVisiblePlusOverscanSurface'],
      'requiredVisibleFragmentCount': paint['requiredVisibleFragmentCount'],
      'laidOutVisiblePlusOverscanFragmentCount':
          paint['laidOutVisiblePlusOverscanFragmentCount'],
      'requiredVisibleFragments': paint['requiredVisibleFragments'],
      'laidOutVisiblePlusOverscanFragments':
          paint['laidOutVisiblePlusOverscanFragments'],
      'paintedFragments': paint['paintedFragments'],
      'paintedRowCount': paint['paintedRowCount'],
      'paintedSourceUtf16Start': paint['paintedSourceUtf16Start'],
      'paintedSourceUtf16End': paint['paintedSourceUtf16End'],
      'visiblePlusOverscanUtf16Start': paint['visiblePlusOverscanUtf16Start'],
      'visiblePlusOverscanUtf16End': paint['visiblePlusOverscanUtf16End'],
      'visiblePlusOverscanSourceSha256':
          paint['visiblePlusOverscanSourceSha256'],
      'expectedVisiblePlusOverscanSourceSha256':
          paint['expectedVisiblePlusOverscanSourceSha256'],
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
    final declaredInputs = declared
        .map(
          (generation) => inputObservations.firstWhere(
            (input) => input['sourceGeneration'] == generation,
          ),
        )
        .toList(growable: false);
    final scheduleAcceptedMicros =
        declaredInputs.first['acceptedMicros']! as int;
    final measuredGeneration = expectation.measuredGeneration;
    final measuredInput = declaredInputs.firstWhere(
      (input) => input['sourceGeneration'] == measuredGeneration.generation,
    );
    final accepted = measuredInput['acceptedMicros']! as int;
    final engineMicros =
        engineObservations.firstWhere(
              (engine) =>
                  engine['sourceGeneration'] == measuredGeneration.generation,
            )['nativeFfiMicros']!
            as int;
    if (operation < denominator.warmups) {
      warmups.add({
        'index': operation,
        'sessionOrdinal': sessionOrdinal,
        'acceptedMicros': accepted,
        'scheduleAcceptedMicros': scheduleAcceptedMicros,
        'acceptedSourceGenerations': declared,
        'sourceGeneration': measuredGeneration.generation,
        'sourceSha256': _sha(measuredGeneration.source),
        'canonicalSelectionBaseUtf16': measuredGeneration.selectionBase,
        'canonicalSelectionExtentUtf16': measuredGeneration.selectionExtent,
        'engineMicros': engineMicros,
      });
      continue;
    }
    var summaryPaints = paintObservations
        .where(
          (paint) => paint['sourceGeneration'] == measuredGeneration.generation,
        )
        .toList();
    var paintedGeneration = measuredGeneration;
    var paintedAcceptedMicros = accepted;
    var visibilityDisposition = 'painted';
    int? supersededBySourceGeneration;
    if (summaryPaints.isEmpty) {
      final nextExpectation = operation + 1 < expected.length
          ? expected[operation + 1].measuredGeneration
          : null;
      final nextInputs = nextExpectation == null
          ? const <Map<String, Object?>>[]
          : inputObservations
                .where(
                  (input) =>
                      input['sourceGeneration'] == nextExpectation.generation,
                )
                .toList();
      final nextInput = nextInputs.isEmpty ? null : nextInputs.first;
      final firstFrameOrdinal = _firstFrameCoveringAcceptance(frames, accepted);
      final firstFrameBuildStart = _frameBuildStartEpochMicros(
        frames[firstFrameOrdinal],
      );
      final nextAccepted = nextInput?['acceptedMicros'];
      final nextPaints = nextExpectation == null
          ? const <Map<String, Object?>>[]
          : paintObservations
                .where(
                  (paint) =>
                      paint['sourceGeneration'] == nextExpectation.generation,
                )
                .toList();
      final provingNextPaints = nextPaints.where(
        (paint) => paint['frameOrdinal'] == firstFrameOrdinal,
      );
      if (nextExpectation != null &&
          nextAccepted is int &&
          nextAccepted >= accepted &&
          nextAccepted < firstFrameBuildStart &&
          provingNextPaints.isNotEmpty) {
        visibilityDisposition = 'superseded-before-frame';
        supersededBySourceGeneration = nextExpectation.generation;
        paintedGeneration = nextExpectation;
        paintedAcceptedMicros = nextAccepted;
        summaryPaints = nextPaints;
      }
    }
    if (summaryPaints.isEmpty) {
      final nearby = paintObservations
          .where((paint) {
            final generation = paint['sourceGeneration'];
            return generation is int &&
                (generation - measuredGeneration.generation).abs() <= 3;
          })
          .map(
            (paint) =>
                '${paint['sourceGeneration']}@${paint['timestampMicros']}'
                '/f${paint['frameOrdinal']}',
          )
          .toList(growable: false);
      final nearbyInputs = inputObservations
          .where((input) {
            final generation = input['sourceGeneration']! as int;
            return (generation - measuredGeneration.generation).abs() <= 3;
          })
          .map((input) {
            final generation = input['sourceGeneration']! as int;
            final performance = performanceByGeneration[generation]!;
            return '$generation@${input['acceptedMicros']}'
                '/sync${input['editorSyncMicros']}'
                '/ffi${performance['nativeFfiMicros']}'
                '/worker${performance['workerRoundTripMicros']}'
                '/acceptToReceipt${performance['acceptanceToReceiptMicros'] ?? performance['callbackToReceiptMicros']}';
          })
          .toList(growable: false);
      final nearbyFrames = frames
          .where((frame) {
            final vsync = frame['vsyncMicros']! as int;
            return vsync >= accepted - 50000 && vsync <= accepted + 70000;
          })
          .map(
            (frame) =>
                'f${frame['ordinal']}@${frame['vsyncMicros']}'
                '/b${frame['buildMicros']}/r${frame['rasterMicros']}'
                '/missed=${frame['missed']}',
          )
          .toList(growable: false);
      throw StateError(
        'generation ${measuredGeneration.generation} never painted; '
        'accepted=$accepted nearbyInputs=$nearbyInputs nearbyPaints=$nearby '
        'nearbyFrames=$nearbyFrames',
      );
    }
    final firstPaint = summaryPaints.first;
    final provingPaint = summaryPaints.cast<Map<String, Object?>>().firstWhere(
      (paint) => paint['frameOrdinal'] is int,
      orElse: () {
        final distances =
            summaryPaints
                .map((paint) => paint['_nearestFrameDistanceMicros'])
                .whereType<int>()
                .toList()
              ..sort();
        throw StateError(
          'generation ${paintedGeneration.generation} has no proving '
          'FrameTiming; nearest=${distances.isEmpty ? 'none' : distances.first}',
        );
      },
    );
    final provingFrameOrdinal = provingPaint['frameOrdinal']! as int;
    final currentPaint = summaryPaints.cast<Map<String, Object?>>().where(
      (paint) => paint['semanticsCurrent'] == true,
    );
    final sample = <String, Object?>{
      'index': operation - denominator.warmups,
      'sessionOrdinal': sessionOrdinal,
      'visibilityDisposition': visibilityDisposition,
      'supersededBySourceGeneration': supersededBySourceGeneration,
      'scheduledMicros': expectation.scheduledMicros,
      'acceptedMicros': accepted,
      'scheduleAcceptedMicros': scheduleAcceptedMicros,
      'sourcePaintMicros': firstPaint['timestampMicros'],
      'caretPaintMicros': firstPaint['timestampMicros'],
      'selectionPaintMicros': firstPaint['timestampMicros'],
      'acceptedSourceGenerations': declared,
      'sourceGeneration': measuredGeneration.generation,
      'paintedSourceGeneration': firstPaint['sourceGeneration'],
      'sourceSha256': _sha(measuredGeneration.source),
      'visibleSourceSha256': firstPaint['visibleSourceSha256'],
      'canonicalSelectionBaseUtf16': measuredGeneration.selectionBase,
      'canonicalSelectionExtentUtf16': measuredGeneration.selectionExtent,
      'paintedCaretSourceUtf16': firstPaint['caretSourceUtf16'],
      'startFrameOrdinal': _firstFrameCoveringAcceptance(frames, accepted),
      'endFrameOrdinal': summaryPaints
          .where((paint) => paint['frameOrdinal'] is int)
          .map((paint) => paint['frameOrdinal']! as int)
          .reduce(math.max),
      'provingFrameOrdinal': provingFrameOrdinal,
      'engineMicros': engineMicros,
      'visibleCertificationMicros': currentPaint.isEmpty
          ? 0
          : (currentPaint.first['timestampMicros']! as int) -
                paintedAcceptedMicros,
      'rawProjectionFrames': summaryPaints
          .where((paint) => (paint['activeNeutralRowCount']! as int) > 0)
          .length,
      'sourceIdentityMatched': summaryPaints.every(
        (paint) =>
            paint['visibleSourceSha256'] ==
            paint['expectedVisibleSourceSha256'],
      ),
      'caretIdentityMatched': summaryPaints.every(
        (paint) => paint['activeRowVisible'] == true
            ? paint['caretSourceUtf16'] == paintedGeneration.selectionExtent &&
                  paint['caretDisplayUtf16'] != null
            : paint['caretSourceUtf16'] == null &&
                  paint['caretDisplayUtf16'] == null,
      ),
      'selectionIdentityMatched': summaryPaints.every(
        (paint) =>
            paint['canonicalSelectionBaseUtf16'] ==
                paintedGeneration.selectionBase &&
            paint['canonicalSelectionExtentUtf16'] ==
                paintedGeneration.selectionExtent,
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
    'faulted': settled.faulted,
    'resyncCount': settled.resyncCount,
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

String? _expectedVisiblePlusOverscanSourceSha256(
  String source,
  int? start,
  int? end,
) {
  if (start == null || end == null || start < 0 || end <= start) return null;
  if (end > source.length) return null;
  return _sha(source.substring(start, end));
}

int _firstFrameCoveringAcceptance(
  List<Map<String, Object?>> frames,
  int acceptedMicros,
) {
  final candidates = frames.where(
    (frame) => _frameBuildStartEpochMicros(frame) >= acceptedMicros,
  );
  if (candidates.isEmpty) {
    throw StateError('accepted input has no following FrameTiming interval');
  }
  return candidates.first['ordinal']! as int;
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

Map<String, Object?> _inputObservation(
  _ExpectedGeneration expectation, {
  required int sessionOrdinal,
  required String measurementSessionIdentity,
  required int acceptedMicros,
  required int editorSyncMicros,
  Map<String, Object?>? operationTiming,
}) => {
  'sessionOrdinal': sessionOrdinal,
  'measurementSessionIdentity': measurementSessionIdentity,
  'sourceGeneration': expectation.generation,
  'acceptedMicros': acceptedMicros,
  'editorSyncMicros': editorSyncMicros,
  'sourceSha256': _sha(expectation.source),
  'canonicalSelectionBaseUtf16': expectation.selectionBase,
  'canonicalSelectionExtentUtf16': expectation.selectionExtent,
  if (operationTiming != null)
    'operationTimingKind': operationTiming['operationTimingKind'],
  if (operationTiming != null)
    'operationTimingEvent': operationTiming['operationTimingEvent'],
};

Map<int, Map<String, Object?>> _operationTimingsByGeneration(
  List<String> inputEvents,
) {
  final result = <int, Map<String, Object?>>{};
  final pastePattern = RegExp(
    r'^\d+:completed-paste:generation=(\d+)'
    r':acceptedAtEpochMicros=(\d+):elapsedMicros=(\d+)$',
  );
  for (final event in inputEvents) {
    final match = pastePattern.firstMatch(event);
    if (match == null) continue;
    final generation = int.parse(match.group(1)!);
    if (result.containsKey(generation)) {
      throw StateError(
        'generation $generation has duplicate platform operation timings',
      );
    }
    result[generation] = {
      'operationTimingKind': 'platform-paste',
      'operationTimingEvent': event,
      '_acceptedMicros': int.parse(match.group(2)!),
      '_editorSyncMicros': int.parse(match.group(3)!),
    };
  }
  return result;
}

String _utf16Slice(String source, int start, int length) {
  final units = source.codeUnits;
  final end = start + length;
  if (start < 0 || end < start || end > units.length) {
    throw RangeError.range(end, start, units.length, 'visible UTF-16 end');
  }
  return String.fromCharCodes(units.sublist(start, end));
}

String _sha(String source) => sha256.convert(utf8.encode(source)).toString();

int? _frameOrdinalForPaint({
  required int paintMonotonicMicros,
  required int frameStampMicros,
  required List<Map<String, Object?>> frames,
  required int framePeriodMicros,
}) {
  final containing = frames
      .where((frame) {
        final start = frame['buildStartMonotonicMicros'];
        final finish = frame['buildFinishMonotonicMicros'];
        return start is int &&
            finish is int &&
            paintMonotonicMicros >= start &&
            paintMonotonicMicros <= finish;
      })
      .toList(growable: false);
  // currentSystemFrameTimeStamp is the engine's nominal target time. Keep it
  // as a bounded independent guard and, if synthetic/test intervals overlap,
  // as the disambiguator. Every candidate must still contain the exact paint
  // clock; nominal-period arithmetic is never sufficient by itself.
  final ranked = <({int distance, int ordinal})>[];
  for (final frame in containing) {
    final vsync = frame['monotonicVsyncMicros'];
    final ordinal = frame['ordinal'];
    if (vsync is! int || ordinal is! int) continue;
    final distance = (vsync + framePeriodMicros - frameStampMicros).abs();
    if (distance <= framePeriodMicros ~/ 8) {
      ranked.add((distance: distance, ordinal: ordinal));
    }
  }
  ranked.sort((left, right) => left.distance.compareTo(right.distance));
  if (ranked.isEmpty ||
      (ranked.length > 1 && ranked[0].distance == ranked[1].distance)) {
    return null;
  }
  return ranked.first.ordinal;
}
