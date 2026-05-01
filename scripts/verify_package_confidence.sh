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

echo "Sovereign package confidence gate"
echo "Repo: $REPO_ROOT"
echo "Package: $PKG_ROOT"

run_in_pkg flutter analyze hook lib test

# High-signal regression suites (fast enough for local confidence).
run_in_pkg flutter test test/widgets/sovereign/predictive_inline_markers_test.dart
run_in_pkg flutter test test/widgets/sovereign/engine/controller_engine_wiring_test.dart
run_in_pkg flutter test test/widgets/sovereign/engine/syntax_sync_coordinator_regression_test.dart
run_in_pkg flutter test test/widgets/sovereign/code_fence_exit_test.dart
run_in_pkg flutter test test/widgets/sovereign/list_key_integration_test.dart
run_in_pkg flutter test test/widgets/sovereign/task_checkbox_interaction_test.dart
run_in_pkg flutter test test/widgets/sovereign/link_actions_overlay_test.dart
run_in_pkg flutter test test/widgets/sovereign/image_actions_overlay_test.dart

if [ "$run_native" -eq 1 ]; then
  run_in_pkg flutter test test/widgets/sovereign/engine/native_comrak_parse_backend_test.dart
  run_in_pkg flutter test test/widgets/sovereign/engine/native_commonmark_upstream_parity_test.dart
fi

if [ "$run_full_suite" -eq 1 ]; then
  run_in_pkg flutter test test --reporter compact
fi

if [ "$run_benchmarks" -eq 1 ]; then
  run_in_pkg ./scripts/verify_benchmark_lane.sh
fi

echo
echo "Sovereign package confidence gate passed."
