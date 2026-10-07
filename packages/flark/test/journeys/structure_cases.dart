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

    test('return leaves a quote, item or footnote that ends with a list', () {
      // comrak continues the inner item over the blank lines that close it,
      // without its indentation. Each Return leaves one container: the item,
      // then the one around it, rather than opening another item forever.
      for (final (source, item, exit, left, shells) in [
        (
          '> - a',
          '> - a\n> - ',
          '> - a\n> \n> ',
          '> - a\n> \n\n',
          <ShellKind>[],
        ),
        (
          '> 1. a',
          '> 1. a\n> 2. ',
          '> 1. a\n> \n> ',
          '> 1. a\n> \n\n',
          <ShellKind>[],
        ),
        (
          '> > - a',
          '> > - a\n> > - ',
          '> > - a\n> > \n> > ',
          '> > - a\n> > \n> \n> ',
          [ShellKind.blockQuote],
        ),
        (
          '- - a',
          '- - a\n  - ',
          '- - a\n  \n  ',
          '- - a\n  \n\n',
          <ShellKind>[],
        ),
        (
          '[^1]: - a',
          '[^1]: - a\n    - ',
          '[^1]: - a\n    \n    ',
          '[^1]: - a\n    \n\n',
          <ShellKind>[],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: source.length);
        session.act(
          const Newline(),
          source: item,
          rows: ['a', ''],
          caret: const DisplayPosition(1, 0),
        );
        session.act(
          const Newline(),
          source: exit,
          rows: ['a', '', ''],
          caret: const DisplayPosition(2, 0),
        );
        session.act(
          const Newline(),
          source: left,
          rows: ['a', '', '', ''],
          caret: const DisplayPosition(3, 0),
        );
        expect(
          session.editor.document.caretRow.shells.map((s) => s.kind),
          shells,
          reason: source,
        );
        session.act(
          const InsertText('b'),
          source: '${left}b',
          rows: ['a', '', '', 'b'],
          caret: const DisplayPosition(3, 1),
        );
        expect(
          session.editor.document.caretRow.shells.map((s) => s.kind),
          shells,
          reason: source,
        );
      }
      // Backspace on that line lifts the same prefix, as on any empty quote
      // line, and then removes the line.
      final back = _Session(backend, source: '> - a\n> \n> ', caret: 11);
      back.act(
        const DeleteBackward(),
        source: '> - a\n> \n',
        rows: ['a', '', ''],
        caret: const DisplayPosition(2, 0),
      );
      back.act(
        const DeleteBackward(),
        source: '> - a\n> ',
        rows: ['a', ''],
        caret: const DisplayPosition(1, 0),
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

    test('a task toggle off any task item does nothing quietly', () {
      // There is no checkbox to toggle: nothing to do, which a host does not
      // report as a refused edit.
      for (final (source, caret) in [('a', 1), ('- a', 3), ('> b', 3)]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const ToggleTask(), applied: false, source: source);
        expect(session.editor.lastRejection, isNull, reason: source);
      }
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

    test('clearing the level of a line that is no heading does nothing', () {
      // An empty line, like a paragraph, is at level 0 already: the command
      // is inert, with no refusal for a host to report.
      for (final (source, caret) in [
        ('a\n\nb', 2),
        ('- a\n\n  b', 4),
        ('a', 1),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const SetHeadingLevel(0), applied: false, source: source);
        expect(session.editor.lastRejection, isNull, reason: source);
      }
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

    test(
      'outdent lifts an item that opens on its parent\'s line onto a line of its own',
      () {
        // Merging the markers (`-- a`) would paint them as text.
        final session = _Session(backend, source: '- - a\n    b', caret: 4);
        session.act(
          const Outdent(),
          source: '-\n- a\n  b',
          rows: ['', 'a\nb'],
          caret: const DisplayPosition(1, 0),
        );
        expect(session.editor.document.caretRow.shells.map((s) => s.kind), [
          ShellKind.list,
          ShellKind.item,
        ]);
        final ordered = _Session(backend, source: '1. 2. ', caret: 6);
        ordered.act(const Outdent(), source: '1.\n2. ', rows: ['', '']);
      },
    );

    test('outdent refuses when the emptied parent would show its marker', () {
      // Alone in its list, a bare `-` is presented as the text it is.
      final session = _Session(backend, source: '- 1. ', caret: 5);
      session.act(const Outdent(), applied: false, source: '- 1. ');
    });

    test('outdent shifts tab-indented lines by the columns they show', () {
      // Two columns of the tab go, not the one space before the item's
      // column, so the code keeps its own indentation.
      final code = _Session(
        backend,
        source: '- a\n  - b\n\n\t      code',
        caret: 8,
      );
      code.act(
        const Outdent(),
        source: '- a\n- b\n\n        code',
        rows: ['a', 'b', '', '  code'],
      );
      // An item that opens on its parent's line keeps a tab-indented child.
      final child = _Session(backend, source: '- -\n\tz', caret: 3);
      child.act(const Outdent(), source: '-\n-\n  z', rows: ['', '', 'z']);
      expect(child.editor.projection.rows.last.shells.length, 2);
    });

    test('outdent lifts an item nested past its parent\'s content to the '
        'parent\'s column', () {
      // Four spaces or a tab nest an item past its parent's content offset.
      // Lifted by that offset it stayed nested, so the shift was refused and
      // Shift-Tab did nothing.
      for (final (source, caret, outdented, rows) in [
        ('- a\n    - b', 10, '- a\n- b', ['a', 'b']),
        ('- a\n\t- b', 7, '- a\n- b', ['a', 'b']),
        ('> - a\n>     - b', 14, '> - a\n> - b', ['a', 'b']),
        ('- a\n    - b\n      c', 10, '- a\n- b\n  c', ['a', 'b\nc']),
        ('- a\n    - b\n        - c', 10, '- a\n- b\n    - c', ['a', 'b', 'c']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final depth = session.editor.document.caretRow.shells.length;
        session.act(
          const Outdent(),
          source: outdented,
          rows: rows,
          caret: const DisplayPosition(1, 0),
        );
        expect(
          session.editor.document.caretRow.shells.length,
          depth - 2,
          reason: source,
        );
      }
      // The item's own child moves with it and stays its child.
      final child = _Session(
        backend,
        source: '- a\n    - b\n        - c',
        caret: 10,
      )..act(const Outdent());
      expect(child.editor.projection.rows.last.shells.length, 4);
    });

    test('indent refuses when it would move a block after the item', () {
      // A sibling short of the nested item's column would nest with it.
      final sibling = _Session(backend, source: '- a\n - b\n  - c', caret: 8);
      sibling.act(const Indent(), applied: false, source: '- a\n - b\n  - c');
      // The first item would no longer end at its empty line, and the code
      // after the list would become its paragraph.
      final code = _Session(backend, source: '-\n-\n\n    a', caret: 3);
      code.act(const Indent(), applied: false, source: '-\n-\n\n    a');
    });

    test('indent refuses to paint a marker the item above would take', () {
      // An HTML block in the previous item reads the marker as its text.
      final html = _Session(backend, source: '- <v>\n-', caret: 7);
      html.act(const Indent(), applied: false, source: '- <v>\n-');
      // An empty item cannot interrupt a paragraph: nested under one, its
      // marker would underline the paragraph as a heading.
      final empty = _Session(backend, source: '- a\n- ', caret: 6);
      empty.act(const Indent(), applied: false, source: '- a\n- ');
      // Tab that cannot indent does nothing, with no reason a host would
      // show: source mode is no way to indent the item.
      expect(empty.editor.lastRejection, isNull);
      final first = _Session(backend, source: '- a', caret: 3);
      first.act(const Indent(), applied: false, source: '- a');
      expect(first.editor.lastRejection, isNull);
    });

    test(
      'a heading level on an empty line keeps the pending style for its text',
      () {
        final session = _Session(backend, source: 'a\n\n\nb', caret: 3);
        session.act(
          const ToggleStyle(Style.strikethrough),
          context: Style.strikethrough,
        );
        session.act(
          const SetHeadingLevel(3),
          source: 'a\n\n### \nb',
          rows: ['a', '', '', 'b'],
          context: Style.strikethrough,
        );
        session.act(
          const InsertText('x'),
          source: 'a\n\n### ~~x~~\nb',
          rows: ['a', '', 'x', 'b'],
        );
        expect(session.editor.document.caretRow.headingLevel, 3);
      },
    );

    test('a heading level on an empty line keeps it in its containers', () {
      // An item runs on over an empty line without its indentation, which
      // the heading takes.
      final item = _Session(backend, source: '- a\n\n  b', caret: 4);
      item.act(
        const SetHeadingLevel(2),
        source: '- a\n  ## \n  b',
        rows: ['a', '', 'b'],
      );
      expect(item.editor.projection.rows.map((r) => r.shells.length), [
        2,
        2,
        2,
      ]);
      // An empty item's marker is spaced from the heading's.
      final empty = _Session(backend, source: '-\n      a', caret: 1);
      empty.act(
        const SetHeadingLevel(4),
        source: '- #### \n      a',
        rows: ['', 'a'],
      );
      expect(empty.editor.projection.rows.map((r) => r.kind), [
        RowKind.heading,
        RowKind.codeBlock,
      ]);
    });

    test(
      'a heading level on an empty line drops indentation and leaves HTML apart',
      () {
        final tab = _Session(backend, source: 'a\n\n\t', caret: 4);
        tab.act(
          const SetHeadingLevel(2),
          source: 'a\n\n## ',
          rows: ['a', '', ''],
        );
        expect(tab.editor.document.caretRow.headingLevel, 2);
        // An HTML block runs on to an empty line, so the heading takes the
        // line after this one.
        final html = _Session(backend, source: '<v>\n\n_', caret: 4);
        html.act(
          const SetHeadingLevel(1),
          source: '<v>\n\n# \n_',
          rows: ['<v>', '', '', '_'],
        );
        expect(html.editor.document.caretRow.kind, RowKind.heading);
      },
    );

    test(
      'a heading level on a paragraph with a lazy line keeps the rest in its item',
      () {
        final session = _Session(backend, source: '- a\nb\n\n  c', caret: 2);
        session.act(
          const SetHeadingLevel(1),
          source: '- # a\n  b\n\n  c',
          rows: ['a', 'b', '', 'c'],
          caret: const DisplayPosition(0, 0),
        );
        expect(session.editor.projection.rows.map((r) => r.shells.length), [
          2,
          2,
          2,
          2,
        ]);
        // A heading is the paragraph's first line, so the level refuses from
        // a later one, and where it would cut a span in two.
        final later = _Session(backend, source: '- b\nc\n- ', caret: 4);
        later.act(const SetHeadingLevel(1), applied: false);
        final span = _Session(backend, source: '*a\nb*', caret: 1);
        span.act(const SetHeadingLevel(1), applied: false);
      },
    );

    test('a heading level heads a first line whose text shows a line feed', () {
      // `&#10;` displays a line feed inside the paragraph's first line; the
      // paragraph's own line break is the one after it.
      final session = _Session(backend, source: 'a&#10;b\nc', caret: 0);
      session.act(
        const SetHeadingLevel(1),
        source: '# a&#10;b\nc',
        rows: ['a\nb', 'c'],
      );
    });

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
      // A row that displays nothing still joins, which removes it. The
      // caret stays at the end of the code, not past the hidden closer.
      final blank = _Session(backend, source: '```\nx\n```\n\nb', caret: 10);
      blank.act(
        const DeleteBackward(),
        source: '```\nx\n```\nb',
        rows: ['x', 'b'],
        caret: const DisplayPosition(0, 1),
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

    test('joining into an empty closed heading keeps its sequence hidden', () {
      // An empty heading's text starts where its closing sequence does: the
      // spaces between them separate the opening marker. Joined text goes
      // where typed text would, before the last of several spaces, or after
      // a single one with a space of its own before the sequence, as text in
      // a heading has, rather than running into it and painting it.
      for (final (source, caret, backward, joined, text) in [
        ('# #\n## J ##', 2, false, '# J #', 'J'),
        ('#  ###\r\n##### foo ##', 3, false, '# foo ###', 'foo'),
        ('#####  ##\n#boo', 10, true, '##### #boo ##', '#boo'),
        ('###  ###   \r\n#f', 13, true, '### #f ###   ', '#f'),
        (' # ###\nbar', 3, false, ' # bar ###', 'bar'),
        ('##### #\r\n[foo](/url)', 6, false, '##### [foo](/url) #', 'foo'),
        ('-  ##  #\n#o', 7, false, '-  ## #o #', '#o'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          backward ? const DeleteBackward() : const DeleteForward(),
          source: joined,
          rows: [text],
          caret: const DisplayPosition(0, 0),
        );
        expect(session.editor.projection.rows.single.kind, RowKind.heading);
      }
    });

    test('typing into an empty closed heading keeps its sequence hidden', () {
      // A cleared title is retyped against the closing sequence. The text
      // goes before the last of the spaces that separate them, or after a
      // single one with a space of its own, as joined text does, and the
      // caret stays after the text, so the title goes on from there.
      final session = _Session(backend, source: '# foo #', caret: 5);
      session.act(const DeleteBackward(), times: 3, source: '#  #', rows: ['']);
      session.act(
        const InsertText('b'),
        source: '# b #',
        rows: ['b'],
        caret: const DisplayPosition(0, 1),
      );
      session.act(
        const InsertText('a'),
        source: '# ba #',
        rows: ['ba'],
        caret: const DisplayPosition(0, 2),
      );
      // Paste, a collapsed replacement and composed text land the same way.
      // One host composes with InsertText, another replaces its preedit
      // where the kernel reports it landed, which can be before the caret.
      for (final (source, caret, placed) in [
        ('#  ###', 3, '# ab ###'),
        ('- # #', 4, '- # ab #'),
      ]) {
        final pasted = _Session(backend, source: source, caret: caret);
        pasted.act(const Paste('ab'), source: placed, rows: ['ab']);
        final replaced = _Session(backend, source: source, caret: caret);
        replaced.act(
          ReplaceRange(caret, caret, 'ab'),
          source: placed,
          rows: ['ab'],
        );
        final typed = _Session(backend, source: source, caret: caret);
        typed.editor.beginComposition();
        typed.act(const InsertText('a'));
        typed.act(const InsertText('b'));
        typed.editor.commitComposition();
        final preedit = _Session(backend, source: source, caret: caret);
        preedit.editor.beginComposition();
        preedit.act(ReplaceRange(caret, caret, 'a'));
        final landed = preedit.editor.selection.extent;
        preedit.act(ReplaceRange(landed - 1, landed, 'ab'));
        preedit.editor.commitComposition();
        for (final composed in [typed, preedit]) {
          composed.expectState(
            source: placed,
            rows: ['ab'],
            caret: const DisplayPosition(0, 2),
          );
          composed.act(const Undo(), source: source);
        }
      }
    });

    test('typing then erasing in an empty closed heading restores it', () {
      // The last of the spaces before the sequence separates the typed text
      // from it, so erasing the text gives back the same spaces instead of
      // leaving one more hidden space each time.
      for (final (source, caret, typed) in [
        ('#  #', 3, '# b #'),
        ('#   ###', 4, '#  b ###'),
        ('- ##\t\t#', 6, '- ##\tb\t#'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        for (var i = 0; i < 3; i++) {
          session.act(
            const InsertText('b'),
            source: typed,
            rows: ['b'],
            caret: const DisplayPosition(0, 1),
          );
          session.act(
            const DeleteBackward(),
            source: source,
            rows: [''],
            caret: const DisplayPosition(0, 0),
          );
        }
      }
      // After a single space the text takes one of its own; erased, it leaves
      // two, which then stay as they are.
      final single = _Session(backend, source: '# #', caret: 2);
      for (var i = 0; i < 2; i++) {
        single.act(const InsertText('b'), source: '# b #', rows: ['b']);
        single.act(const DeleteBackward(), source: '#  #', rows: ['']);
      }
      // Text joined after several spaces is placed the same way.
      final joined = _Session(backend, source: '#  #\nbar', caret: 3);
      joined.act(
        const DeleteForward(),
        source: '# bar #',
        rows: ['bar'],
        caret: const DisplayPosition(0, 0),
      );
    });

    test('a space typed before a heading\'s last `#` leaves it text', () {
      // After whitespace, a `#` that ends a heading's text would read as its
      // closing sequence: hidden, with the caret back before the space, so
      // the next word would go there. Escaped, the `#` stays the heading's
      // text, as a pipe typed in a cell does, and the word follows the space.
      for (final (source, caret, column, spaced, typed, shown) in [
        ('# alpha#', 7, 6, r'# alpha \#', r'# alpha b\#', 'alpha b#'),
        ('# alpha##', 7, 6, r'# alpha \##', r'# alpha b\##', 'alpha b##'),
        ('> # a#', 5, 2, r'> # a \#', r'> # a b\#', 'a b#'),
        ('# *alpha*#', 9, 6, r'# *alpha* \#', r'# *alpha* b\#', 'alpha b#'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          const InsertText(' '),
          source: spaced,
          caret: DisplayPosition(0, column),
        );
        session.act(const InsertText('b'), source: typed, rows: [shown]);
        session.act(const Undo(), source: source);
      }
      // A typed tab, or typed text that ends in whitespace, escapes it too.
      for (final (text, typed, shown) in [
        ('\t', '# alpha\t\\#', 'alpha\t#'),
        ('x ', r'# alphax \#', 'alphax #'),
      ]) {
        _Session(
          backend,
          source: '# alpha#',
          caret: 7,
        ).act(InsertText(text), source: typed, rows: [shown]);
      }
      // A `#` that stays text after the space goes in as it is, and a paste
      // keeps Markdown's literal meaning.
      for (final (source, command, typed, shown) in [
        ('# alpha#b', const InsertText(' '), '# alpha #b', 'alpha #b'),
        ('# alpha# #', const InsertText(' '), '# alpha # #', 'alpha #'),
        ('# alpha#', const Paste(' '), '# alpha #', 'alpha'),
      ]) {
        _Session(
          backend,
          source: source,
          caret: 7,
        ).act(command, source: typed, rows: [shown]);
      }
    });

    test('setting a link in an empty closed heading hides its sequence', () {
      // A new link or image goes where typed text would, so the closing
      // sequence is not run into its markup and painted after it.
      for (final (source, caret) in [('#  #', 3), ('# #', 2)]) {
        final link = _Session(backend, source: source, caret: caret);
        link.act(
          const SetLink('u', text: 't'),
          source: '# [t](<u>) #',
          rows: ['t'],
        );
        final image = _Session(backend, source: source, caret: caret);
        image.act(
          const SetImage('u', alt: 'a'),
          source: '# ![a](<u>) #',
          rows: ['a'],
        );
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

    test('removing a rule keeps the next block in its containers', () {
      for (final (source, removed, rows) in [
        ('***\n> b', '> b', ['b']),
        ('***\n- [ ] b', '- [ ] b', ['b']),
        ('x[^1]\n\n***\n[^1]: b', 'x[^1]\n\n[^1]: b', ['x[^1]', '', 'b']),
        ('***\n    code', '    code', ['code']),
        // A rule that opens an item takes the marker with its line, so the
        // next row joins the rule's line instead.
        ('- ***\n  b', '- b', ['b']),
      ]) {
        final session = _Session(
          backend,
          source: source,
          caret: source.indexOf('***'),
        );
        final last = session.editor.projection.rows.last;
        session.act(const DeleteForward(), source: removed, rows: rows);
        final now = session.editor.projection.rows.last;
        expect(now.kind, last.kind, reason: source);
        expect(
          now.shells.map((s) => s.kind),
          last.shells.map((s) => s.kind),
          reason: source,
        );
      }
      // After a rule below a list item, the paragraph would read on lazily
      // inside the item, as it would after an empty line there.
      for (final source in ['- a\n***\nb', '- a\n\nb']) {
        final session = _Session(
          backend,
          source: source,
          caret: source.length - 1,
        );
        session.act(const DeleteBackward(), applied: false, source: source);
      }
    });

    test('backspace on a first-row rule keeps the next block in place', () {
      // A rule that is all of an item leaves the item empty, and an empty
      // item's content starts one column past its marker: the heading, too
      // shallow for the rule's item, would move into it. The rule's line
      // goes whole instead, as Delete on the rule takes it.
      for (final newline in ['\n', '\r\n']) {
        final session = _Session(
          backend,
          source: '-   ***$newline  ## r',
          caret: 7,
        );
        session.act(
          const DeleteBackward(),
          source: '  ## r',
          rows: ['r'],
          caret: const DisplayPosition(0, 0),
        );
        expect(session.editor.projection.rows.single.shells, isEmpty);
      }
      // A block in the item stays there, behind the item's marker.
      final inside = _Session(backend, source: '- ***\n  b', caret: 5);
      inside.act(
        const DeleteBackward(),
        source: '- \n  b',
        rows: ['', 'b'],
        caret: const DisplayPosition(0, 0),
      );
      expect(inside.editor.projection.rows.last.shells.map((s) => s.kind), [
        ShellKind.list,
        ShellKind.item,
      ]);
      // Neither the rule nor its line can go without moving these blocks:
      // an empty item ends at a blank line, leaving `b` outside it, and
      // `code` would be indented code with or without the item's marker.
      for (final (source, caret) in [
        ('- ***\n\n  b', 5),
        ('-   ***\n      code', 7),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const DeleteBackward(), applied: false, source: source);
      }
    });

    test('removing the first line keeps a byte order mark before it', () {
      // comrak skips a leading byte order mark, so it belongs to the
      // document rather than to the first line, and stays when that goes.
      final bom = String.fromCharCode(0xfeff);
      for (final (source, caret, command, removed, rows) in [
        ('$bom-   ***\n  ## r', 8, const DeleteBackward(), '$bom  ## r', ['r']),
        ('$bom***\nb', 5, const DeleteBackward(), '${bom}b', ['b']),
        ('$bom***\nb', 4, const DeleteForward(), '${bom}b', ['b']),
        ('$bom***\nb', 4, const DeleteBackward(), '$bom\nb', ['', 'b']),
        ('$bom\nb', 2, const DeleteBackward(), '${bom}b', ['b']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          command,
          source: removed,
          rows: rows,
          caret: const DisplayPosition(0, 0),
        );
      }
    });

    test('return after a leading byte order mark keeps the containers', () {
      // Copied with the line's container prefix, the mark would be text in
      // the middle of the document, and the quote or list marker after it
      // would be painted as text too.
      final bom = String.fromCharCode(0xfeff);
      for (final (source, continued, rows) in [
        ('$bom> a', '$bom> a\n> x', ['a\nx']),
        ('$bom- a', '$bom- a\n- x', ['a', 'x']),
      ]) {
        final session = _Session(backend, source: source, caret: source.length);
        session.act(const Newline());
        session.act(const InsertText('x'), source: continued, rows: rows);
      }
      // So is a fence typed there, whose new lines continue the quote.
      final fence = _Session(backend, source: '$bom> ``', caret: 5);
      fence.act(
        const InsertText('`'),
        source: '$bom> ```\n> \n> ```\n> \n',
        rows: ['', '', ''],
      );
      expect(fence.editor.document.caretRow.kind, RowKind.codeBlock);
    });

    test('removing an empty line keeps the next block out of a container', () {
      // Without the gap the paragraph would read on lazily inside the quote
      // or item above it, or `2. b` would be painted as its text.
      for (final (source, caret, command) in [
        ('> a\n\nb', 3, const DeleteForward()),
        ('> a\n\nb', 4, const DeleteBackward()),
        ('- a\n\nb', 4, const DeleteBackward()),
        ('- y\n- \nz', 3, const DeleteForward()),
        ('a\n\n2. b', 1, const DeleteForward()),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, applied: false, source: source);
      }
      // A block that stays in containers of the same kinds still joins up.
      final list = _Session(backend, source: '- a\n\n- b', caret: 3);
      list.act(const DeleteForward(), source: '- a\n- b', rows: ['a', 'b']);
      final quote = _Session(backend, source: '> a\n\n> b', caret: 3);
      quote.act(const DeleteForward(), source: '> a\n> b', rows: ['a\nb']);
    });

    test('the empty line after a setext heading can be removed', () {
      for (final newline in ['\n', '\r\n']) {
        final source = ['H', '===', '', 'p'].join(newline);
        for (final (caret, command) in [
          (1, const DeleteForward()),
          (source.indexOf('p') - newline.length, const DeleteBackward()),
        ]) {
          final session = _Session(backend, source: source, caret: caret);
          // The caret stays at the end of the heading's text, not past its
          // underline at the start of the next row.
          session.act(
            command,
            source: ['H', '===', 'p'].join(newline),
            rows: ['H', 'p'],
            caret: const DisplayPosition(0, 1),
          );
          expect(session.editor.projection.rows.first.kind, RowKind.heading);
        }
      }
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

    test('lines put in a heading leave its markup with the first line', () {
      // As pasted, a closing sequence or underline would follow the last line
      // the text adds, where it ends no heading: the sequence shows as that
      // line's text, and an underline under an empty line as text, or as a
      // rule. It stays on the line the text's first line break ends, as
      // Return leaves it, with a space of its own after an empty heading.
      for (final (source, caret, command, edited, rows) in [
        ('# h #', 3, const Paste('x\ny'), '# hx #\ny', ['hx', 'y']),
        ('# ab ##', 3, const Paste('x\ny'), '# ax ##\nyb', ['ax', 'yb']),
        ('# h #', 3, const InsertText('x\n'), '# hx #\n', ['hx', '']),
        (
          '# h #\r\n',
          3,
          const Paste('x\r\ny'),
          '# hx #\r\ny\r\n',
          ['hx', 'y', ''],
        ),
        (
          '> # h #',
          5,
          const ReplaceRange(5, 5, '\ny'),
          '> # h #\ny',
          ['h', 'y'],
        ),
        ('# #', 2, const Paste('x\ny'), '# x #\ny', ['x', 'y']),
        ('h\n===', 1, const Paste('x\n'), 'hx\n===\n', ['hx', '']),
        ('h\n---', 1, const Paste('x\n# y'), 'hx\n---\n# y', ['hx', 'y']),
        (
          'a\nb\n===',
          3,
          const Paste('x\n- y'),
          'a\nbx\n===\n- y',
          ['a\nbx', 'y'],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final level = session.editor.projection.rows.first.headingLevel;
        session.act(command, source: edited, rows: rows);
        final heading = session.editor.projection.rows.first;
        expect(heading.kind, RowKind.heading, reason: source);
        expect(heading.headingLevel, level, reason: source);
        session.act(const Undo(), source: source);
      }
      // Where the markup ends a heading after the lines as well, they go in
      // as pasted: the underline takes the paragraph lines above it, and a
      // closing sequence ends the heading the last line opens.
      final setext = _Session(backend, source: 'h\n===', caret: 1);
      setext.act(const Paste('x\ny'), source: 'hx\ny\n===', rows: ['hx\ny']);
      final atx = _Session(backend, source: '# h #', caret: 3);
      atx.act(const Paste('x\n# y'), source: '# hx\n# y #', rows: ['hx', 'y']);
      // An underline cannot go up from an emptied last line to the lines
      // above it, as Return over that line cannot take it there.
      final last = _Session(backend, source: 'a\nbc\n===');
      last.act(const SetSelection(2, 4));
      last.act(const Paste('\n'), applied: false, source: 'a\nbc\n===');
    });

    test('return over all of a setext heading\'s text leaves it empty', () {
      // A setext heading cannot be empty: left behind, `===` would be painted
      // as text and `---` would be read as a rule. The text goes as with a
      // deletion, leaving an empty ATX heading of the same level, and the
      // line break follows it, with the caret on the new line.
      for (final (source, end, marker) in [
        ('abc\n===', 3, '#'),
        ('abc\n---', 3, '##'),
        ('**abc**\n===', 7, '#'),
        ('ab\ncd\n===', 5, '#'),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(0, end));
        session.act(
          const Newline(),
          source: '$marker \n',
          rows: ['', ''],
          caret: const DisplayPosition(1, 0),
        );
        expect(session.editor.projection.rows.first.kind, RowKind.heading);
      }
      // The heading keeps a list item's content where it was, and the next
      // item, which Return opens, takes the block after it.
      final item = _Session(backend, source: '- abc\n  ===\n\n  para');
      item.act(const SetSelection(2, 5));
      item.act(
        const Newline(),
        source: '- # \n\n- \n  para',
        rows: ['', '', '', 'para'],
      );
      expect(item.editor.projection.rows.last.shells.map((s) => s.kind), [
        ShellKind.list,
        ShellKind.item,
      ]);
      // With text before the split, the underline stays with that text; a
      // split from an earlier line that leaves text on the last one keeps
      // that text above the underline, as on an earlier line.
      final kept = _Session(backend, source: 'ab\ncd\n===');
      kept.act(const SetSelection(1, 5));
      kept.act(const Newline(), source: 'a\n===\n', rows: ['a', '']);
      final within = _Session(backend, source: 'ab\ncd\n===');
      within.act(const SetSelection(1, 4));
      within.act(const Newline(), source: 'a\nd\n===', rows: ['a\nd']);
      // The last of several lines cannot hand its underline up to the lines
      // before it, so that split refuses.
      final last = _Session(backend, source: 'ab\ncd\n===');
      last.act(const SetSelection(3, 5));
      last.act(const Newline(), applied: false, source: 'ab\ncd\n===');
    });

    test('a non-breaking space typed over a setext heading is its text', () {
      // Markdown strips only spaces and tabs from a heading. Another space is
      // text the heading keeps, with its underline, as other text typed over
      // all of it is.
      for (final space in ['\u00A0', '\u3000']) {
        final session = _Session(backend, source: 'abc\n===\n\np');
        session.act(const SetSelection(0, 3));
        session.act(
          InsertText(space),
          source: '$space\n===\n\np',
          rows: [space, '', 'p'],
        );
        expect(session.editor.projection.rows.first.kind, RowKind.heading);
      }
    });

    test('deleting all of a setext heading\'s text leaves it empty', () {
      // Left behind, `===` would be painted as text and `---` read as a rule.
      // The heading is respelled as an empty ATX heading of the same level in
      // the same containers, as a level change respells it, and the caret
      // stays where typing goes on in it.
      for (final (source, start, end, deleted, rows) in [
        ('abc\n===', 0, 3, '# ', ['']),
        ('abc\n---\n\np', 0, 3, '## \n\np', ['', '', 'p']),
        ('abc\r\n---\r\n\r\np', 0, 3, '## \r\n\r\np', ['', '', 'p']),
        ('**abc**\n===', 2, 5, '# ', ['']),
        ('ab\ncd\n===', 0, 5, '# ', ['']),
        ('> abc\n> ===', 2, 5, '> # ', ['']),
        ('- abc\n  ---\n- b', 2, 5, '- ## \n- b', ['', 'b']),
      ]) {
        for (final command in [
          const DeleteBackward(),
          const DeleteForward(),
          ReplaceRange(start, end, ''),
        ]) {
          final session = _Session(backend, source: source);
          final level = session.editor.projection.rows.first.headingLevel;
          session.act(SetSelection(start, end));
          session.act(
            command,
            source: deleted,
            rows: rows,
            caret: const DisplayPosition(0, 0),
          );
          final heading = session.editor.projection.rows.first;
          expect(heading.kind, RowKind.heading);
          expect(heading.headingLevel, level);
          session.act(
            const Undo(),
            source: source,
            selection: FlarkSelection(start, end),
          );
        }
      }
      // So does Backspace on its only character, and removing the image that
      // is all of its text; an empty-alt image left behind keeps it as it is.
      final single = _Session(backend, source: 'a\n===\n\np', caret: 1);
      single.act(
        const DeleteBackward(),
        source: '# \n\np',
        rows: ['', '', 'p'],
      );
      final image = _Session(backend, source: '![a](u)\n---', caret: 3);
      image.act(const RemoveImage(), source: '## ', rows: ['']);
      final kept = _Session(backend, source: '![](u) b\n---', caret: 8);
      kept.act(const DeleteBackward(), source: '![](u) \n---', rows: [' ']);
      expect(kept.editor.projection.rows.single.kind, RowKind.heading);
    });

    test('emptying a setext heading keeps the blocks after it in place', () {
      // Taken with the text, the underline would leave a list item empty,
      // and an empty item's content starts one column past its marker:
      // `para` would leave the item, or turn into indented code in it.
      for (final (source, caret, emptied) in [
        ('- abc\n  ===\n\n  para', 5, '- # \n\n  para'),
        ('-   abc\n    ===\n\n    para', 7, '-   # \n\n    para'),
        ('> - abc\n>   ===\n>\n>   para', 7, '> - # \n>\n>   para'),
        ('-   abc\n    ===\n      para', 7, '-   # \n      para'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final last = session.editor.projection.rows.last;
        session.act(
          const DeleteBackward(),
          times: 3,
          source: emptied,
          caret: const DisplayPosition(0, 0),
        );
        final now = session.editor.projection.rows.last;
        expect(now.text, 'para', reason: source);
        expect(now.kind, last.kind, reason: source);
        expect(
          now.shells.map((s) => s.kind),
          last.shells.map((s) => s.kind),
          reason: source,
        );
      }
    });

    test('backspace removes an empty heading whose marker cannot go', () {
      // Without its marker, the emptied item's `- ` would underline the
      // paragraph above, so the empty heading goes with its line, as Delete
      // at the end of that paragraph takes it.
      const source = '- foo\n  - bar\n    - ## ';
      final session = _Session(backend, source: source, caret: 23);
      session.act(
        const DeleteBackward(),
        source: '- foo\n  - bar',
        rows: ['foo', 'bar'],
        caret: const DisplayPosition(1, 3),
      );
      expect(session.editor.projection.rows.last.kind, RowKind.paragraph);
      // So does the empty heading that emptying a setext heading there leaves.
      final emptied = _Session(
        backend,
        source: '- foo\n  - bar\n    - baz\n      -',
        caret: 23,
      );
      emptied.act(const DeleteBackward(), times: 3, source: source);
      emptied.act(
        const DeleteBackward(),
        source: '- foo\n  - bar',
        rows: ['foo', 'bar'],
      );
    });

    test('clearing a setext heading then typing keeps a heading', () {
      // However the text is cleared, the same empty heading is left, so the
      // next text typed continues the heading rather than a paragraph.
      final typed = _Session(backend, source: 'abc\n===', caret: 3);
      typed.act(const DeleteBackward(), times: 3, source: '# ', rows: ['']);
      typed.act(
        const InsertText('x'),
        source: '# x',
        rows: ['x'],
        caret: const DisplayPosition(0, 1),
      );
      for (final (clear, retyped) in [
        (const DeleteBackward(), '## x\n\np'),
        (const DeleteForward(), '## x\n\np'),
        (const ReplaceRange(0, 3, ''), '## x\n\np'),
        (const InsertText(' '), '##  x\n\np'),
      ]) {
        final session = _Session(backend, source: 'abc\n---\n\np');
        session.act(const SetSelection(0, 3));
        session.act(clear, rows: ['', '', 'p']);
        session.act(
          const InsertText('x'),
          source: retyped,
          rows: ['x', '', 'p'],
          caret: const DisplayPosition(0, 1),
        );
        expect(session.editor.projection.rows.first.headingLevel, 2);
      }
      // A preedit that passes through empty on its way to new text is held
      // as the platform holds it, and the text it commits keeps the
      // heading; committed empty, it is deleted as Backspace deletes.
      for (final last in ['z', '']) {
        final composed = _Session(backend, source: 'abc\n---\n\np');
        composed.act(const SetSelection(0, 3));
        composed.editor.beginComposition();
        composed.act(const ReplaceRange(0, 3, 'x'), source: 'x\n---\n\np');
        composed.act(const ReplaceRange(0, 1, ''), source: '\n---\n\np');
        if (last.isNotEmpty) {
          final caret = composed.editor.selection.extent;
          composed.act(
            ReplaceRange(caret, caret, last),
            source: '$last\n---\n\np',
            rows: [last, '', 'p'],
          );
        }
        composed.editor.commitComposition();
        composed.expectState(
          source: last.isEmpty ? '## \n\np' : 'z\n---\n\np',
          rows: [last, '', 'p'],
          caret: DisplayPosition(0, last.length),
        );
        expect(composed.editor.projection.rows.first.headingLevel, 2);
        composed.act(const Undo(), source: 'abc\n---\n\np');
      }
    });

    test('typing over all of a setext heading\'s text keeps its underline', () {
      for (final command in [
        const InsertText('x'),
        const Paste('x'),
        const ReplaceRange(0, 3, 'x'),
      ]) {
        final session = _Session(backend, source: 'abc\n---\n\np');
        session.act(const SetSelection(0, 3));
        session.act(
          command,
          source: 'x\n---\n\np',
          rows: ['x', '', 'p'],
          caret: const DisplayPosition(0, 1),
        );
        expect(session.editor.projection.rows.first.kind, RowKind.heading);
      }
      // Whitespace cannot be a heading's text, so typed over all of it the
      // heading is left empty as with a deletion, the whitespace hidden after
      // its marker; a line break follows the empty heading as with Return.
      for (final (command, typed, caret) in [
        (const InsertText(' '), '##  \n\np', const DisplayPosition(0, 0)),
        (
          const ReplaceRange(0, 3, ' '),
          '##  \n\np',
          const DisplayPosition(0, 0),
        ),
        (const Paste('\n'), '## \n\n\np', const DisplayPosition(1, 0)),
      ]) {
        final session = _Session(backend, source: 'abc\n---\n\np');
        session.act(const SetSelection(0, 3));
        session.act(command, source: typed, caret: caret);
        final heading = session.editor.projection.rows.first;
        expect(heading.kind, RowKind.heading);
        expect(heading.text, isEmpty);
      }
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
      // The four columns count from the line's start, not from an indented
      // label: kept, its indentation would make the next marker underline
      // the item before it.
      final indented = _Session(
        backend,
        source: 'x[^1]\n\n  [^1]: - a',
        caret: 18,
      );
      indented.act(const Newline(), source: 'x[^1]\n\n  [^1]: - a\n    - ');
      indented.act(const InsertText('b'), rows: ['x[^1]', '', 'a', 'b']);
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

    test('code lines end the way the line they are edited on does', () {
      // In a document whose lines end differently, Return, a pasted line
      // break and a first body line follow the code's own line, not
      // whichever ending the document holds somewhere.
      for (final (source, caret, command, edited) in [
        ('a\r\n\n```\nx\n```', 9, const Newline(), 'a\r\n\n```\nx\n\n```'),
        (
          'a\n\n```\r\nx\r\n```',
          10,
          const Newline(),
          'a\n\n```\r\nx\r\n\r\n```',
        ),
        (
          'a\r\n\n```\nx\n```',
          9,
          const Paste('p\nq'),
          'a\r\n\n```\nxp\nq\n```',
        ),
        ('a\r\n\r\n```\n```', 9, const InsertText('x'), 'a\r\n\r\n```\nx\n```'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, source: edited);
      }
    });

    test('a word delete at the end of a code line joins the next word', () {
      // Only a single-character join can take a tab's columns without
      // reaching them; a word delete takes the word after the break with it.
      for (final (source, caret, edited) in [
        ('```\nfoo\nbar baz\n```', 7, '```\nfoo baz\n```'),
        ('> ```\n> foo\n> bar baz\n> ```', 11, '> ```\n> foo baz\n> ```'),
        ('```\r\nfoo\r\nbar baz\r\n```', 8, '```\r\nfoo baz\r\n```'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const DeleteForward(word: true), source: edited);
      }
    });

    test('fence characters typed, deleted or shifted in code stay code', () {
      // Each edit leaves a body line the parser would read as the closing
      // fence, which would turn the rest of the code into prose and the old
      // closer into a new fence. The fences grow past it instead.
      const run = '```\n``x`\ny\n```', grown = '````\n```\ny\n````';
      const indented = '```\nx\n    ```\n```', body = '```\ny';
      const crlf = '```\r\n``x`\r\ny\r\n```';
      for (final (source, caret, command, edited, shown) in [
        ('```\n``\ny\n```', 6, const InsertText('`'), grown, body),
        (run, 7, const DeleteBackward(), grown, body),
        (run, 6, const DeleteForward(), grown, body),
        (run, 0, const ReplaceRange(6, 7, ''), grown, body),
        (crlf, 8, const DeleteBackward(), '````\r\n```\r\ny\r\n````', body),
        ('```\nx```\n```', 5, const Newline(), '````\nx\n```\n````', 'x\n```'),
        (
          indented,
          7,
          const DeleteBackward(),
          '````\nx\n   ```\n````',
          'x\n   ```',
        ),
        (indented, 10, const Outdent(), '````\nx\n  ```\n````', 'x\n  ```'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, source: edited, rows: [shown]);
        session.act(const Undo(), source: source);
      }
      // A body may hold such a run already, indented or beside other text,
      // so while the parser reads the block unchanged, the fences stay.
      final kept = _Session(
        backend,
        source: '```\n    ```\nab\n```',
        caret: 14,
      );
      kept.act(const Paste('x'), source: '```\n    ```\nabx\n```');
      kept.act(const DeleteBackward(), source: '```\n    ```\nab\n```');
    });

    test('a line break in quoted code takes the quote\'s prefix', () {
      for (final command in [
        const InsertText('\n'),
        const ReplaceRange(9, 9, '\n'),
      ]) {
        final session = _Session(
          backend,
          source: '> ```\n> ab\n> ```',
          caret: 9,
        );
        session.act(
          command,
          source: '> ```\n> a\n> b\n> ```',
          rows: ['a\nb'],
          caret: const DisplayPosition(0, 2),
        );
      }
    });

    test('text put on an empty code line stays in its list item', () {
      // The empty line needs none of the item's indentation, so the source
      // omits it; text put there takes the indentation the code continues
      // with instead of leaving the item and splitting the code in two.
      const source = '- a\n\n  ```\n  x\n\n  y\n  ```\n';
      for (final (caret, command, edited, body) in [
        (
          15,
          const InsertText('z'),
          '- a\n\n  ```\n  x\n  z\n  y\n  ```\n',
          'x\nz\ny',
        ),
        (
          15,
          const Paste('z\nw'),
          '- a\n\n  ```\n  x\n  z\n  w\n  y\n  ```\n',
          'x\nz\nw\ny',
        ),
        (18, const DeleteBackward(), '- a\n\n  ```\n  x\n  y\n  ```\n', 'x\ny'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, source: edited, rows: ['a', '', body, '']);
        expect(session.editor.document.caretRow.shells.map((s) => s.kind), [
          ShellKind.list,
          ShellKind.item,
        ]);
      }
    });

    test('text put on an empty code line after a tab marker stays in it', () {
      // The empty line can lack the indentation a tab after the item's
      // marker reaches; text put there takes the indentation the item's
      // later lines take, tab and all, rather than leaving the item and
      // splitting the code. A fence a tab indents past an item's content
      // would show the rest of the tab as code on a line the tab starts:
      // its lines take the item's own indentation instead.
      for (final (source, caret, command, edited, rows) in [
        (
          '-\t```\n\n  ```',
          6,
          const InsertText('x'),
          '-\t```\n \tx\n  ```',
          ['x', ''],
        ),
        (
          '1.\t```\n\n\t```',
          7,
          const Paste('x\ny'),
          '1.\t```\n  \tx\n  \ty\n\t```',
          ['x\ny'],
        ),
        (
          '- -\t```\n\n    ```',
          8,
          const InsertText('x'),
          '- -\t```\n   \tx\n    ```',
          ['x'],
        ),
        (
          '>\t-\t```\n>\n>\t \t```',
          9,
          const InsertText('x'),
          '>\t-\t```\n>\t \tx\n>\t \t```',
          ['x'],
        ),
        (
          '- a\n\n\t```\n\n\t```',
          10,
          const InsertText('x'),
          '- a\n\n\t```\n  x\n\t```',
          ['a', '', 'x'],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, source: edited, rows: rows);
        final row = session.editor.document.caretRow;
        expect(row.kind, RowKind.codeBlock, reason: source);
        expect(row.shells.last.kind, ShellKind.item, reason: source);
      }
      // A line in a quote carries the quote's marker, after which the text
      // goes as it did.
      final quoted = _Session(backend, source: '>\t```\n>\n>\t```', caret: 7);
      quoted.act(
        const InsertText('x'),
        source: '>\t```\n>x\n>\t```',
        rows: ['x'],
      );
    });

    test('an edit that leaves code as it is only moves the caret', () {
      // As in a paragraph, there is nothing to commit or undo, and nothing
      // that needs source mode: the caret goes where the edit puts it.
      for (final (source, start, end, command, caret) in [
        ('```\nabc\ndef\n```\n', 7, 8, const Newline() as FlarkCommand, 8),
        ('> ```\n> abc\n> def\n> ```\n', 11, 14, const Newline(), 14),
        ('```\nabc\n```\n', 5, 6, const InsertText('b'), 6),
        ('```\nabc\n```\n', 5, 6, const Paste('b'), 6),
        ('```\nabc\n```\n', 4, 4, const ReplaceRange(4, 7, 'abc'), 7),
        ('```\n``` x\nab\n```\n', 10, 11, const InsertText('a'), 11),
      ]) {
        final session = _Session(backend, source: source, caret: start);
        if (end != start) session.act(SetSelection(start, end));
        session.act(
          command,
          source: source,
          selection: FlarkSelection.collapsed(caret),
        );
        expect(session.editor.history.canUndo, isFalse, reason: source);
      }
      // With the caret already there, the edit is inert.
      final inert = _Session(backend, source: '```\nabc\n```\n', caret: 7);
      inert.act(
        const ReplaceRange(4, 7, 'abc'),
        applied: false,
        source: '```\nabc\n```\n',
        anchor: 7,
      );
      expect(inert.editor.lastRejection, isNull);
    });

    test('outdenting code that steps by a tab takes a tab\'s columns', () {
      // A snippet indented with tabs steps by a tab, which Markdown counts
      // as four columns, as the code delegate does: a line indented with
      // spaces loses up to four of them, not one.
      const source = '```\n\tx\n      y\n```\n';
      final session = _Session(backend, source: source);
      session.act(const SetSelection(4, 14));
      session.act(
        const Outdent(),
        source: '```\nx\n  y\n```\n',
        rows: ['x\n  y', ''],
        selection: const FlarkSelection(4, 9),
      );
    });

    test('indenting lines of code leaves their blank lines as they are', () {
      // A blank line in a quote or list item can lack part of the
      // container's prefix, which would absorb the indentation, so no blank
      // line takes any.
      for (final (source, start, end, indented, body, selection) in [
        (
          '> ```\n> abc\n>\n> ``` x\n> ```\n',
          8,
          21,
          '> ```\n>   abc\n>\n>   ``` x\n> ```\n',
          '  abc\n\n  ``` x',
          const FlarkSelection(10, 25),
        ),
        (
          '- ```\n  abc\n\n  x ```\n  ```\n',
          8,
          20,
          '- ```\n    abc\n\n    x ```\n  ```\n',
          '  abc\n\n  x ```',
          const FlarkSelection(10, 24),
        ),
        (
          '- ```\n\tabc\n\n\tdef\n\t```\n',
          7,
          15,
          '- ```\n\t  abc\n\n\t  def\n\t```\n',
          '    abc\n\n    def',
          const FlarkSelection(9, 19),
        ),
        (
          '```\nabc\n\ndef\n```\n',
          4,
          12,
          '```\n  abc\n\n  def\n```\n',
          '  abc\n\n  def',
          const FlarkSelection(6, 16),
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(start, end));
        session.act(
          const Indent(),
          source: indented,
          rows: [body, ''],
          selection: selection,
        );
        session.act(
          const Undo(),
          source: source,
          selection: FlarkSelection(start, end),
        );
      }
      // Indenting only blank lines changes nothing, which is not a refusal.
      const blank = '```\nabc\n\n\n\ndef\n```\n';
      final session = _Session(backend, source: blank);
      session.act(const SetSelection(8, 10));
      session.act(const Indent(), applied: false, source: blank);
      expect(session.editor.lastRejection, isNull);
    });

    test('indenting indented code keeps it out of the item before it', () {
      // After an item whose content starts past four columns, four spaces
      // make code. Indented to the item's column, the code would join the
      // item as its paragraph, and the table after it would read on as that
      // paragraph's text: Tab does nothing, with no refusal to report.
      for (final (source, base, extent) in [
        ('  1. b\n\n    c\n| d |\n| - |', 12, 12),
        ('  1. b\n\n    c\n    e\n| d |\n| - |', 12, 19),
      ]) {
        final session = _Session(backend, source: source, caret: base);
        if (extent != base) session.act(SetSelection(base, extent));
        session.act(const Indent(), applied: false, source: source);
        expect(session.editor.lastRejection, isNull, reason: source);
      }
      // Where the code stays code, Tab indents it.
      final kept = _Session(
        backend,
        source: 'b\n\n    c\n| d |\n| - |',
        caret: 7,
      );
      kept.act(
        const Indent(),
        source: 'b\n\n      c\n| d |\n| - |',
        rows: ['b', '', '  c', 'd', '| - |'],
        anchor: 9,
      );
    });

    test(
      'a join of code lines that would take a tab\'s columns is refused',
      () {
        // The tab before `def` shows two columns of code past the item's
        // indentation. Deleting the line break before it deletes the tab too,
        // and with it those columns, which Delete did not reach.
        for (final (source, caret) in [
          ('- ```\n\tabc\n\n\tdef\n\t```\n', 11),
          ('- ```\n\tabc\n\tdef\n\t```\n', 10),
        ]) {
          final session = _Session(backend, source: source, caret: caret);
          session.act(const DeleteForward(), applied: false, source: source);
          expect(session.editor.lastRejection, FlarkRejection.unsupportedEdit);
        }
        // A selection that ends there shows those columns, so deleting it
        // takes them; a tab that shows none joins as any line does.
        final selected = _Session(
          backend,
          source: '- ```\n\tabc\n\n\tdef\n\t```\n',
        );
        selected.act(const SetSelection(11, 13));
        selected.act(
          const DeleteForward(),
          source: '- ```\n\tabc\n  def\n\t```\n',
          rows: ['  abc\ndef', ''],
        );
        final whole = _Session(
          backend,
          source: '-\t```\n\tx\n\ty\n\t```\n',
          caret: 8,
        );
        whole.act(
          const DeleteForward(),
          source: '-\t```\n\txy\n\t```\n',
          rows: ['xy', ''],
        );
      },
    );

    test('a composition erased on an empty code line leaves it as it was', () {
      // Text composed there takes the prefix the fence's lines continue
      // with; erased before the composition ends, it takes that prefix away
      // again, so the composition changes nothing and records no step.
      for (final (source, caret) in [
        ('- ```\n  a\n\n  b\n  ```\n', 10),
        ('> ```\n> a\n>\n> b\n> ```\n', 11),
        ('> ```\n> a\n> \n> b\n> ```\n', 12),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.editor.beginComposition();
        session.act(const InsertText('k'), rows: ['a\nk\nb', '']);
        session.act(const DeleteBackward(), source: source, anchor: caret);
        session.editor.commitComposition();
        expect(session.editor.history.canUndo, isFalse, reason: source);
      }
    });

    test('lifting an item\'s marker lifts the blocks inside it too', () {
      // The blocks indented under the item leave it with its marker, as the
      // profile lets them; no other block moves, so the lift goes ahead.
      for (final (source, lifted, shells) in [
        ('- a\n\n  b', 'a\n\n  b', ['', '', '']),
        ('- a\n  - b', 'a\n  - b', ['', 'list/item']),
      ]) {
        final session = _Session(backend, source: source, caret: 2);
        session.act(const DeleteBackward(), source: lifted);
        expect(_shellsOf(session.editor), shells, reason: source);
      }
    });

    test('removing the empty line above indented code keeps it code', () {
      // Without the line, `    b` would read on as part of the paragraph
      // above, and the code would change kind: Delete refuses.
      final session = _Session(backend, source: 'a\n\n    b', caret: 2);
      session.act(const DeleteForward(), applied: false, source: 'a\n\n    b');
    });

    test('delete at the end of code removes an empty fence after it', () {
      // Joined onto the closing fence, the empty fence's lines would run
      // the two fences together: it goes whole instead, and the caret stays
      // at the end of the code rather than past its hidden closer.
      final session = _Session(
        backend,
        source: '```\na\n```\n```\n```\n',
        caret: 5,
      );
      session.act(
        const DeleteForward(),
        source: '```\na\n```\n',
        rows: ['a', ''],
        caret: const DisplayPosition(0, 1),
      );
    });

    test('delete on the empty line before an empty fence removes the line', () {
      // Joined into the fence instead, the line break would take the
      // opening fence's line with it.
      for (final (source, removed) in [
        ('a\n\n~~~\n\n~~~\n', 'a\n~~~\n\n~~~\n'),
        ('a\n\n~~~\n', 'a\n~~~\n'),
      ]) {
        final session = _Session(backend, source: source, caret: 2);
        session.act(const DeleteForward(), source: removed);
      }
    });

    test('a join that would show a setext underline refuses', () {
      // Joined after `## `, `Foo` would be the ATX heading's text, and its
      // `=` underline a paragraph of its own, painted.
      for (final (source, caret) in [
        ('## \nFoo\n=\n', 3),
        ('> ## \n> Foo\n> =\n', 5),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const DeleteForward(), applied: false, source: source);
      }
    });

    test('indent that would not nest an empty item does nothing', () {
      // `>-` gives its quote no space after the marker, so the columns
      // Indent adds leave the empty item in its own list rather than under
      // `a`: the key does nothing, with no refusal to report.
      final session = _Session(backend, source: '> - a\n>-', caret: 8);
      session.act(const Indent(), applied: false, source: '> - a\n>-');
      expect(session.editor.lastRejection, isNull);
    });

    test('a heading cleared of its level is a paragraph, or keeps it', () {
      // Without its marker the heading's text must read as a paragraph in
      // the same containers. Under a table it would be the table's next row,
      // and `---`, `<div>`, a fence or a definition would be blocks of their
      // own: as `# >` does, the heading keeps its level.
      for (final (source, caret) in [
        ('| a |\n| - |\n| b |\n# x', 20),
        ('> | a |\n> | - |\n> | b |\n> # x', 28),
        ('# ---', 2),
        ('# <div>', 2),
        ('a\n\n# ```', 5),
        ('# [a]: /u', 2),
      ]) {
        for (final command in [
          const DeleteBackward(),
          const SetHeadingLevel(0),
        ]) {
          final session = _Session(backend, source: source, caret: caret);
          session.act(command, applied: false, source: source);
        }
      }
    });

    test('return beside a heading\'s text that shows nothing keeps it', () {
      // An image without alt text, or a link without text, shows nothing but
      // is the heading's text, as deleting reads it. Return after it opens a
      // line below the underline, and before it moves the heading down;
      // respelling the heading as an empty one would lose it.
      for (final (source, caret, split) in [
        ('![](u)\n===', 6, '![](u)\n===\n'),
        ('![](u)\n===', 0, '\n![](u)\n==='),
        ('[](u)\n===', 5, '[](u)\n===\n'),
        ('> ![](u)\n> ===', 8, '> ![](u)\n> ===\n> '),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const Newline(), source: split, rows: ['', '']);
        expect(
          session.editor.projection.rows.map((r) => r.kind),
          contains(RowKind.heading),
          reason: source,
        );
      }
    });

    test('backspace removes an empty line or a rule after a table', () {
      // A row that shows nothing joins the table's last row whole, which
      // keeps its cells, so Backspace erases on into the table.
      for (final (source, caret, removed) in [
        ('| a |\n| - |\n| b |\n', 18, '| a |\n| - |\n| b |'),
        ('| a |\n| - |\n| b |\n---', 21, '| a |\n| - |\n| b |'),
        ('| a | b |\n| - | - |\n| c |\n', 26, '| a | b |\n| - | - |\n| c |'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const DeleteBackward(), source: removed);
      }
    });

    test('a join never runs a line into a table row', () {
      // What a join puts on a table row's line is read as one of its cells,
      // or as one the table drops from view: text, or the fence of code with
      // no body. Only an empty line or a rule joins, going whole.
      for (final (source, caret, command) in <(String, int, FlarkCommand)>[
        ('| a |\n| - |\n| b |\n<div>', 18, const DeleteBackward()),
        ('| a |\n| - |\n| b |\n~~~', 18, const DeleteBackward()),
        ('x\n| a |\n| - |', 1, const DeleteForward()),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(command, applied: false, source: source);
      }
      // An empty line before a table goes whole, as before any row.
      final session = _Session(backend, source: 'x\n\n| a |\n| - |', caret: 2);
      session.act(const DeleteForward(), source: 'x\n| a |\n| - |');
    });
    test('choosing automatic keeps a fence\'s metadata in its place', () {
      // Automatic removes the language. With metadata after it, the fence
      // takes the `auto` tag instead: with the language gone, the
      // metadata's first word would be read as the language.
      const tagged = '```ruby title="x"\nputs 1\n```';
      _Session(backend, source: tagged, caret: tagged.indexOf('puts')).act(
        const SetCodeLanguage(''),
        source: '```auto title="x"\nputs 1\n```',
        rows: ['puts 1'],
        anchor: tagged.indexOf('puts'),
      );
      const bare = '```ruby\nputs 1\n```';
      _Session(backend, source: bare, caret: bare.indexOf('puts')).act(
        const SetCodeLanguage(''),
        source: '```\nputs 1\n```',
        rows: ['puts 1'],
        anchor: 4,
      );
    });
  });
}
