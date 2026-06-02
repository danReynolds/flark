import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('Sovereign render-plan parity', () {
    testWidgets('editable and preview surfaces share controller render state', (
      tester,
    ) async {
      final controller = SovereignFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      controller.applyParseResult(_strongParseResult(controller));

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Column(
            children: [
              SovereignEditableText(controller: controller, maxLines: null),
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

SovereignMarkdownParseResult _strongParseResult(
  SovereignFlutterController controller,
) {
  return SovereignMarkdownParseResult(
    schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.state.document.length,
    blocks: [
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: const SovereignSourceRange(0, 8),
      ),
    ],
    inlineTokens: [
      SovereignMarkdownInlineToken(
        kind: SovereignMarkdownInlineKind.strong,
        type: 'strong',
        sourceRange: const SovereignSourceRange(0, 8),
      ),
    ],
    hiddenRanges: [
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: const SovereignSourceRange(0, 2),
      ),
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
        type: 'inlineMarker',
        sourceRange: const SovereignSourceRange(6, 8),
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
