import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test('Enter inside fenced code preserves leading space indentation', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\n  foo\n```',
      selection: TextSelection.collapsed(offset: 9),
    );

    // Simulate engine newline insertion at end of "  foo".
    controller.value = const TextEditingValue(
      text: '```\n  foo\n\n```',
      selection: TextSelection.collapsed(offset: 10),
    );

    expect(controller.text, equals('```\n  foo\n  \n```'));
    expect(controller.selection.baseOffset, equals(12));
  });

  test('Enter inside fenced code preserves leading tab indentation', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\n\tfoo\n```',
      selection: TextSelection.collapsed(offset: 8),
    );

    controller.value = const TextEditingValue(
      text: '```\n\tfoo\n\n```',
      selection: TextSelection.collapsed(offset: 9),
    );

    expect(controller.text, equals('```\n\tfoo\n\t\n```'));
    expect(controller.selection.baseOffset, equals(10));
  });

  test('Enter after opener inside fenced code increases indent depth', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\nif (x) {\n```';
    final caret = initial.indexOf('{') + 1;
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    final expected = initial.replaceRange(caret, caret, '\n  ');
    expect(controller.text, equals(expected));
    expect(controller.selection.baseOffset, equals(caret + 3));
  });

  test(
    'Enter after indented opener inside fenced code keeps base and adds one level',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      const initial = '```\n  if (x) {\n```';
      final caret = initial.indexOf('{') + 1;
      final rawEnter = initial.replaceRange(caret, caret, '\n');

      controller.value = TextEditingValue(
        text: initial,
        selection: TextSelection.collapsed(offset: caret),
      );
      controller.value = TextEditingValue(
        text: rawEnter,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      final expected = initial.replaceRange(caret, caret, '\n    ');
      expect(controller.text, equals(expected));
      expect(controller.selection.baseOffset, equals(caret + 5));
    },
  );

  test('Enter after tab-indented opener adds one tab level', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\n\tif (x) {\n```';
    final caret = initial.indexOf('{') + 1;
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    final expected = initial.replaceRange(caret, caret, '\n\t\t');
    expect(controller.text, equals(expected));
    expect(controller.selection.baseOffset, equals(caret + 3));
  });

  test('Python fence increases indent after colon line', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```python\nif ready:\n```';
    final caret = initial.indexOf(':') + 1;
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    final expected = initial.replaceRange(caret, caret, '\n  ');
    expect(controller.text, equals(expected));
    expect(controller.selection.baseOffset, equals(caret + 3));
  });

  test('Dart fence does not increase indent after colon line', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```dart\nlabel:\n```';
    final caret = initial.indexOf(':') + 1;
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals(rawEnter));
    expect(controller.selection.baseOffset, equals(caret + 1));
  });

  test('Typing closer on indentation-only line auto-outdents in fence', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\n  \n```';
    const caret = 6; // after two leading spaces on middle line
    const typed = '```\n  }\n```';

    controller.value = const TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = const TextEditingValue(
      text: typed,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals('```\n}\n```'));
    expect(controller.selection.baseOffset, equals(5));
  });

  test('Typing closer on 4-space indented line removes one 4-space level', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\n    \n```';
    const caret = 8; // after four spaces
    const typed = '```\n    }\n```';

    controller.value = const TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = const TextEditingValue(
      text: typed,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals('```\n}\n```'));
    expect(controller.selection.baseOffset, equals(5));
  });

  test('Typing closer after content does not auto-outdent in fence', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\n  foo\n```';
    const caret = 9; // after "foo"
    const typed = '```\n  foo}\n```';

    controller.value = const TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = const TextEditingValue(
      text: typed,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals(typed));
    expect(controller.selection.baseOffset, equals(caret + 1));
  });

  test(
    'Typing closer in nested function blank line aligns with parent indent',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      const initial = '```\nint main() {\n  void run() {\n    \n}\n```';
      final caret = initial.indexOf('    \n') + 4;
      final typed = initial.replaceRange(caret, caret, '}');

      controller.value = TextEditingValue(
        text: initial,
        selection: TextSelection.collapsed(offset: caret),
      );
      controller.value = TextEditingValue(
        text: typed,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      const expected = '```\nint main() {\n  void run() {\n  }\n}\n```';
      expect(controller.text, equals(expected));
      expect(
        controller.selection.baseOffset,
        equals(expected.indexOf('\n  }\n') + 4),
      );
    },
  );

  test('Typing closer outside fenced code does not auto-outdent', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '  ';
    const typed = '  }';
    const caret = 2;

    controller.value = const TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = const TextEditingValue(
      text: typed,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals(typed));
    expect(controller.selection.baseOffset, equals(caret + 1));
  });

  test('Enter on sentinel blank line before closing fence still exits', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\n  foo\n\n```',
      selection: TextSelection.collapsed(offset: 10),
    );

    controller.value = const TextEditingValue(
      text: '```\n  foo\n\n\n```',
      selection: TextSelection.collapsed(offset: 11),
    );

    expect(controller.text, equals('```\n  foo\n```\n'));
    expect(controller.selection.baseOffset, equals('```\n  foo\n```\n'.length));
  });

  test('Exit on Enter trims all trailing blank lines before closing fence', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\nfoo\n\n\n```';
    final caret = initial.indexOf('\n\n\n```') + 1; // start of first blank line
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals('```\nfoo\n```\n'));
    expect(controller.selection.baseOffset, equals('```\nfoo\n```\n'.length));
  });

  test('Exit on Enter trims trailing EOF blank lines in unclosed fence', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial = '```\nfoo\n\n\n';
    final caret = initial.lastIndexOf('\n'); // start of last blank line
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    expect(controller.text, equals('```\nfoo\n```\n'));
    expect(controller.selection.baseOffset, equals('```\nfoo\n```\n'.length));
  });

  test('Enter on indented blank line before closing fence exits and trims', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const initial =
        '```\nint main() {\n  final x = 2;\n  final y = 3;\n  \n```';
    final lineStart = initial.indexOf('\n  \n```') + 1;
    final caret = lineStart + 2; // after the two spaces on the blank line
    final rawEnter = initial.replaceRange(caret, caret, '\n');

    controller.value = TextEditingValue(
      text: initial,
      selection: TextSelection.collapsed(offset: caret),
    );
    controller.value = TextEditingValue(
      text: rawEnter,
      selection: TextSelection.collapsed(offset: caret + 1),
    );

    const expected = '```\nint main() {\n  final x = 2;\n  final y = 3;\n```\n';
    expect(controller.text, equals(expected));
    expect(controller.selection.baseOffset, equals(expected.length));
  });

  test(
    'Enter on indented EOF blank line in unclosed fence exits and closes',
    () {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      const initial = '```\nint main() {\n  final x = 2;\n  \n';
      final lineStart = initial.lastIndexOf('\n  \n') + 1;
      final caret = lineStart + 2; // after two spaces on the EOF blank line
      final rawEnter = initial.replaceRange(caret, caret, '\n');

      controller.value = TextEditingValue(
        text: initial,
        selection: TextSelection.collapsed(offset: caret),
      );
      controller.value = TextEditingValue(
        text: rawEnter,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      const expected = '```\nint main() {\n  final x = 2;\n```\n';
      expect(controller.text, equals(expected));
      expect(controller.selection.baseOffset, equals(expected.length));
    },
  );
}
