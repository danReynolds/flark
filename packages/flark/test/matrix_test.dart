/// The generated matrix: random command sequences over corpus documents,
/// with the kernel invariants checked after every command. A failure prints
/// the seed and command log so it can be minimized into a direct regression.
library;

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

const _alphabet = [
  'a',
  'b',
  ' ',
  ' ',
  '*',
  '*',
  '_',
  '`',
  '#',
  '-',
  '>',
  '[',
  ']',
  '(',
  ')',
  '{',
  '}',
  '\n',
  '|',
  '!',
  '\\',
  '1',
  '.',
  '~',
  'é',
  '😀',
  '\t',
  ':',
  '<',
  '&',
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
  if (k < 80) {
    final rows = e.projection.rows;
    final row = r.nextInt(rows.length);
    return PlaceCaret(
      row,
      r.nextInt(rows[row].text.length + 1),
      leadingHalf: r.nextBool(),
      extend: r.nextInt(4) == 0,
    );
  }
  if (k < 84) {
    return ToggleStyle(
      [Style.emphasis, Style.strong, Style.code, Style.strikethrough][r.nextInt(
        4,
      )],
    );
  }
  if (k < 87) return SetHeadingLevel(r.nextInt(7));
  if (k < 89) return const ToggleTask();
  if (k < 91) return r.nextBool() ? const Indent() : const Outdent();
  if (k == 94) return const SelectAll();
  if (k < 95) return const Undo();
  if (k < 97) return const Redo();
  if (k < 99) {
    return Paste(
      ['**p**', '- x\n- y', '> q', '```\nc\n```', '| a |\n| - |\n| b |'][r
          .nextInt(5)],
    );
  }
  final len = e.source.length;
  final a = r.nextInt(len + 1);
  return ReplaceRange(a, min(len, a + r.nextInt(6)), 'r');
}

String describeCommand(FlarkCommand command) => switch (command) {
  InsertText(:final text) => 'InsertText(${jsonEncode(text)})',
  Paste(:final text) => 'Paste(${jsonEncode(text)})',
  DeleteBackward() => 'DeleteBackward()',
  DeleteForward() => 'DeleteForward()',
  Newline(:final paragraph) => 'Newline(paragraph: $paragraph)',
  ReplaceRange(:final start, :final end, :final text) =>
    'ReplaceRange($start, $end, ${jsonEncode(text)})',
  SetSelection(:final base, :final extent) => 'SetSelection($base, $extent)',
  SelectAll() => 'SelectAll()',
  PlaceCaret(:final row, :final offset, :final leadingHalf, :final extend) =>
    'PlaceCaret($row, $offset, leadingHalf: $leadingHalf, extend: $extend)',
  MoveCaret(:final direction, :final unit, :final extend) =>
    'MoveCaret(${direction.name}, unit: ${unit.name}, extend: $extend)',
  Undo() => 'Undo()',
  Redo() => 'Redo()',
  ToggleTask() => 'ToggleTask()',
  ToggleStyle(:final style) => 'ToggleStyle($style)',
  SetHeadingLevel(:final level) => 'SetHeadingLevel($level)',
  SetCodeLanguage(:final language) => 'SetCodeLanguage($language)',
  Indent() => 'Indent()',
  Outdent() => 'Outdent()',
};

void main() {
  final backend = createParseBackend();
  final iterations =
      int.tryParse(Platform.environment['FLARK_MATRIX_ITERATIONS'] ?? '') ?? 60;
  final seed =
      int.tryParse(Platform.environment['FLARK_MATRIX_SEED'] ?? '') ?? 2026;
  final corpus = <String>[];
  for (final name in ['common_mark_tests.json', 'gfm_tests.json']) {
    final f = File('../../test/fixtures/commonmark/upstream/$name');
    if (!f.existsSync()) continue;
    for (final c in (jsonDecode(f.readAsStringSync()) as List)) {
      corpus.add((c as Map)['markdown'] as String);
    }
  }
  corpus.addAll([
    '# Title\n\nSome **bold** and *em* with `code` and [a link](http://x.y).\n\n- one\n- [x] two\n  > quoted\n\n1. first\n2. second\n\n```\ncode\n```\n\n| a | b |\n| - | - |\n| c | d |\n',
    '',
  ]);

  test(
    'random command sequences keep every invariant (seed $seed, $iterations sequences)',
    () {
      final master = Random(seed);
      for (var i = 0; i < iterations; i++) {
        final s = master.nextInt(1 << 30);
        final r = Random(s);
        final source = corpus[r.nextInt(corpus.length)]
            .replaceAll('\r\n', '\n')
            .replaceAll('\r', '\n');
        final editor = FlarkEditor(
          backend,
          text: source,
          caret: r.nextInt(source.length + 1),
        );
        final log = <String>[];
        try {
          checkStep(editor, 'seed $s load');
          for (var step = 0; step < 40; step++) {
            final c = randomCommand(r, editor);
            log.add(describeCommand(c));
            editor.apply(c, at: Duration(milliseconds: step * 100));
            checkStep(editor, 'seed $s step $step $c');
          }
        } catch (error) {
          // ignore: avoid_print
          print(
            'matrix failure: seed $s source ${jsonEncode(source)}\n'
            'result ${jsonEncode(editor.source)} selection ${editor.selection}\n'
            '  ${log.join('\n  ')}',
          );
          rethrow;
        }
      }
    },
    timeout: const Timeout(Duration(minutes: 5)),
  );
}

void checkStep(FlarkEditor editor, String label) {
  final doc = editor.document;
  final wholeSource =
      !doc.selection.isCollapsed &&
      doc.selection.start == 0 &&
      doc.selection.end == doc.source.length;
  expect(
    wholeSource || doc.isLegal(doc.selection.base),
    isTrue,
    reason: '$label: base ${doc.selection.base} legal',
  );
  expect(
    wholeSource || doc.isLegal(doc.selection.extent),
    isTrue,
    reason: '$label: extent ${doc.selection.extent} legal',
  );
  checkInvariants(doc.source, doc.model, doc.projection, label);
}
