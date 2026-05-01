import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('Unclosed language fence exits on blank-line Enter', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '```dart\n');
    addTearDown(controller.dispose);
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

    controller.selection = TextSelection.collapsed(
      offset: controller.text.length,
    );
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();

    expect(controller.text, equals('```dart\n```\n'));
    expect(controller.selection.baseOffset, equals('```dart\n```\n'.length));
  });

  testWidgets('ArrowDown exits tagged fence with trailing blank lines', (
    WidgetTester tester,
  ) async {
    const text = '```dart\ncode\n\n```\nnext';
    const expected = '```dart\ncode\n```\nnext';
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

    final codeStart = text.indexOf('code');
    controller.selection = TextSelection.collapsed(
      offset: codeStart + 'code'.length,
    );
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();

    expect(controller.text, equals(expected));

    final nextStart = expected.indexOf('next');
    expect(controller.selection.baseOffset, equals(nextStart));
  });

  testWidgets('ArrowUp from outside does not reveal closing fence ticks', (
    WidgetTester tester,
  ) async {
    const text = '```\ncode\n```\nnext';
    final controller = SovereignController();
    addTearDown(controller.dispose);
    controller.value = TextEditingValue(
      text: text,
      selection: const TextSelection.collapsed(offset: 0),
    );
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

    final nextStart = text.indexOf('next');
    controller.selection = TextSelection.collapsed(offset: nextStart);
    await tester.pump();
    expect(
      controller.decoration.hiddenRanges,
      contains(const TextRange(start: 9, end: 12)),
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();

    expect(controller.selection.isCollapsed, isTrue);
    expect(controller.selection.baseOffset, equals(9));
    expect(
      controller.decoration.hiddenRanges,
      contains(const TextRange(start: 9, end: 12)),
    );
  });
}
