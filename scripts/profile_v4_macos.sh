#!/usr/bin/env bash
# Foregrounded macOS frame-profile run for the direct v4 editor.
#
# Wall-clock frame receipts require a live display: this wakes the display,
# then holds display and system awake for the whole drive with caffeinate.
# The in-app activity assertion and the harness's own throttling/stall gate
# remain as independent checks — a run that still saw a quiet display fails
# loudly rather than emitting rejectable samples.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIBRARY="$ROOT/native/comrak_bridge/target/release/libflark_abi.dylib"
SHAPE="${FLARK_PROFILE_SHAPE:-ordinary}"
WORKLOAD="${FLARK_PROFILE_WORKLOAD:-typing}"

cargo build --manifest-path "$ROOT/native/comrak_bridge/Cargo.toml" \
  --package flark-abi --release

# Wake the display now; a run that starts against a sleeping display has no
# vsync at all.
caffeinate -u -t 2 || true

cd "$ROOT/packages/flark/example"
exec caffeinate -dis flutter drive \
  --profile \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/frame_profile_test.dart \
  -d macos \
  --dart-define=FLARK_V4_LIBRARY_PATH="$LIBRARY" \
  --dart-define=FLARK_PROFILE_SHAPE="$SHAPE" \
  --dart-define=FLARK_PROFILE_WORKLOAD="$WORKLOAD"
