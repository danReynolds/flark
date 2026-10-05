/// Structural oracles the generated matrix applies after every command that
/// changes the source. The edit profile keeps an edit local: source outside
/// the affected semantic closure stays exact, joins and lifts keep the blocks
/// around them, Return keeps the new line in the containers of the line it
/// split, and no paint exposes markup the projection hid. These checks read
/// the projections and render models before and after; they never recognize
/// Markdown in Dart.
///
/// - (a) every row the edit did not touch keeps its kind and the kinds of
///   its containers;
/// - (b) no hidden non-whitespace source is painted;
/// - (c) a typed letter or digit shows where the caret was, the caret
///   follows it, and nothing else shown changes;
/// - (d) Return keeps the new line in the containers of the line it split;
/// - (e) Return shows one line break (two for a paragraph break) and changes
///   nothing else shown;
/// - (f) a heading level applies to the caret's row in its containers;
/// - rows paint in source order.
///
/// Markdown legitimately reads some edits beyond the edited rows. Each
/// exemption names the Markdown or profile rule it encodes and applies only
/// to the commands that rule covers:
///
/// - literal lines: text with a line break from paste, range replacement or
///   raw input takes Markdown's meaning with the blocks around it;
/// - inline pairing: an edit of characters can pair the inline delimiters of
///   the rows it touches anew (flanking, backtick strings, brackets);
/// - references: editing a definition or a reference's label re-resolves
///   references across the document;
/// - openers: an edit of characters can complete a block opener (a fence,
///   an HTML block start, a list or quote marker, an ATX heading, a rule, a
///   table delimiter row), which Markdown reads on into the lines after it,
///   and
///   HTML blocks are literal source whose end conditions are their own
///   characters;
/// - removed containers: blocks inside a container whose marker the edit
///   removed lose that container with it;
/// - bare markers: the profile presents a bare marker that starts its own
///   list as text, and one between other items as an empty item;
/// - empty rows on joins: a row that shows no text joins wherever its
///   source lands, so Backspace can always erase a document;
/// - whole document: replacing the whole-document selection replaces it all.
///
/// Undo and Redo restore states the history walk already checks.
library;

import 'dart:convert';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:test/test.dart';

/// What an applied command changed in the source: old `start..oldEnd` became
/// new `start..newEnd`. The source both versions share at the start and at
/// the end locates it. When reading the shared start first and reading the
/// shared end first disagree (a line break inserted beside another), the
/// range covers both readings, so every offset outside it maps the same way
/// under either.
final class SourceEdit {
  factory SourceEdit(String a, String b) {
    final shorter = min(a.length, b.length);
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

    final p1 = prefix(shorter), q1 = suffix(shorter - p1);
    final q2 = suffix(shorter), p2 = prefix(shorter - q2);
    return SourceEdit._(p2, a.length - q1, b.length - q1);
  }

  const SourceEdit._(this.start, this.oldEnd, this.newEnd);

  final int start, oldEnd, newEnd;
  int get delta => newEnd - oldEnd;

  /// An old offset in the new source, or -1 inside the edit.
  int forward(int offset) => offset < start
      ? offset
      : offset >= oldEnd
      ? offset + delta
      : -1;

  /// A new offset in the old source, or -1 in what the edit inserted.
  int back(int offset) => offset < start
      ? offset
      : offset >= newEnd
      ? offset - delta
      : -1;
}

/// The lines of [doc] an edit of `start..end` (in [doc]'s source) touched,
/// first to last; empty (last < first) when the edit only inserted whole
/// lines between two. [otherEnd] is where the edit ends in the other
/// version's source [other]: the line at [end] keeps its own start, and is
/// untouched, only when the edit ends at a line start in both versions.
(int, int) touchedLines(
  FlarkDocument doc,
  int start,
  int end,
  String other,
  int otherEnd,
) {
  bool lineStart(String s, int o) => o == 0 || s.codeUnitAt(o - 1) == 0x0A;
  final m = doc.model;
  final first = m.lineOfUtf16(start);
  var last = m.lineOfUtf16(end);
  if (lineStart(doc.source, end) && lineStart(other, otherEnd)) last--;
  return (first, last);
}

int _lastLine(ProjectedRow row) => row.firstLine + row.lineCount - 1;

/// The kinds of [row]'s containers, outermost first.
String shellKinds(ProjectedRow row) =>
    row.shells.map((shell) => shell.kind.name).join('/');

bool _isSpace(int unit) =>
    unit == 0x20 || unit == 0x09 || unit == 0x0A || unit == 0x0D;

/// Text a command inserts as typed or pasted input, or null.
String? insertedText(FlarkCommand command) => switch (command) {
  InsertText(:final text) => text,
  Paste(:final text) => text,
  ReplaceRange(:final text) => text,
  _ => null,
};

/// A collapsed Backspace at a row's start or Delete at its end: a join or a
/// lift, the edits the profile's structural rules govern most closely.
bool isJoin(FlarkDocument old, FlarkCommand command) {
  if (!old.selection.isCollapsed) return false;
  final at = old.displayOf(old.selection.extent);
  return switch (command) {
    DeleteBackward(word: false) => at.offset == 0,
    DeleteForward(word: false) =>
      at.offset == old.projection.rows[at.row].text.length,
    _ => false,
  };
}

/// Whether [row] is the profile's presentation of a bare empty heading or
/// list marker as paragraph text.
bool isBarePrefix(FlarkDocument doc, ProjectedRow row) =>
    row.kind == RowKind.paragraph &&
    row.block >= 0 &&
    (doc.model.blockKind(row.block) == BlockKind.item ||
        doc.model.blockKind(row.block) == BlockKind.heading);

final _letterOrDigit = RegExp(r'^[\p{L}\p{N}]$', unicode: true);

/// Link reference definitions as their source, and the normalized labels of
/// footnote definitions: what references elsewhere resolve to.
Set<String> definitionsOf(FlarkDocument doc) {
  final m = doc.model, s = doc.source;
  String label(int a, int b) =>
      s.substring(a, b).toLowerCase().replaceAll(RegExp(r'\s+'), ' ').trim();
  return {
    for (final d in m.definitions)
      'link ${s.substring(d.startUtf16, d.endUtf16)}',
    for (final b in m.blocks)
      if (b.kind == BlockKind.footnoteDefinition)
        'note ${label(m.footnoteLabelStart(b.index), m.footnoteLabelEnd(b.index))}',
  };
}

/// Whether [offset] of [doc]'s source lies in the hidden delimiters of an
/// inline run: emphasis, link and code syntax, escapes, break markers.
bool inInlineDelimiter(FlarkDocument doc, int offset) {
  for (final (a, b) in doc.hiddenIntervals) {
    if (a > offset) break;
    if (offset < b) return true;
  }
  return false;
}

/// Whether [offset] lies in a reference link or image (its text or its
/// syntax) or in a footnote reference.
bool inReference(FlarkDocument doc, int offset) {
  final m = doc.model;
  for (var r = 0; r < m.runCount; r++) {
    final kind = m.runKind(r);
    final reference =
        (kind == RunKind.link || kind == RunKind.image) &&
            m.runFlags(r) & 1 != 0 ||
        kind == RunKind.footnoteRef;
    if (reference && m.runStart(r) <= offset && offset <= m.runEnd(r)) {
      return true;
    }
  }
  return false;
}

/// Whether [offset] lies in a raw inline HTML run of [doc].
bool _inRawHtml(FlarkDocument doc, int offset) {
  final m = doc.model;
  for (var r = 0; r < m.runCount; r++) {
    if (m.runKind(r) == RunKind.htmlInline &&
        m.runStart(r) <= offset &&
        offset <= m.runEnd(r)) {
      return true;
    }
  }
  return false;
}

/// The text [projection] paints, row after row with a line break between,
/// as one unit per UTF-16 code unit with the source offset it shows, or -1
/// for a line break or a virtual space. A line break displays as one,
/// including the space a code span shows for one.
List<(String, int)> paintedUnits(Projection projection) {
  final out = <(String, int)>[];
  for (final row in projection.rows) {
    if (row.index > 0) out.add(('\n', -1));
    for (final s in row.segments) {
      for (var d = s.displayStart; d < s.displayEnd; d++) {
        if (s.lineBreak) {
          out.add(('\n', -1));
          continue;
        }
        final source = s.sourceEnd <= s.sourceStart
            ? -1
            : s.exact
            ? s.sourceStart + d - s.displayStart
            : s.sourceStart;
        out.add((row.text[d], source));
      }
    }
  }
  return out;
}

/// The checks after an applied command. [before] is the snapshot the command
/// started from.
void checkStructure(
  FlarkEditorSnapshot before,
  FlarkCommand command,
  FlarkEditor editor,
  String label,
) {
  if (before is! FlarkLiveSnapshot || editor.sourceMode) return;
  final old = before.document, next = editor.document;
  // History restores a state the sequence already reached, which the
  // history walk checks; it is not an edit with a locality of its own.
  if (old.source == next.source || command is Undo || command is Redo) return;
  // Replacing or deleting the whole-document selection replaces the whole
  // document, as the profile says: there is nothing outside it to keep.
  final sel = old.selection;
  final whole =
      !sel.isCollapsed && sel.start == 0 && sel.end == old.source.length;
  if (whole &&
      (command is InsertText ||
          command is Paste ||
          command is DeleteBackward ||
          command is DeleteForward ||
          command is Newline)) {
    return;
  }
  if (command case ReplaceRange(
    :final start,
    :final end,
  ) when min(start, end) <= 0 && max(start, end) >= old.source.length) {
    return;
  }
  _Check(old, next, command, label).run();
}

final class _Check {
  _Check(this.old, this.next, this.command, this.label)
    : edit = SourceEdit(old.source, next.source),
      inserted = insertedText(command),
      join = isJoin(old, command) {
    oldLines = touchedLines(
      old,
      edit.start,
      edit.oldEnd,
      next.source,
      edit.newEnd,
    );
    newLines = touchedLines(
      next,
      edit.start,
      edit.newEnd,
      old.source,
      edit.oldEnd,
    );
  }

  final FlarkDocument old, next;
  final FlarkCommand command;
  final String label;
  final SourceEdit edit;
  final String? inserted;
  final bool join;
  late final (int, int) oldLines, newLines;

  String get transition =>
      '${jsonEncode(old.source)} -> ${jsonEncode(next.source)}';

  /// Inserted text with a line break is a literal source operation: its new
  /// lines take Markdown's meaning with whatever is around them (the profile
  /// keeps paste, range replacement and raw input literal), so the blocks it
  /// touches and those after it may be read anew, and so may the block just
  /// before it, which a new line can underline or make a table's header.
  late final bool literalLines =
      inserted != null &&
      (inserted!.contains('\n') || inserted!.contains('\r'));

  /// An edit of characters within a row: typing, pasting or replacing one
  /// line of text, or deleting a grapheme, word or selection.
  late final bool editsCharacters =
      (inserted != null && !literalLines) ||
      (!join && (command is DeleteBackward || command is DeleteForward));

  bool oldRowTouched(ProjectedRow row) =>
      !(_lastLine(row) < oldLines.$1 || row.firstLine > oldLines.$2);

  bool newRowTouched(ProjectedRow row) =>
      !(_lastLine(row) < newLines.$1 || row.firstLine > newLines.$2);

  bool newLineTouched(int line) => line >= newLines.$1 && line <= newLines.$2;

  /// Whether the rows before literal lines' reach hold [row].
  bool literalReach(ProjectedRow row) =>
      literalLines && _lastLine(row) >= oldLines.$1 - 1;

  late final bool definitionsChanged = () {
    final a = definitionsOf(old), b = definitionsOf(next);
    return a.length != b.length || !a.containsAll(b);
  }();

  /// References: an edit of characters in a link reference definition, or
  /// one that made or broke a definition, or literal lines that did either.
  /// A definition row shows its source and is edited as text; Markdown reads
  /// one only where a paragraph could start, and resolves references by
  /// label across the document, so the definitions after it and the
  /// references elsewhere (with the delimiters they pair) may be read anew.
  late final bool rereadsDefinitions =
      definitionsChanged &&
      (literalLines ||
          editsCharacters &&
              (old.projection.rows.any(
                    (row) =>
                        row.kind == RowKind.definition && oldRowTouched(row),
                  ) ||
                  next.projection.rows.any(
                    (row) =>
                        row.kind == RowKind.definition && newRowTouched(row),
                  )));

  /// Whether old block [block] of [kind] now starts where new block [now]
  /// does: the same block, moved by the edit.
  bool _existed(int kind, int now) => old.model.blocks.any(
    (b) =>
        b.kind == kind &&
        edit.forward(b.startUtf16) == next.model.blockStart(now),
  );

  /// Openers: blocks that an edit of characters opened on a line it touched:
  /// a typed fence or HTML block start, a typed marker and its space, a
  /// digit completing `1.`, a `#` or a rule a deletion leaves at a line's
  /// start, a typed delimiter row that makes the line above a table's
  /// header. Markdown reads a fence on to its closing fence, an HTML block
  /// to its end condition, the indented blocks after an item and the lazy
  /// lines after a quote as their content, and a new block interrupts the
  /// paragraph it was typed into (which can orphan a setext underline). The
  /// profile lets typed input complete source-authored syntax ("Completing
  /// source-authored delimiters may atomically turn literal text into a
  /// rendered construct") and completes only a typed bare fence opener;
  /// other edits keep that literal meaning.
  late final Set<int> openedBlocks = {
    if (editsCharacters)
      for (final b in next.model.blocks)
        if (_opens(b.kind, b.flags) &&
            b.lineCount > 0 &&
            (newLineTouched(b.firstLine) ||
                b.kind == BlockKind.table &&
                    b.firstLine <= newLines.$2 &&
                    b.firstLine + b.lineCount - 1 >= newLines.$1) &&
            !_existed(b.kind, b.index))
          b.index,
  };

  /// Blocks whose own syntax opens them on their first line. Indented code
  /// is whitespace, which the profile has typing paint and advance past, and
  /// a setext heading's syntax is the underline below its first line.
  static bool _opens(int kind, int flags) =>
      kind == BlockKind.codeBlock && flags & 1 != 0 ||
      kind == BlockKind.htmlBlock ||
      kind == BlockKind.item ||
      kind == BlockKind.blockQuote ||
      kind == BlockKind.footnoteDefinition ||
      kind == BlockKind.heading && flags & 1 == 0 ||
      kind == BlockKind.thematicBreak ||
      kind == BlockKind.table;

  /// An HTML block is literal source whose start and end conditions are its
  /// own characters (`<div`, `-->`, `</script>`, a blank line), so an edit
  /// of characters in one, or a line break Return puts in it, may end it
  /// elsewhere or start another.
  late final bool editsHtml =
      (editsCharacters || command is Newline) &&
      old.projection.rows.any(
        (row) => row.kind == RowKind.htmlBlock && oldRowTouched(row),
      );

  /// The old row a source offset lies on, hidden offsets included: the row
  /// whose lines hold it, the one whose source spans it among several (table
  /// cells).
  ProjectedRow oldRowOf(int offset) {
    final line = old.model.lineOfUtf16(offset);
    ProjectedRow? found;
    for (final row in old.projection.rows) {
      if (row.firstLine > line || _lastLine(row) < line) continue;
      if (offset >= row.sourceStart && offset <= row.sourceEnd) return row;
      found ??= row;
    }
    return found ?? old.rowAt(offset);
  }

  bool inOpened(ProjectedRow now) =>
      openedBlocks.contains(now.block) ||
      openedBlocks.contains(now.tableBlock) ||
      now.shells.any((shell) => openedBlocks.contains(shell.block));

  /// Openers: whether [row]'s containers included one the edit destroyed
  /// by opening a block on its line (`- --r` less its `r` is a rule, which
  /// takes the item's line): the blocks it held are read without it.
  bool lostDestroyedContainer(ProjectedRow row) =>
      openedBlocks.isNotEmpty &&
      row.shells.any(
        (shell) =>
            shell.kind != ShellKind.list &&
            !_survives(old.model.blockKind(shell.block), shell.block),
      );

  bool _survives(int kind, int block) {
    final at = edit.forward(old.model.blockStart(block));
    return at >= 0 &&
        next.model.blocks.any((b) => b.kind == kind && b.startUtf16 == at);
  }

  /// Removed containers: whether [row]'s containers included one whose
  /// marker the edit removed or rewrote (a lifted item or quote marker). The
  /// blocks that container held lose it with it.
  bool lostRemovedContainer(ProjectedRow row) => row.shells.any((shell) {
    if (shell.kind == ShellKind.list) return false;
    final at = old.model.blockStart(shell.block);
    return at >= edit.start && at < max(edit.oldEnd, edit.start + 1);
  });

  void run() {
    _checkRowsKept();
    _checkNothingRevealed();
    if (command is Newline) {
      _checkReturnContainers();
      _checkReturnText();
    }
    if (command is SetHeadingLevel) _checkHeadingLevel();
    _checkTypedCharacter();
    _checkRowOrder();
  }

  /// Rows paint in source order: each starts at or after the one before. A
  /// paragraph after link reference definitions in its block starts after
  /// them, though its block starts where the first of them does.
  void _checkRowOrder() {
    final rows = next.projection.rows;
    for (var i = 1; i < rows.length; i++) {
      if (rows[i].sourceStart >= rows[i - 1].sourceStart) continue;
      fail(
        '$label: [order] row $i ${rows[i].kind.name} '
        '${jsonEncode(rows[i].text)} paints before row ${i - 1} '
        '${rows[i - 1].kind.name} ${jsonEncode(rows[i - 1].text)} that it '
        'follows in the source, $transition',
      );
    }
  }

  /// (a) Every row the edit did not touch keeps its kind and the kinds of
  /// its containers. A join that pulled `c` into the item above it
  /// (`- # a\nb\n\n  c`, Delete after `a`) moved a block two rows away.
  /// Text put into an empty list item can take in a block indented for that
  /// item: while the item was empty, Markdown read the block outside it (an
  /// empty item ends at a blank line). The profile keeps that reading once
  /// the item has text rather than refuse the keystroke.
  late final String? _filledItem = () {
    final c = command;
    if (c is! InsertText && c is! Paste && c is! ReplaceRange) return null;
    if (!old.selection.isCollapsed) return null;
    final row = old.caretRow;
    return row.text.isEmpty &&
            row.shells.isNotEmpty &&
            row.shells.last.kind == ShellKind.item
        ? shellKinds(row)
        : null;
  }();

  void _checkRowsKept() {
    for (final row in old.projection.rows) {
      if (row.kind == RowKind.blank || oldRowTouched(row)) continue;
      if (literalReach(row)) continue;
      final after = row.firstLine > oldLines.$2;
      if (after && editsHtml) continue;
      final mapped = after ? row.sourceStart + edit.delta : row.sourceStart;
      final now = _counterpart(row, mapped);
      if (inOpened(now) || lostDestroyedContainer(row)) continue;
      // An opener ends the paragraph it was typed into, whose other lines
      // are read anew, with what follows them.
      if (openedBlocks.isNotEmpty) {
        final was = edit.back(now.sourceStart);
        if (was >= 0 && oldRowTouched(oldRowOf(was))) continue;
      }
      // Openers interrupt the table they are typed in, and a new fence pairs
      // with the next fence line, releasing what a later fence held.
      if (openedBlocks.isNotEmpty &&
          (row.kind == RowKind.tableCell &&
                  old.projection.rows.any(
                    (r) => oldRowTouched(r) && r.tableBlock == row.tableBlock,
                  ) ||
              row.fenced &&
                  openedBlocks.any(
                    (b) =>
                        next.model.blockKind(b) == BlockKind.codeBlock &&
                        next.model.blockFlags(b) & 1 != 0 &&
                        next.model.blockStart(b) < mapped,
                  ))) {
        continue;
      }
      // The profile presents a bare marker that starts its own list as text,
      // and one between other items as an empty item.
      if (isBarePrefix(old, row) &&
          (now.kind == RowKind.blank || isBarePrefix(next, now))) {
        continue;
      }
      if (rereadsDefinitions &&
          (row.kind == RowKind.definition || now.kind == RowKind.definition)) {
        continue;
      }
      if (now.kind != row.kind) {
        fail(
          '$label: [kind] row ${row.index} ${row.kind.name} '
          '${jsonEncode(row.text)} became ${now.kind.name} '
          '${jsonEncode(now.text)}, $transition',
        );
      }
      if (join && row.text.isEmpty && !row.fenced) continue;
      if (lostRemovedContainer(row)) continue;
      if (_filledItem case final item?
          when shellKinds(now).startsWith(item) &&
              item.startsWith(shellKinds(row))) {
        continue;
      }
      if (shellKinds(now) != shellKinds(row)) {
        fail(
          '$label: [shells] row ${row.index} ${row.kind.name} '
          '${jsonEncode(row.text)} moved from <${shellKinds(row)}> to '
          '<${shellKinds(now)}>, $transition',
        );
      }
    }
  }

  /// The row of [next] that now holds [row]'s content start [mapped]: one
  /// that starts there, of the same kind first (an empty last row starts
  /// where the row before it ends), else the row the offset shows on.
  ProjectedRow _counterpart(ProjectedRow row, int mapped) {
    ProjectedRow? any;
    for (final now in next.projection.rows) {
      if (now.sourceStart != mapped) continue;
      if (now.kind == row.kind) return now;
      any ??= now;
    }
    return any ?? next.rowAt(mapped);
  }

  /// (b) No non-whitespace source the old projection hid is painted.
  void _checkNothingRevealed() {
    final a = old.source, b = next.source;
    if (join) {
      // Removing an empty line or marker leaves the parser's reading of the
      // lines around it, which may present a bare marker as text. Joining
      // the line break before a fence that displays nothing turns its
      // delimiter line into text, by design: the way to delete one with no
      // body.
      final at = old.displayOf(old.selection.extent);
      final rows = old.projection.rows;
      final joined =
          rows[command is DeleteBackward
              ? at.row
              : (at.row + 1).clamp(0, rows.length - 1)];
      if (joined.text.isEmpty ||
          joined.fenced && joined.contentStarts.every((s) => s < 0)) {
        return;
      }
    }
    final painted = List<bool>.filled(a.length, false);
    for (final row in old.projection.rows) {
      for (final s in row.segments) {
        if (!s.lineBreak) painted.fillRange(s.sourceStart, s.sourceEnd, true);
      }
    }
    for (final row in next.projection.rows) {
      for (final s in row.segments) {
        if (s.lineBreak) continue;
        for (var o = s.sourceStart; o < s.sourceEnd; o++) {
          if (_isSpace(b.codeUnitAt(o))) continue;
          final was = edit.back(o);
          if (was < 0 || painted[was]) continue;
          // The shared start and end can mistake text an edit moved (the
          // setext underline Return keeps with the first line) for text that
          // stayed. Read character by character around the edit, the longer
          // run wins: a reveal must hold under that reading too.
          if (_alignedFrom <= o && o < _alignedTo && _aligned != null) {
            final near = _aligned[o];
            if (near == null || painted[near]) continue;
          }
          // No reading of the edit can tell which of two equal characters
          // beside it stayed (removing the underline `\n---` after `a-`):
          // a row that shows exactly what the row of the hidden one showed
          // revealed nothing.
          if (row.text == oldRowOf(was).text) continue;
          if (_revealExempt(row, was)) continue;
          // The joins' earlier exemption: a painted character must also be
          // new to the text shown, whitespace aside, so a reinterpretation
          // that only respells shown text (an entity left code) does not
          // count.
          if (join && _respellsShownText()) return;
          fail(
            '$label: [reveal${inInlineDelimiter(old, was) ? ':inline' : ''}] '
            'painted hidden source ${jsonEncode(b[o])} at $o (was $was) in row '
            '${row.index} ${jsonEncode(row.text)}, $transition',
          );
        }
      }
    }
  }

  static const _reach = 96;
  late final int _alignedFrom = max(0, edit.start - _reach);
  late final int _alignedTo = min(next.source.length, edit.newEnd + _reach);

  /// New offsets near the edit and the old offsets they match when the
  /// sources there are aligned as a diff aligns them: the longest run the
  /// two share first, then the same on either side of it, so the longer run
  /// wins and runs stay whole. Inserted text has none. Null when the edit is
  /// too large to read this way.
  late final Map<int, int>? _aligned = () {
    final a = old.source, b = next.source;
    final a1 = min(a.length, edit.oldEnd + _reach);
    if ((a1 - _alignedFrom) * (_alignedTo - _alignedFrom) > 1 << 20) {
      return null;
    }
    final out = <int, int>{};
    void match(int alo, int ahi, int blo, int bhi) {
      var best = 0, bestA = alo, bestB = blo;
      var previous = List<int>.filled(bhi - blo + 1, 0);
      for (var i = alo; i < ahi; i++) {
        final current = List<int>.filled(bhi - blo + 1, 0);
        for (var j = blo; j < bhi; j++) {
          if (a.codeUnitAt(i) != b.codeUnitAt(j)) continue;
          final run = current[j - blo + 1] = previous[j - blo] + 1;
          if (run > best) {
            best = run;
            bestA = i - run + 1;
            bestB = j - run + 1;
          }
        }
        previous = current;
      }
      if (best == 0) return;
      for (var k = 0; k < best; k++) {
        out[bestB + k] = bestA + k;
      }
      match(alo, bestA, blo, bestB);
      match(bestA + best, ahi, bestB + best, bhi);
    }

    match(_alignedFrom, a1, _alignedFrom, _alignedTo);
    return out;
  }();

  bool _revealExempt(ProjectedRow row, int was) {
    final from = oldRowOf(was);
    // The profile presents a bare marker that starts its own list as text,
    // and one between other items as an empty item.
    if (isBarePrefix(next, row) &&
        old.model.blocks.any(
          (b) =>
              (b.kind == BlockKind.item || b.kind == BlockKind.heading) &&
              b.startUtf16 <= was &&
              was <
                  b.startUtf16 +
                      max(1, b.kind == BlockKind.heading ? b.attr : 1),
        )) {
      return true;
    }
    // Literal lines.
    if (literalReach(from)) return true;
    // Openers: the opened block's content, the block it was opened in, and
    // what a container it destroyed held.
    if (inOpened(row) ||
        openedBlocks.isNotEmpty && oldRowTouched(from) ||
        lostDestroyedContainer(from)) {
      return true;
    }
    if (editsHtml && was >= edit.start) return true;
    // Inline pairing. Markdown pairs inline delimiters across a paragraph
    // from the characters beside them (emphasis flanking and the rule of
    // three, backtick strings, brackets, autolink and raw HTML precedence),
    // so an edit of characters can pair them anew in the rows it touches.
    // The profile keeps typed delimiters literal and deleted graphemes
    // plain, and no spelling keeps `*(a)*` emphasized once a letter follows.
    if (editsCharacters && inInlineDelimiter(old, was) && oldRowTouched(from)) {
      return true;
    }
    // References.
    if (rereadsDefinitions &&
        (row.kind == RowKind.definition ||
            from.kind == RowKind.definition ||
            inInlineDelimiter(old, was) ||
            oldRowTouched(from))) {
      return true;
    }
    if ((editsCharacters || literalLines) &&
        inReference(old, was) &&
        (definitionsChanged || oldRowTouched(from))) {
      return true;
    }
    // Return can complete a link reference definition (an angle destination
    // closed at the end of the document): the new definition row shows its
    // source, as every definition row does.
    if (definitionsChanged &&
        command is Newline &&
        row.kind == RowKind.definition) {
      return true;
    }
    return false;
  }

  bool _respellsShownText() {
    String shown(Projection projection) => [
      for (final row in projection.rows) row.text.replaceAll(RegExp(r'\s'), ''),
    ].join();
    final was = shown(old.projection), now = shown(next.projection);
    var i = 0;
    for (var j = 0; j < now.length; j++) {
      while (i < was.length && was.codeUnitAt(i) != now.codeUnitAt(j)) {
        i++;
      }
      if (i++ >= was.length) return false;
    }
    return true;
  }

  /// (d) Return keeps the new line in the containers of the line it split:
  /// an item continues as the next item, a quote or footnote with its
  /// prefix. Only Return on an empty container line leaves a container, and
  /// then it leaves only containers (the line's are a prefix of them). Code
  /// continues as code, or Return on its final blank line exits the fence
  /// into the gap in the same containers.
  void _checkReturnContainers() {
    final sel = old.selection;
    final at = old.displayOf(sel.start);
    final row = old.projection.rows[at.row];
    // Return moves between table rows, and a line break in an HTML block is
    // its literal source (see editsHtml).
    if (row.kind == RowKind.tableCell || row.kind == RowKind.htmlBlock) {
      return;
    }
    final caret = next.displayOf(next.selection.extent);
    final now = next.projection.rows[caret.row];
    final was = shellKinds(row), shells = shellKinds(now);
    // Return in a footnote definition opens a line of its indentation alone,
    // which Markdown reads as blank until text is typed on it.
    if (was.contains('footnote') && now.kind == RowKind.blank) return;
    if (sel.isCollapsed &&
        row.text.isEmpty &&
        row.kind != RowKind.heading &&
        row.kind != RowKind.codeBlock) {
      if (was.startsWith(shells)) return;
    } else if (shells == was) {
      return;
    }
    fail(
      '$label: [return] the line Return made is in <$shells>, the '
      '${row.kind.name} row ${row.index} it split in <$was>, $transition',
    );
  }

  /// (e) Return inserts a line break where the caret showed it, two for a
  /// paragraph break, and changes nothing else shown: the text after the
  /// caret is not read as markup (`a - b` split before `- b` must not hide
  /// `- `), and no delimiter is painted. Whitespace beside a line break is
  /// Markdown's to show or strip. Code (auto-indentation, a fence's exit),
  /// tables (Return moves between rows) and an empty container line (which
  /// Return leaves) are other rules.
  void _checkReturnText() {
    final sel = old.selection;
    if (!sel.isCollapsed) return;
    final at = old.displayOf(sel.extent);
    final row = old.projection.rows[at.row];
    if (row.kind == RowKind.codeBlock ||
        row.kind == RowKind.htmlBlock ||
        row.kind == RowKind.tableCell ||
        row.text.isEmpty) {
      return;
    }
    // References (see (b)): a line break in a definition or a label.
    if (definitionsChanged || inReference(old, sel.extent)) return;
    String visible(Projection p) => p.rows.map((r) => r.text).join('\n');
    // Whitespace beside a line break is Markdown's, and a blank line Return
    // adds to keep a block apart (a heading's text would otherwise read on
    // into the next block) shows nothing: compare the lines with text.
    String squeeze(String text) => text
        .replaceAll(RegExp(r'[ \t]*\n[ \t]*'), '\n')
        .replaceAll(RegExp(r'\n{2,}'), '\n');
    var g = 0;
    for (var i = 0; i < at.row; i++) {
      g += old.projection.rows[i].text.length + 1;
    }
    g += at.offset;
    final before = visible(old.projection), after = visible(next.projection);
    final breaks =
        (command as Newline).paragraph &&
            row.kind == RowKind.paragraph &&
            row.shells.isEmpty
        ? '\n\n'
        : '\n';
    final expected = squeeze(before.replaceRange(g, g, breaks));
    if (squeeze(after) != expected) {
      fail(
        '$label: [return-text] Return at row ${at.row} offset ${at.offset} '
        'showed ${jsonEncode(after)}, expected ${jsonEncode(expected)}, '
        '$transition',
      );
    }
  }

  /// (f) A heading level applies to the caret's row in its containers: a
  /// heading of that level, or no heading for level 0. On an empty line the
  /// command inserts the marker as typing does, so a pending style must not
  /// wrap it and the line's containers must keep it.
  void _checkHeadingLevel() {
    final level = (command as SetHeadingLevel).level;
    final was = old.projection.rows[old.displayOf(old.selection.extent).row];
    final now = next.projection.rows[next.displayOf(next.selection.extent).row];
    final heading = now.kind == RowKind.heading && now.headingLevel == level;
    if ((level == 0 ? now.kind != RowKind.heading : heading) &&
        shellKinds(now) == shellKinds(was)) {
      return;
    }
    fail(
      '$label: [heading-level] SetHeadingLevel($level) left the caret in a '
      '${now.kind.name}${now.headingLevel > 0 ? now.headingLevel : ''} row in '
      '<${shellKinds(now)}>, from a ${was.kind.name} row in '
      '<${shellKinds(was)}>, $transition',
    );
  }

  /// (c) A single typed letter or digit at a caret appears in the visible
  /// text exactly where the caret showed it, the caret follows it, and
  /// nothing else shown changes. Rows are compared as the text they paint,
  /// line by line, so a letter typed on an empty line between two
  /// paragraphs, which joins them, passes: the user sees the same lines.
  /// Inline delimiters the letter pairs anew in the rows it touches may
  /// appear or disappear, as in (b): `*a *b` becomes emphasis once a letter
  /// precedes the second `*`, and `_a_` stops being one once a letter
  /// follows it.
  void _checkTypedCharacter() {
    final c = command;
    if (c is! InsertText ||
        !_letterOrDigit.hasMatch(c.text) ||
        !old.selection.isCollapsed) {
      return;
    }
    // An unwritten table cell's caret shares its source offset with the
    // cell before it; the caret's position names the cell.
    final at = old.caretPosition;
    final row = old.projection.rows[at.row];
    // A whitespace-only line in an HTML block that runs to the document's
    // end is that block's literal text.
    final offset = old.selection.extent;
    if (old.model.blocks.any(
      (b) =>
          b.kind == BlockKind.htmlBlock &&
          b.startUtf16 <= offset &&
          offset <= b.endUtf16,
    )) {
      return;
    }
    // Code and HTML show their source: what a letter typed into either does
    // to its reading (an HTML block's start condition is its first
    // characters, a raw tag's attribute names are ASCII) is literal.
    if (row.kind == RowKind.codeBlock ||
        row.kind == RowKind.htmlBlock ||
        _inRawHtml(old, old.selection.extent) &&
            edit.newEnd - edit.start == c.text.length) {
      return;
    }
    // References, and openers: a digit can complete an ordered list marker
    // and a letter an HTML block's start (`<!a`); Markdown then reads the
    // typed character as syntax.
    if (definitionsChanged ||
        inReference(old, old.selection.extent) ||
        inReference(next, next.selection.extent) ||
        openedBlocks.isNotEmpty) {
      return;
    }
    final caretRowAfter =
        next.projection.rows[next.displayOf(next.selection.extent).row];
    // A letter can complete a table row (`|` becomes `|b`), whose pipes
    // the table then hides.
    if (caretRowAfter.kind == RowKind.tableCell &&
        row.kind != RowKind.tableCell) {
      return;
    }
    // Inline pairing: a letter can complete a construct whose syntax holds
    // it, as `[a](<b)c` reads `a<b` as a link's destination once `a`
    // follows its `(`.
    for (var o = edit.start; o < edit.newEnd; o++) {
      if (next.source.startsWith(c.text, o) && inInlineDelimiter(next, o)) {
        return;
      }
    }
    int global(Projection p, DisplayPosition d) {
      var g = 0;
      for (var i = 0; i < d.row; i++) {
        g += p.rows[i].text.length + 1;
      }
      return g + d.offset;
    }

    bool repaired(FlarkDocument doc, int offset) =>
        offset >= 0 &&
        inInlineDelimiter(doc, offset) &&
        oldRowTouched(oldRowOf(doc == old ? offset : edit.back(offset)));
    // A pending style's delimiters typed against a delimiter run already
    // there pair with it as Markdown's flanking rules and rule of three say
    // (`***b***`), which the profile leaves to Markdown.
    if (edit.newEnd - edit.start > c.text.length) {
      bool delimiter(int o) =>
          o >= 0 && o < next.source.length && '*_~`'.contains(next.source[o]);
      if (delimiter(edit.start - 1) || delimiter(edit.newEnd)) return;
    }
    // What the old text shows, less what the edit paired into delimiters,
    // with the letter where the caret was.
    final g = global(old.projection, at);
    final expected = StringBuffer();
    var caretAt = -1;
    final before = paintedUnits(old.projection);
    // Text typed on a rule starts a paragraph after it: the rule stays, and
    // the text shows on the next line.
    final onRule = old.projection.rows[at.row].kind == RowKind.thematicBreak;
    for (var k = 0; k <= before.length; k++) {
      if (k == g) {
        expected.write(onRule ? '\n${c.text}' : c.text);
        caretAt = expected.length;
      }
      if (k == before.length) break;
      final (char, source) = before[k];
      if (source >= 0 && repaired(next, edit.forward(source))) continue;
      expected.write(char);
    }
    // What the new text shows, less delimiters the edit paired anew.
    final shown = StringBuffer();
    final after = paintedUnits(next.projection);
    final caret = global(
      next.projection,
      next.displayOf(next.selection.extent),
    );
    var caretShown = -1;
    for (var k = 0; k <= after.length; k++) {
      if (k == caret) caretShown = shown.length;
      if (k == after.length) break;
      final (char, source) = after[k];
      final was = source < 0 ? -1 : edit.back(source);
      if (was >= 0 && repaired(old, was)) continue;
      shown.write(char);
    }
    // A blank line typing adds to keep the typed text apart from a block it
    // is not part of, or after a rule it keeps, shows nothing, nor does one
    // it adds at the document's end; and whitespace beside a line break is
    // Markdown's to show or strip (a space left at a line's start once a
    // letter follows it). Compare the lines with text, and the caret within
    // them.
    (String, int) lines(String text, int caret) {
      bool space(int i) => text[i] == ' ' || text[i] == '\t';
      final keep = List<bool>.filled(text.length, true);
      for (var j = 0; j < text.length && space(j); j++) {
        keep[j] = false;
      }
      for (var i = 0; i < text.length; i++) {
        if (text[i] != '\n') continue;
        for (var j = i - 1; j >= 0 && space(j); j--) {
          keep[j] = false;
        }
        for (var j = i + 1; j < text.length && space(j); j++) {
          keep[j] = false;
        }
      }
      final out = StringBuffer();
      var mapped = -1, last = '';
      for (var i = 0; i <= text.length; i++) {
        if (i == caret) mapped = out.length;
        if (i == text.length) break;
        if (!keep[i] || text[i] == '\n' && last == '\n') continue;
        out.write(last = text[i]);
      }
      var lines = '$out';
      if (lines.endsWith('\n')) {
        lines = lines.substring(0, lines.length - 1);
        if (mapped > lines.length) mapped = lines.length;
      }
      return (lines, mapped);
    }

    final (shownLines, shownCaret) = lines('$shown', caretShown);
    final (expectedLines, expectedCaret) = lines('$expected', caretAt);
    if (shownLines != expectedLines) {
      fail(
        '$label: [typed] ${jsonEncode(c.text)} at row ${at.row} offset '
        '${at.offset} showed ${jsonEncode('$shown')}, expected '
        '${jsonEncode('$expected')}, $transition',
      );
    }
    if (shownCaret != expectedCaret) {
      final d = next.displayOf(next.selection.extent);
      fail(
        '$label: [typed-caret] after ${jsonEncode(c.text)} the caret shows at '
        'row ${d.row} offset ${d.offset}, not after it, $transition',
      );
    }
  }
}
