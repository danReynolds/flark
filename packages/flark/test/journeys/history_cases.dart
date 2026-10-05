part of '../journey_test.dart';

void _historyCases(FlarkParseBackend backend) {
  group('history', () {
    test('undo or redo with nothing to undo or redo does nothing quietly', () {
      // Command-Z on a fresh document is no refused edit for a host to
      // report, in source mode too.
      final session = _Session(backend, source: 'abc', caret: 3);
      void quiet(FlarkCommand command, String source) {
        session.act(command, applied: false, source: source);
        expect(session.editor.lastRejection, isNull, reason: '$command');
      }

      quiet(const Undo(), 'abc');
      quiet(const Redo(), 'abc');
      session.act(const InsertText('d'), source: 'abcd');
      quiet(const Redo(), 'abcd');
      session.act(const Undo(), source: 'abc');
      quiet(const Undo(), 'abc');
      session.editor.setSourceMode(true);
      expect(session.editor.apply(const Redo()), isTrue);
      expect(session.editor.apply(const Redo()), isFalse);
      expect(session.editor.lastRejection, isNull);
    });

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

    test('joining two lines of code is an undo step of its own', () {
      // As in a paragraph, the typing and deleting around a join undo apart
      // from it, Backspace or Delete alike.
      const source = '```\nabc\ndef\n```\n';
      final backward = _Session(backend, source: source, caret: 8);
      backward.act(const DeleteBackward(), source: '```\nabcdef\n```\n');
      backward.act(
        const DeleteBackward(),
        times: 2,
        source: '```\nadef\n```\n',
      );
      backward.act(const Undo(), source: '```\nabcdef\n```\n', anchor: 7);
      backward.act(const Undo(), source: source, anchor: 8);
      final forward = _Session(backend, source: source, caret: 7);
      forward.act(const InsertText('x'), source: '```\nabcx\ndef\n```\n');
      forward.act(const DeleteForward(), source: '```\nabcxdef\n```\n');
      forward.act(const DeleteForward(), source: '```\nabcxef\n```\n');
      forward.act(const Undo(), source: '```\nabcxdef\n```\n', anchor: 8);
      forward.act(const Undo(), source: '```\nabcx\ndef\n```\n', anchor: 8);
      forward.act(const Undo(), source: source, anchor: 7);
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

    test('an inert command publishes nothing and records no step', () {
      for (final (source, command) in <(String, FlarkCommand)>[
        ('# H', const SetHeadingLevel(1)),
        // However the heading is spelled: rewriting it as plain ATX would
        // respell its source for no visible change.
        ('H\n===', const SetHeadingLevel(1)),
        ('H\n---', const SetHeadingLevel(2)),
        ('# H #', const SetHeadingLevel(1)),
        ('a', const SetHeadingLevel(0)),
        ('```dart\nx\n```', const SetCodeLanguage('dart')),
        ('[a](http://x)', const SetLink('http://x')),
        ('[a][r]\n\n[r]: http://x', const SetLink('http://x')),
        ('<http://x>', const SetLink('http://x')),
      ]) {
        final session = _Session(backend, source: source, caret: 2);
        final revision = session.editor.revision;
        // Like a repeated SetStyle: false, but not a refusal.
        session.act(command, applied: false, source: source);
        expect(session.editor.lastRejection, isNull, reason: source);
        expect(session.editor.revision, revision, reason: source);
        expect(session.editor.history.canUndo, isFalse, reason: source);
      }
    });
  });
}
