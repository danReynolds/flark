import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_render_sequence.dart';

/// Widget-tier sequences for **selections that span block boundaries** and the
/// edits made over them — the coverage gap where projection mapping
/// (display<->source across multiple blocks' hidden markers) is hardest and
/// least tested.
///
/// [LiveRenderSequence.select] takes SOURCE offsets (so a range can span
/// blocks); the offsets in each test are computed against the literal source
/// string in the comment above the call. Every op re-runs the harness's export
/// round-trip gate (a fresh, caret-free parse of `controller.markdown` must
/// project the identical display), so a cross-block edit that strands edge
/// whitespace, orphans a hidden marker pair, or otherwise leaves invalid
/// CommonMark fails at the op that caused it — see
/// docs/architecture/v2/inline_delimiter_validity_2026-07-10.md. Per the
/// authoring policy the gate is immutable: a failing cross-block edit is kept
/// `skip:`ed as a potential defect, never softened.
///
/// One structural fact drives the pinned rows below: a document with **no**
/// list / quote / code / table / image mounts a *single* whole-document host
/// editable, so [LiveRenderSequence.rows] is one entry carrying the embedded
/// `\n\n`; a document that needs per-block editing (a list/quote/…) mounts one
/// editable per block plus a blank editable per structural separator line.
///
/// Both surfaces honor edits over a cross-block selection: a keystroke or
/// Backspace replaces/deletes the whole selection, and inline markers never
/// leak. Earlier revisions pinned two apparent divergences that empirical
/// tracing (see RFC 021) showed were not real:
///   * the single-document host "inserting at the anchor" on type was a
///     harness artifact — `type` modelled a pure insertion; a real keystroke
///     (and now the harness) replaces the selection; and
///   * the per-block surface dropping a *partial* cross-block Backspace to a
///     one-character delete was a real bug, fixed by projecting a partial
///     cross-block selection as its faithful clipped sub-range in each block
///     (`_clippedLocalSelection`) rather than a block-edge caret — so a
///     partial selection now behaves like the whole-document case.
void main() {
  const strong = FlarkMarkdownInlineStyle.strong;

  group('two paragraphs, delete across the blank', () {
    // 'alpha\n\nbeta' — a l p h a \n \n b e t a
    //                   0 1 2 3 4 5   6   7 8 9 10
    // Plain paragraphs only -> one whole-document host editable, so `rows` is a
    // single entry with the embedded blank line.
    testWidgets('backspace over the cross-block selection merges them', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, 'alpha\n\nbeta');
      seq.expectRows(['alpha\n\nbeta']);

      // Select "alph|a\n\nb|eta": from inside alpha (4) into beta (8).
      await seq.select(4, 8);
      await seq.backspace();

      // The host honors the full source selection: the blank separator is
      // consumed and the two paragraphs fuse into one.
      seq.expectSource('alpheta');
      seq.expectRows(['alpheta']);
    });

    testWidgets('typing over the cross-block selection replaces it', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, 'alpha\n\nbeta');

      // Same span as the backspace variant, but typing a character. A real
      // keystroke over a selection replaces it: the blank separator is
      // consumed and the two paragraphs fuse, exactly as the backspace variant
      // does. (An earlier revision pinned an insert-at-anchor result; that was
      // the harness modelling a pure insertion, not host behavior.)
      await seq.select(4, 8);
      await seq.type('X');

      seq.expectSource('alphXeta');
      seq.expectRows(['alphXeta']);
    });
  });

  group('styled run into the next block, deleted', () {
    // '**bold**\n\nplain' — * * b o l d * * \n \n p  l  a  i  n
    //                       0 1 2 3 4 5 6 7 8  9  10 11 12 13 14
    // Inline styling still mounts the single host; the '**' pair is hidden, so
    // the visible text is "bold\n\nplain".
    testWidgets('deleting from inside the run into the next block keeps the '
        'marker pair balanced', (tester) async {
      final seq = await LiveRenderSequence.start(tester, '**bold**\n\nplain');
      seq.expectRows(['bold\n\nplain']);

      // Select from inside the bold run (4, after "bo") into the next paragraph
      // (12, after "pl"). The range covers the run's *closing* `**` but not its
      // open — a plain delete would leave `**boain`, an orphaned opening marker
      // leaking as literal text.
      await seq.select(4, 12);
      await seq.backspace();

      // The marker-crossing repair relocates the surviving open's partner to
      // hug the surviving core "bo": the source stays valid and the markers
      // stay hidden. (No leak — a positive result for the hardest case.)
      seq.expectSource('**bo**ain');
      seq.expectRows(['boain']);
    });
  });

  group('list boundary + toggleStyle', () {
    // '- one\n- two' — - _ o n e \n - _ t w  o
    //                  0 1 2 3 4 5  6 7 8 9 10
    testWidgets('strong over a selection spanning two list items is vetoed '
        'by the parser judge', (tester) async {
      final seq = await LiveRenderSequence.start(tester, '- one\n- two');
      seq.expectRows(['one', 'two']);

      // Select "one\n- two" — from the first item's content through the second
      // item's content, crossing the list marker boundary.
      await seq.select(2, 11);
      await seq.toggleStyle(strong);

      // The command still wraps the whole span with no block-boundary
      // awareness (a single newline is not a paragraph break), so it would
      // write `- **one\n- two**` — but comrak cannot match a strong run
      // across two list items, so those `**` would be literal text. The
      // RFC 022 parser judge now vetoes that authored claim and the toggle
      // no-ops instead of spraying literal markers into the document.
      // (Per-list-item wrapping — the enhancement — needs block boundaries
      // from the parse and is tracked for Phase 4.)
      seq.expectSource('- one\n- two');
      seq.expectRows(['one', 'two']);
    });
  });

  group('quote and following paragraph, deleted', () {
    // '> quoted\n\ntail' — > _ q u o t e d \n \n t  a  i  l
    //                      0 1 2 3 4 5 6 7 8  9  10 11 12 13
    testWidgets('partial cross-block backspace deletes the whole selection', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '> quoted\n\ntail');
      seq.expectRows(['quoted', '', 'tail']);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);
      seq.expectRowNotInBlock(2, LiveBlockKeys.blockquote);

      // Select "quo|ted\n\ntai|l": from inside the quote (5) into the tail
      // paragraph (13). The controller selection is the full (5,13) range, and
      // focus routes to the extent block (the paragraph).
      await seq.select(5, 13);
      await seq.backspace();

      // The per-block surface honors the full document selection: the whole
      // (5,13) range is deleted and what survives ("> quo" + "l") merges into
      // one quote line — matching the whole-document backspace in "select-all
      // over a mixed doc". (Before the `_clippedLocalSelection` fix the extent
      // block projected a collapsed-at-end caret one past the selection extent,
      // which the preservation guard declined to protect, so only "l" was
      // removed.)
      seq.expectSource('> quol');
      seq.expectRows(['quol']);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);
    });
  });

  group('whole-document select + delete', () {
    // '# Title\n\n- item\n\n> quote' (length 24) — heading, list, quote; the
    // list + quote force per-block editing, so the two structural separators
    // render as their own blank rows.
    testWidgets('select-all over a mixed doc, delete, empties it', (
      tester,
    ) async {
      const doc = '# Title\n\n- item\n\n> quote';
      final seq = await LiveRenderSequence.start(tester, doc);
      seq.expectRows(['Title', '', 'item', '', 'quote']);

      // A whole-document selection is fully contained by every block, so the
      // per-block selection-preservation holds and backspace deletes it all.
      await seq.select(0, doc.length);
      await seq.backspace();

      seq.expectSource('');
      seq.expectRows(['']);
    });
  });

  group('replace a cross-block selection with plain text', () {
    // '> aaa\n\nbbb\n\nccc' — > _ a a a \n \n b b b \n  \n  c  c  c
    //                        0 1 2 3 4 5  6  7 8 9 10 11 12 13 14
    // The leading quote forces per-block editing, where `type` over a
    // cross-block selection routes through the document-spanning echo and
    // replaces the whole range.
    testWidgets('typing collapses three blocks into one', (tester) async {
      final seq = await LiveRenderSequence.start(tester, '> aaa\n\nbbb\n\nccc');
      seq.expectRows(['aaa', '', 'bbb', '', 'ccc']);

      // Select "a|aa\n\nbbb\n\nc|cc": from inside the quote (3) into the last
      // paragraph (13), swallowing the whole middle block.
      await seq.select(3, 13);
      await seq.type('x');

      // The three blocks collapse into a single quote carrying "axcc".
      seq.expectSource('> axcc');
      seq.expectRows(['axcc']);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);
    });
  });
}
