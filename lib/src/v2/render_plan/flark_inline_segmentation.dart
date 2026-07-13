import 'flark_render_plan.dart';

/// One column-disjoint slice of a block's display text together with every
/// inline run covering it (RFC 022 Phase 3).
///
/// Comrak emits one inline token per AST node, so nested styles produce runs
/// that OVERLAP over the same display columns (`***x***`, `**[x](u)**`).
/// Rendering them correctly requires cutting the block at every run boundary
/// and merging the styles of the runs covering each segment — the covering
/// model. Both rendering surfaces (the live editor and the read-only
/// preview) MUST consume this one implementation: the preview's independent
/// span walk once re-emitted overlapping runs' text (`boldbold`), the drift
/// class this module removes.
final class FlarkInlineSegment {
  const FlarkInlineSegment({
    required this.start,
    required this.end,
    required this.coveringRuns,
  });

  /// Absolute display offset where this segment begins (inclusive).
  final int start;

  /// Absolute display offset where this segment ends (exclusive).
  final int end;

  /// Runs whose display range fully covers `[start, end)`, in the block's
  /// run order. Boundary cutting guarantees a run either fully covers a
  /// segment or does not touch it.
  final List<FlarkRenderInlineRun> coveringRuns;
}

/// Cuts the display window `[start, end)` at every intersecting run edge and
/// reports each resulting segment with its covering runs. Segments partition
/// the window exactly: they are adjacent, non-empty, and concatenate back to
/// `[start, end)`. Zero-width runs (an empty-alt image's display range)
/// cover no segment; they are the caller's to place by position.
List<FlarkInlineSegment> flarkSegmentInlineRuns({
  required int start,
  required int end,
  required List<FlarkRenderInlineRun> runs,
}) {
  if (start >= end) return const [];

  final intersecting = <FlarkRenderInlineRun>[
    for (final run in runs)
      if (run.displayRange.end > start && run.displayRange.start < end) run,
  ];

  final boundaries = <int>{start, end};
  for (final run in intersecting) {
    boundaries
      ..add(run.displayRange.start.clamp(start, end))
      ..add(run.displayRange.end.clamp(start, end));
  }
  final sorted = boundaries.toList()..sort();

  final segments = <FlarkInlineSegment>[];
  for (var index = 0; index < sorted.length - 1; index += 1) {
    final segmentStart = sorted[index];
    final segmentEnd = sorted[index + 1];
    if (segmentStart >= segmentEnd) continue;
    segments.add(
      FlarkInlineSegment(
        start: segmentStart,
        end: segmentEnd,
        coveringRuns: List.unmodifiable([
          for (final run in intersecting)
            if (run.displayRange.start <= segmentStart &&
                run.displayRange.end >= segmentEnd)
              run,
        ]),
      ),
    );
  }
  return List.unmodifiable(segments);
}
