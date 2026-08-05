import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

/// Disposable prototype for Flark's UI-side canonical text store.
///
/// All public offsets are UTF-16 code-unit offsets, matching Flutter. Parser
/// deltas carry UTF-8 byte offsets. The tree stores both metrics so conversion
/// touches O(log n) nodes plus one bounded leaf.
final class PrototypePersistentDocument {
  const PrototypePersistentDocument._({
    required _TextNode? root,
    required this.revision,
    required int chunkSize,
  }) : _root = root,
       _chunkSize = chunkSize;

  factory PrototypePersistentDocument.fromString(
    String source, {
    int chunkSize = 4096,
  }) {
    if (chunkSize < 2) {
      throw RangeError.range(chunkSize, 2, null, 'chunkSize');
    }
    _validateScalarString(source, 'source');
    return PrototypePersistentDocument._(
      root: _treeFromString(source, chunkSize),
      revision: 0,
      chunkSize: chunkSize,
    );
  }

  final _TextNode? _root;
  final int _chunkSize;

  final int revision;

  int get utf16Length => _root?.utf16Length ?? 0;

  int get utf8Length => _root?.utf8Length ?? 0;

  int get lineCount => (_root?.newlines ?? 0) + 1;

  int get contentHash32 => _root?.hash32 ?? 0;

  PrototypeDocumentFingerprint get fingerprint => PrototypeDocumentFingerprint(
    revision: revision,
    utf16Length: utf16Length,
    utf8Length: utf8Length,
    contentHash32: contentHash32,
  );

  String substring(int startUtf16, [int? endUtf16]) {
    final end = endUtf16 ?? utf16Length;
    _checkUtf16Range(startUtf16, end);
    if (startUtf16 == end) return '';
    final output = StringBuffer();
    _writeRange(_root, startUtf16, end, output);
    return output.toString();
  }

  @override
  String toString() => substring(0, utf16Length);

  int utf16ToUtf8(int utf16Offset) {
    _checkUtf16Offset(utf16Offset);
    _checkScalarBoundary(utf16Offset);
    return _utf16ToUtf8(_root, utf16Offset);
  }

  int utf8ToUtf16(int utf8Offset) {
    if (utf8Offset < 0 || utf8Offset > utf8Length) {
      throw RangeError.range(utf8Offset, 0, utf8Length, 'utf8Offset');
    }
    return _utf8ToUtf16(_root, utf8Offset);
  }

  int lineAtUtf16(int utf16Offset) {
    _checkUtf16Offset(utf16Offset);
    return _newlinesBefore(_root, utf16Offset);
  }

  int lineStartUtf16(int lineIndex) {
    if (lineIndex < 0 || lineIndex >= lineCount) {
      throw RangeError.range(lineIndex, 0, lineCount - 1, 'lineIndex');
    }
    if (lineIndex == 0) return 0;
    return _offsetAfterNthNewline(_root!, lineIndex);
  }

  PrototypeAppliedEdit apply(PrototypeDocumentEdit edit) {
    if (edit.baseRevision != revision) {
      throw PrototypeRevisionMismatch(
        expected: revision,
        actual: edit.baseRevision,
      );
    }
    _checkUtf16Range(edit.startUtf16, edit.endUtf16);
    _checkScalarBoundary(edit.startUtf16);
    _checkScalarBoundary(edit.endUtf16);
    _validateScalarString(edit.replacement, 'replacement');

    final before = fingerprint;
    final startUtf8 = utf16ToUtf8(edit.startUtf16);
    final endUtf8 = utf16ToUtf8(edit.endUtf16);
    final first = _split(_root, edit.startUtf16, _chunkSize);
    final second = _split(
      first.right,
      edit.endUtf16 - edit.startUtf16,
      _chunkSize,
    );
    final inserted = _treeFromString(edit.replacement, _chunkSize);
    final nextRoot = _concat(
      _concat(first.left, inserted, _chunkSize),
      second.right,
      _chunkSize,
    );
    final next = PrototypePersistentDocument._(
      root: nextRoot,
      revision: revision + 1,
      chunkSize: _chunkSize,
    );
    final replacementUtf8 = Uint8List.fromList(utf8.encode(edit.replacement));
    return PrototypeAppliedEdit(
      document: next,
      parserDelta: PrototypeParserEditDelta(
        baseRevision: revision,
        revision: next.revision,
        startUtf8: startUtf8,
        endUtf8: endUtf8,
        replacementUtf8: replacementUtf8,
        beforeHash32: before.contentHash32,
        afterHash32: next.contentHash32,
      ),
    );
  }

  void _checkUtf16Offset(int offset) {
    if (offset < 0 || offset > utf16Length) {
      throw RangeError.range(offset, 0, utf16Length, 'utf16Offset');
    }
  }

  void _checkUtf16Range(int start, int end) {
    _checkUtf16Offset(start);
    _checkUtf16Offset(end);
    if (end < start) {
      throw RangeError.range(end, start, utf16Length, 'endUtf16');
    }
  }

  void _checkScalarBoundary(int offset) {
    if (offset == 0 || offset == utf16Length) return;
    final root = _root!;
    final previous = _codeUnitAt(root, offset - 1);
    final next = _codeUnitAt(root, offset);
    if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
      throw FormatException('UTF-16 offset $offset splits a scalar value.');
    }
  }
}

final class PrototypeDocumentEdit {
  const PrototypeDocumentEdit({
    required this.baseRevision,
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
  });

  final int baseRevision;
  final int startUtf16;
  final int endUtf16;
  final String replacement;
}

final class PrototypeAppliedEdit {
  const PrototypeAppliedEdit({
    required this.document,
    required this.parserDelta,
  });

  final PrototypePersistentDocument document;
  final PrototypeParserEditDelta parserDelta;
}

final class PrototypeParserEditDelta {
  const PrototypeParserEditDelta({
    required this.baseRevision,
    required this.revision,
    required this.startUtf8,
    required this.endUtf8,
    required this.replacementUtf8,
    required this.beforeHash32,
    required this.afterHash32,
  });

  final int baseRevision;
  final int revision;
  final int startUtf8;
  final int endUtf8;
  final Uint8List replacementUtf8;
  final int beforeHash32;
  final int afterHash32;

  int get wireBytes => 28 + replacementUtf8.length;
}

final class PrototypeDocumentFingerprint {
  const PrototypeDocumentFingerprint({
    required this.revision,
    required this.utf16Length,
    required this.utf8Length,
    required this.contentHash32,
  });

  final int revision;
  final int utf16Length;
  final int utf8Length;
  final int contentHash32;

  @override
  bool operator ==(Object other) {
    return other is PrototypeDocumentFingerprint &&
        other.revision == revision &&
        other.utf16Length == utf16Length &&
        other.utf8Length == utf8Length &&
        other.contentHash32 == contentHash32;
  }

  @override
  int get hashCode =>
      Object.hash(revision, utf16Length, utf8Length, contentHash32);
}

final class PrototypeRevisionMismatch implements Exception {
  const PrototypeRevisionMismatch({
    required this.expected,
    required this.actual,
  });

  final int expected;
  final int actual;

  @override
  String toString() =>
      'PrototypeRevisionMismatch(expected: $expected, actual: $actual)';
}

sealed class _TextNode {
  const _TextNode();

  int get utf16Length;
  int get utf8Length;
  int get newlines;
  int get height;
  int get hash32;
  int get hashPower32;
}

final class _TextLeaf extends _TextNode {
  const _TextLeaf._({
    required this.source,
    required this.start,
    required this.utf16Length,
    required this.utf8Length,
    required this.newlineOffsets,
    required this.hash32,
    required this.hashPower32,
  });

  factory _TextLeaf.fromSource(String source, int start, int end) {
    final newlines = <int>[];
    var offset = start;
    while (offset < end) {
      final codeUnit = source.codeUnitAt(offset);
      if (codeUnit == 0x0A) newlines.add(offset - start);
      final width = _scalarUtf16Width(source, offset, end);
      offset += width;
    }
    // This fingerprint crosses the Dart/native boundary, so it must hash the
    // parser's canonical UTF-8 bytes rather than Dart's UTF-16 code units.
    final bytes = utf8.encode(source.substring(start, end));
    var hash = 0;
    var power = 1;
    for (final byte in bytes) {
      hash = (_mul32(hash, _hashBase) + byte + 1) & _mask32;
      power = _mul32(power, _hashBase);
    }
    return _TextLeaf._(
      source: source,
      start: start,
      utf16Length: end - start,
      utf8Length: bytes.length,
      newlineOffsets: List<int>.unmodifiable(newlines),
      hash32: hash,
      hashPower32: power,
    );
  }

  final String source;
  final int start;
  @override
  final int utf16Length;
  @override
  final int utf8Length;
  final List<int> newlineOffsets;
  @override
  final int hash32;
  @override
  final int hashPower32;

  @override
  int get newlines => newlineOffsets.length;

  @override
  int get height => 1;

  _TextLeaf slice(int relativeStart, int relativeEnd) {
    return _TextLeaf.fromSource(
      source,
      start + relativeStart,
      start + relativeEnd,
    );
  }

  String materialize() => source.substring(start, start + utf16Length);
}

final class _TextBranch extends _TextNode {
  _TextBranch(this.left, this.right)
    : utf16Length = left.utf16Length + right.utf16Length,
      utf8Length = left.utf8Length + right.utf8Length,
      newlines = left.newlines + right.newlines,
      height = math.max(left.height, right.height) + 1,
      hash32 =
          (_mul32(left.hash32, right.hashPower32) + right.hash32) & _mask32,
      hashPower32 = _mul32(left.hashPower32, right.hashPower32);

  final _TextNode left;
  final _TextNode right;
  @override
  final int utf16Length;
  @override
  final int utf8Length;
  @override
  final int newlines;
  @override
  final int height;
  @override
  final int hash32;
  @override
  final int hashPower32;
}

final class _TextSplit {
  const _TextSplit(this.left, this.right);

  final _TextNode? left;
  final _TextNode? right;
}

const int _mask32 = 0xFFFFFFFF;
const int _hashBase = 0x00100193;

_TextNode? _treeFromString(String source, int chunkSize) {
  if (source.isEmpty) return null;
  final leaves = <_TextNode>[];
  var start = 0;
  while (start < source.length) {
    var end = math.min(start + chunkSize, source.length);
    if (end < source.length &&
        _isHighSurrogate(source.codeUnitAt(end - 1)) &&
        _isLowSurrogate(source.codeUnitAt(end))) {
      end -= 1;
    }
    if (end == start) end = math.min(start + 2, source.length);
    leaves.add(_TextLeaf.fromSource(source, start, end));
    start = end;
  }
  return _buildBalanced(leaves, 0, leaves.length);
}

_TextNode? _buildBalanced(List<_TextNode> nodes, int start, int end) {
  if (start >= end) return null;
  if (end - start == 1) return nodes[start];
  final middle = start + ((end - start) >> 1);
  return _TextBranch(
    _buildBalanced(nodes, start, middle)!,
    _buildBalanced(nodes, middle, end)!,
  );
}

_TextSplit _split(_TextNode? node, int offset, int chunkSize) {
  if (node == null) return const _TextSplit(null, null);
  if (offset == 0) return _TextSplit(null, node);
  if (offset == node.utf16Length) return _TextSplit(node, null);

  if (node case final _TextLeaf leaf) {
    final absolute = leaf.start + offset;
    final previous = leaf.source.codeUnitAt(absolute - 1);
    final next = leaf.source.codeUnitAt(absolute);
    if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
      throw FormatException('UTF-16 split $offset divides a scalar value.');
    }
    return _TextSplit(
      leaf.slice(0, offset),
      leaf.slice(offset, leaf.utf16Length),
    );
  }

  final branch = node as _TextBranch;
  if (offset < branch.left.utf16Length) {
    final split = _split(branch.left, offset, chunkSize);
    return _TextSplit(
      split.left,
      _concat(split.right, branch.right, chunkSize),
    );
  }
  if (offset == branch.left.utf16Length) {
    return _TextSplit(branch.left, branch.right);
  }
  final split = _split(
    branch.right,
    offset - branch.left.utf16Length,
    chunkSize,
  );
  return _TextSplit(_concat(branch.left, split.left, chunkSize), split.right);
}

_TextNode? _concat(_TextNode? left, _TextNode? right, int chunkSize) {
  if (left == null) return right;
  if (right == null) return left;
  if (left is _TextLeaf &&
      right is _TextLeaf &&
      left.utf16Length + right.utf16Length <= chunkSize) {
    final joined = '${left.materialize()}${right.materialize()}';
    return _TextLeaf.fromSource(joined, 0, joined.length);
  }

  if (left.height > right.height + 1) {
    final branch = left as _TextBranch;
    return _balance(
      _TextBranch(branch.left, _concat(branch.right, right, chunkSize)!),
    );
  }
  if (right.height > left.height + 1) {
    final branch = right as _TextBranch;
    return _balance(
      _TextBranch(_concat(left, branch.left, chunkSize)!, branch.right),
    );
  }
  return _TextBranch(left, right);
}

_TextNode _balance(_TextBranch node) {
  final balance = node.left.height - node.right.height;
  if (balance > 1) {
    final left = node.left as _TextBranch;
    if (left.left.height < left.right.height) {
      final pivot = left.right as _TextBranch;
      return _TextBranch(
        _TextBranch(left.left, pivot.left),
        _TextBranch(pivot.right, node.right),
      );
    }
    return _TextBranch(left.left, _TextBranch(left.right, node.right));
  }
  if (balance < -1) {
    final right = node.right as _TextBranch;
    if (right.right.height < right.left.height) {
      final pivot = right.left as _TextBranch;
      return _TextBranch(
        _TextBranch(node.left, pivot.left),
        _TextBranch(pivot.right, right.right),
      );
    }
    return _TextBranch(_TextBranch(node.left, right.left), right.right);
  }
  return node;
}

void _writeRange(_TextNode? node, int start, int end, StringBuffer output) {
  if (node == null || start >= end) return;
  if (node case final _TextLeaf leaf) {
    output.write(leaf.source.substring(leaf.start + start, leaf.start + end));
    return;
  }
  final branch = node as _TextBranch;
  if (start < branch.left.utf16Length) {
    _writeRange(
      branch.left,
      start,
      math.min(end, branch.left.utf16Length),
      output,
    );
  }
  if (end > branch.left.utf16Length) {
    _writeRange(
      branch.right,
      math.max(0, start - branch.left.utf16Length),
      end - branch.left.utf16Length,
      output,
    );
  }
}

int _utf16ToUtf8(_TextNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf16Length) return node.utf8Length;
  if (node case final _TextLeaf leaf) {
    var utf16 = 0;
    var utf8 = 0;
    while (utf16 < offset) {
      final absolute = leaf.start + utf16;
      utf8 += _utf8Width(leaf.source, absolute, leaf.start + leaf.utf16Length);
      utf16 += _scalarUtf16Width(
        leaf.source,
        absolute,
        leaf.start + leaf.utf16Length,
      );
    }
    if (utf16 != offset) {
      throw FormatException('UTF-16 offset $offset divides a scalar value.');
    }
    return utf8;
  }
  final branch = node as _TextBranch;
  if (offset <= branch.left.utf16Length) {
    return _utf16ToUtf8(branch.left, offset);
  }
  return branch.left.utf8Length +
      _utf16ToUtf8(branch.right, offset - branch.left.utf16Length);
}

int _utf8ToUtf16(_TextNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf8Length) return node.utf16Length;
  if (node case final _TextLeaf leaf) {
    var utf16 = 0;
    var utf8 = 0;
    while (utf8 < offset) {
      final absolute = leaf.start + utf16;
      utf8 += _utf8Width(leaf.source, absolute, leaf.start + leaf.utf16Length);
      utf16 += _scalarUtf16Width(
        leaf.source,
        absolute,
        leaf.start + leaf.utf16Length,
      );
    }
    if (utf8 != offset) {
      throw FormatException('UTF-8 offset $offset divides a scalar value.');
    }
    return utf16;
  }
  final branch = node as _TextBranch;
  if (offset <= branch.left.utf8Length) {
    return _utf8ToUtf16(branch.left, offset);
  }
  return branch.left.utf16Length +
      _utf8ToUtf16(branch.right, offset - branch.left.utf8Length);
}

int _newlinesBefore(_TextNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf16Length) return node.newlines;
  if (node case final _TextLeaf leaf) {
    var low = 0;
    var high = leaf.newlineOffsets.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (leaf.newlineOffsets[middle] < offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low;
  }
  final branch = node as _TextBranch;
  if (offset <= branch.left.utf16Length) {
    return _newlinesBefore(branch.left, offset);
  }
  return branch.left.newlines +
      _newlinesBefore(branch.right, offset - branch.left.utf16Length);
}

int _offsetAfterNthNewline(_TextNode node, int count) {
  if (node case final _TextLeaf leaf) {
    return leaf.newlineOffsets[count - 1] + 1;
  }
  final branch = node as _TextBranch;
  if (count <= branch.left.newlines) {
    return _offsetAfterNthNewline(branch.left, count);
  }
  return branch.left.utf16Length +
      _offsetAfterNthNewline(branch.right, count - branch.left.newlines);
}

int _codeUnitAt(_TextNode node, int offset) {
  if (node case final _TextLeaf leaf) {
    return leaf.source.codeUnitAt(leaf.start + offset);
  }
  final branch = node as _TextBranch;
  if (offset < branch.left.utf16Length) {
    return _codeUnitAt(branch.left, offset);
  }
  return _codeUnitAt(branch.right, offset - branch.left.utf16Length);
}

void _validateScalarString(String value, String name) {
  var offset = 0;
  while (offset < value.length) {
    final codeUnit = value.codeUnitAt(offset);
    if (_isHighSurrogate(codeUnit)) {
      if (offset + 1 >= value.length ||
          !_isLowSurrogate(value.codeUnitAt(offset + 1))) {
        throw FormatException('$name contains an unpaired high surrogate.');
      }
      offset += 2;
      continue;
    }
    if (_isLowSurrogate(codeUnit)) {
      throw FormatException('$name contains an unpaired low surrogate.');
    }
    offset += 1;
  }
}

int _scalarUtf16Width(String source, int offset, int end) {
  final codeUnit = source.codeUnitAt(offset);
  return _isHighSurrogate(codeUnit) &&
          offset + 1 < end &&
          _isLowSurrogate(source.codeUnitAt(offset + 1))
      ? 2
      : 1;
}

int _utf8Width(String source, int offset, int end) {
  final codeUnit = source.codeUnitAt(offset);
  if (codeUnit <= 0x7F) return 1;
  if (codeUnit <= 0x7FF) return 2;
  if (_isHighSurrogate(codeUnit) &&
      offset + 1 < end &&
      _isLowSurrogate(source.codeUnitAt(offset + 1))) {
    return 4;
  }
  return 3;
}

bool _isHighSurrogate(int codeUnit) => codeUnit >= 0xD800 && codeUnit <= 0xDBFF;

bool _isLowSurrogate(int codeUnit) => codeUnit >= 0xDC00 && codeUnit <= 0xDFFF;

int _mul32(int left, int right) {
  final low = (left & 0xFFFF) * (right & 0xFFFF);
  final middle =
      ((left >>> 16) * (right & 0xFFFF)) + ((left & 0xFFFF) * (right >>> 16));
  return (low + ((middle & 0xFFFF) << 16)) & _mask32;
}
