# Sovereign Execution Log

This log records work completed toward production readiness. Keep entries
append-only unless correcting a factual error.

## 2026-05-01

### Extraction

- Created `/Users/dan/Coding/sovereign`.
- Initialized a new git repo.
- Copied tracked files from `dune_gemini/packages/sovereign_editor`.
- Copied Sovereign architecture docs from Dune:
  - `docs/architecture/sovereign/`
  - selected RFCs under `docs/architecture/rfc/`
- Confirmed `dune_gemini` had unrelated dirty changes and did not modify them.

### Standalone Cleanup

- Updated `pubspec.yaml`:
  - changed description from Dune-internal wording to package wording;
  - removed `resolution: workspace`;
  - kept `publish_to: none` because the package is not release-ready.
- Added `.gitignore` for Dart/Flutter generated state, Rust targets, native
  dist artifacts, coverage, and `pubspec.lock`.
- Added `analysis_options.yaml` using `flutter_lints`.
- Updated scripts to use root-relative package paths.
- Changed Android and iOS native build outputs to package-local
  `native/comrak_bridge/dist`.
- Updated native bridge load remediation messages away from Dune paths.
- Updated README commands and expected native artifact paths.

### Audit and Planning

- Created `docs/production_readiness/audit_2026-05-01.md`.
- Created `docs/production_readiness/execution_plan.md`.
- Created this execution log.

### Verification

- `flutter pub get`: passed.
- `flutter analyze lib test`: initially failed on deprecated Flutter color APIs;
  fixed by replacing `withOpacity`/`Color.alpha` usages, then passed with no
  issues.
- `./scripts/verify_package_confidence.sh --skip-native`: initially failed
  because ordinary controller tests still need the host native library. After
  host bridge build, passed.
- `./scripts/build_comrak_all.sh --host-only`: passed and produced
  `native/comrak_bridge/target/release/libsovereign_comrak_bridge.dylib`.
- `./scripts/verify_native_editor_ci.sh --skip-build`: passed.
- `flutter test test --reporter compact`: initially exposed one full-suite
  predictive telemetry flake and one stale color expectation from the
  `withOpacity` migration; both were fixed. The default parallel full suite now
  passes with 536 tests.
- `flutter test test --reporter compact --concurrency=1`: passed with 536
  tests.
- `./scripts/verify_benchmark_lane.sh`: failed with enforced budgets:
  - warm `buildTextSpan` cache hit: 10896us vs 800us budget
  - scanner p99: 8327us vs 3500us budget
  - cold `buildTextSpan` p99: 12374us vs 4000us budget
  - emoji scanner p99: 5825us vs 3000us budget

### Test Fixes

- Updated deprecated color opacity calls to `Color.withValues(alpha: ...)`.
- Updated the blockquote color expectation to match `withValues` alpha
  semantics.
- Made the predictive telemetry truncation test deterministic by using the
  existing stale/ambiguous syntax engine instead of racing the real native
  parser.

### Release Gate and API Inventory

- Added `scripts/verify_release.sh` as the first release-readiness gate. The
  default gate includes pub get, analysis, host native build, native editor CI,
  full package tests, and enforced benchmark budgets.
- Added `docs/production_readiness/public_api_inventory_2026-05-01.md` to
  classify current barrel exports and plan public API cleanup waves.
- `./scripts/verify_release.sh --skip-benchmarks --skip-native-build`: passed.
  This verifies the non-benchmark release baseline against the existing host
  native artifact. The default release gate still fails because it includes the
  enforced benchmark lane.
