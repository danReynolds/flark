import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

Future<void> _parse(FlarkFlutterController controller) async {
  final result = await FlarkNativeComrakParseBackend.withNativeBridge().parse(
    FlarkMarkdownParseRequest(
      revision: controller.state.revision,
      markdown: controller.markdown,
      profile: FlarkMarkdownProfile.commonMarkGfm,
    ),
  );
  expect(controller.applyParseResult(result), isTrue);
}

Future<void> _pump(WidgetTester tester, FlarkFlutterController controller) async {
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: FlarkLiveRenderedEditableText(
        controller: controller,
        style: const TextStyle(fontSize: 14),
      ),
    ),
  );
  await tester.pump();
}

void main() {
  final imageBlock = find.byKey(const Key('FlarkLiveBlockImage'));

  testWidgets('renders a standalone image as a block-level picture', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '![a curious owl](https://example.com/owl.png)',
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pump(tester, controller);

    // The picture renders (block mode kicked in) without raw markdown showing.
    expect(imageBlock, findsOneWidget);
    expect(find.textContaining('![a curious owl]'), findsNothing);
    // The alt text remains as the caption editable.
    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'a curious owl');
  });

  testWidgets('detects a standalone image in the middle of a document', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title\n\nIntro paragraph.\n\n'
      '![a curious owl](https://example.com/owl.png)\n\n'
      '- [x] a task\n',
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pump(tester, controller);

    // The block trails a newline the run doesn't, but it's still standalone.
    expect(imageBlock, findsOneWidget);
  });

  testWidgets('an image among other text does not become a block picture', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      'look ![a curious owl](https://example.com/owl.png) here',
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pump(tester, controller);

    // Inline-with-text images stay in the text flow (alt text), no image block.
    expect(imageBlock, findsNothing);
    expect(find.byType(EditableText), findsWidgets);
  });

  testWidgets('editing within the alt caption rewrites the source alt', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '![owl](https://example.com/owl.png)',
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pump(tester, controller);

    expect(imageBlock, findsOneWidget);
    // Insert inside the alt ("owl" -> "oXwl"); the picture stays a block.
    await tester.enterText(find.byType(EditableText), 'oXwl');
    await tester.pump();

    expect(controller.markdown, '![oXwl](https://example.com/owl.png)');
    expect(imageBlock, findsOneWidget);
  });

  testWidgets('removing the image collapses the block away', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '![owl](https://example.com/owl.png)',
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pump(tester, controller);

    expect(imageBlock, findsOneWidget);
    final context = controller.commands.resolveImageEditContext();
    final result = controller.commands.removeImage(
      imageRange: context.replaceRange,
    );
    expect(result.commandResult.isHandled, isTrue);
    await tester.runAsync(() => controller.parseNow());
    await tester.pump();

    expect(controller.markdown.trim(), isEmpty);
    expect(imageBlock, findsNothing);
  });
}
