/// Keystroke budget receipt: one InsertText and one DeleteBackward through
/// the facade on a dense document, plus the parse and projection alone.
/// Run from packages/flark: dart run tool/bench_editor.dart [kilobytes]
library;

import 'dart:io';

import 'package:flark/flark.dart';

String denseDocument(int bytes) {
  final b = StringBuffer();
  var i = 0;
  while (b.length < bytes) {
    b.writeln('## Section $i\n');
    b.writeln('Some **bold $i** text with *emphasis*, `code_$i`, a [link $i](http://example.com/$i), ~~struck~~ words and an image ![alt](http://x.y/$i.png) inline.');
    b.writeln('Second line with **more *nested* bold** and a footnote[^$i] plus &amp; entity and \\*escaped\\* stars.\n');
    b.writeln('- item **$i** with `code`\n- [x] task *$i* done\n  > quoted *inside* the item\n');
    b.writeln('1. first\n2. second with [ref][r$i]\n\n[r$i]: http://example.com/ref/$i\n');
    b.writeln('```dart\nfinal x$i = "code";\n```\n');
    b.writeln('| col **a** | col *b* |\n| - | - |\n| `$i` | ~~$i~~ |\n');
    b.writeln('[^$i]: footnote *text* $i\n');
    i++;
  }
  return b.toString();
}

int percentile(List<int> xs, double p) { final s = List.of(xs)..sort(); return s[((s.length - 1) * p).round()]; }

void main(List<String> args) {
  final kb = args.isNotEmpty ? int.parse(args[0]) : 25;
  final backend = createParseBackend();
  final source = denseDocument(kb * 1024);
  final editor = FlarkEditor(backend, text: source, caret: source.length ~/ 2);
  final inserts = <int>[], deletes = <int>[], parses = <int>[], projections = <int>[];
  final sw = Stopwatch();
  for (var i = 0; i < 500; i++) {
    sw.reset(); sw.start(); editor.apply(const InsertText('x')); sw.stop(); if (i >= 100) inserts.add(sw.elapsedMicroseconds);
    sw.reset(); sw.start(); editor.apply(const DeleteBackward()); sw.stop(); if (i >= 100) deletes.add(sw.elapsedMicroseconds);
  }
  for (var i = 0; i < 300; i++) {
    sw.reset(); sw.start(); final m = backend.parse(editor.source); sw.stop(); if (i >= 50) parses.add(sw.elapsedMicroseconds);
    sw.reset(); sw.start(); Projection.of(m, editor.source); sw.stop(); if (i >= 50) projections.add(sw.elapsedMicroseconds);
  }
  final model = editor.document.model;
  stdout.writeln('document ${source.length} chars, ${model.blockCount} blocks, ${model.runCount} runs, ${editor.projection.rows.length} rows');
  for (final (name, xs) in [('insert (facade)', inserts), ('backspace (facade)', deletes), ('parse only', parses), ('projection only', projections)]) {
    stdout.writeln('${name.padRight(20)} p50 ${(percentile(xs, 0.5) / 1000).toStringAsFixed(2)} ms  p99 ${(percentile(xs, 0.99) / 1000).toStringAsFixed(2)} ms  min ${(xs.reduce((a, b) => a < b ? a : b) / 1000).toStringAsFixed(2)} ms');
  }
}
