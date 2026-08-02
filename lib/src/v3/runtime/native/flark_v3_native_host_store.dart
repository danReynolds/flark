import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../../host/host.dart';
import '../../source/source.dart';
import '../flark_v3_parser_transport.dart';

const int flarkV3NativeHostAbiVersion = 0x00030005;
const int flarkV3NativeHostMaximumQueryBytes = 64 * 1024;
const int flarkV3NativeHostInlineSidecarMaximumQueryBytes = 128 * 1024;
const int flarkV3NativeHostViewportMaximumQueryBytes = 256 * 1024;
const int flarkV3NativeHostBulkScratchBytes =
    flarkV3NativeHostViewportMaximumQueryBytes;
const int flarkV3NativeHostViewportPresentationBeginBytes = 348;
const int flarkV3NativeHostViewportPresentationCommitRequestBytes = 56;
const int flarkV3NativeHostViewportPresentationAckBytes = 296;
const int flarkV3NativeHostViewportPresentationPollReceiptBytes = 324;
const int flarkV3NativeHostViewportPresentationQueryBytes = 320;
const int flarkV3NativeHostViewportPresentationQueryReceiptBytes = 32;
const int flarkV3NativeHostStructuralOrdinalWindowQueryBytes = 92;
const int flarkV3NativeHostStructuralOrdinalWindowReceiptBytes = 132;

const int _statusOk = 0x0000;
const int _pointQuerySchema = 1;
const int _blockRangeQuerySchema = 1;
const int _structuralOrdinalWindowQuerySchema = 1;
const int _inlineSidecarQuerySchema = 3;

final NativeFinalizer _scratchAllocationFinalizer = NativeFinalizer(
  calloc.nativeFree,
);

/// A native host ABI or ownership failure rather than an expected protocol
/// race. Expected stale/backpressure/bound outcomes are returned as typed
/// [FlarkV3HostRejected] values by [FlarkV3NativeHostStore].
final class FlarkV3NativeHostException implements Exception {
  const FlarkV3NativeHostException({
    required this.operation,
    required this.status,
    this.detail,
  });

  final String operation;
  final int status;
  final String? detail;

  @override
  String toString() {
    final suffix = detail == null ? '' : ': $detail';
    return 'FlarkV3NativeHostException($operation, '
        'status=0x${status.toRadixString(16).padLeft(4, '0')}$suffix)';
  }
}

/// Caller-isolate owner of one structural, inline-sidecar, and viewport host.
///
/// All Dart-side FFI inputs, outputs, and query scratch are allocated once.
/// Publication bytes are synchronously copied into bounded Rust-owned staging
/// storage; the caller's Dart view is never retained and native ownership is
/// still charged to the admitted offer envelope.
final class FlarkV3NativeHostStore
    implements
        FlarkV3HostStore,
        FlarkV3BlockRangeHostStore,
        FlarkV3StructuralOrdinalWindowHostStore,
        FlarkV3InlineSidecarHostStore,
        FlarkV3ViewportPresentationHostStore,
        Finalizable {
  FlarkV3NativeHostStore._({
    required _NativeHostSymbols symbols,
    required _NativeHostScratch scratch,
    required Pointer<Void> finalizerToken,
  }) : _symbols = symbols,
       _scratch = scratch,
       _finalizerToken = finalizerToken,
       _emergencyFinalizer = NativeFinalizer(symbols.emergencyFinalize) {
    scratch.attachTo(this);
    _emergencyFinalizer.attach(this, finalizerToken, detach: this);
  }

  factory FlarkV3NativeHostStore.create({
    required DynamicLibrary library,
    required FlarkV3DocumentSessionId documentSession,
    int grammarRevision = flarkV3CurrentGrammarRevision,
    FlarkV3SyntaxProfileId? syntaxProfile,
    FlarkV3StructuralAuthorityMask? authorityMask,
  }) {
    final symbols = _NativeHostSymbols.fromLibrary(library);
    final loadedAbi = symbols.abiVersion();
    if (loadedAbi != flarkV3NativeHostAbiVersion) {
      throw FlarkV3NativeHostException(
        operation: 'abiVersion',
        status: loadedAbi,
        detail: 'expected 0x${flarkV3NativeHostAbiVersion.toRadixString(16)}',
      );
    }
    final scratch = _NativeHostScratch.allocate();
    var handleCreated = false;
    Pointer<Void>? token;
    try {
      var status = symbols.standardConfig(scratch.config);
      _requireNativeOk('configStandard', status);
      if (scratch.config.ref.abiVersion != flarkV3NativeHostAbiVersion ||
          scratch.config.ref.structSize != sizeOf<_NativeHostConfig>() ||
          sizeOf<_NativeHostConfig>() != 56 ||
          sizeOf<_NativeHostSourceVersion>() != 44 ||
          sizeOf<_NativeHostOfferBegin>() != 144 ||
          sizeOf<_NativeHostCommitRequest>() != 56 ||
          sizeOf<_NativeHostStructuralAck>() != 124 ||
          sizeOf<_NativeHostU64>() != 8 ||
          sizeOf<_NativeHostInlineSidecarBinding>() != 56 ||
          sizeOf<_NativeHostInlineSidecarDisposition>() != 80 ||
          sizeOf<_NativeHostInlineSidecarBegin>() != 364 ||
          sizeOf<_NativeHostInlineSidecarCommitRequest>() != 56 ||
          sizeOf<_NativeHostInlineSidecarAck>() != 212 ||
          sizeOf<_NativeHostInlineSidecarPollReceipt>() != 240 ||
          sizeOf<_NativeHostInlineSidecarQuery>() != 80 ||
          sizeOf<_NativeHostInlineSidecarQueryReceipt>() != 32 ||
          sizeOf<_NativeHostViewportPresentationBegin>() !=
              flarkV3NativeHostViewportPresentationBeginBytes ||
          sizeOf<_NativeHostViewportPresentationCommitRequest>() !=
              flarkV3NativeHostViewportPresentationCommitRequestBytes ||
          sizeOf<_NativeHostViewportPresentationAck>() !=
              flarkV3NativeHostViewportPresentationAckBytes ||
          sizeOf<_NativeHostViewportPresentationPollReceipt>() !=
              flarkV3NativeHostViewportPresentationPollReceiptBytes ||
          sizeOf<_NativeHostViewportPresentationQuery>() !=
              flarkV3NativeHostViewportPresentationQueryBytes ||
          sizeOf<_NativeHostViewportPresentationQueryReceipt>() !=
              flarkV3NativeHostViewportPresentationQueryReceiptBytes ||
          sizeOf<_NativeHostPollReceipt>() != 152 ||
          sizeOf<_NativeHostPointQuery>() != 96 ||
          sizeOf<_NativeHostPointQueryReceipt>() != 112 ||
          sizeOf<_NativeHostBlockRangeQuery>() != 172 ||
          sizeOf<_NativeHostBlockRangeQueryReceipt>() != 188 ||
          sizeOf<_NativeHostStructuralOrdinalWindowQuery>() !=
              flarkV3NativeHostStructuralOrdinalWindowQueryBytes ||
          sizeOf<_NativeHostStructuralOrdinalWindowReceipt>() !=
              flarkV3NativeHostStructuralOrdinalWindowReceiptBytes) {
        throw const FlarkV3NativeHostException(
          operation: 'configLayout',
          status: 0x0100,
          detail: 'Dart and native host ABI layouts differ',
        );
      }
      _writeId(scratch.config.ref.documentSession, documentSession);
      scratch.config.ref
        ..grammarRevision = grammarRevision
        ..syntaxProfile = (syntaxProfile ?? FlarkV3SyntaxProfileId(1)).value
        ..authorityMask =
            (authorityMask ?? FlarkV3StructuralAuthorityMask.complete).bits;
      status = symbols.create(scratch.config, scratch.handle);
      _requireNativeOk('create', status);
      handleCreated = true;
      status = symbols.finalizerTokenCreate(
        scratch.handle.ref,
        scratch.finalizerTokenOutput,
      );
      _requireNativeOk('finalizerTokenCreate', status);
      token = scratch.finalizerTokenOutput.value;
      if (token == nullptr) {
        throw const FlarkV3NativeHostException(
          operation: 'finalizerTokenCreate',
          status: 0x0111,
          detail: 'native host returned a null finalizer token',
        );
      }
      return FlarkV3NativeHostStore._(
        symbols: symbols,
        scratch: scratch,
        finalizerToken: token,
      );
    } catch (_) {
      if (token != null && token != nullptr) {
        // No Dart owner was fully constructed, so synchronously invoke the
        // same self-contained token callback a NativeFinalizer would own.
        symbols.emergencyFinalize.asFunction<void Function(Pointer<Void>)>()(
          token,
        );
      } else if (handleCreated) {
        symbols.emergencyDestroy(scratch.handle.ref);
      }
      scratch.free();
      rethrow;
    }
  }

  final _NativeHostSymbols _symbols;
  final _NativeHostScratch _scratch;
  final NativeFinalizer _emergencyFinalizer;
  Pointer<Void> _finalizerToken;

  FlarkV3SourceVersion? _currentSource;
  bool _released = false;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeSourceVersion(_scratch.source.ref, sourceVersion);
    final result = _unitCall(
      'observeSource',
      () => _symbols.observeSource(
        _scratch.handle.ref,
        _scratch.source,
        _scratch.callReceipt,
      ),
    );
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _currentSource = sourceVersion;
    }
    return result;
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeOffer(_scratch.begin.ref, begin);
    return switch ((begin.mode, begin.baseAck)) {
      (FlarkV3PublicationMode.fullSnapshot, null) => _unitCall(
        'beginOffer',
        () => _symbols.beginOffer(
          _scratch.handle.ref,
          _scratch.begin,
          _scratch.callReceipt,
        ),
      ),
      (
        FlarkV3PublicationMode.exactBaseReferencesDelta,
        final FlarkV3StructuralAck base,
      ) =>
        _unitCall('beginReferencesDelta', () {
          _writeAck(_scratch.ack.ref, base);
          return _symbols.beginReferencesDelta(
            _scratch.handle.ref,
            _scratch.begin,
            _scratch.ack,
            _scratch.callReceipt,
          );
        }),
      (
        FlarkV3PublicationMode.exactBaseDelta,
        final FlarkV3StructuralAck base,
      ) =>
        _unitCall('beginExactBaseDelta', () {
          _writeAck(_scratch.ack.ref, base);
          return _symbols.beginExactBaseDelta(
            _scratch.handle.ref,
            _scratch.begin,
            _scratch.ack,
            _scratch.callReceipt,
          );
        }),
      _ => const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.baseMismatch,
          'Publication mode does not bind the required exact base ACK.',
        ),
      ),
    };
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginInlineSidecarOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeInlineSidecarBegin(_scratch.inlineSidecarBegin.ref, begin);
    return _unitCall(
      'beginInlineSidecarOffer',
      () => _symbols.beginInlineSidecarOffer(
        _scratch.handle.ref,
        _scratch.inlineSidecarBegin,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeViewportPresentationBegin(
      _scratch.viewportPresentationBegin.ref,
      begin,
    );
    return _unitCall(
      'beginViewportPresentationOffer',
      () => _symbols.beginViewportPresentationOffer(
        _scratch.handle.ref,
        _scratch.viewportPresentationBegin,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (packet.rawBytes.length > flarkV3NativeHostBulkScratchBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Publication packet exceeds the native bulk-scratch ceiling.',
        ),
      );
    }
    _scratch.bulk
        .asTypedList(packet.rawBytes.length)
        .setRange(0, packet.rawBytes.length, packet.rawBytes);
    return _unitCall(
      'admitPacket',
      () => _symbols.admitPacket(
        _scratch.handle.ref,
        _scratch.bulk,
        packet.rawBytes.length,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitInlineSidecarPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (packet.rawBytes.length > flarkV3NativeHostBulkScratchBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Inline-sidecar packet exceeds the native bulk-scratch ceiling.',
        ),
      );
    }
    _scratch.bulk
        .asTypedList(packet.rawBytes.length)
        .setRange(0, packet.rawBytes.length, packet.rawBytes);
    return _unitCall(
      'admitInlineSidecarPacket',
      () => _symbols.admitInlineSidecarPacket(
        _scratch.handle.ref,
        _scratch.bulk,
        packet.rawBytes.length,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (packet.rawBytes.length > flarkV3NativeHostBulkScratchBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'VPB1 packet exceeds the native bulk-scratch ceiling.',
        ),
      );
    }
    _scratch.bulk
        .asTypedList(packet.rawBytes.length)
        .setRange(0, packet.rawBytes.length, packet.rawBytes);
    return _unitCall(
      'admitViewportPresentationPacket',
      () => _symbols.admitViewportPresentationPacket(
        _scratch.handle.ref,
        _scratch.bulk,
        packet.rawBytes.length,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeCommit(_scratch.commit.ref, request);
    return _unitCall(
      'requestCommit',
      () => _symbols.requestCommit(
        _scratch.handle.ref,
        _scratch.commit,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestInlineSidecarCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeInlineSidecarCommit(_scratch.inlineSidecarCommit.ref, request);
    return _unitCall(
      'requestInlineSidecarCommit',
      () => _symbols.requestInlineSidecarCommit(
        _scratch.handle.ref,
        _scratch.inlineSidecarCommit,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeViewportPresentationCommit(
      _scratch.viewportPresentationCommit.ref,
      request,
    );
    return _unitCall(
      'requestViewportPresentationCommit',
      () => _symbols.requestViewportPresentationCommit(
        _scratch.handle.ref,
        _scratch.viewportPresentationCommit,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeId(_scratch.id128.ref.words, offerId);
    return _unitCall(
      'abortOffer',
      () => _symbols.abortOffer(
        _scratch.handle.ref,
        _scratch.id128,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortInlineSidecarOffer(
    FlarkV3OfferId offerId,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeId(_scratch.id128.ref.words, offerId);
    return _unitCall(
      'abortInlineSidecarOffer',
      () => _symbols.abortInlineSidecarOffer(
        _scratch.handle.ref,
        _scratch.id128,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeId(_scratch.id128.ref.words, offerId);
    return _unitCall(
      'abortViewportPresentationOffer',
      () => _symbols.abortViewportPresentationOffer(
        _scratch.handle.ref,
        _scratch.id128,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final unavailable = _unavailable<FlarkV3HostPollOutcome>();
    if (unavailable != null) return unavailable;
    _scratch.workGrant.ref
      ..inspectBytes = grant.inspectBytes
      ..copyBytes = grant.copyBytes
      ..transitions = grant.transitions;
    final status = _symbols.poll(
      _scratch.handle.ref,
      _scratch.workGrant.ref,
      _scratch.pollReceipt,
    );
    final rejected = _rejection(
      operation: 'poll',
      status: status,
      reason: _scratch.pollReceipt.ref.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    final receipt = _scratch.pollReceipt.ref;
    final outcome = switch (receipt.outcome) {
      0 => const FlarkV3HostPollPending(),
      1 => FlarkV3HostPacketCredit(
        offerId: _readOfferId(receipt.offerId),
        nextFrameOrdinal: receipt.nextFrameOrdinal,
      ),
      2 => FlarkV3HostCommitted(_readAck(receipt.ack)),
      3 => FlarkV3HostAbortComplete(_readOfferId(receipt.offerId)),
      4 => const FlarkV3HostClosed(),
      _ => throw FlarkV3NativeHostException(
        operation: 'pollOutcome',
        status: 0x0111,
        detail: 'unknown outcome ${receipt.outcome}',
      ),
    };
    if (outcome is FlarkV3HostClosed) _releaseNormally();
    return FlarkV3HostAccepted(outcome);
  }

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarHostPollOutcome> pollInlineSidecar(
    FlarkV3HostWorkGrant grant,
  ) {
    final unavailable = _unavailable<FlarkV3InlineSidecarHostPollOutcome>();
    if (unavailable != null) return unavailable;
    _scratch.workGrant.ref
      ..inspectBytes = grant.inspectBytes
      ..copyBytes = grant.copyBytes
      ..transitions = grant.transitions;
    final status = _symbols.pollInlineSidecar(
      _scratch.handle.ref,
      _scratch.workGrant.ref,
      _scratch.inlineSidecarPollReceipt,
    );
    final rejected = _rejection(
      operation: 'pollInlineSidecar',
      status: status,
      reason: _scratch.inlineSidecarPollReceipt.ref.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    final receipt = _scratch.inlineSidecarPollReceipt.ref;
    final outcome = switch (receipt.outcome) {
      0 => const FlarkV3InlineSidecarHostPollPending(),
      1 => FlarkV3InlineSidecarHostPacketCredit(
        offerId: _readOfferId(receipt.offerId),
        nextFrameOrdinal: receipt.nextFrameOrdinal,
      ),
      2 => FlarkV3InlineSidecarHostCommitted(
        _readInlineSidecarAck(receipt.ack),
      ),
      3 => FlarkV3InlineSidecarHostAbortComplete(_readOfferId(receipt.offerId)),
      4 => const FlarkV3InlineSidecarHostClosed(),
      _ => throw FlarkV3NativeHostException(
        operation: 'pollInlineSidecarOutcome',
        status: 0x0111,
        detail: 'unknown outcome ${receipt.outcome}',
      ),
    };
    return FlarkV3HostAccepted(outcome);
  }

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant) {
    final unavailable =
        _unavailable<FlarkV3ViewportPresentationHostPollOutcome>();
    if (unavailable != null) return unavailable;
    _scratch.workGrant.ref
      ..inspectBytes = grant.inspectBytes
      ..copyBytes = grant.copyBytes
      ..transitions = grant.transitions;
    final status = _symbols.pollViewportPresentation(
      _scratch.handle.ref,
      _scratch.workGrant.ref,
      _scratch.viewportPresentationPollReceipt,
    );
    final receipt = _scratch.viewportPresentationPollReceipt.ref;
    final rejected = _rejection(
      operation: 'pollViewportPresentation',
      status: status,
      reason: receipt.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    final outcome = switch (receipt.outcome) {
      0 => const FlarkV3ViewportPresentationHostPollPending(),
      1 => FlarkV3ViewportPresentationHostPacketCredit(
        offerId: _readOfferId(receipt.offerId),
        nextFrameOrdinal: receipt.nextFrameOrdinal,
      ),
      2 => FlarkV3ViewportPresentationHostCommitted(
        _readViewportPresentationAck(receipt.ack),
      ),
      3 => FlarkV3ViewportPresentationHostAbortComplete(
        _readOfferId(receipt.offerId),
      ),
      4 => const FlarkV3ViewportPresentationHostClosed(),
      _ => throw FlarkV3NativeHostException(
        operation: 'pollViewportPresentationOutcome',
        status: 0x0111,
        detail: 'unknown outcome ${receipt.outcome}',
      ),
    };
    return FlarkV3HostAccepted(outcome);
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeAck(_scratch.ack.ref, ack);
    return _unitCall(
      'acknowledgeDelivery',
      () => _symbols.acknowledgeDelivery(
        _scratch.handle.ref,
        _scratch.ack,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeInlineSidecarDelivery(
    FlarkV3InlineSidecarAck ack,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeInlineSidecarAck(_scratch.inlineSidecarAck.ref, ack);
    return _unitCall(
      'acknowledgeInlineSidecarDelivery',
      () => _symbols.acknowledgeInlineSidecarDelivery(
        _scratch.handle.ref,
        _scratch.inlineSidecarAck,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeViewportPresentationAck(_scratch.viewportPresentationAck.ref, ack);
    return _unitCall(
      'acknowledgeViewportPresentationDelivery',
      () => _symbols.acknowledgeViewportPresentationDelivery(
        _scratch.handle.ref,
        _scratch.viewportPresentationAck,
        _scratch.callReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) {
    final unavailable = _unavailable<FlarkV3HostStoreQueryOutcome>();
    if (unavailable != null) return unavailable;
    final source = _currentSource;
    if (source == null || source != query.sourceVersion) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Query does not bind the native host source authority.',
        ),
      );
    }
    if (query.budget.maxEncodedBytes > flarkV3NativeHostMaximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Query copy exceeds the native host scratch bound.',
        ),
      );
    }

    _writePointQuery(_scratch.pointQuery.ref, query);
    final status = _symbols.queryStructural(
      _scratch.handle.ref,
      _scratch.pointQuery,
      _scratch.bulk,
      query.budget.maxEncodedBytes,
      _scratch.pointQueryReceipt,
    );
    final nativeReceipt = _scratch.pointQueryReceipt.ref;
    final rejected = _rejection(
      operation: 'queryStructural',
      status: status,
      reason: nativeReceipt.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    return FlarkV3HostAccepted(
      _decodePointQueryOutcome(
        query: query,
        native: nativeReceipt,
        output: _scratch.bulk,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreBlockRangeQueryOutcome>
  queryStructuralRange(FlarkV3HostBlockRangeQuery query) {
    final unavailable = _unavailable<FlarkV3HostStoreBlockRangeQueryOutcome>();
    if (unavailable != null) return unavailable;
    final source = _currentSource;
    if (source == null || source != query.sourceVersion) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Range query does not bind the native host source authority.',
        ),
      );
    }
    if (query.budget.maxEncodedBytes > flarkV3NativeHostMaximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Range-query copy exceeds the native host scratch bound.',
        ),
      );
    }

    _writeBlockRangeQuery(_scratch.blockRangeQuery.ref, query);
    final status = _symbols.queryStructuralRange(
      _scratch.handle.ref,
      _scratch.blockRangeQuery,
      _scratch.bulk,
      query.budget.maxEncodedBytes,
      _scratch.blockRangeQueryReceipt,
    );
    final nativeReceipt = _scratch.blockRangeQueryReceipt.ref;
    final rejected = _rejection(
      operation: 'queryStructuralRange',
      status: status,
      reason: nativeReceipt.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    return FlarkV3HostAccepted(
      _decodeBlockRangeQueryOutcome(
        query: query,
        native: nativeReceipt,
        output: _scratch.bulk,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStructuralOrdinalWindowOutcome>
  queryStructuralOrdinalWindow(FlarkV3HostStructuralOrdinalWindowQuery query) {
    final unavailable =
        _unavailable<FlarkV3HostStructuralOrdinalWindowOutcome>();
    if (unavailable != null) return unavailable;
    final source = _currentSource;
    if (source == null || source != query.sourceVersion) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Ordinal query does not bind the native host source authority.',
        ),
      );
    }

    _writeStructuralOrdinalWindowQuery(
      _scratch.structuralOrdinalWindowQuery.ref,
      query,
    );
    final status = _symbols.queryStructuralOrdinalWindow(
      _scratch.handle.ref,
      _scratch.structuralOrdinalWindowQuery,
      _scratch.structuralOrdinalWindowReceipt,
    );
    final nativeReceipt = _scratch.structuralOrdinalWindowReceipt.ref;
    _validateStructuralOrdinalWindowReserved(nativeReceipt);
    final rejected = _rejection(
      operation: 'queryStructuralOrdinalWindow',
      status: status,
      reason: nativeReceipt.rejectionReason,
    );
    if (rejected != null) {
      if (!_structuralOrdinalWindowRejectedBodyIsCanonical(nativeReceipt)) {
        throw const FlarkV3NativeHostException(
          operation: 'queryStructuralOrdinalWindowReceipt',
          status: 0x0111,
          detail: 'native host returned a noncanonical rejected ordinal body',
        );
      }
      return FlarkV3HostRejected(rejected);
    }
    return FlarkV3HostAccepted(
      _decodeStructuralOrdinalWindowOutcome(
        query: query,
        native: nativeReceipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> queryInlineSidecar(
    FlarkV3InlineSidecarQuery query,
  ) {
    final unavailable = _unavailable<FlarkV3InlineSidecarQueryOutcome>();
    if (unavailable != null) return unavailable;
    if (query.maximumEncodedBytes >
        flarkV3NativeHostInlineSidecarMaximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Inline-sidecar query exceeds the native host scratch bound.',
        ),
      );
    }
    _writeInlineSidecarQuery(_scratch.inlineSidecarQuery.ref, query);
    final status = _symbols.queryInlineSidecar(
      _scratch.handle.ref,
      _scratch.inlineSidecarQuery,
      _scratch.bulk,
      query.maximumEncodedBytes,
      _scratch.inlineSidecarQueryReceipt,
    );
    final receipt = _scratch.inlineSidecarQueryReceipt.ref;
    final rejected = _rejection(
      operation: 'queryInlineSidecar',
      status: status,
      reason: receipt.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    return FlarkV3HostAccepted(
      _decodeInlineSidecarQueryOutcome(
        query: query,
        native: receipt,
        output: _scratch.bulk,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query) {
    final unavailable = _unavailable<FlarkV3ViewportPresentationQueryOutcome>();
    if (unavailable != null) return unavailable;
    if (query.maximumEncodedBytes >
        flarkV3NativeHostViewportMaximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'VPB1 query copy exceeds the native host scratch bound.',
        ),
      );
    }
    _writeViewportPresentationQuery(
      _scratch.viewportPresentationQuery.ref,
      query,
    );
    final status = _symbols.queryViewportPresentation(
      _scratch.handle.ref,
      _scratch.viewportPresentationQuery,
      _scratch.bulk,
      query.maximumEncodedBytes,
      _scratch.viewportPresentationQueryReceipt,
    );
    final receipt = _scratch.viewportPresentationQueryReceipt.ref;
    final rejected = _rejection(
      operation: 'queryViewportPresentation',
      status: status,
      reason: receipt.rejectionReason,
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    for (var index = 0; index < 4; index += 1) {
      if (receipt.reserved[index] != 0) {
        throw const FlarkV3NativeHostException(
          operation: 'queryViewportPresentationReceipt',
          status: 0x0111,
          detail: 'native host returned nonzero reserved VPB1 query fields',
        );
      }
    }
    final outcome = switch (receipt.outcome) {
      0 when receipt.encodedBytes == 0 && receipt.entryCount == 0 =>
        const FlarkV3ViewportPresentationQueryUnavailable(),
      1
          when receipt.encodedBytes > 0 &&
              receipt.encodedBytes <= query.maximumEncodedBytes &&
              receipt.entryCount == query.ack.envelope.orderedLeafCount =>
        FlarkV3ViewportPresentationQueryAvailable(
          FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
            ack: query.ack,
            encodedPage: Uint8List.fromList(
              _scratch.bulk.asTypedList(receipt.encodedBytes),
            ),
          ),
        ),
      _ => throw FlarkV3NativeHostException(
        operation: 'queryViewportPresentationReceipt',
        status: 0x0111,
        detail:
            'native host returned invalid VPB1 outcome ${receipt.outcome}, '
            'bytes ${receipt.encodedBytes}, entries ${receipt.entryCount}',
      ),
    };
    return FlarkV3HostAccepted(outcome);
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    if (_released) return const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);
    return _unitCall(
      'close',
      () => _symbols.close(_scratch.handle.ref, _scratch.callReceipt),
    );
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> _unitCall(
    String operation,
    int Function() invoke,
  ) {
    final status = invoke();
    final rejected = _rejection(
      operation: operation,
      status: status,
      reason: _scratch.callReceipt.ref.rejectionReason,
    );
    return rejected == null
        ? const FlarkV3HostAccepted(FlarkV3HostUnit.accepted)
        : FlarkV3HostRejected(rejected);
  }

  FlarkV3HostRejection? _rejection({
    required String operation,
    required int status,
    required int reason,
  }) {
    if (reason == 0) {
      _requireNativeOk(operation, status);
      return null;
    }
    if (status == _statusOk) {
      throw FlarkV3NativeHostException(
        operation: operation,
        status: status,
        detail: 'successful status carried rejection $reason',
      );
    }
    final mapped = switch (reason) {
      1 => FlarkV3HostRejectReason.invalid,
      2 => FlarkV3HostRejectReason.backpressure,
      3 => FlarkV3HostRejectReason.staleSource,
      4 => FlarkV3HostRejectReason.exactSourceMismatch,
      // M1.1 has no delta/session-snapshot negotiation. Native NotReady is an
      // invariant failure and must not be aliased to wire reason 5.
      5 => throw FlarkV3NativeHostException(
        operation: operation,
        status: status,
        detail: 'native host was unexpectedly not ready',
      ),
      6 => FlarkV3HostRejectReason.baseMismatch,
      7 => FlarkV3HostRejectReason.wrongOffer,
      8 => FlarkV3HostRejectReason.corruptPublication,
      9 => FlarkV3HostRejectReason.queryBoundExceeded,
      10 => FlarkV3HostRejectReason.foregroundBoundExceeded,
      11 => FlarkV3HostRejectReason.superseded,
      12 => FlarkV3HostRejectReason.closed,
      _ => throw FlarkV3NativeHostException(
        operation: operation,
        status: status,
        detail: 'unknown rejection $reason',
      ),
    };
    return FlarkV3HostRejection(mapped, _rejectionMessage(mapped));
  }

  FlarkV3HostRejected<T>? _unavailable<T>() => _released
      ? const FlarkV3HostRejected(
          FlarkV3HostRejection(
            FlarkV3HostRejectReason.closed,
            'The native host has been reclaimed.',
          ),
        )
      : null;

  void _releaseNormally() {
    if (_released) return;
    final token = _finalizerToken;
    _emergencyFinalizer.detach(this);
    final removeStatus = _symbols.remove(_scratch.handle.ref);
    final reclamationStatus = removeStatus == _statusOk
        ? _statusOk
        : _symbols.emergencyDestroy(_scratch.handle.ref);
    if (reclamationStatus != _statusOk) {
      _emergencyFinalizer.attach(this, token, detach: this);
      throw FlarkV3NativeHostException(
        operation: 'remove',
        status: removeStatus,
        detail:
            'emergency fallback status='
            '0x${reclamationStatus.toRadixString(16)}',
      );
    }

    final tokenStatus = _symbols.finalizerTokenRelease(token);
    if (tokenStatus != _statusOk) {
      // The generation-safe native callback tolerates a handle that has
      // already been removed. Reattach so the still-live token itself cannot
      // leak if its explicit release failed.
      _emergencyFinalizer.attach(this, token, detach: this);
      throw FlarkV3NativeHostException(
        operation: 'finalizerTokenRelease',
        status: tokenStatus,
      );
    }
    _finalizerToken = nullptr;
    _released = true;
    _scratch.free();
  }
}

String _rejectionMessage(FlarkV3HostRejectReason reason) => switch (reason) {
  FlarkV3HostRejectReason.invalid => 'The native host rejected invalid state.',
  FlarkV3HostRejectReason.backpressure =>
    'The native host still owns the prior bounded operation.',
  FlarkV3HostRejectReason.staleSource =>
    'The publication targets an older exact source.',
  FlarkV3HostRejectReason.exactSourceMismatch =>
    'The publication crossed exact source authority.',
  FlarkV3HostRejectReason.sessionSnapshotRequired =>
    'The host requires a full session snapshot.',
  FlarkV3HostRejectReason.baseMismatch =>
    'The publication base or installed revision changed.',
  FlarkV3HostRejectReason.wrongOffer =>
    'The operation belongs to another publication offer.',
  FlarkV3HostRejectReason.corruptPublication =>
    'The publication packet or one of its frames is corrupt.',
  FlarkV3HostRejectReason.queryBoundExceeded =>
    'The structural query exceeded its declared bound.',
  FlarkV3HostRejectReason.foregroundBoundExceeded =>
    'The operation exceeded the caller-isolate work envelope.',
  FlarkV3HostRejectReason.superseded =>
    'A newer exact source superseded the operation.',
  FlarkV3HostRejectReason.closed => 'The native host is closing or closed.',
};

void _requireNativeOk(String operation, int status) {
  if (status != _statusOk) {
    throw FlarkV3NativeHostException(operation: operation, status: status);
  }
}

void _writeId(Array<Uint32> output, FlarkV3ProtocolId128 value) {
  output[0] = value.word0;
  output[1] = value.word1;
  output[2] = value.word2;
  output[3] = value.word3;
}

void _writeSourceVersion(
  _NativeHostSourceVersion output,
  FlarkV3SourceVersion source,
) {
  _writeId(output.documentSession, source.documentSession);
  output
    ..revision = source.revision
    ..utf8Length = source.metric.bytes
    ..utf16Length = source.metric.utf16;
  output.contentHash128[0] = source.contentHash.word0;
  output.contentHash128[1] = source.contentHash.word1;
  output.contentHash128[2] = source.contentHash.word2;
  output.contentHash128[3] = source.contentHash.word3;
}

void _writeOffer(_NativeHostOfferBegin output, FlarkV3HostOfferBegin begin) {
  _writeId(output.offerId, begin.offerId);
  _writeId(output.publicationSession, begin.publicationSession);
  output.targetHostRevision = begin.targetHostRevision.value;
  _writeSourceVersion(output.sourceVersion, begin.sourceVersion);
  output.sourceRoot[0] = begin.sourceRoot.highWord;
  output.sourceRoot[1] = begin.sourceRoot.lowWord;
  output
    ..parseGeneration = begin.parseGeneration
    ..grammarRevision = begin.grammarRevision
    ..syntaxProfile = begin.syntaxProfile.value
    ..authorityMask = begin.authorityMask.bits
    ..transferredRecordCount = begin.transferredRecordCount
    ..targetRecordCount = begin.targetRecordCount
    ..maximumFrameCount = begin.limits.maximumFrameCount
    ..maximumEncodedFrameBytes = begin.limits.maximumEncodedFrameBytes
    ..maximumPacketBytes = begin.limits.maximumPacketBytes
    ..maximumFrameBytes = begin.limits.maximumFrameBytes
    ..maximumProgramChildren = begin.limits.maximumProgramChildren;
}

void _writeCommit(
  _NativeHostCommitRequest output,
  FlarkV3HostCommitRequest request,
) {
  _writeId(output.offerId, request.offerId);
  output
    ..actualFrameCount = request.actualFrameCount
    ..actualEncodedFrameBytes = request.actualEncodedFrameBytes;
  _writeId(output.rollingTransportDigest, request.rollingTransportDigest);
  _writeId(output.canonicalStreamDigest, request.canonicalStreamDigest);
}

void _writeAck(_NativeHostStructuralAck output, FlarkV3StructuralAck ack) {
  _writeId(output.publicationSession, ack.publicationSession);
  output.hostRevision = ack.hostRevision.value;
  _writeSourceVersion(output.sourceVersion, ack.sourceVersion);
  output.sourceRoot[0] = ack.sourceRoot.highWord;
  output.sourceRoot[1] = ack.sourceRoot.lowWord;
  output
    ..parseGeneration = ack.parseGeneration
    ..grammarRevision = ack.grammarRevision
    ..syntaxProfile = ack.syntaxProfile.value
    ..authorityMask = ack.authorityMask.bits
    ..recordCount = ack.recordCount;
  _writeId(output.sequenceDigest, ack.sequenceDigest);
  _writeId(output.manifestDigest, ack.manifestDigest);
}

void _writeU64(_NativeHostU64 output, FlarkV3ProtocolU64 value) {
  output
    ..lowWord = value.lowWord
    ..highWord = value.highWord;
}

void _writeDigest256(Array<Uint32> output, FlarkV3ProtocolDigest256 digest) {
  output[0] = digest.word0;
  output[1] = digest.word1;
  output[2] = digest.word2;
  output[3] = digest.word3;
  output[4] = digest.word4;
  output[5] = digest.word5;
  output[6] = digest.word6;
  output[7] = digest.word7;
}

void _writeInlineSidecarBinding(
  _NativeHostInlineSidecarBinding output,
  FlarkV3HotInlineSidecarBinding binding,
) {
  _writeU64(
    output.parserProfile,
    FlarkV3ProtocolU64.fromU32(binding.parserProfile.value),
  );
  _writeU64(output.refinementGeneration, binding.refinementGeneration);
  _writeU64(output.blockOrdinal, binding.blockOrdinal);
  output
    ..physicalStartUtf8 = binding.physicalStartUtf8
    ..physicalEndUtf8 = binding.physicalEndUtf8
    ..visibleStartUtf8 = binding.visibleStartUtf8
    ..visibleEndUtf8 = binding.visibleEndUtf8
    ..physicalStartUtf16 = binding.physicalStartUtf16
    ..physicalEndUtf16 = binding.physicalEndUtf16
    ..visibleStartUtf16 = binding.visibleStartUtf16
    ..visibleEndUtf16 = binding.visibleEndUtf16;
}

void _writeInlineSidecarBegin(
  _NativeHostInlineSidecarBegin output,
  FlarkV3HotInlineSidecarOfferBegin begin,
) {
  output
    ..schema = begin.schema
    ..mode = 1;
  _writeId(output.offerId, begin.offerId);
  _writeId(output.publicationSession, begin.publicationSession);
  _writeAck(output.baseAck, begin.baseAck);
  _writeInlineSidecarBinding(output.binding, begin.binding);
  output
    ..hio1EncodedBytes = begin.envelope.hio1EncodedBytes
    ..ipr2DescriptorBytes = begin.envelope.ipr2DescriptorBytes
    ..transferredNodeCount = begin.envelope.transferredNodeCount;
  switch (begin.envelope.disposition) {
    case FlarkV3HotInlineSidecarAuthoritative(
      :final logicalPageCount,
      :final factCount,
      :final storagePageCount,
      :final linkValueEntryCount,
      :final linkValueStoragePageCount,
      :final linkValueEncodedBytes,
      :final orderedCommitment256,
    ):
      output.sidecarDisposition
        ..disposition = 1
        ..reason = 0;
      _writeU64(output.sidecarDisposition.logicalPageCount, logicalPageCount);
      _writeU64(output.sidecarDisposition.factCount, factCount);
      _writeU64(output.sidecarDisposition.storagePageCount, storagePageCount);
      _writeU64(
        output.sidecarDisposition.linkValueStoragePageCount,
        linkValueStoragePageCount,
      );
      output.sidecarDisposition
        ..linkValueEntryCount = linkValueEntryCount
        ..linkValueEncodedBytes = linkValueEncodedBytes;
      _writeDigest256(
        output.sidecarDisposition.commitment256,
        orderedCommitment256,
      );
    case FlarkV3HotInlineSidecarUnsupported(
      :final reason,
      :final metadataCommitment256,
    ):
      output.sidecarDisposition
        ..disposition = 2
        ..reason = reason;
      _writeU64(
        output.sidecarDisposition.logicalPageCount,
        FlarkV3ProtocolU64.zero,
      );
      _writeU64(output.sidecarDisposition.factCount, FlarkV3ProtocolU64.zero);
      _writeU64(
        output.sidecarDisposition.storagePageCount,
        FlarkV3ProtocolU64.zero,
      );
      _writeU64(
        output.sidecarDisposition.linkValueStoragePageCount,
        FlarkV3ProtocolU64.zero,
      );
      output.sidecarDisposition
        ..linkValueEntryCount = 0
        ..linkValueEncodedBytes = 0;
      _writeDigest256(
        output.sidecarDisposition.commitment256,
        metadataCommitment256,
      );
  }
  _writeDigest256(
    output.hio1EnvelopeDigest256,
    begin.envelope.hio1EnvelopeDigest256,
  );
  output
    ..maximumFrameCount = begin.limits.maximumFrameCount
    ..maximumEncodedFrameBytes = begin.limits.maximumEncodedFrameBytes
    ..maximumPacketBytes = begin.limits.maximumPacketBytes
    ..maximumFrameBytes = begin.limits.maximumFrameBytes
    ..maximumProgramChildren = begin.limits.maximumProgramChildren;
}

void _writeInlineSidecarCommit(
  _NativeHostInlineSidecarCommitRequest output,
  FlarkV3HotInlineSidecarCommitRequest request,
) {
  _writeId(output.offerId, request.offerId);
  output
    ..actualFrameCount = request.actualFrameCount
    ..actualEncodedFrameBytes = request.actualEncodedFrameBytes;
  _writeId(output.rollingTransportDigest, request.rollingTransportDigest);
  _writeId(output.rootStreamDigest, request.rootStreamDigest);
}

void _writeInlineSidecarAck(
  _NativeHostInlineSidecarAck output,
  FlarkV3InlineSidecarAck ack,
) {
  _writeId(output.publicationSession, ack.publicationSession);
  _writeAck(output.baseAck, ack.baseAck);
  _writeU64(output.refinementGeneration, ack.refinementGeneration);
  _writeU64(output.blockOrdinal, ack.blockOrdinal);
  output
    ..transferredNodeCount = ack.transferredNodeCount
    ..disposition = ack.disposition.index + 1;
  _writeDigest256(output.hio1EnvelopeDigest256, ack.hio1EnvelopeDigest256);
  _writeId(output.rootStreamDigest, ack.rootStreamDigest);
}

void _writeInlineSidecarQuery(
  _NativeHostInlineSidecarQuery output,
  FlarkV3InlineSidecarQuery query,
) {
  output
    ..schema = _inlineSidecarQuerySchema
    ..structSize = sizeOf<_NativeHostInlineSidecarQuery>()
    ..maximumEncodedBytes = query.maximumEncodedBytes;
  _writeInlineSidecarBinding(output.binding, query.binding);
  for (var index = 0; index < 3; index += 1) {
    output.reserved[index] = 0;
  }
}

void _writeViewportPresentationRange(
  _NativeHostViewportPresentationMetricRange output,
  FlarkV3ViewportPresentationMetricRange range,
) {
  output
    ..startUtf8 = range.startUtf8
    ..startUtf16 = range.startUtf16
    ..endUtf8 = range.endUtf8
    ..endUtf16 = range.endUtf16;
}

void _writeViewportPresentationVisitStart(
  _NativeHostViewportPresentationVisitStart output,
  FlarkV3ViewportPresentationVisitStart start,
) {
  _writeU64(output.blockOrdinal, start.blockOrdinal);
  output
    ..utf8Offset = start.utf8Offset
    ..utf16Offset = start.utf16Offset;
}

void _writeViewportPresentationBinding(
  _NativeHostViewportPresentationBinding output,
  FlarkV3ViewportPresentationBinding binding,
) {
  output.viewportGeneration = binding.viewportGeneration;
  _writeViewportPresentationRange(
    output.requestedRange,
    binding.requestedRange,
  );
  _writeViewportPresentationRange(output.coveredRange, binding.coveredRange);
  _writeViewportPresentationVisitStart(output.start, binding.start);
  _writeViewportPresentationVisitStart(output.next, binding.next);
  output.complete = binding.complete ? 1 : 0;
}

void _writeViewportPresentationEnvelope(
  _NativeHostViewportPresentationEnvelope output,
  FlarkV3ViewportPresentationEnvelopeMetrics envelope,
) {
  output
    ..visitedStructuralEntries = envelope.visitedStructuralEntries
    ..visitedStoragePages = envelope.visitedStoragePages
    ..orderedLeafCount = envelope.orderedLeafCount
    ..inlineSourceBytes = envelope.inlineSourceBytes
    ..factCount = envelope.factCount
    ..transferredNodeCount = envelope.transferredNodeCount
    ..parserTransitions = envelope.parserTransitions;
  _writeDigest256(
    output.aggregateEnvelopeDigest256,
    envelope.aggregateEnvelopeDigest256,
  );
}

void _writeViewportPresentationQueryLimits(
  _NativeHostViewportPresentationQueryLimits output,
  FlarkV3ViewportPresentationQueryLimits limits,
) {
  output
    ..maximumStructuralEntries = limits.maximumStructuralEntries
    ..maximumStoragePages = limits.maximumStoragePages
    ..maximumInlineLeaves = limits.maximumInlineLeaves
    ..maximumInlineLeafSourceBytes = limits.maximumInlineLeafSourceBytes
    ..maximumInlineSourceBytes = limits.maximumInlineSourceBytes
    ..maximumFactRecords = limits.maximumFactRecords
    ..maximumEncodedFrameBytes = limits.maximumEncodedFrameBytes
    ..maximumParserTransitions = limits.maximumParserTransitions;
}

void _writeViewportPresentationOfferLimits(
  _NativeHostViewportPresentationOfferLimits output,
  FlarkV3ViewportPresentationOfferLimits limits,
) {
  output
    ..maximumFrameCount = limits.maximumFrameCount
    ..maximumEncodedFrameBytes = limits.maximumEncodedFrameBytes
    ..maximumPacketBytes = limits.maximumPacketBytes
    ..maximumFrameBytes = limits.maximumFrameBytes
    ..maximumProgramChildren = limits.maximumProgramChildren;
}

void _writeViewportPresentationBegin(
  _NativeHostViewportPresentationBegin output,
  FlarkV3ViewportPresentationOfferBegin begin,
) {
  output
    ..schema = begin.schema
    ..mode = 1;
  _writeId(output.offerId, begin.offerId);
  _writeId(output.publicationSession, begin.publicationSession);
  _writeAck(output.baseAck, begin.baseAck);
  _writeViewportPresentationBinding(output.binding, begin.binding);
  _writeViewportPresentationEnvelope(output.envelope, begin.envelope);
  _writeViewportPresentationQueryLimits(output.queryLimits, begin.queryLimits);
  _writeViewportPresentationOfferLimits(output.limits, begin.limits);
}

void _writeViewportPresentationCommit(
  _NativeHostViewportPresentationCommitRequest output,
  FlarkV3ViewportPresentationCommitRequest request,
) {
  _writeId(output.offerId, request.offerId);
  output
    ..actualFrameCount = request.actualFrameCount
    ..actualEncodedFrameBytes = request.actualEncodedFrameBytes;
  _writeId(output.rollingTransportDigest, request.rollingTransportDigest);
  _writeId(output.aggregateRootStreamDigest, request.aggregateRootStreamDigest);
}

void _writeViewportPresentationAck(
  _NativeHostViewportPresentationAck output,
  FlarkV3ViewportPresentationAck ack,
) {
  _writeId(output.publicationSession, ack.publicationSession);
  _writeAck(output.baseAck, ack.baseAck);
  _writeViewportPresentationBinding(output.binding, ack.binding);
  _writeViewportPresentationEnvelope(output.envelope, ack.envelope);
  output
    ..actualFrameCount = ack.actualFrameCount
    ..actualEncodedFrameBytes = ack.actualEncodedFrameBytes;
  _writeId(output.aggregateRootStreamDigest, ack.aggregateRootStreamDigest);
}

void _writeViewportPresentationQuery(
  _NativeHostViewportPresentationQuery output,
  FlarkV3ViewportPresentationQuery query,
) {
  output
    ..schema = 1
    ..structSize = flarkV3NativeHostViewportPresentationQueryBytes
    ..maximumEncodedBytes = query.maximumEncodedBytes;
  _writeViewportPresentationAck(output.ack, query.ack);
  for (var index = 0; index < 3; index += 1) {
    output.reserved[index] = 0;
  }
}

FlarkV3InlineSidecarQueryOutcome _decodeInlineSidecarQueryOutcome({
  required FlarkV3InlineSidecarQuery query,
  required _NativeHostInlineSidecarQueryReceipt native,
  required Pointer<Uint8> output,
}) {
  if (native.encodedBytes > query.maximumEncodedBytes) {
    throw const FlarkV3NativeHostException(
      operation: 'queryInlineSidecarReceipt',
      status: 0x0111,
      detail: 'native host exceeded the sidecar query copy bound',
    );
  }
  final factBytes =
      native.factCount *
      FlarkV3InlineSidecarQueryAuthoritative.inlineFactRecordBytes;
  if (native.outcome == 1 &&
      factBytes + native.valueEncodedBytes != native.encodedBytes) {
    throw const FlarkV3NativeHostException(
      operation: 'queryInlineSidecarReceipt',
      status: 0x0111,
      detail: 'native host sidecar receipt does not frame facts and values',
    );
  }
  final encoded = Uint8List.fromList(output.asTypedList(native.encodedBytes));
  return switch (native.outcome) {
    0
        when native.reason == 0 &&
            native.encodedBytes == 0 &&
            native.factCount == 0 &&
            native.treeNodesVisited == 0 &&
            native.valueEntryCount == 0 &&
            native.valueEncodedBytes == 0 =>
      const FlarkV3InlineSidecarQueryUnavailable(),
    1 when native.reason == 0 => FlarkV3InlineSidecarQueryAuthoritative(
      factCount: native.factCount,
      valueEntryCount: native.valueEntryCount,
      treeNodesVisited: native.treeNodesVisited,
      encodedFacts: Uint8List.sublistView(encoded, 0, factBytes),
      encodedValues: Uint8List.sublistView(encoded, factBytes),
    ),
    2
        when native.reason != 0 &&
            native.factCount == 0 &&
            native.treeNodesVisited == 0 &&
            native.valueEntryCount == 0 &&
            native.valueEncodedBytes == 0 =>
      FlarkV3InlineSidecarQueryUnsupported(
        reason: native.reason,
        metadata: encoded,
      ),
    _ => throw FlarkV3NativeHostException(
      operation: 'queryInlineSidecarReceipt',
      status: 0x0111,
      detail: 'native host returned invalid sidecar outcome ${native.outcome}',
    ),
  };
}

void _writePointQuery(
  _NativeHostPointQuery output,
  FlarkV3HostPointQuery query,
) {
  output
    ..schema = _pointQuerySchema
    ..structSize = sizeOf<_NativeHostPointQuery>();
  _writeSourceVersion(output.sourceVersion, query.sourceVersion);
  output
    ..positionUtf8 = query.position.bytes
    ..positionUtf16 = query.position.utf16
    ..affinity = switch (query.affinity) {
      FlarkV3MetricAffinity.upstream => 0,
      FlarkV3MetricAffinity.downstream => 1,
    }
    ..maximumEncodedBytes = query.budget.maxEncodedBytes
    ..maximumOpenDepth = query.budget.maxOpenDepth
    ..maximumLeafCount = query.budget.maxLeafCount
    ..maximumTreeNodesVisited = query.budget.maxTreeNodesVisited;
  for (var index = 0; index < 4; index += 1) {
    output.reserved[index] = 0;
  }
}

FlarkV3HostStoreQueryOutcome _decodePointQueryOutcome({
  required FlarkV3HostPointQuery query,
  required _NativeHostPointQueryReceipt native,
  required Pointer<Uint8> output,
}) {
  for (var index = 0; index < 5; index += 1) {
    if (native.reserved[index] != 0) {
      throw const FlarkV3NativeHostException(
        operation: 'queryStructuralReceipt',
        status: 0x0111,
        detail: 'native host returned nonzero reserved query fields',
      );
    }
  }
  final source = _readSourceVersion(native.sourceVersion);
  final rangeStart = FlarkV3SourceMetric(
    bytes: native.rangeStartUtf8,
    utf16: native.rangeStartUtf16,
  );
  final rangeEnd = FlarkV3SourceMetric(
    bytes: native.rangeEndUtf8,
    utf16: native.rangeEndUtf16,
  );
  if (!rangeEnd.contains(rangeStart)) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralReceipt',
      status: 0x0111,
      detail: 'native host returned an inverted query range',
    );
  }
  final range = FlarkV3MetricRange(start: rangeStart, end: rangeEnd);
  final point = query.position;
  final rangeContainsPoint =
      point.bytes >= range.start.bytes &&
      point.utf16 >= range.start.utf16 &&
      range.end.contains(point);
  if (source != query.sourceVersion ||
      !source.metric.contains(range.end) ||
      !rangeContainsPoint ||
      native.encodedBytes > query.budget.maxEncodedBytes ||
      native.leafCount > query.budget.maxLeafCount ||
      native.openDepth > query.budget.maxOpenDepth ||
      native.treeNodesVisited > query.budget.maxTreeNodesVisited) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralReceipt',
      status: 0x0111,
      detail: 'native host returned an out-of-authority query receipt',
    );
  }
  final receipt = FlarkV3HostViewportReceipt(
    encodedBytes: native.encodedBytes,
    leafCount: native.leafCount,
    openDepth: native.openDepth,
    treeNodesVisited: native.treeNodesVisited,
    summaryNodesSkipped: native.summaryNodesSkipped,
  );
  return switch (native.outcome) {
    1 when native.gapReason == 0 && native.encodedBytes > 0 =>
      FlarkV3HostStoreStructuralQuery(
        FlarkV3HostStructuralViewport.owned(
          sourceVersion: source,
          range: range,
          encoded: Uint8List.fromList(output.asTypedList(native.encodedBytes)),
          receipt: receipt,
        ),
      ),
    2 when native.encodedBytes == 0 => FlarkV3HostStoreSourceGapQuery(
      FlarkV3HostLocalSourceGap(
        sourceVersion: source,
        range: range,
        reason: _readPointQueryGapReason(native.gapReason),
        receipt: receipt,
      ),
    ),
    _ => throw FlarkV3NativeHostException(
      operation: 'queryStructuralReceipt',
      status: 0x0111,
      detail:
          'native host returned invalid query outcome ${native.outcome} '
          'and gap ${native.gapReason}',
    ),
  };
}

void _writeBlockRangeQuery(
  _NativeHostBlockRangeQuery output,
  FlarkV3HostBlockRangeQuery query,
) {
  output
    ..schema = _blockRangeQuerySchema
    ..structSize = sizeOf<_NativeHostBlockRangeQuery>();
  _writeSourceVersion(output.sourceVersion, query.sourceVersion);
  final requested = query.requestedRange;
  final budget = query.budget;
  output
    ..requestedStartUtf8 = requested.start.bytes
    ..requestedStartUtf16 = requested.start.utf16
    ..requestedEndUtf8 = requested.end.bytes
    ..requestedEndUtf16 = requested.end.utf16
    ..maximumEncodedBytes = budget.maxEncodedBytes
    ..maximumBlockCount = budget.maxBlockCount
    ..maximumStoragePagesVisited = budget.maxStoragePagesVisited
    ..maximumOpenDepth = budget.maxOpenDepth
    ..maximumTreeNodesVisited = budget.maxTreeNodesVisited;
  final continuation = query.continuation?.copyEncoded();
  output.continuationLength = continuation?.length ?? 0;
  for (
    var index = 0;
    index < FlarkV3HostBlockRangeContinuation.encodedBytes;
    index += 1
  ) {
    output.continuation[index] = continuation?[index] ?? 0;
  }
  for (var index = 0; index < 4; index += 1) {
    output.reserved[index] = 0;
  }
}

void _writeStructuralOrdinalWindowQuery(
  _NativeHostStructuralOrdinalWindowQuery output,
  FlarkV3HostStructuralOrdinalWindowQuery query,
) {
  output
    ..schema = _structuralOrdinalWindowQuerySchema
    ..structSize = sizeOf<_NativeHostStructuralOrdinalWindowQuery>();
  _writeSourceVersion(output.sourceVersion, query.sourceVersion);
  _writeU64(output.startBlockOrdinal, query.startBlockOrdinal);
  final budget = query.budget;
  output
    ..maximumEntries = budget.maximumEntries
    ..maximumStoragePagesVisited = budget.maximumStoragePagesVisited
    ..maximumTreeNodesVisited = budget.maximumTreeNodesVisited
    ..maximumPackedEntriesInspected = budget.maximumPackedEntriesInspected;
  for (var index = 0; index < 4; index += 1) {
    output.reserved[index] = 0;
  }
}

FlarkV3HostStructuralOrdinalWindowOutcome
_decodeStructuralOrdinalWindowOutcome({
  required FlarkV3HostStructuralOrdinalWindowQuery query,
  required _NativeHostStructuralOrdinalWindowReceipt native,
}) {
  final source = _readSourceVersion(native.sourceVersion);
  final total = _readU64(native.totalBlockCount);
  final start = _readU64(native.startBlockOrdinal);
  final next = _readU64(native.nextBlockOrdinal);
  final work = FlarkV3HostStructuralOrdinalWindowWorkReceipt(
    storagePagesVisited: native.storagePagesVisited,
    treeNodesVisited: native.treeNodesVisited,
    packedEntriesInspected: native.packedEntriesInspected,
    summaryNodesSkipped: native.summaryNodesSkipped,
  );
  final flags = native.flags;
  if ((flags & ~1) != 0 ||
      source != query.sourceVersion ||
      start != query.startBlockOrdinal ||
      !work.fits(query.budget)) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralOrdinalWindowReceipt',
      status: 0x0111,
      detail: 'native host returned an out-of-authority ordinal receipt',
    );
  }

  final outcome = switch (native.outcome) {
    1 when native.failureReason == 0 => FlarkV3HostStructuralOrdinalWindow(
      sourceVersion: source,
      totalBlockCount: total,
      startBlockOrdinal: start,
      nextBlockOrdinal: next,
      startSource: FlarkV3SourceMetric(
        bytes: native.startUtf8,
        utf16: native.startUtf16,
      ),
      nextSource: FlarkV3SourceMetric(
        bytes: native.nextUtf8,
        utf16: native.nextUtf16,
      ),
      work: work,
      complete: (flags & 1) != 0,
    ),
    2
        when native.failureReason >= 1 &&
            native.failureReason <= 7 &&
            next.isZero &&
            native.startUtf8 == 0 &&
            native.startUtf16 == 0 &&
            native.nextUtf8 == 0 &&
            native.nextUtf16 == 0 &&
            flags == 0 =>
      FlarkV3HostStructuralOrdinalWindowFailure(
        sourceVersion: source,
        totalBlockCount: total,
        startBlockOrdinal: start,
        reason: _structuralOrdinalWindowFailure(native.failureReason),
        work: work,
      ),
    _ => throw FlarkV3NativeHostException(
      operation: 'queryStructuralOrdinalWindowReceipt',
      status: 0x0111,
      detail:
          'native host returned invalid ordinal outcome ${native.outcome} '
          'and failure ${native.failureReason}',
    ),
  };
  if (!outcome.binds(query)) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralOrdinalWindowReceipt',
      status: 0x0111,
      detail: 'native host returned an invalid ordinal-window witness',
    );
  }
  return outcome;
}

FlarkV3HostStructuralOrdinalWindowFailureReason _structuralOrdinalWindowFailure(
  int value,
) => switch (value) {
  1 => FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable,
  2 => FlarkV3HostStructuralOrdinalWindowFailureReason.entryLimit,
  3 => FlarkV3HostStructuralOrdinalWindowFailureReason.storagePageLimit,
  4 => FlarkV3HostStructuralOrdinalWindowFailureReason.treeNodeLimit,
  5 => FlarkV3HostStructuralOrdinalWindowFailureReason.packedEntryLimit,
  6 => FlarkV3HostStructuralOrdinalWindowFailureReason.ordinalOutOfRange,
  7 => FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable,
  _ => throw FlarkV3NativeHostException(
    operation: 'queryStructuralOrdinalWindowReceipt',
    status: 0x0111,
    detail: 'native host returned unknown ordinal failure $value',
  ),
};

void _validateStructuralOrdinalWindowReserved(
  _NativeHostStructuralOrdinalWindowReceipt native,
) {
  for (var index = 0; index < 4; index += 1) {
    if (native.reserved[index] != 0) {
      throw const FlarkV3NativeHostException(
        operation: 'queryStructuralOrdinalWindowReceipt',
        status: 0x0111,
        detail: 'native host returned nonzero reserved ordinal fields',
      );
    }
  }
}

bool _structuralOrdinalWindowRejectedBodyIsCanonical(
  _NativeHostStructuralOrdinalWindowReceipt native,
) =>
    native.outcome == 0 &&
    native.failureReason == 0 &&
    native.flags == 0 &&
    _readU64(native.totalBlockCount).isZero &&
    _readU64(native.startBlockOrdinal).isZero &&
    _readU64(native.nextBlockOrdinal).isZero &&
    native.startUtf8 == 0 &&
    native.startUtf16 == 0 &&
    native.nextUtf8 == 0 &&
    native.nextUtf16 == 0 &&
    native.storagePagesVisited == 0 &&
    native.treeNodesVisited == 0 &&
    native.packedEntriesInspected == 0 &&
    native.summaryNodesSkipped == 0 &&
    _nativeSourceVersionIsZero(native.sourceVersion);

bool _nativeSourceVersionIsZero(_NativeHostSourceVersion source) {
  for (var index = 0; index < 4; index += 1) {
    if (source.documentSession[index] != 0 ||
        source.contentHash128[index] != 0) {
      return false;
    }
  }
  return source.revision == 0 &&
      source.utf8Length == 0 &&
      source.utf16Length == 0;
}

FlarkV3HostStoreBlockRangeQueryOutcome _decodeBlockRangeQueryOutcome({
  required FlarkV3HostBlockRangeQuery query,
  required _NativeHostBlockRangeQueryReceipt native,
  required Pointer<Uint8> output,
}) {
  for (var index = 0; index < 4; index += 1) {
    if (native.reserved[index] != 0) {
      throw const FlarkV3NativeHostException(
        operation: 'queryStructuralRangeReceipt',
        status: 0x0111,
        detail: 'native host returned nonzero reserved range fields',
      );
    }
  }
  if ((native.flags & ~1) != 0 ||
      (native.continuationLength != 0 &&
          native.continuationLength !=
              FlarkV3HostBlockRangeContinuation.encodedBytes)) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail: 'native host returned invalid range flags or continuation',
    );
  }
  final source = _readSourceVersion(native.sourceVersion);
  final coverageStart = FlarkV3SourceMetric(
    bytes: native.coverageStartUtf8,
    utf16: native.coverageStartUtf16,
  );
  final coverageEnd = FlarkV3SourceMetric(
    bytes: native.coverageEndUtf8,
    utf16: native.coverageEndUtf16,
  );
  if (!coverageEnd.contains(coverageStart)) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail: 'native host returned an inverted range coverage',
    );
  }
  final coverage = FlarkV3MetricRange(start: coverageStart, end: coverageEnd);
  final budget = query.budget;
  final complete = (native.flags & 1) != 0;
  final continuationBytes = Uint8List.fromList(<int>[
    for (
      var index = 0;
      index < FlarkV3HostBlockRangeContinuation.encodedBytes;
      index += 1
    )
      native.continuation[index],
  ]);
  if (source != query.sourceVersion ||
      !source.metric.contains(coverage.end) ||
      native.encodedBytes > budget.maxEncodedBytes ||
      native.blockCount > budget.maxBlockCount ||
      native.storagePagesVisited > budget.maxStoragePagesVisited ||
      native.openDepth > budget.maxOpenDepth ||
      native.treeNodesVisited > budget.maxTreeNodesVisited ||
      !flarkV3HostPackedEntryReceiptFitsStoragePages(
        storagePagesVisited: native.storagePagesVisited,
        packedEntriesInspected: native.packedEntriesInspected,
      ) ||
      (native.continuationLength == 0 &&
          !continuationBytes.every((byte) => byte == 0)) ||
      (native.outcome == 1 && complete != (native.continuationLength == 0))) {
    throw const FlarkV3NativeHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail: 'native host returned an out-of-authority range receipt',
    );
  }
  final receipt = FlarkV3HostBlockRangeReceipt(
    encodedBytes: native.encodedBytes,
    blockCount: native.blockCount,
    storagePagesVisited: native.storagePagesVisited,
    openDepth: native.openDepth,
    treeNodesVisited: native.treeNodesVisited,
    packedEntriesInspected: native.packedEntriesInspected,
    summaryNodesSkipped: native.summaryNodesSkipped,
    complete: complete,
  );
  final continuation = native.continuationLength == 0
      ? null
      : FlarkV3HostBlockRangeContinuation.owned(continuationBytes);
  final requestedNonempty =
      query.requestedRange.start != query.requestedRange.end;
  final encoded = native.encodedBytes == 0
      ? Uint8List(0)
      : Uint8List.fromList(output.asTypedList(native.encodedBytes));
  final canonicalEnvelope = _canonicalBlockRangeEnvelopeLength(
    encoded,
    native.blockCount,
  );
  final coverageOverlapsRequest = _rangesOverlap(
    coverage,
    query.requestedRange,
  );
  return switch (native.outcome) {
    1
        when native.gapReason == 0 &&
            canonicalEnvelope &&
            (!requestedNonempty || native.blockCount > 0) &&
            (!requestedNonempty || coverageOverlapsRequest) =>
      FlarkV3HostStoreStructuralBlockRangeQuery(
        FlarkV3HostStructuralBlockRange.owned(
          sourceVersion: source,
          requestedRange: query.requestedRange,
          coveredRange: coverage,
          encoded: encoded,
          receipt: receipt,
          continuation: continuation,
        ),
      ),
    2
        when native.gapReason != 0 &&
            native.encodedBytes == 0 &&
            native.blockCount == 0 &&
            native.flags == 0 &&
            continuation == null &&
            coverage == query.requestedRange =>
      FlarkV3HostStoreBlockRangeSourceGapQuery(
        FlarkV3HostBlockRangeSourceGap(
          sourceVersion: source,
          requestedRange: query.requestedRange,
          reason: _readPointQueryGapReason(native.gapReason),
          receipt: receipt,
        ),
      ),
    _ => throw FlarkV3NativeHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail:
          'native host returned invalid range outcome ${native.outcome} '
          'and gap ${native.gapReason}',
    ),
  };
}

bool _canonicalBlockRangeEnvelopeLength(Uint8List encoded, int rowCount) {
  const magic = <int>[70, 76, 75, 86, 82, 48, 48, 49]; // FLKVR001
  if (encoded.lengthInBytes < 12) return false;
  for (var index = 0; index < magic.length; index += 1) {
    if (encoded[index] != magic[index]) return false;
  }
  final data = ByteData.sublistView(encoded);
  return switch (data.getUint32(8, Endian.little)) {
    1 => encoded.lengthInBytes == 32 + 160 * rowCount,
    11 when encoded.lengthInBytes >= 96 =>
      data.getUint32(12, Endian.little) == 96 &&
          data.getUint32(16, Endian.little) == 64 &&
          data.getUint32(20, Endian.little) == 48 &&
          data.getUint32(24, Endian.little) == rowCount &&
          encoded.lengthInBytes ==
              96 + 64 * rowCount + 48 * data.getUint32(28, Endian.little),
    _ => false,
  };
}

FlarkV3HostSourceGapReason _readPointQueryGapReason(int value) =>
    switch (value) {
      1 => FlarkV3HostSourceGapReason.openDepthLimit,
      2 => FlarkV3HostSourceGapReason.encodedByteLimit,
      3 => FlarkV3HostSourceGapReason.leafLimit,
      4 => FlarkV3HostSourceGapReason.treeNodeLimit,
      5 => FlarkV3HostSourceGapReason.undecodableClosure,
      6 => FlarkV3HostSourceGapReason.unavailableFacts,
      _ => throw FlarkV3NativeHostException(
        operation: 'queryStructuralReceipt',
        status: 0x0111,
        detail: 'native host returned unknown query gap $value',
      ),
    };

bool _rangesOverlap(FlarkV3MetricRange left, FlarkV3MetricRange right) =>
    left.start.bytes < right.end.bytes &&
    left.start.utf16 < right.end.utf16 &&
    right.start.bytes < left.end.bytes &&
    right.start.utf16 < left.end.utf16;

FlarkV3SourceVersion _readSourceVersion(_NativeHostSourceVersion source) =>
    FlarkV3SourceVersion(
      documentSession: FlarkV3DocumentSessionId(
        source.documentSession[0],
        source.documentSession[1],
        source.documentSession[2],
        source.documentSession[3],
      ),
      revision: source.revision,
      metric: FlarkV3SourceMetric(
        bytes: source.utf8Length,
        utf16: source.utf16Length,
      ),
      contentHash: FlarkV3ContentHash128(
        source.contentHash128[0],
        source.contentHash128[1],
        source.contentHash128[2],
        source.contentHash128[3],
      ),
    );

FlarkV3OfferId _readOfferId(Array<Uint32> words) =>
    FlarkV3OfferId(words[0], words[1], words[2], words[3]);

FlarkV3ProtocolDigest128 _readDigest(Array<Uint32> words) =>
    FlarkV3ProtocolDigest128(words[0], words[1], words[2], words[3]);

FlarkV3ProtocolU64 _readU64(_NativeHostU64 value) =>
    FlarkV3ProtocolU64(lowWord: value.lowWord, highWord: value.highWord);

FlarkV3ProtocolDigest256 _readDigest256(Array<Uint32> words) =>
    FlarkV3ProtocolDigest256(
      words[0],
      words[1],
      words[2],
      words[3],
      words[4],
      words[5],
      words[6],
      words[7],
    );

FlarkV3StructuralAck _readAck(_NativeHostStructuralAck ack) =>
    FlarkV3StructuralAck(
      publicationSession: FlarkV3PublicationSessionId(
        ack.publicationSession[0],
        ack.publicationSession[1],
        ack.publicationSession[2],
        ack.publicationSession[3],
      ),
      hostRevision: FlarkV3HostRevisionId(ack.hostRevision),
      sourceVersion: _readSourceVersion(ack.sourceVersion),
      sourceRoot: FlarkV3SourceRootId(ack.sourceRoot[0], ack.sourceRoot[1]),
      parseGeneration: ack.parseGeneration,
      grammarRevision: ack.grammarRevision,
      syntaxProfile: FlarkV3SyntaxProfileId(ack.syntaxProfile),
      authorityMask: FlarkV3StructuralAuthorityMask(ack.authorityMask),
      recordCount: ack.recordCount,
      sequenceDigest: _readDigest(ack.sequenceDigest),
      manifestDigest: _readDigest(ack.manifestDigest),
    );

FlarkV3InlineSidecarAck _readInlineSidecarAck(_NativeHostInlineSidecarAck ack) {
  final disposition = switch (ack.disposition) {
    1 => FlarkV3InlineSidecarAckDisposition.authoritative,
    2 => FlarkV3InlineSidecarAckDisposition.unsupported,
    _ => throw FlarkV3NativeHostException(
      operation: 'pollInlineSidecarAck',
      status: 0x0111,
      detail: 'unknown sidecar ACK disposition ${ack.disposition}',
    ),
  };
  return FlarkV3InlineSidecarAck(
    publicationSession: FlarkV3PublicationSessionId(
      ack.publicationSession[0],
      ack.publicationSession[1],
      ack.publicationSession[2],
      ack.publicationSession[3],
    ),
    baseAck: _readAck(ack.baseAck),
    refinementGeneration: _readU64(ack.refinementGeneration),
    blockOrdinal: _readU64(ack.blockOrdinal),
    transferredNodeCount: ack.transferredNodeCount,
    disposition: disposition,
    hio1EnvelopeDigest256: _readDigest256(ack.hio1EnvelopeDigest256),
    rootStreamDigest: _readDigest(ack.rootStreamDigest),
  );
}

FlarkV3ViewportPresentationMetricRange _readViewportPresentationRange(
  _NativeHostViewportPresentationMetricRange range,
) => FlarkV3ViewportPresentationMetricRange(
  startUtf8: range.startUtf8,
  startUtf16: range.startUtf16,
  endUtf8: range.endUtf8,
  endUtf16: range.endUtf16,
);

FlarkV3ViewportPresentationVisitStart _readViewportPresentationVisitStart(
  _NativeHostViewportPresentationVisitStart start,
) => FlarkV3ViewportPresentationVisitStart(
  blockOrdinal: _readU64(start.blockOrdinal),
  utf8Offset: start.utf8Offset,
  utf16Offset: start.utf16Offset,
);

FlarkV3ViewportPresentationBinding _readViewportPresentationBinding(
  _NativeHostViewportPresentationBinding binding,
) {
  final complete = switch (binding.complete) {
    0 => false,
    1 => true,
    _ => throw FlarkV3NativeHostException(
      operation: 'viewportPresentationBinding',
      status: 0x0111,
      detail: 'native host returned invalid complete flag ${binding.complete}',
    ),
  };
  return FlarkV3ViewportPresentationBinding(
    viewportGeneration: binding.viewportGeneration,
    requestedRange: _readViewportPresentationRange(binding.requestedRange),
    coveredRange: _readViewportPresentationRange(binding.coveredRange),
    start: _readViewportPresentationVisitStart(binding.start),
    next: _readViewportPresentationVisitStart(binding.next),
    complete: complete,
  );
}

FlarkV3ViewportPresentationEnvelopeMetrics _readViewportPresentationEnvelope(
  _NativeHostViewportPresentationEnvelope envelope,
) => FlarkV3ViewportPresentationEnvelopeMetrics(
  visitedStructuralEntries: envelope.visitedStructuralEntries,
  visitedStoragePages: envelope.visitedStoragePages,
  orderedLeafCount: envelope.orderedLeafCount,
  inlineSourceBytes: envelope.inlineSourceBytes,
  factCount: envelope.factCount,
  transferredNodeCount: envelope.transferredNodeCount,
  parserTransitions: envelope.parserTransitions,
  aggregateEnvelopeDigest256: _readDigest256(
    envelope.aggregateEnvelopeDigest256,
  ),
);

FlarkV3ViewportPresentationAck _readViewportPresentationAck(
  _NativeHostViewportPresentationAck ack,
) => FlarkV3ViewportPresentationAck(
  publicationSession: FlarkV3PublicationSessionId(
    ack.publicationSession[0],
    ack.publicationSession[1],
    ack.publicationSession[2],
    ack.publicationSession[3],
  ),
  baseAck: _readAck(ack.baseAck),
  binding: _readViewportPresentationBinding(ack.binding),
  envelope: _readViewportPresentationEnvelope(ack.envelope),
  actualFrameCount: ack.actualFrameCount,
  actualEncodedFrameBytes: ack.actualEncodedFrameBytes,
  aggregateRootStreamDigest: _readDigest(ack.aggregateRootStreamDigest),
);

final class _NativeHostScratch {
  _NativeHostScratch._({
    required this.allocation,
    required this.allocationBytes,
    required this.config,
    required this.handle,
    required this.source,
    required this.begin,
    required this.commit,
    required this.ack,
    required this.inlineSidecarBegin,
    required this.inlineSidecarCommit,
    required this.inlineSidecarAck,
    required this.id128,
    required this.callReceipt,
    required this.workGrant,
    required this.pollReceipt,
    required this.inlineSidecarPollReceipt,
    required this.pointQuery,
    required this.pointQueryReceipt,
    required this.blockRangeQuery,
    required this.blockRangeQueryReceipt,
    required this.structuralOrdinalWindowQuery,
    required this.structuralOrdinalWindowReceipt,
    required this.inlineSidecarQuery,
    required this.inlineSidecarQueryReceipt,
    required this.viewportPresentationBegin,
    required this.viewportPresentationCommit,
    required this.viewportPresentationAck,
    required this.viewportPresentationPollReceipt,
    required this.viewportPresentationQuery,
    required this.viewportPresentationQueryReceipt,
    required this.finalizerTokenOutput,
    required this.bulk,
  });

  factory _NativeHostScratch.allocate() {
    final sizes = <int>[
      sizeOf<_NativeHostConfig>(),
      sizeOf<_NativeHostHandle>(),
      sizeOf<_NativeHostSourceVersion>(),
      sizeOf<_NativeHostOfferBegin>(),
      sizeOf<_NativeHostCommitRequest>(),
      sizeOf<_NativeHostStructuralAck>(),
      sizeOf<_NativeHostInlineSidecarBegin>(),
      sizeOf<_NativeHostInlineSidecarCommitRequest>(),
      sizeOf<_NativeHostInlineSidecarAck>(),
      sizeOf<_NativeHostId128>(),
      sizeOf<_NativeHostCallReceipt>(),
      sizeOf<_NativeHostWorkGrant>(),
      sizeOf<_NativeHostPollReceipt>(),
      sizeOf<_NativeHostInlineSidecarPollReceipt>(),
      sizeOf<_NativeHostPointQuery>(),
      sizeOf<_NativeHostPointQueryReceipt>(),
      sizeOf<_NativeHostBlockRangeQuery>(),
      sizeOf<_NativeHostBlockRangeQueryReceipt>(),
      sizeOf<_NativeHostStructuralOrdinalWindowQuery>(),
      sizeOf<_NativeHostStructuralOrdinalWindowReceipt>(),
      sizeOf<_NativeHostInlineSidecarQuery>(),
      sizeOf<_NativeHostInlineSidecarQueryReceipt>(),
      sizeOf<_NativeHostViewportPresentationBegin>(),
      sizeOf<_NativeHostViewportPresentationCommitRequest>(),
      sizeOf<_NativeHostViewportPresentationAck>(),
      sizeOf<_NativeHostViewportPresentationPollReceipt>(),
      sizeOf<_NativeHostViewportPresentationQuery>(),
      sizeOf<_NativeHostViewportPresentationQueryReceipt>(),
      sizeOf<Pointer<Void>>(),
      flarkV3NativeHostBulkScratchBytes,
    ];
    final allocationBytes = sizes.fold<int>(
      0,
      (total, size) => total + _alignedScratchBytes(size),
    );
    final allocation = calloc<Uint8>(allocationBytes);
    var offset = 0;
    Pointer<T> take<T extends NativeType>(int bytes) {
      final pointer = (allocation + offset).cast<T>();
      offset += _alignedScratchBytes(bytes);
      return pointer;
    }

    return _NativeHostScratch._(
      allocation: allocation,
      allocationBytes: allocationBytes,
      config: take(sizeOf<_NativeHostConfig>()),
      handle: take(sizeOf<_NativeHostHandle>()),
      source: take(sizeOf<_NativeHostSourceVersion>()),
      begin: take(sizeOf<_NativeHostOfferBegin>()),
      commit: take(sizeOf<_NativeHostCommitRequest>()),
      ack: take(sizeOf<_NativeHostStructuralAck>()),
      inlineSidecarBegin: take(sizeOf<_NativeHostInlineSidecarBegin>()),
      inlineSidecarCommit: take(
        sizeOf<_NativeHostInlineSidecarCommitRequest>(),
      ),
      inlineSidecarAck: take(sizeOf<_NativeHostInlineSidecarAck>()),
      id128: take(sizeOf<_NativeHostId128>()),
      callReceipt: take(sizeOf<_NativeHostCallReceipt>()),
      workGrant: take(sizeOf<_NativeHostWorkGrant>()),
      pollReceipt: take(sizeOf<_NativeHostPollReceipt>()),
      inlineSidecarPollReceipt: take(
        sizeOf<_NativeHostInlineSidecarPollReceipt>(),
      ),
      pointQuery: take(sizeOf<_NativeHostPointQuery>()),
      pointQueryReceipt: take(sizeOf<_NativeHostPointQueryReceipt>()),
      blockRangeQuery: take(sizeOf<_NativeHostBlockRangeQuery>()),
      blockRangeQueryReceipt: take(sizeOf<_NativeHostBlockRangeQueryReceipt>()),
      structuralOrdinalWindowQuery: take(
        sizeOf<_NativeHostStructuralOrdinalWindowQuery>(),
      ),
      structuralOrdinalWindowReceipt: take(
        sizeOf<_NativeHostStructuralOrdinalWindowReceipt>(),
      ),
      inlineSidecarQuery: take(sizeOf<_NativeHostInlineSidecarQuery>()),
      inlineSidecarQueryReceipt: take(
        sizeOf<_NativeHostInlineSidecarQueryReceipt>(),
      ),
      viewportPresentationBegin: take(
        sizeOf<_NativeHostViewportPresentationBegin>(),
      ),
      viewportPresentationCommit: take(
        sizeOf<_NativeHostViewportPresentationCommitRequest>(),
      ),
      viewportPresentationAck: take(
        sizeOf<_NativeHostViewportPresentationAck>(),
      ),
      viewportPresentationPollReceipt: take(
        sizeOf<_NativeHostViewportPresentationPollReceipt>(),
      ),
      viewportPresentationQuery: take(
        sizeOf<_NativeHostViewportPresentationQuery>(),
      ),
      viewportPresentationQueryReceipt: take(
        sizeOf<_NativeHostViewportPresentationQueryReceipt>(),
      ),
      finalizerTokenOutput: take(sizeOf<Pointer<Void>>()),
      bulk: take(flarkV3NativeHostBulkScratchBytes),
    );
  }

  final Pointer<Uint8> allocation;
  final int allocationBytes;
  final Pointer<_NativeHostConfig> config;
  final Pointer<_NativeHostHandle> handle;
  final Pointer<_NativeHostSourceVersion> source;
  final Pointer<_NativeHostOfferBegin> begin;
  final Pointer<_NativeHostCommitRequest> commit;
  final Pointer<_NativeHostStructuralAck> ack;
  final Pointer<_NativeHostInlineSidecarBegin> inlineSidecarBegin;
  final Pointer<_NativeHostInlineSidecarCommitRequest> inlineSidecarCommit;
  final Pointer<_NativeHostInlineSidecarAck> inlineSidecarAck;
  final Pointer<_NativeHostId128> id128;
  final Pointer<_NativeHostCallReceipt> callReceipt;
  final Pointer<_NativeHostWorkGrant> workGrant;
  final Pointer<_NativeHostPollReceipt> pollReceipt;
  final Pointer<_NativeHostInlineSidecarPollReceipt> inlineSidecarPollReceipt;
  final Pointer<_NativeHostPointQuery> pointQuery;
  final Pointer<_NativeHostPointQueryReceipt> pointQueryReceipt;
  final Pointer<_NativeHostBlockRangeQuery> blockRangeQuery;
  final Pointer<_NativeHostBlockRangeQueryReceipt> blockRangeQueryReceipt;
  final Pointer<_NativeHostStructuralOrdinalWindowQuery>
  structuralOrdinalWindowQuery;
  final Pointer<_NativeHostStructuralOrdinalWindowReceipt>
  structuralOrdinalWindowReceipt;
  final Pointer<_NativeHostInlineSidecarQuery> inlineSidecarQuery;
  final Pointer<_NativeHostInlineSidecarQueryReceipt> inlineSidecarQueryReceipt;
  final Pointer<_NativeHostViewportPresentationBegin> viewportPresentationBegin;
  final Pointer<_NativeHostViewportPresentationCommitRequest>
  viewportPresentationCommit;
  final Pointer<_NativeHostViewportPresentationAck> viewportPresentationAck;
  final Pointer<_NativeHostViewportPresentationPollReceipt>
  viewportPresentationPollReceipt;
  final Pointer<_NativeHostViewportPresentationQuery> viewportPresentationQuery;
  final Pointer<_NativeHostViewportPresentationQueryReceipt>
  viewportPresentationQueryReceipt;
  final Pointer<Pointer<Void>> finalizerTokenOutput;
  final Pointer<Uint8> bulk;

  final Object _detachKey = Object();
  bool _attached = false;
  bool _freed = false;

  void attachTo(Finalizable owner) {
    if (_attached || _freed) {
      throw StateError('Native host scratch has an invalid ownership state.');
    }
    _attached = true;
    _scratchAllocationFinalizer.attach(
      owner,
      allocation.cast(),
      detach: _detachKey,
      externalSize: allocationBytes,
    );
  }

  void free() {
    if (_freed) return;
    _freed = true;
    if (_attached) _scratchAllocationFinalizer.detach(_detachKey);
    calloc.free(allocation);
  }
}

int _alignedScratchBytes(int bytes) => (bytes + 7) & ~7;

final class _NativeHostHandle extends Struct {
  @Uint32()
  external int slot;

  @Uint32()
  external int generation;
}

final class _NativeHostConfig extends Struct {
  @Uint32()
  external int abiVersion;

  @Uint32()
  external int structSize;

  @Array(4)
  external Array<Uint32> documentSession;

  @Uint32()
  external int grammarRevision;

  @Uint32()
  external int syntaxProfile;

  @Uint32()
  external int authorityMask;

  @Uint32()
  external int maximumQueryBytes;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostSourceVersion extends Struct {
  @Array(4)
  external Array<Uint32> documentSession;

  @Uint32()
  external int revision;

  @Uint32()
  external int utf8Length;

  @Uint32()
  external int utf16Length;

  @Array(4)
  external Array<Uint32> contentHash128;
}

final class _NativeHostOfferBegin extends Struct {
  @Array(4)
  external Array<Uint32> offerId;

  @Array(4)
  external Array<Uint32> publicationSession;

  @Uint32()
  external int targetHostRevision;

  external _NativeHostSourceVersion sourceVersion;

  @Array(2)
  external Array<Uint32> sourceRoot;

  @Uint32()
  external int parseGeneration;

  @Uint32()
  external int grammarRevision;

  @Uint32()
  external int syntaxProfile;

  @Uint32()
  external int authorityMask;

  @Uint32()
  external int transferredRecordCount;

  @Uint32()
  external int targetRecordCount;

  @Uint32()
  external int maximumFrameCount;

  @Uint32()
  external int maximumEncodedFrameBytes;

  @Uint32()
  external int maximumPacketBytes;

  @Uint32()
  external int maximumFrameBytes;

  @Uint32()
  external int maximumProgramChildren;

  @Array(3)
  external Array<Uint32> reserved;
}

final class _NativeHostCommitRequest extends Struct {
  @Array(4)
  external Array<Uint32> offerId;

  @Uint32()
  external int actualFrameCount;

  @Uint32()
  external int actualEncodedFrameBytes;

  @Array(4)
  external Array<Uint32> rollingTransportDigest;

  @Array(4)
  external Array<Uint32> canonicalStreamDigest;
}

final class _NativeHostStructuralAck extends Struct {
  @Array(4)
  external Array<Uint32> publicationSession;

  @Uint32()
  external int hostRevision;

  external _NativeHostSourceVersion sourceVersion;

  @Array(2)
  external Array<Uint32> sourceRoot;

  @Uint32()
  external int parseGeneration;

  @Uint32()
  external int grammarRevision;

  @Uint32()
  external int syntaxProfile;

  @Uint32()
  external int authorityMask;

  @Uint32()
  external int recordCount;

  @Array(4)
  external Array<Uint32> sequenceDigest;

  @Array(4)
  external Array<Uint32> manifestDigest;
}

final class _NativeHostU64 extends Struct {
  @Uint32()
  external int lowWord;

  @Uint32()
  external int highWord;
}

final class _NativeHostInlineSidecarBinding extends Struct {
  external _NativeHostU64 parserProfile;

  external _NativeHostU64 refinementGeneration;

  external _NativeHostU64 blockOrdinal;

  @Uint32()
  external int physicalStartUtf8;

  @Uint32()
  external int physicalEndUtf8;

  @Uint32()
  external int visibleStartUtf8;

  @Uint32()
  external int visibleEndUtf8;

  @Uint32()
  external int physicalStartUtf16;

  @Uint32()
  external int physicalEndUtf16;

  @Uint32()
  external int visibleStartUtf16;

  @Uint32()
  external int visibleEndUtf16;
}

final class _NativeHostInlineSidecarDisposition extends Struct {
  @Uint32()
  external int disposition;

  @Uint32()
  external int reason;

  external _NativeHostU64 logicalPageCount;

  external _NativeHostU64 factCount;

  external _NativeHostU64 storagePageCount;

  @Uint32()
  external int linkValueEntryCount;

  @Uint32()
  external int linkValueEncodedBytes;

  external _NativeHostU64 linkValueStoragePageCount;

  @Array(8)
  external Array<Uint32> commitment256;
}

final class _NativeHostInlineSidecarBegin extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int mode;

  @Array(4)
  external Array<Uint32> offerId;

  @Array(4)
  external Array<Uint32> publicationSession;

  external _NativeHostStructuralAck baseAck;

  external _NativeHostInlineSidecarBinding binding;

  @Uint32()
  external int hio1EncodedBytes;

  @Uint32()
  external int ipr2DescriptorBytes;

  @Uint32()
  external int transferredNodeCount;

  external _NativeHostInlineSidecarDisposition sidecarDisposition;

  @Array(8)
  external Array<Uint32> hio1EnvelopeDigest256;

  @Uint32()
  external int maximumFrameCount;

  @Uint32()
  external int maximumEncodedFrameBytes;

  @Uint32()
  external int maximumPacketBytes;

  @Uint32()
  external int maximumFrameBytes;

  @Uint32()
  external int maximumProgramChildren;
}

final class _NativeHostInlineSidecarCommitRequest extends Struct {
  @Array(4)
  external Array<Uint32> offerId;

  @Uint32()
  external int actualFrameCount;

  @Uint32()
  external int actualEncodedFrameBytes;

  @Array(4)
  external Array<Uint32> rollingTransportDigest;

  @Array(4)
  external Array<Uint32> rootStreamDigest;
}

final class _NativeHostInlineSidecarAck extends Struct {
  @Array(4)
  external Array<Uint32> publicationSession;

  external _NativeHostStructuralAck baseAck;

  external _NativeHostU64 refinementGeneration;

  external _NativeHostU64 blockOrdinal;

  @Uint32()
  external int transferredNodeCount;

  @Uint32()
  external int disposition;

  @Array(8)
  external Array<Uint32> hio1EnvelopeDigest256;

  @Array(4)
  external Array<Uint32> rootStreamDigest;
}

final class _NativeHostInlineSidecarPollReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Array(4)
  external Array<Uint32> offerId;

  @Uint32()
  external int nextFrameOrdinal;

  external _NativeHostInlineSidecarAck ack;
}

final class _NativeHostInlineSidecarQuery extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int structSize;

  external _NativeHostInlineSidecarBinding binding;

  @Uint32()
  external int maximumEncodedBytes;

  @Array(3)
  external Array<Uint32> reserved;
}

final class _NativeHostInlineSidecarQueryReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Uint32()
  external int reason;

  @Uint32()
  external int encodedBytes;

  @Uint32()
  external int factCount;

  @Uint32()
  external int treeNodesVisited;

  @Uint32()
  external int valueEntryCount;

  @Uint32()
  external int valueEncodedBytes;
}

final class _NativeHostViewportPresentationMetricRange extends Struct {
  @Uint32()
  external int startUtf8;

  @Uint32()
  external int startUtf16;

  @Uint32()
  external int endUtf8;

  @Uint32()
  external int endUtf16;
}

final class _NativeHostViewportPresentationVisitStart extends Struct {
  external _NativeHostU64 blockOrdinal;

  @Uint32()
  external int utf8Offset;

  @Uint32()
  external int utf16Offset;
}

final class _NativeHostViewportPresentationBinding extends Struct {
  @Uint32()
  external int viewportGeneration;

  external _NativeHostViewportPresentationMetricRange requestedRange;

  external _NativeHostViewportPresentationMetricRange coveredRange;

  external _NativeHostViewportPresentationVisitStart start;

  external _NativeHostViewportPresentationVisitStart next;

  @Uint32()
  external int complete;
}

final class _NativeHostViewportPresentationEnvelope extends Struct {
  @Uint32()
  external int visitedStructuralEntries;

  @Uint32()
  external int visitedStoragePages;

  @Uint32()
  external int orderedLeafCount;

  @Uint32()
  external int inlineSourceBytes;

  @Uint32()
  external int factCount;

  @Uint32()
  external int transferredNodeCount;

  @Uint32()
  external int parserTransitions;

  @Array(8)
  external Array<Uint32> aggregateEnvelopeDigest256;
}

final class _NativeHostViewportPresentationQueryLimits extends Struct {
  @Uint32()
  external int maximumStructuralEntries;

  @Uint32()
  external int maximumStoragePages;

  @Uint32()
  external int maximumInlineLeaves;

  @Uint32()
  external int maximumInlineLeafSourceBytes;

  @Uint32()
  external int maximumInlineSourceBytes;

  @Uint32()
  external int maximumFactRecords;

  @Uint32()
  external int maximumEncodedFrameBytes;

  @Uint32()
  external int maximumParserTransitions;
}

final class _NativeHostViewportPresentationOfferLimits extends Struct {
  @Uint32()
  external int maximumFrameCount;

  @Uint32()
  external int maximumEncodedFrameBytes;

  @Uint32()
  external int maximumPacketBytes;

  @Uint32()
  external int maximumFrameBytes;

  @Uint32()
  external int maximumProgramChildren;
}

final class _NativeHostViewportPresentationBegin extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int mode;

  @Array(4)
  external Array<Uint32> offerId;

  @Array(4)
  external Array<Uint32> publicationSession;

  external _NativeHostStructuralAck baseAck;

  external _NativeHostViewportPresentationBinding binding;

  external _NativeHostViewportPresentationEnvelope envelope;

  external _NativeHostViewportPresentationQueryLimits queryLimits;

  external _NativeHostViewportPresentationOfferLimits limits;
}

final class _NativeHostViewportPresentationCommitRequest extends Struct {
  @Array(4)
  external Array<Uint32> offerId;

  @Uint32()
  external int actualFrameCount;

  @Uint32()
  external int actualEncodedFrameBytes;

  @Array(4)
  external Array<Uint32> rollingTransportDigest;

  @Array(4)
  external Array<Uint32> aggregateRootStreamDigest;
}

final class _NativeHostViewportPresentationAck extends Struct {
  @Array(4)
  external Array<Uint32> publicationSession;

  external _NativeHostStructuralAck baseAck;

  external _NativeHostViewportPresentationBinding binding;

  external _NativeHostViewportPresentationEnvelope envelope;

  @Uint32()
  external int actualFrameCount;

  @Uint32()
  external int actualEncodedFrameBytes;

  @Array(4)
  external Array<Uint32> aggregateRootStreamDigest;
}

final class _NativeHostViewportPresentationPollReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Array(4)
  external Array<Uint32> offerId;

  @Uint32()
  external int nextFrameOrdinal;

  external _NativeHostViewportPresentationAck ack;
}

final class _NativeHostViewportPresentationQuery extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int structSize;

  external _NativeHostViewportPresentationAck ack;

  @Uint32()
  external int maximumEncodedBytes;

  @Array(3)
  external Array<Uint32> reserved;
}

final class _NativeHostViewportPresentationQueryReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Uint32()
  external int encodedBytes;

  @Uint32()
  external int entryCount;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostCallReceipt extends Struct {
  @Uint32()
  external int rejectionReason;
}

final class _NativeHostWorkGrant extends Struct {
  @Uint32()
  external int inspectBytes;

  @Uint32()
  external int copyBytes;

  @Uint32()
  external int transitions;
}

final class _NativeHostPollReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Array(4)
  external Array<Uint32> offerId;

  @Uint32()
  external int nextFrameOrdinal;

  external _NativeHostStructuralAck ack;
}

final class _NativeHostPointQuery extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int structSize;

  external _NativeHostSourceVersion sourceVersion;

  @Uint32()
  external int positionUtf8;

  @Uint32()
  external int positionUtf16;

  @Uint32()
  external int affinity;

  @Uint32()
  external int maximumEncodedBytes;

  @Uint32()
  external int maximumOpenDepth;

  @Uint32()
  external int maximumLeafCount;

  @Uint32()
  external int maximumTreeNodesVisited;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostPointQueryReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Uint32()
  external int gapReason;

  @Uint32()
  external int encodedBytes;

  @Uint32()
  external int leafCount;

  @Uint32()
  external int openDepth;

  @Uint32()
  external int treeNodesVisited;

  @Uint32()
  external int summaryNodesSkipped;

  @Uint32()
  external int rangeStartUtf8;

  @Uint32()
  external int rangeStartUtf16;

  @Uint32()
  external int rangeEndUtf8;

  @Uint32()
  external int rangeEndUtf16;

  external _NativeHostSourceVersion sourceVersion;

  @Array(5)
  external Array<Uint32> reserved;
}

final class _NativeHostBlockRangeQuery extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int structSize;

  external _NativeHostSourceVersion sourceVersion;

  @Uint32()
  external int requestedStartUtf8;

  @Uint32()
  external int requestedStartUtf16;

  @Uint32()
  external int requestedEndUtf8;

  @Uint32()
  external int requestedEndUtf16;

  @Uint32()
  external int maximumEncodedBytes;

  @Uint32()
  external int maximumBlockCount;

  @Uint32()
  external int maximumStoragePagesVisited;

  @Uint32()
  external int maximumOpenDepth;

  @Uint32()
  external int maximumTreeNodesVisited;

  @Uint32()
  external int continuationLength;

  @Array(FlarkV3HostBlockRangeContinuation.encodedBytes)
  external Array<Uint8> continuation;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostBlockRangeQueryReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Uint32()
  external int gapReason;

  @Uint32()
  external int encodedBytes;

  @Uint32()
  external int blockCount;

  @Uint32()
  external int storagePagesVisited;

  @Uint32()
  external int openDepth;

  @Uint32()
  external int treeNodesVisited;

  @Uint32()
  external int packedEntriesInspected;

  @Uint32()
  external int summaryNodesSkipped;

  @Uint32()
  external int flags;

  @Uint32()
  external int coverageStartUtf8;

  @Uint32()
  external int coverageStartUtf16;

  @Uint32()
  external int coverageEndUtf8;

  @Uint32()
  external int coverageEndUtf16;

  external _NativeHostSourceVersion sourceVersion;

  @Uint32()
  external int continuationLength;

  @Array(FlarkV3HostBlockRangeContinuation.encodedBytes)
  external Array<Uint8> continuation;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostStructuralOrdinalWindowQuery extends Struct {
  @Uint32()
  external int schema;

  @Uint32()
  external int structSize;

  external _NativeHostSourceVersion sourceVersion;

  external _NativeHostU64 startBlockOrdinal;

  @Uint32()
  external int maximumEntries;

  @Uint32()
  external int maximumStoragePagesVisited;

  @Uint32()
  external int maximumTreeNodesVisited;

  @Uint32()
  external int maximumPackedEntriesInspected;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostStructuralOrdinalWindowReceipt extends Struct {
  @Uint32()
  external int rejectionReason;

  @Uint32()
  external int outcome;

  @Uint32()
  external int failureReason;

  @Uint32()
  external int flags;

  external _NativeHostU64 totalBlockCount;

  external _NativeHostU64 startBlockOrdinal;

  external _NativeHostU64 nextBlockOrdinal;

  @Uint32()
  external int startUtf8;

  @Uint32()
  external int startUtf16;

  @Uint32()
  external int nextUtf8;

  @Uint32()
  external int nextUtf16;

  @Uint32()
  external int storagePagesVisited;

  @Uint32()
  external int treeNodesVisited;

  @Uint32()
  external int packedEntriesInspected;

  @Uint32()
  external int summaryNodesSkipped;

  external _NativeHostSourceVersion sourceVersion;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeHostId128 extends Struct {
  @Array(4)
  external Array<Uint32> words;
}

typedef _AbiVersionNative = Uint32 Function();
typedef _AbiVersionDart = int Function();
typedef _StandardConfigNative =
    Uint32 Function(Pointer<_NativeHostConfig> output);
typedef _StandardConfigDart = int Function(Pointer<_NativeHostConfig> output);
typedef _CreateNative =
    Uint32 Function(
      Pointer<_NativeHostConfig> config,
      Pointer<_NativeHostHandle> output,
    );
typedef _CreateDart =
    int Function(
      Pointer<_NativeHostConfig> config,
      Pointer<_NativeHostHandle> output,
    );
typedef _ObserveSourceNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostSourceVersion> source,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _ObserveSourceDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostSourceVersion> source,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginOfferNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostOfferBegin> begin,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginOfferDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostOfferBegin> begin,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginInlineSidecarOfferNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarBegin> begin,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginInlineSidecarOfferDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarBegin> begin,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginViewportPresentationOfferNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationBegin> begin,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginViewportPresentationOfferDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationBegin> begin,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginReferencesDeltaNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostOfferBegin> begin,
      Pointer<_NativeHostStructuralAck> baseAck,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginReferencesDeltaDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostOfferBegin> begin,
      Pointer<_NativeHostStructuralAck> baseAck,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginExactBaseDeltaNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostOfferBegin> begin,
      Pointer<_NativeHostStructuralAck> baseAck,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _BeginExactBaseDeltaDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostOfferBegin> begin,
      Pointer<_NativeHostStructuralAck> baseAck,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AdmitPacketNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<Uint8> packet,
      Uint32 length,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AdmitPacketDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<Uint8> packet,
      int length,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CommitNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostCommitRequest> request,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CommitDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostCommitRequest> request,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CommitInlineSidecarNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarCommitRequest> request,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CommitInlineSidecarDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarCommitRequest> request,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CommitViewportPresentationNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationCommitRequest> request,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CommitViewportPresentationDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationCommitRequest> request,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AbortNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostId128> offerId,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AbortDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostId128> offerId,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _PollNative =
    Uint32 Function(
      _NativeHostHandle handle,
      _NativeHostWorkGrant grant,
      Pointer<_NativeHostPollReceipt> receipt,
    );
typedef _PollDart =
    int Function(
      _NativeHostHandle handle,
      _NativeHostWorkGrant grant,
      Pointer<_NativeHostPollReceipt> receipt,
    );
typedef _PollInlineSidecarNative =
    Uint32 Function(
      _NativeHostHandle handle,
      _NativeHostWorkGrant grant,
      Pointer<_NativeHostInlineSidecarPollReceipt> receipt,
    );
typedef _PollInlineSidecarDart =
    int Function(
      _NativeHostHandle handle,
      _NativeHostWorkGrant grant,
      Pointer<_NativeHostInlineSidecarPollReceipt> receipt,
    );
typedef _PollViewportPresentationNative =
    Uint32 Function(
      _NativeHostHandle handle,
      _NativeHostWorkGrant grant,
      Pointer<_NativeHostViewportPresentationPollReceipt> receipt,
    );
typedef _PollViewportPresentationDart =
    int Function(
      _NativeHostHandle handle,
      _NativeHostWorkGrant grant,
      Pointer<_NativeHostViewportPresentationPollReceipt> receipt,
    );
typedef _AcknowledgeDeliveryNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostStructuralAck> ack,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AcknowledgeDeliveryDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostStructuralAck> ack,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AcknowledgeInlineSidecarDeliveryNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarAck> ack,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AcknowledgeInlineSidecarDeliveryDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarAck> ack,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AcknowledgeViewportPresentationDeliveryNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationAck> ack,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _AcknowledgeViewportPresentationDeliveryDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationAck> ack,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _QueryStructuralNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostPointQuery> query,
      Pointer<Uint8> output,
      Uint32 capacity,
      Pointer<_NativeHostPointQueryReceipt> receipt,
    );
typedef _QueryStructuralDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostPointQuery> query,
      Pointer<Uint8> output,
      int capacity,
      Pointer<_NativeHostPointQueryReceipt> receipt,
    );
typedef _QueryStructuralRangeNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostBlockRangeQuery> query,
      Pointer<Uint8> output,
      Uint32 capacity,
      Pointer<_NativeHostBlockRangeQueryReceipt> receipt,
    );
typedef _QueryStructuralRangeDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostBlockRangeQuery> query,
      Pointer<Uint8> output,
      int capacity,
      Pointer<_NativeHostBlockRangeQueryReceipt> receipt,
    );
typedef _QueryStructuralOrdinalWindowNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostStructuralOrdinalWindowQuery> query,
      Pointer<_NativeHostStructuralOrdinalWindowReceipt> receipt,
    );
typedef _QueryStructuralOrdinalWindowDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostStructuralOrdinalWindowQuery> query,
      Pointer<_NativeHostStructuralOrdinalWindowReceipt> receipt,
    );
typedef _QueryInlineSidecarNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarQuery> query,
      Pointer<Uint8> output,
      Uint32 capacity,
      Pointer<_NativeHostInlineSidecarQueryReceipt> receipt,
    );
typedef _QueryInlineSidecarDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostInlineSidecarQuery> query,
      Pointer<Uint8> output,
      int capacity,
      Pointer<_NativeHostInlineSidecarQueryReceipt> receipt,
    );
typedef _QueryViewportPresentationNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationQuery> query,
      Pointer<Uint8> output,
      Uint32 capacity,
      Pointer<_NativeHostViewportPresentationQueryReceipt> receipt,
    );
typedef _QueryViewportPresentationDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostViewportPresentationQuery> query,
      Pointer<Uint8> output,
      int capacity,
      Pointer<_NativeHostViewportPresentationQueryReceipt> receipt,
    );
typedef _CloseNative =
    Uint32 Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _CloseDart =
    int Function(
      _NativeHostHandle handle,
      Pointer<_NativeHostCallReceipt> receipt,
    );
typedef _DestroyNative = Uint32 Function(_NativeHostHandle handle);
typedef _DestroyDart = int Function(_NativeHostHandle handle);
typedef _FinalizerTokenCreateNative =
    Uint32 Function(_NativeHostHandle handle, Pointer<Pointer<Void>> output);
typedef _FinalizerTokenCreateDart =
    int Function(_NativeHostHandle handle, Pointer<Pointer<Void>> output);
typedef _FinalizerTokenReleaseNative = Uint32 Function(Pointer<Void> token);
typedef _FinalizerTokenReleaseDart = int Function(Pointer<Void> token);

final class _NativeHostSymbols {
  const _NativeHostSymbols({
    required this.abiVersion,
    required this.standardConfig,
    required this.create,
    required this.observeSource,
    required this.beginOffer,
    required this.beginInlineSidecarOffer,
    required this.beginViewportPresentationOffer,
    required this.beginReferencesDelta,
    required this.beginExactBaseDelta,
    required this.admitPacket,
    required this.admitInlineSidecarPacket,
    required this.admitViewportPresentationPacket,
    required this.requestCommit,
    required this.requestInlineSidecarCommit,
    required this.requestViewportPresentationCommit,
    required this.abortOffer,
    required this.abortInlineSidecarOffer,
    required this.abortViewportPresentationOffer,
    required this.poll,
    required this.pollInlineSidecar,
    required this.pollViewportPresentation,
    required this.acknowledgeDelivery,
    required this.acknowledgeInlineSidecarDelivery,
    required this.acknowledgeViewportPresentationDelivery,
    required this.queryStructural,
    required this.queryStructuralRange,
    required this.queryStructuralOrdinalWindow,
    required this.queryInlineSidecar,
    required this.queryViewportPresentation,
    required this.close,
    required this.remove,
    required this.emergencyDestroy,
    required this.finalizerTokenCreate,
    required this.finalizerTokenRelease,
    required this.emergencyFinalize,
  });

  factory _NativeHostSymbols.fromLibrary(
    DynamicLibrary library,
  ) => _NativeHostSymbols(
    abiVersion: library.lookupFunction<_AbiVersionNative, _AbiVersionDart>(
      'flark_v3_host_native_abi_version',
    ),
    standardConfig: library
        .lookupFunction<_StandardConfigNative, _StandardConfigDart>(
          'flark_v3_host_config_standard',
        ),
    create: library.lookupFunction<_CreateNative, _CreateDart>(
      'flark_v3_host_create',
    ),
    observeSource: library
        .lookupFunction<_ObserveSourceNative, _ObserveSourceDart>(
          'flark_v3_host_observe_source',
        ),
    beginOffer: library.lookupFunction<_BeginOfferNative, _BeginOfferDart>(
      'flark_v3_host_begin_offer',
    ),
    beginInlineSidecarOffer: library
        .lookupFunction<
          _BeginInlineSidecarOfferNative,
          _BeginInlineSidecarOfferDart
        >('flark_v3_host_begin_inline_sidecar_offer'),
    beginViewportPresentationOffer: library
        .lookupFunction<
          _BeginViewportPresentationOfferNative,
          _BeginViewportPresentationOfferDart
        >('flark_v3_host_begin_viewport_presentation_offer'),
    beginReferencesDelta: library
        .lookupFunction<_BeginReferencesDeltaNative, _BeginReferencesDeltaDart>(
          'flark_v3_host_begin_references_delta',
        ),
    beginExactBaseDelta: library
        .lookupFunction<_BeginExactBaseDeltaNative, _BeginExactBaseDeltaDart>(
          'flark_v3_host_begin_exact_base_delta',
        ),
    admitPacket: library.lookupFunction<_AdmitPacketNative, _AdmitPacketDart>(
      'flark_v3_host_admit_packet',
    ),
    admitInlineSidecarPacket: library
        .lookupFunction<_AdmitPacketNative, _AdmitPacketDart>(
          'flark_v3_host_admit_inline_sidecar_packet',
        ),
    admitViewportPresentationPacket: library
        .lookupFunction<_AdmitPacketNative, _AdmitPacketDart>(
          'flark_v3_host_admit_viewport_presentation_packet',
        ),
    requestCommit: library.lookupFunction<_CommitNative, _CommitDart>(
      'flark_v3_host_request_commit',
    ),
    requestInlineSidecarCommit: library
        .lookupFunction<_CommitInlineSidecarNative, _CommitInlineSidecarDart>(
          'flark_v3_host_request_inline_sidecar_commit',
        ),
    requestViewportPresentationCommit: library
        .lookupFunction<
          _CommitViewportPresentationNative,
          _CommitViewportPresentationDart
        >('flark_v3_host_request_viewport_presentation_commit'),
    abortOffer: library.lookupFunction<_AbortNative, _AbortDart>(
      'flark_v3_host_abort_offer',
    ),
    abortInlineSidecarOffer: library.lookupFunction<_AbortNative, _AbortDart>(
      'flark_v3_host_abort_inline_sidecar_offer',
    ),
    abortViewportPresentationOffer: library
        .lookupFunction<_AbortNative, _AbortDart>(
          'flark_v3_host_abort_viewport_presentation_offer',
        ),
    poll: library.lookupFunction<_PollNative, _PollDart>('flark_v3_host_poll'),
    pollInlineSidecar: library
        .lookupFunction<_PollInlineSidecarNative, _PollInlineSidecarDart>(
          'flark_v3_host_poll_inline_sidecar',
        ),
    pollViewportPresentation: library
        .lookupFunction<
          _PollViewportPresentationNative,
          _PollViewportPresentationDart
        >('flark_v3_host_poll_viewport_presentation'),
    acknowledgeDelivery: library
        .lookupFunction<_AcknowledgeDeliveryNative, _AcknowledgeDeliveryDart>(
          'flark_v3_host_acknowledge_delivery',
        ),
    acknowledgeInlineSidecarDelivery: library
        .lookupFunction<
          _AcknowledgeInlineSidecarDeliveryNative,
          _AcknowledgeInlineSidecarDeliveryDart
        >('flark_v3_host_acknowledge_inline_sidecar_delivery'),
    acknowledgeViewportPresentationDelivery: library
        .lookupFunction<
          _AcknowledgeViewportPresentationDeliveryNative,
          _AcknowledgeViewportPresentationDeliveryDart
        >('flark_v3_host_acknowledge_viewport_presentation_delivery'),
    queryStructural: library
        .lookupFunction<_QueryStructuralNative, _QueryStructuralDart>(
          'flark_v3_host_query_structural',
        ),
    queryStructuralRange: library
        .lookupFunction<_QueryStructuralRangeNative, _QueryStructuralRangeDart>(
          'flark_v3_host_query_structural_range',
        ),
    queryStructuralOrdinalWindow: library
        .lookupFunction<
          _QueryStructuralOrdinalWindowNative,
          _QueryStructuralOrdinalWindowDart
        >('flark_v3_host_query_structural_ordinal_window'),
    queryInlineSidecar: library
        .lookupFunction<_QueryInlineSidecarNative, _QueryInlineSidecarDart>(
          'flark_v3_host_query_inline_sidecar',
        ),
    queryViewportPresentation: library
        .lookupFunction<
          _QueryViewportPresentationNative,
          _QueryViewportPresentationDart
        >('flark_v3_host_query_viewport_presentation'),
    close: library.lookupFunction<_CloseNative, _CloseDart>(
      'flark_v3_host_close',
    ),
    remove: library.lookupFunction<_DestroyNative, _DestroyDart>(
      'flark_v3_host_remove',
    ),
    emergencyDestroy: library.lookupFunction<_DestroyNative, _DestroyDart>(
      'flark_v3_host_emergency_destroy',
    ),
    finalizerTokenCreate: library
        .lookupFunction<_FinalizerTokenCreateNative, _FinalizerTokenCreateDart>(
          'flark_v3_host_finalizer_token_create',
        ),
    finalizerTokenRelease: library
        .lookupFunction<
          _FinalizerTokenReleaseNative,
          _FinalizerTokenReleaseDart
        >('flark_v3_host_finalizer_token_release'),
    emergencyFinalize: library
        .lookup<NativeFunction<Void Function(Pointer<Void>)>>(
          'flark_v3_host_emergency_finalize',
        ),
  );

  final _AbiVersionDart abiVersion;
  final _StandardConfigDart standardConfig;
  final _CreateDart create;
  final _ObserveSourceDart observeSource;
  final _BeginOfferDart beginOffer;
  final _BeginInlineSidecarOfferDart beginInlineSidecarOffer;
  final _BeginViewportPresentationOfferDart beginViewportPresentationOffer;
  final _BeginReferencesDeltaDart beginReferencesDelta;
  final _BeginExactBaseDeltaDart beginExactBaseDelta;
  final _AdmitPacketDart admitPacket;
  final _AdmitPacketDart admitInlineSidecarPacket;
  final _AdmitPacketDart admitViewportPresentationPacket;
  final _CommitDart requestCommit;
  final _CommitInlineSidecarDart requestInlineSidecarCommit;
  final _CommitViewportPresentationDart requestViewportPresentationCommit;
  final _AbortDart abortOffer;
  final _AbortDart abortInlineSidecarOffer;
  final _AbortDart abortViewportPresentationOffer;
  final _PollDart poll;
  final _PollInlineSidecarDart pollInlineSidecar;
  final _PollViewportPresentationDart pollViewportPresentation;
  final _AcknowledgeDeliveryDart acknowledgeDelivery;
  final _AcknowledgeInlineSidecarDeliveryDart acknowledgeInlineSidecarDelivery;
  final _AcknowledgeViewportPresentationDeliveryDart
  acknowledgeViewportPresentationDelivery;
  final _QueryStructuralDart queryStructural;
  final _QueryStructuralRangeDart queryStructuralRange;
  final _QueryStructuralOrdinalWindowDart queryStructuralOrdinalWindow;
  final _QueryInlineSidecarDart queryInlineSidecar;
  final _QueryViewportPresentationDart queryViewportPresentation;
  final _CloseDart close;
  final _DestroyDart remove;
  final _DestroyDart emergencyDestroy;
  final _FinalizerTokenCreateDart finalizerTokenCreate;
  final _FinalizerTokenReleaseDart finalizerTokenRelease;
  final Pointer<NativeFunction<Void Function(Pointer<Void>)>> emergencyFinalize;
}
