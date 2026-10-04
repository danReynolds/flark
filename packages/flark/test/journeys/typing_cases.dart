part of '../journey_test.dart';

/// Text typed where it changes the block structure of its line: an empty
/// row, a rule, a bare or hidden marker, a lazy line, an empty line of
/// indented code, or the keystroke that completes a construct.
void _typingCases(FlarkParseBackend backend) {
  String shellsOf(ProjectedRow row) =>
      row.shells.map((shell) => shell.kind.name).join('/');
  String shells(FlarkEditor editor) => shellsOf(editor.document.caretRow);

  group('typing', () {
    test('text typed after a list stays outside it', () {
      for (final (source, caret, typed) in [
        ('- a\n', 4, '- a\n\nx'),
        ('> a\n', 4, '> a\n\nx'),
        ('1) i\n', 5, '1) i\n\nx'),
        ('[^n]:0\n', 7, '[^n]:0\n\nx'),
        ('- a\r\n', 5, '- a\r\n\r\nx'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const InsertText('x'), source: typed);
        expect(shells(session.editor), '', reason: source);
        expect(session.editor.document.caretRow.text, 'x', reason: source);
      }
    });

    test('backspace on an empty item then typing leaves the list', () {
      // The same document Return on the empty item leaves.
      final session = _Session(backend, source: '- a\n- ', caret: 6);
      session.act(const DeleteBackward(), source: '- a\n');
      session.act(
        const InsertText('s'),
        source: '- a\n\ns',
        rows: ['a', '', 's'],
        caret: const DisplayPosition(2, 1),
      );
      expect(shells(session.editor), '');
    });

    test('text typed in the gap of a loose list stays where it shows', () {
      final session = _Session(backend, source: '- a\n\n- b\n', caret: 4);
      session.act(
        const InsertText('x'),
        source: '- a\n\nx\n- b\n',
        rows: ['a', '', 'x', 'b', ''],
        caret: const DisplayPosition(2, 1),
      );
      expect(shells(session.editor), '');
      expect(session.editor.projection.rows[3].shells.map((s) => s.kind), [
        ShellKind.list,
        ShellKind.item,
      ]);
    });

    test('text typed on an empty line of an item takes its indentation', () {
      final session = _Session(
        backend,
        source: '- a\n  - b\n\n  c\n',
        caret: 10,
      );
      session.act(
        const InsertText('x'),
        source: '- a\n  - b\n\n  x\n  c\n',
        rows: ['a', 'b', '', 'x\nc', ''],
        caret: const DisplayPosition(3, 1),
      );
      expect(shells(session.editor), 'list/item');
      final indented = _Session(backend, source: '-     a\n\n  b', caret: 8);
      indented.act(
        const InsertText('x'),
        source: '-     a\n  x\n  b',
        rows: ['a', 'x\nb'],
      );
      expect(shells(indented.editor), 'list/item');
    });

    test('text typed between blocks keeps the block after it', () {
      const p = RowKind.paragraph, b = RowKind.blank;
      for (final (source, caret, typed, rows, kinds) in [
        (
          'para\n\n    code\n',
          5,
          'para\nx\n\n    code\n',
          ['para\nx', '', 'code', ''],
          [p, b, RowKind.codeBlock, b],
        ),
        ('\n2. ', 0, 'x\n\n2. ', ['x', '', ''], [p, b, b]),
        ('\n* ', 0, 'x\n\n* ', ['x', '', ''], [p, b, b]),
        ('>\n>5. ', 1, '>x\n>\n>5. ', ['x', '', ''], [p, b, b]),
        (
          'para\n\n\n---\n',
          6,
          'para\n\nx\n\n---\n',
          ['para', '', 'x', '', '', ''],
          [p, b, p, b, RowKind.thematicBreak, b],
        ),
        ('\n-', 0, 'x\n\n-', ['x', '', '-'], [p, b, p]),
        (
          'Intro text\n\nTitle\n-----\n',
          11,
          'Intro text\nx\n\nTitle\n-----\n',
          ['Intro text\nx', '', 'Title', ''],
          [p, b, RowKind.heading, b],
        ),
        (
          '[^2]:\nNext paragraph\n',
          5,
          '[^2]:x\n\nNext paragraph\n',
          ['x', '', 'Next paragraph', ''],
          [p, b, p, b],
        ),
        ('>o\n\nr', 3, '>o\n\nx\nr', ['o', '', 'x\nr'], [p, b, p]),
        (
          'a\n\n[o]: /u',
          2,
          'a\nx\n\n[o]: /u',
          ['a\nx', '', '[o]: /u'],
          [p, b, RowKind.definition],
        ),
        (
          '<details>\n<summary>More</summary>\n\nHidden **text**.\n',
          34,
          '<details>\n<summary>More</summary>\nx\n\nHidden **text**.\n',
          ['<details>\n<summary>More</summary>\nx', '', 'Hidden text.', ''],
          [RowKind.htmlBlock, b, p, b],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final before = {
          for (final row in session.editor.projection.rows)
            if (row.kind != RowKind.blank) row.text: shellsOf(row),
        };
        session.act(const InsertText('x'), source: typed, rows: rows);
        expect(
          session.editor.projection.rows.map((row) => row.kind),
          kinds,
          reason: source,
        );
        // The rows that kept their text kept their containers.
        for (final row in session.editor.projection.rows) {
          if (before[row.text] case final was?) {
            expect(shellsOf(row), was, reason: '$source: ${row.text}');
          }
        }
      }
      // The empty footnote keeps the typed text, and the paragraph after it
      // stays outside.
      final note = _Session(
        backend,
        source: '[^2]:\nNext paragraph\n',
        caret: 5,
      );
      note.act(const InsertText('x'));
      expect(shells(note.editor), 'footnoteDefinition');
      expect(note.editor.projection.rows[2].shells, isEmpty);
    });

    test('text typed after a table starts its first row, not the next', () {
      final session = _Session(
        backend,
        source: '| a |\n| - |\n\nnext\n',
        caret: 12,
      );
      session.act(
        const InsertText('x'),
        source: '| a |\n| - |\nx\n\nnext\n',
        rows: ['a ', 'x', '', 'next', ''],
      );
      expect(session.editor.document.caretRow.kind, RowKind.tableCell);
      expect(session.editor.projection.rows[3].kind, RowKind.paragraph);
    });

    test('text typed on a rule starts a paragraph after it', () {
      for (final (source, caret, typed, rows) in [
        (
          'para\n\n---\n\nnext',
          9,
          'para\n\n---\nx\n\nnext',
          ['para', '', '', 'x', '', 'next'],
        ),
        ('***', 3, '***\nx', ['', 'x']),
        ('- ***\n', 5, '- ***\n  x\n', ['', 'x', '']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const InsertText('x'), source: typed, rows: rows);
        expect(
          session.editor.projection.rows
              .where((row) => row.kind == RowKind.thematicBreak)
              .length,
          1,
          reason: source,
        );
      }
      final pasted = _Session(backend, source: '---', caret: 3);
      pasted.act(const Paste('word'), source: '---\nword');
    });

    test('text typed after a hidden item marker keeps the item', () {
      for (final (source, caret, typed, rows) in [
        ('1.\n', 2, '1. s\n', ['s', '']),
        ('1)\n', 2, '1) s\n', ['s', '']),
        ('2) 7.\n', 5, '2) 7. s\n', ['s', '']),
        ('- a\n-\n', 5, '- a\n- s\n', ['a', 's', '']),
        ('-\n  ', 1, '- s\n  ', ['s', '']),
        ('- [ ]\n', 5, '- [ ] s\n', ['s', '']),
        ('-\n-', 3, '-\n- s', ['', 's']),
        ('1. a\n2.\n3. b', 7, '1. a\n2. s\n3. b', ['a', 's', 'b']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const InsertText('s'), source: typed, rows: rows);
        expect(shells(session.editor), endsWith('list/item'), reason: source);
      }
      final symbol = _Session(backend, source: '-\n  e', caret: 1);
      symbol.act(const InsertText('<'), source: '- <\n  e', rows: ['<\ne']);
    });

    test('text typed beside a bare marker keeps the block after it', () {
      for (final (source, caret, typed, rows) in [
        ('#\n[r]: /u', 1, '#x\n\n[r]: /u', ['#x', '', '[r]: /u']),
        (
          'a\n\n-\n[r]: /u',
          4,
          'a\n\n-x\n\n[r]: /u',
          ['a', '', '-x', '', '[r]: /u'],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const InsertText('x'), source: typed, rows: rows);
        expect(
          session.editor.projection.rows.last.kind,
          RowKind.definition,
          reason: source,
        );
      }
    });

    test('block syntax typed on a lazy line stays in its container', () {
      final session = _Session(backend, source: '- a\n#b\n\n  c', caret: 5);
      session.act(
        const InsertText(' '),
        source: '- a\n  # b\n\n  c',
        rows: ['a', 'b', '', 'c'],
      );
      expect(session.editor.document.caretRow.kind, RowKind.heading);
      expect(shells(session.editor), 'list/item');
      expect(session.editor.projection.rows.last.shells.length, 2);
      // So does text typed over a selection there.
      final selected = _Session(backend, source: '- a\nbc\n\n  d');
      selected.act(const SetSelection(4, 6));
      selected.act(
        const InsertText('#'),
        source: '- a\n  #\n\n  d',
        rows: ['a', '#', '', 'd'],
      );
      expect(shells(selected.editor), 'list/item');
      expect(selected.editor.projection.rows.last.shells.length, 2);
    });

    test('typing on a lazy line after a quoted definition stays quoted', () {
      // The paragraph's block opens with the definition, a line before the
      // row it shows; the prefix for its lazy lines is that first line's.
      for (final (typed, edited, kind) in [
        ('x', '> [a]: /u\nb\nxc', RowKind.paragraph),
        ('# ', '> [a]: /u\nb\n> # c', RowKind.heading),
      ]) {
        final session = _Session(backend, source: '> [a]: /u\nb\nc', caret: 12);
        session.act(InsertText(typed), source: edited);
        expect(session.editor.document.caretRow.kind, kind);
        expect(shells(session.editor), 'blockQuote');
      }
    });

    test('a heading level after definitions keeps them', () {
      // A paragraph whose block opens with definitions starts its row, and
      // so its heading, on its own first line.
      for (final (source, caret, edited) in [
        ('[a]: /u\nb', 9, '[a]: /u\n# b'),
        ('> [a]: /u\n> b', 13, '> [a]: /u\n> # b'),
        ('- [a]: /u\n  b', 13, '- [a]: /u\n  # b'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const SetHeadingLevel(1), source: edited);
        expect(session.editor.projection.rows.first.kind, RowKind.definition);
        expect(session.editor.document.caretRow.kind, RowKind.heading);
      }
    });

    test('text typed on an empty line of indented code is code', () {
      final session = _Session(
        backend,
        source: '    a\n\n    b\n---',
        caret: 6,
      );
      session.act(
        const InsertText('x'),
        source: '    a\n    x\n    b\n---',
        rows: ['a\nx\nb', ''],
        caret: const DisplayPosition(0, 3),
      );
    });

    test('typed leading whitespace that would move blocks is refused', () {
      for (final (source, caret, typed) in [
        ('- a\n  - b', 2, ' '),
        ('a\n\nb', 3, '\t'),
        ('>>1.  e\n>>\n>>     o', 6, ' '),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(InsertText(typed), applied: false, source: source);
      }
      // Typed over a selection where a line's content starts, likewise.
      const nested = '>>1.  e\n>>\n>>     o';
      final selected = _Session(backend, source: nested);
      selected.act(const SetSelection(6, 7));
      selected.act(const InsertText(' '), applied: false, source: nested);
      // Whitespace that moves nothing is typed as before.
      final plain = _Session(backend, source: 'a\n\nb', caret: 3);
      plain.act(const InsertText(' '), source: 'a\n\n b');
    });

    test('a pending style that cannot wrap the character is dropped', () {
      for (final (source, caret, style, typed) in [
        ('a\\', 2, Style.code, 'a\\b'),
        ('www.a.b', 7, Style.strikethrough, 'www.a.bb'),
        ('a~', 1, Style.strikethrough, 'ab~'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(ToggleStyle(style));
        session.act(const InsertText('b'), source: typed);
        expect(
          session.editor.document.hiddenIntervals.where(
            (h) => h.$1 >= caret && h.$2 <= caret + 1,
          ),
          isEmpty,
          reason: source,
        );
      }
      // Where it can pair, the style still applies.
      final styled = _Session(backend, source: 'a ', caret: 2);
      styled.act(const ToggleStyle(Style.strikethrough));
      styled.act(
        const InsertText('b'),
        source: 'a ~~b~~',
        context: Style.strikethrough,
      );
    });

    test('a backslash that would escape a cell boundary is refused', () {
      // GFM reads a backslash before a pipe as escaping it, a doubled one
      // too, so typed against the delimiter it would end the cell elsewhere.
      final session = _Session(backend, source: 'a|\n|-', caret: 1);
      session.act(const InsertText('\\'), applied: false, source: 'a|\n|-');
      // Elsewhere a backslash is typed as it is.
      final inner = _Session(backend, source: '| ab |\n| - |', caret: 3);
      inner.act(
        const InsertText('\\'),
        source: '| a\\b |\n| - |',
        caret: const DisplayPosition(0, 2),
      );
      final padded = _Session(backend, source: '| a |\n| - |', caret: 3);
      padded.act(const InsertText('\\'), source: '| a\\ |\n| - |');
    });

    test('typing in a cell\'s code with an escaped pipe moves the caret', () {
      // The span shows its source less the pipe's backslash; the other
      // characters map one to one, so the caret follows each typed one.
      final session = _Session(backend, source: 'o\n-|\n`\\|`', caret: 6);
      session.act(
        const InsertText('x'),
        source: 'o\n-|\n`x\\|`',
        rows: ['o', 'x|'],
        caret: const DisplayPosition(1, 1),
      );
      session.act(
        const InsertText('y'),
        rows: ['o', 'xy|'],
        caret: const DisplayPosition(1, 2),
      );
      // The escaped pipe stays one unit: Backspace after it takes both.
      session.act(const SetSelection.caret(10));
      session.act(const DeleteBackward(), source: 'o\n-|\n`xy`');
    });

    test('a typed table delimiter row keeps the caret on it', () {
      final session = _Session(backend, source: '| a | b |\n| - | ', caret: 16);
      session.act(
        const InsertText('-'),
        source: '| a | b |\n| - | -',
        rows: ['a ', 'b ', '| - | -'],
        anchor: 17,
      );
      session.act(const InsertText(' '), source: '| a | b |\n| - | - ');
      session.act(
        const InsertText('|'),
        source: '| a | b |\n| - | - |',
        anchor: 19,
      );
      session.act(
        const Newline(),
        source: '| a | b |\n| - | - |\n',
        anchor: 20,
      );
      session.act(
        const InsertText('c'),
        source: '| a | b |\n| - | - |\nc',
        rows: ['a ', 'b ', 'c', ''],
        anchor: 21,
      );
      expect(session.editor.document.caretRow.kind, RowKind.tableCell);
      // Text on the next line stays a paragraph, apart from the new table.
      final followed = _Session(
        backend,
        source: '| a | b |\n| - | \nnext',
        caret: 16,
      );
      followed.act(
        const InsertText('-'),
        source: '| a | b |\n| - | -\n\nnext',
        rows: ['a ', 'b ', '| - | -', '', 'next'],
        anchor: 17,
      );
      // An edit of that row keeps the table, or is refused: restructuring a
      // table uses source mode.
      final edited = _Session(
        backend,
        source: '| a | b |\n| --- | --- |',
        caret: 15,
      );
      edited.act(const DeleteBackward(), source: '| a | b |\n| -- | --- |');
      edited.act(const SetSelection.caret(23));
      edited.act(const InsertText('x'), applied: false);
      edited.act(const SetSelection.caret(14));
      edited.act(const DeleteBackward(word: true), applied: false);
    });

    test('the third fence marker completes the fence wherever it is typed', () {
      for (final (source, caret, typed) in [
        ('~~\n\nnext\n', 0, '~'),
        ('``\n\nnext\n', 1, '`'),
        ('``\n\nnext\n', 2, '`'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final fence = typed * 3;
        session.act(
          InsertText(typed),
          source: '$fence\n\n$fence\n\n\nnext\n',
          rows: ['', '', '', 'next', ''],
          anchor: 4,
        );
        expect(session.editor.document.caretRow.fenced, isTrue);
      }
      // On a line with text after the run the marker would open a fence
      // whose info string hides that text and turns what follows into code:
      // it stays a literal character instead.
      final text = _Session(backend, source: '~~a~~\n\nnext\n', caret: 0);
      text.act(
        const InsertText('~'),
        source: '\\~~~a~~\n\nnext\n',
        rows: ['~a', '', 'next', ''],
        anchor: 2,
      );
    });
  });
}
