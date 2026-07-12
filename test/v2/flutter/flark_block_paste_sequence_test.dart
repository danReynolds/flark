import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/inline_sequence_harness.dart';
import 'support/live_render_sequence.dart';

/// Paste **into block contexts** — the gap the isolated smart-paste
/// (`flark_smart_paste_test`) and HTML-paste (`flark_html_paste_test`) suites
/// leave open: dropping multi-block Markdown into the *middle* of already
/// structured blocks (lists, code fences, tables) and asserting both the source
/// it yields and the rendered block structure.
///
/// Tier split follows the README: source correctness rides the headless
/// [InlineSequence]; rendered block/row structure rides the widget-pumped
/// [LiveRenderSequence]. Both re-run the export round-trip gate after every
/// step, so a mid-sequence break is caught where it happens.
///
/// Rows are pinned from reality (guess, run, paste actual). The two immutable
/// semantic claims exercised here are (1) every paste keeps the source a valid
/// round-trip and (2) Markdown markers pasted into a code fence stay literal.
/// A pipe pasted into a focused table cell is escaped to `\|` — identical to
/// typing it — because the paste routes through the cell's real editable (see
/// the pipe test's comment).
void main() {
  group('paste multi-block markdown into an empty doc', () {
    test('source: heading + list round-trips', () async {
      final seq = await InlineSequence.start('');
      await seq.paste('# Heading\n\n- a\n- b');
      seq.expectSource('# Heading\n\n- a\n- b');
      expect(seq.display, 'Heading\n\na\nb');
      seq.dispose();
    });

    testWidgets('rendered blocks: heading row above two bullet rows', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '', caret: 0);
      await seq.paste('# Heading\n\n- a\n- b');
      seq.expectSource('# Heading\n\n- a\n- b');
      // Heading, the structural inter-block blank (the `\n\n` separator renders
      // as its own row, as any inter-block blank does), then the two items.
      seq.expectRows(['Heading', '', 'a', 'b']);
      // Two bullet glyphs prove `- a\n- b` became a real list, not plain text.
      expect(find.byKey(LiveBlockKeys.listMarker), findsNWidgets(2));
    });
  });

  group('paste into the middle of a list item', () {
    test(
      'source: the item splits, the remainder becomes a paragraph',
      () async {
        final seq = await InlineSequence.start('- item');
        await seq.moveCaret(2); // display 'item', caret after 'it'
        await seq.paste('X\n\nY');
        // Not inlined: 'itX' stays in the list item, the blank line ends the
        // list, and 'Y' + the pushed-down 'em' become a following paragraph.
        seq.expectSource('- itX\n\nYem');
        seq.dispose();
      },
    );

    testWidgets('rendered blocks: one bullet item then a plain paragraph', (
      tester,
    ) async {
      // Source caret 4 = after '- it'.
      final seq = await LiveRenderSequence.start(tester, '- item', caret: 4);
      await seq.paste('X\n\nY');
      seq.expectSource('- itX\n\nYem');
      seq.expectRows(['itX', '', 'Yem']);
      // Exactly one bullet: 'Yem' is a paragraph, not a second list item.
      expect(find.byKey(LiveBlockKeys.listMarker), findsOneWidget);
    });
  });

  group('paste markdown into a fenced code block body', () {
    test('source: pasted markers stay literal inside the fence', () async {
      final seq = await InlineSequence.start('```\ncode\n```');
      await seq.moveCaret(seq.display.indexOf('code') + 2); // after 'co'
      await seq.paste('**not bold**');
      seq.expectSource('```\nco**not bold**de\n```');
      // Immutable: markers inside a code fence never style — they remain
      // visible, literal text in the display.
      expect(seq.display, 'co**not bold**de');
      seq.dispose();
    });

    testWidgets('rendered blocks: literal markers inside the code fence', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '```\ncode\n```');
      await seq.moveCaret(seq.source.indexOf('code') + 2); // after 'co'
      await seq.paste('**not bold**');
      seq.expectSource('```\nco**not bold**de\n```');
      seq.expectRows(['co**not bold**de']);
      seq.expectRowInBlock(0, LiveBlockKeys.codeFence);
    });
  });

  group('paste into a table cell', () {
    const table = '| Area | Status |\n| --- | --- |\n| Preview | Guarded |';

    testWidgets('plain text lands in the cell and keeps the table intact', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);
      await seq.moveCaret(seq.source.indexOf('Preview') + 'Preview'.length);
      await seq.paste('x');
      seq.expectSource(
        '| Area | Status |\n| --- | --- |\n| Previewx | Guarded |',
      );
      seq.expectRows(['Area', 'Status', 'Previewx', 'Guarded']);
      expect(find.byKey(LiveBlockKeys.table), findsOneWidget);
    });

    testWidgets('a pasted pipe is escaped in the cell, like typing', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);
      // Tap the body cell to focus it — the real precursor to a paste — with
      // the caret at its end, then paste through the cell's own editable.
      await _focusCellAtEnd(tester, 'Guarded');
      await seq.paste('a | b');
      // Verified reality (the earlier "inserted raw" finding was a harness
      // artifact of the document-level paste path): a real paste into the
      // focused cell runs the same cell input path as typing, escaping the pipe
      // to `\|`. The body row stays one intact cell — no raw pipe, no dropped
      // ' b', no third column — and the round-trip gate holds.
      seq.expectSource(
        '| Area | Status |\n| --- | --- |\n| Preview | Guardeda \\| b |',
      );
      seq.expectRows(['Area', 'Status', 'Preview', 'Guardeda | b']);
      expect(find.byKey(LiveBlockKeys.table), findsOneWidget);
    });
  });

  group('smart-link paste over a selection in a block', () {
    // The controller recognizes a smart-link paste only when the edit REPLACES
    // a non-empty selection (`_selectionReplacement` returns null for a
    // collapsed caret). The harness models paste-over-selection with
    // `replaceSelection`; its `paste` inserts at the extent without removing the
    // selection, which would not trigger the recognizer.
    test('wraps the selected word as a link', () async {
      final seq = await InlineSequence.start('the word here');
      await seq.select(4, 8); // 'word'
      await seq.replaceSelection('https://example.com', sourceSemantics: true);
      seq.expectSource('the [word](https://example.com) here');
      expect(seq.display, 'the word here');
      seq.dispose();
    });
  });

  group('paste a trailing space while a strong style is armed', () {
    test(
      'the pasted text joins the run, trailing space stays outside',
      () async {
        final seq = await InlineSequence.start('');
        await seq.toggle(FlarkMarkdownInlineStyle.strong);
        await seq.type('a');
        seq.expectSource('**a**');
        await seq.paste(' tail ');
        // The armed run absorbs the pasted text; the trailing space is placed
        // outside the closing markers (never the invalid `**a tail **`).
        seq.expectSource('**a tail** ');
        expect(seq.display, 'a tail ');
        seq.dispose();
      },
    );
  });
}

/// Taps the table cell whose editable currently reads [cellText] to focus it,
/// then places the caret at the end of its text — the deterministic starting
/// point for a paste into that cell.
Future<void> _focusCellAtEnd(WidgetTester tester, String cellText) async {
  final cell = find.byWidgetPredicate(
    (widget) => widget is EditableText && widget.controller.text == cellText,
  );
  await tester.tap(cell);
  await tester.pump();
  final state = tester.state<EditableTextState>(cell);
  state.userUpdateTextEditingValue(
    state.textEditingValue.copyWith(
      selection: TextSelection.collapsed(offset: cellText.length),
    ),
    SelectionChangedCause.keyboard,
  );
  await tester.pump();
}
