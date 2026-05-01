import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Fenced code smart pairs', () {
    test('Typing opener auto-inserts closer in fenced body', () {
      const text = '```\n\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 4);
      controller.value = const TextEditingValue(
        text: '```\n{\n```',
        selection: TextSelection.collapsed(offset: 5),
      );

      expect(controller.text, '```\n{}\n```');
      expect(controller.selection, const TextSelection.collapsed(offset: 5));
    });

    test('Typing closer before existing closer skips over it', () {
      const text = '```\n{}\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 5);
      controller.value = const TextEditingValue(
        text: '```\n{}}\n```',
        selection: TextSelection.collapsed(offset: 6),
      );

      expect(controller.text, text);
      expect(controller.selection, const TextSelection.collapsed(offset: 6));
    });

    test('Backspace between empty pair removes both chars', () {
      const text = '```\n{}\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 5);
      controller.value = const TextEditingValue(
        text: '```\n}\n```',
        selection: TextSelection.collapsed(offset: 4),
      );

      expect(controller.text, '```\n\n```');
      expect(controller.selection, const TextSelection.collapsed(offset: 4));
    });

    test('Enter between braces expands into indented block', () {
      const initial = '```dart\n{}\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.indexOf('{') + 1;
      final rawEnter = initial.replaceRange(caret, caret, '\n');
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: rawEnter,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      const expected = '```dart\n{\n  \n}\n```';
      expect(controller.text, expected);
      expect(
        controller.selection,
        TextSelection.collapsed(offset: expected.indexOf('\n  \n') + 3),
      );
    });

    test('Typing opener with selection wraps selected fenced content', () {
      const initial = '```\nfoo\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final start = initial.indexOf('foo');
      final end = start + 3;
      controller.selection = TextSelection(
        baseOffset: start,
        extentOffset: end,
      );
      controller.value = TextEditingValue(
        text: initial.replaceRange(start, end, '('),
        selection: TextSelection.collapsed(offset: start + 1),
      );

      expect(controller.text, '```\n(foo)\n```');
      expect(controller.selection, TextSelection.collapsed(offset: start + 5));
    });

    test('Typing opener with selection outside fence does not wrap', () {
      const initial = 'foo';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection(
        baseOffset: 0,
        extentOffset: 3,
      );
      controller.value = const TextEditingValue(
        text: '(',
        selection: TextSelection.collapsed(offset: 1),
      );

      expect(controller.text, '(');
      expect(controller.selection, const TextSelection.collapsed(offset: 1));
    });
  });
}
