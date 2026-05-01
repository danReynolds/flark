import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('Sovereign table editing', () {
    test(
      'Enter continues a GFM table row after separator row is established',
      () {
        final controller = SovereignController(
          text: '| a | b |\n| --- | --- |\n| c | d |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        controller.selection = TextSelection.collapsed(
          offset: controller.text.length,
        );
        controller.handleEnter();

        expect(
          controller.text,
          equals('| a   | b   |\n| --- | --- |\n| c   | d   |\n|     |     |'),
        );
        expect(
          controller.selection.baseOffset,
          equals('| a   | b   |\n| --- | --- |\n| c   | d   |\n| '.length),
        );
      },
    );

    test('Enter on a table separator row inserts an empty row template', () {
      final controller = SovereignController(
        text: '| a | b |\n| --- | --- |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(
        controller.text,
        equals('| a   | b   |\n| --- | --- |\n|     |     |'),
      );
      expect(
        controller.selection.baseOffset,
        equals('| a   | b   |\n| --- | --- |\n| '.length),
      );
    });

    test('Enter formats table rows to aligned widths', () {
      final controller = SovereignController(
        text: '| longer | x |\n| --- | --- |\n| c | d |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(
        controller.text,
        equals(
          '| longer | x   |\n'
          '| ------ | --- |\n'
          '| c      | d   |\n'
          '|        |     |',
        ),
      );
    });

    test('Enter preserves separator alignment markers when formatting', () {
      final controller = SovereignController(
        text: '| left | right | center |\n'
            '| :-- | --: | :-: |\n'
            '| a | b | c |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(
        controller.text,
        equals(
          '| left | right | center |\n'
          '| :---- | -----: | :------: |\n'
          '| a    | b     | c      |\n'
          '|      |       |        |',
        ),
      );
    });

    test('Enter preserves indentation for indented tables', () {
      final controller = SovereignController(
        text: '  | a | b |\n  | --- | --- |\n  | c | d |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(
        controller.text,
        equals(
          '  | a   | b   |\n'
          '  | --- | --- |\n'
          '  | c   | d   |\n'
          '  |     |     |',
        ),
      );
      expect(
        controller.selection.baseOffset,
        equals(
          '  | a   | b   |\n  | --- | --- |\n  | c   | d   |\n  | '.length,
        ),
      );
    });

    test('Enter does not continue a pipe row before a separator exists', () {
      final controller = SovereignController(
        text: '| a | b |\n| c | d |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('| a | b |\n| c | d |\n'));
    });

    test('Enter is a no-op while IME composing is active inside a table', () {
      final controller = SovereignController(
        text: '| a | b |\n| --- | --- |\n| c | d |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final original = controller.value;
      final cOffset = controller.text.lastIndexOf('c');
      controller.value = TextEditingValue(
        text: controller.text,
        selection: TextSelection.collapsed(offset: controller.text.length),
        composing: TextRange(start: cOffset, end: cOffset + 1),
      );

      controller.handleEnter();

      expect(controller.text, equals(original.text));
      expect(controller.selection.baseOffset, equals(controller.text.length));
      expect(controller.value.composing.isValid, isTrue);
    });

    test('Tab navigates to next and previous table cells', () {
      final controller = SovereignController(
        text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final cOffset = controller.text.lastIndexOf('c');
      final dOffset = controller.text.lastIndexOf('d');

      controller.selection = TextSelection.collapsed(offset: cOffset);
      expect(controller.handleTabKey(reverse: false), isTrue);
      expect(controller.selection.baseOffset, equals(dOffset));

      expect(controller.handleTabKey(reverse: true), isTrue);
      expect(controller.selection.baseOffset, equals(cOffset));
    });

    test('Tab and Shift+Tab skip the separator row when crossing rows', () {
      final controller = SovereignController(
        text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final headerB = controller.text.indexOf('b');
      final rowC = controller.text.lastIndexOf('c');

      controller.selection = TextSelection.collapsed(offset: headerB);
      expect(controller.handleTabKey(reverse: false), isTrue);
      expect(controller.selection.baseOffset, equals(rowC));

      expect(controller.handleTabKey(reverse: true), isTrue);
      expect(controller.selection.baseOffset, equals(headerB));
    });

    test('Tab on first row first cell with Shift+Tab returns false', () {
      final controller = SovereignController(
        text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final aOffset = controller.text.indexOf('a');
      controller.selection = TextSelection.collapsed(offset: aOffset);
      expect(controller.handleTabKey(reverse: true), isFalse);
      expect(controller.selection.baseOffset, equals(aOffset));
    });

    test('Tab does not activate on table-like lines without separator row', () {
      final controller = SovereignController(
        text: '| a | b |\n| c | d |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final cOffset = controller.text.lastIndexOf('c');
      controller.selection = TextSelection.collapsed(offset: cOffset);
      expect(controller.handleTabKey(reverse: false), isFalse);
      expect(controller.selection.baseOffset, equals(cOffset));
    });

    test(
      'Tab navigation is disabled while IME composing is active in a table',
      () {
        final controller = SovereignController(
          text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        final cOffset = controller.text.lastIndexOf('c');
        controller.value = TextEditingValue(
          text: controller.text,
          selection: TextSelection.collapsed(offset: cOffset),
          composing: TextRange(start: cOffset, end: cOffset + 1),
        );

        final handled = controller.handleTabKey(reverse: false);

        expect(handled, isFalse);
        expect(
          controller.text,
          equals('| a   | b   |\n| --- | --- |\n| c   | d   |'),
        );
        expect(controller.selection.baseOffset, equals(cOffset));
        expect(
          controller.value.composing,
          equals(TextRange(start: cOffset, end: cOffset + 1)),
        );
      },
    );

    test('Tab cell parsing ignores escaped pipes inside cell content', () {
      final controller = SovereignController(
        text: '| a \\| b | c   |\n| ------ | --- |\n| x      | y   |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final firstCellA = controller.text.indexOf('a');
      final secondCellC = controller.text.indexOf('c   |');

      controller.selection = TextSelection.collapsed(offset: firstCellA);
      expect(controller.handleTabKey(reverse: false), isTrue);
      expect(controller.selection.baseOffset, equals(secondCellC));
    });

    test('Table tab navigation does not hijack fenced code tab handling', () {
      final controller = SovereignController(
        text: '```\n| a | b |\n```',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final aOffset = controller.text.indexOf('a');
      final bOffset = controller.text.indexOf('b');
      controller.selection = TextSelection.collapsed(offset: aOffset);

      expect(controller.handleTabKey(reverse: false), isTrue);
      expect(controller.text, contains('|   a | b |'));
      expect(controller.selection.baseOffset, isNot(equals(bOffset)));
    });

    test(
      'Tab on last cell inserts a new aligned row and moves caret to it',
      () {
        final controller = SovereignController(
          text: '| longer | x   |\n| ------ | --- |\n| c      | d   |',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        final dOffset = controller.text.lastIndexOf('d');
        controller.selection = TextSelection.collapsed(offset: dOffset);

        expect(controller.handleTabKey(reverse: false), isTrue);
        expect(
          controller.text,
          equals(
            '| longer | x   |\n'
            '| ------ | --- |\n'
            '| c      | d   |\n'
            '|        |     |',
          ),
        );
        expect(
          controller.selection.baseOffset,
          equals(
            '| longer | x   |\n| ------ | --- |\n| c      | d   |\n| '.length,
          ),
        );
      },
    );

    test('Tab on separator row skips to the next data row first cell', () {
      final controller = SovereignController(
        text: '| a   | b   |\n| --- | --- |\n| c   | d   |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final sepOffset = controller.text.indexOf('---');
      final rowC = controller.text.lastIndexOf('c');
      controller.selection = TextSelection.collapsed(offset: sepOffset);

      expect(controller.handleTabKey(reverse: false), isTrue);
      expect(controller.selection.baseOffset, equals(rowC));
    });

    test('Quoted tables are not auto-continued as tables in phase 1', () {
      final controller = SovereignController(
        text: '> | a | b |\n> | --- | --- |\n> | c | d |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(
        controller.text,
        // Quote continuation may still apply; the table policy should not.
        equals('> | a | b |\n> | --- | --- |\n> | c | d |\n> '),
      );
    });

    test('Formatting stays scoped to the active table region', () {
      final controller = SovereignController(
        text:
            '| a | b |\n| --- | --- |\n| c | d |\n\n| xx | y |\n| --- | --- |\n| z | q |',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      final firstTableEnd = controller.text.indexOf('\n\n');
      controller.selection = TextSelection.collapsed(offset: firstTableEnd);
      controller.handleEnter();

      expect(
        controller.text,
        contains(
          '| a   | b   |\n| --- | --- |\n| c   | d   |\n|     |     |\n\n| xx | y |',
        ),
      );
      // Second table remains source-original (not reformatted).
      expect(controller.text, contains('| xx | y |\n| --- | --- |\n| z | q |'));
    });

    test('Enter on a non-table pipe line does not auto-continue', () {
      final controller = SovereignController(
        text: 'alpha | beta',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('alpha | beta\n'));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });
  });
}
