#!/usr/bin/env bash
set -euo pipefail
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/build/dogfood-native-receipt}"
EXAMPLE="$ROOT/packages/flark/example"
PACKAGE="$ROOT/packages/flark"
APP="$EXAMPLE/build/macos/Build/Products/Profile/Flark Dogfood.app"
MAIN="$APP/Contents/MacOS/Flark Dogfood"
ABI="$APP/Contents/Frameworks/flark_abi.framework/flark_abi"
MANIFEST="$OUT_DIR/app_bundle_manifest.json"
MACHINE_LOG="$OUT_DIR/macos_native_canary.machine.jsonl"
RECEIPT="$OUT_DIR/dogfood_native_receipt.json"
TEST_NAME='macOS routes the native editing canaries without faults or visual relay'

if [[ "$(uname -s)" != Darwin ]]; then
  echo 'verify-v4-native-canary: macOS is required' >&2
  exit 64
fi
if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
  echo 'verify-v4-native-canary: a clean worktree is required' >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
(
  cd "$EXAMPLE"
  flutter build macos --profile
)
for required in "$APP" "$MAIN" "$ABI"; do
  if [[ ! -e "$required" ]]; then
    echo "verify-v4-native-canary: missing built artifact: $required" >&2
    exit 1
  fi
done

(
  cd "$ROOT"
  dart run scripts/dogfood_bundle_manifest.dart "$APP" "$MANIFEST"
)

set +e
(
  cd "$PACKAGE"
  FLARK_CANARY_APP_EXECUTABLE="$MAIN" \
    FLARK_V4_LIBRARY_PATH="$ABI" \
    flutter test test/macos_native_canary_test.dart \
      --name "$TEST_NAME" \
      --machine \
      --concurrency=1
) >"$MACHINE_LOG" 2>&1
flutter_status=$?
set -e
if [[ $flutter_status -ne 0 ]]; then
  tail -n 120 "$MACHINE_LOG" >&2
  exit "$flutter_status"
fi

(
  cd "$ROOT"
  dart run scripts/verify_flutter_machine_test.dart "$MACHINE_LOG" "$TEST_NAME"
  dart run scripts/dogfood_native_receipt.dart \
    "$ROOT" "$APP" "$MANIFEST" "$MAIN" "$ABI" "$MACHINE_LOG" \
    "$TEST_NAME" "$RECEIPT"
)

echo "verify-v4-native-canary: PASS receipt=$RECEIPT"
