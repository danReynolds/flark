import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/markdown/source/sovereign_markdown_editing_result.dart';
import 'package:sovereign_editor/src/v2/markdown/source/sovereign_markdown_input_engine.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  group('SovereignMarkdownInputEngine', () {
    test('returns a source edit for normal Enter handling', () {
      final result = SovereignMarkdownInputEngine.enter(
        markdown: '- item',
        selection: const SovereignSelection.collapsed(6),
      );

      expect(result.range, const SovereignSourceRange(6, 6));
      expect(result.replacementText, '\n- ');
      expect(result.selectionAfter, const SovereignSelection.collapsed(9));
    });

    test('handles Enter at document start', () {
      final result = SovereignMarkdownInputEngine.enter(
        markdown: 'alpha',
        selection: const SovereignSelection.collapsed(0),
      );

      expect(result.range, const SovereignSourceRange(0, 0));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const SovereignSelection.collapsed(1));
    });

    test('returns a source edit for selected replacement on Enter', () {
      final result = SovereignMarkdownInputEngine.enter(
        markdown: 'alpha',
        selection: const SovereignSelection(baseOffset: 1, extentOffset: 4),
      );

      expect(result.range, const SovereignSourceRange(1, 4));
      expect(result.replacementText, '\n');
      expect(result.selectionAfter, const SovereignSelection.collapsed(2));
    });

    test('returns null when Backspace is not handled at document start', () {
      final result = SovereignMarkdownInputEngine.backspace(
        markdown: 'alpha',
        selection: const SovereignSelection.collapsed(0),
      );

      expect(result, isNull);
    });

    test('returns a source edit for structural Backspace handling', () {
      final result = SovereignMarkdownInputEngine.backspace(
        markdown: '- item',
        selection: const SovereignSelection.collapsed(2),
      );

      final edit = result as SovereignMarkdownSourceEdit;
      expect(edit.range, const SovereignSourceRange(0, 2));
      expect(edit.replacementText, isEmpty);
      expect(edit.selectionAfter, const SovereignSelection.collapsed(0));
    });

    test('continues checked task list items as unchecked task items', () {
      final result = SovereignMarkdownInputEngine.enter(
        markdown: '- [x] done',
        selection: const SovereignSelection.collapsed(10),
      );

      expect(result.range, const SovereignSourceRange(10, 10));
      expect(result.replacementText, '\n- [ ] ');
      expect(result.selectionAfter, const SovereignSelection.collapsed(17));
    });

    test('Backspace at empty task item text start removes task marker first',
        () {
      final result = SovereignMarkdownInputEngine.backspace(
        markdown: '- [ ] ',
        selection: const SovereignSelection.collapsed(6),
      );

      final edit = result as SovereignMarkdownSourceEdit;
      expect(edit.range, const SovereignSourceRange(2, 6));
      expect(edit.replacementText, isEmpty);
      expect(edit.selectionAfter, const SovereignSelection.collapsed(2));
    });

    test('removes a newly opened empty fenced code block as a source edit', () {
      final result = SovereignMarkdownInputEngine.backspace(
        markdown: '```\n\n```',
        selection: const SovereignSelection.collapsed(4),
      );

      final edit = result as SovereignMarkdownSourceEdit;
      expect(edit.range, const SovereignSourceRange(0, 8));
      expect(edit.replacementText, isEmpty);
      expect(edit.selectionAfter, const SovereignSelection.collapsed(0));
    });

    test('moves selection at a closed fence boundary without a sentinel edit',
        () {
      final result = SovereignMarkdownInputEngine.backspace(
        markdown: '```dart\nfoo\n```',
        selection: const SovereignSelection.collapsed(15),
      );

      final move = result as SovereignMarkdownSelectionMove;
      expect(move.selectionAfter, const SovereignSelection.collapsed(11));
    });
  });
}
