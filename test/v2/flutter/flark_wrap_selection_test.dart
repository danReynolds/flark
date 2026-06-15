import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('Wrap selection by typing a delimiter', () {
    FlarkFlutterController controllerFor(String markdown, FlarkSelection sel) {
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      controller.applySelection(sel, userEvent: 'test');
      return controller;
    }

    test('typing * over a selection wraps it in emphasis', () {
      final controller = controllerFor(
        'foo bar',
        const FlarkSelection(baseOffset: 0, extentOffset: 3),
      );

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: 'foo bar',
        newDisplayText: '* bar',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '*foo* bar');
      // Inner text stays selected so a second keystroke can nest.
      expect(
        controller.selection,
        const FlarkSelection(baseOffset: 1, extentOffset: 4),
      );
    });

    test('typing the delimiter again nests to the next level', () {
      final controller = controllerFor(
        '*foo*',
        const FlarkSelection(baseOffset: 1, extentOffset: 4),
      );

      controller.applyProjectedTextEdit(
        oldDisplayText: '*foo*',
        newDisplayText: '***',
      );

      expect(controller.markdown, '**foo**');
      expect(
        controller.selection,
        const FlarkSelection(baseOffset: 2, extentOffset: 5),
      );
    });

    test('brackets, quotes and backticks wrap as pairs', () {
      for (final probe in <(String, String)>[
        ('(', '(foo)'),
        ('[', '[foo]'),
        ('{', '{foo}'),
        ('"', '"foo"'),
        ('`', '`foo`'),
        ('_', '_foo_'),
      ]) {
        final controller = controllerFor(
          'foo',
          const FlarkSelection(baseOffset: 0, extentOffset: 3),
        );
        controller.applyProjectedTextEdit(
          oldDisplayText: 'foo',
          newDisplayText: probe.$1,
        );
        expect(controller.markdown, probe.$2, reason: 'wrap with "${probe.$1}"');
      }
    });

    test('typing a normal character over a selection replaces it', () {
      final controller = controllerFor(
        'foo bar',
        const FlarkSelection(baseOffset: 0, extentOffset: 3),
      );

      controller.applyProjectedTextEdit(
        oldDisplayText: 'foo bar',
        newDisplayText: 'x bar',
      );

      expect(controller.markdown, 'x bar');
    });

    test('typing a delimiter with no selection just inserts it', () {
      final controller = controllerFor('foo', const FlarkSelection.collapsed(3));

      controller.applyProjectedTextEdit(
        oldDisplayText: 'foo',
        newDisplayText: 'foo*',
      );

      expect(controller.markdown, 'foo*');
    });
  });
}
