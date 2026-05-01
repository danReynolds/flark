import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('SovereignController: Range Error on Delete in Fenced Block', (
    WidgetTester tester,
  ) async {
    // Scenario: User has a Fenced Code Block.
    // User deletes a character inside or near it.
    // Expectation: No crash (RangeError).

    final text = '```\ncontent\n```';
    final controller = SovereignController(text: text);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: FocusNode()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 1. Focus
    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    // 2. Position Cursor inside content (e.g. end of "content")
    // ```\n (4 chars)
    // content (7 chars) -> offset 4+7 = 11.
    // \n``` (4 chars)

    controller.selection = const TextSelection.collapsed(offset: 11);
    await tester.pump();

    // 3. Delete (Backspace)
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump(); // Sync pump

    // 4. Verify no crash and text updated
    expect(controller.text, equals('```\nconten\n```'));

    // 5. Delete boundary?
    // Try checking deletion of the fence markers themselves.
    controller.text = '```\n';
    controller.selection = const TextSelection.collapsed(offset: 4); // After \n
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace); // Delete \n
    await tester.pump();

    // Empty unclosed fence entry collapses the whole opener on backspace.
    expect(controller.text, equals(''));
    expect(controller.selection.baseOffset, equals(0));

    // 6. Backspace on hidden fence marker should not delete structure.
    controller.text = '```\ncontent\n```';
    controller.selection = const TextSelection.collapsed(offset: 11);
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(controller.text, equals('```\nconten\n```'));

    controller.selection = const TextSelection.collapsed(offset: 14);
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();

    expect(controller.text, equals('```\nconten\n```'));
    expect(controller.selection.baseOffset, equals(10));
  });
}
