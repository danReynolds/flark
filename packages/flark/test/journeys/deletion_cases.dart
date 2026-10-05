part of '../journey_test.dart';

/// The shell kinds of every row, outermost first.
List<String> _shellsOf(FlarkEditor editor) => [
  for (final row in editor.projection.rows)
    row.shells.map((shell) => shell.kind.name).join('/'),
];

void _deletionCases(FlarkParseBackend backend) {
  group('deletion', () {
    test('deleting at the document\'s ends does nothing quietly', () {
      // Nothing precedes the first row or follows the last: the key does
      // nothing, with no refusal for a host to report. Hidden markup beside
      // the caret is no grapheme of its own: a closing `**` or `#`, a fence,
      // a cell's pipe, a link's brackets.
      for (final (source, caret, command) in [
        ('abc', 0, const DeleteBackward() as FlarkCommand),
        ('abc', 3, const DeleteForward()),
        ('', 0, const DeleteBackward()),
        ('a\n\nb', 4, const DeleteForward()),
        ('Hello **wo**', 10, const DeleteForward()),
        ('# Title #', 7, const DeleteForward()),
        ('```\ncode\n```', 8, const DeleteForward()),
        ('| a |\n| - |\n| b |', 16, const DeleteForward()),
        ('| a |\n| - |', 11, const DeleteForward()),
        ('**ab** c', 2, const DeleteBackward()),
        ('[a](u) b', 1, const DeleteBackward()),
        ('```\ncode\n```', 4, const DeleteBackward()),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, applied: false, source: source);
        expect(session.editor.lastRejection, isNull, reason: source);
      }
      // At the start of the first row a heading's marker, indentation or a
      // container still lifts.
      for (final (source, caret, lifted) in [
        ('# Title #', 2, 'Title'),
        ('    code', 4, 'code'),
        ('> a', 2, 'a'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const DeleteBackward(), source: lifted);
      }
    });

    test('emptying an item under a paragraph keeps the item apart from it', () {
      // An empty item cannot interrupt a paragraph: left as it is, `1. `
      // would show as the paragraph's text and `- ` would underline it. A
      // blank line in the outer containers goes before the emptied item.
      for (final (source, base, extent, command, emptied, rows) in [
        (
          'Steps:\n1. one\n',
          13,
          13,
          const DeleteBackward(word: true),
          'Steps:\n\n1. \n',
          ['Steps:', '', '', ''],
        ),
        (
          'Steps:\n- one\n- two\n',
          12,
          12,
          const DeleteBackward(word: true),
          'Steps:\n\n- \n- two\n',
          ['Steps:', '', '', 'two', ''],
        ),
        (
          'Steps:\n- one\n- two\n',
          9,
          12,
          const DeleteBackward(),
          'Steps:\n\n- \n- two\n',
          ['Steps:', '', '', 'two', ''],
        ),
        ('x\n- y', 5, 5, const DeleteBackward(), 'x\n\n- ', ['x', '', '']),
        ('x\n+ y', 4, 4, const DeleteForward(), 'x\n\n+ ', ['x', '', '']),
        (
          'Steps:\r\n- one',
          13,
          13,
          const DeleteBackward(word: true),
          'Steps:\r\n\r\n- ',
          ['Steps:', '', ''],
        ),
        (
          '> Steps:\n> - one',
          16,
          16,
          const DeleteBackward(word: true),
          '> Steps:\n>\n> - ',
          ['Steps:', '', ''],
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(base, extent));
        final shells = _shellsOf(session.editor);
        session.act(
          command,
          source: emptied,
          rows: rows,
          caret: const DisplayPosition(2, 0),
        );
        final now = session.editor.projection.rows;
        expect(now.first.kind, RowKind.paragraph, reason: source);
        expect(_shellsOf(session.editor).first, shells.first, reason: source);
        expect(_shellsOf(session.editor)[2], shells[1], reason: source);
        session.act(const Undo(), source: source);
      }
      // The next character types into the kept item.
      final typed = _Session(backend, source: 'Steps:\n1. one\n', caret: 13);
      typed.act(const DeleteBackward(word: true));
      typed.act(
        const InsertText('x'),
        source: 'Steps:\n\n1. x\n',
        rows: ['Steps:', '', 'x', ''],
        caret: const DisplayPosition(2, 1),
      );
      // So does an item nested under an item's paragraph.
      final nested = _Session(backend, source: '+ B\n\t+ V', caret: 8);
      nested.act(
        const DeleteBackward(),
        source: '+ B\n\n\t+ ',
        rows: ['B', '', ''],
        caret: const DisplayPosition(2, 0),
      );
      expect(_shellsOf(nested.editor).last, 'list/item/list/item');
    });

    test('a deletion that leaves an underline under a paragraph keeps it', () {
      // As with a typed underline, the line it leaves would make the
      // paragraph a heading and hide itself; a blank line keeps it apart.
      for (final (source, caret, deleted, rows) in [
        ('a\n-b', 4, 'a\n\n-', ['a', '', '-']),
        ('Foo\n= =', 7, 'Foo\n\n= ', ['Foo', '', '= ']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const DeleteBackward(), source: deleted, rows: rows);
        expect(session.editor.projection.rows.first.kind, RowKind.paragraph);
      }
    });

    test('emptying an item\'s first line keeps its later blocks in it', () {
      // An item can start with at most one blank line: the blank line after
      // its emptied first line would end it, and the blocks after that line
      // would leave it, or turn into indented code that shows its markers.
      for (final (source, base, extent, emptied, rows) in [
        (
          '- Title\n\n  Details here.\n',
          2,
          7,
          '- \n  Details here.\n',
          ['', 'Details here.', ''],
        ),
        ('* Steps\n\n    1. one\n', 2, 7, '* \n    1. one\n', ['', 'one', '']),
        ('- a\n\n  b', 3, 3, '- \n  b', ['', 'b']),
        (
          'Steps:\n- Title\n\n  Details',
          9,
          14,
          'Steps:\n\n- \n  Details',
          ['Steps:', '', '', 'Details'],
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(base, extent));
        final last = session.editor.projection.rows.lastWhere(
          (row) => row.text.isNotEmpty,
        );
        session.act(const DeleteBackward(), source: emptied, rows: rows);
        final now = session.editor.projection.rows.lastWhere(
          (row) => row.text.isNotEmpty,
        );
        expect(now.kind, last.kind, reason: source);
        expect(
          now.shells.map((s) => s.kind),
          last.shells.map((s) => s.kind),
          reason: source,
        );
      }
    });

    test('deleting an item\'s first word keeps the item\'s content column', () {
      // The space after the word goes with it. Left as marker padding, it
      // would move the item's content column, and blocks indented for the
      // old column would leave the item or stop being code.
      for (final (source, base, extent, command, deleted, rows) in [
        (
          '- The plan\n  + detail\n',
          2,
          5,
          const DeleteBackward(),
          '- plan\n  + detail\n',
          ['plan', 'detail', ''],
        ),
        (
          '- The plan\n  + detail\n',
          5,
          5,
          const DeleteBackward(word: true),
          '- plan\n  + detail\n',
          ['plan', 'detail', ''],
        ),
        (
          '- **The plan**\n  + detail',
          4,
          7,
          const DeleteForward(),
          '- **plan**\n  + detail',
          ['plan', 'detail'],
        ),
        (
          '1.  a b\n\n        c',
          5,
          5,
          const DeleteBackward(),
          '1.  b\n\n        c',
          ['b', '', 'c'],
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(base, extent));
        final kinds = session.editor.projection.rows.map((r) => r.kind);
        final shells = _shellsOf(session.editor);
        session.act(
          command,
          source: deleted,
          rows: rows,
          caret: const DisplayPosition(0, 0),
        );
        expect(
          session.editor.projection.rows.map((r) => r.kind),
          kinds,
          reason: source,
        );
        expect(_shellsOf(session.editor), shells, reason: source);
      }
    });

    test('emptying a cell of a row without its outer pipe keeps the table', () {
      // A row written without its leading or trailing pipe loses that cell
      // with its text: the header would no longer match the delimiter row,
      // and a body row would end the table or leave the caret in another
      // cell. The emptied cell keeps a pipe, and the caret stays in it.
      for (final (source, base, extent, command, deleted, cells, caret) in [
        (
          'Name | Value\n--- | ---\nfoo | 1',
          0,
          4,
          const DeleteBackward(),
          '| | Value\n--- | ---\nfoo | 1',
          ['', 'Value', 'foo ', '1'],
          const DisplayPosition(0, 0),
        ),
        (
          'D|I\n-|-',
          1,
          1,
          const DeleteBackward(),
          '||I\n-|-',
          // The editing view shows a header-only table's delimiter row.
          ['', 'I', '-|-'],
          const DisplayPosition(0, 0),
        ),
        (
          'D|I\n-|-',
          3,
          3,
          const DeleteBackward(),
          'D||\n-|-',
          ['D', '', '-|-'],
          const DisplayPosition(1, 0),
        ),
        (
          'a\n|-',
          1,
          1,
          const DeleteBackward(),
          '||\n|-',
          ['', '|-'],
          const DisplayPosition(0, 0),
        ),
        (
          'a\n|-\n|b',
          7,
          7,
          const DeleteBackward(),
          'a\n|-\n||',
          ['a', ''],
          const DisplayPosition(1, 0),
        ),
        (
          '||`\n-|-\nD',
          9,
          9,
          const DeleteBackward(word: true),
          '||`\n-|-\n||',
          ['', '`', '', ''],
          const DisplayPosition(2, 0),
        ),
        (
          'a|b\n-|-\nbar | baz',
          17,
          17,
          const DeleteBackward(word: true),
          'a|b\n-|-\nbar | |',
          ['a', 'b', 'bar ', ''],
          const DisplayPosition(3, 0),
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(base, extent));
        session.act(command, source: deleted, rows: cells, caret: caret);
        expect(
          session.editor.projection.rows.every(
            (row) => row.kind == RowKind.tableCell,
          ),
          isTrue,
          reason: source,
        );
      }
      final typed = _Session(backend, source: 'Name | Value\n--- | ---\n');
      typed.act(const SetSelection(0, 4));
      typed.act(const DeleteBackward());
      typed.act(
        const InsertText('N'),
        source: '| N| Value\n--- | ---\n',
        caret: const DisplayPosition(0, 1),
      );
    });

    test('a deletion in a cell shows none of its row\'s other source', () {
      // A backslash a deletion leaves before the cell's closing pipe would
      // escape it, painting the pipe as the cell's text: refused, as a typed
      // backslash there is. An emptied first cell of a row without its
      // leading pipe would make that pipe lead the row, showing the cell
      // the table drops: the emptied cell keeps a pipe of its own instead.
      for (final (source, caret, command) in [
        ('| a |\n| - |\n| #\\ |', 16, const DeleteForward()),
        ('\n| a#}<\\\t|\n| - |\n| b|', 9, const DeleteBackward()),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, applied: false, source: source);
      }
      for (final (source, caret, command, deleted, rows) in [
        (
          '| a |\n| - |\n| b|\nc\n~|~~.\n',
          20,
          const DeleteBackward(),
          '| a |\n| - |\n| b|\nc\n||~~.\n',
          ['a ', 'b', 'c', '', ''],
        ),
        (
          '| a{ |\n| - |\n| -)_\nb |bbb\n',
          19,
          const DeleteForward(),
          '| a{ |\n| - |\n| -)_\n| |bbb\n',
          ['a{ ', '-)_', '', ''],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, source: deleted, rows: rows);
      }
    });

    test('deleting all of a line\'s text in a row removes the line', () {
      // Left blank, the line would end a setext heading above its underline,
      // which would then be painted, or move a lazy line's container.
      for (final (source, caret, command, deleted, rows, at) in [
        (
          'a\nb\n-',
          3,
          const DeleteBackward(),
          'a\n-',
          ['a'],
          const DisplayPosition(0, 1),
        ),
        (
          'a\nb\n===',
          3,
          const DeleteBackward(),
          'a\n===',
          ['a'],
          const DisplayPosition(0, 1),
        ),
        (
          '> a\nb',
          5,
          const DeleteBackward(),
          '> a',
          ['a'],
          const DisplayPosition(0, 1),
        ),
        (
          '  1.  h\nwith two lines.\n\n          code',
          6,
          const DeleteForward(),
          '  1.  with two lines.\n\n          code',
          ['with two lines.', '', 'code'],
          const DisplayPosition(0, 0),
        ),
        // A first line emptied goes too: left blank, the line after it
        // would read as indented code and the underline as text.
        (
          'a\n    b\n===',
          1,
          const DeleteBackward(),
          'b\n===',
          ['b'],
          const DisplayPosition(0, 0),
        ),
        (
          '*a\n    b*\n===',
          2,
          const DeleteBackward(),
          '*b*\n===',
          ['b'],
          const DisplayPosition(0, 0),
        ),
        (
          'a\n    b',
          1,
          const DeleteBackward(),
          'b',
          ['b'],
          const DisplayPosition(0, 0),
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final kinds = session.editor.projection.rows
            .where((row) => row.text.isNotEmpty)
            .map((row) => (row.kind, row.headingLevel, row.shells.length));
        session.act(command, source: deleted, rows: rows, caret: at);
        expect(
          session.editor.projection.rows
              .where((row) => row.text.isNotEmpty)
              .map((row) => (row.kind, row.headingLevel, row.shells.length)),
          kinds,
          reason: source,
        );
      }
    });

    test('text deleted from a definition above a setext heading joins it', () {
      // The definition's rest reads as the heading's first line; the
      // underline stays hidden and nothing becomes code.
      final session = _Session(backend, source: '[a]: /u\nb\n===', caret: 4);
      session.act(
        const DeleteBackward(word: true),
        source: ' /u\nb\n===',
        rows: ['/u\nb'],
        caret: const DisplayPosition(0, 0),
      );
      expect(session.editor.projection.rows.single.kind, RowKind.heading);
    });

    test('a word deleted over a whole span takes its delimiters', () {
      // The word before the caret is all of the span's text and the space
      // after it. Its start lies past the span's hidden opening syntax,
      // which deleting the word must not strand: the span goes whole, as
      // deleting all of an owner's text takes its delimiters.
      for (final (source, caret, deleted, rows) in [
        ('a **bold** ', 11, 'a ', ['a ']),
        ('see `draft` now', 12, 'see now', ['see now']),
        ('an ![icon](i.png) here', 18, 'an here', ['an here']),
        ('- *one* two', 8, '- two', ['two']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          const DeleteBackward(word: true),
          source: deleted,
          rows: rows,
        );
        session.act(const Undo(), source: source);
      }
    });

    test('an empty quoted line whose marker cannot go alone goes whole', () {
      // Without its `>`, the empty line between two items of the quoted
      // list would end the quote, and `- c` would be indented code there.
      // The line goes whole instead, as an empty line without a prefix
      // joins the row before it, and the caret ends that row.
      final session = _Session(
        backend,
        source: '> 1) a\n>    - b\n>\n>    - c\n',
        caret: 17,
      );
      final shells = _shellsOf(session.editor)..removeAt(2);
      session.act(
        const DeleteBackward(),
        source: '> 1) a\n>    - b\n>    - c\n',
        rows: ['a', 'b', 'c', ''],
        caret: const DisplayPosition(1, 1),
      );
      expect(_shellsOf(session.editor), shells);
    });

    test('a deletion that would move another block refuses', () {
      // No spelling of the emptied `#` line keeps `[` out of the item: a
      // blank line lets the item go on over it, and without one `[` reads
      // on lazily. The `#` stays, as joins that would move a block do.
      final session = _Session(backend, source: '- b\n#\n  [', caret: 5);
      session.act(
        const DeleteBackward(),
        applied: false,
        source: '- b\n#\n  [',
        caret: const DisplayPosition(1, 1),
      );
    });

    test('a join or lift that would move a later block refuses', () {
      // The block that moves can be rows away from the join: the gap after
      // the joined heading is what kept `c` out of the item.
      for (final (source, caret, command) in <(String, int, FlarkCommand)>[
        ('- # a\nb\n\n  c', 5, const DeleteForward()),
        ('* a\n# b\nc', 3, const DeleteForward()),
        ('- a\n-\n\n  b', 5, const DeleteBackward()),
        ('1. a\n 2. b\n\n   c', 4, const DeleteForward()),
        ('- a\n- ```\nb', 9, const DeleteBackward()),
        ('    >a\nb', 4, const DeleteBackward()),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final rows = session.editor.projection.rows.map((r) => r.text);
        session.act(command, applied: false, source: source);
        expect(
          session.editor.projection.rows.map((r) => r.text),
          rows,
          reason: source,
        );
      }
    });

    test('a heading made a paragraph keeps the next line out of it', () {
      // `b` would read on lazily into the quote's new paragraph; a blank
      // line in the quote keeps it apart. Text that would read as a
      // container of its own (`>`) keeps its heading.
      for (final command in [
        const DeleteBackward(),
        const SetHeadingLevel(0),
      ]) {
        final session = _Session(backend, source: '> # a\nb', caret: 4);
        session.act(
          command,
          source: '> a\n>\nb',
          rows: ['a', '', 'b'],
          caret: const DisplayPosition(0, 0),
        );
        expect(_shellsOf(session.editor), ['blockQuote', 'blockQuote', '']);
      }
      final quote = _Session(backend, source: '# >', caret: 2);
      quote.act(const SetHeadingLevel(0), applied: false, source: '# >');
    });

    test(
      'deleting a span\'s first word keeps the next line\'s quote marker',
      () {
        // Whitespace that leads an emphasis after its first word is deleted
        // moves out of it; past a line break, the opening syntax goes after
        // the next line's container prefix rather than before it.
        final session = _Session(backend, source: '>*p\n>}*', caret: 3);
        session.act(
          const DeleteBackward(),
          source: '>\n>*}*',
          rows: ['', '}'],
          caret: const DisplayPosition(1, 0),
          context: Style.emphasis,
        );
        expect(_shellsOf(session.editor), ['blockQuote', 'blockQuote']);
      },
    );
  });
}
