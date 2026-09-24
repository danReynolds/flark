/// The projection: what a host draws, derived from the render model.
///
/// Rows are the unit of layout. Every source line belongs to exactly one
/// row or is hidden whole (fence lines, a setext underline, a table's
/// delimiter row). A row's display text is its lines' content with hidden
/// run ranges removed and replacements substituted, joined by line breaks.
/// Segments map display ranges back to source ranges; hidden bytes are the
/// gaps between segments and are never inspected, only skipped. A line the
/// row's text does not represent — a fence, a setext underline, a blank line
/// inside indented code — carries -1 and holds no caret: an edit there would
/// be invisible, or would paint at another line's position. Bare empty
/// heading/item prefixes remain literal authoring text until completed.
library;

import 'dart:collection';

import 'package:characters/characters.dart';

import '../parse/render_model.dart';
import '../parse/schema.g.dart';

/// Inline style bits carried by a segment.
abstract final class Style {
  static const int emphasis = 1;
  static const int strong = 2;
  static const int code = 4;
  static const int strikethrough = 8;
  static const int link = 16;
  static const int image = 32;
  static const int footnoteRef = 64;
  static const int htmlInline = 128;
}

enum RowKind {
  paragraph,
  heading,
  codeBlock,
  htmlBlock,
  thematicBreak,
  tableCell,
  definition,
  blank,
}

enum ShellKind { blockQuote, list, item, footnoteDefinition }

/// A container above a row: quotes, lists and their items, footnote definitions.
final class Shell {
  const Shell({
    required this.kind,
    required this.block,
    this.ordered = false,
    this.start = 1,
    this.tight = true,
    this.task = false,
    this.checked = false,
    this.checkboxStart = -1,
    this.checkboxEnd = -1,
    this.itemIndex = 0,
  });
  final ShellKind kind;
  final int block;
  final bool ordered;
  final int start;
  final bool tight;
  final bool task;
  final bool checked;

  /// UTF-16 range of the task checkbox `[x]`, or -1.
  final int checkboxStart, checkboxEnd;

  /// Position of an item among its list's items.
  final int itemIndex;
}

/// One piece of a row's display text and the source it stands for.
final class Segment {
  const Segment({
    required this.displayStart,
    required this.displayEnd,
    required this.sourceStart,
    required this.sourceEnd,
    required this.styles,
    required this.exact,
    this.run = -1,
    this.lineBreak = false,
  });
  final int displayStart, displayEnd;

  /// UTF-16 source range. Exact segments map offsets one to one.
  final int sourceStart, sourceEnd;
  final int styles;
  final bool exact;

  /// The leaf run this text came from, or -1 for a line break or gap.
  final int run;

  /// A line break between two source lines of the row.
  final bool lineBreak;
  int get displayLength => displayEnd - displayStart;
}

/// Which side of a hidden range a display position resolves to in source.
enum Anchor { before, after }

/// A caret position on the projection.
final class DisplayPosition {
  const DisplayPosition(this.row, this.offset, {this.snapped = false});
  final int row;
  final int offset;

  /// The source position lay inside a hidden range and was moved to the
  /// nearest legal display position.
  final bool snapped;
  @override
  bool operator ==(Object other) =>
      other is DisplayPosition &&
      other.row == row &&
      other.offset == offset &&
      other.snapped == snapped;
  @override
  int get hashCode => Object.hash(row, offset, snapped);
  @override
  String toString() =>
      'DisplayPosition($row, $offset${snapped ? ', snapped' : ''})';
}

final class ProjectedRow {
  ProjectedRow({
    required int index,
    required this.kind,
    required this.block,
    required this.firstLine,
    required this.lineCount,
    required this.text,
    required List<Segment> segments,
    required List<Shell> shells,
    required this.sourceStart,
    required this.sourceEnd,
    required List<int> contentStarts,
    required List<int> contentEnds,
    required List<int> prefixStarts,
    this.headingLevel = 0,
    this.fenced = false,
    this.codeInfoStart = -1,
    this.codeInfoEnd = -1,
    this.tableBlock = -1,
    this.tableRowBlock = -1,
    this.column = -1,
    this.header = false,
    this.alignment = 0,
  }) : _index = index,
       segments = UnmodifiableListView(segments),
       shells = UnmodifiableListView(shells),
       contentStarts = UnmodifiableListView(contentStarts),
       contentEnds = UnmodifiableListView(contentEnds),
       prefixStarts = UnmodifiableListView(prefixStarts);

  /// A row reused by a later projection: the same text and segments with
  /// source offsets moved by [delta] and run indexes by [runDelta], and the
  /// placement and container fields of its new block. Lists are shared when
  /// nothing moved.
  ProjectedRow _reused({
    required int index,
    required int block,
    required int firstLine,
    required List<Shell> shells,
    required int delta,
    required int runDelta,
    required int tableBlock,
    required int tableRowBlock,
    required int column,
    required bool header,
    required int alignment,
  }) {
    int move(int offset) => offset < 0 ? offset : offset + delta;
    final moved = delta != 0 || runDelta != 0;
    return ProjectedRow._copy(
      index: index,
      kind: kind,
      block: block,
      firstLine: firstLine,
      lineCount: lineCount,
      text: text,
      segments: moved
          ? UnmodifiableListView([
              for (final s in segments)
                Segment(
                  displayStart: s.displayStart,
                  displayEnd: s.displayEnd,
                  sourceStart: s.sourceStart + delta,
                  sourceEnd: s.sourceEnd + delta,
                  styles: s.styles,
                  exact: s.exact,
                  run: s.run < 0 ? s.run : s.run + runDelta,
                  lineBreak: s.lineBreak,
                ),
            ])
          : segments,
      shells: UnmodifiableListView(shells),
      sourceStart: sourceStart + delta,
      sourceEnd: sourceEnd + delta,
      contentStarts: delta != 0
          ? UnmodifiableListView([for (final o in contentStarts) move(o)])
          : contentStarts,
      contentEnds: delta != 0
          ? UnmodifiableListView([for (final o in contentEnds) move(o)])
          : contentEnds,
      prefixStarts: delta != 0
          ? UnmodifiableListView([for (final o in prefixStarts) move(o)])
          : prefixStarts,
      headingLevel: headingLevel,
      fenced: fenced,
      codeInfoStart: move(codeInfoStart),
      codeInfoEnd: move(codeInfoEnd),
      tableBlock: tableBlock,
      tableRowBlock: tableRowBlock,
      column: column,
      header: header,
      alignment: alignment,
    );
  }

  ProjectedRow._copy({
    required int index,
    required this.kind,
    required this.block,
    required this.firstLine,
    required this.lineCount,
    required this.text,
    required this.segments,
    required this.shells,
    required this.sourceStart,
    required this.sourceEnd,
    required this.contentStarts,
    required this.contentEnds,
    required this.prefixStarts,
    required this.headingLevel,
    required this.fenced,
    required this.codeInfoStart,
    required this.codeInfoEnd,
    required this.tableBlock,
    required this.tableRowBlock,
    required this.column,
    required this.header,
    required this.alignment,
  }) : _index = index;

  /// Position in [Projection.rows], assigned once rows are ordered.
  int _index;
  int get index => _index;
  final RowKind kind;

  /// The block this row projects, or -1 for a blank row; definitions use -1 too.
  final int block;
  final int firstLine, lineCount;
  final String text;
  final List<Segment> segments;
  final List<Shell> shells;

  /// UTF-16 extent of the row's own content on its lines, prefixes excluded.
  final int sourceStart, sourceEnd;

  /// Per line of the row (index = line - firstLine): where the caret may sit
  /// on that line, and where the innermost container prefix begins (equal to
  /// the content start when the line has none). All three are -1 for a line
  /// the row owns but cannot show — a fence's delimiters, a setext underline,
  /// the definition lines a paragraph swallowed — so every reader must treat a
  /// negative entry as "no caret here" rather than as an offset.
  final List<int> contentStarts, contentEnds, prefixStarts;
  final int headingLevel;
  final bool fenced;
  final int codeInfoStart, codeInfoEnd;
  final int tableBlock, tableRowBlock, column;
  final bool header;
  final int alignment;

  /// Source offset for a display offset. At a boundary where hidden bytes
  /// lie between two segments, [anchor] picks the side; that choice is the
  /// caret's typing context.
  int sourceForDisplay(int offset, {Anchor anchor = Anchor.after}) {
    if (segments.isEmpty) {
      return fenced && contentStarts.every((start) => start < 0)
          ? sourceEnd
          : sourceStart;
    }
    final o = offset.clamp(0, text.length);
    for (var i = 0; i < segments.length; i++) {
      final s = segments[i];
      if (o < s.displayEnd || (o == s.displayEnd && i == segments.length - 1)) {
        if (o == s.displayStart && i > 0 && anchor == Anchor.before) {
          return segments[i - 1].sourceEnd;
        }
        if (s.exact) return s.sourceStart + (o - s.displayStart);
        if (o == s.displayStart) return s.sourceStart;
        if (o == s.displayEnd) return s.sourceEnd;
        return anchor == Anchor.before ? s.sourceStart : s.sourceEnd;
      }
      if (o == s.displayEnd) {
        // Boundary between s and the next segment.
        return anchor == Anchor.before
            ? s.sourceEnd
            : segments[i + 1].sourceStart;
      }
    }
    return segments.last.sourceEnd;
  }

  /// Display offset for a source offset inside this row, and whether it had
  /// to be moved out of a hidden range.
  (int, bool) displayForSource(int source) {
    if (segments.isEmpty) return (0, false);
    for (var i = 0; i < segments.length; i++) {
      final s = segments[i];
      if (source < s.sourceStart) return (s.displayStart, true);
      // A zero-width segment (virtual spaces) and a following segment can share
      // a source offset; the later, exact one owns it.
      if (source == s.sourceEnd &&
          i + 1 < segments.length &&
          segments[i + 1].sourceStart == source) {
        continue;
      }
      if (source <= s.sourceEnd) {
        if (s.exact) return (s.displayStart + (source - s.sourceStart), false);
        if (source == s.sourceStart) return (s.displayStart, false);
        if (source == s.sourceEnd) return (s.displayEnd, false);
        return (s.displayEnd, true);
      }
    }
    return (text.length, source > segments.last.sourceEnd);
  }
}

final class ProjectionOptions {
  const ProjectionOptions({this.softBreakAsNewline = true});

  /// Editing view: a source newline inside a paragraph stays a line break.
  /// A read-only view may set false to join lines with a space.
  final bool softBreakAsNewline;
}

final class Projection {
  Projection._(
    this.model,
    this.source,
    List<ProjectedRow> rows,
    this._rowsByLine,
    this.options,
  ) : rows = UnmodifiableListView(rows);

  final RenderModel model;
  final String source;
  final List<ProjectedRow> rows;
  final List<List<int>> _rowsByLine;
  final ProjectionOptions options;

  /// Project [model] of [source]. [previous], a projection of an earlier
  /// version of the source, lets rows of blocks an edit did not touch be
  /// reused instead of rebuilt; the result is the same either way.
  factory Projection.of(
    RenderModel model,
    String source, {
    ProjectionOptions options = const ProjectionOptions(),
    Projection? previous,
  }) => _Builder(
    model,
    source,
    options,
    previous != null &&
            previous.options.softBreakAsNewline == options.softBreakAsNewline
        ? _Reuse(previous, model, source)
        : null,
  ).build();

  /// Rows that own [line] (a table line holds one per cell).
  List<int> rowsOnLine(int line) =>
      UnmodifiableListView(_rowIndexesOnLine(line));

  List<int> _rowIndexesOnLine(int line) =>
      line >= 0 && line < _rowsByLine.length ? _rowsByLine[line] : const [];

  /// Whether any projected line has a caret span.
  late final bool hasCaretSpans = () {
    for (var l = 0; l < _rowsByLine.length; l++) {
      if (lineSpans(l).isNotEmpty) return true;
    }
    return false;
  }();

  /// End of [line]'s content, excluding its terminator. A join boundary is a
  /// line edge, not a caret span, so the editor needs this for a line whose
  /// markup the row hides.
  int lineContentEnd(int line) =>
      line < 0 || line >= model.lineCount ? -1 : _lineEnd(line);

  /// Where the caret may sit on [line]: one (start, end) span per row on the
  /// line, sorted. Empty for a line with no caret positions (a fence line, a
  /// table's delimiter line). A bodyless fence has one anchor at its source end.
  List<(int, int)> lineSpans(int line) {
    final out = <(int, int)>[];
    if (line < 0 || line >= _rowsByLine.length) return out;
    for (final r in _rowsByLine[line]) {
      final row = rows[r];
      final i = line - row.firstLine;
      if (i < 0 || i >= row.contentStarts.length) continue;
      final s = row.contentStarts[i], e = row.contentEnds[i];
      if (s >= 0) out.add((s, e < s ? s : e));
      // An imported fence may have no body line. Give its empty displayed row
      // one anchor; its first insertion creates the body transactionally.
      if (row.fenced &&
          row.contentStarts.every((start) => start < 0) &&
          line == model.lineOfUtf16(row.sourceEnd)) {
        out.add((row.sourceEnd, row.sourceEnd));
      }
    }
    out.sort((a, b) => a.$1 - b.$1);
    return out;
  }

  int _lineEnd(int l) {
    final start = model.lineStartUtf16(l);
    var e = l + 1 < model.lineCount
        ? model.lineStartUtf16(l + 1)
        : source.length;
    while (e > start &&
        (source.codeUnitAt(e - 1) == 0x0A ||
            source.codeUnitAt(e - 1) == 0x0D)) {
      e--;
    }
    return e;
  }

  /// Whether the parser supplied a cell absent from the source row.
  bool isMissingCell(int? index) {
    if (index == null || index <= 0 || index >= rows.length) return false;
    final row = rows[index], previous = rows[index - 1];
    return row.kind == RowKind.tableCell &&
        row.column > 0 &&
        row.tableRowBlock == previous.tableRowBlock &&
        row.sourceStart == row.sourceEnd &&
        row.text.isEmpty &&
        row.sourceStart == previous.sourceEnd;
  }

  /// Display position of a UTF-16 source offset; snapped out of hidden bytes.
  DisplayPosition? displayForSource(int source, {int? tableCell}) {
    if (isMissingCell(tableCell) && rows[tableCell!].sourceStart == source) {
      return DisplayPosition(tableCell, 0);
    }
    final line = model.lineOfUtf16(source);
    final candidates = _rowIndexesOnLine(line);
    if (candidates.isEmpty) {
      // A hidden line: the nearest row before it, at its end.
      for (var l = line - 1; l >= 0; l--) {
        final r = _rowIndexesOnLine(l);
        if (r.isNotEmpty) {
          return DisplayPosition(
            r.last,
            rows[r.last].text.length,
            snapped: true,
          );
        }
      }
      return rows.isEmpty ? null : DisplayPosition(0, 0, snapped: true);
    }
    // Several rows on a line (table cells): the one whose source spans the offset, else the nearest.
    int best = candidates.first;
    for (final r in candidates) {
      final row = rows[r];
      if (source >= row.sourceStart && source <= row.sourceEnd) {
        best = r;
        break;
      }
      if (source > row.sourceEnd) best = r;
    }
    final (offset, snapped) = rows[best].displayForSource(source);
    return DisplayPosition(best, offset, snapped: snapped);
  }
}

final class _Builder {
  _Builder(this.m, this.src, this.options, this._reuse);
  final RenderModel m;
  final String src;
  final ProjectionOptions options;
  final _Reuse? _reuse;

  /// Style bits of each run, filled per block as its inline row is built.
  late final List<int> _styleOf = List.filled(m.runCount, 0);

  late final List<int> _itemIndexOf = () {
    final indexes = List<int>.filled(m.blockCount, 0);
    final nextForList = List<int>.filled(m.blockCount, 0);
    for (var b = 0; b < m.blockCount; b++) {
      if (m.blockKind(b) != BlockKind.item) continue;
      final list = m.blockParent(b);
      if (list == noParent) continue;
      indexes[b] = nextForList[list]++;
    }
    return indexes;
  }();
  late final List<int> _cellIndexOf = () {
    final indexes = List<int>.filled(m.blockCount, 0);
    final nextForRow = List<int>.filled(m.blockCount, 0);
    for (var b = 0; b < m.blockCount; b++) {
      if (m.blockKind(b) != BlockKind.tableCell) continue;
      final row = m.blockParent(b);
      if (row == noParent) continue;
      indexes[b] = nextForRow[row]++;
    }
    return indexes;
  }();

  Projection build() {
    final lineCount = m.lineCount;
    final rowsByLine = List.generate(lineCount, (_) => <int>[]);
    final rows = <ProjectedRow>[];
    final claimed = List<bool>.filled(
      lineCount,
      false,
    ); // owned by a row or hidden
    void addRow(ProjectedRow row) {
      rows.add(row);
      for (
        var l = row.firstLine;
        l < row.firstLine + row.lineCount && l < lineCount;
        l++
      ) {
        claimed[l] = true;
      }
    }

    // Container line ranges from their (widened) source ranges, innermost last.
    final containerOf = List<int>.filled(lineCount, -1);
    for (var b = 0; b < m.blockCount; b++) {
      final kind = m.blockKind(b);
      if (kind != BlockKind.blockQuote &&
          kind != BlockKind.item &&
          kind != BlockKind.footnoteDefinition) {
        continue;
      }
      final first = m.lineOfUtf16(m.blockStart(b));
      final end = m.blockEnd(b);
      final last = m.lineOfUtf16(end > 0 ? end - 1 : 0);
      for (var l = first; l <= last && l < lineCount; l++) {
        containerOf[l] = b;
      }
    }
    // 1. Definitions: source-only rows.
    for (var d = 0; d < m.definitionCount; d++) {
      final v = m.definitionAt(d);
      final first = m.lineOfUtf16(v.startUtf16);
      final last = m.lineOfUtf16(
        v.endUtf16 > v.startUtf16 ? v.endUtf16 - 1 : v.startUtf16,
      );
      final raw = src.substring(v.startUtf16, v.endUtf16).trimRight();
      final end = v.startUtf16 + raw.length;
      final textBuffer = StringBuffer();
      final segments = <Segment>[];
      var sourceCursor = 0;
      void emitExactDefinition(int relativeEnd) {
        if (relativeEnd <= sourceCursor) return;
        final displayStart = textBuffer.length;
        textBuffer.write(raw.substring(sourceCursor, relativeEnd));
        segments.add(
          Segment(
            displayStart: displayStart,
            displayEnd: textBuffer.length,
            sourceStart: v.startUtf16 + sourceCursor,
            sourceEnd: v.startUtf16 + relativeEnd,
            styles: 0,
            exact: true,
          ),
        );
      }

      for (var i = 0; i < raw.length; i++) {
        if (raw.codeUnitAt(i) != 0x0D) continue;
        emitExactDefinition(i);
        final sourceStart = v.startUtf16 + i;
        var relativeEnd = i + 1;
        if (relativeEnd < raw.length && raw.codeUnitAt(relativeEnd) == 0x0A) {
          relativeEnd++;
        }
        final displayStart = textBuffer.length;
        textBuffer.write('\n');
        segments.add(
          Segment(
            displayStart: displayStart,
            displayEnd: textBuffer.length,
            sourceStart: sourceStart,
            sourceEnd: v.startUtf16 + relativeEnd,
            styles: 0,
            exact: false,
            lineBreak: true,
          ),
        );
        sourceCursor = relativeEnd;
        i = relativeEnd - 1;
      }
      emitExactDefinition(raw.length);
      final text = textBuffer.toString();
      final starts = <int>[], ends = <int>[];
      for (var l = first; l <= last; l++) {
        final ls = m.lineStartUtf16(l), le = _lineEnd(l);
        starts.add(v.startUtf16 > ls ? v.startUtf16 : ls);
        ends.add(end < le ? end : le);
      }
      addRow(
        ProjectedRow(
          index: rows.length,
          kind: RowKind.definition,
          block: -1,
          firstLine: first,
          lineCount: last - first + 1,
          text: text,
          segments: segments,
          shells: _shellsFor(containerOf[first]),
          sourceStart: v.startUtf16,
          sourceEnd: end,
          contentStarts: starts,
          contentEnds: ends,
          prefixStarts: List<int>.of(starts),
        ),
      );
    }
    // 2. Leaf rows in document order; their lines without content are hidden.
    for (var b = 0; b < m.blockCount; b++) {
      final kind = m.blockKind(b);
      final first = m.blockFirstLine(b);
      final n = m.blockLineCount(b);
      switch (kind) {
        case BlockKind.paragraph || BlockKind.heading || BlockKind.tableCell:
          addRow(
            _reusedRow(rows.length, b, containerOf) ??
                _inlineRow(rows.length, b, kind, containerOf),
          );
        case BlockKind.codeBlock || BlockKind.htmlBlock:
          addRow(
            _reusedRow(rows.length, b, containerOf) ??
                _literalRow(rows.length, b, kind, containerOf),
          );
        case BlockKind.thematicBreak:
          addRow(
            ProjectedRow(
              index: rows.length,
              kind: RowKind.thematicBreak,
              block: b,
              firstLine: first,
              lineCount: n,
              text: '',
              segments: const [],
              shells: _shellsFor(containerOf[first]),
              sourceStart: m.blockStart(b),
              sourceEnd: m.blockEnd(b),
              contentStarts: _lineEnds(first, n),
              contentEnds: _lineEnds(first, n),
              prefixStarts: _lineEnds(first, n),
            ),
          );
        case BlockKind.table:
          for (var l = first; l < first + n && l < lineCount; l++) {
            claimed[l] = true;
          }
        default:
          break;
      }
      if (kind == BlockKind.paragraph ||
          kind == BlockKind.heading ||
          kind == BlockKind.codeBlock ||
          kind == BlockKind.htmlBlock) {
        // Lines the leaf owns but has no content for are hidden (fences, underlines).
        for (var l = first; l < first + n && l < lineCount; l++) {
          claimed[l] = true;
        }
      }
    }
    // 3. Blank rows for every line nothing claimed.
    for (var l = 0; l < lineCount; l++) {
      if (claimed[l]) continue;
      // A blank row places its caret at the line end, after any prefix. The
      // prefix of an empty item is its marker; of any other container line,
      // the whole line.
      final contentEnd = _lineEnd(l);
      final shells = _shellsFor(containerOf[l]);
      // Whitespace on a blank line outside any container is the author's, not
      // a parser-owned prefix. Calling it prefix leaves a document that is
      // only a tab with no caret before it and nothing Backspace can remove.
      var prefixStart = shells.isEmpty ? m.lineStartUtf16(l) : contentEnd;
      if (shells.isNotEmpty) {
        // A block's content records are in line order (a schema invariant).
        // Search them: scanning a large quote once per blank line in it made
        // projection quadratic.
        final inner = shells.last;
        var lo = m.blockContentOffset(inner.block);
        var hi = lo + m.blockContentCount(inner.block);
        while (lo < hi) {
          final mid = (lo + hi) >> 1;
          if (m.contentLine(mid) <= l) {
            lo = mid + 1;
          } else {
            hi = mid;
          }
        }
        if (lo > m.blockContentOffset(inner.block) &&
            m.contentLine(lo - 1) == l) {
          prefixStart = m.contentPrefixStart(lo - 1);
        }
      }
      // Comrak accepts a bare unordered marker as an empty item. Keep that
      // parser-owned one-character prefix editable while the user may still
      // be starting emphasis/strong. No marker recognition happens in Dart.
      if (shells.isNotEmpty && shells.last.kind == ShellKind.item) {
        final item = shells.last.block;
        final list = m.blockParent(item);
        final start = m.blockStart(item);
        // Only the marker that is starting its own list is authoring text. A
        // bare marker between real items is one of them, and dropping its list
        // shell would paint the row outside the list it belongs to and refuse
        // Indent and list continuation there.
        final alone =
            list != noParent &&
            m.blockStart(list) == start &&
            m.blockEnd(list) == m.blockEnd(item);
        if (alone &&
            m.blockFirstLine(item) == l &&
            contentEnd - start == 1 &&
            m.blockEnd(item) == contentEnd) {
          addRow(
            _barePrefixRow(
              rows.length,
              item,
              l,
              start,
              contentEnd,
              shells.sublist(0, (shells.length - 2).clamp(0, shells.length)),
            ),
          );
          continue;
        }
      }
      addRow(
        ProjectedRow(
          index: rows.length,
          kind: RowKind.blank,
          block: -1,
          firstLine: l,
          lineCount: 1,
          text: '',
          segments: const [],
          shells: shells,
          sourceStart: contentEnd,
          sourceEnd: contentEnd,
          contentStarts: [contentEnd],
          contentEnds: [contentEnd],
          prefixStarts: [prefixStart],
        ),
      );
    }
    // Display order is line order; rows were added per kind, so bucket them
    // by first line and only order within a line when it holds several
    // (table cells arrive in order; a definition and a leaf may not).
    final byFirstLine = List<List<ProjectedRow>?>.filled(lineCount + 1, null);
    for (final r in rows) {
      (byFirstLine[r.firstLine < lineCount ? r.firstLine : lineCount] ??=
              <ProjectedRow>[])
          .add(r);
    }
    final ordered = <ProjectedRow>[];
    for (final bucket in byFirstLine) {
      if (bucket == null) continue;
      if (bucket.length > 1) {
        var sorted = true;
        for (var i = 1; i < bucket.length && sorted; i++) {
          sorted = bucket[i - 1].sourceStart <= bucket[i].sourceStart;
        }
        if (!sorted) bucket.sort((a, b) => a.sourceStart - b.sourceStart);
      }
      for (final r in bucket) {
        r._index = ordered.length;
        ordered.add(r);
        for (
          var l = r.firstLine;
          l < r.firstLine + r.lineCount && l < lineCount;
          l++
        ) {
          rowsByLine[l].add(r.index);
        }
      }
    }
    return Projection._(m, src, ordered, rowsByLine, options);
  }

  /// A table cell's place in its table, which its row carries but its own
  /// records do not determine.
  ({int tableBlock, int tableRowBlock, int column, bool header, int alignment})
  _cellFields(int block) {
    final rowBlock = m.blockParent(block);
    final tableBlock = rowBlock == noParent ? -1 : m.blockParent(rowBlock);
    final column = _cellIndexOf[block];
    final packed = tableBlock < 0 ? 0 : m.tableAlignments(tableBlock);
    return (
      tableBlock: tableBlock,
      tableRowBlock: rowBlock,
      column: column,
      header: rowBlock != noParent && m.blockFlags(rowBlock) & 1 != 0,
      alignment: column < 16 ? (packed >> (2 * column)) & 3 : 0,
    );
  }

  /// The previous projection's row for leaf block [b], moved into place, when
  /// the edit left the block and its lines unchanged.
  ProjectedRow? _reusedRow(int index, int b, List<int> containerOf) {
    final match = _reuse?.match(b);
    if (match == null) return null;
    final first = m.blockFirstLine(b);
    final cell = m.blockKind(b) == BlockKind.tableCell ? _cellFields(b) : null;
    return match.row._reused(
      index: index,
      block: b,
      firstLine: first,
      shells: _shellsFor(containerOf[first]),
      delta: match.delta,
      runDelta: match.runDelta,
      tableBlock: cell?.tableBlock ?? -1,
      tableRowBlock: cell?.tableRowBlock ?? -1,
      column: cell?.column ?? -1,
      header: cell?.header ?? false,
      alignment: cell?.alignment ?? 0,
    );
  }

  /// Presentation of an authenticated empty block's bare opening prefix.
  /// Ranges and heading level come from the parser; this does not recognize
  /// Markdown or change its source/model. Whitespace commits the block on the
  /// next parse, while a second asterisk can continue an inline delimiter.
  ProjectedRow _barePrefixRow(
    int index,
    int block,
    int line,
    int start,
    int end,
    List<Shell> shells,
  ) => ProjectedRow(
    index: index,
    kind: RowKind.paragraph,
    block: block,
    firstLine: line,
    lineCount: 1,
    text: src.substring(start, end),
    segments: [
      Segment(
        displayStart: 0,
        displayEnd: end - start,
        sourceStart: start,
        sourceEnd: end,
        styles: 0,
        exact: true,
      ),
    ],
    shells: shells,
    sourceStart: start,
    sourceEnd: end,
    contentStarts: [start],
    contentEnds: [end],
    prefixStarts: [start],
  );

  /// Styles of the runs from [first] to [end], one block's: a run's own style
  /// over its parent's. A parent precedes its children in the same block.
  void _computeStyles(int first, int end) {
    for (var i = first; i < end; i++) {
      final parent = m.runParent(i);
      final inherited = parent == noParent ? 0 : _styleOf[parent];
      final kind = m.runKind(i);
      final own = switch (kind) {
        RunKind.emph => Style.emphasis,
        RunKind.strong => Style.strong,
        RunKind.code => Style.code,
        RunKind.strike => Style.strikethrough,
        RunKind.link || RunKind.autolink => Style.link,
        RunKind.image => Style.image,
        RunKind.footnoteRef => Style.footnoteRef,
        RunKind.htmlInline => Style.htmlInline,
        _ => 0,
      };
      _styleOf[i] = inherited | own;
    }
  }

  final _shells = <int, List<Shell>>{};

  List<Shell> _shellsFor(int container) {
    if (container < 0) return const [];
    return _shells[container] ??= _buildShells(container);
  }

  List<Shell> _buildShells(int container) {
    final chain = <Shell>[];
    var b = container;
    while (b >= 0 && b != noParent) {
      final kind = m.blockKind(b);
      if (kind == BlockKind.blockQuote) {
        chain.add(Shell(kind: ShellKind.blockQuote, block: b));
      }
      if (kind == BlockKind.item) {
        final flags = m.blockFlags(b);
        final task = flags & 1 != 0;
        // The checkbox is the task symbol with its ASCII brackets.
        final s = m.itemTaskStart(b), e = m.itemTaskEnd(b);
        final list = m.blockParent(b);
        final itemIndex = _itemIndexOf[b];
        final ordered = list != noParent && m.blockFlags(list) & 2 != 0;
        chain.add(
          Shell(
            kind: ShellKind.item,
            block: b,
            ordered: ordered,
            start: ordered ? m.blockAttr(list) : 1,
            task: task,
            checked: task && flags & 2 != 0,
            checkboxStart: task && s > 0 ? s - 1 : -1,
            checkboxEnd: task && e > 0 ? e + 1 : -1,
            itemIndex: itemIndex,
          ),
        );
      }
      if (kind == BlockKind.list) {
        final flags = m.blockFlags(b);
        chain.add(
          Shell(
            kind: ShellKind.list,
            block: b,
            ordered: flags & 2 != 0,
            start: m.blockAttr(b),
            tight: flags & 1 != 0,
          ),
        );
      }
      if (kind == BlockKind.footnoteDefinition) {
        chain.add(Shell(kind: ShellKind.footnoteDefinition, block: b));
      }
      b = m.blockParent(b);
    }
    return chain.reversed.toList(growable: false);
  }

  /// Hidden UTF-16 intervals of a block's runs: delimiters plus break markers.
  List<(int, int)> _hiddenIntervals(int block) {
    final out = <(int, int)>[];
    var sorted = true, last = -1;
    void add(int a, int b) {
      if (a < last) sorted = false;
      last = a;
      out.add((a, b));
    }

    for (
      var r = m.firstRunOfBlock(block), end = m.firstRunOfBlock(block + 1);
      r < end;
      r++
    ) {
      final s = m.runStart(r),
          e = m.runEnd(r),
          cs = m.runContentStart(r),
          ce = m.runContentEnd(r);
      if (cs > s) add(s, cs);
      if (e > ce) add(ce, e);
    }
    if (!sorted) out.sort((a, b) => a.$1 - b.$1);
    return out;
  }

  ProjectedRow _inlineRow(
    int index,
    int block,
    int kind,
    List<int> containerOf,
  ) {
    final first = m.blockFirstLine(block), n = m.blockLineCount(block);
    final co = m.blockContentOffset(block), cn = m.blockContentCount(block);
    final hidden = _hiddenIntervals(block);
    // Text comes from runs without children (a link's or autolink's text is
    // its Text child); containers only contribute style and hidden ranges.
    final firstRun = m.firstRunOfBlock(block),
        endRun = m.firstRunOfBlock(block + 1);
    _computeStyles(firstRun, endRun);
    final hasChildren = List<bool>.filled(endRun - firstRun, false);
    for (var r = firstRun; r < endRun; r++) {
      final parent = m.runParent(r);
      if (parent != noParent && parent >= firstRun) {
        hasChildren[parent - firstRun] = true;
      }
    }
    // Leaf runs with their content ranges, read once: the per-line walk
    // below revisits them.
    final leafRuns = <int>[], leafStarts = <int>[], leafEnds = <int>[];
    // Where each of the block's lines breaks, and the code run owning that
    // break; -1 when none. A line outside the block keeps a map entry.
    final breakStartOf = List<int>.filled(n, -1),
        codeBreakOf = List<int>.filled(n, -1);
    Map<int, int>? otherBreaks, otherCodeBreaks;
    void setBreak(int line, int start, int codeRun) {
      final i = line - first;
      if (i >= 0 && i < n) {
        breakStartOf[i] = start;
        if (codeRun >= 0) codeBreakOf[i] = codeRun;
      } else {
        (otherBreaks ??= {})[line] = start;
        if (codeRun >= 0) (otherCodeBreaks ??= {})[line] = codeRun;
      }
    }

    int? breakStartAt(int line) {
      final i = line - first;
      if (i < 0 || i >= n) return otherBreaks?[line];
      final start = breakStartOf[i];
      return start < 0 ? null : start;
    }

    int? codeBreakAt(int line) {
      final i = line - first;
      if (i < 0 || i >= n) return otherCodeBreaks?[line];
      final run = codeBreakOf[i];
      return run < 0 ? null : run;
    }

    for (var r = firstRun; r < endRun; r++) {
      final k = m.runKind(r);
      if (k == RunKind.softBreak || k == RunKind.hardBreak) {
        final start = m.runStart(r);
        final cs = m.runContentStart(r);
        final ce = m.runContentEnd(r);
        setBreak(m.lineOfUtf16(start), ce > cs ? ce : start, -1);
        continue;
      }
      if (k == RunKind.code) {
        final first = m.lineOfUtf16(m.runContentStart(r));
        final last = m.lineOfUtf16(m.runContentEnd(r));
        for (var line = first; line < last; line++) {
          setBreak(line, _lineEnd(line), r);
        }
      }
      if (!hasChildren[r - firstRun]) {
        leafRuns.add(r);
        leafStarts.add(m.runContentStart(r));
        leafEnds.add(m.runContentEnd(r));
      }
    }
    final text = StringBuffer();
    final segments = <Segment>[];
    var sourceStart = -1, sourceEnd = -1;
    final starts = List<int>.filled(n, -1),
        ends = List<int>.filled(n, -1),
        prefixes = List<int>.filled(n, -1);
    // Abutting hidden runs hide one continuous range; merged, a gap is covered
    // by a single interval or by none.
    final covered = <(int, int)>[];
    for (final h in hidden) {
      if (covered.isNotEmpty && h.$1 <= covered.last.$2) {
        if (h.$2 > covered.last.$2) covered.last = (covered.last.$1, h.$2);
      } else {
        covered.add(h);
      }
    }
    var coverIndex = 0, leafCursor = 0;
    for (var c = 0; c < cn; c++) {
      final rec = m.contentLine(co + c);
      final cs = m.contentStart(co + c), ce = m.contentEnd(co + c);
      if (rec >= first && rec < first + n) {
        starts[rec - first] = cs;
        ends[rec - first] = ce;
        prefixes[rec - first] = m.contentPrefixStart(co + c);
      }
      if (sourceStart < 0) sourceStart = cs;
      sourceEnd = ce;
      var prevEnd = c > 0 ? breakStartAt(m.contentLine(co + c - 1)) : null;
      if (c > 0 && prevEnd == null) {
        // Not every line ending has a break run: comrak reports none inside
        // raw inline HTML, or where an unparsed bracket run spans lines. Only
        // a gap the markup already hides — a link's destination continued on
        // the next line — displays nothing. Otherwise the two lines would be
        // joined with nothing between them, and both sides of the break would
        // share one display position.
        final gap = m.contentEnd(co + c - 1);
        if (cs > gap) {
          // Both this gap and the merged runs advance with c, so one cursor
          // walks them together instead of rescanning per line.
          while (coverIndex < covered.length && covered[coverIndex].$2 < cs) {
            coverIndex++;
          }
          final over =
              coverIndex < covered.length && covered[coverIndex].$1 <= gap;
          if (!over) prevEnd = gap;
        }
      }
      final codeBreak = c > 0 ? codeBreakAt(m.contentLine(co + c - 1)) : null;
      // Inline code normalizes each source line ending to a styled space.
      // Destination-only physical lines still contribute no displayed break.
      if (prevEnd != null) {
        final d0 = text.length;
        text.write(
          codeBreak == null && options.softBreakAsNewline ? '\n' : ' ',
        );
        segments.add(
          Segment(
            displayStart: d0,
            displayEnd: text.length,
            sourceStart: prevEnd,
            sourceEnd: cs,
            styles: codeBreak == null ? 0 : _styleOf[codeBreak],
            exact: false,
            lineBreak: true,
            run: codeBreak ?? -1,
          ),
        );
      }
      final virt = m.contentVirtualSpaces(co + c);
      if (virt > 0) {
        final d0 = text.length;
        text.write(' ' * virt);
        segments.add(
          Segment(
            displayStart: d0,
            displayEnd: text.length,
            sourceStart: cs,
            sourceEnd: cs,
            styles: 0,
            exact: false,
          ),
        );
      }
      var p = cs;
      void emitExact(int a, int b, int style, int run) {
        if (b <= a) return;
        final d0 = text.length;
        text.write(src.substring(a, b));
        // A zero-display replacement immediately before a visible source
        // piece is one rendered grapheme owner. The parse crate uses this for
        // the backslash in an escaped table-cell pipe. Keeping the pieces
        // separate made Backspace delete only `|` and expose the backslash.
        if (segments.isNotEmpty) {
          final previous = segments.last;
          if (!previous.exact &&
              previous.displayLength == 0 &&
              previous.displayEnd == d0 &&
              previous.sourceEnd == a) {
            final firstLength = src.substring(a, b).characters.first.length;
            final firstEnd = a + firstLength;
            segments[segments.length - 1] = Segment(
              displayStart: previous.displayStart,
              displayEnd: d0 + firstLength,
              sourceStart: previous.sourceStart,
              sourceEnd: firstEnd,
              styles: style,
              exact: false,
              run: run,
            );
            if (firstEnd < b) {
              segments.add(
                Segment(
                  displayStart: d0 + firstLength,
                  displayEnd: text.length,
                  sourceStart: firstEnd,
                  sourceEnd: b,
                  styles: style,
                  exact: true,
                  run: run,
                ),
              );
            }
            return;
          }
        }
        segments.add(
          Segment(
            displayStart: d0,
            displayEnd: text.length,
            sourceStart: a,
            sourceEnd: b,
            styles: style,
            exact: true,
            run: run,
          ),
        );
      }

      void emitGap(int a, int b) {
        // Content bytes no leaf run claims: hidden if inside a delimiter interval, else shown exactly.
        var q = a;
        // Merged intervals are sorted and disjoint. Search for the first one
        // ending after q instead of rescanning the block's delimiters per gap.
        var lo = 0, hi = covered.length;
        while (lo < hi) {
          final mid = (lo + hi) >> 1;
          if (covered[mid].$2 <= q) {
            lo = mid + 1;
          } else {
            hi = mid;
          }
        }
        for (var k = lo; k < covered.length; k++) {
          final h = covered[k];
          if (h.$1 >= b) break;
          if (h.$1 > q) emitExact(q, h.$1, 0, -1);
          q = h.$2 > q ? h.$2 : q;
        }
        if (q < b) emitExact(q, b, 0, -1);
      }

      // Leaf runs are in source order, as the segment walk already requires.
      // Lines only advance, so skip the runs ending before this one for good
      // and stop at the first run starting after it. Visiting every leaf for
      // every line made one long paragraph quadratic.
      while (leafCursor < leafRuns.length && leafEnds[leafCursor] <= cs) {
        leafCursor++;
      }
      for (var j = leafCursor; j < leafRuns.length; j++) {
        final r = leafRuns[j];
        final rs = leafStarts[j], re = leafEnds[j];
        if (rs >= ce) break;
        if (re <= cs) continue;
        final a = rs < cs ? cs : rs, b = re > ce ? ce : re;
        if (a > p) emitGap(p, a);

        final style = _styleOf[r];
        final override = m.displayOverride(r);
        if (override != null && rs >= cs && re <= ce) {
          final d0 = text.length;
          text.write(override);
          segments.add(
            Segment(
              displayStart: d0,
              displayEnd: text.length,
              sourceStart: rs,
              sourceEnd: re,
              styles: style,
              exact:
                  override.length == re - rs &&
                  override == src.substring(rs, re),
              run: r,
            ),
          );
        } else if (b > a) {
          emitExact(a, b, style, r);
        }
        p = b > p ? b : p;
      }
      if (p < ce) emitGap(p, ce);
    }
    // An inline leaf's lines without a record are never the row's to show: a
    // paragraph's are the definition lines a definition row already covers,
    // and a setext underline is markup this row hides. Giving either a caret
    // would paint it at the end of the heading or the line above, where the
    // next character silently rewrites markup the user cannot see.
    final shells = _shellsFor(containerOf[first]);
    switch (kind) {
      case BlockKind.heading:
        final blockStart = m.blockStart(block);
        final level = m.blockAttr(block);
        if (n == 1 &&
            sourceStart == sourceEnd &&
            sourceEnd == _lineEnd(first) &&
            sourceEnd - blockStart == level) {
          return _barePrefixRow(
            index,
            block,
            first,
            blockStart,
            sourceEnd,
            shells,
          );
        }
        return ProjectedRow(
          index: index,
          kind: RowKind.heading,
          block: block,
          firstLine: first,
          lineCount: n,
          text: text.toString(),
          segments: segments,
          shells: shells,
          sourceStart: sourceStart < 0 ? m.blockStart(block) : sourceStart,
          sourceEnd: sourceEnd < 0 ? m.blockEnd(block) : sourceEnd,
          contentStarts: starts,
          contentEnds: ends,
          prefixStarts: prefixes,
          headingLevel: m.blockAttr(block),
        );
      case BlockKind.tableCell:
        final cell = _cellFields(block);
        return ProjectedRow(
          index: index,
          kind: RowKind.tableCell,
          block: block,
          firstLine: first,
          lineCount: n,
          text: text.toString(),
          segments: segments,
          shells: shells,
          sourceStart: sourceStart < 0 ? m.blockStart(block) : sourceStart,
          sourceEnd: sourceEnd < 0 ? m.blockEnd(block) : sourceEnd,
          contentStarts: starts,
          contentEnds: ends,
          prefixStarts: prefixes,
          tableBlock: cell.tableBlock,
          tableRowBlock: cell.tableRowBlock,
          column: cell.column,
          header: cell.header,
          alignment: cell.alignment,
        );
      default:
        return ProjectedRow(
          index: index,
          kind: RowKind.paragraph,
          block: block,
          firstLine: first,
          lineCount: n,
          text: text.toString(),
          segments: segments,
          shells: shells,
          sourceStart: sourceStart < 0 ? m.blockStart(block) : sourceStart,
          sourceEnd: sourceEnd < 0 ? m.blockEnd(block) : sourceEnd,
          contentStarts: starts,
          contentEnds: ends,
          prefixStarts: prefixes,
        );
    }
  }

  /// A line the leaf owns but has no content record for keeps the caret at its
  /// end, so it stays reachable: a fence's body line that carries only a
  /// container prefix, a blank line comrak folded into the block. The rows that
  /// hide such a line as markup — a fence's delimiters, a setext underline —
  /// clear it again, because an edit there would be invisible or would paint at
  /// another line's position.
  void _fillLineEnds(
    int first,
    List<int> starts,
    List<int> ends,
    List<int> prefixes,
  ) {
    for (var i = 0; i < starts.length; i++) {
      if (starts[i] >= 0 || first + i >= m.lineCount) continue;
      final e = _lineEnd(first + i);
      starts[i] = e;
      ends[i] = e;
      prefixes[i] = e;
    }
  }

  /// Line end excluding the terminator.
  int _lineEnd(int l) {
    final start = m.lineStartUtf16(l);
    var e = l + 1 < m.lineCount ? m.lineStartUtf16(l + 1) : src.length;
    while (e > start &&
        (src.codeUnitAt(e - 1) == 0x0A || src.codeUnitAt(e - 1) == 0x0D)) {
      e--;
    }
    return e;
  }

  List<int> _lineEnds(int first, int n) => [
    for (var l = first; l < first + n && l < m.lineCount; l++) _lineEnd(l),
  ];

  ProjectedRow _literalRow(
    int index,
    int block,
    int kind,
    List<int> containerOf,
  ) {
    final first = m.blockFirstLine(block), n = m.blockLineCount(block);
    final co = m.blockContentOffset(block), cn = m.blockContentCount(block);
    final text = StringBuffer();
    final segments = <Segment>[];
    var sourceStart = -1, sourceEnd = -1;
    final starts = List<int>.filled(n, -1),
        ends = List<int>.filled(n, -1),
        prefixes = List<int>.filled(n, -1);
    for (var c = 0; c < cn; c++) {
      final rec = m.contentLine(co + c);
      final cs = m.contentStart(co + c), ce = m.contentEnd(co + c);
      if (rec >= first && rec < first + n) {
        starts[rec - first] = cs;
        ends[rec - first] = ce;
        prefixes[rec - first] = m.contentPrefixStart(co + c);
      }
      if (sourceStart < 0) sourceStart = cs;
      sourceEnd = ce;
      if (c > 0) {
        final prevEnd = m.contentEnd(co + c - 1);
        final d0 = text.length;
        text.write('\n');
        segments.add(
          Segment(
            displayStart: d0,
            displayEnd: text.length,
            sourceStart: prevEnd,
            sourceEnd: cs,
            styles: 0,
            exact: false,
            lineBreak: true,
          ),
        );
      }
      final virt = m.contentVirtualSpaces(co + c);
      if (virt > 0) {
        final d0 = text.length;
        text.write(' ' * virt);
        segments.add(
          Segment(
            displayStart: d0,
            displayEnd: text.length,
            sourceStart: cs,
            sourceEnd: cs,
            styles: 0,
            exact: false,
          ),
        );
      }
      if (ce > cs) {
        final d0 = text.length;
        text.write(src.substring(cs, ce));
        segments.add(
          Segment(
            displayStart: d0,
            displayEnd: text.length,
            sourceStart: cs,
            sourceEnd: ce,
            styles: kind == BlockKind.codeBlock ? Style.code : 0,
            exact: true,
          ),
        );
      }
    }
    _fillLineEnds(first, starts, ends, prefixes);
    final flags = m.blockFlags(block);
    // Fence lines hold no caret: an edit there would be invisible. The info
    // string is a host affordance, not a caret position.
    if (kind == BlockKind.codeBlock && flags & 1 != 0 && starts.isNotEmpty) {
      starts[0] = -1;
      ends[0] = -1;
      prefixes[0] = -1;
      if (flags & 2 != 0 && starts.length > 1) {
        starts[starts.length - 1] = -1;
        ends[ends.length - 1] = -1;
        prefixes[prefixes.length - 1] = -1;
      }
    }
    return ProjectedRow(
      index: index,
      kind: kind == BlockKind.codeBlock ? RowKind.codeBlock : RowKind.htmlBlock,
      block: block,
      firstLine: first,
      lineCount: n,
      text: text.toString(),
      segments: segments,
      shells: _shellsFor(containerOf[first]),
      sourceStart: sourceStart < 0 ? m.blockStart(block) : sourceStart,
      sourceEnd: sourceEnd < 0 ? m.blockEnd(block) : sourceEnd,
      contentStarts: starts,
      contentEnds: ends,
      prefixStarts: prefixes,
      fenced: kind == BlockKind.codeBlock && flags & 1 != 0,
      codeInfoStart: kind == BlockKind.codeBlock && flags & 1 != 0
          ? m.codeInfoStart(block)
          : -1,
      codeInfoEnd: kind == BlockKind.codeBlock && flags & 1 != 0
          ? m.codeInfoEnd(block)
          : -1,
    );
  }
}

/// Rows of an earlier projection that a new one can take over.
///
/// A leaf's row depends only on its block's own records and on the source of
/// the lines it occupies; container shells and table fields are recomputed
/// for every row. A row is reused when its block's lines lie wholly in source
/// the edit left untouched, before the change or after it, and the block's
/// records equal the old block's relative to where each block's lines start.
final class _Reuse {
  _Reuse(this.previous, this.m, String source)
    : old = previous.model,
      length = source.length,
      oldLength = previous.source.length,
      blockDelta = m.blockCount - previous.model.blockCount,
      rowOfBlock = List.filled(previous.model.blockCount, null) {
    final before = previous.source;
    final shorter = length < oldLength ? length : oldLength;
    // Whole chunks compare as substrings first: in the browser that measured
    // 5 to 30 times faster than reading code units one at a time, and on the
    // VM the two cost the same.
    const chunk = 256;
    var p = 0;
    while (p + chunk <= shorter &&
        before.substring(p, p + chunk) == source.substring(p, p + chunk)) {
      p += chunk;
    }
    while (p < shorter && before.codeUnitAt(p) == source.codeUnitAt(p)) {
      p++;
    }
    var q = 0;
    while (q + chunk <= shorter - p &&
        before.substring(oldLength - q - chunk, oldLength - q) ==
            source.substring(length - q - chunk, length - q)) {
      q += chunk;
    }
    while (q < shorter - p &&
        before.codeUnitAt(oldLength - 1 - q) ==
            source.codeUnitAt(length - 1 - q)) {
      q++;
    }
    prefix = p;
    suffix = q;
    for (final row in previous.rows) {
      final b = row.block;
      if (b >= 0 && _isLeaf(old.blockKind(b))) rowOfBlock[b] = row;
    }
  }

  final Projection previous;
  final RenderModel m, old;
  final int length, oldLength, blockDelta;
  final List<ProjectedRow?> rowOfBlock;

  /// Source both versions share at the start and at the end.
  late final int prefix, suffix;

  static bool _isLeaf(int kind) =>
      kind == BlockKind.paragraph ||
      kind == BlockKind.heading ||
      kind == BlockKind.tableCell ||
      kind == BlockKind.codeBlock ||
      kind == BlockKind.htmlBlock;

  /// Where the lines of block [b] start, or the block if it starts earlier.
  static int _extentStart(RenderModel m, int b) {
    final first = m.blockFirstLine(b), start = m.blockStart(b);
    final line = first < m.lineCount ? m.lineStartUtf16(first) : start;
    return start < line ? start : line;
  }

  /// Where the line after block [b] starts. A run may end one past its block.
  static int _extentEnd(RenderModel m, int length, int b) {
    final next = m.blockFirstLine(b) + m.blockLineCount(b);
    final line = next < m.lineCount ? m.lineStartUtf16(next) : length;
    final end = m.blockEnd(b) < length ? m.blockEnd(b) + 1 : length;
    return end > line ? end : line;
  }

  /// The old row for new block [b], with how far its source offsets and run
  /// indexes move, or null when the row must be rebuilt.
  ({ProjectedRow row, int delta, int runDelta})? match(int b) {
    if (m.blockLineCount(b) == 0) return null;
    final start = _extentStart(m, b), end = _extentEnd(m, length, b);
    final int ob, oldStart;
    if (end <= prefix) {
      ob = b;
      oldStart = start;
    } else if (start >= length - suffix) {
      ob = b - blockDelta;
      oldStart = start - (length - oldLength);
    } else {
      return null;
    }
    if (ob < 0 || ob >= old.blockCount) return null;
    final row = rowOfBlock[ob];
    if (row == null ||
        _extentStart(old, ob) != oldStart ||
        _extentEnd(old, oldLength, ob) != oldStart + (end - start) ||
        !_sameBlock(b, ob, start, oldStart)) {
      return null;
    }
    return (
      row: row,
      delta: start - oldStart,
      runDelta: m.firstRunOfBlock(b) - old.firstRunOfBlock(ob),
    );
  }

  /// Whether block [b] and old block [ob] have the same records relative to
  /// [base] and [oldBase]: kind and attributes, content records and runs.
  bool _sameBlock(int b, int ob, int base, int oldBase) {
    final kind = m.blockKind(b), flags = m.blockFlags(b);
    if (kind != old.blockKind(ob) ||
        flags != old.blockFlags(ob) ||
        m.blockAttr(b) != old.blockAttr(ob) ||
        m.blockLineCount(b) != old.blockLineCount(ob) ||
        m.blockStart(b) - base != old.blockStart(ob) - oldBase ||
        m.blockEnd(b) - base != old.blockEnd(ob) - oldBase) {
      return false;
    }
    if (kind == BlockKind.codeBlock &&
        flags & 1 != 0 &&
        (m.codeInfoStart(b) - base != old.codeInfoStart(ob) - oldBase ||
            m.codeInfoEnd(b) - base != old.codeInfoEnd(ob) - oldBase)) {
      return false;
    }
    final count = m.blockContentCount(b);
    if (count != old.blockContentCount(ob) ||
        !sameContentRecords(
          m,
          m.blockContentOffset(b),
          old,
          old.blockContentOffset(ob),
          count,
          aBase: base,
          bBase: oldBase,
          aLine: m.blockFirstLine(b),
          bLine: old.blockFirstLine(ob),
        )) {
      return false;
    }
    final r0 = m.firstRunOfBlock(b), q0 = old.firstRunOfBlock(ob);
    final runs = m.firstRunOfBlock(b + 1) - r0;
    return runs == old.firstRunOfBlock(ob + 1) - q0 &&
        sameRunRecords(m, r0, old, q0, runs, aBase: base, bBase: oldBase);
  }
}
