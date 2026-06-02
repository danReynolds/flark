#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$PACKAGE_ROOT/native/comrak_bridge"
ASSET_DIR="$PACKAGE_ROOT/lib/assets/wasm"
TARGET="wasm32-unknown-unknown"

if [ ! -f "$CRATE_DIR/Cargo.toml" ]; then
  echo "Could not locate flark comrak bridge Cargo.toml at $CRATE_DIR."
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  rustup target add "$TARGET"
  RUSTC_CMD="$(rustup which rustc --toolchain stable)"
  CARGO_CMD=(rustup run stable cargo)
else
  RUSTC_CMD="$(command -v rustc)"
  CARGO_CMD=(cargo)
fi

mkdir -p "$ASSET_DIR"

echo "Building Comrak WASM bridge..."
RUSTC="$RUSTC_CMD" "${CARGO_CMD[@]}" build \
  --manifest-path "$CRATE_DIR/Cargo.toml" \
  --release \
  --target "$TARGET"

cp "$CRATE_DIR/target/$TARGET/release/flark_comrak_bridge.wasm" \
  "$ASSET_DIR/flark_comrak_bridge.wasm"

echo "Comrak WASM bridge staged at lib/assets/wasm/flark_comrak_bridge.wasm."
