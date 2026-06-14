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
