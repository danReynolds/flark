@JS()
library;

import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';

import '../flark_v3_byte_endpoint.dart';

const int flarkV3WebMaximumFrameBytes = 262168;

const int _commandInitialize = 0;
const int _commandDispatch = 1;
const int _commandStrictClose = 2;
const int _commandRecover = 3;
const int _commandDispose = 4;
const int _commandDispatchHostPoll = 5;
const int _commandCheckpointBProbe = 6;
const int _commandDispatchInlineSidecarHostPoll = 7;
const int _commandDispatchViewportPresentationHostPoll = 8;

const int _eventReady = 0;
const int _eventFrame = 1;
const int _eventFailure = 2;
const int _eventDisposed = 3;
const int _eventReclamationUnproven = 4;
const int _eventCheckpointBProbe = 5;

const Duration _startupReclamationTimeout = Duration(seconds: 5);
const int _checkpointBMaximumJsonBytes = 64 * 1024;

final Finalizer<_WebEndpointCleanup> _webEndpointFinalizer =
    Finalizer<_WebEndpointCleanup>((cleanup) => cleanup.abandon());

@JS('Worker')
extension type _BrowserWorker._(JSObject _) implements JSObject {
  external factory _BrowserWorker(JSAny scriptUrl);

  external void postMessage(JSAny? message, [JSObject optionsOrTransfer]);
  external void terminate();

  external JSFunction? get onmessage;
  external set onmessage(JSFunction? value);
  external JSFunction? get onmessageerror;
  external set onmessageerror(JSFunction? value);
  external JSFunction? get onerror;
  external set onerror(JSFunction? value);
}

extension type _MessageEvent._(JSObject _) implements JSObject {
  external JSAny? get data;
}

extension type _ErrorEvent._(JSObject _) implements JSObject {
  external String get message;
  external String get filename;
  external int get lineno;
  external int get colno;
  external void preventDefault();
}

/// Finalizer token that never retains its Dart endpoint owner.
///
/// Abandonment cannot await the removal receipt, but it still orders the
/// Worker's proof-based dispose command instead of immediately terminating a
/// Worker that may own a live Wasm registry slot. The Worker closes itself only
/// after removal or an explicit unproven-reclamation terminal event.
final class _WebEndpointCleanup {
  _WebEndpointCleanup(this.worker);

  final _BrowserWorker worker;
  bool _finished = false;

  void abandon() {
    if (_finished) return;
    _finished = true;
    try {
      worker.postMessage(<JSAny?>[_commandDispose.toJS].toJS, <JSAny?>[].toJS);
    } catch (_) {
      // The proof request was attempted. With no reachable owner left to
      // observe an unproven receipt, termination is the only bounded fallback.
      worker.terminate();
      return;
    }
    // Finalizers cannot await. Give the serialized Worker protocol the same
    // bounded teardown window as startup, then contain a wedged Worker. A live
    // owner never takes this path: its [done] future still requires the actual
    // removal receipt.
    Timer(_startupReclamationTimeout, () => worker.terminate());
  }

  void finish() {
    if (_finished) return;
    _finished = true;
    worker
      ..onmessage = null
      ..onmessageerror = null
      ..onerror = null
      ..terminate();
  }
}

/// Failure reported by the external parser Worker or its Wasm endpoint.
final class FlarkV3WebEndpointException implements Exception {
  const FlarkV3WebEndpointException({
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
    return 'FlarkV3WebEndpointException($operation, '
        'status=0x${status.toRadixString(16).padLeft(4, '0')}$suffix)';
  }
}

/// Web implementation of Flark's bounded platform byte seam.
///
/// One external classic Worker owns one Wasm module instance and one endpoint
/// registry slot for this object's complete lifetime. Only fixed-width
/// recovery identity and transferred FLK3 `ArrayBuffer`s cross the Worker
/// boundary; Markdown and parser-owned pointers never enter the main context.
final class FlarkV3WebWorkerByteEndpoint implements FlarkV3ByteEndpoint {
  FlarkV3WebWorkerByteEndpoint._(this._worker) {
    // Protocol futures can fail before the startup coroutine or public owner
    // reaches its await. Attach handlers immediately without changing what a
    // later await of the original future observes.
    _ready.future.ignore();
    _removed.future.ignore();
    _done.future.ignore();
    _cleanup = _WebEndpointCleanup(_worker);
    final owner = WeakReference<FlarkV3WebWorkerByteEndpoint>(this);
    _messageHandler = ((JSAny? value) {
      final target = owner.target;
      if (target == null) return;
      final event = _messageEvent(value);
      target._receive(event.data);
    }).toJS;
    _messageErrorHandler = ((JSAny? _) {
      owner.target?._workerCrashed(
        const FormatException(
          'The Flark parser Worker could not deserialize a message.',
        ),
        StackTrace.current,
      );
    }).toJS;
    _errorHandler = ((JSAny? value) {
      final target = owner.target;
      if (target == null) return;
      final event = _errorEvent(value);
      event.preventDefault();
      target._workerCrashed(
        StateError(
          'Flark parser Worker crashed: ${event.message} '
          '(${event.filename}:${event.lineno}:${event.colno}).',
        ),
        StackTrace.current,
      );
    }).toJS;
    _worker
      ..onmessage = _messageHandler
      ..onmessageerror = _messageErrorHandler
      ..onerror = _errorHandler;
    _webEndpointFinalizer.attach(this, _cleanup, detach: this);
  }

  /// Starts one external Worker and waits until its Wasm endpoint exists.
  static Future<FlarkV3WebWorkerByteEndpoint> start({
    required Uri workerUri,
    required Uri wasmUri,
    Duration startupTimeout = const Duration(seconds: 15),
  }) async {
    if (workerUri.toString().isEmpty) {
      throw ArgumentError.value(workerUri, 'workerUri', 'must not be empty');
    }
    if (const <String>{
      'blob',
      'data',
      'javascript',
    }.contains(workerUri.scheme.toLowerCase())) {
      throw ArgumentError.value(
        workerUri,
        'workerUri',
        'must identify an external classic Worker script',
      );
    }
    if (wasmUri.toString().isEmpty) {
      throw ArgumentError.value(wasmUri, 'wasmUri', 'must not be empty');
    }
    if (startupTimeout <= Duration.zero) {
      throw ArgumentError.value(
        startupTimeout,
        'startupTimeout',
        'must be positive',
      );
    }

    late final _BrowserWorker worker;
    try {
      worker = _BrowserWorker(workerUri.toString().toJS);
    } catch (error, stackTrace) {
      Error.throwWithStackTrace(
        FlarkV3WebEndpointException(
          operation: 'workerCreate',
          status: 0x0111,
          detail: '$error',
        ),
        stackTrace,
      );
    }
    final endpoint = FlarkV3WebWorkerByteEndpoint._(worker);
    endpoint._post(<JSAny?>[_commandInitialize.toJS, wasmUri.toString().toJS]);
    try {
      await endpoint._ready.future.timeout(
        startupTimeout,
        onTimeout: () => throw TimeoutException(
          'Flark parser Worker startup exceeded $startupTimeout.',
          startupTimeout,
        ),
      );
      return endpoint;
    } catch (error, stackTrace) {
      endpoint._requestDispose();
      Object? reclamationFailure;
      StackTrace? reclamationStackTrace;
      try {
        await endpoint._removed.future.timeout(
          _startupReclamationTimeout,
          onTimeout: () => throw TimeoutException(
            'Flark parser Worker startup reclamation did not produce a '
            'truthful removal receipt.',
            _startupReclamationTimeout,
          ),
        );
      } catch (cleanupError, cleanupStackTrace) {
        reclamationFailure = cleanupError;
        reclamationStackTrace = cleanupStackTrace;
      }
      endpoint._finishWorker();
      if (reclamationFailure != null) {
        Error.throwWithStackTrace(
          FlarkV3WebEndpointException(
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

  final _BrowserWorker _worker;
  late final _WebEndpointCleanup _cleanup;
  final Completer<void> _ready = Completer<void>();
  final Completer<void> _removed = Completer<void>();
  final Completer<void> _done = Completer<void>();
  Completer<String>? _checkpointBProbe;

  late final JSFunction _messageHandler;
  late final JSFunction _messageErrorHandler;
  late final JSFunction _errorHandler;

  FlarkV3ByteFrameCallback? _onFrame;
  FlarkV3ByteEndpointFailureCallback? _onFailure;
  (Object, StackTrace)? _pendingFailure;
  (Object, StackTrace)? _terminalFailure;
  bool _bound = false;
  bool _readyReceived = false;
  bool _disposeRequested = false;
  bool _workerFinished = false;

  /// Completes successfully only after the Worker proves endpoint removal.
  ///
  /// A Worker crash or an explicitly unproven reclamation completes this
  /// future with an error instead of pretending that the Wasm slot was freed.
  Future<void> get done => _done.future;

  /// Runs the private Checkpoint B identity-reuse probe once in this Worker.
  ///
  /// This is an implementation checkpoint seam, not part of the product
  /// document protocol. The returned JSON is produced entirely off the main
  /// context and copied back as one bounded transferred buffer.
  Future<String> runCheckpointBProbeJson() {
    _requireLive();
    if (_checkpointBProbe != null) {
      throw StateError('Checkpoint B probe has already been requested.');
    }
    final completer = Completer<String>();
    completer.future.ignore();
    _checkpointBProbe = completer;
    _post(<JSAny?>[_commandCheckpointBProbe.toJS]);
    return completer.future;
  }

  @override
  void bind({
    required FlarkV3ByteFrameCallback onFrame,
    required FlarkV3ByteEndpointFailureCallback onFailure,
  }) {
    if (_workerFinished || _disposeRequested) {
      throw StateError('Web parser endpoint is closed.');
    }
    if (_bound) throw StateError('Web parser endpoint is already bound.');
    _bound = true;
    _onFrame = onFrame;
    _onFailure = onFailure;
    final pending = _pendingFailure;
    _pendingFailure = null;
    if (pending != null) {
      onFailure(pending.$1, pending.$2);
    }
  }

  @override
  void recover(FlarkV3ByteEndpointBinding previousBinding) {
    _requireLive();
    _post(<JSAny?>[
      _commandRecover.toJS,
      for (final word in previousBinding.documentSessionWords) word.toJS,
      previousBinding.sourceSessionIdentity.toJS,
      previousBinding.workerGeneration.toJS,
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
    if (frame.isEmpty || frame.length > flarkV3WebMaximumFrameBytes) {
      throw RangeError.range(
        frame.length,
        1,
        flarkV3WebMaximumFrameBytes,
        'frame.length',
      );
    }
    // Own an exact-size backing buffer before detaching it. This avoids
    // transferring unrelated bytes when a caller passes a typed-data view.
    final owned = Uint8List.fromList(frame);
    final buffer = owned.buffer.toJS;
    _post(<JSAny?>[command.toJS, buffer], transfer: <JSAny?>[buffer]);
  }

  @override
  void close() => _requestDispose();

  void _requestDispose() {
    if (_disposeRequested || _workerFinished) return;
    _disposeRequested = true;
    _post(<JSAny?>[_commandDispose.toJS]);
  }

  void _receive(JSAny? data) {
    if (_workerFinished) return;
    try {
      final message = _jsArray(data, 'Worker event');
      if (message.isEmpty) {
        throw const FormatException('Empty Flark parser Worker event.');
      }
      final kind = _jsInt(message[0], 'Worker event kind');
      switch (kind) {
        case _eventReady:
          if (message.length != 1 || _readyReceived) {
            throw const FormatException(
              'Malformed or duplicate Flark parser ready event.',
            );
          }
          _readyReceived = true;
          _ready.complete();
        case _eventFrame:
          if (message.length != 2 || !_readyReceived || _disposeRequested) {
            throw const FormatException('Malformed Flark parser frame event.');
          }
          final buffer = _jsArrayBuffer(message[1], 'Worker frame buffer');
          final callback = _onFrame;
          if (callback == null) {
            throw StateError(
              'Web parser emitted before its callback was bound.',
            );
          }
          callback(buffer.toDart.asUint8List());
        case _eventFailure:
          if (message.length != 4) {
            throw const FormatException(
              'Malformed Flark parser failure event.',
            );
          }
          final failure = FlarkV3WebEndpointException(
            operation: _jsString(message[1], 'failure operation'),
            status: _jsInt(message[2], 'failure status'),
            detail: _jsNullableString(message[3], 'failure detail'),
          );
          if (!_ready.isCompleted) {
            _ready.completeError(failure, StackTrace.current);
          }
          _reportFailure(failure, StackTrace.current);
        case _eventDisposed:
          if (message.length != 1) {
            throw const FormatException('Malformed Worker disposal receipt.');
          }
          if (!_ready.isCompleted) {
            _ready.completeError(
              StateError('Web parser endpoint was disposed before readiness.'),
              StackTrace.current,
            );
          }
          _finishFromRemovalReceipt();
        case _eventReclamationUnproven:
          if (message.length != 4) {
            throw const FormatException(
              'Malformed unproven-reclamation event.',
            );
          }
          final failure = FlarkV3WebEndpointException(
            operation: _jsString(message[1], 'reclamation operation'),
            status: _jsInt(message[2], 'reclamation status'),
            detail:
                _jsNullableString(message[3], 'reclamation detail') ??
                'Wasm endpoint removal was not proven',
          );
          if (!_ready.isCompleted) {
            _ready.completeError(failure, StackTrace.current);
          }
          _reportFailure(failure, StackTrace.current);
          _finishUnproven(failure, StackTrace.current);
        case _eventCheckpointBProbe:
          final probe = _checkpointBProbe;
          if (message.length != 2 ||
              !_readyReceived ||
              probe == null ||
              probe.isCompleted) {
            throw const FormatException(
              'Malformed or unsolicited Checkpoint B probe event.',
            );
          }
          final buffer = _jsArrayBuffer(
            message[1],
            'Checkpoint B probe buffer',
          );
          final bytes = buffer.toDart.asUint8List();
          if (bytes.isEmpty || bytes.length > _checkpointBMaximumJsonBytes) {
            throw const FormatException(
              'Checkpoint B probe JSON exceeded its byte contract.',
            );
          }
          probe.complete(utf8.decode(bytes, allowMalformed: false));
        default:
          throw FormatException('Unknown Flark parser Worker event $kind.');
      }
    } catch (error, stackTrace) {
      _reportFailure(error, stackTrace);
      _requestDispose();
    }
  }

  void _workerCrashed(Object error, StackTrace stackTrace) {
    if (_workerFinished) return;
    if (!_ready.isCompleted) _ready.completeError(error, stackTrace);
    _reportFailure(error, stackTrace);
    _finishUnproven(error, stackTrace);
  }

  void _reportFailure(Object error, StackTrace stackTrace) {
    final probe = _checkpointBProbe;
    if (probe != null && !probe.isCompleted) {
      probe.completeError(error, stackTrace);
    }
    if (_terminalFailure != null || _workerFinished) return;
    _terminalFailure = (error, stackTrace);
    final callback = _onFailure;
    if (callback == null) {
      _pendingFailure = (error, stackTrace);
      return;
    }
    callback(error, stackTrace);
  }

  void _finishFromRemovalReceipt() {
    if (_workerFinished) return;
    final probe = _checkpointBProbe;
    if (probe != null && !probe.isCompleted) {
      _reportFailure(
        StateError(
          'Web parser endpoint was removed before the Checkpoint B probe '
          'produced its receipt.',
        ),
        StackTrace.current,
      );
    }
    if (!_removed.isCompleted) _removed.complete();
    _finishWorker();
    final failure = _terminalFailure;
    if (_done.isCompleted) return;
    if (failure == null) {
      _done.complete();
    } else {
      _done.completeError(failure.$1, failure.$2);
    }
  }

  void _finishUnproven(Object error, StackTrace stackTrace) {
    if (_workerFinished) return;
    if (!_removed.isCompleted) _removed.completeError(error, stackTrace);
    _finishWorker();
    if (!_done.isCompleted) _done.completeError(error, stackTrace);
  }

  void _finishWorker() {
    if (_workerFinished) return;
    _workerFinished = true;
    _webEndpointFinalizer.detach(this);
    _cleanup.finish();
  }

  void _requireLive() {
    if (!_readyReceived ||
        _disposeRequested ||
        _workerFinished ||
        _terminalFailure != null) {
      throw StateError('Web parser endpoint is unavailable.');
    }
  }

  void _post(List<JSAny?> message, {List<JSAny?>? transfer}) {
    if (_workerFinished) return;
    final transferArray = transfer?.toJS ?? <JSAny?>[].toJS;
    _worker.postMessage(message.toJS, transferArray);
  }
}

_MessageEvent _messageEvent(JSAny? value) {
  if (value == null || !value.isA<JSObject>()) {
    throw const FormatException(
      'Worker message event is not a JavaScript object.',
    );
  }
  return _MessageEvent._(value as JSObject);
}

_ErrorEvent _errorEvent(JSAny? value) {
  if (value == null || !value.isA<JSObject>()) {
    throw const FormatException(
      'Worker error event is not a JavaScript object.',
    );
  }
  return _ErrorEvent._(value as JSObject);
}

List<JSAny?> _jsArray(JSAny? value, String name) {
  if (value == null || !value.isA<JSArray<JSAny?>>()) {
    throw FormatException('$name is not an Array.');
  }
  return (value as JSArray<JSAny?>).toDart;
}

int _jsInt(JSAny? value, String name) {
  if (value == null || !value.isA<JSNumber>()) {
    throw FormatException('$name is not a number.');
  }
  final number = (value as JSNumber).toDartDouble;
  if (!number.isFinite ||
      number != number.truncateToDouble() ||
      number < 0 ||
      number > 0xffffffff) {
    throw FormatException('$name is not a u32.');
  }
  return number.toInt();
}

String _jsString(JSAny? value, String name) {
  if (value == null || !value.isA<JSString>()) {
    throw FormatException('$name is not a string.');
  }
  return (value as JSString).toDart;
}

String? _jsNullableString(JSAny? value, String name) =>
    value == null ? null : _jsString(value, name);

JSArrayBuffer _jsArrayBuffer(JSAny? value, String name) {
  if (value == null || !value.isA<JSArrayBuffer>()) {
    throw FormatException('$name is not an ArrayBuffer.');
  }
  return value as JSArrayBuffer;
}
