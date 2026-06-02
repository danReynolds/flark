import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  group('SovereignMarkdownCommandQueries', () {
    test('reports active inline and heading state at the caret', () {
      final state = SovereignEditorState.fromMarkdown(
        '## **alpha**',
        selection: const SovereignSelection.collapsed(6),
      );

      final capabilities =
          SovereignMarkdownCommandQueries.capabilitiesAtSelection(state);

      expect(capabilities.activeHeadingLevel, 2);
      expect(
        capabilities.isInlineStyleActive(SovereignMarkdownInlineStyle.strong),
        isTrue,
      );
      expect(
        capabilities.isInlineStyleActive(SovereignMarkdownInlineStyle.emphasis),
        isFalse,
      );
      expect(capabilities.quoteActive, isFalse);
    });

    test('reports selected text surrounded by inline markers as active', () {
      final state = SovereignEditorState.fromMarkdown(
        '**alpha**',
        selection: const SovereignSelection(baseOffset: 2, extentOffset: 7),
      );

      final capabilities =
          SovereignMarkdownCommandQueries.capabilitiesAtSelection(state);

      expect(
        capabilities.isInlineStyleActive(SovereignMarkdownInlineStyle.strong),
        isTrue,
      );
    });

    test('does not report escaped inline markers as active', () {
      final state = SovereignEditorState.fromMarkdown(
        r'\**alpha**',
        selection: const SovereignSelection.collapsed(4),
      );

      final capabilities =
          SovereignMarkdownCommandQueries.capabilitiesAtSelection(state);

      expect(capabilities.activeInlineStyles, isEmpty);
    });

    test('reports quote, bullet, ordered, and task-list state', () {
      final state = SovereignEditorState.fromMarkdown(
        '> - [ ] item',
        selection: const SovereignSelection.collapsed(7),
      );

      final capabilities =
          SovereignMarkdownCommandQueries.capabilitiesAtSelection(state);

      expect(capabilities.quoteActive, isTrue);
      expect(capabilities.bulletListActive, isTrue);
      expect(capabilities.taskListActive, isTrue);

      final ordered = SovereignEditorState.fromMarkdown(
        '> 1. item',
        selection: const SovereignSelection.collapsed(5),
      );
      final orderedCapabilities =
          SovereignMarkdownCommandQueries.capabilitiesAtSelection(ordered);
      expect(orderedCapabilities.quoteActive, isTrue);
      expect(orderedCapabilities.orderedListActive, isTrue);
      expect(orderedCapabilities.bulletListActive, isFalse);
    });

    test('reports table context around separator rows', () {
      const markdown = '| A | B |\n| - | - |\n| x | y |';
      final state = SovereignEditorState.fromMarkdown(
        markdown,
        selection: SovereignSelection.collapsed(markdown.indexOf('x')),
      );

      final capabilities =
          SovereignMarkdownCommandQueries.capabilitiesAtSelection(state);

      expect(capabilities.tableActive, isTrue);
    });
  });
}
