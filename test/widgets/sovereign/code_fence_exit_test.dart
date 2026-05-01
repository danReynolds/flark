import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets('ArrowDown on last content line exits fenced code block', (
    WidgetTester tester,
  ) async {
    final text = '```\ncode\n```\nnext';
    final controller = SovereignController(text: text);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: FocusNode()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    // Place caret at end of "code".
    final codeStart = text.indexOf('code');
    expect(codeStart, isNot(-1));
    controller.selection = TextSelection.collapsed(
      offset: codeStart + 'code'.length,
    );
    await tester.pump();

    // ArrowDown would land on the (hidden) closing fence line; we should skip out.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();

    expect(controller.text, equals(text));

    final nextStart = text.indexOf('next');
    expect(nextStart, isNot(-1));
    expect(controller.selection.baseOffset, equals(nextStart));
  });

  testWidgets('ArrowDown on indented last line exits fenced code block', (
    WidgetTester tester,
  ) async {
    final text = 'before\n```\nint main() {\n  final x = 2;\n  \n```\nafter';
    const expected = 'before\n```\nint main() {\n  final x = 2;\n```\nafter';
    final controller = SovereignController(text: text);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: FocusNode()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    final blankLine = text.indexOf('\n  \n```');
    expect(blankLine, isNot(-1));
    controller.selection = TextSelection.collapsed(offset: blankLine + 3);
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();

    expect(controller.text, equals(expected));

    final afterStart = expected.indexOf('after');
    expect(afterStart, isNot(-1));
    expect(controller.selection.baseOffset, equals(afterStart));
  });

  testWidgets('ArrowUp on first content line exits fenced code block', (
    WidgetTester tester,
  ) async {
    final text = 'before\n```\ncode\n```\nafter';
    final controller = SovereignController(text: text);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(controller: controller, focusNode: FocusNode()),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byType(SovereignEditor));
    await tester.pump();

    final codeStart = text.indexOf('code');
    expect(codeStart, isNot(-1));
    controller.selection = TextSelection.collapsed(
      offset: codeStart + 'code'.length,
    );
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();

    expect(controller.selection.baseOffset, equals('before\n'.length));
  });

  testWidgets(
    'ArrowDown exits when caret lands on hidden closing fence line boundary',
    (WidgetTester tester) async {
      final text = 'before\n```\ncode\n```\nafter';
      final controller = SovereignController(text: text);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditor(
              controller: controller,
              focusNode: FocusNode(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      final closeFenceStart = text.lastIndexOf('```');
      expect(closeFenceStart, greaterThan(0));
      controller.selection = TextSelection.collapsed(offset: closeFenceStart);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();

      final afterStart = text.indexOf('after');
      expect(controller.selection.baseOffset, equals(afterStart));
    },
  );

  testWidgets(
    'ArrowUp exits when caret lands on hidden opening fence line boundary',
    (WidgetTester tester) async {
      final text = 'before\n```\ncode\n```\nafter';
      final controller = SovereignController(text: text);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditor(
              controller: controller,
              focusNode: FocusNode(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      final fenceStart = text.indexOf('```');
      expect(fenceStart, equals('before\n'.length));
      controller.selection = TextSelection.collapsed(offset: fenceStart + 3);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      await tester.pump();

      expect(controller.selection.baseOffset, equals(fenceStart));
    },
  );

  testWidgets(
    'Enter on blank line before closing fence exits fenced code block',
    (WidgetTester tester) async {
      final text = '```\ncode\n\n```\nnext';
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

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      // Caret on the blank sentinel line (immediately before the closing fence).
      final markerIdx = text.indexOf('\n\n```');
      expect(markerIdx, isNot(-1));
      final blankLineStart = markerIdx + 1;
      controller.selection = TextSelection.collapsed(offset: blankLineStart);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('```\ncode\n```\nnext'));
      expect(
        controller.selection.baseOffset,
        equals('```\ncode\n```\n'.length),
      );
    },
  );

  testWidgets(
    'Shift+Enter on blank line before closing fence inserts newline and does not exit',
    (WidgetTester tester) async {
      const text = '```\ncode\n\n```\nnext';
      const expected = '```\ncode\n\n\n```\nnext';
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

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      final markerIdx = text.indexOf('\n\n```');
      expect(markerIdx, isNot(-1));
      final blankLineStart = markerIdx + 1;
      controller.selection = TextSelection.collapsed(offset: blankLineStart);
      await tester.pump();

      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.pump();

      expect(controller.text, equals(expected));
      final closeFenceStart = expected.lastIndexOf('```');
      expect(closeFenceStart, greaterThan(0));
      expect(controller.selection.baseOffset, lessThan(closeFenceStart));
    },
  );

  testWidgets(
    'Second Enter from last content line exits fence and trims intermediate blank lines',
    (WidgetTester tester) async {
      const text = '```\nint main() {\n}\n```\nnext';
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

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      final closingBraceStart = text.indexOf('\n}\n') + 1;
      expect(closingBraceStart, greaterThan(0));
      controller.selection = TextSelection.collapsed(
        offset: closingBraceStart + 1,
      );
      await tester.pump();

      // Enter #1: move to blank line before closing fence.
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(controller.text, contains('\n}\n\n```'));

      // Enter #2: exit fence, trimming trailing blank lines.
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals(text));
      final nextStart = text.indexOf('next');
      expect(controller.selection.baseOffset, equals(nextStart));
    },
  );

  testWidgets(
    'Second Enter exits closed EOF fence and places caret on outside line',
    (WidgetTester tester) async {
      const initial = '```\nint main() {\n}\n```';
      const expected = '```\nint main() {\n}\n```\n';
      final controller = SovereignController(text: initial);

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

      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      final braceLineStart = initial.indexOf('\n}\n') + 1;
      expect(braceLineStart, greaterThan(0));
      controller.selection = TextSelection.collapsed(
        offset: braceLineStart + 1,
      );
      await tester.pump();

      // Enter #1: create blank line before the existing closing fence.
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(controller.text, contains('\n}\n\n```'));

      // Enter #2: trim the blank line and move outside the fence at EOF.
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals(expected));
      expect(controller.selection.baseOffset, equals(expected.length));
    },
  );
}
