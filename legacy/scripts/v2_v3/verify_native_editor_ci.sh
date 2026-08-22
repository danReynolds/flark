#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENGINE_ROOT="$REPO_ROOT"
FLUTTER_PACKAGE_ROOT="$REPO_ROOT/packages/flark_flutter"

run_build=1
run_android_verify=0

usage() {
  cat <<'EOF'
Verify the Flark native editor end-to-end CI gate locally.

Usage:
  ./scripts/verify_native_editor_ci.sh [options]

Options:
  --skip-build            Skip native artifact build step.
  --android-verify        Run an app-level Android native library verification task.
  -h, --help              Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-build)
      run_build=0
      ;;
    --android-verify)
      run_android_verify=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
  shift
done

cd "$REPO_ROOT"

run() {
  echo
  echo "==> $*"
  "$@"
}

run_in_engine() {
  echo
  echo "==> (cd . && $*)"
  (
    cd "$ENGINE_ROOT"
    "$@"
  )
}

run_in_flutter() {
  echo
  echo "==> (cd packages/flark_flutter && $*)"
  (
    cd "$FLUTTER_PACKAGE_ROOT"
    "$@"
  )
}

if [ "$run_build" -eq 1 ]; then
  run ./scripts/build_comrak_all.sh --strict
fi

# The workspace covers the legacy ABI, v3 endpoint/registry, persistent engine,
# exact parser, self-contained publication, and independent host. Selecting
# only the root package would silently omit the two production v3 crates.
# The vendored Comrak package is a dependency/source oracle whose upstream-only
# dev dependencies are intentionally not retained, so it is compiled as a
# dependency but excluded as a workspace test and Clippy target.
run cargo fmt --all --manifest-path native/comrak_bridge/Cargo.toml -- --check
run cargo test --workspace --all-targets --locked \
  --exclude comrak \
  --manifest-path native/comrak_bridge/Cargo.toml
# The 10 MiB joined producer/transport/independent-host replacement receipt is
# deliberately ignored in ordinary Rust test discovery, so select it exactly.
run cargo test --release --locked \
  --manifest-path native/comrak_bridge/Cargo.toml \
  --package flark_comrak_bridge \
  v3_endpoint::tests::ten_mib_single_line_crosses_endpoint_host_replacement_and_close_to_zero \
  -- --exact --ignored --nocapture
# Keep compiler and selected Clippy warnings strict without turning the active
# architecture prototype into a toolchain-version-sensitive style cleanup.
# These shape/style categories remain a named production-hardening backlog;
# several intentionally describe move-only parser state machines where boxing
# would add hot-path allocation or source-break a capability handoff.
run cargo clippy --workspace --all-targets --locked \
  --exclude comrak \
  --manifest-path native/comrak_bridge/Cargo.toml -- \
  -D warnings \
  -A clippy::large_enum_variant \
  -A clippy::result_large_err \
  -A clippy::type_complexity \
  -A clippy::too_many_arguments \
  -A clippy::while_let_loop \
  -A clippy::manual_div_ceil \
  -A clippy::useless_conversion

run_in_engine dart analyze \
  hook \
  lib \
  test/v2 \
  test/v3 \
  test/public_api
run_in_flutter flutter analyze lib test/v2 test/v3

run_in_engine dart test test/v2/native/flark_native_comrak_bridge_test.dart
run_in_engine dart test test/v2/packaging/flark_v2_native_packaging_contract_test.dart
run_in_engine dart test test/v2/packaging/flark_wasm_freshness_test.dart
run_in_engine dart test test/v2/markdown/flark_native_comrak_parse_backend_test.dart
run_in_engine dart test test/v2/markdown/flark_v2_native_upstream_contract_test.dart
run_in_engine dart test test/public_api --reporter compact
# Keep the v3 grammar claim tied to the complete pinned CommonMark inventory.
# This is an accounting gate, not an inflated HTML-equivalence score.
run_in_engine dart test test/v3/conformance --reporter compact
# `dart test` treats only the first positional directory as a test suite on
# some SDK versions. Keep these as separate commands so the native gate cannot
# report green after silently selecting source tests but skipping runtime tests.
run_in_engine dart test test/v3/source --reporter compact
run_in_engine dart test test/v3/host --reporter compact
run_in_engine dart test test/v3/session --reporter compact
run_in_engine dart test \
  test/v3/runtime/flark_v3_native_wasm_digest_parity_test.dart \
  --reporter compact
run_in_engine dart test \
  test/v3/runtime/flark_v3_public_runtime_semantic_parity_test.dart \
  --reporter compact
run_in_engine dart test test/v3/runtime --reporter compact
run_in_flutter flutter test test/v3 --reporter compact

if [ "$run_android_verify" -eq 1 ]; then
  run ./scripts/verify_example_packaging.sh --android
fi

echo
echo "Flark native editor CI gate passed."
