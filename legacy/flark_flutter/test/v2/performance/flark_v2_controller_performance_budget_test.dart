@Tags(<String>['benchmark'])
library;

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('adopts parse results into controller state within budget', () {
    final markdown = List.filled(1000, '**line**').join('\n');
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
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

    final stopwatch = Stopwatch()..start();
    expect(controller.applyParseResult(parseResult), isTrue);
    stopwatch.stop();

    expect(stopwatch.elapsed, lessThan(const Duration(milliseconds: 250)));
  });
}
