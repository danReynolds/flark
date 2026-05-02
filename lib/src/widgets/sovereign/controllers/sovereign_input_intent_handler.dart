part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

class _ControllerSovereignInputIntentHost implements SovereignInputIntentHost {
  _ControllerSovereignInputIntentHost(this._c);

  final SovereignController _c;

  @override
  TextEditingValue get value => _c.value;
  @override
  TextSelection get selection => _c.selection;
  @override
  set selection(TextSelection value) => _c.selection = value;

  @override
  LineIndex get lineIndex => _c._lineIndex;
  @override
  GeometryModel get geometry => _c._geometry;
  @override
  List<TextRange> get projectedHiddenRanges => _c._projectedHiddenRanges;

  @override
  void commitProgrammaticTextEdit(TextEditingValue newValue) =>
      _c._commitProgrammaticTextEdit(newValue);

  @override
  bool isCaretInFenceBody(String text, int caret) =>
      _c._isCaretInFenceBody(text, caret);

  @override
  bool tryHandleIndentedCodeBlockEnter(String text, int caret) =>
      _c._indentedCodeEnterService.tryHandleEnter(
        value: _c.value,
        caret: caret,
        lineIndex: _c._lineIndex,
        geometry: _c._geometry,
        commitProgrammaticTextEdit: _c._commitProgrammaticTextEdit,
      );

  @override
  bool tryHandleTableTabKey({required bool reverse}) =>
      _c._tableTabIntents.tryHandleTabKey(reverse: reverse);

  @override
  structure.FenceContext? fenceContextForCaret(
    String text,
    int caret, {
    required bool includeUnclosedEof,
  }) =>
      _c._fenceContextForCaret(
        text,
        caret,
        includeUnclosedEof: includeUnclosedEof,
      );

  @override
  String preferredOutdentUnitForLine({
    required String text,
    required MeasuredBlock block,
    required int line,
    required String currentIndent,
  }) =>
      _c._preferredOutdentUnitForLine(
        text: text,
        block: block,
        line: line,
        currentIndent: currentIndent,
      );

  @override
  structure.ListMarkerContext? listMarkerForLineAllowingQuotePrefix(
    String text,
    int lineStart,
    int lineEnd,
  ) =>
      MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
        text,
        lineStart,
        lineEnd,
      );

  @override
  String preferredIndentUnit(String currentIndent) =>
      FenceEditingUtils.preferredIndentUnit(currentIndent);

  @override
  String removeOneIndentUnit(String indent, String unit) =>
      FenceEditingUtils.removeOneIndentUnit(indent, unit);

  @override
  structure.QuoteContext? quoteContextForLine(String text, int line) =>
      _c._quoteContextForLine(text, line);

  @override
  bool shouldExitFenceOnArrowDown({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _c._shouldExitFenceOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
      );

  @override
  bool shouldExitBlockquoteOnArrowDown({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _c._shouldExitBlockquoteOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
      );

  @override
  bool shouldExitFenceOnArrowUp({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _c._shouldExitFenceOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
      );

  @override
  bool shouldExitBlockquoteOnArrowUp({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _c._shouldExitBlockquoteOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
      );

  @override
  int columnAlignedOffsetForLineOrBoundary({
    required String text,
    required int line,
    required int column,
    required bool afterDocument,
  }) =>
      NavigationLineUtils.columnAlignedOffsetForLineOrBoundary(
        text: text,
        lineIndex: _c._lineIndex,
        line: line,
        column: column,
        afterDocument: afterDocument,
      );

  @override
  bool moveCaretVertically({required bool forward}) =>
      _c._moveCaretVertically(forward: forward);

  @override
  FenceEnterExitResult? computeFenceExitOnEnter({
    required String text,
    required int caret,
    required structure.FenceContext context,
  }) =>
      _c._computeFenceExitOnEnter(text: text, caret: caret, context: context);

  @override
  int trailingBlankTrimStart(
    String text,
    int openLine,
    int closeLineExclusive,
  ) =>
      _c._trailingBlankTrimStart(text, openLine, closeLineExclusive);

  @override
  bool isWhitespaceLine(String text, int start, int end) =>
      NavigationLineUtils.isWhitespaceLine(text, start, end);

  @override
  bool hasTaskMarker(String text, int markerEnd, int lineEnd) =>
      MarkdownLineHelpers.taskMarkerInfo(text, markerEnd, lineEnd) != null;

  @override
  bool isUnclosedFenceAtEof(String text, MeasuredBlock block) =>
      _c._isUnclosedFenceAtEof(text, block);
}
