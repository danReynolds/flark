import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  late FlarkParseBackend backend;
  setUpAll(() => backend = createParseBackend());

  test('schema version matches the generated constants', () {
    expect(backend.schemaVersion, RenderModelSchema.version);
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

  test('invalid UTF-16 in the Dart string never faults the native side', () {
    // A lone surrogate encodes to U+FFFD through utf8.encode; the parser sees valid UTF-8.
    final m = backend.parse('x\uD800y');
    expect(m.blockCount, 2);
  });
}
