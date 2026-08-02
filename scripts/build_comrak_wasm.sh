#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$PACKAGE_ROOT/native/comrak_bridge"
ROOT_ASSET_DIR="$PACKAGE_ROOT/lib/assets/wasm"
FLUTTER_ASSET_DIR="$PACKAGE_ROOT/packages/flark_flutter/lib/assets/wasm"
ROOT_WORKER_DIR="$PACKAGE_ROOT/lib/assets/worker"
FLUTTER_WORKER_DIR="$PACKAGE_ROOT/packages/flark_flutter/lib/assets/worker"
WORKER_NAME="flark_v3_parser_worker.js"
TARGET="wasm32-unknown-unknown"
RUST_TOOLCHAIN="${FLARK_RUST_TOOLCHAIN:-stable}"

if [ ! -f "$CRATE_DIR/Cargo.toml" ]; then
  echo "Could not locate flark comrak bridge Cargo.toml at $CRATE_DIR."
  exit 1
fi

if [ ! -f "$ROOT_WORKER_DIR/$WORKER_NAME" ]; then
  echo "Could not locate the external v3 parser worker at $ROOT_WORKER_DIR/$WORKER_NAME."
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  # Cargo, rustc, and the target standard library must come from one toolchain.
  # The override is useful for reproducible release builds and for recovering
  # from a locally stale/corrupt `stable` target installation.
  rustup target add --toolchain "$RUST_TOOLCHAIN" "$TARGET"
  RUSTC_CMD="$(rustup which rustc --toolchain "$RUST_TOOLCHAIN")"
  CARGO_CMD=(rustup run "$RUST_TOOLCHAIN" cargo)
else
  RUSTC_CMD="$(command -v rustc)"
  CARGO_CMD=(cargo)
fi

mkdir -p \
  "$ROOT_ASSET_DIR" \
  "$FLUTTER_ASSET_DIR" \
  "$FLUTTER_WORKER_DIR"

echo "Building Comrak WASM bridge..."
REMAP_RUSTFLAG="--remap-path-prefix=$PACKAGE_ROOT=."
BUILD_RUSTFLAGS="${RUSTFLAGS:-}"
if [ -n "$BUILD_RUSTFLAGS" ]; then
  BUILD_RUSTFLAGS="$BUILD_RUSTFLAGS $REMAP_RUSTFLAG"
else
  BUILD_RUSTFLAGS="$REMAP_RUSTFLAG"
fi
BUILD_ENV=(env "RUSTC=$RUSTC_CMD")
if [ -n "${CARGO_ENCODED_RUSTFLAGS:-}" ]; then
  BUILD_ENV+=(
    "CARGO_ENCODED_RUSTFLAGS=${CARGO_ENCODED_RUSTFLAGS}"$'\x1f'"$REMAP_RUSTFLAG"
  )
else
  BUILD_ENV+=("RUSTFLAGS=$BUILD_RUSTFLAGS")
fi
"${BUILD_ENV[@]}" "${CARGO_CMD[@]}" build \
  --locked \
  --manifest-path "$CRATE_DIR/Cargo.toml" \
  --release \
  --target "$TARGET"

cp "$CRATE_DIR/target/$TARGET/release/flark_comrak_bridge.wasm" \
  "$ROOT_ASSET_DIR/flark_comrak_bridge.wasm"
cp "$ROOT_ASSET_DIR/flark_comrak_bridge.wasm" \
  "$FLUTTER_ASSET_DIR/flark_comrak_bridge.wasm"
cp "$ROOT_WORKER_DIR/$WORKER_NAME" \
  "$FLUTTER_WORKER_DIR/$WORKER_NAME"

# Record the Rust sources this binary was built from, so the packaging
# freshness test can fail the gate if the WASM is ever left stale relative to
# the crate (FFI-vs-WASM behavioral drift). The generator writes the identical
# manifest beside both staged binaries.
echo "Recording WASM source manifest..."
dart run "$PACKAGE_ROOT/tool/gen_wasm_buildinfo.dart"

echo "Comrak WASM bridge staged at:"
echo "  lib/assets/wasm/flark_comrak_bridge.wasm"
echo "  packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm"
echo "External v3 Worker staged at:"
echo "  lib/assets/worker/$WORKER_NAME"
echo "  packages/flark_flutter/lib/assets/worker/$WORKER_NAME"
