import 'package:flark/flark.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/code.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  tearDownAll(code.dispose);
  final backend = createParseBackend();
  const languageCases = {
    'dart': ('void main() {', '}'),
    'python': ('values = [', ']'),
    'ruby': ('values = [', ']'),
    'javascript': ('function run() {', '}'),
    'typescript': ('function run(): void {', '}'),
    'rust': ('fn main() {', '}'),
    'go': ('func main() {', '}'),
    'json': ('{"values": [', ']'),
    'yaml': ('settings: {', '}'),
    'sql': ('SELECT COALESCE(', ')'),
    'bash': ('function run() {', '}'),
    'html': ('<div>', '</div>'),
    'xml': ('<note>', '</note>'),
    'css': ('.card {', '}'),
  };
  test('each registered grammar has a declared closer behavior', () {
    expect(
      languageCases.keys.toSet(),
      codeLanguages.keys.where((k) => k != 'text').toSet(),
    );
  });
  for (final entry in languageCases.entries) {
    test('${entry.key}: shared closer contract or conservative fallback', () {
      final (opener, closer) = entry.value;
      final source = '```${entry.key}\n$opener\n    \n```';
      final at = source.lastIndexOf('\n```');
      final e = FlarkEditor(
        backend,
        codeEditing: code,
        text: source,
        caret: at,
      );
      for (final rune in closer.runes) {
        e.apply(InsertText(String.fromCharCode(rune)));
      }
      expect(e.source, '```${entry.key}\n$opener\n$closer\n```');
    });
  }
  test('unsupported tags and plain text retain literal indentation', () {
    for (final language in ['text', 'unknown-language']) {
      final source = '```$language\nif (ready) {\n  \n```';
      final at = source.lastIndexOf('\n```');
      final e = FlarkEditor(
        backend,
        codeEditing: code,
        text: source,
        caret: at,
      );
      e.apply(const InsertText('}'));
      expect(e.source, source.replaceRange(at, at, '}'));
    }
  });
  test('JavaScript regex delimiters do not close the surrounding block', () {
    const source = '```js\nfunction run() {\n  const re = /[}]/;\n  \n```';
    final at = source.lastIndexOf('\n```');
    final e = FlarkEditor(backend, codeEditing: code, text: source, caret: at);
    e.apply(const InsertText('}'));
    expect(e.source, '```js\nfunction run() {\n  const re = /[}]/;\n}\n```');
  });
  for (final (name, language, prefix, eol, body, closer, expected) in [
    (
      'owner example',
      '',
      '',
      '\n',
      'for (final x in [1,2,3]) {\n  ',
      '}',
      'for (final x in [1,2,3]) {\n}',
    ),
    (
      'nested brace',
      'dart',
      '',
      '\n',
      'void main() {\n  if (ready) {\n    ',
      '}',
      'void main() {\n  if (ready) {\n  }',
    ),
    (
      'bracket',
      'dart',
      '',
      '\n',
      'final values = [\n  ',
      ']',
      'final values = [\n]',
    ),
    ('parenthesis', 'dart', '', '\n', 'call(\n  ', ')', 'call(\n)'),
    ('tabs', 'go', '', '\n', '\tif ready {\n\t\t', '}', '\tif ready {\n\t}'),
    (
      'quoted list and CRLF',
      'dart',
      '>   ',
      '\r\n',
      'for (;;) {\n  ',
      '}',
      'for (;;) {\n}',
    ),
    (
      'ignore closed literals',
      'dart',
      '',
      '\n',
      'if (ready) {\n  print("}"); /* } */\n  ',
      '}',
      'if (ready) {\n  print("}"); /* } */\n}',
    ),
  ]) {
    test('typed closer aligns with opener: $name', () {
      final opener = prefix.isEmpty ? '```$language' : '> - ```$language';
      String fenced(String code) =>
          '$opener$eol$prefix${code.replaceAll('\n', '$eol$prefix')}$eol$prefix```\n\nafter';
      final source = fenced(body), result = fenced(expected);
      final at = source.indexOf('$eol$prefix```');
      final e = FlarkEditor(
        backend,
        codeEditing: code,
        text: source,
        caret: at,
      );
      expect(e.apply(InsertText(closer)), isTrue);
      expect(e.source, result);
      final caret = result.indexOf('$eol$prefix```');
      expect(e.selection.extent, caret);
      expect(e.apply(const InsertText(';')), isTrue);
      expect(e.source, result.replaceRange(caret, caret, ';'));
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), (result, caret));
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), (source, at));
      expect(e.apply(const Redo()), isTrue);
      expect((e.source, e.selection.extent), (result, caret));
    });
  }
  for (final body in [
    '  ',
    'if (ready) {  ',
    'call([\n  ',
    'if (ready) {\n  /*\n    ',
    'final text = """{\n  ',
  ]) {
    test(
      'unmatched, inline, comment or string closer stays literal: $body',
      () {
        final source = '```dart\n$body\n```';
        final at = source.lastIndexOf('\n```');
        final e = FlarkEditor(
          backend,
          codeEditing: code,
          text: source,
          caret: at,
        );
        expect(e.apply(const InsertText('}')), isTrue);
        expect(e.source, source.replaceRange(at, at, '}'));
        expect(e.selection.extent, at + 1);
      },
    );
  }
  test(
    'paste, replacement, composition and source mode retain literal whitespace',
    () {
      const source = '```dart\nif (ready) {\n  \n```';
      final at = source.lastIndexOf('\n```');
      for (final mode in [
        'paste',
        'replace',
        'selection',
        'composition',
        'source',
      ]) {
        final e = FlarkEditor(
          backend,
          codeEditing: code,
          text: source,
          caret: at,
        );
        if (mode == 'composition') e.beginComposition();
        if (mode == 'source') e.setSourceMode(true);
        if (mode == 'selection') e.apply(SetSelection(at - 1, at));
        e.apply(switch (mode) {
          'paste' => const Paste('}'),
          'replace' => ReplaceRange(at, at, '}'),
          _ => const InsertText('}'),
        });
        final start = mode == 'selection' ? at - 1 : at;
        // Typed selection replacement follows the shared syntax proposal;
        // paste, explicit range replacement, IME and source mode stay literal.
        expect(
          e.source,
          source.replaceRange(mode == 'selection' ? at - 2 : start, at, '}'),
          reason: mode,
        );
      }
    },
  );
}
