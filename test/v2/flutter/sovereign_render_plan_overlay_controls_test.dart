import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';
import 'package:sovereign_editor/src/v2/render_plan/render_plan.dart';

void main() {
  group('SovereignRenderPlanOverlayControls', () {
    testWidgets('renders typed controls from render-plan overlay targets',
        (tester) async {
      final controller = SovereignFlutterController.fromMarkdown(
        'link image task table code',
      );
      addTearDown(controller.dispose);
      controller.applyParseResult(_overlayParseResult(controller));
      final pressed = <SovereignRenderOverlayTarget>[];

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignRenderPlanOverlayControls(
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

      expect(pressed.single.kind, SovereignRenderOverlayKind.link);
      expect(pressed.single.action!.destination, 'https://example.com');
    });

    testWidgets('hides controls while render-plan state is stale',
        (tester) async {
      final controller = SovereignFlutterController.fromMarkdown('link');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignRenderPlanOverlayControls(controller: controller),
        ),
      );

      expect(find.byType(SovereignRenderPlanOverlayControls), findsOneWidget);
      expect(find.textContaining('Link'), findsNothing);
    });
  });
}

SovereignMarkdownParseResult _overlayParseResult(
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
        sourceRange: const SovereignSourceRange(0, 10),
      ),
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.listItem,
        type: 'listItem',
        sourceRange: const SovereignSourceRange(11, 15),
        attributes: const {'checked': true},
      ),
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.table,
        type: 'table',
        sourceRange: const SovereignSourceRange(16, 21),
        attributes: const {
          'alignments': ['left', 'right'],
        },
      ),
      SovereignMarkdownBlockNode(
        kind: SovereignMarkdownBlockKind.codeBlock,
        type: 'codeBlock',
        sourceRange: const SovereignSourceRange(22, 26),
        attributes: const {'language': 'dart'},
      ),
    ],
    inlineTokens: [
      SovereignMarkdownInlineToken(
        kind: SovereignMarkdownInlineKind.link,
        type: 'link',
        sourceRange: const SovereignSourceRange(0, 4),
        attributes: const {'destination': 'https://example.com'},
      ),
      SovereignMarkdownInlineToken(
        kind: SovereignMarkdownInlineKind.image,
        type: 'image',
        sourceRange: const SovereignSourceRange(5, 10),
        attributes: const {'src': 'asset://diagram.png', 'alt': 'diagram'},
      ),
    ],
  );
}
