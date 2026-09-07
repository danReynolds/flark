import 'dart:convert';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';

import 'probe_native.dart' if (dart.library.js_interop) 'probe_web.dart';
import 'edit_cases.dart';
import 'catalog_cases.dart';

Future<void> main() async {
  final analyzer = await createAnalyzer();
  final results = <Object>[];
  for (final c in detectionCases.entries) {
    results.add(['detect', c.key, analyzer.detect(c.key).name]);
  }
  for (final language in CodeLanguage.values) {
    for (final source in [
      '',
      '\r\n',
      '😀 café 中文\r\n\t',
      'for (final x in [1, 2, 3]) {\n  print("😀");\n}',
      'const x = `value: \${f({a: 1})}`; /* comment */',
      'if ready:\n    run()\nelse:\n    stop()',
      'settings: |\n  text\nitems:\n  - value',
      "def hello\n  print '😀 café'\nend",
      'x' * 8192,
      '😀' * 4097,
    ]) {
      final value = analyzer.analyze(source, language: language);
      results.add({
        'language': language.name,
        'source': value.source,
        'status': value.status.name,
        'spans': [
          for (final span in value.spans)
            [span.start, span.end, span.startByte, span.endByte, span.scopes],
        ],
      });
    }
  }
  for (final c in editCases) {
    results.add(runEditCase(analyzer, c));
  }
  analyzer.dispose();
  print(jsonEncode(results));
}
