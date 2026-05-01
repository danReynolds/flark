import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import 'fenced_code_scanner.dart';
import 'sovereign_code_highlighter.dart';

/// Synchronous scanner for detecting Fenced Code Blocks.
///
/// This is a "Tier 1" operation. It scans the text in the same transaction
/// as the edit to provide authoritative geometry for the Painter.
///
/// **Contracts (RFC 007)**:
/// 1. **Subset Equivalence**: It implements the exact same V1 "Column 0" dialect
///    as the async parser:
///    - Start: `^` ` ``` ` (3 backticks strict)
///    - End: `\n` ` ``` `
///    - Info Strings: Allowed (consumed)
///    - Interior: ` ``` ` inside a line does NOT close.
/// 2. **Performance Budget**: Target < 0.5ms. Budget is monitored in benchmark
///    tests; this synchronous scanner intentionally stays deterministic and does
///    not degrade to a local fallback path.
class SovereignGeometryScanner {
  const SovereignGeometryScanner();

  /// Scans the entire [text] for fenced code blocks.
  GeometryModel scan(String text, LineIndex lineIndex) {
    if (text.isEmpty) return GeometryModel.empty;

    final blocks = <MeasuredBlock>[];
    final quoteBlocks = <MeasuredBlock>[];
    final fences = FencedCodeScanner.scan(text);

    for (final fence in fences) {
      final startOffset = fence.start;
      final endOffset = fence.end;

      // 3. Compute Measured Geometry (RFC 007 - 7.1)
      final startLine = lineIndex.lineAtOffset(startOffset);

      // Base endLine: The line containing the last character of the block.
      // We use (endOffset - 1) because endOffset is exclusive.
      // If endOffset > 0, we look at the char before it.
      // endLine (for painting loop validity) is +1 of the line index.
      int endLine =
          endOffset > 0 ? lineIndex.lineAtOffset(endOffset - 1) + 1 : 0;

      // Trailing Empty Line Rule:
      // If the last character was a newline, the block conceptually extends
      // to include the *newly created* empty line (which isn't covered by endOffset-1).
      //
      // Apply this only for unclosed fences. For closed fences, extending one
      // extra line makes the line after the closing ``` look like it is still
      // inside code, which causes an "extra Enter to exit" UX.
      final hasClosingFence = _hasClosingFence(text, startOffset, endOffset);
      if (!hasClosingFence &&
          endOffset > 0 &&
          endOffset <= text.length &&
          text.codeUnitAt(endOffset - 1) == 10) {
        endLine++;
      }

      // Paint only visible body lines. Keep source geometry unchanged so
      // editing policies still reason against opener/closer lines.
      //
      // If opener trailing text is visible (unknown info string, or extra
      // content after a recognized tag), include opener line in paint extent.
      final openerLineVisible = _openerLineHasVisibleContent(text, startOffset);
      final openerOnlyUnclosedLine =
          !hasClosingFence && !openerLineVisible && endLine == startLine + 1;
      final paintStartLine = (openerLineVisible || openerOnlyUnclosedLine)
          ? startLine
          : startLine + 1;
      var paintEndLine = hasClosingFence ? endLine - 1 : endLine;
      if (paintEndLine <= paintStartLine) {
        paintEndLine = paintStartLine + 1;
      }

      blocks.add(
        MeasuredBlock(
          startOffset: startOffset,
          endOffset: endOffset,
          startLine: startLine,
          endLine: endLine,
          paintStartLine: paintStartLine,
          paintEndLine: paintEndLine,
        ),
      );
    }

    int fenceIndex = 0;
    int lineStart = 0;
    int? quoteStartOffset;
    int? quoteEndOffset;

    while (lineStart < text.length) {
      while (
          fenceIndex < fences.length && fences[fenceIndex].end <= lineStart) {
        fenceIndex++;
      }
      final inFence = fenceIndex < fences.length &&
          lineStart >= fences[fenceIndex].start &&
          lineStart < fences[fenceIndex].end;

      final lineEnd = FencedCodeScanner.endOfLine(text, lineStart);
      final isQuoteLine = !inFence &&
          lineStart + 1 <= text.length &&
          lineStart < lineEnd &&
          text.codeUnitAt(lineStart) == 62 && // '>'
          lineStart + 1 < lineEnd &&
          text.codeUnitAt(lineStart + 1) == 32; // ' '

      if (isQuoteLine) {
        quoteStartOffset ??= lineStart;
        quoteEndOffset = lineEnd;
      } else if (quoteStartOffset != null && quoteEndOffset != null) {
        final startLine = lineIndex.lineAtOffset(quoteStartOffset);
        final endLine = quoteEndOffset > 0
            ? lineIndex.lineAtOffset(quoteEndOffset - 1) + 1
            : startLine + 1;
        quoteBlocks.add(
          MeasuredBlock(
            startOffset: quoteStartOffset,
            endOffset: quoteEndOffset,
            startLine: startLine,
            endLine: endLine,
          ),
        );
        quoteStartOffset = null;
        quoteEndOffset = null;
      }

      lineStart = lineEnd > lineStart ? lineEnd : lineStart + 1;
    }

    if (quoteStartOffset != null && quoteEndOffset != null) {
      final startLine = lineIndex.lineAtOffset(quoteStartOffset);
      final endLine = quoteEndOffset > 0
          ? lineIndex.lineAtOffset(quoteEndOffset - 1) + 1
          : startLine + 1;
      quoteBlocks.add(
        MeasuredBlock(
          startOffset: quoteStartOffset,
          endOffset: quoteEndOffset,
          startLine: startLine,
          endLine: endLine,
        ),
      );
    }

    return GeometryModel(codeBlocks: blocks, quoteBlocks: quoteBlocks);
  }

  static bool _hasClosingFence(String text, int startOffset, int endOffset) {
    if (endOffset <= 0 || endOffset > text.length) return false;
    final closeLineStart = _lineStartForOffset(text, endOffset - 1);
    return closeLineStart != startOffset &&
        closeLineStart + 3 <= text.length &&
        text.startsWith('```', closeLineStart);
  }

  static bool _openerLineHasVisibleContent(String text, int startOffset) {
    if (startOffset < 0 ||
        startOffset + 3 > text.length ||
        !text.startsWith('```', startOffset)) {
      return false;
    }

    final openLineEnd = FencedCodeScanner.endOfLine(text, startOffset);
    final openLineContentEnd =
        (openLineEnd > 0 && text.codeUnitAt(openLineEnd - 1) == 10)
            ? openLineEnd - 1
            : openLineEnd;
    final infoStart = (startOffset + 3).clamp(0, text.length);

    var hiddenTokenEnd = infoStart;
    if (infoStart < openLineContentEnd) {
      int tokenStart = infoStart;
      while (tokenStart < openLineContentEnd) {
        final cu = text.codeUnitAt(tokenStart);
        if (cu != 32 && cu != 9) break;
        tokenStart++;
      }
      int tokenEnd = tokenStart;
      while (tokenEnd < openLineContentEnd) {
        final cu = text.codeUnitAt(tokenEnd);
        if (cu == 32 || cu == 9) break;
        tokenEnd++;
      }
      if (tokenStart < tokenEnd) {
        final token = text.substring(tokenStart, tokenEnd).trim().toLowerCase();
        final normalized = SovereignCodeHighlighter.normalizeFenceTag(token);
        if (normalized != null) {
          hiddenTokenEnd = tokenEnd;
          if (hiddenTokenEnd < openLineContentEnd) {
            final cu = text.codeUnitAt(hiddenTokenEnd);
            if (cu == 32 || cu == 9) hiddenTokenEnd++;
          }
        }
      }
    }

    // Marker ticks are always hidden. A recognized info string is also hidden.
    for (var i = startOffset + 3; i < openLineContentEnd; i++) {
      if (i < hiddenTokenEnd) continue;
      final cu = text.codeUnitAt(i);
      if (cu == 32 || cu == 9) continue;
      return true;
    }
    return false;
  }

  static int _lineStartForOffset(String text, int offset) {
    if (text.isEmpty) return 0;
    int safe = offset.clamp(0, text.length - 1);
    if (text.codeUnitAt(safe) == 10 && safe > 0) {
      safe--;
    }
    final idx = text.lastIndexOf('\n', safe);
    return idx == -1 ? 0 : idx + 1;
  }
}
