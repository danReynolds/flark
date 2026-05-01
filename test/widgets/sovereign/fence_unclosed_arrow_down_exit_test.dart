import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  test('ArrowDown selection move in unclosed fence does not insert close', () {
    final controller = SovereignController(text: '```\ncode\n');
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\ncode\n',
      selection: TextSelection.collapsed(offset: 8),
    );

    // Simulate an ArrowDown selection move from the code line into trailing
    // blank line without an explicit keyboard-intent auto-close.
    controller.value = const TextEditingValue(
      text: '```\ncode\n',
      selection: TextSelection.collapsed(offset: 9),
    );

    expect(controller.text, equals('```\ncode\n'));
    expect(controller.selection.baseOffset, equals(9));
  });

  testWidgets('ArrowDown at EOF closes an unclosed fence and exits', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '```\ncode\n');
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

    controller.selection = TextSelection.collapsed(
      offset: controller.text.length,
    );
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();

    expect(controller.text, equals('```\ncode\n```\n'));
    expect(controller.selection.baseOffset, equals('```\ncode\n```\n'.length));
  });

  testWidgets(
    'ArrowDown at EOF trims trailing blank lines before closing unclosed fence',
    (WidgetTester tester) async {
      final controller = SovereignController(text: '```\ncode\n\n\n');
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

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();

      expect(controller.text, equals('```\ncode\n```\n'));
      expect(
        controller.selection.baseOffset,
        equals('```\ncode\n```\n'.length),
      );
    },
  );

  testWidgets(
    'ArrowDown on last nonblank line of unclosed fence does not mutate text',
    (WidgetTester tester) async {
      final controller = SovereignController(text: '```\ncode');
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

      controller.selection = const TextSelection.collapsed(
        offset: '```\ncode'.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();

      expect(controller.text, equals('```\ncode'));
      expect(controller.selection.baseOffset, equals('```\ncode'.length));
    },
  );
}
