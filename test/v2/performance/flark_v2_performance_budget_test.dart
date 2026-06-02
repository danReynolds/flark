@Tags(<String>['benchmark'])
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('Flark v2 performance budgets', () {
    test('applies a localized source transaction within budget', () {
      final markdown = List.filled(2000, 'line').join('\n');
      final state = FlarkEditorState.fromMarkdown(markdown);
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(markdown.length ~/ 2, '**'),
      );

      final elapsed = _measure(() {
        state.applyTransaction(transaction);
      });

      expect(elapsed, lessThan(const Duration(milliseconds: 75)));
    });

    test('builds projection and maps offsets within budget', () {
      final hiddenRanges = [
        for (var i = 0; i < 1000; i++)
          FlarkHiddenRange(
            range: FlarkSourceRange(i * 10, i * 10 + 2),
            kind: FlarkHiddenRangeKind.inlineMarker,
          ),
      ];

      final elapsed = _measure(() {
        final projection = FlarkProjection(
          textLength: 10000,
          hiddenRanges: hiddenRanges,
        );
        for (var i = 0; i < 1000; i++) {
          projection.sourceToDisplayOffset(i * 10);
          projection.displayToSourceOffset(i * 8);
        }
      });

      expect(elapsed, lessThan(const Duration(milliseconds: 150)));
    });

    test('adopts parse results into controller state within budget', () {
      final markdown = List.filled(1000, '**line**').join('\n');
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: markdown.length,
        blocks: [
          for (var i = 0; i < 1000; i++)
            FlarkMarkdownBlockNode(
              kind: FlarkMarkdownBlockKind.paragraph,
              type: 'paragraph',
              sourceRange: FlarkSourceRange(i * 9, i * 9 + 8),
            ),
        ],
        inlineTokens: [
          for (var i = 0; i < 1000; i++)
            FlarkMarkdownInlineToken(
              kind: FlarkMarkdownInlineKind.strong,
              type: 'strong',
              sourceRange: FlarkSourceRange(i * 9 + 2, i * 9 + 6),
            ),
        ],
        hiddenRanges: [
          for (var i = 0; i < 1000; i++) ...[
            FlarkMarkdownHiddenRange(
              kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: FlarkSourceRange(i * 9, i * 9 + 2),
            ),
            FlarkMarkdownHiddenRange(
              kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: FlarkSourceRange(i * 9 + 6, i * 9 + 8),
            ),
          ],
        ],
      );

      final elapsed = _measure(() {
        expect(controller.applyParseResult(parseResult), isTrue);
      });

      expect(elapsed, lessThan(const Duration(milliseconds: 250)));
    });

    test('builds render plans from parsed blocks within budget', () {
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 12000,
        blocks: [
          for (var i = 0; i < 1000; i++)
            FlarkMarkdownBlockNode(
              kind: FlarkMarkdownBlockKind.paragraph,
              type: 'paragraph',
              sourceRange: FlarkSourceRange(i * 12, i * 12 + 8),
            ),
        ],
        inlineTokens: [
          for (var i = 0; i < 1000; i++)
            FlarkMarkdownInlineToken(
              kind: FlarkMarkdownInlineKind.strong,
              type: 'strong',
              sourceRange: FlarkSourceRange(i * 12, i * 12 + 8),
            ),
        ],
      );

      final elapsed = _measure(() {
        FlarkRenderPlan.fromParseResult(parseResult: parseResult);
      });

      expect(elapsed, lessThan(const Duration(milliseconds: 200)));
    });
  });
}

Duration _measure(void Function() body) {
  final stopwatch = Stopwatch()..start();
  body();
  stopwatch.stop();
  return stopwatch.elapsed;
}
