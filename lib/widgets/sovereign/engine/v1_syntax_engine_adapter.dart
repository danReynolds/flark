import 'package:flutter/services.dart';

import '../logic/block_parser.dart';
import '../logic/fenced_code_scanner.dart';
import '../logic/sovereign_markdown_markers.dart';
import '../logic/sovereign_style_scanner.dart';
import '../models/block_tree.dart';
import 'syntax_engine.dart';
import 'syntax_snapshot.dart';
import 'syntax_types.dart';

/// Adapter that exposes Sovereign V1 syntax behavior through the new
/// [SyntaxEngine] boundary.
class V1SyntaxEngineAdapter implements SyntaxEngine {
  final int predictiveScanTimeBudgetMicros;
  final int predictiveScanSpanBudget;

  const V1SyntaxEngineAdapter({
    this.predictiveScanTimeBudgetMicros =
        SovereignStyleScanner.kTimeBudgetMicros,
    this.predictiveScanSpanBudget = SovereignStyleScanner.kSpanBudget,
  });

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    final text = request.text;
    final tree = BlockParser.parse(text);
    final fencedBlocks = FencedCodeScanner.scan(text);
    final fenceExclusionRanges = _normalizeRanges(
      fencedBlocks.map((b) => TextRange(start: b.start, end: b.end)),
      text.length,
    );
    final fenceMarkerRanges = SovereignMarkdownMarkers.fencedCodeFenceMarkers(
      text,
      fencedBlocks,
    );
    final blockMarkerRanges =
        SovereignMarkdownMarkers.markdownBlockMarkerRanges(
      text,
      fencedBlocks: fencedBlocks,
    );
    final inlineExcludedRanges = _normalizeRanges(<TextRange>[
      ...fenceExclusionRanges,
      ...fenceMarkerRanges,
      ...blockMarkerRanges,
    ], text.length);
    final scanResult = SovereignStyleScanner.scan(
      text,
      excludedRanges: inlineExcludedRanges,
    );
    final markerRanges = _buildMarkerRanges(
      text: text,
      fenceMarkerRanges: fenceMarkerRanges,
      blockMarkerRanges: blockMarkerRanges,
      inlineRuns: scanResult.runs,
    );

    return SyntaxSnapshot(
      revision: request.revision,
      blocks: _toBlockSpans(tree),
      inlineTokens: [
        for (final run in scanResult.runs)
          InlineSpanToken(style: run.style, start: run.start, end: run.end),
      ],
      markerRanges: markerRanges,
      exclusionRanges: fenceExclusionRanges,
      ambiguityZones: const [],
      cursorMask: HiddenRangeCursorValidationMask(
        textLength: text.length,
        hiddenRanges: markerRanges,
      ),
      diagnostics: const [],
    );
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    final text = request.text;
    final fencedBlocks = FencedCodeScanner.scan(text);
    final fenceExclusionRanges = _normalizeRanges(
      fencedBlocks.map((b) => TextRange(start: b.start, end: b.end)),
      text.length,
    );
    final fenceMarkerRanges = SovereignMarkdownMarkers.fencedCodeFenceMarkers(
      text,
      fencedBlocks,
    );
    final blockMarkerRanges =
        SovereignMarkdownMarkers.markdownBlockMarkerRanges(
      text,
      fencedBlocks: fencedBlocks,
    );
    final inlineExcludedRanges = _normalizeRanges(<TextRange>[
      ...fenceExclusionRanges,
      ...fenceMarkerRanges,
      ...blockMarkerRanges,
    ], text.length);
    final scanResult = SovereignStyleScanner.scan(
      text,
      excludedRanges: inlineExcludedRanges,
      timeBudgetMicros:
          request.timeBudgetMicros ?? predictiveScanTimeBudgetMicros,
      spanBudget: request.spanBudget ?? predictiveScanSpanBudget,
      charLimit: request.charLimit,
    );

    final markerRanges = _buildMarkerRanges(
      text: text,
      fenceMarkerRanges: fenceMarkerRanges,
      blockMarkerRanges: blockMarkerRanges,
      inlineRuns: scanResult.runs,
    );

    final ambiguityZones = scanResult.complete
        ? const <TextRange>[]
        : [
            TextRange(
              start: scanResult.validTo.clamp(0, text.length),
              end: text.length,
            ),
          ];

    return SyntaxPrediction(
      revision: request.revision,
      markerRanges: markerRanges,
      exclusionRanges: fenceExclusionRanges,
      ambiguityZones: ambiguityZones,
      cursorMask: HiddenRangeCursorValidationMask(
        textLength: text.length,
        hiddenRanges: markerRanges,
      ),
    );
  }

  static List<BlockSpan> _toBlockSpans(BlockTree tree) {
    return [
      for (final block in tree.blocks)
        BlockSpan(
          type: block.type,
          start: block.start,
          end: block.end,
          payload: block.payload == null
              ? const {}
              : Map<String, Object?>.from(block.payload!),
        ),
    ];
  }

  static List<TextRange> _buildMarkerRanges({
    required String text,
    required List<TextRange> fenceMarkerRanges,
    required List<TextRange> blockMarkerRanges,
    required List<StyleRun> inlineRuns,
  }) {
    final merged = <String, TextRange>{};

    for (final range in SovereignStyleScanner.extractHiddenRanges(
      text,
      inlineRuns,
    )) {
      merged['${range.start}:${range.end}'] = range;
    }
    for (final range in fenceMarkerRanges) {
      merged['${range.start}:${range.end}'] = range;
    }
    for (final range in blockMarkerRanges) {
      merged['${range.start}:${range.end}'] = range;
    }

    final canonicalBlockMarkers = <TextRange>[
      ...fenceMarkerRanges,
      ...blockMarkerRanges,
    ];
    final canonicalByStart = <int, int>{};
    for (final range in canonicalBlockMarkers) {
      final existingEnd = canonicalByStart[range.start];
      if (existingEnd == null || range.end > existingEnd) {
        canonicalByStart[range.start] = range.end;
      }
    }

    final filtered = <TextRange>[];
    for (final range in merged.values) {
      final canonicalEnd = canonicalByStart[range.start];
      if (canonicalEnd != null && canonicalEnd > range.end) continue;
      filtered.add(range);
    }
    filtered.addAll(canonicalBlockMarkers);
    return _normalizeRanges(filtered, text.length);
  }

  static List<TextRange> _normalizeRanges(
    Iterable<TextRange> ranges,
    int textLength,
  ) {
    final normalized = <TextRange>[];
    for (final range in ranges) {
      final start = range.start.clamp(0, textLength);
      final end = range.end.clamp(0, textLength);
      if (end <= start) continue;
      normalized.add(TextRange(start: start, end: end));
    }
    normalized.sort((a, b) => a.start.compareTo(b.start));

    final deduped = <TextRange>[];
    for (final range in normalized) {
      if (deduped.isEmpty) {
        deduped.add(range);
        continue;
      }
      final last = deduped.last;
      if (range.start < last.end) continue;
      if (range.start == last.start && range.end == last.end) continue;
      deduped.add(range);
    }
    return deduped;
  }
}
