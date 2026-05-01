import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('cursor height follows heading text style at caret', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '# Heading',
      syntaxEngine: const V1SyntaxEngineAdapter(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, wrapText: true),
        ),
      ),
    );
    await tester.pumpAndSettle();

    controller.selection = TextSelection.collapsed(
      offset: controller.text.length,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    final headingField = tester.widget<TextField>(find.byType(TextField));
    final headingCursorHeight = headingField.cursorHeight ?? 0;

    controller.value = const TextEditingValue(
      text: 'Body',
      selection: TextSelection.collapsed(offset: 4),
      composing: TextRange.empty,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    final bodyField = tester.widget<TextField>(find.byType(TextField));
    final bodyCursorHeight = bodyField.cursorHeight ?? 0;

    expect(headingCursorHeight, greaterThan(bodyCursorHeight));
  });

  testWidgets('heading cursor height stays within fixed-line editor budget', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '# Heading',
      syntaxEngine: const V1SyntaxEngineAdapter(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            wrapText: true,
            textStyle: const TextStyle(fontSize: 18, height: 1.4),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    controller.selection = TextSelection.collapsed(
      offset: controller.text.length,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    final headingField = tester.widget<TextField>(find.byType(TextField));
    final headingCursorHeight = headingField.cursorHeight ?? 0;

    controller.value = const TextEditingValue(
      text: 'Body',
      selection: TextSelection.collapsed(offset: 4),
      composing: TextRange.empty,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    final bodyField = tester.widget<TextField>(find.byType(TextField));
    final bodyCursorHeight = bodyField.cursorHeight ?? 0;

    expect(headingCursorHeight, greaterThan(bodyCursorHeight));
    expect(headingCursorHeight, lessThanOrEqualTo(bodyCursorHeight * 1.4));
  });
}
