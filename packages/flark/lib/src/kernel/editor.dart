/// The editor facade: the one object a host talks to. It owns the document,
/// applies commands, keeps history, and reports the typing context.
library;

import 'package:characters/characters.dart';

import '../parse/backend.dart';
import '../parse/schema.g.dart';
import 'commands.dart';
import 'document.dart';
import 'history.dart';
import 'projection.dart';

typedef FlarkListener = void Function();

final class FlarkEditor {
  FlarkEditor(this._backend, {String text = '', int caret = 0, this.syncLimit = 256 * 1024, ProjectionOptions options = const ProjectionOptions()})
      : _doc = FlarkDocument.load(text, _backend, caret: caret, options: options);

  final FlarkParseBackend _backend;

  /// Source length above which the host should show source, not a projection.
  final int syncLimit;
  final History history = History();
  final List<FlarkListener> _listeners = [];
  FlarkDocument _doc;
  PendingStyle? _pending;
  Duration _now = Duration.zero;

  FlarkDocument get document => _doc;
  String get source => _doc.source;
  FlarkSelection get selection => _doc.selection;
  Projection get projection => _doc.projection;
  bool get sourceMode => _doc.source.length > syncLimit;

  /// Style bits the next typed character takes: the caret anchor's context,
  /// flipped by any pending formatting command.
  int get typingContext => _doc.typingContextAt(selection.extent) ^ (_pending?.styles ?? 0);

  void addListener(FlarkListener listener) => _listeners.add(listener);
  void removeListener(FlarkListener listener) => _listeners.remove(listener);

  /// Apply one command. Returns whether anything changed. [at] is the
  /// command's time, used only for history coalescing.
  bool apply(FlarkCommand command, {Duration? at}) {
    if (at != null) _now = at;
    final applied = switch (command) {
      InsertText(:final text) => _insert(text, typing: true),
      Paste(:final text) => _insert(text, typing: false),
      DeleteBackward() => _delete(backward: true),
      DeleteForward() => _delete(backward: false),
      Newline(:final paragraph) => _newline(paragraph),
      ReplaceRange(:final start, :final end, :final text) => _replace(start, end, text),
      SetSelection(:final base, :final extent) => _select(FlarkSelection(base, extent)),
      PlaceCaret(:final row, :final offset, :final leadingHalf, :final extend) => _place(row, offset, leadingHalf, extend),
      MoveCaret(:final direction, :final unit, :final extend) => _move(direction, unit, extend),
      Undo() => _undo(),
      Redo() => _redo(),
      ToggleTask() => _toggleTask(),
      ToggleStyle(:final style) => _toggleStyle(style),
      SetHeadingLevel(:final level) => _setHeading(level),
      Indent() || Outdent() => false,
    };
    if (applied) { for (final l in List.of(_listeners)) { l(); } }
    return applied;
  }

  /// Record the state before, replace the source, and set the typing intent
  /// the command leaves behind.
  void _commit(String newSource, FlarkSelection sel, {required bool typing, PendingStyle? pending}) {
    history.record(_doc, pending: _pending, typing: typing, at: _now);
    _doc = _doc.withSource(newSource, sel, _backend);
    _pending = pending;
  }

  bool _select(FlarkSelection s) {
    final next = _doc.withSelection(s);
    if (next.selection == selection) return false;
    _pending = null;
    _doc = next;
    return true;
  }

  // ------------------------------------------------------------- inline

  bool _insert(String text, {required bool typing}) {
    if (text.isEmpty) return false;
    final sel = selection;
    var inserted = text;
    var caret = sel.start + text.length;
    final p = _pending;
    // A pending style wraps the next ordinary character; whitespace exits it.
    if (p != null && sel.isCollapsed && text.trim().isNotEmpty) { inserted = '${p.open}$text${p.close}'; caret = sel.start + p.open.length + text.length; }
    _commit(source.replaceRange(sel.start, sel.end, inserted), FlarkSelection.collapsed(caret), typing: typing && text != '\n' && text.characters.length == 1);
    return true;
  }

  bool _replace(int start, int end, String text) {
    final s = start.clamp(0, source.length), e = end.clamp(s, source.length);
    if (s == e && text.isEmpty) return false;
    _commit(source.replaceRange(s, e, text), FlarkSelection.collapsed(s + text.length), typing: false);
    return true;
  }

  bool _delete({required bool backward}) {
    final sel = selection;
    if (!sel.isCollapsed) { _commit(source.replaceRange(sel.start, sel.end, ''), FlarkSelection.collapsed(sel.start), typing: false); return true; }
    final pos = _doc.displayOf(sel.extent);
    final row = projection.rows[pos.row];
    final d = pos.offset;
    if (backward ? d == 0 : d >= row.text.length) return backward ? _joinBackward(row) : _joinForward(row);
    // The rendered grapheme's own source bytes, hidden neighbours excluded.
    final int a, b;
    if (backward) {
      final g = row.text.substring(0, d).characters.last.length;
      a = row.sourceForDisplay(d - g, anchor: Anchor.after);
      b = row.sourceForDisplay(d, anchor: Anchor.before);
    } else {
      final g = row.text.substring(d).characters.first.length;
      a = row.sourceForDisplay(d, anchor: Anchor.after);
      b = row.sourceForDisplay(d + g, anchor: Anchor.before);
    }
    if (b <= a) return false;
    // EP1-DELETE-TO-EMPTY: an owner this deletion empties goes with it, and
    // its delimiters wait for the next ordinary character.
    var s = a, e = b, styles = 0, recreate = true;
    for (var grew = true; grew;) {
      grew = false;
      for (final o in _doc.ownersOfContent(s, e)) {
        if (o.start < s || o.end > e) { s = o.start < s ? o.start : s; e = o.end > e ? o.end : e; styles |= o.style; if (o.kind == RunKind.escape) recreate = false; grew = true; }
      }
    }
    final next = recreate && e - s > b - a ? PendingStyle(source.substring(s, a), source.substring(b, e), styles) : null;
    _commit(source.replaceRange(s, e, ''), FlarkSelection.collapsed(s), typing: true, pending: next);
    return true;
  }

  // ---------------------------------------------------------- structure

  /// Backspace at a row start: lift the line's innermost prefix, else join
  /// the previous row.
  bool _joinBackward(ProjectedRow row) {
    final prefixStart = row.prefixStarts.first, contentStart = row.contentStarts.first;
    if (prefixStart < contentStart) {
      // A lifted line that would lazily continue the previous paragraph
      // stays a paragraph of its own.
      final prev = row.index > 0 ? projection.rows[row.index - 1] : null;
      final separate = row.text.isNotEmpty && prev != null && prev.kind == RowKind.paragraph && prev.firstLine + prev.lineCount == row.firstLine;
      _commit(source.replaceRange(prefixStart, contentStart, separate ? '\n' : ''), FlarkSelection.collapsed(separate ? prefixStart + 1 : prefixStart), typing: false);
      return true;
    }
    if (row.index == 0) return false;
    final prev = projection.rows[row.index - 1];
    if (prev.firstLine + prev.lineCount > row.firstLine) return false;
    final from = prev.contentEnds.last, to = row.sourceStart;
    if (to <= from) return false;
    _commit(source.replaceRange(from, to, ''), FlarkSelection.collapsed(from), typing: false);
    return true;
  }

  /// Delete at a row end: join the next row onto this one.
  bool _joinForward(ProjectedRow row) {
    if (row.index + 1 >= projection.rows.length) return false;
    final next = projection.rows[row.index + 1];
    if (next.firstLine < row.firstLine + row.lineCount) return false;
    final from = row.contentEnds.last, to = next.contentStarts.first;
    if (to <= from) return false;
    _commit(source.replaceRange(from, to, ''), FlarkSelection.collapsed(from), typing: false);
    return true;
  }

  bool _newline(bool paragraph) {
    final sel = selection;
    if (!sel.isCollapsed) { _commit(source.replaceRange(sel.start, sel.end, '\n'), FlarkSelection.collapsed(sel.start + 1), typing: false); return true; }
    final caret = sel.extent;
    final pos = _doc.displayOf(caret);
    final row = projection.rows[pos.row];
    final line = _doc.model.lineOfUtf16(caret);
    final i = (line - row.firstLine).clamp(0, row.contentStarts.length - 1);
    String text;
    if (row.shells.isNotEmpty) {
      final prefixStart = row.prefixStarts[i], contentStart = row.contentStarts[i];
      // Return on an empty container line exits the container.
      if (row.text.isEmpty && prefixStart < contentStart) { _commit(source.replaceRange(prefixStart, contentStart, ''), FlarkSelection.collapsed(prefixStart), typing: false); return true; }
      final inner = row.shells.last;
      text = '\n${inner.kind == ShellKind.item ? _nextMarker(inner) : source.substring(_doc.model.lineStartUtf16(line), contentStart)}';
    } else {
      text = paragraph && row.kind == RowKind.paragraph ? '\n\n' : '\n';
    }
    _commit(source.replaceRange(caret, caret, text), FlarkSelection.collapsed(caret + text.length), typing: false);
    return true;
  }

  /// The marker line for the item after [item]: the same outer prefixes,
  /// the next number for ordered lists, an unchecked box for tasks.
  String _nextMarker(Shell item) {
    final m = _doc.model;
    final itemLine = m.block(item.block, BlockField.firstLine), itemStart = m.block(item.block, BlockField.startUtf16);
    var contentStart = itemStart;
    for (final r in projection.rowsOnLine(itemLine)) {
      final row = projection.rows[r];
      if (row.firstLine == itemLine && row.prefixStarts.first == itemStart) { contentStart = row.contentStarts.first; break; }
    }
    final outer = source.substring(m.lineStartUtf16(itemLine), itemStart);
    final markerEnd = item.task ? item.checkboxStart : contentStart;
    var marker = source.substring(itemStart, markerEnd);
    if (item.ordered) {
      final delimiter = marker.trimRight();
      marker = '${item.start + item.itemIndex + 1}${delimiter.substring(delimiter.length - 1)} ';
    } else if (marker.trimRight() == marker) {
      marker = '$marker ';
    }
    if (item.task) marker = '$marker[ ] ';
    return outer + marker;
  }

  bool _toggleTask() {
    for (final sh in _doc.rowAt(selection.extent).shells.reversed) {
      if (sh.kind != ShellKind.item || !sh.task) continue;
      _commit(source.replaceRange(sh.checkboxStart + 1, sh.checkboxEnd - 1, sh.checked ? ' ' : 'x'), selection, typing: false);
      return true;
    }
    return false;
  }

  bool _setHeading(int level) {
    if (level < 0 || level > 6) return false;
    final row = _doc.rowAt(selection.extent);
    if (row.kind != RowKind.paragraph && row.kind != RowKind.heading) return false;
    if (row.kind == RowKind.heading && row.lineCount > 1) return false;
    final m = _doc.model;
    final blockStart = m.block(row.block, BlockField.startUtf16), blockEnd = m.block(row.block, BlockField.endUtf16);
    final prefix = level == 0 ? '' : '${'#' * level} ';
    var s = source;
    if (level == 0 && blockEnd > row.sourceEnd) s = s.replaceRange(row.sourceEnd, blockEnd, '');
    s = s.replaceRange(blockStart, row.sourceStart, prefix);
    final shift = prefix.length - (row.sourceStart - blockStart);
    int move(int o) => o >= row.sourceStart ? o + shift : o;
    _commit(s, FlarkSelection(move(selection.base), move(selection.extent)), typing: false);
    return true;
  }

  bool _toggleStyle(int style) {
    final delimiter = switch (style) { Style.emphasis => '*', Style.strong => '**', Style.strikethrough => '~~', Style.code => '`', _ => null };
    if (delimiter == null) return false;
    final sel = selection;
    if (!sel.isCollapsed) {
      for (final o in _doc.ownersOfContent(sel.start, sel.end)) {
        if (o.style != style) continue;
        final s = source.replaceRange(o.contentEnd, o.end, '').replaceRange(o.start, o.contentStart, '');
        _commit(s, FlarkSelection(o.start, o.start + (o.contentEnd - o.contentStart)), typing: false);
        return true;
      }
      _commit(source.replaceRange(sel.start, sel.end, '$delimiter${source.substring(sel.start, sel.end)}$delimiter'), FlarkSelection(sel.start + delimiter.length, sel.end + delimiter.length), typing: false);
      return true;
    }
    final caret = sel.extent;
    // At an edge of an owner, step across its delimiter: out when inside,
    // in when outside. Strictly inside, unwrap it.
    for (final o in _doc.ownersAt(caret)) {
      if (o.style != style) continue;
      if (caret == o.contentEnd) return _select(FlarkSelection.collapsed(o.end));
      if (caret == o.contentStart) return _select(FlarkSelection.collapsed(o.start));
      final s = source.replaceRange(o.contentEnd, o.end, '').replaceRange(o.start, o.contentStart, '');
      _commit(s, FlarkSelection.collapsed(caret - (o.contentStart - o.start)), typing: false);
      return true;
    }
    for (final o in _doc.ownersTouching(caret)) {
      if (o.style != style) continue;
      return _select(FlarkSelection.collapsed(caret == o.end ? o.contentEnd : o.contentStart));
    }
    final p = _pending;
    _pending = p != null && p.styles == style ? null : PendingStyle(delimiter, delimiter, style);
    return true;
  }

  // ------------------------------------------------------------- caret

  /// The anchor for display offset [d] of [row] when arriving from the
  /// direction given: the caret keeps the context it came from, so it
  /// changes only by crossing a glyph of another style.
  int _anchorFor(ProjectedRow row, int d, {required bool forward}) {
    final anchors = _doc.anchorsAt(row.sourceForDisplay(d, anchor: forward ? Anchor.before : Anchor.after));
    return forward ? anchors.first : anchors.last;
  }

  ProjectedRow? _rowAfter(int index, {required bool forward}) {
    final i = forward ? index + 1 : index - 1;
    return i < 0 || i >= projection.rows.length ? null : projection.rows[i];
  }

  bool _move(MoveDirection direction, MoveUnit unit, bool extend) {
    final sel = selection;
    final forward = direction == MoveDirection.forward;
    final cur = sel.extent;
    final pos = _doc.displayOf(cur);
    final row = projection.rows[pos.row];
    final d = pos.offset;
    int target;
    if (!extend && !sel.isCollapsed && unit == MoveUnit.grapheme) {
      target = forward ? sel.end : sel.start;
    } else {
      switch (unit) {
        case MoveUnit.grapheme:
          target = _step(row, d, forward: forward, cur: cur);
        case MoveUnit.word:
          final t = forward ? _wordEnd(row.text, d) : _wordStart(row.text, d);
          target = t == d ? _step(row, d, forward: forward, cur: cur) : _anchorFor(row, t, forward: forward);
        case MoveUnit.line:
          // Row edges take the outermost anchor, so the caret leaves a span there.
          final anchors = _doc.anchorsAt(row.sourceForDisplay(forward ? row.text.length : 0, anchor: forward ? Anchor.before : Anchor.after));
          target = forward ? anchors.last : anchors.first;
        case MoveUnit.row:
          final other = _rowAfter(row.index, forward: forward);
          if (other == null) return false;
          target = _anchorFor(other, d.clamp(0, other.text.length), forward: forward);
      }
    }
    return _select(extend ? FlarkSelection(sel.base, target) : FlarkSelection.collapsed(target));
  }

  int _step(ProjectedRow row, int d, {required bool forward, required int cur}) {
    if (forward) {
      if (d < row.text.length) return _anchorFor(row, d + row.text.substring(d).characters.first.length, forward: true);
      final next = _rowAfter(row.index, forward: true);
      return next == null ? _doc.anchorsAt(cur).last : _anchorFor(next, 0, forward: true);
    }
    if (d > 0) return _anchorFor(row, d - row.text.substring(0, d).characters.last.length, forward: false);
    final prev = _rowAfter(row.index, forward: false);
    return prev == null ? _doc.anchorsAt(cur).first : _anchorFor(prev, prev.text.length, forward: false);
  }

  static bool _isSpace(String text, int i) => text.codeUnitAt(i) == 0x20 || text.codeUnitAt(i) == 0x0A || text.codeUnitAt(i) == 0x09;

  static int _wordEnd(String text, int d) {
    var i = d;
    while (i < text.length && _isSpace(text, i)) { i++; }
    while (i < text.length && !_isSpace(text, i)) { i++; }
    return i;
  }

  static int _wordStart(String text, int d) {
    var i = d;
    while (i > 0 && _isSpace(text, i - 1)) { i--; }
    while (i > 0 && !_isSpace(text, i - 1)) { i--; }
    return i;
  }

  bool _place(int rowIndex, int offset, bool leadingHalf, bool extend) {
    if (projection.rows.isEmpty) return false;
    final row = projection.rows[rowIndex.clamp(0, projection.rows.length - 1)];
    // The half of the glyph hit says which side of a boundary was meant.
    final anchors = _doc.anchorsAt(row.sourceForDisplay(offset, anchor: leadingHalf ? Anchor.after : Anchor.before));
    final target = leadingHalf ? anchors.last : anchors.first;
    return _select(extend ? FlarkSelection(selection.base, target) : FlarkSelection.collapsed(target));
  }

  // ----------------------------------------------------------- history

  bool _undo() {
    final e = history.undo(_doc, _pending);
    if (e == null) return false;
    _doc = _doc.withSource(e.source, e.selection, _backend);
    _pending = e.pending;
    return true;
  }

  bool _redo() {
    final e = history.redo(_doc, _pending);
    if (e == null) return false;
    _doc = _doc.withSource(e.source, e.selection, _backend);
    _pending = e.pending;
    return true;
  }
}
