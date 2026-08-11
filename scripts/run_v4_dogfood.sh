#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"

cargo build \
  --manifest-path "$BRIDGE/Cargo.toml" \
  --package flark-abi \
  --release

export FLARK_V4_LIBRARY_PATH="$BRIDGE/target/release/libflark_abi.dylib"

cd "$ROOT/packages/flark/example"
flutter pub get
exec flutter run -d macos --profile "$@"
