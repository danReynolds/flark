import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('Empty fence exits on second Enter in editor flow', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController();
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

    controller.value = const TextEditingValue(
      text: '```',
      selection: TextSelection.collapsed(offset: 3),
    );
    await tester.pump();

    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(controller.text, '```\n');
    expect(controller.selection.baseOffset, 4);

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(controller.text, '```\n```\n');
    expect(controller.selection.baseOffset, 8);
  });
}
