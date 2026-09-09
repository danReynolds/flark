/// The host's generated matrix: random kernel command sequences, with the cell
/// geometry contract checked at several widths after every command. Glyphs stay
/// inside the grid, a row's visual lines partition its display text, every
/// column resolves to a source offset and the caret is always addressable.
/// A failure prints the seed and command log so it can be minimized into a
/// directly named regression.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:test/test.dart';

const _alphabet = [
  'a', 'b', ' ', ' ', '*', '*', '_', '`', '#', '-', '>', '[', ']', '(', ')',
  '{', '}', '\n', '|', '!', '\\', '1', '.', '~', 'é', '😀', '\t', ':', '<', '&',
];

FlarkCommand randomCommand(Random r, FlarkEditor e) {
  final k = r.nextInt(100);
  if (k < 40) return InsertText(_alphabet[r.nextInt(_alphabet.length)]);
  if (k < 52) return const DeleteBackward();
  if (k < 58) return const DeleteForward();
  if (k < 63) return Newline(paragraph: r.nextBool());
  if (k < 75) {
    return MoveCaret(
      r.nextBool() ? MoveDirection.forward : MoveDirection.backward,
      unit: MoveUnit.values[r.nextInt(MoveUnit.values.length)],
      extend: r.nextInt(4) == 0,
    );
  }
  if (k < 80 && !e.sourceMode) {
    final rows = e.projection.rows;
    final row = r.nextInt(rows.length);
    return PlaceCaret(row, r.nextInt(rows[row].text.length + 1),
        leadingHalf: r.nextBool(), extend: r.nextInt(4) == 0);
  }
  if (k < 84) {
    return ToggleStyle([Style.emphasis, Style.strong, Style.code,
        Style.strikethrough][r.nextInt(4)]);
  }
  if (k < 87) return SetHeadingLevel(r.nextInt(7));
  if (k < 89) return const ToggleTask();
  if (k < 91) return r.nextBool() ? const Indent() : const Outdent();
  if (k == 94) return const SelectAll();
  if (k < 95) return const Undo();
  if (k < 97) return const Redo();
  if (k < 99) {
    return Paste(['**p**', '- x\n- y', '> q', '```\nc\n```',
        '| a |\n| - |\n| b |'][r.nextInt(5)]);
  }
  final len = e.source.length;
  final a = r.nextInt(len + 1);
  return ReplaceRange(a, min(len, a + r.nextInt(6)), 'r');
}

void checkLayout(FlarkEditor editor, CellDocumentLayout layout, int cols,
    String label) {
  final rows = editor.sourceMode ? null : editor.projection.rows;
  expect(layout.lines, isNotEmpty, reason: '$label: no lines');
  // 1. Glyph geometry stays inside the grid and is ordered.
  for (var i = 0; i < layout.lines.length; i++) {
    final line = layout.lines[i];
    expect(line.prefix.length, lessThanOrEqualTo(cols),
        reason: '$label line $i: prefix wider than the grid');
    var col = line.prefix.length, offset = line.start;
    for (final glyph in line.glyphs) {
      expect(glyph.col, greaterThanOrEqualTo(col),
          reason: '$label line $i: glyph column moved backwards');
      expect(glyph.col + glyph.width, lessThanOrEqualTo(cols),
          reason: '$label line $i: glyph ${jsonEncode(glyph.text)} at '
              '${glyph.col}+${glyph.width} overflows $cols');
      expect(glyph.width, greaterThan(0), reason: '$label line $i: zero width');
      expect(glyph.start, offset,
          reason: '$label line $i: glyph offsets not contiguous');
      expect(glyph.end, greaterThan(glyph.start),
          reason: '$label line $i: empty glyph range');
      col = glyph.col + glyph.width;
      offset = glyph.end;
    }
    expect(line.end, greaterThanOrEqualTo(line.start),
        reason: '$label line $i: end before start');
    // 2. Every column resolves to an offset the row can map to a source.
    for (var c = 0; c <= cols; c++) {
      final (hit, _) = line.hit(c);
      final source = line.sourceAt(hit);
      expect(source, inInclusiveRange(0, editor.source.length),
          reason: '$label line $i col $c: source $source out of range');
      if (line.row != null) {
        expect(hit, inInclusiveRange(0, line.row!.text.length),
            reason: '$label line $i col $c: display $hit outside row text');
      }
    }
    // 3. columnAt stays in the grid for every offset the line owns.
    for (var o = line.start; o <= line.end; o++) {
      final c = line.columnAt(o);
      expect(c, inInclusiveRange(0, cols),
          reason: '$label line $i offset $o: column $c outside 0..$cols');
    }
  }
  // 4. Each projected row's lines partition its display text contiguously.
  if (rows != null) {
    var index = 0;
    for (final row in rows) {
      final owned = <CellLine>[];
      while (index < layout.lines.length &&
          identical(layout.lines[index].row, row)) {
        owned.add(layout.lines[index]);
        index++;
      }
      expect(owned, isNotEmpty, reason: '$label: row ${row.index} has no line');
      expect(owned.first.start, 0,
          reason: '$label: row ${row.index} does not start at 0');
      for (var i = 1; i < owned.length; i++) {
        final skipped =
            row.text.substring(owned[i - 1].end, owned[i].start);
        expect(skipped == '' || skipped == '\n' || skipped == '\r\n', isTrue,
            reason: '$label: row ${row.index} line $i skips '
                '${jsonEncode(skipped)} between visual lines');
      }
      expect(owned.last.end, row.text.length,
          reason: '$label: row ${row.index} lines stop before its text end');
    }
    expect(index, layout.lines.length,
        reason: '$label: lines not in projection row order');
  }
  // 5. The caret is addressable.
  final caret = layout.positionFor(editor.selection.extent);
  expect(caret.row, inInclusiveRange(0, layout.lines.length - 1),
      reason: '$label: caret row ${caret.row} outside the layout');
  expect(caret.col, inInclusiveRange(0, cols),
      reason: '$label: caret column ${caret.col} outside 0..$cols');
}

void main() {
  final backend = createParseBackend();
  final iterations =
      int.tryParse(Platform.environment['FLARK_MATRIX_ITERATIONS'] ?? '') ?? 60;
  final seed =
      int.tryParse(Platform.environment['FLARK_MATRIX_SEED'] ?? '') ?? 2026;
  final widths = <int>[4, 7, 12, 40, 200];
  final corpus = <String>[];
  for (final name in ['common_mark_tests.json', 'gfm_tests.json']) {
    final f = File('../../test/fixtures/commonmark/upstream/$name');
    if (!f.existsSync()) continue;
    for (final c in (jsonDecode(f.readAsStringSync()) as List)) {
      corpus.add((c as Map)['markdown'] as String);
    }
  }
  corpus.addAll([
    '# Title\n\n- one\n- [x] two\n  > quoted\n\n1. first\n\n```dart\ncode\n```\n\n| a | b |\n| - | - |\n| c | d |\n',
    '',
  ]);

  test(
      'random command sequences keep the cell geometry contract '
      '(seed $seed, $iterations sequences)', () {
    final master = Random(seed);
    for (var i = 0; i < iterations; i++) {
      final s = master.nextInt(1 << 30);
      final r = Random(s);
      final source = corpus[r.nextInt(corpus.length)]
          .replaceAll('\r\n', '\n')
          .replaceAll('\r', '\n');
      final editor = FlarkEditor(backend, text: source,
          caret: r.nextInt(source.length + 1));
      final controller = FlarkFleuryController(editor);
      final log = <String>[];
      try {
        for (var step = 0; step < 25; step++) {
          if (r.nextInt(20) == 0) {
            editor.setSourceMode(!editor.sourceMode);
            log.add('sourceMode ${editor.sourceMode}');
          }
          final c = randomCommand(r, editor);
          log.add('$c');
          editor.apply(c, at: Duration(milliseconds: step * 100));
          for (final cols in widths) {
            final layout = CellDocumentLayout(controller, cols,
                const FlarkCellTheme(), CellWidthPolicy.spec);
            checkLayout(editor, layout, cols, 'seed $s step $step cols $cols');
          }
        }
      } catch (error) {
        // ignore: avoid_print
        print('mine failure: seed $s source ${jsonEncode(source)}\n'
            'result ${jsonEncode(editor.source)} selection ${editor.selection}\n'
            '  ${log.join('\n  ')}');
        rethrow;
      } finally {
        controller.dispose();
      }
    }
  }, timeout: const Timeout(Duration(minutes: 10)));
}
