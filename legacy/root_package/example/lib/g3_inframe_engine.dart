// G3 — in-frame synchronous parse spike (RFC 024 §4.1).
//
// This file contains NO Flutter imports so the same engine can be driven from
// a pure-Dart probe and from a real Flutter frame callback.
//
// What it replaces: `FlarkV3NativeIsolateByteEndpoint`, which spawns a
// long-lived isolate whose `_NativeEndpointWorker` owns the FFI handle and
// self-schedules its poll loop through its own `ReceivePort`. Here the exact
// same FFI entry points (`flark_v3_endpoint_dispatch`, `..._poll`,
// `..._poll_candidate`, `..._encode`) are called directly on the calling
// isolate, and the poll loop is advanced one step at a time by whoever owns
// the frame.
//
// Nothing in `lib/` is modified. The spike only imports package-private files.

// ignore_for_file: implementation_imports

import 'dart:collection';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_byte_endpoint.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_executor.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_parser_transport.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_endpoint_bindings.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_host_store.dart';
import 'package:flark/src/v3/runtime/native/flark_v3_native_isolate_byte_endpoint.dart'
    show FlarkV3NativeDeferredDispatchSlot;
import 'package:flark/src/v3/runtime/native/flark_v3_native_library_locator.dart';
import 'package:flark/src/v3/session/session.dart';
import 'package:flark/src/v3/source/source.dart';

/// Which native entry point one queued command frame belongs to.
enum G3CommandLane {
  session,
  strictClose,
  hostPoll,
  inlineSidecarHostPoll,
  viewportPresentationHostPoll,
}

final class _G3PendingCommand {
  const _G3PendingCommand(this.frame, this.lane);
  const _G3PendingCommand.dispose() : frame = null, lane = G3CommandLane.session;

  final Uint8List? frame;
  final G3CommandLane lane;

  bool get isDispose => frame == null;
}

/// What one endpoint step actually did. Used only for instrumentation.
enum G3StepKind { idle, deliverEvent, dispatch, poll, dispose }

/// Synchronous, same-isolate port of `_NativeEndpointWorker`.
///
/// Every decision here is copied from
/// `lib/src/v3/runtime/native/flark_v3_native_isolate_byte_endpoint.dart`
/// (`_NativeEndpointWorker._dispatchFrame` / `._poll` / `._emitOutstanding`).
/// The only structural change is that the two isolate ports become two local
/// queues, and `_schedulePoll`'s `commands.sendPort.send([_commandPoll])`
/// becomes a `_pollPending` flag drained by [step].
final class G3InFrameByteEndpoint implements FlarkV3ByteEndpoint {
  G3InFrameByteEndpoint._(this._bindings, this._handle);

  factory G3InFrameByteEndpoint.start({String? libraryPath}) {
    final library = openFlarkV3NativeLibrary(overrideLibraryPath: libraryPath);
    final bindings = FlarkV3NativeEndpointBindings.load(library);
    final handle = bindings.create();
    return G3InFrameByteEndpoint._(bindings, handle);
  }

  final FlarkV3NativeEndpointBindings _bindings;
  FlarkV3NativeEndpointHandle _handle;

  final Queue<_G3PendingCommand> _outbound = Queue<_G3PendingCommand>();
  final Queue<Uint8List> _inbound = Queue<Uint8List>();
  final FlarkV3NativeDeferredDispatchSlot _blockedDispatch =
      FlarkV3NativeDeferredDispatchSlot();

  FlarkV3ByteFrameCallback? _onFrame;
  FlarkV3ByteEndpointFailureCallback? _onFailure;
  int? _deliveredEventId;
  int? _deliveredEventKind;
  bool _pollPending = false;
  bool _closed = false;
  bool _disposed = false;
  (Object, StackTrace)? _terminalFailure;

  // Instrumentation.
  int dispatchCalls = 0;
  int pollCalls = 0;
  int candidatePollCalls = 0;
  int encodeCalls = 0;
  int encodedBytes = 0;
  int nativeMicros = 0;

  /// Poll fuel per synchronous poll step. Matches the isolate default.
  FlarkV3NativePollFuel pollFuel = const FlarkV3NativePollFuel();
  FlarkV3NativeCandidatePollFuel candidatePollFuel =
      const FlarkV3NativeCandidatePollFuel();

  bool get isDisposed => _disposed;
  bool get hasWork =>
      !_disposed && (_inbound.isNotEmpty || _outbound.isNotEmpty || _pollPending);

  @override
  void bind({
    required FlarkV3ByteFrameCallback onFrame,
    required FlarkV3ByteEndpointFailureCallback onFailure,
  }) {
    _onFrame = onFrame;
    _onFailure = onFailure;
  }

  @override
  void recover(FlarkV3ByteEndpointBinding previousBinding) {
    final replacement = _bindings.recover(previousBinding);
    final prior = _handle;
    _handle = replacement;
    _bindings.emergencyDestroy(prior);
    _deliveredEventId = null;
    _deliveredEventKind = null;
    _blockedDispatch.clear();
    _pollPending = false;
  }

  @override
  void send(Uint8List frame) => _enqueue(frame, G3CommandLane.session);

  @override
  void sendHostPoll(Uint8List frame) => _enqueue(frame, G3CommandLane.hostPoll);

  @override
  void sendInlineSidecarHostPoll(Uint8List frame) =>
      _enqueue(frame, G3CommandLane.inlineSidecarHostPoll);

  @override
  void sendViewportPresentationHostPoll(Uint8List frame) =>
      _enqueue(frame, G3CommandLane.viewportPresentationHostPoll);

  @override
  void sendClose(Uint8List frame) => _enqueue(frame, G3CommandLane.strictClose);

  @override
  void close() {
    if (_closed) return;
    _closed = true;
    _outbound.add(const _G3PendingCommand.dispose());
  }

  void _enqueue(Uint8List frame, G3CommandLane lane) {
    if (_closed || _disposed || _terminalFailure != null) {
      throw StateError('G3 in-frame endpoint is unavailable.');
    }
    if (frame.isEmpty || frame.length > flarkV3NativeMaximumFrameBytes) {
      throw RangeError.range(
        frame.length,
        1,
        flarkV3NativeMaximumFrameBytes,
        'frame.length',
      );
    }
    _outbound.add(_G3PendingCommand(Uint8List.fromList(frame), lane));
  }

  /// Performs at most one bounded unit of endpoint work on this thread.
  ///
  /// Ordering mirrors the isolate: credited events are consumed by the caller
  /// as promptly as the port would have delivered them; queued commands are
  /// dispatched in order; the poll loop advances one fuel grant per step.
  G3StepKind step() {
    if (_disposed || _terminalFailure != null) return G3StepKind.idle;
    try {
      if (_inbound.isNotEmpty) {
        final frame = _inbound.removeFirst();
        _onFrame!(frame);
        return G3StepKind.deliverEvent;
      }
      if (_outbound.isNotEmpty) {
        final command = _outbound.removeFirst();
        if (command.isDispose) {
          _dispose();
          return G3StepKind.dispose;
        }
        _dispatchFrame(command.frame!, command.lane);
        return G3StepKind.dispatch;
      }
      if (_pollPending) {
        _pollPending = false;
        _poll();
        return G3StepKind.poll;
      }
      return G3StepKind.idle;
    } catch (error, stackTrace) {
      _reportFailure(error, stackTrace);
      return G3StepKind.idle;
    }
  }

  void _dispatchFrame(Uint8List frame, G3CommandLane lane) {
    final watch = Stopwatch()..start();
    final result = switch (lane) {
      G3CommandLane.viewportPresentationHostPoll => _bindings
          .dispatchViewportPresentationHostPoll(_handle, frame),
      G3CommandLane.inlineSidecarHostPoll => _bindings
          .dispatchInlineSidecarHostPoll(_handle, frame),
      G3CommandLane.hostPoll => _bindings.dispatchHostPoll(_handle, frame),
      G3CommandLane.strictClose => _bindings.dispatch(
        _handle,
        frame,
        strictClose: true,
      ),
      G3CommandLane.session => _bindings.dispatch(
        _handle,
        frame,
        strictClose: false,
      ),
    };
    nativeMicros += watch.elapsedMicroseconds;
    dispatchCalls += 1;

    final hostResult =
        lane == G3CommandLane.hostPoll ||
        lane == G3CommandLane.inlineSidecarHostPoll ||
        lane == G3CommandLane.viewportPresentationHostPoll;
    final strictClose = lane == G3CommandLane.strictClose;

    if (result.receipt.hasOutstandingEvent) {
      final newlyDelivered = _isNewOutstanding(
        result.receipt.outstandingEventId,
        result.receipt.outstandingEventKind,
      );
      if (newlyDelivered) {
        _emitOutstanding(
          result.receipt.outstandingEventId,
          result.receipt.outstandingEventKind,
        );
      }
      if (result.status == flarkV3NativeStatusBackpressure) {
        _blockedDispatch.defer(
          frame,
          strictClose: strictClose,
          hostPoll: lane == G3CommandLane.hostPoll,
          inlineSidecarHostPoll: lane == G3CommandLane.inlineSidecarHostPoll,
          viewportPresentationHostPoll:
              lane == G3CommandLane.viewportPresentationHostPoll,
        );
      } else if (!newlyDelivered) {
        _requireStatus('dispatchWithOutstandingCredit', result.status);
      }
      if (!hostResult &&
          result.status == flarkV3NativeStatusOk &&
          result.receipt.action == flarkV3NativeActionCloseLatched) {
        _blockedDispatch.supersedeForClose();
      }
      return;
    }

    final completedEventKind = _deliveredEventKind;
    _deliveredEventId = null;
    _deliveredEventKind = null;
    _requireStatus('dispatch', result.status);
    if (!hostResult &&
        result.receipt.action == flarkV3NativeActionCloseLatched) {
      _blockedDispatch.supersedeForClose();
    } else if (!hostResult &&
        result.receipt.action == flarkV3NativeActionEventReceiptAccepted) {
      final blocked = _blockedDispatch.takeAfterReceipt(completedEventKind);
      if (blocked != null) {
        _dispatchFrame(
          blocked.frame,
          blocked.strictClose
              ? G3CommandLane.strictClose
              : blocked.hostPoll
              ? G3CommandLane.hostPoll
              : blocked.inlineSidecarHostPoll
              ? G3CommandLane.inlineSidecarHostPoll
              : blocked.viewportPresentationHostPoll
              ? G3CommandLane.viewportPresentationHostPoll
              : G3CommandLane.session,
        );
        return;
      }
    }
    _schedulePoll();
  }

  void _poll() {
    final watch = Stopwatch()..start();
    final sourceResult = _bindings.poll(_handle, pollFuel);
    nativeMicros += watch.elapsedMicroseconds;
    pollCalls += 1;
    if (sourceResult.receipt.hasOutstandingEvent) {
      if (_isNewOutstanding(
        sourceResult.receipt.outstandingEventId,
        sourceResult.receipt.outstandingEventKind,
      )) {
        _emitOutstanding(
          sourceResult.receipt.outstandingEventId,
          sourceResult.receipt.outstandingEventKind,
        );
      } else {
        _requireStatus('pollWithOutstandingCredit', sourceResult.status);
      }
      return;
    }
    _deliveredEventId = null;
    _deliveredEventKind = null;
    _requireStatus('poll', sourceResult.status);
    final sourceNeedsAnotherTurn =
        sourceResult.receipt.madeProgress &&
        !sourceResult.receipt.scanComplete &&
        !sourceResult.receipt.cleanupComplete;

    final candidateWatch = Stopwatch()..start();
    final candidateResult = _bindings.pollCandidate(_handle, candidatePollFuel);
    nativeMicros += candidateWatch.elapsedMicroseconds;
    candidatePollCalls += 1;
    if (candidateResult.receipt.hasOutstandingEvent) {
      if (_isNewOutstanding(
        candidateResult.receipt.outstandingEventId,
        candidateResult.receipt.outstandingEventKind,
      )) {
        _emitOutstanding(
          candidateResult.receipt.outstandingEventId,
          candidateResult.receipt.outstandingEventKind,
        );
      } else {
        _requireStatus(
          'pollCandidateWithOutstandingCredit',
          candidateResult.status,
        );
      }
      return;
    }
    _requireStatus('pollCandidate', candidateResult.status);
    if (sourceNeedsAnotherTurn ||
        candidateResult.receipt.madeProgress ||
        !candidateResult.receipt.cleanupComplete) {
      _schedulePoll();
    }
  }

  void _emitOutstanding(int eventId, int eventKind) {
    final watch = Stopwatch()..start();
    final frame = _bindings.encodeOutstanding(_handle);
    nativeMicros += watch.elapsedMicroseconds;
    encodeCalls += 1;
    encodedBytes += frame.length;
    _deliveredEventId = eventId;
    _deliveredEventKind = eventKind;
    _inbound.add(frame);
  }

  bool _isNewOutstanding(int eventId, int eventKind) =>
      eventId != _deliveredEventId || eventKind != _deliveredEventKind;

  void _schedulePoll() {
    if (_disposed || _pollPending || _blockedDispatch.isOccupied) return;
    _pollPending = true;
  }

  void _dispose() {
    if (_disposed) return;
    _disposed = true;
    _blockedDispatch.clear();
    var status = _bindings.remove(_handle);
    if (status != flarkV3NativeStatusOk) {
      status = _bindings.emergencyDestroy(_handle);
    }
    _bindings.dispose();
  }

  void _reportFailure(Object error, StackTrace stackTrace) {
    if (_terminalFailure != null) return;
    _terminalFailure = (error, stackTrace);
    _onFailure?.call(error, stackTrace);
  }

  static void _requireStatus(String operation, int status) {
    if (status != flarkV3NativeStatusOk) {
      throw FlarkV3NativeEndpointException(operation: operation, status: status);
    }
  }
}

/// Replaces the executor's event-queue scheduler with an explicit queue.
///
/// `FlarkV3SessionExecutor` normally hands its bounded turn to
/// `scheduleEventTask`, i.e. the Dart event loop. Here the owner of the frame
/// decides when a turn runs, which is what makes the whole pipeline in-frame.
final class G3ManualScheduler implements FlarkV3SessionTaskScheduler {
  final Queue<FlarkV3SessionExecutorCallback> _queue =
      Queue<FlarkV3SessionExecutorCallback>();

  bool get hasWork => _queue.isNotEmpty;

  @override
  void schedule(FlarkV3SessionExecutorCallback callback) =>
      _queue.add(callback);

  bool runOne() {
    if (_queue.isEmpty) return false;
    _queue.removeFirst()();
    return true;
  }
}

/// Receipt for one bounded in-frame pump.
final class G3PumpReceipt {
  const G3PumpReceipt({
    required this.iterations,
    required this.driverTurns,
    required this.endpointSteps,
    required this.elapsedMicros,
    required this.budgetExhausted,
    required this.quiescent,
    required this.exactCurrent,
  });

  final int iterations;
  final int driverTurns;
  final int endpointSteps;
  final int elapsedMicros;
  final bool budgetExhausted;
  final bool quiescent;
  final bool exactCurrent;

  @override
  String toString() =>
      'pump(iters=$iterations turns=$driverTurns steps=$endpointSteps '
      '${elapsedMicros}us exhausted=$budgetExhausted quiescent=$quiescent '
      'exact=$exactCurrent)';
}

/// One document opened against the in-frame endpoint.
///
/// This is `FlarkV3DocumentRuntime.open` with two substitutions: the isolate
/// endpoint becomes [G3InFrameByteEndpoint], and the executor's scheduler
/// becomes [G3ManualScheduler]. Everything between — the wire transport, the
/// session driver, the host store, the publication protocol — is the shipped
/// code, unmodified.
final class G3InFrameDocument {
  G3InFrameDocument._({
    required this.session,
    required this.executor,
    required this.endpoint,
    required this.scheduler,
    required this.hostStore,
  });

  static G3InFrameDocument open(String markdown, {String? libraryPath}) {
    final documentSession = FlarkV3DocumentSessionId(
      0x464C4B33,
      0x11223344,
      0x55667788,
      0x99aabbcc,
    );
    final sourceSession = FlarkV3SourceSession.fromProvisionalString(markdown);
    final hostStore = FlarkV3NativeHostStore.create(
      library: openFlarkV3NativeLibrary(overrideLibraryPath: libraryPath),
      documentSession: documentSession,
    );
    final session = FlarkDocumentSession.attach(
      sourceSession: sourceSession,
      documentSession: documentSession,
      hostStore: hostStore,
      certifiedSourceVersion: FlarkV3SourceVersion.empty(documentSession),
    );
    final parserBinding = FlarkV3ParserSessionBinding(
      documentSession: documentSession,
      sourceSessionIdentity: sourceSession.sourceSessionIdentity,
      workerGeneration: sourceSession.workerGeneration,
    );
    final endpoint = G3InFrameByteEndpoint.start(libraryPath: libraryPath);
    late final G3InFrameDocument document;
    final transport = FlarkV3WireParserTransport(
      endpoint: endpoint,
      onFailure: (error, stackTrace) => document._recordFailure(error, stackTrace),
    );
    final scheduler = G3ManualScheduler();
    final executor = FlarkV3SessionExecutor.attach(
      session: session,
      transport: transport,
      parserBinding: parserBinding,
      publicationAuthority: FlarkV3ParserPublicationAuthority(
        grammarRevision: flarkV3CurrentGrammarRevision,
        syntaxProfile: FlarkV3SyntaxProfileId(1),
        authorityMask: FlarkV3StructuralAuthorityMask.complete,
      ),
      scheduler: scheduler,
      onFailure: (error, stackTrace) => document._recordFailure(error, stackTrace),
    );
    document = G3InFrameDocument._(
      session: session,
      executor: executor,
      endpoint: endpoint,
      scheduler: scheduler,
      hostStore: hostStore,
    );
    return document;
  }

  final FlarkDocumentSession session;
  final FlarkV3SessionExecutor executor;
  final G3InFrameByteEndpoint endpoint;
  final G3ManualScheduler scheduler;
  final FlarkV3HostStore hostStore;

  Object? failure;
  StackTrace? failureStack;

  void _recordFailure(Object error, [StackTrace? stackTrace]) {
    if (failure != null) return;
    failure = error;
    failureStack = stackTrace;
  }

  /// The driver's own diagnosis, when the protocol failed in-band.
  String get failureDiagnosis {
    final parser = executor.lastFailure;
    final publication = executor.lastPublicationFailure;
    final host = executor.lastHostRejection;
    return 'state=${executor.state.name} '
        'parserFailure=${parser?.failureCode} '
        'publicationFailure=${publication?.failureCode} '
        'hostRejection=${host?.reason.name}';
  }

  /// True when the parser has certified current source AND the installed
  /// structural root is authoritative for it. Identical to the condition
  /// `FlarkV3DocumentRuntimeStatus.sourceCurrent && .structureCurrent`.
  bool get isExactCurrent =>
      session.currentUiSourceCertified &&
      session.sourceWorkerSynchronized &&
      session.presentationState is FlarkV3ExactStructuralPresentation;

  bool get hasWork => scheduler.hasWork || endpoint.hasWork;

  int get sourceLengthUtf16 => session.source.utf16Length;
  int get uiRevision => session.uiRevision;

  /// Applies one exact source transaction. Mirrors
  /// `FlarkV3DocumentRuntime.apply`.
  void applyInsert(int offsetUtf16, String text) {
    final receipt = session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: offsetUtf16,
          endUtf16: offsetUtf16,
          replacement: text,
        ),
      ),
    );
    if (receipt.changed) executor.sourceChanged();
  }

  /// One bounded synchronous pump. This is the whole of RFC 024 §4.1:
  /// drive the parser to quiescence, or spend the budget and stop cleanly.
  G3PumpReceipt pump({required int budgetMicros, int maximumIterations = 100000}) {
    final watch = Stopwatch()..start();
    var iterations = 0;
    var driverTurns = 0;
    var endpointSteps = 0;
    var budgetExhausted = false;
    while (true) {
      if (iterations >= maximumIterations) {
        budgetExhausted = true;
        break;
      }
      if (watch.elapsedMicroseconds >= budgetMicros) {
        budgetExhausted = hasWork;
        break;
      }
      var progressed = false;
      if (scheduler.runOne()) {
        driverTurns += 1;
        progressed = true;
      }
      if (endpoint.step() != G3StepKind.idle) {
        endpointSteps += 1;
        progressed = true;
      }
      if (!progressed) break;
      iterations += 1;
    }
    watch.stop();
    return G3PumpReceipt(
      iterations: iterations,
      driverTurns: driverTurns,
      endpointSteps: endpointSteps,
      elapsedMicros: watch.elapsedMicroseconds,
      budgetExhausted: budgetExhausted,
      quiescent: !hasWork,
      exactCurrent: isExactCurrent,
    );
  }

  void dispose() {
    endpoint.close();
    // Drain the dispose command without re-entering the driver.
    while (endpoint.step() != G3StepKind.idle) {}
    hostStore.close();
  }
}
