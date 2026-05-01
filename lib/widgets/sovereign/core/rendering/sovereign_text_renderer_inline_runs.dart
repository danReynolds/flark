part of 'sovereign_text_renderer.dart';

class _SovereignTextRendererInlineRuns {
  static List<TextRange> inlineStyleScanExcludedRanges({
    required String text,
    required List<TextRange> baseExcludedRanges,
  }) {
    if (text.isEmpty) return const <TextRange>[];

    final fencedBlocks = FencedCodeScanner.scan(text);
    return _SovereignRendererUtils.normalizeHiddenRanges(<TextRange>[
      ...baseExcludedRanges,
      ...SovereignMarkdownMarkers.fencedCodeFenceMarkers(text, fencedBlocks),
      ...SovereignMarkdownMarkers.markdownBlockMarkerRanges(
        text,
        fencedBlocks: fencedBlocks,
      ),
    ], text.length);
  }

  static List<TextRange> inlineExcludedRangesForBuildTextSpan(
    String text, {
    required DecorationModel latestDecoration,
    required int revision,
    required List<TextRange> projectedExclusionRanges,
  }) {
    final excludedRanges = <TextRange>[];

    if (latestDecoration.originRevision == revision) {
      for (final block in latestDecoration.tree.blocks) {
        if (block.type == BlockType.fencedCode) {
          excludedRanges.add(TextRange(start: block.start, end: block.end));
        }
      }
      return excludedRanges;
    }

    if (projectedExclusionRanges.isNotEmpty) {
      excludedRanges.addAll(
        _SovereignRendererUtils.normalizeHiddenRanges(
          projectedExclusionRanges,
          text.length,
        ),
      );
      return excludedRanges;
    }

    excludedRanges.addAll(
      FencedCodeScanner.scan(
        text,
      ).map((b) => TextRange(start: b.start, end: b.end)),
    );
    return excludedRanges;
  }

  static List<TextRange> buildRenderHiddenRangesForTextSpan({
    required String text,
    required List<TextRange> authoritativeHiddenRanges,
    required List<StyleRun> cachedRuns,
    bool includeInlineHiddenFromCachedRuns = true,
    List<TextRange>? supplementalInlineHiddenRanges,
  }) {
    if (text.isEmpty) return const <TextRange>[];

    final merged = <TextRange>[...authoritativeHiddenRanges];
    if (includeInlineHiddenFromCachedRuns) {
      final inlineHidden = SovereignStyleScanner.extractHiddenRanges(
        text,
        cachedRuns,
      );
      for (final range in inlineHidden) {
        final shadowedByExisting = authoritativeHiddenRanges.any(
          (existing) =>
              existing.start == range.start && existing.end > range.end,
        );
        if (!shadowedByExisting) {
          merged.add(range);
        }
      }
    }
    if (supplementalInlineHiddenRanges != null &&
        supplementalInlineHiddenRanges.isNotEmpty) {
      merged.addAll(supplementalInlineHiddenRanges);
    }

    final mayNeedExtendedBlockMarkers = text.contains(']:') ||
        text.contains('---') ||
        text.contains('***') ||
        text.contains('___');
    if (mayNeedExtendedBlockMarkers) {
      final fenced = FencedCodeScanner.scan(text);
      merged.addAll(
        SovereignMarkdownMarkers.markdownBlockMarkerRanges(
          text,
          fencedBlocks: fenced,
        ),
      );
    }

    return _SovereignRendererUtils.normalizeHiddenRanges(merged, text.length);
  }

  static List<TextRange> localSupplementalInlineHiddenRanges({
    required String text,
    required List<TextRange> excludedRanges,
  }) {
    final supplementalRuns = _supplementalInlineRuns(
      text: text,
      excludedRanges: excludedRanges,
    );
    if (supplementalRuns.isEmpty) return const <TextRange>[];
    return SovereignStyleScanner.extractHiddenRanges(text, supplementalRuns);
  }

  static List<StyleRun> mergeAuthoritativeRunsWithLocalSupplementalInline({
    required String text,
    required List<StyleRun> authoritativeRuns,
    required List<TextRange> excludedRanges,
  }) {
    final localSupplemental = _supplementalInlineRuns(
      text: text,
      excludedRanges: excludedRanges,
    );
    if (localSupplemental.isEmpty) return authoritativeRuns;

    final merged = <StyleRun>[...authoritativeRuns];
    for (final run in localSupplemental) {
      final overlapsExisting = merged.any(
        (existing) => existing.start < run.end && run.start < existing.end,
      );
      if (!overlapsExisting) {
        merged.add(run);
      }
    }
    if (merged.length == authoritativeRuns.length) {
      return authoritativeRuns;
    }

    merged.sort((a, b) {
      final byStart = a.start.compareTo(b.start);
      if (byStart != 0) return byStart;
      return a.end.compareTo(b.end);
    });
    return merged;
  }

  static List<StyleRun> _supplementalInlineRuns({
    required String text,
    required List<TextRange> excludedRanges,
  }) {
    if (!_canHaveSupplementalInlineMarkup(text)) {
      return const <StyleRun>[];
    }

    // Supplemental local inline scanning is intentionally conservative.
    final scanExclusions = inlineStyleScanExcludedRanges(
      text: text,
      baseExcludedRanges: excludedRanges,
    );

    final localResult = SovereignStyleScanner.scan(
      text,
      excludedRanges: scanExclusions,
    );
    return <StyleRun>[
      for (final run in localResult.runs)
        if (run.style.types.contains(SovereignStyleType.bold) ||
            run.style.types.contains(SovereignStyleType.italic) ||
            run.style.types.contains(SovereignStyleType.code) ||
            run.style.types.contains(SovereignStyleType.link) ||
            run.style.types.contains(SovereignStyleType.image))
          run,
    ];
  }

  static bool _canHaveSupplementalInlineMarkup(String text) {
    if (text.length < 4) return false;
    return text.contains('`') ||
        text.contains('*') ||
        text.contains('_') ||
        text.contains('http') ||
        text.contains('[') ||
        text.contains('<') ||
        text.contains('![');
  }
}
