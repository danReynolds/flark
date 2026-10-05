part of '../journey_test.dart';

void _inlineCases(FlarkParseBackend backend) {
  group('inline', () {
    test('a range at an autolink\'s hidden edges edits its address', () {
      // Its `<` and `>` are hidden like a span's delimiters: part of the
      // address edits the address, all of it replaces the link.
      for (final (start, end, text, edited, rows) in [
        (14, 16, 'Zz', 'x <https://a.bZz> y', ['x https://a.bZz y']),
        (14, 16, '', 'x <https://a.b> y', ['x https://a.b y']),
        (3, 16, 'Zz', 'x Zz y', ['x Zz y']),
        (2, 15, 'Zz', 'x Zz y', ['x Zz y']),
      ]) {
        _Session(
          backend,
          source: 'x <https://a.bk> y',
        ).act(ReplaceRange(start, end, text), source: edited, rows: rows);
      }
      final selected = _Session(backend, source: 'x <b@c.dk> y');
      selected.act(const SetSelection(8, 10));
      selected.act(const InsertText('Zz'), source: 'x <b@c.dZz> y');
    });

    test('a link or image keeps the blocks around it', () {
      // A link wrapped in a definition's label would end the definition, one
      // after a rule's dashes would paint them, and an image on the empty
      // line before indented code would make the code its paragraph.
      for (final (source, base, extent, command) in [
        ("[spec]: /s 'S'\n[docs]: /d", 0, 7, const SetLink('https://x')),
        ('a\n\n---\n\nb', 6, 6, const SetLink('https://x')),
        ('web\n\n    code', 4, 4, const SetImage('i.png', alt: 'you')),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(base, extent));
        session.act(command, applied: false, source: source);
      }
      // A definition or a rule offers no link at all.
      final definition = _Session(backend, source: '[a]: /u\nb', caret: 1);
      expect(definition.editor.canSetResource(), isFalse);
    });

    test('removing a link or image keeps the blocks around it', () {
      // Unlinked text shows as the link did, or the unlink is refused: here
      // the strong delimiters after the link could no longer open, and
      // would show.
      const strong = '[lbl](<u>)**[(**';
      _Session(
        backend,
        source: strong,
        caret: 2,
      ).act(const RemoveLink(), applied: false, source: strong);
      // Text that would start a block at its line's start escapes its first
      // punctuation.
      for (final (source, unlinked, rows, at) in [
        ('- [1. Intro](#intro)', r'- 1\. Intro', ['1. Intro'], 8),
        ('[# a](u)', r'\# a', ['# a'], 3),
      ]) {
        final session = _Session(
          backend,
          source: source,
          caret: source.indexOf('](') - 1,
        );
        session.act(
          const RemoveLink(),
          source: unlinked,
          rows: rows,
          caret: DisplayPosition(0, at),
        );
        expect(session.editor.document.caretRow.kind, RowKind.paragraph);
      }
      // An image that starts its line takes the whitespace after it, which
      // would indent the line out of its table, as indented code.
      final image = _Session(
        backend,
        source: '![a](<i.png>)\ta|b\n-|-',
        caret: 2,
      );
      image.act(
        const RemoveImage(),
        source: 'a|b\n-|-',
        rows: ['a', 'b', '-|-'],
        caret: const DisplayPosition(0, 0),
      );
      expect(image.editor.document.caretRow.kind, RowKind.tableCell);
    });

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

    test(
      'toggling a style off at the caret refuses an unwrap that re-pairs delimiters',
      () {
        // Unwrapped, the emphasis's text would run into the strikethrough's
        // opener, which then could not open.
        final strike = _Session(backend, source: '*bm*~~😀~~', caret: 2);
        strike.act(
          const ToggleStyle(Style.emphasis),
          applied: false,
          source: '*bm*~~😀~~',
          rows: ['bm😀'],
        );
        // Unwrapped, the code's tilde would pair with the ones around it.
        final code = _Session(backend, source: '~`r~`~', caret: 3);
        code.act(const ToggleStyle(Style.code), applied: false);
        // A span that unwraps cleanly still does.
        final plain = _Session(backend, source: 'a *bc* d', caret: 4);
        plain.act(
          const ToggleStyle(Style.emphasis),
          source: 'a bc d',
          rows: ['a bc d'],
          context: 0,
        );
      },
    );

    test('a phrase typed with strong on stays one span', () {
      final session = _Session(backend, source: 'plain ', caret: 6);
      session.act(const ToggleStyle(Style.strong), context: Style.strong);
      for (final char in 'one'.split('')) {
        session.act(InsertText(char));
      }
      // The space leaves the span, keeping its intent for the next word.
      session.act(
        const InsertText(' '),
        source: 'plain **one** ',
        rows: ['plain one '],
        caret: const DisplayPosition(0, 10),
        context: Style.strong,
      );
      // The next word continues the span instead of opening a second pair.
      session.act(
        const InsertText('t'),
        source: 'plain **one t**',
        rows: ['plain one t'],
        caret: const DisplayPosition(0, 11),
        context: Style.strong,
      );
      for (final char in 'wo three'.split('')) {
        session.act(InsertText(char));
      }
      session.expectState(
        source: 'plain **one two three**',
        rows: ['plain one two three'],
        caret: const DisplayPosition(0, 19),
        context: Style.strong,
      );
      session.act(
        const Undo(),
        source: 'plain ',
        caret: const DisplayPosition(0, 6),
        context: Style.strong,
      );
      session.act(const Redo(), source: 'plain **one two three**');
    });

    test('composed words continue a styled span', () {
      // Input methods compose each word, then commit it before the space.
      final session = _Session(backend, source: 'plain ', caret: 6);
      session.act(const ToggleStyle(Style.emphasis), context: Style.emphasis);
      for (final word in ['one', 'two']) {
        session.editor.beginComposition();
        for (final char in word.split('')) {
          session.act(InsertText(char));
        }
        session.editor.commitComposition();
        session.act(const InsertText(' '));
      }
      session.expectState(
        source: 'plain *one two* ',
        rows: ['plain one two '],
        context: Style.emphasis,
      );
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

    test('retyping the first word of a styled phrase keeps it styled', () {
      for (final (source, caret, open) in [
        ('x **one two**', 7, '**'),
        ('x *one two* y', 6, '*'),
        ('x ~~one two~~', 7, '~~'),
        ('x ***one two***', 8, '***'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final style = session.editor.typingContext;
        // The caret stays where the word was, before the space that now
        // leads the span, and keeps the span's intent.
        session.act(
          const DeleteBackward(),
          times: 3,
          rows: [source.endsWith(' y') ? 'x  two y' : 'x  two'],
          caret: const DisplayPosition(0, 2),
          context: style,
        );
        for (final char in 'new'.split('')) {
          session.act(InsertText(char), context: style);
        }
        // The word rejoins the span rather than fusing with the next one.
        session.expectState(
          source: source.replaceFirst('${open}one', '${open}new'),
          rows: [source.endsWith(' y') ? 'x new two y' : 'x new two'],
          caret: const DisplayPosition(0, 5),
        );
      }
    });

    test('deleting a span\'s first word after a shown line feed keeps the '
        'gap', () {
      // `&#10;` shows a line feed inside the line, so the space the deletion
      // leaves before the span shows, as it does after any other text: the
      // caret stays before it, in the span's intent.
      final session = _Session(backend, source: 'x&#10;*one two*', caret: 10);
      session.act(
        const DeleteBackward(word: true),
        source: 'x&#10; *two*',
        caret: const DisplayPosition(0, 2),
        context: Style.emphasis,
      );
      // The word rejoins the span rather than fusing with the next one.
      session.act(const InsertText('z'), source: 'x&#10;*z two*');
    });

    test('word backspace over a styled first word keeps the gap', () {
      final session = _Session(backend, source: 'x **one two**', caret: 7);
      session.act(
        const DeleteBackward(word: true),
        source: 'x  **two**',
        caret: const DisplayPosition(0, 2),
        context: Style.strong,
      );
      // Erasing the space that led the span returns into it.
      session.act(
        const DeleteForward(),
        source: 'x **two**',
        caret: const DisplayPosition(0, 2),
        context: Style.strong,
      );
      session.act(const InsertText('n'), source: 'x **ntwo**');
    });
  });
}
