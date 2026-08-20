#!/bin/bash
set -euo pipefail
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# Per-platform build smoke for the active v4 example app.
#
# Builds the example for a target platform. Android may additionally run its
# Android-specific integration suite when an explicit device is supplied.
# Because the example depends on flark through the normal package mechanism,
# every app build compiles the native Comrak bridge through `hook/build.dart`
# for that platform, so a green build proves the native-assets cross-compile
# path. It is not device-interaction or packaged-runtime evidence.
#
# Usage:
#   ./scripts/verify_platform_smoke.sh --platform macos
#   ./scripts/verify_platform_smoke.sh --platform linux
#   ./scripts/verify_platform_smoke.sh --platform ios
#   ./scripts/verify_platform_smoke.sh --platform android          # build-only unless --device given
#   ./scripts/verify_platform_smoke.sh --platform android --device <emulator-id>

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLE_DIR="$REPO_ROOT/packages/flark/example"
ANDROID_SMOKE_TARGET="integration_test/android_device_smoke_test.dart"

platform=""
device=""

usage() {
  sed -n '3,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
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
# build-only jobs).
run flutter config --enable-native-assets

run flutter pub get

case "$platform" in
  macos)
    run flutter build macos --debug
    echo "==> macOS build proof only; packaged-app launch evidence remains separate."
    ;;
  linux)
    run flutter build linux --debug
    echo "==> Linux build proof only; packaged-app launch evidence remains separate."
    ;;
  ios)
    # Device arm64, no signing identity needed for a build smoke.
    run flutter build ios --debug --no-codesign
    echo "==> iOS build proof only; no v4 iOS device-interaction receipt is claimed."
    ;;
  android)
    run flutter build apk --debug
    if [ -n "$device" ]; then
      run flutter test "$ANDROID_SMOKE_TARGET" -d "$device"
    else
      echo "==> Android build proof only; pass --device for the Android integration smoke."
    fi
    ;;
  web)
    echo "error: the active v4 package has no web backend or web example" >&2
    exit 64
    ;;
  *)
    echo "error: unknown platform '$platform'" >&2
    usage
    exit 1
    ;;
esac

echo
echo "Flark platform build smoke ($platform) passed."
