import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/markdown/source/flark_markdown_editing_result.dart';
import 'package:flark/src/v2/markdown/source/flark_markdown_input_engine.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('FlarkMarkdownInputEngine', () {
    test('returns a source edit for normal Enter handling', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '- item',
        selection: const FlarkSelection.collapsed(6),
      );

      expect(result.range, const FlarkSourceRange(6, 6));
      expect(result.replacementText, '\n- ');
      expect(result.selectionAfter, const FlarkSelection.collapsed(9));
    });

    test('handles Enter at document start', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: 'alpha',
        selection: const FlarkSelection.collapsed(0),
      );

      expect(result.range, const FlarkSourceRange(0, 0));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(1));
    });

    test('indents a bullet list item by the marker width', () {
      final result = FlarkMarkdownInputEngine.indent(
        markdown: '- item',
        selection: const FlarkSelection.collapsed(4),
      );

      expect(result, isNotNull);
      expect(result!.range, const FlarkSourceRange(0, 6));
      expect(result.replacementText, '  - item');
      expect(result.selectionAfter, const FlarkSelection.collapsed(6));
    });

    test('indents an ordered list item by the wider marker', () {
      final result = FlarkMarkdownInputEngine.indent(
        markdown: '1. item',
        selection: const FlarkSelection.collapsed(5),
      );

      expect(result, isNotNull);
      expect(result!.range, const FlarkSourceRange(0, 7));
      expect(result.replacementText, '   1. item');
      expect(result.selectionAfter, const FlarkSelection.collapsed(8));
    });

    test('indents a task list item by the bullet width', () {
      final result = FlarkMarkdownInputEngine.indent(
        markdown: '- [ ] task',
        selection: const FlarkSelection.collapsed(8),
      );

      expect(result, isNotNull);
      expect(result!.replacementText, '  - [ ] task');
    });

    test('outdents an indented list item by one level', () {
      final result = FlarkMarkdownInputEngine.outdent(
        markdown: '  - item',
        selection: const FlarkSelection.collapsed(6),
      );

      expect(result, isNotNull);
      expect(result!.range, const FlarkSourceRange(0, 8));
      expect(result.replacementText, '- item');
      expect(result.selectionAfter, const FlarkSelection.collapsed(4));
    });

    test('does not indent outside a list item', () {
      expect(
        FlarkMarkdownInputEngine.indent(
          markdown: 'plain text',
          selection: const FlarkSelection.collapsed(3),
        ),
        isNull,
      );
    });

    test('does not outdent a top-level list item', () {
      expect(
        FlarkMarkdownInputEngine.outdent(
          markdown: '- item',
          selection: const FlarkSelection.collapsed(4),
        ),
        isNull,
      );
    });

    test('indents every list line a multi-line selection spans', () {
      final result = FlarkMarkdownInputEngine.indent(
        markdown: '- a\n- b',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 6),
      );

      expect(result, isNotNull);
      expect(result!.range, const FlarkSourceRange(0, 7));
      expect(result.replacementText, '  - a\n  - b');
      // Selection still spans 'a' (now at 4) through 'b' (now at 10).
      expect(
        result.selectionAfter,
        const FlarkSelection(baseOffset: 4, extentOffset: 10),
      );
    });

    test('outdents every list line a multi-line selection spans', () {
      final result = FlarkMarkdownInputEngine.outdent(
        markdown: '  - a\n  - b',
        selection: const FlarkSelection(baseOffset: 4, extentOffset: 10),
      );

      expect(result, isNotNull);
      expect(result!.replacementText, '- a\n- b');
      expect(
        result.selectionAfter,
        const FlarkSelection(baseOffset: 2, extentOffset: 6),
      );
    });

    test('indents only the list lines in a mixed multi-line selection', () {
      final result = FlarkMarkdownInputEngine.indent(
        markdown: '- a\nplain\n- b',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 12),
      );

      expect(result, isNotNull);
      expect(result!.replacementText, '  - a\nplain\n  - b');
    });

    test('ignores a selection ending at the next line start', () {
      // Selecting "- a\n" should not indent the second line.
      final result = FlarkMarkdownInputEngine.indent(
        markdown: '- a\n- b',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 4),
      );

      expect(result, isNotNull);
      expect(result!.range, const FlarkSourceRange(0, 3));
      expect(result.replacementText, '  - a');
    });

    test('backspace after fence-looking lines inside an open fence stays a '
        'plain edit', () {
      // The '~~~' pair lives inside an unclosed ``` fence, so it is body
      // text, not a nested closed fence (CommonMark/Comrak nesting). The
      // old per-line fence probe treated it as a closed fence and moved
      // the caret instead of deleting.
      const markdown = '```\n~~~\n~~~\n';
      final result = FlarkMarkdownInputEngine.backspace(
        markdown: markdown,
        selection: const FlarkSelection.collapsed(12),
      );

      expect(result, isA<FlarkMarkdownSourceEdit>());
    });

    test('Enter at the start of a non-empty quote line keeps its text', () {
      // Caret right after '> ' with text following: Enter must continue the
      // quote at the caret. Judging emptiness from the before-caret content
      // alone deleted the whole line, silently dropping 'hello'.
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '> hello',
        selection: const FlarkSelection.collapsed(2),
      );

      expect(result.range, const FlarkSourceRange(2, 2));
      expect(result.replacementText, '\n> ');
    });

    test('Enter on a marker-only quote line still exits the quote', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '> ',
        selection: const FlarkSelection.collapsed(2),
      );

      expect(result.range, const FlarkSourceRange(0, 2));
      expect(result.replacementText, '\n');
    });

    test('returns a source edit for selected replacement on Enter', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: 'alpha',
        selection: const FlarkSelection(baseOffset: 1, extentOffset: 4),
      );

      expect(result.range, const FlarkSourceRange(1, 4));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(2));
    });

    test('opens an empty fenced code block from a terminal fence opener', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '```',
        selection: const FlarkSelection.collapsed(3),
      );

      expect(result.range, const FlarkSourceRange(3, 3));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(4));
    });

    test('auto-closes a fence opener before following content', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: 'intro\n```\n- item',
        selection: const FlarkSelection.collapsed(9),
      );

      expect(result.range, const FlarkSourceRange(9, 10));
      expect(result.replacementText, '\n```\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(10));
    });

    test(
      'auto-closes an info-string fence opener before following content',
      () {
        final result = FlarkMarkdownInputEngine.enter(
          markdown: 'intro\n```dart\n- item',
          selection: const FlarkSelection.collapsed(13),
        );

        expect(result.range, const FlarkSourceRange(13, 14));
        expect(result.replacementText, '\n```\n');
        expect(result.selectionAfter, const FlarkSelection.collapsed(14));
      },
    );

    test('keeps an info-string fence opener user-authored on Enter', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '```dart',
        selection: const FlarkSelection.collapsed(7),
      );

      expect(result.range, const FlarkSourceRange(7, 7));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(8));
    });

    test('does not auto-open from a manually typed closing fence line', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '```dart\nfoo\n```',
        selection: const FlarkSelection.collapsed(15),
      );

      expect(result.range, const FlarkSourceRange(15, 15));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(16));
    });

    test('returns null when Backspace is not handled at document start', () {
      final result = FlarkMarkdownInputEngine.backspace(
        markdown: 'alpha',
        selection: const FlarkSelection.collapsed(0),
      );

      expect(result, isNull);
    });

    test('returns a source edit for structural Backspace handling', () {
      final result = FlarkMarkdownInputEngine.backspace(
        markdown: '- item',
        selection: const FlarkSelection.collapsed(2),
      );

      final edit = result as FlarkMarkdownSourceEdit;
      expect(edit.range, const FlarkSourceRange(0, 2));
      expect(edit.replacementText, isEmpty);
      expect(edit.selectionAfter, const FlarkSelection.collapsed(0));
    });

    test('continues checked task list items as unchecked task items', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: '- [x] done',
        selection: const FlarkSelection.collapsed(10),
      );

      expect(result.range, const FlarkSourceRange(10, 10));
      expect(result.replacementText, '\n- [ ] ');
      expect(result.selectionAfter, const FlarkSelection.collapsed(17));
    });

    test(
      'Backspace at empty task item text start removes task marker first',
      () {
        final result = FlarkMarkdownInputEngine.backspace(
          markdown: '- [ ] ',
          selection: const FlarkSelection.collapsed(6),
        );

        final edit = result as FlarkMarkdownSourceEdit;
        expect(edit.range, const FlarkSourceRange(2, 6));
        expect(edit.replacementText, isEmpty);
        expect(edit.selectionAfter, const FlarkSelection.collapsed(2));
      },
    );

    test('removes a newly opened empty fenced code block as a source edit', () {
      final result = FlarkMarkdownInputEngine.backspace(
        markdown: '```\n\n```',
        selection: const FlarkSelection.collapsed(4),
      );

      final edit = result as FlarkMarkdownSourceEdit;
      expect(edit.range, const FlarkSourceRange(0, 8));
      expect(edit.replacementText, isEmpty);
      expect(edit.selectionAfter, const FlarkSelection.collapsed(0));
    });

    test(
      'moves selection at a closed fence boundary without a sentinel edit',
      () {
        final result = FlarkMarkdownInputEngine.backspace(
          markdown: '```dart\nfoo\n```',
          selection: const FlarkSelection.collapsed(15),
        );

        final move = result as FlarkMarkdownSelectionMove;
        expect(move.selectionAfter, const FlarkSelection.collapsed(11));
      },
    );
  });
}
