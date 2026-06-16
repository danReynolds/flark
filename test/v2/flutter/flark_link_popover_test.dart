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

/// Taps near the start of the editable, over the link label.
Future<void> _tapLink(WidgetTester tester) async {
  final topLeft = tester.getTopLeft(find.byType(EditableText).first);
  await tester.tapAt(topLeft + const Offset(18, 8));
  await tester.pump();
  await tester.pump();
}

void main() {
  final destinationFinder = find.byKey(const Key('FlarkLinkPopoverDestination'));

  testWidgets('a tap on a link opens the popover with the default actions', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '[clickable link](https://example.com)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    expect(destinationFinder, findsNothing);
    await _tapLink(tester);

    expect(destinationFinder, findsOneWidget);
    expect(tester.widget<Text>(destinationFinder).data, 'https://example.com');
    expect(find.byKey(const ValueKey('FlarkLinkAction.open')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.edit')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.copy')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.remove')), findsOneWidget);
  });

  testWidgets('the caret merely landing in a link does not open it', (
    tester,
  ) async {
    // Finishing link markdown leaves the caret inside the link — that alone
    // must not pop the menu; only a deliberate tap does.
    final controller = FlarkFlutterController.fromMarkdown(
      '[clickable link](https://example.com) tail',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    controller.applySelection(const FlarkSelection.collapsed(3), userEvent: 't');
    await tester.pump();
    await tester.pump();
    expect(destinationFinder, findsNothing);
  });

  testWidgets('the popover closes when the caret leaves the link', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '[clickable link](https://example.com) tail',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    await _tapLink(tester);
    expect(destinationFinder, findsOneWidget);

    controller.applySelection(
      FlarkSelection.collapsed(controller.markdown.length),
      userEvent: 't',
    );
    await tester.pump();
    await tester.pump();
    expect(destinationFinder, findsNothing);
  });

  testWidgets('Open invokes the configured callback', (tester) async {
    final opened = <String>[];
    final controller = FlarkFlutterController.fromMarkdown(
      '[clickable link](https://example.com)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(
      tester,
      controller,
      config: FlarkMarkdownInteractionConfig(onOpenLink: opened.add),
    );

    await _tapLink(tester);
    await tester.tap(find.byKey(const ValueKey('FlarkLinkAction.open')));
    await tester.pump();
    expect(opened, ['https://example.com']);
  });

  testWidgets('custom link actions replace the defaults', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '[clickable link](https://example.com)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(
      tester,
      controller,
      config: FlarkMarkdownInteractionConfig(
        linkActions: [
          const FlarkLinkAction(id: 'unfurl', label: 'Unfurl', onInvoke: _noop),
        ],
      ),
    );

    await _tapLink(tester);
    expect(find.byKey(const ValueKey('FlarkLinkAction.unfurl')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.open')), findsNothing);
  });
}

void _noop(FlarkLinkActionContext link) {}
