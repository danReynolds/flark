import '../core/core.dart' show FlarkSourceRange;
import '../render_plan/render_plan.dart';

/// An offset-independent digest of everything a live-rendered block renders.
///
/// Two blocks with equal signatures render identically, so a block whose
/// signature is unchanged between parses can have its widget instance reused
/// (Stage 3) — even if earlier edits shifted its absolute offsets. Equivalently,
/// the signature MUST change whenever the block's rendered output would change
/// (text, inline styling, checkbox state, code language, table structure/cells,
/// heading level, list kind, …) and MUST NOT change merely because offsets
/// shifted. That completeness is the correctness contract; it is covered by
/// `test/v2/flutter/flark_live_block_signature_test.dart`.
///
/// All positions are emitted relative to the block's display start, so a uniform
/// shift of the whole block leaves the signature unchanged. Style/colours are
/// constant across blocks (per editor) and are intentionally NOT part of the
/// per-block signature — a style change rebuilds every block.
///
/// See `doc/architecture/live_rendered_rebuild_isolation.md`.
String liveBlockContentSignature(
  FlarkRenderBlock block,
  String displayText, {
  String? markdown,
}) {
  final buffer = StringBuffer();
  _writeBlock(buffer, block, displayText, block.displayRange.start, markdown);
  return buffer.toString();
}

void _writeBlock(
  StringBuffer buffer,
  FlarkRenderBlock block,
  String displayText,
  int base,
  String? markdown,
) {
  buffer
    ..write('B|')
    ..write(block.kind.name)
    ..write('|')
    ..write(block.type)
    ..write('|')
    ..write(block.styleToken.name)
    ..write('|')
    ..write(_attributes(block.attributes))
    ..write('|')
    ..write(_slice(displayText, block.displayRange))
    ..write('|');

  final table = block.table;
  if (table != null) {
    buffer
      ..write('T:')
      ..write(table.columnAlignments.map((a) => a.name).join(','))
      ..write(';');
    for (final row in table.rows) {
      buffer
        ..write('r')
        ..write(row.header ? 'H' : '-')
        ..write(':');
      for (final cell in row.cells) {
        buffer
          ..write(_slice(displayText, cell.displayRange))
          ..write('~');
      }
      buffer.write(';');
    }
  }

  final listItem = block.listItem;
  if (listItem != null) {
    buffer
      ..write('L:${listItem.kind.name}:')
      ..write(_sourceListMarkerLabel(markdown, block) ?? '')
      ..write(';');
  }

  final task = block.taskListItem;
  if (task != null) buffer.write('K:${task.checked};');

  final code = block.codeBlock;
  if (code != null) {
    buffer
      ..write('C:')
      ..write(_sourceCodeFenceLanguage(markdown, block) ?? code.language ?? '')
      ..write(';');
  }

  for (final run in block.inlineRuns) {
    buffer
      ..write('i:')
      ..write(run.kind.name)
      ..write(':')
      ..write(run.type)
      ..write(':')
      ..write(run.styleToken.name)
      ..write(':');
    final action = run.action;
    if (action != null) {
      buffer
        ..write(action.kind.name)
        ..write('@')
        ..write(action.destination)
        ..write('@')
        ..write(action.title ?? '')
        ..write('@')
        ..write(action.label ?? '');
    }
    buffer
      ..write(':')
      ..write(run.displayRange.start - base)
      ..write('+')
      ..write(run.displayRange.end - run.displayRange.start)
      ..write(';');
  }

  buffer.write('{');
  for (final child in block.children) {
    _writeBlock(buffer, child, displayText, base, markdown);
    buffer.write('|');
  }
  buffer.write('}');
}

String _slice(String displayText, FlarkSourceRange range) {
  if (range.start < 0 ||
      range.end > displayText.length ||
      range.start > range.end) {
    return '';
  }
  return displayText.substring(range.start, range.end);
}

String _attributes(Map<String, Object?> attributes) {
  if (attributes.isEmpty) return '';
  final keys =
      attributes.keys
          .where((key) => key != 'stableId' && key != 'synthetic')
          .toList()
        ..sort();
  return keys.map((key) => '$key=${attributes[key]}').join(',');
}

String? _sourceListMarkerLabel(String? markdown, FlarkRenderBlock block) {
  if (markdown == null || block.listItem?.kind != FlarkRenderListKind.ordered) {
    return null;
  }
  final line = _sourceLine(markdown, block);
  if (line == null) return null;
  return _orderedListMarkerLabel(line);
}

String? _sourceCodeFenceLanguage(String? markdown, FlarkRenderBlock block) {
  if (markdown == null || block.codeBlock == null) return null;
  final line = _sourceLine(markdown, block);
  if (line == null) return null;
  var index = _skipHorizontalWhitespace(line, 0);
  if (index >= line.length) return null;
  final marker = line.codeUnitAt(index);
  if (marker != 0x60 && marker != 0x7E) return null;
  var markerCount = 0;
  while (index < line.length && line.codeUnitAt(index) == marker) {
    markerCount++;
    index++;
  }
  if (markerCount < 3) return null;
  final languageStart = _skipHorizontalWhitespace(line, index);
  if (languageStart >= line.length) return '';
  final languageEnd = _firstHorizontalWhitespaceOrEnd(line, languageStart);
  return line.substring(languageStart, languageEnd);
}

String? _sourceLine(String markdown, FlarkRenderBlock block) {
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start >= markdown.length ||
      block.sourceRange.start >= block.sourceRange.end) {
    return null;
  }
  final lineEnd = markdown.indexOf('\n', block.sourceRange.start);
  final effectiveLineEnd = lineEnd < 0 || lineEnd > block.sourceRange.end
      ? block.sourceRange.end
      : lineEnd;
  return markdown.substring(block.sourceRange.start, effectiveLineEnd);
}

String? _orderedListMarkerLabel(String line) {
  var index = _skipHorizontalWhitespace(line, 0);
  final digitStart = index;
  while (index < line.length &&
      index - digitStart < 9 &&
      _isAsciiDigit(line.codeUnitAt(index))) {
    index++;
  }
  if (index == digitStart) return null;
  if (index < line.length && _isAsciiDigit(line.codeUnitAt(index))) {
    return null;
  }
  if (index >= line.length) return null;

  final delimiter = line.codeUnitAt(index);
  if (delimiter != 0x2E && delimiter != 0x29) return null;
  return line.substring(digitStart, index + 1);
}

int _firstHorizontalWhitespaceOrEnd(String text, int start) {
  var index = start;
  while (index < text.length &&
      !_isHorizontalWhitespace(text.codeUnitAt(index))) {
    index++;
  }
  return index;
}

int _skipHorizontalWhitespace(String text, int start) {
  var index = start;
  while (index < text.length &&
      _isHorizontalWhitespace(text.codeUnitAt(index))) {
    index++;
  }
  return index;
}

bool _isHorizontalWhitespace(int codeUnit) {
  return codeUnit == 0x20 || codeUnit == 0x09;
}

bool _isAsciiDigit(int codeUnit) {
  return codeUnit >= 0x30 && codeUnit <= 0x39;
}
