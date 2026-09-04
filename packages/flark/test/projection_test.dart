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
