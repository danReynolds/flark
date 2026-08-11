import 'dart:convert';
import 'dart:ffi';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../models.dart';
import 'bindings.dart';

const _ok = 0;
const _notCertified = 3;
const _budgetExhausted = 6;
const _resultCapReached = 7;
const _backpressure = 9;
const _historyRetained = 1;
const _historyDisabled = 2;
const _historyOverBudget = 3;
const _internalFault = 0x0402;
const _historyTokenEvicted = 0x0305;
const _historyTokenStale = 0x0306;
const _closeIncomplete = 0x0404;
const _sourceRecord = 1;
const _semanticRecord = 2;
const _sourceAndSemanticRecord = 3;
const _pendingNeutral = 1;
const _currentCertified = 2;
const _mixedCurrent = 3;
const _headingKind = 12;
const _headingLevelMask = 0xff;
const _headingSetext = 0x100;
const _knownHeadingVariantBits = 0x1ff;
const _paragraphKind = 5;
const _indentedCodeKind = 6;
const _fencedCodeKind = 7;
const _thematicBreakKind = 13;
const _emptyListItemKind = 14;
const _listMarkerMask = 0x7;
const _listHyphen = 1;
const _listPlus = 2;
const _listAsterisk = 3;
const _listOrderedPeriod = 4;
const _listOrderedParenthesis = 5;
const _listDepthShift = 3;
const _listDepthMask = 0x7f8;
const _listMarkerOffsetShift = 11;
const _listMarkerOffsetMask = 0x1800;
const _listSimpleContinuation = 0x2000;
const _listStartsList = 0x4000;
const _listTask = 0x8000;
const _listTaskChecked = 0x10000;
const _knownListVariantBits = 0x1ffff;
const _blockQuotePresentation = 0x10000;
const _blockQuoteDepthShift = 17;
const _blockQuoteDepthMask = 0x1fe0000;
const _blockQuoteSimpleContinuation = 0x2000000;
const _knownBlockQuoteVariantBits = 0x3ff0000;
const _codePresentation = 0x10000;
const _codeFenced = 0x20000;
const _codeTilde = 0x40000;
const _codeClosed = 0x80000;
const _codeFenceOffsetShift = 20;
const _codeFenceOffsetMask = 0x300000;
const _knownCodeVariantBits = 0x3f0000;
const _thematicBreakPresentation = 0x10000;
const _inlineAuthoritative = 0x8;
const _knownViewportRowFlags = 0xf;
const _inlineFactEmphasis = 1;
const _inlineFactStrong = 2;
const _inlineFactCode = 3;
const _inlineFactStrikethrough = 4;
const _inlineFactAutolinkUri = 5;
const _inlineFactAutolinkEmail = 6;
const _inlineFactBackslashEscape = 7;
const _inlineFactHardLineBreak = 8;
const _inlineFactReplacement = 9;
const _inlineFactDirectLink = 10;
const _inlineFactDirectImage = 11;
const _inlineFactReferenceLink = 12;
const _inlineFactReferenceImage = 13;
const _inlineFactAutolinkUriWww = 0x1;
const _inlineFactCodeNormalizeLineEndings = 0x1;
const _inlineFactCodeTrimOneSpace = 0x2;
const _maxInlineFactsPerRow = 64;
const _absentPresentationPrefix = 0xffffffffffffffff;
const _maxChunkBytes = 64 * 1024;
const _maximumSmallEditBytes = 4 * 1024;
const _bulkCommitWorkUnits = 1;
const _resultPayloadBytes = 64 * 1024;
const _defaultWorkUnits = 512;
const _abiMajor = 4;
const _abiMinor = 6;
// Every v4.6 capability is used by the safe core boundary, including
// resumable close and snapshot continuations.
const _requiredCapabilityBits = 0x7fff;

final class FlarkNativeException implements Exception {
  const FlarkNativeException(this.operation, this.status, [this.detail = 0]);

  final String operation;
  final int status;
  final int detail;

  @override
  String toString() =>
      'FlarkNativeException($operation, status: $status, detail: $detail)';
}

final class FlarkNativeEditReceipt {
  const FlarkNativeEditReceipt({
    required this.revision,
    required this.sourceByteLength,
    required this.sourceUtf16Length,
    required this.historyToken,
    required this.historyDisposition,
  });

  final int revision;
  final int sourceByteLength;
  final int sourceUtf16Length;
  final int? historyToken;
  final FlarkNativeHistoryDisposition historyDisposition;
}

enum FlarkNativeHistoryDisposition { retained, disabled, overBudget }

const _coordinateUtf16 = 2;
const _affinityUpstream = 1;
const _affinityDownstream = 2;

final class FlarkNativeSessionInspection {
  const FlarkNativeSessionInspection({
    required this.sessionState,
    required this.revision,
    required this.liveTransactions,
    required this.liveContinuations,
    required this.liveAnchors,
    required this.liveHistoryTokens,
  });

  final int sessionState;
  final int revision;
  final int liveTransactions;
  final int liveContinuations;
  final int liveAnchors;
  final int liveHistoryTokens;
}

final class _HistoryLengthDelta {
  const _HistoryLengthDelta(this.byteDelta, this.utf16Delta);

  final int byteDelta;
  final int utf16Delta;
}

/// One isolate-confined owner of a native Flark v4 document session.
///
/// This class is public for headless/native embedders. Flutter applications
/// should normally use [FlarkCoreDocument], which keeps every call on a
/// persistent worker isolate.
final class FlarkNativeDocument {
  FlarkNativeDocument._({
    required FlarkV4Bindings bindings,
    required int session,
    required int ownerToken,
    required int transaction,
    required int sourceByteLength,
    required int sourceUtf16Length,
  }) : _bindings = bindings,
       _session = session,
       _ownerToken = ownerToken,
       _transaction = transaction,
       _sourceByteLength = sourceByteLength,
       _sourceUtf16Length = sourceUtf16Length;

  static int _nextOwnerToken = 1;

  final FlarkV4Bindings _bindings;
  final int _session;
  final int _ownerToken;
  final int _transaction;
  final Map<int, _HistoryLengthDelta> _historyLengthDeltas = {};

  int _revision = 1;
  int _progressToken = 0;
  int _sourceByteLength;
  int _sourceUtf16Length;
  bool _ready = false;
  bool _closed = false;

  int get revision => _revision;
  int get sourceByteLength => _sourceByteLength;
  int get sourceUtf16Length => _sourceUtf16Length;
  bool get isReady => _ready;

  static FlarkNativeDocument open(
    String source, {
    required String libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
  }) {
    if (historyBudgetBytes < 0) {
      throw RangeError.value(historyBudgetBytes, 'historyBudgetBytes');
    }
    final library = DynamicLibrary.open(libraryPath);
    final bindings = FlarkV4Bindings(library);
    _negotiate(bindings);
    final bytes = utf8.encode(source);
    final ownerToken = _nextOwnerToken++;
    final outcome = calloc<FlarkV4Outcome>();
    final request = calloc<FlarkV4CreateRequest>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4CreateRequest>()
        ..flags = 0
        ..ownerToken = ownerToken
        ..expectedTotalBytes = bytes.length;
      request.ref.config
        ..structSize = sizeOf<FlarkV4SessionConfig>()
        ..parserProfile = 2
        ..historyBudgetBytes = historyBudgetBytes
        ..maxDocumentBytes = math.max(1024 * 1024 * 1024, bytes.length)
        ..flags = 0;

      final firstLength = math.min(bytes.length, _maxChunkBytes);
      final first = _copyBytes(bytes, 0, firstLength);
      try {
        final status = bindings.createBegin(
          request,
          first,
          firstLength,
          outcome,
        );
        _requireStatus('create_begin', status, outcome.ref, {_ok});
      } finally {
        calloc.free(first);
      }

      final document = FlarkNativeDocument._(
        bindings: bindings,
        session: outcome.ref.primaryHandle,
        ownerToken: ownerToken,
        transaction: outcome.ref.secondaryHandle,
        sourceByteLength: bytes.length,
        sourceUtf16Length: source.length,
      );
      try {
        document._appendSource(bytes, firstLength);
        document._commitCreate();
        return document;
      } catch (_) {
        // A successful CREATE_BEGIN owns a provisional session and
        // transaction. Never strand them when append/commit fails.
        try {
          document._abortCreate();
        } catch (_) {
          // Preserve the initiating failure; the abort path is best-effort
          // only after the native boundary has already rejected startup.
        }
        rethrow;
      }
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  void _appendSource(List<int> source, int offset) {
    final request = calloc<FlarkV4StageRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      var cursor = offset;
      while (cursor < source.length) {
        final length = math.min(_maxChunkBytes, source.length - cursor);
        final chunk = _copyBytes(source, cursor, length);
        try {
          request.ref
            ..structSize = sizeOf<FlarkV4StageRequest>()
            ..flags = 0
            ..transaction = _transaction
            ..chunkOffset = cursor
            ..chunkLen = length;
          _fillSession(request.ref.session);
          final status = _bindings.createAppend(
            request,
            chunk,
            length,
            outcome,
          );
          _requireStatus('create_append', status, outcome.ref, {_ok});
        } finally {
          calloc.free(chunk);
        }
        cursor += length;
      }
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  void _commitCreate() {
    final request = calloc<FlarkV4TransactionRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4TransactionRequest>()
        ..flags = 0
        ..transaction = _transaction
        ..expectedRevision = 0
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 64);
      final status = _bindings.createCommit(request, outcome);
      _requireStatus('create_commit', status, outcome.ref, {
        _ok,
        _budgetExhausted,
      });
      _revision = outcome.ref.revision;
      _progressToken = outcome.ref.progressToken;
      _ready = status == _ok;
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  void _abortCreate() {
    final request = calloc<FlarkV4TransactionRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4TransactionRequest>()
        ..flags = 0
        ..transaction = _transaction
        ..expectedRevision = 0
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.createAbort(request, outcome);
      _requireStatus('create_abort', status, outcome.ref, {_ok});
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  /// Advances at most [workUnits] parser work units.
  bool pump({int workUnits = _defaultWorkUnits}) {
    if (_ready) return true;
    final request = calloc<FlarkV4PumpRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4PumpRequest>()
        ..flags = 0
        ..expectedRevision = _revision
        ..progressToken = _progressToken;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: workUnits);
      final status = _bindings.pump(request, outcome);
      _requireStatus('pump', status, outcome.ref, {_ok, _budgetExhausted});
      _ready = status == _ok;
      // A completed pump's echoed token is terminal, not resumable; the next
      // pump chain begins from zero.
      _progressToken = _ready ? 0 : outcome.ref.progressToken;
      return _ready;
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  void pumpUntilReady({int workUnits = _defaultWorkUnits}) {
    while (!pump(workUnits: workUnits)) {}
  }

  FlarkNativeEditReceipt applyEditUtf16(
    int startUtf16,
    int endUtf16,
    String replacement,
  ) {
    if (startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        endUtf16 > _sourceUtf16Length) {
      throw RangeError.range(endUtf16, startUtf16, _sourceUtf16Length);
    }
    final startByte = _convertCoordinate(startUtf16, from: 2, to: 1);
    final endByte = _convertCoordinate(endUtf16, from: 2, to: 1);
    final replacementBytes = utf8.encode(replacement);
    if (sizeOf<FlarkV4EditDescriptor>() +
            replacementBytes.length +
            (endByte - startByte) >
        _maximumSmallEditBytes) {
      return _applyBulkEditUtf16(
        startUtf16,
        endUtf16,
        startByte,
        endByte,
        replacement,
        replacementBytes,
      );
    }
    final request = calloc<FlarkV4SmallEditRequest>();
    final descriptor = calloc<FlarkV4EditDescriptor>();
    final outcome = calloc<FlarkV4Outcome>();
    final replacementPointer = _copyBytes(
      replacementBytes,
      0,
      replacementBytes.length,
    );
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4SmallEditRequest>()
        ..flags = 0
        ..expectedRevision = _revision
        ..editCount = 1
        ..reservedU32 = 0
        ..replacementBytesLen = replacementBytes.length;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      descriptor.ref
        ..startByte = startByte
        ..endByte = endByte
        ..replacementOffset = 0
        ..replacementLen = replacementBytes.length;
      var status = _bindings.smallEdit(
        request,
        descriptor,
        1,
        replacementPointer,
        replacementBytes.length,
        outcome,
      );
      if (status == _backpressure) {
        _requireStatus('small_edit', status, outcome.ref, {_backpressure});
        pump(workUnits: _defaultWorkUnits);
        status = _bindings.smallEdit(
          request,
          descriptor,
          1,
          replacementPointer,
          replacementBytes.length,
          outcome,
        );
      }
      _requireStatus('small_edit', status, outcome.ref, {_ok});
      _revision = outcome.ref.revision;
      _progressToken = 0;
      _ready = false;
      _sourceByteLength += replacementBytes.length - (endByte - startByte);
      _sourceUtf16Length += replacement.length - (endUtf16 - startUtf16);
      final disposition = _historyDisposition(outcome.ref.detailCode);
      final historyToken = outcome.ref.primaryHandle == 0
          ? null
          : outcome.ref.primaryHandle;
      if ((disposition == FlarkNativeHistoryDisposition.retained) !=
          (historyToken != null)) {
        throw const FlarkNativeException('small_edit', _internalFault);
      }
      if (historyToken != null) {
        _historyLengthDeltas[historyToken] = _HistoryLengthDelta(
          (endByte - startByte) - replacementBytes.length,
          (endUtf16 - startUtf16) - replacement.length,
        );
      }
      return FlarkNativeEditReceipt(
        revision: _revision,
        sourceByteLength: _sourceByteLength,
        sourceUtf16Length: _sourceUtf16Length,
        historyToken: historyToken,
        historyDisposition: disposition,
      );
    } finally {
      calloc
        ..free(request)
        ..free(descriptor)
        ..free(outcome)
        ..free(replacementPointer);
    }
  }

  FlarkNativeEditReceipt _applyBulkEditUtf16(
    int startUtf16,
    int endUtf16,
    int startByte,
    int endByte,
    String replacement,
    List<int> replacementBytes,
  ) {
    final begin = calloc<FlarkV4BulkBeginRequest>();
    final stage = calloc<FlarkV4StageRequest>();
    final commit = calloc<FlarkV4TransactionRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    var transaction = 0;
    var committed = false;
    try {
      begin.ref
        ..structSize = sizeOf<FlarkV4BulkBeginRequest>()
        ..flags = 0
        ..expectedRevision = _revision
        ..expectedTotalBytes = replacementBytes.length;
      _fillSession(begin.ref.session);
      begin.ref.range
        ..startByte = startByte
        ..endByte = endByte;
      final beginStatus = _bindings.bulkBegin(begin, outcome);
      _requireStatus('bulk_begin', beginStatus, outcome.ref, {_ok});
      transaction = outcome.ref.primaryHandle;
      if (transaction == 0) {
        throw const FlarkNativeException('bulk_begin', _internalFault);
      }

      var cursor = 0;
      while (cursor < replacementBytes.length) {
        final length = math.min(
          _maxChunkBytes,
          replacementBytes.length - cursor,
        );
        final chunk = _copyBytes(replacementBytes, cursor, length);
        try {
          stage.ref
            ..structSize = sizeOf<FlarkV4StageRequest>()
            ..flags = 0
            ..transaction = transaction
            ..chunkOffset = cursor
            ..chunkLen = length;
          _fillSession(stage.ref.session);
          final appendStatus = _bindings.bulkAppend(
            stage,
            chunk,
            length,
            outcome,
          );
          _requireStatus('bulk_append', appendStatus, outcome.ref, {_ok});
        } finally {
          calloc.free(chunk);
        }
        cursor += length;
      }

      commit.ref
        ..structSize = sizeOf<FlarkV4TransactionRequest>()
        ..flags = 0
        ..transaction = transaction
        ..expectedRevision = _revision
        ..progressToken = 0;
      _fillSession(commit.ref.session);
      _fillBudget(commit.ref.budget, workUnits: _bulkCommitWorkUnits);
      var status = _bindings.bulkCommit(commit, outcome);
      _requireStatus('bulk_commit', status, outcome.ref, {
        _ok,
        _budgetExhausted,
      });
      while (status == _budgetExhausted) {
        commit.ref.progressToken = outcome.ref.progressToken;
        status = _bindings.bulkCommit(commit, outcome);
        _requireStatus('bulk_commit', status, outcome.ref, {
          _ok,
          _budgetExhausted,
        });
      }
      committed = true;
      _revision = outcome.ref.revision;
      _progressToken = 0;
      _ready = false;
      _sourceByteLength += replacementBytes.length - (endByte - startByte);
      _sourceUtf16Length += replacement.length - (endUtf16 - startUtf16);
      final disposition = _historyDisposition(outcome.ref.detailCode);
      final historyToken = outcome.ref.primaryHandle == 0
          ? null
          : outcome.ref.primaryHandle;
      if ((disposition == FlarkNativeHistoryDisposition.retained) !=
          (historyToken != null)) {
        // Detail 1 distinguishes this Dart-side coherence failure from a
        // native INTERNAL_FAULT passed through with detail 0.
        throw const FlarkNativeException('bulk_commit', _internalFault, 1);
      }
      if (historyToken != null) {
        _historyLengthDeltas[historyToken] = _HistoryLengthDelta(
          (endByte - startByte) - replacementBytes.length,
          (endUtf16 - startUtf16) - replacement.length,
        );
      }
      return FlarkNativeEditReceipt(
        revision: _revision,
        sourceByteLength: _sourceByteLength,
        sourceUtf16Length: _sourceUtf16Length,
        historyToken: historyToken,
        historyDisposition: disposition,
      );
    } finally {
      if (!committed && transaction != 0) {
        commit.ref
          ..structSize = sizeOf<FlarkV4TransactionRequest>()
          ..flags = 0
          ..transaction = transaction
          ..expectedRevision = _revision
          ..progressToken = 0;
        _fillSession(commit.ref.session);
        _fillBudget(commit.ref.budget, workUnits: 1);
        _bindings.bulkAbort(commit, outcome);
      }
      calloc
        ..free(begin)
        ..free(stage)
        ..free(commit)
        ..free(outcome);
    }
  }

  FlarkNativeEditReceipt replayHistory(int historyToken) {
    final delta = _historyLengthDeltas[historyToken];
    if (delta == null) {
      throw StateError('Unknown Flark history token');
    }
    final request = calloc<FlarkV4HistoryRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4HistoryRequest>()
        ..flags = 0
        ..expectedRevision = _revision
        ..historyToken = historyToken
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      var status = _bindings.historyReplay(request, outcome);
      if (status == _backpressure) {
        _requireStatus('history_replay', status, outcome.ref, {_backpressure});
        pump(workUnits: _defaultWorkUnits);
        status = _bindings.historyReplay(request, outcome);
      }
      if (status == _historyTokenEvicted) {
        _historyLengthDeltas.remove(historyToken);
      }
      _requireStatus('history_replay', status, outcome.ref, {_ok});
      _historyLengthDeltas.remove(historyToken);
      _revision = outcome.ref.revision;
      _progressToken = 0;
      _ready = false;
      _sourceByteLength += delta.byteDelta;
      _sourceUtf16Length += delta.utf16Delta;
      final disposition = _historyDisposition(outcome.ref.detailCode);
      final reverseToken = outcome.ref.primaryHandle == 0
          ? null
          : outcome.ref.primaryHandle;
      if ((disposition == FlarkNativeHistoryDisposition.retained) !=
          (reverseToken != null)) {
        throw const FlarkNativeException('history_replay', _internalFault);
      }
      if (reverseToken != null) {
        _historyLengthDeltas[reverseToken] = _HistoryLengthDelta(
          -delta.byteDelta,
          -delta.utf16Delta,
        );
      }
      return FlarkNativeEditReceipt(
        revision: _revision,
        sourceByteLength: _sourceByteLength,
        sourceUtf16Length: _sourceUtf16Length,
        historyToken: reverseToken,
        historyDisposition: disposition,
      );
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  void releaseHistory(int historyToken) {
    final request = calloc<FlarkV4HistoryRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4HistoryRequest>()
        ..flags = 0
        ..expectedRevision = _revision
        ..historyToken = historyToken
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.historyRelease(request, outcome);
      if (status == _historyTokenEvicted || status == _historyTokenStale) {
        _historyLengthDeltas.remove(historyToken);
      }
      _requireStatus('history_release', status, outcome.ref, {_ok});
      _historyLengthDeltas.remove(historyToken);
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  /// Creates a source-stable anchor at a UTF-16 scalar boundary.
  ///
  /// The native runtime keeps the anchor at the current revision by
  /// transforming it eagerly through every committed edit; [downstream]
  /// selects which splice edge the anchor follows when an edit lands exactly
  /// on or across it.
  int createAnchorUtf16(int utf16Position, {required bool downstream}) {
    final request = calloc<FlarkV4AnchorRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4AnchorRequest>()
        ..coordinateKind = _coordinateUtf16
        ..revision = _revision
        ..snapshot = 0
        ..anchor = 0
        ..position = utf16Position
        ..affinity = downstream ? _affinityDownstream : _affinityUpstream
        ..reservedU32 = 0
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.anchorCreate(request, outcome);
      _requireStatus('anchor_create', status, outcome.ref, {_ok});
      return outcome.ref.primaryHandle;
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  /// Resolves [anchor] to a UTF-16 offset at the current revision.
  int resolveAnchorUtf16(int anchor) {
    final request = calloc<FlarkV4AnchorRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4AnchorRequest>()
        ..coordinateKind = _coordinateUtf16
        ..revision = _revision
        ..snapshot = 0
        ..anchor = anchor
        ..position = 0
        ..affinity = 0
        ..reservedU32 = 0
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.anchorResolve(request, outcome);
      _requireStatus('anchor_resolve', status, outcome.ref, {_ok});
      return outcome.ref.detailCode;
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  void releaseAnchor(int anchor) {
    final request = calloc<FlarkV4AnchorRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4AnchorRequest>()
        ..coordinateKind = 0
        ..revision = 0
        ..snapshot = 0
        ..anchor = anchor
        ..position = 0
        ..affinity = 0
        ..reservedU32 = 0
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.anchorRelease(request, outcome);
      _requireStatus('anchor_release', status, outcome.ref, {_ok});
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  FlarkNativeSessionInspection inspect() {
    final request = calloc<FlarkV4InspectRequest>();
    final inspection = calloc<FlarkV4SessionInspection>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4InspectRequest>()
        ..flags = 0;
      _fillSession(request.ref.session);
      final status = _bindings.sessionInspect(request, inspection, outcome);
      _requireStatus('session_inspect', status, outcome.ref, {_ok});
      return FlarkNativeSessionInspection(
        sessionState: inspection.ref.sessionState,
        revision: inspection.ref.revision,
        liveTransactions: inspection.ref.liveTransactions,
        liveContinuations: inspection.ref.liveContinuations,
        liveAnchors: inspection.ref.liveAnchors,
        liveHistoryTokens: inspection.ref.liveHistoryTokens,
      );
    } finally {
      calloc
        ..free(request)
        ..free(inspection)
        ..free(outcome);
    }
  }

  FlarkNativeHistoryDisposition _historyDisposition(int value) =>
      switch (value) {
        _historyRetained => FlarkNativeHistoryDisposition.retained,
        _historyDisabled => FlarkNativeHistoryDisposition.disabled,
        _historyOverBudget => FlarkNativeHistoryDisposition.overBudget,
        _ => throw FlarkNativeException(
          'history_disposition',
          _internalFault,
          value,
        ),
      };

  int _convertCoordinate(int position, {required int from, required int to}) {
    final request = calloc<FlarkV4CoordinateRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4CoordinateRequest>()
        ..fromKind = from
        ..toKind = to
        ..reservedU32 = 0
        ..revision = _revision
        ..snapshot = 0
        ..position = position
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.coordinateConvert(request, outcome);
      _requireStatus('coordinate_convert', status, outcome.ref, {_ok});
      return outcome.ref.detailCode;
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  FlarkViewport queryViewport({
    int startByte = 0,
    int? endByte,
    int maxRows = 256,
  }) {
    final resolvedEnd = endByte ?? _sourceByteLength;
    if (startByte < 0 ||
        resolvedEnd < startByte ||
        resolvedEnd > _sourceByteLength) {
      throw RangeError.range(resolvedEnd, startByte, _sourceByteLength);
    }
    final request = calloc<FlarkV4QueryRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    final capacity = sizeOf<FlarkV4ResultPageHeader>() + _resultPayloadBytes;
    final output = calloc<Uint8>(capacity);
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4QueryRequest>()
        ..queryKind = _ready ? 2 : 3
        ..revision = _revision
        ..snapshot = 0
        ..continuation = 0;
      _fillSession(request.ref.session);
      request.ref.range
        ..startByte = startByte
        ..endByte = resolvedEnd;
      _fillBudget(request.ref.budget, workUnits: 1, maxRows: maxRows);
      final status = _bindings.queryViewport(
        request,
        output,
        capacity,
        outcome,
      );
      _requireStatus('query_viewport', status, outcome.ref, {
        _ok,
        _notCertified,
        _resultCapReached,
      });
      return _decodeViewport(output);
    } finally {
      calloc
        ..free(request)
        ..free(outcome)
        ..free(output);
    }
  }

  FlarkViewport queryViewportNext(FlarkViewport previous, {int maxRows = 256}) {
    if (previous.revision != _revision || previous.continuation == 0) {
      throw ArgumentError('viewport has no current continuation');
    }
    final request = calloc<FlarkV4ContinuationRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    final capacity = sizeOf<FlarkV4ResultPageHeader>() + _resultPayloadBytes;
    final output = calloc<Uint8>(capacity);
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4ContinuationRequest>()
        ..flags = 0
        ..revision = previous.revision
        ..snapshot = previous.snapshot
        ..continuation = previous.continuation;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1, maxRows: maxRows);
      final status = _bindings.continuationNext(
        request,
        output,
        capacity,
        outcome,
      );
      _requireStatus('continuation_next', status, outcome.ref, {
        _ok,
        _resultCapReached,
      });
      return _decodeViewport(output);
    } finally {
      calloc
        ..free(request)
        ..free(outcome)
        ..free(output);
    }
  }

  void releaseViewportContinuation(FlarkViewport viewport) {
    if (viewport.continuation == 0 || viewport.revision != _revision) return;
    final request = calloc<FlarkV4ContinuationRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4ContinuationRequest>()
        ..flags = 0
        ..revision = viewport.revision
        ..snapshot = viewport.snapshot
        ..continuation = viewport.continuation;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: 1);
      final status = _bindings.continuationRelease(request, outcome);
      _requireStatus('continuation_release', status, outcome.ref, {_ok});
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  FlarkViewport _decodeViewport(Pointer<Uint8> output) {
    final header = output.cast<FlarkV4ResultPageHeader>().ref;
    final requested = FlarkSourceRange(
      header.requestedRange.startByte,
      header.requestedRange.endByte,
    );
    final covered = FlarkSourceRange(
      header.coveredRange.startByte,
      header.coveredRange.endByte,
    );
    final coveredUtf16 = FlarkSourceRange(
      _convertCoordinate(covered.start, from: 1, to: 2),
      _convertCoordinate(covered.end, from: 1, to: 2),
    );
    final payload = output + sizeOf<FlarkV4ResultPageHeader>();
    if (header.recordKind == _sourceRecord) {
      final source = utf8.decode(payload.asTypedList(header.payloadBytes));
      return FlarkViewport(
        revision: header.revision,
        snapshot: header.snapshot,
        requestedBytes: requested,
        coveredBytes: covered,
        coveredUtf16: coveredUtf16,
        certification: FlarkCertification.pendingNeutral,
        rows: const [],
        neutralSource: source,
        continuation: header.continuation,
      );
    }
    if (header.recordKind == _sourceAndSemanticRecord) {
      final recordBytes =
          header.itemCount * sizeOf<FlarkV4CertificationRangeRecord>();
      if (recordBytes > header.payloadBytes) {
        throw FlarkNativeException(
          'decode_viewport',
          _notCertified,
          header.payloadBytes,
        );
      }
      final records = payload.cast<FlarkV4CertificationRangeRecord>();
      final ranges = List<FlarkCertificationRange>.generate(header.itemCount, (
        index,
      ) {
        final record = (records + index).ref;
        final certification = switch (record.certificationState) {
          _pendingNeutral => FlarkCertification.pendingNeutral,
          _currentCertified => FlarkCertification.currentCertified,
          _ => throw FlarkNativeException(
            'decode_viewport',
            _notCertified,
            record.certificationState,
          ),
        };
        return FlarkCertificationRange(
          certification: certification,
          sourceBytes: FlarkSourceRange(
            record.sourceRange.startByte,
            record.sourceRange.endByte,
          ),
          sourceUtf16: FlarkSourceRange(
            record.sourceUtf16Range.startByte,
            record.sourceUtf16Range.endByte,
          ),
        );
      }, growable: false);
      final source = utf8.decode(
        (payload + recordBytes).asTypedList(header.payloadBytes - recordBytes),
      );
      final certification = switch (header.certificationState) {
        _pendingNeutral => FlarkCertification.pendingNeutral,
        _currentCertified => FlarkCertification.currentCertified,
        _mixedCurrent => FlarkCertification.mixedCurrent,
        _ => throw FlarkNativeException(
          'decode_viewport',
          _notCertified,
          header.certificationState,
        ),
      };
      return FlarkViewport(
        revision: header.revision,
        snapshot: header.snapshot,
        requestedBytes: requested,
        coveredBytes: covered,
        coveredUtf16: coveredUtf16,
        certification: certification,
        rows: const [],
        neutralSource: source,
        continuation: header.continuation,
        certificationRanges: ranges,
      );
    }
    if (header.recordKind != _semanticRecord ||
        header.certificationState != _currentCertified) {
      throw FlarkNativeException(
        'decode_viewport',
        _notCertified,
        header.recordKind,
      );
    }
    final rowRecordBytes =
        header.itemCount * sizeOf<FlarkV4ViewportRowRecord>();
    if (rowRecordBytes > header.payloadBytes) {
      throw FlarkNativeException(
        'decode_viewport',
        _notCertified,
        header.payloadBytes,
      );
    }
    final records = payload.cast<FlarkV4ViewportRowRecord>();
    var totalInlineFacts = 0;
    for (var index = 0; index < header.itemCount; index++) {
      final record = (records + index).ref;
      final inlineIsAuthoritative = record.flags & _inlineAuthoritative != 0;
      if (record.flags & ~_knownViewportRowFlags != 0 ||
          (!inlineIsAuthoritative && record.inlineFactCount != 0) ||
          record.inlineFactCount > _maxInlineFactsPerRow) {
        throw FlarkNativeException(
          'decode_viewport',
          _notCertified,
          record.flags,
        );
      }
      totalInlineFacts += record.inlineFactCount;
    }
    final expectedPayloadBytes =
        rowRecordBytes + totalInlineFacts * sizeOf<FlarkV4InlineFactRecord>();
    if (expectedPayloadBytes != header.payloadBytes) {
      throw FlarkNativeException(
        'decode_viewport',
        _notCertified,
        header.payloadBytes,
      );
    }
    final inlineRecords = (payload + rowRecordBytes)
        .cast<FlarkV4InlineFactRecord>();
    var nextInlineFact = 0;
    final rows = List<FlarkViewportRow>.generate(header.itemCount, (index) {
      final record = (records + index).ref;
      final capability = switch (record.flags) {
        final int flags when flags & 1 != 0 =>
          FlarkViewportRowEditCapability.contiguous,
        final int flags when flags & 2 != 0 =>
          FlarkViewportRowEditCapability.projectedReserved,
        _ => FlarkViewportRowEditCapability.unavailable,
      };
      final editable = capability != FlarkViewportRowEditCapability.unavailable;
      final sourceBytes = FlarkSourceRange(
        record.sourceStartByte,
        record.sourceEndByte,
      );
      final sourceUtf16 = FlarkSourceRange(
        record.sourceStartUtf16,
        record.sourceEndUtf16,
      );
      final editableBytes = editable
          ? FlarkSourceRange(record.editableStartByte, record.editableEndByte)
          : null;
      final editableUtf16 = editable
          ? FlarkSourceRange(record.editableStartUtf16, record.editableEndUtf16)
          : null;
      final headingLevel = record.kind == _headingKind
          ? record.semanticVariant & _headingLevelMask
          : null;
      final headingStyle = record.kind == _headingKind
          ? (record.semanticVariant & _headingSetext == 0
                ? FlarkHeadingStyle.atx
                : FlarkHeadingStyle.setext)
          : null;
      if (record.kind == _headingKind &&
          (headingLevel == 0 ||
              headingLevel! > 6 ||
              record.semanticVariant & ~_knownHeadingVariantBits != 0)) {
        throw FlarkNativeException(
          'decode_viewport',
          _notCertified,
          record.semanticVariant,
        );
      }
      FlarkListItemPresentation? listItem;
      FlarkBlockQuotePresentation? blockQuote;
      FlarkCodeBlockPresentation? codeBlock;
      var thematicBreak = false;
      final listMarker = matchesListRowKind(record.kind)
          ? record.semanticVariant & _listMarkerMask
          : 0;
      final hasBlockQuotePresentation =
          listMarker == 0 &&
          record.semanticVariant & _blockQuotePresentation != 0 &&
          record.kind == _paragraphKind;
      if (hasBlockQuotePresentation) {
        final depth =
            (record.semanticVariant & _blockQuoteDepthMask) >>
            _blockQuoteDepthShift;
        if (depth == 0 ||
            record.semanticVariant & ~_knownBlockQuoteVariantBits != 0 ||
            !_hasValidPresentationPrefix(record) ||
            record.semanticValue != 0) {
          throw FlarkNativeException(
            'decode_viewport',
            _notCertified,
            record.semanticVariant,
          );
        }
        blockQuote = FlarkBlockQuotePresentation(
          prefixBytes: FlarkSourceRange(
            record.presentationPrefixStartByte,
            record.presentationPrefixEndByte,
          ),
          prefixUtf16: FlarkSourceRange(
            record.presentationPrefixStartUtf16,
            record.presentationPrefixEndUtf16,
          ),
          nestingDepth: depth,
          simpleContinuation:
              record.semanticVariant & _blockQuoteSimpleContinuation != 0,
        );
      } else if (listMarker != 0) {
        final depth =
            (record.semanticVariant & _listDepthMask) >> _listDepthShift;
        final markerOffset =
            (record.semanticVariant & _listMarkerOffsetMask) >>
            _listMarkerOffsetShift;
        final markerStyle = switch (listMarker) {
          _listHyphen => FlarkListMarkerStyle.hyphen,
          _listPlus => FlarkListMarkerStyle.plus,
          _listAsterisk => FlarkListMarkerStyle.asterisk,
          _listOrderedPeriod => FlarkListMarkerStyle.orderedPeriod,
          _listOrderedParenthesis => FlarkListMarkerStyle.orderedParenthesis,
          _ => null,
        };
        final ordered = listMarker >= _listOrderedPeriod;
        final task = record.semanticVariant & _listTask != 0;
        final taskChecked = record.semanticVariant & _listTaskChecked != 0;
        if (!matchesListRowKind(record.kind) ||
            markerStyle == null ||
            depth == 0 ||
            (taskChecked && !task) ||
            (record.semanticVariant & ~_knownListVariantBits) != 0 ||
            !_hasValidPresentationPrefix(record) ||
            (ordered
                ? record.semanticValue > 999999999
                : record.semanticValue != 0)) {
          throw FlarkNativeException(
            'decode_viewport',
            _notCertified,
            record.semanticVariant,
          );
        }
        listItem = FlarkListItemPresentation(
          markerStyle: markerStyle,
          markerValue: record.semanticValue,
          prefixBytes: FlarkSourceRange(
            record.presentationPrefixStartByte,
            record.presentationPrefixEndByte,
          ),
          prefixUtf16: FlarkSourceRange(
            record.presentationPrefixStartUtf16,
            record.presentationPrefixEndUtf16,
          ),
          nestingDepth: depth,
          markerOffset: markerOffset,
          simpleContinuation:
              record.semanticVariant & _listSimpleContinuation != 0,
          startsList: record.semanticVariant & _listStartsList != 0,
          taskChecked: task ? taskChecked : null,
        );
      } else if (record.kind == _indentedCodeKind ||
          record.kind == _fencedCodeKind) {
        final fenced = record.semanticVariant & _codeFenced != 0;
        final offset =
            (record.semanticVariant & _codeFenceOffsetMask) >>
            _codeFenceOffsetShift;
        final validIndented =
            record.kind == _indentedCodeKind &&
            record.semanticVariant == _codePresentation &&
            record.semanticValue == 0;
        final validFenced =
            record.kind == _fencedCodeKind &&
            fenced &&
            record.semanticVariant & ~_knownCodeVariantBits == 0 &&
            record.semanticValue >= 3;
        if ((!validIndented && !validFenced) ||
            !_hasAbsentPresentationPrefix(record)) {
          throw FlarkNativeException(
            'decode_viewport',
            _notCertified,
            record.semanticVariant,
          );
        }
        codeBlock = FlarkCodeBlockPresentation(
          style: validIndented
              ? FlarkCodeBlockStyle.indented
              : record.semanticVariant & _codeTilde != 0
              ? FlarkCodeBlockStyle.fencedTilde
              : FlarkCodeBlockStyle.fencedBacktick,
          minimumClosingLength: record.semanticValue,
          fenceOffset: offset,
          closed: record.semanticVariant & _codeClosed != 0,
        );
      } else if (record.kind == _thematicBreakKind) {
        if (record.semanticVariant != _thematicBreakPresentation ||
            record.semanticValue != 0 ||
            !_hasAbsentPresentationPrefix(record)) {
          throw FlarkNativeException(
            'decode_viewport',
            _notCertified,
            record.semanticVariant,
          );
        }
        thematicBreak = true;
      } else if (record.kind != _headingKind &&
          (record.semanticVariant != 0 ||
              record.semanticValue != 0 ||
              !_hasAbsentPresentationPrefix(record))) {
        throw FlarkNativeException(
          'decode_viewport',
          _notCertified,
          record.semanticVariant,
        );
      }
      if (record.kind == _headingKind &&
          (record.semanticValue != 0 ||
              !_hasAbsentPresentationPrefix(record))) {
        throw FlarkNativeException(
          'decode_viewport',
          _notCertified,
          record.semanticVariant,
        );
      }
      final inlineFacts = record.flags & _inlineAuthoritative != 0
          ? List<FlarkInlineFact>.generate(record.inlineFactCount, (index) {
              final fact = _decodeInlineFact(
                (inlineRecords + nextInlineFact + index).ref,
                sourceBytes: sourceBytes,
                sourceUtf16: sourceUtf16,
                editableBytes: editableBytes,
                editableUtf16: editableUtf16,
              );
              return fact;
            }, growable: false)
          : null;
      nextInlineFact += record.inlineFactCount;
      return FlarkViewportRow(
        ordinal: record.ordinal,
        kind: record.kind,
        sourceBytes: sourceBytes,
        sourceUtf16: sourceUtf16,
        editableBytes: editableBytes,
        editableUtf16: editableUtf16,
        editCapability: capability,
        headingLevel: headingLevel,
        headingStyle: headingStyle,
        listItem: listItem,
        blockQuote: blockQuote,
        codeBlock: codeBlock,
        thematicBreak: thematicBreak,
        pathDepth: record.pathDepth,
        inlineFacts: inlineFacts,
      );
    }, growable: false);
    return FlarkViewport(
      revision: header.revision,
      snapshot: header.snapshot,
      requestedBytes: requested,
      coveredBytes: covered,
      coveredUtf16: coveredUtf16,
      certification: FlarkCertification.currentCertified,
      rows: rows,
      neutralSource: null,
      continuation: header.continuation,
    );
  }

  static bool matchesListRowKind(int kind) =>
      kind == _paragraphKind || kind == _emptyListItemKind;

  static FlarkInlineFact _decodeInlineFact(
    FlarkV4InlineFactRecord record, {
    required FlarkSourceRange sourceBytes,
    required FlarkSourceRange sourceUtf16,
    required FlarkSourceRange? editableBytes,
    required FlarkSourceRange? editableUtf16,
  }) {
    final kind = switch (record.kind) {
      _inlineFactEmphasis => FlarkInlineFactKind.emphasis,
      _inlineFactStrong => FlarkInlineFactKind.strong,
      _inlineFactCode => FlarkInlineFactKind.code,
      _inlineFactStrikethrough => FlarkInlineFactKind.strikethrough,
      _inlineFactAutolinkUri => FlarkInlineFactKind.autolinkUri,
      _inlineFactAutolinkEmail => FlarkInlineFactKind.autolinkEmail,
      _inlineFactBackslashEscape => FlarkInlineFactKind.backslashEscape,
      _inlineFactHardLineBreak => FlarkInlineFactKind.hardLineBreak,
      _inlineFactReplacement => FlarkInlineFactKind.replacement,
      _inlineFactDirectLink => FlarkInlineFactKind.directLink,
      _inlineFactDirectImage => FlarkInlineFactKind.directImage,
      _inlineFactReferenceLink => FlarkInlineFactKind.referenceLink,
      _inlineFactReferenceImage => FlarkInlineFactKind.referenceImage,
      _ => throw FlarkNativeException(
        'decode_viewport',
        _notCertified,
        record.kind,
      ),
    };
    final factSourceBytes = FlarkSourceRange(
      record.sourceStartByte,
      record.sourceEndByte,
    );
    final factSourceUtf16 = FlarkSourceRange(
      record.sourceStartUtf16,
      record.sourceEndUtf16,
    );
    final contentBytes = FlarkSourceRange(
      record.contentStartByte,
      record.contentEndByte,
    );
    final contentUtf16 = FlarkSourceRange(
      record.contentStartUtf16,
      record.contentEndUtf16,
    );
    final validFlags = switch (kind) {
      FlarkInlineFactKind.autolinkUri =>
        record.flags & ~_inlineFactAutolinkUriWww == 0,
      FlarkInlineFactKind.code =>
        record.flags &
                ~(_inlineFactCodeNormalizeLineEndings |
                    _inlineFactCodeTrimOneSpace) ==
            0,
      _ => record.flags == 0,
    };
    final replacement = kind == FlarkInlineFactKind.replacement
        ? _decodeReplacement(record.replacementFirst, record.replacementSecond)
        : null;
    if (!validFlags ||
        (kind == FlarkInlineFactKind.replacement && replacement == null) ||
        (kind != FlarkInlineFactKind.replacement &&
            (record.replacementFirst != 0 || record.replacementSecond != 0)) ||
        editableBytes == null ||
        editableUtf16 == null ||
        factSourceBytes.start < sourceBytes.start ||
        factSourceBytes.end > sourceBytes.end ||
        factSourceUtf16.start < sourceUtf16.start ||
        factSourceUtf16.end > sourceUtf16.end ||
        factSourceBytes.start < editableBytes.start ||
        factSourceBytes.end > editableBytes.end ||
        factSourceUtf16.start < editableUtf16.start ||
        factSourceUtf16.end > editableUtf16.end ||
        factSourceBytes.length == 0 ||
        factSourceUtf16.length == 0 ||
        contentBytes.start < factSourceBytes.start ||
        contentBytes.end > factSourceBytes.end ||
        contentUtf16.start < factSourceUtf16.start ||
        contentUtf16.end > factSourceUtf16.end ||
        (contentBytes.length == 0) != (contentUtf16.length == 0) ||
        (kind == FlarkInlineFactKind.replacement &&
            (contentBytes.start != factSourceBytes.start ||
                contentBytes.end != factSourceBytes.end ||
                contentUtf16.start != factSourceUtf16.start ||
                contentUtf16.end != factSourceUtf16.end))) {
      throw FlarkNativeException('decode_viewport', _notCertified, record.kind);
    }
    return FlarkInlineFact(
      kind: kind,
      flags: record.flags,
      sourceBytes: factSourceBytes,
      sourceUtf16: factSourceUtf16,
      contentBytes: contentBytes,
      contentUtf16: contentUtf16,
      replacement: replacement,
    );
  }

  static String? _decodeReplacement(int first, int second) {
    if (!_isUnicodeScalar(first) ||
        (second != 0 && !_isUnicodeScalar(second))) {
      return null;
    }
    return String.fromCharCodes([first, if (second != 0) second]);
  }

  static bool _isUnicodeScalar(int value) =>
      value > 0 && value <= 0x10ffff && (value < 0xd800 || value > 0xdfff);

  static bool _hasAbsentPresentationPrefix(FlarkV4ViewportRowRecord record) =>
      record.presentationPrefixStartByte == _absentPresentationPrefix &&
      record.presentationPrefixEndByte == _absentPresentationPrefix &&
      record.presentationPrefixStartUtf16 == _absentPresentationPrefix &&
      record.presentationPrefixEndUtf16 == _absentPresentationPrefix;

  static bool _hasValidPresentationPrefix(FlarkV4ViewportRowRecord record) =>
      record.presentationPrefixStartByte < record.presentationPrefixEndByte &&
      record.presentationPrefixEndByte <= record.sourceStartByte &&
      record.presentationPrefixStartUtf16 < record.presentationPrefixEndUtf16 &&
      record.presentationPrefixEndUtf16 <= record.sourceStartUtf16;

  String readSource() {
    return readSourceRange(0, _sourceByteLength);
  }

  String readSourceUtf16Range(int startUtf16, int endUtf16) {
    if (startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        endUtf16 > _sourceUtf16Length) {
      throw RangeError.range(endUtf16, startUtf16, _sourceUtf16Length);
    }
    return readSourceRange(
      _convertCoordinate(startUtf16, from: 2, to: 1),
      _convertCoordinate(endUtf16, from: 2, to: 1),
    );
  }

  String readSourceRange(int startByte, int endByte) {
    if (startByte < 0 || endByte < startByte || endByte > _sourceByteLength) {
      throw RangeError.range(endByte, startByte, _sourceByteLength);
    }
    final bytes = BytesBuilder(copy: false);
    for (var start = startByte; start < endByte; start += _maxChunkBytes) {
      final end = math.min(start + _maxChunkBytes, endByte);
      bytes.add(_readSourceRange(start, end));
    }
    return utf8.decode(bytes.takeBytes());
  }

  void close({int workUnits = _defaultWorkUnits}) {
    if (_closed) return;
    final request = calloc<FlarkV4CloseRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4CloseRequest>()
        ..flags = 0
        ..progressToken = 0;
      _fillSession(request.ref.session);
      _fillBudget(request.ref.budget, workUnits: workUnits);
      var status = _bindings.closeBegin(request, outcome);
      _requireStatus('close_begin', status, outcome.ref, {
        _ok,
        _budgetExhausted,
      });
      request.ref.progressToken = outcome.ref.progressToken;
      while (true) {
        while (status == _budgetExhausted) {
          status = _bindings.closePump(request, outcome);
          _requireStatus('close_pump', status, outcome.ref, {
            _ok,
            _budgetExhausted,
          });
          request.ref.progressToken = outcome.ref.progressToken;
        }
        status = _bindings.closeFinish(request, outcome);
        _requireStatus('close_finish', status, outcome.ref, {
          _ok,
          _closeIncomplete,
        });
        if (status == _ok) break;
        status = _bindings.closePump(request, outcome);
        _requireStatus('close_pump', status, outcome.ref, {
          _ok,
          _budgetExhausted,
        });
        request.ref.progressToken = outcome.ref.progressToken;
      }
      _closed = true;
    } finally {
      calloc
        ..free(request)
        ..free(outcome);
    }
  }

  Uint8List _readSourceRange(int start, int end) {
    final request = calloc<FlarkV4SourceReadRequest>();
    final outcome = calloc<FlarkV4Outcome>();
    final payloadLength = end - start;
    final capacity = sizeOf<FlarkV4ResultPageHeader>() + payloadLength;
    final output = calloc<Uint8>(capacity);
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4SourceReadRequest>()
        ..flags = 0
        ..revision = _revision;
      _fillSession(request.ref.session);
      request.ref.range
        ..startByte = start
        ..endByte = end;
      final status = _bindings.sourceRead(request, output, capacity, outcome);
      _requireStatus('source_read', status, outcome.ref, {_ok});
      final header = output.cast<FlarkV4ResultPageHeader>().ref;
      return Uint8List.fromList(
        (output + sizeOf<FlarkV4ResultPageHeader>()).asTypedList(
          header.payloadBytes,
        ),
      );
    } finally {
      calloc
        ..free(request)
        ..free(outcome)
        ..free(output);
    }
  }

  void _fillSession(FlarkV4SessionRef session) {
    session
      ..session = _session
      ..ownerToken = _ownerToken;
  }

  static void _fillBudget(
    FlarkV4WorkBudget budget, {
    required int workUnits,
    int maxRows = 256,
  }) {
    budget
      ..maxWorkUnits = workUnits
      ..advisoryMaxMicros = 0
      ..maxResultItems = maxRows
      ..maxResultBytes = _resultPayloadBytes;
  }

  static Pointer<Uint8> _copyBytes(List<int> bytes, int start, int length) {
    final pointer = calloc<Uint8>(math.max(1, length));
    if (length != 0) {
      pointer.asTypedList(length).setRange(0, length, bytes, start);
    }
    return pointer;
  }

  static void _requireStatus(
    String operation,
    int status,
    FlarkV4Outcome outcome,
    Set<int> accepted,
  ) {
    if (!accepted.contains(status) || outcome.status != status) {
      throw FlarkNativeException(operation, status, outcome.detailCode);
    }
  }

  static void _negotiate(FlarkV4Bindings bindings) {
    final request = calloc<FlarkV4NegotiateRequest>();
    final info = calloc<FlarkV4AbiInfo>();
    final outcome = calloc<FlarkV4Outcome>();
    try {
      request.ref
        ..structSize = sizeOf<FlarkV4NegotiateRequest>()
        ..requestedMajor = _abiMajor
        ..requestedMinor = _abiMinor
        ..requiredCapabilityBits = _requiredCapabilityBits;
      final status = bindings.negotiate(request, info, outcome);
      _requireStatus('negotiate', status, outcome.ref, {_ok});
      final value = info.ref;
      final compatible =
          value.structSize >= sizeOf<FlarkV4AbiInfo>() &&
          value.abiMajor == _abiMajor &&
          value.abiMinor >= _abiMinor &&
          value.capabilityBits & _requiredCapabilityBits ==
              _requiredCapabilityBits &&
          value.maxSmallEditBytes >= _maximumSmallEditBytes &&
          value.maxBulkChunkBytes >= _maxChunkBytes &&
          value.maxSourceChunkBytes >= _maxChunkBytes &&
          value.maxResultBytes >= _resultPayloadBytes &&
          value.maxQueryItems >= 256 &&
          value.maxTransactionEdits >= 1;
      if (!compatible) {
        throw StateError('Flark v4 runtime returned incompatible ABI limits.');
      }
    } finally {
      calloc
        ..free(request)
        ..free(info)
        ..free(outcome);
    }
  }
}
