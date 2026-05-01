import 'package:flutter/services.dart';

import '../../engine/syntax_types.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_markdown_markers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_style_scanner.dart';
import '../structure/markdown_line_helpers.dart';

class ProjectionRangeUtils {
  const ProjectionRangeUtils._();

  static List<TextRange> normalizeHiddenRanges(
    Iterable<TextRange> ranges,
    int textLength,
  ) {
    final sanitized = <TextRange>[];
    for (final range in ranges) {
      final start = range.start.clamp(0, textLength).toInt();
      final end = range.end.clamp(0, textLength).toInt();
      if (end <= start) continue;
      sanitized.add(TextRange(start: start, end: end));
    }
    sanitized.sort((a, b) {
      final byStart = a.start.compareTo(b.start);
      if (byStart != 0) return byStart;
      return a.end.compareTo(b.end);
    });

    final normalized = <TextRange>[];
    for (final range in sanitized) {
      if (normalized.isEmpty) {
        normalized.add(range);
        continue;
      }
      final last = normalized.last;
      if (range.start < last.end) continue;
      if (range.start == last.start && range.end == last.end) continue;
      normalized.add(range);
    }
    return normalized;
  }

  static int rangeKey(TextRange range) =>
      (range.start.toUnsigned(32) << 32) ^ range.end.toUnsigned(32);

  static List<TextRange> overlayCanonicalBlockMarkerRanges(
    String text,
    Iterable<TextRange> ranges,
  ) {
    if (text.isEmpty) return const <TextRange>[];

    final fencedBlocks = FencedCodeScanner.scan(text);
    final canonical = <TextRange>[
      ...fencedCodeFenceMarkers(text, fencedBlocks),
      ...markdownBlockMarkerRanges(text, fencedBlocks: fencedBlocks),
    ];
    if (canonical.isEmpty) {
      return normalizeHiddenRanges(ranges, text.length);
    }

    final canonicalByStart = <int, int>{};
    for (final range in canonical) {
      final existingEnd = canonicalByStart[range.start];
      if (existingEnd == null || range.end > existingEnd) {
        canonicalByStart[range.start] = range.end;
      }
    }

    final filtered = <TextRange>[];
    for (final range in ranges) {
      final canonicalEnd = canonicalByStart[range.start];
      if (canonicalEnd != null && canonicalEnd > range.end) {
        continue;
      }
      filtered.add(range);
    }

    filtered.addAll(canonical);
    return normalizeHiddenRanges(filtered, text.length);
  }

  static List<StyleRun> styleRunsFromInlineTokens(
    List<InlineSpanToken> tokens,
    int textLength,
  ) {
    if (tokens.isEmpty || textLength <= 0) return const [];
    final runs = <StyleRun>[];
    for (final token in tokens) {
      final start = token.start.clamp(0, textLength).toInt();
      final end = token.end.clamp(0, textLength).toInt();
      if (end <= start) continue;
      runs.add(StyleRun(start, end, token.style));
    }
    runs.sort((a, b) {
      final byStart = a.start.compareTo(b.start);
      if (byStart != 0) return byStart;
      return a.end.compareTo(b.end);
    });
    return runs;
  }

  static List<TextRange> stabilizeRangesInAmbiguity({
    required List<TextRange> predictedRanges,
    required List<TextRange> authoritativeRanges,
    required List<TextRange> ambiguityZones,
    required int textLength,
  }) {
    if (ambiguityZones.isEmpty || authoritativeRanges.isEmpty) {
      return normalizeHiddenRanges(predictedRanges, textLength);
    }

    final stabilized = <TextRange>[];
    for (final range in predictedRanges) {
      if (_rangeIntersectsAny(range, ambiguityZones)) continue;
      stabilized.add(range);
    }
    for (final range in authoritativeRanges) {
      if (!_rangeIntersectsAny(range, ambiguityZones)) continue;
      stabilized.add(range);
    }

    return normalizeHiddenRanges(stabilized, textLength);
  }

  static int lineStartForOffset(String text, int offset) =>
      MarkdownLineHelpers.lineStartForOffset(text, offset);

  static List<TextRange> fencedCodeFenceMarkers(
    String text,
    List<FencedCodeBlock> fencedBlocks,
  ) {
    return SovereignMarkdownMarkers.fencedCodeFenceMarkers(text, fencedBlocks);
  }

  static List<TextRange> markdownBlockMarkerRanges(
    String text, {
    List<FencedCodeBlock>? fencedBlocks,
  }) {
    return SovereignMarkdownMarkers.markdownBlockMarkerRanges(
      text,
      fencedBlocks: fencedBlocks,
    );
  }

  static int headerMarkerLength(String text, int lineStart, int lineEnd) =>
      MarkdownLineHelpers.headerMarkerLength(text, lineStart, lineEnd);

  static int blockquoteMarkerLength(String text, int lineStart, int lineEnd) =>
      MarkdownLineHelpers.blockquoteMarkerLength(text, lineStart, lineEnd);

  static int unorderedListMarkerLength(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    return MarkdownLineHelpers.unorderedListMarkerLength(
      text,
      lineStart,
      lineEnd,
    );
  }

  static int orderedListMarkerLength(String text, int lineStart, int lineEnd) =>
      MarkdownLineHelpers.orderedListMarkerLength(text, lineStart, lineEnd);

  static String? inlineMarkerToken(String text, TextRange range) =>
      MarkdownLineHelpers.inlineMarkerToken(text, range);

  static List<TextRange> selectionCenteredEmptyInlineRanges(
    TextEditingValue value,
  ) {
    return MarkdownLineHelpers.selectionCenteredEmptyInlineRanges(value);
  }

  static bool isFenceLineTrailingRange(String text, TextRange range) {
    // A fence "trailing" range starts immediately after ``` at column 0 and
    // ends at end-of-line content (right before '\n' or EOF). We use this to
    // make the info string editable even when the caret is at the end boundary.
    final start = range.start;
    if (start < 3) return false;

    final fenceStart = start - 3;
    if (fenceStart < 0 || fenceStart + 3 > text.length) return false;
    if (!text.startsWith('```', fenceStart)) return false;

    // Only treat as a fence if it begins at column 0.
    if (fenceStart != 0 && text.codeUnitAt(fenceStart - 1) != 10) {
      return false;
    }

    final end = range.end;
    if (end < start || end > text.length) return false;
    if (end == text.length) return true;

    return text.codeUnitAt(end) == 10; // '\n'
  }

  static bool isFenceMarkerRange(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    if (end - start != 3) return false;
    if (start < 0 || end > text.length) return false;
    if (!text.startsWith('```', start)) return false;
    if (start == 0) return true;
    return text.codeUnitAt(start - 1) == 10; // '\n'
  }

  static bool _rangeIntersectsAny(TextRange range, List<TextRange> zones) {
    for (final zone in zones) {
      if (_rangesOverlap(range, zone)) return true;
    }
    return false;
  }

  static bool _rangesOverlap(TextRange a, TextRange b) {
    return a.start < b.end && b.start < a.end;
  }
}
