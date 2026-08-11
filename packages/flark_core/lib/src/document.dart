import 'dart:async';
import 'dart:isolate';

import 'models.dart';
import 'native/native_document.dart';

enum FlarkCoreHistoryDisposition { retained, disabled, overBudget }

/// An opaque, one-shot handle to inverse source retained by the native core.
final class FlarkCoreHistoryToken {
  FlarkCoreHistoryToken._(this._value, this._owner);

  final int _value;
  final Object _owner;
  bool _consumed = false;
}

/// An opaque handle to a native source-stable anchor.
///
/// The runtime keeps every anchor at the current revision, so a resolved
/// offset is always valid for the document's current source.
final class FlarkCoreAnchor {
  FlarkCoreAnchor._(this._value, this._owner);

  final int _value;
  final Object _owner;
  bool _released = false;
}

final class FlarkCoreSessionInspection {
  const FlarkCoreSessionInspection({
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

final class FlarkCoreNativeException implements Exception {
  const FlarkCoreNativeException(
    this.operation,
    this.status, [
    this.detail = 0,
  ]);

  final String operation;
  final int status;
  final int detail;

  @override
  String toString() =>
      'FlarkCoreNativeException($operation, status: $status, detail: $detail)';
}

final class FlarkCoreEditReceipt {
  const FlarkCoreEditReceipt({
    required this.revision,
    required this.sourceByteLength,
    required this.sourceUtf16Length,
    required this.historyToken,
    required this.historyDisposition,
  });

  final int revision;
  final int sourceByteLength;
  final int sourceUtf16Length;
  final FlarkCoreHistoryToken? historyToken;
  final FlarkCoreHistoryDisposition historyDisposition;
}

/// Headless Dart document actor backed by the Rust Flark runtime.
///
/// A single persistent isolate owns the native session. Calls are serialized by
/// its mailbox, so parsing and source reads never execute on Flutter's UI
/// isolate and revision order cannot race.
final class FlarkCoreDocument {
  FlarkCoreDocument._(
    this._isolate,
    this._commands, {
    required int revision,
    required int sourceByteLength,
    required int sourceUtf16Length,
    required bool ready,
  }) : _revision = revision,
       _sourceByteLength = sourceByteLength,
       _sourceUtf16Length = sourceUtf16Length,
       _ready = ready;

  final Isolate _isolate;
  final SendPort _commands;
  final Object _historyOwner = Object();

  int _revision;
  int _sourceByteLength;
  int _sourceUtf16Length;
  bool _ready;
  bool _disposed = false;

  int get revision => _revision;
  int get sourceByteLength => _sourceByteLength;
  int get sourceUtf16Length => _sourceUtf16Length;
  bool get isReady => _ready;

  static Future<FlarkCoreDocument> open(
    String source, {
    required String libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
  }) async {
    if (historyBudgetBytes < 0) {
      throw RangeError.value(historyBudgetBytes, 'historyBudgetBytes');
    }
    final startup = ReceivePort();
    final errors = ReceivePort();
    final exits = ReceivePort();
    final isolate = await Isolate.spawn<List<Object?>>(
      _documentWorker,
      [startup.sendPort, source, libraryPath, historyBudgetBytes],
      onError: errors.sendPort,
      onExit: exits.sendPort,
      errorsAreFatal: true,
      debugName: 'flark-core-document',
    );
    try {
      final message = await Future.any<Object?>([
        startup.first,
        errors.first.then((error) => {'error': error.toString()}),
        exits.first.then(
          (_) => {'error': 'Flark worker exited during startup'},
        ),
      ]);
      final envelope = message! as Map<Object?, Object?>;
      if (envelope case {'nativeError': final Map<Object?, Object?> error}) {
        isolate.kill(priority: Isolate.immediate);
        throw _decodeNativeException(error);
      }
      if (envelope case {'error': final Object error}) {
        isolate.kill(priority: Isolate.immediate);
        throw StateError(error.toString());
      }
      return FlarkCoreDocument._(
        isolate,
        envelope['commands']! as SendPort,
        revision: envelope['revision']! as int,
        sourceByteLength: envelope['sourceByteLength']! as int,
        sourceUtf16Length: envelope['sourceUtf16Length']! as int,
        ready: envelope['ready']! as bool,
      );
    } finally {
      startup.close();
      errors.close();
      exits.close();
    }
  }

  Future<FlarkCoreEditReceipt> applyEditUtf16(
    int startUtf16,
    int endUtf16,
    String replacement,
  ) async {
    final result = await _request('edit', {
      'start': startUtf16,
      'end': endUtf16,
      'replacement': replacement,
    });
    _revision = result['revision']! as int;
    _sourceByteLength = result['sourceByteLength']! as int;
    _sourceUtf16Length = result['sourceUtf16Length']! as int;
    _ready = false;
    return _editReceipt(result);
  }

  /// Replays and consumes [token]. The returned receipt contains the inverse
  /// token for the opposite history direction when native retention succeeds.
  Future<FlarkCoreEditReceipt> replayHistory(
    FlarkCoreHistoryToken token,
  ) async {
    _requireOwnedHistoryToken(token);
    final result = await _request('replayHistory', {
      'historyToken': token._value,
    });
    token._consumed = true;
    _revision = result['revision']! as int;
    _sourceByteLength = result['sourceByteLength']! as int;
    _sourceUtf16Length = result['sourceUtf16Length']! as int;
    _ready = false;
    return _editReceipt(result);
  }

  /// Releases [token] without changing source.
  Future<void> releaseHistory(FlarkCoreHistoryToken token) async {
    _requireOwnedHistoryToken(token);
    await _request('releaseHistory', {'historyToken': token._value});
    token._consumed = true;
  }

  Future<bool> pump({int workUnits = 512}) async {
    final result = await _request('pump', {'workUnits': workUnits});
    _ready = result['ready']! as bool;
    return _ready;
  }

  Future<void> pumpUntilReady({int workUnits = 512}) async {
    final result = await _request('pumpUntilReady', {'workUnits': workUnits});
    _ready = result['ready']! as bool;
  }

  Future<FlarkViewport> queryViewport({
    int startByte = 0,
    int? endByte,
    int maxRows = 256,
  }) async {
    final result = await _request('queryViewport', {
      'startByte': startByte,
      'endByte': endByte,
      'maxRows': maxRows,
    });
    return FlarkViewport.fromMessage(
      result['viewport']! as Map<Object?, Object?>,
    );
  }

  Future<FlarkViewport> queryViewportNext(
    FlarkViewport previous, {
    int maxRows = 256,
  }) async {
    final result = await _request('queryViewportNext', {
      'viewport': previous.toMessage(),
      'maxRows': maxRows,
    });
    return FlarkViewport.fromMessage(
      result['viewport']! as Map<Object?, Object?>,
    );
  }

  Future<void> releaseViewportContinuation(FlarkViewport viewport) async {
    if (viewport.continuation == 0) return;
    await _request('releaseViewportContinuation', {
      'viewport': viewport.toMessage(),
    });
  }

  /// Creates a source-stable anchor at a UTF-16 scalar boundary. The native
  /// runtime transforms it through every later edit; [downstream] selects the
  /// splice edge it follows when an edit lands exactly on or across it.
  Future<FlarkCoreAnchor> createAnchorUtf16(
    int utf16Position, {
    required bool downstream,
  }) async {
    final result = await _request('createAnchor', {
      'utf16': utf16Position,
      'downstream': downstream,
    });
    return FlarkCoreAnchor._(result['anchor']! as int, _historyOwner);
  }

  /// Resolves [anchor] to a UTF-16 offset at the current revision.
  Future<int> resolveAnchorUtf16(FlarkCoreAnchor anchor) async {
    _requireOwnedAnchor(anchor);
    final result = await _request('resolveAnchor', {'anchor': anchor._value});
    return result['utf16']! as int;
  }

  Future<void> releaseAnchor(FlarkCoreAnchor anchor) async {
    _requireOwnedAnchor(anchor);
    await _request('releaseAnchor', {'anchor': anchor._value});
    anchor._released = true;
  }

  Future<FlarkCoreSessionInspection> inspectSession() async {
    final result = await _request('inspect', const {});
    return FlarkCoreSessionInspection(
      sessionState: result['sessionState']! as int,
      revision: result['revision']! as int,
      liveTransactions: result['liveTransactions']! as int,
      liveContinuations: result['liveContinuations']! as int,
      liveAnchors: result['liveAnchors']! as int,
      liveHistoryTokens: result['liveHistoryTokens']! as int,
    );
  }

  Future<String> readSource() async {
    final result = await _request('readSource', const {});
    return result['source']! as String;
  }

  Future<String> readSourceRange(int startByte, int endByte) async {
    final result = await _request('readSourceRange', {
      'startByte': startByte,
      'endByte': endByte,
    });
    return result['source']! as String;
  }

  Future<String> readSourceUtf16Range(int startUtf16, int endUtf16) async {
    final result = await _request('readSourceUtf16Range', {
      'startUtf16': startUtf16,
      'endUtf16': endUtf16,
    });
    return result['source']! as String;
  }

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    try {
      await _send('dispose', const {});
    } finally {
      _isolate.kill(priority: Isolate.immediate);
    }
  }

  Future<Map<Object?, Object?>> _request(
    String operation,
    Map<String, Object?> arguments,
  ) {
    if (_disposed) throw StateError('FlarkCoreDocument is disposed');
    return _send(operation, arguments);
  }

  Future<Map<Object?, Object?>> _send(
    String operation,
    Map<String, Object?> arguments,
  ) async {
    final reply = ReceivePort();
    try {
      _commands.send([operation, arguments, reply.sendPort]);
      final envelope = await reply.first as Map<Object?, Object?>;
      if (envelope case {'nativeError': final Map<Object?, Object?> error}) {
        throw _decodeNativeException(error);
      }
      if (envelope case {'error': final Object error}) {
        throw StateError('Flark $operation failed: $error');
      }
      return envelope;
    } finally {
      reply.close();
    }
  }

  FlarkCoreEditReceipt _editReceipt(Map<Object?, Object?> result) {
    final token = result['historyToken'] as int?;
    return FlarkCoreEditReceipt(
      revision: _revision,
      sourceByteLength: _sourceByteLength,
      sourceUtf16Length: _sourceUtf16Length,
      historyToken: token == null
          ? null
          : FlarkCoreHistoryToken._(token, _historyOwner),
      historyDisposition: FlarkCoreHistoryDisposition
          .values[result['historyDisposition']! as int],
    );
  }

  void _requireOwnedHistoryToken(FlarkCoreHistoryToken token) {
    if (!identical(token._owner, _historyOwner)) {
      throw ArgumentError.value(token, 'token', 'belongs to another document');
    }
    if (token._consumed) {
      throw StateError('Flark history token was already consumed');
    }
  }

  void _requireOwnedAnchor(FlarkCoreAnchor anchor) {
    if (!identical(anchor._owner, _historyOwner)) {
      throw ArgumentError.value(
        anchor,
        'anchor',
        'belongs to another document',
      );
    }
    if (anchor._released) {
      throw StateError('Flark anchor was already released');
    }
  }
}

FlarkCoreNativeException _decodeNativeException(Map<Object?, Object?> error) =>
    FlarkCoreNativeException(
      error['operation']! as String,
      error['status']! as int,
      error['detail']! as int,
    );

Future<void> _documentWorker(List<Object?> startup) async {
  final startupPort = startup[0]! as SendPort;
  try {
    final document = FlarkNativeDocument.open(
      startup[1]! as String,
      libraryPath: startup[2]! as String,
      historyBudgetBytes: startup[3]! as int,
    );
    final commands = ReceivePort();
    startupPort.send({
      'commands': commands.sendPort,
      'revision': document.revision,
      'sourceByteLength': document.sourceByteLength,
      'sourceUtf16Length': document.sourceUtf16Length,
      'ready': document.isReady,
    });
    await for (final raw in commands) {
      final message = raw! as List<Object?>;
      final operation = message[0]! as String;
      final arguments = message[1]! as Map<Object?, Object?>;
      final reply = message[2]! as SendPort;
      try {
        switch (operation) {
          case 'edit':
            final receipt = document.applyEditUtf16(
              arguments['start']! as int,
              arguments['end']! as int,
              arguments['replacement']! as String,
            );
            reply.send({
              'revision': receipt.revision,
              'sourceByteLength': receipt.sourceByteLength,
              'sourceUtf16Length': receipt.sourceUtf16Length,
              'historyToken': receipt.historyToken,
              'historyDisposition': receipt.historyDisposition.index,
            });
          case 'replayHistory':
            final receipt = document.replayHistory(
              arguments['historyToken']! as int,
            );
            reply.send({
              'revision': receipt.revision,
              'sourceByteLength': receipt.sourceByteLength,
              'sourceUtf16Length': receipt.sourceUtf16Length,
              'historyToken': receipt.historyToken,
              'historyDisposition': receipt.historyDisposition.index,
            });
          case 'releaseHistory':
            document.releaseHistory(arguments['historyToken']! as int);
            reply.send(const <Object?, Object?>{});
          case 'pump':
            final ready = document.pump(
              workUnits: arguments['workUnits']! as int,
            );
            reply.send({'ready': ready});
          case 'pumpUntilReady':
            document.pumpUntilReady(workUnits: arguments['workUnits']! as int);
            reply.send({'ready': true});
          case 'queryViewport':
            final viewport = document.queryViewport(
              startByte: arguments['startByte']! as int,
              endByte: arguments['endByte'] as int?,
              maxRows: arguments['maxRows']! as int,
            );
            reply.send({'viewport': viewport.toMessage()});
          case 'queryViewportNext':
            final viewport = document.queryViewportNext(
              FlarkViewport.fromMessage(
                arguments['viewport']! as Map<Object?, Object?>,
              ),
              maxRows: arguments['maxRows']! as int,
            );
            reply.send({'viewport': viewport.toMessage()});
          case 'releaseViewportContinuation':
            document.releaseViewportContinuation(
              FlarkViewport.fromMessage(
                arguments['viewport']! as Map<Object?, Object?>,
              ),
            );
            reply.send(const <Object?, Object?>{});
          case 'createAnchor':
            reply.send({
              'anchor': document.createAnchorUtf16(
                arguments['utf16']! as int,
                downstream: arguments['downstream']! as bool,
              ),
            });
          case 'resolveAnchor':
            reply.send({
              'utf16': document.resolveAnchorUtf16(arguments['anchor']! as int),
            });
          case 'releaseAnchor':
            document.releaseAnchor(arguments['anchor']! as int);
            reply.send(const <Object?, Object?>{});
          case 'inspect':
            final inspection = document.inspect();
            reply.send({
              'sessionState': inspection.sessionState,
              'revision': inspection.revision,
              'liveTransactions': inspection.liveTransactions,
              'liveContinuations': inspection.liveContinuations,
              'liveAnchors': inspection.liveAnchors,
              'liveHistoryTokens': inspection.liveHistoryTokens,
            });
          case 'readSource':
            reply.send({'source': document.readSource()});
          case 'readSourceRange':
            reply.send({
              'source': document.readSourceRange(
                arguments['startByte']! as int,
                arguments['endByte']! as int,
              ),
            });
          case 'readSourceUtf16Range':
            reply.send({
              'source': document.readSourceUtf16Range(
                arguments['startUtf16']! as int,
                arguments['endUtf16']! as int,
              ),
            });
          case 'dispose':
            document.close();
            reply.send(const <Object?, Object?>{});
            commands.close();
          default:
            throw UnsupportedError('Unknown operation: $operation');
        }
      } on FlarkNativeException catch (error) {
        reply.send({
          'nativeError': {
            'operation': error.operation,
            'status': error.status,
            'detail': error.detail,
          },
        });
      } catch (error, stackTrace) {
        reply.send({'error': '$error\n$stackTrace'});
      }
      if (operation == 'dispose') break;
    }
  } on FlarkNativeException catch (error) {
    startupPort.send({
      'nativeError': {
        'operation': error.operation,
        'status': error.status,
        'detail': error.detail,
      },
    });
  } catch (error, stackTrace) {
    startupPort.send({'error': '$error\n$stackTrace'});
  }
}
