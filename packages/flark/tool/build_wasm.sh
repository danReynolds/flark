#!/bin/bash
# Builds native/flark_parse for wasm32-unknown-unknown and copies the module
# into the package's bundled assets. Uses rustup so the cross target resolves.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
CRATE="$(cd "$PKG/../../native/flark_parse" && pwd)"
TARGET=wasm32-unknown-unknown
if command -v rustup >/dev/null 2>&1; then
  rustup target add "$TARGET" --toolchain stable >/dev/null
  RUSTC_CMD="$(rustup which rustc --toolchain stable)"
  RUSTC="$RUSTC_CMD" rustup run stable cargo build --release --lib --manifest-path "$CRATE/Cargo.toml" --target "$TARGET"
else
  cargo build --release --lib --manifest-path "$CRATE/Cargo.toml" --target "$TARGET"
fi
mkdir -p "$PKG/lib/assets/wasm"
cp "$CRATE/target/$TARGET/release/flark_parse.wasm" "$PKG/lib/assets/wasm/flark_parse.wasm"
shasum -a 256 "$PKG/lib/assets/wasm/flark_parse.wasm"
