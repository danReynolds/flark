import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/flutter/flark_markdown_input_policy.dart';

void main() {
  group('Collapsed backspace over an inline run', () {
    FlarkMarkdownInputPolicy policyFor(FlarkFlutterController controller) {
      return FlarkMarkdownInputPolicy(
        controller: controller,
        enterUserEvent: 'enter',
        backspaceUserEvent: 'backspace',
      );
    }

    void backspace(FlarkFlutterController controller) {
      policyFor(controller).dispatchBackspace(
        currentSelection: () => controller.selection,
        applySelection: (selection) =>
            controller.applySelection(selection, userEvent: 'sel'),
      );
    }

    test('deleting the last character removes the orphaned markers', () async {
      for (final probe in <(String, int)>[
        ('**f**', 3),
        ('*e*', 2),
        ('`c`', 2),
        ('~~s~~', 3),
      ]) {
        final controller = FlarkFlutterController.fromMarkdown(probe.$1);
        addTearDown(controller.dispose);
        await controller.parseNow();
        controller.applySelection(
          FlarkSelection.collapsed(probe.$2),
          userEvent: 'test',
        );

        backspace(controller);

        expect(controller.markdown, '', reason: 'backspacing "${probe.$1}"');
      }
    });

    test('deleting a non-final character keeps the longer run intact', () async {
      final controller = FlarkFlutterController.fromMarkdown('**fo**');
      addTearDown(controller.dispose);
      await controller.parseNow();
      controller.applySelection(
        const FlarkSelection.collapsed(4),
        userEvent: 'test',
      );

      backspace(controller);

      expect(controller.markdown, '**f**');
    });

    test('backspacing a whole run leaves no orphaned markers', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo**');
      addTearDown(controller.dispose);
      await controller.parseNow();
      controller.applySelection(
        const FlarkSelection.collapsed(5), // trailing edge
        userEvent: 'test',
      );

      // Three rapid backspaces with no parse between (prediction only).
      for (var i = 0; i < 3; i += 1) {
        backspace(controller);
      }

      expect(controller.markdown, '');
    });

    test('a caret just past the close re-enters the run, not the marker',
        () async {
      // Regression: the source caret sits outside the run (after the hidden
      // closing `**`). A naive delete cut a marker char, leaving the
      // unbalanced `**bold*`; it must drop the last content char instead.
      for (final probe in <(String, int, String)>[
        ('**bold**', 8, '**bol**'),
        ('*em*', 4, '*e*'),
        ('`code`', 6, '`cod`'),
        ('~~done~~', 8, '~~don~~'),
      ]) {
        final controller = FlarkFlutterController.fromMarkdown(probe.$1);
        addTearDown(controller.dispose);
        await controller.parseNow();
        controller.applySelection(
          FlarkSelection.collapsed(probe.$2),
          userEvent: 'test',
        );

        backspace(controller);

        expect(
          controller.markdown,
          probe.$3,
          reason: 'backspacing just past the close of "${probe.$1}"',
        );
      }
    });

    test('backspacing plain text back into a run stays balanced', () async {
      // The reported sequence: type past a bold run, then backspace across the
      // plain text into it. Each step keeps the markers balanced.
      final controller = FlarkFlutterController.fromMarkdown('**bold**x');
      addTearDown(controller.dispose);
      await controller.parseNow();
      controller.applySelection(
        const FlarkSelection.collapsed(9), // after the trailing 'x'
        userEvent: 'test',
      );

      // Rapid backspaces with no parse between (prediction only), matching the
      // reported "backspace all the way" repro.
      backspace(controller); // deletes the plain 'x'
      expect(controller.markdown, '**bold**');

      backspace(controller); // re-enters the run; drops 'd', stays balanced
      expect(controller.markdown, '**bol**');
    });
  });
}
