import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/kernel/projection.dart';
import 'package:test/test.dart';

/// Every projection invariant, checked on one source.
void checkInvariants(String src, RenderModel m, Projection p, String label) {
  // Hidden intervals across the document: run delimiters and break markers.
  final hidden = <(int, int)>[];
  for (final r in m.runs) {
    if (r.kind == RunKind.softBreak || r.kind == RunKind.hardBreak) { if (r.endUtf16 > r.startUtf16) hidden.add((r.startUtf16, r.endUtf16)); continue; }
    if (r.contentStartUtf16 > r.startUtf16) hidden.add((r.startUtf16, r.contentStartUtf16));
    if (r.endUtf16 > r.contentEndUtf16) hidden.add((r.contentEndUtf16, r.endUtf16));
  }
  bool inHidden(int a, int b) => hidden.any((h) => h.$1 < b && h.$2 > a);
  final ownedLines = <int>{};
  for (final row in p.rows) {
    // Display text is the concatenation of its segments, in order and contiguous.
    var expectDisplay = 0;
    final rebuilt = StringBuffer();
    for (final s in row.segments) {
      expect(s.displayStart, expectDisplay, reason: '$label row ${row.index}: segments not contiguous');
      expectDisplay = s.displayEnd;
      final piece = row.text.substring(s.displayStart, s.displayEnd);
      rebuilt.write(piece);
      if (s.exact) {
        expect(piece, src.substring(s.sourceStart, s.sourceEnd), reason: '$label row ${row.index}: exact segment differs from source');
        expect(inHidden(s.sourceStart, s.sourceEnd), isFalse, reason: '$label row ${row.index}: display shows hidden bytes ${s.sourceStart}..${s.sourceEnd}');
      }
      expect(s.sourceStart <= s.sourceEnd, isTrue);
    }
    expect(rebuilt.toString(), row.text, reason: '$label row ${row.index}');
    expect(expectDisplay, row.text.length, reason: '$label row ${row.index}: segments do not cover the text');
    // Round trip through every display offset on exact segments.
    for (final s in row.segments.where((s) => s.exact)) {
      for (var d = s.displayStart; d <= s.displayEnd; d++) {
        final source = row.sourceForDisplay(d);
        final (back, snapped) = row.displayForSource(source);
        expect(back, d, reason: '$label row ${row.index}: display $d -> source $source -> display $back');
        expect(snapped, isFalse);
      }
    }
    for (var l = row.firstLine; l < row.firstLine + row.lineCount; l++) { ownedLines.add(l); }
  }
  // Every line is owned by a row or hidden inside a leaf block or table.
  for (var l = 0; l < m.lineCount; l++) {
    if (ownedLines.contains(l)) continue;
    final coveredByLeaf = m.blocks.any((b) { final k = b.kind; final isLeaf = k == BlockKind.paragraph || k == BlockKind.heading || k == BlockKind.codeBlock || k == BlockKind.htmlBlock || k == BlockKind.table; return isLeaf && l >= b.firstLine && l < b.firstLine + b.lineCount; });
    expect(coveredByLeaf, isTrue, reason: '$label: line $l is neither a row nor hidden');
  }
}

void main() {
  late FlarkParseBackend backend;
  setUpAll(() => backend = createParseBackend());

  Projection project(String src) => Projection.of(backend.parse(src), src);

  test('hidden delimiters are gaps between segments', () {
    final p = project('say *hi* now');
    final row = p.rows.single;
    expect(row.text, 'say hi now');
    expect(row.segments.map((s) => (s.sourceStart, s.sourceEnd, s.styles)), [(0, 4, 0), (5, 7, Style.emphasis), (8, 12, 0)]);
    expect(row.sourceForDisplay(4, anchor: Anchor.before), 4, reason: 'before the opening *');
    expect(row.sourceForDisplay(4, anchor: Anchor.after), 5, reason: 'inside the emphasis');
    expect(row.sourceForDisplay(6, anchor: Anchor.before), 7, reason: 'before the closing *');
    expect(row.sourceForDisplay(6, anchor: Anchor.after), 8, reason: 'after the closing *');
    expect(row.displayForSource(4), (4, false));
    expect(row.displayForSource(5), (4, false));
    expect(row.displayForSource(8), (6, false));
  });

  test('rows: prefixes hidden, breaks kept, blank lines are rows', () {
    final p = project('# Title\n\n> quote\n> more\n\n- item\n-\n');
    expect(p.rows.map((r) => (r.kind, r.text)), [
      (RowKind.heading, 'Title'), (RowKind.blank, ''), (RowKind.paragraph, 'quote\nmore'), (RowKind.blank, ''), (RowKind.paragraph, 'item'), (RowKind.blank, ''), (RowKind.blank, ''),
    ]);
    expect(p.rows[2].shells.map((s) => s.kind), [ShellKind.blockQuote]);
    expect(p.rows[4].shells.map((s) => s.kind), [ShellKind.list, ShellKind.item]);
    expect(p.rows[5].shells.map((s) => s.kind), [ShellKind.list, ShellKind.item], reason: 'the empty item is a blank row inside the item');
    expect(p.rows[0].headingLevel, 1);
  });

  test('entities, code spans, links, and tasks project their display', () {
    final p = project('a &amp; `b` [c](u "t") ![d](i)\n\n- [x] done\n\n```dart\nx\n```\n\n[ref]: /u\n');
    expect(p.rows[0].text, 'a & b c d');
    final link = p.rows[0].segments.firstWhere((s) => s.styles & Style.link != 0);
    expect(p.source.substring(link.sourceStart, link.sourceEnd), 'c');
    final task = p.rows[2];
    expect(task.text, 'done');
    expect(task.shells.last.task, isTrue); expect(task.shells.last.checked, isTrue);
    expect(p.source.substring(task.shells.last.checkboxStart, task.shells.last.checkboxEnd), '[x]');
    final code = p.rows.firstWhere((r) => r.kind == RowKind.codeBlock);
    expect(code.text, 'x'); expect(code.fenced, isTrue); expect(p.source.substring(code.codeInfoStart, code.codeInfoEnd), 'dart');
    expect(p.rows.last.kind, RowKind.blank);
    expect(p.rows.firstWhere((r) => r.kind == RowKind.definition).text, '[ref]: /u');
  });

  test('display positions resolve out of hidden bytes', () {
    final p = project('**b** x');
    expect(p.displayForSource(1), const DisplayPosition(0, 0, snapped: true));
    expect(p.displayForSource(2), const DisplayPosition(0, 0));
    expect(p.displayForSource(5), const DisplayPosition(0, 1));
  });

  test('projection invariants hold across the conformance corpora', () {
    final dir = Directory('${Directory.current.path}/../../test/fixtures/commonmark/upstream');
    var cases = 0;
    for (final file in ['common_mark_tests.json', 'gfm_tests.json']) {
      final list = jsonDecode(File('${dir.path}/$file').readAsStringSync()) as List;
      for (final c in list) {
        final src = ((c as Map)['markdown'] as String).replaceAll('\r\n', '\n').replaceAll('\r', '\n');
        final m = backend.parse(src);
        final p = Projection.of(m, src);
        checkInvariants(src, m, p, '$file #${c['example']}');
        cases++;
      }
    }
    expect(cases, 1322);
  });
}
