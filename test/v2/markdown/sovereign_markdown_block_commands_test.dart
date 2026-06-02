import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignMarkdownBlockCommands', () {
    SovereignCommandRegistry registry() {
      return SovereignExtensionSet([
        const SovereignMarkdownBlockEditingExtension(),
      ]).commandRegistry();
    }

    test('sets heading level on the selected line', () {
      final state = SovereignEditorState.fromMarkdown(
        'Title',
        selection: const SovereignSelection.collapsed(0),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setHeadingLevel,
        payload: const SovereignSetHeadingLevelPayload(2),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '## Title');
      expect(
        result.transaction!.metadata.parseInvalidationRange,
        const SovereignSourceRange(0, 0),
      );
    });

    test('changes an existing heading marker', () {
      final state = SovereignEditorState.fromMarkdown(
        '# Title',
        selection: const SovereignSelection.collapsed(3),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setHeadingLevel,
        payload: const SovereignSetHeadingLevelPayload(3),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, '### Title');
    });

    test('removes an existing heading marker with level zero', () {
      final state = SovereignEditorState.fromMarkdown(
        '### Title',
        selection: const SovereignSelection.collapsed(5),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setHeadingLevel,
        payload: const SovereignSetHeadingLevelPayload(0),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'Title');
    });

    test('rejects invalid heading levels', () {
      final state = SovereignEditorState.fromMarkdown('Title');

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setHeadingLevel,
        payload: const SovereignSetHeadingLevelPayload(7),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('between 0 and 6'));
    });

    test('toggles blockquote markers across selected lines', () {
      final state = SovereignEditorState.fromMarkdown(
        'one\ntwo',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 7),
      );

      final quotedResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.toggleQuote,
        payload: const SovereignToggleQuotePayload(),
      );
      final quoted = state.applyTransaction(quotedResult.transaction!);

      expect(quoted.markdown, '> one\n> two');

      final unquotedResult = registry().dispatch(
        state: quoted,
        command: SovereignMarkdownBlockCommands.toggleQuote,
        payload: const SovereignToggleQuotePayload(),
      );
      final unquoted = quoted.applyTransaction(unquotedResult.transaction!);

      expect(unquoted.markdown, 'one\ntwo');
    });

    test('toggles bullet list markers across selected lines', () {
      final state = SovereignEditorState.fromMarkdown(
        'one\ntwo',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 7),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.toggleBulletList,
        payload: const SovereignToggleBulletListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '- one\n- two');

      final plainResult = registry().dispatch(
        state: listed,
        command: SovereignMarkdownBlockCommands.toggleBulletList,
        payload: const SovereignToggleBulletListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, 'one\ntwo');
    });

    test('toggles bullet list markers after quote prefixes', () {
      final state = SovereignEditorState.fromMarkdown(
        '> item',
        selection: const SovereignSelection.collapsed(3),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.toggleBulletList,
        payload: const SovereignToggleBulletListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '> - item');

      final plainResult = registry().dispatch(
        state: listed,
        command: SovereignMarkdownBlockCommands.toggleBulletList,
        payload: const SovereignToggleBulletListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, '> item');
    });

    test('toggles ordered list markers across selected lines', () {
      final state = SovereignEditorState.fromMarkdown(
        'one\ntwo',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 7),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.toggleOrderedList,
        payload: const SovereignToggleOrderedListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '1. one\n2. two');

      final plainResult = registry().dispatch(
        state: listed,
        command: SovereignMarkdownBlockCommands.toggleOrderedList,
        payload: const SovereignToggleOrderedListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, 'one\ntwo');
    });

    test('toggles ordered list markers after quote prefixes', () {
      final state = SovereignEditorState.fromMarkdown(
        '> item',
        selection: const SovereignSelection.collapsed(3),
      );

      final listedResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.toggleOrderedList,
        payload: const SovereignToggleOrderedListPayload(),
      );
      final listed = state.applyTransaction(listedResult.transaction!);

      expect(listed.markdown, '> 1. item');

      final plainResult = registry().dispatch(
        state: listed,
        command: SovereignMarkdownBlockCommands.toggleOrderedList,
        payload: const SovereignToggleOrderedListPayload(),
      );
      final plain = listed.applyTransaction(plainResult.transaction!);

      expect(plain.markdown, '> item');
    });

    test('toggles task list markers for plain, quoted, and bullet lines', () {
      final plain = SovereignEditorState.fromMarkdown(
        'item',
        selection: const SovereignSelection.collapsed(0),
      );
      final plainResult = registry().dispatch(
        state: plain,
        command: SovereignMarkdownBlockCommands.toggleTaskList,
        payload: const SovereignToggleTaskListPayload(),
      );
      expect(
        plain.applyTransaction(plainResult.transaction!).markdown,
        '- [ ] item',
      );

      final quoted = SovereignEditorState.fromMarkdown(
        '> item',
        selection: const SovereignSelection.collapsed(3),
      );
      final quotedResult = registry().dispatch(
        state: quoted,
        command: SovereignMarkdownBlockCommands.toggleTaskList,
        payload: const SovereignToggleTaskListPayload(),
      );
      expect(
        quoted.applyTransaction(quotedResult.transaction!).markdown,
        '> - [ ] item',
      );

      final bullet = SovereignEditorState.fromMarkdown(
        '- item',
        selection: const SovereignSelection.collapsed(2),
      );
      final bulletResult = registry().dispatch(
        state: bullet,
        command: SovereignMarkdownBlockCommands.toggleTaskList,
        payload: const SovereignToggleTaskListPayload(),
      );
      expect(
        bullet.applyTransaction(bulletResult.transaction!).markdown,
        '- [ ] item',
      );
    });

    test('toggles task checkbox state', () {
      final unchecked = SovereignEditorState.fromMarkdown(
        '- [ ] item',
        selection: const SovereignSelection.collapsed(4),
      );
      final checkedResult = registry().dispatch(
        state: unchecked,
        command: SovereignMarkdownBlockCommands.toggleTaskList,
        payload: const SovereignToggleTaskListPayload(),
      );
      final checked = unchecked.applyTransaction(checkedResult.transaction!);

      expect(checked.markdown, '- [x] item');

      final uncheckedResult = registry().dispatch(
        state: checked,
        command: SovereignMarkdownBlockCommands.toggleTaskList,
        payload: const SovereignToggleTaskListPayload(),
      );
      expect(
        checked.applyTransaction(uncheckedResult.transaction!).markdown,
        '- [ ] item',
      );
    });

    test('sets task checkbox state from an explicit task item range', () {
      final state = SovereignEditorState.fromMarkdown(
        '- [ ] item',
        selection: const SovereignSelection.collapsed(8),
      );

      final checkedResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setTaskListChecked,
        payload: const SovereignSetTaskListCheckedPayload(
          taskItemRange: SovereignSourceRange(0, 10),
          checked: true,
        ),
      );
      final checked = state.applyTransaction(checkedResult.transaction!);
      expect(checked.markdown, '- [x] item');

      final uncheckedResult = registry().dispatch(
        state: checked,
        command: SovereignMarkdownBlockCommands.setTaskListChecked,
        payload: const SovereignSetTaskListCheckedPayload(
          taskItemRange: SovereignSourceRange(0, 10),
          checked: false,
        ),
      );
      final unchecked = checked.applyTransaction(uncheckedResult.transaction!);
      expect(unchecked.markdown, '- [ ] item');
    });

    test('inserts thematic break without creating setext heading text', () {
      final state = SovereignEditorState.fromMarkdown(
        'Title',
        selection: const SovereignSelection.collapsed(5),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.insertThematicBreak,
        payload: const SovereignInsertThematicBreakPayload(),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'Title\n\n---\n');
      expect(next.selection, const SovereignSelection.collapsed(11));
    });

    test('inserts empty fenced code block at the cursor', () {
      final state = SovereignEditorState.fromMarkdown(
        'before\n',
        selection: const SovereignSelection.collapsed(7),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.insertFence,
        payload: const SovereignInsertFencePayload(language: 'dart'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'before\n```dart\n\n```');
      expect(next.selection, const SovereignSelection.collapsed(15));
    });

    test('separates inserted fenced code block from inline paragraph text', () {
      final state = SovereignEditorState.fromMarkdown(
        'before',
        selection: const SovereignSelection.collapsed(6),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.insertFence,
        payload: const SovereignInsertFencePayload(language: 'dart'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'before\n\n```dart\n\n```');
      expect(next.selection, const SovereignSelection.collapsed(16));
    });

    test('sets and clears a fenced code language', () {
      final state = SovereignEditorState.fromMarkdown(
        '```dart\nprint(1);\n```',
        selection: const SovereignSelection.collapsed(8),
      );

      final rustResult = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setFenceLanguage,
        payload: const SovereignSetFenceLanguagePayload(
          codeBlockRange: SovereignSourceRange(0, 21),
          language: 'rust',
        ),
      );
      final rust = state.applyTransaction(rustResult.transaction!);
      expect(rust.markdown, '```rust\nprint(1);\n```');
      expect(rust.selection, const SovereignSelection.collapsed(8));

      final plainResult = registry().dispatch(
        state: rust,
        command: SovereignMarkdownBlockCommands.setFenceLanguage,
        payload: const SovereignSetFenceLanguagePayload(
          codeBlockRange: SovereignSourceRange(0, 21),
          language: '',
        ),
      );
      final plain = rust.applyTransaction(plainResult.transaction!);
      expect(plain.markdown, '```\nprint(1);\n```');
    });

    test('sets fenced code language while preserving fence marker shape', () {
      final state = SovereignEditorState.fromMarkdown(
        '  ~~~~\nbody\n~~~~',
        selection: const SovereignSelection.collapsed(7),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setFenceLanguage,
        payload: const SovereignSetFenceLanguagePayload(
          codeBlockRange: SovereignSourceRange(0, 16),
          language: 'sql',
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, '  ~~~~sql\nbody\n~~~~');
    });

    test('rejects invalid fenced code language edits', () {
      final state = SovereignEditorState.fromMarkdown('```dart\ncode\n```');

      final notFence = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setFenceLanguage,
        payload: const SovereignSetFenceLanguagePayload(
          codeBlockRange: SovereignSourceRange(1, 16),
          language: 'rust',
        ),
      );
      expect(notFence.isRejected, isTrue);

      final invalidInfo = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.setFenceLanguage,
        payload: const SovereignSetFenceLanguagePayload(
          codeBlockRange: SovereignSourceRange(0, 16),
          language: 'bad`info',
        ),
      );
      expect(invalidInfo.isRejected, isTrue);
    });

    test('wraps selected source in fenced code block', () {
      final state = SovereignEditorState.fromMarkdown(
        'print(1);',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 9),
      );

      final result = registry().dispatch(
        state: state,
        command: SovereignMarkdownBlockCommands.insertFence,
        payload: const SovereignInsertFencePayload(language: 'dart'),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, '```dart\nprint(1);\n```');
      expect(
        next.selection,
        const SovereignSelection(baseOffset: 8, extentOffset: 17),
      );
    });
  });
}
