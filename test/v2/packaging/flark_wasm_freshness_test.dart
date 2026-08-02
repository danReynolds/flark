import 'dart:io';

import 'package:test/test.dart';

import '../../../tool/flark_wasm_buildinfo.dart';

/// Guards against FFI-vs-WASM drift and root/adapter staging drift. The root
/// `lib/assets/wasm/flark_comrak_bridge.wasm` is the build-authority artifact;
/// the official Flutter adapter receives an identical staged copy.
///
/// `scripts/build_comrak_wasm.sh` records the exact source inputs of each WASM
/// build in a `.buildinfo` manifest. This test recomputes that manifest from
/// the crate on disk; a mismatch means the WASM is stale and must be rebuilt.
/// It is pure file hashing — no WASM toolchain required — so it runs in every
/// lane and never flakes on build non-reproducibility.
void main() {
  test('freshness manifest covers production Cargo workspace members', () {
    final manifest = flarkWasmBuildInfo(Directory('native/comrak_bridge'));
    final paths = manifest
        .trim()
        .split('\n')
        .map((line) => line.substring(line.indexOf('  ') + 2))
        .toList();

    expect(
      paths,
      contains('native/comrak_bridge/crates/flark_engine/Cargo.toml'),
    );
    expect(
      paths,
      contains('native/comrak_bridge/crates/flark_engine/src/document.rs'),
    );
    expect(paths, orderedEquals([...paths]..sort()));
    expect(paths.where((path) => path.contains('/target/')), isEmpty);
    expect(
      paths.where((path) => path.contains('tool/parser_research')),
      isEmpty,
    );
  });

  test('root and Flutter WASM assets are fresh and identical', () {
    final crateDir = Directory('native/comrak_bridge');
    expect(
      crateDir.existsSync(),
      isTrue,
      reason: 'expected the Rust crate at ${crateDir.absolute.path}',
    );

    final rootWasm = File('lib/assets/wasm/flark_comrak_bridge.wasm');
    final rootBuildinfo = File(
      'lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo',
    );
    final flutterWasm = File(
      'packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm',
    );
    final flutterBuildinfo = File(
      'packages/flark_flutter/lib/assets/wasm/'
      'flark_comrak_bridge.wasm.buildinfo',
    );

    for (final asset in [
      rootWasm,
      rootBuildinfo,
      flutterWasm,
      flutterBuildinfo,
    ]) {
      expect(
        asset.existsSync(),
        isTrue,
        reason:
            'missing staged WASM asset ${asset.path} — run '
            './scripts/build_comrak_wasm.sh',
      );
    }

    final committed = rootBuildinfo.readAsStringSync();
    final current = flarkWasmBuildInfo(crateDir);

    expect(
      current,
      committed,
      reason:
          'The Rust comrak bridge sources changed since the WASM binary was '
          'last built, so lib/assets/wasm/flark_comrak_bridge.wasm is stale '
          'and web builds would ship different parser behavior than native. '
          'Rebuild it with ./scripts/build_comrak_wasm.sh and commit the '
          'updated .wasm and .buildinfo.',
    );

    expect(
      flutterBuildinfo.readAsStringSync(),
      committed,
      reason:
          'The Flutter adapter WASM buildinfo differs from the root build '
          'authority. Restage it with ./scripts/build_comrak_wasm.sh.',
    );
    expect(
      flutterWasm.readAsBytesSync(),
      orderedEquals(rootWasm.readAsBytesSync()),
      reason:
          'The Flutter adapter WASM binary differs from the root build '
          'authority. Restage it with ./scripts/build_comrak_wasm.sh.',
    );
  });
}
