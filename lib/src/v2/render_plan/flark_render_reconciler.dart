import '../core/extension/flark_extension.dart';
import '../core/selection/flark_selection.dart';
import '../markdown/parse/flark_markdown_parse_result.dart';
import '../projection/flark_projection.dart';
import 'flark_render_plan.dart';

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
///
/// (A "sticky inline run" pass used to re-hide the markers of a
/// `**foo **`-shaped run while the caret edited inside it. It is gone: the
/// write paths now keep inline delimiters flanking-valid at every revision —
/// see `FlarkInlineDelimiterPlacement` — so the parse never drops an
/// editor-authored run and the rendered view never depends on caret-local
/// compensation.)
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

  static const List<_ReconciliationPass> _passes = [_extensionsPass];

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
