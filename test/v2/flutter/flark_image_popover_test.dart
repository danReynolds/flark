import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
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

Future<void> _pumpEditor(
  WidgetTester tester,
  FlarkFlutterController controller, {
  FlarkMarkdownInteractionConfig config = const FlarkMarkdownInteractionConfig(),
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: FlarkMarkdownEditor(
          controller: controller,
          editingMode: FlarkMarkdownEditingMode.liveRendered,
          interactionConfig: config,
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.runAsync(() => controller.parseNow());
  await tester.pump();
}

Future<void> _tapStart(WidgetTester tester) async {
  final topLeft = tester.getTopLeft(find.byType(EditableText).first);
  await tester.tapAt(topLeft + const Offset(18, 8));
  await tester.pump();
  await tester.pump();
}

void main() {
  final imageDest = find.byKey(const Key('FlarkImagePopoverDestination'));
  final linkDest = find.byKey(const Key('FlarkLinkPopoverDestination'));

  testWidgets('a tap on an image opens the image popover', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '![a wide image alt](https://example.com/cat.png)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    expect(imageDest, findsNothing);
    await _tapStart(tester);

    expect(imageDest, findsOneWidget);
    // The image popover, not the link one (the link pattern also matches the
    // `[alt](url)` inside an image).
    expect(linkDest, findsNothing);
    expect(
      tester.widget<Text>(imageDest).data,
      'https://example.com/cat.png',
    );
    expect(find.byKey(const ValueKey('FlarkImageAction.open')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkImageAction.edit')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkImageAction.copy')), findsOneWidget);
    expect(
      find.byKey(const ValueKey('FlarkImageAction.remove')),
      findsOneWidget,
    );
  });

  testWidgets('the caret merely landing in an image does not open it', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '![a wide image alt](https://example.com/cat.png) tail',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    controller.applySelection(const FlarkSelection.collapsed(4), userEvent: 't');
    await tester.pump();
    await tester.pump();
    expect(imageDest, findsNothing);
  });

  testWidgets('Open invokes onOpenImage and Remove deletes the image', (
    tester,
  ) async {
    final opened = <String>[];
    final controller = FlarkFlutterController.fromMarkdown(
      '![a wide image alt](https://example.com/cat.png)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(
      tester,
      controller,
      config: FlarkMarkdownInteractionConfig(onOpenImage: opened.add),
    );

    await _tapStart(tester);
    await tester.tap(find.byKey(const ValueKey('FlarkImageAction.open')));
    await tester.pump();
    expect(opened, ['https://example.com/cat.png']);

    await tester.tap(find.byKey(const ValueKey('FlarkImageAction.remove')));
    await tester.pump();
    expect(controller.markdown, '');
  });

  testWidgets('custom image actions replace the defaults', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '![a wide image alt](https://example.com/cat.png)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(
      tester,
      controller,
      config: FlarkMarkdownInteractionConfig(
        imageActions: [
          const FlarkImageAction(id: 'caption', label: 'Caption', onInvoke: _noop),
        ],
      ),
    );

    await _tapStart(tester);
    expect(
      find.byKey(const ValueKey('FlarkImageAction.caption')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('FlarkImageAction.open')), findsNothing);
  });
}

void _noop(FlarkImageActionContext image) {}
