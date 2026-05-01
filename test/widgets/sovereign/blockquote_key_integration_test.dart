import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('Sovereign blockquote key integration', () {
    testWidgets('Enter continues quoted task item and preserves quote mode', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        text: '> - [x] done',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await _pumpEditor(tester, controller: controller, focusNode: focusNode);
      await _focusEditor(tester);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('> - [x] done\n> - [ ] '));
      expect(controller.selection.baseOffset, equals(controller.text.length));

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('> - [x] done\n> - [ ] \n> '));
    });

    testWidgets('Third Enter on empty quoted line exits quote mode', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        text: '> - [x] done\n> - [ ] \n> ',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await _pumpEditor(tester, controller: controller, focusNode: focusNode);
      await _focusEditor(tester);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('> - [x] done\n> - [ ] \n\n'));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    testWidgets(
      'widget ArrowDown exits quote from last quoted task content line over trailing empty quote',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '> - [ ] todo\n> \nnext',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final focusNode = FocusNode();
        addTearDown(focusNode.dispose);

        await _pumpEditor(tester, controller: controller, focusNode: focusNode);
        await _focusEditor(tester);

        controller.selection = const TextSelection.collapsed(offset: 6);
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
        await tester.pump();

        expect(
          controller.selection.baseOffset,
          equals('> - [ ] todo\n> \nnext'.length),
        );
      },
    );

    testWidgets(
      'ArrowUp exits quote from first quoted task content line over leading empty quote',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: 'before\n> \n> - [ ] todo',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final focusNode = FocusNode();
        addTearDown(focusNode.dispose);

        await _pumpEditor(tester, controller: controller, focusNode: focusNode);
        await _focusEditor(tester);

        final quoteLineStart = controller.text.lastIndexOf('> - [ ] todo');
        controller.selection = TextSelection.collapsed(
          offset: quoteLineStart + 6,
        );
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
        await tester.pump();

        // In the widget shortcut path, the raw column is preserved and clamped
        // to the previous line length ("before" -> 6 chars).
        expect(controller.selection.baseOffset, equals(6));
      },
    );
  });
}

Future<void> _pumpEditor(
  WidgetTester tester, {
  required SovereignController controller,
  required FocusNode focusNode,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SizedBox(
          height: 220,
          child: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            enableTestShortcuts: true,
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _focusEditor(WidgetTester tester) async {
  await tester.tap(find.byType(TextField).first, warnIfMissed: false);
  await tester.pump();
}
