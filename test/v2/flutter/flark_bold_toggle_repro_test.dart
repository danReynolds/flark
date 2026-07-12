import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flutter_test/flutter_test.dart';

// Regression coverage for the original field report: toggling bold off after
// typing a trailing space used to leave `**hello world **` in the source —
// invalid CommonMark whose markers leaked as literal text once the caret
// left, the document was saved, or any other consumer parsed it. The write
// paths now commit the space *outside* the close marker (`**hello world** `)
// and keep bold armed so continued styled typing re-enters the run; the
// source is valid CommonMark at every step. The broader scenario matrix
// lives in flark_inline_style_sequence_test.dart.
void main() {
  test(
    'toggling bold off after a trailing space keeps the source valid',
    () async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleStrong();
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'hello world ',
        ),
        isTrue,
      );
      expect(controller.markdown, '**hello world** ');
      expect(controller.commands.strongActive, isTrue);
      await controller.parseNow();
      expect(
        controller.projection.projectText(controller.markdown),
        'hello world ',
      );

      controller.commands.toggleStrong();
      expect(controller.commands.strongActive, isFalse);
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: 'hello world ',
          newDisplayText: 'hello world x',
        ),
        isTrue,
      );
      expect(controller.markdown, '**hello world** x');

      await controller.parseNow();
      expect(
        controller.projection.projectText(controller.markdown),
        'hello world x',
      );
    },
  );

  test('keeping bold on after a trailing space re-enters the run', () async {
    final controller = FlarkFlutterController.fromMarkdown('');
    addTearDown(controller.dispose);

    controller.commands.toggleStrong();
    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: '',
        newDisplayText: 'hello world ',
      ),
      isTrue,
    );
    expect(controller.markdown, '**hello world** ');
    // Surfaces parse immediately after an armed wrap
    // (lastEditRequestsImmediateParse); headless drivers do the same so the
    // projection is current before the next keystroke.
    await controller.parseNow();

    expect(
      controller.applyProjectedTextEdit(
        oldDisplayText: 'hello world ',
        newDisplayText: 'hello world x',
      ),
      isTrue,
    );
    expect(controller.markdown, '**hello world x**');

    await controller.parseNow();
    expect(
      controller.projection.projectText(controller.markdown),
      'hello world x',
    );
  });

  test(
    'control: toggling bold off without a trailing space stays rendered',
    () async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleStrong();
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'hello world',
        ),
        isTrue,
      );
      expect(controller.markdown, '**hello world**');
      await controller.parseNow();
      expect(
        controller.projection.projectText(controller.markdown),
        'hello world',
      );

      controller.commands.toggleStrong();
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: 'hello world',
          newDisplayText: 'hello worldx',
        ),
        isTrue,
      );
      expect(controller.markdown, '**hello world**x');

      await controller.parseNow();
      expect(
        controller.projection.projectText(controller.markdown),
        'hello worldx',
      );
    },
  );
}
