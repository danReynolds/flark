import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/inline/flark_inline_delimiter_placement.dart';
import 'package:flark/src/v2/markdown/inline/flark_inline_run_scanner.dart';
import 'package:test/test.dart';

String _apply(String source, FlarkInlinePlacementEdit edit) {
  return source.replaceRange(
    edit.range.start,
    edit.range.end,
    edit.replacement,
  );
}

/// A hand-built run scan, mirroring what `FlarkProjection.inlineRunScans`
/// pairs from the parser's hidden ranges; the marker is read from [source]
/// at the closing cluster, exactly as the projection does.
FlarkInlineRunScan _run(
  String source,
  int open,
  int content,
  int close,
  int closeEnd,
) {
  return FlarkInlineRunScan(
    openStart: open,
    contentStart: content,
    closeStart: close,
    closeEnd: closeEnd,
    marker: source.substring(close, closeEnd),
  );
}

void main() {
  group('splitEdgeWhitespace', () {
    test('splits leading, core, and trailing', () {
      final split = FlarkInlineDelimiterPlacement.splitEdgeWhitespace(' a b  ');
      expect(split.leading, ' ');
      expect(split.core, 'a b');
      expect(split.trailing, '  ');
    });

    test('whitespace-only text is all leading', () {
      final split = FlarkInlineDelimiterPlacement.splitEdgeWhitespace('  ');
      expect(split.leading, '  ');
      expect(split.core, '');
      expect(split.trailing, '');
    });
  });

  group('armedWrap', () {
    test('hugs the core and pushes trailing whitespace outside', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '',
        caret: 0,
        text: 'hello world ',
        open: '**',
        close: '**',
      );
      expect(_apply('', edit), '**hello world** ');
      expect(edit.caretAfter, 16);
      expect(edit.continuationMarker, '**');
    });

    test('keeps the caret inside when there is no trailing whitespace', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '',
        caret: 0,
        text: 'x',
        open: '**',
        close: '**',
      );
      expect(_apply('', edit), '**x**');
      expect(edit.caretAfter, 3);
      expect(edit.continuationMarker, isNull);
    });

    test('keeps leading whitespace outside the opening delimiter', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '',
        caret: 0,
        text: ' h',
        open: '**',
        close: '**',
      );
      expect(_apply('', edit), ' **h**');
      expect(edit.caretAfter, 4);
      expect(edit.continuationMarker, isNull);
    });

    test('whitespace-only text commits unwrapped and stays armed', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '',
        caret: 0,
        text: ' ',
        open: '**',
        close: '**',
      );
      expect(_apply('', edit), ' ');
      expect(edit.caretAfter, 1);
      expect(edit.continuationMarker, '**');
    });

    test('re-enters a run across its committed trailing whitespace', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '**hello** ',
        caret: 10,
        text: 'x',
        open: '**',
        close: '**',
      );
      expect(_apply('**hello** ', edit), '**hello x**');
      expect(edit.caretAfter, 9);
      expect(edit.continuationMarker, isNull);
    });

    test('re-entry with trailing whitespace stays armed', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '**hello** ',
        caret: 10,
        text: 'x ',
        open: '**',
        close: '**',
      );
      expect(_apply('**hello** ', edit), '**hello x** ');
      expect(edit.caretAfter, 12);
      expect(edit.continuationMarker, '**');
    });

    test('edge-insensitive wrap (inline code) applies verbatim', () {
      final edit = FlarkInlineDelimiterPlacement.armedWrap(
        source: '',
        caret: 0,
        text: ' ',
        open: '`',
        close: '`',
        edgeSensitive: false,
      );
      expect(_apply('', edit), '` `');
      expect(edit.caretAfter, 2);
      expect(edit.continuationMarker, isNull);
    });
  });

  group('contentEditRepair', () {
    test('trailing whitespace typed at the close jumps the delimiter', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**hello**',
        start: 7,
        end: 7,
        text: ' ',
      );
      expect(edit, isNotNull);
      expect(_apply('**hello**', edit!), '**hello** ');
      expect(edit.caretAfter, 10);
      expect(edit.continuationMarker, '**');
    });

    test('mixed text keeps the core inside and whitespace outside', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**hello**',
        start: 7,
        end: 7,
        text: 'x ',
      );
      expect(edit, isNotNull);
      expect(_apply('**hello**', edit!), '**hellox** ');
      expect(edit.caretAfter, 11);
      expect(edit.continuationMarker, '**');
    });

    test('clean edits need no repair', () {
      expect(
        FlarkInlineDelimiterPlacement.contentEditRepair(
          source: '**hello**',
          start: 7,
          end: 7,
          text: 'x',
        ),
        isNull,
      );
      expect(
        FlarkInlineDelimiterPlacement.contentEditRepair(
          source: '**hello**',
          start: 4,
          end: 4,
          text: ' ',
        ),
        isNull,
      );
      expect(
        FlarkInlineDelimiterPlacement.contentEditRepair(
          source: '**a xb**',
          start: 4,
          end: 5,
          text: '',
        ),
        isNull,
      );
    });

    test('leading whitespace stays before the opening delimiter', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**foo**',
        start: 2,
        end: 2,
        text: ' x',
      );
      expect(edit, isNotNull);
      expect(_apply('**foo**', edit!), ' **xfoo**');
      expect(edit.caretAfter, 4);
      expect(edit.continuationMarker, isNull);
    });

    test('whitespace-only text at the leading edge commits outside', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**foo**',
        start: 2,
        end: 2,
        text: ' ',
      );
      expect(edit, isNotNull);
      expect(_apply('**foo**', edit!), ' **foo**');
      expect(edit.caretAfter, 1);
    });

    test('deletion exposing trailing whitespace relocates the close', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**foo x**',
        start: 6,
        end: 7,
        text: '',
      );
      expect(edit, isNotNull);
      expect(_apply('**foo x**', edit!), '**foo** ');
      expect(edit.caretAfter, 8);
      expect(edit.continuationMarker, '**');
    });

    test('deletion exposing leading whitespace relocates the open', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**x foo**',
        start: 2,
        end: 3,
        text: '',
      );
      expect(edit, isNotNull);
      expect(_apply('**x foo**', edit!), ' **foo**');
      expect(edit.caretAfter, 0);
      expect(edit.continuationMarker, isNull);
    });

    test('replacing content with whitespace relocates the close', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**hello world**',
        start: 8,
        end: 13,
        text: ' ',
      );
      expect(edit, isNotNull);
      expect(_apply('**hello world**', edit!), '**hello**  ');
      expect(edit.caretAfter, 11);
      expect(edit.continuationMarker, '**');
    });

    test('dissolves the run when only whitespace would remain', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '**ab**',
        start: 2,
        end: 4,
        text: '',
      );
      expect(edit, isNotNull);
      expect(_apply('**ab**', edit!), '');
      expect(edit.caretAfter, 0);
      expect(edit.continuationMarker, '**');
    });

    test('does not touch literal (invalid) delimiter text', () {
      expect(
        FlarkInlineDelimiterPlacement.contentEditRepair(
          source: '**hello **',
          start: 8,
          end: 8,
          text: ' ',
        ),
        isNull,
      );
      expect(
        FlarkInlineDelimiterPlacement.contentEditRepair(
          source: '**a **b',
          start: 3,
          end: 4,
          text: '',
        ),
        isNull,
      );
    });

    test('whitespace bubbles out through flush nested delimiters', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '*~~f~~*',
        start: 4,
        end: 4,
        text: ' ',
      );
      expect(edit, isNotNull);
      expect(_apply('*~~f~~*', edit!), '*~~f~~* ');
      expect(edit.caretAfter, 8);
      expect(edit.continuationMarker, '~~*');
    });

    test('whitespace parks inside a non-flush enclosing run', () {
      // The em run has content after the strike run, so the space is legal
      // interior content one level out — only the strike close moves.
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '*~~f~~ tail*',
        start: 4,
        end: 4,
        text: ' ',
      );
      expect(edit, isNotNull);
      expect(_apply('*~~f~~ tail*', edit!), '*~~f~~  tail*');
      expect(edit.caretAfter, 7);
    });

    test('a nested dissolve cascades through emptied enclosing runs', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '*~~f~~*',
        start: 3,
        end: 4,
        text: '',
      );
      expect(edit, isNotNull);
      expect(_apply('*~~f~~*', edit!), '');
      expect(edit.caretAfter, 0);
      expect(edit.continuationMarker, '~~*');
    });

    test('a nested dissolve relocates an enclosing run off exposed space', () {
      final edit = FlarkInlineDelimiterPlacement.contentEditRepair(
        source: '*~~f~~ tail*',
        start: 3,
        end: 4,
        text: '',
      );
      expect(edit, isNotNull);
      expect(_apply('*~~f~~ tail*', edit!), ' *tail*');
      expect(edit.caretAfter, 0);
    });
  });

  group('runSplit', () {
    test('moves straddled whitespace between the split delimiters', () {
      final edit = FlarkInlineDelimiterPlacement.runSplit(
        source: '**foo bar**',
        contentRange: FlarkSourceRange(2, 9),
        caret: 6,
        marker: '**',
        text: 'x',
      );
      expect(_apply('**foo bar**', edit), '**foo** x**bar**');
      expect(edit.caretAfter, 9);
    });

    test('splits cleanly with whitespace on both sides of the caret', () {
      final edit = FlarkInlineDelimiterPlacement.runSplit(
        source: '**foo  bar**',
        contentRange: FlarkSourceRange(2, 10),
        caret: 6,
        marker: '**',
        text: 'x',
      );
      expect(_apply('**foo  bar**', edit), '**foo** x **bar**');
      expect(edit.caretAfter, 9);
    });

    test('splits without whitespace exactly like the legacy path', () {
      final edit = FlarkInlineDelimiterPlacement.runSplit(
        source: '**foobar**',
        contentRange: FlarkSourceRange(2, 8),
        caret: 5,
        marker: '**',
        text: 'x',
      );
      expect(_apply('**foobar**', edit), '**foo**x**bar**');
      expect(edit.caretAfter, 8);
    });
  });

  group('markerCrossingRepair', () {
    // `**bold** plain`: strong run open [0,2) content [2,6) close [6,8).
    const boldPlain = '**bold** plain';
    final boldPlainRuns = [_run(boldPlain, 0, 2, 6, 8)];

    test('covered close with typing joins the run and relocates the close', () {
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: boldPlain,
        start: 4,
        end: 12,
        text: 'x',
        runs: boldPlainRuns,
      );
      expect(edit, isNotNull);
      expect(_apply(boldPlain, edit!), '**box**in');
      expect(edit.caretAfter, 5);
      expect(edit.continuationMarker, isNull);
    });

    test('covered close with a pure deletion keeps the pair balanced', () {
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: boldPlain,
        start: 4,
        end: 11,
        text: '',
        runs: boldPlainRuns,
      );
      expect(edit, isNotNull);
      expect(_apply(boldPlain, edit!), '**bo**ain');
      expect(edit.caretAfter, 4);
    });

    test('covered close with trailing-whitespace text keeps it outside', () {
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: boldPlain,
        start: 4,
        end: 12,
        text: 'x ',
        runs: boldPlainRuns,
      );
      expect(edit, isNotNull);
      expect(_apply(boldPlain, edit!), '**box** in');
      expect(edit.caretAfter, 8);
      expect(edit.continuationMarker, '**');
    });

    test('covered close whose survivors end in whitespace hugs the core', () {
      // `**bo x** tail`: the selection starts right after the interior
      // space, so the surviving content ends in whitespace; the relocated
      // close hugs the core and the whitespace stays outside.
      const source = '**bo x** tail';
      final runs = [_run(source, 0, 2, 6, 8)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 5,
        end: 10,
        text: '',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '**bo** ail');
      expect(edit.continuationMarker, '**');
    });

    test('covered close of an underscore run rewrites to the alternate '
        'marker when the close cannot flank', () {
      // `_box_in` is invalid (`_` cannot close intraword); the repair
      // switches the run to `*`, which can.
      const source = '_bold_ plain';
      final runs = [_run(source, 0, 1, 5, 6)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 3,
        end: 10,
        text: 'x',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '*box*in');
      expect(edit.caretAfter, 4);
    });

    // `plain **bold**`: strong run open [6,8) content [8,12) close [12,14).
    const plainBold = 'plain **bold**';
    final plainBoldRuns = [_run(plainBold, 6, 8, 12, 14)];

    test('covered open with typing keeps the text outside the run', () {
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: plainBold,
        start: 4,
        end: 10,
        text: 'x',
        runs: plainBoldRuns,
      );
      expect(edit, isNotNull);
      expect(_apply(plainBold, edit!), 'plaix**ld**');
      expect(edit.caretAfter, 5);
    });

    test('covered open with a pure deletion resumes the run', () {
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: plainBold,
        start: 4,
        end: 10,
        text: '',
        runs: plainBoldRuns,
      );
      expect(edit, isNotNull);
      expect(_apply(plainBold, edit!), 'plai**ld**');
      expect(edit.caretAfter, 4);
    });

    test('covered open sits after whitespace leading the survivors', () {
      // `plain **b old**`: deleting through `b` leaves ` old` — the open
      // relocates past the space CommonMark refuses to style.
      const source = 'plain **b old**';
      final runs = [_run(source, 6, 8, 13, 15)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 4,
        end: 9,
        text: '',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), 'plai **old**');
      expect(edit.caretAfter, 4);
    });

    test('both covered with the same marker merges the runs around the '
        'typed text', () {
      // `**bold** and **brave**`: A close [6,8), B open [13,15).
      const source = '**bold** and **brave**';
      final runs = [_run(source, 0, 2, 6, 8), _run(source, 13, 15, 20, 22)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 4,
        end: 18,
        text: 'x',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '**boxve**');
      expect(edit.caretAfter, 5);
    });

    test('both covered with different markers rebalances both pairs', () {
      // `**bold** and *brave*`: the direct rebalance `**box***ve*` would
      // fuse `**` and `*` into a `***` delimiter run, so B rewrites with
      // its alternate marker character.
      const source = '**bold** and *brave*';
      final runs = [_run(source, 0, 2, 6, 8), _run(source, 13, 14, 19, 20)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 4,
        end: 17,
        text: 'x',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '**box**_ve_');
      expect(edit.caretAfter, 5);
    });

    test('both covered with different non-fusing markers keeps both '
        'markers', () {
      // `~~a~~` close then `*brave*` open: `~~` and `*` never fuse, so the
      // direct rebalanced form stands.
      const source = '~~bold~~ and *brave*';
      final runs = [_run(source, 0, 2, 6, 8), _run(source, 13, 14, 19, 20)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 4,
        end: 17,
        text: 'x',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '~~box~~*ve*');
      expect(edit.caretAfter, 5);
    });

    test('covered close of a code span relocates the backtick verbatim', () {
      // '`code` plain': code span open [0,1) content [1,5) close [5,6).
      const source = '`code` plain';
      final runs = [_run(source, 0, 1, 5, 6)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 3,
        end: 10,
        text: 'x ',
        runs: runs,
      );
      expect(edit, isNotNull);
      // No whitespace splitting: code whitespace is content, the close sits
      // directly after the typed text.
      expect(_apply(source, edit!), '`cox `in');
      expect(edit.caretAfter, 5);
    });

    test('covered open of a code span relocates the backtick verbatim', () {
      // 'plain `code`': code span open [6,7) content [7,11) close [11,12).
      const source = 'plain `code`';
      final runs = [_run(source, 6, 7, 11, 12)];
      final edit = FlarkInlineDelimiterPlacement.markerCrossingRepair(
        source: source,
        start: 4,
        end: 9,
        text: 'x',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), 'plaix`de`');
      expect(edit.caretAfter, 5);
    });

    test('an edit covering a whole pair returns null', () {
      // `x **bold** y`: both delimiters inside the range — the existing
      // expansion/plain behavior owns this.
      const source = 'x **bold** y';
      final runs = [_run(source, 2, 4, 8, 10)];
      expect(
        FlarkInlineDelimiterPlacement.markerCrossingRepair(
          source: source,
          start: 1,
          end: 11,
          text: 'z',
          runs: runs,
        ),
        isNull,
      );
      expect(
        FlarkInlineDelimiterPlacement.markerCrossingRepair(
          source: source,
          start: 1,
          end: 11,
          text: '',
          runs: runs,
        ),
        isNull,
      );
    });

    test('an edit fully inside one run\'s content returns null', () {
      const source = '**hello**';
      final runs = [_run(source, 0, 2, 7, 9)];
      expect(
        FlarkInlineDelimiterPlacement.markerCrossingRepair(
          source: source,
          start: 3,
          end: 6,
          text: 'x',
          runs: runs,
        ),
        isNull,
      );
    });

    test('replacement text carrying delimiter characters returns null', () {
      expect(
        FlarkInlineDelimiterPlacement.markerCrossingRepair(
          source: boldPlain,
          start: 4,
          end: 12,
          text: 'x*',
          runs: boldPlainRuns,
        ),
        isNull,
      );
    });

    test('a stacked (multi-run) crossing returns null', () {
      // `***a*** p`: outer strong close [5,7) and inner emphasis close
      // [4,5) are both covered — two crossed pairs is a shape the repair
      // does not attempt.
      const source = '***a*** p';
      final runs = [_run(source, 0, 2, 5, 7), _run(source, 2, 3, 4, 5)];
      expect(
        FlarkInlineDelimiterPlacement.markerCrossingRepair(
          source: source,
          start: 3,
          end: 8,
          text: '',
          runs: runs,
        ),
        isNull,
      );
    });
  });

  group('joiningDeletionRepair', () {
    test('merges two same-marker runs across a consumed gap', () {
      // `**a** **b**` minus the space would fuse into the literal
      // `**a****b**`.
      const source = '**a** **b**';
      final runs = [_run(source, 0, 2, 3, 5), _run(source, 6, 8, 9, 11)];
      final edit = FlarkInlineDelimiterPlacement.joiningDeletionRepair(
        source: source,
        start: 5,
        end: 6,
        text: '',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '**ab**');
      expect(edit.caretAfter, 3);
    });

    test('merges stacked neighbors cluster-chain-wise', () {
      // `***a*** ***b***`: each side is a strong run and an emphasis run
      // with adjacent clusters; both inner `***` chains drop.
      const source = '***a*** ***b***';
      final runs = [
        _run(source, 0, 2, 5, 7),
        _run(source, 2, 3, 4, 5),
        _run(source, 8, 10, 13, 15),
        _run(source, 10, 11, 12, 13),
      ];
      final edit = FlarkInlineDelimiterPlacement.joiningDeletionRepair(
        source: source,
        start: 7,
        end: 8,
        text: '',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '***ab***');
      expect(edit.caretAfter, 4);
    });

    test('rewrites a same-character different-marker neighbor with the '
        'alternate delimiter', () {
      // `**a***b*` would fuse into a `***` delimiter run and leak; the
      // second run switches character instead.
      const source = '**a** *b*';
      final runs = [_run(source, 0, 2, 3, 5), _run(source, 6, 7, 8, 9)];
      final edit = FlarkInlineDelimiterPlacement.joiningDeletionRepair(
        source: source,
        start: 5,
        end: 6,
        text: '',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '**a**_b_');
      expect(edit.caretAfter, 5);
    });

    test('rewrites the left run when the right one cannot switch', () {
      // `**a** *b*c`: `_b_c` cannot close (underscore intraword), so the
      // left strong run switches character instead: `__a__*b*c`.
      const source = '**a** *b*c';
      final runs = [_run(source, 0, 2, 3, 5), _run(source, 6, 7, 8, 9)];
      final edit = FlarkInlineDelimiterPlacement.joiningDeletionRepair(
        source: source,
        start: 5,
        end: 6,
        text: '',
        runs: runs,
      );
      expect(edit, isNotNull);
      expect(_apply(source, edit!), '__a__*b*c');
      expect(edit.caretAfter, 5);
    });

    test('different-character neighbors need no repair', () {
      // `**a**~~b~~` is valid adjacency — the plain deletion stands.
      const source = '**a** ~~b~~';
      final runs = [_run(source, 0, 2, 3, 5), _run(source, 6, 8, 9, 11)];
      expect(
        FlarkInlineDelimiterPlacement.joiningDeletionRepair(
          source: source,
          start: 5,
          end: 6,
          text: '',
          runs: runs,
        ),
        isNull,
      );
    });

    test('tilde runs of different cluster lengths bail (no alternate '
        'character exists)', () {
      const source = '~a~ ~~b~~';
      final runs = [_run(source, 0, 1, 2, 3), _run(source, 4, 6, 7, 9)];
      expect(
        FlarkInlineDelimiterPlacement.joiningDeletionRepair(
          source: source,
          start: 3,
          end: 4,
          text: '',
          runs: runs,
        ),
        isNull,
      );
    });

    test('a merge whose seam touches delimiter content characters bails', () {
      // Merging `**x~**` and `**~y**` would put `~~` inside the merged run,
      // whose pairing this repair cannot predict.
      const source = '**x~** **~y**';
      final runs = [_run(source, 0, 2, 4, 6), _run(source, 7, 9, 11, 13)];
      expect(
        FlarkInlineDelimiterPlacement.joiningDeletionRepair(
          source: source,
          start: 6,
          end: 7,
          text: '',
          runs: runs,
        ),
        isNull,
      );
    });

    test('a non-empty replacement separates the clusters and needs no '
        'repair', () {
      const source = '**a** **b**';
      final runs = [_run(source, 0, 2, 3, 5), _run(source, 6, 8, 9, 11)];
      expect(
        FlarkInlineDelimiterPlacement.joiningDeletionRepair(
          source: source,
          start: 5,
          end: 6,
          text: 'x',
          runs: runs,
        ),
        isNull,
      );
    });

    test('a partially surviving gap needs no repair', () {
      const source = '**a**  **b**';
      final runs = [_run(source, 0, 2, 3, 5), _run(source, 7, 9, 10, 12)];
      expect(
        FlarkInlineDelimiterPlacement.joiningDeletionRepair(
          source: source,
          start: 5,
          end: 6,
          text: '',
          runs: runs,
        ),
        isNull,
      );
    });
  });
}
