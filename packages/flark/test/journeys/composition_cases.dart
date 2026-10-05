part of '../journey_test.dart';

/// Input-method composition. The platform holds the text it composes, and
/// its next preedit, or its commit, edits the text it holds: while it
/// composes, its text goes into the source as it is. What typing makes of
/// the text is made once, when the composition commits, as one undo step.
void _compositionCases(FlarkParseBackend backend) {
  group('composition', () {
    test(
      'a correction composed beside the word stays when its retype is refused',
      () {
        // An input method corrects `teh` while it composes `wor` after it:
        // typing the two as one span across the emphasis is refused, and
        // withdrawn the composition would take the correction with it.
        final session = _Session(backend, source: '*teh* ', caret: 6);
        final editor = session.editor..beginComposition();
        session.act(const InsertText('wor'), source: '*teh* wor');
        session.act(const ReplaceRange(1, 4, 'thee'), source: '*thee* wor');
        editor.commitComposition();
        session.expectState(source: '*thee* wor', rows: ['thee wor']);
        expect(editor.composing, isFalse);
      },
    );

    test('commands that edit around an open composition commit it whole', () {
      // Commands a host would apply after ending the composition, applied
      // while it is open: its commit retypes all that changed, which may
      // span lines around a hard break inside emphasis, and must neither
      // throw nor leave an illegal state. Found by the web differential.
      final session = _Session(
        backend,
        source: '*fooé`*\n\n*\\\nbar*\n',
        caret: 10,
      );
      session.editor.beginComposition();
      for (final command in const <FlarkCommand>[
        ToggleStyle(Style.strong),
        MoveCaret(MoveDirection.forward, unit: MoveUnit.word),
        InsertText('\t'),
        MoveCaret(MoveDirection.backward, unit: MoveUnit.word),
        DeleteBackward(),
        InsertText('1'),
        DeleteBackward(),
        InsertText('\n'),
        InsertText('_'),
        Paste('```\nc\n```'),
        InsertText('!'),
        MoveCaret(MoveDirection.backward, unit: MoveUnit.row),
        InsertText('!'),
      ]) {
        session.editor.apply(command);
      }
      session.editor.apply(const Undo());
      session.expectState();
      expect(session.editor.composing, isFalse);
    });

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

    test('a correction composed where typing is refused is refused', () {
      // An input method composing at the end corrects text elsewhere, where
      // typing would read on into the quote: that preedit is refused as the
      // first one would be. A correction is never retyped at the commit, so
      // this is its only check.
      const source = '> quote\n-\n> next\n\nend';
      final session = _Session(backend, source: source, caret: 21);
      session.editor.beginComposition();
      session.act(const InsertText('x'), source: '${source}x');
      session.act(
        const ReplaceRange(9, 9, 't'),
        applied: false,
        source: '${source}x',
      );
      expect(session.editor.lastRejection, FlarkRejection.unsupportedEdit);
      session.editor.commitComposition();
      session.expectState(
        source: '${source}x',
        rows: ['quote', '-', 'next', '', 'endx'],
      );
    });

    test(
      'a composition around other edits stays when its retype is refused',
      () {
        // A deletion inside the emphasis while the input method composes
        // after it: retyped as one span, the text would cross the emphasis,
        // which typing refuses, and withdrawn it would take the deletion with
        // it. The composition stays as the platform holds it.
        final session = _Session(backend, source: '*teh* ', caret: 6);
        final editor = session.editor..beginComposition();
        session.act(const InsertText('wor'), source: '*teh* wor');
        session.act(const SetSelection.caret(4));
        session.act(const DeleteBackward(), source: '*te* wor');
        session.act(const SetSelection.caret(8));
        session.act(const InsertText('l'), source: '*te* worl');
        editor.commitComposition();
        session.expectState(source: '*te* worl', rows: ['te worl']);
        expect(editor.lastRejection, isNull);
      },
    );

    test('a retyped composition keeps a surrogate pair whole', () {
      // The input method replaces 😀 with 😁| while a paste lands later in
      // the row, so the commit retypes all that changed. The two emoji share
      // their first code unit; from there the change would start inside a
      // pair, and the text would not be typed at all. Retyped from the
      // pair's start, the composed pipe is escaped as typed pipes are.
      final session = _Session(
        backend,
        source: '| a | b |\n| - | - |\n| 😀 | c |',
        caret: 24,
      );
      final editor = session.editor..beginComposition();
      session.act(
        const ReplaceRange(22, 24, '😁|'),
        source: '| a | b |\n| - | - |\n| 😁| | c |',
      );
      session.act(const SetSelection.caret(26));
      session.act(
        const Paste('!'),
        source: '| a | b |\n| - | - |\n| 😁| !| c |',
      );
      editor.commitComposition();
      session.expectState(
        source: '| a | b |\n| - | - |\n| 😁\\| !| c |',
        rows: ['a ', 'b ', '😁| !', 'c '],
      );
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
