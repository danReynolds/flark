import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Backspace at empty unclosed fence body removes the whole fence opener',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```\n',
        selection: TextSelection.collapsed(offset: 4),
      );

      // Simulate engine backspace at start of empty fence body.
      controller.value = const TextEditingValue(
        text: '```',
        selection: TextSelection.collapsed(offset: 3),
      );

      expect(controller.text, equals(''));
      expect(controller.selection.baseOffset, equals(0));
    },
  );

  test(
    'Backspace at empty tagged unclosed fence body removes the whole opener',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```dart\n',
        selection: TextSelection.collapsed(offset: 8),
      );

      controller.value = const TextEditingValue(
        text: '```dart',
        selection: TextSelection.collapsed(offset: 7),
      );

      expect(controller.text, equals(''));
      expect(controller.selection.baseOffset, equals(0));
    },
  );

  test('Backspace at non-empty unclosed fence body still deletes newline', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\na',
      selection: TextSelection.collapsed(offset: 4),
    );

    // Body is non-empty, so this should remain a normal backspace merge.
    controller.value = const TextEditingValue(
      text: '```a',
      selection: TextSelection.collapsed(offset: 3),
    );

    expect(controller.text, equals('```a'));
    expect(controller.selection.baseOffset, equals(3));
  });

  test(
    'Backspace at empty body does not collapse fence when opener line has visible content',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```hello world\n',
        selection: TextSelection.collapsed(offset: 14),
      );

      // Simulate backspace at the immediate body start. Since opener line
      // has visible content, this should be a normal newline deletion.
      controller.value = const TextEditingValue(
        text: '```hello world',
        selection: TextSelection.collapsed(offset: 13),
      );

      expect(controller.text, equals('```hello world'));
      expect(controller.selection.baseOffset, equals(13));
    },
  );

  test(
    'Backspace in closed fence does not collapse when opener tail has visible content',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```hello world\n\n```',
        selection: TextSelection.collapsed(offset: 15),
      );

      controller.value = const TextEditingValue(
        text: '```hello world\n```',
        selection: TextSelection.collapsed(offset: 14),
      );

      expect(controller.text, equals('```hello world\n```'));
      expect(controller.selection.baseOffset, equals(14));
    },
  );

  test(
    'Backspace cancel removes only empty fence opener after existing text',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: 'before\n```\n',
        selection: TextSelection.collapsed(offset: 11),
      );

      controller.value = const TextEditingValue(
        text: 'before\n```',
        selection: TextSelection.collapsed(offset: 10),
      );

      expect(controller.text, equals('before\n'));
      expect(controller.selection.baseOffset, equals('before\n'.length));
    },
  );

  test('Backspace in empty closed fence body removes entire fence block', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\n\n```',
      selection: TextSelection.collapsed(offset: 5),
    );

    // Simulate backspace from the empty interior line (deletes interior \n).
    controller.value = const TextEditingValue(
      text: '```\n```',
      selection: TextSelection.collapsed(offset: 4),
    );

    expect(controller.text, equals(''));
    expect(controller.selection.baseOffset, equals(0));
  });

  test(
    'Backspace from second empty interior line does not remove whole fence',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```\n\n\n```',
        selection: TextSelection.collapsed(offset: 5),
      );

      // Delete one interior newline; fence should remain.
      controller.value = const TextEditingValue(
        text: '```\n\n```',
        selection: TextSelection.collapsed(offset: 4),
      );

      expect(controller.text, equals('```\n\n```'));
      expect(controller.selection.baseOffset, equals(4));
    },
  );

  test(
    'Backspace from blank line after content in closed fence keeps fence',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```\nabc\n\n```',
        selection: TextSelection.collapsed(offset: 8),
      );

      // Delete the newline before the blank line; this should merge with content
      // and keep the fenced region.
      controller.value = const TextEditingValue(
        text: '```\nabc\n```',
        selection: TextSelection.collapsed(offset: 7),
      );

      expect(controller.text, equals('```\nabc\n```'));
      expect(controller.selection.baseOffset, equals(7));
    },
  );

  test(
    'Backspace from trailing blank line after content in unclosed fence keeps fence',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```\nabc\n',
        selection: TextSelection.collapsed(offset: 8),
      );

      // Delete newline after content; fence should remain and just merge lines.
      controller.value = const TextEditingValue(
        text: '```\nabc',
        selection: TextSelection.collapsed(offset: 7),
      );

      expect(controller.text, equals('```\nabc'));
      expect(controller.selection.baseOffset, equals(7));
    },
  );
}
