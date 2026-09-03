import 'dart:convert';
import 'dart:typed_data';

import 'schema.g.dart';

/// The flat render model written by `flark_parse`, read through typed-data
/// views. Nothing is materialized until a host asks for it.
///
/// Layout: header, line table, blocks, content records, runs, definitions,
/// string table. See `native/flark_parse/SCHEMA.md`.
final class RenderModel {
  /// Wraps [bytes]; a view whose offset is not word-aligned is copied first,
  /// because the words are read through a [Uint32List] view. Both transports
  /// hand in fresh, aligned buffers, so the copy only happens for slices a
  /// host carved out of a larger buffer.
  factory RenderModel(Uint8List bytes) {
    if (bytes.offsetInBytes % 4 != 0) bytes = Uint8List.fromList(bytes);
    return RenderModel._(bytes);
  }

  RenderModel._(this.bytes) : _words = Uint32List.sublistView(bytes, 0, bytes.lengthInBytes ~/ 4 * 4) {
    if (Endian.host != Endian.little) throw UnsupportedError('flark: big-endian hosts are not supported');
    if (bytes.lengthInBytes < RenderModelSchema.headerWords * 4) {
      throw const FormatException('render model shorter than its header');
    }
    if (_words[HeaderField.magic] != RenderModelSchema.magic) {
      throw const FormatException('render model magic mismatch');
    }
    if (_words[HeaderField.version] != RenderModelSchema.version) {
      throw FormatException('render model version ${_words[HeaderField.version]}, expected ${RenderModelSchema.version}');
    }
    lineCount = _words[HeaderField.lineCount];
    blockCount = _words[HeaderField.blockCount];
    contentCount = _words[HeaderField.contentCount];
    runCount = _words[HeaderField.runCount];
    definitionCount = _words[HeaderField.definitionCount];
    sourceBytes = _words[HeaderField.srcBytes];
    sourceUtf16 = _words[HeaderField.srcUtf16];
    _linesOff = RenderModelSchema.headerWords;
    _blocksOff = _linesOff + lineCount * RenderModelSchema.lineWords;
    _contentOff = _blocksOff + blockCount * RenderModelSchema.blockWords;
    _runsOff = _contentOff + contentCount * RenderModelSchema.contentWords;
    _defsOff = _runsOff + runCount * RenderModelSchema.runWords;
    _stringsByteOff = (_defsOff + definitionCount * RenderModelSchema.definitionWords) * 4;
    final stringBytes = _words[HeaderField.stringBytes];
    if (_stringsByteOff + stringBytes > bytes.lengthInBytes) {
      throw const FormatException('render model truncated');
    }
  }

  final Uint8List bytes;
  final Uint32List _words;
  late final int lineCount, blockCount, contentCount, runCount, definitionCount, sourceBytes, sourceUtf16;
  late final int _linesOff, _blocksOff, _contentOff, _runsOff, _defsOff, _stringsByteOff;

  int _w(int wordIndex) => _words[wordIndex];

  int lineStartByte(int line) => _w(_linesOff + line * RenderModelSchema.lineWords + LineField.startByte);
  int lineStartUtf16(int line) => _w(_linesOff + line * RenderModelSchema.lineWords + LineField.startUtf16);

  /// Read one field of block [index]; use [BlockField] for [field].
  int block(int index, int field) => _w(_blocksOff + index * RenderModelSchema.blockWords + field);
  int content(int index, int field) => _w(_contentOff + index * RenderModelSchema.contentWords + field);
  int run(int index, int field) => _w(_runsOff + index * RenderModelSchema.runWords + field);
  int definition(int index, int field) => _w(_defsOff + index * RenderModelSchema.definitionWords + field);

  /// A string-table entry, used by replacement runs and display overrides.
  String string(int offset, int length) => utf8.decode(Uint8List.sublistView(bytes, _stringsByteOff + offset, _stringsByteOff + offset + length));

  BlockView blockAt(int index) => BlockView(this, index);
  RunView runAt(int index) => RunView(this, index);
  Iterable<BlockView> get blocks => Iterable.generate(blockCount, blockAt);
  Iterable<RunView> get runs => Iterable.generate(runCount, runAt);

  /// Index of the first run whose block is at or after [blockIndex]. Runs
  /// are contiguous per block in document order, so a block's runs are
  /// `firstRunOfBlock(b)` up to `firstRunOfBlock(b + 1)`; a block without
  /// runs yields an empty range. Use [runsOfBlock] for the range itself.
  int firstRunOfBlock(int blockIndex) {
    var lo = 0, hi = runCount;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (run(mid, RunField.block) < blockIndex) { lo = mid + 1; } else { hi = mid; }
    }
    return lo;
  }

  /// The runs of block [blockIndex], possibly empty.
  Iterable<RunView> runsOfBlock(int blockIndex) {
    final start = firstRunOfBlock(blockIndex), end = firstRunOfBlock(blockIndex + 1);
    return Iterable.generate(end - start, (i) => runAt(start + i));
  }
}

/// Sentinel for "no parent" in block and run parent fields.
const int noParent = 0xFFFFFFFF;

extension type const BlockView._((RenderModel, int) _rec) {
  const BlockView(RenderModel model, int index) : this._((model, index));
  RenderModel get model => _rec.$1;
  int get index => _rec.$2;
  int field(int f) => model.block(index, f);
  int get kind => field(BlockField.kind);
  int get parent => field(BlockField.parent);
  int get startByte => field(BlockField.startByte);
  int get endByte => field(BlockField.endByte);
  int get startUtf16 => field(BlockField.startUtf16);
  int get endUtf16 => field(BlockField.endUtf16);
  int get firstLine => field(BlockField.firstLine);
  int get lineCount => field(BlockField.lineCount);
  int get contentOffset => field(BlockField.contentOffset);
  int get contentCount => field(BlockField.contentCount);
  int get attr0 => field(BlockField.attr0);
  int get attr1 => field(BlockField.attr1);
  int get attr2 => field(BlockField.attr2);
  int get flags => field(BlockField.flags);
  bool get isLeaf => kind == BlockKind.paragraph || kind == BlockKind.heading || kind == BlockKind.codeBlock || kind == BlockKind.htmlBlock || kind == BlockKind.tableCell || kind == BlockKind.thematicBreak;
  Iterable<ContentView> get contentLines => Iterable.generate(contentCount, (i) => ContentView(model, contentOffset + i));
}

extension type const ContentView._((RenderModel, int) _rec) {
  const ContentView(RenderModel model, int index) : this._((model, index));
  int field(int f) => _rec.$1.content(_rec.$2, f);
  int get line => field(ContentField.line);
  int get startByte => field(ContentField.startByte);
  int get startUtf16 => field(ContentField.startUtf16);
  int get endByte => field(ContentField.endByte);
  int get endUtf16 => field(ContentField.endUtf16);
  int get virtualLeadingSpaces => field(ContentField.virtualLeadingSpaces);
}

extension type const RunView._((RenderModel, int) _rec) {
  const RunView(RenderModel model, int index) : this._((model, index));
  RenderModel get model => _rec.$1;
  int get index => _rec.$2;
  int field(int f) => model.run(index, f);
  int get kind => field(RunField.kind);
  int get block => field(RunField.block);
  int get parent => field(RunField.parent);
  int get startByte => field(RunField.startByte);
  int get endByte => field(RunField.endByte);
  int get contentStartByte => field(RunField.contentStartByte);
  int get contentEndByte => field(RunField.contentEndByte);
  int get startUtf16 => field(RunField.startUtf16);
  int get endUtf16 => field(RunField.endUtf16);
  int get contentStartUtf16 => field(RunField.contentStartUtf16);
  int get contentEndUtf16 => field(RunField.contentEndUtf16);
  int get aux0 => field(RunField.aux0);
  int get aux1 => field(RunField.aux1);
  int get aux2 => field(RunField.aux2);
  int get aux3 => field(RunField.aux3);
  int get flags => field(RunField.flags);
  bool get spansLines => flags & (1 << 8) != 0;

  /// Display text for a run whose content is not a source slice: replacement
  /// runs always, code runs when the override flag is set.
  String? get displayOverride {
    if (kind == RunKind.replacement) return model.string(aux0, aux1);
    if (kind == RunKind.code && flags & 2 != 0) return model.string(aux2, aux3);
    return null;
  }
}

extension type const DefinitionView._((RenderModel, int) _rec) {
  const DefinitionView(RenderModel model, int index) : this._((model, index));
  int field(int f) => _rec.$1.definition(_rec.$2, f);
  int get startByte => field(DefinitionField.startByte);
  int get endByte => field(DefinitionField.endByte);
  int get startUtf16 => field(DefinitionField.startUtf16);
  int get endUtf16 => field(DefinitionField.endUtf16);
  int get labelStartByte => field(DefinitionField.labelStartByte);
  int get labelEndByte => field(DefinitionField.labelEndByte);
  int get destStartByte => field(DefinitionField.destStartByte);
  int get destEndByte => field(DefinitionField.destEndByte);
}
