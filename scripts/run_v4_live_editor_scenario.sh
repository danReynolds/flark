#!/usr/bin/env bash
# Runs the canonical live-editor corpus (or one selected scenario) through the
# no-window controller, the mounted Flutter surface, or both. The optional
# macOS-native mode remains a small real-input diagnostic until its actuator is
# moved behind the shared Dart executor.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"
MODE="${1:-portable}"
SCENARIO="${2:-}"
LIBRARY="$BRIDGE/target/release/libflark_abi.dylib"
APP="$ROOT/packages/flark/example/build/macos/Build/Products/Profile/Flark Dogfood.app/Contents/MacOS/Flark Dogfood"

case "$MODE" in
  headless|surface|portable|macos|all) ;;
  *)
    echo "usage: $0 [headless|surface|portable|macos|all] [scenario.json]" >&2
    exit 64
    ;;
esac

if [[ ! -f "$LIBRARY" || "${FLARK_SCENARIO_REUSE_CORE:-0}" != "1" ]]; then
  cargo build \
    --manifest-path "$BRIDGE/Cargo.toml" \
    --package flark-abi \
    --release
fi

run_portable_test() {
  local test_file="$1"
  (
    cd "$ROOT/packages/flark"
    if [[ -n "$SCENARIO" ]]; then
      FLARK_V4_LIBRARY_PATH="$LIBRARY" \
        FLARK_SCENARIO_PATH="$SCENARIO" \
        flutter test "$test_file" --reporter expanded
    else
      FLARK_V4_LIBRARY_PATH="$LIBRARY" \
        flutter test "$test_file" --reporter expanded
    fi
  )
}

if [[ "$MODE" == "headless" || "$MODE" == "portable" || "$MODE" == "all" ]]; then
  run_portable_test test/live_editor_scenario_test.dart
fi

if [[ "$MODE" == "surface" || "$MODE" == "portable" || "$MODE" == "all" ]]; then
  run_portable_test test/live_editor_scenario_surface_test.dart
fi

if [[ "$MODE" == "macos" || "$MODE" == "all" ]]; then
  NATIVE_SCENARIO="${SCENARIO:-$ROOT/packages/flark/test/scenarios/paragraph_split_rapid_successor.json}"
  if [[ ! -x "$APP" || "${FLARK_SCENARIO_REUSE_APP:-0}" != "1" ]]; then
    (
      cd "$ROOT/packages/flark/example"
      flutter build macos --profile
    )
  fi
  swift "$ROOT/packages/flark/tool/live_editor_scenario_macos.swift" \
    "$NATIVE_SCENARIO" \
    "$APP" \
    "$LIBRARY"
fi
