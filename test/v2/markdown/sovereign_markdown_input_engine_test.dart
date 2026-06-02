import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/markdown/source/sovereign_markdown_editing_result.dart';
import 'package:flark/src/v2/markdown/source/sovereign_markdown_input_engine.dart';
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

    test('returns a source edit for selected replacement on Enter', () {
      final result = FlarkMarkdownInputEngine.enter(
        markdown: 'alpha',
        selection: const FlarkSelection(baseOffset: 1, extentOffset: 4),
      );

      expect(result.range, const FlarkSourceRange(1, 4));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const FlarkSelection.collapsed(2));
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
