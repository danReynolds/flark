import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  // Test scenario:
  // 1. User has text "line1"
  // 2. User taps at end of line (offset 5)
  // 3. User hits Enter
  // 4. Expect: "line1\n" (clean newline)
  // 5. Bug: "line\n1" (split char)

  testWidgets('Enter Key Integration: clean newline at EOL', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: 'line1');

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: FocusNode(),
            enableTestShortcuts: true,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 1. Focus
    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    // 2. Set Cursor to End (Simulate Tap at EOL)
    // "line1" is 5 chars.
    controller.selection = const TextSelection.collapsed(offset: 5);
    await tester.pump();

    // Verify initial cursor state
    expect(
      controller.selection.baseOffset,
      5,
      reason: "Cursor should be at EOL",
    );

    // 3. Press Enter
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();

    // 4. Verify Content
    // Expected: "line1\n"
    // Buggy: "line\n1"

    expect(
      controller.text,
      equals('line1\n'),
      reason: "Enter should verify clean insertion",
    );
    expect(
      controller.selection.baseOffset,
      6,
      reason: "Cursor should be after newline",
    );
  });

  testWidgets('Enter Key Integration: Inside Fenced Code', (
    WidgetTester tester,
  ) async {
    // Hidden ranges (the ```) might be the trigger for degenerate geometry
    final text = '```\nline1\n```';
    // 0123 45678 9012 (indices)
    // ```\n is 4 chars.
    // line1 is 5 chars.
    // \n is 1 char.
    // ``` is 3 chars.
    // Total: 4 + 5 + 1 + 3 = 13?

    final controller = SovereignController(text: text);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: FocusNode(),
            enableTestShortcuts: true,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 1. Focus
    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    // 2. Set Cursor to End of "line1"
    // ` ` ` ` \n l i n e 1
    // 0 1 2 3  4 5 6 7 8 9
    // End of line1 is after '1' (offset 9).

    controller.selection = const TextSelection.collapsed(offset: 9);
    await tester.pump();

    // Verify cursor is strictly at 9
    expect(
      controller.selection.baseOffset,
      9,
      reason: "Cursor should be at offset 9",
    );

    // 3. Press Enter
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    // 4. Verify Content
    // Expect: ```\nline1\n\n```
    // Bug: ```\nline\n1\n```

    expect(
      controller.text,
      contains('line1\n\n'),
      reason: "Should have double newline after line1",
    );
  });

  testWidgets(
      'Enter Key Integration: Tap EOL inside fenced code does not split', (
    WidgetTester tester,
  ) async {
    // This reproduces the real-world bug: tapping at the visual end-of-line
    // inside a fenced code block and pressing Enter should not split the line.
    final controller = SovereignController(text: '```\nline1\n```');

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: FocusNode(),
            enableTestShortcuts: true,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // Focus editor
    await tester.tap(find.byType(SovereignEditor));
    await tester.pumpAndSettle();

    // Compute the caret position at the end of 'line1' in the RENDERED text and tap it.
    final editableState = tester.state<EditableTextState>(
      find.byType(EditableText),
    );
    final RenderEditable renderEditable = editableState.renderEditable;
    final plain = renderEditable.text!.toPlainText();
    final idx = plain.indexOf('line1');
    expect(idx, isNot(-1));
    final eolOffset = idx + 'line1'.length;

    final caretRect = renderEditable.getLocalRectForCaret(
      TextPosition(offset: eolOffset),
    );
    final tapPos = renderEditable.localToGlobal(
      caretRect.center + const Offset(1, 0),
    );

    await tester.tapAt(tapPos);
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();

    expect(controller.text, equals('```\nline1\n\n```'));
  });
}
