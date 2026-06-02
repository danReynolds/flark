@Tags(<String>['benchmark'])
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  group('Sovereign v2 performance budgets', () {
    test('applies a localized source transaction within budget', () {
      final markdown = List.filled(2000, 'line').join('\n');
      final state = SovereignEditorState.fromMarkdown(markdown);
      final transaction = SovereignTransaction.single(
        SovereignSourceOperation.insert(markdown.length ~/ 2, '**'),
      );

      final elapsed = _measure(() {
        state.applyTransaction(transaction);
      });

      expect(elapsed, lessThan(const Duration(milliseconds: 75)));
    });

    test('builds projection and maps offsets within budget', () {
      final hiddenRanges = [
        for (var i = 0; i < 1000; i++)
          SovereignHiddenRange(
            range: SovereignSourceRange(i * 10, i * 10 + 2),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
      ];

      final elapsed = _measure(() {
        final projection = SovereignProjection(
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
      final controller = SovereignFlutterController.fromMarkdown(markdown);
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: controller.state.revision,
        sourceTextLength: markdown.length,
        blocks: [
          for (var i = 0; i < 1000; i++)
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.paragraph,
              type: 'paragraph',
              sourceRange: SovereignSourceRange(i * 9, i * 9 + 8),
            ),
        ],
        inlineTokens: [
          for (var i = 0; i < 1000; i++)
            SovereignMarkdownInlineToken(
              kind: SovereignMarkdownInlineKind.strong,
              type: 'strong',
              sourceRange: SovereignSourceRange(i * 9 + 2, i * 9 + 6),
            ),
        ],
        hiddenRanges: [
          for (var i = 0; i < 1000; i++) ...[
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: SovereignSourceRange(i * 9, i * 9 + 2),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: SovereignSourceRange(i * 9 + 6, i * 9 + 8),
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
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: 12000,
        blocks: [
          for (var i = 0; i < 1000; i++)
            SovereignMarkdownBlockNode(
              kind: SovereignMarkdownBlockKind.paragraph,
              type: 'paragraph',
              sourceRange: SovereignSourceRange(i * 12, i * 12 + 8),
            ),
        ],
        inlineTokens: [
          for (var i = 0; i < 1000; i++)
            SovereignMarkdownInlineToken(
              kind: SovereignMarkdownInlineKind.strong,
              type: 'strong',
              sourceRange: SovereignSourceRange(i * 12, i * 12 + 8),
            ),
        ],
      );

      final elapsed = _measure(() {
        SovereignRenderPlan.fromParseResult(parseResult: parseResult);
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
