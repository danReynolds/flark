import 'dart:async';
import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:math' as math;
import 'dart:typed_data';

/// Disposable second-gate spike for a persistent candidate/certified source.
///
/// Unlike `lazy_bulk_source_probe.dart`, this uses a persistent AVL sum tree.
/// It is still research code: one 32-bit receipt hash stands in for the four
/// production lanes, and only one source edit is accepted per transaction.
Future<void> main(List<String> arguments) async {
  final options = _Options(arguments);
  if (options.string('gate', 'candidate') == 'current-root') {
    await _runCurrentRootGate(options);
    return;
  }
  final sizeMiB = options.integer('size-mib', 10);
  final edits = options.integer('edits', 10000);
  final source = _asciiMarkdownOfLength(sizeMiB * 1024 * 1024);

  _emit('environment', {
    'dart': Platform.version.split('\n').first,
    'size_mib': sizeMiB,
    'rss_mib': _rssMiB(),
  });
  _verifyPersistentTreeSemantics();
  await _verifyCandidateWorkerSemantics(source);
  _runDeepPromotionReceipt();
  _runRepeatedEditReceipts(edits);
  _emit('probe_complete', {'black_hole': _blackHole, 'rss_mib': _rssMiB()});
}

const int _smallSyncLimit = 8 * 1024;
const int _checkpointUtf16 = int.fromEnvironment(
  'FLARK_CHECKPOINT_UTF16',
  defaultValue: 4 * 1024,
);
const int _workerPollUtf16 = 8 * 1024;
const int _workerThrottleEveryPolls = int.fromEnvironment(
  'FLARK_WORKER_THROTTLE_EVERY_POLLS',
  defaultValue: 0,
);
const int _mask32 = 0xFFFFFFFF;
const int _hashBase = 0x01000193;

final class _Summary {
  const _Summary({
    required this.utf16Length,
    required this.utf8Length,
    required this.lineBreaks,
    required this.hash32,
    required this.hashPower32,
    required this.firstCodeUnit,
    required this.lastCodeUnit,
  });

  static const empty = _Summary(
    utf16Length: 0,
    utf8Length: 0,
    lineBreaks: 0,
    hash32: 0,
    hashPower32: 1,
    firstCodeUnit: null,
    lastCodeUnit: null,
  );

  final int utf16Length;
  final int utf8Length;
  final int lineBreaks;
  final int hash32;
  final int hashPower32;
  final int? firstCodeUnit;
  final int? lastCodeUnit;

  int get lineCount => lineBreaks + 1;

  Map<String, int> toMessage() => {
    'utf16': utf16Length,
    'utf8': utf8Length,
    'lines': lineBreaks,
    'hash': hash32,
    'power': hashPower32,
  };

  static _Summary append(_Summary left, _Summary right) {
    if (left.utf16Length == 0) return right;
    if (right.utf16Length == 0) return left;
    return _Summary(
      utf16Length: left.utf16Length + right.utf16Length,
      utf8Length: left.utf8Length + right.utf8Length,
      lineBreaks:
          left.lineBreaks +
          right.lineBreaks -
          (left.lastCodeUnit == 0x0D && right.firstCodeUnit == 0x0A ? 1 : 0),
      hash32: (_mul32(left.hash32, right.hashPower32) + right.hash32) & _mask32,
      hashPower32: _mul32(left.hashPower32, right.hashPower32),
      firstCodeUnit: left.firstCodeUnit,
      lastCodeUnit: right.lastCodeUnit,
    );
  }
}

final class _PrefixSummary {
  const _PrefixSummary({
    required this.utf16Offset,
    required this.utf8Length,
    required this.lineBreaks,
    required this.hash32,
  });

  final int utf16Offset;
  final int utf8Length;
  final int lineBreaks;
  final int hash32;
}

/// Certified prefix checkpoints for one immutable piece range.
///
/// Four u32 values are stored per checkpoint: relative UTF-16, cumulative
/// UTF-8, cumulative logical line breaks, and cumulative polynomial hash.
final class _RangeIndex {
  const _RangeIndex({
    required this.source,
    required this.domainStart,
    required this.domainLength,
    required Uint32List checkpoints,
  }) : _checkpoints = checkpoints;

  factory _RangeIndex.buildSync(
    String source,
    int domainStart,
    int domainLength,
  ) {
    final builder = _RangeIndexBuilder(
      source: source,
      domainStart: domainStart,
      domainLength: domainLength,
    );
    while (!builder.isComplete) {
      builder.poll(math.max(domainLength, 1));
    }
    if (builder.invalidRelativeOffset case final invalid?) {
      throw FormatException('unpaired surrogate at $invalid');
    }
    return builder.buildIndex();
  }

  factory _RangeIndex.fromTransferred({
    required String source,
    required int domainStart,
    required int domainLength,
    required TransferableTypedData data,
  }) {
    final buffer = data.materialize();
    final transferred = buffer.asUint32List();
    return _RangeIndex(
      source: source,
      domainStart: domainStart,
      domainLength: domainLength,
      checkpoints: transferred,
    );
  }

  final String source;
  final int domainStart;
  final int domainLength;
  final Uint32List _checkpoints;

  int get checkpointCount => _checkpoints.length ~/ 4;

  int utf8Before(int absoluteOffset) {
    if (absoluteOffset < domainStart ||
        absoluteOffset > domainStart + domainLength) {
      throw RangeError.range(
        absoluteOffset,
        domainStart,
        domainStart + domainLength,
      );
    }
    return _prefixAt(absoluteOffset - domainStart).utf8Length;
  }

  int utf16AtUtf8Prefix(int utf8Offset) {
    final totalUtf8 = _checkpoints[_checkpoints.length - 3];
    if (utf8Offset < 0 || utf8Offset > totalUtf8) {
      throw RangeError.range(utf8Offset, 0, totalUtf8);
    }
    var low = 0;
    var high = checkpointCount;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (_checkpoints[middle * 4 + 1] <= utf8Offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    final checkpointOffset = math.max(0, low - 1) * 4;
    var relative = _checkpoints[checkpointOffset];
    var bytes = _checkpoints[checkpointOffset + 1];
    while (bytes < utf8Offset) {
      final absolute = domainStart + relative;
      final unit = source.codeUnitAt(absolute);
      if (_isHighSurrogate(unit)) {
        if (relative + 1 >= domainLength ||
            !_isLowSurrogate(source.codeUnitAt(absolute + 1))) {
          throw FormatException('unpaired high surrogate at $absolute');
        }
        bytes += 4;
        relative += 2;
      } else {
        if (_isLowSurrogate(unit)) {
          throw FormatException('unpaired low surrogate at $absolute');
        }
        bytes += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
        relative += 1;
      }
    }
    if (bytes != utf8Offset) {
      throw FormatException('UTF-8 offset splits a scalar');
    }
    return domainStart + relative;
  }

  _Summary summaryFor(int absoluteStart, int absoluteEnd) {
    if (absoluteStart < domainStart ||
        absoluteEnd > domainStart + domainLength ||
        absoluteEnd < absoluteStart) {
      throw RangeError.range(
        absoluteEnd,
        absoluteStart,
        domainStart + domainLength,
      );
    }
    if (absoluteStart == absoluteEnd) return _Summary.empty;
    final start = _prefixAt(absoluteStart - domainStart);
    final end = _prefixAt(absoluteEnd - domainStart);
    final utf8Length = end.utf8Length - start.utf8Length;
    var lineBreaks = end.lineBreaks - start.lineBreaks;
    if (absoluteStart > domainStart &&
        source.codeUnitAt(absoluteStart) == 0x0A &&
        source.codeUnitAt(absoluteStart - 1) == 0x0D) {
      lineBreaks += 1;
    }
    final power = _pow32(_hashBase, utf8Length);
    final hash = (end.hash32 - _mul32(start.hash32, power)) & _mask32;
    return _Summary(
      utf16Length: absoluteEnd - absoluteStart,
      utf8Length: utf8Length,
      lineBreaks: lineBreaks,
      hash32: hash,
      hashPower32: power,
      firstCodeUnit: source.codeUnitAt(absoluteStart),
      lastCodeUnit: source.codeUnitAt(absoluteEnd - 1),
    );
  }

  _PrefixSummary _prefixAt(int relativeOffset) {
    if (relativeOffset < 0 || relativeOffset > domainLength) {
      throw RangeError.range(relativeOffset, 0, domainLength);
    }
    var low = 0;
    var high = checkpointCount;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (_checkpoints[middle * 4] <= relativeOffset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    final checkpointOffset = (low - 1) * 4;
    final checkpoint = _PrefixSummary(
      utf16Offset: _checkpoints[checkpointOffset],
      utf8Length: _checkpoints[checkpointOffset + 1],
      lineBreaks: _checkpoints[checkpointOffset + 2],
      hash32: _checkpoints[checkpointOffset + 3],
    );
    if (checkpoint.utf16Offset == relativeOffset) return checkpoint;
    return _scanPrefix(checkpoint, relativeOffset);
  }

  _PrefixSummary _scanPrefix(_PrefixSummary base, int target) {
    var relative = base.utf16Offset;
    var bytes = base.utf8Length;
    var breaks = base.lineBreaks;
    var hash = base.hash32;
    var previousWasCr =
        relative > 0 && source.codeUnitAt(domainStart + relative - 1) == 0x0D;
    while (relative < target) {
      final absolute = domainStart + relative;
      final unit = source.codeUnitAt(absolute);
      if (_isHighSurrogate(unit)) {
        final low = source.codeUnitAt(absolute + 1);
        final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
        bytes += 4;
        hash = _appendScalarHash(hash, scalar);
        previousWasCr = false;
        relative += 2;
        continue;
      }
      bytes += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
      hash = _appendScalarHash(hash, unit);
      if (unit == 0x0D) {
        breaks += 1;
        previousWasCr = true;
      } else if (unit == 0x0A) {
        if (!previousWasCr) breaks += 1;
        previousWasCr = false;
      } else {
        previousWasCr = false;
      }
      relative += 1;
    }
    if (relative != target) {
      throw FormatException('UTF-16 offset splits a scalar');
    }
    return _PrefixSummary(
      utf16Offset: relative,
      utf8Length: bytes,
      lineBreaks: breaks,
      hash32: hash,
    );
  }
}

final class _RangeIndexBuilder {
  _RangeIndexBuilder({
    required this.source,
    required this.domainStart,
    required this.domainLength,
  }) : _records = [0, 0, 0, 0];

  final String source;
  final int domainStart;
  final int domainLength;
  final List<int> _records;
  int cursor = 0;
  int utf8Length = 0;
  int lineBreaks = 0;
  int hash32 = 0;
  int? invalidRelativeOffset;
  bool _previousWasCr = false;
  int _nextCheckpoint = _checkpointUtf16;

  bool get isComplete =>
      invalidRelativeOffset != null || cursor == domainLength;

  int poll(int maxUtf16) {
    if (isComplete) return 0;
    final start = cursor;
    final workLimit = math.min(domainLength, cursor + maxUtf16);
    while (cursor < workLimit && !isComplete) {
      var checkpoint = math.min(domainLength, _nextCheckpoint);
      if (checkpoint < domainLength &&
          checkpoint > cursor &&
          _isHighSurrogate(source.codeUnitAt(domainStart + checkpoint - 1)) &&
          _isLowSurrogate(source.codeUnitAt(domainStart + checkpoint))) {
        checkpoint += 1;
      }
      final segmentEnd = math.min(workLimit, checkpoint);
      _scanTo(segmentEnd);
      if (invalidRelativeOffset != null) break;
      if (cursor == checkpoint) {
        _records.addAll([cursor, utf8Length, lineBreaks, hash32]);
        _nextCheckpoint = cursor + _checkpointUtf16;
      }
    }
    if (cursor == domainLength && _records[_records.length - 4] != cursor) {
      _records.addAll([cursor, utf8Length, lineBreaks, hash32]);
    }
    return cursor - start;
  }

  void _scanTo(int end) {
    while (cursor < end) {
      final absolute = domainStart + cursor;
      final unit = source.codeUnitAt(absolute);
      if (_isHighSurrogate(unit)) {
        if (cursor + 1 >= domainLength ||
            !_isLowSurrogate(source.codeUnitAt(absolute + 1))) {
          invalidRelativeOffset = cursor;
          return;
        }
        final low = source.codeUnitAt(absolute + 1);
        final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
        utf8Length += 4;
        hash32 = _appendScalarHash(hash32, scalar);
        _previousWasCr = false;
        cursor += 2;
        continue;
      }
      if (_isLowSurrogate(unit)) {
        invalidRelativeOffset = cursor;
        return;
      }
      utf8Length += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
      hash32 = _appendScalarHash(hash32, unit);
      if (unit == 0x0D) {
        lineBreaks += 1;
        _previousWasCr = true;
      } else if (unit == 0x0A) {
        if (!_previousWasCr) lineBreaks += 1;
        _previousWasCr = false;
      } else {
        _previousWasCr = false;
      }
      cursor += 1;
    }
  }

  _RangeIndex buildIndex() {
    if (!isComplete || invalidRelativeOffset != null) {
      throw StateError('range index is not valid and complete');
    }
    return _RangeIndex(
      source: source,
      domainStart: domainStart,
      domainLength: domainLength,
      checkpoints: Uint32List.fromList(_records),
    );
  }

  TransferableTypedData takeTransferable() {
    final values = Uint32List.fromList(_records);
    return TransferableTypedData.fromList([values.buffer.asUint8List()]);
  }
}

final class _Backing {
  const _Backing({required this.id, required this.source});

  final int id;
  final String source;
}

final class _Piece {
  const _Piece({
    required this.pieceId,
    required this.originStart,
    required this.backing,
    required this.backingStart,
    required this.length,
    required this.index,
  });

  final int pieceId;
  final int originStart;
  final _Backing backing;
  final int backingStart;
  final int length;
  final _RangeIndex? index;

  String get key => '$pieceId:$originStart:$length';

  _Summary? get summary =>
      index?.summaryFor(backingStart, backingStart + length);

  _Piece slice(int start, int end) => _Piece(
    pieceId: pieceId,
    originStart: originStart + start,
    backing: backing,
    backingStart: backingStart + start,
    length: end - start,
    index: index,
  );

  _Piece withIndex(_RangeIndex value) => _Piece(
    pieceId: pieceId,
    originStart: originStart,
    backing: backing,
    backingStart: backingStart,
    length: length,
    index: value,
  );
}

sealed class _Node {
  const _Node();

  int get utf16Length;
  int get height;
  int get pieceCount;
  _Summary? get summary;
}

final class _Leaf extends _Node {
  _Leaf(this.piece) : summary = piece.summary;

  final _Piece piece;

  @override
  int get utf16Length => piece.length;
  @override
  int get height => 1;
  @override
  int get pieceCount => 1;
  @override
  final _Summary? summary;
}

final class _Branch extends _Node {
  _Branch(this.left, this.right)
    : utf16Length = left.utf16Length + right.utf16Length,
      height = math.max(left.height, right.height) + 1,
      pieceCount = left.pieceCount + right.pieceCount,
      summary = left.summary == null || right.summary == null
          ? null
          : _Summary.append(left.summary!, right.summary!);

  final _Node left;
  final _Node right;
  @override
  final int utf16Length;
  @override
  final int height;
  @override
  final int pieceCount;
  @override
  final _Summary? summary;
}

final class _TreeSplit {
  const _TreeSplit(this.left, this.right);

  final _Node? left;
  final _Node? right;
}

final class _TreeWork {
  int nodesVisited = 0;
  int branchesAllocated = 0;
  int leavesAllocated = 0;

  void reset() {
    nodesVisited = 0;
    branchesAllocated = 0;
    leavesAllocated = 0;
  }
}

_TreeSplit _split(_Node? node, int offset, _TreeWork work) {
  if (node == null) return const _TreeSplit(null, null);
  work.nodesVisited += 1;
  if (offset == 0) return _TreeSplit(null, node);
  if (offset == node.utf16Length) return _TreeSplit(node, null);
  if (node case final _Leaf leaf) {
    final previous = leaf.piece.backing.source.codeUnitAt(
      leaf.piece.backingStart + offset - 1,
    );
    final next = leaf.piece.backing.source.codeUnitAt(
      leaf.piece.backingStart + offset,
    );
    if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
      throw FormatException('split divides a scalar');
    }
    work.leavesAllocated += 2;
    return _TreeSplit(
      _Leaf(leaf.piece.slice(0, offset)),
      _Leaf(leaf.piece.slice(offset, leaf.piece.length)),
    );
  }
  final branch = node as _Branch;
  if (offset < branch.left.utf16Length) {
    final split = _split(branch.left, offset, work);
    return _TreeSplit(split.left, _concat(split.right, branch.right, work));
  }
  if (offset == branch.left.utf16Length) {
    return _TreeSplit(branch.left, branch.right);
  }
  final split = _split(branch.right, offset - branch.left.utf16Length, work);
  return _TreeSplit(_concat(branch.left, split.left, work), split.right);
}

_Node? _concat(_Node? left, _Node? right, _TreeWork work) {
  if (left == null) return right;
  if (right == null) return left;
  work.nodesVisited += 1;
  if (left.height > right.height + 1) {
    final branch = left as _Branch;
    return _balance(
      _newBranch(branch.left, _concat(branch.right, right, work)!, work),
      work,
    );
  }
  if (right.height > left.height + 1) {
    final branch = right as _Branch;
    return _balance(
      _newBranch(_concat(left, branch.left, work)!, branch.right, work),
      work,
    );
  }
  return _newBranch(left, right, work);
}

_Branch _newBranch(_Node left, _Node right, _TreeWork work) {
  work.branchesAllocated += 1;
  return _Branch(left, right);
}

_Node _balance(_Branch node, _TreeWork work) {
  final balance = node.left.height - node.right.height;
  if (balance > 1) {
    final left = node.left as _Branch;
    if (left.left.height < left.right.height) {
      final pivot = left.right as _Branch;
      return _newBranch(
        _newBranch(left.left, pivot.left, work),
        _newBranch(pivot.right, node.right, work),
        work,
      );
    }
    return _newBranch(
      left.left,
      _newBranch(left.right, node.right, work),
      work,
    );
  }
  if (balance < -1) {
    final right = node.right as _Branch;
    if (right.right.height < right.left.height) {
      final pivot = right.left as _Branch;
      return _newBranch(
        _newBranch(node.left, pivot.left, work),
        _newBranch(pivot.right, right.right, work),
        work,
      );
    }
    return _newBranch(
      _newBranch(node.left, right.left, work),
      right.right,
      work,
    );
  }
  return node;
}

final class _Edit {
  const _Edit({
    required this.start,
    required this.end,
    required this.replacement,
    required this.backingId,
    required this.pieceId,
  });

  final int start;
  final int end;
  final String replacement;
  final int backingId;
  final int pieceId;

  int get byteCharge => math.max(end - start, replacement.length);

  Map<String, Object> toMessage() => {
    'start': start,
    'end': end,
    'replacement': replacement,
    'backing': backingId,
    'piece': pieceId,
  };

  factory _Edit.fromMessage(Map<Object?, Object?> message) => _Edit(
    start: message['start']! as int,
    end: message['end']! as int,
    replacement: message['replacement']! as String,
    backingId: message['backing']! as int,
    pieceId: message['piece']! as int,
  );
}

final class _AppliedTreeEdit {
  const _AppliedTreeEdit(this.root, this.work);

  final _Node? root;
  final _TreeWork work;
}

final class _CompactedEdge {
  const _CompactedEdge(this.root, this.copiedUtf16);

  final _Node? root;
  final int copiedUtf16;
}

_AppliedTreeEdit _applyTreeEdit(_Node? root, _Edit edit) {
  final length = root?.utf16Length ?? 0;
  if (edit.start < 0 || edit.end < edit.start || edit.end > length) {
    throw RangeError.range(edit.end, edit.start, length);
  }
  _checkBoundary(root, edit.start);
  _checkBoundary(root, edit.end);
  final work = _TreeWork();
  final first = _split(root, edit.start, work);
  final second = _split(first.right, edit.end - edit.start, work);
  var retainedLeft = first.left;
  var retainedRight = second.right;
  if (edit.end > edit.start) {
    var budget = _smallSyncLimit;
    final leftCompaction = _compactEdge(
      retainedLeft,
      rightmost: true,
      budget: budget,
      newBackingId: -(edit.backingId * 2),
      work: work,
    );
    retainedLeft = leftCompaction.root;
    budget -= leftCompaction.copiedUtf16;
    final rightCompaction = _compactEdge(
      retainedRight,
      rightmost: false,
      budget: budget,
      newBackingId: -(edit.backingId * 2 + 1),
      work: work,
    );
    retainedRight = rightCompaction.root;
  }
  _Node? replacement;
  if (edit.replacement.isNotEmpty) {
    final backing = _Backing(id: edit.backingId, source: edit.replacement);
    final index = edit.replacement.length <= _smallSyncLimit
        ? _RangeIndex.buildSync(edit.replacement, 0, edit.replacement.length)
        : null;
    replacement = _Leaf(
      _Piece(
        pieceId: edit.pieceId,
        originStart: 0,
        backing: backing,
        backingStart: 0,
        length: edit.replacement.length,
        index: index,
      ),
    );
    work.leavesAllocated += 1;
  }
  return _AppliedTreeEdit(
    _concat(_concat(retainedLeft, replacement, work), retainedRight, work),
    work,
  );
}

_CompactedEdge _compactEdge(
  _Node? node, {
  required bool rightmost,
  required int budget,
  required int newBackingId,
  required _TreeWork work,
}) {
  if (node == null || budget == 0) return _CompactedEdge(node, 0);
  work.nodesVisited += 1;
  if (node case final _Leaf leaf) {
    final piece = leaf.piece;
    if (piece.length > budget ||
        piece.backing.source.length < 1024 * 1024 ||
        piece.length * 8 >= piece.backing.source.length) {
      return _CompactedEdge(node, 0);
    }
    final owned = String.fromCharCodes(
      piece.backing.source.codeUnits.sublist(
        piece.backingStart,
        piece.backingStart + piece.length,
      ),
    );
    final backing = _Backing(id: newBackingId, source: owned);
    final index = _RangeIndex.buildSync(owned, 0, owned.length);
    work.leavesAllocated += 1;
    return _CompactedEdge(
      _Leaf(
        _Piece(
          pieceId: piece.pieceId,
          originStart: piece.originStart,
          backing: backing,
          backingStart: 0,
          length: owned.length,
          index: index,
        ),
      ),
      owned.length,
    );
  }
  final branch = node as _Branch;
  if (rightmost) {
    final compacted = _compactEdge(
      branch.right,
      rightmost: true,
      budget: budget,
      newBackingId: newBackingId,
      work: work,
    );
    if (compacted.copiedUtf16 == 0) return _CompactedEdge(node, 0);
    return _CompactedEdge(
      _newBranch(branch.left, compacted.root!, work),
      compacted.copiedUtf16,
    );
  }
  final compacted = _compactEdge(
    branch.left,
    rightmost: false,
    budget: budget,
    newBackingId: newBackingId,
    work: work,
  );
  if (compacted.copiedUtf16 == 0) return _CompactedEdge(node, 0);
  return _CompactedEdge(
    _newBranch(compacted.root!, branch.right, work),
    compacted.copiedUtf16,
  );
}

void _checkBoundary(_Node? root, int offset) {
  final length = root?.utf16Length ?? 0;
  if (offset <= 0 || offset >= length) return;
  final previous = _codeUnitAt(root!, offset - 1);
  final next = _codeUnitAt(root, offset);
  if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
    throw FormatException('offset $offset splits a scalar');
  }
}

int _codeUnitAt(_Node node, int offset) {
  if (node case final _Leaf leaf) {
    return leaf.piece.backing.source.codeUnitAt(
      leaf.piece.backingStart + offset,
    );
  }
  final branch = node as _Branch;
  if (offset < branch.left.utf16Length) return _codeUnitAt(branch.left, offset);
  return _codeUnitAt(branch.right, offset - branch.left.utf16Length);
}

String _readRange(_Node? root, int start, int end) {
  final length = root?.utf16Length ?? 0;
  if (start < 0 || end < start || end > length) {
    throw RangeError.range(end, start, length);
  }
  final output = StringBuffer();
  void visit(_Node? node, int localStart, int localEnd) {
    if (node == null || localStart >= localEnd) return;
    if (node case final _Leaf leaf) {
      final absoluteStart = leaf.piece.backingStart + localStart;
      final absoluteEnd = leaf.piece.backingStart + localEnd;
      output.write(
        String.fromCharCodes(
          leaf.piece.backing.source.codeUnits.sublist(
            absoluteStart,
            absoluteEnd,
          ),
        ),
      );
      return;
    }
    final branch = node as _Branch;
    if (localStart < branch.left.utf16Length) {
      visit(
        branch.left,
        localStart,
        math.min(localEnd, branch.left.utf16Length),
      );
    }
    if (localEnd > branch.left.utf16Length) {
      visit(
        branch.right,
        math.max(0, localStart - branch.left.utf16Length),
        localEnd - branch.left.utf16Length,
      );
    }
  }

  visit(root, start, end);
  return output.toString();
}

final class _PendingLeaf {
  const _PendingLeaf(this.globalStart, this.leaf);

  final int globalStart;
  final _Leaf leaf;
}

List<_PendingLeaf> _pendingLeaves(_Node? root) {
  final output = <_PendingLeaf>[];
  void visit(_Node? node, int globalStart) {
    if (node == null) return;
    if (node case final _Leaf leaf) {
      if (leaf.piece.index == null) output.add(_PendingLeaf(globalStart, leaf));
      return;
    }
    final branch = node as _Branch;
    visit(branch.left, globalStart);
    visit(branch.right, globalStart + branch.left.utf16Length);
  }

  visit(root, 0);
  return output;
}

_Node _attachIndexAt(
  _Node node,
  int globalStart,
  String expectedKey,
  _RangeIndex index,
  _TreeWork work,
) {
  work.nodesVisited += 1;
  if (node case final _Leaf leaf) {
    if (globalStart != 0 || leaf.piece.key != expectedKey) {
      throw StateError('worker index does not match candidate leaf');
    }
    work.leavesAllocated += 1;
    return _Leaf(leaf.piece.withIndex(index));
  }
  final branch = node as _Branch;
  if (globalStart < branch.left.utf16Length) {
    return _newBranch(
      _attachIndexAt(branch.left, globalStart, expectedKey, index, work),
      branch.right,
      work,
    );
  }
  return _newBranch(
    branch.left,
    _attachIndexAt(
      branch.right,
      globalStart - branch.left.utf16Length,
      expectedKey,
      index,
      work,
    ),
    work,
  );
}

int _appendScalarHash(int hash, int scalar) {
  void byte(int value) {
    hash = (_mul32(hash, _hashBase) + value + 1) & _mask32;
  }

  if (scalar <= 0x7F) {
    byte(scalar);
  } else if (scalar <= 0x7FF) {
    byte(0xC0 | (scalar >> 6));
    byte(0x80 | (scalar & 0x3F));
  } else if (scalar <= 0xFFFF) {
    byte(0xE0 | (scalar >> 12));
    byte(0x80 | ((scalar >> 6) & 0x3F));
    byte(0x80 | (scalar & 0x3F));
  } else {
    byte(0xF0 | (scalar >> 18));
    byte(0x80 | ((scalar >> 12) & 0x3F));
    byte(0x80 | ((scalar >> 6) & 0x3F));
    byte(0x80 | (scalar & 0x3F));
  }
  return hash;
}

int _pow32(int base, int exponent) {
  var result = 1;
  var factor = base;
  var remaining = exponent;
  while (remaining > 0) {
    if (remaining.isOdd) result = _mul32(result, factor);
    factor = _mul32(factor, factor);
    remaining >>= 1;
  }
  return result;
}

int _mul32(int left, int right) {
  final low = (left & 0xFFFF) * (right & 0xFFFF);
  final middle =
      ((left >>> 16) * (right & 0xFFFF)) + ((left & 0xFFFF) * (right >>> 16));
  return (low + ((middle & 0xFFFF) << 16)) & _mask32;
}

bool _isHighSurrogate(int value) => value >= 0xD800 && value <= 0xDBFF;

bool _isLowSurrogate(int value) => value >= 0xDC00 && value <= 0xDFFF;

final class _Snapshot {
  const _Snapshot({
    required this.root,
    required this.revision,
    required this.summary,
  });

  factory _Snapshot.fromString(String source) {
    if (source.isEmpty) {
      return const _Snapshot(root: null, revision: 0, summary: _Summary.empty);
    }
    final backing = _Backing(id: 1, source: source);
    final index = _RangeIndex.buildSync(source, 0, source.length);
    final root = _Leaf(
      _Piece(
        pieceId: 1,
        originStart: 0,
        backing: backing,
        backingStart: 0,
        length: source.length,
        index: index,
      ),
    );
    return _Snapshot(root: root, revision: 0, summary: root.summary!);
  }

  final _Node? root;
  final int revision;
  final _Summary summary;
}

final class _Candidate {
  const _Candidate({
    required this.root,
    required this.baseCertifiedRevision,
    required this.logicalRevision,
    required this.journal,
  });

  final _Node? root;
  final int baseCertifiedRevision;
  final int logicalRevision;
  final List<_Edit> journal;
}

final class _SessionState {
  const _SessionState({
    required this.certified,
    required this.candidate,
    required this.logicalRevision,
  });

  final _Snapshot certified;
  final _Candidate? candidate;
  final int logicalRevision;
}

final class _HistoryEntry {
  const _HistoryEntry({required this.state, required this.byteCharge});

  final _SessionState state;
  final int byteCharge;
}

/// Byte-charged persistent history. The newest entry is always retained so an
/// oversized paste/delete remains immediately undoable; older roots are
/// evicted on the next transaction until the budget is met.
final class _ByteHistory {
  _ByteHistory(this.byteBudget);

  final int byteBudget;
  static const int maxEntries = 2048;
  final List<_HistoryEntry> _entries = [];
  int chargedBytes = 0;
  int evictions = 0;

  int get length => _entries.length;
  bool get overBudget => chargedBytes > byteBudget;

  void push(_SessionState state, int byteCharge) {
    final entry = _HistoryEntry(state: state, byteCharge: byteCharge);
    _entries.add(entry);
    chargedBytes += byteCharge;
    while ((chargedBytes > byteBudget || _entries.length > maxEntries) &&
        _entries.length > 1) {
      final removed = _entries.removeAt(0);
      chargedBytes -= removed.byteCharge;
      evictions += 1;
    }
  }

  _SessionState pop() {
    if (_entries.isEmpty) throw StateError('nothing to undo');
    final entry = _entries.removeLast();
    chargedBytes -= entry.byteCharge;
    return entry.state;
  }
}

enum _EditDisposition { certifiedSynchronously, provisional }

final class _SessionEditReceipt {
  const _SessionEditReceipt({
    required this.disposition,
    required this.work,
    required this.logicalRevision,
    required this.edit,
  });

  final _EditDisposition disposition;
  final _TreeWork work;
  final int logicalRevision;
  final _Edit edit;
}

enum _PromotionDisposition { promoted, stale, rejected }

final class _PromotionReceipt {
  const _PromotionReceipt({
    required this.disposition,
    required this.pathNodesVisited,
    this.invalidUtf16Offset,
  });

  final _PromotionDisposition disposition;
  final int pathNodesVisited;
  final int? invalidUtf16Offset;
}

final class _SourceAnchor {
  const _SourceAnchor({
    required this.pieceId,
    required this.originOffset,
    required this.globalOffset,
    required this.affinity,
  });

  final int pieceId;
  final int originOffset;
  final int globalOffset;
  final _Affinity affinity;

  _SourceAnchor transform(_Edit edit) {
    final delta = edit.replacement.length - (edit.end - edit.start);
    int next;
    if (globalOffset < edit.start) {
      next = globalOffset;
    } else if (globalOffset > edit.end) {
      next = globalOffset + delta;
    } else {
      next = affinity == _Affinity.upstream
          ? edit.start
          : edit.start + edit.replacement.length;
    }
    return _SourceAnchor(
      pieceId: pieceId,
      originOffset: originOffset,
      globalOffset: next,
      affinity: affinity,
    );
  }
}

enum _Affinity { upstream, downstream }

final class _SourceSession {
  _SourceSession({
    required String initialSource,
    required int historyByteBudget,
  }) : certified = _Snapshot.fromString(initialSource),
       logicalRevision = 0,
       history = _ByteHistory(historyByteBudget);

  _Snapshot certified;
  _Candidate? candidate;
  int logicalRevision;
  final _ByteHistory history;
  int _nextBackingId = 2;
  int _nextPieceId = 2;
  int _nextRequestId = 1;
  int? activeRequestId;

  _Node? get logicalRoot => candidate?.root ?? certified.root;
  int get logicalLength => logicalRoot?.utf16Length ?? 0;
  bool get isProvisional => candidate != null;
  _Summary get certifiedSummary => certified.summary;
  _Summary? get logicalSummary => logicalRoot?.summary ?? _Summary.empty;

  String readRange(int start, int end) => _readRange(logicalRoot, start, end);

  _SourceAnchor anchorAt(
    int offset, {
    _Affinity affinity = _Affinity.downstream,
  }) {
    if (offset < 0 || offset > logicalLength) {
      throw RangeError.range(offset, 0, logicalLength);
    }
    final located = _locatePiece(logicalRoot, offset, affinity);
    return _SourceAnchor(
      pieceId: located.piece.pieceId,
      originOffset: located.piece.originStart + located.localOffset,
      globalOffset: offset,
      affinity: affinity,
    );
  }

  _SessionEditReceipt replace(int start, int end, String replacement) {
    final before = _SessionState(
      certified: certified,
      candidate: candidate,
      logicalRevision: logicalRevision,
    );
    final edit = _Edit(
      start: start,
      end: end,
      replacement: replacement,
      backingId: _nextBackingId++,
      pieceId: _nextPieceId++,
    );
    final applied = _applyTreeEdit(logicalRoot, edit);
    history.push(before, edit.byteCharge);
    logicalRevision += 1;
    activeRequestId = null;

    if (candidate == null &&
        replacement.length <= _smallSyncLimit &&
        applied.root?.summary != null) {
      final summary = applied.root?.summary ?? _Summary.empty;
      certified = _Snapshot(
        root: applied.root,
        revision: logicalRevision,
        summary: summary,
      );
      return _SessionEditReceipt(
        disposition: _EditDisposition.certifiedSynchronously,
        work: applied.work,
        logicalRevision: logicalRevision,
        edit: edit,
      );
    }

    final existing = candidate;
    candidate = _Candidate(
      root: applied.root,
      baseCertifiedRevision:
          existing?.baseCertifiedRevision ?? certified.revision,
      logicalRevision: logicalRevision,
      journal: List.unmodifiable([...?existing?.journal, edit]),
    );
    return _SessionEditReceipt(
      disposition: _EditDisposition.provisional,
      work: applied.work,
      logicalRevision: logicalRevision,
      edit: edit,
    );
  }

  void undo() {
    final state = history.pop();
    certified = state.certified;
    candidate = state.candidate;
    logicalRevision = state.logicalRevision;
    activeRequestId = null;
  }

  Map<String, Object> createWorkerRequest() {
    final current = candidate;
    if (current == null) throw StateError('no provisional candidate');
    final requestId = _nextRequestId++;
    activeRequestId = requestId;
    return {
      'type': 'job',
      'id': requestId,
      'baseRevision': current.baseCertifiedRevision,
      'logicalRevision': current.logicalRevision,
      'journal': [for (final edit in current.journal) edit.toMessage()],
    };
  }

  _PromotionReceipt applyWorkerReply(Map<Object?, Object?> message) {
    final type = message['type']! as String;
    final requestId = message['id']! as int;
    final current = candidate;
    if (requestId != activeRequestId ||
        current == null ||
        message['baseRevision'] != current.baseCertifiedRevision ||
        message['logicalRevision'] != current.logicalRevision) {
      return const _PromotionReceipt(
        disposition: _PromotionDisposition.stale,
        pathNodesVisited: 0,
      );
    }
    if (type == 'rejected') {
      return _PromotionReceipt(
        disposition: _PromotionDisposition.rejected,
        pathNodesVisited: 0,
        invalidUtf16Offset: message['invalidUtf16']! as int,
      );
    }
    if (type != 'ack') {
      return const _PromotionReceipt(
        disposition: _PromotionDisposition.stale,
        pathNodesVisited: 0,
      );
    }

    var root = current.root;
    final work = _TreeWork();
    for (final raw in message['indexes']! as List<Object?>) {
      final entry = raw! as Map<Object?, Object?>;
      final globalStart = entry['globalStart']! as int;
      final expectedKey = entry['key']! as String;
      final leaf = _leafAtStart(root!, globalStart);
      if (leaf.piece.key != expectedKey) {
        return const _PromotionReceipt(
          disposition: _PromotionDisposition.stale,
          pathNodesVisited: 0,
        );
      }
      final index = _RangeIndex.fromTransferred(
        source: leaf.piece.backing.source,
        domainStart: leaf.piece.backingStart,
        domainLength: leaf.piece.length,
        data: entry['data']! as TransferableTypedData,
      );
      root = _attachIndexAt(root, globalStart, expectedKey, index, work);
    }
    final summary = root?.summary ?? _Summary.empty;
    final expected = message['summary']! as Map<Object?, Object?>;
    if (summary.utf16Length != expected['utf16'] ||
        summary.utf8Length != expected['utf8'] ||
        summary.lineBreaks != expected['lines'] ||
        summary.hash32 != expected['hash']) {
      throw StateError('worker/main source summary divergence');
    }
    certified = _Snapshot(
      root: root,
      revision: current.logicalRevision,
      summary: summary,
    );
    candidate = null;
    activeRequestId = null;
    return _PromotionReceipt(
      disposition: _PromotionDisposition.promoted,
      pathNodesVisited: work.nodesVisited,
    );
  }
}

final class _LocatedPiece {
  const _LocatedPiece(this.piece, this.localOffset);

  final _Piece piece;
  final int localOffset;
}

_LocatedPiece _locatePiece(_Node? node, int offset, _Affinity affinity) {
  if (node == null) {
    throw StateError('empty source has no piece anchor in this spike');
  }
  if (node case final _Leaf leaf) {
    return _LocatedPiece(leaf.piece, offset);
  }
  final branch = node as _Branch;
  if (offset < branch.left.utf16Length ||
      (offset == branch.left.utf16Length && affinity == _Affinity.upstream)) {
    return _locatePiece(branch.left, offset, affinity);
  }
  return _locatePiece(branch.right, offset - branch.left.utf16Length, affinity);
}

_Leaf _leafAtStart(_Node node, int globalStart) {
  if (node case final _Leaf leaf) {
    if (globalStart != 0) throw StateError('offset is not a leaf start');
    return leaf;
  }
  final branch = node as _Branch;
  if (globalStart < branch.left.utf16Length) {
    return _leafAtStart(branch.left, globalStart);
  }
  return _leafAtStart(branch.right, globalStart - branch.left.utf16Length);
}

final class _BackingDiagnostics {
  const _BackingDiagnostics(this.uniqueBytes, this.uniqueBackings);

  final int uniqueBytes;
  final int uniqueBackings;
}

_BackingDiagnostics _backingDiagnostics(_Node? root) {
  final seen = <int>{};
  var bytes = 0;
  void visit(_Node? node) {
    if (node == null) return;
    if (node case final _Leaf leaf) {
      if (seen.add(leaf.piece.backing.id)) {
        bytes += leaf.piece.backing.source.length;
      }
      return;
    }
    final branch = node as _Branch;
    visit(branch.left);
    visit(branch.right);
  }

  visit(root);
  return _BackingDiagnostics(bytes, seen.length);
}

final class _SourceWorker {
  _SourceWorker._(this._isolate, this._requests, this._responses) {
    _subscription = _responses.listen(_onMessage);
  }

  static Future<_SourceWorker> start(String initialSource) async {
    final responses = ReceivePort();
    final isolate = await Isolate.spawn<List<Object?>>(_sourceWorkerMain, [
      responses.sendPort,
      initialSource,
    ]);
    final iterator = StreamIterator<Object?>(responses);
    if (!await iterator.moveNext()) throw StateError('worker did not start');
    final requests = iterator.current! as SendPort;
    final remainder = ReceivePort();
    // A ReceivePort cannot be split after StreamIterator consumption. Route
    // subsequent worker messages through a dedicated response port instead.
    iterator.cancel();
    requests.send({'type': 'bind', 'reply': remainder.sendPort});
    return _SourceWorker._(isolate, requests, remainder);
  }

  final Isolate _isolate;
  final SendPort _requests;
  final ReceivePort _responses;
  late final StreamSubscription<Object?> _subscription;
  final Map<int, Completer<Map<Object?, Object?>>> _pending = {};

  Future<Map<Object?, Object?>> dispatch(Map<String, Object> request) {
    final id = request['id']! as int;
    final completer = Completer<Map<Object?, Object?>>();
    _pending[id] = completer;
    _requests.send(request);
    return completer.future;
  }

  void commit(int requestId, int revision) {
    _requests.send({'type': 'commit', 'id': requestId, 'revision': revision});
  }

  void applyCertifiedEdit(_Edit edit, int baseRevision, int revision) {
    _requests.send({
      'type': 'certifiedEdit',
      'baseRevision': baseRevision,
      'revision': revision,
      'edit': edit.toMessage(),
    });
  }

  Future<Map<Object?, Object?>> barrier(int id) {
    final completer = Completer<Map<Object?, Object?>>();
    _pending[id] = completer;
    _requests.send({'type': 'barrier', 'id': id});
    return completer.future;
  }

  void _onMessage(Object? raw) {
    final message = raw! as Map<Object?, Object?>;
    final id = message['id']! as int;
    _pending.remove(id)?.complete(message);
  }

  Future<void> close() async {
    _requests.send({'type': 'close'});
    await _subscription.cancel();
    _responses.close();
    _isolate.kill(priority: Isolate.immediate);
  }
}

void _sourceWorkerMain(List<Object?> initialization) {
  final bootstrapReply = initialization[0]! as SendPort;
  final initialSource = initialization[1]! as String;
  final requests = ReceivePort();
  bootstrapReply.send(requests.sendPort);

  var certified = _Snapshot.fromString(initialSource);
  var generation = 0;
  SendPort? reply;
  final completed = <int, _Node?>{};

  requests.listen((Object? raw) {
    final message = raw! as Map<Object?, Object?>;
    switch (message['type']) {
      case 'bind':
        reply = message['reply']! as SendPort;
      case 'job':
        generation += 1;
        final token = generation;
        unawaited(
          _runWorkerJob(
            certified: certified,
            message: message,
            token: token,
            generation: () => generation,
          ).then((result) {
            if (result['type'] == 'ack') {
              completed[result['id']! as int] =
                  result.remove('_root') as _Node?;
            }
            reply?.send(result);
          }),
        );
      case 'commit':
        final id = message['id']! as int;
        final root = completed.remove(id);
        if (root != null || certified.root != null) {
          final summary = root?.summary ?? _Summary.empty;
          certified = _Snapshot(
            root: root,
            revision: message['revision']! as int,
            summary: summary,
          );
        }
        completed.removeWhere((key, value) => key != id);
      case 'certifiedEdit':
        final baseRevision = message['baseRevision']! as int;
        if (baseRevision != certified.revision) {
          reply?.send({
            'type': 'workerError',
            'id': -1,
            'expected': certified.revision,
            'actual': baseRevision,
          });
          break;
        }
        final edit = _Edit.fromMessage(
          message['edit']! as Map<Object?, Object?>,
        );
        final root = _applyTreeEdit(certified.root, edit).root;
        certified = _Snapshot(
          root: root,
          revision: message['revision']! as int,
          summary: root?.summary ?? _Summary.empty,
        );
      case 'barrier':
        reply?.send({
          'type': 'barrier',
          'id': message['id']! as int,
          'revision': certified.revision,
        });
      case 'close':
        requests.close();
    }
  });
}

Future<Map<Object?, Object?>> _runWorkerJob({
  required _Snapshot certified,
  required Map<Object?, Object?> message,
  required int token,
  required int Function() generation,
}) async {
  final id = message['id']! as int;
  final baseRevision = message['baseRevision']! as int;
  final logicalRevision = message['logicalRevision']! as int;
  if (baseRevision != certified.revision) {
    return {
      'type': 'staleBase',
      'id': id,
      'baseRevision': baseRevision,
      'logicalRevision': logicalRevision,
    };
  }
  var root = certified.root;
  for (final raw in message['journal']! as List<Object?>) {
    final edit = _Edit.fromMessage(raw! as Map<Object?, Object?>);
    root = _applyTreeEdit(root, edit).root;
  }

  final indexes = <Map<String, Object>>[];
  for (final pending in _pendingLeaves(root)) {
    final piece = pending.leaf.piece;
    final builder = _RangeIndexBuilder(
      source: piece.backing.source,
      domainStart: piece.backingStart,
      domainLength: piece.length,
    );
    var polls = 0;
    while (!builder.isComplete) {
      builder.poll(_workerPollUtf16);
      polls += 1;
      await Future<void>.delayed(
        _workerThrottleEveryPolls > 0 && polls % _workerThrottleEveryPolls == 0
            ? const Duration(milliseconds: 1)
            : Duration.zero,
      );
      if (token != generation()) {
        return {
          'type': 'cancelled',
          'id': id,
          'baseRevision': baseRevision,
          'logicalRevision': logicalRevision,
        };
      }
    }
    if (builder.invalidRelativeOffset case final invalid?) {
      return {
        'type': 'rejected',
        'id': id,
        'baseRevision': baseRevision,
        'logicalRevision': logicalRevision,
        'invalidUtf16': pending.globalStart + invalid,
      };
    }
    final index = builder.buildIndex();
    final work = _TreeWork();
    root = _attachIndexAt(root!, pending.globalStart, piece.key, index, work);
    indexes.add({
      'globalStart': pending.globalStart,
      'key': piece.key,
      'data': builder.takeTransferable(),
    });
  }
  final summary = root?.summary ?? _Summary.empty;
  return {
    'type': 'ack',
    'id': id,
    'baseRevision': baseRevision,
    'logicalRevision': logicalRevision,
    'summary': summary.toMessage(),
    'indexes': indexes,
    '_root': root,
  };
}

enum _InitialOpenPolicy { preserveSourceSpelling, normalizeBeforeInteractive }

enum _InitialOpenPlan {
  interactiveRaw,
  synchronousNormalize,
  stagedAsyncNormalize,
}

_InitialOpenPlan _initialOpenPlan(int utf16Length, _InitialOpenPolicy policy) {
  return switch (policy) {
    _InitialOpenPolicy.preserveSourceSpelling =>
      _InitialOpenPlan.interactiveRaw,
    _InitialOpenPolicy.normalizeBeforeInteractive =>
      utf16Length <= _smallSyncLimit
          ? _InitialOpenPlan.synchronousNormalize
          : _InitialOpenPlan.stagedAsyncNormalize,
  };
}

void _verifyPersistentTreeSemantics() {
  const source = 'a\r\nb\rc\n😀 café';
  final index = _RangeIndex.buildSync(source, 0, source.length);
  final summary = index.summaryFor(0, source.length);
  _expect(summary.utf8Length == utf8.encode(source).length, 'exact UTF-8');
  _expect(summary.lineCount == 4, 'CRLF and CR line truth');

  final session = _SourceSession(
    initialSource: source,
    historyByteBudget: 1024 * 1024,
  );
  final anchor = session.anchorAt(4);
  final receipt = session.replace(0, 0, 'x');
  _expect(
    receipt.disposition == _EditDisposition.certifiedSynchronously,
    'small ordinary edit certifies synchronously',
  );
  _expect(!session.isProvisional, 'ordinary API remains certified');
  _expect(session.readRange(1, session.logicalLength) == source, 'exact edit');
  _expectSummaryMatches(session, 'x$source');
  _expect(
    anchor
            .transform(
              const _Edit(
                start: 0,
                end: 0,
                replacement: 'x',
                backingId: 0,
                pieceId: 0,
              ),
            )
            .globalOffset ==
        5,
    'stable active anchor transforms in O(1)',
  );

  final boundarySession = _SourceSession(
    initialSource: 'a\rb',
    historyByteBudget: 1024,
  );
  boundarySession.replace(2, 2, '\n');
  _expectSummaryMatches(boundarySession, 'a\r\nb');
  boundarySession.replace(1, 2, '');
  _expectSummaryMatches(boundarySession, 'a\nb');

  final undoSession = _SourceSession(
    initialSource: _asciiMarkdownOfLength(2 * 1024 * 1024),
    historyByteBudget: 64 * 1024,
  );
  final originalLength = undoSession.logicalLength;
  undoSession.replace(4096, originalLength - 4096, '');
  _expect(undoSession.logicalLength == 8192, 'large deletion exact');
  _expect(
    _backingDiagnostics(undoSession.logicalRoot).uniqueBytes == 8192,
    'current root compacts only bounded survivors',
  );
  final retainedWithUndo = _sessionRetainedBackingBytes(undoSession);
  _expect(
    retainedWithUndo >= originalLength,
    'latest history entry intentionally pins undo backing',
  );
  _expect(undoSession.history.overBudget, 'latest oversized undo is retained');
  undoSession.undo();
  _expect(
    undoSession.logicalLength == originalLength,
    'oversized immediate undo',
  );

  final evictionSession = _SourceSession(
    initialSource: _asciiMarkdownOfLength(2 * 1024 * 1024),
    historyByteBudget: 64 * 1024,
  );
  evictionSession.replace(4096, evictionSession.logicalLength - 4096, '');
  evictionSession.replace(0, 0, 'x');
  _expect(evictionSession.history.evictions == 1, 'old oversized root evicted');
  final retainedAfterEviction = _sessionRetainedBackingBytes(evictionSession);
  _expect(
    retainedAfterEviction < 128 * 1024,
    'history eviction releases old bulk backing from session roots',
  );

  _expect(
    _initialOpenPlan(
          100 * 1024 * 1024,
          _InitialOpenPolicy.normalizeBeforeInteractive,
        ) ==
        _InitialOpenPlan.stagedAsyncNormalize,
    'large normalized open is explicitly staged',
  );
  _expect(
    _initialOpenPlan(
          100 * 1024 * 1024,
          _InitialOpenPolicy.preserveSourceSpelling,
        ) ==
        _InitialOpenPlan.interactiveRaw,
    'preserved-spelling open can be interactive immediately',
  );
  _emit('persistent_source_semantic_gate', {
    'ordinary_edit_synchronously_certified': true,
    'current_bytes_after_large_delete': 8192,
    'bytes_with_immediate_undo_root': retainedWithUndo,
    'bytes_after_history_eviction': retainedAfterEviction,
    'history_evictions': evictionSession.history.evictions,
    'large_normalized_open': _InitialOpenPlan.stagedAsyncNormalize.name,
    'large_preserved_open': _InitialOpenPlan.interactiveRaw.name,
  });
}

void _expectSummaryMatches(_SourceSession session, String oracle) {
  _expect(
    session.readRange(0, session.logicalLength) == oracle,
    'source oracle',
  );
  final expected = _RangeIndex.buildSync(
    oracle,
    0,
    oracle.length,
  ).summaryFor(0, oracle.length);
  final actual = session.certifiedSummary;
  _expect(actual.utf16Length == expected.utf16Length, 'UTF-16 summary oracle');
  _expect(actual.utf8Length == expected.utf8Length, 'UTF-8 summary oracle');
  _expect(actual.lineBreaks == expected.lineBreaks, 'line summary oracle');
  _expect(actual.hash32 == expected.hash32, 'hash summary oracle');
}

int _sessionRetainedBackingBytes(_SourceSession session) {
  final seen = <int>{};
  var bytes = 0;
  void addRoot(_Node? root) {
    void visit(_Node? node) {
      if (node == null) return;
      if (node case final _Leaf leaf) {
        if (seen.add(leaf.piece.backing.id)) {
          bytes += leaf.piece.backing.source.length;
        }
        return;
      }
      final branch = node as _Branch;
      visit(branch.left);
      visit(branch.right);
    }

    visit(root);
  }

  addRoot(session.logicalRoot);
  for (final entry in session.history._entries) {
    addRoot(entry.state.certified.root);
    addRoot(entry.state.candidate?.root);
  }
  return bytes;
}

Future<void> _verifyCandidateWorkerSemantics(String bulk) async {
  const initial = 'head\n';
  final session = _SourceSession(
    initialSource: initial,
    historyByteBudget: math.max(32 * 1024 * 1024, bulk.length * 2),
  );
  final worker = await _SourceWorker.start(initial);
  try {
    final pasteWatch = Stopwatch()..start();
    final paste = session.replace(
      session.logicalLength,
      session.logicalLength,
      bulk,
    );
    pasteWatch.stop();
    _expect(
      paste.disposition == _EditDisposition.provisional,
      'bulk paste is provisional',
    );
    final request1 = session.createWorkerRequest();
    final firstFuture = worker.dispatch(request1);

    final boundary = initial.length;
    final crossReadWatch = Stopwatch()..start();
    final crossRead = session.readRange(boundary - 2, boundary + 2);
    crossReadWatch.stop();
    _expect(
      crossRead ==
          '${initial.substring(initial.length - 2)}${bulk.substring(0, 2)}',
      'cross-piece read works before ack',
    );
    final caret = session.anchorAt(session.logicalLength);
    final backspaceWatch = Stopwatch()..start();
    final backspace = session.replace(
      session.logicalLength - 1,
      session.logicalLength,
      '',
    );
    backspaceWatch.stop();
    final transformed = caret.transform(
      _Edit(
        start: session.logicalLength,
        end: session.logicalLength + 1,
        replacement: '',
        backingId: 0,
        pieceId: 0,
      ),
    );
    _expect(
      backspace.disposition == _EditDisposition.provisional,
      'pending edit',
    );
    _expect(transformed.globalOffset == session.logicalLength, 'caret exact');
    final afterBackspace = session.readRange(
      session.logicalLength - 16,
      session.logicalLength,
    );
    final undoWatch = Stopwatch()..start();
    session.undo();
    undoWatch.stop();
    _expect(
      session.readRange(session.logicalLength - 16, session.logicalLength) !=
          afterBackspace,
      'undo before worker ack restores exact candidate',
    );
    session.replace(session.logicalLength - 1, session.logicalLength, '');
    final request2 = session.createWorkerRequest();
    final dispatch = Stopwatch()..start();
    final secondFuture = worker.dispatch(request2);
    dispatch.stop();

    final workerWatch = Stopwatch()..start();
    var lastHeartbeatUs = 0;
    var maxHeartbeatGapUs = 0;
    final heartbeat = Timer.periodic(const Duration(milliseconds: 1), (_) {
      final now = _microseconds(workerWatch);
      maxHeartbeatGapUs = math.max(maxHeartbeatGapUs, now - lastHeartbeatUs);
      lastHeartbeatUs = now;
    });
    final secondReply = await secondFuture;
    maxHeartbeatGapUs = math.max(
      maxHeartbeatGapUs,
      _microseconds(workerWatch) - lastHeartbeatUs,
    );
    heartbeat.cancel();
    workerWatch.stop();
    final promotionWatch = Stopwatch()..start();
    final promotion = session.applyWorkerReply(secondReply);
    promotionWatch.stop();
    _expect(
      promotion.disposition == _PromotionDisposition.promoted,
      'latest candidate atomically promotes',
    );
    _expect(!session.isProvisional, 'provisional layer ends after promotion');
    worker.commit(request2['id']! as int, session.certified.revision);

    final firstReply = await firstFuture;
    final stale = session.applyWorkerReply(firstReply);
    _expect(
      stale.disposition == _PromotionDisposition.stale,
      'old reply rejected',
    );
    _expect(
      session.certifiedSummary.utf16Length == initial.length + bulk.length - 1,
      'certified extent exact',
    );

    var maxPath = 0;
    final activeTimings = <int>[];
    final activeOffset = initial.length + bulk.length ~/ 2;
    for (var index = 0; index < 1000; index += 1) {
      final stopwatch = Stopwatch()..start();
      final local = session.replace(
        activeOffset,
        activeOffset + 1,
        index.isEven ? 'x' : 'y',
      );
      worker.applyCertifiedEdit(
        local.edit,
        local.logicalRevision - 1,
        local.logicalRevision,
      );
      stopwatch.stop();
      activeTimings.add(_nanoseconds(stopwatch));
      _expect(
        local.disposition == _EditDisposition.certifiedSynchronously,
        'active post-promotion edit remains synchronous',
      );
      maxPath = math.max(maxPath, local.work.nodesVisited);
    }

    var seed = 0x13579BDF;
    final coldTimings = <int>[];
    for (var index = 0; index < 1000; index += 1) {
      seed = _next(seed);
      final offset = initial.length + seed % (bulk.length - 1);
      final stopwatch = Stopwatch()..start();
      final local = session.replace(
        offset,
        offset + 1,
        index.isEven ? 'x' : 'y',
      );
      worker.applyCertifiedEdit(
        local.edit,
        local.logicalRevision - 1,
        local.logicalRevision,
      );
      stopwatch.stop();
      coldTimings.add(_nanoseconds(stopwatch));
      _expect(
        local.disposition == _EditDisposition.certifiedSynchronously,
        'post-promotion ordinary edit remains synchronous',
      );
      maxPath = math.max(maxPath, local.work.nodesVisited);
    }
    final barrier = await worker.barrier(0x70000001);
    _expect(
      barrier['revision'] == session.certified.revision,
      'worker mirror consumes ordinary certified edits in order',
    );

    final secondBulk = _asciiMarkdownOfLength(16 * 1024);
    session.replace(session.logicalLength, session.logicalLength, secondBulk);
    final thirdRequest = session.createWorkerRequest();
    final thirdReply = await worker.dispatch(thirdRequest);
    final thirdPromotion = session.applyWorkerReply(thirdReply);
    _expect(
      thirdPromotion.disposition == _PromotionDisposition.promoted,
      'later bulk job starts from mirrored certified revision',
    );
    worker.commit(thirdRequest['id']! as int, session.certified.revision);
    _expect(!session.isProvisional, 'candidate state stays contained');
    final activeSamples = _Samples(activeTimings);
    final coldSamples = _Samples(coldTimings);
    _emit('candidate_worker_receipt', {
      'bulk_utf16': bulk.length,
      'provisional_adoption_us': _microseconds(pasteWatch),
      'pre_ack_cross_piece_read_us': _microseconds(crossReadWatch),
      'pre_ack_backspace_us': _microseconds(backspaceWatch),
      'pre_ack_undo_us': _microseconds(undoWatch),
      'dispatch_us': _microseconds(dispatch),
      'worker_ms': workerWatch.elapsedMicroseconds / 1000,
      'main_heartbeat_max_gap_us': maxHeartbeatGapUs,
      'promotion_us': _microseconds(promotionWatch),
      'promotion_path_nodes': promotion.pathNodesVisited,
      'post_promotion_active_edits': 1000,
      'active_edit_p50_us':
          activeSamples._values[(activeSamples._values.length - 1) ~/ 2] / 1000,
      'active_edit_p99_us':
          activeSamples._values[((activeSamples._values.length - 1) * 99) ~/
              100] /
          1000,
      'active_edit_p999_us':
          activeSamples._values[((activeSamples._values.length - 1) * 999) ~/
              1000] /
          1000,
      'active_edit_max_us': activeSamples._values.last / 1000,
      'post_promotion_cold_edits': 1000,
      'cold_edit_p50_us':
          coldSamples._values[(coldSamples._values.length - 1) ~/ 2] / 1000,
      'cold_edit_p99_us':
          coldSamples._values[((coldSamples._values.length - 1) * 99) ~/ 100] /
          1000,
      'cold_edit_p999_us':
          coldSamples._values[((coldSamples._values.length - 1) * 999) ~/
              1000] /
          1000,
      'cold_edit_max_us': coldSamples._values.last / 1000,
      'post_promotion_max_nodes': maxPath,
      'later_bulk_promoted': true,
      'tree_height': session.logicalRoot?.height ?? 0,
      'piece_count': session.logicalRoot?.pieceCount ?? 0,
      'checkpoint_bytes':
          ((bulk.length + _checkpointUtf16 - 1) ~/ _checkpointUtf16) * 16,
    });
  } finally {
    await worker.close();
  }

  await _verifyMalformedWorkerPaths();
  await _verifyCrlfHotPaste();
}

Future<void> _verifyMalformedWorkerPaths() async {
  const initial = 'base:';
  final prefix = _repeatCodeUnit(0x61, 1024 * 1024);
  final suffix = _repeatCodeUnit(0x62, 1024 * 1024);
  final malformed = '$prefix${String.fromCharCode(0xD800)}$suffix';

  final session = _SourceSession(
    initialSource: initial,
    historyByteBudget: 8 * 1024 * 1024,
  );
  final worker = await _SourceWorker.start(initial);
  try {
    session.replace(initial.length, initial.length, malformed);
    final firstRequest = session.createWorkerRequest();
    final firstFuture = worker.dispatch(firstRequest);

    final invalidGlobal = initial.length + prefix.length;
    session.replace(invalidGlobal, invalidGlobal + 1, '');
    final secondRequest = session.createWorkerRequest();
    final secondReply = await worker.dispatch(secondRequest);
    final promoted = session.applyWorkerReply(secondReply);
    _expect(
      promoted.disposition == _PromotionDisposition.promoted,
      'live-piece validation skips a deleted malformed range',
    );
    worker.commit(secondRequest['id']! as int, session.certified.revision);
    _expect(
      session.readRange(invalidGlobal - 2, invalidGlobal + 2) == 'aabb',
      'malformed deletion joins exact live ranges',
    );
    final old = await firstFuture;
    _expect(
      session.applyWorkerReply(old).disposition == _PromotionDisposition.stale,
      'pre-delete validation result cannot overwrite final source',
    );
  } finally {
    await worker.close();
  }

  final rejectedSession = _SourceSession(
    initialSource: initial,
    historyByteBudget: 8 * 1024 * 1024,
  );
  final rejectedWorker = await _SourceWorker.start(initial);
  try {
    rejectedSession.replace(initial.length, initial.length, malformed);
    final request = rejectedSession.createWorkerRequest();
    final reply = await rejectedWorker.dispatch(request);
    final rejected = rejectedSession.applyWorkerReply(reply);
    _expect(
      rejected.disposition == _PromotionDisposition.rejected &&
          rejected.invalidUtf16Offset == initial.length + prefix.length,
      'unpaired surrogate rejects at exact logical offset',
    );
    _expect(rejectedSession.isProvisional, 'invalid candidate never commits');
    _expect(
      rejectedSession.certified.revision == 0,
      'last certified snapshot remains canonical',
    );
    _emit('malformed_candidate_gate', {
      'deleted_before_scan_promoted': true,
      'undeleted_rejected': true,
      'invalid_utf16_offset': rejected.invalidUtf16Offset,
      'last_certified_revision': rejectedSession.certified.revision,
    });
  } finally {
    await rejectedWorker.close();
  }
}

Future<void> _verifyCrlfHotPaste() async {
  const initial = 'base\n';
  final crlf = _repeatString('\r\nline\r', 2 * 1024 * 1024);
  final session = _SourceSession(
    initialSource: initial,
    historyByteBudget: 8 * 1024 * 1024,
  );
  final worker = await _SourceWorker.start(initial);
  try {
    session.replace(initial.length, initial.length, crlf);
    _expect(
      session.readRange(initial.length, initial.length + 2) == '\r\n',
      'hot paste preserves exact CRLF spelling',
    );
    final request = session.createWorkerRequest();
    final reply = await worker.dispatch(request);
    final result = session.applyWorkerReply(reply);
    _expect(
      result.disposition == _PromotionDisposition.promoted,
      'CRLF promote',
    );
    _expect(
      session.certifiedSummary.lineCount == 2 + _logicalLineBreaksOracle(crlf),
      'CRLF and lone CR count as logical line breaks',
    );
    _emit('crlf_policy_gate', {
      'hot_paste_preserves_spelling': true,
      'paste_utf16': crlf.length,
      'certified_line_count': session.certifiedSummary.lineCount,
      'large_initial_normalized_open':
          _InitialOpenPlan.stagedAsyncNormalize.name,
    });
  } finally {
    await worker.close();
  }
}

void _runDeepPromotionReceipt() {
  final session = _SourceSession(
    initialSource: _asciiMarkdownOfLength(64 * 1024),
    historyByteBudget: 4 * 1024 * 1024,
  );
  for (var index = 0; index < 8192; index += 1) {
    session.replace(
      session.logicalLength,
      session.logicalLength,
      index.isEven ? 'x' : 'y',
    );
  }
  session.replace(
    session.logicalLength,
    session.logicalLength,
    _asciiMarkdownOfLength(64 * 1024),
  );
  final request = session.createWorkerRequest();
  final pending = _pendingLeaves(session.logicalRoot).single;
  final piece = pending.leaf.piece;
  final builder = _RangeIndexBuilder(
    source: piece.backing.source,
    domainStart: piece.backingStart,
    domainLength: piece.length,
  );
  while (!builder.isComplete) {
    builder.poll(_workerPollUtf16);
  }
  final workerIndex = builder.buildIndex();
  final workerWork = _TreeWork();
  final expectedRoot = _attachIndexAt(
    session.logicalRoot!,
    pending.globalStart,
    piece.key,
    workerIndex,
    workerWork,
  );
  final reply = <Object?, Object?>{
    'type': 'ack',
    'id': request['id'],
    'baseRevision': request['baseRevision'],
    'logicalRevision': request['logicalRevision'],
    'summary': expectedRoot.summary!.toMessage(),
    'indexes': <Object?>[
      <Object?, Object?>{
        'globalStart': pending.globalStart,
        'key': piece.key,
        'data': builder.takeTransferable(),
      },
    ],
  };
  final stopwatch = Stopwatch()..start();
  final promotion = session.applyWorkerReply(reply);
  stopwatch.stop();
  _expect(
    promotion.disposition == _PromotionDisposition.promoted,
    'deep tree promotion succeeds',
  );
  _expect(
    promotion.pathNodesVisited <= (session.logicalRoot?.height ?? 0) + 1,
    'promotion touches one logarithmic root-to-leaf path',
  );
  _emit('deep_tree_atomic_promotion', {
    'preexisting_pieces': 8193,
    'tree_height': session.logicalRoot?.height ?? 0,
    'path_nodes_visited': promotion.pathNodesVisited,
    'promotion_us': _microseconds(stopwatch),
    'checkpoint_bytes': builder._records.length * 4,
  });
}

void _runRepeatedEditReceipts(int edits) {
  final session = _SourceSession(
    initialSource: _asciiMarkdownOfLength(64 * 1024),
    historyByteBudget: 4 * 1024 * 1024,
  );
  var caret = session.logicalLength ~/ 2;
  final timings = <int>[];
  var maxNodes = 0;
  var firstNodes = 0;
  var lastNodes = 0;
  for (var index = 0; index < edits; index += 1) {
    final stopwatch = Stopwatch()..start();
    final receipt = session.replace(caret, caret, index.isEven ? 'x' : 'y');
    stopwatch.stop();
    timings.add(_nanoseconds(stopwatch));
    caret += 1;
    maxNodes = math.max(maxNodes, receipt.work.nodesVisited);
    if (index < math.min(100, edits)) firstNodes += receipt.work.nodesVisited;
    if (index >= math.max(0, edits - 100)) {
      lastNodes += receipt.work.nodesVisited;
    }
    _blackHole ^= session.certifiedSummary.hash32;
  }
  final samples = _Samples(timings);
  _emit('persistent_sum_tree_repeated_edits', {
    'edits': edits,
    ...samples.json,
    'first_100_average_nodes': firstNodes / math.min(100, edits),
    'last_100_average_nodes': lastNodes / math.min(100, edits),
    'max_nodes_visited': maxNodes,
    'tree_height': session.logicalRoot?.height ?? 0,
    'piece_count': session.logicalRoot?.pieceCount ?? 0,
    'provisional_after': session.isProvisional,
    'history_entries': session.history.length,
    'history_evictions': session.history.evictions,
    'rss_mib': _rssMiB(),
  });
}

final class _Samples {
  _Samples(List<int> values) : _values = [...values]..sort() {
    if (_values.isEmpty) throw ArgumentError.value(values);
  }

  final List<int> _values;

  Map<String, Object> get json => {
    'p50_us': _values[(_values.length - 1) ~/ 2] / 1000,
    'p99_us': _values[((_values.length - 1) * 99) ~/ 100] / 1000,
    'p999_us': _values[((_values.length - 1) * 999) ~/ 1000] / 1000,
    'max_us': _values.last / 1000,
  };
}

enum _GateHistoryModel { inverse, roots }

final class _GateReplacementIndexes {
  final Map<String, _RangeIndex> _values = <String, _RangeIndex>{};

  _RangeIndex forValue(String value) => _values.putIfAbsent(
    value,
    () => _RangeIndex.buildSync(value, 0, value.length),
  );

  void warm(Iterable<String> values) {
    for (final value in values) {
      if (value.isNotEmpty) forValue(value);
    }
  }
}

final _gateReplacementIndexes = _GateReplacementIndexes();

_Node? _applyGateTreeEdit({
  required _Node? root,
  required int start,
  required int end,
  required String replacement,
  required int backingId,
  required int pieceId,
  required _TreeWork work,
}) {
  final length = root?.utf16Length ?? 0;
  if (start < 0 || end < start || end > length) {
    throw RangeError.range(end, start, length);
  }
  _checkBoundary(root, start);
  _checkBoundary(root, end);
  if (replacement.isNotEmpty) {
    // Building the index also validates scalar boundaries. Benchmark strings
    // are warmed before the sample clock starts, so this is a lookup there.
    _gateReplacementIndexes.forValue(replacement);
  }
  work.reset();
  final first = _split(root, start, work);
  final second = _split(first.right, end - start, work);
  var retainedLeft = first.left;
  var retainedRight = second.right;
  if (end > start) {
    var budget = _smallSyncLimit;
    final leftCompaction = _compactEdge(
      retainedLeft,
      rightmost: true,
      budget: budget,
      newBackingId: -(backingId * 2),
      work: work,
    );
    retainedLeft = leftCompaction.root;
    budget -= leftCompaction.copiedUtf16;
    final rightCompaction = _compactEdge(
      retainedRight,
      rightmost: false,
      budget: budget,
      newBackingId: -(backingId * 2 + 1),
      work: work,
    );
    retainedRight = rightCompaction.root;
  }
  _Node? inserted;
  if (replacement.isNotEmpty) {
    final backing = _Backing(id: backingId, source: replacement);
    inserted = _Leaf(
      _Piece(
        pieceId: pieceId,
        originStart: 0,
        backing: backing,
        backingStart: 0,
        length: replacement.length,
        index: _gateReplacementIndexes.forValue(replacement),
      ),
    );
    work.leavesAllocated += 1;
  }
  return _concat(_concat(retainedLeft, inserted, work), retainedRight, work);
}

/// Fixed storage for inverse source transactions. It retains deleted values,
/// never old tree roots. A null payload means an empty deletion and an int is
/// the common single-code-unit case, avoiding a tiny String allocation.
final class _GateInverseRing {
  _GateInverseRing({
    required this.entryCapacity,
    required this.maxOperationsPerEntry,
    required this.byteBudget,
  }) : _keys = Int64List(entryCapacity),
       _operationCounts = Uint16List(entryCapacity),
       _entryCharges = Uint64List(entryCapacity),
       _starts = Uint32List(entryCapacity * maxOperationsPerEntry),
       _insertedLengths = Uint32List(entryCapacity * maxOperationsPerEntry),
       _deleted = List<Object?>.filled(
         entryCapacity * maxOperationsPerEntry,
         null,
       );

  final int entryCapacity;
  final int maxOperationsPerEntry;
  final int byteBudget;
  final Int64List _keys;
  final Uint16List _operationCounts;
  final Uint64List _entryCharges;
  final Uint32List _starts;
  final Uint32List _insertedLengths;
  final List<Object?> _deleted;

  int _head = 0;
  int length = 0;
  int chargedBytes = 0;
  int evictions = 0;
  int highWaterEntries = 0;
  int highWaterChargedBytes = 0;

  bool get isEmpty => length == 0;
  bool get overBudget => chargedBytes > byteBudget;

  int begin(int key, int expectedOperations) {
    if (expectedOperations < 1 || expectedOperations > maxOperationsPerEntry) {
      throw RangeError.range(expectedOperations, 1, maxOperationsPerEntry);
    }
    if (length > 0) {
      final newest = (_head + length - 1) % entryCapacity;
      if (_keys[newest] == key &&
          _operationCounts[newest] + expectedOperations <=
              maxOperationsPerEntry) {
        return newest;
      }
    }
    if (length == entryCapacity) _evictOldest();
    final slot = (_head + length) % entryCapacity;
    _clearSlot(slot);
    _keys[slot] = key;
    _entryCharges[slot] = 24;
    chargedBytes += 24;
    length += 1;
    highWaterEntries = math.max(highWaterEntries, length);
    highWaterChargedBytes = math.max(highWaterChargedBytes, chargedBytes);
    return slot;
  }

  void append({
    required int slot,
    required int start,
    required int insertedLength,
    required Object? deleted,
  }) {
    final operation = _operationCounts[slot];
    if (operation >= maxOperationsPerEntry) {
      throw StateError('inverse transaction operation capacity exhausted');
    }
    final flat = slot * maxOperationsPerEntry + operation;
    _starts[flat] = start;
    _insertedLengths[flat] = insertedLength;
    _deleted[flat] = deleted;
    _operationCounts[slot] = operation + 1;
    final payloadUtf16 = switch (deleted) {
      null => 0,
      int _ => 1,
      String value => value.length,
      _ => throw StateError('unsupported inverse payload'),
    };
    final charge = 24 + payloadUtf16 * 2;
    _entryCharges[slot] += charge;
    chargedBytes += charge;
    highWaterChargedBytes = math.max(highWaterChargedBytes, chargedBytes);
    while (chargedBytes > byteBudget && length > 1) {
      _evictOldest();
    }
  }

  int get newestSlot {
    if (length == 0) throw StateError('nothing to undo');
    return (_head + length - 1) % entryCapacity;
  }

  int operationCount(int slot) => _operationCounts[slot];

  int startAt(int slot, int operation) =>
      _starts[slot * maxOperationsPerEntry + operation];

  int insertedLengthAt(int slot, int operation) =>
      _insertedLengths[slot * maxOperationsPerEntry + operation];

  String deletedAt(int slot, int operation) {
    final value = _deleted[slot * maxOperationsPerEntry + operation];
    return switch (value) {
      null => '',
      int codeUnit => String.fromCharCode(codeUnit),
      String source => source,
      _ => throw StateError('unsupported inverse payload'),
    };
  }

  int get retainedPayloadUtf16 {
    var total = 0;
    for (var entry = 0; entry < length; entry += 1) {
      final slot = (_head + entry) % entryCapacity;
      for (
        var operation = 0;
        operation < _operationCounts[slot];
        operation += 1
      ) {
        final value = _deleted[slot * maxOperationsPerEntry + operation];
        total += switch (value) {
          null => 0,
          int _ => 1,
          String source => source.length,
          _ => throw StateError('unsupported inverse payload'),
        };
      }
    }
    return total;
  }

  void removeNewest() {
    final slot = newestSlot;
    chargedBytes -= _entryCharges[slot];
    _clearSlot(slot);
    length -= 1;
    if (length == 0) _head = 0;
  }

  void clear() {
    while (length > 0) {
      removeNewest();
    }
  }

  void _evictOldest() {
    if (length == 0) return;
    final slot = _head;
    chargedBytes -= _entryCharges[slot];
    _clearSlot(slot);
    _head = (_head + 1) % entryCapacity;
    length -= 1;
    evictions += 1;
  }

  void _clearSlot(int slot) {
    final base = slot * maxOperationsPerEntry;
    for (
      var operation = 0;
      operation < _operationCounts[slot];
      operation += 1
    ) {
      _deleted[base + operation] = null;
    }
    _operationCounts[slot] = 0;
    _entryCharges[slot] = 0;
    _keys[slot] = 0;
  }
}

/// Fair root-history control. It records one old root per product transaction,
/// using the same fixed entry capacity and functional edit kernel as the
/// inverse model.
final class _GateRootRing {
  _GateRootRing(this.capacity)
    : _roots = List<_Node?>.filled(capacity, null),
      _keys = Int64List(capacity);

  final int capacity;
  final List<_Node?> _roots;
  final Int64List _keys;
  int _head = 0;
  int length = 0;
  int evictions = 0;
  int highWaterEntries = 0;

  void begin(int key, _Node? root) {
    if (length > 0) {
      final newest = (_head + length - 1) % capacity;
      if (_keys[newest] == key) return;
    }
    if (length == capacity) {
      _roots[_head] = null;
      _keys[_head] = 0;
      _head = (_head + 1) % capacity;
      length -= 1;
      evictions += 1;
    }
    final slot = (_head + length) % capacity;
    _roots[slot] = root;
    _keys[slot] = key;
    length += 1;
    highWaterEntries = math.max(highWaterEntries, length);
  }

  void forEachRoot(void Function(_Node? root) visitor) {
    for (var index = 0; index < length; index += 1) {
      visitor(_roots[(_head + index) % capacity]);
    }
  }

  void clear() {
    for (var index = 0; index < length; index += 1) {
      final slot = (_head + index) % capacity;
      _roots[slot] = null;
      _keys[slot] = 0;
    }
    _head = 0;
    length = 0;
  }
}

final class _GateOwnershipDiagnostics {
  const _GateOwnershipDiagnostics({
    required this.nodes,
    required this.branches,
    required this.leaves,
    required this.backingObjects,
    required this.uniqueSourceStrings,
    required this.uniqueSourceUtf16,
  });

  final int nodes;
  final int branches;
  final int leaves;
  final int backingObjects;
  final int uniqueSourceStrings;
  final int uniqueSourceUtf16;
}

_GateOwnershipDiagnostics _gateOwnershipDiagnostics(
  _Node? current,
  _GateRootRing? history,
) {
  final nodes = HashSet<_Node>.identity();
  final backings = HashSet<_Backing>.identity();
  final sources = HashSet<String>.identity();
  var sourceUtf16 = 0;
  var branches = 0;
  var leaves = 0;

  void visit(_Node? node) {
    if (node == null || !nodes.add(node)) return;
    if (node case final _Leaf leaf) {
      leaves += 1;
      backings.add(leaf.piece.backing);
      if (sources.add(leaf.piece.backing.source)) {
        sourceUtf16 += leaf.piece.backing.source.length;
      }
      return;
    }
    branches += 1;
    final branch = node as _Branch;
    visit(branch.left);
    visit(branch.right);
  }

  visit(current);
  history?.forEachRoot(visit);
  return _GateOwnershipDiagnostics(
    nodes: nodes.length,
    branches: branches,
    leaves: leaves,
    backingObjects: backings.length,
    uniqueSourceStrings: sources.length,
    uniqueSourceUtf16: sourceUtf16,
  );
}

final class _GateSourceSession {
  _GateSourceSession({
    required _Node? initialRoot,
    required this.model,
    required int historyEntries,
    required int historyBytes,
    required int historyOperations,
  }) : root = initialRoot,
       _inverse = model == _GateHistoryModel.inverse
           ? _GateInverseRing(
               entryCapacity: historyEntries,
               maxOperationsPerEntry: historyOperations,
               byteBudget: historyBytes,
             )
           : null,
       _roots = model == _GateHistoryModel.roots
           ? _GateRootRing(historyEntries)
           : null,
       totalNodeAllocations = initialRoot == null ? 0 : 1;

  factory _GateSourceSession.fromIndexed({
    required String source,
    required _RangeIndex index,
    required _GateHistoryModel model,
    required int historyEntries,
    required int historyBytes,
    required int historyOperations,
  }) {
    final root = source.isEmpty
        ? null
        : _Leaf(
            _Piece(
              pieceId: 1,
              originStart: 0,
              backing: _Backing(id: 1, source: source),
              backingStart: 0,
              length: source.length,
              index: index,
            ),
          );
    return _GateSourceSession(
      initialRoot: root,
      model: model,
      historyEntries: historyEntries,
      historyBytes: historyBytes,
      historyOperations: historyOperations,
    );
  }

  _Node? root;
  final _GateHistoryModel model;
  final _GateInverseRing? _inverse;
  final _GateRootRing? _roots;
  final _TreeWork _work = _TreeWork();
  int revision = 0;
  int _nextBackingId = 2;
  int _nextPieceId = 2;
  int activeAnchor = 0;
  int selectionAnchor = 0;
  _Affinity activeAffinity = _Affinity.downstream;
  _Affinity selectionAffinity = _Affinity.upstream;

  int totalNodeAllocations;
  int totalBranchesAllocated = 0;
  int totalLeavesAllocated = 0;
  int maxNodesVisited = 0;
  int maxNodesAllocated = 0;
  int lastForwardBaseRevision = 0;
  int lastForwardRevision = 0;
  int lastForwardOperations = 0;
  bool lastForwardWasUndo = false;

  int get length => root?.utf16Length ?? 0;
  _Summary get summary => root?.summary ?? _Summary.empty;
  int get historyEntries => _inverse?.length ?? _roots?.length ?? 0;
  int get historyEvictions => _inverse?.evictions ?? _roots?.evictions ?? 0;
  int get historyChargedBytes => _inverse?.chargedBytes ?? 0;
  int get historyPayloadUtf16 => _inverse?.retainedPayloadUtf16 ?? 0;
  bool get historyOverBudget => _inverse?.overBudget ?? false;

  String readRange(int start, int end) => _readRange(root, start, end);

  void replace(
    int start,
    int end,
    String replacement, {
    required int transactionKey,
  }) {
    _prepareOperations(
      Uint32List.fromList(<int>[start]),
      Uint32List.fromList(<int>[end]),
      <String>[replacement],
      1,
      transactionKey,
    );
  }

  /// Allocation-neutral entry used by the measured single-edit lanes.
  void replaceOne(
    int start,
    int end,
    String replacement, {
    required int transactionKey,
  }) {
    _validateGateOperation(root, start, end, replacement);
    final inverseSlot = _inverse?.begin(transactionKey, 1);
    _roots?.begin(transactionKey, root);
    if (inverseSlot != null) {
      _inverse!.append(
        slot: inverseSlot,
        start: start,
        insertedLength: replacement.length,
        deleted: _captureDeleted(start, end),
      );
    }
    final base = revision;
    _applyOne(start, end, replacement);
    revision += 1;
    _publishForward(base, 1, wasUndo: false);
  }

  void replaceBatch(
    Uint32List starts,
    Uint32List ends,
    List<String> replacements,
    int operationCount, {
    required int transactionKey,
  }) {
    _prepareOperations(
      starts,
      ends,
      replacements,
      operationCount,
      transactionKey,
    );
  }

  void _prepareOperations(
    Uint32List starts,
    Uint32List ends,
    List<String> replacements,
    int operationCount,
    int transactionKey,
  ) {
    if (operationCount < 1 ||
        starts.length < operationCount ||
        ends.length < operationCount ||
        replacements.length < operationCount) {
      throw RangeError('invalid batch shape');
    }
    var priorStart = length + 1;
    for (var operation = 0; operation < operationCount; operation += 1) {
      final start = starts[operation];
      final end = ends[operation];
      if (end > priorStart) {
        throw StateError('batch operations must be nonoverlapping descending');
      }
      _validateGateOperation(root, start, end, replacements[operation]);
      priorStart = start;
    }
    final inverseSlot = _inverse?.begin(transactionKey, operationCount);
    _roots?.begin(transactionKey, root);
    final base = revision;
    for (var operation = 0; operation < operationCount; operation += 1) {
      final start = starts[operation];
      final end = ends[operation];
      final replacement = replacements[operation];
      if (inverseSlot != null) {
        _inverse!.append(
          slot: inverseSlot,
          start: start,
          insertedLength: replacement.length,
          deleted: _captureDeleted(start, end),
        );
      }
      _applyOne(start, end, replacement);
    }
    revision += 1;
    _publishForward(base, operationCount, wasUndo: false);
  }

  void undo() {
    final inverse = _inverse;
    if (inverse == null) {
      throw StateError('root-history control does not exercise forward undo');
    }
    final slot = inverse.newestSlot;
    final operations = inverse.operationCount(slot);
    final base = revision;
    for (var operation = operations - 1; operation >= 0; operation -= 1) {
      final start = inverse.startAt(slot, operation);
      final insertedLength = inverse.insertedLengthAt(slot, operation);
      final deleted = inverse.deletedAt(slot, operation);
      _applyOne(start, start + insertedLength, deleted);
    }
    inverse.removeNewest();
    revision += 1;
    _publishForward(base, operations, wasUndo: true);
  }

  void _applyOne(int start, int end, String replacement) {
    root = _applyGateTreeEdit(
      root: root,
      start: start,
      end: end,
      replacement: replacement,
      backingId: _nextBackingId++,
      pieceId: _nextPieceId++,
      work: _work,
    );
    final allocated = _work.branchesAllocated + _work.leavesAllocated;
    totalNodeAllocations += allocated;
    totalBranchesAllocated += _work.branchesAllocated;
    totalLeavesAllocated += _work.leavesAllocated;
    maxNodesVisited = math.max(maxNodesVisited, _work.nodesVisited);
    maxNodesAllocated = math.max(maxNodesAllocated, allocated);
    activeAnchor = _transformGateAnchor(
      activeAnchor,
      activeAffinity,
      start,
      end,
      replacement.length,
    );
    selectionAnchor = _transformGateAnchor(
      selectionAnchor,
      selectionAffinity,
      start,
      end,
      replacement.length,
    );
  }

  Object? _captureDeleted(int start, int end) {
    final deletedLength = end - start;
    if (deletedLength == 0) return null;
    if (deletedLength == 1) return _codeUnitAt(root!, start);
    return _readRange(root, start, end);
  }

  void _publishForward(int base, int operations, {required bool wasUndo}) {
    lastForwardBaseRevision = base;
    lastForwardRevision = revision;
    lastForwardOperations = operations;
    lastForwardWasUndo = wasUndo;
  }

  int utf16ToUtf8(int utf16Offset) {
    if (utf16Offset < 0 || utf16Offset > length) {
      throw RangeError.range(utf16Offset, 0, length);
    }
    _checkBoundary(root, utf16Offset);
    return _gateUtf16ToUtf8(root, utf16Offset);
  }

  int utf8ToUtf16(int utf8Offset) {
    if (utf8Offset < 0 || utf8Offset > summary.utf8Length) {
      throw RangeError.range(utf8Offset, 0, summary.utf8Length);
    }
    return _gateUtf8ToUtf16(root, utf8Offset);
  }

  _GateOwnershipDiagnostics diagnostics() =>
      _gateOwnershipDiagnostics(root, _roots);

  void close() {
    root = null;
    _inverse?.clear();
    _roots?.clear();
  }
}

void _validateGateOperation(
  _Node? root,
  int start,
  int end,
  String replacement,
) {
  final length = root?.utf16Length ?? 0;
  if (start < 0 || end < start || end > length) {
    throw RangeError.range(end, start, length);
  }
  _checkBoundary(root, start);
  _checkBoundary(root, end);
  if (replacement.isNotEmpty) {
    _gateReplacementIndexes.forValue(replacement);
  }
}

int _transformGateAnchor(
  int offset,
  _Affinity affinity,
  int start,
  int end,
  int replacementLength,
) {
  final delta = replacementLength - (end - start);
  if (offset < start) return offset;
  if (offset > end) return offset + delta;
  return affinity == _Affinity.upstream ? start : start + replacementLength;
}

int _gateUtf16ToUtf8(_Node? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf16Length) return node.summary!.utf8Length;
  if (node case final _Leaf leaf) {
    final piece = leaf.piece;
    final index = piece.index!;
    return index.utf8Before(piece.backingStart + offset) -
        index.utf8Before(piece.backingStart);
  }
  final branch = node as _Branch;
  if (offset <= branch.left.utf16Length) {
    return _gateUtf16ToUtf8(branch.left, offset);
  }
  return branch.left.summary!.utf8Length +
      _gateUtf16ToUtf8(branch.right, offset - branch.left.utf16Length);
}

int _gateUtf8ToUtf16(_Node? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.summary!.utf8Length) return node.utf16Length;
  if (node case final _Leaf leaf) {
    final piece = leaf.piece;
    final index = piece.index!;
    final base = index.utf8Before(piece.backingStart);
    final absolute = index.utf16AtUtf8Prefix(base + offset);
    if (absolute < piece.backingStart ||
        absolute > piece.backingStart + piece.length) {
      throw FormatException('UTF-8 offset escapes piece');
    }
    return absolute - piece.backingStart;
  }
  final branch = node as _Branch;
  final leftUtf8 = branch.left.summary!.utf8Length;
  if (offset <= leftUtf8) return _gateUtf8ToUtf16(branch.left, offset);
  return branch.left.utf16Length +
      _gateUtf8ToUtf16(branch.right, offset - leftUtf8);
}

Future<void> _runCurrentRootGate(_Options options) async {
  final phase = options.string('phase', 'all');
  if (phase != 'all' && phase != 'verify' && phase != 'benchmark') {
    throw FormatException('phase must be all, verify, or benchmark');
  }
  final model = switch (options.string('model', 'current')) {
    'current' => _GateHistoryModel.inverse,
    'roots' => _GateHistoryModel.roots,
    final value => throw FormatException('unknown source model $value'),
  };
  final sizeMiB = options.integer('size-mib', 10);
  final activeEdits = options.integer('active-edits', 1000);
  final coldEdits = options.integer('cold-edits', 1000);
  final batchRounds = options.integer('batch-rounds', 256);
  final batchSize = options.integer('batch-size', 16);
  final churnEdits = options.integer('churn-edits', 10000);
  final historyEntries = options.integer('history-entries', 2048);
  final historyBytes = options.integer('history-bytes', 8 * 1024 * 1024);
  final historyOperations = math.max(
    options.integer('history-operations', 64),
    math.max(batchSize, 8),
  );

  _emit('current_root_environment', {
    'dart': Platform.version.split('\n').first,
    'model': model.name,
    'phase': phase,
    'size_mib': sizeMiB,
    'history_entries_cap': historyEntries,
    'history_bytes_cap': historyBytes,
    'history_operations_cap': historyOperations,
    'rss_mib': _rssMiB(),
  });

  if (phase == 'all' || phase == 'verify') {
    _verifyCurrentRootExactness();
    _emit('current_root_exact_gate', {
      'string_differential_edits': 1024,
      'utf16_utf8_roundtrips': true,
      'crlf_lone_cr': true,
      'scalar_boundaries': true,
      'anchors': true,
      'grouped_undo': true,
      'randomized_grouped_undo_transactions': 128,
      'undo_is_new_forward_revision': true,
      'byte_and_entry_bounded_history': true,
      'old_roots_in_inverse_history': 0,
    });
  }
  if (phase == 'verify') {
    _emit('current_root_probe_complete', {
      'phase': phase,
      'black_hole': _blackHole,
      'rss_mib': _rssMiB(),
    });
    return;
  }

  final sourceWatch = Stopwatch()..start();
  final source = _asciiMarkdownOfLength(sizeMiB * 1024 * 1024);
  sourceWatch.stop();
  final indexWatch = Stopwatch()..start();
  final sourceIndex = _RangeIndex.buildSync(source, 0, source.length);
  indexWatch.stop();
  _gateReplacementIndexes.warm(const <String>['x', 'y', 'q', 'r', 's', 't']);

  await _runCurrentRootLargeLanes(
    source: source,
    sourceIndex: sourceIndex,
    model: model,
    activeEdits: activeEdits,
    coldEdits: coldEdits,
    batchRounds: batchRounds,
    batchSize: batchSize,
    historyEntries: historyEntries,
    historyBytes: historyBytes,
    historyOperations: historyOperations,
    sourceBuildMilliseconds: sourceWatch.elapsedMicroseconds / 1000,
    indexBuildMilliseconds: indexWatch.elapsedMicroseconds / 1000,
  );
  await _runCurrentRootChurnLane(
    source: source,
    sourceIndex: sourceIndex,
    model: model,
    edits: churnEdits,
    historyEntries: historyEntries,
    historyBytes: historyBytes,
    historyOperations: historyOperations,
  );
  _emit('current_root_probe_complete', {
    'phase': phase,
    'model': model.name,
    'black_hole': _blackHole,
    'rss_mib': _rssMiB(),
    'host_only': true,
    'physical_device_gate_open': true,
    'web_worker_gate_open': true,
  });
}

void _verifyCurrentRootExactness() {
  var oracle = 'α\r\nb\rc\n😀z';
  final index = _RangeIndex.buildSync(oracle, 0, oracle.length);
  final session = _GateSourceSession.fromIndexed(
    source: oracle,
    index: index,
    model: _GateHistoryModel.inverse,
    historyEntries: 128,
    historyBytes: 64 * 1024,
    historyOperations: 64,
  );
  session.activeAnchor = oracle.length;
  session.selectionAnchor = 0;
  var seed = 0x517CC1B7;
  const replacements = <String>['x', 'é', '😀', '\r\n', '\r', '\n', ''];
  for (var edit = 0; edit < 1024; edit += 1) {
    final boundaries = _gateScalarBoundaries(oracle);
    seed = _next(seed);
    final startIndex = seed % boundaries.length;
    seed = _next(seed);
    final deleteScalars = seed % 3;
    final endIndex = math.min(
      boundaries.length - 1,
      startIndex + deleteScalars,
    );
    seed = _next(seed);
    final replacement = replacements[seed % replacements.length];
    final start = boundaries[startIndex];
    final end = boundaries[endIndex];
    session.replaceOne(start, end, replacement, transactionKey: edit + 1);
    oracle = oracle.replaceRange(start, end, replacement);
    _expect(
      session.readRange(0, session.length) == oracle,
      'current-root String differential edit $edit',
    );
    if ((edit & 31) == 31) {
      _expectGateSummary(session, oracle);
      _checkGateTree(session.root);
    }
  }
  _expectGateSummary(session, oracle);
  _checkGateTree(session.root);
  for (final boundary in _gateScalarBoundaries(oracle)) {
    final bytes = utf8.encode(oracle.substring(0, boundary)).length;
    _expect(
      session.utf16ToUtf8(boundary) == bytes,
      'UTF-16 to UTF-8 differential at $boundary',
    );
    _expect(
      session.utf8ToUtf16(bytes) == boundary,
      'UTF-8 to UTF-16 differential at $bytes',
    );
  }
  final emoji = oracle.indexOf('😀');
  if (emoji >= 0) {
    _expectGateThrows(
      () => session.replaceOne(
        emoji + 1,
        emoji + 1,
        'x',
        transactionKey: 0x7FFFFFF0,
      ),
      'edit cannot split a Unicode scalar',
    );
  }
  session.close();
  _expect(
    session.diagnostics().nodes == 0,
    'closing current root releases every owned node reference',
  );

  const groupedInitial = 'ab😀\r\ncd';
  final grouped = _GateSourceSession.fromIndexed(
    source: groupedInitial,
    index: _RangeIndex.buildSync(groupedInitial, 0, groupedInitial.length),
    model: _GateHistoryModel.inverse,
    historyEntries: 8,
    historyBytes: 4096,
    historyOperations: 8,
  );
  grouped.activeAnchor = grouped.length;
  grouped.selectionAnchor = grouped.length;
  grouped.replaceOne(1, 1, 'X', transactionKey: 42);
  grouped.replaceOne(2, 2, 'Y', transactionKey: 42);
  grouped.replaceOne(0, 1, 'A', transactionKey: 42);
  _expect(grouped.historyEntries == 1, 'typing edits form one transaction');
  _expect(
    grouped.readRange(0, grouped.length) == 'AXYb😀\r\ncd',
    'grouped forward edits exact',
  );
  final revisionBeforeUndo = grouped.revision;
  grouped.undo();
  _expect(
    grouped.readRange(0, grouped.length) == groupedInitial,
    'grouped inverse restores exact source',
  );
  _expect(
    grouped.revision == revisionBeforeUndo + 1 &&
        grouped.lastForwardBaseRevision == revisionBeforeUndo &&
        grouped.lastForwardRevision == grouped.revision &&
        grouped.lastForwardOperations == 3 &&
        grouped.lastForwardWasUndo,
    'undo publishes one new forward revision',
  );
  _expect(
    grouped.activeAnchor == grouped.length &&
        grouped.selectionAnchor == grouped.length,
    'anchors transform through grouped edit and undo',
  );
  grouped.close();

  const undoInitial = 'α\r\nb😀c\rq\n';
  var undoOracle = undoInitial;
  final randomizedUndo = _GateSourceSession.fromIndexed(
    source: undoOracle,
    index: _RangeIndex.buildSync(undoOracle, 0, undoOracle.length),
    model: _GateHistoryModel.inverse,
    historyEntries: 256,
    historyBytes: 1024 * 1024,
    historyOperations: 8,
  );
  final undoStack = <String>[];
  var undoSeed = 0x6E766572;
  for (var transaction = 0; transaction < 128; transaction += 1) {
    final before = undoOracle;
    undoSeed = _next(undoSeed);
    final operations = 1 + undoSeed % 3;
    for (var operation = 0; operation < operations; operation += 1) {
      final boundaries = _gateScalarBoundaries(undoOracle);
      undoSeed = _next(undoSeed);
      final startIndex = undoSeed % boundaries.length;
      undoSeed = _next(undoSeed);
      final endIndex = math.min(
        boundaries.length - 1,
        startIndex + undoSeed % 3,
      );
      undoSeed = _next(undoSeed);
      final replacement = replacements[undoSeed % replacements.length];
      final start = boundaries[startIndex];
      final end = boundaries[endIndex];
      randomizedUndo.replaceOne(
        start,
        end,
        replacement,
        transactionKey: transaction + 1,
      );
      undoOracle = undoOracle.replaceRange(start, end, replacement);
      _expect(
        randomizedUndo.readRange(0, randomizedUndo.length) == undoOracle,
        'random grouped forward transaction $transaction operation $operation',
      );
    }
    undoStack.add(before);
    _expectGateSummary(randomizedUndo, undoOracle);
    _checkGateTree(randomizedUndo.root);
    if ((transaction & 3) == 3) {
      final expected = undoStack.removeLast();
      randomizedUndo.undo();
      undoOracle = expected;
      _expect(
        randomizedUndo.readRange(0, randomizedUndo.length) == undoOracle,
        'random grouped immediate undo $transaction',
      );
    }
  }
  while (undoStack.isNotEmpty) {
    final expected = undoStack.removeLast();
    randomizedUndo.undo();
    undoOracle = expected;
    _expect(
      randomizedUndo.readRange(0, randomizedUndo.length) == undoOracle,
      'random grouped history unwind',
    );
  }
  _expect(undoOracle == undoInitial, 'random grouped undo restores initial');
  _expectGateSummary(randomizedUndo, undoOracle);
  _checkGateTree(randomizedUndo.root);
  randomizedUndo.close();

  final oversizedSource = _repeatCodeUnit(0x61, 4096);
  final bounded = _GateSourceSession.fromIndexed(
    source: oversizedSource,
    index: _RangeIndex.buildSync(oversizedSource, 0, oversizedSource.length),
    model: _GateHistoryModel.inverse,
    historyEntries: 2,
    historyBytes: 64,
    historyOperations: 8,
  );
  bounded.replaceOne(0, bounded.length, '', transactionKey: 1);
  _expect(
    bounded.historyEntries == 1 && bounded.historyOverBudget,
    'newest oversized inverse remains immediately undoable',
  );
  _expect(
    bounded.historyPayloadUtf16 == oversizedSource.length,
    'oversized inverse is charged by retained payload',
  );
  bounded.replaceOne(0, 0, 'x', transactionKey: 2);
  _expect(
    bounded.historyEntries == 1 &&
        bounded.historyEvictions == 1 &&
        !bounded.historyOverBudget,
    'next transaction evicts an oversized older inverse',
  );
  bounded.close();

  const entrySource = 'abcd';
  final entryBounded = _GateSourceSession.fromIndexed(
    source: entrySource,
    index: _RangeIndex.buildSync(entrySource, 0, entrySource.length),
    model: _GateHistoryModel.inverse,
    historyEntries: 2,
    historyBytes: 64 * 1024,
    historyOperations: 8,
  );
  entryBounded.replaceOne(0, 1, 'A', transactionKey: 1);
  entryBounded.replaceOne(1, 2, 'B', transactionKey: 2);
  entryBounded.replaceOne(2, 3, 'C', transactionKey: 3);
  _expect(
    entryBounded.historyEntries == 2 && entryBounded.historyEvictions == 1,
    'inverse history entry capacity is hard-bounded',
  );
  entryBounded.close();
}

void _expectGateSummary(_GateSourceSession session, String oracle) {
  final expected = _RangeIndex.buildSync(
    oracle,
    0,
    oracle.length,
  ).summaryFor(0, oracle.length);
  final actual = session.summary;
  _expect(actual.utf16Length == oracle.length, 'exact UTF-16 extent');
  _expect(
    actual.utf8Length == utf8.encode(oracle).length,
    'exact UTF-8 extent',
  );
  _expect(
    actual.lineBreaks == _logicalLineBreaksOracle(oracle),
    'exact CRLF/lone-CR logical line count',
  );
  _expect(actual.hash32 == expected.hash32, 'exact content hash');
}

List<int> _gateScalarBoundaries(String source) {
  final output = <int>[0];
  var offset = 0;
  while (offset < source.length) {
    final unit = source.codeUnitAt(offset);
    if (_isHighSurrogate(unit)) {
      if (offset + 1 >= source.length ||
          !_isLowSurrogate(source.codeUnitAt(offset + 1))) {
        throw FormatException('unpaired high surrogate at $offset');
      }
      offset += 2;
    } else {
      if (_isLowSurrogate(unit)) {
        throw FormatException('unpaired low surrogate at $offset');
      }
      offset += 1;
    }
    output.add(offset);
  }
  return output;
}

_Summary _checkGateTree(_Node? node) {
  if (node == null) return _Summary.empty;
  if (node case final _Leaf leaf) {
    _expect(leaf.height == 1 && leaf.pieceCount == 1, 'leaf aggregates');
    _expect(leaf.summary != null, 'gate leaves are certified');
    return leaf.summary!;
  }
  final branch = node as _Branch;
  final left = _checkGateTree(branch.left);
  final right = _checkGateTree(branch.right);
  final expected = _Summary.append(left, right);
  _expect(
    (branch.left.height - branch.right.height).abs() <= 1,
    'object AVL remains balanced',
  );
  _expect(
    branch.height == math.max(branch.left.height, branch.right.height) + 1,
    'height aggregate exact',
  );
  _expect(
    branch.utf16Length == left.utf16Length + right.utf16Length,
    'UTF-16 sum exact',
  );
  _expect(
    branch.summary!.utf8Length == expected.utf8Length &&
        branch.summary!.lineBreaks == expected.lineBreaks &&
        branch.summary!.hash32 == expected.hash32,
    'branch source summary exact',
  );
  return expected;
}

void _expectGateThrows(void Function() body, String message) {
  try {
    body();
  } on Object {
    return;
  }
  throw StateError(message);
}

Future<void> _runCurrentRootLargeLanes({
  required String source,
  required _RangeIndex sourceIndex,
  required _GateHistoryModel model,
  required int activeEdits,
  required int coldEdits,
  required int batchRounds,
  required int batchSize,
  required int historyEntries,
  required int historyBytes,
  required int historyOperations,
  required double sourceBuildMilliseconds,
  required double indexBuildMilliseconds,
}) async {
  final session = _GateSourceSession.fromIndexed(
    source: source,
    index: sourceIndex,
    model: model,
    historyEntries: historyEntries,
    historyBytes: historyBytes,
    historyOperations: historyOperations,
  );
  final activeOffset = source.length ~/ 2;
  session.activeAnchor = activeOffset;
  session.selectionAnchor = activeOffset;
  final starts = Uint32List(batchSize);
  final ends = Uint32List(batchSize);
  final replacements = List<String>.generate(
    batchSize,
    (index) => index.isEven ? 's' : 't',
    growable: false,
  );
  final samplesClock = Stopwatch()..start();
  final heartbeat = _GateHeartbeat()..start();
  final activeSamples = Uint64List(activeEdits);
  final coldSamples = Uint64List(coldEdits);
  final batchSamples = Uint64List(batchRounds);
  await Future<void>.delayed(const Duration(milliseconds: 3));

  for (var edit = 0; edit < activeEdits; edit += 1) {
    final before = samplesClock.elapsedTicks;
    session.replaceOne(
      activeOffset,
      activeOffset + 1,
      edit.isEven ? 'x' : 'y',
      transactionKey: 0x100000 + (edit ~/ 8),
    );
    activeSamples[edit] = _gateTicksToNanoseconds(
      samplesClock.elapsedTicks - before,
    );
    _blackHole ^= session.summary.hash32;
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }

  var seed = 0x13579BDF;
  for (var edit = 0; edit < coldEdits; edit += 1) {
    seed = _next(seed);
    final offset = seed % (session.length - 1);
    final before = samplesClock.elapsedTicks;
    session.replaceOne(
      offset,
      offset + 1,
      edit.isEven ? 'q' : 'r',
      transactionKey: 0x200000 + (edit ~/ 8),
    );
    coldSamples[edit] = _gateTicksToNanoseconds(
      samplesClock.elapsedTicks - before,
    );
    _blackHole ^= session.summary.hash32;
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }

  for (var round = 0; round < batchRounds; round += 1) {
    final stride = session.length ~/ (batchSize + 1);
    final jitter = round & 7;
    for (var operation = 0; operation < batchSize; operation += 1) {
      final ascending = batchSize - operation;
      final offset = math.min(session.length - 1, ascending * stride + jitter);
      starts[operation] = offset;
      ends[operation] = offset + 1;
    }
    final before = samplesClock.elapsedTicks;
    session.replaceBatch(
      starts,
      ends,
      replacements,
      batchSize,
      transactionKey: 0x300000 + round,
    );
    batchSamples[round] = _gateTicksToNanoseconds(
      samplesClock.elapsedTicks - before,
    );
    _blackHole ^= session.summary.hash32;
    if ((round & 7) == 7) await Future<void>.delayed(Duration.zero);
  }
  await Future<void>.delayed(const Duration(milliseconds: 3));
  heartbeat.stop();
  _checkGateTree(session.root);
  final diagnostics = session.diagnostics();
  final active = _Samples(activeSamples);
  final cold = _Samples(coldSamples);
  final batches = _Samples(batchSamples);
  _emit('current_root_large_source', {
    'model': model.name,
    'source_utf16': source.length,
    'source_build_ms': sourceBuildMilliseconds,
    'index_build_ms': indexBuildMilliseconds,
    'index_bytes': sourceIndex.checkpointCount * 16,
    'active_edits': activeEdits,
    ..._prefixGateSamples(active, 'active'),
    'cold_edits': coldEdits,
    ..._prefixGateSamples(cold, 'cold'),
    'batch_rounds': batchRounds,
    'batch_size': batchSize,
    ..._prefixGateSamples(batches, 'batch'),
    'batch_p99_per_operation_us':
        (batches._values[((batches._values.length - 1) * 99) ~/ 100] / 1000) /
        batchSize,
    'heartbeat_max_gap_us': heartbeat.maxGapMicroseconds,
    'revision': session.revision,
    'tree_height': session.root?.height ?? 0,
    'piece_count': session.root?.pieceCount ?? 0,
    'history_entries': session.historyEntries,
    'history_evictions': session.historyEvictions,
    'history_charged_bytes': session.historyChargedBytes,
    'history_payload_utf16': session.historyPayloadUtf16,
    'total_node_versions_allocated': session.totalNodeAllocations,
    'total_branches_allocated': session.totalBranchesAllocated,
    'total_leaves_allocated': session.totalLeavesAllocated,
    'retained_nodes': diagnostics.nodes,
    'retained_branches': diagnostics.branches,
    'retained_leaves': diagnostics.leaves,
    'node_versions_unreachable_now':
        session.totalNodeAllocations - diagnostics.nodes,
    'retained_backing_objects': diagnostics.backingObjects,
    'retained_unique_source_strings': diagnostics.uniqueSourceStrings,
    'retained_unique_source_utf16': diagnostics.uniqueSourceUtf16,
    'max_nodes_visited_per_operation': session.maxNodesVisited,
    'max_nodes_allocated_per_operation': session.maxNodesAllocated,
    'rss_mib': _rssMiB(),
    'timing_samples_preallocated': true,
    'sample_stopwatch_long_lived': true,
  });
  session.close();
  final closed = session.diagnostics();
  _emit('current_root_large_source_release', {
    'model': model.name,
    'owned_nodes_after_close': closed.nodes,
    'owned_backings_after_close': closed.backingObjects,
    'history_entries_after_close': session.historyEntries,
    'logical_release_only': true,
    'dart_gc_completion_observable': false,
  });
}

Future<void> _runCurrentRootChurnLane({
  required String source,
  required _RangeIndex sourceIndex,
  required _GateHistoryModel model,
  required int edits,
  required int historyEntries,
  required int historyBytes,
  required int historyOperations,
}) async {
  final session = _GateSourceSession.fromIndexed(
    source: source,
    index: sourceIndex,
    model: model,
    historyEntries: historyEntries,
    historyBytes: historyBytes,
    historyOperations: historyOperations,
  );
  var caret = session.length ~/ 2;
  session.activeAnchor = caret;
  session.selectionAnchor = caret;
  final samples = Uint64List(edits);
  final clock = Stopwatch()..start();
  final heartbeat = _GateHeartbeat()..start();
  await Future<void>.delayed(const Duration(milliseconds: 3));
  for (var edit = 0; edit < edits; edit += 1) {
    final before = clock.elapsedTicks;
    session.replaceOne(
      caret,
      caret,
      edit.isEven ? 'x' : 'y',
      transactionKey: 0x400000 + (edit ~/ 8),
    );
    samples[edit] = _gateTicksToNanoseconds(clock.elapsedTicks - before);
    caret += 1;
    _blackHole ^= session.summary.hash32;
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }
  await Future<void>.delayed(const Duration(milliseconds: 3));
  heartbeat.stop();
  _checkGateTree(session.root);
  final diagnostics = session.diagnostics();
  _emit('current_root_churn', {
    'model': model.name,
    'source_utf16_before': source.length,
    'edits': edits,
    ..._Samples(samples).json,
    'heartbeat_max_gap_us': heartbeat.maxGapMicroseconds,
    'tree_height': session.root?.height ?? 0,
    'piece_count': session.root?.pieceCount ?? 0,
    'history_entries': session.historyEntries,
    'history_evictions': session.historyEvictions,
    'history_charged_bytes': session.historyChargedBytes,
    'history_payload_utf16': session.historyPayloadUtf16,
    'total_node_versions_allocated': session.totalNodeAllocations,
    'retained_nodes': diagnostics.nodes,
    'node_versions_unreachable_now':
        session.totalNodeAllocations - diagnostics.nodes,
    'retained_backing_objects': diagnostics.backingObjects,
    'retained_unique_source_strings': diagnostics.uniqueSourceStrings,
    'retained_unique_source_utf16': diagnostics.uniqueSourceUtf16,
    'max_nodes_visited_per_edit': session.maxNodesVisited,
    'max_nodes_allocated_per_edit': session.maxNodesAllocated,
    'rss_mib': _rssMiB(),
  });
  session.close();
  final closed = session.diagnostics();
  _emit('current_root_churn_release', {
    'model': model.name,
    'owned_nodes_after_close': closed.nodes,
    'owned_backings_after_close': closed.backingObjects,
    'history_entries_after_close': session.historyEntries,
    'logical_release_only': true,
  });
}

Map<String, Object> _prefixGateSamples(_Samples samples, String prefix) => {
  '${prefix}_p50_us': samples._values[(samples._values.length - 1) ~/ 2] / 1000,
  '${prefix}_p99_us':
      samples._values[((samples._values.length - 1) * 99) ~/ 100] / 1000,
  '${prefix}_p999_us':
      samples._values[((samples._values.length - 1) * 999) ~/ 1000] / 1000,
  '${prefix}_max_us': samples._values.last / 1000,
};

final class _GateHeartbeat {
  final Stopwatch _watch = Stopwatch();
  Timer? _timer;
  int _last = 0;
  int maxGapMicroseconds = 0;

  void start() {
    _watch.start();
    _timer = Timer.periodic(const Duration(milliseconds: 1), (_) {
      final now = _watch.elapsedMicroseconds;
      maxGapMicroseconds = math.max(maxGapMicroseconds, now - _last);
      _last = now;
    });
  }

  void stop() {
    final now = _watch.elapsedMicroseconds;
    maxGapMicroseconds = math.max(maxGapMicroseconds, now - _last);
    _timer?.cancel();
    _timer = null;
    _watch.stop();
  }
}

int _gateTicksToNanoseconds(int ticks) =>
    (ticks * 1000000000) ~/ _stopwatchFrequency;

final class _Options {
  _Options(Iterable<String> arguments) {
    for (final argument in arguments) {
      if (!argument.startsWith('--') || !argument.contains('=')) {
        throw FormatException('invalid option $argument');
      }
      final separator = argument.indexOf('=');
      _values[argument.substring(2, separator)] = argument.substring(
        separator + 1,
      );
    }
  }

  final Map<String, String> _values = {};

  int integer(String name, int fallback) =>
      _values[name] == null ? fallback : int.parse(_values[name]!);

  String string(String name, String fallback) => _values[name] ?? fallback;
}

String _asciiMarkdownOfLength(int length) {
  const line =
      'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  final count = length ~/ line.length;
  final remainder = length % line.length;
  return '${List<String>.filled(count, line).join()}'
      '${line.substring(0, remainder)}';
}

String _repeatCodeUnit(int codeUnit, int length) => String.fromCharCodes(
  Uint8List.fromList(List<int>.filled(length, codeUnit)),
);

String _repeatString(String pattern, int targetLength) {
  final count = targetLength ~/ pattern.length;
  final remainder = targetLength % pattern.length;
  return '${List<String>.filled(count, pattern).join()}'
      '${pattern.substring(0, remainder)}';
}

int _logicalLineBreaksOracle(String source) {
  var breaks = 0;
  var offset = 0;
  while (offset < source.length) {
    final unit = source.codeUnitAt(offset);
    if (unit == 0x0D) {
      breaks += 1;
      if (offset + 1 < source.length && source.codeUnitAt(offset + 1) == 0x0A) {
        offset += 2;
        continue;
      }
    } else if (unit == 0x0A) {
      breaks += 1;
    }
    offset += 1;
  }
  return breaks;
}

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

var _blackHole = 0;
final int _stopwatchFrequency = Stopwatch().frequency;

int _nanoseconds(Stopwatch stopwatch) =>
    (stopwatch.elapsedTicks * 1000000000) ~/ _stopwatchFrequency;

int _microseconds(Stopwatch stopwatch) =>
    (stopwatch.elapsedTicks * 1000000) ~/ _stopwatchFrequency;

int _rssMiB() => (ProcessInfo.currentRss / (1024 * 1024)).round();

void _emit(String receipt, Map<String, Object?> values) {
  stdout.writeln(jsonEncode({'receipt': receipt, ...values}));
}

void _expect(bool condition, String message) {
  if (!condition) throw StateError(message);
}
