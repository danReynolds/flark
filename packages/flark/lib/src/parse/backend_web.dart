import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'backend.dart';
import 'render_model.dart';
import 'schema.g.dart';

@JS('WebAssembly.compile')
external JSPromise<JSObject> _compile(JSAny bytes);

@JS('WebAssembly.Instance')
extension type _Instance._(JSObject _) implements JSObject {
  external _Instance(JSObject module, JSObject imports);
  external JSObject get exports;
}

@JS('fetch')
external JSPromise<JSObject> _fetch(JSString url);

extension type _Response(JSObject _) implements JSObject {
  external JSBoolean get ok;
  external JSNumber get status;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

const int _initialInputCapacity = 4096;

/// The web transport: comrak compiled to wasm32-unknown-unknown, loaded
/// through `dart:js_interop` alone, so it works under dart2js (Fleury) and
/// dart2wasm (Flutter web). Creation is asynchronous; parsing is synchronous.
///
/// wasm32 aborts on panic, so a native fault traps out of the call instead of
/// returning a code. The trap is reported as [FlarkParseException] with
/// [FlarkParseException.faultCode], and the instance is discarded and
/// re-created from the compiled module before the next parse.
final class WasmParseBackend implements FlarkParseBackend {
  WasmParseBackend._(this._module) { _instantiate(); }

  /// Compile and instantiate from raw module bytes.
  static Future<WasmParseBackend> fromBytes(Uint8List bytes) async {
    final module = await _compile(bytes.toJS).toDart;
    return WasmParseBackend._(module);
  }

  /// Fetch, compile, and instantiate. With no [candidates], tries the
  /// locations a Flutter web build and a plain dart2js page serve the
  /// package asset from; see [defaultAssetCandidates].
  static Future<WasmParseBackend> load({List<Uri>? candidates}) async {
    final uris = candidates ?? defaultAssetCandidates();
    Object? lastError;
    for (final uri in uris) {
      try {
        final response = _Response(await _fetch(uri.toString().toJS).toDart);
        if (!response.ok.toDart) { lastError = 'HTTP ${response.status.toDartInt} for $uri'; continue; }
        final buffer = await response.arrayBuffer().toDart;
        return await fromBytes(buffer.toDart.asUint8List());
      } catch (e) { lastError = e; }
    }
    throw FlarkParseException(FlarkParseException.loadFailedCode, 'could not load flark_parse.wasm from ${uris.join(', ')}: $lastError');
  }

  /// Flutter web serves the package-declared asset at
  /// `assets/packages/flark/lib/assets/wasm/flark_parse.wasm`; an app that
  /// declares it itself serves `assets/packages/flark/assets/wasm/...`; a
  /// plain page can place the module beside its HTML.
  static List<Uri> defaultAssetCandidates() => [
        Uri.base.resolve('assets/packages/flark/lib/assets/wasm/flark_parse.wasm'),
        Uri.base.resolve('assets/packages/flark/assets/wasm/flark_parse.wasm'),
        Uri.base.resolve('flark_parse.wasm'),
      ];

  final JSObject _module;
  late JSObject _exports;
  late JSObject _memory;
  late JSFunction _parseFn, _allocFn, _freeFn, _versionFn;
  late int _outCell;
  late int _input;
  late int _inputCapacity;

  void _instantiate() {
    final instance = _Instance(_module, JSObject());
    _exports = instance.exports;
    _memory = _exports.getProperty<JSObject>('memory'.toJS);
    _parseFn = _exports.getProperty<JSFunction>('flark_parse'.toJS);
    _allocFn = _exports.getProperty<JSFunction>('flark_parse_alloc'.toJS);
    _freeFn = _exports.getProperty<JSFunction>('flark_parse_free'.toJS);
    _versionFn = _exports.getProperty<JSFunction>('flark_parse_schema_version'.toJS);
    final version = schemaVersion;
    if (version != RenderModelSchema.version) {
      throw FlarkParseException(FlarkParseException.schemaMismatchCode, 'flark_parse.wasm writes schema $version, this package reads ${RenderModelSchema.version}');
    }
    _outCell = _alloc(16);
    _inputCapacity = _initialInputCapacity;
    _input = _alloc(_inputCapacity);
  }

  Uint8List _heap() => _memory.getProperty<JSArrayBuffer>('buffer'.toJS).toDart.asUint8List();
  int _alloc(int len) => (_allocFn.callAsFunction(null, len.toJS) as JSNumber).toDartInt;
  void _free(int ptr, int len) { _freeFn.callAsFunction(null, ptr.toJS, len.toJS); }

  @override
  int get schemaVersion => (_versionFn.callAsFunction(null) as JSNumber).toDartInt;

  @override
  RenderModel parse(String source) {
    final bytes = utf8.encode(source);
    if (bytes.length > _inputCapacity) {
      _free(_input, _inputCapacity);
      _inputCapacity = bytes.length * 2;
      _input = _alloc(_inputCapacity);
    }
    _heap().setRange(_input, _input + bytes.length, bytes);
    final int rc;
    try {
      rc = (_parseFn.callAsFunction(null, _input.toJS, bytes.length.toJS, _outCell.toJS, (_outCell + 8).toJS) as JSNumber).toDartInt;
    } catch (e) {
      // A trap: the instance is no longer trustworthy. Rebuild it from the
      // compiled module so the next parse starts clean.
      _instantiate();
      throw FlarkParseException(FlarkParseException.faultCode, 'wasm trap during parse: $e');
    }
    if (rc != 0) throw FlarkParseException.fromCode(rc);
    final heap = _heap(); // re-read: memory may have grown
    final view = ByteData.sublistView(heap);
    final outPtr = view.getUint32(_outCell, Endian.little);
    final outLen = view.getUint32(_outCell + 8, Endian.little);
    final copy = Uint8List.fromList(Uint8List.sublistView(heap, outPtr, outPtr + outLen));
    _free(outPtr, outLen);
    return RenderModel(copy);
  }
}

/// On the web the backend is created with [WasmParseBackend.load] or
/// [WasmParseBackend.fromBytes]; a synchronous factory cannot fetch.
FlarkParseBackend createParseBackend() => throw UnsupportedError('flark: on the web, await WasmParseBackend.load() or WasmParseBackend.fromBytes()');
