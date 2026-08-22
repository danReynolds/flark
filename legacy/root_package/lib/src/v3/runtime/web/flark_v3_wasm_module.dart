import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

extension type _FetchResponse(JSObject _) implements JSObject {
  external JSBoolean get ok;
  external JSNumber get status;
  external JSString get statusText;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

extension type _WasmInstantiateResult(JSObject _) implements JSObject {
  external _WasmInstance get instance;
}

extension type _WasmInstance(JSObject _) implements JSObject {
  external JSObject get exports;
}

extension type _WasmMemory(JSObject _) implements JSObject {
  external JSArrayBuffer get buffer;
}

/// Minimal raw-WebAssembly owner shared by the v3 main-context host adapter.
///
/// The parser Worker has its own module instance and JavaScript bridge. This
/// class intentionally exposes only numeric exports and copied memory views;
/// no Rust pointer or live Wasm buffer escapes into the Dart host protocol.
final class FlarkV3WasmModule {
  FlarkV3WasmModule._(this._exports);

  static Future<FlarkV3WasmModule> load(
    Uri uri, {
    required Iterable<String> requiredExports,
  }) async {
    final fetch = globalContext.getProperty<JSFunction>('fetch'.toJS);
    final response =
        await (fetch.callAsFunction(globalContext, uri.toString().toJS)
                as JSPromise<_FetchResponse>)
            .toDart;
    if (!response.ok.toDart) {
      throw StateError(
        'Failed to load Flark v3 WebAssembly from $uri: '
        '${response.status.toDartInt} ${response.statusText.toDart}',
      );
    }
    final bytes = await response.arrayBuffer().toDart;
    final webAssembly = globalContext.getProperty<JSObject>('WebAssembly'.toJS);
    final instantiate = webAssembly.getProperty<JSFunction>('instantiate'.toJS);
    final result =
        await (instantiate.callAsFunction(webAssembly, bytes, JSObject())
                as JSPromise<_WasmInstantiateResult>)
            .toDart;
    final exports = result.instance.exports;
    for (final name in <String>{
      'memory',
      'flark_v3_wasm_alloc',
      'flark_v3_wasm_free',
      ...requiredExports,
    }) {
      if (exports.getProperty<JSAny?>(name.toJS) == null) {
        throw StateError('Flark v3 WebAssembly is missing export: $name.');
      }
    }
    return FlarkV3WasmModule._(exports);
  }

  final JSObject _exports;

  int callInt(String name, [List<int> arguments = const <int>[]]) {
    final result = _call(name, arguments);
    if (result case final JSNumber number) return number.toDartInt;
    throw StateError('Flark v3 WebAssembly export $name returned no number.');
  }

  void callVoid(String name, [List<int> arguments = const <int>[]]) {
    _call(name, arguments);
  }

  int allocate(int length) {
    if (length <= 0 || length > 0xFFFFFFFF) {
      throw RangeError.range(length, 1, 0xFFFFFFFF, 'length');
    }
    final pointer = callInt('flark_v3_wasm_alloc', <int>[length]);
    if (pointer == 0) {
      throw StateError('Flark v3 WebAssembly scratch allocation failed.');
    }
    return pointer;
  }

  void free(int pointer, int length) {
    if (pointer == 0) return;
    final status = callInt('flark_v3_wasm_free', <int>[pointer, length]);
    if (status != 0) {
      throw StateError(
        'Flark v3 WebAssembly scratch release failed with status '
        '0x${status.toRadixString(16)}.',
      );
    }
  }

  ByteData get memoryData => ByteData.view(_memoryBuffer);

  void writeBytes(int pointer, Uint8List bytes) {
    _checkRange(pointer, bytes.length);
    if (bytes.isEmpty) return;
    Uint8List.view(_memoryBuffer, pointer, bytes.length).setAll(0, bytes);
  }

  Uint8List readBytes(int pointer, int length) {
    _checkRange(pointer, length);
    if (length == 0) return Uint8List(0);
    return Uint8List.fromList(Uint8List.view(_memoryBuffer, pointer, length));
  }

  JSAny? _call(String name, List<int> arguments) {
    final function = _exports.getProperty<JSFunction>(name.toJS);
    final args = arguments
        .map((value) {
          if (value < 0 || value > 0xFFFFFFFF) {
            throw RangeError.range(
              value,
              0,
              0xFFFFFFFF,
              'WebAssembly argument',
            );
          }
          return value.toJS;
        })
        .toList(growable: false);
    if (args.length > 4) {
      final reflect = globalContext.getProperty<JSObject>('Reflect'.toJS);
      final apply = reflect.getProperty<JSFunction>('apply'.toJS);
      return apply.callAsFunction(reflect, function, null, args.toJS);
    }
    return switch (args.length) {
      0 => function.callAsFunction(),
      1 => function.callAsFunction(null, args[0]),
      2 => function.callAsFunction(null, args[0], args[1]),
      3 => function.callAsFunction(null, args[0], args[1], args[2]),
      4 => function.callAsFunction(null, args[0], args[1], args[2], args[3]),
      _ => throw StateError('unreachable WebAssembly argument count'),
    };
  }

  ByteBuffer get _memoryBuffer =>
      _WasmMemory(_exports.getProperty<JSObject>('memory'.toJS)).buffer.toDart;

  void _checkRange(int pointer, int length) {
    if (pointer < 0 || length < 0) {
      throw RangeError('WebAssembly memory range must be non-negative.');
    }
    final end = pointer + length;
    if (end < pointer || end > _memoryBuffer.lengthInBytes) {
      throw RangeError(
        'WebAssembly memory range [$pointer, $end) exceeds '
        '${_memoryBuffer.lengthInBytes} bytes.',
      );
    }
  }
}
