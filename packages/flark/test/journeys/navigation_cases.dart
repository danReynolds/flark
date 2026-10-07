part of '../journey_test.dart';

void _navigationCases(FlarkParseBackend backend) {
  group('navigation', () {
    test('word movement stops at word edges', () {
      final session = _Session(backend, source: 'one **two** three');
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.word),
        caret: const DisplayPosition(0, 3),
        anchor: 3,
        context: 0,
      );
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.word),
        caret: const DisplayPosition(0, 7),
        anchor: 9,
        context: Style.strong,
      );
      session.act(
        const MoveCaret(MoveDirection.backward, unit: MoveUnit.word),
        caret: const DisplayPosition(0, 4),
        anchor: 6,
        context: Style.strong,
      );
      session.act(
        const MoveCaret(MoveDirection.backward, unit: MoveUnit.word),
        caret: const DisplayPosition(0, 0),
      );
    });

    test('pointer placement takes the word context at either visible edge', () {
      final session = _Session(backend, source: '**bold** x');
      session.act(
        const PlaceCaret(0, 4, leadingHalf: false),
        anchor: 6,
        context: Style.strong,
      );
      session.act(
        const PlaceCaret(0, 4),
        applied: false,
        anchor: 6,
        context: Style.strong,
      );
      session.act(
        const InsertText('x'),
        source: '**boldx** x',
        rows: ['boldx x'],
        context: Style.strong,
      );
      session.act(const PlaceCaret(0, 0), anchor: 2, context: Style.strong);
      session.act(
        const PlaceCaret(0, 0, leadingHalf: false),
        applied: false,
        anchor: 2,
        context: Style.strong,
      );
      session.act(
        const InsertText('y'),
        source: '**yboldx** x',
        rows: ['yboldx x'],
        context: Style.strong,
      );
      session.act(const PlaceCaret(0, 7), anchor: 11, context: 0);
      session.act(
        const InsertText('z'),
        source: '**yboldx** zx',
        rows: ['yboldx zx'],
        context: 0,
      );
    });

    test('a press where the caret is takes its context and ends the step', () {
      // A pending style is the intent chosen at a caret. A press starts over
      // from where it lands, as in common editors, even where the caret
      // already was; a browser's double click that selected a word in
      // between had dropped the style, and a single press had kept it.
      final session = _Session(backend, source: 'plain', caret: 5);
      session.act(const ToggleStyle(Style.emphasis), context: Style.emphasis);
      session.act(const PlaceCaret(0, 5), anchor: 5, context: 0);
      session.act(
        const InsertText('x'),
        source: 'plainx',
        rows: ['plainx'],
        context: 0,
      );
      // Nothing pending, the press changes nothing it shows, but typing
      // after it is an undo step of its own.
      session.act(const PlaceCaret(0, 6), applied: false, anchor: 6);
      session.act(const InsertText('y'), source: 'plainxy');
      session.act(const Undo(), source: 'plainx');
      session.act(const Undo(), source: 'plain');
    });

    test('up and down move between rows at the same offset', () {
      final session = _Session(
        backend,
        source: 'first line\n\n- item\n\n```\ncode\n```',
        caret: 3,
      );
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.row),
        caret: const DisplayPosition(1, 0),
      );
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.row),
        caret: const DisplayPosition(2, 3),
      );
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.row),
        times: 2,
        caret: const DisplayPosition(4, 3),
      );
      session.act(
        const MoveCaret(MoveDirection.backward, unit: MoveUnit.row),
        times: 4,
        caret: const DisplayPosition(0, 3),
      );
    });

    test('shift arrows extend the selection and typing replaces it', () {
      final session = _Session(backend, source: 'abc *def*', caret: 1);
      session.act(
        const MoveCaret(MoveDirection.forward, extend: true),
        times: 3,
        selection: const FlarkSelection(1, 4),
      );
      session.act(
        const InsertText('Z'),
        source: 'aZ*def*',
        rows: ['aZdef'],
        caret: const DisplayPosition(0, 2),
      );
    });

    test(
      'a shift press back where the selection started selects no syntax',
      () {
        // A press that extends the selection to another anchor of its base,
        // past a closing `**` the document ends with, collapses where the
        // selection started, as shift arrows do: the `**` alone shows nothing.
        final session = _Session(backend, source: 'a **b**', caret: 7);
        for (final leadingHalf in [false, true]) {
          session.act(
            PlaceCaret(0, 3, extend: true, leadingHalf: leadingHalf),
            applied: false,
            selection: const FlarkSelection.collapsed(7),
          );
        }
      },
    );

    test('shift arrows at a document end select no syntax alone', () {
      // After `b` the caret holds the span's context, and the document shows
      // nothing past it. The closing `**` alone would be a selection that
      // shows nothing, which typing could not replace.
      final session = _Session(backend, source: 'a **b**', caret: 5);
      session.act(
        const MoveCaret(MoveDirection.forward, extend: true),
        applied: false,
        selection: const FlarkSelection.collapsed(5),
        context: Style.strong,
      );
      session.act(
        const MoveCaret(MoveDirection.backward, extend: true),
        selection: const FlarkSelection(5, 4),
      );
      session.act(
        const MoveCaret(MoveDirection.forward, extend: true),
        selection: const FlarkSelection.collapsed(5),
        context: Style.strong,
      );
      session.act(
        const MoveCaret(
          MoveDirection.forward,
          unit: MoveUnit.line,
          extend: true,
        ),
        applied: false,
        selection: const FlarkSelection.collapsed(5),
      );
      // A selection that shows text takes the closing syntax with it, so
      // typing replaces the whole span.
      session.act(const SetSelection(0, 5));
      session.act(
        const MoveCaret(MoveDirection.forward, extend: true),
        selection: const FlarkSelection(0, 7),
      );
      session.act(const InsertText('x'), source: 'x', rows: ['x']);
    });

    test('shift arrows back over a line break leave the caret in its span', () {
      // Back at the end of `b`, the selection collapses where it started, in
      // the span, rather than holding the closing `**` alone.
      final session = _Session(backend, source: 'a **b**\nc', caret: 5);
      session.act(
        const MoveCaret(MoveDirection.forward, extend: true),
        selection: const FlarkSelection(5, 8),
      );
      session.act(
        const MoveCaret(MoveDirection.backward, extend: true),
        selection: const FlarkSelection.collapsed(5),
        context: Style.strong,
      );
      session.act(
        const InsertText('x'),
        source: 'a **bx**\nc',
        rows: ['a bx\nc'],
        context: Style.strong,
      );
      // Before a document's opening syntax, as after its closing syntax.
      final start = _Session(backend, source: '**b** a', caret: 2);
      start.act(
        const MoveCaret(MoveDirection.backward, extend: true),
        applied: false,
        selection: const FlarkSelection.collapsed(2),
        context: Style.strong,
      );
      start.act(
        const MoveCaret(MoveDirection.forward, extend: true),
        selection: const FlarkSelection(2, 3),
      );
    });

    test('a plain arrow collapses a selection to its edge', () {
      final session = _Session(backend, source: 'hello');
      session.act(const SetSelection(1, 4));
      session.act(
        const MoveCaret(MoveDirection.backward),
        caret: const DisplayPosition(0, 1),
        selection: const FlarkSelection.collapsed(1),
      );
      session.act(const SetSelection(1, 4));
      session.act(
        const MoveCaret(MoveDirection.forward),
        caret: const DisplayPosition(0, 4),
      );
    });

    test('moving through table cells and past the delimiter line', () {
      final session = _Session(
        backend,
        source: '| a | b |\n| - | - |\n| c | d |',
      );
      session.expectState(
        rows: ['a', 'b', 'c', 'd'],
        caret: const DisplayPosition(0, 0),
      );
      // A cell's text ends at its last character: the space before the
      // pipe is the pipe's, so the caret steps from after it to the next
      // cell.
      session.act(
        const MoveCaret(MoveDirection.forward),
        times: 2,
        caret: const DisplayPosition(1, 0),
      );
      session.act(
        const MoveCaret(MoveDirection.forward),
        times: 2,
        caret: const DisplayPosition(2, 0),
      );
      session.act(
        const InsertText('x'),
        source: '| a | b |\n| - | - |\n| xc | d |',
        rows: ['a', 'b', 'xc', 'd'],
      );
    });

    test('the caret stays out of a fenced code block\'s fence lines', () {
      final session = _Session(backend, source: '```js\nlet a;\n```');
      session.expectState(
        rows: ['let a;'],
        caret: const DisplayPosition(0, 0),
        anchor: 6,
      );
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.line),
        caret: const DisplayPosition(0, 6),
      );
      session.act(
        const InsertText('b'),
        source: '```js\nlet a;b\n```',
        rows: ['let a;b'],
      );
      session.act(
        const Newline(),
        source: '```js\nlet a;b\n\n```',
        rows: ['let a;b\n'],
        caret: const DisplayPosition(0, 8),
      );
    });

    test('replace range and paste land the caret after the text', () {
      final session = _Session(backend, source: 'abcdef');
      session.act(
        const ReplaceRange(1, 3, 'XY'),
        source: 'aXYdef',
        caret: const DisplayPosition(0, 3),
      );
      session.act(
        const Paste('**p**'),
        source: 'aXY**p**def',
        rows: ['aXYpdef'],
        caret: const DisplayPosition(0, 4),
        anchor: 8,
      );
    });

    test('a hard break is one grapheme to delete', () {
      final session = _Session(backend, source: 'a  \nb', caret: 4);
      session.expectState(rows: ['a  \nb'], caret: const DisplayPosition(0, 4));
      session.act(
        const DeleteBackward(),
        source: 'ab',
        rows: ['ab'],
        caret: const DisplayPosition(0, 1),
      );
    });

    test('entities and emoji are single graphemes', () {
      final session = _Session(backend, source: 'x &amp; 😀y', caret: 11);
      session.expectState(
        rows: ['x & 😀y'],
        caret: const DisplayPosition(0, 7),
      );
      session.act(
        const MoveCaret(MoveDirection.backward),
        caret: const DisplayPosition(0, 6),
      );
      session.act(
        const DeleteBackward(),
        source: 'x &amp; y',
        rows: ['x & y'],
        caret: const DisplayPosition(0, 4),
      );
      session.act(
        const MoveCaret(MoveDirection.backward),
        times: 2,
        caret: const DisplayPosition(0, 2),
      );
      session.act(
        const DeleteForward(),
        source: 'x  y',
        rows: ['x  y'],
        caret: const DisplayPosition(0, 2),
      );
    });
  });
}
