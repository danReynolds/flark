import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_render_sequence.dart';

/// Widget-tier (rendered) sequences for **table editing** — the coverage gap
/// where table *commands* are unit-tested and tables appear in goldens, but
/// nothing drives cell navigation and in-cell editing through the real
/// per-cell editables.
///
/// A GFM table renders as one keyed [EditableText] per cell
/// (`_EditableTableCell`, keyed `table-cell:row:col:start`), so each cell is a
/// distinct row in [LiveRenderSequence.rows]. The delimiter row (`| - | - |`)
/// is metadata and does *not* render as an editable, so a 2×2 GFM table
/// projects into exactly four cell rows.
///
/// What this suite pins about the current implementation (all observed, not
/// predicted):
///
///  * **Cells do not self-focus from the controller selection.** Moving the
///    source caret into a cell's range (`moveCaret`) leaves `focusedRow` null;
///    each cell owns its own [FocusNode], driven by taps in a real app, not by
///    the document selection. Programmatic selection alone focuses nothing.
///  * **Tab does not navigate between cells.** There is no
///    `FocusTraversalGroup`/`NextFocusIntent` wiring for the cell editables, so
///    Tab/Shift-Tab never advance from one cell to the next. The only focus
///    that appears is the first cell, which the harness grabs when it shows the
///    keyboard for a key event — never cell 1+.
///  * **Arrow keys do not move focus between cells** either: Down from a header
///    cell stays in that cell, it does not descend into the body cell below.
///  * **Enter is a no-op** inside a table cell — it adds no row, exits nothing,
///    and leaves the source byte-identical (the cell input formatter strips
///    newlines).
///
/// In-cell typing and inline styling both mutate the correct cell's source and
/// hold the export round-trip gate. A literal `|` typed into a cell is escaped
/// to `\|`; it round-trips once the live controller re-parses authoritatively
/// (the pre-parse predicted projection transiently shows the raw backslash —
/// expected, not a defect), pinned under `pipe delimiter in a cell` below.
void main() {
  // A small GFM table: header row `a | b`, body row `c | d`. Content offsets:
  //   'a' -> [2,3)   'b' -> [6,7)   'c' -> [22,23)   'd' -> [26,27)
  const table = '| a | b |\n| - | - |\n| c | d |';

  group('structure', () {
    testWidgets('each cell renders as its own editable row inside the table', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // Four cell editables; the delimiter row is not one of them.
      seq.expectRows(['a', 'b', 'c', 'd']);
      seq.expectRowInBlock(0, LiveBlockKeys.table);
      seq.expectRowInBlock(1, LiveBlockKeys.table);
      seq.expectRowInBlock(2, LiveBlockKeys.table);
      seq.expectRowInBlock(3, LiveBlockKeys.table);

      // Autofocus targets the top-level block editor, not a cell: a pure-table
      // document opens with no cell focused.
      expect(seq.focusedRow, isNull);
    });
  });

  group('cell navigation', () {
    testWidgets('moving the source caret into a cell does not focus it', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // Each cell owns its FocusNode; the controller selection does not route
      // focus to it. Every cell offset leaves focus unclaimed.
      for (final offset in [2, 6, 22, 26]) {
        await seq.moveCaret(offset);
        expect(
          seq.focusedRow,
          isNull,
          reason: 'moveCaret($offset) unexpectedly focused a cell',
        );
      }
      seq.expectRows(['a', 'b', 'c', 'd']);
    });

    testWidgets('Tab and Shift-Tab do not advance between cells', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // The first Tab focuses cell 0 (the harness shows the keyboard on the
      // first editable) and then does nothing — there is no cell traversal.
      await seq.tab();
      seq.expectFocusedRow(0);
      seq.expectSource(table);

      // Were Tab wired to cell navigation, focus would now be cell 1, then 2.
      // It stays on cell 0 across repeated Tabs and a Shift-Tab.
      await seq.tab();
      seq.expectFocusedRow(0);
      await seq.tab();
      seq.expectFocusedRow(0);
      await seq.tab(shift: true);
      seq.expectFocusedRow(0);

      seq.expectSource(table);
      seq.expectRows(['a', 'b', 'c', 'd']);
    });

    testWidgets('arrow keys do not cross the cell boundary', (tester) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // Arrows focus cell 0 (via the harness keyboard) and move the caret
      // within it, but never hand focus to a neighbouring cell — Down from the
      // header cell does not descend into the body cell below.
      await seq.arrow(LogicalKeyboardKey.arrowRight);
      seq.expectFocusedRow(0);
      await seq.arrow(LogicalKeyboardKey.arrowDown);
      seq.expectFocusedRow(0);
      await seq.arrow(LogicalKeyboardKey.arrowLeft);
      seq.expectFocusedRow(0);
      await seq.arrow(LogicalKeyboardKey.arrowUp);
      seq.expectFocusedRow(0);

      seq.expectSource(table);
      seq.expectRows(['a', 'b', 'c', 'd']);
    });
  });

  group('in-cell editing', () {
    testWidgets('typing edits the focused cell and round-trips', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // With no cell explicitly focused, `type` routes to the first cell
      // ("a") — the only cell the harness can reliably reach, since neither
      // caret movement nor Tab/arrows focus a specific cell.
      await seq.type('x');
      seq.expectSource('| ax | b |\n| - | - |\n| c | d |');
      seq.expectRows(['ax', 'b', 'c', 'd']);
      seq.expectFocusedRow(0);
      seq.expectRowInBlock(0, LiveBlockKeys.table);

      // A second keystroke continues in the same cell.
      await seq.type('y');
      seq.expectSource('| axy | b |\n| - | - |\n| c | d |');
      seq.expectRows(['axy', 'b', 'c', 'd']);
    });

    testWidgets('inline styling lands in the cell source and round-trips', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // Select the header cell "a" in source space and bold it through the
      // command path (the same path a toolbar button uses). The markers land
      // inside the cell, and the fresh-parse round-trip gate holds.
      await seq.select(2, 3);
      await seq.toggleStyle(FlarkMarkdownInlineStyle.strong);
      seq.expectSource('| **a** | b |\n| - | - |\n| c | d |');
      seq.expectRows(['**a**', 'b', 'c', 'd']);
      seq.expectRowInBlock(0, LiveBlockKeys.table);
    });
  });

  group('enter in a table', () {
    testWidgets('Enter is a no-op — no new row, no exit, source unchanged', (
      tester,
    ) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // Enter routed into a cell inserts nothing (the cell input formatter
      // strips newlines); the table neither grows a row nor is exited.
      await seq.enter();
      seq.expectSource(table);
      seq.expectRows(['a', 'b', 'c', 'd']);
      seq.expectFocusedRow(0);

      // Pressing it again changes nothing, and the round-trip gate stays green.
      await seq.enter();
      seq.expectSource(table);
      seq.expectRows(['a', 'b', 'c', 'd']);
    });
  });

  group('pipe delimiter in a cell', () {
    testWidgets('typing a literal | escapes it to \\| and round-trips once '
        'the cell re-parses', (tester) async {
      final seq = await LiveRenderSequence.start(tester, table);

      // Type `|` into cell "a" through its real editable — the same input path
      // a keystroke takes: the cell input formatter, then the cell→source
      // write-back that escapes `|`→`\|`. Delivered as a platform value here
      // rather than via `seq.type`, because the escape only round-trips after
      // an authoritative parse (see below) and `seq.type` would run the
      // round-trip gate against the still-predicted projection.
      final cellA = find.byWidgetPredicate(
        (widget) => widget is EditableText && widget.controller.text == 'a',
      );
      await tester.enterText(cellA, 'a|');
      await tester.pumpAndSettle();

      // The pipe is escaped in the source, so the cell literally contains "a|".
      seq.expectSource('| a\\| | b |\n| - | - |\n| c | d |');

      // Verified reality (the earlier "round-trip defect" was expected
      // transient behavior, not a bug): right after the edit the live
      // projection is PREDICTED and still shows the raw backslash
      // (`| a\| | b |…`), while a fresh authoritative parse hides it as an
      // escape marker (`| a| | b |…`). The two disagree *only* until the live
      // controller re-parses; forcing that parse makes the live projection
      // authoritative and byte-identical to a fresh parse, so the export
      // round-trip gate (re-run by `expectRows`) holds.
      await seq.controller.parseNow();
      await tester.pumpAndSettle();
      seq.expectRows(['a|', 'b', 'c', 'd']);
    });
  });
}
