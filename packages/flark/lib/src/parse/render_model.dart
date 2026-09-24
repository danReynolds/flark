import 'dart:convert';
import 'dart:typed_data';

import 'schema.g.dart';

/// The flat render model written by `flark_parse`, read through typed-data
/// views. Nothing is materialized until a host asks for it.
///
/// Every offset is a UTF-16 code unit of the source string. Records are
/// fixed-width words, some packing several fields; values only some kinds
/// carry sit in an extras section. Layout: header, lines, blocks, content
/// records, runs, definitions, run-extra index, extras, string table. See
/// `native/flark_parse/SCHEMA.md`.
final class RenderModel {
  /// Wraps [bytes]; a view whose offset is not word-aligned is copied first,
  /// because the words are read through a [Uint32List] view. Both transports
  /// hand in fresh, aligned buffers, so the copy only happens for slices a
  /// host carved out of a larger buffer.
  factory RenderModel(Uint8List bytes) {
    if (bytes.offsetInBytes % 4 != 0) bytes = Uint8List.fromList(bytes);

    if (Endian.host != Endian.little) {
      throw UnsupportedError('flark: big-endian hosts are not supported');
    }
    if (bytes.lengthInBytes < RenderModelSchema.headerWords * 4) {
      throw const FormatException('render model shorter than its header');
    }
    final words = Uint32List.sublistView(
      bytes,
      0,
      bytes.lengthInBytes ~/ 4 * 4,
    );
    if (words[HeaderField.magic] != RenderModelSchema.magic) {
      throw const FormatException('render model magic mismatch');
    }
    if (words[HeaderField.version] != RenderModelSchema.version) {
      throw FormatException(
        'render model version ${words[HeaderField.version]}, expected ${RenderModelSchema.version}',
      );
    }
    final blocks =
        RenderModelSchema.headerWords +
        words[HeaderField.lineCount] * RenderModelSchema.lineWords;
    final content =
        blocks + words[HeaderField.blockCount] * RenderModelSchema.blockWords;
    final runs =
        content +
        words[HeaderField.contentCount] * RenderModelSchema.contentWords;
    final definitions =
        runs + words[HeaderField.runCount] * RenderModelSchema.runWords;
    final runExtras =
        definitions +
        words[HeaderField.definitionCount] * RenderModelSchema.definitionWords;
    final extras =
        runExtras +
        words[HeaderField.runExtraCount] * RenderModelSchema.runExtraWords;
    final strings = (extras + words[HeaderField.extraWords]) * 4;
    if (strings + words[HeaderField.stringBytes] > bytes.lengthInBytes) {
      throw const FormatException('render model truncated');
    }
    return RenderModel._(
      bytes.asUnmodifiableView(),
      words,
      blocks,
      content,
      runs,
      definitions,
      runExtras,
      extras,
      strings,
    );
  }

  // Plain final fields, not late: every accessor reads an offset, and a late
  // field is checked on each read under dart2js.
  RenderModel._(
    this.bytes,
    this._words,
    this._blocksOff,
    this._contentOff,
    this._runsOff,
    this._defsOff,
    this._runExtrasOff,
    this._extrasOff,
    this._stringsByteOff,
  ) : lineCount = _words[HeaderField.lineCount],
      blockCount = _words[HeaderField.blockCount],
      contentCount = _words[HeaderField.contentCount],
      runCount = _words[HeaderField.runCount],
      definitionCount = _words[HeaderField.definitionCount],
      _runExtraCount = _words[HeaderField.runExtraCount],
      sourceBytes = _words[HeaderField.srcBytes],
      sourceUtf16 = _words[HeaderField.srcUtf16];

  final Uint8List bytes;
  final Uint32List _words;
  final int lineCount,
      blockCount,
      contentCount,
      runCount,
      definitionCount,
      sourceBytes,
      sourceUtf16;
  final int _runExtraCount;
  static const _linesOff = RenderModelSchema.headerWords;
  final int _blocksOff,
      _contentOff,
      _runsOff,
      _defsOff,
      _runExtrasOff,
      _extrasOff,
      _stringsByteOff;

  int _block(int index, int field) =>
      _words[_blocksOff + index * RenderModelSchema.blockWords + field];
  int _content(int index, int field) =>
      _words[_contentOff + index * RenderModelSchema.contentWords + field];
  int _run(int index, int field) =>
      _words[_runsOff + index * RenderModelSchema.runWords + field];
  int _extra(int offset) => _words[_extrasOff + offset];

  // Lines.

  int lineStartUtf16(int line) =>
      _words[_linesOff + line * RenderModelSchema.lineWords + LineField.start];

  // The last line [lineOfUtf16] found. Callers mostly ask about increasing
  // offsets, so the answer is usually that line or the next.
  int _lineHint = 0;

  /// Index of the line containing UTF-16 offset [offset] (the last line for
  /// the end of the source).
  int lineOfUtf16(int offset) {
    final hint = _lineHint;
    if (hint < lineCount && lineStartUtf16(hint) <= offset) {
      if (hint + 1 >= lineCount || offset < lineStartUtf16(hint + 1)) {
        return hint;
      }
      if (hint + 2 >= lineCount || offset < lineStartUtf16(hint + 2)) {
        return _lineHint = hint + 1;
      }
    }
    var lo = 0, hi = lineCount - 1;
    while (lo < hi) {
      final mid = (lo + hi + 1) >> 1;
      if (lineStartUtf16(mid) <= offset) {
        lo = mid;
      } else {
        hi = mid - 1;
      }
    }
    return _lineHint = lo;
  }

  // Blocks. See `SCHEMA.md` for what [blockAttr] and [blockFlags] mean per
  // kind.

  int blockKind(int index) =>
      _block(index, BlockField.kindFlags) >> BlockKindFlags.kindShift &
      BlockKindFlags.kindMask;
  int blockFlags(int index) =>
      _block(index, BlockField.kindFlags) >> BlockKindFlags.flagsShift &
      BlockKindFlags.flagsMask;

  /// Block flags bit 23, on any leaf: its inline extraction could not be
  /// verified against the parser, so it publishes no runs and displays its
  /// source as plain text while the rest of the document renders.
  static const int sourceOnlyFlag = 1 << 23;

  /// Whether block [index] shows its source because its runs were withheld.
  bool blockSourceOnly(int index) => blockFlags(index) & sourceOnlyFlag != 0;

  /// The parent block's index, or [noParent] for the document.
  int blockParent(int index) => _block(index, BlockField.parent);
  int blockStart(int index) => _block(index, BlockField.start);
  int blockEnd(int index) => _block(index, BlockField.end);
  int blockFirstLine(int index) => _block(index, BlockField.firstLine);
  int blockLineCount(int index) => _block(index, BlockField.lineCount);

  /// Heading level, fence length, list start number, item content column or
  /// table column count, by kind.
  int blockAttr(int index) => _block(index, BlockField.attr);

  /// Content records of block [index] are `blockContentOffset(index)` up to
  /// `blockContentOffset(index) + blockContentCount(index)`.
  int blockContentOffset(int index) => _block(index, BlockField.contentOffset);
  int blockContentCount(int index) =>
      (index + 1 < blockCount
          ? _block(index + 1, BlockField.contentOffset)
          : contentCount) -
      _block(index, BlockField.contentOffset);

  /// Index of the first run whose block is at or after [blockIndex]. Runs
  /// are contiguous per block in document order, so a block's runs are
  /// `firstRunOfBlock(b)` up to `firstRunOfBlock(b + 1)`; a block without
  /// runs yields an empty range.
  int firstRunOfBlock(int blockIndex) => blockIndex < blockCount
      ? _block(blockIndex, BlockField.firstRun)
      : runCount;

  /// The runs of block [blockIndex], possibly empty.
  Iterable<RunView> runsOfBlock(int blockIndex) {
    final start = firstRunOfBlock(blockIndex),
        end = firstRunOfBlock(blockIndex + 1);
    return Iterable.generate(end - start, (i) => runAt(start + i));
  }

  int _blockExtra(int index, int field) {
    final offset = _block(index, BlockField.extra);
    return offset == RenderModelSchema.noExtra ? 0 : _extra(offset + field);
  }

  /// The info string of fenced code block [index]; empty when it has none.
  int codeInfoStart(int index) => _blockExtra(index, CodeBlockExtra.infoStart);
  int codeInfoEnd(int index) => _blockExtra(index, CodeBlockExtra.infoEnd);

  /// End of item [index]'s first-line marker and padding, before any task
  /// checkbox.
  int itemMarkerEnd(int index) => _blockExtra(index, ItemExtra.markerEnd);

  /// The task symbol of item [index]; zero unless it is a task item.
  int itemTaskStart(int index) => _blockExtra(index, ItemExtra.taskStart);
  int itemTaskEnd(int index) => _blockExtra(index, ItemExtra.taskEnd);

  /// Column alignments of table [index], two bits per column from column 0.
  int tableAlignments(int index) => _blockExtra(index, TableExtra.alignments);

  /// The label of footnote definition [index].
  int footnoteLabelStart(int index) =>
      _blockExtra(index, FootnoteDefinitionExtra.labelStart);
  int footnoteLabelEnd(int index) =>
      _blockExtra(index, FootnoteDefinitionExtra.labelEnd);

  // Content records.

  int contentLine(int index) =>
      _content(index, ContentField.lineVirtual) >>
          ContentLineVirtual.lineShift &
      ContentLineVirtual.lineMask;
  int contentStart(int index) => _content(index, ContentField.start);
  int contentEnd(int index) => _content(index, ContentField.end);
  int contentPrefixStart(int index) =>
      _content(index, ContentField.prefixStart);
  int contentVirtualSpaces(int index) =>
      _content(index, ContentField.lineVirtual) >>
          ContentLineVirtual.virtualLeadingSpacesShift &
      ContentLineVirtual.virtualLeadingSpacesMask;

  // Runs.

  static const _wideHidden = 0xFFFFFFFF;
  static const _wideDistance = 0xFFFF;

  int runKind(int index) =>
      _run(index, RunField.kindFlagsParent) >> RunKindFlagsParent.kindShift &
      RunKindFlagsParent.kindMask;
  int runFlags(int index) =>
      _run(index, RunField.kindFlagsParent) >> RunKindFlagsParent.flagsShift &
      RunKindFlagsParent.flagsMask;
  int runStart(int index) => _run(index, RunField.start);
  int runEnd(int index) => _run(index, RunField.end);
  int runContentStart(int index) {
    final hidden = _run(index, RunField.hidden);
    if (hidden == _wideHidden) {
      return _extra(_runExtra(index) + WideRunExtra.contentStart);
    }
    return _run(index, RunField.start) +
        (hidden >> RunHidden.beforeShift & RunHidden.beforeMask);
  }

  int runContentEnd(int index) {
    final hidden = _run(index, RunField.hidden);
    if (hidden == _wideHidden) {
      return _extra(_runExtra(index) + WideRunExtra.contentEnd);
    }
    return _run(index, RunField.end) -
        (hidden >> RunHidden.afterShift & RunHidden.afterMask);
  }

  /// The parent run's index, or [noParent].
  int runParent(int index) {
    final distance =
        _run(index, RunField.kindFlagsParent) >>
            RunKindFlagsParent.parentDistanceShift &
        RunKindFlagsParent.parentDistanceMask;
    if (distance == 0) return noParent;
    if (distance == _wideDistance) {
      return _extra(_runExtra(index) + WideRunExtra.parent);
    }
    return index - distance;
  }

  /// The block that owns run [index].
  int runBlock(int index) {
    // The last block whose first run is at or before [index]: blocks before
    // the owner that share its first run are containers with no runs.
    var lo = 0, hi = blockCount - 1;
    while (lo < hi) {
      final mid = (lo + hi + 1) >> 1;
      if (_block(mid, BlockField.firstRun) <= index) {
        lo = mid;
      } else {
        hi = mid - 1;
      }
    }
    return lo;
  }

  /// Offset of run [index]'s record in the extras section, past its wide
  /// fields, or -1 when it has none.
  int _runKindExtra(int index) {
    final offset = _runExtra(index);
    if (offset < 0) return -1;
    return _run(index, RunField.hidden) == _wideHidden
        ? offset + WideRunExtra.words
        : offset;
  }

  int _runExtra(int index) {
    var lo = 0, hi = _runExtraCount;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      final run =
          _words[_runExtrasOff +
              mid * RenderModelSchema.runExtraWords +
              RunExtraField.run];
      if (run < index) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    if (lo < _runExtraCount &&
        _words[_runExtrasOff +
                lo * RenderModelSchema.runExtraWords +
                RunExtraField.run] ==
            index) {
      return _words[_runExtrasOff +
          lo * RenderModelSchema.runExtraWords +
          RunExtraField.offset];
    }
    return -1;
  }

  int _runExtraField(int index, int field) {
    final offset = _runKindExtra(index);
    return offset < 0 ? 0 : _extra(offset + field);
  }

  /// Source range of a link or image destination (for a reference link, its
  /// label) or of an autolink's URL; empty when absent.
  int linkDestinationStart(int index) =>
      _runExtraField(index, LinkExtra.destinationStart);
  int linkDestinationEnd(int index) =>
      _runExtraField(index, LinkExtra.destinationEnd);

  /// Source range of a link or image title; empty when absent.
  int linkTitleStart(int index) => _runExtraField(index, LinkExtra.titleStart);
  int linkTitleEnd(int index) => _runExtraField(index, LinkExtra.titleEnd);

  /// Comrak's resolved destination of a link, image or autolink, including
  /// references, escapes and entities.
  String linkDestination(int index) => string(
    _runExtraField(index, LinkExtra.destinationOffset),
    _runExtraField(index, LinkExtra.destinationLength),
  );

  /// Comrak's resolved title of a link or image.
  String linkTitle(int index) => string(
    _runExtraField(index, LinkExtra.titleOffset),
    _runExtraField(index, LinkExtra.titleLength),
  );

  /// Display text for a run whose content is not a source slice: replacement
  /// runs always, code runs when flags bit 1 is set.
  String? displayOverride(int index) {
    final kind = runKind(index);
    if (kind != RunKind.replacement &&
        (kind != RunKind.code || runFlags(index) & 2 == 0)) {
      return null;
    }
    final offset = _runKindExtra(index);
    return string(
      _extra(offset + DisplayTextExtra.offset),
      _extra(offset + DisplayTextExtra.length),
    );
  }

  /// The label of footnote reference [index].
  int footnoteRefLabelStart(int index) =>
      _runExtraField(index, FootnoteRefExtra.labelStart);
  int footnoteRefLabelEnd(int index) =>
      _runExtraField(index, FootnoteRefExtra.labelEnd);

  // Definitions.

  int _definition(int index, int field) =>
      _words[_defsOff + index * RenderModelSchema.definitionWords + field];
  int definitionStart(int index) => _definition(index, DefinitionField.start);
  int definitionEnd(int index) => _definition(index, DefinitionField.end);
  int definitionLabelStart(int index) =>
      _definition(index, DefinitionField.labelStart);
  int definitionLabelEnd(int index) =>
      _definition(index, DefinitionField.labelEnd);
  int definitionDestinationStart(int index) =>
      _definition(index, DefinitionField.destStart);
  int definitionDestinationEnd(int index) =>
      _definition(index, DefinitionField.destEnd);

  /// A string-table entry, used by replacement runs and display overrides.
  String string(int offset, int length) => utf8.decode(
    Uint8List.sublistView(
      bytes,
      _stringsByteOff + offset,
      _stringsByteOff + offset + length,
    ),
  );

  BlockView blockAt(int index) => BlockView(this, index);
  RunView runAt(int index) => RunView(this, index);
  DefinitionView definitionAt(int index) => DefinitionView(this, index);
  Iterable<DefinitionView> get definitions =>
      Iterable.generate(definitionCount, definitionAt);
  Iterable<BlockView> get blocks => Iterable.generate(blockCount, blockAt);
  Iterable<RunView> get runs => Iterable.generate(runCount, runAt);
}

/// Sentinel for "no parent" in block and run parent fields.
const int noParent = 0xFFFFFFFF;

extension type const BlockView._((RenderModel, int) _rec) {
  const BlockView(RenderModel model, int index) : this._((model, index));
  RenderModel get model => _rec.$1;
  int get index => _rec.$2;
  int get kind => model.blockKind(index);
  int get parent => model.blockParent(index);
  int get startUtf16 => model.blockStart(index);
  int get endUtf16 => model.blockEnd(index);
  int get firstLine => model.blockFirstLine(index);
  int get lineCount => model.blockLineCount(index);
  int get contentOffset => model.blockContentOffset(index);
  int get contentCount => model.blockContentCount(index);
  int get attr => model.blockAttr(index);
  int get flags => model.blockFlags(index);
  bool get sourceOnly => model.blockSourceOnly(index);
  bool get isLeaf =>
      kind == BlockKind.paragraph ||
      kind == BlockKind.heading ||
      kind == BlockKind.codeBlock ||
      kind == BlockKind.htmlBlock ||
      kind == BlockKind.tableCell ||
      kind == BlockKind.thematicBreak;
  Iterable<ContentView> get contentLines => Iterable.generate(
    contentCount,
    (i) => ContentView(model, contentOffset + i),
  );
}

extension type const ContentView._((RenderModel, int) _rec) {
  const ContentView(RenderModel model, int index) : this._((model, index));
  RenderModel get model => _rec.$1;
  int get index => _rec.$2;
  int get line => model.contentLine(index);
  int get startUtf16 => model.contentStart(index);
  int get endUtf16 => model.contentEnd(index);
  int get prefixStartUtf16 => model.contentPrefixStart(index);
  int get virtualLeadingSpaces => model.contentVirtualSpaces(index);
}

extension type const RunView._((RenderModel, int) _rec) {
  const RunView(RenderModel model, int index) : this._((model, index));
  RenderModel get model => _rec.$1;
  int get index => _rec.$2;
  int get kind => model.runKind(index);
  int get block => model.runBlock(index);
  int get parent => model.runParent(index);
  int get startUtf16 => model.runStart(index);
  int get endUtf16 => model.runEnd(index);
  int get contentStartUtf16 => model.runContentStart(index);
  int get contentEndUtf16 => model.runContentEnd(index);
  int get flags => model.runFlags(index);
  bool get spansLines => flags & 4 != 0;

  /// Comrak's resolved values, including references, escapes and entities.
  /// Only meaningful for link, image and autolink runs.
  String get destination => model.linkDestination(index);
  String get title => model.linkTitle(index);

  /// Display text for a run whose content is not a source slice: replacement
  /// runs always, code runs when the override flag is set.
  String? get displayOverride => model.displayOverride(index);
}

extension type const DefinitionView._((RenderModel, int) _rec) {
  const DefinitionView(RenderModel model, int index) : this._((model, index));
  RenderModel get model => _rec.$1;
  int get index => _rec.$2;
  int get startUtf16 => model.definitionStart(index);
  int get endUtf16 => model.definitionEnd(index);
  int get labelStartUtf16 => model.definitionLabelStart(index);
  int get labelEndUtf16 => model.definitionLabelEnd(index);
  int get destStartUtf16 => model.definitionDestinationStart(index);
  int get destEndUtf16 => model.definitionDestinationEnd(index);
}

/// Whether [count] content records of [a] from [aFirst] and of [b] from
/// [bFirst] are the same with source offsets taken relative to [aBase] and
/// [bBase] and lines relative to [aLine] and [bLine]. The projection compares
/// a block this way before reusing its row; not exported.
bool sameContentRecords(
  RenderModel a,
  int aFirst,
  RenderModel b,
  int bFirst,
  int count, {
  required int aBase,
  required int bBase,
  required int aLine,
  required int bLine,
}) {
  const words = RenderModelSchema.contentWords;
  final aw = a._words, bw = b._words;
  var i = a._contentOff + aFirst * words, j = b._contentOff + bFirst * words;
  for (var k = 0; k < count; k++, i += words, j += words) {
    final av = aw[i + ContentField.lineVirtual];
    final bv = bw[j + ContentField.lineVirtual];
    if (aw[i + ContentField.start] - aBase !=
            bw[j + ContentField.start] - bBase ||
        aw[i + ContentField.end] - aBase != bw[j + ContentField.end] - bBase ||
        aw[i + ContentField.prefixStart] - aBase !=
            bw[j + ContentField.prefixStart] - bBase ||
        (av >> ContentLineVirtual.lineShift & ContentLineVirtual.lineMask) -
                aLine !=
            (bv >> ContentLineVirtual.lineShift & ContentLineVirtual.lineMask) -
                bLine ||
        av >> ContentLineVirtual.virtualLeadingSpacesShift !=
            bv >> ContentLineVirtual.virtualLeadingSpacesShift) {
      return false;
    }
  }
  return true;
}

/// Whether [count] runs of [a] from [aFirst] and of [b] from [bFirst] are the
/// same with source offsets taken relative to [aBase] and [bBase]: kind,
/// flags, extent, hidden delimiters, parent and replacement text. A parent
/// distance and hidden widths are already relative, so a record compares as
/// words; a wide run keeps absolute values in the extras. The projection
/// compares a block this way before reusing its row; not exported.
bool sameRunRecords(
  RenderModel a,
  int aFirst,
  RenderModel b,
  int bFirst,
  int count, {
  required int aBase,
  required int bBase,
}) {
  const words = RenderModelSchema.runWords;
  final aw = a._words, bw = b._words;
  var i = a._runsOff + aFirst * words, j = b._runsOff + bFirst * words;
  for (var k = 0; k < count; k++, i += words, j += words) {
    final hidden = aw[i + RunField.hidden];
    final packed = aw[i + RunField.kindFlagsParent];
    if (hidden != bw[j + RunField.hidden] ||
        packed != bw[j + RunField.kindFlagsParent] ||
        aw[i + RunField.start] - aBase != bw[j + RunField.start] - bBase ||
        aw[i + RunField.end] - aBase != bw[j + RunField.end] - bBase) {
      return false;
    }
    final r = aFirst + k, q = bFirst + k;
    if (hidden == RenderModel._wideHidden &&
        (a.runContentStart(r) - aBase != b.runContentStart(q) - bBase ||
            a.runContentEnd(r) - aBase != b.runContentEnd(q) - bBase)) {
      return false;
    }
    final distance =
        packed >> RunKindFlagsParent.parentDistanceShift &
        RunKindFlagsParent.parentDistanceMask;
    if (distance == RenderModel._wideDistance &&
        a.runParent(r) - aFirst != b.runParent(q) - bFirst) {
      return false;
    }
    final kind =
        packed >> RunKindFlagsParent.kindShift & RunKindFlagsParent.kindMask;
    final flags =
        packed >> RunKindFlagsParent.flagsShift & RunKindFlagsParent.flagsMask;
    if ((kind == RunKind.replacement ||
            kind == RunKind.code && flags & 2 != 0) &&
        a.displayOverride(r) != b.displayOverride(q)) {
      return false;
    }
  }
  return true;
}
