import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('FlarkMarkdownCommandQueries', () {
    test('reports active inline and heading state at the caret', () {
      final state = FlarkEditorState.fromMarkdown(
        '## **alpha**',
        selection: const FlarkSelection.collapsed(6),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
      );

      expect(capabilities.activeHeadingLevel, 2);
      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
        isTrue,
      );
      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.emphasis),
        isFalse,
      );
      expect(capabilities.quoteActive, isFalse);
    });

    test('unions pending inline styles at a collapsed caret', () {
      final state = FlarkEditorState.fromMarkdown(
        'plain',
        selection: const FlarkSelection.collapsed(2),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
        pendingInlineStyles: const [FlarkMarkdownInlineStyle.strong],
      );

      // The caret is over plain text, so strong is active only because it is
      // armed (pending), not because of any surrounding source markers.
      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
        isTrue,
      );
      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.emphasis),
        isFalse,
      );
    });

    test('reports a caret at a run trailing edge as inside the run', () {
      // The caret sits between the content and the hidden closing marker — the
      // position the caret lands at after typing armed/styled text. The style
      // must still read active so the toolbar stays lit while typing.
      for (final probe in <(String, int, FlarkMarkdownInlineStyle)>[
        ('**x**', 3, FlarkMarkdownInlineStyle.strong),
        ('*x*', 2, FlarkMarkdownInlineStyle.emphasis),
        ('_x_', 2, FlarkMarkdownInlineStyle.emphasis),
        ('`x`', 2, FlarkMarkdownInlineStyle.inlineCode),
        ('~~x~~', 3, FlarkMarkdownInlineStyle.strikethrough),
        ('**word**', 6, FlarkMarkdownInlineStyle.strong),
      ]) {
        final state = FlarkEditorState.fromMarkdown(
          probe.$1,
          selection: FlarkSelection.collapsed(probe.$2),
        );
        final capabilities =
            FlarkMarkdownCommandQueries.capabilitiesAtSelection(state);
        expect(
          capabilities.isInlineStyleActive(probe.$3),
          isTrue,
          reason: '${probe.$3} should be active at offset ${probe.$2} '
              'in "${probe.$1}"',
        );
      }
    });

    test('does not report a caret outside a run as inside it', () {
      // Carets before the opener, after the closer, or in the middle of a
      // marker are outside the run and must not read active.
      for (final probe in <(String, int)>[
        ('**x**', 0), // before opener
        ('**x**', 5), // after closer
        ('**x**', 4), // middle of closing marker
      ]) {
        final state = FlarkEditorState.fromMarkdown(
          probe.$1,
          selection: FlarkSelection.collapsed(probe.$2),
        );
        final capabilities =
            FlarkMarkdownCommandQueries.capabilitiesAtSelection(state);
        expect(
          capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
          isFalse,
          reason: 'strong should be inactive at offset ${probe.$2} '
              'in "${probe.$1}"',
        );
      }
    });

    test('subtracts muted inline styles at a collapsed caret', () {
      final state = FlarkEditorState.fromMarkdown(
        '**bold**',
        selection: const FlarkSelection.collapsed(4),
      );

      // Without muting, the caret inside the run reads strong active.
      expect(
        FlarkMarkdownCommandQueries.capabilitiesAtSelection(
          state,
        ).isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
        isTrue,
      );

      // Muting (toggled off in place) reads inactive even though the source
      // still carries the markers.
      expect(
        FlarkMarkdownCommandQueries.capabilitiesAtSelection(
          state,
          mutedInlineStyles: const [FlarkMarkdownInlineStyle.strong],
        ).isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
        isFalse,
      );
    });

    test('finds the enclosing inline run for a collapsed caret', () {
      final state = FlarkEditorState.fromMarkdown(
        'a **bold** b',
        selection: const FlarkSelection.collapsed(6),
      );

      final run = FlarkMarkdownCommandQueries.enclosingInlineRun(
        state,
        FlarkMarkdownInlineStyle.strong,
      );

      expect(run, isNotNull);
      expect(run!.openStart, 2);
      expect(run.contentStart, 4);
      expect(run.closeStart, 8);
      expect(run.closeEnd, 10);
    });

    test('ignores pending inline styles when a range is selected', () {
      final state = FlarkEditorState.fromMarkdown(
        'plain',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 5),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
        pendingInlineStyles: const [FlarkMarkdownInlineStyle.strong],
      );

      // Pending styles only apply to a collapsed caret; a real selection takes
      // the wrap/unwrap path, so an (impossible) stray pending set is ignored.
      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
        isFalse,
      );
    });

    test('reports selected text surrounded by inline markers as active', () {
      final state = FlarkEditorState.fromMarkdown(
        '**alpha**',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
      );

      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.strong),
        isTrue,
      );
      // Regression: the bracketing `*` of the `**` pair must not be read as a
      // single-`*` emphasis run, or bolding a selection lights italic too.
      expect(
        capabilities.isInlineStyleActive(FlarkMarkdownInlineStyle.emphasis),
        isFalse,
      );
    });

    test('does not report escaped inline markers as active', () {
      final state = FlarkEditorState.fromMarkdown(
        r'\**alpha**',
        selection: const FlarkSelection.collapsed(4),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
      );

      expect(capabilities.activeInlineStyles, isEmpty);
    });

    test('reports quote, bullet, ordered, and task-list state', () {
      final state = FlarkEditorState.fromMarkdown(
        '> - [ ] item',
        selection: const FlarkSelection.collapsed(7),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
      );

      expect(capabilities.quoteActive, isTrue);
      expect(capabilities.bulletListActive, isTrue);
      expect(capabilities.taskListActive, isTrue);

      final ordered = FlarkEditorState.fromMarkdown(
        '> 1. item',
        selection: const FlarkSelection.collapsed(5),
      );
      final orderedCapabilities =
          FlarkMarkdownCommandQueries.capabilitiesAtSelection(ordered);
      expect(orderedCapabilities.quoteActive, isTrue);
      expect(orderedCapabilities.orderedListActive, isTrue);
      expect(orderedCapabilities.bulletListActive, isFalse);
    });

    test('reports table context around separator rows', () {
      const markdown = '| A | B |\n| - | - |\n| x | y |';
      final state = FlarkEditorState.fromMarkdown(
        markdown,
        selection: FlarkSelection.collapsed(markdown.indexOf('x')),
      );

      final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
        state,
      );

      expect(capabilities.tableActive, isTrue);
    });
  });
}
