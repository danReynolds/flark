// Measures untagged-fence detection: windows of files in each language, as
// fences excerpt them, and prose paragraphs, which should stay plain.
//
//   dart run tool/detect_eval.dart [language=dir ...] [prose=dir ...]
//
// With no arguments it reads the committed corpora in tool/corpus/ and the
// repository's Markdown. Each further `language=dir` adds that directory's
// files as that language, for wider local runs.
import 'dart:io';
import 'dart:math';

import 'package:flark_codemirror/src/detect.dart';
import 'package:flark_codemirror/src/languages.dart';
import 'package:flark_codemirror/src/mode.dart';

/// Non-space characters outside comments, as [language]'s own mode reads
/// [text]: windows with little code say little about detection.
int _codeUnits(String language, String text) {
  final mode = codeMirrorMode(language);
  if (mode == null) return text.replaceAll(RegExp(r'\s'), '').length;
  var n = 0;
  runMode(
    mode,
    text,
    onToken: (line, start, end, style) {
      if (style != null && style.contains('comment')) return;
      final lines = splitLines(text).lines;
      n += lines[line]
          .substring(start, end)
          .replaceAll(RegExp(r'\s'), '')
          .length;
    },
  );
  return n;
}

/// Answers that highlight a language well enough to count as right.
const _families = {
  'javascript': {'typescript'},
  'typescript': {'javascript'},
  'c': {'cpp'},
  'cpp': {'c'},
  'sql': {'postgresql', 'mysql'},
  'postgresql': {'sql', 'mysql'},
  'mysql': {'sql', 'postgresql'},
  'css': {'scss', 'less'},
  'scss': {'css', 'less'},
  'less': {'css', 'scss'},
  'html': {'xml', 'php'},
  'xml': {'html'},
  'php': {'html'},
};

const _sizes = [1, 2, 3, 5, 8, 13, 20, 30];

void main(List<String> args) {
  final root = Directory.current.path;
  final sources = <String, List<File>>{};
  final prose = <File>[];
  void add(String language, FileSystemEntity entity) {
    if (entity is Directory) {
      for (final f in entity.listSync(recursive: true).whereType<File>()) {
        (language == 'prose' ? prose : (sources[language] ??= [])).add(f);
      }
    } else if (entity is File) {
      (language == 'prose' ? prose : (sources[language] ??= [])).add(entity);
    }
  }

  if (args.isEmpty) {
    final corpus = Directory('$root/tool/corpus');
    for (final entity in corpus.listSync()) {
      final name = entity.uri.pathSegments.where((s) => s.isNotEmpty).last;
      if (entity is Directory) {
        add(name == 'html-mixed' ? 'html' : name, entity);
      } else if (name.endsWith('.js')) {
        add('javascript', entity);
      } else if (name.endsWith('.ts')) {
        add('typescript', entity);
      } else if (name.endsWith('.json') || name.endsWith('.jsonld')) {
        add('json', entity);
      }
    }
    add('prose', Directory('$root/../../docs'));
  }
  for (final arg in args) {
    final at = arg.indexOf('=');
    add(
      arg.substring(0, at),
      FileSystemEntity.isDirectorySync(arg.substring(at + 1))
          ? Directory(arg.substring(at + 1))
          : File(arg.substring(at + 1)),
    );
  }

  final random = Random(1);
  var total = 0, strictHits = 0, familyHits = 0, claimedAll = 0;
  final watch = Stopwatch();
  var detections = 0;
  String detect(String text) {
    watch.start();
    final language = detectCodeMirrorLanguage(text, CodeMirrorLanguages.all);
    watch.stop();
    detections++;
    return language;
  }

  final rows = <String>[];
  final languages = sources.keys.toList()..sort();
  for (final language in languages) {
    var n = 0, strict = 0, family = 0, plain = 0;
    final wrong = <String, int>{};
    final examples = <String>[];
    for (final file in sources[language]!) {
      final String text;
      try {
        text = file.readAsStringSync();
      } on FileSystemException {
        continue;
      }
      final lines = text.split('\n');
      final windows = <String>[
        lines.take(12).join('\n'),
        for (var i = 0; i < 3; i++)
          () {
            final size = min(
              _sizes[random.nextInt(_sizes.length)],
              lines.length,
            );
            final from = random.nextInt(lines.length - size + 1);
            return lines.sublist(from, from + size).join('\n');
          }(),
      ];
      for (final window in windows) {
        if (window.trim().length < 12 || _codeUnits(language, window) < 30) {
          continue;
        }
        final found = detect(window);
        n++;
        if (found == language) {
          strict++;
          family++;
        } else if (_families[language]?.contains(found) ?? false) {
          family++;
        } else {
          if (found.isEmpty) {
            plain++;
          } else {
            wrong[found] = (wrong[found] ?? 0) + 1;
          }
          if (examples.length < 5 && found.isNotEmpty) {
            final short = window.length > 160
                ? '${window.substring(0, 160)}...'
                : window;
            examples.add('    as $found: ${short.replaceAll('\n', r'\n')}');
          }
        }
      }
    }
    total += n;
    strictHits += strict;
    familyHits += family;
    claimedAll += n - plain;
    final top = (wrong.entries.toList()..sort((a, b) => b.value - a.value))
        .take(4)
        .map((e) => '${e.key} ${e.value}')
        .join(', ');
    rows.add(
      '${language.padRight(11)} n=${n.toString().padLeft(4)}  '
      'strict ${_pct(strict, n)}  family ${_pct(family, n)}  '
      'plain ${_pct(plain, n)}  precision ${_pct(family, n - plain)}  wrong: $top',
    );
    if (Platform.environment['EXAMPLES'] != null) rows.addAll(examples);
  }

  var paragraphs = 0, claimed = 0;
  final claims = <String, int>{};
  final claimExamples = <String>[];
  for (final file in prose.where((f) => f.path.endsWith('.md'))) {
    var inFence = false;
    final current = <String>[];
    void flush() {
      final text = current.join('\n').trim();
      current.clear();
      if (text.length < 12) return;
      paragraphs++;
      final found = detect(text);
      if (found.isNotEmpty) {
        claimed++;
        claims[found] = (claims[found] ?? 0) + 1;
        if (claimExamples.length < 40) {
          final short = text.length > 120
              ? '${text.substring(0, 120)}...'
              : text;
          claimExamples.add('    as $found: ${short.replaceAll('\n', r'\n')}');
        }
      }
    }

    for (final line in file.readAsLinesSync()) {
      if (line.trimLeft().startsWith('```')) {
        inFence = !inFence;
        flush();
        continue;
      }
      if (inFence) continue;
      if (line.trim().isEmpty || line.startsWith('|')) {
        flush();
      } else {
        current.add(line);
      }
    }
    flush();
  }

  // Tool output and logs, as windows of lines like the code's.
  var outputs = 0, outputClaims = 0;
  final outputClaimed = <String, int>{};
  final outputExamples = <String>[];
  for (final file in prose.where((f) => !f.path.endsWith('.md'))) {
    final String text;
    try {
      text = file.readAsStringSync();
    } on FileSystemException {
      continue;
    }
    final lines = text.split('\n');
    for (var k = 0; k < 4; k++) {
      final size = min(_sizes[random.nextInt(_sizes.length)], lines.length);
      final from = random.nextInt(lines.length - size + 1);
      final window = lines.sublist(from, from + size).join('\n');
      if (window.trim().length < 12) continue;
      outputs++;
      final found = detect(window);
      if (found.isNotEmpty) {
        outputClaims++;
        outputClaimed[found] = (outputClaimed[found] ?? 0) + 1;
        if (outputExamples.length < 60) {
          final short = window.length > 140
              ? '${window.substring(0, 140)}...'
              : window;
          outputExamples.add('    as $found: ${short.replaceAll('\n', r'\n')}');
        }
      }
    }
  }

  rows.forEach(print);
  print(
    'output: $outputClaims of $outputs windows claimed '
    '(${_pct(outputClaims, outputs)}): $outputClaimed',
  );
  if (Platform.environment['EXAMPLES'] != null) outputExamples.forEach(print);
  print(
    'code: ${_pct(strictHits, total)} strict, ${_pct(familyHits, total)} '
    'family of $total windows; precision ${_pct(familyHits, claimedAll)}',
  );
  print(
    'prose: $claimed of $paragraphs paragraphs claimed '
    '(${_pct(claimed, paragraphs)}): $claims',
  );
  if (Platform.environment['EXAMPLES'] != null) claimExamples.forEach(print);
  print(
    'time: ${(watch.elapsedMicroseconds / detections).toStringAsFixed(0)} us '
    'per detection over $detections (VM, JIT)',
  );
}

String _pct(int part, int whole) =>
    '${(whole == 0 ? 0 : 100 * part / whole).toStringAsFixed(0).padLeft(3)}%';
