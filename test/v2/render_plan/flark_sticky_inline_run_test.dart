import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/projection/projection.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('FlarkStickyInlineRun', () {
    FlarkRenderPlan planFor(String source) {
      return FlarkRenderPlan(
        blocks: [
          FlarkRenderBlock(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: FlarkSourceRange(0, source.length),
            displayRange: FlarkSourceRange(0, source.length),
            styleToken: FlarkRenderTextStyleToken.body,
            inlineRuns: const [],
            children: const [],
          ),
        ],
      );
    }

    FlarkStickyInlineRunResult reconcile(String source, int caret) {
      return FlarkStickyInlineRun.reconcile(
        // An identity projection with no hidden ranges models the literal parse
        // the renderer would otherwise adopt for a trailing-space-broken run.
        projection: FlarkProjection(textLength: source.length),
        renderPlan: planFor(source),
        source: source,
        selection: FlarkSelection.collapsed(caret),
      );
    }

    test('holds a strong run broken only by a trailing space', () {
      final result = reconcile('**foo **', 6);

      expect(result.projection.projectText('**foo **'), 'foo ');
      expect(
        result.renderPlan.allInlineRuns.map((run) => run.styleToken),
        contains(FlarkRenderTextStyleToken.strong),
      );
    });

    test('holds emphasis and strikethrough runs too', () {
      expect(reconcile('*foo *', 5).projection.projectText('*foo *'), 'foo ');
      expect(reconcile('_foo _', 5).projection.projectText('_foo _'), 'foo ');
      expect(
        reconcile('~~foo ~~', 6).projection.projectText('~~foo ~~'),
        'foo ',
      );
    });

    test('holds a run with multiple words and a trailing space', () {
      expect(
        reconcile('**foo bar **', 10).projection.projectText('**foo bar **'),
        'foo bar ',
      );
    });

    test('does nothing when the caret is outside the run', () {
      final result = reconcile('**foo **', 0);

      expect(result.projection.projectText('**foo **'), '**foo **');
      expect(result.renderPlan.allInlineRuns, isEmpty);
    });

    test('does nothing for whitespace-only content', () {
      expect(reconcile('** **', 3).projection.projectText('** **'), '** **');
    });

    test('does nothing for ambiguous *** runs', () {
      expect(
        reconcile('***foo ***', 6).projection.projectText('***foo ***'),
        '***foo ***',
      );
    });

    test('does not touch code spans (already valid with a trailing space)', () {
      expect(reconcile('`foo `', 4).projection.projectText('`foo `'), '`foo `');
    });

    test('does nothing for a non-collapsed selection', () {
      final result = FlarkStickyInlineRun.reconcile(
        projection: FlarkProjection(textLength: 8),
        renderPlan: planFor('**foo **'),
        source: '**foo **',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 6),
      );

      expect(result.projection.projectText('**foo **'), '**foo **');
    });

    test('does not hold an escaped marker', () {
      expect(
        reconcile(r'\**foo **', 7).projection.projectText(r'\**foo **'),
        r'\**foo **',
      );
    });
  });
}
