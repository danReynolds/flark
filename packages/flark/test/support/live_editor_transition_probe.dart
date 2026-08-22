import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

final class MarkedSource {
  const MarkedSource(this.source, this.caret);

  factory MarkedSource.parse(String marked) {
    const marker = '¦';
    final caret = marked.indexOf(marker);
    if (caret < 0 || marked.indexOf(marker, caret + 1) >= 0) {
      throw ArgumentError.value(marked, 'marked', 'expected exactly one ¦');
    }
    return MarkedSource(marked.replaceFirst(marker, ''), caret);
  }

  final String source;
  final int caret;
}

typedef SurfaceRunSample = ({
  String text,
  int sourceStart,
  int sourceEnd,
  bool sourceExact,
  Set<String> styles,
});

SurfaceRunSample _captureRun(FlarkSurfaceTextRun run) => (
  text: run.text,
  sourceStart: run.sourceUtf16Start,
  sourceEnd: run.sourceUtf16End,
  sourceExact: run.sourceExact,
  styles: run.styles.map((style) => style.name).toSet(),
);

typedef SurfaceRowSample = ({
  int ordinal,
  int kind,
  int? headingLevel,
  int? quoteDepth,
  String leadingText,
  String text,
  List<SurfaceRunSample> runs,
});

SurfaceRowSample _captureRow(FlarkSurfaceRow row) => (
  ordinal: row.ordinal,
  kind: row.kind,
  headingLevel: row.headingLevel,
  quoteDepth: row.blockQuoteDepth,
  leadingText: row.leadingText,
  text: row.text,
  runs: row.runs.map(_captureRun).toList(growable: false),
);

String _rowManifest(SurfaceRowSample row) => <Object?>[
  row.ordinal,
  row.kind,
  row.headingLevel,
  row.quoteDepth,
  row.leadingText,
  row.text,
  row.runs
      .map(
        (run) =>
            '${run.sourceStart}:${run.sourceEnd}:'
            '${run.sourceExact ? 'exact' : 'mapped'}:'
            '${run.styles.toList()..sort()}:${run.text}',
      )
      .join('|'),
].join('~');

/// One synchronous state made observable by the controller.
/// This deliberately contains no asynchronous source read: such a read waits
/// for the mutation tail and would associate a later source with this sample.
final class PublicationSample {
  const PublicationSample({
    required this.sequence,
    required this.revision,
    required this.status,
    required this.pendingEdits,
    required this.semanticsCurrent,
    required this.visibleSource,
    required this.visibleStart,
    required this.sourceUtf16Length,
    required this.inputGlobalStart,
    required this.inputValue,
    required this.globalSelectionBase,
    required this.globalSelectionExtent,
    required this.hasOversizedSelection,
    required this.resyncCount,
    required this.lastError,
    required this.rows,
  });

  factory PublicationSample.capture(
    FlarkEditorController controller,
    int sequence,
  ) => PublicationSample(
    sequence: sequence,
    revision: controller.revision,
    status: controller.status,
    pendingEdits: controller.pendingEdits,
    semanticsCurrent: controller.semanticsCurrent,
    visibleSource: controller.visibleSource,
    visibleStart: controller.visibleUtf16Start,
    sourceUtf16Length: controller.sourceUtf16Length,
    inputGlobalStart: controller.inputWindowShadow.globalUtf16Start,
    inputValue: controller.inputValue,
    globalSelectionBase: controller.globalSelectionBase,
    globalSelectionExtent: controller.globalSelectionExtent,
    hasOversizedSelection: controller.hasOversizedSelection,
    resyncCount: controller.resyncCount,
    lastError: controller.lastError,
    rows: controller.rows
        .map(controller.surfaceRow)
        .map(_captureRow)
        .toList(growable: false),
  );

  final int sequence;
  final int revision;
  final FlarkEditorStatus status;
  final int pendingEdits;
  final bool semanticsCurrent;
  final String visibleSource;
  final int visibleStart;
  final int sourceUtf16Length;
  final int inputGlobalStart;
  final TextEditingValue inputValue;
  final int globalSelectionBase;
  final int globalSelectionExtent;
  final bool hasOversizedSelection;
  final int resyncCount;
  final Object? lastError;
  final List<SurfaceRowSample> rows;

  String get presentation =>
      rows.map((row) => '${row.leadingText}${row.text}').join('\n');

  void expectMechanicallyValid() {
    // This validator also runs synchronously from a ChangeNotifier listener.
    // `expectSync` preserves immediate failure without entering Flutter's
    // guarded async API while a pump is delivering a native completion.
    expectSync(pendingEdits, greaterThanOrEqualTo(0));
    expectSync(status, isNot(FlarkEditorStatus.faulted));
    expectSync(lastError, isNull);
    expectSync(resyncCount, 0);
    final projectedUtf16Length = <int>[
      sourceUtf16Length,
      visibleStart + visibleSource.length,
      inputGlobalStart + inputValue.text.length,
    ].reduce((left, right) => left > right ? left : right);
    expectSync(visibleStart, inInclusiveRange(0, projectedUtf16Length));
    expectSync(
      visibleStart + visibleSource.length,
      lessThanOrEqualTo(projectedUtf16Length),
    );
    expectSync(globalSelectionBase, inInclusiveRange(0, projectedUtf16Length));
    expectSync(
      globalSelectionExtent,
      inInclusiveRange(0, projectedUtf16Length),
    );
    final selection = inputValue.selection;
    expectSync(selection.isValid, isTrue);
    expectSync(selection.start, inInclusiveRange(0, inputValue.text.length));
    expectSync(selection.end, inInclusiveRange(0, inputValue.text.length));
    if (!hasOversizedSelection) {
      expectSync(
        inputGlobalStart + selection.baseOffset,
        globalSelectionBase,
        reason: 'the platform input base must represent the canonical base',
      );
      expectSync(
        inputGlobalStart + selection.extentOffset,
        globalSelectionExtent,
        reason: 'the platform input extent must represent the canonical extent',
      );
    }
    final composing = inputValue.composing;
    if (composing != TextRange.empty) {
      expectSync(composing.isValid, isTrue);
      expectSync(composing.start, inInclusiveRange(0, inputValue.text.length));
      expectSync(composing.end, inInclusiveRange(0, inputValue.text.length));
    }
    for (final run in rows.expand((row) => row.runs)) {
      expectSync(run.sourceStart, lessThanOrEqualTo(run.sourceEnd));
      expectSync(run.sourceStart, greaterThanOrEqualTo(visibleStart));
      expectSync(
        run.sourceEnd,
        lessThanOrEqualTo(visibleStart + visibleSource.length),
      );
      if (!run.sourceExact) continue;
      final localStart = run.sourceStart - visibleStart;
      final localEnd = run.sourceEnd - visibleStart;
      expectSync(
        visibleSource.substring(localStart, localEnd),
        run.text,
        reason: 'exact run disagrees with its represented source',
      );
    }
  }
}

final class ActionTrace {
  const ActionTrace({
    required this.before,
    required this.publications,
    required this.callbackReturn,
  });

  final PublicationSample before;
  final List<PublicationSample> publications;
  final PublicationSample callbackReturn;

  Iterable<PublicationSample> get observableStates sync* {
    yield before;
    yield* publications;
    yield callbackReturn;
  }
}

final class LiveEditorTransitionProbe {
  LiveEditorTransitionProbe._(
    this.controller,
    this.libraryPath,
    this._platformValue,
  ) {
    controller.addListener(_recordPublication);
    _recordPublication();
  }

  static Future<LiveEditorTransitionProbe> open(
    String marked, {
    String? libraryPath,
  }) async {
    final parsed = MarkedSource.parse(marked);
    final resolvedLibrary =
        libraryPath ?? Platform.environment['FLARK_V4_LIBRARY_PATH']!;
    final controller = await FlarkEditorController.open(
      parsed.source,
      libraryPath: resolvedLibrary,
    );
    await controller.continueParsing();
    final row = controller.rows.firstWhere(
      (candidate) =>
          candidate.editableUtf16 != null &&
          candidate.editableUtf16!.start <= parsed.caret &&
          parsed.caret <= candidate.editableUtf16!.end,
      orElse: () => throw StateError(
        'caret ${parsed.caret} is not in an editable certified row',
      ),
    );
    controller.activateRow(row, parsed.caret);
    return LiveEditorTransitionProbe._(
      controller,
      resolvedLibrary,
      controller.inputValue,
    );
  }

  final FlarkEditorController controller;
  final String libraryPath;
  TextEditingValue _platformValue;
  final List<PublicationSample> publications = [];
  int _sampleSequence = 0;

  void _recordPublication() {
    final sample = PublicationSample.capture(controller, _sampleSequence++);
    sample.expectMechanicallyValid();
    publications.add(sample);
  }

  ActionTrace observe(void Function() action) {
    final before = PublicationSample.capture(controller, _sampleSequence++);
    before.expectMechanicallyValid();
    final publicationStart = publications.length;
    action();
    final callbackReturn = PublicationSample.capture(
      controller,
      _sampleSequence++,
    );
    callbackReturn.expectMechanicallyValid();
    return ActionTrace(
      before: before,
      publications: List.unmodifiable(publications.skip(publicationStart)),
      callbackReturn: callbackReturn,
    );
  }

  Future<ActionTrace> observeAsync(Future<void> Function() action) async {
    final before = PublicationSample.capture(controller, _sampleSequence++);
    before.expectMechanicallyValid();
    final publicationStart = publications.length;
    await action();
    final callbackReturn = PublicationSample.capture(
      controller,
      _sampleSequence++,
    );
    callbackReturn.expectMechanicallyValid();
    return ActionTrace(
      before: before,
      publications: List.unmodifiable(publications.skip(publicationStart)),
      callbackReturn: callbackReturn,
    );
  }

  List<ActionTrace> typeText(String text) {
    final traces = <ActionTrace>[];
    for (final rune in text.runes) {
      final character = String.fromCharCode(rune);
      final before = _platformValue;
      final selection = before.selection;
      final delta = selection.isCollapsed
          ? TextEditingDeltaInsertion(
              oldText: before.text,
              textInserted: character,
              insertionOffset: selection.start,
              selection: TextSelection.collapsed(
                offset: selection.start + character.length,
              ),
              composing: TextRange.empty,
            )
          : TextEditingDeltaReplacement(
              oldText: before.text,
              replacementText: character,
              replacedRange: TextRange(
                start: selection.start,
                end: selection.end,
              ),
              selection: TextSelection.collapsed(
                offset: selection.start + character.length,
              ),
              composing: TextRange.empty,
            );
      traces.add(observe(() => controller.applyDeltas([delta])));
      _platformValue = delta.apply(before);
    }
    return traces;
  }

  ActionTrace pressReturn() {
    final before = _platformValue;
    final selection = before.selection;
    final delta = selection.isCollapsed
        ? TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: '\n',
            insertionOffset: selection.start,
            selection: TextSelection.collapsed(offset: selection.start + 1),
            composing: TextRange.empty,
          )
        : TextEditingDeltaReplacement(
            oldText: before.text,
            replacementText: '\n',
            replacedRange: TextRange(
              start: selection.start,
              end: selection.end,
            ),
            selection: TextSelection.collapsed(offset: selection.start + 1),
            composing: TextRange.empty,
          );
    final trace = observe(() {
      controller.applyDeltas([delta]);
      controller.observePlatformNewlineAction();
    });
    _platformValue = delta.apply(before);
    return trace;
  }

  ActionTrace pressBackspace() {
    final trace = observe(controller.deleteBackward);
    _platformValue = controller.inputValue;
    return trace;
  }

  ActionTrace pressDelete() {
    final trace = observe(controller.deleteForward);
    _platformValue = controller.inputValue;
    return trace;
  }

  ActionTrace replaceSelection(String replacement) {
    final before = _platformValue;
    final selection = before.selection;
    final delta = selection.isCollapsed
        ? TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: replacement,
            insertionOffset: selection.start,
            selection: TextSelection.collapsed(
              offset: selection.start + replacement.length,
            ),
            composing: TextRange.empty,
          )
        : TextEditingDeltaReplacement(
            oldText: before.text,
            replacementText: replacement,
            replacedRange: TextRange(
              start: selection.start,
              end: selection.end,
            ),
            selection: TextSelection.collapsed(
              offset: selection.start + replacement.length,
            ),
            composing: TextRange.empty,
          );
    final trace = observe(() => controller.applyDeltas([delta]));
    _platformValue = delta.apply(before);
    return trace;
  }

  Future<ActionTrace> undo() async {
    final trace = await observeAsync(() async {
      if (!await controller.undo()) {
        throw StateError('undo was unavailable');
      }
    });
    _platformValue = controller.inputValue;
    return trace;
  }

  Future<ActionTrace> redo() async {
    final trace = await observeAsync(() async {
      if (!await controller.redo()) {
        throw StateError('redo was unavailable');
      }
    });
    _platformValue = controller.inputValue;
    return trace;
  }

  void moveCaret(int globalUtf16Offset) {
    final row = controller.rows.firstWhere(
      (candidate) {
        final editable = candidate.editableUtf16;
        return editable != null &&
            editable.start <= globalUtf16Offset &&
            globalUtf16Offset <= editable.end;
      },
      orElse: () => throw StateError(
        'caret $globalUtf16Offset is not in an editable certified row',
      ),
    );
    controller.activateRow(row, globalUtf16Offset);
    _platformValue = controller.inputValue;
  }

  Future<void> selectRange(int baseUtf16, int extentUtf16) async {
    final start = baseUtf16 < extentUtf16 ? baseUtf16 : extentUtf16;
    final end = baseUtf16 < extentUtf16 ? extentUtf16 : baseUtf16;
    final row = controller.rows.firstWhere(
      (candidate) {
        final editable = candidate.editableUtf16;
        return editable != null &&
            editable.start <= start &&
            end <= editable.end;
      },
      orElse: () => throw StateError(
        'selection $baseUtf16..$extentUtf16 is not in one editable row',
      ),
    );
    controller.activateRow(row, baseUtf16, selectionExtent: extentUtf16);
    _platformValue = controller.inputValue;
    await controller.debugWaitForMutationSettled();
  }

  Future<PublicationSample> presentationSettled() async {
    await controller.debugWaitForPresentationSettled();
    _platformValue = controller.inputValue;
    final sample = PublicationSample.capture(controller, _sampleSequence++);
    sample.expectMechanicallyValid();
    return sample;
  }

  Future<void> expectSourceAndCaret(String marked) async {
    final expected = MarkedSource.parse(marked);
    await controller.debugWaitForMutationSettled();
    expect(await controller.readSource(), expected.source);
    final selection = await controller.resolveCanonicalSelection();
    expect(selection?.base, expected.caret);
    expect(selection?.extent, expected.caret);
  }

  Future<void> expectHealthy() async {
    await controller.debugWaitForMutationSettled();
    expect(controller.status, isNot(FlarkEditorStatus.faulted));
    expect(controller.lastError, isNull);
    expect(controller.resyncCount, 0);
  }

  Future<void> expectConvergesWithCleanRebuild() async {
    await controller.debugWaitForPresentationSettled();
    final source = await controller.readSource();
    final fresh = await FlarkEditorController.open(
      source,
      libraryPath: libraryPath,
    );
    try {
      await fresh.continueParsing();
      expect(_semanticManifest(controller), _semanticManifest(fresh));
    } finally {
      await fresh.close();
    }
  }

  Future<void> close() async {
    controller.removeListener(_recordPublication);
    await controller.close();
  }
}

String _semanticManifest(FlarkEditorController controller) => controller.rows
    .map((row) => controller.surfaceRow(row, includeEditingState: false))
    .map(_captureRow)
    .map(_rowManifest)
    .join('\n');

final class MountedTransitionRecorder {
  MountedTransitionRecorder._(this.tester, this.probe);

  static Future<MountedTransitionRecorder> mount(
    WidgetTester tester,
    LiveEditorTransitionProbe probe, {
    Size size = const Size(420, 600),
    TextDirection textDirection = TextDirection.ltr,
  }) async {
    await tester.binding.setSurfaceSize(size);
    final recorder = MountedTransitionRecorder._(tester, probe);
    await tester.pumpWidget(
      Directionality(
        textDirection: textDirection,
        child: SizedBox.expand(
          child: FlarkRenderSurfaceWidget(
            controller: probe.controller,
            textStyle: const TextStyle(fontSize: 17, height: 1.45),
            padding: EdgeInsets.zero,
            caretColor: const Color(0xff246bfd),
            selectionColor: const Color(0x40246bfd),
            debugPaintObserver: recorder._recordPaint,
          ),
        ),
      ),
    );
    await tester.pump();
    recorder.paints.clear();
    return recorder;
  }

  final WidgetTester tester;
  final LiveEditorTransitionProbe probe;
  final List<FlarkSurfacePaintObservation> paints = [];

  void _recordPaint(FlarkSurfacePaintObservation paint) {
    if (paint.caretRect != null) {
      expect(
        paint.caretSourceUtf16,
        paint.canonicalSelectionExtentUtf16,
        reason:
            'every painted caret must represent the controller selection in '
            'the same frame',
      );
    }
    paints.add(paint);
  }

  Future<List<ActionTrace>> typeText(String text) async =>
      (await tester.runAsync(() async => probe.typeText(text)))!;

  Future<List<ActionTrace>> typeTextAndPumpEachCharacter(String text) async {
    final traces = <ActionTrace>[];
    for (final rune in text.runes) {
      traces.addAll(await typeText(String.fromCharCode(rune)));
      await pumpImmediate();
    }
    return traces;
  }

  Future<ActionTrace> pressReturn() async =>
      (await tester.runAsync(() async => probe.pressReturn()))!;

  Future<ActionTrace> pressBackspace() async =>
      (await tester.runAsync(() async => probe.pressBackspace()))!;

  Future<ActionTrace> pressDelete() async =>
      (await tester.runAsync(() async => probe.pressDelete()))!;

  Future<ActionTrace> replaceSelection(String replacement) async =>
      (await tester.runAsync(() async => probe.replaceSelection(replacement)))!;

  Future<ActionTrace> undo() async => (await tester.runAsync(probe.undo))!;

  Future<ActionTrace> redo() async => (await tester.runAsync(probe.redo))!;

  Future<void> moveCaret(int globalUtf16Offset) async {
    await tester.runAsync(() async => probe.moveCaret(globalUtf16Offset));
    await tester.pump();
  }

  Future<void> selectRange(int baseUtf16, int extentUtf16) async {
    await tester.runAsync(() => probe.selectRange(baseUtf16, extentUtf16));
    await tester.pump();
  }

  Future<void> pumpImmediate() => tester.pump();

  Future<void> pumpPresentationSettled() async {
    await tester.runAsync(probe.presentationSettled);
    await tester.pump();
  }

  Future<void> close() async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.binding.setSurfaceSize(null);
  }
}
