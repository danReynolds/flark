import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  FlarkViewportRow row() => FlarkViewportRow(
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

  FlarkViewportRow projectedRow() => FlarkViewportRow(
    ordinal: 0,
    kind: 5,
    sourceBytes: const FlarkSourceRange(0, 5),
    sourceUtf16: const FlarkSourceRange(0, 5),
    editableBytes: const FlarkSourceRange(0, 5),
    editableUtf16: const FlarkSourceRange(0, 5),
    editCapability: FlarkViewportRowEditCapability.contiguous,
    headingLevel: null,
    headingStyle: null,
    listItem: null,
    blockQuote: null,
    codeBlock: null,
    thematicBreak: false,
    pathDepth: 0,
    inlineFacts: const [],
    projectionSegments: const [
      FlarkProjectionSegment(
        sourceBytes: FlarkSourceRange(2, 3),
        sourceUtf16: FlarkSourceRange(2, 3),
      ),
    ],
  );

  FlarkSurfaceProjector projectedProjector(FlarkTextSelection selection) =>
      FlarkSurfaceProjector(
        pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
        visibleUtf16Start: 0,
        visibleSource: '**a**',
        inputGlobalUtf16Start: 0,
        inputValue: FlarkEditorInputValue(text: '**a**', selection: selection),
        activeOrdinal: 0,
        selectionBaseUtf16: selection.baseOffset,
        selectionExtentUtf16: selection.extentOffset,
        crossRowSelection: false,
        semanticViewportCurrent: true,
        certificationRevisionCurrent: true,
        certificationRanges: const [],
        optimisticRanges: FlarkOptimisticRangeMap(),
      );

  test('projector captures optimistic state instead of sharing it', () {
    final optimisticRanges = FlarkOptimisticRangeMap();
    final projector = FlarkSurfaceProjector(
      pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
      visibleUtf16Start: 0,
      visibleSource: 'abc',
      inputGlobalUtf16Start: 0,
      inputValue: const FlarkEditorInputValue(
        text: 'abc',
        selection: FlarkTextSelection.collapsed(offset: 3),
      ),
      activeOrdinal: 0,
      selectionBaseUtf16: 3,
      selectionExtentUtf16: 3,
      crossRowSelection: false,
      semanticViewportCurrent: true,
      certificationRevisionCurrent: true,
      certificationRanges: const [],
      optimisticRanges: optimisticRanges,
    );

    optimisticRanges.add(
      const FlarkOptimisticViewportEdit(start: 0, end: 0, replacementLength: 2),
    );

    final sourceRange = projector.surfaceSourceRange(row());
    expect((sourceRange.start, sourceRange.end), (0, 3));
    expect(projector.surfaceRow(row()).text, 'abc');
  });

  test(
    'portable input state preserves direction, affinity, and composition',
    () {
      const value = FlarkEditorInputValue(
        text: 'abc',
        selection: FlarkTextSelection(
          baseOffset: 3,
          extentOffset: 1,
          affinity: FlarkTextAffinity.upstream,
          isDirectional: true,
        ),
        composing: FlarkTextRange(start: 1, end: 3),
      );

      expect(value.selection.start, 1);
      expect(value.selection.end, 3);
      expect(value.selection.isCollapsed, isFalse);
      expect(value.selection.isDirectional, isTrue);
      expect(value.selection.affinity, FlarkTextAffinity.upstream);
      expect(value.composing.isValid, isTrue);
      expect(value.composing.isCollapsed, isFalse);
      expect(value, equals(value));

      expect(FlarkTextRange.empty.isValid, isFalse);
      expect(FlarkTextRange.empty.isCollapsed, isTrue);
    },
  );

  test('projector normalizes only illegal hidden-source caret positions', () {
    final projector = projectedProjector(
      const FlarkTextSelection.collapsed(offset: 1),
    );
    final projected = projectedRow();

    expect(projector.surfaceHasProjection(projected), isTrue);
    expect(
      projector.normalizeProjectedSelection(
        projected,
        const FlarkEditorInputValue(
          text: '**a**',
          selection: FlarkTextSelection.collapsed(offset: 1),
        ),
      ),
      const FlarkTextSelection.collapsed(offset: 2),
    );
    expect(
      projector.normalizeProjectedSelection(
        projected,
        const FlarkEditorInputValue(
          text: '**a**',
          selection: FlarkTextSelection.collapsed(offset: 2),
        ),
      ),
      const FlarkTextSelection.collapsed(offset: 2),
    );
  });

  test('projector classifies hidden-only selections in source space', () {
    final projector = projectedProjector(
      const FlarkTextSelection(baseOffset: 0, extentOffset: 2),
    );
    final projected = projectedRow();

    expect(
      projector.mutationTouchesOnlyHiddenProjection(
        projected,
        sourceStartUtf16: 0,
        sourceEndUtf16: 2,
      ),
      isTrue,
    );
    expect(
      projector.mutationTouchesOnlyHiddenProjection(
        projected,
        sourceStartUtf16: 2,
        sourceEndUtf16: 3,
      ),
      isFalse,
    );
    expect(
      projector.normalizeProjectedSelection(
        projected,
        const FlarkEditorInputValue(
          text: '**a**',
          selection: FlarkTextSelection(baseOffset: 0, extentOffset: 2),
        ),
      ),
      const FlarkTextSelection.collapsed(offset: 0),
    );
  });

  test(
    'projected deletion targets a visible grapheme or consumes its edge',
    () {
      final backward = projectedProjector(
        const FlarkTextSelection.collapsed(offset: 3),
      ).projectedDeletion(projectedRow(), backward: true);
      final forward = projectedProjector(
        const FlarkTextSelection.collapsed(offset: 3),
      ).projectedDeletion(projectedRow(), backward: false);

      expect(backward, isA<FlarkProjectedDeletionSource>());
      final source = (backward! as FlarkProjectedDeletionSource).sourceUtf16;
      expect((source.start, source.end), (2, 3));
      expect(forward, isA<FlarkProjectedDeletionBoundary>());
    },
  );
}
