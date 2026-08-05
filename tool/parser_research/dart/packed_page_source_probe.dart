import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

/// Disposable gate for a page-backed, persistent UTF-16 source on the main
/// Dart isolate.
///
/// The point of this spike is deliberately narrow: determine whether removing
/// per-node Dart objects can bound edit and retirement work without moving the
/// canonical interactive source into the parser worker. It is not a complete
/// editor source implementation.
Future<void> main(List<String> arguments) async {
  final options = _Options(arguments);
  final sizeMiB = options.integer('size-mib', 10);
  final activeEdits = options.integer('active-edits', 1000);
  final coldEdits = options.integer('cold-edits', 1000);
  final churnEdits = options.integer('churn-edits', 10000);
  final reserveNodes = options.integer('reserve-nodes', 131072);

  _emit('environment', {
    'dart': Platform.version.split('\n').first,
    'size_mib': sizeMiB,
    'rss_mib': _rssMiB(),
  });

  _verifyExactSemantics();
  _verifyPendingLargeBacking();
  _verifyBoundedReclamation();

  await _runLargeSourceLane(
    sizeMiB: sizeMiB,
    activeEdits: activeEdits,
    coldEdits: coldEdits,
    reserveNodes: reserveNodes,
  );
  await _runHistoryChurnLane(edits: churnEdits, reserveNodes: reserveNodes);

  _emit('probe_complete', {'black_hole': _blackHole, 'rss_mib': _rssMiB()});
}

const int _checkpointUtf16 = 4096;
const int _smallExactUtf16 = 8192;
const int _mask32 = 0xFFFFFFFF;
const int _hashBase = 0x01000193;

final class _RangeIndex {
  _RangeIndex._(this.source, this.records, this.recordCount);

  factory _RangeIndex.build(String source) {
    final capacity =
        (source.length + _checkpointUtf16 - 1) ~/ _checkpointUtf16 + 2;
    final records = Uint32List(capacity * 4);
    var recordCount = 1;
    var cursor = 0;
    var utf8Length = 0;
    var lineBreaks = 0;
    var hash = 0;
    var previousWasCr = false;
    var nextCheckpoint = math.min(_checkpointUtf16, source.length);

    while (cursor < source.length) {
      var target = nextCheckpoint;
      if (target < source.length &&
          target > cursor &&
          _isHighSurrogate(source.codeUnitAt(target - 1)) &&
          _isLowSurrogate(source.codeUnitAt(target))) {
        target += 1;
      }
      while (cursor < target) {
        final unit = source.codeUnitAt(cursor);
        if (_isHighSurrogate(unit)) {
          if (cursor + 1 >= source.length ||
              !_isLowSurrogate(source.codeUnitAt(cursor + 1))) {
            throw FormatException('unpaired high surrogate at $cursor');
          }
          final low = source.codeUnitAt(cursor + 1);
          final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
          utf8Length += 4;
          hash = _appendScalarHash(hash, scalar);
          previousWasCr = false;
          cursor += 2;
          continue;
        }
        if (_isLowSurrogate(unit)) {
          throw FormatException('unpaired low surrogate at $cursor');
        }
        utf8Length += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
        hash = _appendScalarHash(hash, unit);
        if (unit == 0x0D) {
          lineBreaks += 1;
          previousWasCr = true;
        } else if (unit == 0x0A) {
          if (!previousWasCr) lineBreaks += 1;
          previousWasCr = false;
        } else {
          previousWasCr = false;
        }
        cursor += 1;
      }
      final base = recordCount * 4;
      records[base] = cursor;
      records[base + 1] = utf8Length;
      records[base + 2] = lineBreaks;
      records[base + 3] = hash;
      recordCount += 1;
      nextCheckpoint = math.min(source.length, cursor + _checkpointUtf16);
    }
    return _RangeIndex._(source, records, recordCount);
  }

  final String source;
  final Uint32List records;
  final int recordCount;

  int get byteLength => recordCount * 16;

  void prefixInto(
    int target,
    Uint32List output,
    int outputOffset,
    _NodeArena arena,
  ) {
    if (target < 0 || target > source.length) {
      throw RangeError.range(target, 0, source.length);
    }
    var low = 0;
    var high = recordCount;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (records[middle * 4] <= target) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    final record = math.max(0, low - 1) * 4;
    var cursor = records[record];
    var utf8Length = records[record + 1];
    var lineBreaks = records[record + 2];
    var hash = records[record + 3];
    var previousWasCr = cursor > 0 && source.codeUnitAt(cursor - 1) == 0x0D;
    final scanStart = cursor;
    while (cursor < target) {
      final unit = source.codeUnitAt(cursor);
      if (_isHighSurrogate(unit)) {
        if (cursor + 1 >= source.length ||
            !_isLowSurrogate(source.codeUnitAt(cursor + 1)) ||
            cursor + 2 > target) {
          throw FormatException('UTF-16 offset splits a scalar at $target');
        }
        final lowUnit = source.codeUnitAt(cursor + 1);
        final scalar = 0x10000 + ((unit - 0xD800) << 10) + (lowUnit - 0xDC00);
        utf8Length += 4;
        hash = _appendScalarHash(hash, scalar);
        previousWasCr = false;
        cursor += 2;
        continue;
      }
      if (_isLowSurrogate(unit)) {
        throw FormatException('unpaired low surrogate at $cursor');
      }
      utf8Length += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
      hash = _appendScalarHash(hash, unit);
      if (unit == 0x0D) {
        lineBreaks += 1;
        previousWasCr = true;
      } else if (unit == 0x0A) {
        if (!previousWasCr) lineBreaks += 1;
        previousWasCr = false;
      } else {
        previousWasCr = false;
      }
      cursor += 1;
    }
    final scanned = cursor - scanStart;
    arena.totalSummaryScanUtf16 += scanned;
    arena.maxSummaryScanChunkUtf16 = math.max(
      arena.maxSummaryScanChunkUtf16,
      scanned,
    );
    output[outputOffset] = cursor;
    output[outputOffset + 1] = utf8Length;
    output[outputOffset + 2] = lineBreaks;
    output[outputOffset + 3] = hash;
  }
}

final class _BackingArena {
  _BackingArena(int capacity)
    : _sources = List<String?>.filled(capacity + 1, null),
      _indexes = List<_RangeIndex?>.filled(capacity + 1, null),
      _references = Uint32List(capacity + 1),
      _generations = Uint32List(capacity + 1),
      _next = Uint32List(capacity + 1) {
    for (var id = capacity; id >= 1; id -= 1) {
      _next[id] = _freeHead;
      _freeHead = id;
    }
  }

  final List<String?> _sources;
  final List<_RangeIndex?> _indexes;
  final Uint32List _references;
  final Uint32List _generations;
  final Uint32List _next;
  int _freeHead = 0;
  int liveBackings = 0;
  int highWaterBackings = 0;
  int allocations = 0;
  int reuses = 0;

  int allocate(String source, {_RangeIndex? index}) {
    final id = _freeHead;
    if (id == 0) throw StateError('fixed backing arena exhausted');
    _freeHead = _next[id];
    if (_generations[id] != 0) reuses += 1;
    _generations[id] += 1;
    _sources[id] = source;
    _indexes[id] = index;
    _references[id] = 0;
    allocations += 1;
    liveBackings += 1;
    highWaterBackings = math.max(highWaterBackings, liveBackings);
    return id;
  }

  String source(int id) => _sources[id]!;

  _RangeIndex? index(int id) => _indexes[id];

  void retain(int id) {
    _references[id] += 1;
  }

  void release(int id) {
    final next = _references[id] - 1;
    _references[id] = next;
    if (next != 0) return;
    _sources[id] = null;
    _indexes[id] = null;
    _next[id] = _freeHead;
    _freeHead = id;
    liveBackings -= 1;
  }
}

const int _nodeLeaf = 1;
const int _nodeBranch = 2;

const int _fKind = 0;
const int _fReferences = 1;
const int _fA = 2;
const int _fB = 3;
const int _fUtf16 = 4;
const int _fHeight = 5;
const int _fPieces = 6;
const int _fUtf8 = 7;
const int _fLines = 8;
const int _fHash = 9;
const int _fPower = 10;
const int _fFirstPlusOne = 11;
const int _fLastPlusOne = 12;
const int _fSummaryReady = 13;
const int _fNext = 14;
const int _fGeneration = 15;
const int _nodeStride = 16;
const int _nodesPerPage = 2048;

final class _Work {
  int nodesVisited = 0;
  int nodesAllocated = 0;
  int retireNodes = 0;
  int retireMicroseconds = 0;

  void reset() {
    nodesVisited = 0;
    nodesAllocated = 0;
    retireNodes = 0;
    retireMicroseconds = 0;
  }
}

final class _NodeArena {
  _NodeArena(this.backings);

  final _BackingArena backings;
  final List<Uint32List> _pages = [];
  final Uint32List _summaryScratch = Uint32List(8);
  final Stopwatch _clock = Stopwatch()..start();
  int _freeHead = 0;
  int _retireHead = 0;
  int _retireTail = 0;
  int retirementQueueLength = 0;
  int maxRetirementQueueLength = 0;
  int liveNodes = 0;
  int highWaterLiveNodes = 0;
  int allocations = 0;
  int reusedNodes = 0;
  int retiredNodes = 0;
  int maxRetireSliceNodes = 0;
  int maxRetireSliceMicroseconds = 0;
  int totalSummaryScanUtf16 = 0;
  int maxSummaryScanChunkUtf16 = 0;

  int get pageCount => _pages.length;
  int get capacity => pageCount * _nodesPerPage;

  void resetHotDiagnostics() {
    highWaterLiveNodes = liveNodes;
    maxRetirementQueueLength = retirementQueueLength;
    maxRetireSliceNodes = 0;
    maxRetireSliceMicroseconds = 0;
    totalSummaryScanUtf16 = 0;
    maxSummaryScanChunkUtf16 = 0;
    backings.highWaterBackings = backings.liveBackings;
  }

  void reserveNodes(int count) {
    while (capacity < count) {
      _addPage();
    }
  }

  void _addPage() {
    final pageIndex = _pages.length;
    _pages.add(Uint32List(_nodesPerPage * _nodeStride));
    for (var slot = _nodesPerPage - 1; slot >= 0; slot -= 1) {
      final id = pageIndex * _nodesPerPage + slot + 1;
      _set(id, _fNext, _freeHead);
      _freeHead = id;
    }
  }

  int _get(int id, int field) {
    final zero = id - 1;
    return _pages[zero ~/ _nodesPerPage][(zero % _nodesPerPage) * _nodeStride +
        field];
  }

  void _set(int id, int field, int value) {
    final zero = id - 1;
    _pages[zero ~/ _nodesPerPage][(zero % _nodesPerPage) * _nodeStride +
            field] =
        value;
  }

  int _allocateSlot(_Work work) {
    if (_freeHead == 0) _addPage();
    final id = _freeHead;
    _freeHead = _get(id, _fNext);
    if (_get(id, _fGeneration) != 0) reusedNodes += 1;
    _set(id, _fGeneration, _get(id, _fGeneration) + 1);
    _set(id, _fReferences, 1);
    _set(id, _fNext, 0);
    allocations += 1;
    work.nodesAllocated += 1;
    liveNodes += 1;
    highWaterLiveNodes = math.max(highWaterLiveNodes, liveNodes);
    return id;
  }

  int allocateLeaf(int backing, int backingStart, int length, _Work work) {
    final id = _allocateSlot(work);
    _set(id, _fKind, _nodeLeaf);
    _set(id, _fA, backing);
    _set(id, _fB, backingStart);
    _set(id, _fUtf16, length);
    _set(id, _fHeight, 1);
    _set(id, _fPieces, 1);
    backings.retain(backing);
    _writeLeafSummary(id);
    return id;
  }

  int allocateBranchOwned(int left, int right, _Work work) {
    if (left == 0 || right == 0) {
      throw StateError('branch children must be non-empty');
    }
    final id = _allocateSlot(work);
    _set(id, _fKind, _nodeBranch);
    _set(id, _fA, left);
    _set(id, _fB, right);
    _set(id, _fUtf16, utf16Length(left) + utf16Length(right));
    _set(id, _fHeight, math.max(height(left), height(right)) + 1);
    _set(id, _fPieces, pieceCount(left) + pieceCount(right));
    _writeBranchSummary(id, left, right);
    return id;
  }

  void _writeLeafSummary(int id) {
    final backing = _get(id, _fA);
    final start = _get(id, _fB);
    final length = utf16Length(id);
    if (length == 0) throw StateError('empty leaves are not stored');
    final source = backings.source(backing);
    final index = backings.index(backing);
    if (index == null && source.length > _smallExactUtf16) {
      _set(id, _fSummaryReady, 0);
      _set(id, _fUtf8, 0);
      _set(id, _fLines, 0);
      _set(id, _fHash, 0);
      _set(id, _fPower, 0);
      _set(id, _fFirstPlusOne, source.codeUnitAt(start) + 1);
      _set(id, _fLastPlusOne, source.codeUnitAt(start + length - 1) + 1);
      return;
    }
    if (index != null) {
      index.prefixInto(start, _summaryScratch, 0, this);
      index.prefixInto(start + length, _summaryScratch, 4, this);
      final utf8Length = _summaryScratch[5] - _summaryScratch[1];
      var lines = _summaryScratch[6] - _summaryScratch[2];
      if (start > 0 &&
          source.codeUnitAt(start) == 0x0A &&
          source.codeUnitAt(start - 1) == 0x0D) {
        lines += 1;
      }
      final power = _pow32(_hashBase, utf8Length);
      final hash =
          (_summaryScratch[7] - _mul32(_summaryScratch[3], power)) & _mask32;
      _set(id, _fUtf8, utf8Length);
      _set(id, _fLines, lines);
      _set(id, _fHash, hash);
      _set(id, _fPower, power);
    } else {
      _writeDirectLeafSummary(id, source, start, length);
    }
    _set(id, _fSummaryReady, 1);
    _set(id, _fFirstPlusOne, source.codeUnitAt(start) + 1);
    _set(id, _fLastPlusOne, source.codeUnitAt(start + length - 1) + 1);
  }

  void _writeDirectLeafSummary(int id, String source, int start, int length) {
    var cursor = start;
    final end = start + length;
    var utf8Length = 0;
    var lines = 0;
    var hash = 0;
    var previousWasCr = false;
    while (cursor < end) {
      final unit = source.codeUnitAt(cursor);
      if (_isHighSurrogate(unit)) {
        if (cursor + 1 >= end ||
            !_isLowSurrogate(source.codeUnitAt(cursor + 1))) {
          throw FormatException('unpaired high surrogate at $cursor');
        }
        final low = source.codeUnitAt(cursor + 1);
        final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
        utf8Length += 4;
        hash = _appendScalarHash(hash, scalar);
        previousWasCr = false;
        cursor += 2;
        continue;
      }
      if (_isLowSurrogate(unit)) {
        throw FormatException('unpaired low surrogate at $cursor');
      }
      utf8Length += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
      hash = _appendScalarHash(hash, unit);
      if (unit == 0x0D) {
        lines += 1;
        previousWasCr = true;
      } else if (unit == 0x0A) {
        if (!previousWasCr) lines += 1;
        previousWasCr = false;
      } else {
        previousWasCr = false;
      }
      cursor += 1;
    }
    totalSummaryScanUtf16 += length;
    maxSummaryScanChunkUtf16 = math.max(maxSummaryScanChunkUtf16, length);
    _set(id, _fUtf8, utf8Length);
    _set(id, _fLines, lines);
    _set(id, _fHash, hash);
    _set(id, _fPower, _pow32(_hashBase, utf8Length));
  }

  void _writeBranchSummary(int id, int left, int right) {
    if (!summaryReady(left) || !summaryReady(right)) {
      _set(id, _fSummaryReady, 0);
      _set(id, _fUtf8, 0);
      _set(id, _fLines, 0);
      _set(id, _fHash, 0);
      _set(id, _fPower, 0);
      _set(id, _fFirstPlusOne, _get(left, _fFirstPlusOne));
      _set(id, _fLastPlusOne, _get(right, _fLastPlusOne));
      return;
    }
    final rightPower = _get(right, _fPower);
    final lines =
        _get(left, _fLines) +
        _get(right, _fLines) -
        (lastCodeUnit(left) == 0x0D && firstCodeUnit(right) == 0x0A ? 1 : 0);
    _set(id, _fSummaryReady, 1);
    _set(id, _fUtf8, _get(left, _fUtf8) + _get(right, _fUtf8));
    _set(id, _fLines, lines);
    _set(
      id,
      _fHash,
      (_mul32(_get(left, _fHash), rightPower) + _get(right, _fHash)) & _mask32,
    );
    _set(id, _fPower, _mul32(_get(left, _fPower), rightPower));
    _set(id, _fFirstPlusOne, _get(left, _fFirstPlusOne));
    _set(id, _fLastPlusOne, _get(right, _fLastPlusOne));
  }

  int retain(int id) {
    if (id != 0) _set(id, _fReferences, _get(id, _fReferences) + 1);
    return id;
  }

  void release(int id) {
    if (id == 0) return;
    final references = _get(id, _fReferences);
    if (references == 0) throw StateError('double release of node $id');
    _set(id, _fReferences, references - 1);
    if (references != 1) return;
    _set(id, _fNext, 0);
    if (_retireTail == 0) {
      _retireHead = id;
    } else {
      _set(_retireTail, _fNext, id);
    }
    _retireTail = id;
    retirementQueueLength += 1;
    maxRetirementQueueLength = math.max(
      maxRetirementQueueLength,
      retirementQueueLength,
    );
  }

  int retireSlice(int budget) {
    final startTicks = _clock.elapsedTicks;
    var processed = 0;
    while (processed < budget && _retireHead != 0) {
      final id = _retireHead;
      final next = _get(id, _fNext);
      _retireHead = next;
      if (next == 0) _retireTail = 0;
      retirementQueueLength -= 1;
      final kind = _get(id, _fKind);
      if (kind == _nodeBranch) {
        final left = _get(id, _fA);
        final right = _get(id, _fB);
        release(left);
        release(right);
      } else if (kind == _nodeLeaf) {
        backings.release(_get(id, _fA));
      } else {
        throw StateError('retiring invalid node $id');
      }
      final generation = _get(id, _fGeneration);
      final zero = id - 1;
      final page = _pages[zero ~/ _nodesPerPage];
      final base = (zero % _nodesPerPage) * _nodeStride;
      page.fillRange(base, base + _nodeStride, 0);
      page[base + _fGeneration] = generation;
      page[base + _fNext] = _freeHead;
      _freeHead = id;
      liveNodes -= 1;
      retiredNodes += 1;
      processed += 1;
    }
    final elapsedMicroseconds =
        (_clock.elapsedTicks - startTicks) * 1000000 ~/ _stopwatchFrequency;
    maxRetireSliceNodes = math.max(maxRetireSliceNodes, processed);
    maxRetireSliceMicroseconds = math.max(
      maxRetireSliceMicroseconds,
      elapsedMicroseconds,
    );
    return processed;
  }

  int utf16Length(int id) => id == 0 ? 0 : _get(id, _fUtf16);
  int height(int id) => id == 0 ? 0 : _get(id, _fHeight);
  int pieceCount(int id) => id == 0 ? 0 : _get(id, _fPieces);
  bool summaryReady(int id) => id == 0 || _get(id, _fSummaryReady) != 0;
  int firstCodeUnit(int id) => _get(id, _fFirstPlusOne) - 1;
  int lastCodeUnit(int id) => _get(id, _fLastPlusOne) - 1;
  int lineBreaks(int id) => id == 0 ? 0 : _get(id, _fLines);
  int utf8Length(int id) => id == 0 ? 0 : _get(id, _fUtf8);
  int hash32(int id) => id == 0 ? 0 : _get(id, _fHash);

  int codeUnitAt(int node, int offset) {
    if (offset < 0 || offset >= utf16Length(node)) {
      throw RangeError.range(offset, 0, utf16Length(node) - 1);
    }
    var current = node;
    var local = offset;
    while (_get(current, _fKind) == _nodeBranch) {
      final left = _get(current, _fA);
      final leftLength = utf16Length(left);
      if (local < leftLength) {
        current = left;
      } else {
        local -= leftLength;
        current = _get(current, _fB);
      }
    }
    final source = backings.source(_get(current, _fA));
    return source.codeUnitAt(_get(current, _fB) + local);
  }

  int splitBorrowed(int node, int offset, _Work work) {
    work.nodesVisited += 1;
    if (node == 0) return 0;
    if (offset == 0) return _packPair(0, retain(node));
    if (offset == utf16Length(node)) return _packPair(retain(node), 0);
    if (offset < 0 || offset > utf16Length(node)) {
      throw RangeError.range(offset, 0, utf16Length(node));
    }
    if (_get(node, _fKind) == _nodeLeaf) {
      final backing = _get(node, _fA);
      final start = _get(node, _fB);
      final source = backings.source(backing);
      final previous = source.codeUnitAt(start + offset - 1);
      final next = source.codeUnitAt(start + offset);
      if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
        throw FormatException('split divides a scalar');
      }
      return _packPair(
        allocateLeaf(backing, start, offset, work),
        allocateLeaf(backing, start + offset, utf16Length(node) - offset, work),
      );
    }
    final left = _get(node, _fA);
    final right = _get(node, _fB);
    final leftLength = utf16Length(left);
    if (offset < leftLength) {
      final split = splitBorrowed(left, offset, work);
      final joined = concatOwned(_pairRight(split), retain(right), work);
      return _packPair(_pairLeft(split), joined);
    }
    if (offset == leftLength) {
      return _packPair(retain(left), retain(right));
    }
    final split = splitBorrowed(right, offset - leftLength, work);
    final joined = concatOwned(retain(left), _pairLeft(split), work);
    return _packPair(joined, _pairRight(split));
  }

  int concatOwned(int left, int right, _Work work) {
    if (left == 0) return right;
    if (right == 0) return left;
    work.nodesVisited += 1;
    if (height(left) > height(right) + 1) {
      if (_get(left, _fKind) != _nodeBranch) {
        throw StateError('unbalanced leaf');
      }
      final leftLeft = retain(_get(left, _fA));
      final leftRight = retain(_get(left, _fB));
      release(left);
      final joined = concatOwned(leftRight, right, work);
      return _balanceOwnedChildren(leftLeft, joined, work);
    }
    if (height(right) > height(left) + 1) {
      if (_get(right, _fKind) != _nodeBranch) {
        throw StateError('unbalanced leaf');
      }
      final rightLeft = retain(_get(right, _fA));
      final rightRight = retain(_get(right, _fB));
      release(right);
      final joined = concatOwned(left, rightLeft, work);
      return _balanceOwnedChildren(joined, rightRight, work);
    }
    return allocateBranchOwned(left, right, work);
  }

  int _balanceOwnedChildren(int left, int right, _Work work) {
    final difference = height(left) - height(right);
    if (difference.abs() <= 1) {
      return allocateBranchOwned(left, right, work);
    }
    if (difference > 1) {
      if (_get(left, _fKind) != _nodeBranch || difference > 2) {
        throw StateError('unexpected left balance delta $difference');
      }
      final leftLeft = _get(left, _fA);
      final leftRight = _get(left, _fB);
      if (height(leftLeft) >= height(leftRight)) {
        final ownedLeftLeft = retain(leftLeft);
        final ownedLeftRight = retain(leftRight);
        release(left);
        final nextRight = allocateBranchOwned(ownedLeftRight, right, work);
        return allocateBranchOwned(ownedLeftLeft, nextRight, work);
      }
      final pivot = leftRight;
      final ownedLeftLeft = retain(leftLeft);
      final ownedPivotLeft = retain(_get(pivot, _fA));
      final ownedPivotRight = retain(_get(pivot, _fB));
      release(left);
      final nextLeft = allocateBranchOwned(ownedLeftLeft, ownedPivotLeft, work);
      final nextRight = allocateBranchOwned(ownedPivotRight, right, work);
      return allocateBranchOwned(nextLeft, nextRight, work);
    }

    if (_get(right, _fKind) != _nodeBranch || difference < -2) {
      throw StateError('unexpected right balance delta $difference');
    }
    final rightLeft = _get(right, _fA);
    final rightRight = _get(right, _fB);
    if (height(rightRight) >= height(rightLeft)) {
      final ownedRightLeft = retain(rightLeft);
      final ownedRightRight = retain(rightRight);
      release(right);
      final nextLeft = allocateBranchOwned(left, ownedRightLeft, work);
      return allocateBranchOwned(nextLeft, ownedRightRight, work);
    }
    final pivot = rightLeft;
    final ownedPivotLeft = retain(_get(pivot, _fA));
    final ownedPivotRight = retain(_get(pivot, _fB));
    final ownedRightRight = retain(rightRight);
    release(right);
    final nextLeft = allocateBranchOwned(left, ownedPivotLeft, work);
    final nextRight = allocateBranchOwned(
      ownedPivotRight,
      ownedRightRight,
      work,
    );
    return allocateBranchOwned(nextLeft, nextRight, work);
  }

  String readRange(int root, int start, int end) {
    final length = utf16Length(root);
    if (start < 0 || end < start || end > length) {
      throw RangeError.range(end, start, length);
    }
    final output = StringBuffer();
    final nodes = <int>[root];
    final starts = <int>[0];
    while (nodes.isNotEmpty) {
      final node = nodes.removeLast();
      final globalStart = starts.removeLast();
      final globalEnd = globalStart + utf16Length(node);
      if (globalEnd <= start || globalStart >= end) continue;
      if (_get(node, _fKind) == _nodeLeaf) {
        final localStart = math.max(start, globalStart) - globalStart;
        final localEnd = math.min(end, globalEnd) - globalStart;
        final source = backings.source(_get(node, _fA));
        final backingStart = _get(node, _fB);
        output.write(
          String.fromCharCodes(
            source.codeUnits.sublist(
              backingStart + localStart,
              backingStart + localEnd,
            ),
          ),
        );
        continue;
      }
      final left = _get(node, _fA);
      final right = _get(node, _fB);
      nodes.add(right);
      starts.add(globalStart + utf16Length(left));
      nodes.add(left);
      starts.add(globalStart);
    }
    return output.toString();
  }

  void checkBalanced(int root) {
    if (root == 0) return;
    final nodes = <int>[root];
    while (nodes.isNotEmpty) {
      final node = nodes.removeLast();
      if (_get(node, _fKind) != _nodeBranch) continue;
      final left = _get(node, _fA);
      final right = _get(node, _fB);
      if ((height(left) - height(right)).abs() > 1) {
        throw StateError('node $node is not AVL balanced');
      }
      if (utf16Length(node) != utf16Length(left) + utf16Length(right)) {
        throw StateError('invalid sum at node $node');
      }
      nodes.add(left);
      nodes.add(right);
    }
  }
}

int _packPair(int left, int right) => (left << 32) | right;
int _pairLeft(int pair) => pair >>> 32;
int _pairRight(int pair) => pair & _mask32;

final class _RootHistory {
  _RootHistory(this.capacity)
    : _roots = Uint32List(capacity),
      _anchors = Uint32List(capacity);

  final int capacity;
  final Uint32List _roots;
  final Uint32List _anchors;
  int _head = 0;
  int length = 0;
  int evictions = 0;

  void pushOwned(int root, int anchor, _NodeArena arena) {
    if (capacity == 0) {
      arena.release(root);
      return;
    }
    if (length == capacity) {
      arena.release(_roots[_head]);
      _roots[_head] = root;
      _anchors[_head] = anchor;
      _head = (_head + 1) % capacity;
      evictions += 1;
      return;
    }
    final tail = (_head + length) % capacity;
    _roots[tail] = root;
    _anchors[tail] = anchor;
    length += 1;
  }

  int popRoot() {
    if (length == 0) throw StateError('nothing to undo');
    final tail = (_head + length - 1) % capacity;
    final root = _roots[tail];
    _roots[tail] = 0;
    length -= 1;
    return root;
  }

  int popAnchor() {
    final tail = (_head + length) % capacity;
    final anchor = _anchors[tail];
    _anchors[tail] = 0;
    return anchor;
  }

  void releaseAll(_NodeArena arena) {
    while (length > 0) {
      arena.release(popRoot());
      popAnchor();
    }
  }
}

final class _PackedSourceSession {
  _PackedSourceSession._({
    required this.arena,
    required this.root,
    required int historyCapacity,
  }) : history = _RootHistory(historyCapacity),
       activeAnchor = arena.utf16Length(root);

  factory _PackedSourceSession.fromString({
    required _NodeArena arena,
    required String source,
    required bool certified,
    required int historyCapacity,
    required _Work work,
  }) {
    if (source.isEmpty) {
      return _PackedSourceSession._(
        arena: arena,
        root: 0,
        historyCapacity: historyCapacity,
      );
    }
    final index = certified ? _RangeIndex.build(source) : null;
    final backing = arena.backings.allocate(source, index: index);
    final root = arena.allocateLeaf(backing, 0, source.length, work);
    return _PackedSourceSession._(
      arena: arena,
      root: root,
      historyCapacity: historyCapacity,
    );
  }

  final _NodeArena arena;
  final _RootHistory history;
  int root;
  int activeAnchor;
  bool anchorDownstream = true;
  final _Work work = _Work();
  int revision = 0;
  int maxEditSummaryScanChunk = 0;
  int maxEditSummaryScanTotal = 0;

  int get length => arena.utf16Length(root);
  bool get summaryReady => arena.summaryReady(root);

  void replace(int start, int end, String replacement) {
    if (start < 0 || end < start || end > length) {
      throw RangeError.range(end, start, length);
    }
    if (start > 0 && start < length) {
      final previous = arena.codeUnitAt(root, start - 1);
      final next = arena.codeUnitAt(root, start);
      if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
        throw FormatException('start splits a scalar');
      }
    }
    if (end > 0 && end < length) {
      final previous = arena.codeUnitAt(root, end - 1);
      final next = arena.codeUnitAt(root, end);
      if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
        throw FormatException('end splits a scalar');
      }
    }

    work.reset();
    final scanBefore = arena.totalSummaryScanUtf16;
    final maxScanBefore = arena.maxSummaryScanChunkUtf16;
    final first = arena.splitBorrowed(root, start, work);
    final rest = _pairRight(first);
    final second = arena.splitBorrowed(rest, end - start, work);
    arena.release(rest);
    arena.release(_pairLeft(second));

    var replacementRoot = 0;
    if (replacement.isNotEmpty) {
      final backing = arena.backings.allocate(replacement);
      replacementRoot = arena.allocateLeaf(
        backing,
        0,
        replacement.length,
        work,
      );
    }
    final joinedLeft = arena.concatOwned(
      _pairLeft(first),
      replacementRoot,
      work,
    );
    final nextRoot = arena.concatOwned(joinedLeft, _pairRight(second), work);

    final oldAnchor = activeAnchor;
    history.pushOwned(root, oldAnchor, arena);
    root = nextRoot;
    final delta = replacement.length - (end - start);
    if (activeAnchor < start) {
      // No change.
    } else if (activeAnchor > end) {
      activeAnchor += delta;
    } else {
      activeAnchor = anchorDownstream ? start + replacement.length : start;
    }
    revision += 1;
    work.retireNodes = arena.retireSlice(256);
    work.retireMicroseconds = arena.maxRetireSliceMicroseconds;
    maxEditSummaryScanTotal = math.max(
      maxEditSummaryScanTotal,
      arena.totalSummaryScanUtf16 - scanBefore,
    );
    if (arena.maxSummaryScanChunkUtf16 > maxScanBefore) {
      maxEditSummaryScanChunk = math.max(
        maxEditSummaryScanChunk,
        arena.maxSummaryScanChunkUtf16,
      );
    }
  }

  void undo() {
    if (history.length == 0) throw StateError('nothing to undo');
    arena.release(root);
    root = history.popRoot();
    activeAnchor = history.popAnchor();
    revision -= 1;
    arena.retireSlice(256);
  }

  String readRange(int start, int end) => arena.readRange(root, start, end);

  void close() {
    arena.release(root);
    root = 0;
    history.releaseAll(arena);
  }
}

/// Conservative proxy for the simpler challenger: only the current packed
/// root is leased, while undo owns inverse source transactions rather than old
/// tree roots. The tree update is still functional, so a truly mutable AVL/B-
/// tree can only allocate less than this lane.
final class _InverseHistorySession {
  _InverseHistorySession(this.source, int capacity)
    : _starts = Uint32List(capacity),
      _insertedLengths = Uint32List(capacity),
      _deleted = List<String?>.filled(capacity, null);

  final _PackedSourceSession source;
  final Uint32List _starts;
  final Uint32List _insertedLengths;
  final List<String?> _deleted;
  int length = 0;

  void replace(int start, int end, String replacement) {
    if (length == _starts.length) {
      throw StateError('fixed inverse history exhausted');
    }
    final deletedLength = end - start;
    final deleted = deletedLength == 0
        ? ''
        : deletedLength == 1
        ? String.fromCharCode(source.arena.codeUnitAt(source.root, start))
        : source.readRange(start, end);
    _starts[length] = start;
    _insertedLengths[length] = replacement.length;
    _deleted[length] = deleted;
    length += 1;
    source.replace(start, end, replacement);
  }

  void undo() {
    if (length == 0) throw StateError('nothing to undo');
    length -= 1;
    final start = _starts[length];
    final insertedLength = _insertedLengths[length];
    final deleted = _deleted[length]!;
    _deleted[length] = null;
    source.replace(start, start + insertedLength, deleted);
  }

  void close() {
    for (var index = 0; index < length; index += 1) {
      _deleted[index] = null;
    }
    length = 0;
    source.close();
  }
}

void _verifyExactSemantics() {
  final backings = _BackingArena(4096);
  final arena = _NodeArena(backings)..reserveNodes(8192);
  final work = _Work();
  var oracle = 'a\r\nb\rc\n😀z';
  final session = _PackedSourceSession.fromString(
    arena: arena,
    source: oracle,
    certified: true,
    historyCapacity: 32,
    work: work,
  );
  _expect(
    session.readRange(0, session.length) == oracle,
    'initial exact source',
  );
  _expect(arena.lineBreaks(session.root) == 3, 'CRLF and lone CR exact');
  _expect(
    arena.utf8Length(session.root) == utf8.encode(oracle).length,
    'UTF-8 extent exact',
  );

  final edits = <(int, int, String)>[
    (1, 1, '**'),
    (4, 5, 'Q'),
    (0, 1, ''),
    (oracle.length - 1, oracle.length, 'tail'),
  ];
  for (final edit in edits) {
    final start = math.min(edit.$1, oracle.length);
    final end = math.min(math.max(start, edit.$2), oracle.length);
    if (_splitsScalar(oracle, start) || _splitsScalar(oracle, end)) continue;
    session.replace(start, end, edit.$3);
    oracle = oracle.replaceRange(start, end, edit.$3);
    _expect(
      session.readRange(0, session.length) == oracle,
      'packed edit matches String oracle',
    );
    _expect(session.summaryReady, 'small oracle summary remains exact');
    _expect(
      arena.lineBreaks(session.root) == _logicalLineBreaks(oracle),
      'line summary matches String oracle',
    );
    _expect(
      arena.utf8Length(session.root) == utf8.encode(oracle).length,
      'UTF-8 summary matches String oracle',
    );
    arena.checkBalanced(session.root);
  }

  final emoji = oracle.indexOf('😀');
  if (emoji >= 0) {
    _expectThrows(
      () => session.replace(emoji + 1, emoji + 1, 'x'),
      'surrogate boundary rejected',
    );
  }
  final beforeUndo = oracle;
  session.replace(0, 0, 'undo:');
  _expect(session.readRange(0, 5) == 'undo:', 'edit visible before undo');
  session.undo();
  _expect(
    session.readRange(0, session.length) == beforeUndo,
    'old root remains exact for undo',
  );
  session.close();
  while (arena.retirementQueueLength > 0) {
    arena.retireSlice(64);
  }
  _expect(arena.liveNodes == 0, 'semantic test nodes reclaimed');
  _expect(backings.liveBackings == 0, 'semantic test backings reclaimed');

  final inverseBacking = _BackingArena(128);
  final inverseArena = _NodeArena(inverseBacking)..reserveNodes(1024);
  final inverseSource = _PackedSourceSession.fromString(
    arena: inverseArena,
    source: 'alpha\r\nbeta😀',
    certified: true,
    historyCapacity: 0,
    work: _Work(),
  );
  final inverse = _InverseHistorySession(inverseSource, 8);
  inverse.replace(5, 5, '**');
  inverse.replace(0, 1, 'A');
  _expect(
    inverseSource.readRange(0, inverseSource.length) == 'Alpha**\r\nbeta😀',
    'inverse history edits are exact',
  );
  inverse.undo();
  inverse.undo();
  _expect(
    inverseSource.readRange(0, inverseSource.length) == 'alpha\r\nbeta😀',
    'inverse source transactions undo without root leases',
  );
  inverse.close();
  while (inverseArena.retirementQueueLength > 0) {
    inverseArena.retireSlice(64);
  }
  _expect(inverseArena.liveNodes == 0, 'inverse nodes reclaimed');
}

void _verifyPendingLargeBacking() {
  final backings = _BackingArena(128);
  final arena = _NodeArena(backings)..reserveNodes(1024);
  final work = _Work();
  final source = _asciiMarkdownOfLength(2 * _smallExactUtf16);
  final session = _PackedSourceSession.fromString(
    arena: arena,
    source: source,
    certified: false,
    historyCapacity: 8,
    work: work,
  );
  _expect(
    !session.summaryReady,
    'large lazy backing remains explicitly pending',
  );
  session.activeAnchor = source.length ~/ 2;
  session.replace(source.length ~/ 2, source.length ~/ 2 + 1, 'x');
  _expect(!session.summaryReady, 'small edit does not fake bulk certification');
  _expect(session.activeAnchor == source.length ~/ 2 + 1, 'anchor transforms');
  _expect(
    session
        .readRange(source.length ~/ 2 - 1, source.length ~/ 2 + 2)
        .contains('x'),
    'pending source remains exactly readable',
  );
  _expect(
    session.maxEditSummaryScanChunk <= _smallExactUtf16,
    'pending edit performs no document-sized scan',
  );
  session.close();
  while (arena.retirementQueueLength > 0) {
    arena.retireSlice(64);
  }
  _expect(arena.liveNodes == 0, 'pending nodes reclaimed');
}

void _verifyBoundedReclamation() {
  final backings = _BackingArena(1024);
  final arena = _NodeArena(backings)..reserveNodes(4096);
  final session = _PackedSourceSession.fromString(
    arena: arena,
    source: _asciiMarkdownOfLength(4096),
    certified: true,
    historyCapacity: 64,
    work: _Work(),
  );
  var offset = session.length ~/ 2;
  for (var index = 0; index < 500; index += 1) {
    session.replace(offset, offset, index.isEven ? 'x' : 'y');
    offset += 1;
  }
  _expect(session.history.evictions > 0, 'bounded history evicts old roots');
  session.close();
  var slices = 0;
  while (arena.retirementQueueLength > 0) {
    final retired = arena.retireSlice(32);
    _expect(retired <= 32, 'retirement slice is bounded');
    slices += 1;
  }
  _expect(slices > 1, 'large release is iterative');
  _expect(arena.liveNodes == 0, 'all nodes reclaimed iteratively');
  _expect(backings.liveBackings == 0, 'all backing leases reclaimed');
}

Future<void> _runLargeSourceLane({
  required int sizeMiB,
  required int activeEdits,
  required int coldEdits,
  required int reserveNodes,
}) async {
  final source = _asciiMarkdownOfLength(sizeMiB * 1024 * 1024);
  final indexWatch = Stopwatch()..start();
  final index = _RangeIndex.build(source);
  indexWatch.stop();
  final backings = _BackingArena(activeEdits + coldEdits + 4096);
  final arena = _NodeArena(backings);
  final reserveWatch = Stopwatch()..start();
  arena.reserveNodes(reserveNodes);
  reserveWatch.stop();
  final work = _Work();
  final backing = backings.allocate(source, index: index);
  final root = arena.allocateLeaf(backing, 0, source.length, work);
  final session = _PackedSourceSession._(
    arena: arena,
    root: root,
    historyCapacity: 2048,
  );
  session.activeAnchor = source.length ~/ 2;
  final pagesBefore = arena.pageCount;

  final heartbeat = _Heartbeat()..start();
  final sampleClock = Stopwatch()..start();
  await Future<void>.delayed(const Duration(milliseconds: 3));
  final activeSamples = Uint64List(activeEdits);
  final activeOffset = source.length ~/ 2;
  for (var edit = 0; edit < activeEdits; edit += 1) {
    final startTicks = sampleClock.elapsedTicks;
    session.replace(activeOffset, activeOffset + 1, edit.isEven ? 'x' : 'y');
    activeSamples[edit] = _ticksToNanoseconds(
      sampleClock.elapsedTicks - startTicks,
    );
    _blackHole ^= arena.hash32(session.root);
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }

  var seed = 0x13579BDF;
  final coldSamples = Uint64List(coldEdits);
  for (var edit = 0; edit < coldEdits; edit += 1) {
    seed = _next(seed);
    final offset = seed % (source.length - 1);
    final startTicks = sampleClock.elapsedTicks;
    session.replace(offset, offset + 1, edit.isEven ? 'q' : 'r');
    coldSamples[edit] = _ticksToNanoseconds(
      sampleClock.elapsedTicks - startTicks,
    );
    _blackHole ^= arena.hash32(session.root);
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }
  await Future<void>.delayed(const Duration(milliseconds: 3));
  heartbeat.stop();
  arena.checkBalanced(session.root);
  _expect(
    session.readRange(activeOffset - 2, activeOffset + 3).length == 5,
    'bounded active read remains exact',
  );
  _expect(
    session.maxEditSummaryScanChunk <= _checkpointUtf16,
    'no edit scans a document-sized source range',
  );

  _emit('packed_large_source', {
    'source_utf16': source.length,
    'index_build_ms': indexWatch.elapsedMicroseconds / 1000,
    'index_bytes': index.byteLength,
    'reserve_nodes': reserveNodes,
    'reserve_ms': reserveWatch.elapsedMicroseconds / 1000,
    'active_edits': activeEdits,
    ..._Samples(activeSamples).prefixed('active'),
    'cold_edits': coldEdits,
    ..._Samples(coldSamples).prefixed('cold'),
    'heartbeat_max_gap_us': heartbeat.maxGapMicroseconds,
    'tree_height': arena.height(session.root),
    'piece_count': arena.pieceCount(session.root),
    'node_pages': arena.pageCount,
    'page_growth_during_edits': arena.pageCount - pagesBefore,
    'node_capacity': arena.capacity,
    'node_live': arena.liveNodes,
    'node_high_water': arena.highWaterLiveNodes,
    'node_allocations': arena.allocations,
    'node_reuses': arena.reusedNodes,
    'retirement_queue': arena.retirementQueueLength,
    'retirement_queue_max': arena.maxRetirementQueueLength,
    'retired_nodes': arena.retiredNodes,
    'retire_slice_max_nodes': arena.maxRetireSliceNodes,
    'retire_slice_max_us': arena.maxRetireSliceMicroseconds,
    'backings_live': backings.liveBackings,
    'backings_high_water': backings.highWaterBackings,
    'max_summary_scan_chunk_utf16': arena.maxSummaryScanChunkUtf16,
    'max_edit_summary_scan_total_utf16': session.maxEditSummaryScanTotal,
    'rss_mib': _rssMiB(),
  });

  session.close();
  var drainSlices = 0;
  var maxDrainNodes = 0;
  var maxDrainUs = 0;
  final drainClock = Stopwatch()..start();
  while (arena.retirementQueueLength > 0) {
    final startTicks = drainClock.elapsedTicks;
    final retired = arena.retireSlice(256);
    drainSlices += 1;
    maxDrainNodes = math.max(maxDrainNodes, retired);
    maxDrainUs = math.max(
      maxDrainUs,
      _ticksToMicroseconds(drainClock.elapsedTicks - startTicks),
    );
    await Future<void>.delayed(Duration.zero);
  }
  _expect(arena.liveNodes == 0, 'large lane nodes fully reclaimed');
  _expect(backings.liveBackings == 0, 'large lane backings fully reclaimed');
  _emit('packed_large_source_drain', {
    'slices': drainSlices,
    'slice_budget': 256,
    'max_nodes': maxDrainNodes,
    'max_us': maxDrainUs,
    'live_nodes_after': arena.liveNodes,
    'live_backings_after': backings.liveBackings,
  });

  // A conservative lower-complexity challenger. It still uses functional
  // packed edits, but discards the old root immediately and keeps inverse text
  // transactions. A truly mutable current tree would remove additional path
  // allocation; this lane asks whether root persistence itself buys enough to
  // justify its lifetime machinery.
  final challengerWork = _Work();
  final challengerBacking = backings.allocate(source, index: index);
  final challengerRoot = arena.allocateLeaf(
    challengerBacking,
    0,
    source.length,
    challengerWork,
  );
  final challengerSource = _PackedSourceSession._(
    arena: arena,
    root: challengerRoot,
    historyCapacity: 0,
  );
  challengerSource.activeAnchor = source.length ~/ 2;
  final challenger = _InverseHistorySession(
    challengerSource,
    activeEdits + coldEdits,
  );
  arena.resetHotDiagnostics();
  final challengerPagesBefore = arena.pageCount;
  final challengerAllocationsBefore = arena.allocations;
  final challengerReusesBefore = arena.reusedNodes;
  final challengerRetiredBefore = arena.retiredNodes;
  final challengerActive = Uint64List(activeEdits);
  final challengerCold = Uint64List(coldEdits);
  final challengerHeartbeat = _Heartbeat()..start();
  final challengerClock = Stopwatch()..start();
  await Future<void>.delayed(const Duration(milliseconds: 3));
  for (var edit = 0; edit < activeEdits; edit += 1) {
    final startTicks = challengerClock.elapsedTicks;
    challenger.replace(activeOffset, activeOffset + 1, edit.isEven ? 'x' : 'y');
    challengerActive[edit] = _ticksToNanoseconds(
      challengerClock.elapsedTicks - startTicks,
    );
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }
  seed = 0x13579BDF;
  for (var edit = 0; edit < coldEdits; edit += 1) {
    seed = _next(seed);
    final offset = seed % (source.length - 1);
    final startTicks = challengerClock.elapsedTicks;
    challenger.replace(offset, offset + 1, edit.isEven ? 'q' : 'r');
    challengerCold[edit] = _ticksToNanoseconds(
      challengerClock.elapsedTicks - startTicks,
    );
    if ((edit & 7) == 7) await Future<void>.delayed(Duration.zero);
  }
  await Future<void>.delayed(const Duration(milliseconds: 3));
  challengerHeartbeat.stop();
  arena.checkBalanced(challengerSource.root);
  _emit('packed_inverse_history_challenger', {
    'source_utf16': source.length,
    'inverse_entries': challenger.length,
    'active_edits': activeEdits,
    ..._Samples(challengerActive).prefixed('active'),
    'cold_edits': coldEdits,
    ..._Samples(challengerCold).prefixed('cold'),
    'heartbeat_max_gap_us': challengerHeartbeat.maxGapMicroseconds,
    'tree_height': arena.height(challengerSource.root),
    'piece_count': arena.pieceCount(challengerSource.root),
    'node_pages': arena.pageCount,
    'page_growth_during_edits': arena.pageCount - challengerPagesBefore,
    'node_live': arena.liveNodes,
    'node_high_water': arena.highWaterLiveNodes,
    'node_allocations': arena.allocations - challengerAllocationsBefore,
    'node_reuses': arena.reusedNodes - challengerReusesBefore,
    'retired_nodes': arena.retiredNodes - challengerRetiredBefore,
    'retirement_queue': arena.retirementQueueLength,
    'retirement_queue_max': arena.maxRetirementQueueLength,
    'retire_slice_max_nodes': arena.maxRetireSliceNodes,
    'retire_slice_max_us': arena.maxRetireSliceMicroseconds,
    'backings_live': backings.liveBackings,
    'backings_high_water': backings.highWaterBackings,
    'max_summary_scan_chunk_utf16': arena.maxSummaryScanChunkUtf16,
    'max_edit_summary_scan_total_utf16':
        challengerSource.maxEditSummaryScanTotal,
    'rss_mib': _rssMiB(),
  });
  challenger.close();
  var challengerDrainSlices = 0;
  var challengerDrainMaxUs = 0;
  final challengerDrainClock = Stopwatch()..start();
  while (arena.retirementQueueLength > 0) {
    final startTicks = challengerDrainClock.elapsedTicks;
    arena.retireSlice(256);
    challengerDrainSlices += 1;
    challengerDrainMaxUs = math.max(
      challengerDrainMaxUs,
      _ticksToMicroseconds(challengerDrainClock.elapsedTicks - startTicks),
    );
    await Future<void>.delayed(Duration.zero);
  }
  _expect(arena.liveNodes == 0, 'challenger nodes fully reclaimed');
  _expect(backings.liveBackings == 0, 'challenger backings fully reclaimed');
  _emit('packed_inverse_history_drain', {
    'slices': challengerDrainSlices,
    'slice_budget': 256,
    'max_us': challengerDrainMaxUs,
    'live_nodes_after': arena.liveNodes,
    'live_backings_after': backings.liveBackings,
  });
}

Future<void> _runHistoryChurnLane({
  required int edits,
  required int reserveNodes,
}) async {
  final backings = _BackingArena(edits + 4096);
  final arena = _NodeArena(backings);
  final reserveWatch = Stopwatch()..start();
  arena.reserveNodes(reserveNodes);
  reserveWatch.stop();
  final session = _PackedSourceSession.fromString(
    arena: arena,
    source: _asciiMarkdownOfLength(64 * 1024),
    certified: true,
    historyCapacity: 2048,
    work: _Work(),
  );
  var caret = session.length ~/ 2;
  session.activeAnchor = caret;
  final pagesBefore = arena.pageCount;
  final samples = Uint64List(edits);
  var maxVisited = 0;
  var maxAllocated = 0;
  final heartbeat = _Heartbeat()..start();
  final sampleClock = Stopwatch()..start();
  await Future<void>.delayed(const Duration(milliseconds: 3));
  for (var edit = 0; edit < edits; edit += 1) {
    final startTicks = sampleClock.elapsedTicks;
    session.replace(caret, caret, edit.isEven ? 'x' : 'y');
    samples[edit] = _ticksToNanoseconds(sampleClock.elapsedTicks - startTicks);
    caret += 1;
    maxVisited = math.max(maxVisited, session.work.nodesVisited);
    maxAllocated = math.max(maxAllocated, session.work.nodesAllocated);
    _blackHole ^= arena.hash32(session.root);
    if ((edit & 31) == 31) await Future<void>.delayed(Duration.zero);
  }
  await Future<void>.delayed(const Duration(milliseconds: 3));
  heartbeat.stop();
  arena.checkBalanced(session.root);
  _emit('packed_history_churn', {
    'edits': edits,
    ..._Samples(samples).json,
    'heartbeat_max_gap_us': heartbeat.maxGapMicroseconds,
    'reserve_ms': reserveWatch.elapsedMicroseconds / 1000,
    'tree_height': arena.height(session.root),
    'piece_count': arena.pieceCount(session.root),
    'history_entries': session.history.length,
    'history_evictions': session.history.evictions,
    'max_nodes_visited_per_edit': maxVisited,
    'max_nodes_allocated_per_edit': maxAllocated,
    'node_pages': arena.pageCount,
    'page_growth_during_edits': arena.pageCount - pagesBefore,
    'node_capacity': arena.capacity,
    'node_live': arena.liveNodes,
    'node_high_water': arena.highWaterLiveNodes,
    'node_allocations': arena.allocations,
    'node_reuses': arena.reusedNodes,
    'retirement_queue': arena.retirementQueueLength,
    'retirement_queue_max': arena.maxRetirementQueueLength,
    'retired_nodes': arena.retiredNodes,
    'retire_slice_max_nodes': arena.maxRetireSliceNodes,
    'retire_slice_max_us': arena.maxRetireSliceMicroseconds,
    'backings_live': backings.liveBackings,
    'backings_high_water': backings.highWaterBackings,
    'rss_mib': _rssMiB(),
  });

  session.close();
  var slices = 0;
  var maxNodes = 0;
  var maxMicroseconds = 0;
  final drainClock = Stopwatch()..start();
  while (arena.retirementQueueLength > 0) {
    final startTicks = drainClock.elapsedTicks;
    final retired = arena.retireSlice(256);
    slices += 1;
    maxNodes = math.max(maxNodes, retired);
    maxMicroseconds = math.max(
      maxMicroseconds,
      _ticksToMicroseconds(drainClock.elapsedTicks - startTicks),
    );
    await Future<void>.delayed(Duration.zero);
  }
  _expect(arena.liveNodes == 0, 'churn nodes fully reclaimed');
  _expect(backings.liveBackings == 0, 'churn backings fully reclaimed');
  _emit('packed_history_drain', {
    'slices': slices,
    'slice_budget': 256,
    'max_nodes': maxNodes,
    'max_us': maxMicroseconds,
    'live_nodes_after': arena.liveNodes,
    'live_backings_after': backings.liveBackings,
  });
}

final class _Heartbeat {
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
    _watch.stop();
  }
}

final class _Samples {
  _Samples(Uint64List values) : _values = values.toList()..sort() {
    if (_values.isEmpty) throw ArgumentError.value(values);
  }

  final List<int> _values;

  Map<String, Object> get json => {
    'p50_us': _at(50),
    'p99_us': _at(99),
    'p999_us': _at(999, denominator: 1000),
    'max_us': _values.last / 1000,
  };

  Map<String, Object> prefixed(String prefix) => {
    '${prefix}_p50_us': _at(50),
    '${prefix}_p99_us': _at(99),
    '${prefix}_p999_us': _at(999, denominator: 1000),
    '${prefix}_max_us': _values.last / 1000,
  };

  double _at(int percentile, {int denominator = 100}) =>
      _values[((_values.length - 1) * percentile) ~/ denominator] / 1000;
}

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
      int.parse(_values[name] ?? '$fallback');
}

bool _splitsScalar(String source, int offset) =>
    offset > 0 &&
    offset < source.length &&
    _isHighSurrogate(source.codeUnitAt(offset - 1)) &&
    _isLowSurrogate(source.codeUnitAt(offset));

bool _isHighSurrogate(int value) => value >= 0xD800 && value <= 0xDBFF;
bool _isLowSurrogate(int value) => value >= 0xDC00 && value <= 0xDFFF;

int _appendScalarHash(int hash, int scalar) {
  if (scalar <= 0x7F) {
    return _appendHashByte(hash, scalar);
  } else if (scalar <= 0x7FF) {
    hash = _appendHashByte(hash, 0xC0 | (scalar >> 6));
    return _appendHashByte(hash, 0x80 | (scalar & 0x3F));
  } else if (scalar <= 0xFFFF) {
    hash = _appendHashByte(hash, 0xE0 | (scalar >> 12));
    hash = _appendHashByte(hash, 0x80 | ((scalar >> 6) & 0x3F));
    return _appendHashByte(hash, 0x80 | (scalar & 0x3F));
  }
  hash = _appendHashByte(hash, 0xF0 | (scalar >> 18));
  hash = _appendHashByte(hash, 0x80 | ((scalar >> 12) & 0x3F));
  hash = _appendHashByte(hash, 0x80 | ((scalar >> 6) & 0x3F));
  return _appendHashByte(hash, 0x80 | (scalar & 0x3F));
}

int _appendHashByte(int hash, int value) =>
    (_mul32(hash, _hashBase) + value + 1) & _mask32;

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

String _asciiMarkdownOfLength(int length) {
  const line =
      'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  final count = length ~/ line.length;
  final remainder = length % line.length;
  return '${List<String>.filled(count, line).join()}'
      '${line.substring(0, remainder)}';
}

int _logicalLineBreaks(String source) {
  var lines = 0;
  var cursor = 0;
  while (cursor < source.length) {
    final unit = source.codeUnitAt(cursor);
    if (unit == 0x0D) {
      lines += 1;
      if (cursor + 1 < source.length && source.codeUnitAt(cursor + 1) == 0x0A) {
        cursor += 2;
        continue;
      }
    } else if (unit == 0x0A) {
      lines += 1;
    }
    cursor += 1;
  }
  return lines;
}

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

var _blackHole = 0;
final int _stopwatchFrequency = Stopwatch().frequency;

int _ticksToNanoseconds(int ticks) => ticks * 1000000000 ~/ _stopwatchFrequency;

int _ticksToMicroseconds(int ticks) => ticks * 1000000 ~/ _stopwatchFrequency;

double _rssMiB() => ProcessInfo.currentRss / (1024 * 1024);

void _expect(bool condition, String message) {
  if (!condition) throw StateError(message);
}

void _expectThrows(void Function() body, String message) {
  try {
    body();
  } on FormatException {
    return;
  }
  throw StateError(message);
}

void _emit(String event, Map<String, Object> values) {
  stdout.writeln(jsonEncode({'event': event, ...values}));
}
