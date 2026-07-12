import 'package:flutter_test/flutter_test.dart';

import 'support/live_render_sequence.dart';

/// Widget-tier rendering sequences for block *exit* flows — the class of bug
/// a headless source/display gate cannot see: a valid document that lays out
/// into the wrong number of visible rows or focuses the wrong one.
///
/// The exemplar is the blockquote exit: `> q` + Enter + Enter produces the
/// valid source `> q\n\n`, whose closing blank is structural (it stops the
/// next paragraph lazy-continuing back into the quote) and so is absorbed as
/// the quote's boundary. The source and projected display are correct at
/// every step; only the rendered row count reveals a regression, which is
/// exactly what [LiveRenderSequence.rows] snapshots.
void main() {
  group('blockquote exit', () {
    testWidgets('continue, exit, and keep pressing Enter add one row each', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '> quote', caret: 7);
      seq.expectRows(['quote']);

      await seq.enter(); // continue the quote
      seq.expectSource('> quote\n> ');
      // The quote's two lines stay in one multi-line editable until exit.
      seq.expectRows(['quote\n']);

      await seq.enter(); // exit the quote
      seq.expectSource('> quote\n\n');
      // The structural separator is absorbed: quote + exactly one cursor row.
      seq.expectRows(['quote', '']);
      seq.expectRowInBlock(0, LiveBlockKeys.blockquote);
      seq.expectRowNotInBlock(1, LiveBlockKeys.blockquote);

      await seq.enter(); // one more line below the quote
      seq.expectSource('> quote\n\n\n');
      // One Enter → one new row (not two): the absorbed separator stays hidden.
      seq.expectRows(['quote', '', '']);
    });

    testWidgets('typing after an exit lands outside the quote', (tester) async {
      final seq = await LiveRenderSequence.start(tester, '> quote', caret: 7);
      await seq.enter();
      await seq.enter(); // exit
      seq.expectRows(['quote', '']);

      await seq.type('after');
      seq.expectSource('> quote\n\nafter');
      // Once content follows the exit, the structural `\n\n` renders as a
      // blank row between the quote and the paragraph — the same way any
      // inter-block blank renders (it is only absorbed while it is the
      // document's terminal exit region). The typed text is a normal
      // paragraph, not lazy-continued into the quote.
      seq.expectRows(['quote', '', 'after']);
      seq.expectRowNotInBlock(2, LiveBlockKeys.blockquote);
    });
  });

  group('bullet list exit', () {
    testWidgets('continue, exit, and keep pressing Enter add one row each', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '- item', caret: 6);
      seq.expectRows(['item']);

      await seq.enter(); // continue the list
      seq.expectSource('- item\n- ');
      seq.expectRows(['item', '']);

      await seq.enter(); // exit the list
      seq.expectSource('- item\n\n');
      seq.expectRows(['item', '']);

      await seq.enter(); // one more line below the list
      seq.expectSource('- item\n\n\n');
      seq.expectRows(['item', '', '']);
    });

    testWidgets('typing after an exit lands in a plain paragraph', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '- item', caret: 6);
      await seq.enter();
      await seq.enter(); // exit
      await seq.type('after');
      seq.expectSource('- item\n\nafter');
      // As with the quote, the structural exit blank shows once text follows.
      seq.expectRows(['item', '', 'after']);
    });
  });

  group('ordered and task list exit', () {
    testWidgets('ordered list exit absorbs the separator like a bullet list', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, '1. a', caret: 4);
      seq.expectRows(['a']);
      await seq.enter(); // continue (the marker auto-increments)
      seq.expectSource('1. a\n2. ');
      await seq.enter(); // exit
      seq.expectSource('1. a\n\n');
      seq.expectRows(['a', '']);
      await seq.enter();
      seq.expectSource('1. a\n\n\n');
      seq.expectRows(['a', '', '']);
    });

    testWidgets('task list exit absorbs the separator', (tester) async {
      final seq = await LiveRenderSequence.start(
        tester,
        '- [ ] task',
        caret: 10,
      );
      seq.expectRows(['task']);
      await seq.enter(); // continue
      seq.expectSource('- [ ] task\n- [ ] ');
      await seq.enter(); // exit
      seq.expectSource('- [ ] task\n\n');
      seq.expectRows(['task', '']);
    });
  });

  group('mixed', () {
    testWidgets('exiting a quote that follows a paragraph adds one row', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(
        tester,
        'intro\n\n> quote',
        caret: 14,
      );
      // The blank line between the paragraph and the quote is a genuine
      // inter-block line and shows as its own row; only a block's *own*
      // terminal exit separator is absorbed.
      seq.expectRows(['intro', '', 'quote']);
      await seq.enter(); // continue
      await seq.enter(); // exit
      seq.expectSource('intro\n\n> quote\n\n');
      // The exit adds exactly one cursor row below the quote; the absorbed
      // terminal separator stays hidden.
      seq.expectRows(['intro', '', 'quote', '']);
      seq.expectRowInBlock(2, LiveBlockKeys.blockquote);
      seq.expectRowNotInBlock(3, LiveBlockKeys.blockquote);
    });
  });
}
