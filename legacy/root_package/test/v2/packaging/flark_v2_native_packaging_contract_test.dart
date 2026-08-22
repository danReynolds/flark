import 'dart:io';

import 'package:test/test.dart';

void main() {
  group('Flark v2 native packaging contract', () {
    test('v2 native backend shares the hook-owned native bridge asset', () {
      final hook = _read('hook/build.dart');
      final v2Backend = _read(
        'lib/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart',
      );
      final nativeFfi = _read('lib/src/v2/native/native_comrak_ffi.dart');

      expect(
        hook,
        contains("const _assetName = 'src/v2/native/native_comrak_ffi.dart';"),
      );
      expect(
        v2Backend,
        contains("../../native/native_comrak_bridge_factory.dart"),
      );
      expect(v2Backend, contains("../../native/native_comrak_ffi.dart"));
      expect(
        nativeFfi,
        contains('abstract interface class NativeComrakBridge'),
      );
      expect(nativeFfi, isNot(contains('package:flutter/')));
      expect(nativeFfi, isNot(contains("import 'dart:ui'")));
    });

    test('native ABI symbols are exported by the crate and declared in the '
        'header', () {
      // Every FFI platform (macOS/Android/Linux/iOS) links the hook-built
      // dynamic library and resolves these symbols from it — there is no
      // per-app anchor to keep in sync anymore, so the crate export and the C
      // header are the two sources of truth the contract guards.
      final rustExports = _read('native/comrak_bridge/src/lib.rs');
      final header = _read('native/comrak_bridge/flark_comrak_bridge.h');

      for (final symbol in _abiSymbols) {
        expect(rustExports, contains('fn $symbol'));
        expect(header, contains(symbol));
      }
    });

    test('package declares native asset dependencies used by the hook', () {
      final pubspec = _read('pubspec.yaml');
      final hook = _read('hook/build.dart');

      expect(pubspec, contains('ffi:'));
      expect(pubspec, contains('hooks:'));
      expect(pubspec, contains('code_assets:'));
      expect(hook, contains('package:hooks/hooks.dart'));
      expect(hook, contains('package:code_assets/code_assets.dart'));
      // Every FFI platform builds a bundled dynamic library through the hook —
      // iOS included, via its own cross-compile triples (no static XCFramework,
      // no process-linked fallback).
      expect(hook, contains('DynamicLoadingBundled()'));
      expect(hook, contains('aarch64-apple-ios')); // iOS device
      expect(hook, contains('aarch64-apple-ios-sim')); // iOS simulator (arm64)
      expect(hook, isNot(contains('LookupInProcess()')));
    });

    test('Dart engine owns WASM while Flutter adapter declares its asset', () {
      final enginePubspec = _read('pubspec.yaml');
      final adapterPubspec = _read('packages/flark_flutter/pubspec.yaml');
      final adapterOverrides = _read(
        'packages/flark_flutter/pubspec_overrides.yaml',
      );
      final buildAll = _read('scripts/build_comrak_all.sh');
      final wasmBuild = _read('scripts/build_comrak_wasm.sh');
      final webFactory = _read(
        'lib/src/v2/native/native_comrak_bridge_factory_web.dart',
      );
      final flutterWebLoader = _read(
        'packages/flark_flutter/lib/src/v2/flutter/'
        'flark_default_parse_backend_web.dart',
      );

      expect(enginePubspec, isNot(contains('sdk: flutter')));
      expect(adapterPubspec, contains('flark: ^0.1.1'));
      expect(adapterOverrides, contains('path: ../..'));
      expect(
        adapterPubspec,
        contains('lib/assets/wasm/flark_comrak_bridge.wasm'),
      );
      expect(buildAll, contains('--wasm-only'));
      expect(wasmBuild, contains('wasm32-unknown-unknown'));
      expect(wasmBuild, contains('lib/assets/wasm/flark_comrak_bridge.wasm'));
      expect(wasmBuild, contains('packages/flark_flutter/lib/assets/wasm'));
      expect(webFactory, contains('dart:js_interop'));
      expect(webFactory, contains('dart:js_interop_unsafe'));
      expect(webFactory, isNot(contains('dart:ui_web')));
      expect(webFactory, isNot(contains('assetManager')));
      expect(webFactory, contains('NativeComrakWasmUriSource'));
      expect(webFactory, contains('NativeComrakWasmBytesSource'));
      expect(webFactory, contains('NativeComrakWasmBytesLoaderSource'));
      expect(webFactory, contains('Uint8List.fromList(bytes)'));
      expect(webFactory, isNot(contains('flark_flutter')));
      expect(webFactory, isNot(contains('rootBundle')));
      expect(flutterWebLoader, contains("'dart:ui_web'"));
      expect(flutterWebLoader, contains('assetManager.load'));
      expect(
        flutterWebLoader,
        contains('packages/flark_flutter/lib/assets/wasm/'),
      );
      expect(flutterWebLoader, contains('packages/flark_flutter/assets/wasm/'));
      expect(flutterWebLoader, contains('NativeComrakWasmBytesLoaderSource'));
      expect(webFactory, contains('Uri.base.resolve'));
      expect(webFactory, contains('WebAssembly'));
      expect(webFactory, contains('fetch'));
    });
  });
}

const _abiSymbols = [
  'flark_comrak_bridge_version',
  'flark_comrak_input_alloc',
  'flark_comrak_input_free',
  'flark_comrak_parse',
  'flark_comrak_response_free',
];

String _read(String path) => File(path).readAsStringSync();
