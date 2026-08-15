#!/usr/bin/env bash
# Build, run, verify, or profile the physical Android product path. flark_core's
# build hook owns the Rust compilation and APK native-asset delivery; this
# script must never stage a parallel jniLibs copy.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLE="$ROOT/packages/flark/example"
ACTION="${1:-verify}"
DEVICE="${2:-${FLARK_ANDROID_DEVICE:-}}"
PROFILE_SHAPE="${FLARK_PROFILE_SHAPE:-ordinary}"
PROFILE_WORKLOAD="${FLARK_PROFILE_WORKLOAD:-inline-typing}"
PROFILE_SOURCE_BYTES="${FLARK_PROFILE_SOURCE_BYTES:-1048576}"
PROFILE_SAMPLE_COUNT="${FLARK_PROFILE_SAMPLE_COUNT:-120}"
PROFILE_REOPEN_COUNT="${FLARK_PROFILE_REOPEN_COUNT:-0}"

build() {
  (cd "$EXAMPLE" && flutter build apk --profile --target-platform android-arm64)
}

require_device() {
  if [[ -z "$DEVICE" ]]; then
    echo "v4_android: pass a Flutter Android device id or set FLARK_ANDROID_DEVICE" >&2
    exit 1
  fi
}

case "$ACTION" in
  build) build ;;
  verify)
    require_device
    (cd "$EXAMPLE" && flutter test integration_test/android_device_smoke_test.dart -d "$DEVICE")
    ;;
  run)
    require_device
    (cd "$EXAMPLE" && exec flutter run -d "$DEVICE" --profile)
    ;;
  profile)
    require_device
    (
      cd "$EXAMPLE"
      flutter drive \
        --profile \
        --driver=test_driver/integration_test.dart \
        --target=integration_test/frame_profile_test.dart \
        -d "$DEVICE" \
        --dart-define=FLARK_PROFILE_SHAPE="$PROFILE_SHAPE" \
        --dart-define=FLARK_PROFILE_WORKLOAD="$PROFILE_WORKLOAD" \
        --dart-define=FLARK_PROFILE_SOURCE_BYTES="$PROFILE_SOURCE_BYTES" \
        --dart-define=FLARK_PROFILE_SAMPLE_COUNT="$PROFILE_SAMPLE_COUNT" \
        --dart-define=FLARK_PROFILE_REOPEN_COUNT="$PROFILE_REOPEN_COUNT"
    )
    echo "v4_android: receipt $EXAMPLE/build/integration_response_data.json"
    ;;
  *)
    echo "usage: $0 [build|verify|profile|run] [flutter-device-id]" >&2
    exit 64
    ;;
esac
