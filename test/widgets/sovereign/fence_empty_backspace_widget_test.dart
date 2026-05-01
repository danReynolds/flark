import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('Fenced backspace widget flow', () {
    testWidgets('Enter then immediate Backspace cancels empty fence opener', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      await _pumpFocusedEditor(tester, controller);

      controller.value = const TextEditingValue(
        text: '```',
        selection: TextSelection.collapsed(offset: 3),
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(controller.text, equals('```\n'));
      expect(controller.selection.baseOffset, equals(4));

      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      expect(controller.text, equals(''));
      expect(controller.selection.baseOffset, equals(0));
    });

    testWidgets(
      'Enter then immediate Backspace cancels opener only after existing text',
      (WidgetTester tester) async {
        final controller = SovereignController(
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        await _pumpFocusedEditor(tester, controller);

        controller.value = const TextEditingValue(
          text: 'before\n```',
          selection: TextSelection.collapsed(offset: 10),
        );
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pump();
        expect(controller.text, equals('before\n```\n'));
        expect(controller.selection.baseOffset, equals(11));

        await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
        await tester.pump();
        expect(controller.text, equals('before\n'));
        expect(controller.selection.baseOffset, equals('before\n'.length));
      },
    );

    testWidgets(
      'Backspace from blank line after first-line content keeps fence',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '```\nabc\n\n```',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        await _pumpFocusedEditor(tester, controller);

        controller.selection = const TextSelection.collapsed(offset: 8);
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
        await tester.pump();

        expect(controller.text, equals('```\nabc\n```'));
        expect(controller.selection.baseOffset, equals(7));
      },
    );
  });
}

Future<void> _pumpFocusedEditor(
  WidgetTester tester,
  SovereignController controller,
) async {
  final focusNode = FocusNode();
  addTearDown(focusNode.dispose);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SovereignEditor(
          controller: controller,
          focusNode: focusNode,
          enableTestShortcuts: true,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.byType(SovereignEditor));
  await tester.pump();
}
