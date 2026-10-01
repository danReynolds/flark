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
      // stack. Packed as tightly as comrak links them, five bytes each, the
      // widest line the editor parses by default links 819 and stays live.
      String packed(int n) => 'a@b.c${'+@d.e' * (n - 1)}\n';
      expect(FlarkEditor(backend, text: packed(819)).sourceMode, isFalse);
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
}
