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
        // Literal HTML would read the text as its own: an empty line keeps
        // it apart, and the paragraph after takes the text as its first line.
        (
          '<details>\n<summary>More</summary>\n\nHidden **text**.\n',
          34,
          '<details>\n<summary>More</summary>\n\nx\nHidden **text**.\n',
          ['<details>\n<summary>More</summary>', '', 'x\nHidden text.', ''],
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

    test('text typed on an empty row shows no whitespace the row hid', () {
      // Whitespace after an empty line's container prefix shows nothing.
      // Text run into it would show it: as a tab's columns before code the
      // indentation makes of the text, or as a line of code the text joins.
      // The text goes where the prefix ends instead, an empty item's marker
      // padded with one space, with a blank line after it where the block
      // below would read on into it. Typing never shortens the source: the
      // whitespace becomes spaces a paragraph does not show, or that blank
      // line.
      for (final (source, caret, typed, edited, rows) in [
        (
          '    foo *[\n    \t\n\t',
          18,
          'a',
          '    foo *[\n    \t\n a',
          ['foo *[', '', 'a'],
        ),
        (
          '-\t\t\n \t\t{(>oob\n \t\t<#]',
          3,
          'a',
          '- a\n\n \t\t{(>oob\n \t\t<#]',
          ['a', '', '  {(>oob\n  <#]'],
        ),
        ('>\t\tfoo\n>\t\t\n', 10, 'é', '>\t\tfoo\n>  é\n', ['  foo', 'é', '']),
        (
          '- foo\n\n\t\tba_\n\t\t\n',
          15,
          '1',
          '- foo\n\n\t\tba_\n  1\n',
          ['foo', '', '  ba_', '1', ''],
        ),
        (
          '1. foo\r\n2.  \t\r\n3. \tr\r\n',
          13,
          '1',
          '1. foo\r\n2.   1\r\n3. \tr\r\n',
          ['foo', '1', 'r', ''],
        ),
        (
          '-\t\tf._>`[1|\r\n \t\t  \r\n',
          18,
          'b',
          '-\t\tf._>`[1|\r\n \tb\r\n \t\t  \r\n',
          ['  f._>`[1|', 'b', '', ''],
        ),
        // Under literal HTML the text joins none of it: an empty line keeps
        // the HTML apart, and the whitespace stays before the text.
        (
          '<div {\n \r\n',
          8,
          '1',
          '<div {\n\r\n 1\r\n',
          ['<div {', '', '1', ''],
        ),
        (
          '<table>\n\n  <tr>\n\n</table>\n',
          8,
          '1',
          '<table>\n\n1\n  <tr>\n\n</table>\n',
          ['<table>', '', '1', '<tr>', '', '</table>', ''],
        ),
        // A marker right after a quote's `>` left it no optional space:
        // the item's indentation takes one more.
        (
          '>>- one\n>>  |}\n>>\n>>\n  >  > two\n',
          17,
          'a',
          '>>- one\n>>  |}\n>>   a\n>>\n  >  > two\n',
          ['one\n |}\na', '', 'two', ''],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final shellsBefore = shells(session.editor);
        session.act(InsertText(typed), source: edited, rows: rows);
        expect(shells(session.editor), shellsBefore, reason: source);
      }
    });

    test('text typed on an empty row keeps an empty item or fence apart', () {
      // Text typed over an empty item would make the item its setext
      // underline; under a fence with no body and no closing fence it would
      // be the fence's first line of code. A blank line keeps the item, and
      // the fence closes.
      for (final (source, caret, edited, rows) in [
        ('- #### \n\n- ', 8, '- #### \nb\n\n- ', ['', 'b', '', '']),
        ('```\n', 4, '```\n```\nb', ['', 'b']),
        ('~~~~~~\r\n', 8, '~~~~~~\r\n~~~~~~\r\nb', ['', 'b']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const InsertText('b'), source: edited, rows: rows);
        expect(session.editor.document.caretRow.kind, RowKind.paragraph);
      }
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

    test('text typed under a table or literal HTML starts its own block', () {
      // Markdown would read the text as the table's next row or as more of
      // the HTML: an empty line before it keeps it apart, and a paragraph
      // after may take it as its first line. Under a table with no body
      // row it starts that row instead (above).
      for (final (source, caret, edited, rows, at) in [
        (
          '| a |\n| - |\n| b |\n',
          18,
          '| a |\n| - |\n| b |\n\nx',
          ['a ', 'b ', '', 'x'],
          const DisplayPosition(3, 1),
        ),
        (
          '> | a |\n> | - |\n> | b |\n>',
          24,
          '> | a |\n> | - |\n> | b |\n>\n>x',
          ['a ', 'b ', '', 'x'],
          const DisplayPosition(3, 1),
        ),
        (
          '<div>\n\nb',
          6,
          '<div>\n\nx\nb',
          ['<div>', '', 'x\nb'],
          const DisplayPosition(2, 1),
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final shellsBefore = shells(session.editor);
        session.act(
          const InsertText('x'),
          source: edited,
          rows: rows,
          caret: at,
        );
        expect(
          session.editor.document.caretRow.kind,
          RowKind.paragraph,
          reason: source,
        );
        expect(shells(session.editor), shellsBefore, reason: source);
      }
    });

    test('a typed underline or fence keeps the blocks around it', () {
      // A typed `-` or `=` takes a blank line before it rather than
      // underline the paragraph above, and one after it where the block
      // below would read it otherwise: indented code as the content of the
      // empty item `-` starts, a rule as the underline of `=`, a paragraph
      // after a quote as the lazy line of `=`. A typed fence on an item's
      // empty line without its indentation takes it, so the item keeps the
      // blocks after it.
      const p = RowKind.paragraph, b = RowKind.blank, c = RowKind.codeBlock;
      for (final (source, caret, typed, edited, rows, kinds, at) in [
        (
          'Intro\n\n    code',
          6,
          '-',
          'Intro\n\n-\n\n    code',
          ['Intro', '', '-', '', 'code'],
          [p, b, p, b, c],
          const DisplayPosition(2, 1),
        ),
        (
          '> a\n>\nb',
          5,
          '=',
          '> a\n>\n>=\n>\nb',
          ['a', '', '=', '', 'b'],
          [p, b, p, b, p],
          const DisplayPosition(2, 1),
        ),
        (
          'Intro\n\n---',
          6,
          '=',
          'Intro\n\n=\n\n---',
          ['Intro', '', '=', '', ''],
          [p, b, p, b, RowKind.thematicBreak],
          const DisplayPosition(2, 1),
        ),
        (
          '- a\n\n  b',
          4,
          '```',
          '- a\n  ```\n  \n  ```\n  \n  b',
          ['a', '', '', 'b'],
          [p, c, b, p],
          const DisplayPosition(1, 0),
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final last = shellsOf(session.editor.projection.rows.last);
        session.act(InsertText(typed), source: edited, rows: rows, caret: at);
        expect(
          session.editor.projection.rows.map((row) => row.kind),
          kinds,
          reason: source,
        );
        expect(
          shellsOf(session.editor.projection.rows.last),
          last,
          reason: source,
        );
        if (typed == '```') {
          expect(session.editor.document.caretRow.fenced, isTrue);
        }
      }
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

    test('pasted indentation never makes a row code or moves a block', () {
      for (final (source, caret, text, pasted, rows) in [
        // A tab after an item's marker would move its content column past
        // the item nested under it, or make the item's text code.
        (
          '- a\n  - b\n    1) c\n',
          8,
          '\ttabbed ',
          '- a\n  - tabbed b\n    1) c\n',
          ['a', 'tabbed b', 'c', ''],
        ),
        (
          '- a\n  - b\n    1) c\n',
          8,
          '   x',
          '- a\n  - xb\n    1) c\n',
          ['a', 'xb', 'c', ''],
        ),
        // Four spaces at a paragraph's start would make it indented code.
        ('abc\n', 0, '    x', 'xabc\n', ['xabc', '']),
        // Indentation Markdown strips anyway stays in the source.
        ('a\n\nbc\n', 3, '  x', 'a\n\n  xbc\n', ['a', '', 'xbc', '']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(Paste(text), source: pasted, rows: rows);
      }
      // The text's own markup still applies.
      _Session(
        backend,
        source: 'abc\n',
      ).act(const Paste('  # x'), source: '# xabc\n', rows: ['xabc', '']);
      // Four spaces before a setext heading's text would make it code, and
      // its underline a rule.
      _Session(backend, source: 'abc\n---\n').act(
        const Paste('    x'),
        source: 'xabc\n---\n',
        rows: ['xabc', ''],
        caret: const DisplayPosition(0, 1),
      );
      // Text put over the whole of a span that starts an item's line moves
      // its leading whitespace out before the span's delimiters, where the
      // item's content starts: it goes too, so the item's text stays a
      // paragraph and the list nested under it stays nested.
      const nested = '- a\n  - **snippet** more\n    1. child\n';
      final span = _Session(backend, source: nested);
      span.act(const SetSelection(10, 17));
      span.act(
        const Paste('\ttabbed text'),
        source: '- a\n  - **tabbed text** more\n    1. child\n',
        rows: ['a', 'tabbed text more', 'child', ''],
        caret: const DisplayPosition(1, 11),
      );
      expect(span.editor.projection.rows[2].shells, hasLength(6));
      // So does a typed space before a word after the span's opening.
      _Session(backend, source: '- **a**\n\n  b', caret: 4).act(
        const InsertText(' a'),
        source: '- **aa**\n\n  b',
        rows: ['aa', '', 'b'],
        caret: const DisplayPosition(0, 1),
      );
      // Typed or pasted, indentation after a quoted item's marker would
      // leave the item's later paragraph outside it.
      for (final (command, edited, at) in [
        (const InsertText(' a'), '> - aa\n>\n>   b', 1),
        (const Paste('  lead'), '> - leada\n>\n>   b', 4),
      ]) {
        final session = _Session(backend, source: '> - a\n>\n>   b', caret: 4);
        session.act(command, source: edited, caret: DisplayPosition(0, at));
        expect(
          session.editor.projection.rows.last.shells,
          hasLength(3),
          reason: edited,
        );
      }
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
      // A paragraph after the marker would read on as part of the text.
      for (final (source, caret, typed, rows) in [
        ('-\nb', 1, '-a\n\nb', ['-a', '', 'b']),
        ('#\nb', 1, '#a\n\nb', ['#a', '', 'b']),
        ('> -\n> b', 3, '> -a\n>\n> b', ['-a', '', 'b']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          const InsertText('a'),
          source: typed,
          rows: rows,
          caret: const DisplayPosition(0, 2),
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

    test('typed whitespace over a row or in its first span keeps blocks', () {
      // Whitespace typed over all of an item's text, even across its lines,
      // empties the item, and typed inside the hidden syntax that starts a
      // line it goes before that syntax, as marker padding. Either can move
      // the item's content column or its later blocks, so it is refused.
      for (final (source, base, extent) in [
        (
          '  1.  A paragraph\n      with two lines.\n\n          code\n\n'
              '      > A quote.\n',
          6,
          39,
        ),
        ('1.  A paragraph\n    wth two lines.\n\n        code\n', 21, 4),
        ('- foo\n  r|\n\n\t\t*😀(]~1b!', 10, 2),
        (' -    o#]\n -    **p**\n\n     two\n', 18, 18),
        ('* # _a_\n* _a<._\n  > ## b>', 11, 11),
      ]) {
        final session = _Session(backend, source: source, caret: base);
        if (extent != base) session.act(SetSelection(base, extent));
        session.act(const InsertText(' '), applied: false, source: source);
      }
      // Where nothing moves, the whitespace is typed.
      final emptied = _Session(backend, source: '- a b\n- c');
      emptied.act(const SetSelection(2, 5));
      emptied.act(const InsertText(' '), source: '-  \n- c');
      final span = _Session(backend, source: '**p** q', caret: 2);
      span.act(const InsertText(' '), source: ' **p** q', rows: ['p q']);
    });

    test(
      'text put in a table\'s delimiter row shown as its source keeps it',
      () {
        // A replacement or pasted lines that would dissolve the table are
        // refused, as typing and deleting there are; ones that keep it edit
        // the row.
        for (final (source, command) in [
          (
            '| abc | def |\n| --- | --- |\n|\n',
            const ReplaceRange(19, 21, 'r'),
          ),
          (
            '| a:éc | def |\r\n| --- | --- |\r\n',
            const ReplaceRange(16, 17, 'r'),
          ),
          ('! _\n:-', const ReplaceRange(4, 6, 'r')),
          ('| a | b |\n| - | - |', const ReplaceRange(12, 13, '')),
        ]) {
          final session = _Session(backend, source: source);
          session.act(command, applied: false, source: source);
        }
        final pasted = _Session(
          backend,
          source: '| a | b |\n| - | - |',
          caret: 12,
        );
        pasted.act(
          const Paste('- x\n- y'),
          applied: false,
          source: '| a | b |\n| - | - |',
        );
        final kept = _Session(backend, source: '| a | b |\n| -- | - |');
        kept.act(
          const ReplaceRange(12, 13, ''),
          source: '| a | b |\n| - | - |',
          rows: ['a ', 'b ', '| - | - |'],
        );
        kept.act(
          const ReplaceRange(11, 12, ':'),
          source: '| a | b |\n|:- | - |',
          rows: ['a ', 'b ', '|:- | - |'],
        );
        final row = _Session(
          backend,
          source: '| a | b |\n| - | - |',
          caret: 19,
        );
        row.act(
          const Paste('\n| c | d |'),
          source: '| a | b |\n| - | - |\n| c | d |',
          rows: ['a ', 'b ', 'c ', 'd '],
        );
      },
    );

    test('text put in a table cell keeps the cells of its row', () {
      // Whitespace that would indent a row out of its table, a replacement
      // that would let a pipe split the row, and pasted text with a pipe are
      // refused; a typed pipe is escaped as before.
      for (final (source, base, extent, command) in [
        ('| a |\n| - |\n| _\n(b |:', 16, 16, const InsertText('\t')),
        ('| abc \n.!| def |\n| --- | --- |\n', 9, 7, const InsertText('\t')),
        ('| a | b |\n| - | - |\n| c | d |', 24, 24, const Paste('x|y')),
        (
          '| `\\|\\\\|abcé>] | def |\n | --- | --- |\n',
          2,
          2,
          const ReplaceRange(2, 7, 'r'),
        ),
      ]) {
        final session = _Session(backend, source: source, caret: base);
        if (extent != base) session.act(SetSelection(base, extent));
        session.act(command, applied: false, source: source);
      }
      final typed = _Session(
        backend,
        source: '| a | b |\n| - | - |\n| c | d |',
        caret: 24,
      );
      typed.act(
        const InsertText('\t'),
        source: '| a | b |\n| - | - |\n| c \t| d |',
        rows: ['a ', 'b ', 'c \t', 'd '],
      );
      typed.act(
        const ReplaceRange(22, 23, 'x'),
        source: '| a | b |\n| - | - |\n| x \t| d |',
        rows: ['a ', 'b ', 'x \t', 'd '],
      );
    });

    test('text typed in a row\'s unwritten cell keeps the cell before it', () {
      // A row without its closing pipe gets the pipes the cell needs right
      // after its last cell, which shows no new trailing space; after a
      // backslash, which would escape the pipe, a space goes first.
      for (final (source, typed, rows) in [
        (
          '| abc | def |\n| --- | --- |\n!:',
          '| abc | def |\n| --- | --- |\n!:| b|',
          ['abc ', 'def ', '!:', 'b'],
        ),
        (
          '| a | b |\n| - | - |\n| ~baz\n',
          '| a | b |\n| - | - |\n| ~baz| b|\n',
          ['a ', 'b ', '~baz', 'b', ''],
        ),
        (
          '| a | b |\n| - | - |\nc\\',
          '| a | b |\n| - | - |\nc\\ | b|',
          ['a ', 'b ', 'c\\ ', 'b'],
        ),
      ]) {
        final session = _Session(backend, source: source);
        final cell = session.editor.projection.rows.lastWhere(
          (row) => row.kind == RowKind.tableCell,
        );
        session.act(PlaceCaret(cell.index, 0));
        expect(session.editor.selection.tableCell, cell.index);
        session.act(const InsertText('b'), source: typed, rows: rows);
        expect(session.editor.document.caretRow.column, 1);
      }
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
