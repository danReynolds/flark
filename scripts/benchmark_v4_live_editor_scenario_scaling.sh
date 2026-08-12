#!/usr/bin/env bash
# Measures the same synthetic live-editor interaction shape through the
# no-window controller, the fast mounted Flutter test surface, and optionally
# a macOS host surface. This is runner throughput, not a product-performance or
# native-input receipt.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"
MODE="${1:-all}"
COUNTS="${2:-25,100,300}"
LIBRARY="$BRIDGE/target/release/libflark_abi.dylib"

case "$MODE" in
  headless|surface|macos-surface|all) ;;
  *)
    echo "usage: $0 [headless|surface|macos-surface|all] [comma-separated-counts]" >&2
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
      FLARK_SCENARIO_SCALE_COUNTS="$COUNTS" \
      flutter test test/live_editor_scenario_scaling_test.dart \
        --reporter expanded
  )
fi

if [[ "$MODE" == "surface" || "$MODE" == "all" ]]; then
  (
    cd "$ROOT/packages/flark"
    FLARK_V4_LIBRARY_PATH="$LIBRARY" \
      FLARK_SCENARIO_SCALE_COUNTS="$COUNTS" \
      flutter test test/live_editor_scenario_surface_scaling_test.dart \
        --reporter expanded
  )
fi

if [[ "$MODE" == "macos-surface" || "$MODE" == "all" ]]; then
  (
    cd "$ROOT/packages/flark/example"
    flutter test integration_test/scenario_corpus_scaling_test.dart \
      -d macos \
      --dart-define="FLARK_V4_LIBRARY_PATH=$LIBRARY" \
      --dart-define="FLARK_SCENARIO_SCALE_COUNTS=$COUNTS" \
      --reporter expanded
  )
fi
