import 'dart:convert';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'probe_native.dart' if (dart.library.js_interop) 'probe_web.dart';

Future<void> main() async {
  final analyzer = await createAnalyzer();
  final rows = <Object>[];
  for (final (language, body, suffix) in [
    (CodeLanguage.dart, 'void f() { print("😀"); }\n', 'void g() {'),
    (CodeLanguage.javascript, 'const x = {value: "😀"};\n', 'if (ready) {'),
    (CodeLanguage.python, 'if ready:\n    print("😀")\n', 'if ready:'),
    (CodeLanguage.yaml, 'name: "😀"\nitems: [one, two]\n', 'settings: |'),
  ]) {
    for (final limit in [512, 8192]) {
      final source =
          body * ((limit - suffix.length - 2) ~/ body.length) + suffix;
      for (final action in [CodeEditAction.insert, CodeEditAction.newline]) {
        final samples = <int>[];
        var first = 0;
        for (var i = 0; i < 13; i++) {
          final clock = Stopwatch()..start();
          final e = analyzer.proposeEdit(
            source,
            language: language,
            base: source.length,
            extent: source.length,
            action: action,
            text: action == CodeEditAction.insert ? 'x' : '',
            indentUnit: language == CodeLanguage.python ? '    ' : '  ',
          );
          if (e == null) throw StateError('Unexpected admission failure');
          final micros = clock.elapsedMicroseconds;
          if (i == 0) {
            first = micros;
          } else {
            samples.add(micros);
          }
        }
        samples.sort();
        rows.add({
          'language': language.name,
          'units': source.length,
          'action': action.name,
          'firstMicros': first,
          'medianMicros': samples[samples.length ~/ 2],
          'maxMicros': samples.last,
        });
      }
    }
  }
  analyzer.dispose();
  print(jsonEncode(rows));
}
