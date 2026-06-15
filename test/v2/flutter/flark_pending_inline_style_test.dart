import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('Pending inline styles (armed formatting)', () {
    test('collapsed toggle arms the style without editing the document', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      final result = controller.commands.toggleStrong();

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '');
      expect(
        controller.pendingInlineStyles,
        contains(FlarkMarkdownInlineStyle.strong),
      );
      // Toolbars read this; it must light up before any text is typed.
      expect(controller.commands.strongActive, isTrue);
    });

    test('typing while armed wraps the run and leaves the caret inside', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleStrong();
      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '',
        newDisplayText: 'x',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '**x**');
      // Caret between 'x' and the closing '**', so continued typing extends
      // the run through the normal caret-affinity model.
      expect(controller.selection, const FlarkSelection.collapsed(3));
      // Pending is consumed by the wrap; "still armed" is satisfied because the
      // caret now sits inside the real source run.
      expect(controller.pendingInlineStyles, isEmpty);
      // The toolbar stays lit: the caret at the run's trailing edge reads as
      // inside the run, so the next character keeps typing bold.
      expect(controller.commands.strongActive, isTrue);
    });

    test('toggling the same style twice before typing disarms it', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleStrong();
      controller.commands.toggleStrong();

      expect(controller.pendingInlineStyles, isEmpty);
      expect(controller.commands.strongActive, isFalse);

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '',
        newDisplayText: 'x',
      );
      expect(applied, isTrue);
      // No markers: the style was disarmed before typing.
      expect(controller.markdown, 'x');
    });

    test('moving the caret before typing clears pending', () {
      final controller = FlarkFlutterController.fromMarkdown('ab');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );

      controller.commands.toggleStrong();
      expect(controller.pendingInlineStyles, isNotEmpty);

      controller.applySelection(
        const FlarkSelection.collapsed(2),
        userEvent: 'test',
      );

      expect(controller.pendingInlineStyles, isEmpty);
      expect(controller.commands.strongActive, isFalse);
    });

    test('stacked bold + italic wraps the typed run as ***x***', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleStrong();
      controller.commands.toggleEmphasis();
      expect(controller.pendingInlineStyles, hasLength(2));

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '',
        newDisplayText: 'x',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '***x***');
      expect(controller.selection, const FlarkSelection.collapsed(4));
    });

    test('inline code arming wraps the typed run in backticks', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleInlineCode();
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'x',
        ),
        isTrue,
      );
      expect(controller.markdown, '`x`');
    });

    test('strikethrough arming wraps the typed run in tildes', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      controller.commands.toggleStrikethrough();
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'x',
        ),
        isTrue,
      );
      expect(controller.markdown, '~~x~~');
    });

    test('collapsed toggle inside an existing run turns the style off', () {
      for (final probe in <(String, int, FlarkMarkdownInlineStyle, String)>[
        ('**bold**', 4, FlarkMarkdownInlineStyle.strong, 'bold'),
        ('*em*', 2, FlarkMarkdownInlineStyle.emphasis, 'em'),
        ('`code`', 3, FlarkMarkdownInlineStyle.inlineCode, 'code'),
        ('~~done~~', 4, FlarkMarkdownInlineStyle.strikethrough, 'done'),
      ]) {
        final controller = FlarkFlutterController.fromMarkdown(probe.$1);
        addTearDown(controller.dispose);
        controller.applySelection(
          FlarkSelection.collapsed(probe.$2),
          userEvent: 'test',
        );

        controller.commands.toggleInlineStyle(probe.$3);

        // The run is unwrapped, not re-armed.
        expect(controller.markdown, probe.$4, reason: 'unwrap of "${probe.$1}"');
        expect(controller.pendingInlineStyles, isEmpty);
      }
    });

    test('collapsed toggle of an active style never arms or corrupts', () {
      // Regression: arming a style already active at the caret used to nest
      // markers and corrupt the source on the next keystroke.
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(4),
        userEvent: 'test',
      );

      controller.commands.toggleStrong();

      expect(controller.pendingInlineStyles, isEmpty);
      expect(controller.markdown, 'bold');
      // No stray markers, and bold is no longer active at the caret.
      expect(controller.commands.strongActive, isFalse);
    });

    test('skips the wrap when markers would merge with an adjacent marker', () {
      // A literal '*' immediately before the caret: wrapping armed italic would
      // produce the corrupt '**y*'. The wrap is skipped and the text inserts
      // plainly instead.
      final controller = FlarkFlutterController.fromMarkdown('*');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(1),
        userEvent: 'test',
      );

      controller.commands.toggleEmphasis();
      expect(
        controller.pendingInlineStyles,
        contains(FlarkMarkdownInlineStyle.emphasis),
      );

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '*',
        newDisplayText: '*y',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '*y');
    });

    test('emits a pendingInlineStylesChanged event when armed', () async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      final events = <FlarkControllerEvent>[];
      final subscription = controller.events.listen(events.add);
      addTearDown(subscription.cancel);

      controller.commands.toggleStrong();
      await Future<void>.delayed(Duration.zero);

      expect(events, hasLength(1));
      expect(events.single.kind, FlarkControllerEventKind.pendingInlineStylesChanged);
      // A pure arming change touches neither document nor selection, so it does
      // not leak into the markdown/selection projections.
      expect(events.single.markdownChanged, isFalse);
      expect(events.single.selectionChanged, isFalse);
    });
  });
}
