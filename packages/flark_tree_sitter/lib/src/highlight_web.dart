import 'dart:async';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'backend.dart';
import 'highlight_transport.dart';

@JS('Worker')
extension type _Worker._(JSObject _) implements JSObject {
  external _Worker(String uri, JSObject options);
  external set onmessage(JSFunction callback);
  external set onerror(JSFunction callback);
  external void postMessage(JSAny message);
  external void terminate();
}

Future<HighlightTransport> startHighlightTransport({
  Uri? workerUri,
  Uri? wasmUri,
}) async {
  final worker = Uri.base.resolveUri(
    workerUri ??
        Uri.parse(
          'assets/packages/flark_tree_sitter/lib/assets/highlight_worker.mjs',
        ),
  );
  // Resolve custom relative Wasm URLs against the document, before sending
  // them to the worker (whose own base URL points into the package assets).
  final wasm = Uri.base.resolveUri(
    wasmUri ??
        Uri.parse(
          'assets/packages/flark_tree_sitter/lib/assets/wasm/flark_tree_sitter.wasm',
        ),
  );
  final transport = _WebTransport(
    _Worker(worker.toString(), {'type': 'module'}.jsify()! as JSObject),
  );
  try {
    transport.worker.postMessage(
      {'kind': 'init', 'wasm': wasm.toString()}.jsify()!,
    );
    await transport.ready.future.timeout(const Duration(seconds: 15));
    return transport;
  } catch (_) {
    transport.dispose();
    rethrow;
  }
}

final class _WebTransport implements HighlightTransport {
  _WebTransport(this.worker) {
    worker.onmessage = ((JSObject event) {
      final data = event.getProperty<JSObject>('data'.toJS);
      final kind = data.getProperty<JSString>('kind'.toJS).toDart;
      if (kind == 'ready') {
        ready.complete();
      } else if (kind == 'result') {
        pending?.complete(data.getProperty<JSUint8Array>('bytes'.toJS).toDart);
        pending = null;
      } else {
        _fail(data.getProperty<JSString>('error'.toJS).toDart);
      }
    }).toJS;
    worker.onerror = ((JSObject event) {
      _fail(event.getProperty<JSString>('message'.toJS).toDart);
    }).toJS;
  }
  final _Worker worker;
  final ready = Completer<void>();
  Completer<Uint8List>? pending;
  bool closed = false;
  void _fail(String message) {
    final error = CodeException('Highlight worker failed: $message');
    if (!ready.isCompleted) ready.completeError(error);
    pending?.completeError(error);
    pending = null;
    dispose();
  }

  @override
  Future<Uint8List> analyze(String source, int language) {
    if (closed) throw StateError('Highlight worker is closed.');
    if (pending != null) throw StateError('Concurrent worker request.');
    final result = pending = Completer<Uint8List>();
    worker.postMessage(
      {'kind': 'analyze', 'source': source, 'language': language}.jsify()!,
    );
    return result.future;
  }

  @override
  void dispose() {
    if (closed) return;
    closed = true;
    pending?.completeError(StateError('Highlight worker disposed.'));
    pending = null;
    worker.terminate();
  }
}
