import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  testWidgets(
    'ArrowDown from last non-blank line trims trailing blank fence lines on exit',
    (WidgetTester tester) async {
      final text = '```\ncode\n\n\n```\nnext';
      const expected = '```\ncode\n```\nnext';
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

      final codeStart = text.indexOf('code');
      expect(codeStart, isNot(-1));
      controller.selection = TextSelection.collapsed(
        offset: codeStart + 'code'.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();

      expect(controller.text, equals(expected));

      final nextStart = expected.indexOf('next');
      expect(nextStart, isNot(-1));
      expect(controller.selection.baseOffset, equals(nextStart));
    },
  );
}
