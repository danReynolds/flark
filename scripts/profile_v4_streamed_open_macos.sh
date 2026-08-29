#!/usr/bin/env bash
# Foregrounded macOS receipt for the RFC 029 A3 streamed open: time from the
# public open call to the first painted frame carrying certified rows.
#
# Frame receipts need a live display, so this wakes the display and holds it
# awake for the drive, exactly as the typing profile does. The library must
# carry the opening-session entry points; build it with
#   cargo build --release -p flark-abi --features opening-session
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_BYTES="${FLARK_PROFILE_SOURCE_BYTES:-10485760}"
RUN_COUNT="${FLARK_PROFILE_RUN_COUNT:-5}"
LIBRARY="${FLARK_V4_LIBRARY_PATH:-$ROOT/packages/flark/native/comrak_bridge/target/release/libflark_abi.dylib}"

if [[ ! -f "$LIBRARY" ]]; then
  echo "profile_v4_streamed_open_macos: missing $LIBRARY" >&2
  echo "build it with: cargo build --release -p flark-abi --features opening-session" >&2
  exit 1
fi

caffeinate -u -t 2 || true

cd "$ROOT/packages/flark_flutter/example"
exec caffeinate -dis flutter drive \
  --profile \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/streamed_open_paint_test.dart \
  -d macos \
  --dart-define=FLARK_V4_LIBRARY_PATH="$LIBRARY" \
  --dart-define=FLARK_PROFILE_SOURCE_BYTES="$SOURCE_BYTES" \
  --dart-define=FLARK_PROFILE_RUN_COUNT="$RUN_COUNT"
