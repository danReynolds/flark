part of '../journey_test.dart';

void _structureCases(FlarkParseBackend backend) {
  group('structure', () {
    test('return continues a bullet item', () {
      final session = _Session(backend, source: '- one', caret: 5);
      session.act(
        const Newline(),
        source: '- one\n- ',
        rows: ['one', ''],
        caret: const DisplayPosition(1, 0),
      );
      session.act(
        const InsertText('t'),
        source: '- one\n- t',
        rows: ['one', 't'],
        caret: const DisplayPosition(1, 1),
      );
    });

    test('return on an empty item exits the list', () {
      final session = _Session(backend, source: '- one\n- ', caret: 8);
      session.act(
        const Newline(),
        source: '- one\n\n',
        rows: ['one', '', ''],
        caret: const DisplayPosition(2, 0),
      );
      session.act(
        const InsertText('p'),
        source: '- one\n\np',
        rows: ['one', '', 'p'],
      );
      expect(session.editor.projection.rows.last.shells, isEmpty);
    });

    test('ordered items number the next marker', () {
      final session = _Session(backend, source: '1. a', caret: 4);
      session.act(
        const Newline(),
        source: '1. a\n2. ',
        rows: ['a', ''],
        caret: const DisplayPosition(1, 0),
      );
    });

    test('task items continue unchecked', () {
      final session = _Session(backend, source: '- [x] a', caret: 7);
      session.act(
        const Newline(),
        source: '- [x] a\n- [ ] ',
        rows: ['a', ''],
        caret: const DisplayPosition(1, 0),
      );
    });

    test('nested items keep their indentation', () {
      final session = _Session(backend, source: '- a\n  - b', caret: 9);
      session.act(
        const Newline(),
        source: '- a\n  - b\n  - ',
        rows: ['a', 'b', ''],
        caret: const DisplayPosition(2, 0),
      );
    });

    test('return in a quote continues the quote', () {
      final session = _Session(backend, source: '> a', caret: 3);
      session.act(
        const Newline(),
        source: '> a\n> ',
        rows: ['a', ''],
        caret: const DisplayPosition(1, 0),
      );
      session.act(
        const Newline(),
        source: '> a\n\n',
        rows: ['a', '', ''],
        caret: const DisplayPosition(2, 0),
      );
    });

    test(
      'return splits a paragraph with a line break, or a paragraph break',
      () {
        final session = _Session(backend, source: 'ab', caret: 1);
        session.act(
          const Newline(),
          source: 'a\nb',
          rows: ['a\nb'],
          caret: const DisplayPosition(0, 2),
        );
        session.act(const SetSelection.caret(1));
        session.act(
          const Newline(paragraph: true),
          source: 'a\n\n\nb',
          rows: ['a', '', '', 'b'],
          caret: const DisplayPosition(2, 0),
        );
      },
    );

    test('return after a heading gives a plain paragraph', () {
      final session = _Session(backend, source: '# T', caret: 3);
      session.expectState(rows: ['T']);
      session.act(
        const Newline(),
        source: '# T\n',
        rows: ['T', ''],
        caret: const DisplayPosition(1, 0),
      );
      session.act(const InsertText('p'), source: '# T\np', rows: ['T', 'p']);
    });

    test('a dash typed under a paragraph starts a list, not a heading', () {
      final session = _Session(backend, source: 'para', caret: 4);
      session.act(
        const Newline(),
        source: 'para\n',
        rows: ['para', ''],
        caret: const DisplayPosition(1, 0),
      );
      // Alone under a paragraph, `-` would underline a setext heading and
      // hide itself; it gets a block of its own as a bare list marker.
      session.act(
        const InsertText('-'),
        source: 'para\n\n-',
        rows: ['para', '', '-'],
        caret: const DisplayPosition(2, 1),
      );
      session.act(
        const InsertText(' '),
        source: 'para\n\n- ',
        rows: ['para', '', ''],
        caret: const DisplayPosition(2, 0),
      );
      session.act(
        const InsertText('a'),
        source: 'para\n\n- a',
        rows: ['para', '', 'a'],
        caret: const DisplayPosition(2, 1),
      );
      session.act(
        const Undo(),
        source: 'para\n',
        caret: const DisplayPosition(1, 0),
      );
    });

    test('dashes typed under a paragraph make a rule; = stays text', () {
      final rule = _Session(backend, source: 'para\n', caret: 5);
      rule.act(const InsertText('-'), times: 3, source: 'para\n\n---');
      final text = _Session(backend, source: 'para\n', caret: 5);
      text.act(
        const InsertText('='),
        source: 'para\n\n=',
        rows: ['para', '', '='],
        caret: const DisplayPosition(2, 1),
      );
    });

    test('a dash typed under a quoted paragraph stays in the quote', () {
      final session = _Session(backend, source: '> para', caret: 6);
      session.act(const Newline(), source: '> para\n> ');
      session.act(const InsertText('-'), source: '> para\n>\n> -');
      session.act(const InsertText(' '), source: '> para\n>\n> - ');
      session.act(
        const InsertText('a'),
        source: '> para\n>\n> - a',
        rows: ['para', '', 'a'],
        caret: const DisplayPosition(2, 1),
      );
    });

    test('backspace at an item start lifts the marker into a paragraph', () {
      final session = _Session(backend, source: '- one\n- two', caret: 8);
      session.expectState(caret: const DisplayPosition(1, 0));
      session.act(
        const DeleteBackward(),
        source: '- one\n\ntwo',
        rows: ['one', '', 'two'],
        caret: const DisplayPosition(2, 0),
      );
    });

    test('backspace at the first item start removes the list', () {
      final session = _Session(backend, source: '- one', caret: 2);
      session.act(
        const DeleteBackward(),
        source: 'one',
        rows: ['one'],
        caret: const DisplayPosition(0, 0),
      );
    });

    test('backspace at a paragraph start removes the blank row then joins', () {
      final session = _Session(backend, source: 'one\n\ntwo', caret: 5);
      session.act(
        const DeleteBackward(),
        source: 'one\ntwo',
        rows: ['one\ntwo'],
        caret: const DisplayPosition(0, 4),
      );
      session.act(
        const DeleteBackward(),
        source: 'onetwo',
        rows: ['onetwo'],
        caret: const DisplayPosition(0, 3),
      );
    });

    test('delete at a row end joins the next row', () {
      final session = _Session(backend, source: 'one\n\ntwo', caret: 3);
      session.act(
        const DeleteForward(),
        source: 'one\ntwo',
        rows: ['one\ntwo'],
        caret: const DisplayPosition(0, 3),
      );
      session.act(
        const DeleteForward(),
        source: 'onetwo',
        rows: ['onetwo'],
        caret: const DisplayPosition(0, 3),
      );
    });

    test('repeated return then typing leaves one live caret', () {
      final session = _Session(backend, source: 'a', caret: 1);
      session.act(
        const Newline(),
        times: 3,
        source: 'a\n\n\n',
        rows: ['a', '', '', ''],
        caret: const DisplayPosition(3, 0),
      );
      session.act(
        const InsertText('b'),
        source: 'a\n\n\nb',
        rows: ['a', '', '', 'b'],
        caret: const DisplayPosition(3, 1),
      );
    });

    test('a task checkbox toggles from the caret\'s item', () {
      final session = _Session(backend, source: '- [ ] a', caret: 7);
      session.act(const ToggleTask(), source: '- [x] a', rows: ['a']);
      session.act(const ToggleTask(), source: '- [ ] a');
    });

    test('heading level is set and cleared on the caret\'s row', () {
      final session = _Session(backend, source: 'title', caret: 5);
      session.act(
        const SetHeadingLevel(2),
        source: '## title',
        rows: ['title'],
        caret: const DisplayPosition(0, 5),
      );
      session.act(
        const SetHeadingLevel(0),
        source: 'title',
        rows: ['title'],
        caret: const DisplayPosition(0, 5),
      );
    });

    test('backspace joins a quote line into the previous quote line', () {
      final session = _Session(backend, source: '> a\n> b', caret: 6);
      session.expectState(rows: ['a\nb'], caret: const DisplayPosition(0, 2));
      session.act(
        const DeleteBackward(),
        source: '> ab',
        rows: ['ab'],
        caret: const DisplayPosition(0, 1),
      );
    });

    test(
      'indent nests an item under its previous sibling and outdent lifts it back',
      () {
        final session = _Session(backend, source: '- a\n- b\n  c', caret: 6);
        session.act(
          const Indent(),
          source: '- a\n  - b\n    c',
          rows: ['a', 'b\nc'],
          caret: const DisplayPosition(1, 0),
        );
        session.act(
          const Outdent(),
          source: '- a\n- b\n  c',
          rows: ['a', 'b\nc'],
          caret: const DisplayPosition(1, 0),
        );
        session.act(const Outdent(), applied: false);
      },
    );

    test(
      'the first item cannot indent and ordered items nest by their marker width',
      () {
        final session = _Session(backend, source: '1. a\n2. b');
        session.act(const Indent(), applied: false);
        session.act(const SetSelection.caret(8));
        session.act(const Indent(), source: '1. a\n   1. b', rows: ['a', 'b']);
      },
    );

    test('backspace at a nested item start keeps the outer containers', () {
      final nested = _Session(backend, source: '- a\n  - b', caret: 8);
      nested.act(
        const DeleteBackward(),
        source: '- a\n  \n  b',
        rows: ['a', '', 'b'],
        caret: const DisplayPosition(2, 0),
      );
      expect(nested.editor.document.caretRow.shells.map((s) => s.kind), [
        ShellKind.list,
        ShellKind.item,
      ]);
      final quoted = _Session(backend, source: '> - a\n> - b', caret: 10);
      quoted.act(
        const DeleteBackward(),
        source: '> - a\n> \n> b',
        rows: ['a', '', 'b'],
        caret: const DisplayPosition(2, 0),
      );
      expect(quoted.editor.document.caretRow.shells.map((s) => s.kind), [
        ShellKind.blockQuote,
      ]);
      // An item marker that opens an outer item cannot leave a lazy line.
      final parent = _Session(backend, source: '- a\n- - b', caret: 8);
      parent.act(const DeleteBackward(), source: '- a\n- b', rows: ['a', 'b']);
    });

    test('backspace at the first numbered item keeps the rest a list', () {
      // Lifted alone, `2. bar` could not interrupt the paragraph and would be
      // painted as its text; a blank line keeps the list.
      final session = _Session(backend, source: '1. foo\n2. bar', caret: 3);
      session.act(
        const DeleteBackward(),
        source: 'foo\n\n2. bar',
        rows: ['foo', '', 'bar'],
        caret: const DisplayPosition(0, 0),
      );
      session.act(const Undo(), source: '1. foo\n2. bar');
    });

    test('lifting a quoted paragraph above a rule keeps the rule', () {
      final session = _Session(backend, source: '> foo\n---', caret: 2);
      session.act(
        const DeleteBackward(),
        source: 'foo\n\n---',
        rows: ['foo', '', ''],
      );
      expect(session.editor.projection.rows.last.kind, RowKind.thematicBreak);
    });

    test('backspace at the start of code removes the gap, never the fence', () {
      final session = _Session(backend, source: 'a\n\n```\nx\n```', caret: 7);
      session.act(
        const DeleteBackward(),
        source: 'a\n```\nx\n```',
        rows: ['a', 'x'],
        caret: const DisplayPosition(1, 0),
      );
      session.act(const DeleteBackward(), applied: false);
      session.act(const SetSelection.caret(1));
      session.act(const DeleteForward(), applied: false);
    });

    test('no join lands on a closing fence', () {
      const source = '```\nx\n```\nb\n\n# H\n';
      final session = _Session(backend, source: source, caret: 10);
      session.act(const DeleteBackward(), applied: false, source: source);
      session.act(const SetSelection.caret(5));
      session.act(const DeleteForward(), applied: false, source: source);
      // A row that displays nothing still joins, which removes it.
      final blank = _Session(backend, source: '```\nx\n```\n\nb', caret: 10);
      blank.act(
        const DeleteBackward(),
        source: '```\nx\n```\nb',
        rows: ['x', 'b'],
      );
    });

    test('joining after a setext underline or closing sequence keeps it', () {
      for (final (source, caret, backward, joined) in [
        ('H\n===\np', 6, true, 'Hp\n==='),
        ('H\n===\np', 1, false, 'Hp\n==='),
        ('# H #\np', 6, true, '# Hp #'),
        ('# H\n## J ##', 3, false, '# HJ'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          backward ? const DeleteBackward() : const DeleteForward(),
          source: joined,
          rows: [joined.contains('J') ? 'HJ' : 'Hp'],
        );
        expect(session.editor.projection.rows.single.kind, RowKind.heading);
      }
    });

    test('backspace after a rule removes the rule', () {
      final session = _Session(backend, source: '***\nb', caret: 4);
      session.act(
        const DeleteBackward(),
        source: 'b',
        rows: ['b'],
        caret: const DisplayPosition(0, 0),
      );
    });

    test('a join that would make a setext heading refuses', () {
      const source = 'a\n\n---\n\nb';
      final session = _Session(backend, source: source, caret: 1);
      session.act(const DeleteForward(), applied: false, source: source);
    });

    test('delete on the blank line above a heading keeps the heading', () {
      final session = _Session(backend, source: 'a\n\n## J', caret: 2);
      session.act(
        const DeleteForward(),
        source: 'a\n## J',
        rows: ['a', 'J'],
        caret: const DisplayPosition(1, 0),
      );
      expect(session.editor.projection.rows.last.kind, RowKind.heading);
    });

    test(
      'return at the end of a setext or closed heading opens a paragraph',
      () {
        for (final (source, caret, split) in [
          ('H\n===', 1, 'H\n===\n'),
          ('# H #', 3, '# H #\n'),
          ('> H\n> ===', 3, '> H\n> ===\n> '),
        ]) {
          final session = _Session(backend, source: source, caret: caret);
          session.act(const Newline(), source: split);
          session.act(
            const InsertText('x'),
            source: '${split}x',
            rows: ['H', 'x'],
          );
          expect(session.editor.document.caretRow.kind, RowKind.paragraph);
        }
      },
    );

    test('return inside a heading leaves its markup with the first part', () {
      final setext = _Session(backend, source: 'Hello\n===', caret: 3);
      setext.act(
        const Newline(),
        source: 'Hel\n===\nlo',
        rows: ['Hel', 'lo'],
        caret: const DisplayPosition(1, 0),
      );
      final closed = _Session(backend, source: '# Hello #', caret: 5);
      closed.act(
        const Newline(),
        source: '# Hel #\nlo',
        rows: ['Hel', 'lo'],
        caret: const DisplayPosition(1, 0),
      );
    });

    test('return in a quote opened on an item line stays in that item', () {
      for (final marker in ['- ', '1. ']) {
        final source = '$marker> a';
        final pad = ' ' * marker.length;
        final session = _Session(backend, source: source, caret: source.length);
        session.act(const Newline(), source: '$source\n$pad> ');
        session.act(
          const InsertText('x'),
          source: '$source\n$pad> x',
          rows: ['a\nx'],
        );
      }
    });

    test('return in a nested item that opens on its parent line', () {
      final session = _Session(backend, source: '- - a', caret: 5);
      session.act(
        const Newline(),
        source: '- - a\n  - ',
        rows: ['a', ''],
        caret: const DisplayPosition(1, 0),
      );
    });

    test('return after a quoted heading opens a quoted paragraph', () {
      final session = _Session(backend, source: '> # H', caret: 5);
      session.act(const Newline(), source: '> # H\n> ');
      session.act(
        const InsertText('x'),
        source: '> # H\n> x',
        rows: ['H', 'x'],
      );
      expect(session.editor.document.caretRow.kind, RowKind.paragraph);
    });

    test('return in a footnote definition continues that footnote', () {
      final session = _Session(
        backend,
        source: 'x[^1]\n\n[^1]: note',
        caret: 17,
      );
      session.act(const Newline(), source: 'x[^1]\n\n[^1]: note\n    ');
      session.act(
        const InsertText('y'),
        source: 'x[^1]\n\n[^1]: note\n    y',
        rows: ['x[^1]', '', 'note\ny'],
      );
      final list = _Session(backend, source: '[^1]: - a', caret: 9);
      list.act(const Newline(), source: '[^1]: - a\n    - ');
      list.act(const InsertText('b'), rows: ['a', 'b']);
      expect(list.editor.projection.rows.last.shells.map((s) => s.kind), [
        ShellKind.footnoteDefinition,
        ShellKind.list,
        ShellKind.item,
      ]);
    });

    test('return keeps the line ending of a CRLF document', () {
      for (final (source, caret, split) in [
        ('ab\r\ncd', 1, 'a\r\nb\r\ncd'),
        ('- a\r\n- b', 8, '- a\r\n- b\r\n- '),
        ('> a\r\n> b', 8, '> a\r\n> b\r\n> '),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const Newline(), source: split);
      }
    });
  });
}
