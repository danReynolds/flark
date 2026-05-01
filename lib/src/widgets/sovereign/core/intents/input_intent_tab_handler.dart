import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import '../structure/models/fence_context.dart';
import '../structure/models/list_marker_context.dart';

abstract class SovereignTabIntentHost {
  TextEditingValue get value;
  TextSelection get selection;
  LineIndex get lineIndex;
  GeometryModel get geometry;

  void commitProgrammaticTextEdit(TextEditingValue newValue);
  bool tryHandleTableTabKey({required bool reverse});
  FenceContext? fenceContextForCaret(
    String text,
    int caret, {
    required bool includeUnclosedEof,
  });
  String preferredOutdentUnitForLine({
    required String text,
    required MeasuredBlock block,
    required int line,
    required String currentIndent,
  });

  ListMarkerContext? listMarkerForLineAllowingQuotePrefix(
    String text,
    int lineStart,
    int lineEnd,
  );
  String preferredIndentUnit(String currentIndent);
  String removeOneIndentUnit(String indent, String unit);
}

class SovereignTabIntentHandler {
  SovereignTabIntentHandler(this._host);

  final SovereignTabIntentHost _host;

  bool handleTabKey({required bool reverse}) {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid) return false;

    final text = _host.value.text;
    if (sel.start < 0 || sel.end < 0 || sel.start > text.length) return false;
    if (sel.end > text.length) return false;

    if (_host.tryHandleTableTabKey(reverse: reverse)) return true;
    if (_tryHandleListTabKey(reverse: reverse)) return true;

    final anchor = sel.start;
    final extentAnchor = sel.isCollapsed
        ? sel.start
        : (sel.end > sel.start ? sel.end - 1 : sel.end);

    final startContext = _host.fenceContextForCaret(
      text,
      anchor,
      includeUnclosedEof: true,
    );
    if (startContext == null) return false;

    final endContext = _host.fenceContextForCaret(
      text,
      extentAnchor.clamp(0, text.length),
      includeUnclosedEof: true,
    );
    if (endContext == null || endContext.block != startContext.block) {
      return false;
    }

    final contentStartLine = startContext.openLine + 1;
    final contentEndLine = startContext.closeLineExclusive - 1;
    if (contentStartLine > contentEndLine) return false;

    if (sel.isCollapsed) {
      final caret = sel.baseOffset;
      final caretLine = _host.lineIndex.lineAtOffset(caret);
      if (caretLine < contentStartLine || caretLine > contentEndLine) {
        return false;
      }

      if (!reverse) {
        final lineStart = _host.lineIndex.offsetAtLine(caretLine);
        final lineEnd = (caretLine + 1 < _host.lineIndex.lineCount)
            ? _host.lineIndex.offsetAtLine(caretLine + 1)
            : text.length;
        final lineContentEnd =
            (lineEnd > lineStart && text.codeUnitAt(lineEnd - 1) == 10)
                ? lineEnd - 1
                : lineEnd;
        final lineText = text.substring(lineStart, lineContentEnd);
        final indent = _host.preferredIndentUnit(
          _leadingWhitespacePrefix(lineText),
        );
        final newText = text.replaceRange(caret, caret, indent);
        _host.commitProgrammaticTextEdit(
          _host.value.copyWith(
            text: newText,
            selection: TextSelection.collapsed(offset: caret + indent.length),
            composing: TextRange.empty,
          ),
        );
        return true;
      }

      final lineStart = _host.lineIndex.offsetAtLine(caretLine);
      final lineEnd = (caretLine + 1 < _host.lineIndex.lineCount)
          ? _host.lineIndex.offsetAtLine(caretLine + 1)
          : text.length;
      final lineContentEnd =
          (lineEnd > lineStart && text.codeUnitAt(lineEnd - 1) == 10)
              ? lineEnd - 1
              : lineEnd;
      final lineText = text.substring(lineStart, lineContentEnd);
      final leading = _leadingWhitespacePrefix(lineText);
      if (leading.isEmpty) return true;
      final unit = _host.preferredOutdentUnitForLine(
        text: text,
        block: startContext.block,
        line: caretLine,
        currentIndent: leading,
      );
      final newLeading = _host.removeOneIndentUnit(leading, unit);
      final removeLen = leading.length - newLeading.length;
      if (removeLen <= 0) return true;

      final newText = text.replaceRange(lineStart, lineStart + removeLen, '');
      final newCaret = (caret - removeLen).clamp(lineStart, newText.length);
      _host.commitProgrammaticTextEdit(
        _host.value.copyWith(
          text: newText,
          selection: TextSelection.collapsed(offset: newCaret),
          composing: TextRange.empty,
        ),
      );
      return true;
    }

    var startLine = _host.lineIndex.lineAtOffset(sel.start);
    var endLine = _host.lineIndex.lineAtOffset(
      sel.end > sel.start ? sel.end - 1 : sel.end,
    );
    if (startLine < contentStartLine) startLine = contentStartLine;
    if (endLine > contentEndLine) endLine = contentEndLine;
    if (startLine > endLine) return false;

    var newText = text;
    var base = sel.baseOffset;
    var extent = sel.extentOffset;

    int shiftOffset(int offset, int editStart, int oldLen, int newLen) {
      if (offset <= editStart) return offset;
      if (offset >= editStart + oldLen) return offset + (newLen - oldLen);
      return editStart + newLen;
    }

    var delta = 0;
    String indentUnit = '  ';
    if (!reverse) {
      final firstStart = _host.lineIndex.offsetAtLine(startLine);
      final firstEnd = (startLine + 1 < _host.lineIndex.lineCount)
          ? _host.lineIndex.offsetAtLine(startLine + 1)
          : text.length;
      final firstContentEnd =
          (firstEnd > firstStart && text.codeUnitAt(firstEnd - 1) == 10)
              ? firstEnd - 1
              : firstEnd;
      final firstLineText = text.substring(firstStart, firstContentEnd);
      indentUnit = _host.preferredIndentUnit(
        _leadingWhitespacePrefix(firstLineText),
      );
    }

    for (var line = startLine; line <= endLine; line++) {
      final originalLineStart = _host.lineIndex.offsetAtLine(line);
      final lineStart = originalLineStart + delta;
      final lineEnd = (line + 1 < _host.lineIndex.lineCount)
          ? _host.lineIndex.offsetAtLine(line + 1) + delta
          : newText.length;
      final lineContentEnd =
          (lineEnd > lineStart && newText.codeUnitAt(lineEnd - 1) == 10)
              ? lineEnd - 1
              : lineEnd;

      if (!reverse) {
        newText = newText.replaceRange(lineStart, lineStart, indentUnit);
        base = shiftOffset(base, lineStart, 0, indentUnit.length);
        extent = shiftOffset(extent, lineStart, 0, indentUnit.length);
        delta += indentUnit.length;
        continue;
      }

      final lineText = newText.substring(lineStart, lineContentEnd);
      final leading = _leadingWhitespacePrefix(lineText);
      if (leading.isEmpty) continue;
      final unit = _host.preferredOutdentUnitForLine(
        text: newText,
        block: startContext.block,
        line: line,
        currentIndent: leading,
      );
      final newLeading = _host.removeOneIndentUnit(leading, unit);
      final removeLen = leading.length - newLeading.length;
      if (removeLen <= 0) continue;

      newText = newText.replaceRange(lineStart, lineStart + removeLen, '');
      base = shiftOffset(base, lineStart, removeLen, 0);
      extent = shiftOffset(extent, lineStart, removeLen, 0);
      delta -= removeLen;
    }

    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: newText,
        selection: TextSelection(
          baseOffset: base.clamp(0, newText.length),
          extentOffset: extent.clamp(0, newText.length),
          affinity: sel.affinity,
          isDirectional: sel.isDirectional,
        ),
        composing: TextRange.empty,
      ),
    );
    return true;
  }

  bool _tryHandleListTabKey({required bool reverse}) {
    final sel = _host.selection;
    if (!sel.isValid) return false;
    final text = _host.value.text;
    if (text.isEmpty) return false;

    final anchor = sel.start.clamp(0, text.length);
    final extentAnchor = sel.isCollapsed
        ? anchor
        : (sel.end > sel.start ? sel.end - 1 : sel.end).clamp(0, text.length);

    if (_host.fenceContextForCaret(text, anchor, includeUnclosedEof: true) !=
        null) {
      return false;
    }
    if (_host.fenceContextForCaret(
          text,
          extentAnchor,
          includeUnclosedEof: true,
        ) !=
        null) {
      return false;
    }

    if (sel.isCollapsed) {
      return _handleCollapsedListTab(
        text: text,
        caret: anchor,
        reverse: reverse,
      );
    }
    return _handleSelectionListTab(
      text: text,
      selection: sel,
      reverse: reverse,
    );
  }

  bool _handleCollapsedListTab({
    required String text,
    required int caret,
    required bool reverse,
  }) {
    if (caret < 0 || caret > text.length) return false;
    if (_host.lineIndex.lineCount <= 0) return false;

    final line = _host.lineIndex.lineAtOffset(caret);
    final lineStart = _host.lineIndex.offsetAtLine(line);
    final lineEnd = (line + 1 < _host.lineIndex.lineCount)
        ? _host.lineIndex.offsetAtLine(line + 1)
        : text.length;
    final lineContentEnd =
        (lineEnd > lineStart && text.codeUnitAt(lineEnd - 1) == 10)
            ? lineEnd - 1
            : lineEnd;
    final marker = listMarkerForTabLine(text, lineStart, lineContentEnd);
    if (marker == null) return false;

    final markerStart = marker.markerStart;
    final markerEnd = marker.markerEnd;
    if (!reverse) {
      const indentUnit = '  ';
      final newText = text.replaceRange(markerStart, markerStart, indentUnit);
      final newCaret = caret >= markerStart ? caret + indentUnit.length : caret;
      _host.commitProgrammaticTextEdit(
        _host.value.copyWith(
          text: newText,
          selection: TextSelection.collapsed(
            offset: newCaret.clamp(0, newText.length),
          ),
          composing: TextRange.empty,
        ),
      );
      return true;
    }

    final removable = _removableListIndentSpan(
      text: text,
      lineStart: lineStart,
      markerStart: markerStart,
      markerEnd: markerEnd,
    );
    if (removable == null) return true;

    final newText = text.replaceRange(removable.start, removable.end, '');
    final removedLen = removable.end - removable.start;
    final newCaret = caret <= removable.start
        ? caret
        : (caret - removedLen).clamp(removable.start, newText.length);
    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: newText,
        selection: TextSelection.collapsed(offset: newCaret),
        composing: TextRange.empty,
      ),
    );
    return true;
  }

  bool _handleSelectionListTab({
    required String text,
    required TextSelection selection,
    required bool reverse,
  }) {
    if (_host.lineIndex.lineCount <= 0) return false;
    var startLine = _host.lineIndex.lineAtOffset(
      selection.start.clamp(0, text.length),
    );
    var endLine = _host.lineIndex.lineAtOffset(
      (selection.end > selection.start ? selection.end - 1 : selection.end)
          .clamp(0, text.length),
    );
    if (startLine > endLine) {
      final tmp = startLine;
      startLine = endLine;
      endLine = tmp;
    }

    var handledAny = false;
    var newText = text;
    var base = selection.baseOffset;
    var extent = selection.extentOffset;
    var delta = 0;

    int shiftOffset(int offset, int editStart, int oldLen, int newLen) {
      if (offset <= editStart) return offset;
      if (offset >= editStart + oldLen) return offset + (newLen - oldLen);
      return editStart + newLen;
    }

    for (var line = startLine; line <= endLine; line++) {
      final rawLineStart = _host.lineIndex.offsetAtLine(line);
      final lineStart = rawLineStart + delta;
      final rawLineEnd = (line + 1 < _host.lineIndex.lineCount)
          ? _host.lineIndex.offsetAtLine(line + 1)
          : text.length;
      final lineEnd = rawLineEnd + delta;
      final lineContentEnd =
          (lineEnd > lineStart && newText.codeUnitAt(lineEnd - 1) == 10)
              ? lineEnd - 1
              : lineEnd;

      final marker = listMarkerForTabLine(newText, lineStart, lineContentEnd);
      if (marker == null) continue;
      handledAny = true;

      final markerStart = marker.markerStart;
      final markerEnd = marker.markerEnd;
      if (!reverse) {
        const indentUnit = '  ';
        newText = newText.replaceRange(markerStart, markerStart, indentUnit);
        base = shiftOffset(base, markerStart, 0, indentUnit.length);
        extent = shiftOffset(extent, markerStart, 0, indentUnit.length);
        delta += indentUnit.length;
        continue;
      }

      final removable = _removableListIndentSpan(
        text: newText,
        lineStart: lineStart,
        markerStart: markerStart,
        markerEnd: markerEnd,
      );
      if (removable == null) continue;

      newText = newText.replaceRange(removable.start, removable.end, '');
      final removedLen = removable.end - removable.start;
      base = shiftOffset(base, removable.start, removedLen, 0);
      extent = shiftOffset(extent, removable.start, removedLen, 0);
      delta -= removedLen;
    }

    if (!handledAny) return false;
    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: newText,
        selection: TextSelection(
          baseOffset: base.clamp(0, newText.length),
          extentOffset: extent.clamp(0, newText.length),
          affinity: selection.affinity,
          isDirectional: selection.isDirectional,
        ),
        composing: TextRange.empty,
      ),
    );
    return true;
  }

  TextRange? _removableListIndentSpan({
    required String text,
    required int lineStart,
    required int markerStart,
    required int markerEnd,
  }) {
    if (markerStart <= lineStart) return null;
    var whitespaceStart = markerStart;
    while (whitespaceStart > lineStart) {
      final cu = text.codeUnitAt(whitespaceStart - 1);
      if (cu == 32 || cu == 9) {
        whitespaceStart--;
        continue;
      }
      break;
    }

    var removableStart = whitespaceStart;
    if (whitespaceStart < markerStart &&
        whitespaceStart > lineStart &&
        text.codeUnitAt(whitespaceStart - 1) == 62 &&
        text.codeUnitAt(whitespaceStart) == 32) {
      removableStart = whitespaceStart + 1;
    }
    if (removableStart >= markerStart) return null;

    final currentIndent = text.substring(removableStart, markerStart);
    if (currentIndent.isEmpty) return null;
    final unit = _host.preferredIndentUnit(currentIndent);
    final nextIndent = _host.removeOneIndentUnit(currentIndent, unit);
    final removeLen = currentIndent.length - nextIndent.length;
    if (removeLen <= 0) return null;

    return TextRange(start: removableStart, end: removableStart + removeLen);
  }

  ListMarkerContext? listMarkerForTabLine(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    final direct = _host.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (direct != null) return direct;

    var cursor = lineStart;
    while (cursor < lineEnd) {
      final cu = text.codeUnitAt(cursor);
      if (cu == 32 || cu == 9) {
        cursor++;
        continue;
      }
      break;
    }
    if (cursor == lineStart) return null;
    if (cursor >= lineEnd) return null;
    return _host.listMarkerForLineAllowingQuotePrefix(text, cursor, lineEnd);
  }

  String _leadingWhitespacePrefix(String input) {
    var i = 0;
    while (i < input.length) {
      final ch = input.codeUnitAt(i);
      if (ch != 32 && ch != 9) break;
      i++;
    }
    return i == 0 ? '' : input.substring(0, i);
  }
}
