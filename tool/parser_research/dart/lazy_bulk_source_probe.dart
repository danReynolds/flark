import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

/// Disposable Option-A/Option-B source-ownership spike.
///
/// This deliberately does not modify the v3 production source. It asks a
/// narrower question: can the main isolate adopt a large immutable Dart String
/// without walking it, while keeping exact UTF-16 edits/selection/undo?
///
/// Run this AOT for useful timings:
///
///   dart compile exe tool/parser_research/dart/lazy_bulk_source_probe.dart \
///     -o /tmp/flark_lazy_bulk_probe
///   /tmp/flark_lazy_bulk_probe --size-mib=10
Future<void> main(List<String> arguments) async {
  final options = _Options(arguments);
  final sizeMiB = options.integer('size-mib', 10);
  final iterations = options.integer('iterations', sizeMiB >= 100 ? 200 : 800);
  final source = _asciiMarkdownOfLength(sizeMiB * 1024 * 1024);

  _emit('environment', {
    'dart': Platform.version.split('\n').first,
    'os': Platform.operatingSystem,
    'size_mib': sizeMiB,
    'rss_mib': _rssMiB(),
  });

  _verifyModel();
  _runStringPrimitiveReceipts(source, iterations);
  _runOptionAReceipts(source, iterations);
  _runOptionBComparison(source);
  _emit('probe_complete', {'black_hole': _blackHole, 'rss_mib': _rssMiB()});
}

/// A logical candidate snapshot, not necessarily a committed source revision.
///
/// Pending bulk content may be echoed by the UI and edited/undone exactly, but
/// the last certified snapshot remains canonical until a worker validates this
/// candidate and returns its fingerprint. Invalid candidates are rejected;
/// they are never silently encoded with replacement characters.
final class LazyBulkSourceDocument {
  LazyBulkSourceDocument._({
    required List<_Piece> pieces,
    required this.revision,
  }) : _pieces = List.unmodifiable(pieces),
       utf16Length = pieces.fold(0, (total, piece) => total + piece.length);

  /// Adopts [source] without normalizing, validating, hashing, encoding, or
  /// scanning for newlines.
  ///
  /// This is the hot-paste contract. Initial file-open CRLF normalization is a
  /// separate staged operation because its normalized UTF-16 extent cannot be
  /// known without reading the input.
  factory LazyBulkSourceDocument.adoptBulk(String source) {
    if (source.isEmpty) {
      return LazyBulkSourceDocument._(pieces: const [], revision: 0);
    }
    final backing = _Backing.bulk(source);
    return LazyBulkSourceDocument._(
      pieces: [_Piece.whole(backing)],
      revision: 0,
    );
  }

  final List<_Piece> _pieces;
  final int revision;
  final int utf16Length;

  /// Summary APIs are typed as unavailable until all live bulk backings have
  /// been scanned. Returning zero or a provisional hash would be incorrect.
  LazySummary<int> get utf8Length => _summary((index) => index.utf8Length);

  LazySummary<int> get lineCount {
    var lineBreaks = 0;
    int? previousLast;
    for (final piece in _pieces) {
      final invalid = piece.backing.index.invalidUtf16Offset;
      if (invalid != null &&
          invalid >= piece.backingStart &&
          invalid < piece.backingStart + piece.length) {
        return LazySummary.invalid(invalid);
      }
      final start = piece.backing.index.summaryAt(piece.backingStart);
      final end = piece.backing.index.summaryAt(
        piece.backingStart + piece.length,
      );
      if (start == null || end == null) return const LazySummary.pending();
      var pieceBreaks = end.lineBreaks - start.lineBreaks;
      if (piece.backingStart > 0 &&
          piece.length > 0 &&
          piece.backing.source.codeUnitAt(piece.backingStart) == 0x0A &&
          piece.backing.source.codeUnitAt(piece.backingStart - 1) == 0x0D) {
        pieceBreaks += 1;
      }
      final first = piece.length == 0 ? null : piece.codeUnitAt(0);
      if (previousLast == 0x0D && first == 0x0A) pieceBreaks -= 1;
      lineBreaks += pieceBreaks;
      previousLast = piece.length == 0
          ? previousLast
          : piece.codeUnitAt(piece.length - 1);
    }
    return LazySummary.ready(lineBreaks + 1);
  }

  /// The spike only exposes a hash for the unsliced whole backing. A real
  /// implementation needs composable range-hash checkpoints or, preferably,
  /// adopts the worker-certified revision hash after applying UTF-16 intents.
  LazySummary<int> get contentHash32 {
    if (_pieces.length != 1) return const LazySummary.pending();
    final piece = _pieces.single;
    if (piece.backingStart != 0 ||
        piece.length != piece.backing.source.length) {
      return const LazySummary.pending();
    }
    final invalid = piece.backing.index.invalidUtf16Offset;
    if (invalid != null) {
      return LazySummary.invalid(invalid);
    }
    final summary = piece.backing.index.summaryAt(piece.length);
    if (summary == null) return const LazySummary.pending();
    return LazySummary.ready(summary.hash32);
  }

  LazyDocumentCertification get certification {
    var pending = false;
    var documentOffset = 0;
    for (final piece in _pieces) {
      final invalid = piece.backing.index.invalidUtf16Offset;
      if (invalid != null &&
          invalid >= piece.backingStart &&
          invalid < piece.backingStart + piece.length) {
        return LazyDocumentCertification.invalid(
          documentOffset + invalid - piece.backingStart,
        );
      }
      if (piece.backing.index.cursor < piece.backingStart + piece.length) {
        pending = true;
      }
      documentOffset += piece.length;
    }
    return pending
        ? const LazyDocumentCertification.pending()
        : const LazyDocumentCertification.valid();
  }

  /// Exact bounded source reads are available before enrichment.
  String readRange(int startUtf16, int endUtf16) {
    _checkRange(startUtf16, endUtf16);
    if (startUtf16 == endUtf16) return '';
    final output = StringBuffer();
    var documentOffset = 0;
    for (final piece in _pieces) {
      final pieceEnd = documentOffset + piece.length;
      if (pieceEnd > startUtf16 && documentOffset < endUtf16) {
        final localStart = math.max(0, startUtf16 - documentOffset);
        final localEnd = math.min(piece.length, endUtf16 - documentOffset);
        output.write(piece.read(localStart, localEnd));
      }
      documentOffset = pieceEnd;
      if (documentOffset >= endUtf16) break;
    }
    return output.toString();
  }

  /// Produces a piece-relative anchor without reading the piece payload.
  LazyPieceAnchor anchorAt(
    int utf16Offset, {
    LazyAnchorAffinity affinity = LazyAnchorAffinity.downstream,
  }) {
    _checkOffset(utf16Offset);
    if (_pieces.isEmpty) {
      return LazyPieceAnchor(
        pieceId: 0,
        originUtf16Offset: 0,
        affinity: affinity,
      );
    }
    var documentOffset = 0;
    for (var index = 0; index < _pieces.length; index += 1) {
      final piece = _pieces[index];
      final atEnd = utf16Offset == documentOffset + piece.length;
      final chooseEnd =
          atEnd &&
          (affinity == LazyAnchorAffinity.upstream ||
              index == _pieces.length - 1);
      if (utf16Offset < documentOffset + piece.length || chooseEnd) {
        return LazyPieceAnchor(
          pieceId: piece.pieceId,
          originUtf16Offset: piece.originStart + (utf16Offset - documentOffset),
          affinity: affinity,
        );
      }
      documentOffset += piece.length;
    }
    throw StateError('unreachable anchor lookup');
  }

  /// Resolves an anchor while its referenced source position survives.
  /// Deleted anchors intentionally return null; a selection policy transforms
  /// them at edit time instead of guessing later.
  int? resolveAnchor(LazyPieceAnchor anchor) {
    var documentOffset = 0;
    int? upstreamBoundary;
    int? downstreamBoundary;
    for (final piece in _pieces) {
      if (piece.pieceId == anchor.pieceId) {
        final originEnd = piece.originStart + piece.length;
        if (anchor.originUtf16Offset > piece.originStart &&
            anchor.originUtf16Offset < originEnd) {
          return documentOffset + anchor.originUtf16Offset - piece.originStart;
        }
        if (anchor.originUtf16Offset == piece.originStart) {
          downstreamBoundary ??= documentOffset;
          if (anchor.affinity == LazyAnchorAffinity.downstream) {
            return documentOffset;
          }
        }
        if (anchor.originUtf16Offset == originEnd) {
          upstreamBoundary = documentOffset + piece.length;
          if (anchor.affinity == LazyAnchorAffinity.upstream) {
            return upstreamBoundary;
          }
        }
      }
      documentOffset += piece.length;
    }
    return anchor.affinity == LazyAnchorAffinity.upstream
        ? upstreamBoundary ?? downstreamBoundary
        : downstreamBoundary ?? upstreamBoundary;
  }

  /// Applies an exact UTF-16 edit. Small replacement payloads are validated
  /// and indexed synchronously; a large replacement becomes another pending
  /// bulk backing without being walked.
  LazyBulkEditResult replaceRange(
    int startUtf16,
    int endUtf16,
    String replacement, {
    int synchronousPayloadLimit = 8 * 1024,
    int synchronousCompactionLimit = 8 * 1024,
  }) {
    _checkRange(startUtf16, endUtf16);
    _checkScalarBoundary(startUtf16);
    _checkScalarBoundary(endUtf16);

    final splitStart = _splitPieces(_pieces, startUtf16);
    final splitEnd = _splitPieces(splitStart.right, endUtf16 - startUtf16);
    final nextPieces = <_Piece>[...splitStart.left];
    if (replacement.isNotEmpty) {
      final backing = replacement.length <= synchronousPayloadLimit
          ? _Backing.small(replacement)
          : _Backing.bulk(replacement);
      nextPieces.add(_Piece.whole(backing));
    }
    nextPieces.addAll(splitEnd.right);

    // This only compacts tiny survivors. Larger high-ratio survivors retain
    // their backing until an off-frame compaction job publishes a replacement
    // piece; substring/copy is an atomic payload-sized operation in Dart.
    var compactionBudget = synchronousCompactionLimit;
    final compacted = <_Piece>[];
    for (final piece in nextPieces) {
      final result = piece.compactWithin(compactionBudget);
      compacted.add(result.piece);
      compactionBudget -= result.copiedUtf16;
    }

    final next = LazyBulkSourceDocument._(
      pieces: compacted,
      revision: revision + 1,
    );
    return LazyBulkEditResult(
      before: this,
      document: next,
      workerIntent: LazyWorkerUtf16EditIntent(
        baseRevision: revision,
        revision: next.revision,
        startUtf16: startUtf16,
        endUtf16: endUtf16,
        replacement: replacement,
        replacementIsPendingBulk: replacement.length > synchronousPayloadLimit,
      ),
    );
  }

  /// Scans at most [maxUtf16] plus one low surrogate in one backing.
  ///
  /// This models one cooperative worker transition. An actual isolate sends
  /// immutable backing handles and returns checkpoints; it must not execute
  /// this loop in a Flutter frame callback.
  LazyEnrichmentReceipt enrichNext({int maxUtf16 = 8 * 1024}) {
    if (maxUtf16 < 1) throw RangeError.range(maxUtf16, 1, null, 'maxUtf16');
    for (final backing in _uniqueBackings) {
      if (!backing.index.isComplete) {
        return backing.index.enrich(maxUtf16);
      }
    }
    return const LazyEnrichmentReceipt.idle();
  }

  /// Byte coordinates exist only after the relevant backing prefixes are
  /// enriched. The UI-to-worker protocol therefore carries UTF-16 intents;
  /// the worker derives this byte coordinate against its source snapshot.
  int? utf16ToUtf8IfIndexed(int utf16Offset) {
    _checkOffset(utf16Offset);
    var documentUtf16 = 0;
    var documentUtf8 = 0;
    for (final piece in _pieces) {
      final localEnd = utf16Offset - documentUtf16;
      final pieceUtf8Start = piece.backing.index.utf8At(piece.backingStart);
      if (pieceUtf8Start == null) return null;
      if (localEnd <= piece.length) {
        final target = piece.backing.index.utf8At(
          piece.backingStart + localEnd,
        );
        return target == null ? null : documentUtf8 + target - pieceUtf8Start;
      }
      final pieceUtf8End = piece.backing.index.utf8At(
        piece.backingStart + piece.length,
      );
      if (pieceUtf8End == null) return null;
      documentUtf8 += pieceUtf8End - pieceUtf8Start;
      documentUtf16 += piece.length;
    }
    return documentUtf8;
  }

  LazyBulkDiagnostics get diagnostics {
    final backings = _uniqueBackings.toList(growable: false);
    return LazyBulkDiagnostics(
      pieces: _pieces.length,
      uniqueBackings: backings.length,
      retainedBackingUtf16: backings.fold(
        0,
        (total, backing) => total + backing.source.length,
      ),
      liveUtf16: utf16Length,
      indexedUtf16: backings.fold(
        0,
        (total, backing) => total + backing.index.cursor,
      ),
      pendingBackings: backings
          .where((backing) => !backing.index.isComplete)
          .length,
    );
  }

  Iterable<_Backing> get _uniqueBackings sync* {
    final seen = <int>{};
    for (final piece in _pieces) {
      if (seen.add(piece.backing.id)) yield piece.backing;
    }
  }

  LazySummary<int> _summary(
    int Function(_IndexSummary index) value, {
    int Function(int value)? finish,
  }) {
    var total = 0;
    for (final piece in _pieces) {
      final invalid = piece.backing.index.invalidUtf16Offset;
      if (invalid != null &&
          invalid >= piece.backingStart &&
          invalid < piece.backingStart + piece.length) {
        return LazySummary.invalid(invalid);
      }
      final start = piece.backing.index.summaryAt(piece.backingStart);
      final end = piece.backing.index.summaryAt(
        piece.backingStart + piece.length,
      );
      if (start == null || end == null) {
        return const LazySummary.pending();
      }
      total += value(end) - value(start);
    }
    return LazySummary.ready(finish?.call(total) ?? total);
  }

  void _checkOffset(int offset) {
    if (offset < 0 || offset > utf16Length) {
      throw RangeError.range(offset, 0, utf16Length, 'utf16Offset');
    }
  }

  void _checkRange(int start, int end) {
    _checkOffset(start);
    _checkOffset(end);
    if (end < start) throw RangeError.range(end, start, utf16Length, 'end');
  }

  void _checkScalarBoundary(int offset) {
    if (offset == 0 || offset == utf16Length) return;
    final previous = _codeUnitAt(offset - 1);
    final next = _codeUnitAt(offset);
    if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
      throw FormatException('UTF-16 offset $offset splits a scalar value.');
    }
  }

  int _codeUnitAt(int offset) {
    var local = offset;
    for (final piece in _pieces) {
      if (local < piece.length) return piece.codeUnitAt(local);
      local -= piece.length;
    }
    throw RangeError.index(offset, this, 'offset', null, utf16Length);
  }
}

enum LazySummaryStatus { pending, ready, invalid }

final class LazySummary<T> {
  const LazySummary.pending()
    : status = LazySummaryStatus.pending,
      value = null,
      invalidUtf16Offset = null;

  const LazySummary.ready(this.value)
    : status = LazySummaryStatus.ready,
      invalidUtf16Offset = null;

  const LazySummary.invalid(this.invalidUtf16Offset)
    : status = LazySummaryStatus.invalid,
      value = null;

  final LazySummaryStatus status;
  final T? value;
  final int? invalidUtf16Offset;
}

enum LazyValidationStatus { pending, valid, invalid }

final class LazyDocumentCertification {
  const LazyDocumentCertification.pending()
    : status = LazyValidationStatus.pending,
      invalidUtf16Offset = null;

  const LazyDocumentCertification.valid()
    : status = LazyValidationStatus.valid,
      invalidUtf16Offset = null;

  const LazyDocumentCertification.invalid(this.invalidUtf16Offset)
    : status = LazyValidationStatus.invalid;

  final LazyValidationStatus status;
  final int? invalidUtf16Offset;
}

final class LazyBulkEditResult {
  const LazyBulkEditResult({
    required this.before,
    required this.document,
    required this.workerIntent,
  });

  final LazyBulkSourceDocument before;
  final LazyBulkSourceDocument document;
  final LazyWorkerUtf16EditIntent workerIntent;

  bool get isProvisional =>
      document.certification.status == LazyValidationStatus.pending;

  LazyBulkSourceDocument undo() => before;
}

/// This replaces the eager v3 parser edit's UTF-8 coordinates/bytes/hash for a
/// pending bulk operation. The worker receives the immutable String handle and
/// derives byte coordinates, validation, and the next fingerprint.
final class LazyWorkerUtf16EditIntent {
  const LazyWorkerUtf16EditIntent({
    required this.baseRevision,
    required this.revision,
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
    required this.replacementIsPendingBulk,
  });

  final int baseRevision;
  final int revision;
  final int startUtf16;
  final int endUtf16;
  final String replacement;
  final bool replacementIsPendingBulk;
}

enum LazyAnchorAffinity { upstream, downstream }

final class LazyPieceAnchor {
  const LazyPieceAnchor({
    required this.pieceId,
    required this.originUtf16Offset,
    required this.affinity,
  });

  final int pieceId;
  final int originUtf16Offset;
  final LazyAnchorAffinity affinity;
}

/// Global UTF-16 selection state can be transformed from an edit descriptor
/// without source scanning. Boundary affinity is explicit so insertion at an
/// endpoint is not guessed.
final class LazyUtf16Selection {
  const LazyUtf16Selection({required this.base, required this.extent});

  final int base;
  final int extent;

  LazyUtf16Selection transformForReplace({
    required int start,
    required int end,
    required int replacementLength,
    LazyAnchorAffinity baseAffinity = LazyAnchorAffinity.upstream,
    LazyAnchorAffinity extentAffinity = LazyAnchorAffinity.downstream,
  }) => LazyUtf16Selection(
    base: _transformPosition(base, start, end, replacementLength, baseAffinity),
    extent: _transformPosition(
      extent,
      start,
      end,
      replacementLength,
      extentAffinity,
    ),
  );
}

int _transformPosition(
  int position,
  int start,
  int end,
  int replacementLength,
  LazyAnchorAffinity affinity,
) {
  final delta = replacementLength - (end - start);
  if (position < start) return position;
  if (position > end) return position + delta;
  return affinity == LazyAnchorAffinity.upstream
      ? start
      : start + replacementLength;
}

final class LazyBulkDiagnostics {
  const LazyBulkDiagnostics({
    required this.pieces,
    required this.uniqueBackings,
    required this.retainedBackingUtf16,
    required this.liveUtf16,
    required this.indexedUtf16,
    required this.pendingBackings,
  });

  final int pieces;
  final int uniqueBackings;
  final int retainedBackingUtf16;
  final int liveUtf16;
  final int indexedUtf16;
  final int pendingBackings;
}

final class LazyEnrichmentReceipt {
  const LazyEnrichmentReceipt({
    required this.backingId,
    required this.scannedUtf16,
    required this.status,
    required this.cursorUtf16,
  });

  const LazyEnrichmentReceipt.idle()
    : backingId = 0,
      scannedUtf16 = 0,
      status = LazyValidationStatus.valid,
      cursorUtf16 = 0;

  final int backingId;
  final int scannedUtf16;
  final LazyValidationStatus status;
  final int cursorUtf16;
}

final class _Backing {
  _Backing._(this.source, {required bool eager})
    : id = _nextBackingId++,
      index = _LazyBackingIndex(source) {
    if (eager && source.isNotEmpty) index.enrich(source.length);
  }

  factory _Backing.bulk(String source) => _Backing._(source, eager: false);

  factory _Backing.small(String source) {
    final backing = _Backing._(source, eager: true);
    if (backing.index.status == LazyValidationStatus.invalid) {
      throw FormatException(
        'replacement contains an unpaired surrogate at '
        '${backing.index.invalidUtf16Offset}',
      );
    }
    return backing;
  }

  final int id;
  final String source;
  final _LazyBackingIndex index;
}

var _nextBackingId = 1;
var _nextPieceId = 1;

final class _Piece {
  const _Piece({
    required this.pieceId,
    required this.originStart,
    required this.backing,
    required this.backingStart,
    required this.length,
  });

  factory _Piece.whole(_Backing backing) => _Piece(
    pieceId: _nextPieceId++,
    originStart: 0,
    backing: backing,
    backingStart: 0,
    length: backing.source.length,
  );

  final int pieceId;
  final int originStart;
  final _Backing backing;
  final int backingStart;
  final int length;

  String read(int start, int end) {
    final absoluteStart = backingStart + start;
    final absoluteEnd = backingStart + end;
    // Passing start/end directly to String.fromCharCodes over String.codeUnits
    // walked the iterable prefix on the tested VM (a 4 KiB suffix read from a
    // 10 MiB backing took ~100 ms). Materialize only the bounded sublist first.
    return String.fromCharCodes(
      backing.source.codeUnits.sublist(absoluteStart, absoluteEnd),
    );
  }

  int codeUnitAt(int offset) =>
      backing.source.codeUnitAt(backingStart + offset);

  _Piece slice(int start, int end) => _Piece(
    pieceId: pieceId,
    originStart: originStart + start,
    backing: backing,
    backingStart: backingStart + start,
    length: end - start,
  );

  _CompactionResult compactWithin(int budget) {
    const oversizedBacking = 1024 * 1024;
    if (length == 0 ||
        length > budget ||
        backing.source.length < oversizedBacking ||
        length * 8 >= backing.source.length) {
      return _CompactionResult(this, 0);
    }
    final owned = _Backing.small(read(0, length));
    return _CompactionResult(
      _Piece(
        pieceId: pieceId,
        originStart: originStart,
        backing: owned,
        backingStart: 0,
        length: length,
      ),
      length,
    );
  }
}

final class _CompactionResult {
  const _CompactionResult(this.piece, this.copiedUtf16);

  final _Piece piece;
  final int copiedUtf16;
}

final class _PieceSplit {
  const _PieceSplit(this.left, this.right);

  final List<_Piece> left;
  final List<_Piece> right;
}

_PieceSplit _splitPieces(List<_Piece> pieces, int offset) {
  if (offset == 0) return _PieceSplit(const [], pieces);
  final left = <_Piece>[];
  final right = <_Piece>[];
  var remaining = offset;
  var split = false;
  for (final piece in pieces) {
    if (split) {
      right.add(piece);
      continue;
    }
    if (remaining >= piece.length) {
      left.add(piece);
      remaining -= piece.length;
      if (remaining == 0) split = true;
      continue;
    }
    if (remaining > 0) left.add(piece.slice(0, remaining));
    if (remaining < piece.length) {
      right.add(piece.slice(remaining, piece.length));
    }
    remaining = 0;
    split = true;
  }
  if (remaining != 0) throw RangeError.value(offset, 'offset');
  return _PieceSplit(left, right);
}

final class _LazyBackingIndex {
  _LazyBackingIndex(this.source)
    : _checkpoints = [_IndexSummary(0, 0, 0, _hashSeed)];

  final String source;
  final List<_IndexSummary> _checkpoints;
  int cursor = 0;
  int utf8Length = 0;
  int lineBreaks = 0;
  int hash32 = _hashSeed;
  int? invalidUtf16Offset;
  bool _previousWasCr = false;

  LazyValidationStatus get status {
    if (invalidUtf16Offset != null) return LazyValidationStatus.invalid;
    if (cursor == source.length) return LazyValidationStatus.valid;
    return LazyValidationStatus.pending;
  }

  bool get isComplete => status != LazyValidationStatus.pending;

  LazyEnrichmentReceipt enrich(int maxUtf16) {
    if (isComplete) {
      return LazyEnrichmentReceipt(
        backingId: 0,
        scannedUtf16: 0,
        status: status,
        cursorUtf16: cursor,
      );
    }
    final start = cursor;
    var limit = math.min(source.length, cursor + maxUtf16);
    if (limit < source.length &&
        limit > cursor &&
        _isHighSurrogate(source.codeUnitAt(limit - 1)) &&
        _isLowSurrogate(source.codeUnitAt(limit))) {
      limit += 1;
    }

    while (cursor < limit) {
      final unit = source.codeUnitAt(cursor);
      if (_isHighSurrogate(unit)) {
        if (cursor + 1 >= source.length ||
            !_isLowSurrogate(source.codeUnitAt(cursor + 1))) {
          invalidUtf16Offset = cursor;
          break;
        }
        final low = source.codeUnitAt(cursor + 1);
        final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
        utf8Length += 4;
        hash32 = _hashUtf8Scalar(hash32, scalar);
        _previousWasCr = false;
        cursor += 2;
        continue;
      }
      if (_isLowSurrogate(unit)) {
        invalidUtf16Offset = cursor;
        break;
      }

      utf8Length += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
      hash32 = _hashUtf8Scalar(hash32, unit);
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

    _checkpoints.add(_IndexSummary(cursor, utf8Length, lineBreaks, hash32));
    return LazyEnrichmentReceipt(
      backingId: 0,
      scannedUtf16: cursor - start,
      status: status,
      cursorUtf16: cursor,
    );
  }

  int? utf8At(int utf16Offset) => summaryAt(utf16Offset)?.utf8Length;

  _IndexSummary? summaryAt(int utf16Offset) {
    if (utf16Offset < 0 || utf16Offset > cursor) return null;
    if (invalidUtf16Offset != null && utf16Offset > invalidUtf16Offset!) {
      return null;
    }
    var low = 0;
    var high = _checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (_checkpoints[middle].utf16Offset <= utf16Offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    final checkpoint = _checkpoints[low - 1];
    if (checkpoint.utf16Offset == utf16Offset) return checkpoint;
    return _scanSummary(checkpoint, utf16Offset);
  }

  _IndexSummary _scanSummary(_IndexSummary base, int end) {
    var offset = base.utf16Offset;
    var bytes = base.utf8Length;
    var breaks = base.lineBreaks;
    var hash = base.hash32;
    var previousWasCr = offset > 0 && source.codeUnitAt(offset - 1) == 0x0D;
    while (offset < end) {
      final unit = source.codeUnitAt(offset);
      if (_isHighSurrogate(unit)) {
        final low = source.codeUnitAt(offset + 1);
        final scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
        bytes += 4;
        hash = _hashUtf8Scalar(hash, scalar);
        previousWasCr = false;
        offset += 2;
        continue;
      }
      bytes += unit <= 0x7F ? 1 : (unit <= 0x7FF ? 2 : 3);
      hash = _hashUtf8Scalar(hash, unit);
      if (unit == 0x0D) {
        breaks += 1;
        previousWasCr = true;
      } else if (unit == 0x0A) {
        if (!previousWasCr) breaks += 1;
        previousWasCr = false;
      } else {
        previousWasCr = false;
      }
      offset += 1;
    }
    return _IndexSummary(offset, bytes, breaks, hash);
  }
}

final class _IndexSummary {
  const _IndexSummary(
    this.utf16Offset,
    this.utf8Length,
    this.lineBreaks,
    this.hash32,
  );

  final int utf16Offset;
  final int utf8Length;
  final int lineBreaks;
  final int hash32;
}

const int _hashSeed = 0x811C9DC5;

int _hashUtf8Scalar(int hash, int scalar) {
  if (scalar <= 0x7F) return _hashByte(hash, scalar);
  if (scalar <= 0x7FF) {
    hash = _hashByte(hash, 0xC0 | (scalar >> 6));
    return _hashByte(hash, 0x80 | (scalar & 0x3F));
  }
  if (scalar <= 0xFFFF) {
    hash = _hashByte(hash, 0xE0 | (scalar >> 12));
    hash = _hashByte(hash, 0x80 | ((scalar >> 6) & 0x3F));
    return _hashByte(hash, 0x80 | (scalar & 0x3F));
  }
  hash = _hashByte(hash, 0xF0 | (scalar >> 18));
  hash = _hashByte(hash, 0x80 | ((scalar >> 12) & 0x3F));
  hash = _hashByte(hash, 0x80 | ((scalar >> 6) & 0x3F));
  return _hashByte(hash, 0x80 | (scalar & 0x3F));
}

int _hashByte(int hash, int byte) => ((hash ^ byte) * 0x01000193) & 0xFFFFFFFF;

/// Thin Option-B comparison: only the active island and an ordered intent
/// journal are synchronous on the main isolate.
final class WorkerCanonicalMainCache {
  WorkerCanonicalMainCache.open({
    required int utf16Length,
    required this.activeIslandStart,
    required String activeIsland,
    required this.baseReplayable,
  }) : _utf16Length = utf16Length,
       _activeIsland = activeIsland;

  int _utf16Length;
  int activeIslandStart;
  String _activeIsland;
  final bool baseReplayable;
  final List<_WorkerJournalEntry> _journal = [];

  int get utf16Length => _utf16Length;
  int get pendingTransactions => _journal.length;

  String? readRangeIfCached(int start, int end) {
    final localStart = start - activeIslandStart;
    final localEnd = end - activeIslandStart;
    if (localStart < 0 || localEnd > _activeIsland.length) return null;
    return _activeIsland.substring(localStart, localEnd);
  }

  void replaceCached(int start, int end, String replacement) {
    final localStart = start - activeIslandStart;
    final localEnd = end - activeIslandStart;
    if (localStart < 0 || localEnd > _activeIsland.length) {
      throw StateError('worker prefetch is required');
    }
    final before = _activeIsland;
    _activeIsland = _activeIsland.replaceRange(
      localStart,
      localEnd,
      replacement,
    );
    _utf16Length += replacement.length - (end - start);
    _journal.add(
      _WorkerJournalEntry(
        start: start,
        end: end,
        replacement: replacement,
        previousIsland: before,
      ),
    );
  }

  void undoPending() {
    final operation = _journal.removeLast();
    _utf16Length -=
        operation.replacement.length - (operation.end - operation.start);
    _activeIsland = operation.previousIsland;
  }

  WorkerRestartStatus restartWorker() => baseReplayable
      ? WorkerRestartStatus.replayBaseThenJournal
      : WorkerRestartStatus.blockedMissingBase;
}

enum WorkerRestartStatus { replayBaseThenJournal, blockedMissingBase }

final class _WorkerJournalEntry {
  const _WorkerJournalEntry({
    required this.start,
    required this.end,
    required this.replacement,
    required this.previousIsland,
  });

  final int start;
  final int end;
  final String replacement;
  final String previousIsland;
}

void _verifyModel() {
  final raw = LazyBulkSourceDocument.adoptBulk('a\r\nb\rc\n😀z');
  _expect(raw.utf16Length == 10, 'bulk adoption preserves raw UTF-16 extent');
  _expect(raw.lineCount.status == LazySummaryStatus.pending, 'lines pending');
  _expect(
    raw.contentHash32.status == LazySummaryStatus.pending,
    'hash pending',
  );
  _expect(raw.utf16ToUtf8IfIndexed(1) == null, 'bytes pending');

  var polls = 0;
  while (raw.certification.status == LazyValidationStatus.pending) {
    final receipt = raw.enrichNext(maxUtf16: 1);
    _expect(receipt.scannedUtf16 <= 2, 'surrogate is only +1 over poll bound');
    polls += 1;
  }
  _expect(raw.certification.status == LazyValidationStatus.valid, 'valid raw');
  _expect(raw.lineCount.value == 4, 'CRLF and lone CR are line breaks');
  _expect(
    raw.utf8Length.value == utf8.encode(raw.readRange(0, 10)).length,
    'utf8',
  );
  _expect(raw.utf16ToUtf8IfIndexed(7) == 7, 'byte prefix');
  _expect(polls >= 9, 'one-unit cooperative polls occurred');

  final malformed = LazyBulkSourceDocument.adoptBulk(
    String.fromCharCodes([0x61, 0xD800, 0x62]),
  );
  _expect(
    malformed.certification.status == LazyValidationStatus.pending,
    'malformed bulk is a provisional candidate',
  );
  malformed.enrichNext(maxUtf16: 16);
  _expect(
    malformed.certification.status == LazyValidationStatus.invalid &&
        malformed.certification.invalidUtf16Offset == 1,
    'malformed bulk fails with exact offset',
  );

  var invalidSmallRejected = false;
  try {
    raw.replaceRange(0, 0, String.fromCharCode(0xDC00));
  } on FormatException {
    invalidSmallRejected = true;
  }
  _expect(invalidSmallRejected, 'small invalid edit is rejected synchronously');

  final anchor = raw.anchorAt(4);
  const selection = LazyUtf16Selection(base: 4, extent: 8);
  final edited = raw.replaceRange(0, 0, 'x').document;
  _expect(
    edited.resolveAnchor(anchor) == 5,
    'piece anchor survives prefix edit',
  );
  final shiftedSelection = selection.transformForReplace(
    start: 0,
    end: 0,
    replacementLength: 1,
  );
  _expect(
    shiftedSelection.base == 5 && shiftedSelection.extent == 9,
    'selection transforms without scanning source',
  );
  _expect(
    edited.readRange(1, edited.utf16Length) ==
        raw.readRange(0, raw.utf16Length),
    'exact source survives edit',
  );

  final bulkPaste = raw.replaceRange(3, 3, _asciiMarkdownOfLength(32 * 1024));
  _expect(
    bulkPaste.document.certification.status == LazyValidationStatus.pending,
    'large paste remains an explicitly provisional candidate',
  );
  _expect(
    bulkPaste.workerIntent.replacementIsPendingBulk,
    'worker gets a UTF-16 bulk intent',
  );
  _expect(identical(bulkPaste.undo(), raw), 'undo is exact before enrichment');
  _expect(
    bulkPaste.document.readRange(1, 5) ==
        '${raw.readRange(1, 3)}'
            '${bulkPaste.workerIntent.replacement.substring(0, 2)}',
    'cross-piece range read is exact before enrichment',
  );

  final compactBase = LazyBulkSourceDocument.adoptBulk(
    _asciiMarkdownOfLength(2 * 1024 * 1024),
  );
  final compacted = compactBase
      .replaceRange(32, compactBase.utf16Length - 32, '')
      .document;
  _expect(compacted.utf16Length == 64, 'large deletion exact');
  _expect(
    compacted.diagnostics.retainedBackingUtf16 == 64,
    'current root no longer retains giant backing',
  );
  _expect(
    compactBase.diagnostics.retainedBackingUtf16 == 2 * 1024 * 1024,
    'undo snapshot intentionally retains old backing',
  );

  final cache = WorkerCanonicalMainCache.open(
    utf16Length: 1000000,
    activeIslandStart: 999000,
    activeIsland: _asciiMarkdownOfLength(1000),
    baseReplayable: false,
  );
  _expect(cache.readRangeIfCached(0, 4) == null, 'cold read is unavailable');
  cache.replaceCached(999999, 1000000, 'x');
  _expect(cache.pendingTransactions == 1, 'worker journal records intent');
  cache.undoPending();
  _expect(cache.pendingTransactions == 0, 'pending undo is local');
  _expect(
    cache.restartWorker() == WorkerRestartStatus.blockedMissingBase,
    'journal cannot reconstruct a missing worker base',
  );
}

void _runStringPrimitiveReceipts(String source, int iterations) {
  var string = source;
  final lengthSamples = _measure(
    iterations: iterations * 10,
    warmup: 100,
    body: () {
      _blackHole ^= string.length;
      if ((_blackHole & 0x3FFF) == 7) string = source;
    },
  );
  _emit('string_length', {
    'utf16': source.length,
    ...lengthSamples.json,
    'claim': 'native-host field access only; browser gate remains',
  });

  var seed = 0x13579BDF;
  final unitSamples = _measure(
    iterations: iterations,
    warmup: 100,
    body: () {
      seed = _next(seed);
      _blackHole ^= source.codeUnitAt(seed % source.length);
    },
  );
  _emit('string_random_code_unit', {
    'utf16': source.length,
    ...unitSamples.json,
  });

  final middle = source.length ~/ 2;
  final rangeSamples = _measure(
    iterations: iterations,
    warmup: 100,
    body: () {
      final copy = source.substring(middle, middle + 4096);
      _blackHole ^= copy.codeUnitAt(0) ^ copy.length;
    },
  );
  _emit('string_substring_4k', {'utf16': source.length, ...rangeSamples.json});
}

void _runOptionAReceipts(String source, int iterations) {
  LazyBulkSourceDocument? adopted;
  final adoptionSamples = _measure(
    iterations: iterations,
    warmup: 100,
    body: () {
      adopted = LazyBulkSourceDocument.adoptBulk(source);
      _blackHole ^= adopted!.utf16Length + adopted!.diagnostics.pieces;
    },
  );
  final document = adopted!;
  _emit('option_a_bulk_adoption', {
    'utf16': source.length,
    ...adoptionSamples.json,
    'utf8_status': document.utf8Length.status.name,
    'line_status': document.lineCount.status.name,
    'hash_status': document.contentHash32.status.name,
    'certification': document.certification.status.name,
    'rss_mib': _rssMiB(),
  });

  final suffix = document.readRange(
    document.utf16Length - 4096,
    document.utf16Length,
  );
  final suffixReadSamples = _measure(
    iterations: iterations,
    warmup: 100,
    body: () {
      final value = document.readRange(
        document.utf16Length - 4096,
        document.utf16Length,
      );
      _blackHole ^= value.length ^ value.codeUnitAt(0);
    },
  );
  _expect(suffix.length == 4096, 'suffix read exact');
  _emit('option_a_bounded_range_read', {
    'range_utf16': 4096,
    ...suffixReadSamples.json,
  });

  final backspaceSamples = _measure(
    iterations: iterations,
    warmup: 100,
    body: () {
      final result = document.replaceRange(
        document.utf16Length - 1,
        document.utf16Length,
        '',
      );
      _blackHole ^= result.document.utf16Length ^ result.undo().utf16Length;
    },
  );
  _emit('option_a_immediate_backspace_and_undo', {
    'utf16': source.length,
    ...backspaceSamples.json,
    'undo_before_enrichment': true,
  });

  final pasteBase = LazyBulkSourceDocument.adoptBulk('prefix\n');
  final pasteSamples = _measure(
    iterations: math.max(30, iterations ~/ 8),
    warmup: 10,
    body: () {
      final result = pasteBase.replaceRange(
        pasteBase.utf16Length,
        pasteBase.utf16Length,
        source,
      );
      _blackHole ^=
          result.document.utf16Length ^ result.workerIntent.replacement.length;
    },
  );
  final pasted = pasteBase
      .replaceRange(pasteBase.utf16Length, pasteBase.utf16Length, source)
      .document;
  final immediate = pasted.replaceRange(
    pasted.utf16Length - 1,
    pasted.utf16Length,
    '',
  );
  _expect(
    immediate.undo().utf16Length == pasted.utf16Length,
    'paste backspace undo exact',
  );
  _emit('option_a_bulk_paste', {
    'paste_utf16': source.length,
    ...pasteSamples.json,
    'post_paste_certification': pasted.certification.status.name,
    'immediate_backspace_exact': true,
  });

  final deletionStart = math.min(4 * 1024, document.utf16Length ~/ 4);
  final deletionEnd = document.utf16Length - deletionStart;
  final deletion = Stopwatch()..start();
  final compacted = document
      .replaceRange(deletionStart, deletionEnd, '')
      .document;
  deletion.stop();
  _emit('option_a_large_deletion_compaction', {
    'before_retained_utf16': document.diagnostics.retainedBackingUtf16,
    'after_live_utf16': compacted.utf16Length,
    'after_retained_utf16': compacted.diagnostics.retainedBackingUtf16,
    'elapsed_us': _microseconds(deletion),
    'sync_copy_bound_utf16': 8 * 1024,
    'old_snapshot_retains_for_undo': true,
  });

  final enrichDocument = LazyBulkSourceDocument.adoptBulk(source);
  final samples = <int>[];
  var scanned = 0;
  while (enrichDocument.certification.status == LazyValidationStatus.pending) {
    final stopwatch = Stopwatch()..start();
    final receipt = enrichDocument.enrichNext(maxUtf16: 8 * 1024);
    stopwatch.stop();
    samples.add(_nanoseconds(stopwatch));
    scanned += receipt.scannedUtf16;
  }
  final enrichment = _Samples(samples);
  _expect(scanned == source.length, 'enrichment scans each code unit once');
  _emit('option_a_cooperative_enrichment', {
    'utf16': source.length,
    'slice_limit_utf16': 8 * 1024,
    'polls': samples.length,
    ...enrichment.json,
    'total_ms': samples.fold<int>(0, (sum, value) => sum + value) / 1000000,
    'certification': enrichDocument.certification.status.name,
    'utf8_length': enrichDocument.utf8Length.value,
    'line_count': enrichDocument.lineCount.value,
    'hash_status': enrichDocument.contentHash32.status.name,
  });
}

void _runOptionBComparison(String source) {
  const islandSize = 4096;
  final cache = WorkerCanonicalMainCache.open(
    utf16Length: source.length,
    activeIslandStart: source.length - islandSize,
    activeIsland: String.fromCharCodes(
      source.codeUnits,
      source.length - islandSize,
      source.length,
    ),
    baseReplayable: false,
  );
  final coldRead = cache.readRangeIfCached(0, 128);
  cache.replaceCached(source.length - 1, source.length, 'x');
  final pendingAfterEdit = cache.pendingTransactions;
  cache.undoPending();
  _emit('option_b_main_cache_comparison', {
    'retained_source_utf16': islandSize,
    'global_utf16_length_exact': cache.utf16Length,
    'cold_range_read_available': coldRead != null,
    'pending_after_local_edit': pendingAfterEdit,
    'local_undo_available': cache.pendingTransactions == 0,
    'restart_status': cache.restartWorker().name,
    'requires_async_copy_outside_island': true,
  });
}

final class _Samples {
  _Samples(List<int> values) : _values = [...values]..sort() {
    if (_values.isEmpty) throw ArgumentError.value(values, 'values');
  }

  final List<int> _values;

  int get p50 => _values[(_values.length - 1) ~/ 2];
  int get p99 => _values[((_values.length - 1) * 99) ~/ 100];
  int get max => _values.last;

  Map<String, Object> get json => {
    'samples': _values.length,
    'p50_us': p50 / 1000,
    'p99_us': p99 / 1000,
    'max_us': max / 1000,
  };
}

_Samples _measure({
  required int iterations,
  required int warmup,
  required void Function() body,
}) {
  for (var index = 0; index < warmup; index += 1) {
    body();
  }
  final values = <int>[];
  for (var index = 0; index < iterations; index += 1) {
    final stopwatch = Stopwatch()..start();
    body();
    stopwatch.stop();
    values.add(_nanoseconds(stopwatch));
  }
  return _Samples(values);
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
      _values[name] == null ? fallback : int.parse(_values[name]!);
}

var _blackHole = 0;
final int _stopwatchFrequency = Stopwatch().frequency;

String _asciiMarkdownOfLength(int length) {
  const line =
      'Paragraph with **bold**, *emphasis*, `code`, [link][target], and text.\n';
  final chunk = StringBuffer();
  while (chunk.length < 64 * 1024) {
    chunk.write(line);
  }
  final chunkText = chunk.toString();
  final fullChunks = length ~/ chunkText.length;
  final remainder = length % chunkText.length;
  return '${List<String>.filled(fullChunks, chunkText).join()}'
      '${chunkText.substring(0, remainder)}';
}

bool _isHighSurrogate(int value) => value >= 0xD800 && value <= 0xDBFF;

bool _isLowSurrogate(int value) => value >= 0xDC00 && value <= 0xDFFF;

int _next(int value) => (value * 1664525 + 1013904223) & 0x7FFFFFFF;

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
