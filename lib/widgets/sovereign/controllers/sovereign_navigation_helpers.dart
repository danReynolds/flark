import '../core/intents/input_intent_models.dart';
import '../core/structure/fence/fence_editing_utils.dart';
import '../core/structure/models/fence_context.dart' as structure;
import '../core/structure/models/quote_context.dart' as structure;
import '../core/structure/navigation/navigation_line_utils.dart';
import '../core/syntax/projection_range_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_code_highlighter.dart';
import '../models/geometry_model.dart';
import '../models/line_index.dart';

class SovereignNavigationHelpers {
  const SovereignNavigationHelpers();

  structure.FenceContext? fenceContextForCaret({
    required String text,
    required int caret,
    required LineIndex lineIndex,
    required GeometryModel geometry,
    required bool includeUnclosedEof,
  }) {
    MeasuredBlock? containing;
    for (final block in geometry.codeBlocks) {
      final inside = caret >= block.startOffset && caret < block.endOffset;
      final atUnclosedEofEnd = includeUnclosedEof &&
          caret == block.endOffset &&
          isUnclosedFenceAtEof(text: text, block: block);
      if (inside || atUnclosedEofEnd) {
        containing = block;
        break;
      }
    }
    if (containing == null) return null;

    final closeLineStart = ProjectionRangeUtils.lineStartForOffset(
      text,
      containing.endOffset - 1,
    );
    final hasClosingFence = closeLineStart != containing.startOffset &&
        closeLineStart + 3 <= text.length &&
        text.startsWith('```', closeLineStart);
    final openLine = lineIndex.lineAtOffset(containing.startOffset);
    final closeLine =
        hasClosingFence ? lineIndex.lineAtOffset(closeLineStart) : null;
    final closeLineExclusive = closeLine ?? lineIndex.lineCount;

    return structure.FenceContext(
      block: containing,
      openLine: openLine,
      closeLineExclusive: closeLineExclusive,
      closeLine: closeLine,
      hasClosingFence: hasClosingFence,
    );
  }

  structure.QuoteContext? quoteContextForLine({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required GeometryModel geometry,
  }) {
    if (line < 0 || line >= lineIndex.lineCount) return null;
    if (!_isQuoteLineOutsideFences(
      text: text,
      line: line,
      lineIndex: lineIndex,
      geometry: geometry,
    )) {
      return null;
    }

    var startLine = line;
    while (startLine > 0 &&
        _isQuoteLineOutsideFences(
          text: text,
          line: startLine - 1,
          lineIndex: lineIndex,
          geometry: geometry,
        )) {
      startLine--;
    }

    var endLineExclusive = line + 1;
    while (endLineExclusive < lineIndex.lineCount &&
        _isQuoteLineOutsideFences(
          text: text,
          line: endLineExclusive,
          lineIndex: lineIndex,
          geometry: geometry,
        )) {
      endLineExclusive++;
    }

    int firstContentLine = -1;
    int lastContentLine = -1;
    for (var i = startLine; i < endLineExclusive; i++) {
      if (isQuoteLineBodyBlank(text: text, line: i, lineIndex: lineIndex)) {
        continue;
      }
      firstContentLine = firstContentLine == -1 ? i : firstContentLine;
      lastContentLine = i;
    }

    if (firstContentLine == -1) {
      firstContentLine = startLine;
      lastContentLine = endLineExclusive - 1;
    }

    return structure.QuoteContext(
      startLine: startLine,
      endLineExclusive: endLineExclusive,
      firstContentLine: firstContentLine,
      lastContentLine: lastContentLine,
    );
  }

  bool isQuoteLineBodyBlank({
    required String text,
    required int line,
    required LineIndex lineIndex,
  }) {
    if (line < 0 || line >= lineIndex.lineCount) return false;
    final lineStart = lineIndex.offsetAtLine(line);
    final lineEndWithBreak = NavigationLineUtils.lineEndWithBreak(
      lineIndex,
      text,
      line,
    );
    final lineEnd = NavigationLineUtils.lineContentEnd(
      text,
      lineStart,
      lineEndWithBreak,
    );
    final markerLen = ProjectionRangeUtils.blockquoteMarkerLength(
      text,
      lineStart,
      lineEnd,
    );
    if (markerLen <= 0) return false;

    for (var i = lineStart + markerLen; i < lineEnd; i++) {
      final cu = text.codeUnitAt(i);
      if (cu == 32 || cu == 9) continue;
      return false;
    }
    return true;
  }

  bool isLineInsideFencedGeometry({
    required int lineStartOffset,
    required GeometryModel geometry,
  }) {
    for (final block in geometry.codeBlocks) {
      if (lineStartOffset >= block.startOffset &&
          lineStartOffset < block.endOffset) {
        return true;
      }
    }
    return false;
  }

  bool shouldExitBlockquoteOnArrowDown({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
    required LineIndex lineIndex,
    required GeometryModel geometry,
  }) {
    if (toLine >= context.endLineExclusive) return true;

    if (context.lastContentLine != fromLine) return false;
    if (fromLine + 1 >= context.endLineExclusive) return true;

    for (var line = fromLine + 1; line < context.endLineExclusive; line++) {
      if (!_isQuoteLineOutsideFences(
        text: text,
        line: line,
        lineIndex: lineIndex,
        geometry: geometry,
      )) {
        return false;
      }
      if (!isQuoteLineBodyBlank(text: text, line: line, lineIndex: lineIndex)) {
        return false;
      }
    }
    return true;
  }

  bool shouldExitBlockquoteOnArrowUp({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
    required LineIndex lineIndex,
    required GeometryModel geometry,
  }) {
    if (toLine < context.startLine) return true;

    if (context.firstContentLine != fromLine) return false;
    if (fromLine - 1 < context.startLine) return true;

    for (var line = context.startLine; line < fromLine; line++) {
      if (!_isQuoteLineOutsideFences(
        text: text,
        line: line,
        lineIndex: lineIndex,
        geometry: geometry,
      )) {
        return false;
      }
      if (!isQuoteLineBodyBlank(text: text, line: line, lineIndex: lineIndex)) {
        return false;
      }
    }
    return true;
  }

  bool isUnclosedFenceAtEof({
    required String text,
    required MeasuredBlock block,
  }) {
    if (block.endOffset != text.length) return false;
    if (block.endOffset <= 0) return true;
    final closeLineStart = ProjectionRangeUtils.lineStartForOffset(
      text,
      block.endOffset - 1,
    );
    final hasClosingFence = closeLineStart != block.startOffset &&
        closeLineStart + 3 <= text.length &&
        text.startsWith('```', closeLineStart);
    return !hasClosingFence;
  }

  bool shouldExitFenceOnArrowDown({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
    required LineIndex lineIndex,
  }) {
    final closeLine = context.closeLine;
    if (closeLine == null) return false;

    if (toLine == closeLine) return true;

    final lastContentLine = _lastNonWhitespaceLine(
      text: text,
      openLine: context.openLine,
      closeLine: closeLine,
      lineIndex: lineIndex,
    );
    if (lastContentLine == -1 || lastContentLine != fromLine) {
      return false;
    }
    if (fromLine + 1 >= closeLine) return true;

    for (var line = fromLine + 1; line < closeLine; line++) {
      final start = lineIndex.offsetAtLine(line);
      final end = (line + 1 < lineIndex.lineCount)
          ? lineIndex.offsetAtLine(line + 1)
          : text.length;
      if (!NavigationLineUtils.isWhitespaceLine(text, start, end)) {
        return false;
      }
    }
    return true;
  }

  bool shouldExitFenceOnArrowUp({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
    required LineIndex lineIndex,
  }) {
    if (toLine == context.openLine) return true;

    final firstContentLine = _firstNonWhitespaceLine(
      text: text,
      openLine: context.openLine,
      closeLineExclusive: context.closeLineExclusive,
      lineIndex: lineIndex,
    );
    if (firstContentLine == -1 || firstContentLine != fromLine) {
      return false;
    }
    if (fromLine - 1 <= context.openLine) return true;

    for (var line = context.openLine + 1; line < fromLine; line++) {
      final start = lineIndex.offsetAtLine(line);
      final end = (line + 1 < lineIndex.lineCount)
          ? lineIndex.offsetAtLine(line + 1)
          : text.length;
      if (!NavigationLineUtils.isWhitespaceLine(text, start, end)) {
        return false;
      }
    }
    return true;
  }

  FenceEnterExitResult? computeFenceExitOnEnter({
    required String text,
    required int caret,
    required structure.FenceContext context,
    required LineIndex lineIndex,
  }) {
    if (!context.hasClosingFence) {
      // Unclosed fence: pressing Enter on a blank EOF line exits.
      final caretLine = lineIndex.lineAtOffset(caret);
      final lineStart = lineIndex.offsetAtLine(caretLine);
      final lineEnd = (caretLine + 1 < lineIndex.lineCount)
          ? lineIndex.offsetAtLine(caretLine + 1)
          : text.length;
      if (!NavigationLineUtils.isWhitespaceLine(text, lineStart, lineEnd)) {
        return null;
      }
      if (!NavigationLineUtils.isWhitespaceLine(text, lineEnd, text.length)) {
        return null;
      }

      // Trim all trailing blank lines in the unclosed fence body, then close.
      final trimStart = trailingBlankTrimStart(
        text: text,
        openLine: context.openLine,
        closeLineExclusive: lineIndex.lineCount,
        lineIndex: lineIndex,
      );
      var prefix = text.substring(0, trimStart.clamp(0, text.length));
      if (prefix.isNotEmpty && prefix.codeUnitAt(prefix.length - 1) != 10) {
        prefix = '$prefix\n';
      }
      final exitText = '$prefix```\n';
      return FenceEnterExitResult(text: exitText, caret: exitText.length);
    }

    final closeLine = context.closeLine;
    if (closeLine == null || closeLine == 0) return null;
    final caretLine = lineIndex.lineAtOffset(caret);
    final closeLineStart = lineIndex.offsetAtLine(closeLine);
    final trimStart = trailingBlankTrimStart(
      text: text,
      openLine: context.openLine,
      closeLineExclusive: closeLine,
      lineIndex: lineIndex,
    );
    if (trimStart >= closeLineStart) return null;

    // Allow Enter-to-exit from any trailing blank line immediately before the
    // closing fence, not only the last blank line.
    final firstTrailingBlankLine = lineIndex.lineAtOffset(trimStart);
    if (caretLine < firstTrailingBlankLine || caretLine >= closeLine) {
      return null;
    }
    final caretLineStart = lineIndex.offsetAtLine(caretLine);
    final caretLineEnd = (caretLine + 1 < lineIndex.lineCount)
        ? lineIndex.offsetAtLine(caretLine + 1)
        : text.length;
    if (!NavigationLineUtils.isWhitespaceLine(
      text,
      caretLineStart,
      caretLineEnd,
    )) {
      return null;
    }

    final deletedLen = closeLineStart - trimStart;
    var exitText = text.replaceRange(trimStart, closeLineStart, '');
    var exitCaret = context.block.endOffset - deletedLen;
    // If the closing fence is at EOF without a trailing newline, exiting on
    // Enter should still place the caret on a fresh line outside the fence.
    final closesAtEof = context.block.endOffset == text.length;
    final hasTrailingNewline =
        exitText.isNotEmpty && exitText.codeUnitAt(exitText.length - 1) == 10;
    if (closesAtEof && !hasTrailingNewline) {
      exitText = '$exitText\n';
      exitCaret = exitText.length;
    }

    return FenceEnterExitResult(text: exitText, caret: exitCaret);
  }

  String? fenceLanguageForBlock({
    required String text,
    required int blockStartOffset,
  }) {
    if (blockStartOffset < 0 ||
        blockStartOffset + 3 > text.length ||
        !text.startsWith('```', blockStartOffset)) {
      return null;
    }

    final openLineEnd = FencedCodeScanner.endOfLine(text, blockStartOffset);
    final openLineContentEnd =
        (openLineEnd > 0 && text.codeUnitAt(openLineEnd - 1) == 10)
            ? openLineEnd - 1
            : openLineEnd;
    final infoStart = (blockStartOffset + 3).clamp(0, text.length);
    if (infoStart >= openLineContentEnd) return null;

    int tokenStart = infoStart;
    while (tokenStart < openLineContentEnd) {
      final cu = text.codeUnitAt(tokenStart);
      if (cu != 32 && cu != 9) break;
      tokenStart++;
    }
    if (tokenStart >= openLineContentEnd) return null;

    int tokenEnd = tokenStart;
    while (tokenEnd < openLineContentEnd) {
      final cu = text.codeUnitAt(tokenEnd);
      if (cu == 32 || cu == 9) break;
      tokenEnd++;
    }
    if (tokenStart >= tokenEnd) return null;

    final raw = text.substring(tokenStart, tokenEnd).trim().toLowerCase();
    if (raw.isEmpty) return null;
    return SovereignCodeHighlighter.normalizeFenceTag(raw) ?? raw;
  }

  String preferredOutdentUnitForLine({
    required String text,
    required MeasuredBlock block,
    required int line,
    required String currentIndent,
    required LineIndex lineIndex,
  }) {
    if (currentIndent.isNotEmpty && currentIndent.codeUnitAt(0) == 9) {
      return '\t';
    }
    if (!NavigationLineUtils.isHorizontalWhitespaceOnly(currentIndent)) {
      return FenceEditingUtils.preferredIndentUnit(currentIndent);
    }

    final currentWidth = currentIndent.length;
    if (currentWidth <= 0) {
      return FenceEditingUtils.preferredIndentUnit(currentIndent);
    }

    final openLine = lineIndex.lineAtOffset(block.startOffset);
    for (var scan = line - 1; scan > openLine; scan--) {
      final start = lineIndex.offsetAtLine(scan);
      final end = (scan + 1 < lineIndex.lineCount)
          ? lineIndex.offsetAtLine(scan + 1)
          : text.length;
      if (NavigationLineUtils.isWhitespaceLine(text, start, end)) continue;

      final eol = FencedCodeScanner.endOfLine(text, start);
      final lineText = text.substring(start, eol);
      final prevIndent = NavigationLineUtils.leadingWhitespacePrefix(lineText);
      if (!NavigationLineUtils.isHorizontalWhitespaceOnly(prevIndent)) {
        continue;
      }

      final prevWidth = prevIndent.length;
      if (prevWidth >= currentWidth) continue;

      final delta = currentWidth - prevWidth;
      if (delta <= 0 || delta > currentWidth) continue;
      return List.filled(delta, ' ').join();
    }

    return FenceEditingUtils.preferredIndentUnit(currentIndent);
  }

  int trailingBlankTrimStart({
    required String text,
    required int openLine,
    required int closeLineExclusive,
    required LineIndex lineIndex,
  }) {
    final lastContentLine = _lastNonWhitespaceLine(
      text: text,
      openLine: openLine,
      closeLine: closeLineExclusive,
      lineIndex: lineIndex,
    );
    final trimLine = lastContentLine == -1 ? openLine + 1 : lastContentLine + 1;
    if (trimLine <= 0) return 0;
    if (trimLine >= lineIndex.lineCount) return text.length;
    return lineIndex.offsetAtLine(trimLine);
  }

  bool _isQuoteLineOutsideFences({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required GeometryModel geometry,
  }) {
    final lineStart = lineIndex.offsetAtLine(line);
    if (isLineInsideFencedGeometry(
      lineStartOffset: lineStart,
      geometry: geometry,
    )) {
      return false;
    }

    final lineEndWithBreak = NavigationLineUtils.lineEndWithBreak(
      lineIndex,
      text,
      line,
    );
    final lineEnd = NavigationLineUtils.lineContentEnd(
      text,
      lineStart,
      lineEndWithBreak,
    );
    return ProjectionRangeUtils.blockquoteMarkerLength(
          text,
          lineStart,
          lineEnd,
        ) >
        0;
  }

  int _lastNonWhitespaceLine({
    required String text,
    required int openLine,
    required int closeLine,
    required LineIndex lineIndex,
  }) {
    for (var line = closeLine - 1; line > openLine; line--) {
      final start = lineIndex.offsetAtLine(line);
      final end = (line + 1 < lineIndex.lineCount)
          ? lineIndex.offsetAtLine(line + 1)
          : text.length;
      if (!NavigationLineUtils.isWhitespaceLine(text, start, end)) return line;
    }
    return -1;
  }

  int _firstNonWhitespaceLine({
    required String text,
    required int openLine,
    required int closeLineExclusive,
    required LineIndex lineIndex,
  }) {
    for (var line = openLine + 1; line < closeLineExclusive; line++) {
      final start = lineIndex.offsetAtLine(line);
      final end = (line + 1 < lineIndex.lineCount)
          ? lineIndex.offsetAtLine(line + 1)
          : text.length;
      if (!NavigationLineUtils.isWhitespaceLine(text, start, end)) return line;
    }
    return -1;
  }
}
