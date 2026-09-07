import 'dart:convert';
import 'package:shiki_flutter/engine.dart';
import 'package:shiki_flutter/langs.dart';

Future<void> main() async {
  final highlighter = ShikiHighlighter();
  final theme = ShikiTheme(id: 'probe', type: 'dark', json: jsonEncode({
    'name': 'probe', 'type': 'dark', 'settings': [
      {'settings': {'foreground': '#FFFFFF', 'background': '#000000'}},
      {'scope': 'string', 'settings': {'foreground': '#00FF00'}},
      {'scope': 'comment', 'settings': {'foreground': '#888888'}},
    ],
  }));
  await highlighter.preload(langs: [CodeLanguages.dart, CodeLanguages.javascript, CodeLanguages.yaml, CodeLanguages.python, CodeLanguages.html], themes: [theme]);
  final examples = <String, (String, String)>{
    'dart-owner': ('dart', 'for (final x in [1,2,3]) {\n  }'),
    'yaml-flow': ('yaml', 'settings: {\n  }'),
    'js-interpolation': ('javascript', r'const s = `${(() => {' '\n  }'),
    'js-regex': ('javascript', 'function run() {\n  const re = /[}]/;\n}'),
    'python-open-string': ('python', 'def run():\n    """\n    }'),
    'html-script': ('html', '<script>function run() {\n  }</script>'),
  };
  final results = <String, Object>{};
  for (final entry in examples.entries) {
    final (lang, source) = entry.value;
    final lines = highlighter.codeToTokens(source, TokenizeOptions(lang: lang, theme: 'probe', includeExplanation: true));
    final rebuilt = lines.map((line) => line.map((t) => t.content).join()).join('\n');
    if (rebuilt != source) throw StateError('Text mismatch: ${entry.key}');
    results[entry.key] = {'language': lang, 'exactText': true, 'tokens': [
      for (final line in lines) for (final token in line)
        {'text': token.content, 'offset': token.offset, 'scopes': token.scopes},
    ]};
  }
  print(jsonEncode(results));
}
