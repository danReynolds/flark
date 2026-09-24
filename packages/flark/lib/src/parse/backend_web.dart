import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'backend.dart';
import 'bundled_wasm.g.dart';
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

@JS('TextEncoder')
extension type _TextEncoder._(JSObject _) implements JSObject {
  external _TextEncoder();
  external _EncodeResult encodeInto(JSString source, JSUint8Array target);
}

extension type _EncodeResult._(JSObject _) implements JSObject {
  external JSNumber get read;
  external JSNumber get written;
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
  WasmParseBackend._(this._module) {
    _instantiate();
  }

  static Future<JSObject>? _bundledModule;

  /// The package carries its own parser, including plain dart2js hosts and
  /// non-root deployments. Compile once, allocate a separate instance per owner.
  static Future<WasmParseBackend> bundled() async {
    final pending = _bundledModule ??= _compile(
      base64Decode(bundledParserBase64).toJS,
    ).toDart;
    try {
      return WasmParseBackend._(await pending);
    } catch (_) {
      if (identical(_bundledModule, pending)) _bundledModule = null;
      rethrow;
    }
  }

  bool _disposed = false;

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _free(_input, _inputCapacity);
    _free(_outCell, 16);
    // Drop the WebAssembly memory/export references as well as its buffers.
    _exports = JSObject();
    _memory = JSObject();
    _parseFn = _allocFn = _freeFn = _versionFn = null;
  }

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
        if (!response.ok.toDart) {
          lastError = 'HTTP ${response.status.toDartInt} for $uri';
          continue;
        }
        final buffer = await response.arrayBuffer().toDart;
        return await fromBytes(buffer.toDart.asUint8List());
      } catch (e) {
        lastError = e;
      }
    }
    throw FlarkParseException(
      FlarkParseException.loadFailedCode,
      'could not load flark_parse.wasm from ${uris.join(', ')}: $lastError',
    );
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
  final _encoder = _TextEncoder();
  late JSObject _exports;
  late JSObject _memory;
  JSFunction? _parseFn, _allocFn, _freeFn, _versionFn;
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
    _versionFn = _exports.getProperty<JSFunction>(
      'flark_parse_schema_version'.toJS,
    );
    final version = schemaVersion;
    if (version != RenderModelSchema.version) {
      throw FlarkParseException(
        FlarkParseException.schemaMismatchCode,
        'flark_parse.wasm writes schema $version, this package reads ${RenderModelSchema.version}',
      );
    }
    _outCell = _alloc(16);
    _inputCapacity = _initialInputCapacity;
    _input = _alloc(_inputCapacity);
  }

  int _alloc(int len) =>
      (_allocFn!.callAsFunction(null, len.toJS) as JSNumber).toDartInt;
  void _free(int ptr, int len) {
    _freeFn!.callAsFunction(null, ptr.toJS, len.toJS);
  }

  @override
  int get schemaVersion =>
      (_versionFn!.callAsFunction(null) as JSNumber).toDartInt;

  @override
  RenderModel parse(String source) {
    if (_disposed) throw StateError('WasmParseBackend used after dispose');
    validateFlarkSourceText(source);
    // One UTF-16 code unit never needs more than three UTF-8 bytes. Keep
    // headroom so typing does not reallocate on every keystroke.
    if (source.length * 3 > _inputCapacity) {
      _free(_input, _inputCapacity);
      _inputCapacity = source.length * 6;
      _input = _alloc(_inputCapacity);
    }
    // Encode directly into Wasm memory, read only after any allocation above
    // grew it. utf8.encode plus a copy into the JS-backed heap took
    // milliseconds per keystroke under dart2wasm. Validation above means the
    // encoder never substitutes U+FFFD for a lone surrogate.
    final encoded = _encoder.encodeInto(
      source.toJS,
      JSUint8Array(
        _memory.getProperty<JSArrayBuffer>('buffer'.toJS),
        _input,
        _inputCapacity,
      ),
    );
    if (encoded.read.toDartInt != source.length) {
      throw StateError('flark_parse input buffer too small');
    }
    final length = encoded.written.toDartInt;
    final int rc;
    try {
      rc =
          (_parseFn!.callAsFunction(
                    null,
                    _input.toJS,
                    length.toJS,
                    _outCell.toJS,
                    (_outCell + 8).toJS,
                  )
                  as JSNumber)
              .toDartInt;
    } catch (e) {
      // A trap: the instance is no longer trustworthy. Rebuild it from the
      // compiled module so the next parse starts clean.
      _instantiate();
      throw FlarkParseException(
        FlarkParseException.faultCode,
        'wasm trap during parse: $e',
      );
    }
    if (rc != 0) throw FlarkParseException.fromCode(rc);
    // Re-read the buffer: memory may have grown. The out cell and the model
    // are whole-word allocations, so both can be read as 32-bit words.
    final heap = _memory.getProperty<JSArrayBuffer>('buffer'.toJS);
    final cell = JSUint32Array(heap, _outCell, 3).toDart;
    final outPtr = cell[0], outLen = cell[2];
    // Copy words, not bytes. dart2wasm copies a JS array with one call per
    // element, so this makes a quarter of the calls, and the copy is backed
    // by a 32-bit array that RenderModel reads without assembling bytes.
    // Together they made a 32 KiB keystroke 23-30% faster under dart2wasm.
    final words = Uint32List.fromList(
      JSUint32Array(heap, outPtr, outLen ~/ 4).toDart,
    );
    _free(outPtr, outLen);
    return RenderModel(words.buffer.asUint8List());
  }
}

/// On the web the backend is created with [WasmParseBackend.load] or
/// [WasmParseBackend.fromBytes]; a synchronous factory cannot fetch.
FlarkParseBackend createParseBackend() => throw UnsupportedError(
  'flark: on the web, await WasmParseBackend.load() or WasmParseBackend.fromBytes()',
);
