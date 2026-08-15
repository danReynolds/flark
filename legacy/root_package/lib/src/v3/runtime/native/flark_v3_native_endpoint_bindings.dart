import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../flark_v3_byte_endpoint.dart';

const int flarkV3NativeEndpointAbiVersion = 0x00020002;
const int flarkV3NativeMaximumFrameBytes = 262168;
const int flarkV3NativeMaximumPollSourceBytes = 65536;
const int flarkV3NativeMaximumPollCheckpoints = 64;
const int flarkV3NativeMaximumRetirementTransitions = 256;
const int flarkV3NativeMaximumCandidateTransitions = 256;
const int flarkV3NativeCheckpointBMaximumJsonBytes = 65536;

const int flarkV3NativeStatusOk = 0x0000;
const int flarkV3NativeStatusBackpressure = 0x0102;
const int flarkV3NativeStatusForegroundBoundExceeded = 0x010a;
const int flarkV3NativeStatusClosed = 0x010c;
const int flarkV3NativeStatusNotReady = 0x010d;

// Fixed-width values from `flark_comrak_bridge.h`. These are kept here rather
// than inferred from command bytes so the isolate can make lifecycle decisions
// from the native endpoint's accepted action.
const int flarkV3NativeActionEventReceiptAccepted = 5;
const int flarkV3NativeActionCloseLatched = 6;

const int flarkV3NativeEventKindFailed = 5;
const int flarkV3NativeEventKindClosed = 7;
const int flarkV3NativeEventKindInlinePublicationBegin = 15;
const int flarkV3NativeEventKindInlinePublicationPacket = 16;
const int flarkV3NativeEventKindInlinePublicationCommit = 17;
const int flarkV3NativeEventKindInlinePublicationDeliveryAcknowledged = 18;
const int flarkV3NativeEventKindViewportPublicationBegin = 21;
const int flarkV3NativeEventKindViewportPublicationPacket = 22;
const int flarkV3NativeEventKindViewportPublicationCommit = 23;
const int flarkV3NativeEventKindViewportPublicationDeliveryAcknowledged = 24;

final class FlarkV3NativeEndpointException implements Exception {
  const FlarkV3NativeEndpointException({
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
    return 'FlarkV3NativeEndpointException($operation, '
        'status=0x${status.toRadixString(16).padLeft(4, '0')}$suffix)';
  }
}

/// Isolated binding for the private, one-shot Feedback Checkpoint B probe.
///
/// Keeping this symbol acquisition separate prevents the diagnostic seam from
/// becoming a requirement of the ordinary product endpoint.
final class FlarkV3NativeCheckpointBProbeBindings {
  FlarkV3NativeCheckpointBProbeBindings._(this._probe);

  factory FlarkV3NativeCheckpointBProbeBindings.load(DynamicLibrary library) =>
      FlarkV3NativeCheckpointBProbeBindings._(
        library.lookupFunction<_CheckpointBProbeNative, _CheckpointBProbeDart>(
          'flark_v3_checkpoint_b_probe',
        ),
      );

  final _CheckpointBProbeDart _probe;

  Uint8List run() {
    final output = calloc<Uint8>(flarkV3NativeCheckpointBMaximumJsonBytes);
    try {
      final written = calloc<Uint32>();
      try {
        final status = _probe(
          output,
          flarkV3NativeCheckpointBMaximumJsonBytes,
          written,
        );
        if (status != flarkV3NativeStatusOk) {
          throw FlarkV3NativeEndpointException(
            operation: 'checkpointBProbe',
            status: status,
          );
        }
        final length = written.value;
        if (length <= 0 || length > flarkV3NativeCheckpointBMaximumJsonBytes) {
          throw FlarkV3NativeEndpointException(
            operation: 'checkpointBProbeLength',
            status: 0x0111,
            detail: 'written=$length',
          );
        }
        return Uint8List.fromList(output.asTypedList(length));
      } finally {
        calloc.free(written);
      }
    } finally {
      calloc.free(output);
    }
  }
}

final class FlarkV3NativeEndpointHandle {
  FlarkV3NativeEndpointHandle({required this.slot, required this.generation}) {
    if (slot <= 0 || slot > 0xffffffff) {
      throw RangeError.range(slot, 1, 0xffffffff, 'slot');
    }
    if (generation <= 0 || generation > 0xffffffff) {
      throw RangeError.range(generation, 1, 0xffffffff, 'generation');
    }
  }

  final int slot;
  final int generation;
}

/// Opaque, isolate-confined ownership of one native emergency-finalizer token.
///
/// The token contains only a generation-checked registry handle. It never
/// contains an endpoint pointer and must be consumed exactly once, either by
/// explicit release after teardown or by the native finalizer callback.
final class FlarkV3NativeEndpointFinalizerLease {
  FlarkV3NativeEndpointFinalizerLease._(this._token);

  final Pointer<Void> _token;
  final Object _detachKey = Object();
  bool _attached = true;
  bool _consumed = false;
}

final class FlarkV3NativeDispatchReceipt {
  const FlarkV3NativeDispatchReceipt({
    required this.correlationId,
    required this.action,
    required this.outstandingEventId,
    required this.outstandingEventKind,
  });

  final int correlationId;
  final int action;
  final int outstandingEventId;
  final int outstandingEventKind;

  bool get hasOutstandingEvent => outstandingEventId != 0;
}

final class FlarkV3NativeDispatchResult {
  const FlarkV3NativeDispatchResult({
    required this.status,
    required this.receipt,
  });

  final int status;
  final FlarkV3NativeDispatchReceipt receipt;
}

final class FlarkV3NativePollFuel {
  const FlarkV3NativePollFuel({
    this.maximumSourceBytes = 64 * 1024,
    this.maximumCheckpoints = 64,
    this.maximumRetirementTransitions = 32,
  });

  final int maximumSourceBytes;
  final int maximumCheckpoints;
  final int maximumRetirementTransitions;

  void validate() {
    if (maximumSourceBytes <= 0 ||
        maximumSourceBytes > flarkV3NativeMaximumPollSourceBytes) {
      throw RangeError.range(
        maximumSourceBytes,
        1,
        flarkV3NativeMaximumPollSourceBytes,
        'maximumSourceBytes',
      );
    }
    if (maximumCheckpoints <= 0 ||
        maximumCheckpoints > flarkV3NativeMaximumPollCheckpoints) {
      throw RangeError.range(
        maximumCheckpoints,
        1,
        flarkV3NativeMaximumPollCheckpoints,
        'maximumCheckpoints',
      );
    }
    if (maximumRetirementTransitions < 0 ||
        maximumRetirementTransitions >
            flarkV3NativeMaximumRetirementTransitions) {
      throw RangeError.range(
        maximumRetirementTransitions,
        0,
        flarkV3NativeMaximumRetirementTransitions,
        'maximumRetirementTransitions',
      );
    }
  }
}

final class FlarkV3NativePollReceipt {
  const FlarkV3NativePollReceipt({
    required this.sourceBytesExamined,
    required this.sourceBytesBuffered,
    required this.cursorRefills,
    required this.cursorCopyBytesUpperBound,
    required this.checkpointsEmitted,
    required this.sourceFactTransitions,
    required this.releasedSourceLeases,
    required this.releasedSourceBytes,
    required this.arenaTransitions,
    required this.arenaNodesReclaimed,
    required this.scanComplete,
    required this.certification,
    required this.cleanupComplete,
    required this.outstandingEventId,
    required this.outstandingEventKind,
  });

  final int sourceBytesExamined;
  final int sourceBytesBuffered;
  final int cursorRefills;
  final int cursorCopyBytesUpperBound;
  final int checkpointsEmitted;
  final int sourceFactTransitions;
  final int releasedSourceLeases;
  final int releasedSourceBytes;
  final int arenaTransitions;
  final int arenaNodesReclaimed;
  final bool scanComplete;
  final int certification;
  final bool cleanupComplete;
  final int outstandingEventId;
  final int outstandingEventKind;

  bool get hasOutstandingEvent => outstandingEventId != 0;

  bool get madeProgress =>
      sourceBytesExamined != 0 ||
      sourceBytesBuffered != 0 ||
      cursorRefills != 0 ||
      checkpointsEmitted != 0 ||
      sourceFactTransitions != 0 ||
      releasedSourceLeases != 0 ||
      releasedSourceBytes != 0 ||
      arenaTransitions != 0 ||
      arenaNodesReclaimed != 0 ||
      scanComplete ||
      certification != 0 ||
      cleanupComplete;
}

final class FlarkV3NativePollResult {
  const FlarkV3NativePollResult({required this.status, required this.receipt});

  final int status;
  final FlarkV3NativePollReceipt receipt;
}

final class FlarkV3NativeCandidatePollFuel {
  const FlarkV3NativeCandidatePollFuel({this.maximumTransitions = 32});

  final int maximumTransitions;

  void validate() {
    if (maximumTransitions <= 0 ||
        maximumTransitions > flarkV3NativeMaximumCandidateTransitions) {
      throw RangeError.range(
        maximumTransitions,
        1,
        flarkV3NativeMaximumCandidateTransitions,
        'maximumTransitions',
      );
    }
  }
}

final class FlarkV3NativeCandidatePollReceipt {
  const FlarkV3NativeCandidatePollReceipt({
    required this.transitions,
    required this.cleanupComplete,
    required this.outstandingEventId,
    required this.outstandingEventKind,
  });

  final int transitions;
  final bool cleanupComplete;
  final int outstandingEventId;
  final int outstandingEventKind;

  bool get hasOutstandingEvent => outstandingEventId != 0;
  bool get madeProgress => transitions != 0;
}

final class FlarkV3NativeCandidatePollResult {
  const FlarkV3NativeCandidatePollResult({
    required this.status,
    required this.receipt,
  });

  final int status;
  final FlarkV3NativeCandidatePollReceipt receipt;
}

/// Synchronous, isolate-confined owner of the fixed-width native endpoint ABI.
///
/// No object from this class crosses an isolate port. Callers copy command
/// bytes into native storage and copy each credited event out exactly once.
final class FlarkV3NativeEndpointBindings {
  FlarkV3NativeEndpointBindings._(this._symbols, this._config)
    : _emergencyFinalizer = NativeFinalizer(_symbols.emergencyFinalize);

  factory FlarkV3NativeEndpointBindings.load(DynamicLibrary library) {
    final symbols = _FlarkV3NativeSymbols.fromLibrary(library);
    final loadedAbi = symbols.abiVersion();
    if (loadedAbi != flarkV3NativeEndpointAbiVersion) {
      throw FlarkV3NativeEndpointException(
        operation: 'abiVersion',
        status: loadedAbi,
        detail:
            'expected 0x${flarkV3NativeEndpointAbiVersion.toRadixString(16)}',
      );
    }
    final config = calloc<_NativeEndpointConfig>();
    final status = symbols.standardConfig(config);
    if (status != flarkV3NativeStatusOk) {
      calloc.free(config);
      throw FlarkV3NativeEndpointException(
        operation: 'configStandard',
        status: status,
      );
    }
    if (config.ref.abiVersion != flarkV3NativeEndpointAbiVersion ||
        config.ref.structSize != sizeOf<_NativeEndpointConfig>()) {
      final actualAbi = config.ref.abiVersion;
      final actualSize = config.ref.structSize;
      calloc.free(config);
      throw FlarkV3NativeEndpointException(
        operation: 'configLayout',
        status: 0x0100,
        detail: 'abi=$actualAbi size=$actualSize',
      );
    }
    return FlarkV3NativeEndpointBindings._(symbols, config);
  }

  final _FlarkV3NativeSymbols _symbols;
  final Pointer<_NativeEndpointConfig> _config;
  final NativeFinalizer _emergencyFinalizer;
  bool _disposed = false;

  FlarkV3NativeEndpointHandle create() {
    _requireLive();
    final output = calloc<_NativeEndpointHandle>();
    try {
      final status = _symbols.create(_config, output);
      _requireOk('create', status);
      return _handle(output.ref, operation: 'create');
    } finally {
      calloc.free(output);
    }
  }

  FlarkV3NativeEndpointHandle recover(
    FlarkV3ByteEndpointBinding previousBinding,
  ) {
    _requireLive();
    final binding = calloc<_NativeSessionBinding>();
    final output = calloc<_NativeEndpointHandle>();
    try {
      for (var index = 0; index < 4; index += 1) {
        binding.ref.documentSession[index] =
            previousBinding.documentSessionWords[index];
      }
      binding.ref
        ..sourceSessionIdentity = previousBinding.sourceSessionIdentity
        ..workerGeneration = previousBinding.workerGeneration;
      final status = _symbols.recover(_config, binding, output);
      _requireOk('recover', status);
      return _handle(output.ref, operation: 'recover');
    } finally {
      calloc.free(output);
      calloc.free(binding);
    }
  }

  FlarkV3NativeDispatchResult dispatch(
    FlarkV3NativeEndpointHandle handle,
    Uint8List frame, {
    required bool strictClose,
  }) => _dispatch(
    handle,
    frame,
    strictClose: strictClose,
    hostPoll: false,
    inlineSidecarHostPoll: false,
    viewportPresentationHostPoll: false,
  );

  FlarkV3NativeDispatchResult dispatchHostPoll(
    FlarkV3NativeEndpointHandle handle,
    Uint8List frame,
  ) => _dispatch(
    handle,
    frame,
    strictClose: false,
    hostPoll: true,
    inlineSidecarHostPoll: false,
    viewportPresentationHostPoll: false,
  );

  FlarkV3NativeDispatchResult dispatchInlineSidecarHostPoll(
    FlarkV3NativeEndpointHandle handle,
    Uint8List frame,
  ) => _dispatch(
    handle,
    frame,
    strictClose: false,
    hostPoll: false,
    inlineSidecarHostPoll: true,
    viewportPresentationHostPoll: false,
  );

  FlarkV3NativeDispatchResult dispatchViewportPresentationHostPoll(
    FlarkV3NativeEndpointHandle handle,
    Uint8List frame,
  ) => _dispatch(
    handle,
    frame,
    strictClose: false,
    hostPoll: false,
    inlineSidecarHostPoll: false,
    viewportPresentationHostPoll: true,
  );

  FlarkV3NativeDispatchResult _dispatch(
    FlarkV3NativeEndpointHandle handle,
    Uint8List frame, {
    required bool strictClose,
    required bool hostPoll,
    required bool inlineSidecarHostPoll,
    required bool viewportPresentationHostPoll,
  }) {
    _requireLive();
    if ((strictClose ? 1 : 0) +
            (hostPoll ? 1 : 0) +
            (inlineSidecarHostPoll ? 1 : 0) +
            (viewportPresentationHostPoll ? 1 : 0) >
        1) {
      throw ArgumentError('A dispatch must select exactly one protocol lane.');
    }
    if (frame.isEmpty || frame.length > flarkV3NativeMaximumFrameBytes) {
      throw RangeError.range(
        frame.length,
        1,
        flarkV3NativeMaximumFrameBytes,
        'frame.length',
      );
    }
    final nativeHandle = calloc<_NativeEndpointHandle>();
    final input = calloc<Uint8>(frame.length);
    final output = calloc<_NativeDispatchReceipt>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      input.asTypedList(frame.length).setAll(0, frame);
      final status = viewportPresentationHostPoll
          ? _symbols.dispatchViewportPresentationHostPoll(
              nativeHandle.ref,
              input,
              frame.length,
              output,
            )
          : inlineSidecarHostPoll
          ? _symbols.dispatchInlineSidecarHostPoll(
              nativeHandle.ref,
              input,
              frame.length,
              output,
            )
          : hostPoll
          ? _symbols.dispatchHostPoll(
              nativeHandle.ref,
              input,
              frame.length,
              output,
            )
          : strictClose
          ? _symbols.close(nativeHandle.ref, input, frame.length, output)
          : _symbols.dispatch(nativeHandle.ref, input, frame.length, output);
      return FlarkV3NativeDispatchResult(
        status: status,
        receipt: _dispatchReceipt(output.ref),
      );
    } finally {
      calloc.free(output);
      calloc.free(input);
      calloc.free(nativeHandle);
    }
  }

  FlarkV3NativePollResult poll(
    FlarkV3NativeEndpointHandle handle,
    FlarkV3NativePollFuel fuel,
  ) {
    _requireLive();
    fuel.validate();
    final nativeHandle = calloc<_NativeEndpointHandle>();
    final nativeFuel = calloc<_NativePollFuel>();
    final output = calloc<_NativePollReceipt>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      nativeFuel.ref
        ..maximumSourceBytes = fuel.maximumSourceBytes
        ..maximumCheckpoints = fuel.maximumCheckpoints
        ..maximumRetirementTransitions = fuel.maximumRetirementTransitions;
      final status = _symbols.poll(nativeHandle.ref, nativeFuel.ref, output);
      return FlarkV3NativePollResult(
        status: status,
        receipt: _pollReceipt(output.ref),
      );
    } finally {
      calloc.free(output);
      calloc.free(nativeFuel);
      calloc.free(nativeHandle);
    }
  }

  FlarkV3NativeCandidatePollResult pollCandidate(
    FlarkV3NativeEndpointHandle handle,
    FlarkV3NativeCandidatePollFuel fuel,
  ) {
    _requireLive();
    fuel.validate();
    final nativeHandle = calloc<_NativeEndpointHandle>();
    final output = calloc<_NativeCandidatePollReceipt>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      final status = _symbols.pollCandidate(
        nativeHandle.ref,
        fuel.maximumTransitions,
        output,
      );
      final receipt = output.ref;
      return FlarkV3NativeCandidatePollResult(
        status: status,
        receipt: FlarkV3NativeCandidatePollReceipt(
          transitions: receipt.transitions,
          cleanupComplete: receipt.cleanupComplete != 0,
          outstandingEventId: receipt.outstandingEventId,
          outstandingEventKind: receipt.outstandingEventKind,
        ),
      );
    } finally {
      calloc.free(output);
      calloc.free(nativeHandle);
    }
  }

  Uint8List encodeOutstanding(FlarkV3NativeEndpointHandle handle) {
    _requireLive();
    final nativeHandle = calloc<_NativeEndpointHandle>();
    final written = calloc<Uint32>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      final queryStatus = _symbols.encode(
        nativeHandle.ref,
        nullptr,
        0,
        written,
      );
      final required = written.value;
      if (queryStatus != flarkV3NativeStatusForegroundBoundExceeded ||
          required <= 0 ||
          required > flarkV3NativeMaximumFrameBytes) {
        throw FlarkV3NativeEndpointException(
          operation: 'encodeSize',
          status: queryStatus,
          detail: 'required=$required',
        );
      }
      final output = calloc<Uint8>(required);
      try {
        written.value = 0;
        final status = _symbols.encode(
          nativeHandle.ref,
          output,
          required,
          written,
        );
        _requireOk('encode', status);
        if (written.value != required) {
          throw FlarkV3NativeEndpointException(
            operation: 'encodeLength',
            status: 0x0111,
            detail: 'required=$required written=${written.value}',
          );
        }
        return Uint8List.fromList(output.asTypedList(required));
      } finally {
        calloc.free(output);
      }
    } finally {
      calloc.free(written);
      calloc.free(nativeHandle);
    }
  }

  int remove(FlarkV3NativeEndpointHandle handle) {
    _requireLive();
    final nativeHandle = calloc<_NativeEndpointHandle>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      return _symbols.remove(nativeHandle.ref);
    } finally {
      calloc.free(nativeHandle);
    }
  }

  int emergencyDestroy(FlarkV3NativeEndpointHandle handle) {
    _requireLive();
    final nativeHandle = calloc<_NativeEndpointHandle>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      return _symbols.emergencyDestroy(nativeHandle.ref);
    } finally {
      calloc.free(nativeHandle);
    }
  }

  FlarkV3NativeEndpointFinalizerLease attachEmergencyFinalizer(
    Finalizable owner,
    FlarkV3NativeEndpointHandle handle,
  ) {
    _requireLive();
    final nativeHandle = calloc<_NativeEndpointHandle>();
    final output = calloc<Pointer<Void>>();
    try {
      _writeHandle(nativeHandle.ref, handle);
      final status = _symbols.finalizerTokenCreate(nativeHandle.ref, output);
      _requireOk('finalizerTokenCreate', status);
      final token = output.value;
      if (token == nullptr) {
        throw const FlarkV3NativeEndpointException(
          operation: 'finalizerTokenCreate',
          status: 0x0111,
          detail: 'native returned a null token',
        );
      }
      final lease = FlarkV3NativeEndpointFinalizerLease._(token);
      try {
        _emergencyFinalizer.attach(owner, token, detach: lease._detachKey);
      } catch (_) {
        final release = _symbols.finalizerTokenRelease(token);
        if (release == flarkV3NativeStatusOk) lease._consumed = true;
        rethrow;
      }
      return lease;
    } finally {
      calloc.free(output);
      calloc.free(nativeHandle);
    }
  }

  void detachEmergencyFinalizer(FlarkV3NativeEndpointFinalizerLease lease) {
    _requireLive();
    if (lease._consumed) {
      throw StateError('Native finalizer token is already consumed.');
    }
    if (!lease._attached) return;
    _emergencyFinalizer.detach(lease._detachKey);
    lease._attached = false;
  }

  void reattachEmergencyFinalizer(
    Finalizable owner,
    FlarkV3NativeEndpointFinalizerLease lease,
  ) {
    _requireLive();
    if (lease._consumed) {
      throw StateError('Native finalizer token is already consumed.');
    }
    if (lease._attached) return;
    _emergencyFinalizer.attach(owner, lease._token, detach: lease._detachKey);
    lease._attached = true;
  }

  int releaseEmergencyFinalizer(FlarkV3NativeEndpointFinalizerLease lease) {
    _requireLive();
    if (lease._attached) {
      throw StateError('Detach the native finalizer before releasing it.');
    }
    if (lease._consumed) {
      throw StateError('Native finalizer token is already consumed.');
    }
    final status = _symbols.finalizerTokenRelease(lease._token);
    if (status == flarkV3NativeStatusOk) lease._consumed = true;
    return status;
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    calloc.free(_config);
  }

  void _requireLive() {
    if (_disposed) throw StateError('Native endpoint bindings are disposed.');
  }

  static FlarkV3NativeEndpointHandle _handle(
    _NativeEndpointHandle value, {
    required String operation,
  }) {
    try {
      return FlarkV3NativeEndpointHandle(
        slot: value.slot,
        generation: value.generation,
      );
    } on RangeError catch (error) {
      throw FlarkV3NativeEndpointException(
        operation: operation,
        status: 0x0111,
        detail: 'invalid handle: $error',
      );
    }
  }

  static void _writeHandle(
    _NativeEndpointHandle output,
    FlarkV3NativeEndpointHandle handle,
  ) {
    output
      ..slot = handle.slot
      ..generation = handle.generation;
  }

  static FlarkV3NativeDispatchReceipt _dispatchReceipt(
    _NativeDispatchReceipt value,
  ) => FlarkV3NativeDispatchReceipt(
    correlationId: value.correlationId,
    action: value.action,
    outstandingEventId: value.outstandingEventId,
    outstandingEventKind: value.outstandingEventKind,
  );

  static FlarkV3NativePollReceipt _pollReceipt(_NativePollReceipt value) =>
      FlarkV3NativePollReceipt(
        sourceBytesExamined: value.sourceBytesExamined,
        sourceBytesBuffered: value.sourceBytesBuffered,
        cursorRefills: value.cursorRefills,
        cursorCopyBytesUpperBound: value.cursorCopyBytesUpperBound,
        checkpointsEmitted: value.checkpointsEmitted,
        sourceFactTransitions: value.sourceFactTransitions,
        releasedSourceLeases: value.releasedSourceLeases,
        releasedSourceBytes: value.releasedSourceBytes,
        arenaTransitions: value.arenaTransitions,
        arenaNodesReclaimed: value.arenaNodesReclaimed,
        scanComplete: value.scanComplete != 0,
        certification: value.certification,
        cleanupComplete: value.cleanupComplete != 0,
        outstandingEventId: value.outstandingEventId,
        outstandingEventKind: value.outstandingEventKind,
      );

  static void _requireOk(String operation, int status) {
    if (status != flarkV3NativeStatusOk) {
      throw FlarkV3NativeEndpointException(
        operation: operation,
        status: status,
      );
    }
  }
}

final class _NativeEndpointHandle extends Struct {
  @Uint32()
  external int slot;

  @Uint32()
  external int generation;
}

final class _NativeEndpointConfig extends Struct {
  @Uint32()
  external int abiVersion;

  @Uint32()
  external int structSize;

  @Uint32()
  external int maxRetiredSources;

  @Uint32()
  external int maxRetiredSourceBytes;

  @Uint32()
  external int arenaMaxSlots;

  @Uint32()
  external int arenaMaxLivePayloadBytes;

  @Uint32()
  external int arenaMaxChildrenPerNode;

  @Uint32()
  external int checkpointSpacingUtf16;

  @Uint32()
  external int sourceFactsMaxCheckpoints;

  @Uint32()
  external int sourceFactsMaxPages;

  @Uint32()
  external int sourceFactsMaxResidentBytes;

  @Uint32()
  external int parserProfile;

  @Array(4)
  external Array<Uint32> reserved;
}

final class _NativeSessionBinding extends Struct {
  @Array(4)
  external Array<Uint32> documentSession;

  @Uint32()
  external int sourceSessionIdentity;

  @Uint32()
  external int workerGeneration;
}

final class _NativeDispatchReceipt extends Struct {
  @Uint32()
  external int correlationId;

  @Uint32()
  external int action;

  @Uint32()
  external int outstandingEventId;

  @Uint32()
  external int outstandingEventKind;
}

final class _NativePollFuel extends Struct {
  @Uint32()
  external int maximumSourceBytes;

  @Uint32()
  external int maximumCheckpoints;

  @Uint32()
  external int maximumRetirementTransitions;
}

final class _NativePollReceipt extends Struct {
  @Uint32()
  external int sourceBytesExamined;

  @Uint32()
  external int sourceBytesBuffered;

  @Uint32()
  external int cursorRefills;

  @Uint32()
  external int cursorCopyBytesUpperBound;

  @Uint32()
  external int checkpointsEmitted;

  @Uint32()
  external int sourceFactTransitions;

  @Uint32()
  external int releasedSourceLeases;

  @Uint32()
  external int releasedSourceBytes;

  @Uint32()
  external int arenaTransitions;

  @Uint32()
  external int arenaNodesReclaimed;

  @Uint32()
  external int scanComplete;

  @Uint32()
  external int certification;

  @Uint32()
  external int cleanupComplete;

  @Uint32()
  external int outstandingEventId;

  @Uint32()
  external int outstandingEventKind;
}

final class _NativeCandidatePollReceipt extends Struct {
  @Uint32()
  external int transitions;

  @Uint32()
  external int cleanupComplete;

  @Uint32()
  external int outstandingEventId;

  @Uint32()
  external int outstandingEventKind;
}

typedef _AbiVersionNative = Uint32 Function();
typedef _AbiVersionDart = int Function();
typedef _CheckpointBProbeNative =
    Uint32 Function(
      Pointer<Uint8> output,
      Uint32 outputCapacity,
      Pointer<Uint32> written,
    );
typedef _CheckpointBProbeDart =
    int Function(
      Pointer<Uint8> output,
      int outputCapacity,
      Pointer<Uint32> written,
    );
typedef _StandardConfigNative =
    Uint32 Function(Pointer<_NativeEndpointConfig> output);
typedef _StandardConfigDart =
    int Function(Pointer<_NativeEndpointConfig> output);
typedef _CreateNative =
    Uint32 Function(
      Pointer<_NativeEndpointConfig> config,
      Pointer<_NativeEndpointHandle> output,
    );
typedef _CreateDart =
    int Function(
      Pointer<_NativeEndpointConfig> config,
      Pointer<_NativeEndpointHandle> output,
    );
typedef _RecoverNative =
    Uint32 Function(
      Pointer<_NativeEndpointConfig> config,
      Pointer<_NativeSessionBinding> previous,
      Pointer<_NativeEndpointHandle> output,
    );
typedef _RecoverDart =
    int Function(
      Pointer<_NativeEndpointConfig> config,
      Pointer<_NativeSessionBinding> previous,
      Pointer<_NativeEndpointHandle> output,
    );
typedef _DispatchNative =
    Uint32 Function(
      _NativeEndpointHandle handle,
      Pointer<Uint8> input,
      Uint32 inputLength,
      Pointer<_NativeDispatchReceipt> output,
    );
typedef _DispatchDart =
    int Function(
      _NativeEndpointHandle handle,
      Pointer<Uint8> input,
      int inputLength,
      Pointer<_NativeDispatchReceipt> output,
    );
typedef _PollNative =
    Uint32 Function(
      _NativeEndpointHandle handle,
      _NativePollFuel fuel,
      Pointer<_NativePollReceipt> output,
    );
typedef _PollDart =
    int Function(
      _NativeEndpointHandle handle,
      _NativePollFuel fuel,
      Pointer<_NativePollReceipt> output,
    );
typedef _CandidatePollNative =
    Uint32 Function(
      _NativeEndpointHandle handle,
      Uint32 maximumTransitions,
      Pointer<_NativeCandidatePollReceipt> output,
    );
typedef _CandidatePollDart =
    int Function(
      _NativeEndpointHandle handle,
      int maximumTransitions,
      Pointer<_NativeCandidatePollReceipt> output,
    );
typedef _EncodeNative =
    Uint32 Function(
      _NativeEndpointHandle handle,
      Pointer<Uint8> output,
      Uint32 outputCapacity,
      Pointer<Uint32> written,
    );
typedef _EncodeDart =
    int Function(
      _NativeEndpointHandle handle,
      Pointer<Uint8> output,
      int outputCapacity,
      Pointer<Uint32> written,
    );
typedef _DestroyNative = Uint32 Function(_NativeEndpointHandle handle);
typedef _DestroyDart = int Function(_NativeEndpointHandle handle);
typedef _FinalizerTokenCreateNative =
    Uint32 Function(
      _NativeEndpointHandle handle,
      Pointer<Pointer<Void>> output,
    );
typedef _FinalizerTokenCreateDart =
    int Function(_NativeEndpointHandle handle, Pointer<Pointer<Void>> output);
typedef _FinalizerTokenReleaseNative = Uint32 Function(Pointer<Void> token);
typedef _FinalizerTokenReleaseDart = int Function(Pointer<Void> token);

final class _FlarkV3NativeSymbols {
  const _FlarkV3NativeSymbols({
    required this.abiVersion,
    required this.standardConfig,
    required this.create,
    required this.recover,
    required this.dispatch,
    required this.dispatchHostPoll,
    required this.dispatchInlineSidecarHostPoll,
    required this.dispatchViewportPresentationHostPoll,
    required this.poll,
    required this.pollCandidate,
    required this.encode,
    required this.close,
    required this.remove,
    required this.emergencyDestroy,
    required this.finalizerTokenCreate,
    required this.finalizerTokenRelease,
    required this.emergencyFinalize,
  });

  factory _FlarkV3NativeSymbols.fromLibrary(
    DynamicLibrary library,
  ) => _FlarkV3NativeSymbols(
    abiVersion: library.lookupFunction<_AbiVersionNative, _AbiVersionDart>(
      'flark_v3_endpoint_native_abi_version',
    ),
    standardConfig: library
        .lookupFunction<_StandardConfigNative, _StandardConfigDart>(
          'flark_v3_endpoint_config_standard',
        ),
    create: library.lookupFunction<_CreateNative, _CreateDart>(
      'flark_v3_endpoint_create',
    ),
    recover: library.lookupFunction<_RecoverNative, _RecoverDart>(
      'flark_v3_endpoint_recover',
    ),
    dispatch: library.lookupFunction<_DispatchNative, _DispatchDart>(
      'flark_v3_endpoint_dispatch',
    ),
    dispatchHostPoll: library.lookupFunction<_DispatchNative, _DispatchDart>(
      'flark_v3_endpoint_dispatch_host_poll',
    ),
    dispatchInlineSidecarHostPoll: library
        .lookupFunction<_DispatchNative, _DispatchDart>(
          'flark_v3_endpoint_dispatch_inline_sidecar_host_poll',
        ),
    dispatchViewportPresentationHostPoll: library
        .lookupFunction<_DispatchNative, _DispatchDart>(
          'flark_v3_endpoint_dispatch_viewport_presentation_host_poll',
        ),
    poll: library.lookupFunction<_PollNative, _PollDart>(
      'flark_v3_endpoint_poll',
    ),
    pollCandidate: library
        .lookupFunction<_CandidatePollNative, _CandidatePollDart>(
          'flark_v3_endpoint_poll_candidate',
        ),
    encode: library.lookupFunction<_EncodeNative, _EncodeDart>(
      'flark_v3_endpoint_encode',
    ),
    close: library.lookupFunction<_DispatchNative, _DispatchDart>(
      'flark_v3_endpoint_close',
    ),
    remove: library.lookupFunction<_DestroyNative, _DestroyDart>(
      'flark_v3_endpoint_remove',
    ),
    emergencyDestroy: library.lookupFunction<_DestroyNative, _DestroyDart>(
      'flark_v3_endpoint_emergency_destroy',
    ),
    finalizerTokenCreate: library
        .lookupFunction<_FinalizerTokenCreateNative, _FinalizerTokenCreateDart>(
          'flark_v3_endpoint_finalizer_token_create',
        ),
    finalizerTokenRelease: library
        .lookupFunction<
          _FinalizerTokenReleaseNative,
          _FinalizerTokenReleaseDart
        >('flark_v3_endpoint_finalizer_token_release'),
    emergencyFinalize: library
        .lookup<NativeFunction<Void Function(Pointer<Void>)>>(
          'flark_v3_endpoint_emergency_finalize',
        ),
  );

  final _AbiVersionDart abiVersion;
  final _StandardConfigDart standardConfig;
  final _CreateDart create;
  final _RecoverDart recover;
  final _DispatchDart dispatch;
  final _DispatchDart dispatchHostPoll;
  final _DispatchDart dispatchInlineSidecarHostPoll;
  final _DispatchDart dispatchViewportPresentationHostPoll;
  final _PollDart poll;
  final _CandidatePollDart pollCandidate;
  final _EncodeDart encode;
  final _DispatchDart close;
  final _DestroyDart remove;
  final _DestroyDart emergencyDestroy;
  final _FinalizerTokenCreateDart finalizerTokenCreate;
  final _FinalizerTokenReleaseDart finalizerTokenRelease;
  final Pointer<NativeFunction<Void Function(Pointer<Void>)>> emergencyFinalize;
}
