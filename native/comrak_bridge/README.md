# Sovereign Comrak Bridge

This crate hosts the native parser bridge for Sovereign's CommonMark adapter.

Developer workflow:

- after changing Rust code/ABI here, run:
  `./scripts/build_comrak_all.sh`
- if you only need one platform artifact, use:
  `build_comrak_ios.sh`, `build_comrak_android.sh`, or `build_comrak_all.sh --host-only`

Current state:

- ABI scaffold is wired (`sovereign_comrak_bridge_version`,
  `sovereign_comrak_parse`, `sovereign_comrak_response_free`).
- Parse uses `comrak` and returns JSON payloads with block spans, inline spans,
  block + inline delimiter marker ranges, exclusion ranges, and diagnostics.
- Dart wiring lives in `lib/widgets/sovereign/engine/native_comrak_ffi.dart`.

Planned deliverables:

1. Stable C ABI parse entrypoint.
2. Broaden span mapping fidelity for additional inline markdown constructs.
3. Harden conformance/performance gates before full cutover.
