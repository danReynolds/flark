import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets(
    'Backspace on hidden closing marker of empty fence is non-destructive',
    (WidgetTester tester) async {
      const text = 'before\n```\n```\nafter';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditor(controller: controller, focusNode: focusNode),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      final closeStart = text.lastIndexOf('```');
      expect(closeStart, isNonNegative);
      final closeEnd = closeStart + 3;

      controller.selection = TextSelection.collapsed(offset: closeEnd);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();

      expect(controller.text, equals(text));
      expect(controller.selection.baseOffset, equals(closeStart - 1));
    },
  );

  testWidgets('Backspace on hidden closing fence marker is non-destructive', (
    WidgetTester tester,
  ) async {
    const text = '```\nfinal x = 2;\n```\n';
    final controller = SovereignController(text: text);
    addTearDown(controller.dispose);
    final focusNode = FocusNode();
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: focusNode),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    final closeStart = text.lastIndexOf('```');
    expect(closeStart, isNonNegative);
    final closeEnd = closeStart + 3;

    // Put caret at the end boundary of the hidden closing fence marker.
    controller.selection = TextSelection.collapsed(offset: closeEnd);
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();

    // Guard: preserve fence text and remap caret to previous visible boundary.
    expect(controller.text, equals(text));
    expect(controller.selection.baseOffset, equals(closeStart - 1));
    expect(controller.text.substring(closeStart, closeEnd), equals('```'));
  });
}
