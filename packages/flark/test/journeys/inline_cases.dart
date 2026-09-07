part of '../journey_test.dart';

void _inlineCases(FlarkParseBackend backend) {
  group('inline', () {
    test('typing inside emphasis continues it', () {
      final session = _Session(backend, source: 'say *hi* now', caret: 7);
      session.expectState(context: Style.emphasis);
      session.act(
        const InsertText('x'),
        source: 'say *hix* now',
        rows: ['say hix now'],
        caret: const DisplayPosition(0, 7),
        context: Style.emphasis,
      );
    });

    test('arrow right keeps the context it came from', () {
      final session = _Session(backend, source: '**bold** here');
      session.act(
        const MoveCaret(MoveDirection.forward),
        times: 4,
        caret: const DisplayPosition(0, 4),
        anchor: 6,
        context: Style.strong,
      );
      session.act(
        const InsertText('!'),
        source: '**bold!** here',
        rows: ['bold! here'],
        caret: const DisplayPosition(0, 5),
        context: Style.strong,
      );
    });

    test('arrow left from plain text stays plain at the closing boundary', () {
      final session = _Session(backend, source: '**bold** here', caret: 9);
      session.act(
        const MoveCaret(MoveDirection.backward),
        caret: const DisplayPosition(0, 4),
        anchor: 8,
        context: 0,
      );
      session.act(
        const InsertText('!'),
        source: '**bold**! here',
        rows: ['bold! here'],
        caret: const DisplayPosition(0, 5),
        context: 0,
      );
    });

    test(
      'entering a span from the left does not start it before its first glyph',
      () {
        final session = _Session(backend, source: 'a **b**');
        session.act(
          const MoveCaret(MoveDirection.forward),
          times: 2,
          caret: const DisplayPosition(0, 2),
          anchor: 2,
          context: 0,
        );
        session.act(
          const InsertText('x'),
          source: 'a x**b**',
          rows: ['a xb'],
          context: 0,
        );
      },
    );

    test('home and end take the outermost anchor', () {
      final session = _Session(backend, source: '**bold**', caret: 6);
      session.act(
        const MoveCaret(MoveDirection.backward, unit: MoveUnit.line),
        caret: const DisplayPosition(0, 0),
        anchor: 0,
        context: 0,
      );
      session.act(
        const MoveCaret(MoveDirection.forward, unit: MoveUnit.line),
        caret: const DisplayPosition(0, 4),
        anchor: 8,
        context: 0,
      );
      session.act(const InsertText('.'), source: '**bold**.', rows: ['bold.']);
    });

    test('arrow right at the document end leaves the span', () {
      final session = _Session(backend, source: '**bold**', caret: 6);
      session.expectState(context: Style.strong);
      session.act(
        const MoveCaret(MoveDirection.forward),
        caret: const DisplayPosition(0, 4),
        anchor: 8,
        context: 0,
      );
    });

    test('backspace inside strong deletes the grapheme and stays strong', () {
      final session = _Session(backend, source: '**bold**', caret: 6);
      session.act(
        const DeleteBackward(),
        source: '**bol**',
        rows: ['bol'],
        caret: const DisplayPosition(0, 3),
        context: Style.strong,
      );
    });

    test('backspace outside a span deletes its last visible grapheme', () {
      final session = _Session(backend, source: '**bold** x', caret: 8);
      session.expectState(context: 0);
      session.act(
        const DeleteBackward(),
        source: '**bol** x',
        rows: ['bol x'],
        caret: const DisplayPosition(0, 3),
      );
    });

    test(
      'delete to empty removes the owner and the next character recreates it',
      () {
        final session = _Session(backend, source: 'a *t* b', caret: 4);
        session.act(
          const DeleteBackward(),
          source: 'a  b',
          rows: ['a  b'],
          caret: const DisplayPosition(0, 2),
          context: Style.emphasis,
        );
        session.act(
          const InsertText('x'),
          source: 'a *x* b',
          rows: ['a x b'],
          caret: const DisplayPosition(0, 3),
          context: Style.emphasis,
        );
      },
    );

    test('delete to empty then whitespace exits the context', () {
      final session = _Session(backend, source: '*t*', caret: 2);
      session.act(
        const DeleteBackward(),
        source: '',
        rows: [''],
        caret: const DisplayPosition(0, 0),
        context: Style.emphasis,
      );
      session.act(const InsertText(' '), source: ' ', rows: [''], context: 0);
    });

    test('nested delete to empty removes every emptied owner', () {
      final session = _Session(backend, source: '***t***', caret: 4);
      session.act(
        const DeleteBackward(),
        source: '',
        rows: [''],
        context: Style.emphasis | Style.strong,
      );
      session.act(
        const InsertText('y'),
        source: '***y***',
        rows: ['y'],
        caret: const DisplayPosition(0, 1),
        context: Style.emphasis | Style.strong,
      );
    });

    test('forward delete to empty', () {
      final session = _Session(backend, source: '`c` z', caret: 1);
      session.act(
        const DeleteForward(),
        source: ' z',
        rows: ['z'],
        caret: const DisplayPosition(0, 0),
        context: Style.code,
      );
      session.act(
        const InsertText('d'),
        source: ' `d`z',
        rows: ['dz'],
        context: Style.code,
      );
    });

    test('an escaped delimiter is deleted as one visible character', () {
      final session = _Session(backend, source: r'\*not\*', caret: 2);
      session.expectState(rows: ['*not*'], caret: const DisplayPosition(0, 1));
      session.act(
        const DeleteBackward(),
        source: r'not\*',
        rows: ['not*'],
        caret: const DisplayPosition(0, 0),
        context: 0,
      );
    });

    test('typing the closing delimiter renders the construct', () {
      final session = _Session(backend, source: '*hi', caret: 3);
      session.expectState(rows: ['*hi']);
      session.act(
        const InsertText('*'),
        source: '*hi*',
        rows: ['hi'],
        caret: const DisplayPosition(0, 2),
        context: 0,
      );
    });

    test('typing replaces the selection', () {
      final session = _Session(backend, source: 'hello world');
      session.act(
        const SetSelection(0, 5),
        selection: const FlarkSelection(0, 5),
      );
      session.act(
        const InsertText('bye'),
        source: 'bye world',
        rows: ['bye world'],
        caret: const DisplayPosition(0, 3),
      );
    });

    test('backspace deletes the selection across a span', () {
      final session = _Session(backend, source: 'a **bc** d');
      session.act(const SetSelection(0, 10));
      session.act(const DeleteBackward(), source: '', rows: ['']);
    });

    test('toggling strong on a selection wraps it and again unwraps it', () {
      final session = _Session(backend, source: 'make this bold');
      session.act(const SetSelection(5, 9));
      session.act(
        const ToggleStyle(Style.strong),
        source: 'make **this** bold',
        rows: ['make this bold'],
        selection: const FlarkSelection(7, 11),
      );
      session.act(
        const ToggleStyle(Style.strong),
        source: 'make this bold',
        selection: const FlarkSelection(5, 9),
      );
    });

    test('toggling strong at the caret styles the next character', () {
      final session = _Session(backend, source: 'ab', caret: 2);
      session.act(
        const ToggleStyle(Style.strong),
        source: 'ab',
        context: Style.strong,
      );
      session.act(
        const InsertText('c'),
        source: 'ab**c**',
        rows: ['abc'],
        context: Style.strong,
      );
      session.act(const ToggleStyle(Style.strong), anchor: 7, context: 0);
      session.act(const InsertText('d'), source: 'ab**c**d', rows: ['abcd']);
    });

    test('a link projects its text and the caret never enters the url', () {
      final session = _Session(
        backend,
        source: 'see [docs](http://x.y) now',
        caret: 9,
      );
      session.expectState(
        rows: ['see docs now'],
        caret: const DisplayPosition(0, 8),
        context: Style.link,
      );
      session.act(
        const MoveCaret(MoveDirection.forward),
        caret: const DisplayPosition(0, 9),
        anchor: 23,
        context: 0,
      );
      session.act(
        const MoveCaret(MoveDirection.backward),
        caret: const DisplayPosition(0, 8),
        anchor: 22,
        context: 0,
      );
    });
  });
}
