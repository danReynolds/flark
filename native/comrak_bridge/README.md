# Flark Comrak Bridge

This crate hosts the native parser bridge for Flark's CommonMark adapter.

Developer workflow:

- after changing Rust code/ABI here, run:
  `./scripts/build_comrak_all.sh`
- if you only need one platform artifact, use:
  `build_comrak_ios.sh`, `build_comrak_android.sh`,
  `build_comrak_all.sh --host-only`, or `build_comrak_all.sh --wasm-only`

Current state:

- ABI scaffold is wired (`sovereign_comrak_bridge_version`,
  `sovereign_comrak_input_alloc`, `sovereign_comrak_input_free`,
  `sovereign_comrak_parse`, `sovereign_comrak_response_free`).
- Parse uses `comrak` and returns JSON payloads with block spans, inline spans,
  block + inline delimiter marker ranges, exclusion ranges, and diagnostics.
- Dart wiring lives in `lib/src/v2/native/native_comrak_ffi.dart` for FFI
  targets and `lib/src/v2/native/native_comrak_bridge_factory_web.dart` for
  browser WASM.
- The browser artifact is staged at
  `lib/assets/wasm/sovereign_comrak_bridge.wasm`.

Planned deliverables:

1. Stable C ABI parse entrypoint.
2. Broaden span mapping fidelity for additional inline markdown constructs.
3. Harden conformance/performance gates before full cutover.
