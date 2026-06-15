import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('Sticky inline run (controller parse adoption)', () {
    test('keeps a trailing-space run rendered through a real parse', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo **');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );

      await controller.parseNow();

      // The parser alone treats `**foo **` as literal; the sticky reconciler at
      // adoption keeps the markers hidden while the caret is inside the run.
      expect(controller.projection.projectText(controller.markdown), 'foo ');
    });

    test('reveals the markers once the caret leaves the run', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo **');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );

      await controller.parseNow();

      expect(controller.projection.projectText(controller.markdown), '**foo **');
    });

    test('a multi-word trailing-space run stays rendered', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo bar **');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(10),
        userEvent: 'test',
      );

      await controller.parseNow();

      expect(controller.projection.projectText(controller.markdown), 'foo bar ');
    });
  });

  group('Immediate parse after an armed wrap', () {
    test('the armed-typed run renders immediately (no raw-marker flicker)', () async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);
      controller.commands.toggleStrong();

      controller.applyProjectedTextEdit(oldDisplayText: '', newDisplayText: 'x');
      expect(controller.lastEditRequestsImmediateParse, isTrue);

      // The surface parses immediately when the flag is set; the markers hide
      // right away rather than after the debounced parse.
      await controller.parseNow();
      expect(controller.projection.projectText(controller.markdown), 'x');
    });

    test('backspacing the only armed character removes the empty markers', () async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);
      controller.commands.toggleStrong();
      controller.applyProjectedTextEdit(oldDisplayText: '', newDisplayText: 'x');
      await controller.parseNow(); // markers hidden, bold "x"

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: 'x',
        newDisplayText: '',
      );

      expect(applied, isTrue);
      // The deletion expands over the now-recognized markers — no stray `****`.
      expect(controller.markdown, '');
    });
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
