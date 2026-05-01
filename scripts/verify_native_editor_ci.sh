#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

run_build=1
run_android_verify=0

usage() {
  cat <<'EOF'
Verify the Sovereign native editor end-to-end CI gate locally.

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

run_in_pkg() {
  echo
  echo "==> (cd . && $*)"
  (
    cd "$REPO_ROOT"
    "$@"
  )
}

if [ "$run_build" -eq 1 ]; then
  run ./scripts/build_comrak_all.sh --strict
fi

run_in_pkg flutter analyze \
  lib \
  test/widgets/sovereign/engine \
  test/widgets/sovereign/predictive_inline_markers_test.dart

run_in_pkg flutter test test/widgets/sovereign/predictive_inline_markers_test.dart
run_in_pkg flutter test test/widgets/sovereign/engine/controller_engine_wiring_test.dart
run_in_pkg flutter test test/widgets/sovereign/engine/native_live_editing_regression_test.dart
run_in_pkg flutter test test/widgets/sovereign/engine/native_commonmark_upstream_parity_test.dart

if [ "$run_android_verify" -eq 1 ]; then
  echo
  echo "==> (cd android && ./gradlew :app:verifySovereignComrakNativeLibs)"
  if [ ! -d android ]; then
    echo "Android app verification requested, but no android/ app harness exists in this package repo yet."
    exit 1
  fi
  (
    cd android
    ./gradlew :app:verifySovereignComrakNativeLibs
  )
fi

echo
echo "Sovereign native editor CI gate passed."
