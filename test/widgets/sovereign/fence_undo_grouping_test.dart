import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Fenced code undo grouping', () {
    test('Tab indentation is an atomic undo step', () {
      const initial = '```\nfoo\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final insertOffset = initial.indexOf('foo');
      final typed = initial.replaceRange(insertOffset, insertOffset, 'x');
      controller.value = TextEditingValue(
        text: typed,
        selection: TextSelection.collapsed(offset: insertOffset + 1),
      );

      final tabHandled = controller.handleTabKey(reverse: false);
      expect(tabHandled, isTrue);
      expect(controller.text, '```\nx  foo\n```');

      controller.undo();
      expect(
        controller.text,
        typed,
        reason: 'First undo should only revert Tab',
      );

      controller.undo();
      expect(
        controller.text,
        initial,
        reason: 'Second undo should revert typing',
      );

      controller.redo();
      expect(controller.text, typed);
      controller.redo();
      expect(controller.text, '```\nx  foo\n```');
    });

    test('Fence language change does not merge with nearby typing', () {
      const initial = '```\nfoo\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final insideFence = initial.indexOf('foo');
      controller.selection = TextSelection.collapsed(offset: insideFence);
      final changed = controller.setFencedCodeLanguageForSelection('dart');
      expect(changed, isTrue);
      expect(controller.text, '```dart\nfoo\n```');

      final contentOffset = controller.text.indexOf('foo');
      final typed = controller.text.replaceRange(
        contentOffset,
        contentOffset,
        'x',
      );
      controller.value = TextEditingValue(
        text: typed,
        selection: TextSelection.collapsed(offset: contentOffset + 1),
      );

      controller.undo();
      expect(
        controller.text,
        '```dart\nfoo\n```',
        reason: 'Typing should undo first and keep language change',
      );

      controller.undo();
      expect(
        controller.text,
        initial,
        reason: 'Second undo should revert language change',
      );
    });

    test('Auto-pair insertion undoes in one step', () {
      const initial = '```\n\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 4);
      controller.value = const TextEditingValue(
        text: '```\n{\n```',
        selection: TextSelection.collapsed(offset: 5),
      );

      expect(controller.text, '```\n{}\n```');
      controller.undo();
      expect(controller.text, initial);
    });

    test('Auto-indent on Enter undoes in one step', () {
      const initial = '```\nif (x) {\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.indexOf('{') + 1;
      final rawEnter = initial.replaceRange(caret, caret, '\n');
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: rawEnter,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, initial.replaceRange(caret, caret, '\n  '));
      controller.undo();
      expect(controller.text, initial);
    });

    test('Auto-outdent on closer insertion undoes in one step', () {
      const initial = '```\n  \n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 6);
      controller.value = const TextEditingValue(
        text: '```\n  }\n```',
        selection: TextSelection.collapsed(offset: 7),
      );

      expect(controller.text, '```\n}\n```');
      controller.undo();
      expect(controller.text, initial);
    });

    test('Fence exit on Enter undoes in one step', () {
      const initial = '```\nfoo\n\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      const caret = 8; // start of blank sentinel line before closing fence
      controller.selection = const TextSelection.collapsed(offset: caret);
      controller.value = const TextEditingValue(
        text: '```\nfoo\n\n\n```',
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '```\nfoo\n```\n');
      controller.undo();
      expect(controller.text, initial);
    });
  });
}
