// Highlighting and proposal timings on the VM or an AOT build:
//
//   dart run tool/bench.dart
//   dart compile exe tool/bench.dart -o /tmp/bench && /tmp/bench
//
// Snippets are the corpus files repeated to each size. Web builds call
// benchCodeMirror with the same texts embedded.
import 'dart:convert';
import 'dart:io';

import 'bench_core.dart';

void main() {
  final corpus = Directory.fromUri(Platform.script.resolve('corpus/'));
  String read(String name) => File('${corpus.path}$name').readAsStringSync();
  final results = benchCodeMirror({
    'javascript': read('modern.js'),
    'typescript': read('types.ts'),
    'json': read('data.json'),
  });
  for (final result in results) {
    stdout.writeln(jsonEncode(result));
  }
}
