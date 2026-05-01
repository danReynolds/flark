import 'package:flutter/services.dart';

import 'fenced_code_scanner.dart';
import 'markdown_marker_grammar.dart';
import 'sovereign_code_highlighter.dart';

/// Shared markdown marker extraction used by controller projection and engine
/// snapshots to keep hiding/render behavior consistent.
abstract final class SovereignMarkdownMarkers {
  static int lineStartForOffset(String text, int offset) {
    if (text.isEmpty) return 0;
    int safe = offset.clamp(0, text.length - 1);
    // If offset points at '\n', treat it as the previous line's end.
    if (text.codeUnitAt(safe) == 10 && safe > 0) {
      safe--;
    }
    final idx = text.lastIndexOf('\n', safe);
    return idx == -1 ? 0 : idx + 1;
  }

  static List<TextRange> fencedCodeFenceMarkers(
    String text,
    List<FencedCodeBlock> fencedBlocks,
  ) {
    final markers = <TextRange>[];

    for (final block in fencedBlocks) {
      final start = block.start;
      if (start + 3 <= text.length && text.startsWith('```', start)) {
        markers.add(TextRange(start: start, end: start + 3));

        final openLineEnd = FencedCodeScanner.endOfLine(text, start);
        final openLineContentEnd =
            (openLineEnd > 0 && text.codeUnitAt(openLineEnd - 1) == 10)
                ? openLineEnd - 1
                : openLineEnd;
        final infoStart = (start + 3).clamp(0, text.length);
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
            final token =
                text.substring(tokenStart, tokenEnd).trim().toLowerCase();
            final normalized = SovereignCodeHighlighter.normalizeFenceTag(
              token,
            );
            if (normalized != null) {
              var hideEnd = tokenEnd;
              if (hideEnd < openLineContentEnd) {
                final cu = text.codeUnitAt(hideEnd);
                if (cu == 32 || cu == 9) hideEnd++;
              }
              markers.add(TextRange(start: infoStart, end: hideEnd));
            }
          }
        }
      }

      if (block.end <= 0 || block.end > text.length) continue;
      final closeLineStart = lineStartForOffset(text, block.end - 1);
      if (closeLineStart != start &&
          closeLineStart + 3 <= text.length &&
          text.startsWith('```', closeLineStart)) {
        markers.add(TextRange(start: closeLineStart, end: closeLineStart + 3));

        final closeLineEnd = FencedCodeScanner.endOfLine(text, closeLineStart);
        final closeLineContentEnd =
            (closeLineEnd > 0 && text.codeUnitAt(closeLineEnd - 1) == 10)
                ? closeLineEnd - 1
                : closeLineEnd;
        final closeInfoStart = (closeLineStart + 3).clamp(0, text.length);
        if (closeInfoStart < closeLineContentEnd) {
          markers.add(
            TextRange(start: closeInfoStart, end: closeLineContentEnd),
          );
        }
      }
    }

    return markers;
  }

  static List<TextRange> markdownBlockMarkerRanges(
    String text, {
    List<FencedCodeBlock>? fencedBlocks,
  }) {
    if (text.isEmpty) return const [];

    final blocks = fencedBlocks ?? FencedCodeScanner.scan(text);
    final markers = <TextRange>[];
    int fenceIndex = 0;
    int lineStart = 0;

    while (lineStart < text.length) {
      final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
      final lineEnd =
          (lineEndWithBreak > 0 && text.codeUnitAt(lineEndWithBreak - 1) == 10)
              ? lineEndWithBreak - 1
              : lineEndWithBreak;

      while (
          fenceIndex < blocks.length && blocks[fenceIndex].end <= lineStart) {
        fenceIndex++;
      }
      final inFence = fenceIndex < blocks.length &&
          lineStart >= blocks[fenceIndex].start &&
          lineStart < blocks[fenceIndex].end;
      if (!inFence && lineEnd > lineStart) {
        final line = text.substring(lineStart, lineEnd);
        final heading = MarkdownMarkerGrammar.matchAtxHeading(
          line,
          dialect: MarkdownMarkerDialect.sovereignV1,
        );
        if (heading != null) {
          final markerEnd = (heading.markerStartIndex + heading.level + 1)
              .clamp(0, line.length);
          markers.add(
            TextRange(
              start: lineStart + heading.markerStartIndex,
              end: lineStart + markerEnd,
            ),
          );
        } else {
          final blockquoteEnd = MarkdownMarkerGrammar.matchBlockquoteMarkerEnd(
            line,
            dialect: MarkdownMarkerDialect.sovereignV1,
          );
          if (blockquoteEnd != null) {
            markers.add(
              TextRange(start: lineStart, end: lineStart + blockquoteEnd),
            );
          }
        }

        final listMarker = listMarkerRangeForLineAllowingQuotePrefix(
          text,
          lineStart,
          lineEnd,
        );
        if (listMarker != null) {
          markers.add(listMarker);
          final taskMarker = taskCheckboxMarkerRangeForLine(
            text,
            lineStart: lineStart,
            lineEnd: lineEnd,
            listMarker: listMarker,
          );
          if (taskMarker != null) {
            markers.add(taskMarker);
          }
        }

        final refDefMarker =
            MarkdownMarkerGrammar.matchReferenceDefinitionMarkerRange(
          line,
          dialect: MarkdownMarkerDialect.commonMark,
        );
        if (refDefMarker != null) {
          markers.add(
            TextRange(
              start: lineStart + refDefMarker.start,
              end: lineStart + refDefMarker.end,
            ),
          );
        }

        final thematicBreakMarker =
            MarkdownMarkerGrammar.matchThematicBreakMarkerRange(
          line,
          dialect: MarkdownMarkerDialect.commonMark,
        );
        if (thematicBreakMarker != null) {
          markers.add(
            TextRange(
              start: lineStart + thematicBreakMarker.start,
              end: lineStart + thematicBreakMarker.end,
            ),
          );
        }
      }

      lineStart =
          lineEndWithBreak > lineStart ? lineEndWithBreak : lineStart + 1;
    }

    return markers;
  }

  static TextRange? listMarkerRangeForLineAllowingQuotePrefix(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    if (lineEnd <= lineStart) return null;
    final line = text.substring(lineStart, lineEnd);
    final inLine =
        MarkdownMarkerGrammar.listMarkerRangeInLineAllowingQuotePrefix(
      line,
      from: 0,
      dialect: MarkdownMarkerDialect.sovereignV1,
    );
    if (inLine == null) return null;
    return TextRange(
      start: lineStart + inLine.start,
      end: lineStart + inLine.end,
    );
  }

  static TextRange? taskCheckboxMarkerRangeForLine(
    String text, {
    required int lineStart,
    required int lineEnd,
    TextRange? listMarker,
  }) {
    final marker = listMarker ??
        listMarkerRangeForLineAllowingQuotePrefix(text, lineStart, lineEnd);
    if (marker == null) return null;
    final start = marker.end;
    if (start + 4 > lineEnd) return null;
    if (text.codeUnitAt(start) != 91) return null; // [
    final state = text.codeUnitAt(start + 1);
    if (text.codeUnitAt(start + 2) != 93) return null; // ]
    if (text.codeUnitAt(start + 3) != 32) return null; // space
    final isChecked = state == 120 || state == 88; // x/X
    final isUnchecked = state == 32;
    if (!isChecked && !isUnchecked) return null;
    return TextRange(start: start, end: start + 4);
  }
}
