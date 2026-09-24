/// The document: source, selection, and the derived model and projection.
library;

import 'package:characters/characters.dart';

import '../parse/backend.dart';
import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'projection.dart';
import 'resource.dart';

/// A selection in source UTF-16 offsets. Collapsed when base == extent.
/// Ordinary endpoints are legal caret positions: never strictly inside a hidden
/// range, always on a row's content. Which of several legal offsets a display
/// position holds is the typing context. Unwritten table cells additionally
/// carry a projected cell address because they share a source boundary.
/// A noncollapsed 0..source.length range explicitly selects the
/// whole source, including block syntax outside the first/last caret spans.
final class FlarkSelection {
  const FlarkSelection(this.base, this.extent) : tableCell = null;
  const FlarkSelection.collapsed(int offset, {this.tableCell})
    : base = offset,
      extent = offset;

  /// Projected row index for an unwritten table cell. Valid only for this
  /// source snapshot; hosts must use PlaceCaret instead of inventing indexes.
  /// It distinguishes empty cells that share the same source boundary.
  final int? tableCell;
  final int base;
  final int extent;
  bool get isCollapsed => base == extent;
  int get start => base < extent ? base : extent;
  int get end => base < extent ? extent : base;
  @override
  bool operator ==(Object other) =>
      other is FlarkSelection &&
      other.base == base &&
      other.extent == extent &&
      other.tableCell == tableCell;
  @override
  int get hashCode => Object.hash(base, extent, tableCell);
  @override
  String toString() => isCollapsed ? 'caret $base' : 'selection $base..$extent';
}

/// An inline run that owns content: its full range and its content range.
final class Owner {
  const Owner(
    this.run,
    this.kind,
    this.start,
    this.end,
    this.contentStart,
    this.contentEnd,
  );
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
  FlarkDocument._(
    this.source,
    this.selection,
    this.model,
    this.projection,
    this.normalizedLineEndings, {
    FlarkDocument? positions,
  }) : _positions = positions;

  // Selection-only snapshots share the source's positional index. Parsing a
  // new source creates a fresh index; changing an anchor must not rebuild it.
  final FlarkDocument? _positions;

  /// Parse and project [text], preserving every source code unit.
  ///
  /// CRLF is supported as-is. Bare CR and malformed UTF-16 are outside the
  /// source contract and are rejected before the parser sees them.
  factory FlarkDocument.load(
    String text,
    FlarkParseBackend backend, {
    int caret = 0,
    ProjectionOptions options = const ProjectionOptions(),
  }) {
    validateFlarkSource(text);
    final model = backend.parse(text);
    final doc = FlarkDocument._(
      text,
      const FlarkSelection.collapsed(0),
      model,
      Projection.of(model, text, options: options),
      false,
    );
    return doc.withSelection(FlarkSelection.collapsed(caret));
  }

  final String source;
  final FlarkSelection selection;
  final RenderModel model;
  final Projection projection;
  final bool normalizedLineEndings;

  late final List<InlineResource> resources =
      _positions?.resources ??
      List.unmodifiable([
        for (final run in model.runs)
          if (run.kind == RunKind.link ||
              run.kind == RunKind.autolink ||
              run.kind == RunKind.image)
            InlineResource.fromRun(this, run),
      ]);

  /// Image-only view keeps ordinary layout from materializing link metadata.
  late final List<InlineResource> images =
      _positions?.images ??
      List.unmodifiable([
        for (final run in model.runs)
          if (run.kind == RunKind.image) InlineResource.fromRun(this, run),
      ]);

  /// Innermost resource containing the selected content, of the requested kind.
  InlineResource? resourceAt(FlarkSelection selection, {required bool image}) {
    if (selection.tableCell != null) return null;
    for (final resource in resources.reversed) {
      if (resource.isImage == image &&
          resource.start <= selection.start &&
          selection.end <= resource.end) {
        return resource;
      }
    }
    return null;
  }

  /// Display text of a source range, derived entirely from the projection.
  String visibleText(int start, int end) {
    final first = displayOf(start), last = displayOf(end);
    return [
      for (var i = first.row; i <= last.row; i++)
        projection.rows[i].text.substring(
          i == first.row ? first.offset : 0,
          i == last.row ? last.offset : projection.rows[i].text.length,
        ),
    ].join('\n');
  }

  /// The same document with [source] replaced: one parse, one projection.
  FlarkDocument withSource(
    String newSource,
    FlarkSelection newSelection,
    FlarkParseBackend backend,
  ) {
    validateFlarkSource(newSource);
    final model = backend.parse(newSource);
    final doc = FlarkDocument._(
      newSource,
      const FlarkSelection.collapsed(0),
      model,
      Projection.of(model, newSource, options: projection.options),
      normalizedLineEndings,
    );
    return doc.withSelection(newSelection);
  }

  /// Preserve an explicit whole-source range; legalize ordinary caret endpoints.
  FlarkDocument withSelection(FlarkSelection s) => FlarkDocument._(
    source,
    !s.isCollapsed && s.start == 0 && s.end == source.length
        ? s
        : projection.isMissingCell(s.tableCell) &&
              s.isCollapsed &&
              projection.rows[s.tableCell!].sourceStart == s.extent
        ? s
        : FlarkSelection(legalize(s.base), legalize(s.extent)),
    model,
    projection,
    normalizedLineEndings,
    positions: _positions ?? this,
  );

  /// The caret retains an unwritten cell address separately from its source.
  DisplayPosition get caretPosition => projection.displayForSource(
    selection.extent,
    tableCell: selection.tableCell,
  )!;
  ProjectedRow get caretRow => projection.rows[caretPosition.row];

  // ------------------------------------------------------------ positions

  // Interval sets below are flat `[start, end, start, end, ...]` lists sorted
  // by start. They are rebuilt for every new source, so they avoid sorting
  // and per-interval records: sorting the whole document's intervals was a
  // fifth of a keystroke on a dense 32 KiB document.

  /// Hidden intervals of inline runs (delimiters, break markers), and the
  /// escapes whose backslash and escaped character are one caret unit.
  /// Runs arrive in document order and nest, so openings are already sorted
  /// and each closing is emitted once the walk passes its start.
  late final ({List<int> hidden, List<int> escapes}) _runPairs = () {
    if (_positions != null) return _positions._runPairs;
    final hidden = <int>[], escapes = <int>[], pending = <int>[];
    var sorted = true;
    void emit(int a, int b) {
      if (hidden.isNotEmpty && a < hidden[hidden.length - 2]) sorted = false;
      hidden
        ..add(a)
        ..add(b);
    }

    void flush(int position) {
      while (pending.isNotEmpty && pending[pending.length - 2] <= position) {
        final b = pending.removeLast(), a = pending.removeLast();
        emit(a, b);
      }
    }

    for (var r = 0; r < model.runCount; r++) {
      final s = model.runStart(r), e = model.runEnd(r);
      final cs = model.runContentStart(r), ce = model.runContentEnd(r);
      flush(s);
      if (cs > s) emit(s, cs);
      if (e > ce) {
        pending
          ..add(ce)
          ..add(e);
      }
      if (model.runKind(r) == RunKind.escape && ce > s) {
        escapes
          ..add(s)
          ..add(ce);
      }
    }
    flush(source.length);
    // Runs that overlap without nesting would break the order; sort then.
    return (hidden: sorted ? hidden : _sortPairs(hidden), escapes: escapes);
  }();

  /// Hidden intervals of inline runs (delimiters, break markers) in UTF-16,
  /// sorted by start.
  late final List<(int, int)> hiddenIntervals =
      _positions?.hiddenIntervals ??
      List<(int, int)>.unmodifiable([
        for (var i = 0; i < _runPairs.hidden.length; i += 2)
          (_runPairs.hidden[i], _runPairs.hidden[i + 1]),
      ]);

  /// Source ranges represented by one non-exact display glyph (normalized
  /// code spans, replacements, line breaks) and displayed graphemes joined
  /// across segments are atomic caret units. Escapes are in [_runPairs].
  late final List<int> _rowAtomicPairs = () {
    if (_positions != null) return _positions._rowAtomicPairs;
    final pairs = <int>[];
    var sorted = true;
    void add(int a, int b) {
      final n = pairs.length;
      if (n > 0 &&
          (a < pairs[n - 2] || a == pairs[n - 2] && b < pairs[n - 1])) {
        sorted = false;
      }
      pairs
        ..add(a)
        ..add(b);
    }

    for (final row in projection.rows) {
      for (final segment in row.segments) {
        if (!segment.exact && segment.sourceEnd > segment.sourceStart) {
          add(segment.sourceStart, segment.sourceEnd);
        }
      }
      // Hidden syntax can separate source graphemes that become one displayed
      // grapheme, such as *a* followed by a combining accent. Keep the whole
      // displayed unit atomic, including any replacement it intersects. Only
      // a grapheme crossing a segment boundary can be joined that way, so
      // inspect the boundaries rather than every grapheme of the document. No
      // grapheme rule joins two code units below U+0300 except CR LF.
      final text = row.text, segments = row.segments;
      var handled = 0;
      for (var i = 0; i < segments.length; i++) {
        final boundary = segments[i].displayEnd;
        if (boundary <= 0 || boundary >= text.length || boundary < handled) {
          continue;
        }
        final before = text.codeUnitAt(boundary - 1),
            after = text.codeUnitAt(boundary);
        if (before < 0x300 &&
            after < 0x300 &&
            (before != 0x0D || after != 0x0A)) {
          continue;
        }
        final range = CharacterRange.at(text, boundary);
        if (range.isEmpty) continue;
        final offset = range.stringBeforeLength;
        final end = offset + range.current.length;
        handled = end;
        var firstIndex = i;
        while (firstIndex > 0 && segments[firstIndex - 1].displayEnd > offset) {
          firstIndex--;
        }
        var lastIndex = firstIndex;
        while (segments[lastIndex].displayEnd < end) {
          lastIndex++;
        }
        final first = segments[firstIndex], last = segments[lastIndex];
        final startSource = first.exact
            ? first.sourceStart + offset - first.displayStart
            : first.sourceStart;
        final endSource = last.exact
            ? last.sourceStart + end - last.displayStart
            : last.sourceEnd;
        if (endSource > startSource) add(startSource, endSource);
      }
    }
    // Definition rows come before leaf rows and blank rows after them.
    return sorted ? pairs : _sortPairs(pairs);
  }();

  /// [pairs] sorted by start, then end.
  static List<int> _sortPairs(List<int> pairs) {
    final records = [
      for (var i = 0; i < pairs.length; i += 2) (pairs[i], pairs[i + 1]),
    ]..sort((a, b) => a.$1 != b.$1 ? a.$1 - b.$1 : a.$2 - b.$2);
    return [
      for (final (a, b) in records) ...[a, b],
    ];
  }

  /// Merge sorted pair lists into one sorted list, joining intervals that
  /// overlap, or that also touch when [joinTouching].
  static List<int> _mergePairs(
    List<List<int>> sources, {
    required bool joinTouching,
  }) {
    final heads = List<int>.filled(sources.length, 0);
    final merged = <int>[];
    while (true) {
      var pick = -1;
      for (var k = 0; k < sources.length; k++) {
        final i = heads[k], source = sources[k];
        if (i >= source.length) continue;
        if (pick < 0) {
          pick = k;
          continue;
        }
        final j = heads[pick], best = sources[pick];
        if (source[i] < best[j] ||
            source[i] == best[j] && source[i + 1] < best[j + 1]) {
          pick = k;
        }
      }
      if (pick < 0) return merged;
      final source = sources[pick], i = heads[pick];
      heads[pick] = i + 2;
      final a = source[i], b = source[i + 1];
      final n = merged.length;
      if (n > 0 && (joinTouching ? a <= merged[n - 1] : a < merged[n - 1])) {
        if (b > merged[n - 1]) merged[n - 1] = b;
      } else {
        merged
          ..add(a)
          ..add(b);
      }
    }
  }

  /// Legal source offsets, restricted to Unicode grapheme boundaries and to
  /// the edges of non-exact projection replacements.
  late final List<int> _legalOffsets = () {
    if (_positions != null) return _positions._legalOffsets;
    // This is a linear merge over three ordered interval streams. Calling an
    // interval scan for every grapheme made dense 64 KiB documents quadratic.
    final blocked = _blockedPairs;
    final caretSpans = projection.hasCaretSpans
        ? _mergePairs([
            _sortPairs([
              for (var line = 0; line < model.lineCount; line++)
                for (final (a, b) in projection.lineSpans(line)) ...[a, b],
            ]),
          ], joinTouching: true)
        : <int>[source.length, source.length];
    final legal = <int>[];
    var blockedIndex = 0, caretIndex = 0;
    void consider(int boundary) {
      while (blockedIndex < blocked.length &&
          blocked[blockedIndex + 1] <= boundary) {
        blockedIndex += 2;
      }
      if (blockedIndex < blocked.length &&
          blocked[blockedIndex] < boundary &&
          boundary < blocked[blockedIndex + 1]) {
        return;
      }
      while (caretIndex < caretSpans.length &&
          caretSpans[caretIndex + 1] < boundary) {
        caretIndex += 2;
      }
      if (caretIndex < caretSpans.length &&
          caretSpans[caretIndex] <= boundary) {
        legal.add(boundary);
      }
    }

    consider(0);
    var offset = 0;
    for (final grapheme in source.characters) {
      offset += grapheme.length;
      consider(offset);
    }
    return legal;
  }();

  /// Intervals no caret may be strictly inside: hidden syntax and atomic
  /// display units, merged where they overlap. Touching intervals stay
  /// apart, because the offset between them is legal.
  late final List<int> _blockedPairs =
      _positions?._blockedPairs ??
      _mergePairs([
        _runPairs.hidden,
        _runPairs.escapes,
        _rowAtomicPairs,
      ], joinTouching: false);

  /// Whether a source offset is a legal caret position.
  bool isLegal(int offset) {
    if (offset < 0 || offset > source.length) return false;
    final blocked = _blockedPairs;
    var lo = 0, hi = blocked.length >> 1;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (blocked[mid << 1] < offset) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    if (lo > 0 && offset < blocked[(lo << 1) - 1]) return false;
    final spans = projection.lineSpans(model.lineOfUtf16(offset));
    if (!spans.any((s) => s.$1 <= offset && offset <= s.$2)) {
      if (projection.hasCaretSpans || offset != source.length) return false;
    }
    // Common edits already target legal anchors. Check that one boundary
    // directly instead of enumerating every grapheme in the document.
    return CharacterRange.at(source, offset).isEmpty;
  }

  /// The nearest legal offset to [offset]: out of a hidden interval
  /// forwards, then onto the closest caret span of its line, else the first
  /// span of a following line or the last span of a preceding one.
  int legalize(int offset) {
    var o = offset.clamp(0, source.length);
    if (isLegal(o)) return o;
    final hidden = _runPairs.hidden;
    for (var i = 0; i < hidden.length; i += 2) {
      if (hidden[i] >= o) break;
      if (o < hidden[i + 1]) o = hidden[i + 1];
    }
    if (isLegal(o)) return o;
    if (_legalOffsets.isEmpty) return source.length;
    final line = model.lineOfUtf16(o);
    var low = 0, high = _legalOffsets.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (_legalOffsets[middle] < o) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    final after = low < _legalOffsets.length ? _legalOffsets[low] : null;
    final before = low > 0 ? _legalOffsets[low - 1] : null;
    final afterOnLine = after != null && model.lineOfUtf16(after) == line;
    final beforeOnLine = before != null && model.lineOfUtf16(before) == line;
    if (afterOnLine && beforeOnLine) {
      return after - o < o - before ? after : before;
    }
    if (afterOnLine) return after;
    if (beforeOnLine) return before;
    return after ?? before ?? source.length;
  }

  /// Display position of a source offset.
  DisplayPosition displayOf(int offset) =>
      projection.displayForSource(legalize(offset)) ??
      const DisplayPosition(0, 0);

  /// Every legal offset displayed at the same place as [offset], ascending:
  /// the caret's possible anchors there. Adjacent legal offsets separated
  /// only by hidden bytes share a display position.
  List<int> anchorsAt(int offset) {
    final o = legalize(offset);
    final pos = displayOf(o);
    final seen = <int>{o};
    final queue = [o];
    final hidden = _runPairs.hidden;
    while (queue.isNotEmpty) {
      final x = queue.removeLast();
      for (var i = 0; i < hidden.length; i += 2) {
        if (hidden[i + 1] == x && seen.add(hidden[i])) queue.add(hidden[i]);
        if (hidden[i] == x && seen.add(hidden[i + 1])) {
          queue.add(hidden[i + 1]);
        }
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

  /// Resolve a pointer's nearest displayed caret to its source context.
  /// At a word edge beside whitespace or the row boundary, take the word's
  /// side. Tiny horizontal differences must not pick different formatting at
  /// that same visible caret. Between two non-whitespace glyphs, the hit half
  /// still distinguishes their contexts.
  int pointerAnchorAt(int rowIndex, int offset, {required bool leadingHalf}) {
    final row = projection.rows[rowIndex];
    final at = offset.clamp(0, row.text.length);
    final beforeSpace = at == 0 || row.text[at - 1].trim().isEmpty;
    final afterSpace = at == row.text.length || row.text[at].trim().isEmpty;
    final fromRight = beforeSpace != afterSpace ? beforeSpace : leadingHalf;
    final anchors = anchorsAt(
      row.sourceForDisplay(
        at,
        anchor: fromRight ? Anchor.after : Anchor.before,
      ),
    );
    return fromRight ? anchors.last : anchors.first;
  }

  static bool _owns(int kind) =>
      kind == RunKind.emph ||
      kind == RunKind.strong ||
      kind == RunKind.strike ||
      kind == RunKind.code ||
      kind == RunKind.link ||
      kind == RunKind.image ||
      kind == RunKind.escape;

  Owner _owner(int r) => Owner(
    r,
    model.runKind(r),
    model.runStart(r),
    model.runEnd(r),
    model.runContentStart(r),
    model.runContentEnd(r),
  );

  /// The runs of the leaf block projected at [offset]: (first, end).
  (int, int) _runsNear(int offset) {
    final row = rowAt(offset);
    if (row.block < 0) return (0, 0);
    return (
      model.firstRunOfBlock(row.block),
      model.firstRunOfBlock(row.block + 1),
    );
  }

  /// Styled owners whose content contains [offset], outermost first. An
  /// offset at a content edge counts as inside: that is what makes the
  /// anchor the typing context.
  List<Owner> ownersAt(int offset) {
    final (first, end) = _runsNear(offset);
    return [
      for (var r = first; r < end; r++)
        if (_owns(model.runKind(r)) &&
            model.runKind(r) != RunKind.escape &&
            offset >= model.runContentStart(r) &&
            offset <= model.runContentEnd(r))
          _owner(r),
    ];
  }

  /// Owners whose content is exactly [start, end): emptied by deleting it.
  List<Owner> ownersOfContent(int start, int end) {
    final (first, last) = _runsNear(start);
    return [
      for (var r = first; r < last; r++)
        if (_owns(model.runKind(r)) &&
            model.runContentStart(r) == start &&
            model.runContentEnd(r) == end)
          _owner(r),
    ];
  }

  /// Owners whose full range starts or ends at [offset]: adjacent from outside.
  List<Owner> ownersTouching(int offset) {
    final (first, end) = _runsNear(offset);
    return [
      for (var r = first; r < end; r++)
        if (_owns(model.runKind(r)) &&
            model.runKind(r) != RunKind.escape &&
            (model.runStart(r) == offset || model.runEnd(r) == offset))
          _owner(r),
    ];
  }

  /// Style bits the next keystroke at [offset] inherits.
  int typingContextAt(int offset) {
    var mask = 0;
    for (final o in ownersAt(offset)) {
      mask |= o.style;
    }
    return mask;
  }

  /// The row holding a source offset.
  ProjectedRow rowAt(int offset) => projection.rows[displayOf(offset).row];
}

/// Package-internal deletion closure for projected graphemes and replacements.
/// Replacement segments can cover several displayed graphemes, so deleting
/// one segment must also include a combining suffix attached to its last glyph.
({int start, int end}) expandFlarkAtomicRange(
  FlarkDocument document,
  int start,
  int end,
) {
  final blocked = document._blockedPairs;
  for (var i = 0; i < blocked.length; i += 2) {
    final a = blocked[i], b = blocked[i + 1];
    if (a >= end) break;
    if (a < start && start < b) start = a;
    if (a < end && end < b) end = b;
  }
  return (start: start, end: end);
}

/// Package-internal construction after the editor has admitted a parsed model.
FlarkDocument projectFlarkDocument(
  String source,
  RenderModel model,
  FlarkSelection selection,
  ProjectionOptions options,
) => FlarkDocument._(
  source,
  const FlarkSelection.collapsed(0),
  model,
  Projection.of(model, source, options: options),
  false,
).withSelection(selection);

/// Package-internal source preflight shared by the document and editor.
/// This is intentionally not exported from `package:flark/flark.dart`.
void validateFlarkSource(String source) {
  for (var i = 0; i < source.length; i++) {
    final codeUnit = source.codeUnitAt(i);
    if (codeUnit == 0x0D &&
        (i + 1 >= source.length || source.codeUnitAt(i + 1) != 0x0A)) {
      throw const FormatException(
        'flark source contains a bare carriage return',
      );
    }
    if (codeUnit >= 0xD800 && codeUnit <= 0xDBFF) {
      if (i + 1 >= source.length) {
        throw const FormatException('flark source contains malformed UTF-16');
      }
      final low = source.codeUnitAt(i + 1);
      if (low < 0xDC00 || low > 0xDFFF) {
        throw const FormatException('flark source contains malformed UTF-16');
      }
      i++;
    } else if (codeUnit >= 0xDC00 && codeUnit <= 0xDFFF) {
      throw const FormatException('flark source contains malformed UTF-16');
    }
  }
}
