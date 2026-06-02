import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('FlarkRenderPlanOverlayControls', () {
    testWidgets('renders typed controls from render-plan overlay targets', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown(
        'link image task table code',
      );
      addTearDown(controller.dispose);
      controller.applyParseResult(_overlayParseResult(controller));
      final pressed = <FlarkRenderOverlayTarget>[];

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkRenderPlanOverlayControls(
            controller: controller,
            onPressed: pressed.add,
          ),
        ),
      );

      expect(find.text('Link: https://example.com'), findsOneWidget);
      expect(find.text('Image: diagram'), findsOneWidget);
      expect(find.text('Task: checked'), findsOneWidget);
      expect(find.text('Table: 2 columns'), findsOneWidget);
      expect(find.text('Code: dart'), findsOneWidget);

      await tester.tap(find.text('Link: https://example.com'));
      await tester.pump();

      expect(pressed.single.kind, FlarkRenderOverlayKind.link);
      expect(pressed.single.action!.destination, 'https://example.com');
    });

    testWidgets('hides controls while render-plan state is stale', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('link');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkRenderPlanOverlayControls(controller: controller),
        ),
      );

      expect(find.byType(FlarkRenderPlanOverlayControls), findsOneWidget);
      expect(find.textContaining('Link'), findsNothing);
    });
  });
}

FlarkMarkdownParseResult _overlayParseResult(
  FlarkFlutterController controller,
) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.state.document.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.paragraph,
        type: 'paragraph',
        sourceRange: const FlarkSourceRange(0, 10),
      ),
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.listItem,
        type: 'listItem',
        sourceRange: const FlarkSourceRange(11, 15),
        attributes: const {'checked': true},
      ),
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.table,
        type: 'table',
        sourceRange: const FlarkSourceRange(16, 21),
        attributes: const {
          'alignments': ['left', 'right'],
        },
      ),
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: const FlarkSourceRange(22, 26),
        attributes: const {'language': 'dart'},
      ),
    ],
    inlineTokens: [
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.link,
        type: 'link',
        sourceRange: const FlarkSourceRange(0, 4),
        attributes: const {'destination': 'https://example.com'},
      ),
      FlarkMarkdownInlineToken(
        kind: FlarkMarkdownInlineKind.image,
        type: 'image',
        sourceRange: const FlarkSourceRange(5, 10),
        attributes: const {'src': 'asset://diagram.png', 'alt': 'diagram'},
      ),
    ],
  );
}
