part of 'sovereign_controller.dart';

class _ControllerSovereignSyntaxSyncHost implements SovereignSyntaxSyncHost {
  _ControllerSovereignSyntaxSyncHost(this._c);

  final SovereignController _c;

  @override
  TextEditingValue get value => _c.value;
  @override
  String get text => _c.text;

  @override
  SyntaxParseScheduler? get syntaxParseScheduler => _c._syntaxParseScheduler;
  @override
  MarkdownSyntaxProfile get markdownProfile => _c._markdownProfile;
  @override
  bool get isDecorationClosed => _c._decorationController.isClosed;
  @override
  int get revision => _c._revision;
  @override
  SyntaxEngine get syntaxEngine => _c._syntaxEngine;
  @override
  int get predictiveScanTimeBudgetMicros => _c._predictiveScanTimeBudgetMicros;
  @override
  int get predictiveScanSpanBudget => _c._predictiveScanSpanBudget;
  @override
  int? get predictiveScanCharLimitOverride =>
      _c._predictiveScanCharLimitOverride;
  @override
  int get predictiveBudgetExhaustionCount =>
      _c._predictiveBudgetExhaustionCount;
  @override
  set predictiveBudgetExhaustionCount(int value) =>
      _c._predictiveBudgetExhaustionCount = value;
  @override
  int get predictiveLocalFallbackCount => _c._predictiveLocalFallbackCount;
  @override
  set predictiveLocalFallbackCount(int value) =>
      _c._predictiveLocalFallbackCount = value;

  @override
  SyntaxSnapshot? get latestAuthoritativeSnapshot =>
      _c._latestAuthoritativeSnapshot;
  @override
  set latestAuthoritativeSnapshot(SyntaxSnapshot? value) =>
      _c._latestAuthoritativeSnapshot = value;
  @override
  List<StyleRun>? get authoritativeInlineRuns => _c._authoritativeInlineRuns;
  @override
  set authoritativeInlineRuns(List<StyleRun>? value) =>
      _c._authoritativeInlineRuns = value;
  @override
  int get authoritativeInlineRunsRevision =>
      _c._authoritativeInlineRunsRevision;
  @override
  set authoritativeInlineRunsRevision(int value) =>
      _c._authoritativeInlineRunsRevision = value;
  @override
  List<TextRange> get authoritativeHiddenRanges =>
      _c._authoritativeHiddenRanges;
  @override
  set authoritativeHiddenRanges(List<TextRange> value) =>
      _c._authoritativeHiddenRanges = value;
  @override
  List<TextRange> get authoritativeExclusionRanges =>
      _c._authoritativeExclusionRanges;
  @override
  set authoritativeExclusionRanges(List<TextRange> value) =>
      _c._authoritativeExclusionRanges = value;
  @override
  CursorValidationMask get authoritativeCursorMask =>
      _c._authoritativeCursorMask;
  @override
  set authoritativeCursorMask(CursorValidationMask value) =>
      _c._authoritativeCursorMask = value;

  @override
  List<TextRange> get projectedHiddenRanges => _c._projectedHiddenRanges;
  @override
  set projectedHiddenRanges(List<TextRange> value) =>
      _c._projectedHiddenRanges = value;
  @override
  List<TextRange> get projectedExclusionRanges => _c._projectedExclusionRanges;
  @override
  set projectedExclusionRanges(List<TextRange> value) =>
      _c._projectedExclusionRanges = value;
  @override
  CursorValidationMask get projectedCursorMask => _c._projectedCursorMask;
  @override
  set projectedCursorMask(CursorValidationMask value) =>
      _c._projectedCursorMask = value;
  @override
  CursorValidationMask get activeCursorMask => _c._activeCursorMask;
  @override
  set activeCursorMask(CursorValidationMask value) =>
      _c._activeCursorMask = value;

  @override
  DecorationModel get latestDecoration => _c._latestDecoration;
  @override
  LineIndex get lineIndex => _c._lineIndex;
  @override
  void publishDecoration(DecorationModel decoration) {
    _c._latestDecoration = decoration;
    _c._projector = Projector(decoration);
    _c._decorationController.add(decoration);
  }

  @override
  CursorValidationMask normalizeCursorMaskToText(
    CursorValidationMask mask, {
    required int textLength,
    List<TextRange> fallbackHiddenRanges = const [],
  }) =>
      _c._normalizeCursorMaskToText(
        mask,
        textLength: textLength,
        fallbackHiddenRanges: fallbackHiddenRanges,
      );

  @override
  List<TextRange> shiftAndVerifyRanges(
    List<TextRange> oldRanges,
    EditOp op,
    String newText,
  ) =>
      PredictiveEditRangeUtils.shiftAndVerifyRanges(oldRanges, op, newText);

  @override
  List<TextRange> scanInlineMarkersNearEdit({
    required String targetText,
    required int editStart,
    required int editEnd,
    required List<TextRange> excludedRanges,
  }) {
    final result = PredictiveEditRangeUtils.scanInlineMarkersNearEdit(
      targetText: targetText,
      editStart: editStart,
      editEnd: editEnd,
      excludedRanges: excludedRanges,
      charCap: PredictiveEditRangeUtils.defaultLocalInlineScanCharCap,
    );
    _c._predictiveLocalFallbackLastScannedChars = result.scannedChars;
    return result.hiddenRanges;
  }

  @override
  List<StyleRun> styleRunsFromInlineTokens(
    List<InlineSpanToken> tokens,
    int textLength,
  ) =>
      ProjectionRangeUtils.styleRunsFromInlineTokens(tokens, textLength);

  @override
  List<TextRange> normalizeHiddenRanges(
    List<TextRange> ranges,
    int textLength,
  ) =>
      ProjectionRangeUtils.normalizeHiddenRanges(ranges, textLength);

  @override
  int rangeKey(TextRange range) => ProjectionRangeUtils.rangeKey(range);

  @override
  List<TextRange> overlayCanonicalBlockMarkerRanges(
    String text,
    List<TextRange> ranges,
  ) =>
      ProjectionRangeUtils.overlayCanonicalBlockMarkerRanges(text, ranges);

  @override
  BlockTree blockTreeFromSnapshot(SyntaxSnapshot snapshot) =>
      SyntaxSnapshotMapper.blockTreeFromSnapshot(snapshot);

  @override
  List<TextRange> stabilizeRangesInAmbiguity({
    required List<TextRange> predictedRanges,
    required List<TextRange> authoritativeRanges,
    required List<TextRange> ambiguityZones,
    required int textLength,
  }) =>
      ProjectionRangeUtils.stabilizeRangesInAmbiguity(
        predictedRanges: predictedRanges,
        authoritativeRanges: authoritativeRanges,
        ambiguityZones: ambiguityZones,
        textLength: textLength,
      );

  @override
  int lineStartForOffset(String text, int offset) =>
      ProjectionRangeUtils.lineStartForOffset(text, offset);

  @override
  List<TextRange> fencedCodeFenceMarkers(
    String text,
    List<FencedCodeBlock> blocks,
  ) =>
      ProjectionRangeUtils.fencedCodeFenceMarkers(text, blocks);

  @override
  List<TextRange> markdownBlockMarkerRanges(
    String text, {
    required List<FencedCodeBlock> fencedBlocks,
  }) =>
      ProjectionRangeUtils.markdownBlockMarkerRanges(
        text,
        fencedBlocks: fencedBlocks,
      );

  @override
  int headerMarkerLength(String text, int lineStart, int lineEnd) =>
      ProjectionRangeUtils.headerMarkerLength(text, lineStart, lineEnd);

  @override
  int blockquoteMarkerLength(String text, int lineStart, int lineEnd) =>
      ProjectionRangeUtils.blockquoteMarkerLength(text, lineStart, lineEnd);

  @override
  int unorderedListMarkerLength(String text, int lineStart, int lineEnd) =>
      ProjectionRangeUtils.unorderedListMarkerLength(text, lineStart, lineEnd);

  @override
  int orderedListMarkerLength(String text, int lineStart, int lineEnd) =>
      ProjectionRangeUtils.orderedListMarkerLength(text, lineStart, lineEnd);

  @override
  bool isFenceLineTrailingRange(String text, TextRange range) =>
      ProjectionRangeUtils.isFenceLineTrailingRange(text, range);

  @override
  bool isFenceMarkerRange(String text, TextRange range) =>
      ProjectionRangeUtils.isFenceMarkerRange(text, range);

  @override
  String? inlineMarkerToken(String text, TextRange range) =>
      ProjectionRangeUtils.inlineMarkerToken(text, range);

  @override
  List<TextRange> selectionCenteredEmptyInlineRanges(TextEditingValue value) =>
      ProjectionRangeUtils.selectionCenteredEmptyInlineRanges(value);
}
