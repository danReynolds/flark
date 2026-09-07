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
  });
}
