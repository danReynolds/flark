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
  // The editor owns its parser once a surface attaches; force a real parse so
  // the live-rendered block tree (and our popover hook) is built.
  await tester.runAsync(() => controller.parseNow());
  await tester.pump();
  // Focus the editor so it renders the editable multi-block tree (unfocused it
  // shows a plain fallback editable).
  await tester.tap(find.byType(EditableText).first);
  await tester.pump();
  await tester.pump();
}

void main() {
  final destinationFinder = find.byKey(const Key('FlarkLinkPopoverDestination'));

  testWidgets('shows the popover with actions when the caret enters a link', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '[Test](https://example.com) tail',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    // No link under the caret yet (caret at 0, before the link).
    expect(destinationFinder, findsNothing);

    // Place the caret inside the link label and let the portal show.
    controller.applySelection(const FlarkSelection.collapsed(3), userEvent: 't');
    await tester.pump();
    await tester.pump();

    expect(destinationFinder, findsOneWidget);
    expect(
      tester.widget<Text>(destinationFinder).data,
      'https://example.com',
    );
    // Default actions render (Open, Edit, Copy, Remove).
    expect(find.byKey(const ValueKey('FlarkLinkAction.open')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.edit')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.copy')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.remove')), findsOneWidget);
  });

  testWidgets('hides the popover when the caret leaves the link', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '[Test](https://example.com) tail',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(tester, controller);

    controller.applySelection(const FlarkSelection.collapsed(3), userEvent: 't');
    await tester.pump();
    await tester.pump();
    expect(destinationFinder, findsOneWidget);

    // Move the caret out into the trailing text.
    controller.applySelection(
      FlarkSelection.collapsed(controller.markdown.length),
      userEvent: 't',
    );
    await tester.pump();
    await tester.pump();
    expect(destinationFinder, findsNothing);
  });

  testWidgets('opens the link through the configured callback', (tester) async {
    final opened = <String>[];
    final controller = FlarkFlutterController.fromMarkdown(
      '[Test](https://example.com)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(
      tester,
      controller,
      config: FlarkMarkdownInteractionConfig(onOpenLink: opened.add),
    );

    controller.applySelection(const FlarkSelection.collapsed(3), userEvent: 't');
    await tester.pump();
    await tester.pump();

    await tester.tap(find.byKey(const ValueKey('FlarkLinkAction.open')));
    await tester.pump();
    expect(opened, ['https://example.com']);
  });

  testWidgets('custom link actions replace the defaults', (tester) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '[Test](https://example.com)',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);
    await _parse(controller);
    await _pumpEditor(
      tester,
      controller,
      config: FlarkMarkdownInteractionConfig(
        linkActions: [
          const FlarkLinkAction(
            id: 'unfurl',
            label: 'Unfurl',
            onInvoke: _noop,
          ),
        ],
      ),
    );

    controller.applySelection(const FlarkSelection.collapsed(3), userEvent: 't');
    await tester.pump();
    await tester.pump();

    expect(find.byKey(const ValueKey('FlarkLinkAction.unfurl')), findsOneWidget);
    expect(find.byKey(const ValueKey('FlarkLinkAction.open')), findsNothing);
  });
}

void _noop(FlarkLinkActionContext link) {}
