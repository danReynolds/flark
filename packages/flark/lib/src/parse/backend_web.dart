import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'backend.dart';
import 'render_model.dart';

@JS('WebAssembly.instantiate')
external JSPromise<JSObject> _instantiate(JSAny bytesOrModule, JSObject imports);

@JS('fetch')
external JSPromise<JSObject> _fetch(JSString url);

extension type _Response(JSObject _) implements JSObject {
  external JSBoolean get ok;
  external JSNumber get status;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

/// The web transport: comrak compiled to wasm32-unknown-unknown, loaded
/// through `dart:js_interop` alone, so it works under dart2js (Fleury) and
/// dart2wasm (Flutter web). Creation is asynchronous; parsing is synchronous.
final class WasmParseBackend implements FlarkParseBackend {
  WasmParseBackend._(this._exports, this._memory) {
    _outCell = _call1('flark_parse_alloc', 16);
  }

  /// Instantiate from raw module bytes.
  static Future<WasmParseBackend> fromBytes(Uint8List bytes) async {
    final result = await _instantiate(bytes.toJS, JSObject()).toDart;
    final instance = result.getProperty<JSObject>('instance'.toJS);
    final exports = instance.getProperty<JSObject>('exports'.toJS);
    final memory = exports.getProperty<JSObject>('memory'.toJS);
    return WasmParseBackend._(exports, memory);
  }

  /// Fetch and instantiate. With no [candidates], tries the package asset
  /// locations Flutter web and a plain dart2js page serve.
  static Future<WasmParseBackend> load({List<Uri>? candidates}) async {
    final uris = candidates ?? defaultAssetCandidates();
    Object? lastError;
    for (final uri in uris) {
      try {
        final response = _Response(await _fetch(uri.toString().toJS).toDart);
        if (!response.ok.toDart) { lastError = 'HTTP ${response.status.toDartInt} for $uri'; continue; }
        final buffer = await response.arrayBuffer().toDart;
        return fromBytes(buffer.toDart.asUint8List());
      } catch (e) { lastError = e; }
    }
    throw FlarkParseException(-1, 'could not load flark_parse.wasm from ${uris.join(', ')}: $lastError');
  }

  static List<Uri> defaultAssetCandidates() => [
        Uri.base.resolve('assets/packages/flark/lib/assets/wasm/flark_parse.wasm'),
        Uri.base.resolve('packages/flark/lib/assets/wasm/flark_parse.wasm'),
        Uri.base.resolve('flark_parse.wasm'),
      ];

  final JSObject _exports;
  final JSObject _memory;
  late final int _outCell;
  int _input = 0;
  int _inputCapacity = 0;

  Uint8List _heap() => _memory.getProperty<JSArrayBuffer>('buffer'.toJS).toDart.asUint8List();
  int _call1(String name, int a) => (_exports.getProperty<JSFunction>(name.toJS).callAsFunction(null, a.toJS) as JSNumber).toDartInt;
  int _call4(String name, int a, int b, int c, int d) => (_exports.getProperty<JSFunction>(name.toJS).callAsFunction(null, a.toJS, b.toJS, c.toJS, d.toJS) as JSNumber).toDartInt;
  void _callVoid2(String name, int a, int b) { _exports.getProperty<JSFunction>(name.toJS).callAsFunction(null, a.toJS, b.toJS); }

  @override
  int get schemaVersion => (_exports.getProperty<JSFunction>('flark_parse_schema_version'.toJS).callAsFunction(null) as JSNumber).toDartInt;

  @override
  RenderModel parse(String source) {
    final bytes = utf8.encode(source);
    if (bytes.length > _inputCapacity) {
      if (_input != 0) _callVoid2('flark_parse_free', _input, _inputCapacity);
      _inputCapacity = bytes.length * 2 + 1024;
      _input = _call1('flark_parse_alloc', _inputCapacity);
    }
    _heap().setRange(_input, _input + bytes.length, bytes);
    final rc = _call4('flark_parse', _input, bytes.length, _outCell, _outCell + 8);
    if (rc != 0) {
      throw FlarkParseException(rc, switch (rc) { 1 => 'null argument', 2 => 'invalid UTF-8', 3 => 'contained fault', _ => 'unknown' });
    }
    final heap = _heap(); // re-read: memory may have grown
    final view = ByteData.sublistView(heap);
    final outPtr = view.getUint32(_outCell, Endian.little);
    final outLen = view.getUint32(_outCell + 8, Endian.little);
    final copy = Uint8List.fromList(Uint8List.sublistView(heap, outPtr, outPtr + outLen));
    _callVoid2('flark_parse_free', outPtr, outLen);
    return RenderModel(copy);
  }
}

/// On the web the backend must be created with [WasmParseBackend.load] or
/// [WasmParseBackend.fromBytes]; the synchronous factory cannot fetch.
FlarkParseBackend createParseBackend() => throw UnsupportedError('flark: on the web, await WasmParseBackend.load() or fromBytes()');
