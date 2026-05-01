import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('Sovereign heading key integration', () {
    testWidgets('Enter on empty heading exits heading mode in widget path', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        text: '# ',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await _pumpEditor(tester, controller: controller, focusNode: focusNode);
      await _focusEditor(tester);

      controller.selection = const TextSelection.collapsed(offset: 2);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, '\n');
      expect(controller.selection.baseOffset, 1);
    });

    testWidgets('Enter on non-empty heading splits line normally', (
      WidgetTester tester,
    ) async {
      const initial = '## Title';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await _pumpEditor(tester, controller: controller, focusNode: focusNode);
      await _focusEditor(tester);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, '$initial\n');
      expect(controller.selection.baseOffset, initial.length + 1);
    });

    testWidgets(
      'Enter on empty heading inside quote exits heading but keeps quote',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '> # ',
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

        // Quote policy should remain authoritative for quote-contained headings.
        expect(controller.text, '> # \n> ');
        expect(controller.selection.baseOffset, controller.text.length);
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
          height: 180,
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
