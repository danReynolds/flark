import 'dart:convert';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';

import 'probe_native.dart' if (dart.library.js_interop) 'probe_web.dart';

Future<void> main() async {
  final analyzer = await createAnalyzer();
  final results = <Object>[];
  for (final (language, snippet) in [
    (CodeLanguage.dart, 'void f() { print("😀"); }\n'),
    (CodeLanguage.javascript, 'const x = {value: "😀"};\n'),
    (CodeLanguage.python, 'if ready:\n    print("😀")\n'),
    (CodeLanguage.yaml, 'name: "😀"\nitems: [one, two]\n'),
  ]) {
    for (final target in [512, 8192]) {
      final source = snippet * (target ~/ snippet.length);
      final clock = Stopwatch()..start();
      analyzer.analyze(source, language: language);
      final first = clock.elapsedMicroseconds;
      final runs = <int>[];
      for (var i = 0; i < 12; i++) {
        clock
          ..reset()
          ..start();
        analyzer.analyze(source, language: language);
        runs.add(clock.elapsedMicroseconds);
      }
      runs.sort();
      results.add({
        'language': language.name,
        'units': source.length,
        'firstMicros': first,
        'medianMicros': runs[runs.length ~/ 2],
        'maxMicros': runs.last,
      });
    }
  }
  analyzer.dispose();
  print(jsonEncode(results));
}
