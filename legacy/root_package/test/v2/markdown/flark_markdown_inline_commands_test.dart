import 'package:test/test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkMarkdownInlineCommands', () {
    test('wraps a selected source range with style markers', () {
      final state = FlarkEditorState.fromMarkdown(
        'hello world',
        selection: const FlarkSelection(baseOffset: 6, extentOffset: 11),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(
        result.transaction!.metadata.projectionInvalidationRange,
        const FlarkSourceRange(6, 11),
      );
      expect(next.markdown, 'hello **world**');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 8, extentOffset: 13),
      );
    });

    String toggleAll(String markdown, FlarkMarkdownInlineStyle style) {
      final state = FlarkEditorState.fromMarkdown(
        markdown,
        selection: FlarkSelection(baseOffset: 0, extentOffset: markdown.length),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();
      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: FlarkToggleInlineStylePayload(style),
      );
      if (!result.isHandled || result.transaction == null) return markdown;
      return state.applyTransaction(result.transaction!).markdown;
    }

    test('wraps each paragraph when the selection spans a blank line', () {
      // A single delimiter pair cannot span a blank line, so a whole-selection
      // wrap would emit invalid `**alpha\n\nbeta**`. Each paragraph is wrapped
      // instead, keeping the source valid CommonMark.
      expect(
        toggleAll('alpha\n\nbeta', FlarkMarkdownInlineStyle.strong),
        '**alpha**\n\n**beta**',
      );
      expect(
        toggleAll('one\n\ntwo\n\nthree', FlarkMarkdownInlineStyle.emphasis),
        '*one*\n\n*two*\n\n*three*',
      );
      // A single soft line break stays one paragraph (emphasis may soft-wrap).
      expect(
        toggleAll('soft\nwrap', FlarkMarkdownInlineStyle.strong),
        '**soft\nwrap**',
      );
    });

    test('leaves a paragraph unstyled when the wrap would misparse', () {
      // 'a**b' wrapped in ** would close early ('**a**b**'); the valid subset
      // is to leave it unstyled rather than emit invalid markdown.
      expect(toggleAll('a**b', FlarkMarkdownInlineStyle.strong), 'a**b');
      // Mixed: the clean paragraph is styled, the colliding one is left alone.
      expect(
        toggleAll('alpha\n\na**b', FlarkMarkdownInlineStyle.strong),
        '**alpha**\n\na**b',
      );
    });

    test('leaves a paragraph unstyled when the wrap would fuse at an edge', () {
      // A marker char at the core edge fuses with the injected delimiter
      // (`foo*` -> `**foo***`) and a trailing backslash escapes the injected
      // closing marker (`a\` -> `*a\*`); both misparse, so leave them unstyled.
      expect(toggleAll('foo*', FlarkMarkdownInlineStyle.strong), 'foo*');
      expect(toggleAll('*bar', FlarkMarkdownInlineStyle.strong), '*bar');
      expect(toggleAll('a\\', FlarkMarkdownInlineStyle.emphasis), 'a\\');
    });

    test('does not regress valid interior markers when wrapping', () {
      // A space-flanked '*' is literal, and a nested emphasis run is legal
      // inside strong — neither should block the wrap.
      expect(
        toggleAll('2 * 3', FlarkMarkdownInlineStyle.strong),
        '**2 * 3**',
      );
      expect(
        toggleAll('a *b* c', FlarkMarkdownInlineStyle.strong),
        '**a *b* c**',
      );
    });

    test('applies fresh over a word abutting an intraword underscore', () {
      // 'my_variable', select 'variable' (3..11): its left edge touches the
      // intraword `_`, which is NOT a real emphasis delimiter (flanking), so
      // Italic must wrap fresh rather than misread a partial `_` wrap and
      // reject (which would leave the word unformatted).
      final state = FlarkEditorState.fromMarkdown(
        'my_variable',
        selection: const FlarkSelection(baseOffset: 3, extentOffset: 11),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );

      expect(result.isHandled, isTrue);
      final next = state.applyTransaction(result.transaction!);
      expect(next.markdown, 'my_*variable*');
    });

    test('unwraps markers around a selected source range', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world**',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'world');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 0, extentOffset: 5),
      );
    });

    test('unwraps markers included in the selected source range', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world**',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 9),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'world');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 0, extentOffset: 5),
      );
    });

    test('wraps selected text with inline code markers', () {
      final state = FlarkEditorState.fromMarkdown(
        'use code',
        selection: const FlarkSelection(baseOffset: 4, extentOffset: 8),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.inlineCode,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'use `code`');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 5, extentOffset: 9),
      );
    });

    test('rejects selections that include only the opening marker', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world**',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('partially overlap'));
      expect(result.transaction, isNull);
    });

    test('rejects selections with only one surrounding marker', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('partially overlap'));
    });

    test('toggling emphasis inside strong nests instead of stripping', () {
      // The inner '*' of '**bold**' must not pass as an emphasis pair: a
      // 2-run carries strong only. Toggling italic adds a layer.
      final state = FlarkEditorState.fromMarkdown(
        '**bold**',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 6),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '***bold***');
    });

    test('toggling emphasis off em+strong keeps the strong pair', () {
      final state = FlarkEditorState.fromMarkdown(
        '***bold***',
        selection: const FlarkSelection(baseOffset: 3, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '**bold**');
    });

    test('toggling strong off em+strong keeps the emphasis pair', () {
      final state = FlarkEditorState.fromMarkdown(
        '***bold***',
        selection: const FlarkSelection(baseOffset: 3, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '*bold*');
    });

    test('does not unwrap escaped surrounding markers', () {
      final state = FlarkEditorState.fromMarkdown(
        r'\*world\*',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, r'\**world*\*');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 3, extentOffset: 8),
      );
    });

    test('unwraps a selection wrapped in either equivalent delimiter form', () {
      // Toggling a style off must unwrap whichever spelling the source uses,
      // canonical or alternate. Against the pre-fix command every alternate row
      // corrupts instead of unwrapping (e.g. `_text_` -> `_*text*_`, `__text__`
      // -> `__**text**__`, `~text~` -> `~~~text~~~`) because only the canonical
      // marker was recognized.
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      for (final probe in <(String, int, int, FlarkMarkdownInlineStyle)>[
        // (wrapped source, inner start, inner end, style)
        ('*text*', 1, 5, FlarkMarkdownInlineStyle.emphasis),
        ('_text_', 1, 5, FlarkMarkdownInlineStyle.emphasis),
        ('**text**', 2, 6, FlarkMarkdownInlineStyle.strong),
        ('__text__', 2, 6, FlarkMarkdownInlineStyle.strong),
        ('~~text~~', 2, 6, FlarkMarkdownInlineStyle.strikethrough),
        ('~text~', 1, 5, FlarkMarkdownInlineStyle.strikethrough),
        ('`text`', 1, 5, FlarkMarkdownInlineStyle.inlineCode),
      ]) {
        final state = FlarkEditorState.fromMarkdown(
          probe.$1,
          selection: FlarkSelection(
            baseOffset: probe.$2,
            extentOffset: probe.$3,
          ),
        );
        final result = registry.dispatch(
          state: state,
          command: FlarkMarkdownInlineCommands.toggleInlineStyle,
          payload: FlarkToggleInlineStylePayload(probe.$4),
        );

        expect(
          result.isHandled,
          isTrue,
          reason: 'toggling ${probe.$4} over "${probe.$1}" should unwrap',
        );
        final next = state.applyTransaction(result.transaction!);
        expect(
          next.markdown,
          'text',
          reason: 'toggling ${probe.$4} over "${probe.$1}" should yield "text"',
        );
        expect(
          next.selection,
          const FlarkSelection(baseOffset: 0, extentOffset: 4),
          reason: 'unwrapping "${probe.$1}" should reselect the inner text',
        );
      }
    });

    test('applies a style to plain text with the canonical marker', () {
      // The control for the unwrap matrix: applying a style always writes the
      // canonical delimiter, never an alternate.
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      for (final probe in <(FlarkMarkdownInlineStyle, String)>[
        (FlarkMarkdownInlineStyle.emphasis, '*text*'),
        (FlarkMarkdownInlineStyle.strong, '**text**'),
        (FlarkMarkdownInlineStyle.strikethrough, '~~text~~'),
        (FlarkMarkdownInlineStyle.inlineCode, '`text`'),
      ]) {
        final state = FlarkEditorState.fromMarkdown(
          'text',
          selection: const FlarkSelection(baseOffset: 0, extentOffset: 4),
        );
        final result = registry.dispatch(
          state: state,
          command: FlarkMarkdownInlineCommands.toggleInlineStyle,
          payload: FlarkToggleInlineStylePayload(probe.$1),
        );
        final next = state.applyTransaction(result.transaction!);

        expect(
          next.markdown,
          probe.$2,
          reason:
              'applying ${probe.$1} to plain text should write the '
              'canonical marker',
        );
      }
    });

    test('rejects collapsed selections until active mark state exists', () {
      final state = FlarkEditorState.fromMarkdown('world');
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('selected source range'));
      expect(result.transaction, isNull);
    });

    test('rejects a collapsed caret (arming is handled on the controller)', () {
      // The command operates on ranges only; collapsed-caret arm-on/arm-off
      // lives on FlarkFlutterController (pending / muted).
      final state = FlarkEditorState.fromMarkdown(
        'a **bold** b',
        selection: const FlarkSelection.collapsed(6),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.transaction, isNull);
    });
  });
}
