/// A projection built with the previous one reuses the rows of blocks an
/// edit did not touch. It must equal a projection built from scratch, after
/// one edit and after a second edit that reuses rows already reused.
library;

import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:flark/rendering.dart';
import 'package:flark/src/kernel/document.dart' show validateFlarkSource;
import 'package:test/test.dart';

import 'support/invariants.dart';

const _insertions = [
  'x',
  '\n',
  '\n\n',
  '*',
  '**',
  '`',
  '> ',
  '- ',
  '1. ',
  '# ',
  '|',
  '```\n',
  '===\n',
  '[a](b)',
  '[^1]',
  '&amp;',
  '\\',
  '    ',
  'é😀',
  '[r]: /u\n',
  // A footnote definition turns every `[^1]` text run into a reference run
  // with the same range: only the run's kind tells them apart.
  '[^1]: n\n',
];

List<String> _corpus() {
  const root = '../../test/fixtures/commonmark/upstream';
  return [
    for (final name in ['common_mark_tests.json', 'gfm_tests.json'])
      for (final c
          in jsonDecode(File('$root/$name').readAsStringSync()) as List)
        (c as Map)['markdown'] as String,
  ];
}

String _mixed(int sections) {
  final b = StringBuffer();
  for (var i = 0; i < sections; i++) {
    b
      ..writeln('## Section $i\n')
      ..writeln('Some **bold $i** text with *em*, `code`, a [link](u$i) and')
      ..writeln('a second line with ~~strike~~ &amp; \\*escapes\\*.\n')
      ..writeln('- item **$i**\n- [x] task\n  > quoted *inside*\n')
      ..writeln('1. first\n2. second [ref][r$i]\n\n[r$i]: /ref/$i\n')
      ..writeln('```dart\nfinal x$i = 1;\n```\n')
      ..writeln('| a | b |\n| - | :-: |\n| `$i` | ~~$i~~ |\n')
      ..writeln('> quote $i\n>\n> > nested\n')
      ..writeln('A footnote[^$i] and [^1] in text.\n')
      // Alone, a reference and its text differ only in the run's kind.
      ..writeln('[^1]\n');
  }
  return b.toString();
}

/// Offsets to edit at: the ends, and a spread of line starts and middles.
List<int> _offsets(String source) {
  final lineStarts = [0];
  for (var i = 0; i < source.length; i++) {
    if (source.codeUnitAt(i) == 0x0A) lineStarts.add(i + 1);
  }
  final step = lineStarts.length > 8 ? lineStarts.length ~/ 8 : 1;
  return {
    0,
    source.length,
    source.length ~/ 2,
    for (var i = 0; i < lineStarts.length; i += step) lineStarts[i],
    for (var i = 0; i < lineStarts.length; i += step)
      (lineStarts[i] + 2).clamp(0, source.length),
  }.toList();
}

/// [source] with [remove] code units at [at] replaced by [insert], or null
/// when the result is outside the source contract (split pairs, bare CR).
String? _edit(String source, int at, int remove, String insert) {
  if (at + remove > source.length) return null;
  final result = source.replaceRange(at, at + remove, insert);
  try {
    validateFlarkSource(result);
  } on FormatException {
    return null;
  }
  return result;
}

void main() {
  final backend = createParseBackend();

  // Sources the extraction refuses would open in source mode instead.
  RenderModel? parse(String source) {
    try {
      return backend.parse(source);
    } on FlarkParseException catch (e) {
      if (e.code == FlarkParseException.extractionDeviationCode) return null;
      rethrow;
    }
  }

  void check(String before, String after, String label) {
    final model0 = parse(before), model = parse(after);
    if (model0 == null || model == null) return;
    final first = Projection.of(model0, before);
    final reused = Projection.of(model, after, previous: first);
    expectSameProjection(reused, Projection.of(model, after), label);
    // A second edit reuses rows that were themselves reused.
    final again = _edit(after, after.length ~/ 3, 0, 'y');
    if (again == null) return;
    final model2 = parse(again);
    if (model2 == null) return;
    expectSameProjection(
      Projection.of(model2, again, previous: reused),
      Projection.of(model2, again),
      '$label, then y at ${after.length ~/ 3}',
    );
  }

  test(
    'corpus documents: insertions and deletions at many offsets',
    () {
      var edits = 0;
      for (final source in _corpus()) {
        for (final at in _offsets(source)) {
          for (final insert in _insertions) {
            final after = _edit(source, at, 0, insert);
            if (after == null) continue;
            check(
              source,
              after,
              '${jsonEncode(source)}: insert $insert at $at',
            );
            edits++;
          }
          for (final remove in [1, 2, 5]) {
            final after = _edit(source, at, remove, '');
            if (after == null) continue;
            check(
              source,
              after,
              '${jsonEncode(source)}: remove $remove at $at',
            );
            edits++;
          }
        }
      }
      expect(edits, greaterThan(50000));
    },
    timeout: const Timeout(Duration(minutes: 10)),
  );

  test('a large mixed document: every insertion at every line start', () {
    final source = _mixed(12);
    final lineStarts = [
      0,
      for (var i = 0; i < source.length; i++)
        if (source.codeUnitAt(i) == 0x0A) i + 1,
    ];
    for (final at in lineStarts) {
      for (final insert in _insertions) {
        final after = _edit(source, at, 0, insert);
        if (after != null) check(source, after, 'mixed: insert $insert at $at');
      }
      final removed = _edit(source, at, 3, '');
      if (removed != null) check(source, removed, 'mixed: remove 3 at $at');
    }
  });

  test('rows away from an edit are reused, not rebuilt', () {
    final source = _mixed(20);
    final before = Projection.of(backend.parse(source), source);
    final at = source.length ~/ 2;
    final after = source.replaceRange(at, at, 'x');
    final reused = Projection.of(backend.parse(after), after, previous: before);
    // A reused row keeps its text object; a rebuilt one has a new string.
    final previousTexts = Set<String>.identity()
      ..addAll(before.rows.map((row) => row.text));
    final kept = reused.rows
        .where((row) => row.text.isNotEmpty)
        .where((row) => previousTexts.contains(row.text))
        .length;
    final withText = reused.rows.where((row) => row.text.isNotEmpty).length;
    expect(kept, greaterThan(withText * 0.9));
  });

  test('a read-only document reuses rows when its markdown grows', () {
    final source = _mixed(20);
    final reader = FlarkReadDocument(backend, source);
    final before = reader.projection;
    // Appending, as a streamed response does, leaves every earlier row alone.
    final grown = '$source\nMore **streamed** text.\n';
    expect(reader.update(grown), isTrue);
    final after = reader.projection;
    expectSameProjection(
      after,
      Projection.of(reader.document.model, grown),
      'grown read-only document',
    );
    final previousTexts = Set<String>.identity()
      ..addAll(before.rows.map((row) => row.text));
    final withText = before.rows.where((row) => row.text.isNotEmpty).length;
    final kept = after.rows
        .where((row) => row.text.isNotEmpty)
        .where((row) => previousTexts.contains(row.text))
        .length;
    expect(kept, greaterThan(withText * 0.9));
  });
}
