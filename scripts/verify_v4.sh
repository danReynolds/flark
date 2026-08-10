#!/usr/bin/env bash
# One local gate for the direct v4 path (macOS, per the Mac-first plan).
# Builds the flark-abi library and runs every focused v4 suite with
# FLARK_V4_LIBRARY_PATH exported: without it the Dart and Flutter suites
# silently skip, so a run without this script can look green while executing
# nothing. No pipeline here may mask an exit code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"
PROFILE="${FLARK_V4_PROFILE:-debug}"

if [[ "$PROFILE" == "release" ]]; then
  cargo build --manifest-path "$BRIDGE/Cargo.toml" --package flark-abi --release
else
  cargo build --manifest-path "$BRIDGE/Cargo.toml" --package flark-abi
fi
LIBRARY="$BRIDGE/target/$PROFILE/libflark_abi.dylib"
if [[ ! -f "$LIBRARY" ]]; then
  echo "verify_v4: missing $LIBRARY" >&2
  exit 1
fi

cargo test --manifest-path "$BRIDGE/Cargo.toml" -p flark-runtime -p flark-abi

(cd "$ROOT/packages/flark_core" && dart analyze)
(cd "$ROOT/packages/flark_core" && FLARK_V4_LIBRARY_PATH="$LIBRARY" dart test)
(cd "$ROOT/packages/flark" && FLARK_V4_LIBRARY_PATH="$LIBRARY" flutter analyze)
(cd "$ROOT/packages/flark" && FLARK_V4_LIBRARY_PATH="$LIBRARY" flutter test)

echo "verify_v4: rust + dart + flutter v4 suites all executed and passed."
