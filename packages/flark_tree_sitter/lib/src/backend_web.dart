import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'backend.dart';

@JS('WebAssembly.compile')
external JSPromise<JSObject> _compile(JSAny bytes);

@JS('WebAssembly.instantiate')
external JSPromise<_Instance> _instantiate(JSObject module, JSObject imports);

extension type _Instance(JSObject _) implements JSObject {
  external JSObject get exports;
}

@JS('fetch')
external JSPromise<JSObject> _fetch(JSString url);

extension type _Response(JSObject _) implements JSObject {
  external JSBoolean get ok;
  external JSNumber get status;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

/// A separate Wasm instance per backend. Traps invalidate the backend; callers
/// may load a fresh one. Results are copied before any native buffer is freed.
final class WasmCodeBackend implements CodeBackend {
  WasmCodeBackend._(JSObject exports) : _exports = exports;

  JSObject? _exports;
  JSObject get _live =>
      _exports ?? (throw StateError('Code backend is disposed or trapped.'));
  JSFunction _function(String name) => _live.getProperty<JSFunction>(name.toJS);
  Uint8List _heap() => _live
      .getProperty<JSObject>('memory'.toJS)
      .getProperty<JSArrayBuffer>('buffer'.toJS)
      .toDart
      .asUint8List();

  static Future<WasmCodeBackend> fromBytes(Uint8List bytes) async {
    final module = await _compile(bytes.toJS).toDart;
    // Both stages must be asynchronous: browsers restrict synchronous
    // instantiation of large modules on the main thread (the catalog is >8MB).
    final instance = await _instantiate(module, JSObject()).toDart;
    return WasmCodeBackend._(instance.exports);
  }

  static Future<WasmCodeBackend> load({Uri? uri}) async {
    final resolved =
        uri ??
        Uri.base.resolve(
          'assets/packages/flark_tree_sitter/lib/assets/wasm/flark_tree_sitter.wasm',
        );
    final response = _Response(await _fetch(resolved.toString().toJS).toDart);
    if (!response.ok.toDart) {
      throw CodeException(
        'Wasm load failed: HTTP ${response.status.toDartInt} for $resolved',
      );
    }
    return fromBytes(
      (await response.arrayBuffer().toDart).toDart.asUint8List(),
    );
  }

  @override
  int get version =>
      (_function('flark_tree_sitter_version').callAsFunction(null) as JSNumber)
          .toDartInt;

  int _alloc(int length) =>
      (_function('flark_tree_sitter_alloc').callAsFunction(null, length.toJS)
              as JSNumber)
          .toDartInt;
  void _free(int ptr, int length) => _function(
    'flark_tree_sitter_free',
  ).callAsFunction(null, ptr.toJS, length.toJS);

  @override
  Uint8List analyze(Uint8List source, int language) {
    return _exchange(source, language, 'analyze');
  }

  @override
  Uint8List edit(Uint8List request, int language) =>
      _exchange(request, language, 'edit');

  @override
  Uint8List detect(Uint8List source) => _exchange(source, 0, 'detect');

  Uint8List _exchange(Uint8List source, int language, String operation) {
    final input = _alloc(source.length);
    final cell = _alloc(16);
    var trapped = false;
    try {
      _heap().setRange(input, input + source.length, source);
      final int status;
      try {
        status = _live.callMethodVarArgs<JSNumber>(
          'flark_tree_sitter_$operation'.toJS,
          [
            input.toJS,
            source.length.toJS,
            language.toJS,
            cell.toJS,
            (cell + 8).toJS,
          ],
        ).toDartInt;
      } catch (e) {
        trapped = true;
        dispose();
        throw CodeException('Wasm analysis trapped: $e');
      }
      if (status != 0) throw CodeException('Wasm analysis failed ($status).');
      final heap = _heap(); // Every call may grow memory.
      final data = ByteData.sublistView(heap);
      final output = data.getUint32(cell, Endian.little);
      final length = data.getUint32(cell + 8, Endian.little);
      try {
        return Uint8List.fromList(
          Uint8List.sublistView(heap, output, output + length),
        );
      } finally {
        _free(output, length);
      }
    } finally {
      if (!trapped) {
        _free(input, source.length);
        _free(cell, 16);
      }
    }
  }

  @override
  void dispose() => _exports = null;
}

CodeBackend createCodeBackend() => throw UnsupportedError(
  'On web, await WasmCodeBackend.load() and pass it to CodeAnalyzer.',
);
