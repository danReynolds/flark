// Regressions for documents the parse crate refused, published with text
// shown twice, or could not parse without overflowing the stack. The crate's
// own regressions (native/flark_parse/tests/regressions.rs) pin the ranges;
// these pin what the editor shows and admits.
import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();

  String cellText(FlarkEditor e) => e.projection.rows
      .lastWhere((r) => r.kind == RowKind.tableCell)
      .text
      .trim();

  test('a document led by a byte order mark loads live and keeps it first', () {
    final editor = FlarkEditor(backend, text: 'x');
    expect(editor.loadMarkdown('\u{FEFF}hello *world*\n'), isTrue);
    expect(editor.sourceMode, isFalse);
    expect(editor.projection.rows.first.text, 'hello world');
    // The mark is neither content nor a prefix: the caret starts after it.
    expect(editor.selection.extent, 1);
    expect(editor.apply(const InsertText('x')), isTrue);
    expect(editor.source, '\u{FEFF}xhello *world*\n');
  });

  test('an entity before a cell\'s escaped pipe shows only its decoding', () {
    final e = FlarkEditor(backend, text: '| a |\n|---|\n| &amp;\\| |\n');
    expect(cellText(e), '&|');
  });

  test('text after a link whose title spans lines shows once', () {
    final e = FlarkEditor(backend, text: '[a](/u "t\nt")\nsee &amp; ok\n');
    final row = e.projection.rows.firstWhere(
      (r) => r.kind == RowKind.paragraph,
    );
    expect(row.text.split('\n').last, 'see & ok');
  });

  test('definitions a setext underline resolved stay above a table', () {
    final e = FlarkEditor(
      backend,
      text: '[r]: /ref\n---\n| a | b |\n|---|---|\n',
    );
    expect(e.sourceMode, isFalse);
    expect(
      [
        for (final r in e.projection.rows)
          if (r.kind != RowKind.blank) (r.kind, r.text.trim()),
      ],
      [
        (RowKind.definition, '[r]: /ref'),
        (RowKind.paragraph, '---'),
        (RowKind.tableCell, 'a'),
        (RowKind.tableCell, 'b'),
      ],
    );
  });

  test('a definition title may hold NUL', () {
    final m = backend.parse('[a]: /u "x\u0000y"\n\n[a]\n');
    expect(m.definitionCount, 1);
  });

  test(
    'a line with more email addresses than comrak links safely is refused',
    () {
      // comrak links the addresses of one text node recursively, so a line
      // that could link more than 1,024 is refused rather than risk the
      // stack. Packed as tightly as comrak links them, four bytes each
      // (`c@.r` is an address), the widest line the editor parses by
      // default, 4,096 code units, links 1,024 and stays live.
      String packed(int n) => 'a@.b${'+@.c' * (n - 1)}\n';
      expect(packed(1024).length, 4096 + 1);
      expect(FlarkEditor(backend, text: packed(1024)).sourceMode, isFalse);
      expect(backend.parse(packed(1024)).runCount, greaterThan(1024));
      expect(
        () => backend.parse(packed(1025)),
        throwsA(
          isA<FlarkParseException>().having(
            (e) => e.code,
            'code',
            FlarkParseException.extractionDeviationCode,
          ),
        ),
      );
      // A line of `@` signs as wide as the editor takes links nothing.
      expect(FlarkEditor(backend, text: '${'@' * 4000}\n').sourceMode, isFalse);
    },
  );

  test('a table after a blank line under definitions a setext line resolved '
      'loads live and admits its delimiter row', () {
    // Only a table that split the paragraph is numbered from the
    // definitions' stale start. Moving one that came from a paragraph of its
    // own left its header line in no block: the document opened in source
    // mode, and the keystroke that made the delimiter row was refused.
    const doc =
        '[home]: https://example.com\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n';
    final e = FlarkEditor(backend, text: doc);
    expect(e.sourceMode, isFalse);
    expect(
      [
        for (final r in e.projection.rows)
          if (r.kind != RowKind.blank) (r.kind, r.text.trim()),
      ],
      [
        (RowKind.definition, '[home]: https://example.com'),
        (RowKind.paragraph, '---'),
        (RowKind.tableCell, 'a'),
        (RowKind.tableCell, 'b'),
        (RowKind.tableCell, '1'),
        (RowKind.tableCell, '2'),
      ],
    );
    const typed = '[home]: https://example.com\n---\n\n| a |\n|';
    final t = FlarkEditor(backend, text: typed, caret: typed.length);
    expect(
      t.apply(const InsertText('-')),
      isTrue,
      reason: '${t.lastRejection}',
    );
    expect(t.sourceMode, isFalse);
    expect(cellText(t), 'a');
  });

  test('an escaped pipe before a pipe reference in a cell shows two pipes', () {
    // A text opening with `\|&#124;` lost the backslash's own piece: alone
    // the cell showed its source, inside emphasis the backslash showed.
    for (final cell in ['\\|&#124;', '*\\|&#124;*', '[\\|&#124;](u)']) {
      final e = FlarkEditor(backend, text: '| a |\n|---|\n| $cell |\n');
      expect(cellText(e), '||', reason: cell);
    }
  });

  test('text after a link whose parentheses span lines keeps an escaped '
      'ampersand and a lone entity', () {
    String lastLine(String doc) => FlarkEditor(backend, text: doc)
        .projection
        .rows
        .firstWhere((r) => r.kind == RowKind.paragraph)
        .text
        .split('\n')
        .last;
    expect(lastLine('[r]: /ref\n---\n[a](/u "t\n2")\\&amp;\n'), 'a&amp;');
    expect(lastLine('[](\n)\n&amp;\\&\n'), '&&');
  });
}
