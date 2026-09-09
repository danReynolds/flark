import 'package:test/test.dart';
import 'package:flark_tree_sitter/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';

void main() {
  final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  tearDownAll(code.dispose);
  final h = code;
  test(
    'Ruby method highlights through explicit, alias and Automatic modes',
    () {
      const source = 'def test\n\nend';
      for (final language in ['ruby', 'rb', '']) {
        final result = h.highlight(source, language);
        expect(result.language, 'ruby', reason: language);
        expect(result.kindAt(0), 'keyword');
        expect(result.kindAt(source.indexOf('test')), 'function');
        expect(result.kindAt(source.indexOf('end')), 'keyword');
        expect(
          result.tokens.map((t) => source.substring(t.start, t.end)).join(),
          source,
        );
      }
    },
  );
  test(
    'known tag, alias and automatic detection preserve exact token text',
    () {
      const source = '{"answer": 42, "ok": true}';
      for (final language in ['json', '']) {
        final r = h.highlight(source, language);
        expect(r.language, 'json');
        expect(
          r.tokens.map((t) => source.substring(t.start, t.end)).join(),
          source,
        );
        expect(r.kindAt(source.indexOf('42')), 'number');
        expect(h.highlight(source, language), same(r));
      }
      expect(h.highlight('const x = 1;', 'js').language, 'javascript');
    },
  );
  test(
    'unknown tags, plain text and oversized blocks retain their exact text',
    () {
      for (final language in ['text', 'not-a-language']) {
        final r = h.highlight('const x = 1;', language);
        expect(r.language, isNull);
        expect(r.tokens.single.kind, isNull);
      }
      expect(h.highlight('x' * 8193, 'dart').tokens.single.end, 8193);
    },
  );
  test('supported grammars leave Unicode, tabs and newlines untouched', () {
    const source = '// 😀 café\nconst text = "λ";\n\t42';
    for (final language in codeLanguages.keys) {
      final r = h.highlight(source, language);
      expect(
        r.tokens.map((t) => source.substring(t.start, t.end)).join(),
        source,
        reason: language,
      );
    }
  });
}
