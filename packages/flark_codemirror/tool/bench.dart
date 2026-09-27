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
  final results = benchCodeMirror(
    {
      'javascript': read('modern.js'),
      'typescript': read('types.ts'),
      'json': read('data.json'),
      'python': read('python/sample.py'),
      'c': read('c/sample.c'),
      'cpp': read('cpp/sample.cpp'),
      'java': read('java/Inventory.java'),
      'csharp': read('csharp/Orders.cs'),
      'kotlin': read('kotlin/Tasks.kt'),
      'dart': read('dart/inventory.dart'),
      'bash': read('bash/release.sh'),
      'yaml': read('yaml/services.yaml'),
      'go': read('go/pipeline.go'),
      'ruby': read('ruby/catalog.rb'),
      'rust': read('rust/sample.rs'),
      'powershell': read('powershell/sample.ps1'),
      'xml': read('xml/sample.xml'),
      'html': read('html-mixed/page.html'),
      'sql': read('sql/sample.sql'),
      'postgresql': read('postgresql/sample.sql'),
      'mysql': read('mysql/sample.sql'),
      'css': read('css/sample.css'),
      'scss': read('scss/sample.scss'),
      'less': read('less/sample.less'),
      'php': read('php/app.php'),
    },
    sizes: const [2048, 8192],
  );
  for (final result in results) {
    stdout.writeln(jsonEncode(result));
  }
}
