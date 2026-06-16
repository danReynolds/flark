import '../core/selection/flark_selection.dart';
import '../core/transaction/flark_source_range.dart';
import '../projection/flark_projection.dart';
import 'flark_render_plan.dart';

/// Renders a checkbox while the user is still typing a task marker, instead of
/// the bullet-plus-literal-`[ ]` the parser produces mid-keystroke.
///
/// A task item's `- ` prefix is valid bullet-list syntax on its own, so a
/// source-driven editor renders a bullet the instant you type `- `, then shows
/// `[`, `[ `, `[ ]` as literal item text until `- [ ] ` completes and the
/// parser recognizes a task. This pure `(projection, render plan, source,
/// caret)` pass closes that gap: when the caret sits in a bullet item whose
/// whole content is a forming-checkbox token (`[`, `[ `, `[x`, `[ ]`, `[x]`,
/// …), it hides the token and marks the block a task item, so a checkbox shows
/// immediately. It holds no state and is recomputed at every adoption, so it
/// releases the moment the caret leaves or the token resolves into a real
/// `- [ ] ` task (which the parser then owns).
abstract final class FlarkOptimisticCheckbox {
  static FlarkOptimisticCheckboxResult reconcile({
    required FlarkProjection projection,
    required FlarkRenderPlan renderPlan,
    required String source,
    required FlarkSelection selection,
  }) {
    final unchanged = FlarkOptimisticCheckboxResult(
      projection: projection,
      renderPlan: renderPlan,
    );
    if (!selection.isCollapsed) return unchanged;
    final caret = selection.extentOffset;
    if (caret < 0 || caret > source.length) return unchanged;

    final forming = _detect(renderPlan.blocks, source, caret);
    if (forming == null) return unchanged;

    final patchedProjection = _patchProjection(projection, forming.tokenRange);
    return FlarkOptimisticCheckboxResult(
      projection: patchedProjection,
      renderPlan: _patchRenderPlan(renderPlan, forming, patchedProjection),
    );
  }

  /// The forming-checkbox token: `[` optionally followed by a state character
  /// (` `, `x`, `X`) and an optional closing `]`. A trailing space is excluded
  /// — `- [ ] ` is a real task the parser already owns.
  static final RegExp _tokenPattern = RegExp(r'^([ \t]*[-*+] +)(\[[ xX]?\]?)$');

  /// Whether the line containing [caret] is a bullet item whose whole content
  /// is a forming-checkbox token. An edit that lands here parses immediately so
  /// the checkbox renders without waiting for the debounced parse (otherwise
  /// the just-typed token character flashes literal until the next parse).
  static bool isFormingCheckboxLine(String source, int caret) {
    if (caret < 0 || caret > source.length) return false;
    var start = caret;
    while (start > 0 && source.codeUnitAt(start - 1) != 0x0A) {
      start -= 1;
    }
    var end = caret;
    while (end < source.length && source.codeUnitAt(end) != 0x0A) {
      end += 1;
    }
    return _tokenPattern.hasMatch(source.substring(start, end));
  }

  static _FormingCheckbox? _detect(
    List<FlarkRenderBlock> blocks,
    String source,
    int caret,
  ) {
    for (final block in blocks) {
      if (block.sourceRange.start > caret || caret > block.sourceRange.end) {
        continue;
      }
      // Prefer the innermost matching list item (items nest as children).
      final child = _detect(block.children, source, caret);
      if (child != null) return child;

      if (block.listItem == null || block.taskListItem != null) continue;
      final text = source.substring(
        block.sourceRange.start,
        block.sourceRange.end,
      );
      final match = _tokenPattern.firstMatch(text);
      if (match == null) continue;
      final tokenStart = block.sourceRange.start + match.group(1)!.length;
      final token = match.group(2)!;
      return _FormingCheckbox(
        block: block,
        tokenRange: FlarkSourceRange(tokenStart, block.sourceRange.end),
        checked: token.contains('x') || token.contains('X'),
      );
    }
    return null;
  }

  static FlarkProjection _patchProjection(
    FlarkProjection projection,
    FlarkSourceRange tokenRange,
  ) {
    return FlarkProjection(
      textLength: projection.textLength,
      hiddenRanges: [
        ...projection.hiddenRanges,
        FlarkHiddenRange(
          range: tokenRange,
          kind: FlarkHiddenRangeKind.blockMarker,
        ),
      ],
      replacementRanges: projection.replacementRanges,
      ambiguityZones: projection.ambiguityZones,
    );
  }

  static FlarkRenderPlan _patchRenderPlan(
    FlarkRenderPlan renderPlan,
    _FormingCheckbox forming,
    FlarkProjection projection,
  ) {
    return FlarkRenderPlan(
      blocks: _patchBlocks(renderPlan.blocks, forming, projection),
      metadata: renderPlan.metadata,
      fidelity: renderPlan.fidelity,
    );
  }

  static List<FlarkRenderBlock> _patchBlocks(
    List<FlarkRenderBlock> blocks,
    _FormingCheckbox forming,
    FlarkProjection projection,
  ) {
    return [
      for (final block in blocks)
        if (identical(block, forming.block))
          _toTaskBlock(block, forming, projection)
        else if (block.children.isEmpty)
          block
        else
          _copyBlock(
            block,
            children: _patchBlocks(block.children, forming, projection),
          ),
    ];
  }

  /// Marks [block] a task item and drops the now-hidden token from its display:
  /// the token is the whole content, so the item renders as an empty checkbox.
  static FlarkRenderBlock _toTaskBlock(
    FlarkRenderBlock block,
    _FormingCheckbox forming,
    FlarkProjection projection,
  ) {
    return FlarkRenderBlock(
      kind: block.kind,
      type: block.type,
      sourceRange: block.sourceRange,
      displayRange: FlarkSourceRange(
        projection.sourceToDisplayOffset(block.sourceRange.start),
        projection.sourceToDisplayOffset(block.sourceRange.end),
      ),
      styleToken: block.styleToken,
      inlineRuns: [
        for (final run in block.inlineRuns)
          if (run.sourceRange.end <= forming.tokenRange.start) run,
      ],
      children: block.children,
      table: block.table,
      listItem: block.listItem,
      taskListItem: FlarkRenderTaskListItemDescriptor(checked: forming.checked),
      codeBlock: block.codeBlock,
      attributes: block.attributes,
    );
  }

  static FlarkRenderBlock _copyBlock(
    FlarkRenderBlock block, {
    required Iterable<FlarkRenderBlock> children,
  }) {
    return FlarkRenderBlock(
      kind: block.kind,
      type: block.type,
      sourceRange: block.sourceRange,
      displayRange: block.displayRange,
      styleToken: block.styleToken,
      inlineRuns: block.inlineRuns,
      children: children,
      table: block.table,
      listItem: block.listItem,
      taskListItem: block.taskListItem,
      codeBlock: block.codeBlock,
      attributes: block.attributes,
    );
  }
}

final class _FormingCheckbox {
  const _FormingCheckbox({
    required this.block,
    required this.tokenRange,
    required this.checked,
  });

  final FlarkRenderBlock block;
  final FlarkSourceRange tokenRange;
  final bool checked;
}

final class FlarkOptimisticCheckboxResult {
  const FlarkOptimisticCheckboxResult({
    required this.projection,
    required this.renderPlan,
  });

  final FlarkProjection projection;
  final FlarkRenderPlan renderPlan;
}
