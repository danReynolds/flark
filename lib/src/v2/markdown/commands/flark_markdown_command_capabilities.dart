import '../../core/state/flark_editor_state.dart';
import '../inline/flark_markdown_inline_style.dart';
import '../source/flark_markdown_line_selection.dart';

final class FlarkMarkdownCommandCapabilities {
  FlarkMarkdownCommandCapabilities({
    Iterable<FlarkMarkdownInlineStyle> activeInlineStyles = const [],
    this.activeHeadingLevel,
    this.quoteActive = false,
    this.bulletListActive = false,
    this.orderedListActive = false,
    this.taskListActive = false,
    this.tableActive = false,
    this.canMutate = true,
  }) : activeInlineStyles = Set<FlarkMarkdownInlineStyle>.unmodifiable(
         activeInlineStyles,
       );

  final Set<FlarkMarkdownInlineStyle> activeInlineStyles;
  final int? activeHeadingLevel;
  final bool quoteActive;
  final bool bulletListActive;
  final bool orderedListActive;
  final bool taskListActive;
  final bool tableActive;
  final bool canMutate;

  bool isInlineStyleActive(FlarkMarkdownInlineStyle style) {
    return activeInlineStyles.contains(style);
  }
}

abstract final class FlarkMarkdownCommandQueries {
  static FlarkMarkdownCommandCapabilities capabilitiesAtSelection(
    FlarkEditorState state, {
    Iterable<FlarkMarkdownInlineStyle> pendingInlineStyles = const [],
    Iterable<FlarkMarkdownInlineStyle> mutedInlineStyles = const [],
  }) {
    final line = selectedMarkdownLines(state).first;
    return FlarkMarkdownCommandCapabilities(
      activeInlineStyles: _activeInlineStyles(
        state,
        pendingInlineStyles,
        mutedInlineStyles,
      ),
      activeHeadingLevel: _headingLevel(line.text),
      quoteActive: _quotePrefixLength(line.text) > 0,
      bulletListActive: _bulletMarker(line.text) != null,
      orderedListActive: _orderedMarker(line.text) != null,
      taskListActive: _taskMarker(line.text) != null,
      tableActive: _isInsideTable(state),
    );
  }

  /// The source span of the inline [style] run enclosing a collapsed caret, or
  /// null when the caret is not inside such a run. Emphasis matches either `*`
  /// or `_` delimiters.
  static FlarkInlineRunRange? enclosingInlineRun(
    FlarkEditorState state,
    FlarkMarkdownInlineStyle style,
  ) {
    final selection = state.selection;
    if (!selection.isCollapsed) return null;
    final text = state.markdown;
    final caret = selection.extentOffset.clamp(0, text.length);
    final markers = style == FlarkMarkdownInlineStyle.emphasis
        ? const ['*', '_']
        : [style.marker];
    for (final marker in markers) {
      final run = _enclosingDelimitedRun(text, caret, marker);
      if (run != null) return run;
    }
    return null;
  }
}

/// The source span of an inline run: its marker boundaries and content range.
final class FlarkInlineRunRange {
  const FlarkInlineRunRange({
    required this.openStart,
    required this.contentStart,
    required this.closeStart,
    required this.closeEnd,
  });

  /// Start of the opening marker.
  final int openStart;

  /// First content character (`openStart + markerLength`).
  final int contentStart;

  /// Start of the closing marker.
  final int closeStart;

  /// End of the closing marker.
  final int closeEnd;
}

Set<FlarkMarkdownInlineStyle> _activeInlineStyles(
  FlarkEditorState state,
  Iterable<FlarkMarkdownInlineStyle> pendingInlineStyles,
  Iterable<FlarkMarkdownInlineStyle> mutedInlineStyles,
) {
  final selection = state.selection;
  final text = state.markdown;
  final styles = <FlarkMarkdownInlineStyle>{};
  // Pending ("armed") styles only exist for a collapsed caret; union them so
  // toolbars light up the moment a style is armed, before any text is typed.
  if (selection.isCollapsed) styles.addAll(pendingInlineStyles);

  if (text.isNotEmpty) {
    final rangeStart = selection.start.clamp(0, text.length);
    final rangeEnd = selection.end.clamp(rangeStart, text.length);

    if (_isInsideInlineCode(text, rangeStart, rangeStart, rangeEnd)) {
      styles.add(FlarkMarkdownInlineStyle.inlineCode);
    }
    if (_isInsideDelimitedSpan(text, rangeStart, rangeStart, rangeEnd, '**')) {
      styles.add(FlarkMarkdownInlineStyle.strong);
    }
    if (_isInsideDelimitedSpan(text, rangeStart, rangeStart, rangeEnd, '*') ||
        _isInsideDelimitedSpan(text, rangeStart, rangeStart, rangeEnd, '_')) {
      styles.add(FlarkMarkdownInlineStyle.emphasis);
    }
    if (_isInsideDelimitedSpan(text, rangeStart, rangeStart, rangeEnd, '~~')) {
      styles.add(FlarkMarkdownInlineStyle.strikethrough);
    }
  }

  // Muted ("armed off") styles are removed for the collapsed caret so a style
  // toggled off inside its run reads inactive even though the source still
  // carries the markers.
  if (selection.isCollapsed) styles.removeAll(mutedInlineStyles);
  return styles;
}

bool _isInsideInlineCode(
  String text,
  int probeOffset,
  int rangeStart,
  int rangeEnd,
) {
  return _isInsideDelimitedSpan(text, probeOffset, rangeStart, rangeEnd, '`');
}

bool _isInsideDelimitedSpan(
  String text,
  int probeOffset,
  int rangeStart,
  int rangeEnd,
  String delimiter,
) {
  final selected = !identical(rangeStart, rangeEnd) && rangeStart < rangeEnd;
  if (selected &&
      rangeStart >= delimiter.length &&
      rangeEnd + delimiter.length <= text.length &&
      text.substring(rangeStart - delimiter.length, rangeStart) == delimiter &&
      text.substring(rangeEnd, rangeEnd + delimiter.length) == delimiter &&
      // The bracketing delimiter must be a genuine run, not part of a longer
      // one — otherwise a `*` italic probe matches the inner `*` of a `**`
      // bold pair, so bolding a selection falsely reports italic active too.
      _isDelimiterRun(text, rangeStart - delimiter.length, delimiter) &&
      _isDelimiterRun(text, rangeEnd, delimiter)) {
    return true;
  }

  return _enclosingDelimitedRun(text, probeOffset, delimiter) != null;
}

/// The run of [delimiter] enclosing a collapsed caret at [probeOffset], or
/// null. A caret exactly at the closing delimiter's start counts as inside the
/// run (its trailing edge).
FlarkInlineRunRange? _enclosingDelimitedRun(
  String text,
  int probeOffset,
  String delimiter,
) {
  // A single opener lookup would mistake a closing delimiter sitting at the
  // caret for an opener, so keep searching backward for the real opener
  // whenever the nearest candidate sits at or after the caret's content start.
  var searchCeiling = probeOffset;
  while (searchCeiling >= 0) {
    final before = _findOpeningDelimiter(text, delimiter, searchCeiling);
    if (before == null) return null;
    final contentStart = before + delimiter.length;
    if (probeOffset < contentStart) {
      searchCeiling = before - 1;
      continue;
    }
    final after = _findClosingDelimiter(text, delimiter, contentStart);
    if (after == null) return null;
    if (probeOffset > after) return null;
    return FlarkInlineRunRange(
      openStart: before,
      contentStart: contentStart,
      closeStart: after,
      closeEnd: after + delimiter.length,
    );
  }
  return null;
}

int? _findOpeningDelimiter(String text, String delimiter, int probeOffset) {
  var searchOffset = probeOffset.clamp(0, text.length);
  while (searchOffset >= 0) {
    final index = text.lastIndexOf(delimiter, searchOffset);
    if (index < 0) return null;
    if (!_isEscaped(text, index) && _isDelimiterRun(text, index, delimiter)) {
      return index;
    }
    searchOffset = index - 1;
  }
  return null;
}

int? _findClosingDelimiter(String text, String delimiter, int startOffset) {
  var searchOffset = startOffset.clamp(0, text.length);
  while (searchOffset < text.length) {
    final index = text.indexOf(delimiter, searchOffset);
    if (index < 0) return null;
    if (!_isEscaped(text, index) && _isDelimiterRun(text, index, delimiter)) {
      return index;
    }
    searchOffset = index + delimiter.length;
  }
  return null;
}

bool _isDelimiterRun(String text, int offset, String delimiter) {
  if (delimiter != '*' && delimiter != '_') return true;
  final codeUnit = delimiter.codeUnitAt(0);
  final before = offset > 0 ? text.codeUnitAt(offset - 1) : null;
  final afterOffset = offset + delimiter.length;
  final after = afterOffset < text.length ? text.codeUnitAt(afterOffset) : null;
  if (delimiter.length == 1) {
    return before != codeUnit && after != codeUnit;
  }
  return true;
}

bool _isEscaped(String text, int offset) {
  var slashCount = 0;
  for (var cursor = offset - 1; cursor >= 0; cursor -= 1) {
    if (text.codeUnitAt(cursor) != 92) break;
    slashCount += 1;
  }
  return slashCount.isOdd;
}

int? _headingLevel(String line) {
  final match = RegExp(r'^(#{1,6})(?:\s+|$)').firstMatch(line);
  return match?.group(1)?.length;
}

int _quotePrefixLength(String text) {
  final match = RegExp(r'^(?:>\s?)+').firstMatch(text);
  return match?.group(0)?.length ?? 0;
}

RegExpMatch? _bulletMarker(String line) {
  final prefixLength = _quotePrefixLength(line);
  return RegExp(r'^[-+*]\s+').firstMatch(line.substring(prefixLength));
}

RegExpMatch? _orderedMarker(String line) {
  final prefixLength = _quotePrefixLength(line);
  return RegExp(r'^\d{1,9}[.)]\s+').firstMatch(line.substring(prefixLength));
}

RegExpMatch? _taskMarker(String line) {
  final prefixLength = _quotePrefixLength(line);
  return RegExp(
    r'^[-+*]\s+\[[ xX]\]\s+',
  ).firstMatch(line.substring(prefixLength));
}

bool _isInsideTable(FlarkEditorState state) {
  final line = selectedMarkdownLines(state).first;
  if (!_looksLikeTableRow(line.text)) return false;
  final buffer = state.document.buffer;
  var scan = line.index - 1;
  while (scan >= 0) {
    final text = state.markdown.substring(
      buffer.lineStart(scan),
      buffer.lineEnd(scan),
    );
    if (!_looksLikeTableRow(text)) break;
    if (_looksLikeTableSeparator(text)) return true;
    scan -= 1;
  }
  scan = line.index + 1;
  while (scan < buffer.lineCount) {
    final text = state.markdown.substring(
      buffer.lineStart(scan),
      buffer.lineEnd(scan),
    );
    if (!_looksLikeTableRow(text)) break;
    if (_looksLikeTableSeparator(text)) return true;
    scan += 1;
  }
  return false;
}

bool _looksLikeTableRow(String line) {
  final cells = _splitTableCells(line.trim());
  return cells != null && cells.length >= 2;
}

bool _looksLikeTableSeparator(String line) {
  final cells = _splitTableCells(line.trim());
  return cells != null && cells.length >= 2 && cells.every(_isSeparatorCell);
}

List<String>? _splitTableCells(String line) {
  var body = line;
  if (body.startsWith('|')) body = body.substring(1);
  if (body.endsWith('|')) body = body.substring(0, body.length - 1);
  final cells = body.split('|');
  if (cells.length < 2) return null;
  return cells;
}

bool _isSeparatorCell(String cell) {
  var trimmed = cell.trim();
  if (trimmed.startsWith(':')) trimmed = trimmed.substring(1);
  if (trimmed.endsWith(':')) trimmed = trimmed.substring(0, trimmed.length - 1);
  return trimmed.isNotEmpty &&
      trimmed.codeUnits.every((codeUnit) => codeUnit == 45);
}
