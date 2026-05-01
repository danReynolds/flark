import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Fenced code Tab indentation', () {
    test('Tab inserts indentation at caret inside fence content', () {
      const text = '```\nabc\n```\n';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      final caret = text.indexOf('abc');
      controller.selection = TextSelection.collapsed(offset: caret);

      final handled = controller.handleTabKey(reverse: false);
      expect(handled, isTrue);
      expect(controller.text, '```\n  abc\n```\n');
      expect(controller.selection, TextSelection.collapsed(offset: caret + 2));
    });

    test('Shift+Tab outdents current content line inside fence', () {
      const text = '```\n  abc\n```\n';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      final caret = text.indexOf('abc') + 1;
      controller.selection = TextSelection.collapsed(offset: caret);

      final handled = controller.handleTabKey(reverse: true);
      expect(handled, isTrue);
      expect(controller.text, '```\nabc\n```\n');
      expect(controller.selection, const TextSelection.collapsed(offset: 5));
    });

    test('Tab then Shift+Tab round-trips a multi-line fence selection', () {
      const text = '```\nfoo\nbar\n```\n';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      final start = text.indexOf('foo');
      final end = text.indexOf('bar') + 3;
      controller.selection = TextSelection(
        baseOffset: start,
        extentOffset: end,
      );

      final indented = controller.handleTabKey(reverse: false);
      expect(indented, isTrue);
      expect(controller.text, '```\n  foo\n  bar\n```\n');

      final outdented = controller.handleTabKey(reverse: true);
      expect(outdented, isTrue);
      expect(controller.text, text);
    });

    test('Tab outside fenced code is not handled', () {
      const text = 'plain text';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: text.length);
      final handled = controller.handleTabKey(reverse: false);
      expect(handled, isFalse);
      expect(controller.text, text);
    });

    test('Tab selection touching fence lines indents content only', () {
      const text = '```\nfoo\nbar\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: text.length,
      );

      final handled = controller.handleTabKey(reverse: false);
      expect(handled, isTrue);
      expect(controller.text, '```\n  foo\n  bar\n```');
    });

    test('Tab selection spanning outside a fence is not handled', () {
      const text = 'x\n```\nfoo\n```';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: text.length,
      );

      final handled = controller.handleTabKey(reverse: false);
      expect(handled, isFalse);
      expect(controller.text, text);
    });

    test(
      'Shift+Tab with leading tab + spaces removes one leading tab only',
      () {
        const text = '```\n\t  foo\n```';
        final controller = SovereignController(text: text);
        addTearDown(controller.dispose);

        final caret = text.indexOf('foo');
        controller.selection = TextSelection.collapsed(offset: caret);

        final handled = controller.handleTabKey(reverse: true);
        expect(handled, isTrue);
        expect(controller.text, '```\n  foo\n```');
      },
    );

    test(
      'Shift+Tab with leading spaces + tab removes one leading space unit',
      () {
        const text = '```\n  \tfoo\n```';
        final controller = SovereignController(text: text);
        addTearDown(controller.dispose);

        final caret = text.indexOf('foo');
        controller.selection = TextSelection.collapsed(offset: caret);

        final handled = controller.handleTabKey(reverse: true);
        expect(handled, isTrue);
        expect(controller.text, '```\n\tfoo\n```');
      },
    );

    test(
      'Backspace at indentation boundary outdents one unit in fenced code',
      () {
        const initial = '```\n  foo\n```';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        final caret = initial.indexOf('foo');
        final rawBackspace = initial.replaceRange(caret - 1, caret, '');

        controller.value = TextEditingValue(
          text: initial,
          selection: TextSelection.collapsed(offset: caret),
        );
        controller.value = TextEditingValue(
          text: rawBackspace,
          selection: TextSelection.collapsed(offset: caret - 1),
        );

        expect(controller.text, '```\nfoo\n```');
        expect(
          controller.selection,
          TextSelection.collapsed(offset: initial.indexOf('foo') - 2),
        );
      },
    );

    test(
      'Backspace at indentation boundary removes full 4-space indent unit',
      () {
        const initial = '```\n    foo\n```';
        final controller = SovereignController(text: initial);
        addTearDown(controller.dispose);

        final caret = initial.indexOf('foo');
        final rawBackspace = initial.replaceRange(caret - 1, caret, '');

        controller.value = TextEditingValue(
          text: initial,
          selection: TextSelection.collapsed(offset: caret),
        );
        controller.value = TextEditingValue(
          text: rawBackspace,
          selection: TextSelection.collapsed(offset: caret - 1),
        );

        expect(controller.text, '```\nfoo\n```');
        expect(controller.selection, const TextSelection.collapsed(offset: 4));
      },
    );

    test('Backspace in content does not trigger indentation outdent logic', () {
      const initial = '```\n  foo\n```';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final caret = initial.indexOf('foo') + 2;
      final rawBackspace = initial.replaceRange(caret - 1, caret, '');

      controller.value = TextEditingValue(
        text: initial,
        selection: TextSelection.collapsed(offset: caret),
      );
      controller.value = TextEditingValue(
        text: rawBackspace,
        selection: TextSelection.collapsed(offset: caret - 1),
      );

      expect(controller.text, rawBackspace);
      expect(controller.selection, TextSelection.collapsed(offset: caret - 1));
    });
  });
}
