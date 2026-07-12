import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../../../tool/flark_wasm_buildinfo.dart';

/// Guards against FFI-vs-WASM drift: the committed
/// `lib/assets/wasm/flark_comrak_bridge.wasm` is a compiled artifact of the
/// Rust crate, and nothing at build time forces them to stay in sync. If the
/// Rust source changes but the WASM is not rebuilt, web consumers silently get
/// stale parser behavior while native consumers get the new behavior.
///
/// `scripts/build_comrak_wasm.sh` records the exact source inputs of each WASM
/// build in a `.buildinfo` manifest. This test recomputes that manifest from
/// the crate on disk; a mismatch means the WASM is stale and must be rebuilt.
/// It is pure file hashing — no WASM toolchain required — so it runs in every
/// lane and never flakes on build non-reproducibility.
void main() {
  test('committed WASM manifest matches the current Rust bridge sources', () {
    final crateDir = Directory('native/comrak_bridge');
    expect(
      crateDir.existsSync(),
      isTrue,
      reason: 'expected the Rust crate at ${crateDir.absolute.path}',
    );

    final manifestFile = File(
      'lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo',
    );
    expect(
      manifestFile.existsSync(),
      isTrue,
      reason:
          'missing WASM build manifest — run ./scripts/build_comrak_wasm.sh to '
          'generate lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo',
    );

    final committed = manifestFile.readAsStringSync();
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
  });
}
