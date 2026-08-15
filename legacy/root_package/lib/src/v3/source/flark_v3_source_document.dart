import 'dart:collection';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:characters/characters.dart';

const int _maximumSourceChunkUtf16 = 8192;
const int _maximumGraphemeContextUtf16 = 8192;
const int _maximumSynchronousSourceOperations = 256;
const int _minimumPendingDiscoveryNodes = 64;
const int _maximumSynchronousCertificationAttachments = 64;
const int _maximumOneShotCertificationUtf16 = 8192;
const int _sourceFactCheckpointWireBytes = 32;
const int _sourceFactPageBaseWireBytes = 96;
const int _sourceFactCompletionWireBytes = 128;
const int _maximumWorkerSyncPageEntries = 64;
const int _maximumSourceFactScanNodes = 1024;
const int _maximumSourceFactAdoptionPathNodes = 4096;
const int _maximumSourceFactWireBytes =
    _sourceFactPageBaseWireBytes +
    _maximumWorkerSyncPageEntries * _sourceFactCheckpointWireBytes;
const int _maximumWorkerSyncPageOperations = 1024;
const int _maximumWorkerSyncPagePayloadUtf16 = 8192;

void _validateSourceChunkSize(int chunkSize) {
  if (chunkSize < 2 || chunkSize > _maximumSourceChunkUtf16) {
    throw RangeError.range(chunkSize, 2, _maximumSourceChunkUtf16, 'chunkSize');
  }
}

/// Internal canonical source substrate for the v3 engine.
///
/// This implementation is never blanket-exported by a normal package barrel.
/// Only the small source-edit value types at the bottom of the file are
/// explicitly selected by `flark_v3.dart`; the document, certification, and
/// worker-replica APIs remain an internal/adapter contract.
final class FlarkV3SourceDocument {
  const FlarkV3SourceDocument._({
    required _SourceNode? root,
    required this.revision,
    required int chunkSize,
    required int nextPieceId,
    _CanonicalSourceFacts? canonicalFacts,
  }) : _root = root,
       _chunkSize = chunkSize,
       _nextPieceId = nextPieceId,
       _canonicalFacts = canonicalFacts;

  factory FlarkV3SourceDocument.fromString(
    String source, {
    int chunkSize = 4096,
  }) {
    _validateSourceChunkSize(chunkSize);
    _validateScalarString(source, 'source');
    return FlarkV3SourceDocument._(
      root: _treeFromString(source, chunkSize),
      revision: 0,
      chunkSize: chunkSize,
      nextPieceId: 1,
    );
  }

  /// Adopts exact UTF-16 source without scanning, normalizing, or encoding it.
  ///
  /// The returned document is deliberately provisional: bounded source reads
  /// and edits are available immediately, while UTF-8, line, hash, and scalar
  /// validity facts remain unavailable until a typed certification receipt is
  /// attached. Product code should normally use
  /// [FlarkV3SourceSession.fromProvisionalString], which also owns the worker
  /// seed and stale-reply barrier.
  factory FlarkV3SourceDocument.fromProvisionalString(
    String source, {
    int chunkSize = 4096,
  }) {
    _validateSourceChunkSize(chunkSize);
    return FlarkV3SourceDocument._(
      root: source.isEmpty
          ? null
          : _SourceLeaf.provisional(
              source: source,
              start: 0,
              utf16Length: source.length,
              pieceId: 1,
            ),
      revision: source.isEmpty ? 0 : 1,
      chunkSize: chunkSize,
      nextPieceId: 2,
    );
  }

  final _SourceNode? _root;
  final int _chunkSize;
  final int _nextPieceId;
  final _CanonicalSourceFacts? _canonicalFacts;

  final int revision;

  int get utf16Length => _root?.utf16Length ?? 0;

  /// Whether every live source piece has certified derived facts.
  ///
  /// This is the legacy piece-local index predicate. Production native/Wasm
  /// certification installs one canonical global overlay instead; callers that
  /// need source authority use [hasCertifiedFacts].
  bool get isFullyIndexed => _root?.isCertified ?? true;

  /// Whether this exact revision owns complete certified source facts.
  bool get hasCertifiedFacts => _canonicalFacts != null || isFullyIndexed;

  /// O(1) identity guard for the exact source target represented by this root.
  ///
  /// A known stamp carries aggregates already owned by the persistent source
  /// tree. A provisional stamp deliberately carries only facts the Dart source
  /// authority can prove without scanning. Neither form substitutes hash
  /// equality for exact source authority.
  FlarkV3SourceStamp get sourceStamp => hasCertifiedFacts
      ? FlarkV3KnownSourceStamp(
          revision: revision,
          utf16Length: utf16Length,
          utf8Length:
              _canonicalFacts?.fingerprint.utf8Length ?? _root?.utf8Length ?? 0,
          contentHash128:
              _canonicalFacts?.fingerprint.contentHash128 ??
              _root?.contentHash128 ??
              FlarkV3ContentHash128.zero,
        )
      : FlarkV3ProvisionalSourceStamp(
          revision: revision,
          utf16Length: utf16Length,
        );

  int get utf8Length {
    _requireCertifiedFacts();
    return _canonicalFacts?.fingerprint.utf8Length ?? _root?.utf8Length ?? 0;
  }

  int get lineCount {
    _requireCertifiedFacts();
    return (_canonicalFacts?.logicalLineBreaks ?? _root?.newlines ?? 0) + 1;
  }

  FlarkV3ContentHash128 get contentHash128 {
    _requireCertifiedFacts();
    return _canonicalFacts?.fingerprint.contentHash128 ??
        _root?.contentHash128 ??
        FlarkV3ContentHash128.zero;
  }

  /// Compatibility view of the first 32-bit lane.
  int get contentHash32 => contentHash128.word0;

  FlarkV3SourceFingerprint get fingerprint => FlarkV3SourceFingerprint(
    revision: revision,
    utf16Length: utf16Length,
    utf8Length: utf8Length,
    contentHash128: contentHash128,
  );

  /// Cold whole-document materialization for export and differential oracles.
  ///
  /// Engine hot paths must use [readRange].
  @override
  String toString() => readRange(0, utf16Length);

  String readRange(int startUtf16, int endUtf16) {
    _checkUtf16Range(startUtf16, endUtf16);
    if (startUtf16 == endUtf16) return '';
    final output = StringBuffer();
    _writeRange(_root, startUtf16, endUtf16, output);
    return output.toString();
  }

  int utf16ToUtf8(int utf16Offset) {
    _requireCertifiedFacts();
    _checkUtf16Offset(utf16Offset);
    _checkScalarBoundary(utf16Offset);
    final canonical = _canonicalFacts;
    if (canonical != null) {
      return canonical.prefixAtUtf16(this, utf16Offset).utf8Offset;
    }
    return _utf16ToUtf8(_root, utf16Offset);
  }

  int utf8ToUtf16(int utf8Offset) {
    _requireCertifiedFacts();
    if (utf8Offset < 0 || utf8Offset > utf8Length) {
      throw RangeError.range(utf8Offset, 0, utf8Length, 'utf8Offset');
    }
    final canonical = _canonicalFacts;
    if (canonical != null) {
      return canonical.utf16AtUtf8(this, utf8Offset);
    }
    return _utf8ToUtf16(_root, utf8Offset);
  }

  int lineAtUtf16(int utf16Offset) {
    _requireCertifiedFacts();
    _checkUtf16Offset(utf16Offset);
    final canonical = _canonicalFacts;
    if (canonical != null) {
      return canonical.prefixAtUtf16(this, utf16Offset).newlines;
    }
    return _newlinesBefore(_root, utf16Offset);
  }

  int lineStartUtf16(int lineIndex) {
    _requireCertifiedFacts();
    if (lineIndex < 0 || lineIndex >= lineCount) {
      throw RangeError.range(lineIndex, 0, lineCount - 1, 'lineIndex');
    }
    if (lineIndex == 0) return 0;
    final canonical = _canonicalFacts;
    if (canonical != null) {
      return canonical.offsetAfterNthNewline(this, lineIndex);
    }
    return _offsetAfterNthNewline(_root!, lineIndex);
  }

  /// Finds the extended grapheme immediately before [caretUtf16] when its
  /// complete line prefix fits inside [maxContextUtf16].
  ///
  /// A line boundary is a conservative Unicode segmentation restart. If that
  /// boundary cannot be reached within the budget, the method returns
  /// [FlarkV3GraphemeLookup.needsMoreContext] rather than guessing. Oversized
  /// lines/clusters can then use a worker or exact-source fallback.
  FlarkV3GraphemeLookup graphemeBefore(
    int caretUtf16, {
    int maxContextUtf16 = 4096,
  }) {
    _checkUtf16Offset(caretUtf16);
    _checkScalarBoundary(caretUtf16);
    if (maxContextUtf16 < 1) {
      throw RangeError.range(maxContextUtf16, 1, null, 'maxContextUtf16');
    }
    if (maxContextUtf16 > _maximumGraphemeContextUtf16) {
      throw RangeError.range(
        maxContextUtf16,
        1,
        _maximumGraphemeContextUtf16,
        'maxContextUtf16',
      );
    }
    if (caretUtf16 == 0) {
      return const FlarkV3GraphemeLookup.certified(0, 0);
    }
    final inspectedStart = math.max(0, caretUtf16 - maxContextUtf16);
    final inspected = readRange(inspectedStart, caretUtf16);
    int? lineStart;
    for (var local = inspected.length - 1; local >= 0; local -= 1) {
      final codeUnit = inspected.codeUnitAt(local);
      final absolute = inspectedStart + local;
      if (codeUnit == 0x0A) {
        lineStart = absolute + 1;
        break;
      }
      if (codeUnit != 0x0D) continue;
      final followedByLineFeed =
          absolute + 1 < utf16Length &&
          (absolute + 1 < caretUtf16
              ? inspected.codeUnitAt(local + 1) == 0x0A
              : _codeUnitAt(_root!, absolute + 1) == 0x0A);
      if (followedByLineFeed && absolute + 1 == caretUtf16) {
        // The caret splits one CRLF grapheme/logical break. Do not guess.
        return FlarkV3GraphemeLookup.needsMoreContext(
          requiredStartUtf16: absolute,
          inspectedStartUtf16: inspectedStart,
          caretUtf16: caretUtf16,
        );
      }
      lineStart = absolute + (followedByLineFeed ? 2 : 1);
      break;
    }
    if (lineStart == null && inspectedStart > 0) {
      return FlarkV3GraphemeLookup.needsMoreContext(
        requiredStartUtf16: 0,
        inspectedStartUtf16: inspectedStart,
        caretUtf16: caretUtf16,
      );
    }
    lineStart ??= 0;
    final prefix = lineStart == inspectedStart
        ? inspected
        : readRange(lineStart, caretUtf16);
    try {
      _validateScalarString(prefix, 'grapheme context');
    } on FormatException {
      return FlarkV3GraphemeLookup.needsMoreContext(
        requiredStartUtf16: lineStart,
        inspectedStartUtf16: lineStart,
        caretUtf16: caretUtf16,
      );
    }
    final range = CharacterRange.at(prefix, prefix.length);
    if (!range.moveBack()) {
      return FlarkV3GraphemeLookup.certified(caretUtf16, caretUtf16);
    }
    return FlarkV3GraphemeLookup.certified(
      caretUtf16 - range.current.length,
      caretUtf16,
    );
  }

  FlarkV3AppliedSourceTransaction apply(FlarkV3SourceTransaction transaction) {
    return _apply(transaction, provisionalRoute: !isFullyIndexed);
  }

  FlarkV3AppliedSourceTransaction _apply(
    FlarkV3SourceTransaction transaction, {
    required bool provisionalRoute,
  }) {
    if (transaction.baseRevision != revision) {
      throw FlarkV3RevisionMismatch(
        expected: revision,
        actual: transaction.baseRevision,
      );
    }

    final sorted = _validatedOperations(
      transaction.operations,
      validateReplacements: !provisionalRoute,
    );
    final work = _MutableSourceWorkReceipt();
    if (sorted.isEmpty || (!provisionalRoute && _isNoOp(sorted, work))) {
      return FlarkV3AppliedSourceTransaction.noOp(
        document: this,
        sourceWork: work.seal(),
      );
    }

    final before = isFullyIndexed && !provisionalRoute ? fingerprint : null;
    final parserOperations = <FlarkV3ParserEdit>[];
    final preparedOperations = <_PreparedSourceEdit>[];
    var nextPieceId = _nextPieceId;
    for (final indexed in sorted) {
      final operation = indexed.operation;
      late final _SourceNode? replacementRoot;
      if (provisionalRoute) {
        replacementRoot = operation.replacement.isEmpty
            ? null
            : _SourceLeaf.provisional(
                source: operation.replacement,
                start: 0,
                utf16Length: operation.replacement.length,
                pieceId: nextPieceId++,
              );
      } else {
        final prepared = _prepareSource(
          operation.replacement,
          _chunkSize,
          collectUtf8: true,
        );
        replacementRoot = prepared.root;
        work.replacementUtf8BytesEncoded += prepared.utf8.length;
        work.replacementChunksEncoded += prepared.encodedChunks;
        parserOperations.add(
          FlarkV3ParserEdit(
            startUtf8: utf16ToUtf8(operation.startUtf16),
            endUtf8: utf16ToUtf8(operation.endUtf16),
            replacementUtf8: prepared.utf8,
          ),
        );
      }
      preparedOperations.add(
        _PreparedSourceEdit(indexed: indexed, replacementRoot: replacementRoot),
      );
    }

    var nextRoot = _root;
    int allocateProvisionalPieceId() => nextPieceId++;
    final compaction = _CompactionAccumulator(
      sourceRevision: revision + 1,
      chunkSize: _chunkSize,
    );
    // Operations use coordinates from the original revision. Applying from the
    // end preserves those coordinates. Stable reverse order also preserves the
    // v2 contract for multiple insertions at the same offset.
    for (final prepared in preparedOperations.reversed) {
      final indexed = prepared.indexed;
      final operation = indexed.operation;
      final first = _split(nextRoot, operation.startUtf16, _chunkSize);
      final second = _split(
        first.right,
        operation.endUtf16 - operation.startUtf16,
        _chunkSize,
      );
      compaction.consider(_rightmostLeaf(first.left));
      compaction.consider(_leftmostLeaf(second.right));
      nextRoot = _concat(
        _concat(
          first.left,
          prepared.replacementRoot,
          _chunkSize,
          allocateProvisionalPieceId: allocateProvisionalPieceId,
        ),
        second.right,
        _chunkSize,
        allocateProvisionalPieceId: allocateProvisionalPieceId,
      );
    }

    final next = FlarkV3SourceDocument._(
      root: nextRoot,
      revision: revision + 1,
      chunkSize: _chunkSize,
      nextPieceId: nextPieceId,
    );
    if (before == null) {
      return FlarkV3AppliedSourceTransaction.provisional(
        document: next,
        sourceWork: work.seal(),
        compactionObligations: compaction.seal(),
      );
    }
    return FlarkV3AppliedSourceTransaction(
      document: next,
      parserBatch: FlarkV3ParserEditBatch(
        baseRevision: revision,
        revision: next.revision,
        beforeHash128: before.contentHash128,
        afterHash128: next.contentHash128,
        beforeUtf8Length: before.utf8Length,
        afterUtf8Length: next.utf8Length,
        operations: parserOperations,
      ),
      sourceWork: work.seal(),
      compactionObligations: compaction.seal(),
    );
  }

  FlarkV3SourcePendingPiecePage _pendingPiecePage({
    required int cursorUtf16,
    required int maximumPieces,
    required int maximumNodes,
  }) {
    if (cursorUtf16 < 0 || cursorUtf16 > utf16Length) {
      throw RangeError.range(cursorUtf16, 0, utf16Length, 'cursorUtf16');
    }
    if (maximumPieces < 1) {
      throw RangeError.range(maximumPieces, 1, null, 'maximumPieces');
    }
    final minimumNodes = math.max(
      _minimumPendingDiscoveryNodes,
      (_root?.height ?? 0) + 1,
    );
    if (maximumNodes < minimumNodes) {
      throw RangeError.range(maximumNodes, minimumNodes, null, 'maximumNodes');
    }
    final pieces = <FlarkV3SourcePieceToCertify>[];
    var nodesVisited = 0;
    int? nextCursor;

    void visit(_SourceNode? node, int globalStart) {
      if (node == null || nextCursor != null || node.isCertified) return;
      final globalEnd = globalStart + node.utf16Length;
      if (globalEnd <= cursorUtf16) return;
      if (nodesVisited >= maximumNodes) {
        nextCursor = math.max(cursorUtf16, globalStart);
        return;
      }
      nodesVisited += 1;
      if (node case final _SourceLeaf leaf) {
        if (pieces.length >= maximumPieces) {
          nextCursor = globalStart;
          return;
        }
        pieces.add(
          FlarkV3SourcePieceToCertify._(
            pieceId: leaf.pieceId!,
            sourceStartUtf16: leaf.start,
            utf16Length: leaf.utf16Length,
            globalStartUtf16: globalStart,
          ),
        );
        if (pieces.length >= maximumPieces && globalEnd < utf16Length) {
          nextCursor = globalEnd;
        }
        return;
      }
      final branch = node as _SourceBranch;
      visit(branch.left, globalStart);
      visit(branch.right, globalStart + branch.left.utf16Length);
    }

    visit(_root, 0);
    return FlarkV3SourcePendingPiecePage._(
      pieces: pieces,
      nextCursorUtf16: nextCursor,
      nodesVisited: nodesVisited,
    );
  }

  List<FlarkV3SourcePieceToCertify> _allPiecesToCertify({
    required int maximumPieces,
  }) {
    final pieces = <FlarkV3SourcePieceToCertify>[];
    void visit(_SourceNode? node, int globalStart) {
      if (node == null || node.isCertified || pieces.length >= maximumPieces) {
        return;
      }
      if (node case final _SourceLeaf leaf) {
        pieces.add(
          FlarkV3SourcePieceToCertify._(
            pieceId: leaf.pieceId!,
            sourceStartUtf16: leaf.start,
            utf16Length: leaf.utf16Length,
            globalStartUtf16: globalStart,
          ),
        );
        return;
      }
      final branch = node as _SourceBranch;
      visit(branch.left, globalStart);
      visit(branch.right, globalStart + branch.left.utf16Length);
    }

    visit(_root, 0);
    return pieces;
  }

  _AttachedSourceCertification _attachCertification(
    List<FlarkV3CertifiedSourcePiece> certifications,
  ) {
    var next = this;
    var pathNodesVisited = 0;
    final livePieces = _allPiecesToCertify(
      maximumPieces: _maximumSynchronousCertificationAttachments + 1,
    );
    if (livePieces.length > _maximumSynchronousCertificationAttachments) {
      throw FlarkV3SourceStagedCertificationRequired(
        pieceCount: livePieces.length,
        maximumSynchronousAttachments:
            _maximumSynchronousCertificationAttachments,
      );
    }
    if (livePieces.length != certifications.length) {
      throw StateError(
        'Certification covered ${certifications.length} pending pieces; '
        '${livePieces.length} are live.',
      );
    }
    final byKey = <_SourcePieceKey, FlarkV3CertifiedSourcePiece>{
      for (final certification in certifications)
        certification._key: certification,
    };
    if (byKey.length != certifications.length) {
      throw StateError('Certification contains duplicate source pieces.');
    }
    for (final piece in livePieces) {
      final certification = byKey[piece._key];
      if (certification == null) {
        throw StateError(
          'Certification does not match the live source pieces.',
        );
      }
      final attached = next._attachCertificationPiece(piece, certification);
      next = attached.document;
      pathNodesVisited += attached.pathNodesVisited;
    }
    if (!next.isFullyIndexed) {
      throw StateError('Certification left provisional source pieces live.');
    }
    return _AttachedSourceCertification(
      document: next,
      pathNodesVisited: pathNodesVisited,
      piecesAttached: certifications.length,
    );
  }

  /// Attaches exactly one completed piece to a non-authoritative document.
  ///
  /// The persistent update visits one source-tree path. Callers own whether
  /// this returned root is a hidden candidate or the authoritative document.
  _AttachedSourceCertification _attachCertificationPiece(
    FlarkV3SourcePieceToCertify piece,
    FlarkV3CertifiedSourcePiece certification,
  ) {
    if (piece._key != certification._key) {
      throw StateError('Certification addressed a different source piece.');
    }
    final replaced = _replacePendingLeafAt(
      _root,
      piece.globalStartUtf16,
      piece,
      certification,
    );
    return _AttachedSourceCertification(
      document: FlarkV3SourceDocument._(
        root: replaced.node,
        revision: revision,
        chunkSize: _chunkSize,
        nextPieceId: _nextPieceId,
      ),
      pathNodesVisited: replaced.pathNodesVisited,
      piecesAttached: 1,
    );
  }

  FlarkV3SourceTreeDiagnostics get diagnostics {
    var leaves = 0;
    var largestLeaf = 0;
    final backingIdentities = <int>{};
    void visit(_SourceNode? node) {
      if (node == null) return;
      if (node case final _SourceLeaf leaf) {
        leaves += 1;
        backingIdentities.add(leaf.backing.identity);
        largestLeaf = math.max(largestLeaf, leaf.utf16Length);
        return;
      }
      final branch = node as _SourceBranch;
      visit(branch.left);
      visit(branch.right);
    }

    visit(_root);
    return FlarkV3SourceTreeDiagnostics(
      leafCount: leaves,
      largestLeafUtf16: largestLeaf,
      treeHeight: _root?.height ?? 0,
      uniqueBackingCount: backingIdentities.length,
    );
  }

  List<_IndexedSourceEdit> _validatedOperations(
    List<FlarkV3SourceEdit> operations, {
    required bool validateReplacements,
  }) {
    if (operations.length > _maximumSynchronousSourceOperations) {
      throw FlarkV3SourceBulkOperationRequired(
        operationCount: operations.length,
        maximumSynchronousOperations: _maximumSynchronousSourceOperations,
      );
    }
    final indexed = <_IndexedSourceEdit>[
      for (var index = 0; index < operations.length; index += 1)
        _IndexedSourceEdit(index, operations[index]),
    ]..sort(_compareIndexedOperations);

    var previousEnd = 0;
    for (final entry in indexed) {
      final operation = entry.operation;
      _checkUtf16Range(operation.startUtf16, operation.endUtf16);
      _checkScalarBoundary(operation.startUtf16);
      _checkScalarBoundary(operation.endUtf16);
      if (validateReplacements) {
        _validateScalarString(operation.replacement, 'replacement');
      }
      if (operation.startUtf16 < previousEnd) {
        throw StateError(
          'V3 source transactions cannot contain overlapping operations.',
        );
      }
      previousEnd = operation.endUtf16;
    }
    return indexed;
  }

  bool _isNoOp(
    List<_IndexedSourceEdit> operations,
    _MutableSourceWorkReceipt work,
  ) {
    for (final entry in operations) {
      final operation = entry.operation;
      if (operation.startUtf16 == operation.endUtf16) {
        if (operation.replacement.isNotEmpty) return false;
        continue;
      }
      if (operation.endUtf16 - operation.startUtf16 !=
          operation.replacement.length) {
        return false;
      }
      if (!_rangeEqualsString(
        _root,
        operation.startUtf16,
        operation.endUtf16,
        operation.replacement,
        work,
      )) {
        return false;
      }
    }
    return true;
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

  void _requireCertifiedFacts() {
    if (!hasCertifiedFacts) {
      throw const FlarkV3SourceFactsPending();
    }
  }
}

final class FlarkV3SourceTransaction {
  FlarkV3SourceTransaction({
    required this.baseRevision,
    required List<FlarkV3SourceEdit> operations,
  }) : operations = _boundedSourceOperations(operations);

  factory FlarkV3SourceTransaction.single({
    required int baseRevision,
    required FlarkV3SourceEdit operation,
  }) => FlarkV3SourceTransaction(
    baseRevision: baseRevision,
    operations: [operation],
  );

  final int baseRevision;
  final List<FlarkV3SourceEdit> operations;
}

List<FlarkV3SourceEdit> _boundedSourceOperations(
  List<FlarkV3SourceEdit> operations,
) {
  if (operations.length > _maximumSynchronousSourceOperations) {
    throw FlarkV3SourceBulkOperationRequired(
      operationCount: operations.length,
      maximumSynchronousOperations: _maximumSynchronousSourceOperations,
    );
  }
  return List.unmodifiable(operations);
}

final class FlarkV3SourceEdit {
  const FlarkV3SourceEdit({
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
  });

  final int startUtf16;
  final int endUtf16;
  final String replacement;
}

abstract interface class FlarkV3SourcePayload {
  int get utf16Length;

  String readRange(int startUtf16, int endUtf16);
}

final class FlarkV3StringSourcePayload implements FlarkV3SourcePayload {
  const FlarkV3StringSourcePayload(this.source);

  final String source;

  @override
  int get utf16Length => source.length;

  @override
  String readRange(int startUtf16, int endUtf16) =>
      source.substring(startUtf16, endUtf16);
}

final class FlarkV3SourceIntentEdit {
  const FlarkV3SourceIntentEdit({
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
  });

  final int startUtf16;
  final int endUtf16;
  final FlarkV3SourcePayload replacement;
}

final class FlarkV3SourceIntent {
  FlarkV3SourceIntent({
    required this.workerGeneration,
    required this.sequence,
    required this.baseUiRevision,
    required this.uiRevision,
    required this.baseStamp,
    required this.targetStamp,
    required List<FlarkV3SourceIntentEdit> operations,
  }) : operations = List.unmodifiable(operations);

  /// Epoch of the replica stream that owns this intent.
  final int workerGeneration;

  /// Globally monotonic sequence. It never resets across worker generations.
  final int sequence;
  final int baseUiRevision;
  final int uiRevision;
  final FlarkV3SourceStamp baseStamp;
  final FlarkV3SourceStamp targetStamp;
  final List<FlarkV3SourceIntentEdit> operations;

  int get payloadUtf16 => operations.fold<int>(
    0,
    (total, operation) => total + operation.replacement.utf16Length,
  );

  int get deletedUtf16 => operations.fold<int>(
    0,
    (total, operation) => total + operation.endUtf16 - operation.startUtf16,
  );

  /// Conservative retained heap charge for Dart UTF-16 payload storage.
  int get retainedPayloadBytesUpperBound => payloadUtf16 * 2;
}

final class FlarkV3SourcePieceToCertify {
  const FlarkV3SourcePieceToCertify._({
    required this.pieceId,
    required this.sourceStartUtf16,
    required this.utf16Length,
    required this.globalStartUtf16,
  });

  final int pieceId;
  final int sourceStartUtf16;
  final int utf16Length;
  final int globalStartUtf16;

  _SourcePieceKey get _key =>
      _SourcePieceKey(pieceId, sourceStartUtf16, utf16Length);
}

final class FlarkV3SourcePendingPiecePage {
  FlarkV3SourcePendingPiecePage._({
    required List<FlarkV3SourcePieceToCertify> pieces,
    required this.nextCursorUtf16,
    required this.nodesVisited,
  }) : pieces = List.unmodifiable(pieces);

  final List<FlarkV3SourcePieceToCertify> pieces;
  final int? nextCursorUtf16;
  final int nodesVisited;

  bool get hasMore => nextCursorUtf16 != null;
}

final class FlarkV3SourceIntentPage {
  FlarkV3SourceIntentPage._({
    required List<FlarkV3SourceIntent> intents,
    required this.nextIntentIndex,
  }) : intents = List.unmodifiable(intents);

  final List<FlarkV3SourceIntent> intents;
  final int? nextIntentIndex;

  bool get hasMore => nextIntentIndex != null;
}

enum FlarkV3SourceWorkerSyncKind { snapshot, intents }

/// One bounded, credited unit of source-replica work.
///
/// A lease is immutable. The UI may continue editing while it is live. Only
/// one lease exists per session, and only its exact typed acknowledgement can
/// advance replica credit.
sealed class FlarkV3SourceWorkerSyncLease {
  const FlarkV3SourceWorkerSyncLease({
    required this.sourceSessionIdentity,
    required this.leaseId,
    required this.workerGeneration,
  });

  final int sourceSessionIdentity;
  final int leaseId;
  final int workerGeneration;
  FlarkV3SourceWorkerSyncKind get kind;
}

final class FlarkV3SourceSnapshotSyncLease
    extends FlarkV3SourceWorkerSyncLease {
  FlarkV3SourceSnapshotSyncLease._({
    required super.sourceSessionIdentity,
    required super.leaseId,
    required super.workerGeneration,
    required this.baseUiRevision,
    required this.startUtf16,
    required this.endUtf16,
    required this.totalUtf16Length,
    required this.throughIntentSequence,
    required this.targetStamp,
    required this.source,
  });

  @override
  FlarkV3SourceWorkerSyncKind get kind => FlarkV3SourceWorkerSyncKind.snapshot;

  final int baseUiRevision;
  final int startUtf16;
  final int endUtf16;
  final int totalUtf16Length;
  final int throughIntentSequence;
  final FlarkV3SourceStamp targetStamp;
  final String source;

  bool get isLast => endUtf16 == totalUtf16Length;

  FlarkV3SourceSnapshotSyncAcknowledgement acknowledgement({
    required FlarkV3ObservedSourceReplicaVersion? observedReplica,
  }) => FlarkV3SourceSnapshotSyncAcknowledgement(
    sourceSessionIdentity: sourceSessionIdentity,
    leaseId: leaseId,
    workerGeneration: workerGeneration,
    baseUiRevision: baseUiRevision,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    throughIntentSequence: throughIntentSequence,
    observedReplica: observedReplica,
  );
}

final class FlarkV3SourceIntentSyncLease extends FlarkV3SourceWorkerSyncLease {
  FlarkV3SourceIntentSyncLease._({
    required super.sourceSessionIdentity,
    required super.leaseId,
    required super.workerGeneration,
    required List<FlarkV3SourceIntent> intents,
    required this.payloadUtf16,
  }) : intents = List.unmodifiable(intents);

  @override
  FlarkV3SourceWorkerSyncKind get kind => FlarkV3SourceWorkerSyncKind.intents;

  final List<FlarkV3SourceIntent> intents;
  final int payloadUtf16;

  int get firstSequence => intents.first.sequence;
  int get lastSequence => intents.last.sequence;
  FlarkV3SourceStamp get baseStamp => intents.first.baseStamp;
  FlarkV3SourceStamp get targetStamp => intents.last.targetStamp;

  FlarkV3SourceIntentSyncAcknowledgement acknowledgement({
    required FlarkV3ObservedSourceReplicaVersion observedReplica,
  }) => FlarkV3SourceIntentSyncAcknowledgement(
    sourceSessionIdentity: sourceSessionIdentity,
    leaseId: leaseId,
    workerGeneration: workerGeneration,
    firstSequence: firstSequence,
    lastSequence: lastSequence,
    entryCount: intents.length,
    payloadUtf16: payloadUtf16,
    observedReplica: observedReplica,
  );
}

sealed class FlarkV3SourceWorkerSyncAcknowledgement {
  const FlarkV3SourceWorkerSyncAcknowledgement({
    required this.sourceSessionIdentity,
    required this.leaseId,
    required this.workerGeneration,
  });

  final int sourceSessionIdentity;
  final int leaseId;
  final int workerGeneration;
  FlarkV3SourceWorkerSyncKind get kind;
}

final class FlarkV3SourceSnapshotSyncAcknowledgement
    extends FlarkV3SourceWorkerSyncAcknowledgement {
  const FlarkV3SourceSnapshotSyncAcknowledgement({
    required super.sourceSessionIdentity,
    required super.leaseId,
    required super.workerGeneration,
    required this.baseUiRevision,
    required this.startUtf16,
    required this.endUtf16,
    required this.throughIntentSequence,
    required this.observedReplica,
  });

  @override
  FlarkV3SourceWorkerSyncKind get kind => FlarkV3SourceWorkerSyncKind.snapshot;

  final int baseUiRevision;
  final int startUtf16;
  final int endUtf16;
  final int throughIntentSequence;

  /// Present only on the final page of a snapshot installation.
  final FlarkV3ObservedSourceReplicaVersion? observedReplica;
}

final class FlarkV3SourceIntentSyncAcknowledgement
    extends FlarkV3SourceWorkerSyncAcknowledgement {
  const FlarkV3SourceIntentSyncAcknowledgement({
    required super.sourceSessionIdentity,
    required super.leaseId,
    required super.workerGeneration,
    required this.firstSequence,
    required this.lastSequence,
    required this.entryCount,
    required this.payloadUtf16,
    required this.observedReplica,
  });

  @override
  FlarkV3SourceWorkerSyncKind get kind => FlarkV3SourceWorkerSyncKind.intents;

  final int firstSequence;
  final int lastSequence;
  final int entryCount;
  final int payloadUtf16;
  final FlarkV3ObservedSourceReplicaVersion observedReplica;
}

enum FlarkV3SourceWorkerSyncAckDisposition { acknowledged, stale }

final class FlarkV3SourceWorkerSyncAckReceipt {
  const FlarkV3SourceWorkerSyncAckReceipt._({
    required this.disposition,
    required this.droppedIntentEntries,
    required this.droppedPayloadUtf16,
    required this.droppedDeletedUtf16,
    required this.droppedOperationCount,
    required this.workerRevision,
  });

  const FlarkV3SourceWorkerSyncAckReceipt.stale({required int workerRevision})
    : this._(
        disposition: FlarkV3SourceWorkerSyncAckDisposition.stale,
        droppedIntentEntries: 0,
        droppedPayloadUtf16: 0,
        droppedDeletedUtf16: 0,
        droppedOperationCount: 0,
        workerRevision: workerRevision,
      );

  const FlarkV3SourceWorkerSyncAckReceipt.acknowledged({
    required int droppedIntentEntries,
    required int droppedPayloadUtf16,
    required int droppedDeletedUtf16,
    required int droppedOperationCount,
    required int workerRevision,
  }) : this._(
         disposition: FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
         droppedIntentEntries: droppedIntentEntries,
         droppedPayloadUtf16: droppedPayloadUtf16,
         droppedDeletedUtf16: droppedDeletedUtf16,
         droppedOperationCount: droppedOperationCount,
         workerRevision: workerRevision,
       );

  final FlarkV3SourceWorkerSyncAckDisposition disposition;
  final int droppedIntentEntries;
  final int droppedPayloadUtf16;
  final int droppedDeletedUtf16;
  final int droppedOperationCount;
  final int workerRevision;
}

final class FlarkV3SourceWorkerSyncDiagnostics {
  const FlarkV3SourceWorkerSyncDiagnostics({
    required this.workerGeneration,
    required this.nextIntentSequence,
    required this.retainedJournalEntries,
    required this.retainedJournalPayloadUtf16,
    required this.retainedJournalDeletedUtf16,
    required this.retainedJournalOperationCount,
    required this.retainedSnapshotRootCount,
    required this.retainedSnapshotBackingBytesUpperBound,
    required this.liveLeaseCount,
    required this.invalidatedLeaseAwaitingDrainCount,
    required this.rebaseCount,
    required this.replacedSnapshotCount,
    required this.snapshotInstallPathNodesVisited,
    required this.snapshotInstallUtf16Copied,
    required this.pageUtf16Copied,
  });

  final int workerGeneration;
  final int nextIntentSequence;
  final int retainedJournalEntries;
  final int retainedJournalPayloadUtf16;
  final int retainedJournalDeletedUtf16;
  final int retainedJournalOperationCount;
  final int retainedSnapshotRootCount;
  final int retainedSnapshotBackingBytesUpperBound;
  final int liveLeaseCount;
  final int invalidatedLeaseAwaitingDrainCount;
  final int rebaseCount;
  final int replacedSnapshotCount;

  /// Snapshot install retains one immutable persistent root in O(1).
  final int snapshotInstallPathNodesVisited;
  final int snapshotInstallUtf16Copied;

  /// UTF-16 materialized only in explicitly credited snapshot pages.
  final int pageUtf16Copied;

  int get retainedJournalPayloadBytesUpperBound =>
      retainedJournalPayloadUtf16 * 2;

  int get retainedJournalSyncDebtBytesUpperBound =>
      (retainedJournalPayloadUtf16 + retainedJournalDeletedUtf16) * 2;

  int get authoritativeCurrentRootCount => 1;
}

final class _WorkerSnapshotRebase {
  _WorkerSnapshotRebase({
    required this.document,
    required this.workerGeneration,
    required this.throughIntentSequence,
  });

  final FlarkV3SourceDocument document;
  final int workerGeneration;
  final int throughIntentSequence;
  int acknowledgedUtf16 = 0;
}

final class _WorkerLiveSyncLease {
  const _WorkerLiveSyncLease(this.lease);

  final FlarkV3SourceWorkerSyncLease lease;
}

/// Session-minted proof that a worker replica has acknowledged one exact root
/// epoch. The stand-in scanner accepts this capability, never an arbitrary
/// same-shaped [FlarkV3SourceDocument].
final class FlarkV3AcknowledgedSourceReplica {
  const FlarkV3AcknowledgedSourceReplica._({
    required this.sourceSessionIdentity,
    required this.workerGeneration,
    required this.revision,
    required this.intentHighWater,
    required this.observedReplica,
    required FlarkV3SourceDocument document,
  }) : _document = document;

  final int sourceSessionIdentity;
  final int workerGeneration;
  final int revision;
  final int intentHighWater;
  final FlarkV3ObservedSourceReplicaVersion observedReplica;
  final FlarkV3SourceDocument _document;

  int get utf16Length => observedReplica.utf16Length;
  int get utf8Length => observedReplica.utf8Length;
}

/// Exact source-replica epoch owned by one certification transaction.
///
/// Every scanner page and completion repeats this lineage. Parser/source-fact
/// work can never advance worker credit; the lineage only proves which already
/// acknowledged replica the work observed.
final class FlarkV3SourceCertificationLineage {
  const FlarkV3SourceCertificationLineage({
    required this.sourceSessionIdentity,
    required this.requestId,
    required this.workerGeneration,
    required this.workerReplicaRevision,
    required this.uiRevision,
    required this.utf16Length,
    required this.intentHighWater,
  });

  final int sourceSessionIdentity;
  final int requestId;
  final int workerGeneration;
  final int workerReplicaRevision;
  final int uiRevision;
  final int utf16Length;
  final int intentHighWater;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourceCertificationLineage &&
      other.sourceSessionIdentity == sourceSessionIdentity &&
      other.requestId == requestId &&
      other.workerGeneration == workerGeneration &&
      other.workerReplicaRevision == workerReplicaRevision &&
      other.uiRevision == uiRevision &&
      other.utf16Length == utf16Length &&
      other.intentHighWater == intentHighWater;

  @override
  int get hashCode => Object.hash(
    sourceSessionIdentity,
    requestId,
    workerGeneration,
    workerReplicaRevision,
    uiRevision,
    utf16Length,
    intentHighWater,
  );
}

/// Independent budgets for one resumable worker scan poll.
///
/// A scalar value or CRLF pair may consume two UTF-16 code units when
/// [maximumSourceUtf16] is one; that one-unit safe overshoot is the only
/// exception and prevents a caller-selected fuel value from livelocking.
final class FlarkV3SourceFactScanCredit {
  const FlarkV3SourceFactScanCredit({
    required this.maximumSourceUtf16,
    required this.maximumSourceNodes,
    required this.maximumOutputCheckpoints,
    required this.maximumWireBytes,
  });

  final int maximumSourceUtf16;
  final int maximumSourceNodes;
  final int maximumOutputCheckpoints;
  final int maximumWireBytes;

  void _validate() {
    if (maximumSourceUtf16 < 1 ||
        maximumSourceUtf16 > _maximumSourceChunkUtf16) {
      throw RangeError.range(
        maximumSourceUtf16,
        1,
        _maximumSourceChunkUtf16,
        'maximumSourceUtf16',
      );
    }
    if (maximumSourceNodes < 1 ||
        maximumSourceNodes > _maximumSourceFactScanNodes) {
      throw RangeError.range(
        maximumSourceNodes,
        1,
        _maximumSourceFactScanNodes,
        'maximumSourceNodes',
      );
    }
    if (maximumOutputCheckpoints < 1 ||
        maximumOutputCheckpoints > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        maximumOutputCheckpoints,
        1,
        _maximumWorkerSyncPageEntries,
        'maximumOutputCheckpoints',
      );
    }
    final minimumWire =
        _sourceFactPageBaseWireBytes + _sourceFactCheckpointWireBytes;
    if (maximumWireBytes < minimumWire ||
        maximumWireBytes > _maximumSourceFactWireBytes) {
      throw RangeError.range(
        maximumWireBytes,
        minimumWire,
        _maximumSourceFactWireBytes,
        'maximumWireBytes',
      );
    }
  }
}

List<FlarkV3SourcePrefixFacts> _boundedCheckpointCopy(
  List<FlarkV3SourcePrefixFacts> checkpoints,
) {
  if (checkpoints.isEmpty ||
      checkpoints.length > _maximumWorkerSyncPageEntries) {
    throw RangeError.range(
      checkpoints.length,
      1,
      _maximumWorkerSyncPageEntries,
      'checkpoints.length',
    );
  }
  return List<FlarkV3SourcePrefixFacts>.of(checkpoints, growable: false);
}

/// One numeric, source-free output page from a resumable scan.
///
/// The scanner creates an owned page. Staging transfers that bounded list into
/// the candidate's checkpoint tree and consumes this transport object.
final class FlarkV3SourceFactCheckpointPage {
  factory FlarkV3SourceFactCheckpointPage({
    required FlarkV3SourceCertificationLineage lineage,
    required FlarkV3SourcePieceToCertify piece,
    required int pageOrdinal,
    required int piecePageOrdinal,
    required int relativeStartUtf16,
    required int relativeEndUtf16,
    required int checkpointSpacingUtf16,
    required bool isLast,
    required List<FlarkV3SourcePrefixFacts> checkpoints,
  }) => FlarkV3SourceFactCheckpointPage._owned(
    lineage: lineage,
    piece: piece,
    pageOrdinal: pageOrdinal,
    piecePageOrdinal: piecePageOrdinal,
    relativeStartUtf16: relativeStartUtf16,
    relativeEndUtf16: relativeEndUtf16,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
    isLast: isLast,
    checkpoints: _boundedCheckpointCopy(checkpoints),
  );

  FlarkV3SourceFactCheckpointPage._owned({
    required this.lineage,
    required this.piece,
    required this.pageOrdinal,
    required this.piecePageOrdinal,
    required this.relativeStartUtf16,
    required this.relativeEndUtf16,
    required this.checkpointSpacingUtf16,
    required this.isLast,
    required List<FlarkV3SourcePrefixFacts> checkpoints,
  }) : _checkpoints = checkpoints {
    if (checkpoints.isEmpty ||
        checkpoints.length > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        checkpoints.length,
        1,
        _maximumWorkerSyncPageEntries,
        'checkpoints.length',
      );
    }
  }

  final FlarkV3SourceCertificationLineage lineage;
  final FlarkV3SourcePieceToCertify piece;
  final int pageOrdinal;
  final int piecePageOrdinal;
  final int relativeStartUtf16;
  final int relativeEndUtf16;
  final int checkpointSpacingUtf16;
  final bool isLast;
  List<FlarkV3SourcePrefixFacts>? _checkpoints;

  bool get isConsumed => _checkpoints == null;
  int get checkpointCount => _checkpoints?.length ?? 0;
  int get declaredWireBytes =>
      _sourceFactPageBaseWireBytes +
      checkpointCount * _sourceFactCheckpointWireBytes;

  List<FlarkV3SourcePrefixFacts> get checkpoints {
    final facts = _checkpoints;
    if (facts == null) throw StateError('Checkpoint page was already staged.');
    return UnmodifiableListView(facts);
  }

  FlarkV3SourcePrefixFacts get endFacts => checkpoints.last;

  List<FlarkV3SourcePrefixFacts> _takeCheckpoints() {
    final facts = _checkpoints;
    if (facts == null) throw StateError('Checkpoint page was already staged.');
    _checkpoints = null;
    return facts;
  }
}

/// One canonical global-prefix page emitted by the runtime-owned Rust scan.
///
/// Unlike [FlarkV3SourceFactCheckpointPage], this production transport shape
/// is independent of Dart rope-piece boundaries. Page ownership moves into one
/// hidden global candidate; no Dart source scan is performed while staging it.
final class FlarkV3CanonicalSourceFactCheckpointPage {
  factory FlarkV3CanonicalSourceFactCheckpointPage({
    required FlarkV3SourceCertificationLineage lineage,
    required int pageOrdinal,
    required int pageCount,
    required int checkpointCount,
    required int checkpointSpacingUtf16,
    required List<FlarkV3SourcePrefixFacts> checkpoints,
  }) => FlarkV3CanonicalSourceFactCheckpointPage._owned(
    lineage: lineage,
    pageOrdinal: pageOrdinal,
    pageCount: pageCount,
    checkpointCount: checkpointCount,
    checkpointSpacingUtf16: checkpointSpacingUtf16,
    checkpoints: _boundedCheckpointCopy(checkpoints),
  );

  FlarkV3CanonicalSourceFactCheckpointPage._owned({
    required this.lineage,
    required this.pageOrdinal,
    required this.pageCount,
    required this.checkpointCount,
    required this.checkpointSpacingUtf16,
    required List<FlarkV3SourcePrefixFacts> checkpoints,
  }) : _checkpoints = checkpoints;

  final FlarkV3SourceCertificationLineage lineage;
  final int pageOrdinal;
  final int pageCount;
  final int checkpointCount;
  final int checkpointSpacingUtf16;
  List<FlarkV3SourcePrefixFacts>? _checkpoints;

  bool get isConsumed => _checkpoints == null;
  int get pageCheckpointCount => _checkpoints?.length ?? 0;
  int get declaredWireBytes =>
      _sourceFactPageBaseWireBytes +
      pageCheckpointCount * _sourceFactCheckpointWireBytes;

  List<FlarkV3SourcePrefixFacts> get checkpoints {
    final facts = _checkpoints;
    if (facts == null) throw StateError('Checkpoint page was already staged.');
    return UnmodifiableListView(facts);
  }

  List<FlarkV3SourcePrefixFacts> _takeCheckpoints() {
    final facts = _checkpoints;
    if (facts == null) throw StateError('Checkpoint page was already staged.');
    _checkpoints = null;
    return facts;
  }
}

/// Session-minted authority for one exact installed canonical SourceFacts root.
///
/// The object identity is intentional: a runtime adapter must retain the
/// authority returned by this session and present that same object when it
/// begins an incremental certification. Reconstructing equal-looking fields
/// from the wire does not manufacture base authority.
final class FlarkV3CanonicalSourceFactAuthority {
  const FlarkV3CanonicalSourceFactAuthority._({
    required this.sourceSessionIdentity,
    required this.workerGeneration,
    required this.workerReplicaRevision,
    required this.intentHighWater,
    required this.fingerprintAlgorithm,
    required this.fingerprint,
    required this.logicalLineBreaks,
    required this.checkpointSpacingUtf16,
    required this.checkpointCount,
    required this.pageCount,
    required this.checkpointHash128,
  });

  final int sourceSessionIdentity;
  final int workerGeneration;
  final int workerReplicaRevision;
  final int intentHighWater;
  final int fingerprintAlgorithm;
  final FlarkV3SourceFingerprint fingerprint;
  final int logicalLineBreaks;
  final int checkpointSpacingUtf16;
  final int checkpointCount;
  final int pageCount;

  /// Exact adopted checkpoint-root guard.
  ///
  /// A clean certification installs its v1 portable fold; an incremental
  /// certification installs the opaque v2 persistent-root guard. Object
  /// identity binds this value to the installed base across either form.
  final FlarkV3ContentHash128 checkpointHash128;
}

/// Persistent SourceFacts root-guard algorithm emitted by the incremental
/// engine path.
///
/// The guard is opaque to Dart. Promotion authenticates its version and exact
/// begin/completion equality while independently validating the local
/// persistent splice, replacement facts, counts, and terminal source facts.
/// Algorithm 2 is domain-separated as
/// `flark.source-facts.persistent-checkpoint-root-guard.v2\0`.
const int flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm = 2;

/// Authenticated header for one bounded canonical SourceFacts page splice.
///
/// Page ranges are half-open and use canonical checkpoint-page ordinals. The
/// target range must start at the base range start. Only the replacement pages
/// in [targetPageStart, targetPageEnd) cross the native/Wasm boundary.
final class FlarkV3CanonicalSourceFactDelta {
  const FlarkV3CanonicalSourceFactDelta({
    required this.lineage,
    required this.baseAuthority,
    required this.baseFingerprint,
    required this.baseCheckpointRootGuard128,
    required this.baseCheckpointCount,
    required this.basePageCount,
    required this.baseCheckpointSpacingUtf16,
    required this.basePageStart,
    required this.basePageEnd,
    required this.targetPageStart,
    required this.targetPageEnd,
    required this.targetCheckpointCount,
    required this.targetPageCount,
    required this.targetCheckpointRootGuardAlgorithm,
    required this.targetCheckpointRootGuard128,
    required this.replacementCheckpointCount,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final FlarkV3CanonicalSourceFactAuthority baseAuthority;
  final FlarkV3SourceFingerprint baseFingerprint;
  final FlarkV3ContentHash128 baseCheckpointRootGuard128;
  final int baseCheckpointCount;
  final int basePageCount;
  final int baseCheckpointSpacingUtf16;
  final int basePageStart;
  final int basePageEnd;
  final int targetPageStart;
  final int targetPageEnd;
  final int targetCheckpointCount;
  final int targetPageCount;
  final int targetCheckpointRootGuardAlgorithm;
  final FlarkV3ContentHash128 targetCheckpointRootGuard128;
  final int replacementCheckpointCount;

  int get replacementPageCount => targetPageEnd - targetPageStart;
}

/// One bounded replacement page in an accepted canonical SourceFacts delta.
///
/// Facts remain absolute target-prefix facts on the wire. Staging consumes the
/// list and converts it to page-relative facts, which allows unchanged suffix
/// pages to remain structurally shared even when their global offsets move.
final class FlarkV3CanonicalSourceFactDeltaCheckpointPage {
  factory FlarkV3CanonicalSourceFactDeltaCheckpointPage({
    required FlarkV3SourceCertificationLineage lineage,
    required int pageOrdinal,
    required List<FlarkV3SourcePrefixFacts> checkpoints,
  }) => FlarkV3CanonicalSourceFactDeltaCheckpointPage._owned(
    lineage: lineage,
    pageOrdinal: pageOrdinal,
    checkpoints: _boundedCheckpointCopy(checkpoints),
  );

  FlarkV3CanonicalSourceFactDeltaCheckpointPage._owned({
    required this.lineage,
    required this.pageOrdinal,
    required List<FlarkV3SourcePrefixFacts> checkpoints,
  }) : _checkpoints = checkpoints;

  final FlarkV3SourceCertificationLineage lineage;
  final int pageOrdinal;
  List<FlarkV3SourcePrefixFacts>? _checkpoints;

  bool get isConsumed => _checkpoints == null;
  int get checkpointCount => _checkpoints?.length ?? 0;
  int get declaredWireBytes =>
      _sourceFactPageBaseWireBytes +
      checkpointCount * _sourceFactCheckpointWireBytes;

  List<FlarkV3SourcePrefixFacts> get checkpoints {
    final facts = _checkpoints;
    if (facts == null) throw StateError('Delta page was already staged.');
    return UnmodifiableListView(facts);
  }

  List<FlarkV3SourcePrefixFacts> _takeCheckpoints() {
    final facts = _checkpoints;
    if (facts == null) throw StateError('Delta page was already staged.');
    _checkpoints = null;
    return facts;
  }
}

/// Terminal target proof for one canonical SourceFacts delta.
///
/// [replacementCheckpointHash128] authenticates exactly the bounded absolute
/// facts transferred for this splice. The complete target proof is still
/// checked against the exact live source and the locally composed persistent
/// checkpoint root before promotion.
final class FlarkV3CanonicalSourceFactDeltaCompletion {
  const FlarkV3CanonicalSourceFactDeltaCompletion({
    required this.lineage,
    required this.fingerprintAlgorithm,
    required this.fingerprint,
    required this.logicalLineBreaks,
    required this.checkpointSpacingUtf16,
    required this.checkpointCount,
    required this.pageCount,
    required this.checkpointRootGuardAlgorithm,
    required this.checkpointRootGuard128,
    required this.replacementCheckpointHash128,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final int fingerprintAlgorithm;
  final FlarkV3SourceFingerprint fingerprint;
  final int logicalLineBreaks;
  final int checkpointSpacingUtf16;
  final int checkpointCount;
  final int pageCount;
  final int checkpointRootGuardAlgorithm;
  final FlarkV3ContentHash128 checkpointRootGuard128;
  final FlarkV3ContentHash128 replacementCheckpointHash128;

  int get declaredWireBytes => _sourceFactCompletionWireBytes;
}

final class FlarkV3SourceFactScanWorkReceipt {
  const FlarkV3SourceFactScanWorkReceipt({
    required this.sourceUtf16Examined,
    required this.sourceNodesVisited,
    required this.checkpointsEmitted,
    required this.wireBytesEmitted,
  });

  final int sourceUtf16Examined;
  final int sourceNodesVisited;
  final int checkpointsEmitted;
  final int wireBytesEmitted;

  /// The scanner retains tree/backing references and never materializes a
  /// whole source or whole piece.
  int get wholeSourceUtf16Copied => 0;

  /// UTF-8 hashing writes directly into four integer lanes. It allocates no
  /// temporary byte List per scalar.
  int get utf8ScratchCollectionsAllocated => 0;
}

/// Aggregate proof emitted only after the scanner has visited the exact root.
final class FlarkV3SourceFactCompletion {
  const FlarkV3SourceFactCompletion({
    required this.lineage,
    required this.fingerprint,
    required this.logicalLineBreaks,
    required this.pieceCount,
    required this.checkpointCount,
    required this.pageCount,
    required this.descriptorHash128,
    required this.checkpointHash128,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final FlarkV3SourceFingerprint fingerprint;
  final int logicalLineBreaks;
  final int pieceCount;
  final int checkpointCount;
  final int pageCount;
  final FlarkV3ContentHash128 descriptorHash128;
  final FlarkV3ContentHash128 checkpointHash128;
  int get declaredWireBytes => _sourceFactCompletionWireBytes;
}

/// Terminal authority for one canonical global SourceFacts stream.
final class FlarkV3CanonicalSourceFactCompletion {
  const FlarkV3CanonicalSourceFactCompletion({
    required this.lineage,
    required this.fingerprintAlgorithm,
    required this.fingerprint,
    required this.logicalLineBreaks,
    required this.checkpointSpacingUtf16,
    required this.checkpointCount,
    required this.pageCount,
    required this.checkpointHash128,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final int fingerprintAlgorithm;
  final FlarkV3SourceFingerprint fingerprint;
  final int logicalLineBreaks;
  final int checkpointSpacingUtf16;
  final int checkpointCount;
  final int pageCount;
  final FlarkV3ContentHash128 checkpointHash128;

  int get declaredWireBytes => _sourceFactCompletionWireBytes;
}

final class FlarkV3SourceFactScanPoll {
  const FlarkV3SourceFactScanPoll._({
    required this.page,
    required this.completion,
    required this.work,
    required this.isCancelled,
  });

  final FlarkV3SourceFactCheckpointPage? page;
  final FlarkV3SourceFactCompletion? completion;
  final FlarkV3SourceFactScanWorkReceipt work;
  final bool isCancelled;

  bool get isComplete => completion != null;
}

final class _SourceFactScanFrame {
  const _SourceFactScanFrame(this.node, this.globalStartUtf16);

  final _SourceNode node;
  final int globalStartUtf16;
}

final class _ActiveSourceFactScanPiece {
  _ActiveSourceFactScanPiece({
    required this.leaf,
    required this.piece,
    required this.facts,
    required this.nextCheckpointUtf16,
  }) : emittedThroughUtf16 = facts.utf16Offset;

  final _SourceLeaf leaf;
  final FlarkV3SourcePieceToCertify piece;
  FlarkV3SourcePrefixFacts facts;
  int nextCheckpointUtf16;
  int emittedThroughUtf16;
  int pageOrdinal = 0;
}

/// Resumable worker stand-in over an exact persistent source replica.
///
/// Polling retains a DFS cursor and scans backing ranges directly. It emits no
/// source strings, makes progress with node/source fuel of one, and can be
/// cancelled by dropping two root pointers.
final class FlarkV3SourceFactScanner {
  FlarkV3SourceFactScanner(
    FlarkV3SourceCertificationRequest request, {
    required FlarkV3AcknowledgedSourceReplica sourceReplica,
    int checkpointSpacingUtf16 = 4096,
  }) : lineage = request.lineage,
       _checkpointSpacingUtf16 = checkpointSpacingUtf16,
       _expectedFirstPieces = request.firstPiecePage.pieces,
       _firstPageHasMore = request.firstPiecePage.hasMore {
    if (checkpointSpacingUtf16 < 2 ||
        checkpointSpacingUtf16 > _maximumSourceChunkUtf16) {
      throw RangeError.range(
        checkpointSpacingUtf16,
        2,
        _maximumSourceChunkUtf16,
        'checkpointSpacingUtf16',
      );
    }
    final replicaDocument = sourceReplica._document;
    if (sourceReplica.sourceSessionIdentity != lineage.sourceSessionIdentity ||
        sourceReplica.workerGeneration != lineage.workerGeneration ||
        sourceReplica.revision != lineage.workerReplicaRevision ||
        sourceReplica.intentHighWater != lineage.intentHighWater ||
        replicaDocument.revision != lineage.uiRevision ||
        replicaDocument.utf16Length != lineage.utf16Length ||
        !identical(replicaDocument._root, request._sourceRoot)) {
      throw StateError('Certification source replica is stale.');
    }
    _stack = replicaDocument._root == null
        ? <_SourceFactScanFrame>[]
        : <_SourceFactScanFrame>[
            _SourceFactScanFrame(replicaDocument._root, 0),
          ];
  }

  final FlarkV3SourceCertificationLineage lineage;
  final int _checkpointSpacingUtf16;
  final List<FlarkV3SourcePieceToCertify> _expectedFirstPieces;
  final bool _firstPageHasMore;
  List<_SourceFactScanFrame>? _stack;
  _ActiveSourceFactScanPiece? _active;
  bool _cancelled = false;
  bool _completionEmitted = false;
  int _expectedFirstPieceIndex = 0;
  int _globalPageOrdinal = 0;
  int _pieceCount = 0;
  int _checkpointCount = 0;
  int _pageCount = 0;
  int _aggregateUtf8 = 0;
  int _aggregateNewlines = 0;
  FlarkV3ContentHash128 _aggregateHash = FlarkV3ContentHash128.zero;
  FlarkV3ContentHash128 _descriptorHash = FlarkV3ContentHash128.zero;
  FlarkV3ContentHash128 _checkpointHash = FlarkV3ContentHash128.zero;
  bool _previousEndsWithCarriageReturn = false;

  bool get isCancelled => _cancelled;
  bool get isComplete => _completionEmitted;

  /// Releases the replica cursor and active backing reference in O(1).
  bool cancel() {
    if (_cancelled || _completionEmitted) return false;
    _cancelled = true;
    _stack = null;
    _active = null;
    return true;
  }

  FlarkV3SourceFactScanPoll poll(FlarkV3SourceFactScanCredit credit) {
    credit._validate();
    if (_cancelled) {
      return const FlarkV3SourceFactScanPoll._(
        page: null,
        completion: null,
        work: FlarkV3SourceFactScanWorkReceipt(
          sourceUtf16Examined: 0,
          sourceNodesVisited: 0,
          checkpointsEmitted: 0,
          wireBytesEmitted: 0,
        ),
        isCancelled: true,
      );
    }
    if (_completionEmitted) {
      throw StateError('Source-fact scanner already emitted completion.');
    }

    var nodesVisited = 0;
    while (_active == null &&
        _stack!.isNotEmpty &&
        nodesVisited < credit.maximumSourceNodes) {
      final frame = _stack!.removeLast();
      nodesVisited += 1;
      final node = frame.node;
      if (node case final _SourceBranch branch) {
        _stack!.add(
          _SourceFactScanFrame(
            branch.right,
            frame.globalStartUtf16 + branch.left.utf16Length,
          ),
        );
        _stack!.add(_SourceFactScanFrame(branch.left, frame.globalStartUtf16));
        continue;
      }
      final leaf = node as _SourceLeaf;
      if (leaf.isCertified) {
        _appendAggregate(
          utf8Length: leaf.utf8Length,
          newlines: leaf.newlines,
          hash: leaf.contentHash128,
          power: leaf.hashPower128,
          startsWithLineFeed: leaf.startsWithLineFeed,
          endsWithCarriageReturn: leaf.endsWithCarriageReturn,
        );
        continue;
      }
      final piece = FlarkV3SourcePieceToCertify._(
        pieceId: leaf.pieceId!,
        sourceStartUtf16: leaf.start,
        utf16Length: leaf.utf16Length,
        globalStartUtf16: frame.globalStartUtf16,
      );
      _validateFirstDescriptor(piece);
      _descriptorHash = _appendSourcePieceDescriptorHash(
        _descriptorHash,
        piece,
      );
      _active = _ActiveSourceFactScanPiece(
        leaf: leaf,
        piece: piece,
        facts: FlarkV3SourcePrefixFacts(
          utf16Offset: leaf.start,
          utf8Offset: 0,
          newlines: 0,
          hash: FlarkV3ContentHash128.zero,
        ),
        nextCheckpointUtf16: math.min(
          leaf.start + leaf.utf16Length,
          leaf.start + _checkpointSpacingUtf16,
        ),
      );
    }

    if (_active case final active?) {
      try {
        final outputLimit = math.min(
          credit.maximumOutputCheckpoints,
          (credit.maximumWireBytes - _sourceFactPageBaseWireBytes) ~/
              _sourceFactCheckpointWireBytes,
        );
        final pageStart = active.emittedThroughUtf16;
        final pieceEnd = active.leaf.start + active.leaf.utf16Length;
        final output = <FlarkV3SourcePrefixFacts>[];
        var examined = 0;
        while (active.facts.utf16Offset < pieceEnd &&
            output.length < outputLimit &&
            examined < credit.maximumSourceUtf16) {
          final remainingFuel = credit.maximumSourceUtf16 - examined;
          var requestedEnd = math.min(
            pieceEnd,
            math.min(
              active.nextCheckpointUtf16,
              active.facts.utf16Offset + remainingFuel,
            ),
          );
          if (requestedEnd <= active.facts.utf16Offset) {
            requestedEnd = math.min(pieceEnd, active.facts.utf16Offset + 1);
          }
          requestedEnd = _scalarAndCrLfSafePageEnd(
            active.leaf.source,
            active.facts.utf16Offset,
            requestedEnd,
            pieceEnd,
          );
          final before = active.facts.utf16Offset;
          active.facts = _advanceSourcePrefix(
            active.leaf.source,
            active.facts,
            pieceEnd,
            requestedEnd,
            validationGlobalStart: active.piece.globalStartUtf16,
            validationRangeStart: active.leaf.start,
          );
          examined += active.facts.utf16Offset - before;
          if (active.facts.utf16Offset >= active.nextCheckpointUtf16) {
            output.add(active.facts);
            active.nextCheckpointUtf16 = math.min(
              pieceEnd,
              active.facts.utf16Offset + _checkpointSpacingUtf16,
            );
          }
        }
        if (output.isEmpty) {
          return FlarkV3SourceFactScanPoll._(
            page: null,
            completion: null,
            work: FlarkV3SourceFactScanWorkReceipt(
              sourceUtf16Examined: examined,
              sourceNodesVisited: nodesVisited,
              checkpointsEmitted: 0,
              wireBytesEmitted: 0,
            ),
            isCancelled: false,
          );
        }
        final emittedEnd = output.last.utf16Offset;
        final isLast = emittedEnd == pieceEnd;
        final page = FlarkV3SourceFactCheckpointPage._owned(
          lineage: lineage,
          piece: active.piece,
          pageOrdinal: _globalPageOrdinal++,
          piecePageOrdinal: active.pageOrdinal++,
          relativeStartUtf16: pageStart - active.leaf.start,
          relativeEndUtf16: emittedEnd - active.leaf.start,
          checkpointSpacingUtf16: _checkpointSpacingUtf16,
          isLast: isLast,
          checkpoints: output,
        );
        active.emittedThroughUtf16 = emittedEnd;
        _checkpointCount += output.length;
        for (final fact in output) {
          _checkpointHash = _appendSourcePrefixFactsHash(_checkpointHash, fact);
        }
        _pageCount += 1;
        if (isLast) {
          _appendAggregate(
            utf8Length: active.facts.utf8Offset,
            newlines: active.facts.newlines,
            hash: active.facts.hash,
            power: _powHash128(active.facts.utf8Offset),
            startsWithLineFeed: active.leaf.startsWithLineFeed,
            endsWithCarriageReturn: active.leaf.endsWithCarriageReturn,
          );
          _pieceCount += 1;
          _active = null;
        }
        return FlarkV3SourceFactScanPoll._(
          page: page,
          completion: null,
          work: FlarkV3SourceFactScanWorkReceipt(
            sourceUtf16Examined: examined,
            sourceNodesVisited: nodesVisited,
            checkpointsEmitted: output.length,
            wireBytesEmitted: page.declaredWireBytes,
          ),
          isCancelled: false,
        );
      } on FlarkV3SourceCertificationFailure catch (failure) {
        cancel();
        throw FlarkV3SourceCertificationFailure(
          utf16Offset: failure.utf16Offset,
          lineage: lineage,
        );
      }
    }

    if (_stack!.isEmpty) {
      if (_expectedFirstPieceIndex < _expectedFirstPieces.length) {
        cancel();
        throw StateError('Certification request does not match its replica.');
      }
      final completion = FlarkV3SourceFactCompletion(
        lineage: lineage,
        fingerprint: FlarkV3SourceFingerprint(
          revision: lineage.uiRevision,
          utf16Length: lineage.utf16Length,
          utf8Length: _aggregateUtf8,
          contentHash128: _aggregateHash,
        ),
        logicalLineBreaks: _aggregateNewlines,
        pieceCount: _pieceCount,
        checkpointCount: _checkpointCount,
        pageCount: _pageCount,
        descriptorHash128: _descriptorHash,
        checkpointHash128: _checkpointHash,
      );
      _completionEmitted = true;
      _stack = null;
      return FlarkV3SourceFactScanPoll._(
        page: null,
        completion: completion,
        work: const FlarkV3SourceFactScanWorkReceipt(
          sourceUtf16Examined: 0,
          sourceNodesVisited: 0,
          checkpointsEmitted: 0,
          wireBytesEmitted: _sourceFactCompletionWireBytes,
        ),
        isCancelled: false,
      );
    }

    return FlarkV3SourceFactScanPoll._(
      page: null,
      completion: null,
      work: FlarkV3SourceFactScanWorkReceipt(
        sourceUtf16Examined: 0,
        sourceNodesVisited: nodesVisited,
        checkpointsEmitted: 0,
        wireBytesEmitted: 0,
      ),
      isCancelled: false,
    );
  }

  void _validateFirstDescriptor(FlarkV3SourcePieceToCertify piece) {
    if (_expectedFirstPieceIndex < _expectedFirstPieces.length) {
      final expected = _expectedFirstPieces[_expectedFirstPieceIndex++];
      if (expected._key != piece._key ||
          expected.globalStartUtf16 != piece.globalStartUtf16) {
        cancel();
        throw StateError('Certification request does not match its replica.');
      }
      return;
    }
    if (!_firstPageHasMore) {
      cancel();
      throw StateError('Certification replica has an unexpected live piece.');
    }
  }

  void _appendAggregate({
    required int utf8Length,
    required int newlines,
    required FlarkV3ContentHash128 hash,
    required FlarkV3ContentHash128 power,
    required bool startsWithLineFeed,
    required bool endsWithCarriageReturn,
  }) {
    _aggregateNewlines +=
        newlines -
        (_previousEndsWithCarriageReturn && startsWithLineFeed ? 1 : 0);
    _aggregateUtf8 += utf8Length;
    _aggregateHash = _appendHash128(_aggregateHash, hash, power);
    _previousEndsWithCarriageReturn = endsWithCarriageReturn;
  }
}

int _scalarAndCrLfSafePageEnd(
  String source,
  int start,
  int requestedEnd,
  int indexedEnd,
) {
  var end = requestedEnd;
  if (end <= start || end >= indexedEnd) return end;
  final previous = source.codeUnitAt(end - 1);
  final next = source.codeUnitAt(end);
  if ((_isHighSurrogate(previous) && _isLowSurrogate(next)) ||
      (previous == 0x0D && next == 0x0A)) {
    end += 1;
  }
  return end;
}

FlarkV3ContentHash128 _appendSourcePieceDescriptorHash(
  FlarkV3ContentHash128 hash,
  FlarkV3SourcePieceToCertify piece,
) {
  var result = hash;
  for (final value in <int>[
    piece.pieceId,
    piece.sourceStartUtf16,
    piece.utf16Length,
    piece.globalStartUtf16,
  ]) {
    for (var shift = 0; shift < 64; shift += 8) {
      result = _appendHashByte(result, (value >>> shift) & 0xFF);
    }
  }
  return result;
}

FlarkV3ContentHash128 _appendSourcePrefixFactsHash(
  FlarkV3ContentHash128 hash,
  FlarkV3SourcePrefixFacts facts,
) {
  var result = hash;
  for (final value in <int>[
    facts.utf16Offset,
    facts.utf8Offset,
    facts.newlines,
    facts.hash.word0,
    facts.hash.word1,
    facts.hash.word2,
    facts.hash.word3,
  ]) {
    for (var shift = 0; shift < 64; shift += 8) {
      result = _appendHashByte(result, (value >>> shift) & 0xFF);
    }
  }
  return result;
}

final class FlarkV3SourceCertificationRequest {
  FlarkV3SourceCertificationRequest._({
    required this.lineage,
    required this.firstPiecePage,
    required _SourceNode? sourceRoot,
  }) : _sourceRoot = sourceRoot;

  final FlarkV3SourceCertificationLineage lineage;
  final FlarkV3SourcePendingPiecePage firstPiecePage;
  final _SourceNode? _sourceRoot;

  int get sourceSessionIdentity => lineage.sourceSessionIdentity;
  int get requestId => lineage.requestId;
  int get workerGeneration => lineage.workerGeneration;
  int get workerReplicaRevision => lineage.workerReplicaRevision;
  int get uiRevision => lineage.uiRevision;
  int get utf16Length => lineage.utf16Length;
  int get intentHighWater => lineage.intentHighWater;

  /// Compatibility view of the bounded first page only.
  List<FlarkV3SourcePieceToCertify> get pieces => firstPiecePage.pieces;
}

final class FlarkV3CertifiedSourcePiece {
  const FlarkV3CertifiedSourcePiece._({
    required this.pieceId,
    required this.sourceStartUtf16,
    required this.utf16Length,
    required _SourceRangeIndex index,
  }) : _index = index;

  factory FlarkV3CertifiedSourcePiece.scan(
    FlarkV3SourcePieceToCertify piece, {
    required String sourceFragment,
    int checkpointSpacingUtf16 = 4096,
  }) {
    if (sourceFragment.length != piece.utf16Length) {
      throw StateError('Certified source fragment has the wrong length.');
    }
    if (sourceFragment.length > _maximumOneShotCertificationUtf16) {
      throw StateError(
        'One-shot source certification is limited to '
        '$_maximumOneShotCertificationUtf16 UTF-16 code units; use the '
        'resumable source-fact scanner for this piece.',
      );
    }
    return FlarkV3CertifiedSourcePiece._(
      pieceId: piece.pieceId,
      sourceStartUtf16: piece.sourceStartUtf16,
      utf16Length: piece.utf16Length,
      index: _SourceRangeIndex.scanFragmentForBacking(
        sourceFragment,
        backingStartUtf16: piece.sourceStartUtf16,
        spacingUtf16: checkpointSpacingUtf16,
        globalStartUtf16: piece.globalStartUtf16,
      ),
    );
  }

  final int pieceId;
  final int sourceStartUtf16;
  final int utf16Length;
  final _SourceRangeIndex _index;

  int get checkpointCount => _index.checkpoints.length;

  _SourcePieceKey get _key =>
      _SourcePieceKey(pieceId, sourceStartUtf16, utf16Length);
}

final class FlarkV3SourceCertificationReceipt {
  FlarkV3SourceCertificationReceipt._({
    required this.sourceSessionIdentity,
    required this.requestId,
    required this.uiRevision,
    required this.utf16Length,
    this.workerGeneration = 1,
    int? workerReplicaRevision,
    this.intentHighWater = 0,
    required List<FlarkV3CertifiedSourcePiece> pieces,
  }) : workerReplicaRevision = workerReplicaRevision ?? uiRevision,
       pieces = _boundedCertificationPieceCopy(pieces);

  /// Test/worker stand-in. Production constructs the same typed receipt from
  /// bounded pages emitted by the native/Wasm source job.
  factory FlarkV3SourceCertificationReceipt.scan(
    FlarkV3SourceCertificationRequest request, {
    required FlarkV3SourceDocument sourceReplica,
    int checkpointSpacingUtf16 = 4096,
  }) {
    if (request.firstPiecePage.hasMore) {
      throw StateError(
        'Certification request has additional bounded pages. The worker '
        'protocol must collect them before constructing one final receipt.',
      );
    }
    if (sourceReplica.revision != request.uiRevision ||
        sourceReplica.utf16Length != request.utf16Length ||
        !identical(sourceReplica._root, request._sourceRoot)) {
      throw StateError('Certification source replica is stale.');
    }
    final pendingUtf16 = request.pieces.fold<int>(
      0,
      (total, piece) => total + piece.utf16Length,
    );
    if (pendingUtf16 > _maximumOneShotCertificationUtf16) {
      throw StateError(
        'One-shot source certification is limited to '
        '$_maximumOneShotCertificationUtf16 UTF-16 code units; use the '
        'resumable source-fact scanner.',
      );
    }
    return FlarkV3SourceCertificationReceipt._(
      sourceSessionIdentity: request.sourceSessionIdentity,
      requestId: request.requestId,
      uiRevision: request.uiRevision,
      utf16Length: request.utf16Length,
      workerGeneration: request.workerGeneration,
      workerReplicaRevision: request.workerReplicaRevision,
      intentHighWater: request.intentHighWater,
      pieces: [
        for (final piece in request.pieces)
          FlarkV3CertifiedSourcePiece.scan(
            piece,
            sourceFragment: sourceReplica.readRange(
              piece.globalStartUtf16,
              piece.globalStartUtf16 + piece.utf16Length,
            ),
            checkpointSpacingUtf16: checkpointSpacingUtf16,
          ),
      ],
    );
  }

  final int sourceSessionIdentity;
  final int requestId;
  final int uiRevision;
  final int utf16Length;
  final int workerGeneration;
  final int workerReplicaRevision;
  final int intentHighWater;
  final List<FlarkV3CertifiedSourcePiece> pieces;

  FlarkV3SourceCertificationLineage get lineage =>
      FlarkV3SourceCertificationLineage(
        sourceSessionIdentity: sourceSessionIdentity,
        requestId: requestId,
        workerGeneration: workerGeneration,
        workerReplicaRevision: workerReplicaRevision,
        uiRevision: uiRevision,
        utf16Length: utf16Length,
        intentHighWater: intentHighWater,
      );
}

List<FlarkV3CertifiedSourcePiece> _boundedCertificationPieceCopy(
  List<FlarkV3CertifiedSourcePiece> pieces,
) {
  if (pieces.length > _maximumSynchronousCertificationAttachments) {
    throw FlarkV3SourceStagedCertificationRequired(
      pieceCount: pieces.length,
      maximumSynchronousAttachments:
          _maximumSynchronousCertificationAttachments,
    );
  }
  return List<FlarkV3CertifiedSourcePiece>.unmodifiable(pieces);
}

enum FlarkV3SourcePromotionDisposition { promoted, stale, rejected }

/// Exact proof returned only by a successful canonical promotion.
///
/// The parser event receipt repeats this shape so Rust can make the certified
/// source externally eligible only after Dart and host authority advanced.
final class FlarkV3CanonicalSourcePromotionProof {
  const FlarkV3CanonicalSourcePromotionProof({
    required this.lineage,
    required this.fingerprintAlgorithm,
    required this.fingerprint,
    required this.logicalLineBreaks,
    required this.checkpointSpacingUtf16,
    required this.checkpointCount,
    required this.pageCount,
    required this.checkpointHash128,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final int fingerprintAlgorithm;
  final FlarkV3SourceFingerprint fingerprint;
  final int logicalLineBreaks;
  final int checkpointSpacingUtf16;
  final int checkpointCount;
  final int pageCount;
  final FlarkV3ContentHash128 checkpointHash128;
}

final class FlarkV3SourcePromotionReceipt {
  const FlarkV3SourcePromotionReceipt._({
    required this.disposition,
    required this.pathNodesVisited,
    required this.piecesAttached,
    this.canonicalProof,
  });

  const FlarkV3SourcePromotionReceipt.stale()
    : disposition = FlarkV3SourcePromotionDisposition.stale,
      pathNodesVisited = 0,
      piecesAttached = 0,
      canonicalProof = null;

  final FlarkV3SourcePromotionDisposition disposition;
  final int pathNodesVisited;
  final int piecesAttached;
  final FlarkV3CanonicalSourcePromotionProof? canonicalProof;
}

enum FlarkV3SourceFactStageDisposition { staged, stale, rejected }

final class FlarkV3SourceFactStageReceipt {
  const FlarkV3SourceFactStageReceipt._({
    required this.disposition,
    required this.pieceCompleted,
    required this.checkpointsAccepted,
    required this.pathNodesVisited,
    required this.piecesAttached,
  });

  const FlarkV3SourceFactStageReceipt.stale()
    : disposition = FlarkV3SourceFactStageDisposition.stale,
      pieceCompleted = false,
      checkpointsAccepted = 0,
      pathNodesVisited = 0,
      piecesAttached = 0;

  const FlarkV3SourceFactStageReceipt.rejected()
    : disposition = FlarkV3SourceFactStageDisposition.rejected,
      pieceCompleted = false,
      checkpointsAccepted = 0,
      pathNodesVisited = 0,
      piecesAttached = 0;

  final FlarkV3SourceFactStageDisposition disposition;
  final bool pieceCompleted;
  final int checkpointsAccepted;
  final int pathNodesVisited;
  final int piecesAttached;
}

enum FlarkV3CanonicalSourceFactDeltaBeginDisposition {
  accepted,
  stale,
  rejected,
}

/// Result of authenticating and opening one exact-base SourceFacts splice.
final class FlarkV3CanonicalSourceFactDeltaBeginReceipt {
  const FlarkV3CanonicalSourceFactDeltaBeginReceipt._({
    required this.disposition,
    required this.reusedPageCount,
    required this.reusedCheckpointCount,
    required this.pathNodesAllocated,
  });

  const FlarkV3CanonicalSourceFactDeltaBeginReceipt.stale()
    : disposition = FlarkV3CanonicalSourceFactDeltaBeginDisposition.stale,
      reusedPageCount = 0,
      reusedCheckpointCount = 0,
      pathNodesAllocated = 0;

  const FlarkV3CanonicalSourceFactDeltaBeginReceipt.rejected()
    : disposition = FlarkV3CanonicalSourceFactDeltaBeginDisposition.rejected,
      reusedPageCount = 0,
      reusedCheckpointCount = 0,
      pathNodesAllocated = 0;

  final FlarkV3CanonicalSourceFactDeltaBeginDisposition disposition;
  final int reusedPageCount;
  final int reusedCheckpointCount;
  final int pathNodesAllocated;

  /// Opening a splice only retains immutable subtrees.
  int get checkpointFactsCopied => 0;
}

/// Promotion result for one exact-base canonical SourceFacts splice.
final class FlarkV3CanonicalSourceFactDeltaPromotionReceipt {
  const FlarkV3CanonicalSourceFactDeltaPromotionReceipt._({
    required this.disposition,
    required this.reusedPageCount,
    required this.reusedCheckpointCount,
    required this.transferredPageCount,
    required this.transferredCheckpointCount,
    required this.pathNodesAllocated,
    this.canonicalProof,
  });

  const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.stale()
    : disposition = FlarkV3SourcePromotionDisposition.stale,
      reusedPageCount = 0,
      reusedCheckpointCount = 0,
      transferredPageCount = 0,
      transferredCheckpointCount = 0,
      pathNodesAllocated = 0,
      canonicalProof = null;

  const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.rejected()
    : disposition = FlarkV3SourcePromotionDisposition.rejected,
      reusedPageCount = 0,
      reusedCheckpointCount = 0,
      transferredPageCount = 0,
      transferredCheckpointCount = 0,
      pathNodesAllocated = 0,
      canonicalProof = null;

  final FlarkV3SourcePromotionDisposition disposition;
  final int reusedPageCount;
  final int reusedCheckpointCount;
  final int transferredPageCount;
  final int transferredCheckpointCount;
  final int pathNodesAllocated;
  final FlarkV3CanonicalSourcePromotionProof? canonicalProof;

  /// Persistent splice promotion never flattens an unchanged checkpoint root.
  int get checkpointFactsCopied => 0;
}

final class FlarkV3SourceFactCancellationReceipt {
  const FlarkV3SourceFactCancellationReceipt._({
    required this.cancelled,
    required this.candidateRootsReleased,
    required this.pathNodesVisited,
  });

  final bool cancelled;
  final int candidateRootsReleased;

  /// Candidate release is pointer dropping, never a tree walk.
  final int pathNodesVisited;
}

final class FlarkV3SourceFactCandidateDiagnostics {
  const FlarkV3SourceFactCandidateDiagnostics({
    required this.candidateRootCount,
    required this.hasOpenPiece,
    required this.piecesAttached,
    required this.checkpointsAccepted,
    required this.pagesAccepted,
    required this.pathNodesVisited,
  });

  final int candidateRootCount;
  final bool hasOpenPiece;
  final int piecesAttached;
  final int checkpointsAccepted;
  final int pagesAccepted;
  final int pathNodesVisited;
}

final class _ActiveSourceFactCandidatePiece {
  _ActiveSourceFactCandidatePiece({
    required this.piece,
    required this.indexBuilder,
    required this.nextPiecePageOrdinal,
  });

  final FlarkV3SourcePieceToCertify piece;
  final _SourceRangeIndexBuilder indexBuilder;
  int nextPiecePageOrdinal;
}

sealed class _SourceFactCertificationCandidate {
  const _SourceFactCertificationCandidate({
    required this.lineage,
    required this.baseDocument,
    required this.observedReplica,
  });

  final FlarkV3SourceCertificationLineage lineage;
  final FlarkV3SourceDocument baseDocument;
  final FlarkV3ObservedSourceReplicaVersion observedReplica;
}

/// Compatibility-only candidate for the original Dart per-piece scanner.
///
/// Production native/Wasm certification uses
/// [_CanonicalSourceFactCertificationCandidate]; this shape remains solely so
/// existing source-substrate tests and migration callers do not become a
/// second live certification route.
final class _PieceSourceFactCertificationCandidate
    extends _SourceFactCertificationCandidate {
  _PieceSourceFactCertificationCandidate({
    required super.lineage,
    required super.baseDocument,
    required super.observedReplica,
  }) : document = baseDocument;

  FlarkV3SourceDocument document;
  _ActiveSourceFactCandidatePiece? activePiece;
  int expectedPageOrdinal = 0;
  int piecesAttached = 0;
  int checkpointsAccepted = 0;
  int pagesAccepted = 0;
  int pathNodesVisited = 0;
  FlarkV3ContentHash128 descriptorHash128 = FlarkV3ContentHash128.zero;
  FlarkV3ContentHash128 checkpointHash128 = FlarkV3ContentHash128.zero;
}

/// Hidden production candidate assembled only from canonical Rust facts.
final class _CanonicalSourceFactCertificationCandidate
    extends _SourceFactCertificationCandidate {
  _CanonicalSourceFactCertificationCandidate({
    required super.lineage,
    required super.baseDocument,
    required super.observedReplica,
    required this.checkpointSpacingUtf16,
    required this.checkpointCount,
    required this.pageCount,
  }) : indexBuilder = _SourceRangeIndexBuilder(
         start: 0,
         end: baseDocument.utf16Length,
         spacingUtf16: checkpointSpacingUtf16,
       );

  final int checkpointSpacingUtf16;
  final int checkpointCount;
  final int pageCount;
  final _SourceRangeIndexBuilder indexBuilder;
  int expectedPageOrdinal = 0;
  int checkpointsAccepted = 0;
  FlarkV3ContentHash128 checkpointHash128 = FlarkV3ContentHash128.zero;
  FlarkV3SourcePrefixFacts? lastCheckpoint;
}

/// Hidden exact-base candidate. Prefix and suffix roots are retained from the
/// installed base; only replacement pages are newly owned.
final class _CanonicalSourceFactDeltaCandidate
    extends _SourceFactCertificationCandidate {
  _CanonicalSourceFactDeltaCandidate({
    required super.lineage,
    required super.baseDocument,
    required super.observedReplica,
    required this.delta,
    required this.baseFacts,
    required this.prefixRoot,
    required this.suffixRoot,
    required this.removedCheckpointCount,
    required this.reusedPageCount,
    required this.reusedCheckpointCount,
    required this.pathNodesAllocated,
    required this.lastCheckpoint,
  });

  final FlarkV3CanonicalSourceFactDelta delta;
  final _CanonicalSourceFacts baseFacts;
  final _SourceCheckpointNode? prefixRoot;
  final _SourceCheckpointNode? suffixRoot;
  final int removedCheckpointCount;
  final int reusedPageCount;
  final int reusedCheckpointCount;
  int pathNodesAllocated;
  final _SourceCheckpointPageForestBuilder replacementBuilder =
      _SourceCheckpointPageForestBuilder();
  int expectedReplacementPageOrdinal = 0;
  int replacementCheckpointsAccepted = 0;
  FlarkV3ContentHash128 replacementCheckpointHash128 =
      FlarkV3ContentHash128.zero;
  FlarkV3SourcePrefixFacts lastCheckpoint;
}

final class FlarkV3SourceCertificationFailure implements Exception {
  const FlarkV3SourceCertificationFailure({
    required this.utf16Offset,
    this.lineage,
  });

  final int utf16Offset;
  final FlarkV3SourceCertificationLineage? lineage;

  @override
  String toString() =>
      'FlarkV3SourceCertificationFailure(utf16Offset: $utf16Offset)';
}

int _nextSourceSessionIdentity = 1;

/// Single-owner UI source state for the production-shaped v3 lane.
///
/// It owns exactly one current document root. Undo retains byte-charged inverse
/// source slices, never a previous document snapshot per edit. Worker
/// coordination is an ordered intent journal plus typed certification
/// receipts; neither is a second grammar or source authority.
final class FlarkV3SourceSession {
  FlarkV3SourceSession.fromString(
    String source, {
    int chunkSize = 4096,
    this.ordinaryOperationLimit = 8,
    this.ordinaryReplacementUtf16Limit = 8192,
    this.workerJournalEntryLimit = 1024,
    this.workerJournalOperationLimit = 4096,
    this.workerJournalRetainedPayloadByteLimit = 1024 * 1024,
    int historyEntryLimit = 2048,
    int historyByteLimit = 8 * 1024 * 1024,
  }) : _document = FlarkV3SourceDocument.fromString(
         source,
         chunkSize: chunkSize,
       ),
       _workerRevision = 0,
       _observedWorkerReplica = FlarkV3ObservedSourceReplicaVersion.empty,
       _history = _SourceInverseHistory(
         entryLimit: historyEntryLimit,
         byteLimit: historyByteLimit,
       ) {
    _validateWorkerSyncLimits();
    _lastCertifiedFingerprint = _document.fingerprint;
    _installWorkerSnapshotRebase(document: _document, throughIntentSequence: 0);
  }

  FlarkV3SourceSession.fromProvisionalString(
    String source, {
    int chunkSize = 4096,
    this.ordinaryOperationLimit = 8,
    this.ordinaryReplacementUtf16Limit = 8192,
    this.workerJournalEntryLimit = 1024,
    this.workerJournalOperationLimit = 4096,
    this.workerJournalRetainedPayloadByteLimit = 1024 * 1024,
    int historyEntryLimit = 2048,
    int historyByteLimit = 8 * 1024 * 1024,
  }) : _document = FlarkV3SourceDocument.fromProvisionalString(
         source,
         chunkSize: chunkSize,
       ),
       _workerRevision = 0,
       _observedWorkerReplica = FlarkV3ObservedSourceReplicaVersion.empty,
       _history = _SourceInverseHistory(
         entryLimit: historyEntryLimit,
         byteLimit: historyByteLimit,
       ) {
    _validateWorkerSyncLimits();
    _lastCertifiedFingerprint = const FlarkV3SourceFingerprint(
      revision: 0,
      utf16Length: 0,
      utf8Length: 0,
      contentHash128: FlarkV3ContentHash128.zero,
    );
    final throughIntentSequence = source.isEmpty ? 0 : _nextIntentSequence++;
    _installWorkerSnapshotRebase(
      document: _document,
      throughIntentSequence: throughIntentSequence,
    );
  }

  FlarkV3SourceDocument _document;
  final int sourceSessionIdentity = _nextSourceSessionIdentity++;
  int _workerRevision;
  FlarkV3ObservedSourceReplicaVersion _observedWorkerReplica;
  late FlarkV3SourceFingerprint _lastCertifiedFingerprint;
  final _SourceInverseHistory _history;
  final ListQueue<FlarkV3SourceIntent> _journal = ListQueue();
  int _journalPayloadUtf16 = 0;
  int _journalDeletedUtf16 = 0;
  int _journalOperationCount = 0;
  int _workerGeneration = 1;
  int _nextIntentSequence = 1;
  _WorkerSnapshotRebase? _snapshotRebase;
  _WorkerLiveSyncLease? _liveWorkerSyncLease;
  _WorkerLiveSyncLease? _drainingWorkerSyncLease;
  int _nextWorkerSyncLeaseId = 1;
  int _workerRebaseCount = 0;
  int _replacedWorkerSnapshotCount = 0;
  final int _snapshotInstallPathNodesVisited = 0;
  final int _snapshotInstallUtf16Copied = 0;
  int _workerPageUtf16Copied = 0;
  final Map<int, FlarkV3SourceCompactionObligation> _compactionObligations = {};
  final Map<int, Map<int, int>> _compactionLeases = {};
  int _nextCompactionLeaseId = 1;
  int _nextRequestId = 1;
  int? _latestRequestId;
  int? _latestRequestUiRevision;
  _SourceFactCertificationCandidate? _certificationCandidate;
  _CanonicalSourceFacts? _canonicalDeltaBaseFacts;

  final int ordinaryOperationLimit;
  final int ordinaryReplacementUtf16Limit;
  final int workerJournalEntryLimit;
  final int workerJournalOperationLimit;
  final int workerJournalRetainedPayloadByteLimit;

  FlarkV3SourceDocument get document => _document;
  int get uiRevision => _document.revision;
  int get workerRevision => _workerRevision;
  FlarkV3ObservedSourceReplicaVersion get observedWorkerReplica =>
      _observedWorkerReplica;
  FlarkV3SourceFingerprint get lastCertifiedFingerprint =>
      _lastCertifiedFingerprint;
  int get workerGeneration => _workerGeneration;
  bool get hasPendingCertification => !_document.hasCertifiedFacts;
  bool get hasActiveCertification => _certificationCandidate != null;
  FlarkV3CanonicalSourceFactAuthority?
  get installedCanonicalSourceFactAuthority =>
      _document._canonicalFacts?.authority;
  FlarkV3CanonicalSourceFactAuthority?
  get retainedCanonicalSourceFactDeltaBase =>
      _canonicalDeltaBaseFacts?.authority;

  /// Advances the reusable incremental base only after the matching
  /// structural root has committed.
  ///
  /// Source certification alone is insufficient: a superseded structural
  /// candidate must continue deriving from the last host-committed source
  /// facts, which may be older than the currently certified UI source.
  bool commitInstalledCanonicalSourceFactStructuralBase() {
    final installed = _document._canonicalFacts;
    if (installed == null) {
      _canonicalDeltaBaseFacts = null;
      return false;
    }
    if (installed.fingerprint != _lastCertifiedFingerprint) {
      throw StateError(
        'Structural SourceFacts base requires exact installed certification.',
      );
    }
    _canonicalDeltaBaseFacts = installed;
    return true;
  }

  bool get hasPendingWorkerSync =>
      _snapshotRebase != null || _journal.isNotEmpty;
  bool ownsWorkerSyncLease(int leaseId) =>
      _liveWorkerSyncLease?.lease.leaseId == leaseId ||
      _drainingWorkerSyncLease?.lease.leaseId == leaseId;
  bool get canUndo => _history.isNotEmpty;
  int get undoEntryCount => _history.length;
  int get undoRetainedUtf16Bytes => _history.byteCharge;
  FlarkV3SourceWorkerSyncDiagnostics get workerSyncDiagnostics =>
      FlarkV3SourceWorkerSyncDiagnostics(
        workerGeneration: _workerGeneration,
        nextIntentSequence: _nextIntentSequence,
        retainedJournalEntries: _journal.length,
        retainedJournalPayloadUtf16: _journalPayloadUtf16,
        retainedJournalDeletedUtf16: _journalDeletedUtf16,
        retainedJournalOperationCount: _journalOperationCount,
        retainedSnapshotRootCount: _snapshotRebase == null ? 0 : 1,
        retainedSnapshotBackingBytesUpperBound:
            _snapshotRebase
                ?.document
                ._root
                ?.retainedBackingUtf16BytesUpperBound ??
            0,
        liveLeaseCount:
            (_liveWorkerSyncLease == null ? 0 : 1) +
            (_drainingWorkerSyncLease == null ? 0 : 1),
        invalidatedLeaseAwaitingDrainCount: _drainingWorkerSyncLease == null
            ? 0
            : 1,
        rebaseCount: _workerRebaseCount,
        replacedSnapshotCount: _replacedWorkerSnapshotCount,
        snapshotInstallPathNodesVisited: _snapshotInstallPathNodesVisited,
        snapshotInstallUtf16Copied: _snapshotInstallUtf16Copied,
        pageUtf16Copied: _workerPageUtf16Copied,
      );
  List<FlarkV3SourceCompactionObligation> get pendingCompactionObligations =>
      List.unmodifiable(_compactionObligations.values);
  int get compactionRetainedBackingUtf16Bytes =>
      _compactionObligations.values.fold<int>(
        0,
        (total, obligation) => total + obligation.retainedBackingUtf16Bytes,
      );
  int get activeCompactionLeaseCount => _compactionLeases.length;
  FlarkV3SourceFactCandidateDiagnostics get certificationDiagnostics {
    final candidate = _certificationCandidate;
    final piece = candidate is _PieceSourceFactCertificationCandidate
        ? candidate
        : null;
    final canonical = candidate is _CanonicalSourceFactCertificationCandidate
        ? candidate
        : null;
    final delta = candidate is _CanonicalSourceFactDeltaCandidate
        ? candidate
        : null;
    return FlarkV3SourceFactCandidateDiagnostics(
      candidateRootCount: candidate == null ? 0 : 1,
      hasOpenPiece: piece?.activePiece != null,
      piecesAttached: piece?.piecesAttached ?? 0,
      checkpointsAccepted:
          piece?.checkpointsAccepted ??
          canonical?.checkpointsAccepted ??
          delta?.replacementCheckpointsAccepted ??
          0,
      pagesAccepted:
          piece?.pagesAccepted ??
          canonical?.expectedPageOrdinal ??
          delta?.expectedReplacementPageOrdinal ??
          0,
      pathNodesVisited:
          piece?.pathNodesVisited ?? delta?.pathNodesAllocated ?? 0,
    );
  }

  FlarkV3AcknowledgedSourceReplica acknowledgedSourceReplica() {
    if (!_observedReplicaMatchesCurrentSource() ||
        hasPendingWorkerSync ||
        _liveWorkerSyncLease != null ||
        _drainingWorkerSyncLease != null) {
      throw StateError('The worker has not observed this source root.');
    }
    return FlarkV3AcknowledgedSourceReplica._(
      sourceSessionIdentity: sourceSessionIdentity,
      workerGeneration: _workerGeneration,
      revision: _workerRevision,
      intentHighWater: _nextIntentSequence - 1,
      observedReplica: _observedWorkerReplica,
      document: _document,
    );
  }

  bool _observedReplicaMatchesCurrentSource() {
    final observed = _observedWorkerReplica;
    if (observed.revision != _document.revision ||
        observed.utf16Length != _document.utf16Length ||
        observed.intentHighWater != _nextIntentSequence - 1) {
      return false;
    }
    return !_document.hasCertifiedFacts ||
        observed.utf8Length == _document.utf8Length;
  }

  bool _invalidateCertificationCandidate() {
    final hadCandidate = _certificationCandidate != null;
    _certificationCandidate = null;
    _latestRequestId = null;
    _latestRequestUiRevision = null;
    return hadCandidate;
  }

  void _validateWorkerSyncLimits() {
    if (workerJournalEntryLimit < 1) {
      throw RangeError.range(
        workerJournalEntryLimit,
        1,
        null,
        'workerJournalEntryLimit',
      );
    }
    if (workerJournalRetainedPayloadByteLimit < 2) {
      throw RangeError.range(
        workerJournalRetainedPayloadByteLimit,
        2,
        null,
        'workerJournalRetainedPayloadByteLimit',
      );
    }
    if (workerJournalOperationLimit < 1) {
      throw RangeError.range(
        workerJournalOperationLimit,
        1,
        null,
        'workerJournalOperationLimit',
      );
    }
  }

  void _recordWorkerIntent(FlarkV3SourceIntent intent) {
    if (intent.workerGeneration != _workerGeneration ||
        intent.sequence <= 0 ||
        intent.uiRevision != _document.revision ||
        intent.baseStamp.revision != intent.baseUiRevision ||
        intent.targetStamp.revision != intent.uiRevision ||
        intent.targetStamp != _document.sourceStamp ||
        intent.uiRevision != intent.baseUiRevision + 1 ||
        intent.targetStamp.utf16Length !=
            intent.baseStamp.utf16Length -
                intent.deletedUtf16 +
                intent.payloadUtf16 ||
        (_journal.isNotEmpty &&
            _journal.last.targetStamp != intent.baseStamp)) {
      throw StateError('Worker intent does not bind the current UI source.');
    }
    final nextEntryCount = _journal.length + 1;
    final nextPayloadUtf16 = _journalPayloadUtf16 + intent.payloadUtf16;
    final nextDeletedUtf16 = _journalDeletedUtf16 + intent.deletedUtf16;
    final nextOperationCount =
        _journalOperationCount + intent.operations.length;
    final mustRebase =
        intent.payloadUtf16 > _maximumWorkerSyncPagePayloadUtf16 ||
        intent.operations.length > _maximumWorkerSyncPageOperations ||
        nextEntryCount > workerJournalEntryLimit ||
        nextOperationCount > workerJournalOperationLimit ||
        (nextPayloadUtf16 + nextDeletedUtf16) * 2 >
            workerJournalRetainedPayloadByteLimit;
    if (mustRebase) {
      _installWorkerSnapshotRebase(
        document: _document,
        throughIntentSequence: intent.sequence,
      );
      return;
    }
    _journal.addLast(intent);
    _journalPayloadUtf16 = nextPayloadUtf16;
    _journalDeletedUtf16 = nextDeletedUtf16;
    _journalOperationCount = nextOperationCount;
  }

  void _installWorkerSnapshotRebase({
    required FlarkV3SourceDocument document,
    required int throughIntentSequence,
  }) {
    if (_snapshotRebase != null) {
      _replacedWorkerSnapshotCount += 1;
    }
    _snapshotRebase = _WorkerSnapshotRebase(
      document: document,
      workerGeneration: _workerGeneration,
      throughIntentSequence: throughIntentSequence,
    );
    _journal.clear();
    _journalPayloadUtf16 = 0;
    _journalDeletedUtf16 = 0;
    _journalOperationCount = 0;
    if (_liveWorkerSyncLease != null) {
      if (_drainingWorkerSyncLease != null) {
        throw StateError('More than one worker request became in flight.');
      }
      _drainingWorkerSyncLease = _liveWorkerSyncLease;
      _liveWorkerSyncLease = null;
    }
    _workerRebaseCount += 1;
  }

  FlarkV3SourceWorkerSyncLease beginWorkerSync({
    int maximumEntries = _maximumWorkerSyncPageEntries,
    int maximumOperations = _maximumWorkerSyncPageOperations,
    int maximumPayloadUtf16 = _maximumWorkerSyncPagePayloadUtf16,
    int maximumSnapshotPageUtf16 = _maximumWorkerSyncPagePayloadUtf16,
  }) {
    if (_liveWorkerSyncLease != null || _drainingWorkerSyncLease != null) {
      throw StateError('A source-worker sync lease is already live.');
    }
    if (maximumEntries < 1 || maximumEntries > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        maximumEntries,
        1,
        _maximumWorkerSyncPageEntries,
        'maximumEntries',
      );
    }
    if (maximumPayloadUtf16 < 1 ||
        maximumPayloadUtf16 > _maximumWorkerSyncPagePayloadUtf16) {
      throw RangeError.range(
        maximumPayloadUtf16,
        1,
        _maximumWorkerSyncPagePayloadUtf16,
        'maximumPayloadUtf16',
      );
    }
    if (maximumOperations < 1 ||
        maximumOperations > _maximumWorkerSyncPageOperations) {
      throw RangeError.range(
        maximumOperations,
        1,
        _maximumWorkerSyncPageOperations,
        'maximumOperations',
      );
    }
    if (maximumSnapshotPageUtf16 < 2 ||
        maximumSnapshotPageUtf16 > _maximumWorkerSyncPagePayloadUtf16) {
      throw RangeError.range(
        maximumSnapshotPageUtf16,
        2,
        _maximumWorkerSyncPagePayloadUtf16,
        'maximumSnapshotPageUtf16',
      );
    }

    final snapshot = _snapshotRebase;
    if (snapshot != null) {
      final start = snapshot.acknowledgedUtf16;
      var end = math.min(
        snapshot.document.utf16Length,
        start + maximumSnapshotPageUtf16,
      );
      if (end < snapshot.document.utf16Length) {
        final previous = _codeUnitAt(snapshot.document._root!, end - 1);
        final next = _codeUnitAt(snapshot.document._root!, end);
        if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
          end -= 1;
        }
      }
      final source = snapshot.document.readRange(start, end);
      _workerPageUtf16Copied += source.length;
      final lease = FlarkV3SourceSnapshotSyncLease._(
        sourceSessionIdentity: sourceSessionIdentity,
        leaseId: _nextWorkerSyncLeaseId++,
        workerGeneration: _workerGeneration,
        baseUiRevision: snapshot.document.revision,
        startUtf16: start,
        endUtf16: end,
        totalUtf16Length: snapshot.document.utf16Length,
        throughIntentSequence: snapshot.throughIntentSequence,
        targetStamp: snapshot.document.sourceStamp,
        source: source,
      );
      _liveWorkerSyncLease = _WorkerLiveSyncLease(lease);
      return lease;
    }

    if (_journal.isEmpty) {
      throw StateError('The source worker is already synchronized.');
    }
    final selected = <FlarkV3SourceIntent>[];
    var payloadUtf16 = 0;
    var operationCount = 0;
    var expectedBaseUiRevision = _workerRevision;
    FlarkV3SourceStamp? priorTargetStamp;
    for (final intent in _journal) {
      if (intent.workerGeneration != _workerGeneration) {
        throw StateError('Journal contains an intent from a stale worker.');
      }
      if (intent.baseUiRevision != expectedBaseUiRevision ||
          intent.uiRevision <= intent.baseUiRevision ||
          (selected.isEmpty &&
              (intent.sequence != _observedWorkerReplica.intentHighWater + 1 ||
                  !_observationMatchesTarget(
                    _observedWorkerReplica,
                    intent.baseStamp,
                    intentHighWater: intent.sequence - 1,
                  ))) ||
          (priorTargetStamp != null && intent.baseStamp != priorTargetStamp)) {
        throw StateError('Journal does not form one exact UI revision chain.');
      }
      if (selected.length == maximumEntries) break;
      final nextOperationCount = operationCount + intent.operations.length;
      if (nextOperationCount > maximumOperations) {
        if (selected.isEmpty) {
          _installWorkerSnapshotRebase(
            document: _document,
            throughIntentSequence: _journal.last.sequence,
          );
          return beginWorkerSync(
            maximumEntries: maximumEntries,
            maximumOperations: maximumOperations,
            maximumPayloadUtf16: maximumPayloadUtf16,
            maximumSnapshotPageUtf16: maximumSnapshotPageUtf16,
          );
        }
        break;
      }
      final nextPayload = payloadUtf16 + intent.payloadUtf16;
      if (nextPayload > maximumPayloadUtf16) {
        if (selected.isEmpty) {
          _installWorkerSnapshotRebase(
            document: _document,
            throughIntentSequence: _journal.last.sequence,
          );
          return beginWorkerSync(
            maximumEntries: maximumEntries,
            maximumOperations: maximumOperations,
            maximumPayloadUtf16: maximumPayloadUtf16,
            maximumSnapshotPageUtf16: maximumSnapshotPageUtf16,
          );
        }
        break;
      }
      selected.add(intent);
      payloadUtf16 = nextPayload;
      operationCount = nextOperationCount;
      expectedBaseUiRevision = intent.uiRevision;
      priorTargetStamp = intent.targetStamp;
    }
    final lease = FlarkV3SourceIntentSyncLease._(
      sourceSessionIdentity: sourceSessionIdentity,
      leaseId: _nextWorkerSyncLeaseId++,
      workerGeneration: _workerGeneration,
      intents: selected,
      payloadUtf16: payloadUtf16,
    );
    _liveWorkerSyncLease = _WorkerLiveSyncLease(lease);
    return lease;
  }

  bool _acknowledgementBindsLease(
    FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
    FlarkV3SourceWorkerSyncLease lease,
  ) {
    if (acknowledgement.sourceSessionIdentity != sourceSessionIdentity ||
        lease.sourceSessionIdentity != sourceSessionIdentity ||
        acknowledgement.leaseId != lease.leaseId ||
        acknowledgement.workerGeneration != lease.workerGeneration ||
        acknowledgement.kind != lease.kind) {
      return false;
    }
    if (lease is FlarkV3SourceSnapshotSyncLease &&
        acknowledgement is FlarkV3SourceSnapshotSyncAcknowledgement) {
      return acknowledgement.baseUiRevision == lease.baseUiRevision &&
          acknowledgement.startUtf16 == lease.startUtf16 &&
          acknowledgement.endUtf16 == lease.endUtf16 &&
          acknowledgement.throughIntentSequence == lease.throughIntentSequence;
    }
    if (lease is FlarkV3SourceIntentSyncLease &&
        acknowledgement is FlarkV3SourceIntentSyncAcknowledgement) {
      return acknowledgement.firstSequence == lease.firstSequence &&
          acknowledgement.lastSequence == lease.lastSequence &&
          acknowledgement.entryCount == lease.intents.length &&
          acknowledgement.payloadUtf16 == lease.payloadUtf16;
    }
    return false;
  }

  bool _observationMatchesTarget(
    FlarkV3ObservedSourceReplicaVersion observed,
    FlarkV3SourceStamp target, {
    required int intentHighWater,
  }) {
    if (observed.revision < 0 ||
        observed.utf16Length < 0 ||
        observed.utf8Length < 0 ||
        observed.intentHighWater < 0 ||
        observed.revision != target.revision ||
        observed.utf16Length != target.utf16Length ||
        observed.intentHighWater != intentHighWater) {
      return false;
    }
    return target is! FlarkV3KnownSourceStamp ||
        observed.utf8Length == target.utf8Length;
  }

  bool _malformedAcknowledgementTargetsLiveLease(
    FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
    FlarkV3SourceWorkerSyncLease live,
  ) =>
      acknowledgement.sourceSessionIdentity == sourceSessionIdentity &&
      acknowledgement.workerGeneration == _workerGeneration &&
      acknowledgement.leaseId == live.leaseId;

  FlarkV3SourceWorkerSyncAckReceipt _poisonAndReseedWorkerReplica() {
    _invalidateCertificationCandidate();
    _canonicalDeltaBaseFacts = null;
    _liveWorkerSyncLease = null;
    _installWorkerSnapshotRebase(
      document: _document,
      throughIntentSequence: _nextIntentSequence - 1,
    );
    return FlarkV3SourceWorkerSyncAckReceipt.stale(
      workerRevision: _workerRevision,
    );
  }

  FlarkV3SourceWorkerSyncAckReceipt acknowledgeWorkerSync(
    FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
  ) {
    final draining = _drainingWorkerSyncLease?.lease;
    if (draining != null) {
      if (_acknowledgementBindsLease(acknowledgement, draining)) {
        _drainingWorkerSyncLease = null;
      }
      return FlarkV3SourceWorkerSyncAckReceipt.stale(
        workerRevision: _workerRevision,
      );
    }
    final live = _liveWorkerSyncLease?.lease;
    if (live == null) {
      return FlarkV3SourceWorkerSyncAckReceipt.stale(
        workerRevision: _workerRevision,
      );
    }
    if (!_acknowledgementBindsLease(acknowledgement, live)) {
      return _malformedAcknowledgementTargetsLiveLease(acknowledgement, live)
          ? _poisonAndReseedWorkerReplica()
          : FlarkV3SourceWorkerSyncAckReceipt.stale(
              workerRevision: _workerRevision,
            );
    }

    if (live is FlarkV3SourceSnapshotSyncLease &&
        acknowledgement is FlarkV3SourceSnapshotSyncAcknowledgement) {
      final snapshot = _snapshotRebase;
      if (snapshot == null ||
          snapshot.workerGeneration != _workerGeneration ||
          acknowledgement.baseUiRevision != live.baseUiRevision ||
          acknowledgement.startUtf16 != live.startUtf16 ||
          acknowledgement.endUtf16 != live.endUtf16 ||
          acknowledgement.throughIntentSequence != live.throughIntentSequence ||
          snapshot.throughIntentSequence != live.throughIntentSequence ||
          snapshot.document.revision != live.baseUiRevision ||
          snapshot.document.sourceStamp != live.targetStamp ||
          snapshot.acknowledgedUtf16 != live.startUtf16 ||
          (!live.isLast && acknowledgement.observedReplica != null) ||
          (live.isLast &&
              (acknowledgement.observedReplica == null ||
                  !_observationMatchesTarget(
                    acknowledgement.observedReplica!,
                    live.targetStamp,
                    intentHighWater: live.throughIntentSequence,
                  )))) {
        return _poisonAndReseedWorkerReplica();
      }
      snapshot.acknowledgedUtf16 = live.endUtf16;
      _liveWorkerSyncLease = null;
      if (live.isLast) {
        final observed = acknowledgement.observedReplica!;
        _observedWorkerReplica = observed;
        _workerRevision = observed.revision;
        _snapshotRebase = null;
      }
      return FlarkV3SourceWorkerSyncAckReceipt.acknowledged(
        droppedIntentEntries: 0,
        droppedPayloadUtf16: 0,
        droppedDeletedUtf16: 0,
        droppedOperationCount: 0,
        workerRevision: _workerRevision,
      );
    }

    if (live is FlarkV3SourceIntentSyncLease &&
        acknowledgement is FlarkV3SourceIntentSyncAcknowledgement) {
      if (acknowledgement.firstSequence != live.firstSequence ||
          acknowledgement.lastSequence != live.lastSequence ||
          acknowledgement.entryCount != live.intents.length ||
          acknowledgement.payloadUtf16 != live.payloadUtf16 ||
          _journal.length < live.intents.length ||
          !_observationMatchesTarget(
            acknowledgement.observedReplica,
            live.targetStamp,
            intentHighWater: live.lastSequence,
          )) {
        return _poisonAndReseedWorkerReplica();
      }
      var index = 0;
      for (final intent in _journal) {
        if (index == live.intents.length) break;
        if (intent.sequence != live.intents[index].sequence ||
            intent.workerGeneration != live.intents[index].workerGeneration ||
            intent.baseStamp != live.intents[index].baseStamp ||
            intent.targetStamp != live.intents[index].targetStamp) {
          return _poisonAndReseedWorkerReplica();
        }
        index += 1;
      }
      var droppedPayloadUtf16 = 0;
      var droppedDeletedUtf16 = 0;
      var droppedOperationCount = 0;
      var nextWorkerRevision = _workerRevision;
      for (var removed = 0; removed < live.intents.length; removed += 1) {
        final intent = _journal.removeFirst();
        droppedPayloadUtf16 += intent.payloadUtf16;
        droppedDeletedUtf16 += intent.deletedUtf16;
        droppedOperationCount += intent.operations.length;
        nextWorkerRevision = intent.uiRevision;
      }
      _journalPayloadUtf16 -= droppedPayloadUtf16;
      _journalDeletedUtf16 -= droppedDeletedUtf16;
      _journalOperationCount -= droppedOperationCount;
      if (nextWorkerRevision != acknowledgement.observedReplica.revision) {
        return _poisonAndReseedWorkerReplica();
      }
      _observedWorkerReplica = acknowledgement.observedReplica;
      _workerRevision = acknowledgement.observedReplica.revision;
      _liveWorkerSyncLease = null;
      return FlarkV3SourceWorkerSyncAckReceipt.acknowledged(
        droppedIntentEntries: live.intents.length,
        droppedPayloadUtf16: droppedPayloadUtf16,
        droppedDeletedUtf16: droppedDeletedUtf16,
        droppedOperationCount: droppedOperationCount,
        workerRevision: _workerRevision,
      );
    }

    return FlarkV3SourceWorkerSyncAckReceipt.stale(
      workerRevision: _workerRevision,
    );
  }

  bool releaseWorkerSyncLease(int leaseId) {
    if (_liveWorkerSyncLease?.lease.leaseId == leaseId) {
      _liveWorkerSyncLease = null;
      return true;
    }
    if (_drainingWorkerSyncLease?.lease.leaseId == leaseId) {
      _drainingWorkerSyncLease = null;
      return true;
    }
    return false;
  }

  /// Starts a new replica generation after the prior worker has terminated.
  ///
  /// Observed termination is the transport proof that no prior request can
  /// remain physically in flight. Ordinary rebase never uses this shortcut;
  /// it retains an invalidated draining ticket until ACK or cancellation.
  int restartWorker() {
    _invalidateCertificationCandidate();
    _canonicalDeltaBaseFacts = null;
    _workerGeneration += 1;
    _workerRevision = 0;
    _observedWorkerReplica = FlarkV3ObservedSourceReplicaVersion.empty;
    _liveWorkerSyncLease = null;
    _drainingWorkerSyncLease = null;
    _installWorkerSnapshotRebase(
      document: _document,
      throughIntentSequence: _nextIntentSequence - 1,
    );
    return _workerGeneration;
  }

  FlarkV3SourceCompactionLease takeCompactionObligations({int maximum = 16}) {
    if (maximum < 1) {
      throw RangeError.range(maximum, 1, null, 'maximum');
    }
    _refreshCompactionBlocks();
    final alreadyLeased = <int>{
      for (final lease in _compactionLeases.values) ...lease.keys,
    };
    final selected = <FlarkV3SourceCompactionObligation>[];
    for (final obligation in _compactionObligations.values) {
      if (obligation.blockedByUndoLease ||
          alreadyLeased.contains(obligation.backingIdentity)) {
        continue;
      }
      selected.add(obligation);
      if (selected.length == maximum) break;
    }
    if (selected.isEmpty) {
      return FlarkV3SourceCompactionLease._(leaseId: 0, obligations: const []);
    }
    final leaseId = _nextCompactionLeaseId++;
    _compactionLeases[leaseId] = {
      for (final obligation in selected)
        obligation.backingIdentity: obligation.sourceRevision,
    };
    return FlarkV3SourceCompactionLease._(
      leaseId: leaseId,
      obligations: selected,
    );
  }

  /// Transfers the selected debt to a durable scheduler.
  ///
  /// This does not claim compaction has completed. The scheduler still has to
  /// revalidate [FlarkV3SourceCompactionObligation.sourceRevision] before
  /// publishing a result. Releasing instead of acknowledging keeps the debt.
  void acknowledgeCompactionLease(int leaseId) {
    final leased = _compactionLeases.remove(leaseId);
    if (leased == null) throw StateError('Unknown compaction lease $leaseId.');
    for (final entry in leased.entries) {
      final current = _compactionObligations[entry.key];
      if (current?.sourceRevision == entry.value) {
        _compactionObligations.remove(entry.key);
      }
    }
  }

  void releaseCompactionLease(int leaseId) {
    if (_compactionLeases.remove(leaseId) == null) {
      throw StateError('Unknown compaction lease $leaseId.');
    }
  }

  void _refreshCompactionBlocks() {
    for (final entry in _compactionObligations.entries.toList()) {
      _compactionObligations[entry.key] = entry.value._withBlocked(
        _history.mayRetainBacking(entry.key),
      );
    }
  }

  FlarkV3SourceSessionApplyReceipt apply(FlarkV3SourceTransaction transaction) {
    var replacementUtf16 = 0;
    for (final operation in transaction.operations) {
      replacementUtf16 += operation.replacement.length;
    }
    final provisionalRoute =
        !_document.isFullyIndexed ||
        transaction.operations.length > ordinaryOperationLimit ||
        replacementUtf16 > ordinaryReplacementUtf16Limit;
    final before = _document;
    final beforeStamp = before.sourceStamp;
    final applied = before._apply(
      transaction,
      provisionalRoute: provisionalRoute,
    );
    if (!applied.changed) {
      return FlarkV3SourceSessionApplyReceipt(
        changed: false,
        provisional: false,
        parserBatch: null,
        sourceWork: applied.sourceWork,
        inverseLeasePathNodesVisited: 0,
      );
    }
    _invalidateCertificationCandidate();
    final inverse = _captureInverseTransaction(
      before,
      transaction.operations,
      applied.document.revision,
    );
    _history.push(inverse);
    _document = applied.document;
    if (_document.isFullyIndexed) {
      _lastCertifiedFingerprint = _document.fingerprint;
    }
    for (final obligation in applied.compactionObligations) {
      _compactionObligations[obligation.backingIdentity] = obligation;
    }
    _refreshCompactionBlocks();
    final orderedWorkerOperations = <_IndexedSourceEdit>[
      for (var index = 0; index < transaction.operations.length; index += 1)
        _IndexedSourceEdit(index, transaction.operations[index]),
    ]..sort(_compareIndexedOperations);
    _recordWorkerIntent(
      FlarkV3SourceIntent(
        workerGeneration: _workerGeneration,
        sequence: _nextIntentSequence++,
        baseUiRevision: transaction.baseRevision,
        uiRevision: _document.revision,
        baseStamp: beforeStamp,
        targetStamp: _document.sourceStamp,
        operations: [
          for (final indexed in orderedWorkerOperations)
            FlarkV3SourceIntentEdit(
              startUtf16: indexed.operation.startUtf16,
              endUtf16: indexed.operation.endUtf16,
              replacement: FlarkV3StringSourcePayload(
                indexed.operation.replacement,
              ),
            ),
        ],
      ),
    );
    return FlarkV3SourceSessionApplyReceipt(
      changed: true,
      provisional: provisionalRoute || !_document.isFullyIndexed,
      parserBatch: applied.parserBatch,
      sourceWork: applied.sourceWork,
      inverseLeasePathNodesVisited: inverse.capturePathNodesVisited,
    );
  }

  FlarkV3SourceSessionApplyReceipt? undo() {
    final inverse = _history.pop();
    if (inverse == null) return null;
    _invalidateCertificationCandidate();
    _refreshCompactionBlocks();
    final beforeRevision = _document.revision;
    final beforeStamp = _document.sourceStamp;
    var nextRoot = _document._root;
    var nextPieceId = _document._nextPieceId;
    int allocateProvisionalPieceId() => nextPieceId++;
    for (final operation in inverse.operations.reversed) {
      final first = _split(
        nextRoot,
        operation.startUtf16,
        _document._chunkSize,
      );
      final second = _split(
        first.right,
        operation.endUtf16 - operation.startUtf16,
        _document._chunkSize,
      );
      nextRoot = _concat(
        _concat(
          first.left,
          operation.replacement.buildTree(),
          _document._chunkSize,
          allocateProvisionalPieceId: allocateProvisionalPieceId,
        ),
        second.right,
        _document._chunkSize,
        allocateProvisionalPieceId: allocateProvisionalPieceId,
      );
    }
    _document = FlarkV3SourceDocument._(
      root: nextRoot,
      revision: beforeRevision + 1,
      chunkSize: _document._chunkSize,
      nextPieceId: nextPieceId,
    );
    if (_document.isFullyIndexed) {
      _lastCertifiedFingerprint = _document.fingerprint;
    }
    _recordWorkerIntent(
      FlarkV3SourceIntent(
        workerGeneration: _workerGeneration,
        sequence: _nextIntentSequence++,
        baseUiRevision: beforeRevision,
        uiRevision: _document.revision,
        baseStamp: beforeStamp,
        targetStamp: _document.sourceStamp,
        operations: [
          for (final operation in inverse.operations)
            FlarkV3SourceIntentEdit(
              startUtf16: operation.startUtf16,
              endUtf16: operation.endUtf16,
              replacement: operation.replacement,
            ),
        ],
      ),
    );
    return FlarkV3SourceSessionApplyReceipt(
      changed: true,
      provisional: !_document.isFullyIndexed,
      parserBatch: null,
      sourceWork: const FlarkV3SourceWorkReceipt(
        noOpComparedUtf16: 0,
        replacementUtf8BytesEncoded: 0,
        replacementChunksEncoded: 0,
      ),
      inverseLeasePathNodesVisited: 0,
    );
  }

  FlarkV3SourceCertificationRequest beginCertification({
    int maximumPieceDescriptors = 64,
    int maximumDiscoveryNodes = 512,
  }) {
    if (!hasPendingCertification) {
      throw StateError('The source has no pending derived facts.');
    }
    if (!_observedReplicaMatchesCurrentSource() ||
        hasPendingWorkerSync ||
        _liveWorkerSyncLease != null ||
        _drainingWorkerSyncLease != null) {
      throw StateError(
        'Source facts require an exact installed worker replica first.',
      );
    }
    if (maximumPieceDescriptors < 1 ||
        maximumPieceDescriptors > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        maximumPieceDescriptors,
        1,
        _maximumWorkerSyncPageEntries,
        'maximumPieceDescriptors',
      );
    }
    if (maximumDiscoveryNodes > _maximumSourceFactAdoptionPathNodes) {
      throw RangeError.range(
        maximumDiscoveryNodes,
        1,
        _maximumSourceFactAdoptionPathNodes,
        'maximumDiscoveryNodes',
      );
    }
    _invalidateCertificationCandidate();
    final requestId = _nextRequestId++;
    _latestRequestId = requestId;
    _latestRequestUiRevision = _document.revision;
    final lineage = FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: sourceSessionIdentity,
      requestId: requestId,
      workerGeneration: _workerGeneration,
      workerReplicaRevision: _workerRevision,
      uiRevision: _document.revision,
      utf16Length: _document.utf16Length,
      intentHighWater: _nextIntentSequence - 1,
    );
    _certificationCandidate = _PieceSourceFactCertificationCandidate(
      lineage: lineage,
      baseDocument: _document,
      observedReplica: _observedWorkerReplica,
    );
    return FlarkV3SourceCertificationRequest._(
      lineage: lineage,
      sourceRoot: _document._root,
      firstPiecePage: _document._pendingPiecePage(
        cursorUtf16: 0,
        maximumPieces: maximumPieceDescriptors,
        maximumNodes: maximumDiscoveryNodes,
      ),
    );
  }

  FlarkV3SourcePendingPiecePage continueCertificationPieces({
    required int requestId,
    required int cursorUtf16,
    int maximumPieceDescriptors = 64,
    int maximumDiscoveryNodes = 512,
  }) {
    _checkLiveCertificationRequest(requestId);
    if (maximumPieceDescriptors < 1 ||
        maximumPieceDescriptors > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        maximumPieceDescriptors,
        1,
        _maximumWorkerSyncPageEntries,
        'maximumPieceDescriptors',
      );
    }
    if (maximumDiscoveryNodes > _maximumSourceFactAdoptionPathNodes) {
      throw RangeError.range(
        maximumDiscoveryNodes,
        1,
        _maximumSourceFactAdoptionPathNodes,
        'maximumDiscoveryNodes',
      );
    }
    return _document._pendingPiecePage(
      cursorUtf16: cursorUtf16,
      maximumPieces: maximumPieceDescriptors,
      maximumNodes: maximumDiscoveryNodes,
    );
  }

  void _checkLiveCertificationRequest(int requestId) {
    if (requestId != _latestRequestId ||
        _latestRequestUiRevision != _document.revision ||
        _certificationCandidate?.lineage.requestId != requestId ||
        _certificationCandidate?.lineage.workerGeneration !=
            _workerGeneration) {
      throw StateError('Certification page request is stale.');
    }
  }

  bool _canonicalLineageBindsLiveSource(
    FlarkV3SourceCertificationLineage lineage,
  ) =>
      lineage.sourceSessionIdentity == sourceSessionIdentity &&
      lineage.requestId > 0 &&
      lineage.workerGeneration == _workerGeneration &&
      lineage.workerReplicaRevision == _observedWorkerReplica.revision &&
      lineage.uiRevision == _document.revision &&
      lineage.utf16Length == _document.utf16Length &&
      lineage.intentHighWater == _observedWorkerReplica.intentHighWater &&
      lineage.intentHighWater == _nextIntentSequence - 1 &&
      _observedReplicaMatchesCurrentSource() &&
      !hasPendingWorkerSync &&
      _liveWorkerSyncLease == null &&
      _drainingWorkerSyncLease == null;

  bool _validCanonicalShape({
    required int checkpointSpacingUtf16,
    required int checkpointCount,
    required int pageCount,
  }) {
    if (checkpointSpacingUtf16 < 2 ||
        checkpointSpacingUtf16 > _maximumSourceChunkUtf16 ||
        checkpointCount < 0 ||
        pageCount < 0) {
      return false;
    }
    final maximumCheckpoints = (_document.utf16Length + 1) >> 1;
    if (checkpointCount > maximumCheckpoints) return false;
    if (_document.utf16Length == 0) {
      return checkpointCount == 0 && pageCount == 0;
    }
    return checkpointCount > 0 &&
        pageCount ==
            (checkpointCount + _maximumWorkerSyncPageEntries - 1) ~/
                _maximumWorkerSyncPageEntries;
  }

  /// Stages one bounded canonical page from the runtime-owned Rust scan.
  ///
  /// This path is global by design: it neither discovers Dart rope pieces nor
  /// scans source text. A different lineage is stale and cannot disturb the
  /// one live candidate.
  FlarkV3SourceFactStageReceipt stageCanonicalSourceFactCheckpointPage(
    FlarkV3CanonicalSourceFactCheckpointPage page,
  ) {
    if (!_canonicalLineageBindsLiveSource(page.lineage)) {
      return const FlarkV3SourceFactStageReceipt.stale();
    }
    var candidate = _certificationCandidate;
    if (candidate == null) {
      if (!_validCanonicalShape(
        checkpointSpacingUtf16: page.checkpointSpacingUtf16,
        checkpointCount: page.checkpointCount,
        pageCount: page.pageCount,
      )) {
        return const FlarkV3SourceFactStageReceipt.rejected();
      }
      candidate = _CanonicalSourceFactCertificationCandidate(
        lineage: page.lineage,
        baseDocument: _document,
        observedReplica: _observedWorkerReplica,
        checkpointSpacingUtf16: page.checkpointSpacingUtf16,
        checkpointCount: page.checkpointCount,
        pageCount: page.pageCount,
      );
      _certificationCandidate = candidate;
      _latestRequestId = page.lineage.requestId;
      _latestRequestUiRevision = page.lineage.uiRevision;
    }
    if (candidate is! _CanonicalSourceFactCertificationCandidate ||
        page.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument)) {
      return const FlarkV3SourceFactStageReceipt.stale();
    }

    final expectedPageCheckpointCount = math.min(
      _maximumWorkerSyncPageEntries,
      candidate.checkpointCount -
          candidate.expectedPageOrdinal * _maximumWorkerSyncPageEntries,
    );
    if (page.isConsumed ||
        page.pageOrdinal != candidate.expectedPageOrdinal ||
        page.pageCount != candidate.pageCount ||
        page.checkpointCount != candidate.checkpointCount ||
        page.checkpointSpacingUtf16 != candidate.checkpointSpacingUtf16 ||
        page.pageCheckpointCount != expectedPageCheckpointCount ||
        expectedPageCheckpointCount <= 0) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }

    var prior = candidate.lastCheckpoint;
    for (final fact in page.checkpoints) {
      final priorUtf16 = prior?.utf16Offset ?? 0;
      final priorUtf8 = prior?.utf8Offset ?? 0;
      final priorNewlines = prior?.newlines ?? 0;
      final gap = fact.utf16Offset - priorUtf16;
      if (gap <= 0 ||
          gap > candidate.checkpointSpacingUtf16 + 1 ||
          fact.utf16Offset > candidate.lineage.utf16Length ||
          fact.utf8Offset <= priorUtf8 ||
          fact.utf8Offset > candidate.observedReplica.utf8Length ||
          fact.newlines < priorNewlines ||
          fact.newlines > fact.utf16Offset) {
        _invalidateCertificationCandidate();
        return const FlarkV3SourceFactStageReceipt.rejected();
      }
      prior = fact;
    }
    final finalPage = page.pageOrdinal + 1 == page.pageCount;
    final last = prior!;
    if (finalPage !=
        (last.utf16Offset == candidate.lineage.utf16Length &&
            last.utf8Offset == candidate.observedReplica.utf8Length)) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }

    final facts = page.checkpoints;
    var checkpointHash = candidate.checkpointHash128;
    for (final fact in facts) {
      checkpointHash = _appendSourcePrefixFactsHash(checkpointHash, fact);
    }
    try {
      candidate.indexBuilder.addOwnedPage(page._takeCheckpoints());
    } on Object {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }
    candidate.checkpointHash128 = checkpointHash;
    candidate.lastCheckpoint = last;
    candidate.checkpointsAccepted += facts.length;
    candidate.expectedPageOrdinal += 1;
    return FlarkV3SourceFactStageReceipt._(
      disposition: FlarkV3SourceFactStageDisposition.staged,
      pieceCompleted: finalPage,
      checkpointsAccepted: facts.length,
      pathNodesVisited: 0,
      piecesAttached: 0,
    );
  }

  /// Atomically promotes one completely staged canonical fact root.
  FlarkV3SourcePromotionReceipt commitCanonicalSourceFactCertification(
    FlarkV3CanonicalSourceFactCompletion completion,
  ) {
    if (!_canonicalLineageBindsLiveSource(completion.lineage)) {
      return const FlarkV3SourcePromotionReceipt.stale();
    }
    var candidate = _certificationCandidate;
    if (candidate == null &&
        _validCanonicalShape(
          checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
          checkpointCount: completion.checkpointCount,
          pageCount: completion.pageCount,
        ) &&
        completion.pageCount == 0) {
      candidate = _CanonicalSourceFactCertificationCandidate(
        lineage: completion.lineage,
        baseDocument: _document,
        observedReplica: _observedWorkerReplica,
        checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
        checkpointCount: completion.checkpointCount,
        pageCount: completion.pageCount,
      );
      _certificationCandidate = candidate;
    }
    if (candidate is! _CanonicalSourceFactCertificationCandidate ||
        completion.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument)) {
      return const FlarkV3SourcePromotionReceipt.stale();
    }

    final last = candidate.lastCheckpoint;
    final empty = _document.utf16Length == 0;
    final terminalMatches = empty
        ? last == null &&
              completion.fingerprint.utf8Length == 0 &&
              completion.logicalLineBreaks == 0 &&
              completion.fingerprint.contentHash128 ==
                  FlarkV3ContentHash128.zero
        : last != null &&
              last.utf16Offset == _document.utf16Length &&
              last.utf8Offset == completion.fingerprint.utf8Length &&
              last.newlines == completion.logicalLineBreaks &&
              last.hash == completion.fingerprint.contentHash128;
    final valid =
        completion.fingerprintAlgorithm == 1 &&
        completion.checkpointSpacingUtf16 == candidate.checkpointSpacingUtf16 &&
        completion.checkpointCount == candidate.checkpointCount &&
        completion.pageCount == candidate.pageCount &&
        candidate.expectedPageOrdinal == candidate.pageCount &&
        candidate.checkpointsAccepted == candidate.checkpointCount &&
        completion.checkpointHash128 == candidate.checkpointHash128 &&
        completion.fingerprint.revision == _document.revision &&
        completion.fingerprint.utf16Length == _document.utf16Length &&
        completion.fingerprint.utf8Length ==
            candidate.observedReplica.utf8Length &&
        completion.logicalLineBreaks >= 0 &&
        completion.logicalLineBreaks <= _document.utf16Length &&
        terminalMatches;
    if (!valid) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourcePromotionReceipt._(
        disposition: FlarkV3SourcePromotionDisposition.rejected,
        pathNodesVisited: 0,
        piecesAttached: 0,
      );
    }

    late final _SourceRangeIndex index;
    try {
      index = candidate.indexBuilder._seal();
    } on Object {
      _invalidateCertificationCandidate();
      return const FlarkV3SourcePromotionReceipt._(
        disposition: FlarkV3SourcePromotionDisposition.rejected,
        pathNodesVisited: 0,
        piecesAttached: 0,
      );
    }
    final authority = FlarkV3CanonicalSourceFactAuthority._(
      sourceSessionIdentity: sourceSessionIdentity,
      workerGeneration: completion.lineage.workerGeneration,
      workerReplicaRevision: completion.lineage.workerReplicaRevision,
      intentHighWater: completion.lineage.intentHighWater,
      fingerprintAlgorithm: completion.fingerprintAlgorithm,
      fingerprint: completion.fingerprint,
      logicalLineBreaks: completion.logicalLineBreaks,
      checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
      checkpointCount: completion.checkpointCount,
      pageCount: completion.pageCount,
      checkpointHash128: completion.checkpointHash128,
    );
    final proof = FlarkV3CanonicalSourcePromotionProof(
      lineage: completion.lineage,
      fingerprintAlgorithm: completion.fingerprintAlgorithm,
      fingerprint: completion.fingerprint,
      logicalLineBreaks: completion.logicalLineBreaks,
      checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
      checkpointCount: completion.checkpointCount,
      pageCount: completion.pageCount,
      checkpointHash128: completion.checkpointHash128,
    );
    _document = FlarkV3SourceDocument._(
      root: candidate.baseDocument._root,
      revision: candidate.baseDocument.revision,
      chunkSize: candidate.baseDocument._chunkSize,
      nextPieceId: candidate.baseDocument._nextPieceId,
      canonicalFacts: _CanonicalSourceFacts(
        authority: authority,
        fingerprintAlgorithm: completion.fingerprintAlgorithm,
        fingerprint: completion.fingerprint,
        logicalLineBreaks: completion.logicalLineBreaks,
        checkpointHash128: completion.checkpointHash128,
        checkpointCount: completion.checkpointCount,
        pageCount: completion.pageCount,
        index: index,
      ),
    );
    _lastCertifiedFingerprint = completion.fingerprint;
    _canonicalDeltaBaseFacts = null;
    _invalidateCertificationCandidate();
    return FlarkV3SourcePromotionReceipt._(
      disposition: FlarkV3SourcePromotionDisposition.promoted,
      pathNodesVisited: 0,
      piecesAttached: 0,
      canonicalProof: proof,
    );
  }

  /// Opens one exact-base incremental canonical SourceFacts certification.
  ///
  /// The installed base is never copied or flattened. Two persistent splits
  /// retain the unchanged prefix and suffix while dropping only the replaced
  /// page range from the candidate.
  FlarkV3CanonicalSourceFactDeltaBeginReceipt beginCanonicalSourceFactDelta(
    FlarkV3CanonicalSourceFactDelta delta,
  ) {
    if (!_canonicalLineageBindsLiveSource(delta.lineage)) {
      return const FlarkV3CanonicalSourceFactDeltaBeginReceipt.stale();
    }
    final base = _canonicalDeltaBaseFacts;
    if (base == null ||
        !identical(delta.baseAuthority, base.authority) ||
        delta.baseAuthority.sourceSessionIdentity != sourceSessionIdentity ||
        delta.baseAuthority.workerGeneration != _workerGeneration ||
        delta.baseAuthority.fingerprint.revision >= _document.revision ||
        delta.baseFingerprint != base.fingerprint ||
        delta.baseCheckpointRootGuard128 != base.checkpointHash128 ||
        delta.baseCheckpointCount != base.checkpointCount ||
        delta.basePageCount != base.pageCount ||
        delta.baseCheckpointSpacingUtf16 != base.index.spacingUtf16 ||
        base.index.checkpoints.pageCount != base.pageCount) {
      return const FlarkV3CanonicalSourceFactDeltaBeginReceipt.stale();
    }
    if (_certificationCandidate != null) {
      return const FlarkV3CanonicalSourceFactDeltaBeginReceipt.rejected();
    }

    final removedPageCount = delta.basePageEnd - delta.basePageStart;
    final replacementPageCount = delta.replacementPageCount;
    final validTargetShape = _document.utf16Length == 0
        ? delta.targetCheckpointCount == 0 && delta.targetPageCount == 0
        : delta.targetCheckpointCount > 0 &&
              delta.targetPageCount > 0 &&
              delta.targetCheckpointCount <= (_document.utf16Length + 1) >> 1;
    if (delta.basePageStart < 0 ||
        delta.basePageEnd < delta.basePageStart ||
        delta.basePageEnd > base.pageCount ||
        delta.targetPageStart != delta.basePageStart ||
        delta.targetPageEnd < delta.targetPageStart ||
        delta.replacementCheckpointCount < 0 ||
        delta.replacementCheckpointCount >
            replacementPageCount * _maximumWorkerSyncPageEntries ||
        delta.targetPageCount !=
            base.pageCount - removedPageCount + replacementPageCount ||
        delta.targetCheckpointRootGuardAlgorithm !=
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm ||
        (_document.utf16Length == 0 &&
            delta.targetCheckpointRootGuard128 != FlarkV3ContentHash128.zero) ||
        !validTargetShape) {
      return const FlarkV3CanonicalSourceFactDeltaBeginReceipt.rejected();
    }

    final work = _SourceCheckpointTreeWork();
    late final _SourceCheckpointSplit throughRemoved;
    late final _SourceCheckpointSplit beforeRemoved;
    try {
      throughRemoved = _splitSourceCheckpointPages(
        base.index.checkpoints._root,
        delta.basePageEnd,
        work: work,
      );
      beforeRemoved = _splitSourceCheckpointPages(
        throughRemoved.left,
        delta.basePageStart,
        work: work,
      );
    } on Object {
      return const FlarkV3CanonicalSourceFactDeltaBeginReceipt.rejected();
    }
    final removed = beforeRemoved.right;
    final removedCheckpoints = removed?.count ?? 0;
    final reusedPageCount =
        (beforeRemoved.left?.pageCount ?? 0) +
        (throughRemoved.right?.pageCount ?? 0);
    final reusedCheckpointCount =
        (beforeRemoved.left?.count ?? 0) + (throughRemoved.right?.count ?? 0);
    if ((removed?.pageCount ?? 0) != removedPageCount ||
        delta.targetCheckpointCount !=
            base.checkpointCount -
                removedCheckpoints +
                delta.replacementCheckpointCount ||
        reusedPageCount + removedPageCount != base.pageCount ||
        reusedCheckpointCount + removedCheckpoints != base.checkpointCount) {
      return const FlarkV3CanonicalSourceFactDeltaBeginReceipt.rejected();
    }

    final prefixLast = beforeRemoved.left?.summary ?? _sourcePrefixOrigin;
    _certificationCandidate = _CanonicalSourceFactDeltaCandidate(
      lineage: delta.lineage,
      baseDocument: _document,
      observedReplica: _observedWorkerReplica,
      delta: delta,
      baseFacts: base,
      prefixRoot: beforeRemoved.left,
      suffixRoot: throughRemoved.right,
      removedCheckpointCount: removedCheckpoints,
      reusedPageCount: reusedPageCount,
      reusedCheckpointCount: reusedCheckpointCount,
      pathNodesAllocated: work.nodesAllocated,
      lastCheckpoint: prefixLast,
    );
    _latestRequestId = delta.lineage.requestId;
    _latestRequestUiRevision = delta.lineage.uiRevision;
    return FlarkV3CanonicalSourceFactDeltaBeginReceipt._(
      disposition: FlarkV3CanonicalSourceFactDeltaBeginDisposition.accepted,
      reusedPageCount: reusedPageCount,
      reusedCheckpointCount: reusedCheckpointCount,
      pathNodesAllocated: work.nodesAllocated,
    );
  }

  /// Moves one bounded absolute target checkpoint page into the live delta.
  FlarkV3SourceFactStageReceipt stageCanonicalSourceFactDeltaCheckpointPage(
    FlarkV3CanonicalSourceFactDeltaCheckpointPage page,
  ) {
    if (!_canonicalLineageBindsLiveSource(page.lineage)) {
      return const FlarkV3SourceFactStageReceipt.stale();
    }
    final candidate = _certificationCandidate;
    if (candidate is! _CanonicalSourceFactDeltaCandidate ||
        page.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument)) {
      return const FlarkV3SourceFactStageReceipt.stale();
    }
    final delta = candidate.delta;
    final remainingCheckpoints =
        delta.replacementCheckpointCount -
        candidate.replacementCheckpointsAccepted;
    if (page.isConsumed ||
        page.pageOrdinal != candidate.expectedReplacementPageOrdinal ||
        page.pageOrdinal < 0 ||
        page.pageOrdinal >= delta.replacementPageCount ||
        page.checkpointCount <= 0 ||
        page.checkpointCount > remainingCheckpoints) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }

    var previous = candidate.lastCheckpoint;
    for (final fact in page.checkpoints) {
      final gap = fact.utf16Offset - previous.utf16Offset;
      if (gap <= 0 ||
          gap > candidate.baseFacts.index.spacingUtf16 + 1 ||
          fact.utf16Offset > candidate.lineage.utf16Length ||
          fact.utf8Offset <= previous.utf8Offset ||
          fact.utf8Offset > candidate.observedReplica.utf8Length ||
          fact.newlines < previous.newlines ||
          fact.newlines > fact.utf16Offset) {
        _invalidateCertificationCandidate();
        return const FlarkV3SourceFactStageReceipt.rejected();
      }
      previous = fact;
    }
    final facts = page.checkpoints;
    var replacementHash = candidate.replacementCheckpointHash128;
    for (final fact in facts) {
      replacementHash = _appendSourcePrefixFactsHash(replacementHash, fact);
    }
    final accepted = facts.length;
    try {
      candidate.replacementBuilder.addOwnedAbsolutePage(
        page._takeCheckpoints(),
        pagePrefix: candidate.lastCheckpoint,
      );
    } on Object {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }
    candidate.lastCheckpoint = previous;
    candidate.replacementCheckpointHash128 = replacementHash;
    candidate.replacementCheckpointsAccepted += accepted;
    candidate.expectedReplacementPageOrdinal += 1;
    return FlarkV3SourceFactStageReceipt._(
      disposition: FlarkV3SourceFactStageDisposition.staged,
      pieceCompleted:
          candidate.expectedReplacementPageOrdinal ==
          delta.replacementPageCount,
      checkpointsAccepted: accepted,
      pathNodesVisited: 0,
      piecesAttached: 0,
    );
  }

  /// Atomically installs a fully authenticated persistent checkpoint splice.
  FlarkV3CanonicalSourceFactDeltaPromotionReceipt
  commitCanonicalSourceFactDeltaCertification(
    FlarkV3CanonicalSourceFactDeltaCompletion completion,
  ) {
    if (!_canonicalLineageBindsLiveSource(completion.lineage)) {
      return const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.stale();
    }
    final candidate = _certificationCandidate;
    if (candidate is! _CanonicalSourceFactDeltaCandidate ||
        completion.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument) ||
        !identical(
          _canonicalDeltaBaseFacts?.authority,
          candidate.delta.baseAuthority,
        )) {
      return const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.stale();
    }
    final delta = candidate.delta;
    final headerAndTerminalMatch =
        completion.fingerprintAlgorithm == 1 &&
        delta.targetCheckpointRootGuardAlgorithm ==
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm &&
        completion.checkpointRootGuardAlgorithm ==
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm &&
        completion.checkpointSpacingUtf16 ==
            candidate.baseFacts.index.spacingUtf16 &&
        completion.checkpointCount == delta.targetCheckpointCount &&
        completion.pageCount == delta.targetPageCount &&
        completion.checkpointRootGuard128 ==
            delta.targetCheckpointRootGuard128 &&
        completion.replacementCheckpointHash128 ==
            candidate.replacementCheckpointHash128 &&
        candidate.expectedReplacementPageOrdinal ==
            delta.replacementPageCount &&
        candidate.replacementCheckpointsAccepted ==
            delta.replacementCheckpointCount &&
        completion.fingerprint.revision == _document.revision &&
        completion.fingerprint.utf16Length == _document.utf16Length &&
        completion.fingerprint.utf8Length ==
            candidate.observedReplica.utf8Length &&
        completion.logicalLineBreaks >= 0 &&
        completion.logicalLineBreaks <= _document.utf16Length;
    if (!headerAndTerminalMatch) {
      _invalidateCertificationCandidate();
      return const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.rejected();
    }

    late final _SourceCheckpointNode? replacementRoot;
    try {
      replacementRoot = candidate.replacementBuilder.seal();
    } on Object {
      _invalidateCertificationCandidate();
      return const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.rejected();
    }
    final work = _SourceCheckpointTreeWork();
    final targetRoot = _joinSourceCheckpointTrees(
      _joinSourceCheckpointTrees(
        candidate.prefixRoot,
        replacementRoot,
        work: work,
      ),
      candidate.suffixRoot,
      work: work,
    );
    final targetStore = _PagedSourceCheckpointStore._(
      targetRoot,
      _sourcePrefixOrigin,
    );
    final terminal = targetStore.terminalFacts;
    final empty = _document.utf16Length == 0;
    final terminalMatches = empty
        ? targetRoot == null &&
              completion.fingerprint.utf8Length == 0 &&
              completion.logicalLineBreaks == 0 &&
              completion.fingerprint.contentHash128 ==
                  FlarkV3ContentHash128.zero
        : targetRoot != null &&
              terminal.utf16Offset == _document.utf16Length &&
              terminal.utf8Offset == completion.fingerprint.utf8Length &&
              terminal.newlines == completion.logicalLineBreaks &&
              terminal.hash == completion.fingerprint.contentHash128;
    if (!terminalMatches ||
        targetStore.pageCount != delta.targetPageCount ||
        targetStore.length - 1 != delta.targetCheckpointCount) {
      _invalidateCertificationCandidate();
      return const FlarkV3CanonicalSourceFactDeltaPromotionReceipt.rejected();
    }

    final authority = FlarkV3CanonicalSourceFactAuthority._(
      sourceSessionIdentity: sourceSessionIdentity,
      workerGeneration: completion.lineage.workerGeneration,
      workerReplicaRevision: completion.lineage.workerReplicaRevision,
      intentHighWater: completion.lineage.intentHighWater,
      fingerprintAlgorithm: completion.fingerprintAlgorithm,
      fingerprint: completion.fingerprint,
      logicalLineBreaks: completion.logicalLineBreaks,
      checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
      checkpointCount: completion.checkpointCount,
      pageCount: completion.pageCount,
      checkpointHash128: completion.checkpointRootGuard128,
    );
    final proof = FlarkV3CanonicalSourcePromotionProof(
      lineage: completion.lineage,
      fingerprintAlgorithm: completion.fingerprintAlgorithm,
      fingerprint: completion.fingerprint,
      logicalLineBreaks: completion.logicalLineBreaks,
      checkpointSpacingUtf16: completion.checkpointSpacingUtf16,
      checkpointCount: completion.checkpointCount,
      pageCount: completion.pageCount,
      checkpointHash128: completion.checkpointRootGuard128,
    );
    _document = FlarkV3SourceDocument._(
      root: candidate.baseDocument._root,
      revision: candidate.baseDocument.revision,
      chunkSize: candidate.baseDocument._chunkSize,
      nextPieceId: candidate.baseDocument._nextPieceId,
      canonicalFacts: _CanonicalSourceFacts(
        authority: authority,
        fingerprintAlgorithm: completion.fingerprintAlgorithm,
        fingerprint: completion.fingerprint,
        logicalLineBreaks: completion.logicalLineBreaks,
        checkpointHash128: completion.checkpointRootGuard128,
        checkpointCount: completion.checkpointCount,
        pageCount: completion.pageCount,
        index: _SourceRangeIndex._(
          start: 0,
          end: _document.utf16Length,
          spacingUtf16: completion.checkpointSpacingUtf16,
          checkpoints: targetStore,
        ),
      ),
    );
    _lastCertifiedFingerprint = completion.fingerprint;
    final pathNodesAllocated =
        candidate.pathNodesAllocated + work.nodesAllocated;
    final reusedPageCount = candidate.reusedPageCount;
    final reusedCheckpointCount = candidate.reusedCheckpointCount;
    final transferredPageCount = delta.replacementPageCount;
    final transferredCheckpointCount = candidate.replacementCheckpointsAccepted;
    _invalidateCertificationCandidate();
    return FlarkV3CanonicalSourceFactDeltaPromotionReceipt._(
      disposition: FlarkV3SourcePromotionDisposition.promoted,
      reusedPageCount: reusedPageCount,
      reusedCheckpointCount: reusedCheckpointCount,
      transferredPageCount: transferredPageCount,
      transferredCheckpointCount: transferredCheckpointCount,
      pathNodesAllocated: pathNodesAllocated,
      canonicalProof: proof,
    );
  }

  /// Moves one bounded numeric checkpoint page into the hidden candidate.
  ///
  /// At most one piece is attached, and the discovery plus attachment paths
  /// must fit [maximumPathNodes]. No authoritative source facts change here.
  FlarkV3SourceFactStageReceipt stageCertificationCheckpointPage(
    FlarkV3SourceFactCheckpointPage page, {
    int maximumPathNodes = 512,
  }) {
    final candidate = _certificationCandidate;
    if (candidate is! _PieceSourceFactCertificationCandidate ||
        page.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument)) {
      return const FlarkV3SourceFactStageReceipt.stale();
    }
    final height = candidate.document._root?.height ?? 0;
    final minimumDiscoveryNodes = math.max(
      _minimumPendingDiscoveryNodes,
      height + 1,
    );
    final minimumPathNodes = minimumDiscoveryNodes + height;
    if (maximumPathNodes < minimumPathNodes) {
      throw RangeError.range(
        maximumPathNodes,
        minimumPathNodes,
        null,
        'maximumPathNodes',
      );
    }
    if (maximumPathNodes > _maximumSourceFactAdoptionPathNodes) {
      throw RangeError.range(
        maximumPathNodes,
        minimumPathNodes,
        _maximumSourceFactAdoptionPathNodes,
        'maximumPathNodes',
      );
    }
    if (page.isConsumed ||
        page.pageOrdinal != candidate.expectedPageOrdinal ||
        page.relativeStartUtf16 < 0 ||
        page.relativeEndUtf16 <= page.relativeStartUtf16 ||
        page.relativeEndUtf16 > page.piece.utf16Length ||
        page.checkpointSpacingUtf16 < 2 ||
        page.checkpointSpacingUtf16 > _maximumSourceChunkUtf16 ||
        page.isLast != (page.relativeEndUtf16 == page.piece.utf16Length)) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }

    var active = candidate.activePiece;
    var discoveryNodesVisited = 0;
    if (active == null) {
      final pending = candidate.document._pendingPiecePage(
        cursorUtf16: 0,
        maximumPieces: 1,
        maximumNodes: _maximumDiscoveryNodesForPathBudget(
          maximumPathNodes,
          height,
        ),
      );
      discoveryNodesVisited = pending.nodesVisited;
      if (pending.pieces.length != 1 ||
          pending.pieces.single._key != page.piece._key ||
          pending.pieces.single.globalStartUtf16 !=
              page.piece.globalStartUtf16 ||
          page.piecePageOrdinal != 0 ||
          page.relativeStartUtf16 != 0) {
        _invalidateCertificationCandidate();
        return const FlarkV3SourceFactStageReceipt.rejected();
      }
      active = _ActiveSourceFactCandidatePiece(
        piece: pending.pieces.single,
        indexBuilder: _SourceRangeIndexBuilder(
          start: page.piece.sourceStartUtf16,
          end: page.piece.sourceStartUtf16 + page.piece.utf16Length,
          spacingUtf16: page.checkpointSpacingUtf16,
        ),
        nextPiecePageOrdinal: 0,
      );
      candidate.activePiece = active;
      candidate.descriptorHash128 = _appendSourcePieceDescriptorHash(
        candidate.descriptorHash128,
        active.piece,
      );
    }

    if (active.piece._key != page.piece._key ||
        active.piece.globalStartUtf16 != page.piece.globalStartUtf16 ||
        page.piecePageOrdinal != active.nextPiecePageOrdinal ||
        page.checkpointSpacingUtf16 != active.indexBuilder.spacingUtf16 ||
        page.relativeStartUtf16 !=
            active.indexBuilder.nextUtf16Offset -
                active.piece.sourceStartUtf16) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }

    final facts = page.checkpoints;
    if (facts.last.utf16Offset !=
            active.piece.sourceStartUtf16 + page.relativeEndUtf16 ||
        facts.first.utf16Offset <= active.indexBuilder.nextUtf16Offset) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }
    for (final fact in facts) {
      candidate.checkpointHash128 = _appendSourcePrefixFactsHash(
        candidate.checkpointHash128,
        fact,
      );
    }
    final accepted = facts.length;
    try {
      active.indexBuilder.addOwnedPage(page._takeCheckpoints());
    } on StateError {
      _invalidateCertificationCandidate();
      return const FlarkV3SourceFactStageReceipt.rejected();
    }
    active.nextPiecePageOrdinal += 1;
    candidate.expectedPageOrdinal += 1;
    candidate.checkpointsAccepted += accepted;
    candidate.pagesAccepted += 1;

    var attachmentPathNodes = 0;
    if (page.isLast) {
      try {
        final certification = FlarkV3CertifiedSourcePiece._(
          pieceId: active.piece.pieceId,
          sourceStartUtf16: active.piece.sourceStartUtf16,
          utf16Length: active.piece.utf16Length,
          index: active.indexBuilder._seal(),
        );
        final attached = candidate.document._attachCertificationPiece(
          active.piece,
          certification,
        );
        attachmentPathNodes = attached.pathNodesVisited;
        if (discoveryNodesVisited + attachmentPathNodes > maximumPathNodes) {
          throw StateError('Source-fact attachment exceeded path credit.');
        }
        candidate.document = attached.document;
        candidate.activePiece = null;
        candidate.piecesAttached += 1;
      } on StateError {
        _invalidateCertificationCandidate();
        return const FlarkV3SourceFactStageReceipt.rejected();
      }
    }
    final visited = discoveryNodesVisited + attachmentPathNodes;
    candidate.pathNodesVisited += visited;
    return FlarkV3SourceFactStageReceipt._(
      disposition: FlarkV3SourceFactStageDisposition.staged,
      pieceCompleted: page.isLast,
      checkpointsAccepted: accepted,
      pathNodesVisited: visited,
      piecesAttached: page.isLast ? 1 : 0,
    );
  }

  /// Validates the completed hidden root through O(1) aggregates, then swaps
  /// the authoritative document pointer. It never acknowledges worker sync.
  FlarkV3SourcePromotionReceipt commitSourceFactCertification(
    FlarkV3SourceFactCompletion completion,
  ) {
    final candidate = _certificationCandidate;
    if (candidate is! _PieceSourceFactCertificationCandidate ||
        completion.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument)) {
      return const FlarkV3SourcePromotionReceipt.stale();
    }
    final valid =
        candidate.activePiece == null &&
        candidate.document.isFullyIndexed &&
        completion.fingerprint.revision == candidate.observedReplica.revision &&
        completion.fingerprint.utf16Length ==
            candidate.observedReplica.utf16Length &&
        completion.fingerprint.utf8Length ==
            candidate.observedReplica.utf8Length &&
        completion.lineage.intentHighWater ==
            candidate.observedReplica.intentHighWater &&
        completion.fingerprint == candidate.document.fingerprint &&
        completion.logicalLineBreaks == candidate.document.lineCount - 1 &&
        completion.pieceCount == candidate.piecesAttached &&
        completion.checkpointCount == candidate.checkpointsAccepted &&
        completion.pageCount == candidate.pagesAccepted &&
        completion.descriptorHash128 == candidate.descriptorHash128 &&
        completion.checkpointHash128 == candidate.checkpointHash128;
    if (!valid) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourcePromotionReceipt._(
        disposition: FlarkV3SourcePromotionDisposition.rejected,
        pathNodesVisited: 0,
        piecesAttached: 0,
      );
    }
    final piecesAttached = candidate.piecesAttached;
    _document = candidate.document;
    _lastCertifiedFingerprint = _document.fingerprint;
    _invalidateCertificationCandidate();
    return FlarkV3SourcePromotionReceipt._(
      disposition: FlarkV3SourcePromotionDisposition.promoted,
      pathNodesVisited: 0,
      piecesAttached: piecesAttached,
    );
  }

  FlarkV3SourceFactCancellationReceipt cancelSourceFactCertification(
    int requestId,
  ) {
    final candidate = _certificationCandidate;
    if (candidate == null || candidate.lineage.requestId != requestId) {
      return const FlarkV3SourceFactCancellationReceipt._(
        cancelled: false,
        candidateRootsReleased: 0,
        pathNodesVisited: 0,
      );
    }
    _invalidateCertificationCandidate();
    return const FlarkV3SourceFactCancellationReceipt._(
      cancelled: true,
      candidateRootsReleased: 1,
      pathNodesVisited: 0,
    );
  }

  /// Applies a typed worker failure to its exact live candidate. A stale
  /// failure cannot cancel a newer transaction.
  FlarkV3SourceFactCancellationReceipt rejectSourceFactCertification(
    FlarkV3SourceCertificationFailure failure,
  ) {
    final candidate = _certificationCandidate;
    if (candidate == null || failure.lineage != candidate.lineage) {
      return const FlarkV3SourceFactCancellationReceipt._(
        cancelled: false,
        candidateRootsReleased: 0,
        pathNodesVisited: 0,
      );
    }
    _invalidateCertificationCandidate();
    return const FlarkV3SourceFactCancellationReceipt._(
      cancelled: true,
      candidateRootsReleased: 1,
      pathNodesVisited: 0,
    );
  }

  FlarkV3SourcePromotionReceipt applyCertification(
    FlarkV3SourceCertificationReceipt receipt,
  ) {
    final candidate = _certificationCandidate;
    if (candidate is! _PieceSourceFactCertificationCandidate ||
        receipt.lineage != candidate.lineage ||
        !identical(_document, candidate.baseDocument)) {
      return const FlarkV3SourcePromotionReceipt.stale();
    }
    if (candidate.activePiece != null) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourcePromotionReceipt._(
        disposition: FlarkV3SourcePromotionDisposition.rejected,
        pathNodesVisited: 0,
        piecesAttached: 0,
      );
    }
    late final _AttachedSourceCertification attached;
    try {
      attached = candidate.baseDocument._attachCertification(receipt.pieces);
    } on Object {
      _invalidateCertificationCandidate();
      rethrow;
    }
    if (attached.document.revision != candidate.observedReplica.revision ||
        attached.document.utf16Length !=
            candidate.observedReplica.utf16Length ||
        attached.document.utf8Length != candidate.observedReplica.utf8Length ||
        receipt.intentHighWater != candidate.observedReplica.intentHighWater) {
      _invalidateCertificationCandidate();
      return const FlarkV3SourcePromotionReceipt._(
        disposition: FlarkV3SourcePromotionDisposition.rejected,
        pathNodesVisited: 0,
        piecesAttached: 0,
      );
    }
    _document = attached.document;
    _lastCertifiedFingerprint = _document.fingerprint;
    _invalidateCertificationCandidate();
    return FlarkV3SourcePromotionReceipt._(
      disposition: FlarkV3SourcePromotionDisposition.promoted,
      pathNodesVisited: attached.pathNodesVisited,
      piecesAttached: attached.piecesAttached,
    );
  }
}

int _maximumDiscoveryNodesForPathBudget(int pathBudget, int treeHeight) =>
    pathBudget - treeHeight;

final class FlarkV3SourceSessionApplyReceipt {
  const FlarkV3SourceSessionApplyReceipt({
    required this.changed,
    required this.provisional,
    required this.parserBatch,
    required this.sourceWork,
    required this.inverseLeasePathNodesVisited,
  });

  final bool changed;
  final bool provisional;
  final FlarkV3ParserEditBatch? parserBatch;
  final FlarkV3SourceWorkReceipt sourceWork;
  final int inverseLeasePathNodesVisited;
}

/// Typed request to copy live slices off a disproportionately retained backing.
///
/// A background compactor must re-enumerate live ranges for [backingIdentity]
/// at [sourceRevision] and the session must reject a stale result. The byte
/// charge is for the unique backing, never the sum of visible slice lengths.
final class FlarkV3SourceCompactionObligation {
  const FlarkV3SourceCompactionObligation._({
    required this.sourceRevision,
    required this.backingIdentity,
    required this.retainedBackingUtf16Bytes,
    required this.observedLiveUtf16,
    this.blockedByUndoLease = false,
  });

  final int sourceRevision;
  final int backingIdentity;
  final int retainedBackingUtf16Bytes;
  final int observedLiveUtf16;
  final bool blockedByUndoLease;

  FlarkV3SourceCompactionObligation _withBlocked(bool blocked) =>
      FlarkV3SourceCompactionObligation._(
        sourceRevision: sourceRevision,
        backingIdentity: backingIdentity,
        retainedBackingUtf16Bytes: retainedBackingUtf16Bytes,
        observedLiveUtf16: observedLiveUtf16,
        blockedByUndoLease: blocked,
      );
}

final class FlarkV3SourceCompactionLease {
  FlarkV3SourceCompactionLease._({
    required this.leaseId,
    required List<FlarkV3SourceCompactionObligation> obligations,
  }) : obligations = List.unmodifiable(obligations);

  final int leaseId;
  final List<FlarkV3SourceCompactionObligation> obligations;
}

final class FlarkV3AppliedSourceTransaction {
  const FlarkV3AppliedSourceTransaction({
    required this.document,
    required this.parserBatch,
    required this.sourceWork,
    this.compactionObligations = const [],
  }) : changed = true;

  const FlarkV3AppliedSourceTransaction.provisional({
    required this.document,
    required this.sourceWork,
    this.compactionObligations = const [],
  }) : parserBatch = null,
       changed = true;

  const FlarkV3AppliedSourceTransaction.noOp({
    required this.document,
    required this.sourceWork,
  }) : parserBatch = null,
       compactionObligations = const [],
       changed = false;

  final FlarkV3SourceDocument document;
  final FlarkV3ParserEditBatch? parserBatch;
  final FlarkV3SourceWorkReceipt sourceWork;
  final List<FlarkV3SourceCompactionObligation> compactionObligations;
  final bool changed;
}

/// Auditable synchronous work performed while accepting one source transaction.
///
/// This excludes balanced-tree pointer allocation, but pins the two accidental
/// document-scale paths that previously hid in no-op detection and replacement
/// encoding.
final class FlarkV3SourceWorkReceipt {
  const FlarkV3SourceWorkReceipt({
    required this.noOpComparedUtf16,
    required this.replacementUtf8BytesEncoded,
    required this.replacementChunksEncoded,
  });

  final int noOpComparedUtf16;
  final int replacementUtf8BytesEncoded;
  final int replacementChunksEncoded;
}

final class _MutableSourceWorkReceipt {
  int noOpComparedUtf16 = 0;
  int replacementUtf8BytesEncoded = 0;
  int replacementChunksEncoded = 0;

  FlarkV3SourceWorkReceipt seal() => FlarkV3SourceWorkReceipt(
    noOpComparedUtf16: noOpComparedUtf16,
    replacementUtf8BytesEncoded: replacementUtf8BytesEncoded,
    replacementChunksEncoded: replacementChunksEncoded,
  );
}

final class FlarkV3ParserEditBatch {
  FlarkV3ParserEditBatch({
    required this.baseRevision,
    required this.revision,
    required this.beforeHash128,
    required this.afterHash128,
    required this.beforeUtf8Length,
    required this.afterUtf8Length,
    required List<FlarkV3ParserEdit> operations,
  }) : operations = List.unmodifiable(operations);

  final int baseRevision;
  final int revision;
  final FlarkV3ContentHash128 beforeHash128;
  final FlarkV3ContentHash128 afterHash128;
  int get beforeHash32 => beforeHash128.word0;
  int get afterHash32 => afterHash128.word0;
  final int beforeUtf8Length;
  final int afterUtf8Length;

  /// Sorted by original-revision start/end and stable input order.
  ///
  /// The parser mirror applies this list in reverse to preserve original
  /// coordinates and same-offset insertion order, then advances one revision.
  final List<FlarkV3ParserEdit> operations;

  int get wireBytes =>
      56 +
      operations.fold<int>(
        0,
        (total, operation) => total + 12 + operation.replacementUtf8.length,
      );
}

final class FlarkV3ParserEdit {
  const FlarkV3ParserEdit({
    required this.startUtf8,
    required this.endUtf8,
    required this.replacementUtf8,
  });

  final int startUtf8;
  final int endUtf8;
  final Uint8List replacementUtf8;
}

/// Tagged O(1) source target identity carried across the replica protocol.
///
/// Revisions are ordering facts, not content identity. Known stamps add the
/// persistent tree's UTF-8 and rolling-hash guards; provisional stamps expose
/// only the UTF-16 facts the Dart source authority can currently prove.
sealed class FlarkV3SourceStamp {
  const FlarkV3SourceStamp({required this.revision, required this.utf16Length});

  final int revision;
  final int utf16Length;
}

final class FlarkV3KnownSourceStamp extends FlarkV3SourceStamp {
  const FlarkV3KnownSourceStamp({
    required super.revision,
    required super.utf16Length,
    required this.utf8Length,
    required this.contentHash128,
  });

  static const empty = FlarkV3KnownSourceStamp(
    revision: 0,
    utf16Length: 0,
    utf8Length: 0,
    contentHash128: FlarkV3ContentHash128.zero,
  );

  final int utf8Length;
  final FlarkV3ContentHash128 contentHash128;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3KnownSourceStamp &&
      other.revision == revision &&
      other.utf16Length == utf16Length &&
      other.utf8Length == utf8Length &&
      other.contentHash128 == contentHash128;

  @override
  int get hashCode =>
      Object.hash(revision, utf16Length, utf8Length, contentHash128);
}

final class FlarkV3ProvisionalSourceStamp extends FlarkV3SourceStamp {
  const FlarkV3ProvisionalSourceStamp({
    required super.revision,
    required super.utf16Length,
  });

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ProvisionalSourceStamp &&
      other.revision == revision &&
      other.utf16Length == utf16Length;

  @override
  int get hashCode => Object.hash(revision, utf16Length);
}

/// What the worker reports after it has installed a complete source target.
///
/// This is intentionally not a source fingerprint: it contains no echoed hash
/// and cannot upgrade provisional Dart facts. It is replica-state evidence
/// that must independently agree with source-fact certification before the
/// session can publish certified source.
final class FlarkV3ObservedSourceReplicaVersion {
  const FlarkV3ObservedSourceReplicaVersion({
    required this.revision,
    required this.utf16Length,
    required this.utf8Length,
    required this.intentHighWater,
  });

  static const empty = FlarkV3ObservedSourceReplicaVersion(
    revision: 0,
    utf16Length: 0,
    utf8Length: 0,
    intentHighWater: 0,
  );

  final int revision;
  final int utf16Length;
  final int utf8Length;
  final int intentHighWater;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ObservedSourceReplicaVersion &&
      other.revision == revision &&
      other.utf16Length == utf16Length &&
      other.utf8Length == utf8Length &&
      other.intentHighWater == intentHighWater;

  @override
  int get hashCode =>
      Object.hash(revision, utf16Length, utf8Length, intentHighWater);
}

final class FlarkV3SourceFingerprint {
  const FlarkV3SourceFingerprint({
    required this.revision,
    required this.utf16Length,
    required this.utf8Length,
    required this.contentHash128,
  });

  final int revision;
  final int utf16Length;
  final int utf8Length;
  final FlarkV3ContentHash128 contentHash128;
  int get contentHash32 => contentHash128.word0;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourceFingerprint &&
      other.revision == revision &&
      other.utf16Length == utf16Length &&
      other.utf8Length == utf8Length &&
      other.contentHash128 == contentHash128;

  @override
  int get hashCode =>
      Object.hash(revision, utf16Length, utf8Length, contentHash128);
}

/// Four independently combined 32-bit lanes used for transport integrity.
///
/// Parser convergence still requires exact source identity; this fingerprint
/// is a strong corruption/stale-result guard, not a substitute for equality.
final class FlarkV3ContentHash128 {
  const FlarkV3ContentHash128(this.word0, this.word1, this.word2, this.word3);

  static const zero = FlarkV3ContentHash128(0, 0, 0, 0);
  static const powerIdentity = FlarkV3ContentHash128(1, 1, 1, 1);

  final int word0;
  final int word1;
  final int word2;
  final int word3;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ContentHash128 &&
      other.word0 == word0 &&
      other.word1 == word1 &&
      other.word2 == word2 &&
      other.word3 == word3;

  @override
  int get hashCode => Object.hash(word0, word1, word2, word3);
}

enum FlarkV3GraphemeLookupStatus { certified, needsMoreContext }

final class FlarkV3GraphemeLookup {
  const FlarkV3GraphemeLookup.certified(int startUtf16, int endUtf16)
    : status = FlarkV3GraphemeLookupStatus.certified,
      startUtf16 = startUtf16,
      endUtf16 = endUtf16,
      requiredStartUtf16 = startUtf16,
      inspectedStartUtf16 = startUtf16,
      caretUtf16 = endUtf16;

  const FlarkV3GraphemeLookup.needsMoreContext({
    required this.requiredStartUtf16,
    required this.inspectedStartUtf16,
    required this.caretUtf16,
  }) : status = FlarkV3GraphemeLookupStatus.needsMoreContext,
       startUtf16 = null,
       endUtf16 = null;

  final FlarkV3GraphemeLookupStatus status;
  final int? startUtf16;
  final int? endUtf16;
  final int requiredStartUtf16;
  final int inspectedStartUtf16;
  final int caretUtf16;

  bool get isCertified => status == FlarkV3GraphemeLookupStatus.certified;
}

final class FlarkV3SourceTreeDiagnostics {
  const FlarkV3SourceTreeDiagnostics({
    required this.leafCount,
    required this.largestLeafUtf16,
    required this.treeHeight,
    required this.uniqueBackingCount,
  });

  final int leafCount;
  final int largestLeafUtf16;
  final int treeHeight;
  final int uniqueBackingCount;
}

final class FlarkV3RevisionMismatch implements Exception {
  const FlarkV3RevisionMismatch({required this.expected, required this.actual});

  final int expected;
  final int actual;

  @override
  String toString() =>
      'FlarkV3RevisionMismatch(expected: $expected, actual: $actual)';
}

final class FlarkV3SourceFactsPending implements Exception {
  const FlarkV3SourceFactsPending();

  @override
  String toString() =>
      'FlarkV3SourceFactsPending(exact UTF-16 source is available, derived '
      'facts require worker certification)';
}

final class FlarkV3SourceBulkOperationRequired implements Exception {
  const FlarkV3SourceBulkOperationRequired({
    required this.operationCount,
    required this.maximumSynchronousOperations,
  });

  final int operationCount;
  final int maximumSynchronousOperations;

  @override
  String toString() =>
      'FlarkV3SourceBulkOperationRequired(operationCount: $operationCount, '
      'maximumSynchronousOperations: $maximumSynchronousOperations)';
}

final class FlarkV3SourceStagedCertificationRequired implements Exception {
  const FlarkV3SourceStagedCertificationRequired({
    required this.pieceCount,
    required this.maximumSynchronousAttachments,
  });

  final int pieceCount;
  final int maximumSynchronousAttachments;

  @override
  String toString() =>
      'FlarkV3SourceStagedCertificationRequired(pieceCount: $pieceCount, '
      'maximumSynchronousAttachments: $maximumSynchronousAttachments)';
}

sealed class _SourceNode {
  const _SourceNode();

  int get utf16Length;
  bool get isCertified;
  int get utf8Length;
  int get newlines;
  bool get startsWithLineFeed;
  bool get endsWithCarriageReturn;
  int get height;
  FlarkV3ContentHash128 get contentHash128;
  FlarkV3ContentHash128 get hashPower128;
  _SourceBacking? get homogeneousBacking;
  int get retainedBackingUtf16BytesUpperBound;
  int get backingIdentityBloom;
}

int _nextSourceBackingIdentity = 1;

final class _SourceBacking {
  _SourceBacking(this.source) : identity = _nextSourceBackingIdentity++;

  final String source;
  final int identity;

  int get retainedUtf16Bytes => source.length * 2;
}

final class _SourceLeaf extends _SourceNode {
  const _SourceLeaf._({
    required this.backing,
    required this.start,
    required this.utf16Length,
    required this.isCertified,
    required this.utf8Length,
    required this.lineBreakEndOffsets,
    required this.contentHash128,
    required this.hashPower128,
    required this.pieceId,
    required this.rangeIndex,
  });

  factory _SourceLeaf.owned(String source) {
    final encoded = utf8.encode(source);
    return _SourceLeaf.ownedEncoded(source, encoded);
  }

  factory _SourceLeaf.ownedEncoded(String source, Uint8List encoded) {
    final lineBreakEndOffsets = <int>[];
    for (var offset = 0; offset < source.length; offset += 1) {
      final codeUnit = source.codeUnitAt(offset);
      if (codeUnit == 0x0D) {
        if (offset + 1 < source.length &&
            source.codeUnitAt(offset + 1) == 0x0A) {
          offset += 1;
        }
        lineBreakEndOffsets.add(offset + 1);
      } else if (codeUnit == 0x0A) {
        lineBreakEndOffsets.add(offset + 1);
      }
    }
    final hash = _hashBytes128(encoded);
    return _SourceLeaf._(
      backing: _SourceBacking(source),
      start: 0,
      utf16Length: source.length,
      isCertified: true,
      utf8Length: encoded.length,
      lineBreakEndOffsets: List.unmodifiable(lineBreakEndOffsets),
      contentHash128: hash.hash,
      hashPower128: hash.power,
      pieceId: null,
      rangeIndex: null,
    );
  }

  factory _SourceLeaf.provisional({
    required String source,
    required int start,
    required int utf16Length,
    required int pieceId,
    _SourceBacking? backing,
  }) {
    final resolvedBacking = backing ?? _SourceBacking(source);
    if (!identical(resolvedBacking.source, source)) {
      throw StateError('A source leaf backing must own the same String.');
    }
    return _SourceLeaf._(
      backing: resolvedBacking,
      start: start,
      utf16Length: utf16Length,
      isCertified: false,
      utf8Length: 0,
      lineBreakEndOffsets: const [],
      contentHash128: FlarkV3ContentHash128.zero,
      hashPower128: FlarkV3ContentHash128.powerIdentity,
      pieceId: pieceId,
      rangeIndex: null,
    );
  }

  factory _SourceLeaf.certifiedPiece(
    _SourceLeaf leaf,
    FlarkV3CertifiedSourcePiece certification,
  ) {
    final summary = certification._index.summary(
      leaf.source,
      leaf.start,
      leaf.start + leaf.utf16Length,
    );
    return _SourceLeaf._(
      backing: leaf.backing,
      start: leaf.start,
      utf16Length: leaf.utf16Length,
      isCertified: true,
      utf8Length: summary.utf8Length,
      lineBreakEndOffsets: const [],
      contentHash128: summary.hash,
      hashPower128: summary.power,
      pieceId: leaf.pieceId,
      rangeIndex: certification._index,
    );
  }

  final _SourceBacking backing;
  String get source => backing.source;
  final int start;

  @override
  final int utf16Length;
  @override
  final bool isCertified;
  @override
  final int utf8Length;
  final List<int> lineBreakEndOffsets;
  @override
  final FlarkV3ContentHash128 contentHash128;
  @override
  final FlarkV3ContentHash128 hashPower128;
  final int? pieceId;
  final _SourceRangeIndex? rangeIndex;

  @override
  _SourceBacking get homogeneousBacking => backing;

  @override
  int get retainedBackingUtf16BytesUpperBound => backing.retainedUtf16Bytes;

  @override
  int get backingIdentityBloom => 1 << (backing.identity & 63);

  @override
  int get newlines => rangeIndex == null
      ? lineBreakEndOffsets.length
      : rangeIndex!.newlinesInRange(source, start, start + utf16Length);

  @override
  bool get startsWithLineFeed =>
      utf16Length > 0 && source.codeUnitAt(start) == 0x0A;

  @override
  bool get endsWithCarriageReturn =>
      utf16Length > 0 && source.codeUnitAt(start + utf16Length - 1) == 0x0D;

  @override
  int get height => 1;

  _SourceLeaf slice(int relativeStart, int relativeEnd) {
    if (!isCertified) {
      return _SourceLeaf.provisional(
        source: source,
        start: start + relativeStart,
        utf16Length: relativeEnd - relativeStart,
        pieceId: pieceId!,
        backing: backing,
      );
    }
    if (rangeIndex case final index?) {
      final absoluteStart = start + relativeStart;
      final absoluteEnd = start + relativeEnd;
      final summary = index.summary(source, absoluteStart, absoluteEnd);
      return _SourceLeaf._(
        backing: backing,
        start: absoluteStart,
        utf16Length: absoluteEnd - absoluteStart,
        isCertified: true,
        utf8Length: summary.utf8Length,
        lineBreakEndOffsets: const [],
        contentHash128: summary.hash,
        hashPower128: summary.power,
        pieceId: pieceId,
        rangeIndex: index,
      );
    }
    final owned = source.substring(start + relativeStart, start + relativeEnd);
    return _SourceLeaf.owned(owned);
  }

  String materialize() => source.substring(start, start + utf16Length);
}

final class _SourceBranch extends _SourceNode {
  _SourceBranch(this.left, this.right)
    : utf16Length = left.utf16Length + right.utf16Length,
      isCertified = left.isCertified && right.isCertified,
      utf8Length = left.isCertified && right.isCertified
          ? left.utf8Length + right.utf8Length
          : 0,
      newlines = left.isCertified && right.isCertified
          ? left.newlines +
                right.newlines -
                (left.endsWithCarriageReturn && right.startsWithLineFeed
                    ? 1
                    : 0)
          : 0,
      height = math.max(left.height, right.height) + 1,
      contentHash128 = left.isCertified && right.isCertified
          ? _appendHash128(
              left.contentHash128,
              right.contentHash128,
              right.hashPower128,
            )
          : FlarkV3ContentHash128.zero,
      hashPower128 = left.isCertified && right.isCertified
          ? _multiplyHash128(left.hashPower128, right.hashPower128)
          : FlarkV3ContentHash128.powerIdentity,
      homogeneousBacking =
          left.homogeneousBacking != null &&
              identical(left.homogeneousBacking, right.homogeneousBacking)
          ? left.homogeneousBacking
          : null,
      retainedBackingUtf16BytesUpperBound =
          left.homogeneousBacking != null &&
              identical(left.homogeneousBacking, right.homogeneousBacking)
          ? left.homogeneousBacking!.retainedUtf16Bytes
          : left.retainedBackingUtf16BytesUpperBound +
                right.retainedBackingUtf16BytesUpperBound,
      backingIdentityBloom =
          left.backingIdentityBloom | right.backingIdentityBloom;

  final _SourceNode left;
  final _SourceNode right;
  @override
  final int utf16Length;
  @override
  final bool isCertified;
  @override
  final int utf8Length;
  @override
  final int newlines;
  @override
  bool get startsWithLineFeed => left.startsWithLineFeed;
  @override
  bool get endsWithCarriageReturn => right.endsWithCarriageReturn;
  @override
  final int height;
  @override
  final FlarkV3ContentHash128 contentHash128;
  @override
  final FlarkV3ContentHash128 hashPower128;
  @override
  final _SourceBacking? homogeneousBacking;
  @override
  final int retainedBackingUtf16BytesUpperBound;
  @override
  final int backingIdentityBloom;
}

final class _CompactionAccumulator {
  _CompactionAccumulator({
    required this.sourceRevision,
    required this.chunkSize,
  });

  final int sourceRevision;
  final int chunkSize;
  final Map<int, _CompactionBackingObservation> _byBacking = {};

  void consider(_SourceLeaf? leaf) {
    if (leaf == null) return;
    final backing = leaf.backing;
    if (backing.source.length < chunkSize * 8 ||
        leaf.utf16Length * 8 > backing.source.length) {
      return;
    }
    final observation = _byBacking.putIfAbsent(
      backing.identity,
      () => _CompactionBackingObservation(backing),
    );
    observation.observedLiveUtf16 += leaf.utf16Length;
  }

  List<FlarkV3SourceCompactionObligation> seal() => List.unmodifiable([
    for (final observation in _byBacking.values)
      if (observation.observedLiveUtf16 * 8 <=
          observation.backing.source.length)
        FlarkV3SourceCompactionObligation._(
          sourceRevision: sourceRevision,
          backingIdentity: observation.backing.identity,
          retainedBackingUtf16Bytes: observation.backing.retainedUtf16Bytes,
          observedLiveUtf16: observation.observedLiveUtf16,
        ),
  ]);
}

final class _CompactionBackingObservation {
  _CompactionBackingObservation(this.backing);

  final _SourceBacking backing;
  int observedLiveUtf16 = 0;
}

_SourceLeaf? _leftmostLeaf(_SourceNode? node) {
  var current = node;
  while (current is _SourceBranch) {
    current = current.left;
  }
  return current as _SourceLeaf?;
}

_SourceLeaf? _rightmostLeaf(_SourceNode? node) {
  var current = node;
  while (current is _SourceBranch) {
    current = current.right;
  }
  return current as _SourceLeaf?;
}

final class _SourceSplit {
  const _SourceSplit(this.left, this.right);

  final _SourceNode? left;
  final _SourceNode? right;
}

final class _SourcePieceKey {
  const _SourcePieceKey(this.pieceId, this.sourceStartUtf16, this.utf16Length);

  final int pieceId;
  final int sourceStartUtf16;
  final int utf16Length;

  @override
  bool operator ==(Object other) =>
      other is _SourcePieceKey &&
      other.pieceId == pieceId &&
      other.sourceStartUtf16 == sourceStartUtf16 &&
      other.utf16Length == utf16Length;

  @override
  int get hashCode => Object.hash(pieceId, sourceStartUtf16, utf16Length);
}

final class _ReplacePendingLeafResult {
  const _ReplacePendingLeafResult(this.node, this.pathNodesVisited);

  final _SourceNode node;
  final int pathNodesVisited;
}

_ReplacePendingLeafResult _replacePendingLeafAt(
  _SourceNode? node,
  int offset,
  FlarkV3SourcePieceToCertify expected,
  FlarkV3CertifiedSourcePiece certification,
) {
  if (node == null) {
    throw StateError('Certification addressed an empty source tree.');
  }
  if (node case final _SourceLeaf leaf) {
    if (offset != 0 ||
        leaf.isCertified ||
        leaf.pieceId != expected.pieceId ||
        leaf.start != expected.sourceStartUtf16 ||
        leaf.utf16Length != expected.utf16Length) {
      throw StateError(
        'Certification addressed a different live source piece.',
      );
    }
    return _ReplacePendingLeafResult(
      _SourceLeaf.certifiedPiece(leaf, certification),
      1,
    );
  }
  final branch = node as _SourceBranch;
  if (offset < branch.left.utf16Length) {
    final replaced = _replacePendingLeafAt(
      branch.left,
      offset,
      expected,
      certification,
    );
    return _ReplacePendingLeafResult(
      _SourceBranch(replaced.node, branch.right),
      replaced.pathNodesVisited + 1,
    );
  }
  final replaced = _replacePendingLeafAt(
    branch.right,
    offset - branch.left.utf16Length,
    expected,
    certification,
  );
  return _ReplacePendingLeafResult(
    _SourceBranch(branch.left, replaced.node),
    replaced.pathNodesVisited + 1,
  );
}

final class _AttachedSourceCertification {
  const _AttachedSourceCertification({
    required this.document,
    required this.pathNodesVisited,
    required this.piecesAttached,
  });

  final FlarkV3SourceDocument document;
  final int pathNodesVisited;
  final int piecesAttached;
}

/// Explicit byte-charged lease for exactly the range deleted by one edit.
///
/// This is not a retained prior-document snapshot: it owns only the isolated
/// middle subtree returned by two logarithmic splits. Undo can splice that
/// immutable range directly without enumerating or rebuilding its leaves.
final class _DeletedRangeSubtreeLease implements FlarkV3SourcePayload {
  const _DeletedRangeSubtreeLease(this.subtree);

  final _SourceNode? subtree;

  @override
  int get utf16Length => subtree?.utf16Length ?? 0;

  int? get homogeneousBackingIdentity => subtree?.homogeneousBacking?.identity;

  int get retainedBackingUtf16Bytes =>
      subtree?.retainedBackingUtf16BytesUpperBound ?? 0;

  int get backingIdentityBloom => subtree?.backingIdentityBloom ?? 0;

  _SourceNode? buildTree() => subtree;

  @override
  String readRange(int startUtf16, int endUtf16) {
    if (startUtf16 < 0 || endUtf16 < startUtf16 || endUtf16 > utf16Length) {
      throw RangeError.range(endUtf16, startUtf16, utf16Length, 'endUtf16');
    }
    final output = StringBuffer();
    _writeRange(subtree, startUtf16, endUtf16, output);
    return output.toString();
  }
}

final class _InverseSourceOperation {
  const _InverseSourceOperation({
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
  });

  final int startUtf16;
  final int endUtf16;
  final _DeletedRangeSubtreeLease replacement;
}

final class _InverseSourceTransaction {
  const _InverseSourceTransaction({
    required this.afterRevision,
    required this.operations,
    required this.metadataByteCharge,
    required this.backingByteCharges,
    required this.conservativeBackingByteCharge,
    required this.backingIdentityBloom,
    required this.capturePathNodesVisited,
  });

  final int afterRevision;
  final List<_InverseSourceOperation> operations;
  final int metadataByteCharge;
  final Map<int, int> backingByteCharges;
  final int conservativeBackingByteCharge;
  final int backingIdentityBloom;
  final int capturePathNodesVisited;
}

final class _SourceInverseHistory {
  _SourceInverseHistory({required this.entryLimit, required this.byteLimit}) {
    if (entryLimit < 1) {
      throw RangeError.range(entryLimit, 1, null, 'historyEntryLimit');
    }
    if (byteLimit < 1) {
      throw RangeError.range(byteLimit, 1, null, 'historyByteLimit');
    }
  }

  final int entryLimit;
  final int byteLimit;
  final List<_InverseSourceTransaction> _entries = [];
  final Map<int, int> _backingReferenceCounts = {};
  final Map<int, int> _backingByteCharges = {};
  final List<int> _bloomBitReferenceCounts = List.filled(64, 0);
  int _metadataByteCharge = 0;
  int _conservativeBackingByteCharge = 0;
  int _uniqueBackingByteCharge = 0;

  int get length => _entries.length;
  int get byteCharge =>
      _metadataByteCharge +
      _conservativeBackingByteCharge +
      _uniqueBackingByteCharge;
  bool get isNotEmpty => _entries.isNotEmpty;

  bool mayRetainBacking(int backingIdentity) =>
      _bloomBitReferenceCounts[backingIdentity & 63] > 0;

  void push(_InverseSourceTransaction transaction) {
    while (_entries.isNotEmpty && byteCharge > byteLimit) {
      _evictOldest();
    }
    _entries.add(transaction);
    _addCharge(transaction);
    while (_entries.length > entryLimit ||
        (_entries.length > 1 && byteCharge > byteLimit)) {
      _evictOldest();
    }
  }

  _InverseSourceTransaction? pop() {
    if (_entries.isEmpty) return null;
    final result = _entries.removeLast();
    _removeCharge(result);
    return result;
  }

  void _evictOldest() {
    final evicted = _entries.removeAt(0);
    _removeCharge(evicted);
  }

  void _addCharge(_InverseSourceTransaction transaction) {
    _metadataByteCharge += transaction.metadataByteCharge;
    _conservativeBackingByteCharge += transaction.conservativeBackingByteCharge;
    for (final entry in transaction.backingByteCharges.entries) {
      final count = _backingReferenceCounts[entry.key] ?? 0;
      _backingReferenceCounts[entry.key] = count + 1;
      if (count == 0) {
        _backingByteCharges[entry.key] = entry.value;
        _uniqueBackingByteCharge += entry.value;
      }
    }
    _adjustBloom(transaction.backingIdentityBloom, 1);
  }

  void _removeCharge(_InverseSourceTransaction transaction) {
    _metadataByteCharge -= transaction.metadataByteCharge;
    _conservativeBackingByteCharge -= transaction.conservativeBackingByteCharge;
    for (final entry in transaction.backingByteCharges.entries) {
      final count = _backingReferenceCounts[entry.key]! - 1;
      if (count == 0) {
        _backingReferenceCounts.remove(entry.key);
        _uniqueBackingByteCharge -= _backingByteCharges.remove(entry.key)!;
      } else {
        _backingReferenceCounts[entry.key] = count;
      }
    }
    _adjustBloom(transaction.backingIdentityBloom, -1);
  }

  void _adjustBloom(int bloom, int delta) {
    for (var bit = 0; bit < 64; bit += 1) {
      if ((bloom & (1 << bit)) != 0) {
        _bloomBitReferenceCounts[bit] += delta;
      }
    }
  }
}

_InverseSourceTransaction _captureInverseTransaction(
  FlarkV3SourceDocument before,
  List<FlarkV3SourceEdit> operations,
  int afterRevision,
) {
  final sorted = <_IndexedSourceEdit>[
    for (var index = 0; index < operations.length; index += 1)
      _IndexedSourceEdit(index, operations[index]),
  ]..sort(_compareIndexedOperations);
  final inverse = <_InverseSourceOperation>[];
  final traversalWork = _TreeTraversalWork();
  final backingByteCharges = <int, int>{};
  var delta = 0;
  var metadataByteCharge = 32;
  var conservativeBackingByteCharge = 0;
  var backingIdentityBloom = 0;
  for (final indexed in sorted) {
    final operation = indexed.operation;
    final replacement = _captureDeletedRangeLease(
      before._root,
      operation.startUtf16,
      operation.endUtf16,
      before._chunkSize,
      traversalWork,
    );
    final nextStart = operation.startUtf16 + delta;
    inverse.add(
      _InverseSourceOperation(
        startUtf16: nextStart,
        endUtf16: nextStart + operation.replacement.length,
        replacement: replacement,
      ),
    );
    metadataByteCharge += 32;
    backingIdentityBloom |= replacement.backingIdentityBloom;
    final homogeneousBackingIdentity = replacement.homogeneousBackingIdentity;
    if (homogeneousBackingIdentity == null) {
      conservativeBackingByteCharge += replacement.retainedBackingUtf16Bytes;
    } else {
      backingByteCharges[homogeneousBackingIdentity] =
          replacement.retainedBackingUtf16Bytes;
    }
    delta +=
        operation.replacement.length -
        (operation.endUtf16 - operation.startUtf16);
  }
  return _InverseSourceTransaction(
    afterRevision: afterRevision,
    operations: List.unmodifiable(inverse),
    metadataByteCharge: metadataByteCharge,
    backingByteCharges: Map.unmodifiable(backingByteCharges),
    conservativeBackingByteCharge: conservativeBackingByteCharge,
    backingIdentityBloom: backingIdentityBloom,
    capturePathNodesVisited: traversalWork.nodesVisited,
  );
}

final class _TreeTraversalWork {
  int nodesVisited = 0;
}

_DeletedRangeSubtreeLease _captureDeletedRangeLease(
  _SourceNode? root,
  int start,
  int end,
  int chunkSize,
  _TreeTraversalWork work,
) {
  if (start == end || root == null) {
    return const _DeletedRangeSubtreeLease(null);
  }
  final first = _split(root, start, chunkSize, work: work);
  final middle = _split(first.right, end - start, chunkSize, work: work).left;
  return _DeletedRangeSubtreeLease(middle);
}

final class _IndexedSourceEdit {
  const _IndexedSourceEdit(this.index, this.operation);

  final int index;
  final FlarkV3SourceEdit operation;
}

final class _PreparedSourceEdit {
  const _PreparedSourceEdit({
    required this.indexed,
    required this.replacementRoot,
  });

  final _IndexedSourceEdit indexed;
  final _SourceNode? replacementRoot;
}

final class _PreparedSource {
  const _PreparedSource({
    required this.root,
    required this.utf8,
    required this.encodedChunks,
  });

  final _SourceNode? root;
  final Uint8List utf8;
  final int encodedChunks;
}

const int _mask32 = 0xFFFFFFFF;
const int _hashBase0 = 0x00100193;
const int _hashBase1 = 0x9E3779B1;
const int _hashBase2 = 0x85EBCA77;
const int _hashBase3 = 0xC2B2AE3D;

final class _SourceRangeSummary {
  const _SourceRangeSummary({
    required this.utf8Length,
    required this.newlines,
    required this.hash,
    required this.power,
  });

  final int utf8Length;
  final int newlines;
  final FlarkV3ContentHash128 hash;
  final FlarkV3ContentHash128 power;
}

/// Numeric cumulative source facts at one exact backing UTF-16 offset.
///
/// Source-fact transports move only these values. They never move or retain a
/// source [String].
final class FlarkV3SourcePrefixFacts {
  const FlarkV3SourcePrefixFacts({
    required this.utf16Offset,
    required this.utf8Offset,
    required this.newlines,
    required this.hash,
  });

  final int utf16Offset;
  final int utf8Offset;
  final int newlines;
  final FlarkV3ContentHash128 hash;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3SourcePrefixFacts &&
      other.utf16Offset == utf16Offset &&
      other.utf8Offset == utf8Offset &&
      other.newlines == newlines &&
      other.hash == hash;

  @override
  int get hashCode => Object.hash(utf16Offset, utf8Offset, newlines, hash);
}

const FlarkV3SourcePrefixFacts _sourcePrefixOrigin = FlarkV3SourcePrefixFacts(
  utf16Offset: 0,
  utf8Offset: 0,
  newlines: 0,
  hash: FlarkV3ContentHash128.zero,
);

FlarkV3SourcePrefixFacts _appendRelativeSourcePrefixFacts(
  FlarkV3SourcePrefixFacts prefix,
  FlarkV3SourcePrefixFacts relative,
) => FlarkV3SourcePrefixFacts(
  utf16Offset: prefix.utf16Offset + relative.utf16Offset,
  utf8Offset: prefix.utf8Offset + relative.utf8Offset,
  newlines: prefix.newlines + relative.newlines,
  hash: _appendHash128(
    prefix.hash,
    relative.hash,
    _powHash128(relative.utf8Offset),
  ),
);

FlarkV3SourcePrefixFacts _relativeSourcePrefixFacts(
  FlarkV3SourcePrefixFacts absolute,
  FlarkV3SourcePrefixFacts prefix,
) {
  final utf16Length = absolute.utf16Offset - prefix.utf16Offset;
  final utf8Length = absolute.utf8Offset - prefix.utf8Offset;
  final newlines = absolute.newlines - prefix.newlines;
  if (utf16Length <= 0 || utf8Length < 0 || newlines < 0) {
    throw StateError('Checkpoint page regresses from its page prefix.');
  }
  final power = _powHash128(utf8Length);
  return FlarkV3SourcePrefixFacts(
    utf16Offset: utf16Length,
    utf8Offset: utf8Length,
    newlines: newlines,
    hash: _subtractHash128(absolute.hash, prefix.hash, power),
  );
}

sealed class _SourceCheckpointNode {
  const _SourceCheckpointNode();

  int get count;
  int get pageCount;
  int get height;
  FlarkV3SourcePrefixFacts get summary;
}

/// One bounded page whose list ownership has moved from a scanner/page into
/// the range index. Facts are relative to the page start, so this exact leaf
/// remains reusable when an earlier edit shifts global offsets and hashes.
final class _SourceCheckpointLeaf extends _SourceCheckpointNode {
  const _SourceCheckpointLeaf(this.facts);

  final List<FlarkV3SourcePrefixFacts> facts;

  @override
  int get count => facts.length;

  @override
  int get pageCount => 1;

  @override
  int get height => 1;

  @override
  FlarkV3SourcePrefixFacts get summary => facts.last;
}

final class _SourceCheckpointBranch extends _SourceCheckpointNode {
  _SourceCheckpointBranch(this.left, this.right)
    : count = left.count + right.count,
      pageCount = left.pageCount + right.pageCount,
      summary = _appendRelativeSourcePrefixFacts(left.summary, right.summary),
      height = math.max(left.height, right.height) + 1;

  final _SourceCheckpointNode left;
  final _SourceCheckpointNode right;

  @override
  final int count;

  @override
  final int pageCount;

  @override
  final int height;

  @override
  final FlarkV3SourcePrefixFacts summary;
}

FlarkV3SourcePrefixFacts _checkpointAt(
  _SourceCheckpointNode node,
  int index,
  FlarkV3SourcePrefixFacts prefix,
) {
  if (node case final _SourceCheckpointLeaf leaf) {
    return _appendRelativeSourcePrefixFacts(prefix, leaf.facts[index]);
  }
  final branch = node as _SourceCheckpointBranch;
  if (index < branch.left.count) {
    return _checkpointAt(branch.left, index, prefix);
  }
  return _checkpointAt(
    branch.right,
    index - branch.left.count,
    _appendRelativeSourcePrefixFacts(prefix, branch.left.summary),
  );
}

final class _PagedSourceCheckpointStore {
  const _PagedSourceCheckpointStore._(this._root, this._anchor);

  final _SourceCheckpointNode? _root;
  final FlarkV3SourcePrefixFacts _anchor;

  /// Index zero is the implicit all-zero document-prefix anchor.
  int get length => (_root?.count ?? 0) + 1;
  int get pageCount => _root?.pageCount ?? 0;
  int get pageTreeHeight => _root?.height ?? 0;
  FlarkV3SourcePrefixFacts get terminalFacts => _root == null
      ? _anchor
      : _appendRelativeSourcePrefixFacts(_anchor, _root.summary);

  FlarkV3SourcePrefixFacts operator [](int index) {
    if (index < 0 || index >= length) {
      throw RangeError.index(index, this, 'index', null, length);
    }
    if (index == 0) return _anchor;
    return _checkpointAt(_root!, index - 1, _anchor);
  }
}

final class _SourceCheckpointTreeWork {
  int nodesAllocated = 0;
}

final class _SourceCheckpointSplit {
  const _SourceCheckpointSplit(this.left, this.right);

  final _SourceCheckpointNode? left;
  final _SourceCheckpointNode? right;
}

_SourceCheckpointBranch _checkpointBranch(
  _SourceCheckpointNode left,
  _SourceCheckpointNode right,
  _SourceCheckpointTreeWork? work,
) {
  work?.nodesAllocated += 1;
  return _SourceCheckpointBranch(left, right);
}

_SourceCheckpointNode? _joinSourceCheckpointTrees(
  _SourceCheckpointNode? left,
  _SourceCheckpointNode? right, {
  _SourceCheckpointTreeWork? work,
}) {
  if (left == null) return right;
  if (right == null) return left;
  if (left.height > right.height + 1) {
    final branch = left as _SourceCheckpointBranch;
    return _balanceSourceCheckpointTree(
      _checkpointBranch(
        branch.left,
        _joinSourceCheckpointTrees(branch.right, right, work: work)!,
        work,
      ),
      work,
    );
  }
  if (right.height > left.height + 1) {
    final branch = right as _SourceCheckpointBranch;
    return _balanceSourceCheckpointTree(
      _checkpointBranch(
        _joinSourceCheckpointTrees(left, branch.left, work: work)!,
        branch.right,
        work,
      ),
      work,
    );
  }
  return _checkpointBranch(left, right, work);
}

_SourceCheckpointNode _balanceSourceCheckpointTree(
  _SourceCheckpointBranch node,
  _SourceCheckpointTreeWork? work,
) {
  final balance = node.left.height - node.right.height;
  if (balance > 1) {
    final left = node.left as _SourceCheckpointBranch;
    if (left.left.height < left.right.height) {
      final pivot = left.right as _SourceCheckpointBranch;
      return _checkpointBranch(
        _checkpointBranch(left.left, pivot.left, work),
        _checkpointBranch(pivot.right, node.right, work),
        work,
      );
    }
    return _checkpointBranch(
      left.left,
      _checkpointBranch(left.right, node.right, work),
      work,
    );
  }
  if (balance < -1) {
    final right = node.right as _SourceCheckpointBranch;
    if (right.right.height < right.left.height) {
      final pivot = right.left as _SourceCheckpointBranch;
      return _checkpointBranch(
        _checkpointBranch(node.left, pivot.left, work),
        _checkpointBranch(pivot.right, right.right, work),
        work,
      );
    }
    return _checkpointBranch(
      _checkpointBranch(node.left, right.left, work),
      right.right,
      work,
    );
  }
  return node;
}

_SourceCheckpointSplit _splitSourceCheckpointPages(
  _SourceCheckpointNode? node,
  int pageOrdinal, {
  _SourceCheckpointTreeWork? work,
}) {
  final pageCount = node?.pageCount ?? 0;
  if (pageOrdinal < 0 || pageOrdinal > pageCount) {
    throw RangeError.range(pageOrdinal, 0, pageCount, 'pageOrdinal');
  }
  if (node == null || pageOrdinal == 0) {
    return _SourceCheckpointSplit(null, node);
  }
  if (pageOrdinal == pageCount) {
    return _SourceCheckpointSplit(node, null);
  }
  final branch = node as _SourceCheckpointBranch;
  if (pageOrdinal < branch.left.pageCount) {
    final split = _splitSourceCheckpointPages(
      branch.left,
      pageOrdinal,
      work: work,
    );
    return _SourceCheckpointSplit(
      split.left,
      _joinSourceCheckpointTrees(split.right, branch.right, work: work),
    );
  }
  if (pageOrdinal == branch.left.pageCount) {
    return _SourceCheckpointSplit(branch.left, branch.right);
  }
  final split = _splitSourceCheckpointPages(
    branch.right,
    pageOrdinal - branch.left.pageCount,
    work: work,
  );
  return _SourceCheckpointSplit(
    _joinSourceCheckpointTrees(branch.left, split.left, work: work),
    split.right,
  );
}

/// Binary-carry owner for page-relative checkpoint leaves.
final class _SourceCheckpointPageForestBuilder {
  List<_SourceCheckpointNode?>? _forest = <_SourceCheckpointNode?>[];
  int acceptedCheckpointCount = 0;
  int acceptedPageCount = 0;

  void addOwnedAbsolutePage(
    List<FlarkV3SourcePrefixFacts> facts, {
    required FlarkV3SourcePrefixFacts pagePrefix,
  }) {
    final forest = _forest;
    if (forest == null) {
      throw StateError('The checkpoint page forest is already sealed.');
    }
    if (facts.isEmpty || facts.length > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        facts.length,
        1,
        _maximumWorkerSyncPageEntries,
        'checkpointCount',
      );
    }
    final relative = <FlarkV3SourcePrefixFacts>[
      for (final fact in facts) _relativeSourcePrefixFacts(fact, pagePrefix),
    ];
    _SourceCheckpointNode carry = _SourceCheckpointLeaf(relative);
    var level = 0;
    while (level < forest.length && forest[level] != null) {
      carry = _SourceCheckpointBranch(forest[level]!, carry);
      forest[level] = null;
      level += 1;
    }
    if (level == forest.length) {
      forest.add(carry);
    } else {
      forest[level] = carry;
    }
    acceptedCheckpointCount += facts.length;
    acceptedPageCount += 1;
  }

  _SourceCheckpointNode? seal() {
    final forest = _forest;
    if (forest == null) {
      throw StateError('The checkpoint page forest is already sealed.');
    }
    _forest = null;
    _SourceCheckpointNode? root;
    for (var level = forest.length - 1; level >= 0; level -= 1) {
      final subtree = forest[level];
      if (subtree == null) continue;
      root = root == null ? subtree : _SourceCheckpointBranch(root, subtree);
    }
    return root;
  }
}

/// Append-only owner for bounded checkpoint pages.
///
/// [_seal] transfers the balanced page root in O(1). The builder is unusable
/// afterwards, making ownership explicit and preventing a final flat copy.
final class _SourceRangeIndexBuilder {
  _SourceRangeIndexBuilder({
    required this.start,
    required this.end,
    required this.spacingUtf16,
  }) {
    if (start < 0 || end < start) {
      throw RangeError('Invalid source range [$start, $end).');
    }
    if (spacingUtf16 < 2 || spacingUtf16 > _maximumSourceChunkUtf16) {
      throw RangeError.range(
        spacingUtf16,
        2,
        _maximumSourceChunkUtf16,
        'spacingUtf16',
      );
    }
    _anchor = FlarkV3SourcePrefixFacts(
      utf16Offset: start,
      utf8Offset: 0,
      newlines: 0,
      hash: FlarkV3ContentHash128.zero,
    );
    _last = _anchor;
  }

  final int start;
  final int end;
  final int spacingUtf16;

  final _SourceCheckpointPageForestBuilder _pageBuilder =
      _SourceCheckpointPageForestBuilder();
  late final FlarkV3SourcePrefixFacts _anchor;
  late FlarkV3SourcePrefixFacts _last;
  bool _sealed = false;

  int get nextUtf16Offset => _last.utf16Offset;
  int get acceptedCheckpointCount => _pageBuilder.acceptedCheckpointCount;
  int get acceptedPageCount => _pageBuilder.acceptedPageCount;

  void addOwnedPage(List<FlarkV3SourcePrefixFacts> facts) {
    if (_sealed) throw StateError('The source range index is already sealed.');
    if (facts.isEmpty || facts.length > _maximumWorkerSyncPageEntries) {
      throw RangeError.range(
        facts.length,
        1,
        _maximumWorkerSyncPageEntries,
        'checkpointCount',
      );
    }
    var previous = _last;
    for (final fact in facts) {
      if (fact.utf16Offset <= previous.utf16Offset ||
          fact.utf16Offset > end ||
          fact.utf8Offset < previous.utf8Offset ||
          fact.newlines < previous.newlines) {
        throw StateError('Checkpoint page is non-contiguous or regresses.');
      }
      previous = fact;
    }
    _pageBuilder.addOwnedAbsolutePage(facts, pagePrefix: _last);
    _last = previous;
  }

  _SourceRangeIndex _seal() {
    if (_sealed) throw StateError('The source range index is already sealed.');
    if (_last.utf16Offset != end) {
      throw StateError(
        'Checkpoint pages ended at ${_last.utf16Offset}, expected $end.',
      );
    }
    _sealed = true;
    final root = _pageBuilder.seal();
    return _SourceRangeIndex._(
      start: start,
      end: end,
      spacingUtf16: spacingUtf16,
      checkpoints: _PagedSourceCheckpointStore._(root, _anchor),
    );
  }
}

/// Sparse facts for one worker-certified backing range.
///
/// Every query scans at most one checkpoint interval. The index deliberately
/// retains exact backing coordinates so later leaf splits remain bounded and
/// do not copy or rescan a giant certified source.
final class _SourceRangeIndex {
  const _SourceRangeIndex._({
    required this.start,
    required this.end,
    required this.spacingUtf16,
    required this.checkpoints,
  });

  /// Explicitly bounded compatibility helper. Production staged work uses
  /// [FlarkV3SourceFactScanner] over the original backing and never enters this
  /// fragment route.
  factory _SourceRangeIndex.scanFragmentForBacking(
    String sourceFragment, {
    required int backingStartUtf16,
    int spacingUtf16 = 4096,
    int globalStartUtf16 = 0,
  }) {
    final builder = _SourceRangeIndexBuilder(
      start: backingStartUtf16,
      end: backingStartUtf16 + sourceFragment.length,
      spacingUtf16: spacingUtf16,
    );
    var relative = const FlarkV3SourcePrefixFacts(
      utf16Offset: 0,
      utf8Offset: 0,
      newlines: 0,
      hash: FlarkV3ContentHash128.zero,
    );
    var nextCheckpoint = spacingUtf16;
    while (relative.utf16Offset < sourceFragment.length) {
      relative = _advanceSourcePrefix(
        sourceFragment,
        relative,
        sourceFragment.length,
        math.min(
          sourceFragment.length,
          math.max(nextCheckpoint, relative.utf16Offset + 1),
        ),
        validationGlobalStart: globalStartUtf16,
        validationRangeStart: 0,
      );
      builder.addOwnedPage(<FlarkV3SourcePrefixFacts>[
        FlarkV3SourcePrefixFacts(
          utf16Offset: relative.utf16Offset + backingStartUtf16,
          utf8Offset: relative.utf8Offset,
          newlines: relative.newlines,
          hash: relative.hash,
        ),
      ]);
      nextCheckpoint = relative.utf16Offset + spacingUtf16;
    }
    return builder._seal();
  }

  final int start;
  final int end;
  final int spacingUtf16;
  final _PagedSourceCheckpointStore checkpoints;

  FlarkV3SourcePrefixFacts checkpointAtOrBeforeUtf16(int offset) {
    var low = 0;
    var high = checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (checkpoints[middle].utf16Offset <= offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return checkpoints[low - 1];
  }

  FlarkV3SourcePrefixFacts checkpointAtOrBeforeUtf8(int offset) {
    var low = 0;
    var high = checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (checkpoints[middle].utf8Offset <= offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return checkpoints[low - 1];
  }

  FlarkV3SourcePrefixFacts checkpointBeforeNewline(int target) {
    var low = 0;
    var high = checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (checkpoints[middle].newlines < target) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return checkpoints[math.max(0, low - 1)];
  }

  FlarkV3SourcePrefixFacts _prefix(String source, int offset) {
    if (offset < start || offset > end) {
      throw RangeError.range(offset, start, end, 'offset');
    }
    var low = 0;
    var high = checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (checkpoints[middle].utf16Offset <= offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    final checkpoint = checkpoints[low - 1];
    return checkpoint.utf16Offset == offset
        ? checkpoint
        : _advanceSourcePrefix(source, checkpoint, end, offset);
  }

  int utf8Before(String source, int offset) =>
      _prefix(source, offset).utf8Offset;

  int newlinesInRange(String source, int rangeStart, int rangeEnd) {
    final before = _prefix(source, rangeStart).newlines;
    final after = _prefix(source, rangeEnd).newlines;
    final endsInsideCrLf =
        rangeEnd > rangeStart &&
        rangeEnd < end &&
        source.codeUnitAt(rangeEnd - 1) == 0x0D &&
        source.codeUnitAt(rangeEnd) == 0x0A;
    return after - before + (endsInsideCrLf ? 1 : 0);
  }

  _SourceRangeSummary summary(String source, int rangeStart, int rangeEnd) {
    final before = _prefix(source, rangeStart);
    final after = _prefix(source, rangeEnd);
    final utf8Length = after.utf8Offset - before.utf8Offset;
    final power = _powHash128(utf8Length);
    return _SourceRangeSummary(
      utf8Length: utf8Length,
      newlines: newlinesInRange(source, rangeStart, rangeEnd),
      hash: _subtractHash128(after.hash, before.hash, power),
      power: power,
    );
  }

  int utf16AtUtf8(
    String source,
    int rangeStart,
    int rangeEnd,
    int absoluteUtf8,
  ) {
    final startFacts = _prefix(source, rangeStart);
    final endFacts = _prefix(source, rangeEnd);
    if (absoluteUtf8 < startFacts.utf8Offset ||
        absoluteUtf8 > endFacts.utf8Offset) {
      throw RangeError.range(
        absoluteUtf8,
        startFacts.utf8Offset,
        endFacts.utf8Offset,
        'utf8Offset',
      );
    }
    var low = 0;
    var high = checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (checkpoints[middle].utf8Offset <= absoluteUtf8) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    var facts = checkpoints[low - 1];
    if (facts.utf16Offset < rangeStart) facts = startFacts;
    while (facts.utf8Offset < absoluteUtf8) {
      final codeUnit = source.codeUnitAt(facts.utf16Offset);
      final scalarWidth = _isHighSurrogate(codeUnit) ? 2 : 1;
      final next = _advanceSourcePrefix(
        source,
        facts,
        end,
        math.min(rangeEnd, facts.utf16Offset + scalarWidth),
      );
      if (next.utf8Offset > absoluteUtf8) {
        throw FormatException(
          'UTF-8 offset $absoluteUtf8 divides a scalar value.',
        );
      }
      facts = next;
    }
    if (facts.utf8Offset != absoluteUtf8) {
      throw FormatException(
        'UTF-8 offset $absoluteUtf8 divides a scalar value.',
      );
    }
    return facts.utf16Offset;
  }

  int offsetAfterNthNewline(
    String source,
    int rangeStart,
    int rangeEnd,
    int count,
  ) {
    final before = _prefix(source, rangeStart).newlines;
    final atEnd = _prefix(source, rangeEnd).newlines;
    final indexedCount = atEnd - before;
    if (count > indexedCount) {
      if (count == indexedCount + 1 &&
          rangeEnd > rangeStart &&
          rangeEnd < end &&
          source.codeUnitAt(rangeEnd - 1) == 0x0D &&
          source.codeUnitAt(rangeEnd) == 0x0A) {
        return rangeEnd;
      }
      throw RangeError.range(
        count,
        1,
        newlinesInRange(source, rangeStart, rangeEnd),
      );
    }
    final target = before + count;
    var low = 0;
    var high = checkpoints.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (checkpoints[middle].newlines < target) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    var cursor = low == 0 ? rangeStart : checkpoints[low - 1].utf16Offset;
    if (cursor < rangeStart) cursor = rangeStart;
    var seen = _prefix(source, cursor).newlines;
    while (cursor < rangeEnd) {
      final codeUnit = source.codeUnitAt(cursor);
      if (codeUnit == 0x0D) {
        if (cursor + 1 < end && source.codeUnitAt(cursor + 1) == 0x0A) {
          cursor += 2;
          seen += 1;
          if (seen == target) return cursor;
          continue;
        } else {
          seen += 1;
        }
      } else if (codeUnit == 0x0A) {
        seen += 1;
      }
      cursor += 1;
      if (seen == target) return cursor;
    }
    throw StateError('Certified newline index did not contain count $count.');
  }
}

/// Installed global SourceFacts authority for one exact document revision.
///
/// Promotion itself only adopts numeric Rust output. Exact offset queries may
/// inspect at most one checkpoint interval of the Dart-owned UTF-16 source;
/// they never materialize or rescan the whole document.
final class _CanonicalSourceFacts {
  const _CanonicalSourceFacts({
    required this.authority,
    required this.fingerprintAlgorithm,
    required this.fingerprint,
    required this.logicalLineBreaks,
    required this.checkpointHash128,
    required this.checkpointCount,
    required this.pageCount,
    required this.index,
  });

  final FlarkV3CanonicalSourceFactAuthority authority;
  final int fingerprintAlgorithm;
  final FlarkV3SourceFingerprint fingerprint;
  final int logicalLineBreaks;
  final FlarkV3ContentHash128 checkpointHash128;
  final int checkpointCount;
  final int pageCount;
  final _SourceRangeIndex index;

  FlarkV3SourcePrefixFacts prefixAtUtf16(
    FlarkV3SourceDocument document,
    int offset,
  ) {
    final checkpoint = index.checkpointAtOrBeforeUtf16(offset);
    if (checkpoint.utf16Offset == offset) return checkpoint;
    final readEnd = math.min(document.utf16Length, offset + 1);
    final fragment = document.readRange(checkpoint.utf16Offset, readEnd);
    final advanced = _advanceSourcePrefix(
      fragment,
      FlarkV3SourcePrefixFacts(
        utf16Offset: 0,
        utf8Offset: checkpoint.utf8Offset,
        newlines: checkpoint.newlines,
        hash: checkpoint.hash,
      ),
      fragment.length,
      offset - checkpoint.utf16Offset,
    );
    return FlarkV3SourcePrefixFacts(
      utf16Offset: checkpoint.utf16Offset + advanced.utf16Offset,
      utf8Offset: advanced.utf8Offset,
      newlines: advanced.newlines,
      hash: advanced.hash,
    );
  }

  int utf16AtUtf8(FlarkV3SourceDocument document, int utf8Offset) {
    final checkpoint = index.checkpointAtOrBeforeUtf8(utf8Offset);
    if (checkpoint.utf8Offset == utf8Offset) return checkpoint.utf16Offset;
    final readEnd = math.min(
      document.utf16Length,
      checkpoint.utf16Offset + index.spacingUtf16 + 2,
    );
    final fragment = document.readRange(checkpoint.utf16Offset, readEnd);
    var localUtf16 = 0;
    var currentUtf8 = checkpoint.utf8Offset;
    while (currentUtf8 < utf8Offset && localUtf16 < fragment.length) {
      final width = _utf8Width(fragment, localUtf16, fragment.length);
      if (currentUtf8 + width > utf8Offset) {
        throw FormatException(
          'UTF-8 offset $utf8Offset divides a scalar value.',
        );
      }
      currentUtf8 += width;
      localUtf16 += _scalarUtf16Width(fragment, localUtf16, fragment.length);
    }
    if (currentUtf8 != utf8Offset) {
      throw StateError('Canonical source facts did not cover UTF-8 offset.');
    }
    return checkpoint.utf16Offset + localUtf16;
  }

  int offsetAfterNthNewline(FlarkV3SourceDocument document, int target) {
    final checkpoint = index.checkpointBeforeNewline(target);
    final readEnd = math.min(
      document.utf16Length,
      checkpoint.utf16Offset + index.spacingUtf16 + 2,
    );
    final fragment = document.readRange(checkpoint.utf16Offset, readEnd);
    var cursor = 0;
    var seen = checkpoint.newlines;
    while (cursor < fragment.length) {
      final codeUnit = fragment.codeUnitAt(cursor);
      if (codeUnit == 0x0D) {
        if (cursor + 1 < fragment.length &&
            fragment.codeUnitAt(cursor + 1) == 0x0A) {
          cursor += 2;
        } else {
          cursor += 1;
        }
        seen += 1;
      } else {
        cursor += 1;
        if (codeUnit == 0x0A) seen += 1;
      }
      if (seen == target) return checkpoint.utf16Offset + cursor;
    }
    throw StateError('Canonical source facts did not contain line $target.');
  }
}

FlarkV3SourcePrefixFacts _advanceSourcePrefix(
  String source,
  FlarkV3SourcePrefixFacts initial,
  int indexedEnd,
  int requestedEnd, {
  int? validationGlobalStart,
  int? validationRangeStart,
}) {
  var cursor = initial.utf16Offset;
  var utf8Offset = initial.utf8Offset;
  var newlines = initial.newlines;
  var hash0 = initial.hash.word0;
  var hash1 = initial.hash.word1;
  var hash2 = initial.hash.word2;
  var hash3 = initial.hash.word3;

  void appendByte(int byte) {
    final value = byte + 1;
    hash0 = (_mul32(hash0, _hashBase0) + value) & _mask32;
    hash1 = (_mul32(hash1, _hashBase1) + value) & _mask32;
    hash2 = (_mul32(hash2, _hashBase2) + value) & _mask32;
    hash3 = (_mul32(hash3, _hashBase3) + value) & _mask32;
    utf8Offset += 1;
  }

  while (cursor < requestedEnd) {
    final codeUnit = source.codeUnitAt(cursor);
    late final int codePoint;
    late final int width;
    if (_isHighSurrogate(codeUnit)) {
      if (cursor + 1 >= indexedEnd ||
          !_isLowSurrogate(source.codeUnitAt(cursor + 1))) {
        final relative = cursor - (validationRangeStart ?? initial.utf16Offset);
        throw FlarkV3SourceCertificationFailure(
          utf16Offset: (validationGlobalStart ?? 0) + relative,
        );
      }
      if (cursor + 2 > requestedEnd) {
        requestedEnd = cursor + 2;
      }
      codePoint =
          0x10000 +
          ((codeUnit - 0xD800) << 10) +
          (source.codeUnitAt(cursor + 1) - 0xDC00);
      width = 2;
    } else if (_isLowSurrogate(codeUnit)) {
      final relative = cursor - (validationRangeStart ?? initial.utf16Offset);
      throw FlarkV3SourceCertificationFailure(
        utf16Offset: (validationGlobalStart ?? 0) + relative,
      );
    } else {
      codePoint = codeUnit;
      width = 1;
    }
    if (codeUnit == 0x0D) {
      if (cursor + 1 >= indexedEnd || source.codeUnitAt(cursor + 1) != 0x0A) {
        newlines += 1;
      }
    } else if (codeUnit == 0x0A) {
      newlines += 1;
    }
    if (codePoint <= 0x7F) {
      appendByte(codePoint);
    } else if (codePoint <= 0x7FF) {
      appendByte(0xC0 | (codePoint >>> 6));
      appendByte(0x80 | (codePoint & 0x3F));
    } else if (codePoint <= 0xFFFF) {
      appendByte(0xE0 | (codePoint >>> 12));
      appendByte(0x80 | ((codePoint >>> 6) & 0x3F));
      appendByte(0x80 | (codePoint & 0x3F));
    } else {
      appendByte(0xF0 | (codePoint >>> 18));
      appendByte(0x80 | ((codePoint >>> 12) & 0x3F));
      appendByte(0x80 | ((codePoint >>> 6) & 0x3F));
      appendByte(0x80 | (codePoint & 0x3F));
    }
    cursor += width;
  }
  return FlarkV3SourcePrefixFacts(
    utf16Offset: cursor,
    utf8Offset: utf8Offset,
    newlines: newlines,
    hash: FlarkV3ContentHash128(hash0, hash1, hash2, hash3),
  );
}

FlarkV3ContentHash128 _appendHashByte(FlarkV3ContentHash128 hash, int byte) {
  final value = byte + 1;
  return FlarkV3ContentHash128(
    (_mul32(hash.word0, _hashBase0) + value) & _mask32,
    (_mul32(hash.word1, _hashBase1) + value) & _mask32,
    (_mul32(hash.word2, _hashBase2) + value) & _mask32,
    (_mul32(hash.word3, _hashBase3) + value) & _mask32,
  );
}

FlarkV3ContentHash128 _powHash128(int exponent) => FlarkV3ContentHash128(
  _pow32(_hashBase0, exponent),
  _pow32(_hashBase1, exponent),
  _pow32(_hashBase2, exponent),
  _pow32(_hashBase3, exponent),
);

int _pow32(int base, int exponent) {
  var result = 1;
  var factor = base;
  var remaining = exponent;
  while (remaining > 0) {
    if (remaining.isOdd) result = _mul32(result, factor);
    factor = _mul32(factor, factor);
    remaining >>>= 1;
  }
  return result;
}

FlarkV3ContentHash128 _subtractHash128(
  FlarkV3ContentHash128 after,
  FlarkV3ContentHash128 before,
  FlarkV3ContentHash128 power,
) => FlarkV3ContentHash128(
  (after.word0 - _mul32(before.word0, power.word0)) & _mask32,
  (after.word1 - _mul32(before.word1, power.word1)) & _mask32,
  (after.word2 - _mul32(before.word2, power.word2)) & _mask32,
  (after.word3 - _mul32(before.word3, power.word3)) & _mask32,
);

final class _HashAndPower128 {
  const _HashAndPower128(this.hash, this.power);

  final FlarkV3ContentHash128 hash;
  final FlarkV3ContentHash128 power;
}

_HashAndPower128 _hashBytes128(Uint8List bytes) {
  var hash0 = 0;
  var hash1 = 0;
  var hash2 = 0;
  var hash3 = 0;
  var power0 = 1;
  var power1 = 1;
  var power2 = 1;
  var power3 = 1;
  for (final byte in bytes) {
    final value = byte + 1;
    hash0 = (_mul32(hash0, _hashBase0) + value) & _mask32;
    hash1 = (_mul32(hash1, _hashBase1) + value) & _mask32;
    hash2 = (_mul32(hash2, _hashBase2) + value) & _mask32;
    hash3 = (_mul32(hash3, _hashBase3) + value) & _mask32;
    power0 = _mul32(power0, _hashBase0);
    power1 = _mul32(power1, _hashBase1);
    power2 = _mul32(power2, _hashBase2);
    power3 = _mul32(power3, _hashBase3);
  }
  return _HashAndPower128(
    FlarkV3ContentHash128(hash0, hash1, hash2, hash3),
    FlarkV3ContentHash128(power0, power1, power2, power3),
  );
}

FlarkV3ContentHash128 _appendHash128(
  FlarkV3ContentHash128 left,
  FlarkV3ContentHash128 right,
  FlarkV3ContentHash128 rightPower,
) => FlarkV3ContentHash128(
  (_mul32(left.word0, rightPower.word0) + right.word0) & _mask32,
  (_mul32(left.word1, rightPower.word1) + right.word1) & _mask32,
  (_mul32(left.word2, rightPower.word2) + right.word2) & _mask32,
  (_mul32(left.word3, rightPower.word3) + right.word3) & _mask32,
);

FlarkV3ContentHash128 _multiplyHash128(
  FlarkV3ContentHash128 left,
  FlarkV3ContentHash128 right,
) => FlarkV3ContentHash128(
  _mul32(left.word0, right.word0),
  _mul32(left.word1, right.word1),
  _mul32(left.word2, right.word2),
  _mul32(left.word3, right.word3),
);

_SourceNode? _treeFromString(String source, int chunkSize) {
  return _prepareSource(source, chunkSize).root;
}

_PreparedSource _prepareSource(
  String source,
  int chunkSize, {
  bool collectUtf8 = false,
}) {
  if (source.isEmpty) {
    return _PreparedSource(root: null, utf8: Uint8List(0), encodedChunks: 0);
  }
  final leaves = <_SourceNode>[];
  final encodedOutput = collectUtf8 ? BytesBuilder(copy: false) : null;
  var start = 0;
  var encodedChunks = 0;
  while (start < source.length) {
    var end = math.min(start + chunkSize, source.length);
    if (end < source.length &&
        _isHighSurrogate(source.codeUnitAt(end - 1)) &&
        _isLowSurrogate(source.codeUnitAt(end))) {
      end -= 1;
    }
    if (end == start) end = math.min(start + 2, source.length);
    // Own each bounded chunk. Leaves must not retain one giant ingest or paste
    // string after most of its content has been deleted.
    final owned = String.fromCharCodes(source.codeUnits.sublist(start, end));
    final encoded = utf8.encode(owned);
    leaves.add(_SourceLeaf.ownedEncoded(owned, encoded));
    encodedOutput?.add(encoded);
    encodedChunks += 1;
    start = end;
  }
  return _PreparedSource(
    root: _buildBalanced(leaves, 0, leaves.length),
    utf8: encodedOutput?.takeBytes() ?? Uint8List(0),
    encodedChunks: encodedChunks,
  );
}

_SourceNode? _buildBalanced(List<_SourceNode> nodes, int start, int end) {
  if (start >= end) return null;
  if (end - start == 1) return nodes[start];
  final middle = start + ((end - start) >> 1);
  return _SourceBranch(
    _buildBalanced(nodes, start, middle)!,
    _buildBalanced(nodes, middle, end)!,
  );
}

_SourceSplit _split(
  _SourceNode? node,
  int offset,
  int chunkSize, {
  _TreeTraversalWork? work,
}) {
  if (node == null) return const _SourceSplit(null, null);
  work?.nodesVisited += 1;
  if (offset == 0) return _SourceSplit(null, node);
  if (offset == node.utf16Length) return _SourceSplit(node, null);

  if (node case final _SourceLeaf leaf) {
    final previous = leaf.source.codeUnitAt(leaf.start + offset - 1);
    final next = leaf.source.codeUnitAt(leaf.start + offset);
    if (_isHighSurrogate(previous) && _isLowSurrogate(next)) {
      throw FormatException('UTF-16 split $offset divides a scalar value.');
    }
    return _SourceSplit(
      leaf.slice(0, offset),
      leaf.slice(offset, leaf.utf16Length),
    );
  }

  final branch = node as _SourceBranch;
  if (offset < branch.left.utf16Length) {
    final split = _split(branch.left, offset, chunkSize, work: work);
    return _SourceSplit(
      split.left,
      _concat(split.right, branch.right, chunkSize),
    );
  }
  if (offset == branch.left.utf16Length) {
    return _SourceSplit(branch.left, branch.right);
  }
  final split = _split(
    branch.right,
    offset - branch.left.utf16Length,
    chunkSize,
    work: work,
  );
  return _SourceSplit(_concat(branch.left, split.left, chunkSize), split.right);
}

_SourceNode? _concat(
  _SourceNode? left,
  _SourceNode? right,
  int chunkSize, {
  int Function()? allocateProvisionalPieceId,
}) {
  if (left == null) return right;
  if (right == null) return left;
  final merged = _mergeSourceBoundaryLeaves(
    _rightmostLeaf(left)!,
    _leftmostLeaf(right)!,
    chunkSize,
    allocateProvisionalPieceId: allocateProvisionalPieceId,
  );
  if (merged != null) {
    final leftPop = _popRightmostSourceLeaf(left);
    final rightPop = _popLeftmostSourceLeaf(right);
    return _joinSourceTrees(
      _joinSourceTrees(leftPop.rest, merged),
      rightPop.rest,
    );
  }
  return _joinSourceTrees(left, right);
}

_SourceLeaf? _mergeSourceBoundaryLeaves(
  _SourceLeaf left,
  _SourceLeaf right,
  int chunkSize, {
  required int Function()? allocateProvisionalPieceId,
}) {
  if (left.utf16Length + right.utf16Length > chunkSize) return null;
  if (left.isCertified && right.isCertified) {
    return _SourceLeaf.owned('${left.materialize()}${right.materialize()}');
  }
  if (!left.isCertified &&
      !right.isCertified &&
      identical(left.backing, right.backing) &&
      left.pieceId == right.pieceId &&
      left.start + left.utf16Length == right.start) {
    return _SourceLeaf.provisional(
      source: left.source,
      start: left.start,
      utf16Length: left.utf16Length + right.utf16Length,
      pieceId: left.pieceId!,
      backing: left.backing,
    );
  }
  if (allocateProvisionalPieceId == null) return null;
  final merged = '${left.materialize()}${right.materialize()}';
  return _SourceLeaf.provisional(
    source: merged,
    start: 0,
    utf16Length: merged.length,
    pieceId: allocateProvisionalPieceId(),
  );
}

final class _SourceLeafPop {
  const _SourceLeafPop(this.rest, this.leaf);

  final _SourceNode? rest;
  final _SourceLeaf leaf;
}

_SourceLeafPop _popRightmostSourceLeaf(_SourceNode node) {
  if (node case final _SourceLeaf leaf) {
    return _SourceLeafPop(null, leaf);
  }
  final branch = node as _SourceBranch;
  final popped = _popRightmostSourceLeaf(branch.right);
  return _SourceLeafPop(
    _joinSourceTrees(branch.left, popped.rest),
    popped.leaf,
  );
}

_SourceLeafPop _popLeftmostSourceLeaf(_SourceNode node) {
  if (node case final _SourceLeaf leaf) {
    return _SourceLeafPop(null, leaf);
  }
  final branch = node as _SourceBranch;
  final popped = _popLeftmostSourceLeaf(branch.left);
  return _SourceLeafPop(
    _joinSourceTrees(popped.rest, branch.right),
    popped.leaf,
  );
}

_SourceNode? _joinSourceTrees(_SourceNode? left, _SourceNode? right) {
  if (left == null) return right;
  if (right == null) return left;
  if (left.height > right.height + 1) {
    final branch = left as _SourceBranch;
    return _balance(
      _SourceBranch(branch.left, _joinSourceTrees(branch.right, right)!),
    );
  }
  if (right.height > left.height + 1) {
    final branch = right as _SourceBranch;
    return _balance(
      _SourceBranch(_joinSourceTrees(left, branch.left)!, branch.right),
    );
  }
  return _SourceBranch(left, right);
}

_SourceNode _balance(_SourceBranch node) {
  final balance = node.left.height - node.right.height;
  if (balance > 1) {
    final left = node.left as _SourceBranch;
    if (left.left.height < left.right.height) {
      final pivot = left.right as _SourceBranch;
      return _SourceBranch(
        _SourceBranch(left.left, pivot.left),
        _SourceBranch(pivot.right, node.right),
      );
    }
    return _SourceBranch(left.left, _SourceBranch(left.right, node.right));
  }
  if (balance < -1) {
    final right = node.right as _SourceBranch;
    if (right.right.height < right.left.height) {
      final pivot = right.left as _SourceBranch;
      return _SourceBranch(
        _SourceBranch(node.left, pivot.left),
        _SourceBranch(pivot.right, right.right),
      );
    }
    return _SourceBranch(_SourceBranch(node.left, right.left), right.right);
  }
  return node;
}

void _writeRange(_SourceNode? node, int start, int end, StringBuffer output) {
  if (node == null || start >= end) return;
  if (node case final _SourceLeaf leaf) {
    output.write(leaf.source.substring(leaf.start + start, leaf.start + end));
    return;
  }
  final branch = node as _SourceBranch;
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

bool _rangeEqualsString(
  _SourceNode? node,
  int start,
  int end,
  String expected,
  _MutableSourceWorkReceipt work,
) {
  if (end - start != expected.length) return false;
  var expectedOffset = 0;

  bool compare(_SourceNode? current, int localStart, int localEnd) {
    if (current == null || localStart >= localEnd) return true;
    if (current case final _SourceLeaf leaf) {
      for (var offset = localStart; offset < localEnd; offset += 1) {
        work.noOpComparedUtf16 += 1;
        if (leaf.source.codeUnitAt(leaf.start + offset) !=
            expected.codeUnitAt(expectedOffset)) {
          return false;
        }
        expectedOffset += 1;
      }
      return true;
    }
    final branch = current as _SourceBranch;
    if (localStart < branch.left.utf16Length &&
        !compare(
          branch.left,
          localStart,
          math.min(localEnd, branch.left.utf16Length),
        )) {
      return false;
    }
    if (localEnd > branch.left.utf16Length &&
        !compare(
          branch.right,
          math.max(0, localStart - branch.left.utf16Length),
          localEnd - branch.left.utf16Length,
        )) {
      return false;
    }
    return true;
  }

  return compare(node, start, end) && expectedOffset == expected.length;
}

int _utf16ToUtf8(_SourceNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf16Length) return node.utf8Length;
  if (node case final _SourceLeaf leaf) {
    if (leaf.rangeIndex case final index?) {
      return index.utf8Before(leaf.source, leaf.start + offset) -
          index.utf8Before(leaf.source, leaf.start);
    }
    var utf16Offset = 0;
    var utf8Offset = 0;
    while (utf16Offset < offset) {
      final absolute = leaf.start + utf16Offset;
      utf8Offset += _utf8Width(
        leaf.source,
        absolute,
        leaf.start + leaf.utf16Length,
      );
      utf16Offset += _scalarUtf16Width(
        leaf.source,
        absolute,
        leaf.start + leaf.utf16Length,
      );
    }
    if (utf16Offset != offset) {
      throw FormatException('UTF-16 offset $offset divides a scalar value.');
    }
    return utf8Offset;
  }
  final branch = node as _SourceBranch;
  if (offset <= branch.left.utf16Length) {
    return _utf16ToUtf8(branch.left, offset);
  }
  return branch.left.utf8Length +
      _utf16ToUtf8(branch.right, offset - branch.left.utf16Length);
}

int _utf8ToUtf16(_SourceNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf8Length) return node.utf16Length;
  if (node case final _SourceLeaf leaf) {
    if (leaf.rangeIndex case final index?) {
      final absoluteUtf8 = index.utf8Before(leaf.source, leaf.start) + offset;
      return index.utf16AtUtf8(
            leaf.source,
            leaf.start,
            leaf.start + leaf.utf16Length,
            absoluteUtf8,
          ) -
          leaf.start;
    }
    var utf16Offset = 0;
    var utf8Offset = 0;
    while (utf8Offset < offset) {
      final absolute = leaf.start + utf16Offset;
      utf8Offset += _utf8Width(
        leaf.source,
        absolute,
        leaf.start + leaf.utf16Length,
      );
      utf16Offset += _scalarUtf16Width(
        leaf.source,
        absolute,
        leaf.start + leaf.utf16Length,
      );
    }
    if (utf8Offset != offset) {
      throw FormatException('UTF-8 offset $offset divides a scalar value.');
    }
    return utf16Offset;
  }
  final branch = node as _SourceBranch;
  if (offset <= branch.left.utf8Length) {
    return _utf8ToUtf16(branch.left, offset);
  }
  return branch.left.utf16Length +
      _utf8ToUtf16(branch.right, offset - branch.left.utf8Length);
}

int _newlinesBefore(_SourceNode? node, int offset) {
  if (node == null || offset == 0) return 0;
  if (offset == node.utf16Length) return node.newlines;
  if (node case final _SourceLeaf leaf) {
    if (leaf.rangeIndex case final index?) {
      return index.newlinesInRange(
        leaf.source,
        leaf.start,
        leaf.start + offset,
      );
    }
    var low = 0;
    var high = leaf.lineBreakEndOffsets.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (leaf.lineBreakEndOffsets[middle] <= offset) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low;
  }
  final branch = node as _SourceBranch;
  final joinsCrLf =
      branch.left.endsWithCarriageReturn && branch.right.startsWithLineFeed;
  if (offset <= branch.left.utf16Length) {
    final count = _newlinesBefore(branch.left, offset);
    return offset == branch.left.utf16Length && joinsCrLf ? count - 1 : count;
  }
  return branch.left.newlines +
      _newlinesBefore(branch.right, offset - branch.left.utf16Length) -
      (joinsCrLf ? 1 : 0);
}

int _offsetAfterNthNewline(_SourceNode node, int count) {
  if (node case final _SourceLeaf leaf) {
    if (leaf.rangeIndex case final index?) {
      return index.offsetAfterNthNewline(
            leaf.source,
            leaf.start,
            leaf.start + leaf.utf16Length,
            count,
          ) -
          leaf.start;
    }
    return leaf.lineBreakEndOffsets[count - 1];
  }
  final branch = node as _SourceBranch;
  final joinsCrLf =
      branch.left.endsWithCarriageReturn && branch.right.startsWithLineFeed;
  final completeLeftBreaks = branch.left.newlines - (joinsCrLf ? 1 : 0);
  if (count <= completeLeftBreaks) {
    return _offsetAfterNthNewline(branch.left, count);
  }
  return branch.left.utf16Length +
      _offsetAfterNthNewline(branch.right, count - completeLeftBreaks);
}

int _codeUnitAt(_SourceNode node, int offset) {
  if (node case final _SourceLeaf leaf) {
    return leaf.source.codeUnitAt(leaf.start + offset);
  }
  final branch = node as _SourceBranch;
  if (offset < branch.left.utf16Length) {
    return _codeUnitAt(branch.left, offset);
  }
  return _codeUnitAt(branch.right, offset - branch.left.utf16Length);
}

int _compareIndexedOperations(_IndexedSourceEdit a, _IndexedSourceEdit b) {
  final start = a.operation.startUtf16.compareTo(b.operation.startUtf16);
  if (start != 0) return start;
  final end = a.operation.endUtf16.compareTo(b.operation.endUtf16);
  if (end != 0) return end;
  return a.index.compareTo(b.index);
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
