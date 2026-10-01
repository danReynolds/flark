import 'package:flark/code.dart';
import 'package:flark/flark.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_codemirror/src/highlight.dart';
import 'package:flark_codemirror/src/mode.dart';
import 'package:flark_codemirror/src/stream.dart';
import 'package:test/test.dart';

/// Reads one character a token until [failAt], where it throws, or, without
/// [failAt], never advances: the two ways a defective mode fails.
final class _Defective extends Mode<Object?> {
  _Defective({this.failAt});
  final int? failAt;

  @override
  Object? startState([int baseColumn = 0]) => null;

  @override
  Object? copyState(Object? state) => state;

  @override
  String? token(StringStream stream, Object? state) {
    if (failAt == null) return 'keyword';
    if (stream.pos == failAt) throw StateError('defect');
    stream.next();
    return 'keyword';
  }
}

void main() {
  final code = FlarkCodeMirror();

  String? kindOf(CodeHighlight h, String source, String text, [int from = 0]) =>
      h.kindAt(source.indexOf(text, from));

  group('highlight', () {
    test('tokens tile the snippet, including Unicode, tabs and newlines', () {
      const source = '// 😀 café\nconst text = "λ";\n\t42\r\n\n}';
      for (final language in [
        ...CodeMirrorLanguages.all.map((l) => l.name),
        'text',
        'kotlin',
      ]) {
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

    test('a character a mode cannot read costs only that character', () {
      // Rust's comment state reads only `.*`, and its string escapes
      // `\\(?:.|$)`; `.` does not match U+2028. Upstream throws there, which
      // would fail the frame that paints the fence.
      final separator = String.fromCharCode(0x2028);
      final source = '/* a${separator}b */\nfn main() {}';
      final h = code.highlight(source, 'rust');
      expect(
        h.tokens.map((t) => source.substring(t.start, t.end)).join(),
        source,
      );
      expect(kindOf(h, source, separator), 'comment');
      expect(kindOf(h, source, 'b */'), 'comment');
      expect(kindOf(h, source, 'fn'), 'keyword');
      final escaped = 'let s = "a\\${separator}b"; let t = 1;';
      final e = code.highlight(escaped, 'rust');
      expect(kindOf(e, escaped, 'b"'), 'string');
      expect(kindOf(e, escaped, 'let t'), 'keyword');
    });

    test('a mode that never advances steps one character at a time', () {
      final steps = <String>[];
      runMode(
        _Defective(),
        'a\u{1F600}b',
        onToken: (line, start, end, style) => steps.add('$start-$end $style'),
      );
      // The surrogate pair is one step.
      expect(steps, ['0-1 keyword', '1-3 keyword', '3-4 keyword']);
    });

    test('a mode that throws leaves the rest of its snippet plain', () {
      final tokens = codeMirrorTokens(_Defective(failAt: 3), 'abcdef\nghi');
      expect(
        [for (final t in tokens) (t.start, t.end, t.kind)],
        [(0, 3, 'keyword'), (3, 10, null)],
      );
    });

    test('long lines highlight in linear time', () {
      for (final (language, line) in [
        // YAML's key pattern backtracked through every split of a run of
        // spaces: 256 took 2.7 s, 512 took 42 s.
        ('yaml', '${' ' * 256}x'),
        // And retried it at every character of a line without a colon.
        ('yaml', '- ${'the quick brown fox ' * 400}'),
        // C-like modes copied and searched the line before every name.
        ('java', 'String s = ${'a + b + ' * 1000}"";'),
        ('c', 'a' * 8000),
      ]) {
        final watch = Stopwatch()..start();
        code.highlight(line, language);
        expect(
          watch.elapsedMilliseconds,
          lessThan(100),
          reason: '$language, ${line.length} code units',
        );
      }
    });

    test('modes read long lines in linear time beyond the delegate cap', () {
      // Each was quadratic in the line: a search for tabs that ran to its end
      // at every column() call, a scan for type arguments at every name
      // before a `<`, and PowerShell reading a whole run of name characters
      // for every token it took from the run.
      String repeat(String unit) =>
          (unit * ((1 << 17) ~/ unit.length)).padRight(1 << 17, unit[0]);
      for (final (language, line) in [
        (CodeMirrorLanguages.javascript, repeat('abc ')),
        (CodeMirrorLanguages.javascript, repeat('a<')),
        (CodeMirrorLanguages.powershell, repeat('-')),
      ]) {
        final watch = Stopwatch()..start();
        codeMirrorTokens(language.mode(const ModeConfig()), line);
        expect(
          watch.elapsedMilliseconds,
          lessThan(500),
          reason: '${language.name}: ${line.substring(0, 4)}…',
        );
      }
    });

    test('unported, plain and oversized snippets are one plain token', () {
      for (final (source, language) in [
        ('main = putStrLn "hi"', 'haskell'),
        ('const x = 1;', 'text'),
        ('Just some words.', ''),
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

  group('detection', () {
    for (final (name, source, expected) in [
      (
        'interface and annotations',
        'interface Point { x: number }\nconst p: Point = { x: 1 };',
        'typescript',
      ),
      (
        'plain JavaScript',
        'function add(a, b) {\n  return a + b; // sum\n}',
        'javascript',
      ),
      ('an object', '{\n  "answer": 42,\n  "ok": true\n}', 'json'),
      ('an array of objects', '[{"id": 1}, {"id": 2}]', 'json'),
      ('an array expression', '[1, 2, 3].map((x) => x * 2);', 'javascript'),
      (
        'a method with a return type',
        'async test(): Promise<void> {\n'
            '  [1,2,3].map((test) => console.log(test));\n}',
        'typescript',
      ),
      (
        'a JavaScript class that extends another',
        'class Store extends EventTarget {\n  #items = new Map();\n}',
        'javascript',
      ),
      ('prose', 'Just some words about the weather.', ''),
      ('prose with keywords', 'Wait for it if you can, then do it.', ''),
      ('nothing', '  \n', ''),
      // flark_tree_sitter's detection cases for these languages and prose.
      // Too little to tell among the C-like languages.
      ('if (ready) {', 'if (ready) {', ''),
      (
        'const value',
        "const value = 'hello'; console.log(value);",
        'javascript',
      ),
      ('typed const', "const value: string = 'hello';", 'typescript'),
      ('JSON with an array', '{"hello": [1, true]}', 'json'),
      ('function hello', 'function hello() {', 'javascript'),
      ('x', 'x', ''),
      ('Hello world', 'Hello world', ''),
      ('This is a simple test.', 'This is a simple test.', ''),
      ('Please select the item.', 'Please select the item.', ''),
      ('An end to the story.', 'An end to the story.', ''),
      ('The final result is here.', 'The final result is here.', ''),
      // The rest of flark_tree_sitter's cases, for the other languages.
      ('SQL', "SELECT name FROM users WHERE name = 'hello';", 'sql'),
      ('CSS', '.card { color: red; content: "hello"; }', 'css'),
      ('if true; then', 'if true; then', 'bash'),
      ('an if block', 'if true; then\n  echo "hello"\nfi', 'bash'),
      ('for x in', 'for x in one two; do', 'bash'),
      ('while true', 'while true; do', 'bash'),
      ('echo', 'echo "hello"', 'bash'),
      ('void main', "void main() { print('hello'); }", 'dart'),
      ('for final', 'for (final x in [1,2,3]) {', 'dart'),
      ('def hello():', "def hello():\n    print('hello')", 'python'),
      ('def with a colon', 'def hello():', 'python'),
      ('YAML', 'name: "hello"\nitems: [one, two]', 'yaml'),
      ('a block scalar', 'settings: |', 'yaml'),
      ('def ... end', "def hello\n  print 'hello'\nend", 'ruby'),
      ('def without a colon', 'def hello', 'ruby'),
      ('fn main', 'fn main() { println!("hello"); }', 'rust'),
      ('package main', 'package main\nfunc main() { println("hello") }', 'go'),
      ('a div', '<div class="card">hello</div>', 'html'),
      ('XML', '<?xml version="1.0"?><note>hello</note>', 'xml'),
      ('PHP', '<?php echo \$greeting; ?>', 'php'),
      (
        'PowerShell',
        'Get-ChildItem -Path . | Where-Object { \$_.Length -gt 1kb }',
        'powershell',
      ),
      (
        'Kotlin',
        'fun main() {\n  val items = listOf(1, 2)\n  println(items)\n}',
        'kotlin',
      ),
      (
        'Java',
        'public static void main(String[] args) {\n'
            '  System.out.println("hello");\n}',
        'java',
      ),
      ('C#', 'using System;\n\nConsole.WriteLine("hello");', 'csharp'),
      ('C', '#include <stdio.h>\nint main(void) { printf("hi"); }', 'c'),
      ('C++', '#include <iostream>\nint main() { std::cout << "hi"; }', 'cpp'),
      ('SCSS', '\$accent: red;\n.card { color: \$accent; }', 'scss'),
      ('a shell session', r'$ npm install flark', 'bash'),
      (
        'a sentence with SQL words',
        'Please select the item from the list.',
        '',
      ),
      (
        'a list of notes',
        '- Prefer one way to do each thing.\n- Keep it small.',
        '',
      ),
    ]) {
      test(
        name,
        () => expect(
          detectCodeMirrorLanguage(source, CodeMirrorLanguages.all),
          expected,
        ),
      );
    }

    test('an untagged fence highlights as what it looks like', () {
      const source = 'interface Point { x: number }';
      final h = code.highlight(source, '');
      expect(h.language, 'typescript');
      expect(kindOf(h, source, 'interface'), 'keyword');
      expect(code.resolveLanguage(source, ''), 'typescript');
      expect(code.resolveLanguage(source, 'json'), 'json');
    });

    test('typing below the sample keeps the answer', () {
      final head = 'const answer = 42;\n' * 80;
      expect(code.resolveLanguage(head, ''), 'javascript');
      expect(code.resolveLanguage('$head{"not": "json"}', ''), 'javascript');
    });
  });

  test('changing a fence to an unknown language clears its colors', () {
    const source = 'def greet(name):\n    return name';
    expect(code.highlight(source, 'python').tokens.length, greaterThan(1));
    final unknown = code.highlight(source, 'haskell');
    expect(unknown.language, isNull);
    expect(unknown.tokens.single.kind, isNull);
  });

  test('an app highlights the languages it chooses', () {
    final python = FlarkCodeMirror.only([CodeMirrorLanguages.python]);
    expect(python.languages.map((l) => l.name), ['python']);
    const js = 'function add(a, b) {\n  return a + b; // sum\n}';
    expect(code.resolveLanguage(js, ''), 'javascript');
    expect(python.resolveLanguage(js, ''), '');
    expect(python.highlight(js, 'javascript').language, isNull);
    const def = 'def add(a, b):\n    return a + b';
    expect(python.resolveLanguage(def, ''), 'python');
    expect(python.highlight(def, '').language, 'python');
    // A language left out still indents by its brackets.
    final edit = python.propose(
      'f() {',
      language: 'javascript',
      base: 5,
      extent: 5,
      action: CodeEditingAction.newline,
      text: '',
      indentUnit: '  ',
    )!;
    expect(edit.text, '\n  ');
  });

  test('aliases and extensions name their language', () {
    for (final (alias, language) in [
      ('zsh', 'bash'),
      ('yml', 'yaml'),
      ('kt', 'kotlin'),
      ('c++', 'cpp'),
      ('cs', 'csharp'),
      ('ps1', 'powershell'),
      ('pgsql', 'postgresql'),
      ('tsx', 'typescript'),
      ('jsonc', 'json'),
      ('py3', 'python'),
      ('plaintext', 'text'),
    ]) {
      expect(code.resolveLanguage('', alias), language, reason: alias);
    }
  });

  test('language names', () {
    expect(code.resolveLanguage('', 'js title="x"'), 'javascript');
    expect(code.resolveLanguage('', 'TS'), 'typescript');
    expect(code.resolveLanguage('Just words.', ''), '');
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
      expect(propose('x', 1, language: 'text'), isNull);
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

    test('an unported language indents by its brackets', () {
      const block = 'fn main() {';
      expect(
        apply(block, propose(block, block.length, language: 'zig')!),
        'fn main() {\n  ',
      );
      const closing = 'fn main() {\n  run(\n    1,\n    2\n    ';
      final closer = propose(
        closing,
        closing.length,
        language: 'zig',
        action: CodeEditingAction.insert,
        text: ')',
      )!;
      expect(
        apply(closing, closer),
        '${closing.substring(0, closing.length - 4)}  )',
      );
      const flat = 'defmodule Greeter do\n  def hello';
      expect(
        apply(flat, propose(flat, flat.length, language: 'elixir')!),
        '$flat\n  ',
      );
      expect(code.highlight(block, 'zig').tokens.single.kind, isNull);
    });

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

    test('Return works after a character the mode cannot read', () {
      final separator = String.fromCharCode(0x2028);
      final source = '```rust\n/* a${separator}b */\nfn main() {}\n```';
      final caret = source.indexOf('{}') + 1;
      final e = FlarkEditor(
        backend,
        codeEditing: code,
        text: source,
        caret: caret,
      );
      expect(e.apply(const Newline()), isTrue);
      expect(e.source, source.replaceFirst('{}', '{\n  \n}'));
    });

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
