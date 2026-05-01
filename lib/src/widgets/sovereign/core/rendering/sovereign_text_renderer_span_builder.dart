part of 'sovereign_text_renderer.dart';

class _SovereignTextRendererSpanBuilder {
  static _AppendSpanResult appendSpan(
    List<InlineSpan> children,
    String text,
    int start,
    int end,
    TextStyle style,
    List<TextRange> hiddenRanges,
    int hiddenIndex,
    List<_CodeStyleRun> codeRuns,
    int codeIndex,
    List<_BlockStyleRun> blockRuns,
    int blockIndex,
    SovereignMarkdownTheme markdownTheme,
    SovereignHeadingsTheme? headingTheme,
    SovereignTaskCheckboxTheme? taskCheckboxTheme,
  ) {
    var current = start;

    while (hiddenIndex < hiddenRanges.length &&
        hiddenRanges[hiddenIndex].end <= current) {
      hiddenIndex++;
    }
    while (codeIndex < codeRuns.length && codeRuns[codeIndex].end <= current) {
      codeIndex++;
    }
    while (
        blockIndex < blockRuns.length && blockRuns[blockIndex].end <= current) {
      blockIndex++;
    }

    while (current < end) {
      final nextHiddenStart = hiddenIndex < hiddenRanges.length
          ? hiddenRanges[hiddenIndex].start
          : end;
      final visibleEnd = nextHiddenStart < end ? nextHiddenStart : end;

      if (visibleEnd > current) {
        final result = _appendVisibleWithBlockAndCodeHighlight(
          children,
          text,
          current,
          visibleEnd,
          style,
          blockRuns,
          blockIndex,
          codeRuns,
          codeIndex,
          markdownTheme,
          headingTheme,
        );
        codeIndex = result.codeIndex;
        blockIndex = result.blockIndex;
        current = visibleEnd;
        continue;
      }

      if (hiddenIndex >= hiddenRanges.length ||
          hiddenRanges[hiddenIndex].start >= end) {
        final result = _appendVisibleWithBlockAndCodeHighlight(
          children,
          text,
          current,
          end,
          style,
          blockRuns,
          blockIndex,
          codeRuns,
          codeIndex,
          markdownTheme,
          headingTheme,
        );
        codeIndex = result.codeIndex;
        blockIndex = result.blockIndex;
        current = end;
        break;
      }

      final range = hiddenRanges[hiddenIndex];
      final safeEnd = range.end < end ? range.end : end;
      final hiddenText = text.substring(current, safeEnd);
      final hiddenRange = TextRange(start: current, end: safeEnd);

      if (_SovereignTextRendererMarkers.isBlockquoteMarkerRange(
        text,
        hiddenRange,
      )) {
        children.add(
          const TextSpan(
            text: '> ',
            style: TextStyle(
              color: Color(0x00000000),
              decoration: TextDecoration.none,
            ),
          ),
        );
      } else if (_SovereignTextRendererMarkers.isMarkdownImageOpenerMarkerRange(
        text,
        hiddenRange,
      )) {
        children.add(
          TextSpan(
            // Keep source-width parity for caret geometry, but don't render a
            // synthetic icon marker for images; the overlay/alt text provide
            // the image affordance and the square glyph looked like debug UI.
            text: _SovereignTextRendererMarkers.padVisualMarker(
              '',
              hiddenRange.end - hiddenRange.start,
            ),
            style: const TextStyle(
              fontSize: 0,
              letterSpacing: 0,
              wordSpacing: 0,
              color: Color(0x00000000),
              decoration: TextDecoration.none,
            ),
          ),
        );
      } else if (_SovereignTextRendererMarkers
          .isMarkdownLinkOrImageTailMarkerRange(
        text,
        hiddenRange,
      )) {
        children.add(
          TextSpan(
            text: _SovereignTextRendererMarkers.padVisualMarker(
              '',
              hiddenRange.end - hiddenRange.start,
            ),
            style: const TextStyle(
              fontSize: 0,
              letterSpacing: 0,
              wordSpacing: 0,
              color: Color(0x00000000),
              decoration: TextDecoration.none,
            ),
          ),
        );
      } else if (_SovereignTextRendererMarkers.isThematicBreakMarkerRange(
            text,
            hiddenRange,
          ) &&
          (_SovereignTextRendererMarkers.hiddenRangeHasBlockKind(
                blockRuns: blockRuns,
                fromIndex: blockIndex,
                range: hiddenRange,
                kind: _BlockStyleKind.thematicBreak,
              ) ||
              _SovereignTextRendererMarkers.isStandaloneThematicBreakFallback(
                text,
                hiddenRange,
              ))) {
        children.add(
          TextSpan(
            text: _SovereignTextRendererMarkers.thematicBreakVisualMarker(
              hiddenText,
            ),
            style: style.copyWith(
              color: markdownTheme.blockquoteBorderColor.withValues(
                alpha: 0.92,
              ),
              letterSpacing: 0.4,
            ),
          ),
        );
      } else if (_SovereignTextRendererMarkers.taskListMarkerVisualForRange(
        text,
        hiddenRange,
        taskCheckboxTheme: taskCheckboxTheme,
      )
          case final marker?) {
        final override = marker.contains('\u2611')
            ? taskCheckboxTheme?.checked
            : taskCheckboxTheme?.unchecked;
        final useCustomOverlay = taskCheckboxTheme?.useCustomOverlay == true &&
            (marker.startsWith('\u2611') || marker.startsWith('\u2610'));
        if (useCustomOverlay) {
          children.add(
            TextSpan(
              text: marker,
              style: style
                  .merge(override)
                  .copyWith(color: const Color(0x00000000)),
            ),
          );
        } else {
          children.add(TextSpan(text: marker, style: style.merge(override)));
        }
      } else if (_SovereignTextRendererMarkers.isUnorderedListMarkerRange(
        text,
        hiddenRange,
      )) {
        if (_SovereignTextRendererMarkers.isUnorderedTaskListBulletMarkerRange(
          text,
          hiddenRange,
        )) {
          children.add(
            TextSpan(
              // Collapse the raw bullet marker width for task items so the
              // task text aligns with normal list/blockquote content.
              text: _SovereignTextRendererMarkers.padVisualMarker(
                '',
                hiddenRange.end - hiddenRange.start,
              ),
              style: const TextStyle(
                fontSize: 0,
                letterSpacing: 0,
                wordSpacing: 0,
                color: Color(0x00000000),
                decoration: TextDecoration.none,
              ),
            ),
          );
        } else {
          children.add(
            TextSpan(
              text: _SovereignTextRendererMarkers.unorderedListVisualMarker(
                hiddenText,
              ),
              style: style,
            ),
          );
        }
      } else if (_SovereignTextRendererMarkers.isOrderedListMarkerRange(
        text,
        hiddenRange,
      )) {
        children.add(TextSpan(text: hiddenText, style: style));
      } else {
        children.add(
          TextSpan(
            text: hiddenText,
            style: const TextStyle(
              fontSize: 0,
              color: Color(0x00000000),
              decoration: TextDecoration.none,
            ),
          ),
        );
      }

      current = safeEnd;
      if (range.end <= current) hiddenIndex++;
    }

    return _AppendSpanResult(
      hiddenIndex: hiddenIndex,
      codeIndex: codeIndex,
      blockIndex: blockIndex,
    );
  }

  static _BlockCodeAppendResult _appendVisibleWithBlockAndCodeHighlight(
    List<InlineSpan> children,
    String text,
    int start,
    int end,
    TextStyle baseStyle,
    List<_BlockStyleRun> blockRuns,
    int blockIndex,
    List<_CodeStyleRun> codeRuns,
    int codeIndex,
    SovereignMarkdownTheme markdownTheme,
    SovereignHeadingsTheme? headingTheme,
  ) {
    var current = start;
    while (
        blockIndex < blockRuns.length && blockRuns[blockIndex].end <= current) {
      blockIndex++;
    }

    while (current < end) {
      if (blockIndex >= blockRuns.length ||
          blockRuns[blockIndex].start >= end) {
        codeIndex = _appendVisibleWithCodeHighlight(
          children,
          text,
          current,
          end,
          baseStyle,
          codeRuns,
          codeIndex,
        );
        return _BlockCodeAppendResult(
          codeIndex: codeIndex,
          blockIndex: blockIndex,
        );
      }

      final run = blockRuns[blockIndex];
      if (run.start > current) {
        final gapEnd = run.start < end ? run.start : end;
        codeIndex = _appendVisibleWithCodeHighlight(
          children,
          text,
          current,
          gapEnd,
          baseStyle,
          codeRuns,
          codeIndex,
        );
        current = gapEnd;
        continue;
      }

      final runEnd = run.end < end ? run.end : end;
      final mergedBase = _mergeBlockStyle(
        baseStyle,
        run,
        markdownTheme,
        headingTheme,
      );
      codeIndex = _appendVisibleWithCodeHighlight(
        children,
        text,
        current,
        runEnd,
        mergedBase,
        codeRuns,
        codeIndex,
      );
      current = runEnd;
      if (run.end <= current) blockIndex++;
    }

    return _BlockCodeAppendResult(codeIndex: codeIndex, blockIndex: blockIndex);
  }

  static int _appendVisibleWithCodeHighlight(
    List<InlineSpan> children,
    String text,
    int start,
    int end,
    TextStyle baseStyle,
    List<_CodeStyleRun> codeRuns,
    int codeIndex,
  ) {
    var current = start;
    while (codeIndex < codeRuns.length && codeRuns[codeIndex].end <= current) {
      codeIndex++;
    }

    while (current < end) {
      if (codeIndex >= codeRuns.length || codeRuns[codeIndex].start >= end) {
        children.add(
          TextSpan(text: text.substring(current, end), style: baseStyle),
        );
        return codeIndex;
      }

      final run = codeRuns[codeIndex];
      if (run.start > current) {
        final gapEnd = run.start < end ? run.start : end;
        children.add(
          TextSpan(text: text.substring(current, gapEnd), style: baseStyle),
        );
        current = gapEnd;
        continue;
      }

      final runEnd = run.end < end ? run.end : end;
      children.add(
        TextSpan(
          text: text.substring(current, runEnd),
          style: baseStyle.merge(run.style),
        ),
      );
      current = runEnd;
      if (run.end <= current) codeIndex++;
    }

    return codeIndex;
  }

  static TextStyle _mergeBlockStyle(
    TextStyle base,
    _BlockStyleRun run,
    SovereignMarkdownTheme markdownTheme,
    SovereignHeadingsTheme? headingTheme,
  ) {
    switch (run.kind) {
      case _BlockStyleKind.header:
        final level = run.headerLevel?.clamp(1, 6) ?? 1;
        return EditorHeadingStylePolicy.resolve(
          base: base,
          level: level,
          markdownTheme: markdownTheme,
          headingTheme: headingTheme,
        );
      case _BlockStyleKind.blockquote:
        return markdownTheme.blockquoteStyleFor(base);
      case _BlockStyleKind.thematicBreak:
        return base.copyWith(
          color: markdownTheme.blockquoteBorderColor.withValues(alpha: 0.90),
          letterSpacing: 0.4,
        );
      case _BlockStyleKind.table:
        return base.copyWith(
          fontFamily: markdownTheme.monospaceFontFamily,
          height: (base.height ?? 1.35),
        );
      case _BlockStyleKind.taskChecked:
        return markdownTheme.taskCheckedStyleFor(base);
    }
  }
}
