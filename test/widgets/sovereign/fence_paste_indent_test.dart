import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Fenced code paste indentation', () {
    test('Multiline paste aligns to current fenced indentation', () {
      const initial = '```\n  \n```';
      const pasted = 'if (x) {\n  run();\n}';
      const caret = 6; // after two spaces on the fence body line
      final rawPaste = initial.replaceRange(caret, caret, pasted);

      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: rawPaste,
        selection: TextSelection.collapsed(offset: caret + pasted.length),
      );

      const expected = '```\n  if (x) {\n    run();\n  }\n```';
      expect(controller.text, expected);
      expect(
        controller.selection,
        TextSelection.collapsed(offset: expected.lastIndexOf('\n```')),
      );
    });

    test('Multiline paste outside fence is unchanged', () {
      const initial = '  \ntext';
      const pasted = 'if (x) {\n  run();\n}';
      const caret = 2;
      final rawPaste = initial.replaceRange(caret, caret, pasted);

      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: rawPaste,
        selection: TextSelection.collapsed(offset: caret + pasted.length),
      );

      expect(controller.text, rawPaste);
      expect(
        controller.selection,
        TextSelection.collapsed(offset: caret + pasted.length),
      );
    });

    test('Multiline paste in middle of fenced content line is unchanged', () {
      const initial = '```\n  foo\n```';
      const pasted = 'if\n  x';
      final caret = initial.indexOf('foo') + 1; // after "f"
      final rawPaste = initial.replaceRange(caret, caret, pasted);

      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: rawPaste,
        selection: TextSelection.collapsed(offset: caret + pasted.length),
      );

      expect(controller.text, rawPaste);
      expect(
        controller.selection,
        TextSelection.collapsed(offset: caret + pasted.length),
      );
    });
  });
}
