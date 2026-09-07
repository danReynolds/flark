import 'dart:async';
import 'dart:convert';
import 'dart:isolate';
import 'dart:typed_data';

import 'backend.dart';
import 'backend_ffi.dart';
import 'highlight_transport.dart';

Future<HighlightTransport> startHighlightTransport({
  Uri? workerUri,
  Uri? wasmUri,
}) async {
  if (workerUri != null || wasmUri != null) {
    throw ArgumentError('Worker asset URLs are web-only.');
  }
  final transport = _NativeTransport();
  try {
    transport.isolate = await Isolate.spawn(
      _entry,
      transport.events.sendPort,
      onError: transport.events.sendPort,
      onExit: transport.events.sendPort,
    );
    await transport.ready.future.timeout(const Duration(seconds: 15));
    return transport;
  } catch (_) {
    transport.dispose();
    rethrow;
  }
}

final class _NativeTransport implements HighlightTransport {
  _NativeTransport() {
    events.listen((event) {
      if (event is SendPort) {
        sender = event;
        ready.complete();
      } else if (event is List &&
          event.first == true &&
          event[1] is Uint8List) {
        pending?.complete(event[1] as Uint8List);
        pending = null;
      } else {
        final error = CodeException('Highlight isolate failed: $event');
        if (!ready.isCompleted) ready.completeError(error);
        pending?.completeError(error);
        pending = null;
        dispose();
      }
    });
  }
  final events = ReceivePort();
  final ready = Completer<void>();
  Isolate? isolate;
  SendPort? sender;
  Completer<Uint8List>? pending;
  bool closed = false;

  @override
  Future<Uint8List> analyze(String source, int language) {
    if (closed) throw StateError('Highlight isolate is closed.');
    if (pending != null) throw StateError('Concurrent worker request.');
    final result = pending = Completer<Uint8List>();
    sender!.send([source, language]);
    return result.future;
  }

  @override
  void dispose() {
    if (closed) return;
    closed = true;
    pending?.completeError(StateError('Highlight isolate disposed.'));
    pending = null;
    isolate?.kill(priority: Isolate.immediate);
    events.close();
  }
}

void _entry(SendPort parent) {
  final backend = createCodeBackend();
  final messages = ReceivePort();
  try {
    if (backend.version != 4) {
      throw const CodeException('Expected worker ABI 4.');
    }
    parent.send(messages.sendPort);
    messages.listen((dynamic request) {
      try {
        parent.send([
          true,
          backend.analyze(utf8.encode(request[0] as String), request[1] as int),
        ]);
      } catch (error) {
        parent.send([false, error.toString()]);
        backend.dispose();
        messages.close();
      }
    });
  } catch (error) {
    parent.send([false, error.toString()]);
    backend.dispose();
    messages.close();
  }
}
