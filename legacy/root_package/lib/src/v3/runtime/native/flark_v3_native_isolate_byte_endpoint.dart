import 'dart:async';
import 'dart:ffi';
import 'dart:isolate';
import 'dart:typed_data';

import '../flark_v3_byte_endpoint.dart';
import '../flark_v3_wire_protocol.dart';
import 'flark_v3_native_endpoint_bindings.dart';
import 'flark_v3_native_library_locator.dart';

const int _commandDispatch = 0;
const int _commandStrictClose = 1;
const int _commandRecover = 2;
const int _commandPoll = 3;
const int _commandDispose = 4;
const int _commandAbandon = 5;
const int _commandInitialize = 6;
const int _commandDispatchHostPoll = 7;
const int _commandDispatchInlineSidecarHostPoll = 8;
const int _commandDispatchViewportPresentationHostPoll = 9;

const int _eventReady = 0;
const int _eventFrame = 1;
const int _eventFailure = 2;
const int _eventDisposed = 3;
const int _eventControl = 4;

const Duration _startupTeardownTimeout = Duration(seconds: 5);

final Finalizer<_NativeEndpointMainCleanup> _nativeEndpointFinalizer =
    Finalizer<_NativeEndpointMainCleanup>((cleanup) => cleanup.abandon());

/// The worker's one-command backpressure cell.
///
/// A close latch supersedes every earlier open-state command. A failed or
/// closed event receipt also ends the lifecycle in which a deferred command
/// was valid, so it must be discarded rather than replayed into the next
/// lifecycle. The class is package-visible so those terminal transitions can
/// be tested without manufacturing an FFI failure.
final class FlarkV3NativeDeferredDispatchSlot {
  FlarkV3NativeDeferredDispatch? _dispatch;

  bool get isOccupied => _dispatch != null;

  void defer(
    Uint8List frame, {
    required bool strictClose,
    bool hostPoll = false,
    bool inlineSidecarHostPoll = false,
    bool viewportPresentationHostPoll = false,
  }) {
    if ((strictClose ? 1 : 0) +
            (hostPoll ? 1 : 0) +
            (inlineSidecarHostPoll ? 1 : 0) +
            (viewportPresentationHostPoll ? 1 : 0) >
        1) {
      throw ArgumentError('A dispatch must select exactly one protocol lane.');
    }
    final blocked = _dispatch;
    if (blocked != null) {
      if (blocked.strictClose == strictClose &&
          blocked.hostPoll == hostPoll &&
          blocked.inlineSidecarHostPoll == inlineSidecarHostPoll &&
          blocked.viewportPresentationHostPoll ==
              viewportPresentationHostPoll &&
          _sameBytes(blocked.frame, frame)) {
        return;
      }
      throw StateError('Native endpoint exceeded one deferred command.');
    }
    _dispatch = FlarkV3NativeDeferredDispatch(
      frame: Uint8List.fromList(frame),
      strictClose: strictClose,
      hostPoll: hostPoll,
      inlineSidecarHostPoll: inlineSidecarHostPoll,
      viewportPresentationHostPoll: viewportPresentationHostPoll,
    );
  }

  /// Invalidates work accepted before a strict close command.
  void supersedeForClose() => _dispatch = null;

  /// Returns deferred work only when the credited lifecycle remains usable.
  FlarkV3NativeDeferredDispatch? takeAfterReceipt(int? completedEventKind) {
    final blocked = _dispatch;
    _dispatch = null;
    if (completedEventKind == flarkV3NativeEventKindFailed ||
        completedEventKind == flarkV3NativeEventKindClosed) {
      return null;
    }
    return blocked;
  }

  void clear() => _dispatch = null;

  static bool _sameBytes(Uint8List left, Uint8List right) {
    if (left.length != right.length) return false;
    for (var index = 0; index < left.length; index += 1) {
      if (left[index] != right[index]) return false;
    }
    return true;
  }
}

final class FlarkV3NativeDeferredDispatch {
  const FlarkV3NativeDeferredDispatch({
    required this.frame,
    required this.strictClose,
    this.hostPoll = false,
    this.inlineSidecarHostPoll = false,
    this.viewportPresentationHostPoll = false,
  });

  final Uint8List frame;
  final bool strictClose;
  final bool hostPoll;
  final bool inlineSidecarHostPoll;
  final bool viewportPresentationHostPoll;
}

final class _NativeEndpointMainCleanup {
  _NativeEndpointMainCleanup({
    required this.isolate,
    required this.commands,
    required this.events,
    required this.errors,
    required this.exits,
    required this.eventSubscription,
    required this.errorSubscription,
    required this.exitSubscription,
  });

  final Isolate isolate;
  final SendPort commands;
  final ReceivePort events;
  final ReceivePort errors;
  final ReceivePort exits;
  final StreamSubscription<Object?> eventSubscription;
  final StreamSubscription<Object?> errorSubscription;
  final StreamSubscription<Object?> exitSubscription;
  bool _finished = false;

  void abandon() {
    if (_finished) return;
    _finished = true;
    commands.send(const <Object?>[_commandAbandon]);
    _closeCallerPorts();
  }

  void finish() {
    if (_finished) return;
    _finished = true;
    _closeCallerPorts();
    isolate.kill(priority: Isolate.immediate);
  }

  void _closeCallerPorts() {
    unawaited(eventSubscription.cancel());
    unawaited(errorSubscription.cancel());
    unawaited(exitSubscription.cancel());
    events.close();
    errors.close();
    exits.close();
  }
}

/// Native implementation of the platform byte seam.
///
/// The long-lived isolate owns dynamic-library resolution, the registry
/// handle, every FFI allocation, bounded source polling, and event encoding.
/// Only transferred FLK3 buffers and fixed-width recovery identity cross back
/// to the caller isolate.
final class FlarkV3NativeIsolateByteEndpoint implements FlarkV3ByteEndpoint {
  FlarkV3NativeIsolateByteEndpoint._({
    required _NativeEndpointMainCleanup cleanup,
  }) : _cleanup = cleanup,
       _commands = cleanup.commands {
    _nativeEndpointFinalizer.attach(this, cleanup, detach: this);
  }

  static Future<FlarkV3NativeIsolateByteEndpoint> start({
    String? overrideLibraryPath,
    Duration startupTimeout = const Duration(seconds: 15),
  }) async {
    if (startupTimeout <= Duration.zero) {
      throw ArgumentError.value(
        startupTimeout,
        'startupTimeout',
        'must be positive',
      );
    }
    final events = ReceivePort('flark-v3-native-events');
    final errors = ReceivePort('flark-v3-native-errors');
    final exits = ReceivePort('flark-v3-native-exit');
    final control = Completer<SendPort>();
    final ready = Completer<SendPort>();
    final disposed = Completer<void>();
    // These protocol futures can complete before the startup coroutine reaches
    // its corresponding await. Own their errors immediately while retaining
    // the original futures for the ordered handshake below.
    control.future.ignore();
    ready.future.ignore();
    disposed.future.ignore();
    WeakReference<FlarkV3NativeIsolateByteEndpoint>? endpointReference;
    final pending = <Object?>[];
    (Object, StackTrace)? detachedFailure;
    var detachedExit = false;
    SendPort? bootstrapCommands;

    late final StreamSubscription<Object?> eventSubscription;
    eventSubscription = events.listen((message) {
      if (message case <Object?>[_eventDisposed]) {
        if (!disposed.isCompleted) disposed.complete();
      }
      final live = endpointReference?.target;
      if (live != null) {
        live._receive(message);
        return;
      }
      if (message case <Object?>[_eventControl, final SendPort commands]) {
        if (bootstrapCommands != null || control.isCompleted) {
          final failure = StateError(
            'Native parser isolate sent a duplicate control port.',
          );
          if (!ready.isCompleted) {
            ready.completeError(failure, StackTrace.current);
          } else {
            detachedFailure ??= (failure, StackTrace.current);
          }
          return;
        }
        bootstrapCommands = commands;
        control.complete(commands);
        return;
      }
      if (message case <Object?>[_eventReady, final SendPort commands]) {
        final expected = bootstrapCommands;
        if (expected == null || commands != expected) {
          if (!ready.isCompleted) {
            ready.completeError(
              StateError(
                'Native parser isolate became ready without its acknowledged '
                'bootstrap control port.',
              ),
              StackTrace.current,
            );
          }
        } else if (!ready.isCompleted) {
          ready.complete(commands);
        }
        return;
      }
      if (message case <Object?>[
        _eventFailure,
        final String operation,
        final int status,
        final String? detail,
      ]) {
        final failure = FlarkV3NativeEndpointException(
          operation: operation,
          status: status,
          detail: detail,
        );
        if (!control.isCompleted) {
          control.completeError(failure, StackTrace.current);
        }
        if (!ready.isCompleted) {
          ready.completeError(failure, StackTrace.current);
          return;
        }
      }
      pending.add(message);
    });

    late final StreamSubscription<Object?> errorSubscription;
    errorSubscription = errors.listen((message) {
      final failure = _isolateFailure(message);
      final live = endpointReference?.target;
      if (live != null) {
        live._reportFailure(failure.$1, failure.$2);
      } else {
        if (!control.isCompleted) {
          control.completeError(failure.$1, failure.$2);
        }
        if (!ready.isCompleted) {
          ready.completeError(failure.$1, failure.$2);
        } else {
          detachedFailure ??= failure;
        }
      }
    });

    late final StreamSubscription<Object?> exitSubscription;
    exitSubscription = exits.listen((_) {
      // Give already-sent event-port messages (especially `_eventDisposed`)
      // one event turn to arrive before classifying this as an unreceipted
      // exit. The receipt, never the exit notification, proves reclamation.
      Timer.run(() {
        final live = endpointReference?.target;
        if (live != null) {
          live._isolateExited();
          return;
        }
        final failure = StateError(
          'Native parser isolate exited during startup.',
        );
        if (!control.isCompleted) {
          control.completeError(failure, StackTrace.current);
        }
        if (!ready.isCompleted) {
          ready.completeError(failure, StackTrace.current);
        } else {
          detachedExit = true;
        }
        if (!disposed.isCompleted) {
          disposed.completeError(
            StateError(
              'Native parser isolate exited without a disposal receipt.',
            ),
            StackTrace.current,
          );
        }
      });
    });

    Isolate? isolate;
    SendPort? commands;
    final startupClock = Stopwatch()..start();
    try {
      isolate = await Isolate.spawn<List<Object?>>(
        _nativeEndpointIsolateMain,
        <Object?>[events.sendPort, overrideLibraryPath],
        debugName: 'flark-v3-parser',
        errorsAreFatal: true,
        onError: errors.sendPort,
        onExit: exits.sendPort,
      );
      commands = await control.future.timeout(
        _remainingStartupTime(
          startupClock,
          startupTimeout,
          phase: 'control handshake',
        ),
        onTimeout: () => throw TimeoutException(
          'Native parser control handshake exceeded $startupTimeout.',
          startupTimeout,
        ),
      );
      commands.send(const <Object?>[_commandInitialize]);
      final readyCommands = await ready.future.timeout(
        _remainingStartupTime(
          startupClock,
          startupTimeout,
          phase: 'native initialization',
        ),
        onTimeout: () => throw TimeoutException(
          'Native parser initialization exceeded $startupTimeout.',
          startupTimeout,
        ),
      );
      if (readyCommands != commands) {
        throw StateError(
          'Native parser ready port did not match its bootstrap control port.',
        );
      }
      final cleanup = _NativeEndpointMainCleanup(
        isolate: isolate,
        commands: commands,
        events: events,
        errors: errors,
        exits: exits,
        eventSubscription: eventSubscription,
        errorSubscription: errorSubscription,
        exitSubscription: exitSubscription,
      );
      final created = FlarkV3NativeIsolateByteEndpoint._(cleanup: cleanup);
      endpointReference = WeakReference(created);
      for (final message in pending) {
        created._receive(message);
      }
      final failure = detachedFailure;
      if (failure != null) created._reportFailure(failure.$1, failure.$2);
      if (detachedExit) created._isolateExited();
      return created;
    } catch (error, stackTrace) {
      Object? reclamationFailure;
      StackTrace? reclamationStackTrace;
      final controlPort = commands;
      if (controlPort != null) {
        controlPort.send(const <Object?>[_commandAbandon]);
        try {
          await disposed.future.timeout(
            _startupTeardownTimeout,
            onTimeout: () => throw TimeoutException(
              'Native parser startup reclamation did not produce a disposal '
              'receipt within $_startupTeardownTimeout.',
              _startupTeardownTimeout,
            ),
          );
        } catch (cleanupError, cleanupStackTrace) {
          reclamationFailure = cleanupError;
          reclamationStackTrace = cleanupStackTrace;
        }
      }
      // Before the control handshake the worker is forbidden to initialize;
      // after a disposal receipt its native handle is already proven retired.
      // Killing is safe in those two cases. If reclamation was not proven we
      // still stop the isolate, but surface that uncertainty as the result.
      isolate?.kill(priority: Isolate.immediate);
      await eventSubscription.cancel();
      await errorSubscription.cancel();
      await exitSubscription.cancel();
      events.close();
      errors.close();
      exits.close();
      if (reclamationFailure != null) {
        Error.throwWithStackTrace(
          FlarkV3NativeEndpointException(
            operation: 'startupReclamation',
            status: 0x0111,
            detail:
                'startup failure: $error; reclamation failure: '
                '$reclamationFailure',
          ),
          reclamationStackTrace ?? stackTrace,
        );
      }
      Error.throwWithStackTrace(error, stackTrace);
    }
  }

  final _NativeEndpointMainCleanup _cleanup;
  final SendPort _commands;
  final Completer<void> _done = Completer<void>();

  FlarkV3ByteFrameCallback? _onFrame;
  FlarkV3ByteEndpointFailureCallback? _onFailure;
  (Object, StackTrace)? _pendingFailure;
  (Object, StackTrace)? _terminalFailure;
  bool _closed = false;
  bool _failureReported = false;
  bool _portsClosed = false;

  /// Completes after the isolate has removed or emergency-revoked its handle.
  Future<void> get done => _done.future;

  @override
  void bind({
    required FlarkV3ByteFrameCallback onFrame,
    required FlarkV3ByteEndpointFailureCallback onFailure,
  }) {
    if (_closed) throw StateError('Native parser endpoint is closed.');
    if (_onFrame != null || _onFailure != null) {
      throw StateError('Native parser endpoint is already bound.');
    }
    _onFrame = onFrame;
    _onFailure = onFailure;
    final pending = _pendingFailure;
    _pendingFailure = null;
    if (pending != null) {
      _failureReported = true;
      onFailure(pending.$1, pending.$2);
    }
  }

  @override
  void recover(FlarkV3ByteEndpointBinding previousBinding) {
    _requireLive();
    _commands.send(<Object?>[
      _commandRecover,
      previousBinding.documentSessionWords,
      previousBinding.sourceSessionIdentity,
      previousBinding.workerGeneration,
    ]);
  }

  @override
  void send(Uint8List frame) => _sendFrame(_commandDispatch, frame);

  @override
  void sendHostPoll(Uint8List frame) =>
      _sendFrame(_commandDispatchHostPoll, frame);

  @override
  void sendInlineSidecarHostPoll(Uint8List frame) =>
      _sendFrame(_commandDispatchInlineSidecarHostPoll, frame);

  @override
  void sendViewportPresentationHostPoll(Uint8List frame) =>
      _sendFrame(_commandDispatchViewportPresentationHostPoll, frame);

  @override
  void sendClose(Uint8List frame) => _sendFrame(_commandStrictClose, frame);

  void _sendFrame(int command, Uint8List frame) {
    _requireLive();
    if (frame.isEmpty || frame.length > flarkV3NativeMaximumFrameBytes) {
      throw RangeError.range(
        frame.length,
        1,
        flarkV3NativeMaximumFrameBytes,
        'frame.length',
      );
    }
    _commands.send(<Object?>[
      command,
      TransferableTypedData.fromList(<TypedData>[frame]),
    ]);
  }

  @override
  void close() {
    if (_closed) return;
    _closed = true;
    _commands.send(const <Object?>[_commandDispose]);
  }

  void _receive(Object? message) {
    if (_portsClosed) return;
    switch (message) {
      case <Object?>[_eventFrame, final TransferableTypedData transferred]:
        if (_closed || _failureReported) return;
        final callback = _onFrame;
        if (callback == null) {
          _reportFailure(
            StateError('Native parser emitted before its callback was bound.'),
            StackTrace.current,
          );
          return;
        }
        callback(transferred.materialize().asUint8List());
      case <Object?>[
        _eventFailure,
        final String operation,
        final int status,
        final String? detail,
      ]:
        _reportFailure(
          FlarkV3NativeEndpointException(
            operation: operation,
            status: status,
            detail: detail,
          ),
          StackTrace.current,
        );
      case <Object?>[_eventDisposed]:
        _finishPorts();
      case <Object?>[_eventReady, SendPort()]:
        _reportFailure(
          StateError('Native parser endpoint sent a duplicate ready event.'),
          StackTrace.current,
        );
      default:
        _reportFailure(
          FormatException('Malformed native parser isolate message: $message'),
          StackTrace.current,
        );
    }
  }

  void _isolateExited() {
    if (_portsClosed) return;
    Timer.run(() {
      if (_portsClosed) return;
      _reportFailure(
        StateError(
          _closed
              ? 'Native parser isolate exited without a disposal receipt.'
              : 'Native parser isolate exited unexpectedly.',
        ),
        StackTrace.current,
      );
      _finishPorts();
    });
  }

  void _reportFailure(Object error, StackTrace stackTrace) {
    if (_terminalFailure != null || _portsClosed) return;
    _terminalFailure = (error, stackTrace);
    final callback = _onFailure;
    if (callback == null) {
      _pendingFailure = (error, stackTrace);
      return;
    }
    _failureReported = true;
    callback(error, stackTrace);
  }

  void _requireLive() {
    if (_closed || _terminalFailure != null) {
      throw StateError('Native parser endpoint is unavailable.');
    }
  }

  void _finishPorts() {
    if (_portsClosed) return;
    _portsClosed = true;
    _nativeEndpointFinalizer.detach(this);
    _cleanup.finish();
    if (!_done.isCompleted) {
      final failure = _terminalFailure;
      if (failure == null) {
        _done.complete();
      } else {
        _done.completeError(failure.$1, failure.$2);
      }
    }
  }
}

@pragma('vm:entry-point')
void _nativeEndpointIsolateMain(List<Object?> bootstrap) {
  final reply = bootstrap[0] as SendPort;
  final overrideLibraryPath = bootstrap[1] as String?;
  final commands = ReceivePort('flark-v3-native-commands');
  final bootstrapController = _NativeEndpointBootstrapController(
    reply: reply,
    commands: commands,
    overrideLibraryPath: overrideLibraryPath,
  );
  commands.listen(bootstrapController.receive);

  // Phase one is deliberately complete before native initialization can be
  // authorized. This makes a caller-side timeout before control-port receipt
  // safe: no registry handle can exist yet.
  reply.send(<Object?>[_eventControl, commands.sendPort]);
}

/// Owns the interval between isolate spawn and a fully constructed worker.
///
/// Native state is created only after `_commandInitialize`. Once creation is
/// authorized, an abandon command remains queued on [commands] while a
/// synchronous native call is in progress and is handled immediately when the
/// isolate regains control. A native call that never returns is the one
/// irreducible case: the caller times out waiting for `_eventDisposed` and
/// reports unproven reclamation instead of false success.
final class _NativeEndpointBootstrapController {
  _NativeEndpointBootstrapController({
    required this.reply,
    required this.commands,
    required this.overrideLibraryPath,
  });

  final SendPort reply;
  final ReceivePort commands;
  final String? overrideLibraryPath;

  _NativeEndpointWorker? _worker;
  bool _initializationStarted = false;
  bool _terminal = false;

  void receive(Object? message) {
    if (_terminal) return;
    final worker = _worker;
    if (worker != null) {
      worker.receive(message);
      return;
    }
    try {
      switch (message) {
        case <Object?>[_commandInitialize]:
          if (_initializationStarted) {
            throw StateError('Native endpoint initialization was duplicated.');
          }
          _initializationStarted = true;
          _initialize();
        case <Object?>[_commandAbandon] || <Object?>[_commandDispose]:
          _terminal = true;
          commands.close();
          reply.send(const <Object?>[_eventDisposed]);
        default:
          throw FormatException(
            'Native endpoint received a command before initialization: '
            '$message',
          );
      }
    } catch (error) {
      _failBeforeReady(error);
    }
  }

  void _initialize() {
    FlarkV3NativeEndpointBindings? bindings;
    FlarkV3NativeEndpointHandle? handle;
    try {
      final library = openFlarkV3NativeLibrary(
        overrideLibraryPath: overrideLibraryPath,
      );
      bindings = FlarkV3NativeEndpointBindings.load(library);
      handle = bindings.create();
      final worker = _NativeEndpointWorker(
        reply: reply,
        commands: commands,
        bindings: bindings,
        handle: handle,
      );
      _worker = worker;
      reply.send(<Object?>[_eventReady, commands.sendPort]);
    } catch (error) {
      _terminal = true;
      FlarkV3NativeEndpointException? reclamationFailure;
      if (handle != null && bindings != null) {
        try {
          final status = bindings.emergencyDestroy(handle);
          if (status != flarkV3NativeStatusOk) {
            reclamationFailure = FlarkV3NativeEndpointException(
              operation: 'startupEmergencyDestroy',
              status: status,
              detail: 'native handle reclamation was not proven',
            );
          }
        } catch (cleanupError) {
          reclamationFailure = FlarkV3NativeEndpointException(
            operation: 'startupEmergencyDestroy',
            status: 0x0111,
            detail: '$cleanupError',
          );
        }
      }
      bindings?.dispose();
      commands.close();
      _sendFailure(error);
      if (reclamationFailure == null) {
        reply.send(const <Object?>[_eventDisposed]);
      } else {
        _sendFailure(reclamationFailure);
      }
    }
  }

  void _failBeforeReady(Object error) {
    if (_terminal) return;
    _terminal = true;
    commands.close();
    _sendFailure(error);
    // Initialization has not produced a worker or handle, so this receipt is
    // truthful without consulting native state.
    reply.send(const <Object?>[_eventDisposed]);
  }

  void _sendFailure(Object error) {
    final status = error is FlarkV3NativeEndpointException
        ? error.status
        : 0x0111;
    final operation = error is FlarkV3NativeEndpointException
        ? error.operation
        : 'startup';
    reply.send(<Object?>[_eventFailure, operation, status, '$error']);
  }
}

final class _NativeEndpointWorker implements Finalizable {
  _NativeEndpointWorker({
    required this.reply,
    required this.commands,
    required this.bindings,
    required FlarkV3NativeEndpointHandle handle,
  }) : _handle = handle {
    _handleFinalizer = bindings.attachEmergencyFinalizer(this, handle);
  }

  final SendPort reply;
  final ReceivePort commands;
  final FlarkV3NativeEndpointBindings bindings;
  FlarkV3NativeEndpointHandle _handle;
  late FlarkV3NativeEndpointFinalizerLease _handleFinalizer;
  final FlarkV3NativePollFuel _pollFuel = const FlarkV3NativePollFuel();
  final FlarkV3NativeCandidatePollFuel _candidatePollFuel =
      const FlarkV3NativeCandidatePollFuel();
  int? _deliveredEventId;
  int? _deliveredEventKind;
  final FlarkV3NativeDeferredDispatchSlot _blockedDispatch =
      FlarkV3NativeDeferredDispatchSlot();
  bool _pollScheduled = false;
  bool _disposed = false;

  void receive(Object? message) {
    if (_disposed) return;
    try {
      switch (message) {
        case <Object?>[
          _commandDispatch,
          final TransferableTypedData transferred,
        ]:
          _dispatch(transferred, strictClose: false);
        case <Object?>[
          _commandStrictClose,
          final TransferableTypedData transferred,
        ]:
          _dispatch(transferred, strictClose: true, hostPoll: false);
        case <Object?>[
          _commandDispatchHostPoll,
          final TransferableTypedData transferred,
        ]:
          _dispatch(transferred, strictClose: false, hostPoll: true);
        case <Object?>[
          _commandDispatchInlineSidecarHostPoll,
          final TransferableTypedData transferred,
        ]:
          _dispatch(
            transferred,
            strictClose: false,
            inlineSidecarHostPoll: true,
          );
        case <Object?>[
          _commandDispatchViewportPresentationHostPoll,
          final TransferableTypedData transferred,
        ]:
          _dispatch(
            transferred,
            strictClose: false,
            viewportPresentationHostPoll: true,
          );
        case <Object?>[
          _commandRecover,
          final List<Object?> words,
          final int sourceSessionIdentity,
          final int workerGeneration,
        ]:
          _recover(words, sourceSessionIdentity, workerGeneration);
        case <Object?>[_commandPoll]:
          _pollScheduled = false;
          _poll();
        case <Object?>[_commandDispose]:
          _dispose(report: true, gracefulFirst: true);
        case <Object?>[_commandAbandon]:
          // Startup timeout/error recovery waits on this truthful receipt.
          // Finalizer-triggered abandon closes the caller ports first, so the
          // same event is harmless when nobody remains to observe it.
          _dispose(report: true, gracefulFirst: false);
        default:
          throw FormatException('Malformed native endpoint command: $message');
      }
    } catch (error) {
      final native = error is FlarkV3NativeEndpointException
          ? error
          : FlarkV3NativeEndpointException(
              operation: 'isolateCommand',
              status: 0x0111,
              detail: '$error',
            );
      reply.send(<Object?>[
        _eventFailure,
        native.operation,
        native.status,
        native.detail ?? '$native',
      ]);
      // The failure is causal, but successful emergency revocation still needs
      // its own receipt so the caller can settle endpoint ownership promptly
      // instead of inferring cleanup from an isolate-exit notification.
      _dispose(report: true, gracefulFirst: false);
    }
  }

  void _dispatch(
    TransferableTypedData transferred, {
    required bool strictClose,
    bool hostPoll = false,
    bool inlineSidecarHostPoll = false,
    bool viewportPresentationHostPoll = false,
  }) {
    final frame = transferred.materialize().asUint8List();
    _dispatchFrame(
      frame,
      strictClose: strictClose,
      hostPoll: hostPoll,
      inlineSidecarHostPoll: inlineSidecarHostPoll,
      viewportPresentationHostPoll: viewportPresentationHostPoll,
    );
  }

  void _dispatchFrame(
    Uint8List frame, {
    required bool strictClose,
    bool hostPoll = false,
    bool inlineSidecarHostPoll = false,
    bool viewportPresentationHostPoll = false,
  }) {
    final result = viewportPresentationHostPoll
        ? bindings.dispatchViewportPresentationHostPoll(_handle, frame)
        : inlineSidecarHostPoll
        ? bindings.dispatchInlineSidecarHostPoll(_handle, frame)
        : hostPoll
        ? bindings.dispatchHostPoll(_handle, frame)
        : bindings.dispatch(_handle, frame, strictClose: strictClose);
    final hostResult =
        hostPoll || inlineSidecarHostPoll || viewportPresentationHostPoll;
    final operation = viewportPresentationHostPoll
        ? 'dispatchViewportPresentationHostPoll'
        : inlineSidecarHostPoll
        ? 'dispatchInlineSidecarHostPoll'
        : hostPoll
        ? 'dispatchHostPoll'
        : 'dispatch';
    final opcode = frame.length >= FlarkV3WireProtocol.headerBytes
        ? FlarkV3WireOpcode.fromCode(
            ByteData.sublistView(
              frame,
              0,
              FlarkV3WireProtocol.headerBytes,
            ).getUint16(8, Endian.little),
          )
        : null;
    final diagnosedOperation = '$operation(${opcode?.name ?? 'malformed'})';
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
          hostPoll: hostPoll,
          inlineSidecarHostPoll: inlineSidecarHostPoll,
          viewportPresentationHostPoll: viewportPresentationHostPoll,
        );
      } else if (!newlyDelivered) {
        _requireStatus(
          '${diagnosedOperation}WithOutstandingCredit',
          result.status,
        );
      }
      if (!hostResult &&
          result.status == flarkV3NativeStatusOk &&
          result.receipt.action == flarkV3NativeActionCloseLatched) {
        _blockedDispatch.supersedeForClose();
      }
      return;
    }
    final completedEventKind = _deliveredEventKind;
    _clearDeliveredEvent();
    _requireStatus(diagnosedOperation, result.status);
    if (!hostResult &&
        result.receipt.action == flarkV3NativeActionCloseLatched) {
      _blockedDispatch.supersedeForClose();
    } else if (!hostResult &&
        result.receipt.action == flarkV3NativeActionEventReceiptAccepted) {
      final blocked = _blockedDispatch.takeAfterReceipt(completedEventKind);
      if (blocked != null) {
        _dispatchFrame(
          blocked.frame,
          strictClose: blocked.strictClose,
          hostPoll: blocked.hostPoll,
          inlineSidecarHostPoll: blocked.inlineSidecarHostPoll,
          viewportPresentationHostPoll: blocked.viewportPresentationHostPoll,
        );
        return;
      }
    }
    _schedulePoll();
  }

  void _recover(
    List<Object?> words,
    int sourceSessionIdentity,
    int workerGeneration,
  ) {
    if (words.length != 4 || words.any((word) => word is! int)) {
      throw const FormatException('Malformed recovery document identity.');
    }
    final previous = FlarkV3ByteEndpointBinding(
      documentSessionWords: words.cast<int>(),
      sourceSessionIdentity: sourceSessionIdentity,
      workerGeneration: workerGeneration,
    );
    final replacement = bindings.recover(previous);
    FlarkV3NativeEndpointFinalizerLease replacementFinalizer;
    try {
      replacementFinalizer = bindings.attachEmergencyFinalizer(
        this,
        replacement,
      );
    } catch (_) {
      _requireStatus(
        'recoverDestroyUnownedReplacement',
        bindings.emergencyDestroy(replacement),
      );
      rethrow;
    }
    final priorHandle = _handle;
    final priorFinalizer = _handleFinalizer;
    _handle = replacement;
    _handleFinalizer = replacementFinalizer;
    final failure = _retireHandle(
      priorHandle,
      priorFinalizer,
      gracefulFirst: false,
      operation: 'recoverRevokePrior',
    );
    if (failure != null) throw failure;
    _clearDeliveredEvent();
    _blockedDispatch.clear();
    _pollScheduled = false;
  }

  void _poll() {
    // One queued turn spends at most the declared source-fact grant plus the
    // independent candidate-transition grant. Either lane stops immediately
    // when it occupies the endpoint's one global event-credit cell.
    final sourceResult = bindings.poll(_handle, _pollFuel);
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
    _clearDeliveredEvent();
    _requireStatus('poll', sourceResult.status);
    final sourceNeedsAnotherTurn =
        sourceResult.receipt.madeProgress &&
        !sourceResult.receipt.scanComplete &&
        !sourceResult.receipt.cleanupComplete;

    final candidateResult = bindings.pollCandidate(_handle, _candidatePollFuel);
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
    final frame = bindings.encodeOutstanding(_handle);
    _deliveredEventId = eventId;
    _deliveredEventKind = eventKind;
    reply.send(<Object?>[
      _eventFrame,
      TransferableTypedData.fromList(<TypedData>[frame]),
    ]);
  }

  bool _isNewOutstanding(int eventId, int eventKind) =>
      eventId != _deliveredEventId || eventKind != _deliveredEventKind;

  void _clearDeliveredEvent() {
    _deliveredEventId = null;
    _deliveredEventKind = null;
  }

  void _schedulePoll() {
    if (_disposed || _pollScheduled || _blockedDispatch.isOccupied) return;
    _pollScheduled = true;
    commands.sendPort.send(const <Object?>[_commandPoll]);
  }

  void _dispose({required bool report, required bool gracefulFirst}) {
    if (_disposed) return;
    _disposed = true;
    _blockedDispatch.clear();
    FlarkV3NativeEndpointException? failure;
    try {
      failure = _retireHandle(
        _handle,
        _handleFinalizer,
        gracefulFirst: gracefulFirst,
        operation: gracefulFirst ? 'dispose' : 'abandon',
      );
    } catch (error) {
      failure = error is FlarkV3NativeEndpointException
          ? error
          : FlarkV3NativeEndpointException(
              operation: 'dispose',
              status: 0x0111,
              detail: '$error',
            );
    } finally {
      bindings.dispose();
      commands.close();
    }
    if (!report) return;
    if (failure == null) {
      reply.send(const <Object?>[_eventDisposed]);
    } else {
      reply.send(<Object?>[
        _eventFailure,
        failure.operation,
        failure.status,
        failure.detail ?? '$failure',
      ]);
    }
  }

  FlarkV3NativeEndpointException? _retireHandle(
    FlarkV3NativeEndpointHandle handle,
    FlarkV3NativeEndpointFinalizerLease finalizer, {
    required bool gracefulFirst,
    required String operation,
  }) {
    bindings.detachEmergencyFinalizer(finalizer);
    try {
      var status = gracefulFirst
          ? bindings.remove(handle)
          : bindings.emergencyDestroy(handle);
      if (gracefulFirst && status != flarkV3NativeStatusOk) {
        status = bindings.emergencyDestroy(handle);
      }
      if (status != flarkV3NativeStatusOk) {
        bindings.reattachEmergencyFinalizer(this, finalizer);
        return FlarkV3NativeEndpointException(
          operation: operation,
          status: status,
          detail: 'native handle reclamation was not proven',
        );
      }
      final release = bindings.releaseEmergencyFinalizer(finalizer);
      if (release != flarkV3NativeStatusOk) {
        bindings.reattachEmergencyFinalizer(this, finalizer);
        return FlarkV3NativeEndpointException(
          operation: '${operation}FinalizerRelease',
          status: release,
          detail: 'native finalizer token ownership was not released',
        );
      }
      return null;
    } catch (_) {
      bindings.reattachEmergencyFinalizer(this, finalizer);
      rethrow;
    }
  }

  static void _requireStatus(String operation, int status) {
    if (status != flarkV3NativeStatusOk) {
      throw FlarkV3NativeEndpointException(
        operation: operation,
        status: status,
      );
    }
  }
}

Duration _remainingStartupTime(
  Stopwatch clock,
  Duration limit, {
  required String phase,
}) {
  final remaining = limit - clock.elapsed;
  if (remaining <= Duration.zero) {
    throw TimeoutException(
      'Native parser startup exceeded $limit before $phase.',
      limit,
    );
  }
  return remaining;
}

(Object, StackTrace) _isolateFailure(Object? message) {
  if (message case <Object?>[final Object error, final String stack]) {
    return (error, StackTrace.fromString(stack));
  }
  return (
    StateError('Malformed native isolate error notification: $message'),
    StackTrace.current,
  );
}
