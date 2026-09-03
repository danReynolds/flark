/// The document: source, selection, and the derived model and projection.
library;

import '../parse/backend.dart';
import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'projection.dart';

/// A selection in source UTF-16 offsets. Collapsed when base == extent.
/// Offsets are always legal caret positions: never strictly inside a hidden
/// range, always on a row's content. Which of several legal offsets a display
/// position holds is the typing context, so the offset alone encodes the
/// caret's anchor.
final class FlarkSelection {
  const FlarkSelection(this.base, this.extent);
  const FlarkSelection.collapsed(int offset) : base = offset, extent = offset;
  final int base;
  final int extent;
  bool get isCollapsed => base == extent;
  int get start => base < extent ? base : extent;
  int get end => base < extent ? extent : base;
  @override
  bool operator ==(Object other) => other is FlarkSelection && other.base == base && other.extent == extent;
  @override
  int get hashCode => Object.hash(base, extent);
  @override
  String toString() => isCollapsed ? 'caret $base' : 'selection $base..$extent';
}

/// An inline run that owns content: its full range and its content range.
final class Owner {
  const Owner(this.run, this.kind, this.start, this.end, this.contentStart, this.contentEnd);
  final int run, kind, start, end, contentStart, contentEnd;
  int get style => switch (kind) {
        RunKind.emph => Style.emphasis,
        RunKind.strong => Style.strong,
        RunKind.strike => Style.strikethrough,
        RunKind.code => Style.code,
        RunKind.link => Style.link,
        RunKind.image => Style.image,
        _ => 0,
      };
}

final class FlarkDocument {
  FlarkDocument._(this.source, this.selection, this.model, this.projection, this.normalizedLineEndings);

  /// Parse and project [text]. Bare CR line endings are normalized to LF
  /// first (they are outside the parser's fidelity contract) and the
  /// document records that it did so.
  factory FlarkDocument.load(String text, FlarkParseBackend backend, {int caret = 0, ProjectionOptions options = const ProjectionOptions()}) {
    final normalized = text.contains('\r') ? text.replaceAll('\r\n', '\n').replaceAll('\r', '\n') : text;
    final model = backend.parse(normalized);
    final doc = FlarkDocument._(normalized, const FlarkSelection.collapsed(0), model, Projection.of(model, normalized, options: options), normalized != text);
    return doc.withSelection(FlarkSelection.collapsed(caret));
  }

  final String source;
  final FlarkSelection selection;
  final RenderModel model;
  final Projection projection;
  final bool normalizedLineEndings;

  /// The same document with [source] replaced: one parse, one projection.
  FlarkDocument withSource(String newSource, FlarkSelection newSelection, FlarkParseBackend backend) {
    final model = backend.parse(newSource);
    final doc = FlarkDocument._(newSource, const FlarkSelection.collapsed(0), model, Projection.of(model, newSource, options: projection.options), normalizedLineEndings);
    return doc.withSelection(newSelection);
  }

  /// The same document with a selection whose ends are made legal.
  FlarkDocument withSelection(FlarkSelection s) => FlarkDocument._(source, FlarkSelection(legalize(s.base), legalize(s.extent)), model, projection, normalizedLineEndings);

  // ------------------------------------------------------------ positions

  /// Hidden intervals of inline runs (delimiters, break markers) in UTF-16,
  /// sorted by start.
  late final List<(int, int)> hiddenIntervals = () {
    final out = <(int, int)>[];
    var sorted = true, last = -1;
    void add(int a, int b) { if (a < last) sorted = false; last = a; out.add((a, b)); }
    for (var r = 0; r < model.runCount; r++) {
      final k = model.run(r, RunField.kind);
      final s = model.run(r, RunField.startUtf16), e = model.run(r, RunField.endUtf16);
      if (k == RunKind.softBreak || k == RunKind.hardBreak) { if (e > s) add(s, e); continue; }
      final cs = model.run(r, RunField.contentStartUtf16), ce = model.run(r, RunField.contentEndUtf16);
      if (cs > s) add(s, cs);
      if (e > ce) add(ce, e);
    }
    if (!sorted) out.sort((a, b) => a.$1 - b.$1);
    return out;
  }();

  bool _insideHidden(int o) {
    for (final h in hiddenIntervals) { if (h.$1 >= o) break; if (o < h.$2) return true; }
    return false;
  }

  /// Whether a source offset is a legal caret position.
  bool isLegal(int offset) {
    if (offset < 0 || offset > source.length || _insideHidden(offset)) return false;
    if (!projection.hasCaretSpans) return offset == source.length;
    for (final (s, e) in projection.lineSpans(model.lineOfUtf16(offset))) { if (offset >= s && offset <= e) return true; }
    return false;
  }

  /// The nearest legal offset to [offset]: out of a hidden interval
  /// forwards, then onto the closest caret span of its line, else the first
  /// span of a following line or the last span of a preceding one.
  int legalize(int offset) {
    var o = offset.clamp(0, source.length);
    if (isLegal(o)) return o;
    for (final h in hiddenIntervals) { if (h.$1 >= o) break; if (o < h.$2) o = h.$2; }
    if (isLegal(o)) return o;
    final line = model.lineOfUtf16(o);
    int? best;
    var bestDistance = 1 << 40;
    for (final (s, e) in projection.lineSpans(line)) {
      if (o >= s && o <= e) return o;
      for (final c in [s, e]) { final d = (c - o).abs(); if (d < bestDistance) { bestDistance = d; best = c; } }
    }
    if (best != null) return best;
    for (var l = line + 1; l < model.lineCount; l++) { final sp = projection.lineSpans(l); if (sp.isNotEmpty) return sp.first.$1; }
    for (var l = line - 1; l >= 0; l--) { final sp = projection.lineSpans(l); if (sp.isNotEmpty) return sp.last.$2; }
    return source.length;
  }

  /// Display position of a source offset.
  DisplayPosition displayOf(int offset) => projection.displayForSource(legalize(offset)) ?? const DisplayPosition(0, 0);

  /// Every legal offset displayed at the same place as [offset], ascending:
  /// the caret's possible anchors there. Adjacent legal offsets separated
  /// only by hidden bytes share a display position.
  List<int> anchorsAt(int offset) {
    final o = legalize(offset);
    final pos = displayOf(o);
    final seen = <int>{o};
    final queue = [o];
    while (queue.isNotEmpty) {
      final x = queue.removeLast();
      for (final h in hiddenIntervals) {
        if (h.$2 == x && seen.add(h.$1)) queue.add(h.$1);
        if (h.$1 == x && seen.add(h.$2)) queue.add(h.$2);
      }
    }
    final out = <int>[];
    for (final x in seen) {
      if (!isLegal(x)) continue;
      final d = displayOf(x);
      if (d.row == pos.row && d.offset == pos.offset) out.add(x);
    }
    out.sort();
    return out;
  }

  static bool _owns(int kind) => kind == RunKind.emph || kind == RunKind.strong || kind == RunKind.strike || kind == RunKind.code || kind == RunKind.link || kind == RunKind.image || kind == RunKind.escape;

  Owner _owner(int r) => Owner(r, model.run(r, RunField.kind), model.run(r, RunField.startUtf16), model.run(r, RunField.endUtf16), model.run(r, RunField.contentStartUtf16), model.run(r, RunField.contentEndUtf16));

  /// The runs of the leaf block projected at [offset]: (first, end).
  (int, int) _runsNear(int offset) {
    final row = rowAt(offset);
    if (row.block < 0) return (0, 0);
    final first = model.firstRunOfBlock(row.block);
    var end = first;
    while (end < model.runCount && model.run(end, RunField.block) == row.block) { end++; }
    return (first, end);
  }

  /// Styled owners whose content contains [offset], outermost first. An
  /// offset at a content edge counts as inside: that is what makes the
  /// anchor the typing context.
  List<Owner> ownersAt(int offset) {
    final (first, end) = _runsNear(offset);
    return [for (var r = first; r < end; r++) if (_owns(model.run(r, RunField.kind)) && model.run(r, RunField.kind) != RunKind.escape && offset >= model.run(r, RunField.contentStartUtf16) && offset <= model.run(r, RunField.contentEndUtf16)) _owner(r)];
  }

  /// Owners whose content is exactly [start, end): emptied by deleting it.
  List<Owner> ownersOfContent(int start, int end) {
    final (first, last) = _runsNear(start);
    return [for (var r = first; r < last; r++) if (_owns(model.run(r, RunField.kind)) && model.run(r, RunField.contentStartUtf16) == start && model.run(r, RunField.contentEndUtf16) == end) _owner(r)];
  }

  /// Owners whose full range starts or ends at [offset]: adjacent from outside.
  List<Owner> ownersTouching(int offset) {
    final (first, end) = _runsNear(offset);
    return [for (var r = first; r < end; r++) if (_owns(model.run(r, RunField.kind)) && model.run(r, RunField.kind) != RunKind.escape && (model.run(r, RunField.startUtf16) == offset || model.run(r, RunField.endUtf16) == offset)) _owner(r)];
  }

  /// Style bits the next keystroke at [offset] inherits.
  int typingContextAt(int offset) {
    var mask = 0;
    for (final o in ownersAt(offset)) { mask |= o.style; }
    return mask;
  }

  /// The row holding a source offset.
  ProjectedRow rowAt(int offset) => projection.rows[displayOf(offset).row];
}
