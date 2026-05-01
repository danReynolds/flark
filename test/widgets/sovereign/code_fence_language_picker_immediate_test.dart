import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/sovereign_editor.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/painters/tier1_painter.dart';

void main() {
  testWidgets(
    'Language picker is visible immediately for an unclosed EOF fence',
    (WidgetTester tester) async {
      final controller = SovereignController();
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      controller.value = const TextEditingValue(
        text: '```\n',
        selection: TextSelection.collapsed(offset: 4),
      );

      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(useMaterial3: false),
          home: Scaffold(
            body: SovereignEditor(controller: controller, focusNode: focusNode),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final picker = find.byKey(const Key('SovereignCodeFenceLanguagePicker'));
      expect(picker, findsOneWidget);

      // Ensure selecting a language updates the opening fence even when the caret
      // sits at the unclosed fence's endOffset (EOF).
      await tester.tap(picker);
      await tester.pumpAndSettle();

      await tester.tap(find.text('Dart'));
      await tester.pumpAndSettle();

      expect(controller.text, equals('```dart\n'));
    },
  );

  testWidgets('Language picker style is customizable and width-constrained', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController();
    addTearDown(controller.dispose);
    final focusNode = FocusNode();
    addTearDown(focusNode.dispose);

    controller.value = const TextEditingValue(
      text: '```\n',
      selection: TextSelection.collapsed(offset: 4),
    );

    const pickerTheme = SovereignFenceLanguagePickerTheme(
      maxWidth: 72,
      textStyle: TextStyle(
        color: Colors.amber,
        fontSize: 10,
        fontWeight: FontWeight.w700,
      ),
      backgroundColor: Color(0xFF102030),
      borderColor: Color(0xFF405060),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 96,
            child: SovereignEditor(
              controller: controller,
              focusNode: focusNode,
              theme: const SovereignEditorThemeData(
                codeBlock: SovereignCodeBlockTheme(languagePicker: pickerTheme),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final picker = find.byKey(const Key('SovereignCodeFenceLanguagePicker'));
    expect(picker, findsOneWidget);
    expect(tester.getSize(picker).width, lessThanOrEqualTo(72.0));

    final labelText = tester.widget<Text>(
      find.descendant(of: picker, matching: find.text('Plain')).first,
    );
    expect(labelText.style?.color, equals(Colors.amber));
    expect(labelText.style?.fontSize, equals(10));
  });

  testWidgets('Fenced area painter style is configurable', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '```\ncode\n```');
    addTearDown(controller.dispose);
    final focusNode = FocusNode();
    addTearDown(focusNode.dispose);

    const theme = SovereignEditorThemeData(
      codeBlock: SovereignCodeBlockTheme(backgroundColor: Color(0xFF123456)),
      blockquote: SovereignBlockquoteTheme(
        railColor: Color(0xFFABCDEF),
        railWidth: 6,
        railInset: 4,
        railRadius: Radius.circular(3),
      ),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            theme: theme,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final paints = tester.widgetList<CustomPaint>(find.byType(CustomPaint));
    final tier1 = paints.map((p) => p.painter).whereType<Tier1Painter>().first;

    expect(tier1.codeBlockBackgroundColor, equals(const Color(0xFF123456)));
    expect(tier1.quoteRailColor, equals(const Color(0xFFABCDEF)));
    expect(tier1.quoteRailWidth, equals(6));
    expect(tier1.quoteRailInset, equals(4));
  });

  testWidgets('Language picker aligns to fenced background insets', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '```\ncode\n```');
    addTearDown(controller.dispose);
    final focusNode = FocusNode();
    addTearDown(focusNode.dispose);

    const theme = SovereignEditorThemeData(
      editorContentPadding: EdgeInsets.fromLTRB(12, 10, 14, 0),
      codeBlock: SovereignCodeBlockTheme(
        backgroundHorizontalInset: 6,
        backgroundVerticalInset: 3,
        languagePicker: SovereignFenceLanguagePickerTheme(
          margin: EdgeInsets.zero,
          verticalOffset: 0,
        ),
      ),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            child: SovereignEditor(
              controller: controller,
              focusNode: focusNode,
              theme: theme,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    controller.selection = const TextSelection.collapsed(offset: 4);
    await tester.pumpAndSettle();

    final picker = find.byKey(const Key('SovereignCodeFenceLanguagePicker'));
    expect(picker, findsOneWidget);

    final editorRect = tester.getRect(find.byType(SovereignEditor));
    final pickerRect = tester.getRect(picker);

    // Right anchor follows editor content + fenced background inset.
    final expectedRightGap = 14.0 + 6.0;
    expect(
      editorRect.right - pickerRect.right,
      closeTo(expectedRightGap, 1.0),
      reason:
          'Picker should align with the painted fenced block right edge, not '
          'the raw editor edge.',
    );

    // Top anchor includes editor top padding.
    expect(
      pickerRect.top,
      greaterThan(editorRect.top + 8.0),
      reason: 'Picker should not ignore editor top content padding.',
    );
  });
}
