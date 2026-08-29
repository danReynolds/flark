import 'dart:convert';

import 'models.dart';

/// Pure decision for atomically adopting one queried viewport.
///
/// In particular, a certified empty row set is a valid semantic publication:
/// it proves the parser found no rendered rows. That is distinct from the row
/// cache question, which only installs rows when the query actually has them.
final class FlarkViewportInstallationPlan {
  const FlarkViewportInstallationPlan._({
    required this.retainsExistingSurface,
    required this.sourceFitsViewport,
    required this.installsFreshRows,
    required this.installsCertifiedSurface,
  });

  factory FlarkViewportInstallationPlan.evaluate({
    required FlarkViewport viewport,
    required String source,
    required int previousVisibleUtf16Start,
    required String previousVisibleSource,
    required Iterable<FlarkSourceRange> mappedCachedRowRanges,
  }) {
    final previousVisibleEnd =
        previousVisibleUtf16Start + previousVisibleSource.length;
    final cachedRowsFitVisibleSource = mappedCachedRowRanges.every(
      (range) =>
          previousVisibleUtf16Start <= range.start &&
          range.end <= previousVisibleEnd,
    );
    final viewportMatchesVisibleSource =
        viewport.coveredUtf16.start == previousVisibleUtf16Start &&
        viewport.coveredUtf16.end == previousVisibleEnd &&
        source == previousVisibleSource;
    final retainsExistingSurface =
        !viewport.isCertified &&
        mappedCachedRowRanges.isNotEmpty &&
        cachedRowsFitVisibleSource &&
        viewportMatchesVisibleSource;
    final sourceFitsViewport =
        source.length == viewport.coveredUtf16.length &&
        utf8.encode(source).length == viewport.coveredBytes.length;
    final rowsFit = rowsFitViewport(viewport);
    return FlarkViewportInstallationPlan._(
      retainsExistingSurface: retainsExistingSurface,
      sourceFitsViewport: sourceFitsViewport,
      installsFreshRows:
          viewport.rows.isNotEmpty &&
          sourceFitsViewport &&
          rowsFit &&
          !retainsExistingSurface,
      installsCertifiedSurface:
          viewport.isCertified &&
          sourceFitsViewport &&
          rowsFit &&
          !retainsExistingSurface,
    );
  }

  final bool retainsExistingSurface;
  final bool sourceFitsViewport;

  /// Whether the row cache should be replaced by nonempty fresh rows.
  final bool installsFreshRows;

  /// Whether the installed viewport is parser authority for this revision.
  /// This can be true while [installsFreshRows] is false for an exact empty
  /// rendered surface.
  final bool installsCertifiedSurface;

  static bool rowsFitViewport(FlarkViewport viewport) => viewport.rows.every(
    (row) =>
        viewport.coveredBytes.start <= row.sourceBytes.start &&
        row.sourceBytes.end <= viewport.coveredBytes.end &&
        viewport.coveredUtf16.start <= row.sourceUtf16.start &&
        row.sourceUtf16.end <= viewport.coveredUtf16.end,
  );
}
