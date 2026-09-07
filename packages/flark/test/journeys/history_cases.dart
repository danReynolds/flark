part of '../journey_test.dart';

void _historyCases(FlarkParseBackend backend) {
  group('history', () {
    test('consecutive typing is one undo group', () {
      final session = _Session(backend, source: 'abc', caret: 3);
      session.act(const InsertText('d'), source: 'abcd');
      session.act(const InsertText('e'), source: 'abcde');
      session.act(
        const Undo(),
        source: 'abc',
        caret: const DisplayPosition(0, 3),
      );
      session.act(
        const Redo(),
        source: 'abcde',
        caret: const DisplayPosition(0, 5),
      );
    });

    test('a pause splits typing groups', () {
      final session = _Session(backend, source: 'abc', caret: 3);
      session.act(const InsertText('d'), source: 'abcd');
      session.act(
        const InsertText('e'),
        afterMilliseconds: 2000,
        source: 'abcde',
      );
      session.act(
        const Undo(),
        source: 'abcd',
        caret: const DisplayPosition(0, 4),
      );
      session.act(
        const Undo(),
        source: 'abc',
        caret: const DisplayPosition(0, 3),
      );
    });

    test('delete to empty then typing undoes in two steps', () {
      final session = _Session(backend, source: '*t*', caret: 2);
      session.act(const DeleteBackward(), source: '');
      session.act(
        const InsertText('x'),
        afterMilliseconds: 2000,
        source: '*x*',
      );
      session.act(const Undo(), source: '', caret: const DisplayPosition(0, 0));
      session.act(
        const Undo(),
        source: '*t*',
        caret: const DisplayPosition(0, 1),
        anchor: 2,
      );
      session.act(const Redo(), times: 2, source: '*x*');
    });

    test('structural commands are their own entries', () {
      final session = _Session(backend, source: '- a', caret: 3);
      session.act(const Newline(), source: '- a\n- ');
      session.act(const InsertText('b'), source: '- a\n- b');
      session.act(const Undo(), source: '- a\n- ');
      session.act(
        const Undo(),
        source: '- a',
        caret: const DisplayPosition(0, 1),
      );
    });
  });
}
