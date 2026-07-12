#!/bin/bash
set -euo pipefail

# Per-platform launch smoke for the flark example app.
#
# Builds the example for a target platform and (where a device/desktop is
# available) runs the `integration_test/flark_smoke_test.dart` suite on it.
# Because the example depends on flark through the normal package mechanism,
# every app build compiles the native Comrak bridge through `hook/build.dart`
# for that platform — so a green build is itself proof that the native-assets
# cross-compile path works there, and the integration run proves the bridge
# actually loads and parses on the target.
#
# Usage:
#   ./scripts/verify_platform_smoke.sh --platform macos
#   ./scripts/verify_platform_smoke.sh --platform linux            # wrap in xvfb-run in headless CI
#   ./scripts/verify_platform_smoke.sh --platform ios              # build-only unless --device given
#   ./scripts/verify_platform_smoke.sh --platform ios --device <simulator-id>
#   ./scripts/verify_platform_smoke.sh --platform android          # build-only unless --device given
#   ./scripts/verify_platform_smoke.sh --platform android --device <emulator-id>
#   ./scripts/verify_platform_smoke.sh --platform web

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLE_DIR="$REPO_ROOT/example"
SMOKE_TARGET="integration_test/flark_smoke_test.dart"

platform=""
device=""

usage() {
  sed -n '3,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --platform)
      platform="${2:-}"
      shift 2
      ;;
    --device)
      device="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [ -z "$platform" ]; then
  echo "error: --platform is required" >&2
  usage
  exit 1
fi

run() {
  echo
  echo "==> $*"
  "$@"
}

cd "$EXAMPLE_DIR"

# The example compiles the flark native bridge through flark's hook/build.dart,
# which only runs when native-assets is enabled. That flag is persisted in the
# global Flutter config, so a fresh CI runner (or a contributor's first run)
# does not have it — without this the native builds below would silently omit
# the bridge and ship an app with no parser backend (a hollow green for the
# build-only iOS/Android jobs). Web is unaffected (it uses the prebundled WASM).
run flutter config --enable-native-assets

run flutter pub get

case "$platform" in
  macos)
    run flutter build macos --debug
    run flutter test "$SMOKE_TARGET" -d macos
    ;;
  linux)
    run flutter build linux --debug
    run flutter test "$SMOKE_TARGET" -d linux
    ;;
  ios)
    # Device arm64, no signing identity needed for a build smoke.
    run flutter build ios --debug --no-codesign
    if [ -n "$device" ]; then
      run flutter test "$SMOKE_TARGET" -d "$device"
    else
      echo "==> No --device given; ran iOS build smoke only."
    fi
    ;;
  android)
    run flutter build apk --debug
    if [ -n "$device" ]; then
      run flutter test "$SMOKE_TARGET" -d "$device"
    else
      echo "==> No --device given; ran Android build smoke only."
    fi
    ;;
  web)
    run flutter test "$SMOKE_TARGET" -d chrome
    ;;
  *)
    echo "error: unknown platform '$platform'" >&2
    usage
    exit 1
    ;;
esac

echo
echo "Flark platform smoke ($platform) passed."
