import 'dart:async';
import 'dart:typed_data';
import 'dart:ui_web' as ui_web;

import 'package:flark/flark_adapter.dart';

const _bundledWasmAsset =
    'packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm';
const _packageWasmUri =
    '/packages/flark_flutter/assets/wasm/flark_comrak_bridge.wasm';

FlarkNativeComrakParseBackend? _defaultBackend;

FlarkNativeComrakParseBackend flarkDefaultParseBackend() {
  final debugResolver = debugRequiredDefaultBackendResolver;
  if (debugResolver != null) return debugResolver();
  return _defaultBackend ??= FlarkNativeComrakParseBackend.withNativeBridge(
    wasmSource: NativeComrakWasmBytesLoaderSource(_loadWasmAsset),
  );
}

Future<Uint8List> _loadWasmAsset() {
  // Network completion must not be governed by a Flutter test's fake clock.
  return Zone.root.run(() async {
    Object? lastError;
    for (final asset in <String>[
      // Normal downstream Flutter asset-bundle key.
      _bundledWasmAsset,
      // `flutter test --platform chrome` serves package `lib/` files through
      // its package URL handler instead of the application asset bundle.
      Uri.base.resolve(_packageWasmUri).toString(),
    ]) {
      try {
        final data = await ui_web.assetManager.load(asset);
        final bytes = data.buffer.asUint8List(
          data.offsetInBytes,
          data.lengthInBytes,
        );
        if (_hasWasmMagic(bytes)) return bytes;
        lastError = StateError(
          'Flutter asset $asset did not contain a WebAssembly module.',
        );
      } catch (error) {
        lastError = error;
      }
    }
    throw StateError(
      'Unable to load the packaged Flark WASM asset: $lastError',
    );
  });
}

bool _hasWasmMagic(Uint8List bytes) {
  return bytes.length >= 4 &&
      bytes[0] == 0x00 &&
      bytes[1] == 0x61 &&
      bytes[2] == 0x73 &&
      bytes[3] == 0x6d;
}
