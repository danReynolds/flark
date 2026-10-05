/// The generated matrix: random command sequences over corpus documents, in
/// LF and CRLF spelling, with the kernel invariants checked after every
/// command. A refused command must leave no trace. Every applied edit passes
/// the structural oracles of `support/structure_oracles.dart`: rows it did
/// not touch keep their kinds and containers, no hidden markup is painted, a
/// typed letter or digit appears where the caret showed it with nothing else
/// shown changing, Return keeps the new line in the containers of the line it
/// split and shows only the break, a heading level applies to the caret's
/// row, and rows paint in source order, each up to the Markdown rules those
/// oracles name. Each sequence's history must undo through states it reached
/// back to its first source, then redo exactly to its end. Before that walk,
/// host actions follow the commands: an IME composition that commits or
/// cancels, a source-mode round trip, a programmatic select all and an exact
/// source splice. A failure prints the master seed, the sequence's index in
/// its run and its own seed, the document with its initial caret, the result,
/// the command log, and the repro of the `FlarkEditRecorder`
/// (`package:flark/recorder.dart`) every sequence's editor carries: Dart that
/// replays the sequence's calls as a test, to minimize into a direct
/// regression. FLARK_MATRIX_SEQUENCE runs only the sequence at that index of
/// the master seed's run.
///
/// The default seed passes. Long runs of other seeds (FLARK_MATRIX_SEED,
/// FLARK_MATRIX_ITERATIONS) still find rare classes, about 3 in 100,000
/// sequences on 2026-10-04: nested links and angle destinations edited
/// mid-construct, HTML blocks read lazily in quotes, an opener typed so a
/// later underline or rule is read anew, fences in nested lists.
library;

import 'dart:convert';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:flark/recorder.dart';
import 'package:test/test.dart';

import 'support/host.dart';
import 'support/invariants.dart';
import 'support/structure_oracles.dart';

// Other generated-command tests check their steps with the same oracle.
export 'support/structure_oracles.dart' show checkStructure;

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

/// The documents sequences start from: the upstream CommonMark and GFM cases
/// when the repository's fixtures are present, and two of the matrix's own.
List<String> matrixCorpus() => [
  for (final name in ['common_mark_tests.json', 'gfm_tests.json'])
    if (readHostFile('../../test/fixtures/commonmark/upstream/$name')
        case final json?)
      for (final c in jsonDecode(json) as List)
        (c as Map)['markdown'] as String,
  '# Title\n\nSome **bold** and *em* with `code` and [a link](http://x.y).\n\n- one\n- [x] two\n  > quoted\n\n1. first\n2. second\n\n```\ncode\n```\n\n| a | b |\n| - | - |\n| c | d |\n',
  '',
];

void main() {
  // The FFI parser on the Dart VM, the bundled Wasm parser under node.
  late final FlarkParseBackend backend;
  setUpAll(() async => backend = await loadTestBackend());
  final iterations =
      int.tryParse(hostEnvironment('FLARK_MATRIX_ITERATIONS') ?? '') ?? 60;
  final seed = int.tryParse(hostEnvironment('FLARK_MATRIX_SEED') ?? '') ?? 2026;
  // The one sequence to run, by its index in the master seed's run.
  final only = int.tryParse(hostEnvironment('FLARK_MATRIX_SEQUENCE') ?? '');
  final count = only == null ? '$iterations sequences' : 'sequence $only';
  final corpus = matrixCorpus();

  test(
    'random command sequences keep every invariant (seed $seed, $count)',
    () {
      final master = Random(seed);
      for (var i = 0; i < (only == null ? iterations : only + 1); i++) {
        final s = master.nextInt(1 << 30);
        if (only != null && i != only) continue;
        final r = Random(s);
        var source = corpus[r.nextInt(corpus.length)]
            .replaceAll('\r\n', '\n')
            .replaceAll('\r', '\n');
        // Every other sequence edits the CRLF spelling of its document.
        if (i.isOdd) source = source.replaceAll('\n', '\r\n');
        final caret = r.nextInt(source.length + 1);
        final editor = FlarkEditor(backend, text: source, caret: caret);
        // Records the calls the sequence makes, for a failure's repro.
        final recorder = FlarkEditRecorder();
        editor.recorder = recorder;
        final log = <String>[];
        try {
          checkStep(editor, 'seed $s load');
          final reached = {stateOf(editor)};
          for (var step = 0; step < 40; step++) {
            final c = randomCommand(r, editor), command = describeCommand(c);
            log.add(command);
            final label = 'seed $s step $step $command';
            final before = editor.snapshot, state = stateOf(editor);
            final revision = editor.revision;
            if (editor.apply(c, at: Duration(milliseconds: step * 100))) {
              checkStep(editor, label);
              checkStructure(before, c, editor, label, backend);
            } else {
              expect(stateOf(editor), state, reason: '$label: refused');
              expect(editor.revision, revision, reason: '$label: refused');
            }
            reached.add(stateOf(editor));
          }
          // Host actions draw from a stream of their own, so the commands a
          // seed replays stay as they were.
          final host = Random(s ^ 0x5eed);
          for (var k = 0; k < 3; k++) {
            hostAction(host, editor, log, 'seed $s host $k');
            reached.add(stateOf(editor));
          }
          checkHistory(editor, source, reached, 'seed $s');
        } catch (error) {
          // ignore: avoid_print
          print(
            'matrix failure: sequence $i of seed $seed (FLARK_MATRIX_SEED=$seed '
            'FLARK_MATRIX_SEQUENCE=$i runs it alone), its own seed $s\n'
            'source ${jsonEncode(source)} caret $caret\n'
            'result ${jsonEncode(editor.source)} selection ${editor.selection}\n'
            '  ${log.join('\n  ')}\n'
            'repro:\n${recorder.repro}',
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

/// One host action and its oracle. A composition that cancels restores the
/// state before it and leaves history as it was; one that commits is a single
/// undo step, which [checkHistory] walks with the rest. A source-mode round
/// trip keeps the source and the selection's offsets, a programmatic select
/// all keeps the source, and a refused source splice leaves no trace.
void hostAction(Random r, FlarkEditor editor, List<String> log, String label) {
  final before = stateOf(editor), revision = editor.revision;
  final undo = editor.history.undoTarget, redo = editor.history.redoTarget;
  switch (r.nextInt(4)) {
    case 0:
      editor.beginComposition();
      log.add('beginComposition()');
      for (var i = r.nextInt(3); i >= 0; i--) {
        final command = r.nextInt(3) == 0
            ? const DeleteBackward()
            : InsertText(_alphabet[r.nextInt(_alphabet.length)]);
        log.add(describeCommand(command));
        editor.apply(command);
        checkStep(editor, '$label composing');
      }
      if (r.nextBool()) {
        log.add('cancelComposition()');
        editor.cancelComposition();
        expect(stateOf(editor), before, reason: '$label: cancel');
        expect(
          identical(editor.history.undoTarget, undo) &&
              identical(editor.history.redoTarget, redo),
          isTrue,
          reason: '$label: cancel kept history',
        );
      } else {
        log.add('commitComposition()');
        editor.commitComposition();
      }
    case 1:
      log.add('setSourceMode(true), setSourceMode(false)');
      editor
        ..setSourceMode(true)
        ..setSourceMode(false);
      // Source mode has no table cells, so an unwritten cell's address does
      // not survive the trip; its source offsets do.
      expect(
        (editor.source, editor.selection.base, editor.selection.extent),
        (before.$1, before.$2.base, before.$2.extent),
        reason: '$label: source mode',
      );
    case 2:
      final codeBlock = r.nextBool();
      log.add('selectAll(codeBlock: $codeBlock)');
      editor.selectAll(codeBlock: codeBlock);
      expect(editor.source, before.$1, reason: '$label: select all');
    default:
      final length = editor.source.length, start = r.nextInt(length + 1);
      final end = min(length, start + r.nextInt(6));
      final text = ['', 'q', '\n', '**', '\r\n', '😀'][r.nextInt(6)];
      log.add('replaceSourceRange($start, $end, ${jsonEncode(text)})');
      if (!editor.replaceSourceRange(start, end, text)) {
        expect(stateOf(editor), before, reason: '$label: refused splice');
        expect(editor.revision, revision, reason: '$label: refused splice');
      }
  }
  checkStep(editor, label);
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
