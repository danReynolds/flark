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
/// position holds is the typing context, so the offset alone encodes the
/// caret's anchor. A noncollapsed 0..source.length range explicitly selects the
/// whole source, including block syntax outside the first/last caret spans.
final class FlarkSelection {
  const FlarkSelection(this.base, this.extent);
  const FlarkSelection.collapsed(int offset) : base = offset, extent = offset;
  final int base;
  final int extent;
  bool get isCollapsed => base == extent;
  int get start => base < extent ? base : extent;
  int get end => base < extent ? extent : base;
  @override
  bool operator ==(Object other) =>
      other is FlarkSelection && other.base == base && other.extent == extent;
  @override
  int get hashCode => Object.hash(base, extent);
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
        : FlarkSelection(legalize(s.base), legalize(s.extent)),
    model,
    projection,
    normalizedLineEndings,
    positions: _positions ?? this,
  );

  // ------------------------------------------------------------ positions

  /// Hidden intervals of inline runs (delimiters, break markers) in UTF-16,
  /// sorted by start.
  late final List<(int, int)> hiddenIntervals = () {
    if (_positions != null) return _positions.hiddenIntervals;
    final out = <(int, int)>[];
    var sorted = true, last = -1;
    void add(int a, int b) {
      if (a < last) sorted = false;
      last = a;
      out.add((a, b));
    }

    for (var r = 0; r < model.runCount; r++) {
      final s = model.run(r, RunField.startUtf16),
          e = model.run(r, RunField.endUtf16);
      final cs = model.run(r, RunField.contentStartUtf16),
          ce = model.run(r, RunField.contentEndUtf16);
      if (cs > s) add(s, cs);
      if (e > ce) add(ce, e);
    }
    if (!sorted) out.sort((a, b) => a.$1 - b.$1);
    return List<(int, int)>.unmodifiable(out);
  }();

  /// Source ranges represented by one non-exact display glyph (entities,
  /// escapes, and normalized code spans) are atomic caret units.
  late final List<(int, int)> _atomicIntervals = () {
    if (_positions != null) return _positions._atomicIntervals;
    final intervals = <(int, int)>[];
    for (final row in projection.rows) {
      for (final segment in row.segments) {
        if (!segment.exact && segment.sourceEnd > segment.sourceStart) {
          intervals.add((segment.sourceStart, segment.sourceEnd));
        }
      }
      // Hidden syntax can separate source graphemes that become one displayed
      // grapheme, such as *a* followed by a combining accent. Keep the whole
      // displayed unit atomic, including any replacement it intersects.
      var offset = 0, segmentIndex = 0;
      for (final grapheme in row.text.characters) {
        final end = offset + grapheme.length;
        while (segmentIndex < row.segments.length &&
            row.segments[segmentIndex].displayEnd <= offset) {
          segmentIndex++;
        }
        if (segmentIndex < row.segments.length &&
            row.segments[segmentIndex].displayEnd < end) {
          final first = row.segments[segmentIndex];
          var lastIndex = segmentIndex;
          while (row.segments[lastIndex].displayEnd < end) {
            lastIndex++;
          }
          final last = row.segments[lastIndex];
          final startSource = first.exact
              ? first.sourceStart + offset - first.displayStart
              : first.sourceStart;
          final endSource = last.exact
              ? last.sourceStart + end - last.displayStart
              : last.sourceEnd;
          if (endSource > startSource) {
            intervals.add((startSource, endSource));
          }
        }
        offset = end;
      }
    }
    return intervals..sort((a, b) => a.$1 - b.$1);
  }();

  static List<(int, int)> _mergeIntervals(
    List<(int, int)> intervals, {
    required bool joinTouching,
  }) {
    if (intervals.isEmpty) return const [];
    intervals.sort((a, b) => a.$1 != b.$1 ? a.$1 - b.$1 : a.$2 - b.$2);
    final merged = <(int, int)>[intervals.first];
    for (final next in intervals.skip(1)) {
      final last = merged.last;
      final joins = joinTouching ? next.$1 <= last.$2 : next.$1 < last.$2;
      if (joins) {
        if (next.$2 > last.$2) merged[merged.length - 1] = (last.$1, next.$2);
      } else {
        merged.add(next);
      }
    }
    return merged;
  }

  /// Legal source offsets, restricted to Unicode grapheme boundaries and to
  /// the edges of non-exact projection replacements.
  late final List<int> _legalOffsets = () {
    if (_positions != null) return _positions._legalOffsets;
    // This is a linear merge over three ordered interval streams. Calling an
    // interval scan for every grapheme made dense 64 KiB documents quadratic.
    final blocked = _blockedIntervals;
    final caretSpans = projection.hasCaretSpans
        ? _mergeIntervals([
            for (var line = 0; line < model.lineCount; line++)
              ...projection.lineSpans(line),
          ], joinTouching: true)
        : <(int, int)>[(source.length, source.length)];
    final legal = <int>[];
    var blockedIndex = 0, caretIndex = 0;
    void consider(int boundary) {
      while (blockedIndex < blocked.length &&
          blocked[blockedIndex].$2 <= boundary) {
        blockedIndex++;
      }
      if (blockedIndex < blocked.length &&
          blocked[blockedIndex].$1 < boundary &&
          boundary < blocked[blockedIndex].$2) {
        return;
      }
      while (caretIndex < caretSpans.length &&
          caretSpans[caretIndex].$2 < boundary) {
        caretIndex++;
      }
      if (caretIndex < caretSpans.length &&
          caretSpans[caretIndex].$1 <= boundary) {
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

  late final List<(int, int)> _blockedIntervals =
      _positions?._blockedIntervals ??
      _mergeIntervals([
        ...hiddenIntervals,
        ..._atomicIntervals,
      ], joinTouching: false);

  /// Whether a source offset is a legal caret position.
  bool isLegal(int offset) {
    if (offset < 0 || offset > source.length) return false;
    final blocked = _blockedIntervals;
    var lo = 0, hi = blocked.length;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (blocked[mid].$1 < offset) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    if (lo > 0 && offset < blocked[lo - 1].$2) return false;
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
    for (final h in hiddenIntervals) {
      if (h.$1 >= o) break;
      if (o < h.$2) o = h.$2;
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
    model.run(r, RunField.kind),
    model.run(r, RunField.startUtf16),
    model.run(r, RunField.endUtf16),
    model.run(r, RunField.contentStartUtf16),
    model.run(r, RunField.contentEndUtf16),
  );

  /// The runs of the leaf block projected at [offset]: (first, end).
  (int, int) _runsNear(int offset) {
    final row = rowAt(offset);
    if (row.block < 0) return (0, 0);
    final first = model.firstRunOfBlock(row.block);
    var end = first;
    while (end < model.runCount &&
        model.run(end, RunField.block) == row.block) {
      end++;
    }
    return (first, end);
  }

  /// Styled owners whose content contains [offset], outermost first. An
  /// offset at a content edge counts as inside: that is what makes the
  /// anchor the typing context.
  List<Owner> ownersAt(int offset) {
    final (first, end) = _runsNear(offset);
    return [
      for (var r = first; r < end; r++)
        if (_owns(model.run(r, RunField.kind)) &&
            model.run(r, RunField.kind) != RunKind.escape &&
            offset >= model.run(r, RunField.contentStartUtf16) &&
            offset <= model.run(r, RunField.contentEndUtf16))
          _owner(r),
    ];
  }

  /// Owners whose content is exactly [start, end): emptied by deleting it.
  List<Owner> ownersOfContent(int start, int end) {
    final (first, last) = _runsNear(start);
    return [
      for (var r = first; r < last; r++)
        if (_owns(model.run(r, RunField.kind)) &&
            model.run(r, RunField.contentStartUtf16) == start &&
            model.run(r, RunField.contentEndUtf16) == end)
          _owner(r),
    ];
  }

  /// Owners whose full range starts or ends at [offset]: adjacent from outside.
  List<Owner> ownersTouching(int offset) {
    final (first, end) = _runsNear(offset);
    return [
      for (var r = first; r < end; r++)
        if (_owns(model.run(r, RunField.kind)) &&
            model.run(r, RunField.kind) != RunKind.escape &&
            (model.run(r, RunField.startUtf16) == offset ||
                model.run(r, RunField.endUtf16) == offset))
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
  for (final interval in document._blockedIntervals) {
    if (interval.$1 >= end) break;
    if (interval.$1 < start && start < interval.$2) start = interval.$1;
    if (interval.$1 < end && end < interval.$2) end = interval.$2;
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
