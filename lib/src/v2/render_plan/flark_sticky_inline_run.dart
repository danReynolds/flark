import '../core/selection/flark_selection.dart';
import '../core/transaction/flark_source_range.dart';
import '../markdown/parse/flark_markdown_parse_result.dart'
    show FlarkMarkdownInlineKind;
import '../projection/flark_projection.dart';
import 'flark_render_plan.dart';

/// Keeps an emphasis/strong/strikethrough run rendered while the caret is
/// editing inside it, even when a transient trailing space before the closing
/// marker makes CommonMark treat it as literal (`**foo **`).
///
/// CommonMark forbids a closing delimiter preceded by whitespace, so the parse
/// of `**foo **` carries no styled run and the markers would flash into view
/// for the moment a space sits at the run's end. This is a pure function of
/// `(projection, render plan, source, caret)` applied at parse adoption: when
/// the caret sits inside such a run it re-hides the markers and re-styles the
/// content, mirroring a real run. It holds no state and is recomputed on every
/// adoption, so it releases automatically the instant the caret leaves the run
/// or the run becomes valid again.
abstract final class FlarkStickyInlineRun {
  static FlarkStickyInlineRunResult reconcile({
    required FlarkProjection projection,
    required FlarkRenderPlan renderPlan,
    required String source,
    required FlarkSelection selection,
  }) {
    final unchanged = FlarkStickyInlineRunResult(
      projection: projection,
      renderPlan: renderPlan,
    );
    if (!selection.isCollapsed) return unchanged;

    final held = _detectHeldRun(source, selection.extentOffset, projection);
    if (held == null) return unchanged;

    final patchedProjection = _patchProjection(projection, held);
    return FlarkStickyInlineRunResult(
      projection: patchedProjection,
      renderPlan: _patchRenderPlan(renderPlan, held, patchedProjection),
    );
  }

  static _HeldRun? _detectHeldRun(
    String source,
    int caret,
    FlarkProjection projection,
  ) {
    // `**`/`~~` before `*`/`_`, so a `*` probe never matches the inner `*` of a
    // `**` pair.
    const candidates = <_StyleCandidate>[
      _StyleCandidate(
        '**',
        FlarkMarkdownInlineKind.strong,
        FlarkRenderTextStyleToken.strong,
        'strong',
      ),
      _StyleCandidate(
        '~~',
        FlarkMarkdownInlineKind.strikethrough,
        FlarkRenderTextStyleToken.strikethrough,
        'strikethrough',
      ),
      _StyleCandidate(
        '*',
        FlarkMarkdownInlineKind.emphasis,
        FlarkRenderTextStyleToken.emphasis,
        'emphasis',
      ),
      _StyleCandidate(
        '_',
        FlarkMarkdownInlineKind.emphasis,
        FlarkRenderTextStyleToken.emphasis,
        'emphasis',
      ),
    ];

    for (final candidate in candidates) {
      final run = _findTrailingSpaceRun(source, caret, candidate.marker);
      if (run == null) continue;
      // If the parser already styled this pair (its opening marker is hidden),
      // there is nothing to hold — it is valid markdown already.
      if (_marshalsHiddenMarkerAt(projection, run.openStart)) continue;
      return _HeldRun(
        openStart: run.openStart,
        contentStart: run.openStart + candidate.marker.length,
        closeStart: run.closeStart,
        closeEnd: run.closeStart + candidate.marker.length,
        kind: candidate.kind,
        styleToken: candidate.styleToken,
        type: candidate.type,
      );
    }
    return null;
  }

  /// Finds an opening/closing marker pair of [marker] that encloses [caret] and
  /// is valid markdown except for whitespace immediately before the close.
  static _MarkerPair? _findTrailingSpaceRun(
    String source,
    int caret,
    String marker,
  ) {
    final length = marker.length;

    // Closing marker at or after the caret, with whitespace right before it.
    var closeStart = -1;
    var search = caret;
    while (search <= source.length - length) {
      final index = source.indexOf(marker, search);
      if (index < 0) break;
      if (index >= caret &&
          index > 0 &&
          _isWhitespace(source.codeUnitAt(index - 1)) &&
          _isExactMarkerRun(source, index, marker)) {
        closeStart = index;
        break;
      }
      search = index + length;
    }
    if (closeStart < 0) return null;

    // Opening marker before the caret.
    var openStart = -1;
    var probe = caret - length;
    while (probe >= 0) {
      final index = source.lastIndexOf(marker, probe);
      if (index < 0) break;
      if (_isExactMarkerRun(source, index, marker)) {
        openStart = index;
        break;
      }
      probe = index - 1;
    }
    if (openStart < 0) return null;

    final contentStart = openStart + length;
    if (contentStart > caret || caret > closeStart) return null;
    if (contentStart >= closeStart) return null;
    // The opening marker must be left-flanking (content starts non-space), and
    // the content must be more than whitespace.
    if (_isWhitespace(source.codeUnitAt(contentStart))) return null;
    if (source.substring(contentStart, closeStart).trim().isEmpty) return null;

    return _MarkerPair(openStart: openStart, closeStart: closeStart);
  }

  static FlarkProjection _patchProjection(
    FlarkProjection projection,
    _HeldRun held,
  ) {
    return FlarkProjection(
      textLength: projection.textLength,
      hiddenRanges: [
        ...projection.hiddenRanges,
        FlarkHiddenRange(
          range: FlarkSourceRange(held.openStart, held.contentStart),
          kind: FlarkHiddenRangeKind.inlineMarker,
          opensInlineRun: true,
        ),
        FlarkHiddenRange(
          range: FlarkSourceRange(held.closeStart, held.closeEnd),
          kind: FlarkHiddenRangeKind.inlineMarker,
          closesInlineRun: true,
        ),
      ],
      replacementRanges: projection.replacementRanges,
      ambiguityZones: projection.ambiguityZones,
    );
  }

  static FlarkRenderPlan _patchRenderPlan(
    FlarkRenderPlan renderPlan,
    _HeldRun held,
    FlarkProjection projection,
  ) {
    final runRange = FlarkSourceRange(held.openStart, held.closeEnd);
    final run = FlarkRenderInlineRun(
      kind: held.kind,
      type: held.type,
      sourceRange: runRange,
      displayRange: FlarkSourceRange(
        projection.sourceToDisplayOffset(held.openStart),
        projection.sourceToDisplayOffset(held.closeEnd),
      ),
      styleToken: held.styleToken,
    );
    return FlarkRenderPlan(
      blocks: _injectRun(renderPlan.blocks, run, runRange),
      metadata: renderPlan.metadata,
      fidelity: renderPlan.fidelity,
    );
  }

  static List<FlarkRenderBlock> _injectRun(
    List<FlarkRenderBlock> blocks,
    FlarkRenderInlineRun run,
    FlarkSourceRange runRange,
  ) {
    var injected = false;
    final result = <FlarkRenderBlock>[];
    for (final block in blocks) {
      if (!injected && _contains(block.sourceRange, runRange)) {
        result.add(_injectIntoBlock(block, run, runRange));
        injected = true;
      } else {
        result.add(block);
      }
    }
    return result;
  }

  static FlarkRenderBlock _injectIntoBlock(
    FlarkRenderBlock block,
    FlarkRenderInlineRun run,
    FlarkSourceRange runRange,
  ) {
    for (var index = 0; index < block.children.length; index += 1) {
      if (_contains(block.children[index].sourceRange, runRange)) {
        final children = [...block.children];
        children[index] = _injectIntoBlock(block.children[index], run, runRange);
        return _copyBlock(block, children: children);
      }
    }
    return _copyBlock(block, inlineRuns: [...block.inlineRuns, run]);
  }

  static FlarkRenderBlock _copyBlock(
    FlarkRenderBlock block, {
    Iterable<FlarkRenderInlineRun>? inlineRuns,
    Iterable<FlarkRenderBlock>? children,
  }) {
    return FlarkRenderBlock(
      kind: block.kind,
      type: block.type,
      sourceRange: block.sourceRange,
      displayRange: block.displayRange,
      styleToken: block.styleToken,
      inlineRuns: inlineRuns ?? block.inlineRuns,
      children: children ?? block.children,
      table: block.table,
      listItem: block.listItem,
      taskListItem: block.taskListItem,
      codeBlock: block.codeBlock,
      attributes: block.attributes,
    );
  }

  static bool _contains(FlarkSourceRange outer, FlarkSourceRange inner) {
    return outer.start <= inner.start && inner.end <= outer.end;
  }

  static bool _marshalsHiddenMarkerAt(FlarkProjection projection, int offset) {
    for (final hidden in projection.hiddenRanges) {
      if (hidden.range.start == offset) return true;
    }
    return false;
  }

  /// Whether the [marker] at [index] is a complete, unescaped delimiter run of
  /// exactly its own length — not part of a longer run (e.g. the inner `*` of a
  /// `**` pair, or one `**` inside `***`).
  static bool _isExactMarkerRun(String source, int index, String marker) {
    final markerChar = marker.codeUnitAt(0);
    if (index > 0 && source.codeUnitAt(index - 1) == markerChar) return false;
    final after = index + marker.length;
    if (after < source.length && source.codeUnitAt(after) == markerChar) {
      return false;
    }
    var backslashes = 0;
    for (var cursor = index - 1; cursor >= 0; cursor -= 1) {
      if (source.codeUnitAt(cursor) != 0x5C) break;
      backslashes += 1;
    }
    return backslashes.isEven;
  }

  static bool _isWhitespace(int codeUnit) {
    return codeUnit == 0x20 || codeUnit == 0x09;
  }
}

final class FlarkStickyInlineRunResult {
  const FlarkStickyInlineRunResult({
    required this.projection,
    required this.renderPlan,
  });

  final FlarkProjection projection;
  final FlarkRenderPlan renderPlan;
}

final class _StyleCandidate {
  const _StyleCandidate(this.marker, this.kind, this.styleToken, this.type);

  final String marker;
  final FlarkMarkdownInlineKind kind;
  final FlarkRenderTextStyleToken styleToken;
  final String type;
}

final class _MarkerPair {
  const _MarkerPair({required this.openStart, required this.closeStart});

  final int openStart;
  final int closeStart;
}

final class _HeldRun {
  const _HeldRun({
    required this.openStart,
    required this.contentStart,
    required this.closeStart,
    required this.closeEnd,
    required this.kind,
    required this.styleToken,
    required this.type,
  });

  final int openStart;
  final int contentStart;
  final int closeStart;
  final int closeEnd;
  final FlarkMarkdownInlineKind kind;
  final FlarkRenderTextStyleToken styleToken;
  final String type;
}
