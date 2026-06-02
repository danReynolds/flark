import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';
import 'dart:ui_web' as ui_web;

import 'native_comrak_ffi.dart';

const int _kAbiVersion = 1;
const int _kStatusOk = 0;
const String _kLocalAssetBase = 'lib/assets/wasm/';
const String _kPackageAssetBase = 'packages/sovereign_editor/lib/assets/wasm/';
const String _kPackageFileAssetBase = 'packages/sovereign_editor/assets/wasm/';

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

final class WasmNativeComrakBridge implements NativeComrakBridge {
  WasmNativeComrakBridge._({required List<_WasmAssetKeys> assetKeys})
      : _assetKeys = assetKeys;

  final List<_WasmAssetKeys> _assetKeys;

  Future<_LoadedWasmComrakModule>? _moduleFuture;

  factory WasmNativeComrakBridge.load({String? overrideLibraryPath}) {
    if (overrideLibraryPath != null && overrideLibraryPath.isNotEmpty) {
      throw NativeComrakBridgeLoadException(
        kind: NativeComrakBridgeLoadFailureKind.unsupportedPlatform,
        message:
            'overrideLibraryPath is not supported by the browser WASM bridge.',
        platform: 'web',
        overrideLibraryPath: overrideLibraryPath,
        remediationSteps: const [
          'Bundle the package WASM assets and use the default browser loader.',
        ],
      );
    }

    return WasmNativeComrakBridge._(assetKeys: _defaultAssetKeys());
  }

  @override
  Future<NativeComrakParseResult> parse(NativeComrakParseInput input) {
    return Zone.root.run(() => _parseInRootZone(input));
  }

  Future<NativeComrakParseResult> _parseInRootZone(
    NativeComrakParseInput input,
  ) async {
    final markdown = _decodeMarkdownInput(input);
    if (markdown == null) {
      return NativeComrakParseResult(
        revision: input.revision,
        diagnostics: const [
          NativeComrakDiagnostic(
            range: NativeComrakRange(startByte: 0, endByte: 0),
            message: 'Invalid UTF-8 input.',
            code: 'COMRAK_INVALID_UTF8',
            isError: true,
          ),
        ],
      );
    }

    try {
      final loaded = await _module();
      final exports = loaded.instance.exports;
      final inputPtr = _callExportInt(exports, 'sovereign_comrak_input_alloc', [
        input.utf8Text.length.toJS,
      ]);

      try {
        if (input.utf8Text.isNotEmpty && inputPtr == 0) {
          throw StateError(
            'Comrak WASM bridge failed to allocate input memory.',
          );
        }

        _copyToMemory(exports, inputPtr, input.utf8Text);

        final responsePtr = _callExportInt(exports, 'sovereign_comrak_parse', [
          input.revision.toJS,
          _mapProfile(input.profile).toJS,
          inputPtr.toJS,
          input.utf8Text.length.toJS,
        ]);
        if (responsePtr == 0) {
          throw StateError(
            'Comrak WASM bridge returned a null response pointer.',
          );
        }

        try {
          final response = _readResponse(exports, responsePtr);
          var result = _decodePayload(
            revision: response.revision,
            payload: response.payload,
          );
          if (response.abiVersion != _kAbiVersion) {
            result = _appendDiagnostic(
              result,
              NativeComrakDiagnostic(
                range: const NativeComrakRange(startByte: 0, endByte: 0),
                message:
                    'ABI mismatch: bridge=$_kAbiVersion library=${response.abiVersion}.',
                code: 'COMRAK_ABI_MISMATCH',
                isError: true,
              ),
            );
          }
          if (response.statusCode != _kStatusOk) {
            result = _appendDiagnostic(
              result,
              NativeComrakDiagnostic(
                range: const NativeComrakRange(startByte: 0, endByte: 0),
                message:
                    'WASM parse failed with status ${response.statusCode}.',
                code: 'COMRAK_WASM_STATUS_${response.statusCode}',
                isError: true,
              ),
            );
          }
          return result;
        } finally {
          _callExportVoid(exports, 'sovereign_comrak_response_free', [
            responsePtr.toJS,
          ]);
        }
      } finally {
        _callExportVoid(exports, 'sovereign_comrak_input_free', [
          inputPtr.toJS,
          input.utf8Text.length.toJS,
        ]);
      }
    } catch (error) {
      return NativeComrakParseResult(
        revision: input.revision,
        diagnostics: [
          NativeComrakDiagnostic(
            range: const NativeComrakRange(startByte: 0, endByte: 0),
            message: 'Failed to load or run Comrak WASM bridge: $error',
            code: 'COMRAK_WASM_LOAD_FAILED',
            isError: true,
          ),
        ],
      );
    }
  }

  Future<_LoadedWasmComrakModule> _module() {
    return _moduleFuture ??= _loadModule();
  }

  Future<_LoadedWasmComrakModule> _loadModule() async {
    Object? lastError;
    for (final keys in _assetKeys) {
      try {
        final wasmUrl = _assetUrl(keys);
        final instance = await _instantiateWasmAsset(wasmUrl);
        _validateExports(instance.exports);
        final abiVersion = _callExportInt(
          instance.exports,
          'sovereign_comrak_bridge_version',
        );
        if (abiVersion != _kAbiVersion) {
          throw StateError(
            'Comrak WASM ABI mismatch: bridge=$_kAbiVersion library=$abiVersion.',
          );
        }
        return _LoadedWasmComrakModule(instance: instance);
      } catch (error) {
        lastError = error;
      }
    }

    throw lastError ?? StateError('No Comrak WASM asset URLs were generated.');
  }
}

final class _LoadedWasmComrakModule {
  const _LoadedWasmComrakModule({required this.instance});

  final _WasmInstance instance;
}

final class _WasmAssetKeys {
  const _WasmAssetKeys({
    required this.wasmKey,
    this.resolveWithAssetManager = true,
  });

  final String wasmKey;
  final bool resolveWithAssetManager;
}

NativeComrakBridge createNativeComrakBridge({String? overrideLibraryPath}) {
  return WasmNativeComrakBridge.load(overrideLibraryPath: overrideLibraryPath);
}

NativeComrakBridgePreflightResult preflightNativeComrakBridge({
  String? overrideLibraryPath,
}) {
  if (overrideLibraryPath != null && overrideLibraryPath.isNotEmpty) {
    return NativeComrakBridgePreflightResult.unavailable(
      NativeComrakBridgeLoadException(
        kind: NativeComrakBridgeLoadFailureKind.unsupportedPlatform,
        message:
            'overrideLibraryPath is not supported by the browser WASM bridge.',
        platform: 'web',
        overrideLibraryPath: overrideLibraryPath,
        remediationSteps: const [
          'Bundle the package WASM assets and use the default browser loader.',
        ],
      ),
    );
  }

  return const NativeComrakBridgePreflightResult.available();
}

List<_WasmAssetKeys> _defaultAssetKeys() {
  return const [
    _WasmAssetKeys(
      wasmKey: '${_kPackageAssetBase}sovereign_comrak_bridge.wasm',
    ),
    _WasmAssetKeys(wasmKey: '${_kLocalAssetBase}sovereign_comrak_bridge.wasm'),
    // Flutter's Chrome package-test server serves package files from lib/ at
    // /packages/<package>/<lib-relative-path>; production web builds use the
    // asset manager entries above.
    _WasmAssetKeys(
      wasmKey: '${_kPackageFileAssetBase}sovereign_comrak_bridge.wasm',
      resolveWithAssetManager: false,
    ),
  ];
}

String _assetUrl(_WasmAssetKeys keys) {
  final rawUrl = keys.resolveWithAssetManager
      ? ui_web.assetManager.getAssetUrl(keys.wasmKey)
      : keys.wasmKey;
  final root = Uri.base.replace(path: '/', query: null, fragment: null);
  return root.resolve(rawUrl).toString();
}

Future<_WasmInstance> _instantiateWasmAsset(String wasmUrl) async {
  final fetch = globalContext.getProperty<JSFunction>('fetch'.toJS);
  final response = await (fetch.callAsFunction(globalContext, wasmUrl.toJS)
          as JSPromise<_FetchResponse>)
      .toDart;
  if (!response.ok.toDart) {
    throw StateError(
      'Failed to load Comrak WASM bridge from $wasmUrl: '
      '${response.status.toDartInt} ${response.statusText.toDart}',
    );
  }

  final bytes = await response.arrayBuffer().toDart;
  final webAssembly = globalContext.getProperty<JSObject>('WebAssembly'.toJS);
  final instantiate = webAssembly.getProperty<JSFunction>('instantiate'.toJS);
  final result = await (instantiate.callAsFunction(
          webAssembly, bytes, JSObject()) as JSPromise<_WasmInstantiateResult>)
      .toDart;
  return result.instance;
}

void _validateExports(JSObject exports) {
  for (final name in const [
    'memory',
    'sovereign_comrak_bridge_version',
    'sovereign_comrak_input_alloc',
    'sovereign_comrak_input_free',
    'sovereign_comrak_parse',
    'sovereign_comrak_response_free',
  ]) {
    if (exports.getProperty<JSAny?>(name.toJS) == null) {
      throw StateError('Comrak WASM bridge is missing export: $name.');
    }
  }
}

int _callExportInt(
  JSObject exports,
  String name, [
  List<JSAny?> args = const [],
]) {
  final result = _callExport(exports, name, args);
  if (result case final JSNumber number) {
    return number.toDartInt;
  }
  throw StateError('Comrak WASM export $name did not return a number.');
}

void _callExportVoid(
  JSObject exports,
  String name, [
  List<JSAny?> args = const [],
]) {
  _callExport(exports, name, args);
}

JSAny? _callExport(JSObject exports, String name, List<JSAny?> args) {
  final function = exports.getProperty<JSFunction>(name.toJS);
  return switch (args.length) {
    0 => function.callAsFunction(),
    1 => function.callAsFunction(null, args[0]),
    2 => function.callAsFunction(null, args[0], args[1]),
    3 => function.callAsFunction(null, args[0], args[1], args[2]),
    4 => function.callAsFunction(null, args[0], args[1], args[2], args[3]),
    _ => throw ArgumentError.value(args.length, 'args.length'),
  };
}

void _copyToMemory(JSObject exports, int start, Uint8List bytes) {
  if (bytes.isEmpty) return;

  final memory = _memory(exports);
  final target = JSUint8Array(memory.buffer).toDart;
  target.setRange(start, start + bytes.length, bytes);
}

_WasmComrakResponse _readResponse(JSObject exports, int responsePtr) {
  final buffer = _memory(exports).buffer.toDart;
  final view = ByteData.view(buffer);
  final abiVersion = view.getUint32(responsePtr, Endian.little);
  final revision = view.getUint32(responsePtr + 4, Endian.little);
  final statusCode = view.getUint16(responsePtr + 8, Endian.little);
  final payloadPtr = view.getUint32(responsePtr + 12, Endian.little);
  final payloadLen = view.getUint32(responsePtr + 16, Endian.little);
  final payload = payloadPtr == 0 || payloadLen == 0
      ? Uint8List(0)
      : Uint8List.fromList(Uint8List.view(buffer, payloadPtr, payloadLen));

  return _WasmComrakResponse(
    abiVersion: abiVersion,
    revision: revision,
    statusCode: statusCode,
    payload: payload,
  );
}

_WasmMemory _memory(JSObject exports) {
  return _WasmMemory(exports.getProperty<JSObject>('memory'.toJS));
}

final class _WasmComrakResponse {
  const _WasmComrakResponse({
    required this.abiVersion,
    required this.revision,
    required this.statusCode,
    required this.payload,
  });

  final int abiVersion;
  final int revision;
  final int statusCode;
  final Uint8List payload;
}

String? _decodeMarkdownInput(NativeComrakParseInput input) {
  try {
    return utf8.decode(input.utf8Text);
  } on FormatException {
    return null;
  }
}

NativeComrakParseResult _decodePayload({
  required int revision,
  required Uint8List payload,
}) {
  try {
    return NativeComrakPayloadCodec.decode(
      revision: revision,
      payload: payload,
    );
  } on FormatException catch (error) {
    return NativeComrakParseResult(
      revision: revision,
      diagnostics: [
        NativeComrakDiagnostic(
          range: const NativeComrakRange(startByte: 0, endByte: 0),
          message: 'Failed to decode WASM payload: $error',
          code: 'COMRAK_PAYLOAD_DECODE_ERROR',
          isError: true,
        ),
      ],
    );
  }
}

int _mapProfile(NativeComrakProfile profile) {
  return switch (profile) {
    NativeComrakProfile.commonMarkCore => 0,
    NativeComrakProfile.commonMarkGfm => 1,
  };
}

NativeComrakParseResult _appendDiagnostic(
  NativeComrakParseResult result,
  NativeComrakDiagnostic diagnostic,
) {
  return NativeComrakParseResult(
    revision: result.revision,
    blocks: result.blocks,
    inlineTokens: result.inlineTokens,
    markerRanges: result.markerRanges,
    exclusionRanges: result.exclusionRanges,
    diagnostics: [...result.diagnostics, diagnostic],
  );
}
