import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Sovereign toolbar markdown insertion', () {
    test('Empty bold wrapper hides both markers at insertion caret', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '****',
        selection: TextSelection.collapsed(offset: 2),
      );

      _expectAdjacentMarkersHidden(
        controller.decoration.hiddenRanges,
        split: const [TextRange(start: 0, end: 2), TextRange(start: 2, end: 4)],
        merged: const TextRange(start: 0, end: 4),
      );
    });

    test(
      'Typing first character in empty bold wrapper keeps trailing marker hidden',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '****',
          selection: TextSelection.collapsed(offset: 2),
        );
        controller.value = const TextEditingValue(
          text: '**x**',
          selection: TextSelection.collapsed(offset: 3),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 3, end: 5),
          ]),
        );
      },
    );

    test('Empty italic wrapper hides both markers at insertion caret', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '__',
        selection: TextSelection.collapsed(offset: 1),
      );

      _expectAdjacentMarkersHidden(
        controller.decoration.hiddenRanges,
        split: const [TextRange(start: 0, end: 1), TextRange(start: 1, end: 2)],
        merged: const TextRange(start: 0, end: 2),
      );
    });

    test(
      'Typing first character in empty italic wrapper keeps trailing marker hidden',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '__',
          selection: TextSelection.collapsed(offset: 1),
        );
        controller.value = const TextEditingValue(
          text: '_x_',
          selection: TextSelection.collapsed(offset: 2),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 2, end: 3),
          ]),
        );
      },
    );

    test('Empty inline code wrapper hides both markers at insertion caret', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '``',
        selection: TextSelection.collapsed(offset: 1),
      );

      _expectAdjacentMarkersHidden(
        controller.decoration.hiddenRanges,
        split: const [TextRange(start: 0, end: 1), TextRange(start: 1, end: 2)],
        merged: const TextRange(start: 0, end: 2),
      );
    });

    test(
      'Typing first character in empty inline code wrapper keeps trailing marker hidden',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '``',
          selection: TextSelection.collapsed(offset: 1),
        );
        controller.value = const TextEditingValue(
          text: '`x`',
          selection: TextSelection.collapsed(offset: 2),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 2, end: 3),
          ]),
        );
      },
    );

    test(
      'Bold insertion keeps trailing marker hidden while placeholder is selected',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const text = '**bold**';
        controller.value = const TextEditingValue(
          text: text,
          selection: TextSelection(baseOffset: 2, extentOffset: 6),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 6, end: 8),
          ]),
        );
      },
    );

    test(
      'Italic insertion keeps trailing marker hidden while placeholder is selected',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const text = '_italic_';
        controller.value = const TextEditingValue(
          text: text,
          selection: TextSelection(baseOffset: 1, extentOffset: 7),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 7, end: 8),
          ]),
        );
      },
    );

    test(
      'Inline code insertion keeps trailing marker hidden while placeholder is selected',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const text = '`code`';
        controller.value = const TextEditingValue(
          text: text,
          selection: TextSelection(baseOffset: 1, extentOffset: 5),
        );

        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 5, end: 6),
          ]),
        );
      },
    );

    test('Heading insertion hides marker while placeholder is selected', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      const text = '# Heading';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection(baseOffset: 2, extentOffset: 9),
      );

      expect(
        controller.decoration.hiddenRanges,
        contains(const TextRange(start: 0, end: 2)),
      );
    });

    test('Empty heading insertion hides marker at insertion caret', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '# ',
        selection: TextSelection.collapsed(offset: 2),
      );

      expect(
        controller.decoration.hiddenRanges,
        contains(const TextRange(start: 0, end: 2)),
      );
    });

    test(
      'Backspace over bold closer re-enters and deletes wrapped content',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const original = '**abc**';
        controller.value = const TextEditingValue(
          text: original,
          selection: TextSelection.collapsed(offset: original.length),
        );
        controller.value = _singleCharBackspaceValue(controller.value);

        expect(controller.text, '**ab**');
        expect(controller.selection, const TextSelection.collapsed(offset: 4));
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 4, end: 6),
          ]),
        );
      },
    );

    test(
      'Backspace over italic closer re-enters and deletes wrapped content',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const original = '_abc_';
        controller.value = const TextEditingValue(
          text: original,
          selection: TextSelection.collapsed(offset: original.length),
        );
        controller.value = _singleCharBackspaceValue(controller.value);

        expect(controller.text, '_ab_');
        expect(controller.selection, const TextSelection.collapsed(offset: 3));
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 3, end: 4),
          ]),
        );
      },
    );

    test(
      'Backspace over inline-code closer re-enters and deletes wrapped content',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const original = '`abc`';
        controller.value = const TextEditingValue(
          text: original,
          selection: TextSelection.collapsed(offset: original.length),
        );
        controller.value = _singleCharBackspaceValue(controller.value);

        expect(controller.text, '`ab`');
        expect(controller.selection, const TextSelection.collapsed(offset: 3));
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 3, end: 4),
          ]),
        );
      },
    );

    test(
      'Backspace over single-asterisk closer re-enters and deletes wrapped content',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const original = '*abc*';
        controller.value = const TextEditingValue(
          text: original,
          selection: TextSelection.collapsed(offset: original.length),
        );
        controller.value = _singleCharBackspaceValue(controller.value);

        expect(controller.text, '*ab*');
        expect(controller.selection, const TextSelection.collapsed(offset: 3));
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 1),
            TextRange(start: 3, end: 4),
          ]),
        );
      },
    );

    test('Backspace on lone inline-code marker deletes cleanly', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '`',
        selection: TextSelection.collapsed(offset: 1),
      );
      controller.value = _singleCharBackspaceValue(controller.value);

      expect(controller.text, isEmpty);
      expect(controller.selection, const TextSelection.collapsed(offset: 0));
    });

    test('Backspace on lone underscore marker deletes cleanly', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '_',
        selection: TextSelection.collapsed(offset: 1),
      );
      controller.value = _singleCharBackspaceValue(controller.value);

      expect(controller.text, isEmpty);
      expect(controller.selection, const TextSelection.collapsed(offset: 0));
    });

    test('Backspace on lone double-asterisk marker is boundary-safe', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '**',
        selection: TextSelection.collapsed(offset: 2),
      );
      controller.value = _singleCharBackspaceValue(controller.value);
      expect(controller.text, '*');
      expect(controller.selection, const TextSelection.collapsed(offset: 1));

      controller.value = _singleCharBackspaceValue(controller.value);
      expect(controller.text, isEmpty);
      expect(controller.selection, const TextSelection.collapsed(offset: 0));
    });

    test('Backspace over bold closer also consumes wrapped trailing space', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      const original = '**abc **';
      controller.value = const TextEditingValue(
        text: original,
        selection: TextSelection.collapsed(offset: original.length),
      );
      controller.value = _singleCharBackspaceValue(controller.value);

      expect(controller.text, '**abc**');
      expect(controller.selection, const TextSelection.collapsed(offset: 5));
      expect(
        controller.decoration.hiddenRanges,
        containsAll(<TextRange>[
          TextRange(start: 0, end: 2),
          TextRange(start: 5, end: 7),
        ]),
      );
    });

    test(
      'Backspace over inline marker does not re-enter inside fenced code',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        const original = '```\n**abc**\n```';
        final wrapperStart = original.indexOf('**abc**');
        final caret = wrapperStart + '**abc**'.length;
        controller.value = TextEditingValue(
          text: original,
          selection: TextSelection.collapsed(offset: caret),
        );
        final backspaced = _singleCharBackspaceValue(controller.value);
        controller.value = backspaced;

        expect(controller.text, backspaced.text);
        expect(controller.selection, backspaced.selection);
      },
    );
  });
}

TextEditingValue _singleCharBackspaceValue(TextEditingValue value) {
  final selection = value.selection;
  if (!selection.isCollapsed || !selection.isValid) return value;
  final caret = selection.baseOffset;
  if (caret <= 0 || caret > value.text.length) return value;
  return TextEditingValue(
    text: value.text.replaceRange(caret - 1, caret, ''),
    selection: TextSelection.collapsed(offset: caret - 1),
  );
}

void _expectAdjacentMarkersHidden(
  List<TextRange> hiddenRanges, {
  required List<TextRange> split,
  required TextRange merged,
}) {
  final hasSplit = split.every(hiddenRanges.contains);
  final hasMerged = hiddenRanges.contains(merged);
  bool fullyCovered() {
    if (merged.end <= merged.start) return true;
    for (var offset = merged.start; offset < merged.end; offset++) {
      final covered = hiddenRanges.any(
        (r) => offset >= r.start && offset < r.end,
      );
      if (!covered) return false;
    }
    return true;
  }

  expect(
    hasSplit || hasMerged || fullyCovered(),
    isTrue,
    reason:
        'Adjacent marker ranges may be split, merged, or otherwise normalized '
        'as long as the wrapper marker bytes remain hidden.',
  );
}
