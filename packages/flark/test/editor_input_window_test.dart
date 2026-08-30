import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test('bounded activation preserves exact represented endpoints', () {
    final window = FlarkEditorInputWindowPlanner.activate(
      text: '01234😀567890abcdefghij',
      sourceStart: 100,
      caret: 109,
      selectionExtent: 112,
      ordinal: 7,
      affinity: FlarkTextAffinity.upstream,
      maximumCodeUnits: 8,
    );

    expect(window.selectionRepresented, isTrue);
    expect(window.text.length, lessThanOrEqualTo(8));
    expect(window.activeOrdinal, 7);
    expect(window.canonicalSelectionBaseUtf16, 109);
    expect(window.canonicalSelectionExtentUtf16, 112);
    expect(window.crossRowSelection, isTrue);
    expect(
      window.globalUtf16Start + window.selection.baseOffset,
      window.canonicalSelectionBaseUtf16,
    );
    expect(
      window.globalUtf16Start + window.selection.extentOffset,
      window.canonicalSelectionExtentUtf16,
    );
    expect(_startsWithLowSurrogate(window.text), isFalse);
    expect(_endsWithHighSurrogate(window.text), isFalse);
  });

  test('unrepresentable selection retains canonical endpoints', () {
    final window = FlarkEditorInputWindowPlanner.activate(
      text: '0123456789abcdefghij',
      sourceStart: 100,
      caret: 102,
      selectionExtent: 118,
      ordinal: 3,
      affinity: FlarkTextAffinity.downstream,
      maximumCodeUnits: 8,
    );

    expect(window.selectionRepresented, isFalse);
    expect(window.selection.isCollapsed, isTrue);
    expect(window.canonicalSelectionBaseUtf16, 102);
    expect(window.canonicalSelectionExtentUtf16, 118);
    expect(window.text.length, lessThanOrEqualTo(8));
  });

  test('collapsed window is scalar aligned and globally exact', () {
    final window = FlarkEditorInputWindowPlanner.collapsed(
      text: '01234😀567890abcdefghij',
      sourceStart: 40,
      caret: 51,
      ordinal: 4,
      maximumCodeUnits: 7,
    );

    expect(window.canonicalSelectionBaseUtf16, 51);
    expect(window.canonicalSelectionExtentUtf16, 51);
    expect(window.text.length, lessThanOrEqualTo(7));
    expect(_startsWithLowSurrogate(window.text), isFalse);
    expect(_endsWithHighSurrogate(window.text), isFalse);
  });

  test('window planning rejects nonpositive capacities', () {
    expect(
      () => FlarkEditorInputWindowPlanner.activate(
        text: 'a',
        sourceStart: 0,
        caret: 0,
        ordinal: 0,
        affinity: FlarkTextAffinity.downstream,
        maximumCodeUnits: 0,
      ),
      throwsArgumentError,
    );
    expect(
      () => FlarkEditorInputWindowPlanner.collapsed(
        text: 'a',
        sourceStart: 0,
        caret: 0,
        ordinal: 0,
        maximumCodeUnits: -1,
      ),
      throwsArgumentError,
    );
  });

  test('committed splice updates one bounded input window exactly', () {
    final window = FlarkEditorInputWindowPlanner.afterCommittedSplice(
      base: const FlarkEditorInputValue(
        text: 'abc',
        selection: FlarkTextSelection.collapsed(offset: 1),
      ),
      inputGlobalUtf16Start: 10,
      activeOrdinal: 4,
      startUtf16: 11,
      endUtf16: 11,
      replacement: 'xy',
      resultCaretUtf16: 13,
      maximumCodeUnits: 8,
    );

    expect(window, isNotNull);
    expect(window!.text, 'axybc');
    expect(window.globalUtf16Start, 10);
    expect(window.selection.extentOffset, 3);
    expect(window.canonicalSelectionExtentUtf16, 13);
    expect(window.activeOrdinal, 4);
  });

  test('committed splice before a window shifts its global origin', () {
    final window = FlarkEditorInputWindowPlanner.afterCommittedSplice(
      base: const FlarkEditorInputValue(
        text: 'abc',
        selection: FlarkTextSelection.collapsed(offset: 3),
      ),
      inputGlobalUtf16Start: 10,
      activeOrdinal: null,
      startUtf16: 5,
      endUtf16: 7,
      replacement: 'x',
      resultCaretUtf16: 12,
      maximumCodeUnits: 8,
    );

    expect(window, isNotNull);
    expect(window!.text, 'abc');
    expect(window.globalUtf16Start, 9);
    expect(window.selection.extentOffset, 3);
    expect(window.canonicalSelectionExtentUtf16, 12);
    expect(window.activeOrdinal, isNull);
  });

  test('partial or oversized committed window splices fail closed', () {
    const base = FlarkEditorInputValue(
      text: 'abc',
      selection: FlarkTextSelection.collapsed(offset: 1),
    );

    expect(
      FlarkEditorInputWindowPlanner.afterCommittedSplice(
        base: base,
        inputGlobalUtf16Start: 10,
        activeOrdinal: 0,
        startUtf16: 9,
        endUtf16: 11,
        replacement: '',
        resultCaretUtf16: 10,
        maximumCodeUnits: 8,
      ),
      isNull,
    );
    expect(
      FlarkEditorInputWindowPlanner.afterCommittedSplice(
        base: base,
        inputGlobalUtf16Start: 10,
        activeOrdinal: 0,
        startUtf16: 11,
        endUtf16: 11,
        replacement: 'long',
        resultCaretUtf16: 15,
        maximumCodeUnits: 4,
      ),
      isNull,
    );
  });

  test('input mutation plan owns exact global and bounded result state', () {
    final plan = FlarkEditorInputMutationPlanner.plan(
      input: const FlarkEditorInputValue(
        text: 'abc',
        selection: FlarkTextSelection.collapsed(offset: 1),
      ),
      inputGlobalUtf16Start: 10,
      activeOrdinal: 3,
      inlineContinuation: null,
      start: 1,
      end: 1,
      replacement: 'x',
      resultSelection: const FlarkTextSelection.collapsed(offset: 2),
      resultComposing: FlarkTextRange.empty,
      maximumCodeUnits: 8,
    );

    expect(plan, isNotNull);
    expect((plan!.globalStartUtf16, plan.globalEndUtf16), (11, 11));
    expect(plan.replacement, 'x');
    expect(plan.window.text, 'axbc');
    expect(plan.window.globalUtf16Start, 10);
    expect(plan.window.canonicalSelectionExtentUtf16, 12);
    expect(plan.window.activeOrdinal, 3);
    expect(plan.requiresStructuralCertification, isFalse);
    expect(plan.beginsPublicationBarrier, isFalse);
    expect(plan.compositionActive, isFalse);
  });

  test('input mutation plan rewrites one parser-authored continuation', () {
    const continuation = FlarkCoreInlineContinuationV1(
      revision: 4,
      caretUtf16: 0,
      prefix: '*',
      suffix: '*',
      collisionScalars: '*_',
      scalarPolicy:
          FlarkCoreInlineContinuationScalarPolicyV1.stableNonWhitespace,
    );

    final plan = FlarkEditorInputMutationPlanner.plan(
      input: const FlarkEditorInputValue(),
      inputGlobalUtf16Start: 0,
      activeOrdinal: 0,
      inlineContinuation: continuation,
      start: 0,
      end: 0,
      replacement: 'x',
      resultSelection: const FlarkTextSelection.collapsed(offset: 1),
      resultComposing: FlarkTextRange.empty,
      maximumCodeUnits: 8,
    );

    expect(plan, isNotNull);
    expect(plan!.replacement, '*x*');
    expect(plan.window.text, '*x*');
    expect(plan.window.selection.extentOffset, 2);
    expect(plan.beginsPublicationBarrier, isTrue);
    expect(plan.inlineContinuation?.revision, 5);
    expect(plan.inlineContinuation?.caretUtf16, 2);
    expect(plan.inlineContinuation?.ownerMaterialized, isTrue);
  });

  test('input mutation plan leaves the continuation on whitespace', () {
    const continuation = FlarkCoreInlineContinuationV1(
      revision: 4,
      caretUtf16: 0,
      prefix: '*',
      suffix: '*',
      collisionScalars: '*_',
      scalarPolicy:
          FlarkCoreInlineContinuationScalarPolicyV1.stableNonWhitespace,
    );

    final plan = FlarkEditorInputMutationPlanner.plan(
      input: const FlarkEditorInputValue(),
      inputGlobalUtf16Start: 0,
      activeOrdinal: 0,
      inlineContinuation: continuation,
      start: 0,
      end: 0,
      replacement: ' ',
      resultSelection: const FlarkTextSelection.collapsed(offset: 1),
      resultComposing: FlarkTextRange.empty,
      maximumCodeUnits: 8,
    );

    expect(plan, isNotNull);
    expect(plan!.replacement, ' ');
    expect(plan.window.text, ' ');
    expect(plan.inlineContinuation, isNull);
    expect(plan.beginsPublicationBarrier, isFalse);
  });

  test('input mutation plan rejects torn values and classifies line edits', () {
    const input = FlarkEditorInputValue(
      text: 'a\nb',
      selection: FlarkTextSelection.collapsed(offset: 1),
    );
    final structural = FlarkEditorInputMutationPlanner.plan(
      input: input,
      inputGlobalUtf16Start: 0,
      activeOrdinal: 0,
      inlineContinuation: null,
      start: 1,
      end: 2,
      replacement: '',
      resultSelection: const FlarkTextSelection.collapsed(offset: 1),
      resultComposing: FlarkTextRange.empty,
      maximumCodeUnits: 8,
    );

    expect(structural?.requiresStructuralCertification, isTrue);
    expect(
      FlarkEditorInputMutationPlanner.plan(
        input: input,
        inputGlobalUtf16Start: 0,
        activeOrdinal: 0,
        inlineContinuation: null,
        start: 0,
        end: 1,
        replacement: 'x',
        resultSelection: const FlarkTextSelection.collapsed(offset: 1),
        resultComposing: FlarkTextRange.empty,
        fullValue: const FlarkEditorInputValue(text: 'wrong'),
        maximumCodeUnits: 8,
      ),
      isNull,
    );
  });

  test('bounded input can drop a range without ending Core composition', () {
    final plan = FlarkEditorInputMutationPlanner.plan(
      input: const FlarkEditorInputValue(
        text: 'abcdefgh',
        selection: FlarkTextSelection.collapsed(offset: 4),
      ),
      inputGlobalUtf16Start: 0,
      activeOrdinal: 0,
      inlineContinuation: null,
      start: 4,
      end: 4,
      replacement: 'x',
      resultSelection: const FlarkTextSelection.collapsed(offset: 5),
      resultComposing: const FlarkTextRange(start: 0, end: 9),
      maximumCodeUnits: 4,
    );

    expect(plan, isNotNull);
    expect(plan!.compositionActive, isTrue);
    expect(plan.window.composing, FlarkTextRange.empty);
    expect(plan.window.text.length, lessThanOrEqualTo(4));
  });

  test('collapsed restoration selects a certified row without host policy', () {
    final viewportState = FlarkEditorViewportState()
      ..install(_viewport(rows: [_row()]), 'abc');
    final projector = _projector(viewportState);

    final window = FlarkEditorInputWindowPlanner.restoreCollapsed(
      viewportState: viewportState,
      projector: projector,
      pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
      caret: 2,
      preferredOrdinal: 0,
      sourceUtf16Length: 3,
      maximumCodeUnits: 8,
    );

    expect(window.text, 'abc');
    expect(window.globalUtf16Start, 0);
    expect(window.activeOrdinal, 0);
    expect(window.canonicalSelectionExtentUtf16, 2);
  });

  test('collapsed restoration identifies a neutral physical line', () {
    final viewportState = FlarkEditorViewportState()
      ..adoptUncertifiedSourceWindow(
        source: 'first\nsecond\nthird',
        startUtf16: 10,
      );

    final window = FlarkEditorInputWindowPlanner.restoreCollapsed(
      viewportState: viewportState,
      projector: _projector(viewportState),
      pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
      caret: 19,
      sourceUtf16Length: 28,
      maximumCodeUnits: 16,
    );

    expect(window.text, 'second\n');
    expect(window.globalUtf16Start, 16);
    expect(window.activeOrdinal, -2);
    expect(window.canonicalSelectionExtentUtf16, 19);
  });
}

FlarkViewportRow _row() => FlarkViewportRow(
  ordinal: 0,
  kind: 5,
  sourceBytes: const FlarkSourceRange(0, 3),
  sourceUtf16: const FlarkSourceRange(0, 3),
  editableBytes: const FlarkSourceRange(0, 3),
  editableUtf16: const FlarkSourceRange(0, 3),
  editCapability: FlarkViewportRowEditCapability.contiguous,
  headingLevel: null,
  headingStyle: null,
  listItem: null,
  blockQuote: null,
  codeBlock: null,
  thematicBreak: false,
  pathDepth: 0,
  inlineFacts: const [],
);

FlarkViewport _viewport({required List<FlarkViewportRow> rows}) =>
    FlarkViewport(
      revision: 1,
      snapshot: 1,
      requestedBytes: const FlarkSourceRange(0, 3),
      coveredBytes: const FlarkSourceRange(0, 3),
      coveredUtf16: const FlarkSourceRange(0, 3),
      certification: FlarkCertification.currentCertified,
      rows: rows,
      neutralSource: null,
      continuation: 0,
    );

FlarkSurfaceProjector _projector(FlarkEditorViewportState viewportState) =>
    viewportState.captureSurfaceProjector(
      pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
      inputGlobalUtf16Start: viewportState.visibleUtf16Start,
      inputValue: FlarkEditorInputValue(
        text: viewportState.visibleSource,
        selection: const FlarkTextSelection.collapsed(offset: 0),
      ),
      activeOrdinal: null,
      selectionBaseUtf16: viewportState.visibleUtf16Start,
      selectionExtentUtf16: viewportState.visibleUtf16Start,
      crossRowSelection: false,
    );

bool _startsWithLowSurrogate(String value) =>
    value.isNotEmpty &&
    value.codeUnitAt(0) >= 0xDC00 &&
    value.codeUnitAt(0) <= 0xDFFF;

bool _endsWithHighSurrogate(String value) =>
    value.isNotEmpty &&
    value.codeUnitAt(value.length - 1) >= 0xD800 &&
    value.codeUnitAt(value.length - 1) <= 0xDBFF;
