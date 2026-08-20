#!/usr/bin/env bash
set -euo pipefail
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CORE_PACKAGE_ROOT="$REPO_ROOT/packages/flark_core"
FLUTTER_PACKAGE_ROOT="$REPO_ROOT/packages/flark"
EXAMPLE_ROOT="$FLUTTER_PACKAGE_ROOT/example"

run_stress=1
run_archive_runtime=1

usage() {
  cat <<'EOF'
Run the active Flark v4 release-readiness gate.

Usage:
  bash scripts/verify_v4_release.sh [options]

Options:
  --skip-stress           Skip the slow certification-stress lane.
  --skip-archive-runtime  Build and inspect archives without executing the
                          extracted-package Core/Flutter runtime smokes.
  -h, --help              Show this help.

Skip flags produce iteration evidence only, never a release receipt.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-stress)
      run_stress=0
      ;;
    --skip-archive-runtime)
      run_archive_runtime=0
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
  shift
done

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

run_in_dir "$REPO_ROOT" dart pub get
run_in_dir "$CORE_PACKAGE_ROOT" dart pub get
run_in_dir "$FLUTTER_PACKAGE_ROOT" flutter pub get
run_in_dir "$EXAMPLE_ROOT" flutter pub get

# The gate of record builds the native ABI and runs all active Rust, Core,
# Flutter, and qualification suites with an explicit library path so a skipped
# native test cannot masquerade as green.
run bash "$SCRIPT_DIR/verify_v4.sh"

run_in_dir "$REPO_ROOT" dart analyze
run_in_dir "$CORE_PACKAGE_ROOT" dart doc --dry-run
run_in_dir "$EXAMPLE_ROOT" flutter analyze
run_in_dir "$EXAMPLE_ROOT" flutter test test --reporter compact

if [ "$run_archive_runtime" -eq 0 ]; then
  run bash "$SCRIPT_DIR/verify_v4_publish_archives.sh" --skip-runtime
else
  run bash "$SCRIPT_DIR/verify_v4_publish_archives.sh"
fi

if [ "$run_stress" -eq 1 ]; then
  run bash "$SCRIPT_DIR/verify_v4_certification_stress.sh"
else
  echo
  echo "==> Skipping certification-stress lane by request."
fi

echo
echo "Flark v4 release-readiness gate passed."
