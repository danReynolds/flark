#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

run_benchmarks=1
run_native_build=1

usage() {
  cat <<'EOF'
Run the Sovereign release-readiness gate.

Usage:
  ./scripts/verify_release.sh [options]

Options:
  --skip-benchmarks     Skip enforced benchmark budgets.
  --skip-native-build   Reuse existing host native artifacts.
  -h, --help            Show this help.

This gate is expected to fail until all production-readiness blockers are
resolved. Use --skip-benchmarks only for iteration when recording partial
baseline health.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-benchmarks)
      run_benchmarks=0
      ;;
    --skip-native-build)
      run_native_build=0
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

run_in_dir() {
  local dir="$1"
  shift
  echo
  echo "==> (cd ${dir#$REPO_ROOT/} && $*)"
  (
    cd "$dir"
    "$@"
  )
}

run flutter pub get
run flutter analyze hook lib test
run_in_dir "$REPO_ROOT/example" flutter pub get
run_in_dir "$REPO_ROOT/example" flutter analyze
run_in_dir "$REPO_ROOT/example" flutter test test --reporter compact

if [ "$run_native_build" -eq 1 ]; then
  run ./scripts/build_comrak_all.sh --host-only
fi

run ./scripts/verify_native_editor_ci.sh --skip-build
run flutter test test --reporter compact

if [ "$run_benchmarks" -eq 1 ]; then
  run ./scripts/verify_benchmark_lane.sh
else
  echo
  echo "==> Skipping benchmark lane by request."
fi

echo
echo "Sovereign release-readiness gate passed."
