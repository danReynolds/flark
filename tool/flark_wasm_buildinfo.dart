/// Shared logic for the WASM freshness guard.
///
/// The prebundled `lib/assets/wasm/flark_comrak_bridge.wasm` is a compiled
/// artifact of the Rust crate in `native/comrak_bridge`. Nothing at build time
/// forces the two to stay in sync, so a change to the Rust source that is not
/// followed by a WASM rebuild silently ships stale behavior to web consumers
/// (the exact FFI-vs-WASM drift risk the release review called out).
///
/// To catch that deterministically — without depending on byte-reproducible
/// WASM output across toolchains — the WASM build records a manifest of the
/// source inputs that produced it (`*.wasm.buildinfo`). A packaging test
/// recomputes the manifest from the crate on disk and compares. Both the
/// generator (`tool/gen_wasm_buildinfo.dart`) and the test call the functions
/// here, so they can never disagree on how the manifest is formed.
library;

import 'dart:io';

import 'package:crypto/crypto.dart';

/// Every file whose contents determine the compiled WASM bridge.
List<File> flarkWasmSourceInputs(Directory crateDir) {
  final cratePath = crateDir.path;
  final srcDir = Directory('$cratePath/src');
  return <File>[
    File('$cratePath/Cargo.toml'),
    File('$cratePath/Cargo.lock'),
    if (srcDir.existsSync())
      ...srcDir.listSync(recursive: true).whereType<File>(),
  ];
}

/// A deterministic manifest of `<sha256>  <relative-path>` lines, sorted by
/// path, covering every WASM build input. Paths are relative to [crateDir] and
/// always use `/` separators, so the manifest is identical on macOS, Linux,
/// and CI.
String flarkWasmBuildInfo(Directory crateDir) {
  final cratePath = crateDir.absolute.path;
  final entries = <String>[];
  for (final file in flarkWasmSourceInputs(crateDir)) {
    final relative = _relativePath(cratePath, file.absolute.path);
    final digest = sha256.convert(file.readAsBytesSync());
    entries.add('$digest  $relative');
  }
  entries.sort();
  return '${entries.join('\n')}\n';
}

String _relativePath(String base, String full) {
  final normalizedBase = base.replaceAll('\\', '/');
  final normalizedFull = full.replaceAll('\\', '/');
  final prefix =
      normalizedBase.endsWith('/') ? normalizedBase : '$normalizedBase/';
  return normalizedFull.startsWith(prefix)
      ? normalizedFull.substring(prefix.length)
      : normalizedFull;
}
