import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkMarkdownBlockCommands', () {
    FlarkCommandRegistry registry() {
      return FlarkExtensionSet([
        const FlarkMarkdownBlockEditingExtension(),
      ]).commandRegistry();
    }

    test('sets heading level on the selected line', () {
      final state = FlarkEditorState.fromMarkdown(
        'Title',
        selection: const FlarkSelection.collapsed(0),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setHeadingLevel,
        payload: const FlarkSetHeadingLevelPayload(2),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '## Title');
      expect(
        result.transaction!.metadata.parseInvalidationRange,
        const FlarkSourceRange(0, 0),
      );
    });

    test('composes a heading after quote and list prefixes', () {
      // Inserting at the absolute line start would produce '## - item' /
      // '## > q', which changes the block type instead of styling it.
      final cases = <(String, int, String)>[
        ('- item', 2, '- ## item'),
        ('1. item', 1, '1. # item'),
        ('- [ ] todo', 1, '- [ ] # todo'),
        ('> quoted', 1, '> # quoted'),
      ];
      for (final (markdown, level, expected) in cases) {
        final state = FlarkEditorState.fromMarkdown(
          markdown,
          selection: const FlarkSelection.collapsed(0),
        );
        final result = registry().dispatch(
          state: state,
          command: FlarkMarkdownBlockCommands.setHeadingLevel,
          payload: FlarkSetHeadingLevelPayload(level),
        );
        final next = state.applyTransaction(result.transaction!);
        expect(next.markdown, expected, reason: markdown);
      }
    });

    test('replaces and removes headings after a prefix', () {
      final state = FlarkEditorState.fromMarkdown(
        '> # quoted',
        selection: const FlarkSelection.collapsed(0),
      );
      final raised = state.applyTransaction(
        registry()
            .dispatch(
              state: state,
              command: FlarkMarkdownBlockCommands.setHeadingLevel,
              payload: const FlarkSetHeadingLevelPayload(2),
            )
            .transaction!,
      );
      expect(raised.markdown, '> ## quoted');

      final removed = raised.applyTransaction(
        registry()
            .dispatch(
              state: raised,
              command: FlarkMarkdownBlockCommands.setHeadingLevel,
              payload: const FlarkSetHeadingLevelPayload(0),
            )
            .transaction!,
      );
      expect(removed.markdown, '> quoted');
    });

    test('changes an existing heading marker', () {
      final state = FlarkEditorState.fromMarkdown(
        '# Title',
        selection: const FlarkSelection.collapsed(3),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setHeadingLevel,
        payload: const FlarkSetHeadingLevelPayload(3),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, '### Title');
    });

    test('removes an existing heading marker with level zero', () {
      final state = FlarkEditorState.fromMarkdown(
        '### Title',
        selection: const FlarkSelection.collapsed(5),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setHeadingLevel,
        payload: const FlarkSetHeadingLevelPayload(0),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'Title');
    });

    test('rejects invalid heading levels', () {
      final state = FlarkEditorState.fromMarkdown('Title');

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setHeadingLevel,
        payload: const FlarkSetHeadingLevelPayload(7),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('between 0 and 6'));
    });

    test('toggles blockquote markers across selected lines', () {
      final state = FlarkEditorState.fromMarkdown(
        'one\ntwo',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 7),
      );

      final quotedResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.toggleQuote,
        payload: const FlarkToggleQuotePayload(),
      );
      final quoted = state.applyTransaction(quotedResult.transaction!);

      expect(quoted.markdown, '> one\n> two');

      final unquotedResult = registry().dispatch(
        state: quoted,
        command: FlarkMarkdownBlockCommands.toggleQuote,
        payload: const FlarkToggleQuotePayload(),
      );
      final unquoted = quoted.applyTransaction(unquotedResult.transaction!);

      expect(unquoted.markdown, 'one\ntwo');
    });

    test('toggles bullet list markers across selected lines', () {
      final state = FlarkEditorState.fromMarkdown(
        'one\ntwo',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 7),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.toggleBulletList,
        payload: const FlarkToggleBulletListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '- one\n- two');

      final plainResult = registry().dispatch(
        state: listed,
        command: FlarkMarkdownBlockCommands.toggleBulletList,
        payload: const FlarkToggleBulletListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, 'one\ntwo');
    });

    test('toggles bullet list markers after quote prefixes', () {
      final state = FlarkEditorState.fromMarkdown(
        '> item',
        selection: const FlarkSelection.collapsed(3),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.toggleBulletList,
        payload: const FlarkToggleBulletListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '> - item');

      final plainResult = registry().dispatch(
        state: listed,
        command: FlarkMarkdownBlockCommands.toggleBulletList,
        payload: const FlarkToggleBulletListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, '> item');
    });

    test('toggles ordered list markers across selected lines', () {
      final state = FlarkEditorState.fromMarkdown(
        'one\ntwo',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 7),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.toggleOrderedList,
        payload: const FlarkToggleOrderedListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '1. one\n2. two');

      final plainResult = registry().dispatch(
        state: listed,
        command: FlarkMarkdownBlockCommands.toggleOrderedList,
        payload: const FlarkToggleOrderedListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, 'one\ntwo');
    });

    test('toggles ordered list markers after quote prefixes', () {
      final state = FlarkEditorState.fromMarkdown(
        '> item',
        selection: const FlarkSelection.collapsed(3),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.toggleOrderedList,
        payload: const FlarkToggleOrderedListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '> 1. item');

      final plainResult = registry().dispatch(
        state: listed,
        command: FlarkMarkdownBlockCommands.toggleOrderedList,
        payload: const FlarkToggleOrderedListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, '> item');
    });

    test('toggles task list markers for plain, quoted, and bullet lines', () {
      final plain = FlarkEditorState.fromMarkdown(
        'item',
        selection: const FlarkSelection.collapsed(0),
      );
      final plainResult = registry().dispatch(
        state: plain,
        command: FlarkMarkdownBlockCommands.toggleTaskList,
        payload: const FlarkToggleTaskListPayload(),
      );
      expect(
        plain.applyTransaction(plainResult.transaction!).markdown,
        '- [ ] item',
      );

      final quoted = FlarkEditorState.fromMarkdown(
        '> item',
        selection: const FlarkSelection.collapsed(3),
      );
      final quotedResult = registry().dispatch(
        state: quoted,
        command: FlarkMarkdownBlockCommands.toggleTaskList,
        payload: const FlarkToggleTaskListPayload(),
      );
      expect(
        quoted.applyTransaction(quotedResult.transaction!).markdown,
        '> - [ ] item',
      );

      final bullet = FlarkEditorState.fromMarkdown(
        '- item',
        selection: const FlarkSelection.collapsed(2),
      );
      final bulletResult = registry().dispatch(
        state: bullet,
        command: FlarkMarkdownBlockCommands.toggleTaskList,
        payload: const FlarkToggleTaskListPayload(),
      );
      expect(
        bullet.applyTransaction(bulletResult.transaction!).markdown,
        '- [ ] item',
      );
    });

    test('toggles task checkbox state', () {
      final unchecked = FlarkEditorState.fromMarkdown(
        '- [ ] item',
        selection: const FlarkSelection.collapsed(4),
      );
      final checkedResult = registry().dispatch(
        state: unchecked,
        command: FlarkMarkdownBlockCommands.toggleTaskList,
        payload: const FlarkToggleTaskListPayload(),
      );
      final checked = unchecked.applyTransaction(checkedResult.transaction!);

      expect(checked.markdown, '- [x] item');

      final uncheckedResult = registry().dispatch(
        state: checked,
        command: FlarkMarkdownBlockCommands.toggleTaskList,
        payload: const FlarkToggleTaskListPayload(),
      );
      expect(
        checked.applyTransaction(uncheckedResult.transaction!).markdown,
        '- [ ] item',
      );
    });

    test('sets task checkbox state from an explicit task item range', () {
      final state = FlarkEditorState.fromMarkdown(
        '- [ ] item',
        selection: const FlarkSelection.collapsed(8),
      );

      final checkedResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setTaskListChecked,
        payload: const FlarkSetTaskListCheckedPayload(
          taskItemRange: FlarkSourceRange(0, 10),
          checked: true,
        ),
      );
      final checked = state.applyTransaction(checkedResult.transaction!);
      expect(checked.markdown, '- [x] item');

      final uncheckedResult = registry().dispatch(
        state: checked,
        command: FlarkMarkdownBlockCommands.setTaskListChecked,
        payload: const FlarkSetTaskListCheckedPayload(
          taskItemRange: FlarkSourceRange(0, 10),
          checked: false,
        ),
      );
      final unchecked = checked.applyTransaction(uncheckedResult.transaction!);
      expect(unchecked.markdown, '- [ ] item');
    });

    test('inserts thematic break without creating setext heading text', () {
      final state = FlarkEditorState.fromMarkdown(
        'Title',
        selection: const FlarkSelection.collapsed(5),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.insertThematicBreak,
        payload: const FlarkInsertThematicBreakPayload(),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'Title\n\n---\n');
      expect(next.selection, const FlarkSelection.collapsed(11));
    });

    test('inserts empty fenced code block at the cursor', () {
      final state = FlarkEditorState.fromMarkdown(
        'before\n',
        selection: const FlarkSelection.collapsed(7),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.insertFence,
        payload: const FlarkInsertFencePayload(language: 'dart'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'before\n```dart\n\n```');
      expect(next.selection, const FlarkSelection.collapsed(15));
    });

    test('separates inserted fenced code block from inline paragraph text', () {
      final state = FlarkEditorState.fromMarkdown(
        'before',
        selection: const FlarkSelection.collapsed(6),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.insertFence,
        payload: const FlarkInsertFencePayload(language: 'dart'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'before\n\n```dart\n\n```');
      expect(next.selection, const FlarkSelection.collapsed(16));
    });

    test('sets and clears a fenced code language', () {
      final state = FlarkEditorState.fromMarkdown(
        '```dart\nprint(1);\n```',
        selection: const FlarkSelection.collapsed(8),
      );

      final rustResult = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setFenceLanguage,
        payload: const FlarkSetFenceLanguagePayload(
          codeBlockRange: FlarkSourceRange(0, 21),
          language: 'rust',
        ),
      );
      final rust = state.applyTransaction(rustResult.transaction!);
      expect(rust.markdown, '```rust\nprint(1);\n```');
      expect(rust.selection, const FlarkSelection.collapsed(8));

      final plainResult = registry().dispatch(
        state: rust,
        command: FlarkMarkdownBlockCommands.setFenceLanguage,
        payload: const FlarkSetFenceLanguagePayload(
          codeBlockRange: FlarkSourceRange(0, 21),
          language: '',
        ),
      );
      final plain = rust.applyTransaction(plainResult.transaction!);
      expect(plain.markdown, '```\nprint(1);\n```');
    });

    test('sets fenced code language while preserving fence marker shape', () {
      final state = FlarkEditorState.fromMarkdown(
        '  ~~~~\nbody\n~~~~',
        selection: const FlarkSelection.collapsed(7),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setFenceLanguage,
        payload: const FlarkSetFenceLanguagePayload(
          codeBlockRange: FlarkSourceRange(0, 16),
          language: 'sql',
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, '  ~~~~sql\nbody\n~~~~');
    });

    test('rejects invalid fenced code language edits', () {
      final state = FlarkEditorState.fromMarkdown('```dart\ncode\n```');

      final notFence = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setFenceLanguage,
        payload: const FlarkSetFenceLanguagePayload(
          codeBlockRange: FlarkSourceRange(1, 16),
          language: 'rust',
        ),
      );
      expect(notFence.isRejected, isTrue);

      final invalidInfo = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.setFenceLanguage,
        payload: const FlarkSetFenceLanguagePayload(
          codeBlockRange: FlarkSourceRange(0, 16),
          language: 'bad`info',
        ),
      );
      expect(invalidInfo.isRejected, isTrue);
    });

    test('wraps selected source in fenced code block', () {
      final state = FlarkEditorState.fromMarkdown(
        'print(1);',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 9),
      );

      final result = registry().dispatch(
        state: state,
        command: FlarkMarkdownBlockCommands.insertFence,
        payload: const FlarkInsertFencePayload(language: 'dart'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, '```dart\nprint(1);\n```');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 8, extentOffset: 17),
      );
    });
  });
}
