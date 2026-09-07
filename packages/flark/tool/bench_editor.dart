/// Keystroke budget receipt: one InsertText and one DeleteBackward through
/// the facade on a dense document, plus the parse and projection alone.
/// Run from packages/flark:
///   dart run tool/bench_editor.dart [kibibytes] [--spike|--flat-list|--prose]
library;

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flark/flark.dart';

/// The M0 spike's document (spikes/v5/dart_harness/bin/keystroke.dart), for
/// numbers comparable with the RFC 030 receipts.
String spikeDocument(int bytes) {
  final s = StringBuffer();
  var i = 0;
  while (s.length < bytes) {
    s.write(
      '## Section $i\n\nThis is a paragraph with *emphasis*, **strong**, `code`, a [link](https://example.com/$i) and some ~~struck~~ text that wraps across several lines of ordinary prose so that the row is realistic.\n\n- item one with *em*\n- item two with **strong**\n  - nested item\n\n> a quote with `code` inside\n\n```dart\nvoid main() { print(\'hi $i\'); }\n```\n\n| a | b |\n|---|---|\n| 1 | *2* |\n\n[ref$i]: https://example.com/ref$i\n\n',
    );
    i++;
  }
  return s.toString();
}

String denseDocument(int bytes) {
  final b = StringBuffer();
  var i = 0;
  while (b.length < bytes) {
    b.writeln('## Section $i\n');
    b.writeln(
      'Some **bold $i** text with *emphasis*, `code_$i`, a [link $i](http://example.com/$i), ~~struck~~ words and an image ![alt](http://x.y/$i.png) inline.',
    );
    b.writeln(
      'Second line with **more *nested* bold** and a footnote[^$i] plus &amp; entity and \\*escaped\\* stars.\n',
    );
    b.writeln(
      '- item **$i** with `code`\n- [x] task *$i* done\n  > quoted *inside* the item\n',
    );
    b.writeln(
      '1. first\n2. second with [ref][r$i]\n\n[r$i]: http://example.com/ref/$i\n',
    );
    b.writeln('```dart\nfinal x$i = "code";\n```\n');
    b.writeln('| col **a** | col *b* |\n| - | - |\n| `$i` | ~~$i~~ |\n');
    b.writeln('[^$i]: footnote *text* $i\n');
    i++;
  }
  return b.toString();
}

String flatListDocument(int bytes) {
  final b = StringBuffer();
  var i = 0;
  while (b.length < bytes) {
    b.writeln('- item $i');
    i++;
  }
  return b.toString();
}

String proseDocument(int bytes) {
  final b = StringBuffer();
  var i = 0;
  while (b.length < bytes) {
    b.writeln(
      'Paragraph $i is ordinary application prose with a little *emphasis*, '
      'one **strong phrase**, a [link](https://example.com/$i), and `inline '
      'code`. It continues for several sentences so source size does not imply '
      'thousands of tiny blocks. This is the shape expected for messages, notes, '
      'and authored documents rather than a structural stress corpus.\n',
    );
    i++;
  }
  return b.toString();
}

int percentile(List<int> xs, double p) {
  final s = List.of(xs)..sort();
  return s[((s.length - 1) * p).round()];
}

void main(List<String> args) {
  final kb = args.isNotEmpty ? int.parse(args[0]) : 25;
  final spike = args.contains('--spike');
  final flatList = args.contains('--flat-list');
  final prose = args.contains('--prose');
  final backend = createParseBackend();
  final source = flatList
      ? flatListDocument(kb * 1024)
      : prose
      ? proseDocument(kb * 1024)
      : spike
      ? spikeDocument(kb * 1024)
      : denseDocument(kb * 1024);
  final sourceBytes = utf8.encode(source).length;
  // This tool measures the live kernel, including sizes above the product's
  // conservative source-mode boundary. Admit the generated fixture explicitly
  // so a larger benchmark cannot silently measure source mode instead.
  final fixtureModel = backend.parse(source);
  final editor = FlarkEditor(
    backend,
    text: source,
    caret: source.length ~/ 2,
    syncLimit: sourceBytes + 1024,
    sourceLimit: sourceBytes + 1024,
    liveLimits: FlarkLiveLimits(
      lines: fixtureModel.lineCount + 2,
      lineCodeUnits: source.length + 1,
      blocks: fixtureModel.blockCount + 8,
      runs: fixtureModel.runCount + 8,
      blockCodeUnits: source.length + 1,
    ),
  );
  final inserts = <int>[],
      deletes = <int>[],
      parses = <int>[],
      projections = <int>[];
  final sw = Stopwatch();
  for (var i = 0; i < 500; i++) {
    final before = editor.source;
    sw.reset();
    sw.start();
    final inserted = editor.apply(const InsertText('x'));
    sw.stop();
    if (i >= 100) inserts.add(sw.elapsedMicroseconds);
    if (!inserted || editor.source == before || editor.sourceMode) {
      throw StateError('benchmark insertion did not commit live');
    }
    sw.reset();
    sw.start();
    final deleted = editor.apply(const DeleteBackward());
    sw.stop();
    if (i >= 100) deletes.add(sw.elapsedMicroseconds);
    if (!deleted || editor.source != before || editor.sourceMode) {
      throw StateError('benchmark Backspace did not restore source');
    }
  }
  for (var i = 0; i < 300; i++) {
    sw.reset();
    sw.start();
    final m = backend.parse(editor.source);
    sw.stop();
    if (i >= 50) parses.add(sw.elapsedMicroseconds);
    sw.reset();
    sw.start();
    Projection.of(m, editor.source);
    sw.stop();
    if (i >= 50) projections.add(sw.elapsedMicroseconds);
  }
  final model = editor.document.model;
  final shape = flatList
      ? 'flat-list'
      : prose
      ? 'prose'
      : spike
      ? 'spike'
      : 'dense';
  stdout.writeln(
    '\n$shape document $sourceBytes UTF-8 bytes, ${model.blockCount} blocks, ${model.runCount} runs, ${editor.projection.rows.length} rows',
  );
  for (final (name, xs) in [
    ('insert (facade)', inserts),
    ('backspace (facade)', deletes),
    ('parse only', parses),
    ('projection only', projections),
  ]) {
    stdout.writeln(
      '${name.padRight(20)} p50 ${(percentile(xs, 0.5) / 1000).toStringAsFixed(2)} ms  p99 ${(percentile(xs, 0.99) / 1000).toStringAsFixed(2)} ms  min ${(xs.reduce((a, b) => a < b ? a : b) / 1000).toStringAsFixed(2)} ms',
    );
  }
  final paths =
      (Process.runSync('git', [
                'ls-files',
                '--cached',
                '--others',
                '--exclude-standard',
                'lib',
                'tool/bench_editor.dart',
                '../../native/flark_parse/src',
                '../../native/flark_parse/Cargo.toml',
                '../../native/flark_parse/Cargo.lock',
                '../../rust-toolchain.toml',
                'hook/build.dart',
                'pubspec.yaml',
                'pubspec.lock',
              ]).stdout
              as String)
          .trim()
          .split('\n')
        ..add('pubspec.lock')
        ..sort();
  final tree = StringBuffer();
  for (final path in paths) {
    if (path.isNotEmpty && File(path).existsSync()) {
      tree.writeln('$path ${sha256.convert(File(path).readAsBytesSync())}');
    }
  }
  stdout.writeln(
    jsonEncode({
      'head': (Process.runSync('git', ['rev-parse', 'HEAD']).stdout as String)
          .trim(),
      'evidence': 'local working tree',
      'sourceTreeSha256': sha256
          .convert(utf8.encode(tree.toString()))
          .toString(),
      'runtime': Platform.version,
      'os': Platform.operatingSystemVersion,
      'fixture': shape,
      'sourceBytes': sourceBytes,
      'fixtureSha256': sha256.convert(utf8.encode(source)).toString(),
      'modelSha256': sha256.convert(backend.parse(source).bytes).toString(),
      'schema': backend.schemaVersion,
      'blocks': model.blockCount,
      'runs': model.runCount,
      'rows': editor.projection.rows.length,
      'warmupPairs': 100,
      'measuredPairs': inserts.length,
      'insertP99Us': percentile(inserts, .99),
      'backspaceP99Us': percentile(deletes, .99),
    }),
  );
}
