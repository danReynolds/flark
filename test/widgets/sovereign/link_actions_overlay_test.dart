import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('Link actions overlay appears at caret inside markdown link', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: 'Visit [Google](https://google.com) now',
    );
    final focusNode = FocusNode();

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: focusNode),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    final caret = controller.text.indexOf('Google') + 2;
    controller.selection = TextSelection.collapsed(offset: caret);
    await tester.pump();
    await tester.pump();

    expect(
      find.byKey(const Key('SovereignLinkActionsOverlay')),
      findsOneWidget,
    );
    expect(find.text('Copy'), findsOneWidget);
    expect(find.text('Edit'), findsOneWidget);
  });

  testWidgets('Link actions overlay invokes onOpenLink callback', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: 'Visit [Google](https://google.com) now',
    );
    final focusNode = FocusNode();
    String? opened;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            onOpenLink: (url) async {
              opened = url;
            },
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    final caret = controller.text.indexOf('Google') + 1;
    controller.selection = TextSelection.collapsed(offset: caret);
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Open'));
    await tester.pump();

    expect(opened, 'https://google.com');
  });

  testWidgets('Link actions overlay is anchored near the active link text', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '[Google](https://google.com)\n\nTrailing text',
    );
    final focusNode = FocusNode();

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 700,
            child: SovereignEditor(
              controller: controller,
              focusNode: focusNode,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('Google') + 3,
    );
    await tester.pump();
    await tester.pump();

    final topLeft = tester.getTopLeft(
      find.byKey(const Key('SovereignLinkActionsOverlay')),
    );
    expect(topLeft.dx, lessThan(280));
    expect(topLeft.dy, greaterThan(0));
  });

  testWidgets('Link actions overlay stays hidden inside fenced code', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '```\n[Google](https://google.com)\n```',
    );
    final focusNode = FocusNode();

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: focusNode),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    final caret = controller.text.indexOf('Google') + 2;
    controller.selection = TextSelection.collapsed(offset: caret);
    await tester.pump();
    await tester.pump();

    expect(find.byKey(const Key('SovereignLinkActionsOverlay')), findsNothing);
  });

  testWidgets('Reference-style link overlay resolves definition URL for open', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '[Docs][api]\n\n[api]: https://dune.ai/docs',
    );
    final focusNode = FocusNode();
    String? opened;
    addTearDown(controller.dispose);
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            onOpenLink: (url) async => opened = url,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('Docs') + 1,
    );
    await tester.pump();
    await tester.pump();

    expect(
      find.byKey(const Key('SovereignLinkActionsOverlay')),
      findsOneWidget,
    );
    await tester.tap(find.text('Open'));
    await tester.pump();

    expect(opened, 'https://dune.ai/docs');
  });

  testWidgets('Shortcut reference link opens definition URL', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '[Docs]\n\n[docs]: https://dune.ai/docs',
    );
    final focusNode = FocusNode();
    String? opened;
    addTearDown(controller.dispose);
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            onOpenLink: (url) async => opened = url,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('Docs') + 1,
    );
    await tester.pump();
    await tester.pump();

    expect(
      find.byKey(const Key('SovereignLinkActionsOverlay')),
      findsOneWidget,
    );
    await tester.tap(find.text('Open'));
    await tester.pump();

    expect(opened, 'https://dune.ai/docs');
  });

  testWidgets(
    'Reference-style link edit preserves reference syntax and updates definition URL',
    (WidgetTester tester) async {
      final controller = SovereignController(
        text: '[Docs][api]\n\n[api]: https://dune.ai/docs',
      );
      final focusNode = FocusNode();
      addTearDown(controller.dispose);
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditor(controller: controller, focusNode: focusNode),
          ),
        ),
      );
      await tester.pumpAndSettle();

      focusNode.requestFocus();
      controller.selection = TextSelection.collapsed(
        offset: controller.text.indexOf('Docs') + 1,
      );
      await tester.pump();
      await tester.pump();

      await tester.tap(find.text('Edit'));
      await tester.pumpAndSettle();

      final dialogFields = find.descendant(
        of: find.byType(AlertDialog),
        matching: find.byType(TextField),
      );
      expect(dialogFields, findsNWidgets(2));
      await tester.enterText(dialogFields.at(0), 'Docs API');
      await tester.enterText(dialogFields.at(1), 'https://dune.ai/new-docs');
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();

      expect(
        controller.text,
        '[Docs API][api]\n\n[api]: https://dune.ai/new-docs',
      );
    },
  );

  testWidgets('Unresolved reference link edit starts with empty URL field', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '[Docs][api]');
    final focusNode = FocusNode();
    addTearDown(controller.dispose);
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: focusNode),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('Docs') + 1,
    );
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Edit'));
    await tester.pumpAndSettle();

    final dialogFields = find.descendant(
      of: find.byType(AlertDialog),
      matching: find.byType(TextField),
    );
    final urlField = tester.widget<TextField>(dialogFields.at(1));
    expect(urlField.controller?.text, isEmpty);
  });
}
