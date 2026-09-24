import 'package:flark/code.dart';
import 'package:flark/flark.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:test/test.dart';

void main() {
  final code = FlarkCodeMirror();

  String? kindOf(CodeHighlight h, String source, String text, [int from = 0]) =>
      h.kindAt(source.indexOf(text, from));

  group('highlight', () {
    test('tokens tile the snippet, including Unicode, tabs and newlines', () {
      const source = '// 😀 café\nconst text = "λ";\n\t42\r\n\n}';
      for (final language in [...codeMirrorLanguages.keys, 'text', 'kotlin']) {
        final h = code.highlight(source, language);
        expect(
          h.tokens.map((t) => source.substring(t.start, t.end)).join(),
          source,
          reason: language,
        );
        for (var i = 1; i < h.tokens.length; i++) {
          expect(h.tokens[i].start, h.tokens[i - 1].end);
          expect(h.tokens[i].kind, isNot(h.tokens[i - 1].kind));
        }
      }
      expect(code.highlight('', 'js').tokens.single.end, 0);
    });

    test('JavaScript kinds', () {
      const source =
          'import { load } from "./x";\n'
          'const MAX_SIZE = 10, ratio = .5; // note\n'
          'function build(items) {\n'
          '  return new Map(items).get(`k\${ratio}`) ?? null;\n'
          '}\n'
          'const re = /a+/g;\n';
      final h = code.highlight(source, 'javascript');
      expect(h.language, 'javascript');
      expect(kindOf(h, source, 'import'), 'keyword');
      expect(kindOf(h, source, '"./x"'), 'string');
      expect(kindOf(h, source, 'MAX_SIZE'), 'constant');
      expect(kindOf(h, source, 'ratio'), 'variable');
      expect(kindOf(h, source, '.5'), 'number');
      expect(kindOf(h, source, '// note'), 'comment');
      expect(kindOf(h, source, 'build'), 'function');
      expect(kindOf(h, source, 'items'), 'variable');
      expect(kindOf(h, source, 'Map'), 'function');
      expect(kindOf(h, source, 'get'), 'function');
      expect(kindOf(h, source, '`k'), 'string');
      expect(kindOf(h, source, 'null'), 'constant');
      expect(kindOf(h, source, '/a+/g'), 'string');
      expect(kindOf(h, source, '??'), 'operator');
      expect(kindOf(h, source, '('), isNull);
      for (final alias in ['js', 'JavaScript']) {
        expect(code.highlight(source, alias).language, 'javascript');
      }
    });

    test('TypeScript kinds', () {
      const source =
          'interface Point { x: number; label?: string }\n'
          'class Box<T> extends Base { value: T; }\n'
          'let p: Point = { x: 1, label: "a" };\n';
      final h = code.highlight(source, 'ts');
      expect(h.language, 'typescript');
      expect(kindOf(h, source, 'interface'), 'keyword');
      expect(kindOf(h, source, 'number'), 'type');
      expect(kindOf(h, source, 'string'), 'type');
      expect(kindOf(h, source, 'Box'), 'constructor');
      expect(kindOf(h, source, 'value'), 'property');
      expect(kindOf(h, source, 'Point', 50), 'type');
      expect(kindOf(h, source, 'label', 60), 'property');
    });

    test('JSON kinds', () {
      const source = '{"answer": 42, "ok": true, "list": [null, "x"]}';
      final h = code.highlight(source, 'json');
      expect(kindOf(h, source, '"answer"'), 'string');
      expect(kindOf(h, source, '42'), 'number');
      expect(kindOf(h, source, 'true'), 'constant');
      expect(kindOf(h, source, 'null'), 'constant');
      expect(code.highlight(source, 'json'), same(h));
    });

    test('unported, plain and oversized snippets are one plain token', () {
      for (final (source, language) in [
        ('def f(): pass', 'python'),
        ('const x = 1;', 'text'),
        ('const x = 1;', ''),
        ('x' * (FlarkCodeMirror.maxCodeUnits + 1), 'javascript'),
      ]) {
        final h = code.highlight(source, language);
        expect(h.language, isNull);
        expect(h.tokens.single.start, 0);
        expect(h.tokens.single.end, source.length);
        expect(h.tokens.single.kind, isNull);
      }
    });
  });

  test('language names', () {
    expect(code.resolveLanguage('', 'js title="x"'), 'javascript');
    expect(code.resolveLanguage('', 'TS'), 'typescript');
    expect(code.resolveLanguage('{}', ''), '');
    expect(code.resolveLanguage('', 'auto'), '');
    expect(code.resolveLanguage('', 'py'), 'python');
    expect(code.resolveLanguage('', 'kotlin'), 'kotlin');
  });

  group('proposals', () {
    CodeEditProposal? propose(
      String source,
      int at, {
      String language = 'javascript',
      CodeEditingAction action = CodeEditingAction.newline,
      String text = '',
      String unit = '  ',
    }) => code.propose(
      source,
      language: language,
      base: at,
      extent: at,
      action: action,
      text: text,
      indentUnit: unit,
    );
    String apply(String source, CodeEditProposal edit) =>
        source.replaceRange(edit.start, edit.end, edit.text);

    test('declines what it cannot serve', () {
      expect(propose('x', 1, language: 'python'), isNull);
      expect(propose('x', 1, language: ''), isNull);
      final long = 'x' * (FlarkCodeMirror.maxCodeUnits + 1);
      expect(propose(long, long.length), isNull);
      expect(propose('😀', 1), isNull);
      expect(propose('x', 2), isNull);
      expect(propose('x', 1, unit: 'x'), isNull);
      expect(propose('x', 1, unit: ' ' * 9), isNull);
    });

    test(
      'Enter between braces opens and indents a line, except in strings',
      () {
        const code = 'if (x) {}';
        final edit = propose(code, 8)!;
        expect(apply(code, edit), 'if (x) {\n  \n}');
        expect(edit.base, 11);
        const quoted = 'const s = "{}";';
        expect(apply(quoted, propose(quoted, 12)!), 'const s = "{\n}";');
        const commented = '// {}';
        expect(apply(commented, propose(commented, 4)!), '// {\n}');
      },
    );

    test('tabs indent with tabs, measured at four columns', () {
      const source = 'if (a) {\n\tif (b) {';
      final edit = propose(source, source.length, unit: '\t')!;
      expect(apply(source, edit), '$source\n\t\t');
      const aligned = 'call(a,';
      expect(apply(aligned, propose(aligned, 7, unit: '\t')!), 'call(a,\n\t ');
    });

    test('a typed closer re-indents only where the mode says', () {
      const source = 'function f() {\n  if (x) {\n    y();\n    ';
      final brace = propose(
        source,
        source.length,
        action: CodeEditingAction.insert,
        text: '}',
      )!;
      expect(apply(source, brace), 'function f() {\n  if (x) {\n    y();\n  }');
      final letter = propose(
        source,
        source.length,
        action: CodeEditingAction.insert,
        text: 'z',
      )!;
      expect(apply(source, letter), '${source}z');
      const list = 'const a = [\n  1,\n  ';
      expect(
        apply(
          list,
          propose(
            list,
            list.length,
            action: CodeEditingAction.insert,
            text: ']',
          )!,
        ),
        'const a = [\n  1,\n]',
      );
    });
  });

  group('through the kernel', () {
    final backend = createParseBackend();
    // The fence's first line, its continuation prefix, the body, the caret
    // after the first match of `at` in it, and the body after Return.
    for (final (first, cont, body, at, expected) in [
      ('', '', 'if (x) {}', '{', 'if (x) {\n  \n}'),
      ('> ', '> ', '  if (x) {', '{', '  if (x) {\n>     '),
      ('', '', '// {', '{', '// {\n'),
      ('', '', 'print("{");', ';', 'print("{");\n'),
      ('- ', '  ', 'call(a,', ',', 'call(a,\n       '),
    ]) {
      test('Return indents a JavaScript fence: $first$body', () {
        final before = '$first```js\n$cont';
        final source = '$before$body\n$cont```\n\nafter';
        final caret = before.length + body.indexOf(at) + 1;
        final e = FlarkEditor(
          backend,
          codeEditing: code,
          text: source,
          caret: caret,
        );
        expect(e.apply(const Newline()), isTrue);
        expect(e.source, '$before$expected\n$cont```\n\nafter');
        expect(e.document.rowAt(e.selection.extent).kind, RowKind.codeBlock);
        expect(e.apply(const InsertText('z')), isTrue);
        expect(e.apply(const Undo()), isTrue);
        expect(e.apply(const Undo()), isTrue);
        expect((e.source, e.selection.extent), (source, caret));
      });
    }

    test('a typed brace outdents, and Undo restores the indentation', () {
      const source = '```ts\nfunction f(): void {\n  \n```';
      final caret = source.indexOf('\n```');
      final e = FlarkEditor(
        backend,
        codeEditing: code,
        text: source,
        caret: caret,
      );
      expect(e.apply(const InsertText('}')), isTrue);
      expect(e.source, '```ts\nfunction f(): void {\n}\n```');
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), (source, caret));
    });
  });
}
