part of '../journey_test.dart';

/// Input-method composition. The platform holds the text it composes, and
/// its next preedit, or its commit, edits the text it holds: while it
/// composes, its text goes into the source as it is. What typing makes of
/// the text is made once, when the composition commits, as one undo step.
void _compositionCases(FlarkParseBackend backend) {
  group('composition', () {
    test('a pending style wraps the composed text when it commits', () {
      // One host replaces its preedit by range, another cancels and types
      // each preedit afresh.
      for (final (style, open, close) in [
        (Style.strong, '**', '**'),
        (Style.emphasis, '*', '*'),
        (Style.code, '`', '`'),
        (Style.strikethrough, '~~', '~~'),
      ]) {
        for (final replaced in [true, false]) {
          final session = _Session(backend, source: 'abc', caret: 3);
          session.act(ToggleStyle(style), context: style);
          final editor = session.editor..beginComposition();
          session.act(
            const InsertText('n'),
            source: 'abcn',
            rows: ['abcn'],
            anchor: 4,
            context: style,
          );
          if (replaced) {
            session.act(
              const ReplaceRange(3, 4, 'に'),
              source: 'abcに',
              anchor: 4,
              context: style,
            );
          } else {
            editor
              ..cancelComposition()
              ..beginComposition();
            session.expectState(source: 'abc', anchor: 3, context: style);
            session.act(
              const InsertText('に'),
              source: 'abcに',
              anchor: 4,
              context: style,
            );
          }
          editor.commitComposition();
          session.expectState(
            source: 'abc$openに$close',
            rows: ['abcに'],
            caret: const DisplayPosition(0, 4),
            anchor: 3 + open.length + 1,
            context: style,
          );
        }
      }
    });

    test('a pipe composed in a table cell is escaped when it commits', () {
      const table = '| a | b |\n| - | - |\n| 1 | 2 |';
      final session = _Session(backend, source: table, caret: 23);
      session.editor.beginComposition();
      // As the platform holds it, the pipe splits the cell while composing.
      session.act(
        const InsertText('|'),
        source: '| a | b |\n| - | - |\n| 1| | 2 |',
      );
      session.editor.commitComposition();
      session.expectState(
        source: '| a | b |\n| - | - |\n| 1\\| | 2 |',
        rows: ['a ', 'b ', '1| ', '2 '],
        caret: const DisplayPosition(2, 2),
      );
      session.act(
        const InsertText('x'),
        source: '| a | b |\n| - | - |\n| 1\\|x | 2 |',
      );
    });

    test('a backtick composed after two others completes the fence', () {
      final session = _Session(backend, source: '``', caret: 2);
      session.editor.beginComposition();
      session.act(const InsertText('`'), source: '```', anchor: 3);
      session.editor.commitComposition();
      session.expectState(source: '```\n\n```\n\n', anchor: 4);
      expect(session.editor.document.caretRow.fenced, isTrue);
      session.act(const InsertText('x'), source: '```\nx\n```\n\n');
    });

    test('a composition over a span with its delimiters keeps them', () {
      // A double click on a word at an owner's edge selects its hidden
      // delimiters too, and the platform composes over all of it.
      final session = _Session(backend, source: 'a **bold** c');
      session.act(const SetSelection(2, 10));
      session.editor.beginComposition();
      session.act(
        const InsertText('か'),
        source: 'a か c',
        rows: ['a か c'],
        anchor: 3,
      );
      session.act(const ReplaceRange(2, 3, 'かな'), source: 'a かな c');
      session.editor.commitComposition();
      session.expectState(
        source: 'a **かな** c',
        rows: ['a かな c'],
        caret: const DisplayPosition(0, 4),
        context: Style.strong,
      );
      session.act(
        const Undo(),
        source: 'a **bold** c',
        selection: const FlarkSelection(2, 10),
      );
    });

    test('a dead key composes its accent into the letter it makes', () {
      // The accent alone cannot carry the emphasis, but the letter can.
      final session = _Session(backend, source: 'cafe', caret: 4);
      session.act(const ToggleStyle(Style.emphasis), context: Style.emphasis);
      session.editor.beginComposition();
      session.act(
        const InsertText('´'),
        source: 'cafe´',
        context: Style.emphasis,
      );
      session.act(
        const ReplaceRange(4, 5, 'é'),
        source: 'cafeé',
        context: Style.emphasis,
      );
      session.editor.commitComposition();
      session.expectState(
        source: 'cafe*é*',
        rows: ['cafeé'],
        caret: const DisplayPosition(0, 5),
        context: Style.emphasis,
      );
    });

    test(
      'a cancelled composition leaves nothing; a committed one is typed',
      () {
        final session = _Session(backend, source: 'abc', caret: 3);
        session.act(const ToggleStyle(Style.strong), context: Style.strong);
        final editor = session.editor..beginComposition();
        session.act(
          const InsertText('x'),
          source: 'abcx',
          context: Style.strong,
        );
        editor.cancelComposition();
        session.expectState(source: 'abc', anchor: 3, context: Style.strong);
        expect(editor.history.canUndo, isFalse);
        editor.beginComposition();
        session.act(
          const InsertText('y'),
          source: 'abcy',
          context: Style.strong,
        );
        editor.commitComposition();
        session.expectState(
          source: 'abc**y**',
          anchor: 6,
          context: Style.strong,
        );
        expect(editor.composing, isFalse);
      },
    );

    test('composing is refused where typing is', () {
      // Text typed after a bare marker under a quote would read on as the
      // quote's: typing refuses it, and so does the input method's first
      // preedit, which the platform then gives up.
      const source = '> quote\n-\n> next';
      final session = _Session(backend, source: source, caret: 9);
      session.editor.beginComposition();
      session.act(const InsertText('t'), applied: false, source: source);
      expect(session.editor.lastRejection, FlarkRejection.unsupportedEdit);
    });

    test('a commit that typing refuses is withdrawn', () {
      // Composed into a cell, a backslash before its delimiter would escape
      // it, which typing refuses: the composition ends as a cancelled one.
      const table = '| a | b |\n| - | - |\n| 1| 2 |';
      final session = _Session(backend, source: table, caret: 23);
      final editor = session.editor..beginComposition();
      session.act(
        const InsertText('x'),
        source: '| a | b |\n| - | - |\n| 1x| 2 |',
      );
      session.act(
        const ReplaceRange(23, 24, r'\'),
        source: '| a | b |\n| - | - |\n| 1\\| 2 |',
      );
      editor.commitComposition();
      session.expectState(source: table, anchor: 23);
      expect(editor.lastRejection, FlarkRejection.unsupportedEdit);
      expect((editor.composing, editor.history.canUndo), (false, false));
    });

    test(
      'a committed composition is one undo step, typing after it another',
      () {
        // Typed under a paragraph, `-` would underline it: the commit types
        // it on a line of its own, as typing it there does.
        final session = _Session(backend, source: 'Hello\n', caret: 6);
        final editor = session.editor..beginComposition();
        session.act(const InsertText('-'), source: 'Hello\n-');
        editor.commitComposition();
        session.expectState(
          source: 'Hello\n\n-',
          rows: ['Hello', '', '-'],
          caret: const DisplayPosition(2, 1),
        );
        session.act(const InsertText('x'), source: 'Hello\n\n-x');
        session.act(const Undo(), source: 'Hello\n\n-', anchor: 8);
        session.act(const Undo(), source: 'Hello\n', anchor: 6);
        expect(editor.history.canUndo, isFalse);
        session.act(const Redo(), source: 'Hello\n\n-', anchor: 8);
      },
    );
  });
}
