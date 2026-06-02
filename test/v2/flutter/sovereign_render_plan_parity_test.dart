import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('Flark render-plan parity', () {
    testWidgets('editable and preview surfaces share controller render state', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      controller.applyParseResult(_strongParseResult(controller));

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Column(
            children: [
              FlarkEditableText(controller: controller, maxLines: null),
              Markdown(controller: controller),
            ],
          ),
        ),
      );

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, '**bold**');
      expect(_previewPlainText(tester), 'bold');
      expect(controller.hasAuthoritativeRenderPlan, isTrue);

      await tester.enterText(find.byType(EditableText), '**bold**!');
      await tester.pump();

      expect(controller.markdown, '**bold**!');
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
      expect(find.text('**bold**!'), findsOneWidget);
    });
  });
}

FlarkMarkdownParseResult _strongParseResult(FlarkFlutterController controller) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.state.document.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: const FlarkSourceRange(0, 8),
      ),
    ],
    inlineTokens: [
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.strong,
        type: 'strong',
        sourceRange: const FlarkSourceRange(0, 8),
      ),
    ],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: const FlarkSourceRange(0, 2),
      ),
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: const FlarkSourceRange(6, 8),
      ),
    ],
  );
}

String _previewPlainText(WidgetTester tester) {
  final richText = tester
      .widgetList<RichText>(find.byType(RichText))
      .firstWhere((widget) => widget.text.toPlainText() == 'bold');
  return richText.text.toPlainText();
}
