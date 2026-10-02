/// The generated matrix: random command sequences over corpus documents, in
/// LF and CRLF spelling, with the kernel invariants checked after every
/// command. A refused command must leave no trace, a structural Backspace or
/// Delete must not paint hidden markup, change the kind of rows it does not
/// join or move the row after a removed empty line into or out of a
/// container, and each sequence's history must undo through states it reached
/// back to its first source, then redo exactly to its end. A failure prints
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
  MoveTableCell(:final backward) => 'MoveTableCell(backward: $backward)',
  PlaceCaret(:final row, :final offset, :final leadingHalf, :final extend) =>
    'PlaceCaret($row, $offset, leadingHalf: $leadingHalf, extend: $extend)',
  MoveCaret(:final direction, :final unit, :final extend) =>
    'MoveCaret(${direction.name}, unit: ${unit.name}, extend: $extend)',
  Undo() => 'Undo()',
  Redo() => 'Redo()',
  ToggleTask() => 'ToggleTask()',
  ToggleStyle(:final style) => 'ToggleStyle($style)',
  SetStyle(:final style, :final enabled) =>
    'SetStyle($style, enabled: $enabled)',
  SetHeadingLevel(:final level) => 'SetHeadingLevel($level)',
  SetCodeLanguage(:final language) => 'SetCodeLanguage($language)',
  Indent() => 'Indent()',
  Outdent() => 'Outdent()',
  SetLink(:final destination) => 'SetLink(${jsonEncode(destination)})',
  SetImage(:final destination) => 'SetImage(${jsonEncode(destination)})',
  RemoveLink() => 'RemoveLink()',
  RemoveImage() => 'RemoveImage()',
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
        var source = corpus[r.nextInt(corpus.length)]
            .replaceAll('\r\n', '\n')
            .replaceAll('\r', '\n');
        // Every other sequence edits the CRLF spelling of its document.
        if (i.isOdd) source = source.replaceAll('\n', '\r\n');
        final editor = FlarkEditor(
          backend,
          text: source,
          caret: r.nextInt(source.length + 1),
        );
        final log = <String>[];
        try {
          checkStep(editor, 'seed $s load');
          final reached = {stateOf(editor)};
          for (var step = 0; step < 40; step++) {
            final c = randomCommand(r, editor);
            log.add(describeCommand(c));
            final label = 'seed $s step $step $c';
            final before = editor.snapshot, state = stateOf(editor);
            final revision = editor.revision;
            if (editor.apply(c, at: Duration(milliseconds: step * 100))) {
              checkStep(editor, label);
              checkStructure(before, c, editor, label);
            } else {
              expect(stateOf(editor), state, reason: '$label: refused');
              expect(editor.revision, revision, reason: '$label: refused');
            }
            reached.add(stateOf(editor));
          }
          checkHistory(editor, source, reached, 'seed $s');
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

/// What history restores: the source, the selection and the typing intent.
(String, FlarkSelection, int) stateOf(FlarkEditor editor) =>
    (editor.source, editor.selection, editor.typingContext);

/// Undo all the way back passes only through states the sequence reached
/// and ends at its first source; as many Redos return exactly to the end.
void checkHistory(
  FlarkEditor editor,
  String first,
  Set<(String, FlarkSelection, int)> reached,
  String label,
) {
  final end = stateOf(editor);
  var undone = 0;
  while (editor.history.canUndo && undone < 100) {
    expect(editor.apply(const Undo()), isTrue, reason: '$label undo');
    undone++;
    checkStep(editor, '$label undo $undone');
    expect(
      reached,
      contains(stateOf(editor)),
      reason: '$label: undo $undone reached an unseen state',
    );
  }
  expect(editor.source, first, reason: '$label: undo back to the start');
  for (var i = 0; i < undone; i++) {
    expect(editor.apply(const Redo()), isTrue, reason: '$label redo');
    checkStep(editor, '$label redo ${i + 1}');
  }
  expect(stateOf(editor), end, reason: '$label: redo back to the end');
}

/// A Backspace at a row's start or a Delete at its end lifts container
/// markup or joins two rows. It removes line breaks and markup, so it must
/// not paint a character the projection hid (a setext underline, a closing
/// fence or sequence, a rule), rows it did not act on keep their kind, and
/// the row after a removed empty line or rule keeps its containers.
/// The edit is located by the source both versions share at the start and
/// at the end; since a deleted `- ` beside another can sit at either, both
/// readings must agree. A painted character must also be new to the text
/// shown, whitespace aside, so a reinterpretation that only respells shown
/// text (an entity left code) does not count.
void checkStructure(
  FlarkEditorSnapshot before,
  FlarkCommand command,
  FlarkEditor editor,
  String label,
) {
  if (before is! FlarkLiveSnapshot || editor.sourceMode) return;
  final old = before.document, next = editor.document;
  if (!old.selection.isCollapsed) return;
  final at = old.displayOf(old.selection.extent);
  // The rows the command acts on: the caret's, and the one it joins.
  final int other;
  switch (command) {
    case DeleteBackward(word: false) when at.offset == 0:
      other = at.row - 1;
    case DeleteForward(word: false)
        when at.offset == old.projection.rows[at.row].text.length:
      other = at.row + 1;
    default:
      return;
  }
  final a = old.source, b = next.source;
  (int, int) shared({required bool prefixFirst}) {
    int prefix(int limit) {
      var p = 0;
      while (p < limit && a.codeUnitAt(p) == b.codeUnitAt(p)) {
        p++;
      }
      return p;
    }

    int suffix(int limit) {
      var q = 0;
      while (q < limit &&
          a.codeUnitAt(a.length - 1 - q) == b.codeUnitAt(b.length - 1 - q)) {
        q++;
      }
      return q;
    }

    final shorter = a.length < b.length ? a.length : b.length;
    if (prefixFirst) {
      final p = prefix(shorter);
      return (p, suffix(shorter - p));
    }
    final q = suffix(shorter);
    return (prefix(shorter - q), q);
  }

  final alignments = [shared(prefixFirst: true), shared(prefixFirst: false)];
  final delta = b.length - a.length;
  // Removing an empty line or marker leaves the parser's reading of the
  // lines around it, which may present a bare marker as text. Joining the
  // line break before a fence that displays nothing turns its delimiter line
  // into text, by design: the way to delete one with no body.
  final joined =
      old.projection.rows[command is DeleteBackward
          ? at.row
          : other.clamp(0, old.projection.rows.length - 1)];
  final exempt =
      joined.text.isEmpty ||
      joined.fenced && joined.contentStarts.every((start) => start < 0);
  if (!exempt) {
    final painted = List<bool>.filled(a.length, false);
    for (final row in old.projection.rows) {
      for (final s in row.segments) {
        if (!s.lineBreak) painted.fillRange(s.sourceStart, s.sourceEnd, true);
      }
    }
    var revealed = -1;
    for (final row in next.projection.rows) {
      for (final s in row.segments) {
        if (s.lineBreak) continue;
        for (var o = s.sourceStart; o < s.sourceEnd && revealed < 0; o++) {
          if (b[o].trim().isEmpty) continue;
          final hidden = alignments.every((alignment) {
            final (prefix, suffix) = alignment;
            final was = o < prefix
                ? o
                : o >= b.length - suffix
                ? o - delta
                : -1;
            return was >= 0 && !painted[was];
          });
          if (hidden) revealed = o;
        }
      }
    }
    String shown(Projection projection) => [
      for (final row in projection.rows) row.text.replaceAll(RegExp(r'\s'), ''),
    ].join();
    final was = shown(old.projection), now = shown(next.projection);
    var i = 0, subsequence = true;
    for (var j = 0; j < now.length && subsequence; j++) {
      while (i < was.length && was.codeUnitAt(i) != now.codeUnitAt(j)) {
        i++;
      }
      subsequence = i++ < was.length;
    }
    if (revealed >= 0 && !subsequence) {
      fail(
        '$label: painted hidden source ${jsonEncode(a)} -> '
        '${jsonEncode(b)} at $revealed',
      );
    }
  }
  for (final row in old.projection.rows) {
    if (row.kind == RowKind.blank ||
        row.index == at.row ||
        row.index == other) {
      continue;
    }
    final int mapped;
    if (alignments.every((alignment) => row.sourceEnd < alignment.$1)) {
      mapped = row.sourceStart;
    } else if (alignments.every(
      (alignment) => row.sourceStart >= a.length - alignment.$2,
    )) {
      mapped = row.sourceStart + delta;
    } else {
      continue;
    }
    expect(
      next.rowAt(mapped).kind,
      row.kind,
      reason:
          '$label: row ${row.index} changed kind, '
          '${jsonEncode(a)} -> ${jsonEncode(b)}',
    );
  }
  // Removing an empty line or a rule brings the row after it up against the
  // row before, and that row keeps the kinds of its containers: without the
  // gap, a paragraph would read on lazily inside a quote or list item above
  // it. A line with a container prefix is lifted instead, and lifting moves
  // containers by design, so only a row the command removes counts.
  final rows = old.projection.rows;
  bool gap(int index) =>
      index >= 0 &&
      index < rows.length &&
      (rows[index].kind == RowKind.blank ||
          rows[index].kind == RowKind.thematicBreak);
  final caretRow = rows[at.row];
  final i = old.model.lineOfUtf16(old.selection.extent) - caretRow.firstLine;
  final line = i.clamp(0, caretRow.contentStarts.length - 1);
  final lifts =
      caretRow.prefixStarts[line] >= 0 &&
      caretRow.prefixStarts[line] < caretRow.contentStarts[line];
  final removed = command is DeleteBackward
      ? (lifts ? -1 : (gap(at.row) ? at.row : (gap(other) ? other : -1)))
      : (gap(other) ? other : (gap(at.row) ? at.row : -1));
  if (removed < 0 || removed + 1 >= rows.length) return;
  final following = rows[removed + 1];
  if (!alignments.every(
    (alignment) => following.sourceStart >= a.length - alignment.$2,
  )) {
    return;
  }
  String shells(ProjectedRow row) =>
      row.shells.map((shell) => shell.kind.name).join('/');
  expect(
    shells(next.rowAt(following.sourceStart + delta)),
    shells(following),
    reason:
        '$label: removing row $removed moved row ${following.index} '
        'between containers, ${jsonEncode(a)} -> ${jsonEncode(b)}',
  );
}
