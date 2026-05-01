import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('Sovereign table key integration', () {
    testWidgets('Enter key continues an established table row', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        text: '| a | b |\n| --- | --- |\n| c | d |',
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

      expect(
        controller.text,
        equals('| a   | b   |\n| --- | --- |\n| c   | d   |\n|     |     |'),
      );
      expect(
        controller.selection.baseOffset,
        equals('| a   | b   |\n| --- | --- |\n| c   | d   |\n| '.length),
      );
    });

    testWidgets(
      'Enter key is ignored while IME composing is active in a table',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '| a | b |\n| --- | --- |\n| c | d |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final focusNode = FocusNode();
        addTearDown(focusNode.dispose);

        await _pumpEditor(tester, controller: controller, focusNode: focusNode);
        await _focusEditor(tester);

        final cOffset = controller.text.lastIndexOf('c');
        controller.value = TextEditingValue(
          text: controller.text,
          selection: TextSelection.collapsed(offset: controller.text.length),
          composing: TextRange(start: cOffset, end: cOffset + 1),
        );
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pump();

        expect(controller.text, equals('| a | b |\n| --- | --- |\n| c | d |'));
        expect(
          controller.value.composing,
          equals(TextRange(start: cOffset, end: cOffset + 1)),
        );
      },
    );

    testWidgets('Tab key navigates to next cell through widget shortcuts', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await _pumpEditor(tester, controller: controller, focusNode: focusNode);
      await _focusEditor(tester);

      final cOffset = controller.text.lastIndexOf('c');
      final dOffset = controller.text.lastIndexOf('d');
      controller.selection = TextSelection.collapsed(offset: cOffset);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();

      expect(controller.selection.baseOffset, equals(dOffset));
    });

    testWidgets(
      'Shift+Tab navigates to previous cell through widget shortcuts',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final focusNode = FocusNode();
        addTearDown(focusNode.dispose);

        await _pumpEditor(tester, controller: controller, focusNode: focusNode);
        await _focusEditor(tester);

        final cOffset = controller.text.lastIndexOf('c');
        final dOffset = controller.text.lastIndexOf('d');
        controller.selection = TextSelection.collapsed(offset: dOffset);
        await tester.pump();

        await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
        await tester.pump();

        expect(controller.selection.baseOffset, equals(cOffset));
      },
    );

    testWidgets(
      'Tab on last cell inserts a new aligned row via widget shortcut',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '| longer | x   |\n| ------ | --- |\n| c      | d   |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final focusNode = FocusNode();
        addTearDown(focusNode.dispose);

        await _pumpEditor(tester, controller: controller, focusNode: focusNode);
        await _focusEditor(tester);

        controller.selection = TextSelection.collapsed(
          offset: controller.text.lastIndexOf('d'),
        );
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pump();

        expect(
          controller.text,
          equals(
            '| longer | x   |\n'
            '| ------ | --- |\n'
            '| c      | d   |\n'
            '|        |     |',
          ),
        );
        expect(
          controller.selection.baseOffset,
          equals(
            '| longer | x   |\n| ------ | --- |\n| c      | d   |\n| '.length,
          ),
        );
      },
    );

    testWidgets(
      'Tab inside fenced code uses fence indentation instead of table nav',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '```\n| a | b |\n```',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final focusNode = FocusNode();
        addTearDown(focusNode.dispose);

        await _pumpEditor(tester, controller: controller, focusNode: focusNode);
        await _focusEditor(tester);

        controller.selection = TextSelection.collapsed(
          offset: controller.text.indexOf('a'),
        );
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pump();

        expect(controller.text, contains('|   a | b |'));
        expect(
          controller.selection.baseOffset,
          isNot(equals(controller.text.indexOf('b'))),
        );
      },
    );

    testWidgets(
      'Quoted tables use quote Enter continuation, not table continuation',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '> | a | b |\n> | --- | --- |\n> | c | d |',
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

        expect(
          controller.text,
          equals('> | a | b |\n> | --- | --- |\n> | c | d |\n> '),
        );
      },
    );

    testWidgets('Tab falls through to focus traversal when not in a table', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        text: 'alpha | beta',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final editorFocus = FocusNode();
      final nextFocus = FocusNode();
      addTearDown(editorFocus.dispose);
      addTearDown(nextFocus.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Column(
              children: [
                SizedBox(
                  height: 180,
                  child: SovereignEditor(
                    controller: controller,
                    focusNode: editorFocus,
                    enableTestShortcuts: true,
                  ),
                ),
                TextField(focusNode: nextFocus),
              ],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await _focusEditor(tester);
      expect(editorFocus.hasFocus, isTrue);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();

      expect(controller.text, equals('alpha | beta'));
      expect(nextFocus.hasFocus, isTrue);
    });

    testWidgets(
      'Tab does not traverse focus while IME composing is active in a table',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final editorFocus = FocusNode();
        final nextFocus = FocusNode();
        addTearDown(editorFocus.dispose);
        addTearDown(nextFocus.dispose);

        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: Column(
                children: [
                  SizedBox(
                    height: 180,
                    child: SovereignEditor(
                      controller: controller,
                      focusNode: editorFocus,
                      enableTestShortcuts: true,
                    ),
                  ),
                  TextField(focusNode: nextFocus),
                ],
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();

        await _focusEditor(tester);
        expect(editorFocus.hasFocus, isTrue);

        final cOffset = controller.text.lastIndexOf('c');
        controller.value = TextEditingValue(
          text: controller.text,
          selection: TextSelection.collapsed(offset: cOffset),
          composing: TextRange(start: cOffset, end: cOffset + 1),
        );
        await tester.pump();

        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pumpAndSettle();

        expect(editorFocus.hasFocus, isTrue);
        expect(nextFocus.hasFocus, isFalse);
        expect(controller.selection.baseOffset, equals(cOffset));
        expect(
          controller.text,
          equals('| a   | b   |\n| --- | --- |\n| c   | d   |'),
        );
        expect(
          controller.value.composing,
          equals(TextRange(start: cOffset, end: cOffset + 1)),
        );
      },
    );

    testWidgets(
      'Malformed separator + escaped pipe + IME composing does not mutate or traverse focus on Tab',
      (WidgetTester tester) async {
        final controller = SovereignController(
          text: '| a \\| b | c |\n| --- |\n| x | y |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);
        final editorFocus = FocusNode();
        final nextFocus = FocusNode();
        addTearDown(editorFocus.dispose);
        addTearDown(nextFocus.dispose);

        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: Column(
                children: [
                  SizedBox(
                    height: 180,
                    child: SovereignEditor(
                      controller: controller,
                      focusNode: editorFocus,
                      enableTestShortcuts: true,
                    ),
                  ),
                  TextField(focusNode: nextFocus),
                ],
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();

        await _focusEditor(tester);
        expect(editorFocus.hasFocus, isTrue);

        final escapedCellOffset = controller.text.indexOf(r'a \| b');
        controller.value = TextEditingValue(
          text: controller.text,
          selection: TextSelection.collapsed(offset: escapedCellOffset),
          composing: TextRange(
            start: escapedCellOffset,
            end: escapedCellOffset + 1,
          ),
        );
        await tester.pump();

        final before = controller.text;
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pumpAndSettle();

        expect(controller.text, equals(before));
        expect(editorFocus.hasFocus, isTrue);
        expect(nextFocus.hasFocus, isFalse);
        expect(controller.value.composing.isValid, isTrue);

        final hidden = controller.decoration.hiddenRanges;
        var prevEnd = 0;
        for (final range in hidden) {
          expect(
            range.start >= prevEnd,
            isTrue,
            reason:
                'hidden ranges overlap after malformed-table IME tab: $hidden',
          );
          prevEnd = range.end;
        }
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
        body: SovereignEditor(
          controller: controller,
          focusNode: focusNode,
          enableTestShortcuts: true,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _focusEditor(WidgetTester tester) async {
  final editor = tester.widget<SovereignEditor>(find.byType(SovereignEditor));
  final focusNode = editor.focusNode;
  if (focusNode != null) {
    focusNode.requestFocus();
    await tester.pump();
    return;
  }
  await tester.tap(find.byType(SovereignEditor), warnIfMissed: false);
  await tester.pump();
}
