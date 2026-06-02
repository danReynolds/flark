import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Sovereign v2 native packaging contract', () {
    test('v2 native backend shares the hook-owned native bridge asset', () {
      final hook = _read('hook/build.dart');
      final v2Backend = _read(
        'lib/src/v2/markdown/parse/sovereign_native_comrak_parse_backend.dart',
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

    test('native ABI symbols are present in all packaging anchors', () {
      final rustExports = _read('native/comrak_bridge/src/lib.rs');
      final header = _read('native/comrak_bridge/sovereign_comrak_bridge.h');
      final iosAnchor = _read('example/ios/Runner/SovereignComrakAnchor.c');

      for (final symbol in _abiSymbols) {
        expect(rustExports, contains('fn $symbol'));
        expect(header, contains(symbol));
        expect(iosAnchor, contains(symbol));
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
      expect(hook, contains('DynamicLoadingBundled()'));
      expect(hook, contains('LookupInProcess()'));
    });

    test('package declares browser WASM bridge assets', () {
      final pubspec = _read('pubspec.yaml');
      final buildAll = _read('scripts/build_comrak_all.sh');
      final wasmBuild = _read('scripts/build_comrak_wasm.sh');
      final webFactory = _read(
        'lib/src/v2/native/native_comrak_bridge_factory_web.dart',
      );

      expect(pubspec, contains('lib/assets/wasm/sovereign_comrak_bridge.wasm'));
      expect(buildAll, contains('--wasm-only'));
      expect(wasmBuild, contains('wasm32-unknown-unknown'));
      expect(
        wasmBuild,
        contains('lib/assets/wasm/sovereign_comrak_bridge.wasm'),
      );
      expect(webFactory, contains('dart:js_interop'));
      expect(webFactory, contains('dart:ui_web'));
      expect(webFactory, contains('dart:js_interop_unsafe'));
      expect(webFactory, contains('assetManager.getAssetUrl'));
      expect(webFactory, contains('WebAssembly'));
      expect(webFactory, contains('fetch'));
    });
  });
}

const _abiSymbols = [
  'sovereign_comrak_bridge_version',
  'sovereign_comrak_input_alloc',
  'sovereign_comrak_input_free',
  'sovereign_comrak_parse',
  'sovereign_comrak_response_free',
];

String _read(String path) => File(path).readAsStringSync();
