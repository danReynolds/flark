#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$PKG_ROOT"

run_full_suite=0
run_native=1
run_benchmarks=0

usage() {
  cat <<'EOF'
Run the Sovereign editor package confidence gate (fast local maintenance checks).

Usage:
  ./scripts/verify_package_confidence.sh [options]

Options:
  --full-suite       Also run the full package test suite (`flutter test test`).
  --skip-native      Skip native backend/parity tests (useful if native artifacts are not built).
  --benchmarks       Run benchmark lane with enforced budgets.
  -h, --help         Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --full-suite)
      run_full_suite=1
      ;;
    --skip-native)
      run_native=0
      ;;
    --benchmarks)
      run_benchmarks=1
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

run_in_pkg() {
  echo
  echo "==> (cd . && $*)"
  (
    cd "$PKG_ROOT"
    "$@"
  )
}

run_in_example() {
  echo
  echo "==> (cd example && $*)"
  (
    cd "$PKG_ROOT/example"
    "$@"
  )
}

echo "Sovereign package confidence gate"
echo "Repo: $REPO_ROOT"
echo "Package: $PKG_ROOT"

run_in_pkg flutter analyze hook lib test

# High-signal regression suites (fast enough for local confidence).
run_in_pkg flutter test test/v2/core
run_in_pkg flutter test test/v2/markdown
run_in_pkg flutter test test/v2/projection
run_in_pkg flutter test test/v2/render_plan
run_in_pkg flutter test test/v2/flutter/sovereign_flutter_controller_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_markdown_surface_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_markdown_input_policy_contract_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_live_rendered_transition_matrix_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_live_rendered_visual_layout_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_live_rendered_editable_text_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_read_only_preview_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_render_plan_overlay_controls_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_v2_visual_golden_test.dart
run_in_pkg flutter test test/v2/flutter/sovereign_markdown_web_smoke_test.dart -d chrome --reporter compact
run_in_example flutter test test/widget_test.dart --reporter compact

if [ "$run_native" -eq 1 ]; then
  run_in_pkg flutter test test/v2/native/sovereign_native_comrak_bridge_test.dart
  run_in_pkg flutter test test/v2/packaging/sovereign_v2_native_packaging_contract_test.dart
  run_in_pkg flutter test test/v2/markdown/sovereign_native_comrak_parse_backend_test.dart
  run_in_pkg flutter test test/v2/markdown/sovereign_v2_native_upstream_contract_test.dart
fi

if [ "$run_full_suite" -eq 1 ]; then
  run_in_pkg flutter test test --reporter compact
fi

if [ "$run_benchmarks" -eq 1 ]; then
  run_in_pkg ./scripts/verify_benchmark_lane.sh
fi

echo
echo "Sovereign package confidence gate passed."
