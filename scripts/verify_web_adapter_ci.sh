#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FLUTTER_PACKAGE_ROOT="$REPO_ROOT/packages/flark_flutter"

cd "$REPO_ROOT"

v3_public_web_output="$(mktemp "${TMPDIR:-/tmp}/flark-v3-public-web.XXXXXX")"
v3_worker_compile_output="$(mktemp "${TMPDIR:-/tmp}/flark-v3-worker-compile.XXXXXX")"
trap 'rm -f "$v3_public_web_output" "$v3_worker_compile_output"' EXIT

dart compile js \
  -O2 \
  -o "$v3_public_web_output" \
  test/v3/runtime/fixtures/flark_v3_public_web_compile.dart
node "$v3_public_web_output"

dart compile js \
  -O2 \
  -o "$v3_worker_compile_output" \
  test/v3/runtime/fixtures/flark_v3_web_worker_endpoint_compile.dart
node "$v3_worker_compile_output"

dart test \
  test/v2/packaging/flark_wasm_freshness_test.dart \
  --reporter compact

dart test \
  test/v3/packaging/flark_v3_web_asset_contract_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v2/native/flark_wasm_bridge_web_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_wasm_exports_web_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_web_host_store_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_web_runtime_csp_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_public_runtime_semantic_parity_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_native_wasm_digest_parity_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_web_recovery_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_web_large_document_liveness_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_web_viewport_presentation_end_to_end_test.dart \
  --reporter compact

dart test \
  --platform chrome \
  test/v3/runtime/flark_v3_rapid_edit_liveness_test.dart \
  --reporter compact

cd "$FLUTTER_PACKAGE_ROOT"

flutter test \
  --platform chrome \
  test/v2/flutter/flark_markdown_web_smoke_test.dart \
  --reporter compact

flutter test \
  --platform chrome \
  test/v3/flutter/flark_v3_web_asset_packaging_test.dart \
  --reporter compact

flutter test \
  --platform chrome \
  test/v3/flutter/flark_v3_large_document_product_checkpoint_test.dart \
  --reporter compact

(
  cd "$REPO_ROOT/example"
  flutter test \
    test/v3_engine_lab_test.dart \
    --reporter compact

  flutter test \
    --platform chrome \
    test/v3_engine_lab_web_runtime_test.dart \
    --reporter compact

  flutter test \
    --platform chrome \
    test/v3_live_editor_checkpoint_test.dart \
    --reporter compact

  flutter test \
    --platform chrome \
    test/markdown_flow_test.dart \
    --reporter compact
)
