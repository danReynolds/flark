import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/markdown/source/sovereign_markdown_fenced_code_policy.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('FlarkMarkdownInputCommands', () {
    test('inserts a plain newline outside markdown continuations', () {
      final next = _enter('alpha', 5);

      expect(next.markdown, 'alpha\n');
      expect(next.selection, const FlarkSelection.collapsed(6));
    });

    test('continues blockquotes and exits empty quote lines', () {
      final quoted = _enter('> alpha', 7);
      expect(quoted.markdown, '> alpha\n> ');
      expect(quoted.selection, const FlarkSelection.collapsed(10));

      final exited = _enter(quoted.markdown, quoted.selection.extentOffset);
      expect(exited.markdown, '> alpha\n\n');
      expect(exited.selection, const FlarkSelection.collapsed(9));
    });

    test('continues nested blockquotes and quoted list items', () {
      final nestedQuote = _enter('> > alpha', 9);
      final quotedList = _enter('> - item', 8);
      final quotedTask = _enter('> - [x] done', 12);

      expect(nestedQuote.markdown, '> > alpha\n> > ');
      expect(nestedQuote.selection, const FlarkSelection.collapsed(14));
      expect(quotedList.markdown, '> - item\n> - ');
      expect(quotedList.selection, const FlarkSelection.collapsed(13));
      expect(quotedTask.markdown, '> - [x] done\n> - [ ] ');
      expect(quotedTask.selection, const FlarkSelection.collapsed(21));
    });

    test('continues unordered, ordered, and task list markers', () {
      expect(_enter('- item', 6).markdown, '- item\n- ');
      expect(_enter('* item', 6).markdown, '* item\n* ');
      expect(_enter('2. two', 6).markdown, '2. two\n3. ');
      expect(_enter('- [x] done', 10).markdown, '- [x] done\n- [ ] ');
      expect(_enter('1. [x] done', 11).markdown, '1. [x] done\n2. [ ] ');
    });

    test('preserves nested list indentation', () {
      final unordered = _enter('  - item', 8);
      final ordered = _enter('    2. item', 11);

      expect(unordered.markdown, '  - item\n  - ');
      expect(ordered.markdown, '    2. item\n    3. ');
    });

    test('preserves list marker padding on Enter', () {
      final unordered = _enter('-   item', 8);
      final ordered = _enter('9)   item', 9);

      expect(unordered.markdown, '-   item\n-   ');
      expect(ordered.markdown, '9)   item\n10)   ');
    });

    test('exits empty unquoted list items', () {
      final next = _enter('  - ', 4);

      expect(next.markdown, '  \n');
      expect(next.selection, const FlarkSelection.collapsed(3));
    });

    test('exits empty quoted list items but keeps quote mode', () {
      final unordered = _enter('> - ', 4);
      final ordered = _enter('> 1. ', 5);
      final task = _enter('> - [ ] ', 8);
      final nested = _enter('>   - ', 6);

      expect(unordered.markdown, '> \n> ');
      expect(unordered.selection, const FlarkSelection.collapsed(5));
      expect(ordered.markdown, '> \n> ');
      expect(ordered.selection, const FlarkSelection.collapsed(5));
      expect(task.markdown, '> \n> ');
      expect(task.selection, const FlarkSelection.collapsed(5));
      expect(nested.markdown, '>   \n>   ');
      expect(nested.selection, const FlarkSelection.collapsed(9));
    });

    test('replaces selected source with a newline', () {
      final runtime = _runtime(
        'alpha',
        const FlarkSelection(baseOffset: 1, extentOffset: 4),
      );

      final result = runtime.dispatch(
        command: FlarkMarkdownInputCommands.handleEnter,
        payload: const FlarkHandleEnterPayload(),
      );

      expect(result.runtime.state.markdown, 'a\na');
      expect(result.runtime.state.selection, const FlarkSelection.collapsed(2));
    });

    test('exits empty ATX headings on Enter', () {
      final heading = _enter('# ', 2);
      final indented = _enter('  ## ', 5);

      expect(heading.markdown, '\n');
      expect(heading.selection, const FlarkSelection.collapsed(1));
      expect(indented.markdown, '  \n');
      expect(indented.selection, const FlarkSelection.collapsed(3));
    });

    test('continues and exits indented code blocks on Enter', () {
      final continued = _enter('    final x = 1;', 16);
      expect(continued.markdown, '    final x = 1;\n    ');
      expect(continued.selection, const FlarkSelection.collapsed(21));

      final exited = _enter(
        continued.markdown,
        continued.selection.extentOffset,
      );
      expect(exited.markdown, '    final x = 1;\n\n');
      expect(exited.selection, const FlarkSelection.collapsed(18));
    });

    test('preserves fenced code indentation on Enter', () {
      final next = _enter('```\n  foo\n```', 9);

      expect(next.markdown, '```\n  foo\n  \n```');
      expect(next.selection, const FlarkSelection.collapsed(12));
    });

    test('indents fenced code after language block openers', () {
      const spaces = '```\nif (x) {\n```';
      final indentedSpaces = _enter(spaces, spaces.indexOf('{') + 1);
      expect(indentedSpaces.markdown, '```\nif (x) {\n  \n```');
      expect(indentedSpaces.selection, const FlarkSelection.collapsed(15));

      const tabs = '```\n\tif (x) {\n```';
      final indentedTabs = _enter(tabs, tabs.indexOf('{') + 1);
      expect(indentedTabs.markdown, '```\n\tif (x) {\n\t\t\n```');
      expect(indentedTabs.selection, const FlarkSelection.collapsed(16));
    });

    test('language aware colon indentation only applies where expected', () {
      const python = '```python\nif ready:\n```';
      final indentedPython = _enter(python, python.indexOf(':') + 1);
      expect(indentedPython.markdown, '```python\nif ready:\n  \n```');
      expect(indentedPython.selection, const FlarkSelection.collapsed(22));

      const dart = '```dart\nlabel:\n```';
      final plainDart = _enter(dart, dart.indexOf(':') + 1);
      expect(plainDart.markdown, '```dart\nlabel:\n\n```');
      expect(plainDart.selection, const FlarkSelection.collapsed(15));
    });

    test('exits fenced code from trailing blank lines', () {
      const closed = '```\nfoo\n\n```';
      final closedExit = _enter(closed, closed.indexOf('\n\n') + 1);
      expect(closedExit.markdown, '```\nfoo\n```\n');
      expect(closedExit.selection, const FlarkSelection.collapsed(12));

      const multipleBlanks = '```\nfoo\n\n\n```';
      final multipleBlankExit = _enter(
        multipleBlanks,
        multipleBlanks.indexOf('\n\n') + 1,
      );
      expect(multipleBlankExit.markdown, '```\nfoo\n```\n');
      expect(multipleBlankExit.selection, const FlarkSelection.collapsed(12));

      const unclosed = '```\nfoo\n\n\n';
      final unclosedExit = _enter(unclosed, unclosed.indexOf('\n\n') + 1);
      expect(unclosedExit.markdown, '```\nfoo\n```\n');
      expect(unclosedExit.selection, const FlarkSelection.collapsed(12));
    });

    test('auto outdents closer insertion on indentation-only fenced lines', () {
      const shallow = '```\nif (x) {\n  \n```';
      final shallowLineStart = shallow.indexOf('  \n');
      final shallowEdit =
          FlarkMarkdownFencedCodePolicy.autoOutdentCloserInsertion(
            markdown: shallow,
            insertionOffset: shallowLineStart + 2,
            insertedText: '}',
          );
      expect(shallowEdit, isNotNull);
      expect(shallowEdit!.range, FlarkSourceRange(shallowLineStart, 15));
      expect(shallowEdit.replacementText, '}');
      expect(
        shallowEdit.selectionAfter,
        FlarkSelection.collapsed(shallowLineStart + 1),
      );

      const nested = '```\n  if (x) {\n    \n```';
      final nestedLineStart = nested.indexOf('    \n');
      final nestedEdit =
          FlarkMarkdownFencedCodePolicy.autoOutdentCloserInsertion(
            markdown: nested,
            insertionOffset: nestedLineStart + 4,
            insertedText: '}',
          );
      expect(nestedEdit, isNotNull);
      expect(nestedEdit!.replacementText, '  }');
      expect(
        nestedEdit.selectionAfter,
        FlarkSelection.collapsed(nestedLineStart + 3),
      );
    });

    test('normalizes multiline paste indentation inside fenced code', () {
      const markdown = '```\n  \n```';
      const insertedText = 'if (x) {\nprint(1);\n}';
      const expectedReplacement = 'if (x) {\n  print(1);\n  }';
      final insertionOffset = markdown.indexOf('  \n') + 2;

      final edit = FlarkMarkdownFencedCodePolicy.multilinePasteIndentation(
        markdown: markdown,
        insertionOffset: insertionOffset,
        insertedText: insertedText,
      );

      expect(edit, isNotNull);
      expect(edit!.range, FlarkSourceRange(insertionOffset, insertionOffset));
      expect(edit.replacementText, expectedReplacement);
      expect(
        edit.selectionAfter,
        FlarkSelection.collapsed(insertionOffset + expectedReplacement.length),
      );
    });

    test(
      'normalizes trailing-newline paste to keep the next code line indented',
      () {
        const markdown = '```\n  \n```';
        final insertionOffset = markdown.indexOf('  \n') + 2;

        final edit = FlarkMarkdownFencedCodePolicy.multilinePasteIndentation(
          markdown: markdown,
          insertionOffset: insertionOffset,
          insertedText: 'line\n',
        );

        expect(edit, isNotNull);
        expect(edit!.replacementText, 'line\n  ');
      },
    );

    test('Backspace removes newly opened empty fenced code blocks', () {
      final unclosed = _backspace('```\n', 4);
      final closed = _backspace('```\n\n```', 4);
      final language = _backspace('```dart\n```', 8);
      const betweenBlocks = 'before\n```\n\n```\nafter';
      final between = _backspace(betweenBlocks, 11);

      expect(unclosed.markdown, isEmpty);
      expect(unclosed.selection, const FlarkSelection.collapsed(0));
      expect(closed.markdown, isEmpty);
      expect(closed.selection, const FlarkSelection.collapsed(0));
      expect(language.markdown, isEmpty);
      expect(language.selection, const FlarkSelection.collapsed(0));
      expect(between.markdown, 'before\nafter');
      expect(between.selection, const FlarkSelection.collapsed(7));
    });

    test('Backspace at a closed fence boundary moves into the code body', () {
      final terminal = _backspace('```dart\nfoo\n```', 15);
      final beforeParagraph = _backspace('```dart\nfoo\n```\nafter', 16);
      const nested = 'before\n```dart\nfoo\n```\nafter';
      final afterLeadingBlock = _backspace(nested, 23);

      expect(terminal.markdown, '```dart\nfoo\n```');
      expect(terminal.selection, const FlarkSelection.collapsed(11));
      expect(beforeParagraph.markdown, '```dart\nfoo\n```\nafter');
      expect(beforeParagraph.selection, const FlarkSelection.collapsed(11));
      expect(afterLeadingBlock.markdown, nested);
      expect(afterLeadingBlock.selection, const FlarkSelection.collapsed(18));
    });

    test(
      'builds fenced-code indent and outdent operations from source policy',
      () {
        const markdown = '```dart\none\n  two\n```';
        const bodyRange = FlarkSourceRange(8, 17);

        final indent = FlarkMarkdownFencedCodePolicy.indentOperations(
          markdown: markdown,
          bodyRange: bodyRange,
          selection: const FlarkSelection(baseOffset: 8, extentOffset: 17),
        );
        expect(indent, [
          FlarkSourceOperation.insert(8, '  '),
          FlarkSourceOperation.insert(12, '  '),
        ]);

        final outdent = FlarkMarkdownFencedCodePolicy.outdentOperations(
          markdown: markdown,
          bodyRange: bodyRange,
          selection: const FlarkSelection(baseOffset: 8, extentOffset: 17),
        );
        expect(outdent, [FlarkSourceOperation.delete(12, 14)]);
      },
    );

    test('Backspace at list boundaries removes markers structurally', () {
      final unordered = _backspace('- item', 2);
      final ordered = _backspace('1. item', 3);
      final nested = _backspace('  - item', 4);
      final task = _backspace('- [x] done', 6);
      final quotedUnordered = _backspace('> - item', 4);
      final quotedOrdered = _backspace('> 1. item', 5);
      final quotedTask = _backspace('> - [x] done', 8);

      expect(unordered.markdown, 'item');
      expect(unordered.selection, const FlarkSelection.collapsed(0));
      expect(ordered.markdown, 'item');
      expect(ordered.selection, const FlarkSelection.collapsed(0));
      expect(nested.markdown, '  item');
      expect(nested.selection, const FlarkSelection.collapsed(2));
      expect(task.markdown, '- done');
      expect(task.selection, const FlarkSelection.collapsed(2));
      expect(quotedUnordered.markdown, '> item');
      expect(quotedUnordered.selection, const FlarkSelection.collapsed(2));
      expect(quotedOrdered.markdown, '> item');
      expect(quotedOrdered.selection, const FlarkSelection.collapsed(2));
      expect(quotedTask.markdown, '> - done');
      expect(quotedTask.selection, const FlarkSelection.collapsed(4));
    });

    test('Backspace removes full list markers with custom padding', () {
      final unordered = _backspace('-   item', 4);
      final ordered = _backspace('1.\titem', 3);
      final task = _backspace('-   [x] done', 8);

      expect(unordered.markdown, 'item');
      expect(unordered.selection, const FlarkSelection.collapsed(0));
      expect(ordered.markdown, 'item');
      expect(ordered.selection, const FlarkSelection.collapsed(0));
      expect(task.markdown, '-   done');
      expect(task.selection, const FlarkSelection.collapsed(4));
    });

    test('Backspace at heading boundaries removes heading markers', () {
      final empty = _backspace('# ', 2);
      final text = _backspace('## Heading', 3);
      final indented = _backspace('  ### Heading', 6);

      expect(empty.markdown, isEmpty);
      expect(empty.selection, const FlarkSelection.collapsed(0));
      expect(text.markdown, 'Heading');
      expect(text.selection, const FlarkSelection.collapsed(0));
      expect(indented.markdown, '  Heading');
      expect(indented.selection, const FlarkSelection.collapsed(2));
    });

    test('Backspace on an empty quote line removes the quote region', () {
      final empty = _backspace('> ', 2);
      final continued = _backspace('> alpha\n> ', 10);
      final nested = _backspace('> > ', 4);

      expect(empty.markdown, isEmpty);
      expect(empty.selection, const FlarkSelection.collapsed(0));
      expect(continued.markdown, '> alpha\n');
      expect(continued.selection, const FlarkSelection.collapsed(8));
      expect(nested.markdown, '> ');
      expect(nested.selection, const FlarkSelection.collapsed(2));
    });

    test('Backspace at quote content start unwraps one quote level', () {
      final plain = _backspace('> quote', 2);
      final nestedSpaced = _backspace('> > quote', 4);
      final nestedCompact = _backspace('>> quote', 3);

      expect(plain.markdown, 'quote');
      expect(plain.selection, const FlarkSelection.collapsed(0));
      expect(nestedSpaced.markdown, '> quote');
      expect(nestedSpaced.selection, const FlarkSelection.collapsed(2));
      expect(nestedCompact.markdown, '> quote');
      expect(nestedCompact.selection, const FlarkSelection.collapsed(1));
    });

    test('Backspace removes indented code units', () {
      final doubleIndent = _backspace('        code', 8);
      final singleIndent = _backspace('    code', 4);
      final tabIndent = _backspace('\tcode', 1);

      expect(doubleIndent.markdown, '    code');
      expect(doubleIndent.selection, const FlarkSelection.collapsed(4));
      expect(singleIndent.markdown, 'code');
      expect(singleIndent.selection, const FlarkSelection.collapsed(0));
      expect(tabIndent.markdown, 'code');
      expect(tabIndent.selection, const FlarkSelection.collapsed(0));
    });

    test(
      'ordinary Backspace still deletes text inside markdown structures',
      () {
        final quote = _backspace('> quote', 4);
        final heading = _backspace('## Heading', 5);
        final list = _backspace('- item', 4);

        expect(quote.markdown, '> qote');
        expect(quote.selection, const FlarkSelection.collapsed(3));
        expect(heading.markdown, '## Hading');
        expect(heading.selection, const FlarkSelection.collapsed(4));
        expect(list.markdown, '- iem');
        expect(list.selection, const FlarkSelection.collapsed(3));
      },
    );
  });
}

FlarkEditorState _enter(String markdown, int caret) {
  final runtime = _runtime(markdown, FlarkSelection.collapsed(caret));
  final result = runtime.dispatch(
    command: FlarkMarkdownInputCommands.handleEnter,
    payload: const FlarkHandleEnterPayload(),
  );
  expect(result.commandResult.isHandled, isTrue);
  return result.runtime.state;
}

FlarkEditorState _backspace(String markdown, int caret) {
  final runtime = _runtime(markdown, FlarkSelection.collapsed(caret));
  final result = runtime.dispatch(
    command: FlarkMarkdownInputCommands.handleBackspace,
    payload: const FlarkHandleBackspacePayload(),
  );
  expect(result.commandResult.isHandled, isTrue);
  return result.runtime.state;
}

FlarkEditorRuntime _runtime(String markdown, FlarkSelection selection) {
  return FlarkEditorRuntime(
    state: FlarkEditorState.fromMarkdown(markdown, selection: selection),
    commandRegistry: FlarkExtensionSet([
      const FlarkMarkdownInputEditingExtension(),
    ]).commandRegistry(),
  );
}
