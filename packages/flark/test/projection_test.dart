import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

/// Every projection invariant, checked on one source.
void main() {
  late FlarkParseBackend backend;
  setUpAll(() => backend = createParseBackend());

  Projection project(String src) => Projection.of(backend.parse(src), src);

  test('hidden delimiters are gaps between segments', () {
    final p = project('say *hi* now');
    final row = p.rows.single;
    expect(row.text, 'say hi now');
    expect(row.segments.map((s) => (s.sourceStart, s.sourceEnd, s.styles)), [
      (0, 4, 0),
      (5, 7, Style.emphasis),
      (8, 12, 0),
    ]);
    expect(
      row.sourceForDisplay(4, anchor: Anchor.before),
      4,
      reason: 'before the opening *',
    );
    expect(
      row.sourceForDisplay(4, anchor: Anchor.after),
      5,
      reason: 'inside the emphasis',
    );
    expect(
      row.sourceForDisplay(6, anchor: Anchor.before),
      7,
      reason: 'before the closing *',
    );
    expect(
      row.sourceForDisplay(6, anchor: Anchor.after),
      8,
      reason: 'after the closing *',
    );
    expect(row.displayForSource(4), (4, false));
    expect(row.displayForSource(5), (4, false));
    expect(row.displayForSource(8), (6, false));
  });

  test('rows: prefixes hidden, breaks kept, blank lines are rows', () {
    final p = project('# Title\n\n> quote\n> more\n\n- item\n- \n');
    expect(p.rows.map((r) => (r.kind, r.text)), [
      (RowKind.heading, 'Title'),
      (RowKind.blank, ''),
      (RowKind.paragraph, 'quote\nmore'),
      (RowKind.blank, ''),
      (RowKind.paragraph, 'item'),
      (RowKind.blank, ''),
      (RowKind.blank, ''),
    ]);
    expect(p.rows[2].shells.map((s) => s.kind), [ShellKind.blockQuote]);
    expect(p.rows[4].shells.map((s) => s.kind), [
      ShellKind.list,
      ShellKind.item,
    ]);
    expect(
      p.rows[5].shells.map((s) => s.kind),
      [ShellKind.list, ShellKind.item],
      reason: 'the empty item is a blank row inside the item',
    );
    expect(p.rows[0].headingLevel, 1);
  });

  test('CRLF source remains exact through projection', () {
    const src = '[ref]: /u\r\n  "title"\r\n\r\n*a*\r\n';
    final document = FlarkDocument.load(src, backend);
    final model = document.model;
    final p = document.projection;

    expect(p.source, src);
    final definition = p.rows.firstWhere(
      (row) => row.kind == RowKind.definition,
    );
    expect(definition.text, '[ref]: /u\n  "title"');
    expect(
      definition.segments.where((segment) => !segment.exact),
      hasLength(1),
    );
    final cr = src.indexOf('\r');
    expect(document.isLegal(cr), isTrue);
    expect(document.isLegal(cr + 1), isFalse);
    expect(document.isLegal(cr + 2), isTrue);
    expect(document.isLegal(src.indexOf('title') + 2), isTrue);
    expect(p.rows.firstWhere((row) => row.text == 'a').text, 'a');
    checkInvariants(src, model, p, 'CRLF exact source');
  });

  test('entities, code spans, links, and tasks project their display', () {
    final p = project(
      'a &amp; `b` [c](u "t") ![d](i)\n\n- [x] done\n\n```dart\nx\n```\n\n[ref]: /u\n',
    );
    expect(p.rows[0].text, 'a & b c d');
    final link = p.rows[0].segments.firstWhere(
      (s) => s.styles & Style.link != 0,
    );
    expect(p.source.substring(link.sourceStart, link.sourceEnd), 'c');
    final task = p.rows[2];
    expect(task.text, 'done');
    expect(task.shells.last.task, isTrue);
    expect(task.shells.last.checked, isTrue);
    expect(
      p.source.substring(
        task.shells.last.checkboxStart,
        task.shells.last.checkboxEnd,
      ),
      '[x]',
    );
    final code = p.rows.firstWhere((r) => r.kind == RowKind.codeBlock);
    expect(code.text, 'x');
    expect(code.fenced, isTrue);
    expect(p.source.substring(code.codeInfoStart, code.codeInfoEnd), 'dart');
    expect(p.rows.last.kind, RowKind.blank);
    expect(
      p.rows.firstWhere((r) => r.kind == RowKind.definition).text,
      '[ref]: /u',
    );
  });

  test('display positions resolve out of hidden bytes', () {
    final p = project('**b** x');
    expect(p.displayForSource(1), const DisplayPosition(0, 0, snapped: true));
    expect(p.displayForSource(2), const DisplayPosition(0, 0));
    expect(p.displayForSource(5), const DisplayPosition(0, 1));
    expect(
      const DisplayPosition(0, 0),
      isNot(const DisplayPosition(0, 0, snapped: true)),
    );
  });

  test('a paragraph after link reference definitions shows after them', () {
    // comrak starts the paragraph, and a setext heading made of one, at the
    // definitions it took from its start. Its row starts where its text does,
    // after the definition rows, in quotes and items too.
    for (final (source, lines) in [
      (
        '[docs]: https://example.com/docs\n[home]: https://example.com\n'
            'More text',
        [0, 1, 2],
      ),
      ('[a]: /u\r\n[b]: /v\r\nTitle\r\n===\r\n', [0, 1, 2, 4]),
      ('> [a]: /u\n> [b]: /v\n> text', [0, 1, 2]),
      ('- [a]: /u\n  [b]: /v\n  text', [0, 1, 2]),
    ]) {
      final p = project(source);
      checkInvariants(source, p.model, p, source);
      expect(p.rows.map((r) => r.firstLine), lines, reason: source);
      final leaf = p.rows.firstWhere((r) => r.kind != RowKind.definition);
      expect(
        (leaf.lineCount, leaf.contentStarts.first >= 0),
        (leaf.kind == RowKind.heading ? 2 : 1, true),
        reason: source,
      );
      expect(p.rowsOnLine(0), [0], reason: source);
    }
  });

  test('an escaped table pipe is one mapped rendered grapheme', () {
    const src = '| a |\n|---|\n| \\|*x* |\n';
    final p = project(src);
    final row = p.rows.firstWhere((r) => r.text == '|x ');
    final slash = src.indexOf(r'\|');
    expect(row.sourceForDisplay(0, anchor: Anchor.after), slash);
    expect(row.sourceForDisplay(1, anchor: Anchor.before), slash + 2);
    expect(row.displayForSource(slash), (0, false));
    expect(row.displayForSource(slash + 2), (1, false));
  });

  test('an escaped table pipe does not absorb its exact-text suffix', () {
    const src = '| a \\| b |\n|---|\n| c |\n';
    final document = FlarkDocument.load(src, backend);
    final row = document.projection.rows.firstWhere(
      (row) => row.text == 'a | b ',
    );
    final slash = src.indexOf(r'\|');

    expect(
      row.segments.map(
        (segment) => (segment.sourceStart, segment.sourceEnd, segment.exact),
      ),
      containsAllInOrder([
        (slash, slash + 2, false),
        (slash + 2, slash + 4, true),
      ]),
    );
    expect(document.isLegal(slash + 1), isFalse);
    expect(document.isLegal(slash + 2), isTrue);
    expect(document.isLegal(slash + 3), isTrue);
  });

  test('flat-list item ordinals remain correct at scale', () {
    final src = List.generate(5000, (i) => '- item $i').join('\n');
    final p = project(src);
    final items = p.rows
        .where((r) => r.shells.any((s) => s.kind == ShellKind.item))
        .toList();
    expect(items, hasLength(5000));
    expect(items.first.shells.last.itemIndex, 0);
    expect(items[2499].shells.last.itemIndex, 2499);
    expect(items.last.shells.last.itemIndex, 4999);
  });

  test('wide-table cell ordinals remain correct at scale', () {
    const columns = 2000;
    final header = '| ${List.generate(columns, (i) => 'h$i').join(' | ')} |';
    final delimiter = '| ${List.filled(columns, '---').join(' | ')} |';
    final body = '| ${List.generate(columns, (i) => 'v$i').join(' | ')} |';
    final p = project('$header\n$delimiter\n$body\n');
    final headerCells = p.rows
        .where((row) => row.kind == RowKind.tableCell && row.header)
        .toList();
    final bodyCells = p.rows
        .where((row) => row.kind == RowKind.tableCell && !row.header)
        .toList();

    expect(headerCells, hasLength(columns));
    expect(bodyCells, hasLength(columns));
    expect(headerCells.first.column, 0);
    expect(headerCells.last.column, columns - 1);
    expect(bodyCells.first.column, 0);
    expect(bodyCells.last.column, columns - 1);
  });

  test('a row presents the same wherever its source moved', () {
    // Hosts keep a row's shaped text while it presents the same.
    // A table cell's text keeps the cell's padding.
    ProjectedRow rowOf(String src, String text) =>
        project(src).rows.firstWhere((row) => row.text.trim() == text);
    final plain = rowOf('say hi', 'say hi');
    expect(plain.samePresentation(rowOf('x\n\nsay hi', 'say hi')), isTrue);
    for (final src in ['say *hi*', '# say hi', '> say hi']) {
      expect(plain.samePresentation(rowOf(src, 'say hi')), isFalse);
    }
    expect(plain.samePresentation(rowOf('say ho', 'say ho')), isFalse);
    // A table cell's header and alignment.
    final cell = rowOf('| a |\n| - |\n| b |', 'b');
    expect(cell.samePresentation(rowOf('| x |\n| - |\n| b |', 'b')), isTrue);
    expect(cell.samePresentation(rowOf('| a |\n| :-: |\n| b |', 'b')), isFalse);
    expect(cell.samePresentation(rowOf('| b |\n| - |\n| a |', 'b')), isFalse);
  });

  test('projection invariants hold across the conformance corpora', () {
    final dir = Directory(
      '${Directory.current.path}/../../test/fixtures/commonmark/upstream',
    );
    var cases = 0;
    for (final file in ['common_mark_tests.json', 'gfm_tests.json']) {
      final list =
          jsonDecode(File('${dir.path}/$file').readAsStringSync()) as List;
      for (final c in list) {
        final src = (c as Map)['markdown'] as String;
        final m = backend.parse(src);
        final p = Projection.of(m, src);
        checkInvariants(src, m, p, '$file #${c['example']}');
        cases++;
      }
    }
    expect(cases, 1322);
  });
}
