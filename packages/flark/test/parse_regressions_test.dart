// Regressions for documents the parse crate refused, published with text
// shown twice, or could not parse without overflowing the stack. The crate's
// own regressions (native/flark_parse/tests/regressions.rs) pin the ranges;
// these pin what the editor shows and admits.
import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

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

  test(
    'tables that could gain more cells than comrak creates safely are refused',
    () {
      // comrak gives every body row its header's columns, creating the cells
      // a row lacks without the cap it means to apply: this 12 KB table, in
      // the editor's live limits even on a phone, made 4.2 million cells and
      // took 2 GB. It is refused before comrak parses it and opens as source.
      String wide(int columns, int rows) =>
          '${'a|' * columns}\n${'-|' * columns}${'\nx' * rows}';
      final huge = wide(2048, 2046);
      expect(
        () => backend.parse(huge),
        throwsA(
          isA<FlarkParseException>().having(
            (e) => e.code,
            'code',
            FlarkParseException.extractionDeviationCode,
          ),
        ),
      );
      expect(FlarkEditor(backend, text: huge).sourceMode, isTrue);
      // A table whose rows fill their columns stays live.
      final full = '| a | b |\n|---|---|\n${'| x | y |\n' * 100}';
      expect(FlarkEditor(backend, text: full).sourceMode, isFalse);
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

  test('a task checkbox after its item\'s definitions is the one toggled', () {
    // comrak resolves an item's definitions before it looks for a checkbox,
    // so the first `[x]` of these items is a definition's destination. Taken
    // for the checkbox, it refused the first document, and in the second a
    // toggle rewrote the definition.
    for (final (text, toggled) in [
      ('1. [a]:[x]\n[x]', '1. [a]:[x]\n[ ]'),
      ('1. [a]:[x]\n[x] b', '1. [a]:[x]\n[ ] b'),
    ]) {
      final e = FlarkEditor(backend, text: text, caret: text.length);
      expect(e.sourceMode, isFalse, reason: text);
      checkInvariants(e.source, e.document.model, e.projection, text);
      expect(e.apply(const ToggleTask()), isTrue, reason: text);
      expect(e.source, toggled);
    }
  });

  test('a task checkbox followed by a reference to whitespace loads live', () {
    // comrak scans for the checkbox in decoded text, so the reference is the
    // whitespace after it and the text begins after the reference. The
    // content began at the reference, outside comrak's paragraph, and the
    // document opened in source mode.
    for (final text in ['- [ ]&#9;x', '- [ ]&#10;x']) {
      final e = FlarkEditor(backend, text: text, caret: text.length);
      expect(e.sourceMode, isFalse, reason: text);
      checkInvariants(e.source, e.document.model, e.projection, text);
      final row = e.projection.rows.single;
      expect((row.kind, row.text), (RowKind.paragraph, 'x'), reason: text);
      expect(row.shells.last.task, isTrue, reason: text);
      expect(e.apply(const InsertText('y')), isTrue, reason: text);
      expect(e.source, '${text}y');
      expect(e.apply(const ToggleTask()), isTrue, reason: text);
      expect(e.source, '${text.replaceFirst('[ ]', '[x]')}y');
    }
  });

  test('an angle destination closed at the end of the document stays '
      'paragraph text', () {
    // Matrix seed 5023. comrak takes no angle-bracketed destination that
    // reaches the end of an unterminated last line: typing the `>` leaves a
    // paragraph. The crate also recorded a definition over it, and the
    // editor showed that definition's row over bytes the paragraph hid.
    const text =
        "[Foo*bar\\]]:my_(url) 'title (with ~😀bbarens)'\n\n[Foo*bar\\]]:<";
    final e = FlarkEditor(backend, text: text, caret: 61);
    expect(
      e.apply(const InsertText('>')),
      isTrue,
      reason: '${e.lastRejection}',
    );
    checkInvariants(e.source, e.document.model, e.projection, 'typed >');
    expect(
      [
        for (final r in e.projection.rows)
          if (r.kind != RowKind.blank) (r.kind, r.text),
      ],
      [
        (RowKind.definition, "[Foo*bar\\]]:my_(url) 'title (with ~😀bbarens)'"),
        (RowKind.paragraph, 'Foo*bar]:<>'),
      ],
    );
    // A line ending after it makes it a definition, as comrak reads it.
    expect(e.apply(const Newline()), isTrue, reason: '${e.lastRejection}');
    checkInvariants(e.source, e.document.model, e.projection, 'newline');
    expect(e.document.model.definitionCount, 2);
  });

  test('a space after a backslash ending an angle destination\'s line '
      'leaves paragraph text', () {
    // Matrix seed 7287. comrak reads definitions from lines that keep their
    // trailing spaces, and in angle brackets a backslash takes the byte after
    // it: here the typed space, so the line ending ends the destination. The
    // crate trimmed the space and let the backslash take the line ending, and
    // the editor showed a definition's row over the paragraph's hidden text.
    const text = "[Foo bar]:\n<my [\\\nurl>\n'title'\n\n[Fo1 bar]\n]";
    final e = FlarkEditor(backend, text: text, caret: 17);
    expect(e.document.model.definitionCount, 1);
    expect(
      e.apply(const InsertText(' ')),
      isTrue,
      reason: '${e.lastRejection}',
    );
    checkInvariants(e.source, e.document.model, e.projection, 'typed space');
    expect(e.document.model.definitionCount, 0);
    expect(
      [
        for (final r in e.projection.rows)
          if (r.kind != RowKind.blank) r.kind,
      ],
      [RowKind.paragraph, RowKind.paragraph],
    );
  });

  test('a code span shows its unescaped pipe in a cell and its source '
      'across a split paragraph\'s lines', () {
    // comrak strips a cell's code span after unescaping its pipes; the crate
    // stripped first, matched no literal and showed the cell's source.
    final cell = FlarkEditor(backend, text: '| a |\n|---|\n| ` \\| ` |\n');
    expect(cellText(cell), '|');
    // Matrix seed 7308. A span crossing lines of a paragraph split to make a
    // table header drops an escaped pipe's backslash, which display text
    // cannot show across a line: the paragraph now shows its source.
    const text =
        '`\n\n### b``r> q*{> qé``\n``aé\n|{)\\|p**oo!``\n| a |\n| - |\n'
        '| b **p**|';
    final e = FlarkEditor(backend, text: text);
    expect(e.sourceMode, isFalse);
    checkInvariants(e.source, e.document.model, e.projection, 'split code');
    final row = e.projection.rows.firstWhere(
      (r) => r.kind == RowKind.paragraph && r.text.startsWith('``'),
    );
    expect(row.text, '``aé\n|{)\\|p**oo!``');
  });
}
