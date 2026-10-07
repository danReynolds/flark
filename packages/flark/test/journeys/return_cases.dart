part of '../journey_test.dart';

/// Return splits a row and shows a line break where the caret was, and
/// nothing else changes: the new line stays in the containers of the line it
/// split, the text it moves or leaves stays text, and the blocks around it
/// keep their kinds and containers. Where no spelling keeps all of that,
/// Return refuses.
void _returnCases(FlarkParseBackend backend) {
  List<ShellKind> shells(_Session session) => [
    for (final shell in session.editor.document.caretRow.shells) shell.kind,
  ];

  group('rows', () {
    test('Return before literal delimiters starts the new line there', () {
      // The reopened delimiter pairs with the literal ones after it, which
      // a caret after it would leave before the caret: the caret goes where
      // the line starts, or an escape keeps the style faithful.
      for (final (source, caret, paragraph, edited, at) in [
        ('*foo**bar*', 4, false, '*foo*\n***bar*', const DisplayPosition(0, 4)),
        (
          '_fo_bar_baz_',
          3,
          false,
          '_fo_\n_\\_bar_baz_',
          const DisplayPosition(0, 3),
        ),
        (
          '__foo__bar__baz__',
          5,
          true,
          '__foo__\n\n____bar__baz__',
          const DisplayPosition(2, 0),
        ),
      ]) {
        _Session(backend, source: source, caret: caret).act(
          Newline(paragraph: paragraph),
          source: edited,
          caret: at,
        );
      }
    });

    test('Return before a heading\'s closing sequence in an item', () {
      // An escape the split tried fell in the closing sequence it moved;
      // that spelling is passed over and the item continues after it.
      const source =
          '  10.  foo[\r\n\r\n  11. ##### *&amp;😀\t😀 #\r\n\r\n'
          '           bar\r\n';
      for (var caret = 36; caret <= 41; caret++) {
        _Session(backend, source: source, caret: caret).act(const Newline());
      }
    });

    test('return on an empty item with nested items leaves no line deeper', () {
      // Leaving the empty item would leave its nested items to the item
      // before it, and the line Return makes between them: nested deeper
      // than the line it left. Return does nothing, with no refusal.
      const source = '- a\n  - b\n    - c\n  - \n    - d\n';
      final caret = source.indexOf('  - \n') + 4;
      final session = _Session(backend, source: source, caret: caret);
      session.act(const Newline(), applied: false, source: source);
      expect(session.editor.lastRejection, isNull);
      // At the top level the nested item joins the item before it, and the
      // line stays outside the list.
      final top = _Session(
        backend,
        source: '* x\n  * y\n* \n  * z\n',
        caret: 12,
      );
      top.act(const Newline(), source: '* x\n  * y\n\n\n  * z\n');
    });

    test('return before a line break inside a link does not throw', () {
      // The split rewrites that line break, so a lazy line's prefix edit at
      // the next line's start is no edit of its own; spliced out of order it
      // threw.
      for (final (source, caret, edited) in [
        ('[a\n](u)', 2, '[a](u)\n\n'),
        (
          '![foo [bar](/url) *r*\n](/url2)',
          20,
          '![foo [bar](/url) *r*](/url2)\n\n',
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const Newline(), source: edited);
      }
      // The lazy line takes the quote's prefix, and the link reopens after
      // it rather than before it.
      final quoted = _Session(backend, source: '> [a\nb](u)', caret: 4);
      quoted.act(
        const Newline(),
        source: '> [a](u)\n> \n> [b](u)',
        rows: ['a', '', 'b'],
        caret: const DisplayPosition(1, 0),
      );
    });

    test('beside a line break inside a span keeps both parts in its '
        'containers', () {
      // The span closes after the text left and reopens where the next
      // line's text starts, past its container prefix, which a lazy line
      // takes from the row. Spliced into a lazy line's prefix, the
      // delimiters threw, or read as markup and Return refused. Return
      // after the break moves the text after it, with its context; before
      // it, the caret is on the new line.
      for (final (source, caret, split, at, context) in [
        (
          '> > *a\n> b*',
          9,
          '> > *a*\n> > \n> > *b*',
          const DisplayPosition(2, 0),
          Style.emphasis,
        ),
        (
          '> - *a\n> b*',
          9,
          '> - *a*\n>   \n> - *b*',
          const DisplayPosition(2, 0),
          Style.emphasis,
        ),
        (
          '> - [a\n> b](u)',
          9,
          '> - [a](u)\n>   \n> - [b](u)',
          const DisplayPosition(2, 0),
          Style.link,
        ),
        (
          '> 1. **a\n> b**',
          11,
          '> 1. **a**\n>    \n> 2. **b**',
          const DisplayPosition(2, 0),
          Style.strong,
        ),
        (
          '> **a\n> b**',
          8,
          '> **a**\n> \n> **b**',
          const DisplayPosition(2, 0),
          Style.strong,
        ),
        (
          '> **a\n> b**',
          5,
          '> **a**\n> \n> **b**',
          const DisplayPosition(1, 0),
          0,
        ),
        (
          '> > *a\n> b*',
          6,
          '> > *a*\n> > \n> > *b*',
          const DisplayPosition(1, 0),
          0,
        ),
        ('> *a\nb*', 4, '> *a*\n> \n> *b*', const DisplayPosition(1, 0), 0),
        ('- *a\nb*', 4, '- *a*\n- \n  *b*', const DisplayPosition(1, 0), 0),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        final kinds = shells(session);
        session.act(
          const Newline(),
          source: split,
          rows: ['a', '', 'b'],
          caret: at,
          context: context,
        );
        for (final row in session.editor.projection.rows) {
          expect([for (final s in row.shells) s.kind], kinds, reason: source);
        }
      }
      final typed = _Session(backend, source: '> > *a\n> b*', caret: 9);
      typed.act(const Newline());
      typed.act(const InsertText('x'), source: '> > *a*\n> > \n> > *xb*');
    });

    test('before a soft line break inside a span puts the caret on the new '
        'line', () {
      // The whitespace Return moves out of the span runs through the line
      // break, and the caret went after it, onto the text that kept its
      // line: the next letter was typed into `bar`.
      final session = _Session(backend, source: '**foo\nbar**', caret: 5);
      session.act(
        const Newline(),
        source: '**foo**\n\n**bar**',
        rows: ['foo', '', 'bar'],
        caret: const DisplayPosition(1, 0),
      );
      session.act(
        const InsertText('x'),
        source: '**foo**\nx\n**bar**',
        rows: ['foo\nx\nbar'],
        caret: const DisplayPosition(0, 5),
      );
      // After the break the caret moves with the text, in its span.
      final after = _Session(backend, source: '**foo\nbar**', caret: 6);
      after.act(
        const Newline(),
        source: '**foo**\n\n**bar**',
        rows: ['foo', '', 'bar'],
        caret: const DisplayPosition(2, 0),
        context: Style.strong,
      );
    });

    test('text typed after link reference definitions shows below them', () {
      final session = _Session(
        backend,
        source:
            '[docs]: https://example.com/docs\n[home]: https://example.com\n',
        caret: 61,
      );
      session.act(
        const InsertText('M'),
        source:
            '[docs]: https://example.com/docs\n[home]: https://example.com\nM',
        rows: [
          '[docs]: https://example.com/docs',
          '[home]: https://example.com',
          'M',
        ],
        caret: const DisplayPosition(2, 1),
      );
      session.act(
        const Newline(),
        rows: [
          '[docs]: https://example.com/docs',
          '[home]: https://example.com',
          'M',
          '',
        ],
        caret: const DisplayPosition(3, 0),
      );
    });
  });

  group('return', () {
    test('at the end of an autolink goes after its hidden closer', () {
      // A click at the row's end takes the autolink's side, before its
      // hidden `>`, where a break would split it. The anchors around the
      // caret show the same place, so the break goes after the `>`, in the
      // containers of the line.
      for (final (source, end, split, rows) in [
        (
          'see <https://x.y>',
          15,
          'see <https://x.y>\n',
          ['see https://x.y', ''],
        ),
        (
          '- see <https://x.y>',
          15,
          '- see <https://x.y>\n- ',
          ['see https://x.y', ''],
        ),
        (
          '> see <https://x.y>',
          15,
          '> see <https://x.y>\n> ',
          ['see https://x.y', ''],
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(PlaceCaret(0, end, leadingHalf: false));
        expect(
          session.editor.selection.extent,
          source.length - 1,
          reason: '$source: the caret is the autolink\'s',
        );
        session.act(
          const Newline(),
          source: split,
          rows: rows,
          caret: const DisplayPosition(1, 0),
        );
        session.act(const InsertText('z'), source: '${split}z');
      }
    });

    test('over a selection continues the containers of its line', () {
      for (final (source, base, extent, split, typed) in [
        ('> ab', 3, 4, '> a\n> ', '> a\n> z'),
        ('- ab', 3, 4, '- a\n- ', '- a\n- z'),
        ('1. ab', 4, 5, '1. a\n2. ', '1. a\n2. z'),
        ('- [ ] ab', 7, 8, '- [ ] a\n- [ ] ', '- [ ] a\n- [ ] z'),
        ('> - ab', 5, 6, '> - a\n> - ', '> - a\n> - z'),
        ('- > ab', 5, 6, '- > a\n  > ', '- > a\n  > z'),
        ('> ab\r\n> cd', 3, 4, '> a\r\n> \r\n> cd', '> a\r\n> z\r\n> cd'),
        ('> # abc', 4, 7, '> # \n> ', '> # \n> z'),
        (
          'x[^1]\n\n[^1]: note',
          14,
          16,
          'x[^1]\n\n[^1]: n\n    e',
          'x[^1]\n\n[^1]: n\n    ze',
        ),
      ]) {
        final session = _Session(backend, source: source);
        session.act(SetSelection(base, extent));
        final before = shells(session);
        session.act(const Newline(), source: split);
        expect(shells(session), before, reason: source);
        session.act(const InsertText('z'), source: typed);
        expect(shells(session), before, reason: source);
      }
    });

    test('over a setext heading in an item keeps the item whole', () {
      // The heading becomes the empty ATX heading deleting its text leaves,
      // and the next item opens after the blank line, where the item's later
      // block stays with it: an empty item before a blank line would end
      // there and leave `para` outside the list.
      final session = _Session(backend, source: '- abc\n  ===\n\n  para');
      session.act(const SetSelection(2, 5));
      session.act(
        const Newline(),
        source: '- # \n\n- \n  para',
        rows: ['', '', '', 'para'],
        caret: const DisplayPosition(2, 0),
      );
      expect(shells(session), [ShellKind.list, ShellKind.item]);
      session.act(
        const InsertText('z'),
        source: '- # \n\n- z\n  para',
        rows: ['', '', 'z\npara'],
      );
      expect(shells(session), [ShellKind.list, ShellKind.item]);
    });

    test('at the end of an item keeps its later blocks in an item', () {
      final session = _Session(backend, source: '2. a\n\n   b', caret: 4);
      session.act(
        const Newline(),
        source: '2. a\n\n3. \n   b',
        rows: ['a', '', '', 'b'],
        caret: const DisplayPosition(2, 0),
      );
      expect(session.editor.projection.rows.last.shells.map((s) => s.kind), [
        ShellKind.list,
        ShellKind.item,
      ]);
      session.act(const InsertText('z'), source: '2. a\n\n3. z\n   b');
    });

    test('keeps the text it moves from reading as markup', () {
      // A marker that starts the moved text is escaped, so it stays the
      // text it was instead of opening a quote, an item, a fence or an
      // underline. The empty link stays whole before the break.
      for (final (source, caret, split, shown) in [
        ('a> b', 1, 'a\n\\> b', 'a\n> b'),
        ('a - b', 1, 'a\n \\- b', 'a\n- b'),
        ('a 1. b', 1, 'a\n 1\\. b', 'a\n1. b'),
        ('a=', 1, 'a\n\\=', 'a\n='),
        ('a```b', 1, 'a\n\\```b', 'a\n```b'),
        ('[]()>', 1, '[]()\n\\>', '\n>'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const Newline(), source: split, rows: [shown]);
      }
    });

    test('keeps the text it leaves from reading as markup', () {
      for (final (source, caret, split, rows) in [
        ('a\\b', 2, 'a\\\\\nb', ['a\\\nb']),
        ('# a # b', 6, '# a \\# \nb', ['a # ', 'b']),
        ('a*b**', 4, 'a*b\\*\n*', ['a*b*\n*']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const Newline(), source: split, rows: rows);
      }
    });

    test('after a hard break ends the paragraph there', () {
      final session = _Session(backend, source: 'a\\\nb', caret: 3);
      session.act(
        const Newline(),
        source: 'a\n\nb',
        rows: ['a', '', 'b'],
        caret: const DisplayPosition(2, 0),
      );
    });

    test('before the whitespace that ends a line breaks after it', () {
      // The spaces of a hard break, or any whitespace that ends the line,
      // stay where they show, as Return at the line's end leaves them.
      // Carried to the new line, they would follow an item's marker there,
      // unshown, and the text typed next would put them on a blank line of
      // their own between the item's lines.
      for (final (source, caret, split, first) in [
        ('- **a  \n  b**', 5, '- **a**  \n- \n  **b**', 'a  '),
        ('- **a  \r\n  b**', 5, '- **a**  \r\n- \r\n  **b**', 'a  '),
        ('- a  \n  b', 3, '- a  \n- \n  b', 'a  '),
        ('- a  \n  b', 4, '- a  \n- \n  b', 'a  '),
        ('- a \n  b', 3, '- a \n- \n  b', 'a '),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          const Newline(),
          source: split,
          rows: [first, '', 'b'],
          caret: const DisplayPosition(1, 0),
        );
        session.act(const InsertText('x'), rows: [first, 'x\nb']);
      }
    });

    test('refuses where no spelling keeps what it splits', () {
      // An autolink and a reference label hold no line break, and closing
      // and reopening `__` beside a literal `__` would pair them anew.
      for (final (source, caret, paragraph) in [
        ('<http://a.b>', 5, false),
        ('[foo]\n\n[foo]: /u', 2, false),
        ('[foo]\n\n[foo]: /u', 9, false),
        ('__foo __bar__baz__', 13, true),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          Newline(paragraph: paragraph),
          applied: false,
          source: source,
        );
      }
    });

    test('on or before a lazy line keeps the quote around both', () {
      for (final (source, caret, paragraph, split, rows) in [
        ('>a\nb', 4, false, '>a\nb\n>', ['a\nb', '']),
        ('>a\nb', 3, false, '>a\n>\n>b', ['a', '', 'b']),
        ('>a\nb\n-', 2, true, '>a\n>\n>b\n-', ['a', '', 'b', '-']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          Newline(paragraph: paragraph),
          source: split,
          rows: rows,
        );
        expect(shells(session), [ShellKind.blockQuote], reason: source);
        expect(
          session.editor.projection.rows
              .where((row) => row.text.isNotEmpty && row.text != '-')
              .every((row) => row.shells.length == 1),
          isTrue,
          reason: source,
        );
      }
    });

    test('before an indented line that would start a block splits it', () {
      // Without its indentation the next line would start a block of its
      // own, so no spelling keeps it a paragraph: Return splits the line as
      // it does without that check, rather than refuse.
      for (final (source, split) in [
        ('a\n    - x', 'a\n\n    - x'),
        ('a\n    # x', 'a\n\n    # x'),
        ('> a\n    > x', '> a\n> \n>     > x'),
      ]) {
        final session = _Session(
          backend,
          source: source,
          caret: source.indexOf('\n'),
        );
        session.act(const Newline(), source: split);
      }
    });

    test('before an indented line keeps that line a paragraph', () {
      // The empty line Return leaves ends the paragraph, so its next line
      // starts a block of its own, which its indentation would make code.
      // Markdown does not show that indentation, so it goes, as pasted
      // indentation does where it would make the row code.
      for (final (source, caret, split) in [
        ('> a\n    b', 3, '> a\n> \n> b'),
        ('> a\n>     b', 3, '> a\n> \n> b'),
        ('> > a\n>     b', 5, '> > a\n> > \n> > b'),
        ('a\n\tb', 1, 'a\n\nb'),
        ('- a\n      b', 3, '- a\n- \n  b'),
        ('> a\r\n    b', 3, '> a\r\n> \r\n> b'),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          const Newline(),
          source: split,
          rows: ['a', '', 'b'],
          caret: const DisplayPosition(1, 0),
        );
        final rows = session.editor.projection.rows;
        expect(rows.last.kind, RowKind.paragraph, reason: source);
        expect(
          rows.last.shells.map((s) => s.kind),
          rows.first.shells.map((s) => s.kind),
          reason: source,
        );
      }
      // Text typed on the new line joins the paragraph again.
      final session = _Session(backend, source: '> a\n    b', caret: 3);
      session.act(const Newline());
      session.act(
        const InsertText('x'),
        source: '> a\n> x\n> b',
        rows: ['a\nx\nb'],
      );
      final paragraph = _Session(backend, source: 'a\n    b', caret: 1);
      paragraph.act(
        const Newline(paragraph: true),
        source: 'a\n\n\nb',
        rows: ['a', '', '', 'b'],
      );
    });

    test('on a lazy line after a quoted definition stays in the quote', () {
      // The paragraph's block opens with the definition, a line before its
      // row, whose first line reads on lazily: the new line takes the
      // quote's prefix from the block's first line, as typing there does.
      for (final (source, split) in [
        ('> [a]: /u\nb', '> [a]: /u\nb\n> '),
        ('> [a]: /u\nb\nc', '> [a]: /u\nb\nc\n> '),
      ]) {
        final session = _Session(backend, source: source, caret: source.length);
        session.act(const Newline(), source: split);
        expect(shells(session), [ShellKind.blockQuote], reason: source);
        session.act(
          const InsertText('d'),
          source: '${split}d',
          rows: ['[a]: /u', '${source.substring(10)}\nd'],
        );
      }
    });

    test('in a heading keeps the next block apart from its text', () {
      // The text after the caret becomes a paragraph, which would read the
      // next line on as part of it: a quote's lazy line, indented code, an
      // underline. A blank line keeps that block apart, as a lift does.
      for (final (source, caret, split, rows) in [
        ('> # a b\nc', 6, '> # a \n> b\n>\nc', ['a ', 'b', '', 'c']),
        ('# a\n    b', 2, '# \na\n\n    b', ['', 'a', '', 'b']),
        ('# a\n-', 2, '# \na\n\n-', ['', 'a', '', '-']),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(
          const Newline(),
          source: split,
          rows: rows,
          caret: const DisplayPosition(1, 0),
        );
      }
    });

    test('inside a definition keeps the definitions after it', () {
      final session = _Session(backend, source: '[a]: /x*\n[b]: /y', caret: 7);
      session.act(
        const Newline(),
        source: '[a]: /x\n*\n\n[b]: /y',
        rows: ['[a]: /x', '*', '', '[b]: /y'],
        caret: const DisplayPosition(1, 0),
      );
      expect(session.editor.projection.rows.last.kind, RowKind.definition);
    });

    test('continues an item or footnote right after a quote marker', () {
      // comrak reads the first space after `>` as the quote's own, so the
      // item's two columns, or the footnote's four, need one more.
      final session = _Session(backend, source: '>- >a', caret: 5);
      session.act(const Newline(), source: '>- >a\n>   >');
      session.act(
        const InsertText('z'),
        source: '>- >a\n>   >z',
        rows: ['a\nz'],
      );
      expect(shells(session), [
        ShellKind.blockQuote,
        ShellKind.list,
        ShellKind.item,
        ShellKind.blockQuote,
      ]);
      final note = _Session(backend, source: '>[^1]: a\n\nx[^1]', caret: 8);
      note.act(const Newline(), source: '>[^1]: a\n>     \n\nx[^1]');
      note.act(
        const InsertText('z'),
        source: '>[^1]: a\n>     z\n\nx[^1]',
        rows: ['a\nz', '', 'x[^1]'],
      );
      expect(shells(note), [
        ShellKind.blockQuote,
        ShellKind.footnoteDefinition,
      ]);
    });

    test('at the end of a footnote in an item or footnote continues it', () {
      // The new line is a blank line Markdown reads outside the footnote and
      // outside the item or footnote around it, while nothing follows it
      // there. It carries their indentation, so text typed on it continues
      // the footnote, as at the document's level.
      for (final (source, caret, split, typed, kinds) in [
        (
          '- [^2]: a\n\nb[^2]',
          9,
          '- [^2]: a\n      \n\nb[^2]',
          '- [^2]: a\n      x\n\nb[^2]',
          [ShellKind.list, ShellKind.item, ShellKind.footnoteDefinition],
        ),
        (
          '[^1]: [^2]: a\n\nb[^2] [^1]',
          13,
          '[^1]: [^2]: a\n        \n\nb[^2] [^1]',
          '[^1]: [^2]: a\n        x\n\nb[^2] [^1]',
          [ShellKind.footnoteDefinition, ShellKind.footnoteDefinition],
        ),
        (
          '[^1]: x\n    [^2]: a\n[^3]: c',
          19,
          '[^1]: x\n    [^2]: a\n        \n[^3]: c',
          '[^1]: x\n    [^2]: a\n        x\n[^3]: c',
          [ShellKind.footnoteDefinition, ShellKind.footnoteDefinition],
        ),
      ]) {
        final session = _Session(backend, source: source, caret: caret);
        session.act(const Newline(), source: split);
        session.act(const InsertText('x'), source: typed);
        expect(shells(session), kinds, reason: source);
        expect(session.editor.document.caretRow.text, 'a\nx', reason: source);
      }
    });

    test('in code on nested item lines indents past their markers', () {
      // Copied, `- - -` would make the new line a rule.
      final session = _Session(backend, source: '- - -     a', caret: 11);
      session.act(const Newline(), source: '- - -     a\n          ');
      session.act(
        const InsertText('z'),
        source: '- - -     a\n          z',
        rows: ['a\nz'],
      );
      expect(session.editor.document.caretRow.kind, RowKind.codeBlock);
      // A split that leaves a rule on an item's line, or that empties the
      // item's first line so the list after it leaves the item, refuses.
      const rule = '-     -      -      x';
      _Session(
        backend,
        source: rule,
        caret: 14,
      ).act(const Newline(), applied: false, source: rule);
      const nested = '-     a\n\n  - b';
      _Session(backend, source: nested)
        ..act(const SetSelection(6, 7))
        ..act(const Newline(), applied: false, source: nested);
    });

    test('at the end of indented code that ends its item continues it', () {
      // The new last line is a blank line Markdown leaves outside the item,
      // and no spelling keeps it inside: it shows outside, carrying the
      // item's and the code's indentation, and text typed on it continues
      // the code in the item.
      final session = _Session(
        backend,
        source: '-     ind\n\n[r]: /u\n',
        caret: 9,
      );
      session.act(
        const Newline(),
        source: '-     ind\n      \n\n[r]: /u\n',
        rows: ['ind', '', '', '[r]: /u', ''],
        anchor: 16,
      );
      session.act(
        const InsertText('x'),
        source: '-     ind\n      x\n\n[r]: /u\n',
        rows: ['ind\nx', '', '[r]: /u', ''],
        caret: const DisplayPosition(0, 5),
      );
      expect(session.editor.document.caretRow.kind, RowKind.codeBlock);
      expect(shells(session), [ShellKind.list, ShellKind.item]);
    });

    test('leaving an empty quote line keeps the next block out of a list', () {
      // Without the quote, `   b` would read on in the item above.
      final session = _Session(backend, source: '1. a\n>\n   b', caret: 6);
      session.act(const Newline(), applied: false, source: '1. a\n>\n   b');
    });

    test('in a table\'s delimiter row shown as its source ends its line', () {
      // Wherever on the row the caret is, Return or Tab opens the next line
      // after it, in its containers: a break inside it would split the row
      // and dissolve the table, and the line after the quote would read on
      // into it.
      for (final command in [const Newline(), const MoveTableCell()]) {
        for (final caret in [10, 13, 15]) {
          final session = _Session(
            backend,
            source: '> | a |\n> | - |\na',
            caret: caret,
          );
          session.act(
            command,
            source: '> | a |\n> | - |\n> \na',
            rows: ['a', '| - |', '', 'a'],
            anchor: 18,
          );
          expect(shells(session), [ShellKind.blockQuote]);
          expect(session.editor.projection.rows.last.shells, isEmpty);
        }
      }
    });

    test('on an empty heading in a container opens a line after it', () {
      // The empty heading's own marker is part of its line's prefix, so
      // leaving the container, as on an empty line, would delete the heading.
      for (final (source, continued) in [
        ('> # ', '> # \n> '),
        ('- # ', '- # \n- '),
      ]) {
        final session = _Session(backend, source: source, caret: source.length);
        session.act(const Newline(), source: continued);
      }
    });

    test('at the last row of a table in a container refuses', () {
      // A table ends at a blank line, which would end the quote or item
      // around it too, so Return there refuses rather than leave both.
      for (final source in [
        '> | a |\n> | - |\n> | b |',
        '- | a |\n  | - |\n  | b |',
      ]) {
        final session = _Session(backend, source: source, caret: 22);
        session.act(const Newline(), applied: false, source: source);
      }
    });

    test('refuses where the new line would leave the row\'s containers', () {
      // From inside a lazy line's leading spaces, the split would leave an
      // empty line outside the quote and move `b` into other containers.
      final session = _Session(backend, source: '> - > a\n  b', caret: 9);
      session.act(const Newline(), applied: false, source: '> - > a\n  b');
    });

    test('leaving a quote opened on an item\'s line stays in the item', () {
      // The item's marker shares the quote's line: copied before the line,
      // as a line leaving a quote on a line of its own keeps the prefix
      // around it, it would open another item.
      for (final (source, left) in [
        ('- a\n- >', '- a\n- '),
        ('1. a\n2. >', '1. a\n2. '),
        ('> - a\n> - >', '> - a\n> - '),
      ]) {
        final session = _Session(backend, source: source, caret: source.length);
        session.act(const Newline(), source: left, rows: ['a', '']);
        session.act(
          const InsertText('x'),
          source: '${left}x',
          rows: ['a', 'x'],
        );
      }
    });
  });
}
