#!/bin/bash
# Builds native/flark_parse for wasm32-unknown-unknown on the toolchain named
# by the repository's rust-toolchain.toml and copies the module into the
# package's bundled assets. Prints the module's sha256.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
CRATE="$(cd "$PKG/../../native/flark_parse" && pwd)"
TARGET=wasm32-unknown-unknown
OUT="${1:-$PKG/lib/assets/wasm/flark_parse.wasm}"
if command -v rustup >/dev/null 2>&1; then
  TOOLCHAIN="$(cd "$CRATE" && rustup show active-toolchain | awk '{print $1}')"
  rustup target add "$TARGET" --toolchain "$TOOLCHAIN" >/dev/null
  RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" rustup run "$TOOLCHAIN" cargo build --release --locked --lib --manifest-path "$CRATE/Cargo.toml" --target "$TARGET"
else
  cargo build --release --locked --lib --manifest-path "$CRATE/Cargo.toml" --target "$TARGET"
fi
mkdir -p "$(dirname "$OUT")"
cp "$CRATE/target/$TARGET/release/flark_parse.wasm" "$OUT"
shasum -a 256 "$OUT"
