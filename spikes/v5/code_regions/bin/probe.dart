import 'dart:convert';

import 'package:flark_code_region_spike/indentation.dart';
import 'package:flark_code_region_spike/lexical.dart';

void main(List<String> args) {
  final clock = Stopwatch()..start();
  final lexer = TextMateProbe(), observations = <Map<String, Object>>[];
  final setupUs = clock.elapsedMicroseconds;
  final editor = SnippetIndenter(lexer);
  final firstTokenUs = <String, int>{};
  for (final language in TextMateProbe.languages.keys) {
    clock.reset();
    lexer.tokenize(language, 'value = "hello";');
    firstTokenUs[language] = clock.elapsedMicroseconds;
  }

  void check(String name, Object actual, Object expected) {
    observations.add({
      'name': name,
      'actual': actual,
      'expected': expected,
      'pass': jsonEncode(actual) == jsonEncode(expected),
    });
  }

  void journey(
    String name,
    String language,
    String initial,
    List<(String, String)> steps, {
    String unit = '  ',
  }) {
    var source = initial.replaceFirst('¦', ''), at = initial.indexOf('¦');
    for (var i = 0; i < steps.length; i++) {
      final (action, expected) = steps[i];
      final edit = action == 'enter'
          ? editor.newline(language, source, at, unit)
          : action.startsWith('paste:')
          ? SnippetEdit(at, at, action.substring(6), at + action.length - 6)
          : editor.typedCloser(language, source, at, action);
      source = edit.apply(source);
      at = edit.caret;
      check(
        '$name / ${i + 1} $action',
        '${source.substring(0, at)}¦${source.substring(at)}',
        expected,
      );
    }
  }

  journey('owner loop', 'dart', 'for (final x in [1,2,3]) {¦', [
    ('enter', 'for (final x in [1,2,3]) {\n  ¦'),
    ('}', 'for (final x in [1,2,3]) {\n}¦'),
    (';', 'for (final x in [1,2,3]) {\n};¦'),
  ]);
  journey('nested JavaScript', 'javascript', 'function run() {\n  if (ok) {¦', [
    ('enter', 'function run() {\n  if (ok) {\n    ¦'),
    ('}', 'function run() {\n  if (ok) {\n  }¦'),
    ('enter', 'function run() {\n  if (ok) {\n  }\n  ¦'),
    ('}', 'function run() {\n  if (ok) {\n  }\n}¦'),
  ]);
  journey('TypeScript', 'typescript', 'function run(): void {¦', [
    ('enter', 'function run(): void {\n  ¦'),
    ('}', 'function run(): void {\n}¦'),
  ]);
  journey('between paired delimiters', 'dart', 'void run() {¦}', [
    ('enter', 'void run() {\n  ¦\n}'),
    ('x', 'void run() {\n  x¦\n}'),
  ]);
  journey('Python suite', 'python', 'if ready:¦', [
    ('enter', 'if ready:\n    ¦'),
    ('pass', 'if ready:\n    pass¦'),
    ('enter', 'if ready:\n    pass\n    ¦'),
  ], unit: '    ');
  journey('Python bracket', 'python', 'values = [¦', [
    ('enter', 'values = [\n    ¦'),
    (']', 'values = [\n]¦'),
  ], unit: '    ');
  journey('YAML mapping', 'yaml', 'settings:¦', [
    ('enter', 'settings:\n  ¦'),
    ('enabled: true', 'settings:\n  enabled: true¦'),
  ]);
  journey('YAML flow', 'yaml', 'settings: {¦', [
    ('enter', 'settings: {\n  ¦'),
    ('}', 'settings: {\n}¦'),
  ]);
  journey('literal paste', 'dart', 'void run() {\n  ¦', [
    ('paste:}', 'void run() {\n  }¦'),
    (';', 'void run() {\n  };¦'),
  ]);
  journey('tabs', 'dart', '\tvoid run() {¦', [
    ('enter', '\tvoid run() {\n\t\t¦'),
    ('}', '\tvoid run() {\n\t}¦'),
  ], unit: '\t');
  journey('CRLF and Unicode', 'dart', '// 🦋 café\r\nvoid run() {¦', [
    ('enter', '// 🦋 café\r\nvoid run() {\r\n  ¦'),
    ('}', '// 🦋 café\r\nvoid run() {\r\n}¦'),
  ]);
  journey('unknown stays literal', 'unknown', 'thing {¦', [
    ('enter', 'thing {\n¦'),
  ]);
  journey('Python trailing comment', 'python', 'if ready: # why¦', [
    ('enter', 'if ready: # why\n    ¦'),
  ], unit: '    ');
  journey('Dart trailing comment', 'dart', 'void run() { // why¦', [
    ('enter', 'void run() { // why\n  ¦'),
    ('}', 'void run() { // why\n}¦'),
  ]);
  journey(
    'opener before unterminated comment',
    'javascript',
    'function run() { /*¦',
    [('enter', 'function run() { /*\n¦')],
  );
  journey('braces inside closed string', 'javascript', 'const text = "{";¦', [
    ('enter', 'const text = "{";\n¦'),
  ]);
  journey('colon inside Python string', 'python', 'value = "if ready:"¦', [
    ('enter', 'value = "if ready:"\n¦'),
  ], unit: '    ');
  // Desired snippet behavior the selected upstream configuration subset does
  // not necessarily supply. Keep misses visible instead of patching languages.
  journey('Python else alignment', 'python', 'if ready:\n    pass\n    ¦', [
    ('else:', 'if ready:\n    pass\nelse:¦'),
  ], unit: '    ');
  journey('YAML block scalar Enter', 'yaml', 'settings: |¦', [
    ('enter', 'settings: |\n  ¦'),
  ]);
  journey('open comment', 'javascript', 'function run() {\n  /*¦', [
    ('enter', 'function run() {\n  /*\n  ¦'),
    ('}', 'function run() {\n  /*\n  }¦'),
  ]);
  journey(
    'regex delimiter',
    'javascript',
    'function run() {\n  const re = /[}]/;¦',
    [
      ('enter', 'function run() {\n  const re = /[}]/;\n  ¦'),
      ('}', 'function run() {\n  const re = /[}]/;\n}¦'),
    ],
  );
  journey('open Python multiline string', 'python', 'def run():\n    """¦', [
    ('enter', 'def run():\n    """\n    ¦'),
    ('}', 'def run():\n    """\n    }¦'),
  ], unit: '    ');
  journey(
    'blank line in Python multiline string',
    'python',
    'def run():\n    """\n\n    ¦',
    [('}', 'def run():\n    """\n\n    }¦')],
  );
  journey('YAML block literal', 'yaml', 'settings: |\n  text\n  ¦', [
    ('}', 'settings: |\n  text\n  }¦'),
  ]);
  journey('JavaScript interpolation', 'javascript', r'const s = `${(() => {¦', [
    (
      'enter',
      r'const s = `${(() => {'
          '\n  ¦',
    ),
    (
      '}',
      r'const s = `${(() => {'
          '\n}¦',
    ),
  ]);
  journey('Dart interpolation', 'dart', r'final s = "${() {¦', [
    (
      'enter',
      r'final s = "${() {'
          '\n  ¦',
    ),
    (
      '}',
      r'final s = "${() {'
          '\n}¦',
    ),
  ]);
  journey('mismatched closer', 'dart', 'void run() {\n  [\n    ¦', [
    ('}', 'void run() {\n  [\n    }¦'),
  ]);

  final lexicalExamples = {
    'dart': 'for (final x in [1,2,3]) {\n  // }\n}',
    'javascript':
        r'const s = `${(() => {'
        '\n  }',
    'typescript': 'const re: RegExp = /[}]/;\n',
    'python': 'def run():\n    """\n\n    }',
    'yaml': 'key: { value: true }\ntext: |\n  }',
  };
  final snapshots = <String, Object>{};
  for (final e in lexicalExamples.entries) {
    final lexed = lexer.tokenize(e.key, e.value)!;
    check(
      '${e.key} exact text',
      lexed.spans.map((s) => e.value.substring(s.start, s.end)).join(),
      e.value,
    );
    var previous = 0;
    var contiguous = true;
    for (final s in lexed.spans) {
      contiguous = contiguous && s.start == previous && s.end > s.start;
      previous = s.end;
    }
    check(
      '${e.key} exact coordinates',
      contiguous && previous == e.value.length,
      true,
    );
    final before = lexed.spans.map((s) => s.toJson()).toList();
    lexer.changeTheme();
    final after = lexer
        .tokenize(e.key, e.value)!
        .spans
        .map((s) => s.toJson())
        .toList();
    check('${e.key} theme independence', after, before);
    // No incremental cache is implemented: retokenization must respond to
    // edits earlier in a multiline literal and then recover on restoration.
    lexer.tokenize(e.key, '// changed\n${e.value}');
    check(
      '${e.key} re-edit restoration',
      lexer.tokenize(e.key, e.value)!.spans.map((s) => s.toJson()).toList(),
      before,
    );
    snapshots[e.key] = before;
  }
  check(
    'large region remains literal',
    lexer.tokenize('dart', 'x' * 8193) == null,
    true,
  );

  final performance = <String, Object>{
    'setupUs': setupUs,
    'firstTokenUs': firstTokenUs,
  };
  if (args.contains('--bench')) {
    for (final entry in {
      'dart': 'void run() {\n  final value = "hello";\n}\n',
      'javascript': 'function run() {\n  const value = "hello";\n}\n',
      'typescript': 'function run(): void {\n  const value = "hello";\n}\n',
      'python': 'def run():\n    value = "hello"\n',
      'yaml': 'settings:\n  enabled: true\n  name: hello\n',
    }.entries) {
      final sizes = <String, Object>{};
      for (final size in [512, 2048, 8192]) {
        final source = (entry.value * (size ~/ entry.value.length + 1))
            .substring(0, size);
        lexer.tokenize(entry.key, source);
        final samples = <int>[];
        for (var i = 0; i < 12; i++) {
          clock.reset();
          final result = lexer.tokenize(entry.key, source);
          if (result == null) throw StateError('Incomplete benchmark tokenize');
          samples.add(clock.elapsedMicroseconds);
        }
        samples.sort();
        sizes['$size'] = {
          'medianUs': (samples[5] + samples[6]) ~/ 2,
          'maxUs': samples.last,
        };
      }
      performance[entry.key] = sizes;
    }
  }

  final failures = observations.where((o) => o['pass'] == false).toList();
  print(
    jsonEncode({
      'checks': observations,
      'failures': failures.length,
      'lexicalSnapshots': snapshots,
      'performance': performance,
    }),
  );
  if (args.contains('--strict') && failures.isNotEmpty) {
    throw StateError('${failures.length} acceptance checks failed');
  }
}
