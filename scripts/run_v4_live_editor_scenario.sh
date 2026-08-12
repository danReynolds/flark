#!/usr/bin/env bash
# Runs one data-defined live-editor scenario through the fast controller/widget
# lane, the real macOS input lane, or both. Both runners consume the same JSON
# actions and outcome contract.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"
MODE="${1:-all}"
SCENARIO="${2:-$ROOT/packages/flark/test/scenarios/paragraph_split_rapid_successor.json}"
LIBRARY="$BRIDGE/target/release/libflark_abi.dylib"
APP="$ROOT/packages/flark/example/build/macos/Build/Products/Profile/Flark Dogfood.app/Contents/MacOS/Flark Dogfood"

case "$MODE" in
  headless|macos|all) ;;
  *)
    echo "usage: $0 [headless|macos|all] [scenario.json]" >&2
    exit 64
    ;;
esac

if [[ ! -f "$LIBRARY" || "${FLARK_SCENARIO_REBUILD_CORE:-0}" == "1" ]]; then
  cargo build \
    --manifest-path "$BRIDGE/Cargo.toml" \
    --package flark-abi \
    --release
fi

if [[ "$MODE" == "headless" || "$MODE" == "all" ]]; then
  (
    cd "$ROOT/packages/flark"
    FLARK_V4_LIBRARY_PATH="$LIBRARY" \
      FLARK_SCENARIO_PATH="$SCENARIO" \
      flutter test test/live_editor_scenario_test.dart --reporter expanded
  )
fi

if [[ "$MODE" == "macos" || "$MODE" == "all" ]]; then
  if [[ ! -x "$APP" || "${FLARK_SCENARIO_REUSE_APP:-0}" != "1" ]]; then
    (
      cd "$ROOT/packages/flark/example"
      flutter build macos --profile
    )
  fi
  swift "$ROOT/packages/flark/tool/live_editor_scenario_macos.swift" \
    "$SCENARIO" \
    "$APP" \
    "$LIBRARY"
fi
