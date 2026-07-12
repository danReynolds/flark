import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_render_sequence.dart';

/// Widget-tier rendering sequences for **nested-block editing** — a block that
/// contains another block (a list inside a list, a list inside a quote, a
/// fence inside a list, a quote inside a quote). Nesting multiplies the
/// block-boundary interactions that produced the recent quote-exit bug: every
/// Enter now has to decide which of *two* enclosing blocks it continues or
/// exits, and the projected-editable model renders a parent's editable as a
/// superset of its children's, so a single keystroke can re-lay-out several
/// rows at once.
///
/// Everything here is pinned from the live editor's actual output (the harness
/// re-runs the source round-trip gate after every op, so a passing assertion
/// also proves display fidelity). Two authoring notes:
///
///  * The immutable claim is **"one Enter never adds more than one row"** and
///    "no round-trip leak" — not "one Enter always adds a row". As the flat
///    exemplar establishes, a block *exit* absorbs its terminal separator and
///    so keeps the row count flat; that is correct, not a regression.
///  * `LiveBlockKeys.listMarker` is a *sibling glyph* of the list item's
///    editable (a `SizedBox` in the same `Row`), not an ancestor, so
///    `expectRowInBlock(_, listMarker)` is always false. List membership is
///    therefore asserted through the source + row structure; only blockquote,
///    codeFence and table are true wrapping ancestors.
void main() {
  group('list item containing a nested list', () {
    testWidgets('Enter continues the inner list, then outdents it a level', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(
        tester,
        '- outer\n  - inner',
        caret: 17, // end of "inner"
      );
      // The outer item's editable is a superset that spans its nested child;
      // the inner item gets its own editable. Neither row is a quote/fence.
      seq.expectRows(['outer\n  inner', '  inner']);
      seq.expectFocusedRow(1);
      seq.expectRowNotInBlock(0, LiveBlockKeys.blockquote);
      seq.expectRowNotInBlock(1, LiveBlockKeys.blockquote);

      await seq.enter(); // continue the inner list
      seq.expectSource('- outer\n  - inner\n  - ');
      // One Enter -> exactly one new (empty, still-indented) inner row.
      seq.expectRows(['outer\n  inner\n  ', '  inner', '  ']);
      seq.expectFocusedRow(2);

      await seq.enter(); // outdent the empty inner item to the outer level
      // The empty nested item outdents one indentation level in place — the
      // marker survives, re-emitted at the parent's column, with no stray
      // blank-indent line. It is now a sibling of "outer", so the outer
      // superset editable no longer spans it and the cursor row is the new
      // top-level empty item.
      seq.expectSource('- outer\n  - inner\n- ');
      seq.expectRows(['outer\n  inner', '  inner', '']);
      seq.expectFocusedRow(2);

      await seq.enter(); // now a top-level empty item: Enter exits the list
      // The outermost empty item exits as usual; the terminal separator is
      // absorbed, so the row count stays flat and focus stays on the trailing
      // (now plain-paragraph) row.
      seq.expectSource('- outer\n  - inner\n\n');
      seq.expectRows(['outer\n  inner', '  inner', '']);
      seq.expectFocusedRow(2);
    });

    // Regression guard for the old double-row jump: the nested empty item used
    // to leave a stray "  " indent line, and the next Enter revealed that line
    // AND appended a blank, moving the count 3 -> 5. With the outdent fix each
    // Enter changes the row count by at most one (continue +1, outdent +0,
    // exit +0).
    testWidgets('each Enter past the nested continuation changes the row count '
        'by at most one (outdent a level, then exit)', (tester) async {
      final seq = await LiveRenderSequence.start(
        tester,
        '- outer\n  - inner',
        caret: 17,
      );
      expect(seq.rows.length, 2);

      await seq.enter(); // continue inner -> one new empty nested row
      seq.expectSource('- outer\n  - inner\n  - ');
      expect(seq.rows.length, 3);

      await seq.enter(); // outdent the empty item one level, in place
      seq.expectSource('- outer\n  - inner\n- ');
      expect(seq.rows.length, 3, reason: 'outdent re-levels a row in place');

      await seq.enter(); // top-level empty item exits the list
      seq.expectSource('- outer\n  - inner\n\n');
      expect(seq.rows.length, 3, reason: 'exit absorbs the terminal separator');
    });
  });

  group('blockquote containing a list', () {
    testWidgets(
      'continue the quoted list, exit it into the quote, exit quote',
      (tester) async {
        final seq = await LiveRenderSequence.start(
          tester,
          '> - item',
          caret: 8,
        );
        // Pinned actual: the quote's own editable (its projected text is the
        // bare list body "item") and the nested list item's editable both
        // render. Only the quote container carries the blockquote wrapper.
        seq.expectRows(['item', 'item']);
        seq.expectRowInBlock(0, LiveBlockKeys.blockquote);
        seq.expectRowNotInBlock(1, LiveBlockKeys.blockquote);

        await seq.enter(); // continue the quoted list
        seq.expectSource('> - item\n> - ');
        seq.expectRows(['item\n', 'item', '']);
        seq.expectFocusedRow(2);

        await seq.enter(); // exit the list but stay in the quote
        // The list marker is dropped to a bare quote line `> `; the now-empty
        // list-item editable collapses back into the quote container, so the
        // count drops to two and focus returns to the quote row.
        seq.expectSource('> - item\n> \n> ');
        seq.expectRows(['item\n\n', 'item']);
        seq.expectFocusedRow(0);
        seq.expectRowInBlock(0, LiveBlockKeys.blockquote);

        await seq.enter(); // exit the quote
        seq.expectSource('> - item\n> \n\n');
        seq.expectRows(['item', 'item', '']);
        seq.expectFocusedRow(2);
        // The cursor now sits in a plain paragraph outside the quote.
        seq.expectRowNotInBlock(2, LiveBlockKeys.blockquote);
      },
    );
  });

  group('list item containing a fenced code block', () {
    testWidgets('Enter inside the fenced body adds a code line, not an item', (
      tester,
    ) async {
      // A fenced code block indented under a bullet item.
      final seq = await LiveRenderSequence.start(
        tester,
        '- ```dart\n  code\n  ```',
        caret: 16, // end of the "code" body line
      );
      // Pinned actual: the list item's editable shows the fence as raw text
      // while a dedicated (empty) editable carries the codeFence wrapper.
      seq.expectRows(['```dart\n  code\n  ', '']);
      seq.expectRowInBlock(1, LiveBlockKeys.codeFence);
      seq.expectRowNotInBlock(0, LiveBlockKeys.codeFence);

      await seq.enter();
      // The Enter lands a newline *inside the fenced body* — no list marker is
      // introduced (`- ` / `  - ` never appears), the fence still stands, and
      // no extra row is spawned.
      seq.expectSource('- ```dart\n  code\n\n  ```');
      expect(seq.source.contains('\n- '), isFalse);
      expect(seq.source.contains('\n  - '), isFalse);
      seq.expectRows(['```dart\n  code\n\n  ', '']);
      seq.expectRowInBlock(1, LiveBlockKeys.codeFence);

      await seq.type('more');
      // Typed text flows into the code body as a plain code line.
      seq.expectSource('- ```dart\n  code\nmore\n  ```');
      seq.expectRows(['```dart\n  code\nmore\n  ', '']);
    });
  });

  group('nested blockquote', () {
    testWidgets('continue both levels, then exit them together', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '> > deep', caret: 8);
      // Pinned actual: `> > deep` renders as a *single* blockquote whose text
      // keeps the inner `>` literal ("> deep") rather than two nested rails.
      seq.expectRows(['> deep']);
      seq.expectFocusedRow(0);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);

      await seq.enter(); // continue — both levels stay in one editable
      seq.expectSource('> > deep\n> > ');
      seq.expectRows(['> deep\n> ']);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);

      await seq.enter(); // exit — a single Enter drops both quote levels
      seq.expectSource('> > deep\n\n');
      // One new cursor row lands outside the quote; the terminal separator is
      // absorbed, so this is +1 row, not two.
      seq.expectRows(['> deep', '']);
      seq.expectFocusedRow(1);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);
      seq.expectRowNotInBlock(1, LiveBlockKeys.blockquote);

      await seq.enter(); // one more plain line below the quote
      seq.expectSource('> > deep\n\n\n');
      seq.expectRows(['> deep', '', '']);
    });
  });

  group('inline styling inside a nested block', () {
    testWidgets('toggling strong over a selection in a nested list item', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(
        tester,
        '- outer\n  - inner',
        caret: 17,
      );
      seq.expectRows(['outer\n  inner', '  inner']);

      await seq.select(12, 17); // the word "inner" in the nested item
      await seq.toggleStyle(FlarkMarkdownInlineStyle.strong);

      // The source gains valid `**inner**` markers inside the nested item...
      seq.expectSource('- outer\n  - **inner**');
      // ...but the rendered rows still show the bare word (markers hidden, not
      // leaked as literal text), and the row structure is unchanged. The
      // harness's round-trip gate ran inside toggleStyle, so a fresh parse of
      // this source projects the same display.
      seq.expectRows(['outer\n  inner', '  inner']);
      seq.expectFocusedRow(1);
    });
  });
}
