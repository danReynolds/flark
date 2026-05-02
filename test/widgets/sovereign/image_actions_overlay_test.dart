import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';
import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

void main() {
  testWidgets('Image actions overlay appears at caret inside image alt text', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: 'See ![diagram](https://cdn.example/diagram.png) here',
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
      offset: controller.text.indexOf('diagram') + 2,
    );
    await tester.pump();
    await tester.pump();

    expect(
      find.byKey(const Key('SovereignLinkActionsOverlay')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('SovereignInlineImagePreview')),
      findsOneWidget,
    );
    expect(find.text('Open'), findsOneWidget);
    expect(find.text('Copy'), findsOneWidget);
    expect(find.text('Edit'), findsOneWidget);
  });

  testWidgets(
    'Standalone image line uses a larger preview card than inline image',
    (WidgetTester tester) async {
      final inlineController = SovereignController(
        text: 'Inline ![diagram](https://cdn.example/diagram.png) text',
      );
      final inlineFocus = FocusNode();
      addTearDown(inlineController.dispose);
      addTearDown(inlineFocus.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 700,
              child: SovereignEditor(
                controller: inlineController,
                focusNode: inlineFocus,
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      inlineFocus.requestFocus();
      inlineController.selection = TextSelection.collapsed(
        offset: inlineController.text.indexOf('diagram') + 1,
      );
      await tester.pump();
      await tester.pump();

      final inlinePreviewWidth = tester
          .getSize(find.byKey(const Key('SovereignInlineImagePreview')))
          .width;

      final standaloneController = SovereignController(
        text: '![diagram](https://cdn.example/diagram.png)',
      );
      final standaloneFocus = FocusNode();
      addTearDown(standaloneController.dispose);
      addTearDown(standaloneFocus.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 700,
              child: SovereignEditor(
                controller: standaloneController,
                focusNode: standaloneFocus,
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      standaloneFocus.requestFocus();
      standaloneController.selection = TextSelection.collapsed(
        offset: standaloneController.text.indexOf('diagram') + 1,
      );
      await tester.pump();
      await tester.pump();

      final standalonePreviewWidth = tester
          .getSize(find.byKey(const Key('SovereignInlineImagePreview')))
          .width;
      expect(standalonePreviewWidth, greaterThan(inlinePreviewWidth));
    },
  );

  testWidgets(
    'Image actions overlay stays hidden when caret is after image syntax',
    (WidgetTester tester) async {
      final markdown = '![diagram](https://cdn.example/diagram.png)';
      final controller = SovereignController(text: markdown);
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
      controller.selection = TextSelection.collapsed(offset: markdown.length);
      await tester.pump();
      await tester.pump();

      expect(
        find.byKey(const Key('SovereignLinkActionsOverlay')),
        findsNothing,
      );
      expect(
        find.byKey(const Key('SovereignInlineImagePreview')),
        findsNothing,
      );
    },
  );

  testWidgets('Image preview caption uses themed styling', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '![diagram](https://cdn.example/diagram.png)',
    );
    final focusNode = FocusNode();
    addTearDown(controller.dispose);
    addTearDown(focusNode.dispose);
    const captionBg = Color(0xFF10161E);
    const captionBorder = Color(0xFF2A3340);
    const captionStyle = TextStyle(
      color: Color(0xFFECE7D8),
      fontSize: 12,
      fontWeight: FontWeight.w700,
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            theme: const SovereignEditorThemeData(
              linkActions: SovereignLinkActionsTheme(
                imageCaptionBackgroundColor: captionBg,
                imageCaptionBorderColor: captionBorder,
                imageCaptionTextStyle: captionStyle,
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('diagram') + 1,
    );
    await tester.pump();
    await tester.pump();

    final captionFinder = find.byKey(
      const Key('SovereignInlineImagePreviewCaption'),
    );
    expect(captionFinder, findsOneWidget);
    final captionContainer = tester.widget<Container>(captionFinder);
    final decoration = captionContainer.decoration! as BoxDecoration;
    expect(decoration.color, captionBg);
    final border = decoration.border! as Border;
    expect(border.top.color, captionBorder);

    final captionText = find.descendant(
      of: captionFinder,
      matching: find.text('diagram'),
    );
    expect(captionText, findsOneWidget);
    final textWidget = tester.widget<Text>(captionText);
    expect(textWidget.style?.color, captionStyle.color);
    expect(textWidget.style?.fontWeight, captionStyle.fontWeight);
  });

  testWidgets('Image actions overlay invokes onOpenLink with image url', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '![img](https://cdn.example/image.png)',
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
      offset: controller.text.indexOf('img') + 1,
    );
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Open'));
    await tester.pump();

    expect(opened, 'https://cdn.example/image.png');
  });

  testWidgets('Standalone image preview tap invokes onOpenLink callback', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '![hero](https://cdn.example/hero.png)',
    );
    final focusNode = FocusNode();
    String? opened;
    addTearDown(controller.dispose);
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 700,
            child: SovereignEditor(
              controller: controller,
              focusNode: focusNode,
              onOpenLink: (url) async => opened = url,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('hero') + 1,
    );
    await tester.pump();
    await tester.pump();

    await tester.tap(
      find.byKey(const Key('SovereignInlineImagePreviewImageArea')),
    );
    await tester.pump();

    expect(opened, 'https://cdn.example/hero.png');
  });

  testWidgets('Image preview error state shows retry action', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '![broken](https://example.invalid/not-found.png)',
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
      offset: controller.text.indexOf('broken') + 1,
    );
    await tester.pump();
    await tester.pumpAndSettle();

    expect(
      find.byKey(const Key('SovereignInlineImagePreviewRetry')),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const Key('SovereignInlineImagePreviewRetry')));
    await tester.pump();
  });

  testWidgets('Relative image target uses source-first preview placeholder', (
    WidgetTester tester,
  ) async {
    final controller =
        SovereignController(text: '![asset](images/diagram.png)');
    final focusNode = FocusNode();
    String? opened;
    addTearDown(controller.dispose);
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 700,
            child: SovereignEditor(
              controller: controller,
              focusNode: focusNode,
              onOpenLink: (url) async => opened = url,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    focusNode.requestFocus();
    controller.selection = TextSelection.collapsed(
      offset: controller.text.indexOf('asset') + 1,
    );
    await tester.pump();
    await tester.pump();

    expect(
      find.byKey(const Key('SovereignInlineImagePreviewUnsupported')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('SovereignInlineImagePreviewRetry')),
      findsNothing,
    );
    expect(find.text('Preview unavailable'), findsOneWidget);
    expect(find.text('images/diagram.png'), findsOneWidget);

    await tester.tap(
      find.byKey(const Key('SovereignInlineImagePreviewImageArea')),
    );
    await tester.pump();

    expect(opened, 'images/diagram.png');
  });

  testWidgets('Image edit dialog updates alt text and URL', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '![old alt](https://cdn.example/old.png)',
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
      offset: controller.text.indexOf('old alt') + 2,
    );
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Edit'));
    await tester.pumpAndSettle();

    expect(find.text('Edit image'), findsOneWidget);

    final dialogFields = find.descendant(
      of: find.byType(AlertDialog),
      matching: find.byType(TextField),
    );
    expect(dialogFields, findsNWidgets(2));

    await tester.enterText(dialogFields.at(0), 'diagram');
    await tester.enterText(dialogFields.at(1), 'https://cdn.example/new.png');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();

    expect(controller.text, '![diagram](https://cdn.example/new.png)');
  });

  testWidgets('Image actions overlay stays hidden inside fenced code', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '```\n![diagram](https://cdn.example/a.png)\n```',
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
      offset: controller.text.indexOf('diagram') + 2,
    );
    await tester.pump();
    await tester.pump();

    expect(find.byKey(const Key('SovereignLinkActionsOverlay')), findsNothing);
  });
}
