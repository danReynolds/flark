/// The projection: what a host draws, derived from the render model.
///
/// Rows are the unit of layout. Every source line belongs to exactly one
/// row or is hidden whole (fence lines, a setext underline, a table's
/// delimiter row). A row's display text is its lines' content with hidden
/// run ranges removed and replacements substituted, joined by line breaks.
/// Segments map display ranges back to source ranges; hidden bytes are the
/// gaps between segments and are never inspected, only skipped.
library;

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

enum RowKind { paragraph, heading, codeBlock, htmlBlock, thematicBreak, tableCell, definition, blank }

enum ShellKind { blockQuote, list, item, footnoteDefinition }

/// A container above a row: quotes, lists and their items, footnote definitions.
final class Shell {
  const Shell({required this.kind, required this.block, this.ordered = false, this.start = 1, this.tight = true, this.task = false, this.checked = false, this.checkboxStart = -1, this.checkboxEnd = -1, this.itemIndex = 0});
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
  const Segment({required this.displayStart, required this.displayEnd, required this.sourceStart, required this.sourceEnd, required this.styles, required this.exact, this.run = -1, this.lineBreak = false});
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
  bool operator ==(Object other) => other is DisplayPosition && other.row == row && other.offset == offset;
  @override
  int get hashCode => Object.hash(row, offset);
  @override
  String toString() => 'DisplayPosition($row, $offset${snapped ? ', snapped' : ''})';
}

final class ProjectedRow {
  ProjectedRow({required this.index, required this.kind, required this.block, required this.firstLine, required this.lineCount, required this.text, required this.segments, required this.shells, required this.sourceStart, required this.sourceEnd, required this.contentStarts, required this.contentEnds, required this.prefixStarts, this.headingLevel = 0, this.fenced = false, this.codeInfoStart = -1, this.codeInfoEnd = -1, this.tableBlock = -1, this.tableRowBlock = -1, this.column = -1, this.header = false, this.alignment = 0});
  final int index;
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
  /// the content start when the line has none). A line without content, such
  /// as a fence line, holds its line end so it stays reachable.
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
    if (segments.isEmpty) return sourceStart;
    final o = offset.clamp(0, text.length);
    for (var i = 0; i < segments.length; i++) {
      final s = segments[i];
      if (o < s.displayEnd || (o == s.displayEnd && i == segments.length - 1)) {
        if (o == s.displayStart && i > 0 && anchor == Anchor.before) return segments[i - 1].sourceEnd;
        if (s.exact) return s.sourceStart + (o - s.displayStart);
        if (o == s.displayStart) return s.sourceStart;
        if (o == s.displayEnd) return s.sourceEnd;
        return anchor == Anchor.before ? s.sourceStart : s.sourceEnd;
      }
      if (o == s.displayEnd) {
        // Boundary between s and the next segment.
        return anchor == Anchor.before ? s.sourceEnd : segments[i + 1].sourceStart;
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
      if (source == s.sourceEnd && i + 1 < segments.length && segments[i + 1].sourceStart == source) continue;
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
  Projection._(this.model, this.source, this.rows, this._rowsByLine, this.options);

  final RenderModel model;
  final String source;
  final List<ProjectedRow> rows;
  final List<List<int>> _rowsByLine;
  final ProjectionOptions options;

  factory Projection.of(RenderModel model, String source, {ProjectionOptions options = const ProjectionOptions()}) => _Builder(model, source, options).build();

  /// Rows that own [line] (a table line holds one per cell).
  List<int> rowsOnLine(int line) => line < _rowsByLine.length ? _rowsByLine[line] : const [];

  /// Whether any line has a caret span; false only for a document that is
  /// nothing but fence lines, where the caret sits at the end instead.
  late final bool hasCaretSpans = () { for (var l = 0; l < _rowsByLine.length; l++) { if (lineSpans(l).isNotEmpty) return true; } return false; }();

  /// Where the caret may sit on [line]: one (start, end) span per row on the
  /// line, sorted. Empty for a line with no caret positions (a fence line, a
  /// table's delimiter line).
  List<(int, int)> lineSpans(int line) {
    final out = <(int, int)>[];
    if (line < 0 || line >= _rowsByLine.length) return out;
    for (final r in _rowsByLine[line]) {
      final row = rows[r];
      final i = line - row.firstLine;
      if (i < 0 || i >= row.contentStarts.length) continue;
      final s = row.contentStarts[i], e = row.contentEnds[i];
      if (s >= 0) out.add((s, e < s ? s : e));
    }
    out.sort((a, b) => a.$1 - b.$1);
    return out;
  }

  /// Display position of a UTF-16 source offset; snapped out of hidden bytes.
  DisplayPosition? displayForSource(int source) {
    final line = model.lineOfUtf16(source);
    final candidates = rowsOnLine(line);
    if (candidates.isEmpty) {
      // A hidden line: the nearest row before it, at its end.
      for (var l = line - 1; l >= 0; l--) { final r = rowsOnLine(l); if (r.isNotEmpty) return DisplayPosition(r.last, rows[r.last].text.length, snapped: true); }
      return rows.isEmpty ? null : DisplayPosition(0, 0, snapped: true);
    }
    // Several rows on a line (table cells): the one whose source spans the offset, else the nearest.
    int best = candidates.first;
    for (final r in candidates) { final row = rows[r]; if (source >= row.sourceStart && source <= row.sourceEnd) { best = r; break; } if (source > row.sourceEnd) best = r; }
    final (offset, snapped) = rows[best].displayForSource(source);
    return DisplayPosition(best, offset, snapped: snapped);
  }
}

final class _Builder {
  _Builder(this.m, this.src, this.options);
  final RenderModel m;
  final String src;
  final ProjectionOptions options;

  late final List<int> _styleOf = List.filled(m.runCount, 0);
  late final List<int> _linkOf = List.filled(m.runCount, -1);

  Projection build() {
    _computeStyles();
    final lineCount = m.lineCount;
    final rowsByLine = List.generate(lineCount, (_) => <int>[]);
    final rows = <ProjectedRow>[];
    final claimed = List<bool>.filled(lineCount, false); // owned by a row or hidden
    void addRow(ProjectedRow row) { rows.add(row); for (var l = row.firstLine; l < row.firstLine + row.lineCount && l < lineCount; l++) { rowsByLine[l].add(row.index); claimed[l] = true; } }
    // Container line ranges from their (widened) byte ranges, innermost last.
    final containerOf = List<int>.filled(lineCount, -1);
    for (var b = 0; b < m.blockCount; b++) {
      final kind = m.block(b, BlockField.kind);
      if (kind != BlockKind.blockQuote && kind != BlockKind.item && kind != BlockKind.footnoteDefinition) continue;
      final first = m.lineOfByte(m.block(b, BlockField.startByte));
      final end = m.block(b, BlockField.endByte);
      final last = m.lineOfByte(end > 0 ? end - 1 : 0);
      for (var l = first; l <= last && l < lineCount; l++) { containerOf[l] = b; }
    }
    // 1. Definitions: source-only rows.
    for (var d = 0; d < m.definitionCount; d++) {
      final v = m.definitionAt(d);
      final first = m.lineOfByte(v.startByte);
      final endByte = v.endByte > v.startByte ? v.endByte - 1 : v.startByte;
      final last = m.lineOfByte(endByte);
      final text = src.substring(v.startUtf16, v.endUtf16).replaceAll('\r\n', '\n').replaceAll('\r', '\n').trimRight();
      final end = v.startUtf16 + text.length;
      final starts = <int>[], ends = <int>[];
      for (var l = first; l <= last; l++) { final ls = m.lineStartUtf16(l), le = _lineEnd(l); starts.add(v.startUtf16 > ls ? v.startUtf16 : ls); ends.add(end < le ? end : le); }
      addRow(ProjectedRow(index: rows.length, kind: RowKind.definition, block: -1, firstLine: first, lineCount: last - first + 1, text: text, segments: [Segment(displayStart: 0, displayEnd: text.length, sourceStart: v.startUtf16, sourceEnd: end, styles: 0, exact: text.length == end - v.startUtf16)], shells: _shellsFor(containerOf[first]), sourceStart: v.startUtf16, sourceEnd: end, contentStarts: starts, contentEnds: ends, prefixStarts: List<int>.of(starts)));
    }
    // 2. Leaf rows in document order; their lines without content are hidden.
    for (var b = 0; b < m.blockCount; b++) {
      final kind = m.block(b, BlockField.kind);
      final first = m.block(b, BlockField.firstLine);
      final n = m.block(b, BlockField.lineCount);
      switch (kind) {
        case BlockKind.paragraph || BlockKind.heading || BlockKind.tableCell:
          addRow(_inlineRow(rows.length, b, kind, containerOf));
        case BlockKind.codeBlock || BlockKind.htmlBlock:
          addRow(_literalRow(rows.length, b, kind, containerOf));
        case BlockKind.thematicBreak:
          addRow(ProjectedRow(index: rows.length, kind: RowKind.thematicBreak, block: b, firstLine: first, lineCount: n, text: '', segments: const [], shells: _shellsFor(containerOf[first]), sourceStart: m.block(b, BlockField.startUtf16), sourceEnd: m.block(b, BlockField.endUtf16), contentStarts: _lineEnds(first, n), contentEnds: _lineEnds(first, n), prefixStarts: _lineEnds(first, n)));
        case BlockKind.table:
          for (var l = first; l < first + n && l < lineCount; l++) { claimed[l] = true; }
        default:
          break;
      }
      if (kind == BlockKind.paragraph || kind == BlockKind.heading || kind == BlockKind.codeBlock || kind == BlockKind.htmlBlock) {
        // Lines the leaf owns but has no content for are hidden (fences, underlines).
        for (var l = first; l < first + n && l < lineCount; l++) { claimed[l] = true; }
      }
    }
    // 3. Blank rows for every line nothing claimed.
    for (var l = 0; l < lineCount; l++) {
      if (claimed[l]) continue;
      final start = m.lineStartUtf16(l);
      // A blank row places its caret at the line end, after any prefix. The
      // prefix of an empty item is its marker; of any other container line,
      // the whole line.
      final contentEnd = _lineEnd(l);
      final shells = _shellsFor(containerOf[l]);
      var prefixStart = contentEnd;
      if (shells.isNotEmpty) {
        final inner = shells.last;
        prefixStart = (inner.kind == ShellKind.item || inner.kind == ShellKind.footnoteDefinition) && m.block(inner.block, BlockField.firstLine) == l ? m.block(inner.block, BlockField.startUtf16) : start;
      }
      addRow(ProjectedRow(index: rows.length, kind: RowKind.blank, block: -1, firstLine: l, lineCount: 1, text: '', segments: const [], shells: shells, sourceStart: contentEnd, sourceEnd: contentEnd, contentStarts: [contentEnd], contentEnds: [contentEnd], prefixStarts: [prefixStart]));
    }
    rows.sort((a, b) => a.firstLine != b.firstLine ? a.firstLine - b.firstLine : a.sourceStart - b.sourceStart);
    final renumbered = <ProjectedRow>[];
    for (var i = 0; i < rows.length; i++) { renumbered.add(_withIndex(rows[i], i)); }
    for (final list in rowsByLine) { list.clear(); }
    for (final r in renumbered) { for (var l = r.firstLine; l < r.firstLine + r.lineCount && l < lineCount; l++) { rowsByLine[l].add(r.index); } }
    return Projection._(m, src, renumbered, rowsByLine, options);
  }

  ProjectedRow _withIndex(ProjectedRow r, int i) => ProjectedRow(index: i, kind: r.kind, block: r.block, firstLine: r.firstLine, lineCount: r.lineCount, text: r.text, segments: r.segments, shells: r.shells, sourceStart: r.sourceStart, sourceEnd: r.sourceEnd, contentStarts: r.contentStarts, contentEnds: r.contentEnds, prefixStarts: r.prefixStarts, headingLevel: r.headingLevel, fenced: r.fenced, codeInfoStart: r.codeInfoStart, codeInfoEnd: r.codeInfoEnd, tableBlock: r.tableBlock, tableRowBlock: r.tableRowBlock, column: r.column, header: r.header, alignment: r.alignment);

  void _computeStyles() {
    for (var i = 0; i < m.runCount; i++) {
      final parent = m.run(i, RunField.parent);
      final inherited = parent == noParent ? 0 : _styleOf[parent];
      final kind = m.run(i, RunField.kind);
      final own = switch (kind) { RunKind.emph => Style.emphasis, RunKind.strong => Style.strong, RunKind.code => Style.code, RunKind.strike => Style.strikethrough, RunKind.link || RunKind.autolink => Style.link, RunKind.image => Style.image, RunKind.footnoteRef => Style.footnoteRef, RunKind.htmlInline => Style.htmlInline, _ => 0 };
      _styleOf[i] = inherited | own;
      _linkOf[i] = (kind == RunKind.link || kind == RunKind.image || kind == RunKind.autolink) ? i : (parent == noParent ? -1 : _linkOf[parent]);
    }
  }

  List<Shell> _shellsFor(int container) {
    if (container < 0) return const [];
    final chain = <Shell>[];
    var b = container;
    while (b >= 0 && b != noParent) {
      final kind = m.block(b, BlockField.kind);
      if (kind == BlockKind.blockQuote) chain.add(Shell(kind: ShellKind.blockQuote, block: b));
      if (kind == BlockKind.item) {
        final flags = m.block(b, BlockField.flags);
        final task = flags & 1 != 0;
        final s = m.block(b, BlockField.attr1), e = m.block(b, BlockField.attr2);
        final list = m.block(b, BlockField.parent);
        var itemIndex = 0;
        if (list != noParent) { for (var i = list + 1; i < b; i++) { if (m.block(i, BlockField.parent) == list) itemIndex++; } }
        final ordered = list != noParent && m.block(list, BlockField.attr0) == 1;
        chain.add(Shell(kind: ShellKind.item, block: b, ordered: ordered, start: ordered ? m.block(list, BlockField.attr1) : 1, task: task, checked: task && flags & 2 != 0, checkboxStart: task && s > 0 ? _u16(s - 1) : -1, checkboxEnd: task && e > 0 ? _u16(e + 1) : -1, itemIndex: itemIndex));
      }
      if (kind == BlockKind.list) chain.add(Shell(kind: ShellKind.list, block: b, ordered: m.block(b, BlockField.attr0) == 1, start: m.block(b, BlockField.attr1), tight: m.block(b, BlockField.flags) & 1 != 0));
      if (kind == BlockKind.footnoteDefinition) chain.add(Shell(kind: ShellKind.footnoteDefinition, block: b));
      b = m.block(b, BlockField.parent);
    }
    return chain.reversed.toList(growable: false);
  }

  int _u16(int byte) {
    // UTF-16 offset of a byte offset via the line table plus a scan; only used for the rare checkbox range.
    final line = m.lineOfByte(byte);
    final lineByte = m.lineStartByte(line), lineUtf16 = m.lineStartUtf16(line);
    var u = lineUtf16, bt = lineByte;
    while (bt < byte && u < src.length) { final c = src.codeUnitAt(u); if (c >= 0xD800 && c <= 0xDBFF) { bt += 4; u += 2; } else if (c < 0x80) { bt += 1; u += 1; } else if (c < 0x800) { bt += 2; u += 1; } else { bt += 3; u += 1; } }
    return u;
  }

  /// Hidden UTF-16 intervals of a block's runs: delimiters plus break markers.
  List<(int, int)> _hiddenIntervals(int block) {
    final out = <(int, int)>[];
    for (var r = m.firstRunOfBlock(block); r < m.runCount && m.run(r, RunField.block) == block; r++) {
      final kind = m.run(r, RunField.kind);
      final s = m.run(r, RunField.startUtf16), e = m.run(r, RunField.endUtf16), cs = m.run(r, RunField.contentStartUtf16), ce = m.run(r, RunField.contentEndUtf16);
      if (kind == RunKind.softBreak || kind == RunKind.hardBreak) { if (e > s) out.add((s, e)); continue; }
      if (cs > s) out.add((s, cs));
      if (e > ce) out.add((ce, e));
    }
    out.sort((a, b) => a.$1 - b.$1);
    return out;
  }

  ProjectedRow _inlineRow(int index, int block, int kind, List<int> containerOf) {
    final first = m.block(block, BlockField.firstLine), n = m.block(block, BlockField.lineCount);
    final co = m.block(block, BlockField.contentOffset), cn = m.block(block, BlockField.contentCount);
    final hidden = _hiddenIntervals(block);
    // Text comes from runs without children (a link's or autolink's text is
    // its Text child); containers only contribute style and hidden ranges.
    final firstRun = m.firstRunOfBlock(block);
    var endRun = firstRun;
    while (endRun < m.runCount && m.run(endRun, RunField.block) == block) { endRun++; }
    final hasChildren = List<bool>.filled(endRun - firstRun, false);
    for (var r = firstRun; r < endRun; r++) { final parent = m.run(r, RunField.parent); if (parent != noParent && parent >= firstRun) { hasChildren[parent - firstRun] = true; } }
    final leafRuns = <int>[];
    for (var r = firstRun; r < endRun; r++) {
      final k = m.run(r, RunField.kind);
      if (k == RunKind.softBreak || k == RunKind.hardBreak) continue;
      if (!hasChildren[r - firstRun]) leafRuns.add(r);
    }
    final text = StringBuffer();
    final segments = <Segment>[];
    var sourceStart = -1, sourceEnd = -1;
    final starts = _lineEnds(first, n), ends = List<int>.of(starts), prefixes = List<int>.of(starts);
    for (var c = 0; c < cn; c++) {
      final rec = m.content(co + c, ContentField.line);
      final cs = m.content(co + c, ContentField.startUtf16), ce = m.content(co + c, ContentField.endUtf16);
      if (rec >= first && rec < first + n) { starts[rec - first] = cs; ends[rec - first] = ce; prefixes[rec - first] = m.content(co + c, ContentField.prefixStartUtf16); }
      if (sourceStart < 0) sourceStart = cs;
      sourceEnd = ce;
      if (c > 0) {
        final prevEnd = m.content(co + c - 1, ContentField.endUtf16);
        final d0 = text.length;
        text.write(options.softBreakAsNewline ? '\n' : ' ');
        segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: prevEnd, sourceEnd: cs, styles: 0, exact: false, lineBreak: true));
      }
      final virt = m.content(co + c, ContentField.virtualLeadingSpaces);
      if (virt > 0) { final d0 = text.length; text.write(' ' * virt); segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: cs, sourceEnd: cs, styles: 0, exact: false)); }
      var p = cs;
      void emitExact(int a, int b, int style, int run) { if (b <= a) return; final d0 = text.length; text.write(src.substring(a, b)); segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: a, sourceEnd: b, styles: style, exact: true, run: run)); }
      void emitGap(int a, int b) {
        // Content bytes no leaf run claims: hidden if inside a delimiter interval, else shown exactly.
        var q = a;
        for (final h in hidden) {
          if (h.$2 <= q) continue; if (h.$1 >= b) break;
          if (h.$1 > q) emitExact(q, h.$1, 0, -1);
          q = h.$2 > q ? h.$2 : q;
        }
        if (q < b) emitExact(q, b, 0, -1);
      }
      for (final r in leafRuns) {
        final rs = m.run(r, RunField.contentStartUtf16), re = m.run(r, RunField.contentEndUtf16);
        if (re <= cs || rs >= ce) continue;
        final a = rs < cs ? cs : rs, b = re > ce ? ce : re;
        if (a > p) emitGap(p, a);
        final k = m.run(r, RunField.kind);
        final style = _styleOf[r];
        String? override = k == RunKind.replacement ? m.string(m.run(r, RunField.aux0), m.run(r, RunField.aux1)) : (k == RunKind.code && m.run(r, RunField.flags) & 2 != 0 ? m.string(m.run(r, RunField.aux2), m.run(r, RunField.aux3)) : null);
        if (override != null && rs >= cs && re <= ce) {
          final d0 = text.length; text.write(override);
          segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: rs, sourceEnd: re, styles: style, exact: override.length == re - rs && override == src.substring(rs, re), run: r));
        } else if (b > a) {
          emitExact(a, b, style, r);
        }
        p = b > p ? b : p;
      }
      if (p < ce) emitGap(p, ce);
    }
    final shells = _shellsFor(containerOf[first]);
    switch (kind) {
      case BlockKind.heading:
        return ProjectedRow(index: index, kind: RowKind.heading, block: block, firstLine: first, lineCount: n, text: text.toString(), segments: segments, shells: shells, sourceStart: sourceStart < 0 ? m.block(block, BlockField.startUtf16) : sourceStart, sourceEnd: sourceEnd < 0 ? m.block(block, BlockField.endUtf16) : sourceEnd, contentStarts: starts, contentEnds: ends, prefixStarts: prefixes, headingLevel: m.block(block, BlockField.attr0));
      case BlockKind.tableCell:
        final rowBlock = m.block(block, BlockField.parent);
        final tableBlock = rowBlock == noParent ? -1 : m.block(rowBlock, BlockField.parent);
        var column = 0;
        for (var i = rowBlock + 1; i < block; i++) { if (m.block(i, BlockField.parent) == rowBlock) column++; }
        final packed = tableBlock < 0 ? 0 : m.block(tableBlock, BlockField.attr1);
        return ProjectedRow(index: index, kind: RowKind.tableCell, block: block, firstLine: first, lineCount: n, text: text.toString(), segments: segments, shells: shells, sourceStart: sourceStart < 0 ? m.block(block, BlockField.startUtf16) : sourceStart, sourceEnd: sourceEnd < 0 ? m.block(block, BlockField.endUtf16) : sourceEnd, contentStarts: starts, contentEnds: ends, prefixStarts: prefixes, tableBlock: tableBlock, tableRowBlock: rowBlock, column: column, header: rowBlock != noParent && m.block(rowBlock, BlockField.flags) & 1 != 0, alignment: column < 16 ? (packed >> (2 * column)) & 3 : 0);
      default:
        return ProjectedRow(index: index, kind: RowKind.paragraph, block: block, firstLine: first, lineCount: n, text: text.toString(), segments: segments, shells: shells, sourceStart: sourceStart < 0 ? m.block(block, BlockField.startUtf16) : sourceStart, sourceEnd: sourceEnd < 0 ? m.block(block, BlockField.endUtf16) : sourceEnd, contentStarts: starts, contentEnds: ends, prefixStarts: prefixes);
    }
  }

  /// Line end excluding the terminator.
  int _lineEnd(int l) {
    final start = m.lineStartUtf16(l);
    var e = l + 1 < m.lineCount ? m.lineStartUtf16(l + 1) : src.length;
    while (e > start && (src.codeUnitAt(e - 1) == 0x0A || src.codeUnitAt(e - 1) == 0x0D)) { e--; }
    return e;
  }

  List<int> _lineEnds(int first, int n) => [for (var l = first; l < first + n && l < m.lineCount; l++) _lineEnd(l)];

  ProjectedRow _literalRow(int index, int block, int kind, List<int> containerOf) {
    final first = m.block(block, BlockField.firstLine), n = m.block(block, BlockField.lineCount);
    final co = m.block(block, BlockField.contentOffset), cn = m.block(block, BlockField.contentCount);
    final text = StringBuffer();
    final segments = <Segment>[];
    var sourceStart = -1, sourceEnd = -1;
    final starts = _lineEnds(first, n), ends = List<int>.of(starts), prefixes = List<int>.of(starts);
    for (var c = 0; c < cn; c++) {
      final rec = m.content(co + c, ContentField.line);
      final cs = m.content(co + c, ContentField.startUtf16), ce = m.content(co + c, ContentField.endUtf16);
      if (rec >= first && rec < first + n) { starts[rec - first] = cs; ends[rec - first] = ce; prefixes[rec - first] = m.content(co + c, ContentField.prefixStartUtf16); }
      if (sourceStart < 0) sourceStart = cs;
      sourceEnd = ce;
      if (c > 0) { final prevEnd = m.content(co + c - 1, ContentField.endUtf16); final d0 = text.length; text.write('\n'); segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: prevEnd, sourceEnd: cs, styles: 0, exact: false, lineBreak: true)); }
      final virt = m.content(co + c, ContentField.virtualLeadingSpaces);
      if (virt > 0) { final d0 = text.length; text.write(' ' * virt); segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: cs, sourceEnd: cs, styles: 0, exact: false)); }
      if (ce > cs) { final d0 = text.length; text.write(src.substring(cs, ce)); segments.add(Segment(displayStart: d0, displayEnd: text.length, sourceStart: cs, sourceEnd: ce, styles: kind == BlockKind.codeBlock ? Style.code : 0, exact: true)); }
    }
    final flags = m.block(block, BlockField.flags);
    // Fence lines hold no caret: an edit there would be invisible. The info
    // string is a host affordance, not a caret position.
    if (kind == BlockKind.codeBlock && flags & 1 != 0 && starts.isNotEmpty) {
      starts[0] = -1; ends[0] = -1; prefixes[0] = -1;
      if (flags & 2 != 0 && starts.length > 1) { starts[starts.length - 1] = -1; ends[ends.length - 1] = -1; prefixes[prefixes.length - 1] = -1; }
    }
    return ProjectedRow(index: index, kind: kind == BlockKind.codeBlock ? RowKind.codeBlock : RowKind.htmlBlock, block: block, firstLine: first, lineCount: n, text: text.toString(), segments: segments, shells: _shellsFor(containerOf[first]), sourceStart: sourceStart < 0 ? m.block(block, BlockField.startUtf16) : sourceStart, sourceEnd: sourceEnd < 0 ? m.block(block, BlockField.endUtf16) : sourceEnd, contentStarts: starts, contentEnds: ends, prefixStarts: prefixes, fenced: kind == BlockKind.codeBlock && flags & 1 != 0, codeInfoStart: kind == BlockKind.codeBlock && flags & 1 != 0 ? _u16(m.block(block, BlockField.attr1)) : -1, codeInfoEnd: kind == BlockKind.codeBlock && flags & 1 != 0 ? _u16(m.block(block, BlockField.attr2)) : -1);
  }
}
