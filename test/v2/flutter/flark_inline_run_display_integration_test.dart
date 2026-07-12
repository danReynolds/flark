import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Display is caret-independent', () {
    // The sticky-run era re-hid the markers of `**foo **`-shaped literal text
    // while the caret sat inside it, so the same document rendered differently
    // depending on selection — and differently again for any consumer of
    // `controller.markdown`. The write paths now keep editor-authored runs
    // flanking-valid at every revision, and literal text renders literally no
    // matter where the caret is.
    test(
      'literal `**foo **` renders literally with the caret inside',
      () async {
        final controller = FlarkFlutterController.fromMarkdown('**foo **');
        addTearDown(controller.dispose);
        controller.applySelection(
          const FlarkSelection.collapsed(6),
          userEvent: 'test',
        );

        await controller.parseNow();

        expect(
          controller.projection.projectText(controller.markdown),
          '**foo **',
        );
      },
    );

    test(
      'literal `**foo **` renders literally with the caret outside',
      () async {
        final controller = FlarkFlutterController.fromMarkdown('**foo **');
        addTearDown(controller.dispose);
        controller.applySelection(
          const FlarkSelection.collapsed(0),
          userEvent: 'test',
        );

        await controller.parseNow();

        expect(
          controller.projection.projectText(controller.markdown),
          '**foo **',
        );
      },
    );

    test('editor-authored trailing whitespace never needs rescuing', () async {
      // Typing "foo " with bold armed commits the space outside the close
      // marker, so the parse alone styles the run — no caret-dependent
      // reconciliation exists to release or go stale.
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);
      controller.commands.toggleStrong();
      controller.applyProjectedTextEdit(
        oldDisplayText: '',
        newDisplayText: 'foo ',
      );
      expect(controller.markdown, '**foo** ');

      await controller.parseNow();
      expect(controller.projection.projectText(controller.markdown), 'foo ');

      // The caret leaving the run changes nothing about the rendering.
      controller.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );
      await controller.parseNow();
      expect(controller.projection.projectText(controller.markdown), 'foo ');
    });
  });

  group('Immediate parse after an armed wrap', () {
    test(
      'the armed-typed run renders immediately (no raw-marker flicker)',
      () async {
        final controller = FlarkFlutterController.fromMarkdown('');
        addTearDown(controller.dispose);
        controller.commands.toggleStrong();

        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'x',
        );
        expect(controller.lastEditRequestsImmediateParse, isTrue);

        // The surface parses immediately when the flag is set; the markers hide
        // right away rather than after the debounced parse.
        await controller.parseNow();
        expect(controller.projection.projectText(controller.markdown), 'x');
      },
    );

    test(
      'backspacing the only armed character removes the empty markers',
      () async {
        final controller = FlarkFlutterController.fromMarkdown('');
        addTearDown(controller.dispose);
        controller.commands.toggleStrong();
        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'x',
        );
        await controller.parseNow(); // markers hidden, bold "x"

        final applied = controller.applyProjectedTextEdit(
          oldDisplayText: 'x',
          newDisplayText: '',
        );

        expect(applied, isTrue);
        // The deletion expands over the now-recognized markers — no stray `****`.
        expect(controller.markdown, '');
      },
    );
  });

  group('Inline toggle off (exit, do not unwrap)', () {
    test('toggling a style off then typing continues unstyled', () async {
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      await controller.parseNow();
      // Caret at the run's trailing edge: inside, before the hidden close.
      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );

      // Turning bold off keeps the already-written text bold and exits the run.
      controller.commands.toggleStrong();
      expect(controller.markdown, '**bold**');
      expect(controller.commands.strongActive, isFalse);

      // The next typed character lands outside the run as plain text.
      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: 'bold',
        newDisplayText: 'boldx',
      );
      expect(applied, isTrue);
      expect(controller.markdown, '**bold**x');
    });
  });
}
