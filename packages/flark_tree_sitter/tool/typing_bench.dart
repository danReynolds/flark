// Changing-source, complete service cost. No identical-source cache hits.
import 'dart:convert';
import 'dart:typed_data';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';

import 'clock_native.dart' if (dart.library.js_interop) 'clock_web.dart';
import 'probe_native.dart' if (dart.library.js_interop) 'probe_web.dart';

class _TimedBackend implements CodeBackend {
  _TimedBackend(this.inner);
  final CodeBackend inner;
  double highlightMicros = 0;
  int responseBytes = 0;
  @override
  Uint8List detect(Uint8List source) => throw UnimplementedError();
  @override
  int get version => inner.version;
  @override
  Uint8List analyze(Uint8List source, int language) {
    final start = nowMicros();
    final result = inner.analyze(source, language);
    highlightMicros = nowMicros() - start;
    responseBytes = result.length;
    return result;
  }

  @override
  Uint8List edit(Uint8List request, int language) =>
      inner.edit(request, language);
  @override
  void dispose() => inner.dispose();
}

Map<String, int> stats(List<double> values) {
  values.sort();
  return {
    'p50': values[values.length ~/ 2].round(),
    'p95': values[(values.length * .95).ceil() - 1].round(),
    'max': values.last.round(),
  };
}

Future<void> main() async {
  final backend = _TimedBackend(await createBackend());
  final analyzer = CodeAnalyzer(backend: backend);
  final results = <Object>[];
  for (final (language, body, suffix, typed) in [
    (
      CodeLanguage.dart,
      'void f() { print("😀"); }\n',
      'void g() {',
      '\nprint("hello");\n}',
    ),
    (
      CodeLanguage.javascript,
      'const x = {value: "😀"};\n',
      'if (ready) {',
      '\nconsole.log("hello");\n}',
    ),
    (
      CodeLanguage.python,
      'if ready:\n    print("😀")\n',
      'if ready:',
      '\nprint("hello")\n',
    ),
    (
      CodeLanguage.yaml,
      'name: "😀"\nitems: [one, two]\n',
      'settings: |',
      '\nhello world\n',
    ),
  ]) {
    for (final target in [512, 8192]) {
      final seed = body * ((target - 128) ~/ body.length) + suffix;
      final total = <double>[],
          highlight = <double>[],
          transport = <double>[],
          edit = <double>[];
      var largestResponse = 0;
      // First journey warms grammar/query construction and the runtime. The
      // next five replay every character against the preceding exact result.
      for (var journey = 0; journey < 6; journey++) {
        var source = seed;
        var caret = source.length;
        for (final rune in typed.runes) {
          final character = String.fromCharCode(rune);
          final start = nowMicros();
          final proposal = analyzer.proposeEdit(
            source,
            language: language,
            base: caret,
            extent: caret,
            action: character == '\n'
                ? CodeEditAction.newline
                : CodeEditAction.insert,
            text: character == '\n' ? '' : character,
            indentUnit: language == CodeLanguage.python ? '    ' : '  ',
          )!;
          source = proposal.applyTo(source);
          caret = proposal.extent;
          final edited = nowMicros();
          final analysis = analyzer.analyze(source, language: language);
          final done = nowMicros();
          if (analysis.source != source ||
              analysis.spans.last.end != source.length) {
            throw StateError('Analysis lost the edited source');
          }
          if (journey > 0) {
            total.add(done - start);
            edit.add(edited - start);
            highlight.add(done - edited);
            transport.add(backend.highlightMicros);
            if (backend.responseBytes > largestResponse) {
              largestResponse = backend.responseBytes;
            }
          }
        }
      }
      results.add({
        'language': language.name,
        'seedUnits': seed.length,
        'samples': total.length,
        'totalMicros': stats(total),
        'editMicros': stats(edit),
        'highlightMicros': stats(highlight),
        'highlightBackendMicros': stats(transport),
        'largestResponseBytes': largestResponse,
      });
    }
  }
  analyzer.dispose();
  print(jsonEncode(results));
}
