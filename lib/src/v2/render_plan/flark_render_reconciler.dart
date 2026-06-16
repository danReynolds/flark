import '../core/extension/flark_extension.dart';
import '../core/selection/flark_selection.dart';
import '../markdown/parse/flark_markdown_parse_result.dart';
import '../projection/flark_projection.dart';
import 'flark_optimistic_checkbox.dart';
import 'flark_render_plan.dart';
import 'flark_sticky_inline_run.dart';

/// The pair of derived views adopted from a parse: the offset mapping
/// ([projection]) and the block/inline model ([renderPlan]).
final class FlarkRenderAdoption {
  const FlarkRenderAdoption({
    required this.projection,
    required this.renderPlan,
  });

  final FlarkProjection projection;
  final FlarkRenderPlan renderPlan;
}

/// Builds the authoritative [FlarkRenderAdoption] for a parse by running an
/// ordered pipeline of reconciliation passes over a base derived from the parse
/// result.
///
/// Keeping the passes in one ordered list — rather than inline in the
/// controller's parse-adoption path — gives every "the render should show X
/// even though the parser produced Y" refinement a single, explicit home, so a
/// new pass is one entry in [_passes] instead of another override threaded
/// through `applyParseResult`. Passes run in order; each is a pure
/// `(adoption, context) → adoption`.
///
/// Current pipeline:
/// 1. **extensions** — apply registered render-plan extensions.
/// 2. **sticky inline run** — keep an emphasis/strong/strikethrough run
///    rendered while the caret edits inside it, even when a transient trailing
///    space makes the parse drop the styled run.
/// 3. **optimistic checkbox** — render a checkbox while the caret is still
///    typing a task marker (`- [`, `- [ `, `- [ ]`), instead of the bullet plus
///    literal `[ ]` the parser produces until `- [ ] ` completes.
abstract final class FlarkRenderReconciler {
  static FlarkRenderAdoption fromParseResult({
    required FlarkMarkdownParseResult parseResult,
    required String source,
    required FlarkSelection selection,
    required FlarkExtensionSet extensions,
  }) {
    final projection = FlarkProjection.fromParseResult(parseResult);
    var adoption = FlarkRenderAdoption(
      projection: projection,
      renderPlan: FlarkRenderPlan.fromParseResult(
        parseResult: parseResult,
        projection: projection,
      ),
    );

    final context = _ReconciliationContext(
      parseResult: parseResult,
      source: source,
      selection: selection,
      extensions: extensions,
    );
    for (final pass in _passes) {
      adoption = pass(adoption, context);
    }
    return adoption;
  }

  static const List<_ReconciliationPass> _passes = [
    _extensionsPass,
    _stickyInlineRunPass,
    _optimisticCheckboxPass,
  ];

  static FlarkRenderAdoption _extensionsPass(
    FlarkRenderAdoption adoption,
    _ReconciliationContext context,
  ) {
    return FlarkRenderAdoption(
      projection: adoption.projection,
      renderPlan: applyFlarkRenderPlanExtensions(
        renderPlan: adoption.renderPlan,
        parseResult: context.parseResult,
        projection: adoption.projection,
        extensions: context.extensions,
      ),
    );
  }

  static FlarkRenderAdoption _stickyInlineRunPass(
    FlarkRenderAdoption adoption,
    _ReconciliationContext context,
  ) {
    final sticky = FlarkStickyInlineRun.reconcile(
      projection: adoption.projection,
      renderPlan: adoption.renderPlan,
      source: context.source,
      selection: context.selection,
    );
    return FlarkRenderAdoption(
      projection: sticky.projection,
      renderPlan: sticky.renderPlan,
    );
  }

  static FlarkRenderAdoption _optimisticCheckboxPass(
    FlarkRenderAdoption adoption,
    _ReconciliationContext context,
  ) {
    final checkbox = FlarkOptimisticCheckbox.reconcile(
      projection: adoption.projection,
      renderPlan: adoption.renderPlan,
      source: context.source,
      selection: context.selection,
    );
    return FlarkRenderAdoption(
      projection: checkbox.projection,
      renderPlan: checkbox.renderPlan,
    );
  }
}

typedef _ReconciliationPass =
    FlarkRenderAdoption Function(
      FlarkRenderAdoption adoption,
      _ReconciliationContext context,
    );

final class _ReconciliationContext {
  const _ReconciliationContext({
    required this.parseResult,
    required this.source,
    required this.selection,
    required this.extensions,
  });

  final FlarkMarkdownParseResult parseResult;
  final String source;
  final FlarkSelection selection;
  final FlarkExtensionSet extensions;
}
