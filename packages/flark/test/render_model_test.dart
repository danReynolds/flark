import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  late FlarkParseBackend backend;
  setUpAll(() => backend = createParseBackend());

  test('schema version matches the generated constants', () {
    expect(backend.schemaVersion, RenderModelSchema.version);
  });

  test('the empty document parses on a fresh backend', () {
    final fresh = createParseBackend();
    final m = fresh.parse('');
    expect(m.blockCount, 1);
    expect(m.runCount, 0);
  });

  test('a paragraph with emphasis decodes into hidden delimiters and content', () {
    final m = backend.parse('say *hi* now');
    expect(m.blockCount, 2); // document + paragraph
    final para = m.blockAt(1);
    expect(para.kind, BlockKind.paragraph);
    expect(para.contentCount, 1);
    final runs = m.runs.toList();
    expect(runs.map((r) => r.kind), [RunKind.text, RunKind.emph, RunKind.text, RunKind.text]);
    final emph = runs[1];
    expect((emph.startUtf16, emph.endUtf16), (4, 8));
    expect((emph.contentStartUtf16, emph.contentEndUtf16), (5, 7));
    expect(runs[2].parent, emph.index);
    expect(m.runsOfBlock(1).length, 4);
    expect(m.runsOfBlock(0), isEmpty);
  });

  test('a run-less block yields an empty run range, not the next block\'s runs', () {
    final m = backend.parse('para\n\n---\n\n*x*');
    final rule = m.blocks.firstWhere((b) => b.kind == BlockKind.thematicBreak);
    expect(m.runsOfBlock(rule.index), isEmpty);
    final last = m.blocks.last;
    expect(m.runsOfBlock(last.index).map((r) => r.kind), [RunKind.emph, RunKind.text]);
  });

  test('an unaligned byte view is accepted', () {
    final m = backend.parse('x');
    final padded = Uint8List(m.bytes.length + 2)..setRange(2, m.bytes.length + 2, m.bytes);
    final view = Uint8List.sublistView(padded, 2);
    expect(RenderModel(view).blockCount, m.blockCount);
  });

  test('every conformance case decodes with consistent counts', () {
    final dir = Directory('${Directory.current.path}/../../test/fixtures/commonmark/upstream');
    var cases = 0;
    for (final file in ['common_mark_tests.json', 'gfm_tests.json']) {
      final list = jsonDecode(File('${dir.path}/$file').readAsStringSync()) as List;
      for (final c in list) {
        final src = (c as Map)['markdown'] as String;
        final m = backend.parse(src);
        expect(m.sourceUtf16, src.length, reason: '$file #${c['example']}');
        expect(m.sourceBytes, utf8.encode(src).length);
        for (final r in m.runs) {
          expect(r.startUtf16 <= r.contentStartUtf16 && r.contentStartUtf16 <= r.contentEndUtf16 && r.contentEndUtf16 <= r.endUtf16, isTrue, reason: '$file #${c['example']} run ${r.index}');
          expect(r.block < m.blockCount, isTrue);
        }
        cases++;
      }
    }
    expect(cases, 1322);
  });

  test('a lone surrogate never faults the native side', () {
    final m = backend.parse('x\uD800y');
    expect(m.blockCount, 2);
  });
}
