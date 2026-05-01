import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('List policy editing', () {
    test('Enter continues unordered list item', () {
      const initial = '- item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '- item\n- ');
      expect(controller.selection.baseOffset, '- item\n- '.length);
    });

    test('Enter on empty unordered item exits list', () {
      const initial = '- ';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(
        offset: initial.length,
      );
      controller.value = const TextEditingValue(
        text: '- \n',
        selection: TextSelection.collapsed(offset: 3),
      );

      expect(controller.text, '\n');
      expect(controller.selection.baseOffset, 1);
    });

    test('Enter continues ordered list with incremented marker', () {
      const initial = '2. two';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '2. two\n3. ');
      expect(controller.selection.baseOffset, '2. two\n3. '.length);
    });

    test('Enter continues asterisk unordered list item', () {
      const initial = '* item';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      controller.handleEnter();

      expect(controller.text, '* item\n* ');
      expect(controller.selection.baseOffset, '* item\n* '.length);
    });

    test('Enter continues ordered list from 1. with incremented marker', () {
      const initial = '1. one';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      controller.handleEnter();

      expect(controller.text, '1. one\n2. ');
      expect(controller.selection.baseOffset, '1. one\n2. '.length);
    });

    test('Enter continues task list with unchecked marker', () {
      const initial = '- [x] done';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '- [x] done\n- [ ] ');
      expect(controller.selection.baseOffset, '- [x] done\n- [ ] '.length);
    });

    test(
      'Enter continues ordered task list with incremented unchecked marker',
      () {
        const initial = '1. [x] done';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        controller.selection = TextSelection.collapsed(offset: initial.length);
        controller.handleEnter();

        expect(controller.text, '1. [x] done\n2. [ ] ');
        expect(controller.selection.baseOffset, '1. [x] done\n2. [ ] '.length);
      },
    );

    test('Enter continues nested unordered list item preserving indent', () {
      const initial = '  - item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      controller.handleEnter();

      expect(controller.text, '  - item\n  - ');
      expect(controller.selection.baseOffset, '  - item\n  - '.length);
    });

    test('Enter continues nested ordered list item preserving indent', () {
      const initial = '    2. item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      controller.handleEnter();

      expect(controller.text, '    2. item\n    3. ');
      expect(controller.selection.baseOffset, '    2. item\n    3. '.length);
    });

    test(
      'Enter on empty nested list item exits list and keeps indentation',
      () {
        const initial = '  - ';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        controller.selection = const TextSelection.collapsed(
          offset: initial.length,
        );
        controller.handleEnter();

        expect(controller.text, '  \n');
        expect(controller.selection.baseOffset, 3);
      },
    );

    test('Backspace at unordered content boundary removes list marker', () {
      const initial = '- item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 2);
      controller.value = const TextEditingValue(
        text: '-item',
        selection: TextSelection.collapsed(offset: 1),
      );

      expect(controller.text, 'item');
      expect(controller.selection.baseOffset, 0);
    });

    test('Backspace at ordered content boundary removes list marker', () {
      const initial = '1. item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 3);
      controller.value = const TextEditingValue(
        text: '1.item',
        selection: TextSelection.collapsed(offset: 2),
      );

      expect(controller.text, 'item');
      expect(controller.selection.baseOffset, 0);
    });

    test('Backspace at nested list content boundary removes marker only', () {
      const initial = '  - item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 4);
      controller.value = const TextEditingValue(
        text: '  -item',
        selection: TextSelection.collapsed(offset: 3),
      );

      expect(controller.text, '  item');
      expect(controller.selection.baseOffset, 2);
    });

    test(
      'Backspace at task content boundary removes checkbox but keeps bullet',
      () {
        const initial = '- [x] done';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        controller.selection = const TextSelection.collapsed(offset: 6);
        controller.value = const TextEditingValue(
          text: '- [x]done',
          selection: TextSelection.collapsed(offset: 5),
        );

        expect(controller.text, '- done');
        expect(controller.selection.baseOffset, 2);
      },
    );

    test('List enter policy does not run inside fenced code blocks', () {
      const initial = '```\n- item\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.indexOf('item') + 'item'.length;
      final raw = initial.replaceRange(caret, caret, '\n');
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: raw,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, raw);
    });

    test('Enter continues list marker inside blockquote', () {
      const initial = '> - item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '> - item\n> - ');
      expect(controller.selection.baseOffset, '> - item\n> - '.length);
    });

    test('Enter on empty list item inside blockquote exits list only', () {
      const initial = '> - ';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '> - \n> ');
      expect(controller.selection.baseOffset, '> - \n> '.length);
    });

    test('Enter continues ordered list marker inside blockquote', () {
      const initial = '> 4. value';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '> 4. value\n> 5. ');
      expect(controller.selection.baseOffset, '> 4. value\n> 5. '.length);
    });

    test('Enter continues task list marker inside blockquote', () {
      const initial = '> - [x] done';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '> - [x] done\n> - [ ] ');
      expect(controller.selection.baseOffset, '> - [x] done\n> - [ ] '.length);
    });

    test('Enter continues ordered task list marker inside blockquote', () {
      const initial = '> 2. [x] done';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.length;
      controller.selection = TextSelection.collapsed(offset: caret);
      controller.value = TextEditingValue(
        text: initial.replaceRange(caret, caret, '\n'),
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '> 2. [x] done\n> 3. [ ] ');
      expect(
        controller.selection.baseOffset,
        '> 2. [x] done\n> 3. [ ] '.length,
      );
    });

    test(
      'Enter on empty task item inside blockquote exits task but keeps quote',
      () {
        const initial = '> - [ ] ';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        final caret = initial.length;
        controller.selection = TextSelection.collapsed(offset: caret);
        controller.value = TextEditingValue(
          text: initial.replaceRange(caret, caret, '\n'),
          selection: TextSelection.collapsed(offset: caret + 1),
        );

        expect(controller.text, '> - [ ] \n> ');
        expect(controller.selection.baseOffset, '> - [ ] \n> '.length);
      },
    );

    test('Tab indents unordered list item for nesting', () {
      const initial = '- item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 0);
      final handled = controller.handleTabKey(reverse: false);

      expect(handled, isTrue);
      expect(controller.text, '  - item');
      expect(controller.selection.baseOffset, 2);
    });

    test('Shift+Tab outdents nested unordered list item', () {
      const initial = '  - item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 4);
      final handled = controller.handleTabKey(reverse: true);

      expect(handled, isTrue);
      expect(controller.text, '- item');
      expect(controller.selection.baseOffset, 2);
    });

    test('Tab indents quoted task list item after quote prefix', () {
      const initial = '> - [ ] todo';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 2);
      final handled = controller.handleTabKey(reverse: false);

      expect(handled, isTrue);
      expect(controller.text, '>   - [ ] todo');
    });

    test('Shift+Tab on quoted nested list preserves quote separator space', () {
      const initial = '>   - item';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(
        offset: initial.length,
      );
      final handled = controller.handleTabKey(reverse: true);

      expect(handled, isTrue);
      expect(controller.text, '> - item');
    });

    test('Tab indents selected list lines only', () {
      const initial = '- a\nnot list\n1. b';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: initial.length,
      );
      final handled = controller.handleTabKey(reverse: false);

      expect(handled, isTrue);
      expect(controller.text, '  - a\nnot list\n  1. b');
    });

    test('Toggle task checkbox inserts checkbox on list item', () {
      const initial = '- todo';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(
        offset: initial.length,
      );
      final handled = controller.toggleTaskCheckboxAtSelection();

      expect(handled, isTrue);
      expect(controller.text, '- [ ] todo');
    });

    test('Toggle task checkbox flips checked state', () {
      const initial = '- [ ] todo';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(
        offset: initial.length,
      );
      expect(controller.toggleTaskCheckboxAtSelection(), isTrue);
      expect(controller.text, '- [x] todo');

      expect(controller.toggleTaskCheckboxAtSelection(), isTrue);
      expect(controller.text, '- [ ] todo');
    });

    test(
      'Task checkbox visual range aligns unordered task with bullet column',
      () {
        const initial = '- [ ] todo';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        final tapRange = controller.taskCheckboxMarkerRangeForLine(0);
        final visualRange = controller.taskCheckboxVisualRangeForLine(0);

        expect(tapRange, const TextRange(start: 2, end: 6));
        expect(visualRange, const TextRange(start: 0, end: 6));
      },
    );

    test('Task checkbox visual range keeps ordered task after list number', () {
      const initial = '12. [ ] todo';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final tapRange = controller.taskCheckboxMarkerRangeForLine(0);
      final visualRange = controller.taskCheckboxVisualRangeForLine(0);

      expect(tapRange, const TextRange(start: 4, end: 8));
      expect(visualRange, const TextRange(start: 4, end: 8));
    });

    test(
      'Backspace on trailing empty task continuation rejoins previous line',
      () {
        const initial = '- [ ] todo\n- [ ] ';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        controller.value = controller.value.copyWith(
          selection: const TextSelection.collapsed(offset: initial.length),
          composing: TextRange.empty,
        );

        // Simulate platform backspace deleting the trailing space before the caret.
        final old = controller.value;
        controller.value = old.copyWith(
          text: initial.substring(0, initial.length - 1),
          selection: const TextSelection.collapsed(offset: initial.length - 1),
          composing: TextRange.empty,
        );

        expect(controller.text, '- [ ] todo');
        expect(controller.selection.baseOffset, 10);
      },
    );
  });
}
