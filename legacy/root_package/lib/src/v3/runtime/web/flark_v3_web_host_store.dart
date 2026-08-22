import 'dart:typed_data';

import '../../host/host.dart';
import '../../source/source.dart';
import '../flark_v3_parser_transport.dart';
import 'flark_v3_wasm_module.dart';

const int _hostAbiVersion = 0x00030007;
const int _statusOk = 0;
const int _maximumQueryBytes = 64 * 1024;
const int _inlineSidecarMaximumQueryBytes = 128 * 1024;
const int _viewportMaximumQueryBytes = 256 * 1024;
const int _bulkScratchBytes = _viewportMaximumQueryBytes;
const int _pointQuerySchema = 1;
const int _blockRangeQuerySchema = 1;
const int _structuralOrdinalWindowQuerySchema = 1;
const int _inlineSidecarQuerySchema = 3;

const List<String> _requiredHostExports = <String>[
  'flark_v3_wasm_host_native_abi_version',
  'flark_v3_wasm_host_config_standard',
  'flark_v3_wasm_host_create',
  'flark_v3_wasm_host_observe_source',
  'flark_v3_wasm_host_begin_offer',
  'flark_v3_wasm_host_begin_inline_sidecar_offer',
  'flark_v3_wasm_host_begin_viewport_presentation_offer',
  'flark_v3_wasm_host_begin_references_delta',
  'flark_v3_wasm_host_begin_exact_base_delta',
  'flark_v3_wasm_host_admit_packet',
  'flark_v3_wasm_host_admit_inline_sidecar_packet',
  'flark_v3_wasm_host_admit_viewport_presentation_packet',
  'flark_v3_wasm_host_request_commit',
  'flark_v3_wasm_host_request_inline_sidecar_commit',
  'flark_v3_wasm_host_request_viewport_presentation_commit',
  'flark_v3_wasm_host_abort_offer',
  'flark_v3_wasm_host_abort_inline_sidecar_offer',
  'flark_v3_wasm_host_abort_viewport_presentation_offer',
  'flark_v3_wasm_host_poll',
  'flark_v3_wasm_host_poll_inline_sidecar',
  'flark_v3_wasm_host_poll_viewport_presentation',
  'flark_v3_wasm_host_acknowledge_delivery',
  'flark_v3_wasm_host_acknowledge_inline_sidecar_delivery',
  'flark_v3_wasm_host_acknowledge_viewport_presentation_delivery',
  'flark_v3_wasm_host_query_structural',
  'flark_v3_wasm_host_query_structural_range',
  'flark_v3_wasm_host_query_structural_ordinal_window',
  'flark_v3_wasm_host_query_inline_sidecar',
  'flark_v3_wasm_host_query_viewport_presentation',
  'flark_v3_wasm_host_close',
  'flark_v3_wasm_host_remove',
  'flark_v3_wasm_host_emergency_destroy',
];

final Finalizer<_WebHostCleanup> _webHostFinalizer = Finalizer<_WebHostCleanup>(
  (cleanup) => cleanup.emergencyRelease(),
);
final Map<Uri, FlarkV3WasmModule> _sharedHostModules =
    <Uri, FlarkV3WasmModule>{};
final Map<Uri, Future<FlarkV3WasmModule>> _sharedHostModuleLoads =
    <Uri, Future<FlarkV3WasmModule>>{};

final class FlarkV3WebHostException implements Exception {
  const FlarkV3WebHostException({
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
    return 'FlarkV3WebHostException($operation, '
        'status=0x${status.toRadixString(16).padLeft(4, '0')}$suffix)';
  }
}

/// Main-context owner of one structural, inline-sidecar, and viewport host.
///
/// Parser state lives in a separate Worker/module instance. Publication bytes
/// are copied synchronously into this store under the same fixed envelope as
/// native, so installed roots never share linear-memory addresses with parser
/// roots. Each call is bounded by a foreground envelope or explicit poll fuel.
final class FlarkV3WebHostStore
    implements
        FlarkV3HostStore,
        FlarkV3BlockRangeHostStore,
        FlarkV3StructuralOrdinalWindowHostStore,
        FlarkV3InlineSidecarHostStore,
        FlarkV3ViewportPresentationHostStore {
  FlarkV3WebHostStore._({
    required FlarkV3WasmModule module,
    required _WebHostScratch scratch,
    required _WebHostCleanup cleanup,
  }) : _module = module,
       _scratch = scratch,
       _cleanup = cleanup {
    _webHostFinalizer.attach(this, cleanup, detach: this);
  }

  static Future<FlarkV3WebHostStore> create({
    required Uri wasmUri,
    required FlarkV3DocumentSessionId documentSession,
    int grammarRevision = flarkV3CurrentGrammarRevision,
    FlarkV3SyntaxProfileId? syntaxProfile,
    FlarkV3StructuralAuthorityMask? authorityMask,
  }) async {
    final module = await _loadSharedHostModule(wasmUri);
    final loadedAbi = module.callInt('flark_v3_wasm_host_native_abi_version');
    if (loadedAbi != _hostAbiVersion) {
      throw FlarkV3WebHostException(
        operation: 'abiVersion',
        status: loadedAbi,
        detail: 'expected 0x${_hostAbiVersion.toRadixString(16)}',
      );
    }

    final scratch = _WebHostScratch.allocate(module);
    var handleCreated = false;
    try {
      var status = module.callInt('flark_v3_wasm_host_config_standard', <int>[
        scratch.config,
      ]);
      _requireWebOk('configStandard', status);
      final memory = module.memoryData;
      if (_u32(memory, scratch.config) != _hostAbiVersion ||
          _u32(memory, scratch.config + 4) != _WebHostScratch.configBytes) {
        throw const FlarkV3WebHostException(
          operation: 'configLayout',
          status: 0x0100,
          detail: 'Dart and WebAssembly host ABI layouts differ',
        );
      }
      _writeId(memory, scratch.config + 8, documentSession);
      _setU32(memory, scratch.config + 24, grammarRevision);
      _setU32(
        memory,
        scratch.config + 28,
        (syntaxProfile ?? FlarkV3SyntaxProfileId(1)).value,
      );
      _setU32(
        memory,
        scratch.config + 32,
        (authorityMask ?? FlarkV3StructuralAuthorityMask.complete).bits,
      );
      status = module.callInt('flark_v3_wasm_host_create', <int>[
        scratch.config,
        scratch.handle,
      ]);
      _requireWebOk('create', status);
      handleCreated = true;
      final handleMemory = module.memoryData;
      final slot = _u32(handleMemory, scratch.handle);
      final generation = _u32(handleMemory, scratch.handle + 4);
      if (slot == 0 || generation == 0) {
        throw const FlarkV3WebHostException(
          operation: 'create',
          status: 0x0111,
          detail: 'WebAssembly host returned an invalid handle',
        );
      }
      final cleanup = _WebHostCleanup(
        module: module,
        scratchPointer: scratch.base,
        scratchBytes: scratch.bytes,
        slot: slot,
        generation: generation,
      );
      return FlarkV3WebHostStore._(
        module: module,
        scratch: scratch,
        cleanup: cleanup,
      );
    } catch (_) {
      if (handleCreated) {
        final memory = module.memoryData;
        final slot = _u32(memory, scratch.handle);
        final generation = _u32(memory, scratch.handle + 4);
        if (slot != 0 && generation != 0) {
          module.callInt('flark_v3_wasm_host_emergency_destroy', <int>[
            slot,
            generation,
          ]);
        }
      }
      scratch.free();
      rethrow;
    }
  }

  final FlarkV3WasmModule _module;
  final _WebHostScratch _scratch;
  final _WebHostCleanup _cleanup;
  FlarkV3SourceVersion? _currentSource;
  bool _released = false;

  int get _slot => _cleanup.slot;
  int get _generation => _cleanup.generation;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeSourceVersion(_module.memoryData, _scratch.source, sourceVersion);
    final result = _unitCall(
      'observeSource',
      'flark_v3_wasm_host_observe_source',
      <int>[_slot, _generation, _scratch.source, _scratch.callReceipt],
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
    _writeOffer(_module.memoryData, _scratch.begin, begin);
    switch (begin.mode) {
      case FlarkV3PublicationMode.fullSnapshot:
        if (begin.baseAck != null) return _invalidBeginMode();
        return _unitCall('beginOffer', 'flark_v3_wasm_host_begin_offer', <int>[
          _slot,
          _generation,
          _scratch.begin,
          _scratch.callReceipt,
        ]);
      case FlarkV3PublicationMode.exactBaseReferencesDelta:
        final base = begin.baseAck;
        if (base == null) return _invalidBeginMode();
        _writeAck(_module.memoryData, _scratch.ack, base);
        return _unitCall(
          'beginReferencesDelta',
          'flark_v3_wasm_host_begin_references_delta',
          <int>[
            _slot,
            _generation,
            _scratch.begin,
            _scratch.ack,
            _scratch.callReceipt,
          ],
        );
      case FlarkV3PublicationMode.exactBaseDelta:
        final base = begin.baseAck;
        if (base == null) return _invalidBeginMode();
        _writeAck(_module.memoryData, _scratch.ack, base);
        return _unitCall(
          'beginExactBaseDelta',
          'flark_v3_wasm_host_begin_exact_base_delta',
          <int>[
            _slot,
            _generation,
            _scratch.begin,
            _scratch.ack,
            _scratch.callReceipt,
          ],
        );
    }
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> _invalidBeginMode() =>
      const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.baseMismatch,
          'Publication mode does not bind the required exact base ACK.',
        ),
      );

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginInlineSidecarOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeInlineSidecarBegin(
      _module.memoryData,
      _scratch.inlineSidecarBegin,
      begin,
    );
    return _unitCall(
      'beginInlineSidecarOffer',
      'flark_v3_wasm_host_begin_inline_sidecar_offer',
      <int>[
        _slot,
        _generation,
        _scratch.inlineSidecarBegin,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeViewportPresentationBegin(
      _module.memoryData,
      _scratch.viewportPresentationBegin,
      begin,
    );
    return _unitCall(
      'beginViewportPresentationOffer',
      'flark_v3_wasm_host_begin_viewport_presentation_offer',
      <int>[
        _slot,
        _generation,
        _scratch.viewportPresentationBegin,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (packet.rawBytes.length > _bulkScratchBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Publication packet exceeds the Web bulk-scratch ceiling.',
        ),
      );
    }
    _module.writeBytes(_scratch.bulk, packet.rawBytes);
    return _unitCall('admitPacket', 'flark_v3_wasm_host_admit_packet', <int>[
      _slot,
      _generation,
      _scratch.bulk,
      packet.rawBytes.length,
      _scratch.callReceipt,
    ]);
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitInlineSidecarPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (packet.rawBytes.length > _bulkScratchBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Inline-sidecar packet exceeds the Web bulk-scratch ceiling.',
        ),
      );
    }
    _module.writeBytes(_scratch.bulk, packet.rawBytes);
    return _unitCall(
      'admitInlineSidecarPacket',
      'flark_v3_wasm_host_admit_inline_sidecar_packet',
      <int>[
        _slot,
        _generation,
        _scratch.bulk,
        packet.rawBytes.length,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (packet.rawBytes.length > _bulkScratchBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'VPB1 packet exceeds the Web bulk-scratch ceiling.',
        ),
      );
    }
    _module.writeBytes(_scratch.bulk, packet.rawBytes);
    return _unitCall(
      'admitViewportPresentationPacket',
      'flark_v3_wasm_host_admit_viewport_presentation_packet',
      <int>[
        _slot,
        _generation,
        _scratch.bulk,
        packet.rawBytes.length,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeCommit(_module.memoryData, _scratch.commit, request);
    return _unitCall(
      'requestCommit',
      'flark_v3_wasm_host_request_commit',
      <int>[_slot, _generation, _scratch.commit, _scratch.callReceipt],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestInlineSidecarCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeInlineSidecarCommit(
      _module.memoryData,
      _scratch.inlineSidecarCommit,
      request,
    );
    return _unitCall(
      'requestInlineSidecarCommit',
      'flark_v3_wasm_host_request_inline_sidecar_commit',
      <int>[
        _slot,
        _generation,
        _scratch.inlineSidecarCommit,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeViewportPresentationCommit(
      _module.memoryData,
      _scratch.viewportPresentationCommit,
      request,
    );
    return _unitCall(
      'requestViewportPresentationCommit',
      'flark_v3_wasm_host_request_viewport_presentation_commit',
      <int>[
        _slot,
        _generation,
        _scratch.viewportPresentationCommit,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeId(_module.memoryData, _scratch.id128, offerId);
    return _unitCall('abortOffer', 'flark_v3_wasm_host_abort_offer', <int>[
      _slot,
      _generation,
      _scratch.id128,
      _scratch.callReceipt,
    ]);
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortInlineSidecarOffer(
    FlarkV3OfferId offerId,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeId(_module.memoryData, _scratch.id128, offerId);
    return _unitCall(
      'abortInlineSidecarOffer',
      'flark_v3_wasm_host_abort_inline_sidecar_offer',
      <int>[_slot, _generation, _scratch.id128, _scratch.callReceipt],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeId(_module.memoryData, _scratch.id128, offerId);
    return _unitCall(
      'abortViewportPresentationOffer',
      'flark_v3_wasm_host_abort_viewport_presentation_offer',
      <int>[_slot, _generation, _scratch.id128, _scratch.callReceipt],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final unavailable = _unavailable<FlarkV3HostPollOutcome>();
    if (unavailable != null) return unavailable;
    final status = _module.callInt('flark_v3_wasm_host_poll', <int>[
      _slot,
      _generation,
      grant.inspectBytes,
      grant.copyBytes,
      grant.transitions,
      _scratch.pollReceipt,
    ]);
    final memory = _module.memoryData;
    final rejected = _rejection(
      operation: 'poll',
      status: status,
      reason: _u32(memory, _scratch.pollReceipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    final outcome = switch (_u32(memory, _scratch.pollReceipt + 4)) {
      0 => const FlarkV3HostPollPending(),
      1 => FlarkV3HostPacketCredit(
        offerId: _readOfferId(memory, _scratch.pollReceipt + 8),
        nextFrameOrdinal: _u32(memory, _scratch.pollReceipt + 24),
      ),
      2 => FlarkV3HostCommitted(_readAck(memory, _scratch.pollReceipt + 28)),
      3 => FlarkV3HostAbortComplete(
        _readOfferId(memory, _scratch.pollReceipt + 8),
      ),
      4 => const FlarkV3HostClosed(),
      final value => throw FlarkV3WebHostException(
        operation: 'pollOutcome',
        status: 0x0111,
        detail: 'unknown outcome $value',
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
    final status = _module
        .callInt('flark_v3_wasm_host_poll_inline_sidecar', <int>[
          _slot,
          _generation,
          grant.inspectBytes,
          grant.copyBytes,
          grant.transitions,
          _scratch.inlineSidecarPollReceipt,
        ]);
    final memory = _module.memoryData;
    final rejected = _rejection(
      operation: 'pollInlineSidecar',
      status: status,
      reason: _u32(memory, _scratch.inlineSidecarPollReceipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    final outcome = switch (_u32(
      memory,
      _scratch.inlineSidecarPollReceipt + 4,
    )) {
      0 => const FlarkV3InlineSidecarHostPollPending(),
      1 => FlarkV3InlineSidecarHostPacketCredit(
        offerId: _readOfferId(memory, _scratch.inlineSidecarPollReceipt + 8),
        nextFrameOrdinal: _u32(memory, _scratch.inlineSidecarPollReceipt + 24),
      ),
      2 => FlarkV3InlineSidecarHostCommitted(
        _readInlineSidecarAck(memory, _scratch.inlineSidecarPollReceipt + 28),
      ),
      3 => FlarkV3InlineSidecarHostAbortComplete(
        _readOfferId(memory, _scratch.inlineSidecarPollReceipt + 8),
      ),
      4 => const FlarkV3InlineSidecarHostClosed(),
      final value => throw FlarkV3WebHostException(
        operation: 'pollInlineSidecarOutcome',
        status: 0x0111,
        detail: 'unknown outcome $value',
      ),
    };
    // The structural poll is the only close/remove owner. A sidecar terminal
    // receipt must remain observable without invalidating the structural lane.
    return FlarkV3HostAccepted(outcome);
  }

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant) {
    final unavailable =
        _unavailable<FlarkV3ViewportPresentationHostPollOutcome>();
    if (unavailable != null) return unavailable;
    final status = _module
        .callInt('flark_v3_wasm_host_poll_viewport_presentation', <int>[
          _slot,
          _generation,
          grant.inspectBytes,
          grant.copyBytes,
          grant.transitions,
          _scratch.viewportPresentationPollReceipt,
        ]);
    final memory = _module.memoryData;
    final rejected = _rejection(
      operation: 'pollViewportPresentation',
      status: status,
      reason: _u32(memory, _scratch.viewportPresentationPollReceipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    final receipt = _scratch.viewportPresentationPollReceipt;
    final outcome = switch (_u32(memory, receipt + 4)) {
      0 => const FlarkV3ViewportPresentationHostPollPending(),
      1 => FlarkV3ViewportPresentationHostPacketCredit(
        offerId: _readOfferId(memory, receipt + 8),
        nextFrameOrdinal: _u32(memory, receipt + 24),
      ),
      2 => FlarkV3ViewportPresentationHostCommitted(
        _readViewportPresentationAck(memory, receipt + 28),
      ),
      3 => FlarkV3ViewportPresentationHostAbortComplete(
        _readOfferId(memory, receipt + 8),
      ),
      4 => const FlarkV3ViewportPresentationHostClosed(),
      final value => throw FlarkV3WebHostException(
        operation: 'pollViewportPresentationOutcome',
        status: 0x0111,
        detail: 'unknown outcome $value',
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
    _writeAck(_module.memoryData, _scratch.ack, ack);
    return _unitCall(
      'acknowledgeDelivery',
      'flark_v3_wasm_host_acknowledge_delivery',
      <int>[_slot, _generation, _scratch.ack, _scratch.callReceipt],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeInlineSidecarDelivery(
    FlarkV3InlineSidecarAck ack,
  ) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeInlineSidecarAck(_module.memoryData, _scratch.inlineSidecarAck, ack);
    return _unitCall(
      'acknowledgeInlineSidecarDelivery',
      'flark_v3_wasm_host_acknowledge_inline_sidecar_delivery',
      <int>[
        _slot,
        _generation,
        _scratch.inlineSidecarAck,
        _scratch.callReceipt,
      ],
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack) {
    final unavailable = _unavailable<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    _writeViewportPresentationAck(
      _module.memoryData,
      _scratch.viewportPresentationAck,
      ack,
    );
    return _unitCall(
      'acknowledgeViewportPresentationDelivery',
      'flark_v3_wasm_host_acknowledge_viewport_presentation_delivery',
      <int>[
        _slot,
        _generation,
        _scratch.viewportPresentationAck,
        _scratch.callReceipt,
      ],
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
          'Query does not bind the Web host source authority.',
        ),
      );
    }
    if (query.budget.maxEncodedBytes > _maximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Query copy exceeds the Web host scratch bound.',
        ),
      );
    }

    _writePointQuery(_module.memoryData, _scratch.pointQuery, query);
    final status = _module.callInt('flark_v3_wasm_host_query_structural', <int>[
      _slot,
      _generation,
      _scratch.pointQuery,
      _scratch.bulk,
      query.budget.maxEncodedBytes,
      _scratch.pointQueryReceipt,
    ]);
    final memory = _module.memoryData;
    final rejected = _rejection(
      operation: 'queryStructural',
      status: status,
      reason: _u32(memory, _scratch.pointQueryReceipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    return FlarkV3HostAccepted(
      _decodePointQueryOutcome(
        query: query,
        memory: memory,
        receiptOffset: _scratch.pointQueryReceipt,
        outputOffset: _scratch.bulk,
        module: _module,
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
          'Range query does not bind the Web host source authority.',
        ),
      );
    }
    if (query.budget.maxEncodedBytes > _maximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Range-query copy exceeds the Web host scratch bound.',
        ),
      );
    }

    _writeBlockRangeQuery(_module.memoryData, _scratch.blockRangeQuery, query);
    final status = _module
        .callInt('flark_v3_wasm_host_query_structural_range', <int>[
          _slot,
          _generation,
          _scratch.blockRangeQuery,
          _scratch.bulk,
          query.budget.maxEncodedBytes,
          _scratch.blockRangeQueryReceipt,
        ]);
    final memory = _module.memoryData;
    final rejected = _rejection(
      operation: 'queryStructuralRange',
      status: status,
      reason: _u32(memory, _scratch.blockRangeQueryReceipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    return FlarkV3HostAccepted(
      _decodeBlockRangeQueryOutcome(
        query: query,
        memory: memory,
        receiptOffset: _scratch.blockRangeQueryReceipt,
        outputOffset: _scratch.bulk,
        module: _module,
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
          'Ordinal query does not bind the Web host source authority.',
        ),
      );
    }

    _writeStructuralOrdinalWindowQuery(
      _module.memoryData,
      _scratch.structuralOrdinalWindowQuery,
      query,
    );
    final status = _module
        .callInt('flark_v3_wasm_host_query_structural_ordinal_window', <int>[
          _slot,
          _generation,
          _scratch.structuralOrdinalWindowQuery,
          _scratch.structuralOrdinalWindowReceipt,
        ]);
    final memory = _module.memoryData;
    final receipt = _scratch.structuralOrdinalWindowReceipt;
    _validateStructuralOrdinalWindowReserved(memory, receipt);
    final rejected = _rejection(
      operation: 'queryStructuralOrdinalWindow',
      status: status,
      reason: _u32(memory, receipt),
    );
    if (rejected != null) {
      if (!_structuralOrdinalWindowRejectedBodyIsCanonical(memory, receipt)) {
        throw const FlarkV3WebHostException(
          operation: 'queryStructuralOrdinalWindowReceipt',
          status: 0x0111,
          detail: 'Web host returned a noncanonical rejected ordinal receipt',
        );
      }
      return FlarkV3HostRejected(rejected);
    }
    return FlarkV3HostAccepted(
      _decodeStructuralOrdinalWindowOutcome(
        query: query,
        memory: memory,
        receiptOffset: receipt,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> queryInlineSidecar(
    FlarkV3InlineSidecarQuery query,
  ) {
    final unavailable = _unavailable<FlarkV3InlineSidecarQueryOutcome>();
    if (unavailable != null) return unavailable;
    if (query.maximumEncodedBytes > _inlineSidecarMaximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Inline-sidecar query exceeds the Web host scratch bound.',
        ),
      );
    }
    _writeInlineSidecarQuery(
      _module.memoryData,
      _scratch.inlineSidecarQuery,
      query,
    );
    final status = _module
        .callInt('flark_v3_wasm_host_query_inline_sidecar', <int>[
          _slot,
          _generation,
          _scratch.inlineSidecarQuery,
          _scratch.bulk,
          query.maximumEncodedBytes,
          _scratch.inlineSidecarQueryReceipt,
        ]);
    final memory = _module.memoryData;
    final rejected = _rejection(
      operation: 'queryInlineSidecar',
      status: status,
      reason: _u32(memory, _scratch.inlineSidecarQueryReceipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    return FlarkV3HostAccepted(
      _decodeInlineSidecarQueryOutcome(
        query: query,
        memory: memory,
        receiptOffset: _scratch.inlineSidecarQueryReceipt,
        outputOffset: _scratch.bulk,
        module: _module,
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query) {
    final unavailable = _unavailable<FlarkV3ViewportPresentationQueryOutcome>();
    if (unavailable != null) return unavailable;
    if (query.maximumEncodedBytes > _viewportMaximumQueryBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'VPB1 query copy exceeds the Web host scratch bound.',
        ),
      );
    }
    _writeViewportPresentationQuery(
      _module.memoryData,
      _scratch.viewportPresentationQuery,
      query,
    );
    final status = _module
        .callInt('flark_v3_wasm_host_query_viewport_presentation', <int>[
          _slot,
          _generation,
          _scratch.viewportPresentationQuery,
          _scratch.bulk,
          query.maximumEncodedBytes,
          _scratch.viewportPresentationQueryReceipt,
        ]);
    final memory = _module.memoryData;
    final receipt = _scratch.viewportPresentationQueryReceipt;
    final rejected = _rejection(
      operation: 'queryViewportPresentation',
      status: status,
      reason: _u32(memory, receipt),
    );
    if (rejected != null) return FlarkV3HostRejected(rejected);
    for (var reserved = 16; reserved < 32; reserved += 4) {
      if (_u32(memory, receipt + reserved) != 0) {
        throw const FlarkV3WebHostException(
          operation: 'queryViewportPresentationReceipt',
          status: 0x0111,
          detail: 'Web host returned nonzero reserved VPB1 query fields',
        );
      }
    }
    final rawOutcome = _u32(memory, receipt + 4);
    final encodedBytes = _u32(memory, receipt + 8);
    final entryCount = _u32(memory, receipt + 12);
    final outcome = switch (rawOutcome) {
      0 when encodedBytes == 0 && entryCount == 0 =>
        const FlarkV3ViewportPresentationQueryUnavailable(),
      1
          when encodedBytes > 0 &&
              encodedBytes <= query.maximumEncodedBytes &&
              entryCount == query.ack.envelope.orderedLeafCount =>
        FlarkV3ViewportPresentationQueryAvailable(
          FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
            ack: query.ack,
            encodedPage: _module.readBytes(_scratch.bulk, encodedBytes),
          ),
        ),
      _ => throw FlarkV3WebHostException(
        operation: 'queryViewportPresentationReceipt',
        status: 0x0111,
        detail:
            'Web host returned invalid VPB1 outcome $rawOutcome, '
            'bytes $encodedBytes, entries $entryCount',
      ),
    };
    return FlarkV3HostAccepted(outcome);
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    if (_released) return const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);
    return _unitCall('close', 'flark_v3_wasm_host_close', <int>[
      _slot,
      _generation,
      _scratch.callReceipt,
    ]);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> _unitCall(
    String operation,
    String export,
    List<int> arguments,
  ) {
    final status = _module.callInt(export, arguments);
    final rejected = _rejection(
      operation: operation,
      status: status,
      reason: _u32(_module.memoryData, _scratch.callReceipt),
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
      _requireWebOk(operation, status);
      return null;
    }
    if (status == _statusOk) {
      throw FlarkV3WebHostException(
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
      5 => throw FlarkV3WebHostException(
        operation: operation,
        status: status,
        detail: 'Web host was unexpectedly not ready',
      ),
      6 => FlarkV3HostRejectReason.baseMismatch,
      7 => FlarkV3HostRejectReason.wrongOffer,
      8 => FlarkV3HostRejectReason.corruptPublication,
      9 => FlarkV3HostRejectReason.queryBoundExceeded,
      10 => FlarkV3HostRejectReason.foregroundBoundExceeded,
      11 => FlarkV3HostRejectReason.superseded,
      12 => FlarkV3HostRejectReason.closed,
      _ => throw FlarkV3WebHostException(
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
            'The Web host has been reclaimed.',
          ),
        )
      : null;

  void _releaseNormally() {
    if (_released) return;
    _webHostFinalizer.detach(this);
    final removeStatus = _module.callInt('flark_v3_wasm_host_remove', <int>[
      _slot,
      _generation,
    ]);
    final reclamationStatus = removeStatus == _statusOk
        ? _statusOk
        : _module.callInt('flark_v3_wasm_host_emergency_destroy', <int>[
            _slot,
            _generation,
          ]);
    if (reclamationStatus != _statusOk) {
      _webHostFinalizer.attach(this, _cleanup, detach: this);
      throw FlarkV3WebHostException(
        operation: 'remove',
        status: removeStatus,
        detail:
            'emergency fallback status='
            '0x${reclamationStatus.toRadixString(16)}',
      );
    }
    try {
      _cleanup.releaseScratch();
    } catch (error, stackTrace) {
      // The host handle is already generation-safely removed, but the exact
      // scratch allocation still needs an owner if its explicit free failed.
      _webHostFinalizer.attach(this, _cleanup, detach: this);
      Error.throwWithStackTrace(error, stackTrace);
    }
    _released = true;
  }
}

Future<FlarkV3WasmModule> _loadSharedHostModule(Uri uri) {
  final existing = _sharedHostModules[uri];
  if (existing != null) {
    // A completed Future retains the Zone in which it was created. Cache the
    // resource instead and return a caller-zone Future so a long-lived host
    // module cannot strand a later owner on a retired event loop.
    return Future<FlarkV3WasmModule>.value(existing);
  }
  final pending = _sharedHostModuleLoads[uri];
  if (pending != null) return pending;

  late final Future<FlarkV3WasmModule> promoted;
  final load = FlarkV3WasmModule.load(
    uri,
    requiredExports: _requiredHostExports,
  );
  promoted = load.then<FlarkV3WasmModule>(
    (module) {
      if (identical(_sharedHostModuleLoads[uri], promoted)) {
        _sharedHostModules[uri] = module;
        _sharedHostModuleLoads.remove(uri);
      }
      return module;
    },
    onError: (Object error, StackTrace stackTrace) {
      if (identical(_sharedHostModuleLoads[uri], promoted)) {
        _sharedHostModuleLoads.remove(uri);
      }
      Error.throwWithStackTrace(error, stackTrace);
    },
  );
  _sharedHostModuleLoads[uri] = promoted;
  return promoted;
}

final class _WebHostCleanup {
  _WebHostCleanup({
    required this.module,
    required this.scratchPointer,
    required this.scratchBytes,
    required this.slot,
    required this.generation,
  });

  final FlarkV3WasmModule module;
  final int scratchPointer;
  final int scratchBytes;
  final int slot;
  final int generation;
  bool _released = false;

  void emergencyRelease() {
    if (_released) return;
    try {
      module.callInt('flark_v3_wasm_host_emergency_destroy', <int>[
        slot,
        generation,
      ]);
    } on Object {
      // A finalizer cannot report into an already unreachable document. The
      // generation-checked handle prevents any later alias if Wasm is still
      // live; explicit close remains the truthful reclamation path.
    }
    try {
      releaseScratch();
    } on Object {
      // Best effort only during abandonment.
    }
  }

  void releaseScratch() {
    if (_released) return;
    module.free(scratchPointer, scratchBytes);
    _released = true;
  }
}

final class _WebHostScratch {
  const _WebHostScratch._({required this.module, required this.base});

  static const int configBytes = 56;
  static const int _handleBytes = 8;
  static const int _sourceBytes = 44;
  static const int _beginBytes = 144;
  static const int _commitBytes = 56;
  static const int _ackBytes = 124;
  static const int _inlineSidecarBeginBytes = 364;
  static const int _inlineSidecarCommitBytes = 56;
  static const int _inlineSidecarAckBytes = 212;
  static const int _idBytes = 16;
  static const int _callReceiptBytes = 4;
  static const int _pollReceiptBytes = 152;
  static const int _inlineSidecarPollReceiptBytes = 240;
  static const int _pointQueryBytes = 96;
  static const int _pointQueryReceiptBytes = 112;
  static const int _blockRangeQueryBytes = 172;
  static const int _blockRangeQueryReceiptBytes = 188;
  static const int _structuralOrdinalWindowQueryBytes = 92;
  static const int _structuralOrdinalWindowReceiptBytes = 132;
  static const int _inlineSidecarQueryBytes = 80;
  static const int _inlineSidecarQueryReceiptBytes = 36;
  static const int _viewportPresentationBeginBytes = 348;
  static const int _viewportPresentationCommitBytes = 56;
  static const int _viewportPresentationAckBytes = 296;
  static const int _viewportPresentationPollReceiptBytes = 324;
  static const int _viewportPresentationQueryBytes = 320;
  static const int _viewportPresentationQueryReceiptBytes = 32;
  static const int _sourcePaddedBytes = (_sourceBytes + 7) & ~7;
  static const int _ackPaddedBytes = (_ackBytes + 7) & ~7;
  static const int _inlineSidecarBeginPaddedBytes =
      (_inlineSidecarBeginBytes + 7) & ~7;
  static const int _inlineSidecarAckPaddedBytes =
      (_inlineSidecarAckBytes + 7) & ~7;
  static const int _callReceiptPaddedBytes = (_callReceiptBytes + 7) & ~7;
  static const int _blockRangeQueryPaddedBytes =
      (_blockRangeQueryBytes + 7) & ~7;
  static const int _blockRangeQueryReceiptPaddedBytes =
      (_blockRangeQueryReceiptBytes + 7) & ~7;
  static const int _structuralOrdinalWindowQueryPaddedBytes =
      (_structuralOrdinalWindowQueryBytes + 7) & ~7;
  static const int _structuralOrdinalWindowReceiptPaddedBytes =
      (_structuralOrdinalWindowReceiptBytes + 7) & ~7;
  static const int _inlineSidecarQueryReceiptPaddedBytes =
      (_inlineSidecarQueryReceiptBytes + 7) & ~7;
  static const int _viewportPresentationBeginPaddedBytes =
      (_viewportPresentationBeginBytes + 7) & ~7;
  static const int _viewportPresentationPollReceiptPaddedBytes =
      (_viewportPresentationPollReceiptBytes + 7) & ~7;

  static const int _configOffset = 0;
  static const int _handleOffset = _configOffset + configBytes;
  static const int _sourceOffset = _handleOffset + _handleBytes;
  static const int _beginOffset = _sourceOffset + _sourcePaddedBytes;
  static const int _commitOffset = _beginOffset + _beginBytes;
  static const int _ackOffset = _commitOffset + _commitBytes;
  static const int _inlineSidecarBeginOffset = _ackOffset + _ackPaddedBytes;
  static const int _inlineSidecarCommitOffset =
      _inlineSidecarBeginOffset + _inlineSidecarBeginPaddedBytes;
  static const int _inlineSidecarAckOffset =
      _inlineSidecarCommitOffset + _inlineSidecarCommitBytes;
  static const int _idOffset =
      _inlineSidecarAckOffset + _inlineSidecarAckPaddedBytes;
  static const int _callReceiptOffset = _idOffset + _idBytes;
  static const int _pollReceiptOffset =
      _callReceiptOffset + _callReceiptPaddedBytes;
  static const int _inlineSidecarPollReceiptOffset =
      _pollReceiptOffset + _pollReceiptBytes;
  static const int _pointQueryOffset =
      _inlineSidecarPollReceiptOffset + _inlineSidecarPollReceiptBytes;
  static const int _pointQueryReceiptOffset =
      _pointQueryOffset + _pointQueryBytes;
  static const int _blockRangeQueryOffset =
      _pointQueryReceiptOffset + _pointQueryReceiptBytes;
  static const int _blockRangeQueryReceiptOffset =
      _blockRangeQueryOffset + _blockRangeQueryPaddedBytes;
  static const int _structuralOrdinalWindowQueryOffset =
      _blockRangeQueryReceiptOffset + _blockRangeQueryReceiptPaddedBytes;
  static const int _structuralOrdinalWindowReceiptOffset =
      _structuralOrdinalWindowQueryOffset +
      _structuralOrdinalWindowQueryPaddedBytes;
  static const int _inlineSidecarQueryOffset =
      _structuralOrdinalWindowReceiptOffset +
      _structuralOrdinalWindowReceiptPaddedBytes;
  static const int _inlineSidecarQueryReceiptOffset =
      _inlineSidecarQueryOffset + _inlineSidecarQueryBytes;
  static const int _viewportPresentationBeginOffset =
      _inlineSidecarQueryReceiptOffset + _inlineSidecarQueryReceiptPaddedBytes;
  static const int _viewportPresentationCommitOffset =
      _viewportPresentationBeginOffset + _viewportPresentationBeginPaddedBytes;
  static const int _viewportPresentationAckOffset =
      _viewportPresentationCommitOffset + _viewportPresentationCommitBytes;
  static const int _viewportPresentationPollReceiptOffset =
      _viewportPresentationAckOffset + _viewportPresentationAckBytes;
  static const int _viewportPresentationQueryOffset =
      _viewportPresentationPollReceiptOffset +
      _viewportPresentationPollReceiptPaddedBytes;
  static const int _viewportPresentationQueryReceiptOffset =
      _viewportPresentationQueryOffset + _viewportPresentationQueryBytes;
  static const int _bulkOffset =
      _viewportPresentationQueryReceiptOffset +
      _viewportPresentationQueryReceiptBytes;
  static const int allocationBytes = _bulkOffset + _bulkScratchBytes;

  static _WebHostScratch allocate(FlarkV3WasmModule module) =>
      _WebHostScratch._(module: module, base: module.allocate(allocationBytes));

  final FlarkV3WasmModule module;
  final int base;

  int get bytes => allocationBytes;
  int get config => base + _configOffset;
  int get handle => base + _handleOffset;
  int get source => base + _sourceOffset;
  int get begin => base + _beginOffset;
  int get commit => base + _commitOffset;
  int get ack => base + _ackOffset;
  int get inlineSidecarBegin => base + _inlineSidecarBeginOffset;
  int get inlineSidecarCommit => base + _inlineSidecarCommitOffset;
  int get inlineSidecarAck => base + _inlineSidecarAckOffset;
  int get id128 => base + _idOffset;
  int get callReceipt => base + _callReceiptOffset;
  int get pollReceipt => base + _pollReceiptOffset;
  int get inlineSidecarPollReceipt => base + _inlineSidecarPollReceiptOffset;
  int get pointQuery => base + _pointQueryOffset;
  int get pointQueryReceipt => base + _pointQueryReceiptOffset;
  int get blockRangeQuery => base + _blockRangeQueryOffset;
  int get blockRangeQueryReceipt => base + _blockRangeQueryReceiptOffset;
  int get structuralOrdinalWindowQuery =>
      base + _structuralOrdinalWindowQueryOffset;
  int get structuralOrdinalWindowReceipt =>
      base + _structuralOrdinalWindowReceiptOffset;
  int get inlineSidecarQuery => base + _inlineSidecarQueryOffset;
  int get inlineSidecarQueryReceipt => base + _inlineSidecarQueryReceiptOffset;
  int get viewportPresentationBegin => base + _viewportPresentationBeginOffset;
  int get viewportPresentationCommit =>
      base + _viewportPresentationCommitOffset;
  int get viewportPresentationAck => base + _viewportPresentationAckOffset;
  int get viewportPresentationPollReceipt =>
      base + _viewportPresentationPollReceiptOffset;
  int get viewportPresentationQuery => base + _viewportPresentationQueryOffset;
  int get viewportPresentationQueryReceipt =>
      base + _viewportPresentationQueryReceiptOffset;
  int get bulk => base + _bulkOffset;

  void free() => module.free(base, allocationBytes);
}

String _rejectionMessage(FlarkV3HostRejectReason reason) => switch (reason) {
  FlarkV3HostRejectReason.invalid => 'The Web host rejected invalid state.',
  FlarkV3HostRejectReason.backpressure =>
    'The Web host still owns the prior bounded operation.',
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
    'The operation exceeded the main-context work envelope.',
  FlarkV3HostRejectReason.superseded =>
    'A newer exact source superseded the operation.',
  FlarkV3HostRejectReason.closed => 'The Web host is closing or closed.',
};

void _requireWebOk(String operation, int status) {
  if (status != _statusOk) {
    throw FlarkV3WebHostException(operation: operation, status: status);
  }
}

int _u32(ByteData memory, int offset) =>
    memory.getUint32(offset, Endian.little);

void _setU32(ByteData memory, int offset, int value) {
  memory.setUint32(offset, value, Endian.little);
}

void _writeId(ByteData memory, int offset, FlarkV3ProtocolId128 value) {
  _setU32(memory, offset, value.word0);
  _setU32(memory, offset + 4, value.word1);
  _setU32(memory, offset + 8, value.word2);
  _setU32(memory, offset + 12, value.word3);
}

void _writeSourceVersion(
  ByteData memory,
  int offset,
  FlarkV3SourceVersion source,
) {
  _writeId(memory, offset, source.documentSession);
  _setU32(memory, offset + 16, source.revision);
  _setU32(memory, offset + 20, source.metric.bytes);
  _setU32(memory, offset + 24, source.metric.utf16);
  _setU32(memory, offset + 28, source.contentHash.word0);
  _setU32(memory, offset + 32, source.contentHash.word1);
  _setU32(memory, offset + 36, source.contentHash.word2);
  _setU32(memory, offset + 40, source.contentHash.word3);
}

void _writeOffer(ByteData memory, int offset, FlarkV3HostOfferBegin begin) {
  _writeId(memory, offset, begin.offerId);
  _writeId(memory, offset + 16, begin.publicationSession);
  _setU32(memory, offset + 32, begin.targetHostRevision.value);
  _writeSourceVersion(memory, offset + 36, begin.sourceVersion);
  _setU32(memory, offset + 80, begin.sourceRoot.highWord);
  _setU32(memory, offset + 84, begin.sourceRoot.lowWord);
  _setU32(memory, offset + 88, begin.parseGeneration);
  _setU32(memory, offset + 92, begin.grammarRevision);
  _setU32(memory, offset + 96, begin.syntaxProfile.value);
  _setU32(memory, offset + 100, begin.authorityMask.bits);
  _setU32(memory, offset + 104, begin.transferredRecordCount);
  _setU32(memory, offset + 108, begin.targetRecordCount);
  _setU32(memory, offset + 112, begin.limits.maximumFrameCount);
  _setU32(memory, offset + 116, begin.limits.maximumEncodedFrameBytes);
  _setU32(memory, offset + 120, begin.limits.maximumPacketBytes);
  _setU32(memory, offset + 124, begin.limits.maximumFrameBytes);
  _setU32(memory, offset + 128, begin.limits.maximumProgramChildren);
  for (var reserved = 132; reserved < 144; reserved += 4) {
    _setU32(memory, offset + reserved, 0);
  }
}

void _writeCommit(
  ByteData memory,
  int offset,
  FlarkV3HostCommitRequest request,
) {
  _writeId(memory, offset, request.offerId);
  _setU32(memory, offset + 16, request.actualFrameCount);
  _setU32(memory, offset + 20, request.actualEncodedFrameBytes);
  _writeId(memory, offset + 24, request.rollingTransportDigest);
  _writeId(memory, offset + 40, request.canonicalStreamDigest);
}

void _writeAck(ByteData memory, int offset, FlarkV3StructuralAck ack) {
  _writeId(memory, offset, ack.publicationSession);
  _setU32(memory, offset + 16, ack.hostRevision.value);
  _writeSourceVersion(memory, offset + 20, ack.sourceVersion);
  _setU32(memory, offset + 64, ack.sourceRoot.highWord);
  _setU32(memory, offset + 68, ack.sourceRoot.lowWord);
  _setU32(memory, offset + 72, ack.parseGeneration);
  _setU32(memory, offset + 76, ack.grammarRevision);
  _setU32(memory, offset + 80, ack.syntaxProfile.value);
  _setU32(memory, offset + 84, ack.authorityMask.bits);
  _setU32(memory, offset + 88, ack.recordCount);
  _writeId(memory, offset + 92, ack.sequenceDigest);
  _writeId(memory, offset + 108, ack.manifestDigest);
}

void _writeU64(ByteData memory, int offset, FlarkV3ProtocolU64 value) {
  _setU32(memory, offset, value.lowWord);
  _setU32(memory, offset + 4, value.highWord);
}

void _writeDigest256(
  ByteData memory,
  int offset,
  FlarkV3ProtocolDigest256 digest,
) {
  _setU32(memory, offset, digest.word0);
  _setU32(memory, offset + 4, digest.word1);
  _setU32(memory, offset + 8, digest.word2);
  _setU32(memory, offset + 12, digest.word3);
  _setU32(memory, offset + 16, digest.word4);
  _setU32(memory, offset + 20, digest.word5);
  _setU32(memory, offset + 24, digest.word6);
  _setU32(memory, offset + 28, digest.word7);
}

void _writeInlineSidecarBinding(
  ByteData memory,
  int offset,
  FlarkV3HotInlineSidecarBinding binding,
) {
  _writeU64(
    memory,
    offset,
    FlarkV3ProtocolU64.fromU32(binding.parserProfile.value),
  );
  _writeU64(memory, offset + 8, binding.refinementGeneration);
  _writeU64(memory, offset + 16, binding.blockOrdinal);
  _setU32(memory, offset + 24, binding.physicalStartUtf8);
  _setU32(memory, offset + 28, binding.physicalEndUtf8);
  _setU32(memory, offset + 32, binding.visibleStartUtf8);
  _setU32(memory, offset + 36, binding.visibleEndUtf8);
  _setU32(memory, offset + 40, binding.physicalStartUtf16);
  _setU32(memory, offset + 44, binding.physicalEndUtf16);
  _setU32(memory, offset + 48, binding.visibleStartUtf16);
  _setU32(memory, offset + 52, binding.visibleEndUtf16);
}

void _writeInlineSidecarBegin(
  ByteData memory,
  int offset,
  FlarkV3HotInlineSidecarOfferBegin begin,
) {
  _setU32(memory, offset, begin.schema);
  _setU32(memory, offset + 4, 1);
  _writeId(memory, offset + 8, begin.offerId);
  _writeId(memory, offset + 24, begin.publicationSession);
  _writeAck(memory, offset + 40, begin.baseAck);
  _writeInlineSidecarBinding(memory, offset + 164, begin.binding);
  _setU32(memory, offset + 220, begin.envelope.hio1EncodedBytes);
  _setU32(memory, offset + 224, begin.envelope.ipr2DescriptorBytes);
  _setU32(memory, offset + 228, begin.envelope.transferredNodeCount);
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
      _setU32(memory, offset + 232, 1);
      _setU32(memory, offset + 236, 0);
      _writeU64(memory, offset + 240, logicalPageCount);
      _writeU64(memory, offset + 248, factCount);
      _writeU64(memory, offset + 256, storagePageCount);
      _setU32(memory, offset + 264, linkValueEntryCount);
      _setU32(memory, offset + 268, linkValueEncodedBytes);
      _writeU64(memory, offset + 272, linkValueStoragePageCount);
      _writeDigest256(memory, offset + 280, orderedCommitment256);
    case FlarkV3HotInlineSidecarUnsupported(
      :final reason,
      :final metadataCommitment256,
    ):
      _setU32(memory, offset + 232, 2);
      _setU32(memory, offset + 236, reason);
      _writeU64(memory, offset + 240, FlarkV3ProtocolU64.zero);
      _writeU64(memory, offset + 248, FlarkV3ProtocolU64.zero);
      _writeU64(memory, offset + 256, FlarkV3ProtocolU64.zero);
      _setU32(memory, offset + 264, 0);
      _setU32(memory, offset + 268, 0);
      _writeU64(memory, offset + 272, FlarkV3ProtocolU64.zero);
      _writeDigest256(memory, offset + 280, metadataCommitment256);
  }
  _writeDigest256(memory, offset + 312, begin.envelope.hio1EnvelopeDigest256);
  _setU32(memory, offset + 344, begin.limits.maximumFrameCount);
  _setU32(memory, offset + 348, begin.limits.maximumEncodedFrameBytes);
  _setU32(memory, offset + 352, begin.limits.maximumPacketBytes);
  _setU32(memory, offset + 356, begin.limits.maximumFrameBytes);
  _setU32(memory, offset + 360, begin.limits.maximumProgramChildren);
}

void _writeInlineSidecarCommit(
  ByteData memory,
  int offset,
  FlarkV3HotInlineSidecarCommitRequest request,
) {
  _writeId(memory, offset, request.offerId);
  _setU32(memory, offset + 16, request.actualFrameCount);
  _setU32(memory, offset + 20, request.actualEncodedFrameBytes);
  _writeId(memory, offset + 24, request.rollingTransportDigest);
  _writeId(memory, offset + 40, request.rootStreamDigest);
}

void _writeInlineSidecarAck(
  ByteData memory,
  int offset,
  FlarkV3InlineSidecarAck ack,
) {
  _writeId(memory, offset, ack.publicationSession);
  _writeAck(memory, offset + 16, ack.baseAck);
  _writeU64(memory, offset + 140, ack.refinementGeneration);
  _writeU64(memory, offset + 148, ack.blockOrdinal);
  _setU32(memory, offset + 156, ack.transferredNodeCount);
  _setU32(memory, offset + 160, ack.disposition.index + 1);
  _writeDigest256(memory, offset + 164, ack.hio1EnvelopeDigest256);
  _writeId(memory, offset + 196, ack.rootStreamDigest);
}

void _writeInlineSidecarQuery(
  ByteData memory,
  int offset,
  FlarkV3InlineSidecarQuery query,
) {
  _setU32(memory, offset, _inlineSidecarQuerySchema);
  _setU32(memory, offset + 4, _WebHostScratch._inlineSidecarQueryBytes);
  _writeInlineSidecarBinding(memory, offset + 8, query.binding);
  _setU32(memory, offset + 64, query.maximumEncodedBytes);
  for (var reserved = 68; reserved < 80; reserved += 4) {
    _setU32(memory, offset + reserved, 0);
  }
}

void _writeViewportPresentationRange(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationMetricRange range,
) {
  _setU32(memory, offset, range.startUtf8);
  _setU32(memory, offset + 4, range.startUtf16);
  _setU32(memory, offset + 8, range.endUtf8);
  _setU32(memory, offset + 12, range.endUtf16);
}

void _writeViewportPresentationVisitStart(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationVisitStart start,
) {
  _writeU64(memory, offset, start.blockOrdinal);
  _setU32(memory, offset + 8, start.utf8Offset);
  _setU32(memory, offset + 12, start.utf16Offset);
}

void _writeViewportPresentationBinding(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationBinding binding,
) {
  _setU32(memory, offset, binding.viewportGeneration);
  _writeViewportPresentationRange(memory, offset + 4, binding.requestedRange);
  _writeViewportPresentationRange(memory, offset + 20, binding.coveredRange);
  _writeViewportPresentationVisitStart(memory, offset + 36, binding.start);
  _writeViewportPresentationVisitStart(memory, offset + 52, binding.next);
  _setU32(memory, offset + 68, binding.complete ? 1 : 0);
}

void _writeViewportPresentationEnvelope(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationEnvelopeMetrics envelope,
) {
  _setU32(memory, offset, envelope.visitedStructuralEntries);
  _setU32(memory, offset + 4, envelope.visitedStoragePages);
  _setU32(memory, offset + 8, envelope.orderedLeafCount);
  _setU32(memory, offset + 12, envelope.inlineSourceBytes);
  _setU32(memory, offset + 16, envelope.factCount);
  _setU32(memory, offset + 20, envelope.transferredNodeCount);
  _setU32(memory, offset + 24, envelope.parserTransitions);
  _writeDigest256(memory, offset + 28, envelope.aggregateEnvelopeDigest256);
}

void _writeViewportPresentationQueryLimits(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationQueryLimits limits,
) {
  _setU32(memory, offset, limits.maximumStructuralEntries);
  _setU32(memory, offset + 4, limits.maximumStoragePages);
  _setU32(memory, offset + 8, limits.maximumInlineLeaves);
  _setU32(memory, offset + 12, limits.maximumInlineLeafSourceBytes);
  _setU32(memory, offset + 16, limits.maximumInlineSourceBytes);
  _setU32(memory, offset + 20, limits.maximumFactRecords);
  _setU32(memory, offset + 24, limits.maximumEncodedFrameBytes);
  _setU32(memory, offset + 28, limits.maximumParserTransitions);
}

void _writeViewportPresentationOfferLimits(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationOfferLimits limits,
) {
  _setU32(memory, offset, limits.maximumFrameCount);
  _setU32(memory, offset + 4, limits.maximumEncodedFrameBytes);
  _setU32(memory, offset + 8, limits.maximumPacketBytes);
  _setU32(memory, offset + 12, limits.maximumFrameBytes);
  _setU32(memory, offset + 16, limits.maximumProgramChildren);
}

void _writeViewportPresentationBegin(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationOfferBegin begin,
) {
  _setU32(memory, offset, begin.schema);
  _setU32(memory, offset + 4, 1);
  _writeId(memory, offset + 8, begin.offerId);
  _writeId(memory, offset + 24, begin.publicationSession);
  _writeAck(memory, offset + 40, begin.baseAck);
  _writeViewportPresentationBinding(memory, offset + 164, begin.binding);
  _writeViewportPresentationEnvelope(memory, offset + 236, begin.envelope);
  _writeViewportPresentationQueryLimits(
    memory,
    offset + 296,
    begin.queryLimits,
  );
  _writeViewportPresentationOfferLimits(memory, offset + 328, begin.limits);
}

void _writeViewportPresentationCommit(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationCommitRequest request,
) {
  _writeId(memory, offset, request.offerId);
  _setU32(memory, offset + 16, request.actualFrameCount);
  _setU32(memory, offset + 20, request.actualEncodedFrameBytes);
  _writeId(memory, offset + 24, request.rollingTransportDigest);
  _writeId(memory, offset + 40, request.aggregateRootStreamDigest);
}

void _writeViewportPresentationAck(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationAck ack,
) {
  _writeId(memory, offset, ack.publicationSession);
  _writeAck(memory, offset + 16, ack.baseAck);
  _writeViewportPresentationBinding(memory, offset + 140, ack.binding);
  _writeViewportPresentationEnvelope(memory, offset + 212, ack.envelope);
  _setU32(memory, offset + 272, ack.actualFrameCount);
  _setU32(memory, offset + 276, ack.actualEncodedFrameBytes);
  _writeId(memory, offset + 280, ack.aggregateRootStreamDigest);
}

void _writeViewportPresentationQuery(
  ByteData memory,
  int offset,
  FlarkV3ViewportPresentationQuery query,
) {
  _setU32(memory, offset, 1);
  _setU32(memory, offset + 4, _WebHostScratch._viewportPresentationQueryBytes);
  _writeViewportPresentationAck(memory, offset + 8, query.ack);
  _setU32(memory, offset + 304, query.maximumEncodedBytes);
  for (var reserved = 308; reserved < 320; reserved += 4) {
    _setU32(memory, offset + reserved, 0);
  }
}

void _writePointQuery(
  ByteData memory,
  int offset,
  FlarkV3HostPointQuery query,
) {
  _setU32(memory, offset, _pointQuerySchema);
  _setU32(memory, offset + 4, _WebHostScratch._pointQueryBytes);
  _writeSourceVersion(memory, offset + 8, query.sourceVersion);
  _setU32(memory, offset + 52, query.position.bytes);
  _setU32(memory, offset + 56, query.position.utf16);
  _setU32(memory, offset + 60, switch (query.affinity) {
    FlarkV3MetricAffinity.upstream => 0,
    FlarkV3MetricAffinity.downstream => 1,
  });
  _setU32(memory, offset + 64, query.budget.maxEncodedBytes);
  _setU32(memory, offset + 68, query.budget.maxOpenDepth);
  _setU32(memory, offset + 72, query.budget.maxLeafCount);
  _setU32(memory, offset + 76, query.budget.maxTreeNodesVisited);
  for (var reserved = 80; reserved < 96; reserved += 4) {
    _setU32(memory, offset + reserved, 0);
  }
}

void _writeBlockRangeQuery(
  ByteData memory,
  int offset,
  FlarkV3HostBlockRangeQuery query,
) {
  _setU32(memory, offset, _blockRangeQuerySchema);
  _setU32(memory, offset + 4, _WebHostScratch._blockRangeQueryBytes);
  _writeSourceVersion(memory, offset + 8, query.sourceVersion);
  final requested = query.requestedRange;
  final budget = query.budget;
  _setU32(memory, offset + 52, requested.start.bytes);
  _setU32(memory, offset + 56, requested.start.utf16);
  _setU32(memory, offset + 60, requested.end.bytes);
  _setU32(memory, offset + 64, requested.end.utf16);
  _setU32(memory, offset + 68, budget.maxEncodedBytes);
  _setU32(memory, offset + 72, budget.maxBlockCount);
  _setU32(memory, offset + 76, budget.maxStoragePagesVisited);
  _setU32(memory, offset + 80, budget.maxOpenDepth);
  _setU32(memory, offset + 84, budget.maxTreeNodesVisited);
  final continuation = query.continuation?.copyEncoded();
  _setU32(memory, offset + 88, continuation?.length ?? 0);
  for (
    var index = 0;
    index < FlarkV3HostBlockRangeContinuation.encodedBytes;
    index += 1
  ) {
    memory.setUint8(offset + 92 + index, continuation?[index] ?? 0);
  }
  for (var reserved = 156; reserved < 172; reserved += 4) {
    _setU32(memory, offset + reserved, 0);
  }
}

void _writeStructuralOrdinalWindowQuery(
  ByteData memory,
  int offset,
  FlarkV3HostStructuralOrdinalWindowQuery query,
) {
  _setU32(memory, offset, _structuralOrdinalWindowQuerySchema);
  _setU32(
    memory,
    offset + 4,
    _WebHostScratch._structuralOrdinalWindowQueryBytes,
  );
  _writeSourceVersion(memory, offset + 8, query.sourceVersion);
  _writeU64(memory, offset + 52, query.startBlockOrdinal);
  final budget = query.budget;
  _setU32(memory, offset + 60, budget.maximumEntries);
  _setU32(memory, offset + 64, budget.maximumStoragePagesVisited);
  _setU32(memory, offset + 68, budget.maximumTreeNodesVisited);
  _setU32(memory, offset + 72, budget.maximumPackedEntriesInspected);
  for (var reserved = 76; reserved < 92; reserved += 4) {
    _setU32(memory, offset + reserved, 0);
  }
}

FlarkV3HostStructuralOrdinalWindowOutcome
_decodeStructuralOrdinalWindowOutcome({
  required FlarkV3HostStructuralOrdinalWindowQuery query,
  required ByteData memory,
  required int receiptOffset,
}) {
  final source = _readSourceVersion(memory, receiptOffset + 72);
  final total = _readU64(memory, receiptOffset + 16);
  final start = _readU64(memory, receiptOffset + 24);
  final next = _readU64(memory, receiptOffset + 32);
  final work = FlarkV3HostStructuralOrdinalWindowWorkReceipt(
    storagePagesVisited: _u32(memory, receiptOffset + 56),
    treeNodesVisited: _u32(memory, receiptOffset + 60),
    packedEntriesInspected: _u32(memory, receiptOffset + 64),
    summaryNodesSkipped: _u32(memory, receiptOffset + 68),
  );
  final flags = _u32(memory, receiptOffset + 12);
  if ((flags & ~1) != 0 ||
      source != query.sourceVersion ||
      start != query.startBlockOrdinal ||
      !work.fits(query.budget)) {
    throw const FlarkV3WebHostException(
      operation: 'queryStructuralOrdinalWindowReceipt',
      status: 0x0111,
      detail: 'Web host returned an out-of-authority ordinal receipt',
    );
  }

  final rawOutcome = _u32(memory, receiptOffset + 4);
  final rawFailure = _u32(memory, receiptOffset + 8);
  final FlarkV3HostStructuralOrdinalWindowOutcome outcome =
      switch (rawOutcome) {
        1 when rawFailure == 0 => FlarkV3HostStructuralOrdinalWindow(
          sourceVersion: source,
          totalBlockCount: total,
          startBlockOrdinal: start,
          nextBlockOrdinal: next,
          startSource: FlarkV3SourceMetric(
            bytes: _u32(memory, receiptOffset + 40),
            utf16: _u32(memory, receiptOffset + 44),
          ),
          nextSource: FlarkV3SourceMetric(
            bytes: _u32(memory, receiptOffset + 48),
            utf16: _u32(memory, receiptOffset + 52),
          ),
          work: work,
          complete: (flags & 1) != 0,
        ),
        2
            when rawFailure >= 1 &&
                rawFailure <= 7 &&
                next.isZero &&
                _u32(memory, receiptOffset + 40) == 0 &&
                _u32(memory, receiptOffset + 44) == 0 &&
                _u32(memory, receiptOffset + 48) == 0 &&
                _u32(memory, receiptOffset + 52) == 0 &&
                flags == 0 =>
          FlarkV3HostStructuralOrdinalWindowFailure(
            sourceVersion: source,
            totalBlockCount: total,
            startBlockOrdinal: start,
            reason: _structuralOrdinalWindowFailure(rawFailure),
            work: work,
          ),
        _ => throw FlarkV3WebHostException(
          operation: 'queryStructuralOrdinalWindowReceipt',
          status: 0x0111,
          detail:
              'Web host returned invalid ordinal outcome $rawOutcome '
              'and failure $rawFailure',
        ),
      };
  if (!outcome.binds(query)) {
    throw const FlarkV3WebHostException(
      operation: 'queryStructuralOrdinalWindowReceipt',
      status: 0x0111,
      detail: 'Web host returned an invalid ordinal-window witness',
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
  _ => throw FlarkV3WebHostException(
    operation: 'queryStructuralOrdinalWindowReceipt',
    status: 0x0111,
    detail: 'Web host returned unknown ordinal failure $value',
  ),
};

void _validateStructuralOrdinalWindowReserved(
  ByteData memory,
  int receiptOffset,
) {
  for (var reserved = 116; reserved < 132; reserved += 4) {
    if (_u32(memory, receiptOffset + reserved) != 0) {
      throw const FlarkV3WebHostException(
        operation: 'queryStructuralOrdinalWindowReceipt',
        status: 0x0111,
        detail: 'Web host returned nonzero reserved ordinal fields',
      );
    }
  }
}

bool _structuralOrdinalWindowRejectedBodyIsCanonical(
  ByteData memory,
  int receiptOffset,
) {
  for (var offset = 4; offset < 132; offset += 4) {
    if (_u32(memory, receiptOffset + offset) != 0) return false;
  }
  return true;
}

FlarkV3InlineSidecarQueryOutcome _decodeInlineSidecarQueryOutcome({
  required FlarkV3InlineSidecarQuery query,
  required ByteData memory,
  required int receiptOffset,
  required int outputOffset,
  required FlarkV3WasmModule module,
}) {
  final encodedBytes = _u32(memory, receiptOffset + 12);
  if (encodedBytes > query.maximumEncodedBytes) {
    throw const FlarkV3WebHostException(
      operation: 'queryInlineSidecarReceipt',
      status: 0x0111,
      detail: 'Web host exceeded the sidecar query copy bound',
    );
  }
  final outcome = _u32(memory, receiptOffset + 4);
  final reason = _u32(memory, receiptOffset + 8);
  final factCount = _u32(memory, receiptOffset + 16);
  final treeNodesVisited = _u32(memory, receiptOffset + 20);
  final valueEntryCount = _u32(memory, receiptOffset + 24);
  final valueEncodedBytes = _u32(memory, receiptOffset + 28);
  final payloadKindWire = _u32(memory, receiptOffset + 32);
  final payloadKind = FlarkV3InlineSidecarPayloadKind.tryFromWireValue(
    payloadKindWire,
  );
  final factBytes = factCount * (payloadKind?.recordBytes ?? 0);
  if (outcome == 1 &&
      (payloadKind == null ||
          factBytes + valueEncodedBytes != encodedBytes ||
          (payloadKind != FlarkV3InlineSidecarPayloadKind.inline &&
              (valueEntryCount != 0 || valueEncodedBytes != 0)))) {
    throw const FlarkV3WebHostException(
      operation: 'queryInlineSidecarReceipt',
      status: 0x0111,
      detail: 'Web host sidecar receipt does not frame facts and values',
    );
  }
  final encoded = module.readBytes(outputOffset, encodedBytes);
  return switch (outcome) {
    0
        when reason == 0 &&
            encodedBytes == 0 &&
            factCount == 0 &&
            treeNodesVisited == 0 &&
            valueEntryCount == 0 &&
            valueEncodedBytes == 0 &&
            payloadKindWire == 0 =>
      const FlarkV3InlineSidecarQueryUnavailable(),
    1 when reason == 0 => FlarkV3InlineSidecarQueryAuthoritative(
      payloadKind: payloadKind!,
      factCount: factCount,
      valueEntryCount: valueEntryCount,
      treeNodesVisited: treeNodesVisited,
      encodedFacts: Uint8List.sublistView(encoded, 0, factBytes),
      encodedValues: Uint8List.sublistView(encoded, factBytes),
    ),
    2
        when reason != 0 &&
            factCount == 0 &&
            treeNodesVisited == 0 &&
            valueEntryCount == 0 &&
            valueEncodedBytes == 0 &&
            payloadKindWire == 0 =>
      FlarkV3InlineSidecarQueryUnsupported(reason: reason, metadata: encoded),
    _ => throw FlarkV3WebHostException(
      operation: 'queryInlineSidecarReceipt',
      status: 0x0111,
      detail: 'Web host returned invalid sidecar outcome $outcome',
    ),
  };
}

FlarkV3HostStoreQueryOutcome _decodePointQueryOutcome({
  required FlarkV3HostPointQuery query,
  required ByteData memory,
  required int receiptOffset,
  required int outputOffset,
  required FlarkV3WasmModule module,
}) {
  for (var reserved = 92; reserved < 112; reserved += 4) {
    if (_u32(memory, receiptOffset + reserved) != 0) {
      throw const FlarkV3WebHostException(
        operation: 'queryStructuralReceipt',
        status: 0x0111,
        detail: 'Web host returned nonzero reserved query fields',
      );
    }
  }

  final source = _readSourceVersion(memory, receiptOffset + 48);
  final rangeStart = FlarkV3SourceMetric(
    bytes: _u32(memory, receiptOffset + 32),
    utf16: _u32(memory, receiptOffset + 36),
  );
  final rangeEnd = FlarkV3SourceMetric(
    bytes: _u32(memory, receiptOffset + 40),
    utf16: _u32(memory, receiptOffset + 44),
  );
  if (!rangeEnd.contains(rangeStart)) {
    throw const FlarkV3WebHostException(
      operation: 'queryStructuralReceipt',
      status: 0x0111,
      detail: 'Web host returned an inverted query range',
    );
  }
  final range = FlarkV3MetricRange(start: rangeStart, end: rangeEnd);
  final encodedBytes = _u32(memory, receiptOffset + 12);
  final leafCount = _u32(memory, receiptOffset + 16);
  final openDepth = _u32(memory, receiptOffset + 20);
  final treeNodesVisited = _u32(memory, receiptOffset + 24);
  final summaryNodesSkipped = _u32(memory, receiptOffset + 28);
  final point = query.position;
  final rangeContainsPoint =
      point.bytes >= range.start.bytes &&
      point.utf16 >= range.start.utf16 &&
      range.end.contains(point);
  if (source != query.sourceVersion ||
      !source.metric.contains(range.end) ||
      !rangeContainsPoint ||
      encodedBytes > query.budget.maxEncodedBytes ||
      leafCount > query.budget.maxLeafCount ||
      openDepth > query.budget.maxOpenDepth ||
      treeNodesVisited > query.budget.maxTreeNodesVisited) {
    throw const FlarkV3WebHostException(
      operation: 'queryStructuralReceipt',
      status: 0x0111,
      detail: 'Web host returned an out-of-authority query receipt',
    );
  }

  final receipt = FlarkV3HostViewportReceipt(
    encodedBytes: encodedBytes,
    leafCount: leafCount,
    openDepth: openDepth,
    treeNodesVisited: treeNodesVisited,
    summaryNodesSkipped: summaryNodesSkipped,
  );
  final outcome = _u32(memory, receiptOffset + 4);
  final gapReason = _u32(memory, receiptOffset + 8);
  return switch (outcome) {
    1 when gapReason == 0 && encodedBytes > 0 =>
      FlarkV3HostStoreStructuralQuery(
        FlarkV3HostStructuralViewport.owned(
          sourceVersion: source,
          range: range,
          encoded: module.readBytes(outputOffset, encodedBytes),
          receipt: receipt,
        ),
      ),
    2 when encodedBytes == 0 => FlarkV3HostStoreSourceGapQuery(
      FlarkV3HostLocalSourceGap(
        sourceVersion: source,
        range: range,
        reason: _readPointQueryGapReason(gapReason),
        receipt: receipt,
      ),
    ),
    _ => throw FlarkV3WebHostException(
      operation: 'queryStructuralReceipt',
      status: 0x0111,
      detail:
          'Web host returned invalid query outcome $outcome '
          'and gap $gapReason',
    ),
  };
}

FlarkV3HostStoreBlockRangeQueryOutcome _decodeBlockRangeQueryOutcome({
  required FlarkV3HostBlockRangeQuery query,
  required ByteData memory,
  required int receiptOffset,
  required int outputOffset,
  required FlarkV3WasmModule module,
}) {
  for (var reserved = 172; reserved < 188; reserved += 4) {
    if (_u32(memory, receiptOffset + reserved) != 0) {
      throw const FlarkV3WebHostException(
        operation: 'queryStructuralRangeReceipt',
        status: 0x0111,
        detail: 'Web host returned nonzero reserved range fields',
      );
    }
  }
  final outcome = _u32(memory, receiptOffset + 4);
  final gapReason = _u32(memory, receiptOffset + 8);
  final encodedBytes = _u32(memory, receiptOffset + 12);
  final blockCount = _u32(memory, receiptOffset + 16);
  final storagePagesVisited = _u32(memory, receiptOffset + 20);
  final openDepth = _u32(memory, receiptOffset + 24);
  final treeNodesVisited = _u32(memory, receiptOffset + 28);
  final packedEntriesInspected = _u32(memory, receiptOffset + 32);
  final summaryNodesSkipped = _u32(memory, receiptOffset + 36);
  final flags = _u32(memory, receiptOffset + 40);
  final coverageStart = FlarkV3SourceMetric(
    bytes: _u32(memory, receiptOffset + 44),
    utf16: _u32(memory, receiptOffset + 48),
  );
  final coverageEnd = FlarkV3SourceMetric(
    bytes: _u32(memory, receiptOffset + 52),
    utf16: _u32(memory, receiptOffset + 56),
  );
  if (!coverageEnd.contains(coverageStart)) {
    throw const FlarkV3WebHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail: 'Web host returned inverted range coverage',
    );
  }
  final coverage = FlarkV3MetricRange(start: coverageStart, end: coverageEnd);
  final source = _readSourceVersion(memory, receiptOffset + 60);
  final continuationLength = _u32(memory, receiptOffset + 104);
  final continuationBytes = Uint8List.fromList(<int>[
    for (
      var index = 0;
      index < FlarkV3HostBlockRangeContinuation.encodedBytes;
      index += 1
    )
      memory.getUint8(receiptOffset + 108 + index),
  ]);
  final emptyContinuationIsZero =
      continuationLength != 0 || continuationBytes.every((byte) => byte == 0);
  final complete = (flags & 1) != 0;
  final budget = query.budget;
  if ((flags & ~1) != 0 ||
      (continuationLength != 0 &&
          continuationLength !=
              FlarkV3HostBlockRangeContinuation.encodedBytes) ||
      !emptyContinuationIsZero ||
      source != query.sourceVersion ||
      !source.metric.contains(coverage.end) ||
      encodedBytes > budget.maxEncodedBytes ||
      blockCount > budget.maxBlockCount ||
      storagePagesVisited > budget.maxStoragePagesVisited ||
      openDepth > budget.maxOpenDepth ||
      treeNodesVisited > budget.maxTreeNodesVisited ||
      !flarkV3HostPackedEntryReceiptFitsStoragePages(
        storagePagesVisited: storagePagesVisited,
        packedEntriesInspected: packedEntriesInspected,
      ) ||
      (outcome == 1 && complete != (continuationLength == 0))) {
    throw const FlarkV3WebHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail: 'Web host returned an out-of-authority range receipt',
    );
  }
  final receipt = FlarkV3HostBlockRangeReceipt(
    encodedBytes: encodedBytes,
    blockCount: blockCount,
    storagePagesVisited: storagePagesVisited,
    openDepth: openDepth,
    treeNodesVisited: treeNodesVisited,
    packedEntriesInspected: packedEntriesInspected,
    summaryNodesSkipped: summaryNodesSkipped,
    complete: complete,
  );
  final continuation = continuationLength == 0
      ? null
      : FlarkV3HostBlockRangeContinuation.owned(continuationBytes);
  final requestedNonempty =
      query.requestedRange.start != query.requestedRange.end;
  final encoded = encodedBytes == 0
      ? Uint8List(0)
      : module.readBytes(outputOffset, encodedBytes);
  final canonicalEnvelope = _canonicalBlockRangeEnvelopeLength(
    encoded,
    blockCount,
  );
  final coverageOverlapsRequest = _rangesOverlap(
    coverage,
    query.requestedRange,
  );
  return switch (outcome) {
    1
        when gapReason == 0 &&
            canonicalEnvelope &&
            (!requestedNonempty || blockCount > 0) &&
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
        when gapReason != 0 &&
            encodedBytes == 0 &&
            blockCount == 0 &&
            flags == 0 &&
            continuation == null &&
            coverage == query.requestedRange =>
      FlarkV3HostStoreBlockRangeSourceGapQuery(
        FlarkV3HostBlockRangeSourceGap(
          sourceVersion: source,
          requestedRange: query.requestedRange,
          reason: _readPointQueryGapReason(gapReason),
          receipt: receipt,
        ),
      ),
    _ => throw FlarkV3WebHostException(
      operation: 'queryStructuralRangeReceipt',
      status: 0x0111,
      detail:
          'Web host returned invalid range outcome $outcome '
          'and gap $gapReason',
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
      _ => throw FlarkV3WebHostException(
        operation: 'queryStructuralReceipt',
        status: 0x0111,
        detail: 'Web host returned unknown query gap $value',
      ),
    };

bool _rangesOverlap(FlarkV3MetricRange left, FlarkV3MetricRange right) =>
    left.start.bytes < right.end.bytes &&
    left.start.utf16 < right.end.utf16 &&
    right.start.bytes < left.end.bytes &&
    right.start.utf16 < left.end.utf16;

FlarkV3SourceVersion _readSourceVersion(ByteData memory, int offset) =>
    FlarkV3SourceVersion(
      documentSession: FlarkV3DocumentSessionId(
        _u32(memory, offset),
        _u32(memory, offset + 4),
        _u32(memory, offset + 8),
        _u32(memory, offset + 12),
      ),
      revision: _u32(memory, offset + 16),
      metric: FlarkV3SourceMetric(
        bytes: _u32(memory, offset + 20),
        utf16: _u32(memory, offset + 24),
      ),
      contentHash: FlarkV3ContentHash128(
        _u32(memory, offset + 28),
        _u32(memory, offset + 32),
        _u32(memory, offset + 36),
        _u32(memory, offset + 40),
      ),
    );

FlarkV3OfferId _readOfferId(ByteData memory, int offset) => FlarkV3OfferId(
  _u32(memory, offset),
  _u32(memory, offset + 4),
  _u32(memory, offset + 8),
  _u32(memory, offset + 12),
);

FlarkV3ProtocolDigest128 _readDigest(ByteData memory, int offset) =>
    FlarkV3ProtocolDigest128(
      _u32(memory, offset),
      _u32(memory, offset + 4),
      _u32(memory, offset + 8),
      _u32(memory, offset + 12),
    );

FlarkV3StructuralAck _readAck(ByteData memory, int offset) =>
    FlarkV3StructuralAck(
      publicationSession: FlarkV3PublicationSessionId(
        _u32(memory, offset),
        _u32(memory, offset + 4),
        _u32(memory, offset + 8),
        _u32(memory, offset + 12),
      ),
      hostRevision: FlarkV3HostRevisionId(_u32(memory, offset + 16)),
      sourceVersion: _readSourceVersion(memory, offset + 20),
      sourceRoot: FlarkV3SourceRootId(
        _u32(memory, offset + 64),
        _u32(memory, offset + 68),
      ),
      parseGeneration: _u32(memory, offset + 72),
      grammarRevision: _u32(memory, offset + 76),
      syntaxProfile: FlarkV3SyntaxProfileId(_u32(memory, offset + 80)),
      authorityMask: FlarkV3StructuralAuthorityMask(_u32(memory, offset + 84)),
      recordCount: _u32(memory, offset + 88),
      sequenceDigest: _readDigest(memory, offset + 92),
      manifestDigest: _readDigest(memory, offset + 108),
    );

FlarkV3ProtocolU64 _readU64(ByteData memory, int offset) => FlarkV3ProtocolU64(
  lowWord: _u32(memory, offset),
  highWord: _u32(memory, offset + 4),
);

FlarkV3ProtocolDigest256 _readDigest256(ByteData memory, int offset) =>
    FlarkV3ProtocolDigest256(
      _u32(memory, offset),
      _u32(memory, offset + 4),
      _u32(memory, offset + 8),
      _u32(memory, offset + 12),
      _u32(memory, offset + 16),
      _u32(memory, offset + 20),
      _u32(memory, offset + 24),
      _u32(memory, offset + 28),
    );

FlarkV3InlineSidecarAck _readInlineSidecarAck(ByteData memory, int offset) {
  final rawDisposition = _u32(memory, offset + 160);
  final disposition = switch (rawDisposition) {
    1 => FlarkV3InlineSidecarAckDisposition.authoritative,
    2 => FlarkV3InlineSidecarAckDisposition.unsupported,
    _ => throw FlarkV3WebHostException(
      operation: 'pollInlineSidecarAck',
      status: 0x0111,
      detail: 'unknown sidecar ACK disposition $rawDisposition',
    ),
  };
  return FlarkV3InlineSidecarAck(
    publicationSession: FlarkV3PublicationSessionId(
      _u32(memory, offset),
      _u32(memory, offset + 4),
      _u32(memory, offset + 8),
      _u32(memory, offset + 12),
    ),
    baseAck: _readAck(memory, offset + 16),
    refinementGeneration: _readU64(memory, offset + 140),
    blockOrdinal: _readU64(memory, offset + 148),
    transferredNodeCount: _u32(memory, offset + 156),
    disposition: disposition,
    hio1EnvelopeDigest256: _readDigest256(memory, offset + 164),
    rootStreamDigest: _readDigest(memory, offset + 196),
  );
}

FlarkV3ViewportPresentationMetricRange _readViewportPresentationRange(
  ByteData memory,
  int offset,
) => FlarkV3ViewportPresentationMetricRange(
  startUtf8: _u32(memory, offset),
  startUtf16: _u32(memory, offset + 4),
  endUtf8: _u32(memory, offset + 8),
  endUtf16: _u32(memory, offset + 12),
);

FlarkV3ViewportPresentationVisitStart _readViewportPresentationVisitStart(
  ByteData memory,
  int offset,
) => FlarkV3ViewportPresentationVisitStart(
  blockOrdinal: _readU64(memory, offset),
  utf8Offset: _u32(memory, offset + 8),
  utf16Offset: _u32(memory, offset + 12),
);

FlarkV3ViewportPresentationBinding _readViewportPresentationBinding(
  ByteData memory,
  int offset,
) {
  final rawComplete = _u32(memory, offset + 68);
  final complete = switch (rawComplete) {
    0 => false,
    1 => true,
    _ => throw FlarkV3WebHostException(
      operation: 'viewportPresentationBinding',
      status: 0x0111,
      detail: 'Web host returned invalid complete flag $rawComplete',
    ),
  };
  return FlarkV3ViewportPresentationBinding(
    viewportGeneration: _u32(memory, offset),
    requestedRange: _readViewportPresentationRange(memory, offset + 4),
    coveredRange: _readViewportPresentationRange(memory, offset + 20),
    start: _readViewportPresentationVisitStart(memory, offset + 36),
    next: _readViewportPresentationVisitStart(memory, offset + 52),
    complete: complete,
  );
}

FlarkV3ViewportPresentationEnvelopeMetrics _readViewportPresentationEnvelope(
  ByteData memory,
  int offset,
) => FlarkV3ViewportPresentationEnvelopeMetrics(
  visitedStructuralEntries: _u32(memory, offset),
  visitedStoragePages: _u32(memory, offset + 4),
  orderedLeafCount: _u32(memory, offset + 8),
  inlineSourceBytes: _u32(memory, offset + 12),
  factCount: _u32(memory, offset + 16),
  transferredNodeCount: _u32(memory, offset + 20),
  parserTransitions: _u32(memory, offset + 24),
  aggregateEnvelopeDigest256: _readDigest256(memory, offset + 28),
);

FlarkV3ViewportPresentationAck _readViewportPresentationAck(
  ByteData memory,
  int offset,
) => FlarkV3ViewportPresentationAck(
  publicationSession: FlarkV3PublicationSessionId(
    _u32(memory, offset),
    _u32(memory, offset + 4),
    _u32(memory, offset + 8),
    _u32(memory, offset + 12),
  ),
  baseAck: _readAck(memory, offset + 16),
  binding: _readViewportPresentationBinding(memory, offset + 140),
  envelope: _readViewportPresentationEnvelope(memory, offset + 212),
  actualFrameCount: _u32(memory, offset + 272),
  actualEncodedFrameBytes: _u32(memory, offset + 276),
  aggregateRootStreamDigest: _readDigest(memory, offset + 280),
);
