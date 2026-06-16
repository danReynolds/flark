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

    test('collapsed toggle off arms muted without editing or moving', () {
      // (source, caretInside, style)
      for (final probe in <(String, int, FlarkMarkdownInlineStyle)>[
        ('**bold**', 6, FlarkMarkdownInlineStyle.strong),
        ('*em*', 3, FlarkMarkdownInlineStyle.emphasis),
        ('`code`', 5, FlarkMarkdownInlineStyle.inlineCode),
        ('~~done~~', 6, FlarkMarkdownInlineStyle.strikethrough),
      ]) {
        final controller = FlarkFlutterController.fromMarkdown(probe.$1);
        addTearDown(controller.dispose);
        controller.applySelection(
          FlarkSelection.collapsed(probe.$2),
          userEvent: 'test',
        );

        controller.commands.toggleInlineStyle(probe.$3);

        // Nothing is edited or moved; the style is just armed off so the next
        // typed character will leave the run.
        expect(controller.markdown, probe.$1, reason: 'mute of "${probe.$1}"');
        expect(controller.selection, FlarkSelection.collapsed(probe.$2));
        expect(controller.pendingInlineStyles, isEmpty);
        expect(controller.mutedInlineStyles, contains(probe.$3));
        expect(controller.commands.isInlineActive(probe.$3), isFalse);
      }
    });

    test('toggling off mid-run splits the run on the next character', () {
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(4), // mid "bold"
        userEvent: 'test',
      );

      controller.commands.toggleStrong();
      expect(
        controller.mutedInlineStyles,
        contains(FlarkMarkdownInlineStyle.strong),
      );

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '**bold**',
        newDisplayText: '**boxld**',
      );

      expect(applied, isTrue);
      // The run is split: "bo" bold, "x" plain, "ld" bold. Existing text intact.
      expect(controller.markdown, '**bo**x**ld**');
      expect(controller.mutedInlineStyles, isEmpty);
    });

    test('toggling off at the trailing edge exits on the next character', () {
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(6), // trailing edge, inside
        userEvent: 'test',
      );

      controller.commands.toggleStrong();
      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '**bold**',
        newDisplayText: '**boldx**',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '**bold**x');
    });

    test('collapsed toggle off never arms, unwraps, or corrupts', () {
      // Regression: toggling off used to unwrap the run (deleting the markers
      // around text already written); it now only exits.
      final controller = FlarkFlutterController.fromMarkdown('**bold**');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );

      controller.commands.toggleStrong();

      expect(controller.pendingInlineStyles, isEmpty);
      expect(controller.markdown, '**bold**');
      expect(controller.commands.strongActive, isFalse);
    });

    test('does not arm a style whose markers would merge (toolbar honesty)', () {
      // A literal '*' immediately before the caret: arming italic would wrap to
      // the corrupt '**y*', so the wrap is dropped at type time. Arming it
      // anyway would light the toolbar for a style the next keystroke discards,
      // so the toggle is a no-op instead — the armed state stays honest.
      final controller = FlarkFlutterController.fromMarkdown('*');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(1),
        userEvent: 'test',
      );

      controller.commands.toggleEmphasis();
      expect(controller.pendingInlineStyles, isEmpty);
      expect(controller.commands.emphasisActive, isFalse);

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: '*',
        newDisplayText: '*y',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '*y');
    });

    test('does not arm a second style at a run trailing edge', () async {
      // Bold+italic at a run's trailing edge is not representable (`**a*b***`
      // parses as literal). Arming italic there must not light the toolbar.
      final controller = FlarkFlutterController.fromMarkdown('**a**');
      addTearDown(controller.dispose);
      await controller.parseNow();
      controller.applySelection(
        const FlarkSelection.collapsed(3), // inside, before the close
        userEvent: 'test',
      );

      controller.commands.toggleEmphasis();

      expect(controller.pendingInlineStyles, isEmpty);
      expect(controller.commands.emphasisActive, isFalse);
      // Bold is unaffected — the caret is still inside the bold run.
      expect(controller.commands.strongActive, isTrue);
    });

    test('arms a second style mid-run where nesting is representable', () async {
      // Emphasis inside a bold run with trailing bold text (`**ab*x*c**`) is
      // representable, so arming italic in the middle does light the toolbar.
      final controller = FlarkFlutterController.fromMarkdown('**abc**');
      addTearDown(controller.dispose);
      await controller.parseNow();
      controller.applySelection(
        const FlarkSelection.collapsed(4), // between 'b' and 'c'
        userEvent: 'test',
      );

      controller.commands.toggleEmphasis();

      expect(
        controller.pendingInlineStyles,
        contains(FlarkMarkdownInlineStyle.emphasis),
      );
      expect(controller.commands.emphasisActive, isTrue);
    });

    test('an armed wrap flags an immediate parse; plain typing does not', () {
      final armed = FlarkFlutterController.fromMarkdown('');
      addTearDown(armed.dispose);
      armed.commands.toggleStrong();
      armed.applyProjectedTextEdit(oldDisplayText: '', newDisplayText: 'x');
      expect(armed.lastEditRequestsImmediateParse, isTrue);

      final plain = FlarkFlutterController.fromMarkdown('ab');
      addTearDown(plain.dispose);
      plain.applySelection(const FlarkSelection.collapsed(1), userEvent: 'test');
      plain.applyProjectedTextEdit(oldDisplayText: 'ab', newDisplayText: 'axb');
      expect(plain.lastEditRequestsImmediateParse, isFalse);
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
