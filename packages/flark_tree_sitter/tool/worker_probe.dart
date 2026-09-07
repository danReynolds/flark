import 'dart:convert';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';

import 'clock_native.dart' if (dart.library.js_interop) 'clock_web.dart';
import 'probe_native.dart' if (dart.library.js_interop) 'probe_web.dart';
import 'worker_native.dart' if (dart.library.js_interop) 'worker_web.dart';
import 'edit_cases.dart';
import 'catalog_cases.dart';
import 'typing_bench.dart' show stats;

Object serialize(CodeAnalysis value) => [
  value.source,
  value.language.name,
  value.status.name,
  for (final s in value.spans)
    [s.start, s.end, s.startByte, s.endByte, s.scopes],
];

Future<Map<String, Object>> run() async {
  final startup = nowMicros();
  final worker = await createWorker();
  final startupMicros = nowMicros() - startup;
  final direct = await createAnalyzer();
  var comparisons = 0;
  final results = <Object>[];
  try {
    for (final c in editCases) {
      final edit = runEditCase(direct, c);
      for (final source in [
        edit['source'] as String,
        edit['following'] as String,
      ]) {
        final actual = await worker.analyze(source, language: c.language);
        final expected = direct.analyze(source, language: c.language);
        if (jsonEncode(serialize(actual!)) != jsonEncode(serialize(expected))) {
          throw StateError('Worker mismatch: ${c.name}');
        }
        comparisons++;
      }
    }
    // Deliberately issue all requests before the first can complete.
    final burst = [
      for (var i = 0; i < 40; i++)
        worker.analyze('const x = $i;', language: CodeLanguage.javascript),
    ];
    final answers = await Future.wait(burst);
    if (answers.take(39).any((a) => a != null) ||
        answers.last?.source != 'const x = 39;') {
      throw StateError('Burst retained superseded work');
    }
    for (final (language, body, suffix, typed) in [
      for (final c in catalogCases.entries.where((c) => c.key.index > 5))
        (c.key, '${c.value}\n', '', c.value),
      (
        CodeLanguage.ruby,
        "def hello\n  puts '😀'\nend\n",
        'def test',
        "\nputs 'hello'\nend",
      ),
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
        final input = <double>[], latency = <double>[];
        var dropped = 0;
        for (var journey = 0; journey < 4; journey++) {
          var source = seed, caret = seed.length;
          final completions = <Future<void>>[];
          for (final rune in typed.runes) {
            final character = String.fromCharCode(rune);
            final start = nowMicros();
            final edit = direct.proposeEdit(
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
            source = edit.applyTo(source);
            caret = edit.extent;
            final snapshot = source;
            final future = worker.analyze(snapshot, language: language);
            final queued = nowMicros();
            if (journey > 0) input.add(queued - start);
            completions.add(
              future.then((result) {
                if (result != null &&
                    (result.source != snapshot ||
                        result.language != language)) {
                  throw StateError('Wrong worker revision');
                }
                if (journey > 0) {
                  if (result == null) {
                    dropped++;
                  } else {
                    latency.add(nowMicros() - start);
                  }
                }
              }),
            );
            // 125 input events/second, including Enter and typed closers.
            await Future<void>.delayed(const Duration(milliseconds: 8));
          }
          await Future.wait(completions);
        }
        results.add({
          'language': language.name,
          'seedUnits': seed.length,
          'samples': input.length,
          'inputMicros': stats(input),
          'colorLatencyMicros': stats(latency),
          'superseded': dropped,
        });
      }
    }
    final pending = worker.analyze(
      'void f() {}\n' * 500,
      language: CodeLanguage.dart,
    );
    worker.dispose();
    if (await pending != null) throw StateError('Disposed work was delivered');
    return {
      'comparisons': comparisons,
      'burstRequests': burst.length,
      'startupMicros': startupMicros.round(),
      'detection': detectionBench(direct),
      'results': results,
    };
  } finally {
    worker.dispose();
    direct.dispose();
  }
}

Future<void> main() async => print(jsonEncode(await run()));

// Every measured sample changes. Repeating an exact cached source would hide
// the synchronous cost of Automatic mode on the input thread.
List<Object> detectionBench(CodeAnalyzer direct) {
  direct.detect('x');
  final results = <Object>[];
  for (final c in catalogCases.entries) {
    for (final target in [c.value.length, 128]) {
      final values = <double>[];
      for (var i = 0; i < 40; i++) {
        final tail = '\n$i';
        final seed = '${c.value}\n' * (target ~/ (c.value.length + 1) + 1);
        final source = seed.substring(0, target - tail.length) + tail;
        final start = nowMicros();
        direct.detect(source);
        if (i > 0) values.add(nowMicros() - start);
      }
      results.add({
        'language': c.key.name,
        'sampleUnits': target,
        'samples': values.length,
        'micros': stats(values),
      });
    }
  }
  return results;
}
