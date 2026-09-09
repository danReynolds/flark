/// The corpus contract: every upstream case, in LF, CRLF, quoted and indented
/// form, checked against the invariants a host relies on but the projection
/// invariants alone do not state — that a legal caret round trips through its
/// display and its anchors, that offsets sharing a display position share an
/// anchor set, that movement never stalls, that Undo restores exactly, and that
/// Backspace can always empty a document. A failure names the source and the
/// rule so it can be minimized into `caret_contract_test.dart`.
library;

import 'dart:convert';
import 'dart:io';

import 'package:characters/characters.dart';
import 'package:flark/flark.dart';
import 'package:test/test.dart';

final failures = <String, List<String>>{};
void fail_(String rule, String detail) =>
    (failures[rule] ??= []).length < 6 ? failures[rule]!.add(detail) : null;

void checkStatic(String label, FlarkEditor e) {
  final doc = e.document, src = e.source, p = e.projection;
  // 1. Rows are in display order and their source ranges are ordered.
  for (var i = 1; i < p.rows.length; i++) {
    final a = p.rows[i - 1], b = p.rows[i];
    if (a.firstLine > b.firstLine) {
      fail_('rows-line-order', '$label: row $i line ${b.firstLine} after ${a.firstLine}');
    }
    if (a.firstLine == b.firstLine && a.sourceStart > b.sourceStart) {
      fail_('rows-source-order', '$label: row $i ${b.sourceStart} after ${a.sourceStart}');
    }
  }
  for (final row in p.rows) {
    // 2. Per-line arrays match the row's line count and stay on their line.
    if (row.contentStarts.length != row.lineCount ||
        row.contentEnds.length != row.lineCount ||
        row.prefixStarts.length != row.lineCount) {
      fail_('row-line-arrays', '$label: row ${row.index} kind ${row.kind} '
          'lineCount ${row.lineCount} starts ${row.contentStarts.length} '
          'ends ${row.contentEnds.length} prefixes ${row.prefixStarts.length}');
    }
    for (var i = 0; i < row.contentStarts.length && i < row.lineCount; i++) {
      final line = row.firstLine + i;
      if (line >= doc.model.lineCount) continue;
      final ls = doc.model.lineStartUtf16(line);
      final le = line + 1 < doc.model.lineCount
          ? doc.model.lineStartUtf16(line + 1)
          : src.length;
      if (row.contentStarts[i] < 0) continue; // no caret on this line
      if (row.contentStarts[i] < ls || row.contentEnds[i] > le) {
        fail_('row-line-bounds', '$label: row ${row.index} line $i '
            '${row.contentStarts[i]}..${row.contentEnds[i]} outside $ls..$le');
      }
      if (row.contentStarts[i] > row.contentEnds[i]) {
        fail_('row-content-inverted', '$label: row ${row.index} line $i '
            '${row.contentStarts[i]}..${row.contentEnds[i]}');
      }
      if (row.prefixStarts[i] > row.contentStarts[i]) {
        fail_('row-prefix-after-content', '$label: row ${row.index} line $i '
            'prefix ${row.prefixStarts[i]} content ${row.contentStarts[i]}');
      }
    }
    // 3. Segments stay inside the row and advance.
    var last = -1;
    for (final s in row.segments) {
      if (s.sourceStart < row.sourceStart || s.sourceEnd > row.sourceEnd) {
        fail_('segment-outside-row', '$label: row ${row.index} segment '
            '${s.sourceStart}..${s.sourceEnd} outside ${row.sourceStart}..${row.sourceEnd}');
      }
      if (s.sourceStart < last) {
        fail_('segment-backwards', '$label: row ${row.index} segment '
            '${s.sourceStart} after $last');
      }
      last = s.sourceEnd;
    }
    if (row.sourceStart > row.sourceEnd) {
      fail_('row-inverted', '$label: row ${row.index} ${row.sourceStart}..${row.sourceEnd}');
    }
  }
  // 4. Every legal offset round-trips through the display and its anchors.
  for (var o = 0; o <= src.length; o++) {
    if (!doc.isLegal(o)) continue;
    final anchors = doc.anchorsAt(o);
    if (!anchors.contains(o)) {
      fail_('anchor-missing-self', '$label: $o not in $anchors');
    }
    for (final a in anchors) {
      if (!doc.isLegal(a)) fail_('anchor-illegal', '$label: $o -> illegal $a');
    }
    final row = doc.rowAt(o);
    final pos = doc.displayOf(o);
    if (pos.row != row.index) {
      fail_('rowAt-displayOf', '$label: $o rowAt ${row.index} displayOf ${pos.row}');
    }
    if (pos.offset < 0 || pos.offset > row.text.length) {
      fail_('display-offset-range', '$label: $o offset ${pos.offset} of ${row.text.length}');
    }
    final back = row.sourceForDisplay(pos.offset);
    if (!doc.anchorsAt(back).contains(o) && back != o) {
      fail_('display-roundtrip', '$label: $o -> row ${row.index} display '
          '${pos.offset} -> $back (anchors ${doc.anchorsAt(back)})');
    }
    if (doc.legalize(o) != o) {
      fail_('legalize-fixed-point', '$label: legal $o legalizes to ${doc.legalize(o)}');
    }
  }
  // 5. legalize of any offset is legal.
  for (var o = -1; o <= src.length + 1; o++) {
    final l = doc.legalize(o);
    if (src.isNotEmpty && !doc.isLegal(l) && l != src.length) {
      fail_('legalize-illegal', '$label: legalize($o) = $l is illegal');
    }
  }
  // 5b. Legal offsets sharing a display position must share an anchor set:
  //     otherwise the caret paints in one place with two unrelated contexts.
  final byDisplay = <(int, int), List<int>>{};
  for (var o = 0; o <= src.length; o++) {
    if (!doc.isLegal(o)) continue;
    final d = doc.displayOf(o);
    (byDisplay[(d.row, d.offset)] ??= []).add(o);
  }
  for (final entry in byDisplay.entries) {
    if (entry.value.length < 2) continue;
    final anchors = doc.anchorsAt(entry.value.first);
    for (final o in entry.value) {
      if (!anchors.contains(o)) {
        fail_('display-shared-not-anchored', '$label: display ${entry.key} '
            'held by ${entry.value} but anchors of ${entry.value.first} '
            'are $anchors');
        break;
      }
    }
  }
  // 6. Hidden bytes hold no legal caret strictly inside.
  for (final h in doc.hiddenIntervals) {
    for (var o = h.$1 + 1; o < h.$2; o++) {
      if (doc.isLegal(o)) {
        fail_('legal-inside-hidden', '$label: $o inside ${h.$1}..${h.$2}');
      }
    }
  }
}

void checkTraversal(String label, FlarkEditor e) {
  final src = e.source;
  // 7. Forward grapheme movement visits strictly increasing legal offsets and
  //    terminates at the end; backward returns to the start.
  e.apply(SetSelection.caret(e.document.legalize(0)));
  final visited = <int>[e.selection.extent];
  for (var step = 0; step <= src.length + 4; step++) {
    if (!e.apply(const MoveCaret(MoveDirection.forward))) break;
    final at = e.selection.extent;
    if (at <= visited.last) {
      fail_('forward-not-increasing', '$label: $at after ${visited.last}');
      break;
    }
    visited.add(at);
  }
  if (visited.last != src.length && e.document.isLegal(src.length)) {
    fail_('forward-stops-early', '$label: stopped at ${visited.last} of ${src.length}');
  }
  final backward = <int>[e.selection.extent];
  for (var step = 0; step <= src.length + 4; step++) {
    if (!e.apply(const MoveCaret(MoveDirection.backward))) break;
    final at = e.selection.extent;
    if (at >= backward.last) {
      fail_('backward-not-decreasing', '$label: $at after ${backward.last}');
      break;
    }
    backward.add(at);
  }
  if (backward.last != visited.first) {
    fail_('backward-stops-early', '$label: ${backward.last} vs ${visited.first}');
  }
}

void checkMovement(String label, FlarkEditor e) {
  final src = e.source;
  // 13. Word movement terminates and never moves backwards.
  for (final forward in [true, false]) {
    e.apply(SetSelection.caret(
        e.document.legalize(forward ? 0 : src.length)));
    var last = e.selection.extent;
    for (var i = 0; i <= src.length + 4; i++) {
      if (!e.apply(MoveCaret(
          forward ? MoveDirection.forward : MoveDirection.backward,
          unit: MoveUnit.word))) {
        break;
      }
      final at = e.selection.extent;
      if (forward ? at <= last : at >= last) {
        fail_('word-move-stalls', '$label ${forward ? "fwd" : "back"}: '
            '$at after $last');
        break;
      }
      last = at;
      if (i == src.length + 4) {
        fail_('word-move-unbounded', '$label ${forward ? "fwd" : "back"}');
      }
    }
  }
  // 14. Home then End then Home returns to the same offset, and both stay on
  //     the caret's own row.
  for (var o = 0; o <= src.length; o++) {
    if (!e.document.isLegal(o)) continue;
    e.apply(SetSelection.caret(o));
    final row = e.document.displayOf(o).row;
    e.apply(const MoveCaret(MoveDirection.backward, unit: MoveUnit.line));
    final home = e.selection.extent;
    if (e.document.displayOf(home).row != row) {
      fail_('home-leaves-row', '$label from $o row $row -> $home row '
          '${e.document.displayOf(home).row}');
    }
    e.apply(const MoveCaret(MoveDirection.forward, unit: MoveUnit.line));
    final end = e.selection.extent;
    if (e.document.displayOf(end).row != row) {
      fail_('end-leaves-row', '$label from $o row $row -> $end row '
          '${e.document.displayOf(end).row}');
    }
    e.apply(const MoveCaret(MoveDirection.backward, unit: MoveUnit.line));
    if (e.selection.extent != home) {
      fail_('home-not-stable', '$label from $o: $home then ${e.selection.extent}');
    }
  }
  // 15. Vertical movement visits every row once and reaches the last.
  e.apply(SetSelection.caret(e.document.legalize(0)));
  final rows = <int>[e.document.displayOf(e.selection.extent).row];
  for (var i = 0; i < e.projection.rows.length + 4; i++) {
    if (!e.apply(const MoveCaret(MoveDirection.forward, unit: MoveUnit.row))) {
      break;
    }
    final row = e.document.displayOf(e.selection.extent).row;
    if (row <= rows.last) {
      fail_('down-not-advancing', '$label: row $row after ${rows.last}');
      break;
    }
    rows.add(row);
  }
  if (rows.last != e.projection.rows.length - 1) {
    fail_('down-stops-early', '$label: reached row ${rows.last} of '
        '${e.projection.rows.length - 1}');
  }
}

void checkErasure(String label, FlarkEditor e) {
  final src = e.source;
  if (src.isEmpty) return;
  // 17. Backspace from the end, and Delete from the start, each empty the
  //     document. A position that refuses forever strands the caret.
  for (final backward in [true, false]) {
    final editor = FlarkEditor(createBackend(),
        text: src, caret: backward ? src.length : 0);
    // Every counted step strictly shrinks the source, so the loop terminates
    // on its own; the bound only stops a future refusal from hanging the suite.
    var steps = 0;
    while (editor.source.isNotEmpty && steps <= src.length + 2) {
      final before = editor.source;
      // The user keeps pressing at the same end of the document.
      editor.apply(SetSelection.caret(backward ? editor.source.length : 0));
      if (!editor.apply(
          backward ? const DeleteBackward() : const DeleteForward())) {
        // Forward delete legitimately stops when nothing follows the caret.
        // Forward delete legitimately stops at the last caret in the document.
        final last = [
          for (var o = 0; o <= editor.source.length; o++)
            if (editor.document.isLegal(o)) o,
        ].lastOrNull;
        final row = editor.document.rowAt(editor.selection.extent);
        // An unclosed fence's block range is only its opening line, so a first
        // row that is one has nothing a join can safely delete through.
        final openFence = backward &&
            row.index == 0 &&
            row.fenced &&
            row.text.isEmpty;
        // A first row whose whole content is hidden markup — `[](/url)`, a
        // link with no text — displays nothing and has no earlier row to join
        // onto. RemoveLink is the command for it.
        final hiddenLeaf =
            backward && row.index == 0 && row.block >= 0 && row.text.isEmpty;
        // Forward delete has nothing to take from a row that displays nothing,
        // and neither direction may lift a pipe or a delimiter row, so a
        // document ending in a table stops here (see the review note).
        final previous = row.index > 0
            ? editor.projection.rows[row.index - 1]
            : null;
        final atEnd = (!backward &&
                (editor.selection.extent == last || row.text.isEmpty)) ||
            (backward && previous?.kind == RowKind.tableCell) ||
            openFence ||
            hiddenLeaf;
        if (!atEnd) {
          fail_('erase-refused', '$label ${backward ? "backspace" : "delete"}: '
              'stuck at ${jsonEncode(editor.source)} '
              'caret ${editor.selection.extent}');
        }
        break;
      }
      if (editor.source.length >= before.length) {
        fail_('erase-grew', '$label ${backward ? "backspace" : "delete"}: '
            '${jsonEncode(before)} -> ${jsonEncode(editor.source)}');
        break;
      }
      steps++;
    }
    if (editor.source.isNotEmpty && steps > src.length + 2) {
      fail_('erase-unbounded', '$label ${backward ? "backspace" : "delete"}: '
          '${jsonEncode(editor.source)}');
    }
  }
}

void checkExtension(String label, FlarkEditor e) {
  final src = e.source;
  // 18. Extending a selection one grapheme and back leaves it collapsed where
  //     it started.
  for (var o = 0; o <= src.length; o++) {
    if (!e.document.isLegal(o)) continue;
    e.apply(SetSelection.caret(o));
    if (!e.apply(const MoveCaret(MoveDirection.forward, extend: true))) continue;
    if (e.selection.base != o) {
      fail_('extend-moved-base', '$label from $o: ${e.selection}');
    }
    e.apply(const MoveCaret(MoveDirection.backward, extend: true));
    // Several legal offsets share one display position; coming back to the
    // same place, with a different typing context, is the model. Coming back
    // to a different place is not.
    final back = e.document.displayOf(e.selection.extent);
    final from = e.document.displayOf(o);
    if (back.row != from.row || back.offset != from.offset) {
      fail_('extend-not-symmetric', '$label from $o (${from.row},'
          '${from.offset}): ${e.selection} at (${back.row},${back.offset})');
    }
  }
}

void checkVisibleText(String label, FlarkEditor e) {
  // 19. Visible text never reproduces hidden bytes.
  final src = e.source;
  for (final h in e.document.hiddenIntervals) {
    if (h.$2 - h.$1 < 2) continue;
    final marker = src.substring(h.$1, h.$2);
    if (marker.trim().isEmpty) continue;
    final visible = e.document.visibleText(0, src.length);
    if (!visible.contains(marker)) continue;
    // Only report when the marker cannot come from ordinary text elsewhere.
    if (src.split(marker).length == 2) {
      fail_('visible-shows-hidden', '$label: ${jsonEncode(marker)} in '
          '${jsonEncode(visible)}');
    }
  }
}

void checkSourceMode(String label, FlarkEditor e) {
  // 16. Source mode exposes the raw source, every offset is legal, and
  //     returning restores a legal caret without changing the source.
  final before = e.source;
  e.setSourceMode(true);
  if (e.source != before) {
    fail_('source-mode-changed-source', '$label: ${jsonEncode(e.source)}');
  }
  var boundary = 0;
  for (final grapheme in before.characters) {
    if (!e.apply(SetSelection.caret(boundary)) && e.selection.extent != boundary) {
      fail_('source-mode-illegal', '$label: $boundary refused');
      break;
    }
    boundary += grapheme.length;
  }
  e.apply(SetSelection.caret(before.length));
  e.apply(const InsertText('Z'));
  if (e.source != '${before}Z') {
    fail_('source-mode-typing', '$label: ${jsonEncode(e.source)}');
  }
  e.apply(const Undo());
  e.setSourceMode(false);
  if (e.source != before) {
    fail_('source-mode-return', '$label: ${jsonEncode(e.source)}');
  }
  if (!e.document.isLegal(e.selection.extent)) {
    fail_('source-mode-caret', '$label: ${e.selection.extent} illegal');
  }
}

void checkCommands(String label, FlarkEditor e) {
  final src = e.source;
  // 8. Undo restores source and selection exactly; Redo returns.
  for (var o = 0; o <= src.length; o += 1 + src.length ~/ 8) {
    final editor = FlarkEditor(createBackend(), text: src, caret: o);
    final before = (editor.source, editor.selection);
    if (!editor.apply(const InsertText('Z'))) continue;
    final typed = (editor.source, editor.selection);
    if (!editor.apply(const Undo())) {
      fail_('undo-refused', '$label at $o');
      continue;
    }
    if (editor.source != before.$1) {
      fail_('undo-source', '$label at $o: ${jsonEncode(editor.source)} '
          'vs ${jsonEncode(before.$1)}');
    }
    if (editor.selection != before.$2) {
      fail_('undo-selection', '$label at $o: ${editor.selection} vs ${before.$2}');
    }
    if (!editor.apply(const Redo()) || editor.source != typed.$1) {
      fail_('redo-source', '$label at $o: ${jsonEncode(editor.source)} '
          'vs ${jsonEncode(typed.$1)}');
    }
  }
  // 9. Typing at a collapsed caret must insert exactly one character, and the
  //    character must be reachable at the resulting caret.
  for (var o = 0; o <= src.length; o += 1 + src.length ~/ 8) {
    final editor = FlarkEditor(createBackend(), text: src, caret: o);
    final at = editor.selection.extent;
    if (!editor.apply(const InsertText('Z'))) continue;
    final grew = editor.source.length - src.length;
    if (grew < 1) {
      fail_('typing-shrank', '$label at $at: ${jsonEncode(src)} -> '
          '${jsonEncode(editor.source)}');
    }
    if (!editor.source.contains('Z')) {
      fail_('typing-lost', '$label at $at -> ${jsonEncode(editor.source)}');
    }
  }
  // 9b. Every legal caret accepts some edit. A caret the user can reach and
  //     see, where nothing at all can be typed, is a dead position.
  for (var o = 0; o <= src.length; o++) {
    if (!e.document.isLegal(o)) continue;
    final editor = FlarkEditor(createBackend(), text: src, caret: o);
    if (editor.selection.extent != o) continue;
    final accepts = [
      const InsertText('Z'),
      const Newline(),
      const DeleteBackward(),
      const DeleteForward(),
    ].any((c) => FlarkEditor(createBackend(), text: src, caret: o).apply(c));
    if (!accepts) {
      fail_('caret-accepts-nothing', '$label at $o: '
          'row ${jsonEncode(editor.document.rowAt(o).text)}');
    }
  }
  // 10. Select all then delete empties the document, unless Select All is
  //     scoped to a code fence.
  final all = FlarkEditor(createBackend(), text: src, caret: 0);
  all.apply(const SelectAll());
  final whole = all.selection.start == 0 && all.selection.end == src.length;
  if (whole && src.isNotEmpty &&
      (!all.apply(const DeleteBackward()) || all.source.isNotEmpty)) {
    fail_('select-all-delete', '$label -> ${jsonEncode(all.source)}');
  }
  // 11. Pasting the selected text back over the selection is identity.
  final paste = FlarkEditor(createBackend(), text: src, caret: 0);
  paste.apply(const SelectAll());
  if (paste.selection.start == 0 &&
      paste.selection.end == src.length &&
      src.isNotEmpty &&
      paste.apply(Paste(src)) &&
      paste.source != src) {
    fail_('paste-identity', '$label -> ${jsonEncode(paste.source)}');
  }
  // 12. Every command at every legal caret either refuses without a change or
  //     changes the source, keeps every invariant, and undoes exactly.
  const commands = <FlarkCommand>[
    DeleteBackward(),
    DeleteForward(),
    Newline(),
    Newline(paragraph: true),
    Indent(),
    Outdent(),
    ToggleTask(),
    SetHeadingLevel(2),
    SetHeadingLevel(0),
    ToggleStyle(Style.strong),
    ToggleStyle(Style.code),
    InsertText(' '),
    InsertText('*'),
    InsertText('\n'),
  ];
  for (var o = 0; o <= src.length; o += 1 + src.length ~/ 6) {
    for (final command in commands) {
      final editor = FlarkEditor(createBackend(), text: src, caret: o);
      final at = editor.selection.extent;
      final before = editor.source, selection = editor.selection;
      final applied = editor.apply(command);
      if (!applied) {
        if (editor.source != before || editor.selection != selection) {
          fail_('refused-but-changed', '$label $command at $at: '
              '${jsonEncode(editor.source)} ${editor.selection}');
        }
        continue;
      }
      if (editor.source == before) continue; // selection-only effects
      checkStatic('$label after $command at $at', editor);
      if (!editor.apply(const Undo())) {
        fail_('undo-refused-command', '$label $command at $at');
      } else if (editor.source != before) {
        fail_('undo-source-command', '$label $command at $at: '
            '${jsonEncode(editor.source)} vs ${jsonEncode(before)}');
      } else if (editor.selection != selection) {
        fail_('undo-selection-command', '$label $command at $at: '
            '${editor.selection} vs $selection');
      }
    }
  }
}

late FlarkParseBackend Function() createBackend;

void main() {
  final backend = createParseBackend();
  createBackend = () => backend;
  final corpus = <String>[];
  for (final name in ['common_mark_tests.json', 'gfm_tests.json']) {
    final f = File('../../test/fixtures/commonmark/upstream/$name');
    if (!f.existsSync()) continue;
    for (final c in (jsonDecode(f.readAsStringSync()) as List)) {
      corpus.add((c as Map)['markdown'] as String);
    }
  }

  test('deep static invariants across the corpora', () {
    final variants = <String>[];
    for (final raw in corpus) {
      final lf = raw.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
      if (lf.length > 400) continue;
      variants.add(lf);
      variants.add(lf.replaceAll('\n', '\r\n'));
      variants.add(lf.split('\n').map((l) => '> $l').join('\n'));
      variants.add(lf.split('\n').map((l) => '  $l').join('\n'));
    }
    for (final source in variants) {
      final e = FlarkEditor(backend, text: source, caret: 0);
      if (e.sourceMode) continue; // the live limits forced raw source
      checkStatic(jsonEncode(source), e);
      checkTraversal(jsonEncode(source), e);
      checkCommands(jsonEncode(source), e);
      checkMovement(jsonEncode(source), FlarkEditor(backend, text: source, caret: 0));
      checkSourceMode(jsonEncode(source), FlarkEditor(backend, text: source, caret: 0));
      checkErasure(jsonEncode(source), e);
      checkExtension(jsonEncode(source), FlarkEditor(backend, text: source, caret: 0));
      checkVisibleText(jsonEncode(source), e);
    }
    // ignore: avoid_print
    for (final entry in failures.entries) {
      // ignore: avoid_print
      print('### ${entry.key} (${entry.value.length} shown)');
      for (final d in entry.value) {
        // ignore: avoid_print
        print('    $d');
      }
    }
    expect(failures.keys, isEmpty);
  }, timeout: const Timeout(Duration(minutes: 10)));
}
