#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$PACKAGE_ROOT/native/comrak_bridge"
if [ ! -f "$CRATE_DIR/Cargo.toml" ]; then
  echo "Could not locate sovereign comrak bridge Cargo.toml at $CRATE_DIR."
  exit 1
fi

FRAMEWORK_OUTPUT="${IOS_XCFRAMEWORK_OUTPUT:-$PACKAGE_ROOT/native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework}"
HEADER_SRC="$CRATE_DIR/sovereign_comrak_bridge.h"
if [ ! -f "$HEADER_SRC" ]; then
  echo "Header not found: $HEADER_SRC"
  exit 1
fi

CARGO_CMD=(cargo)
RUSTC_PATH="$(command -v rustc || true)"
if command -v rustup >/dev/null 2>&1; then
  CARGO_CMD=(rustup run stable cargo)
  RUSTC_PATH="$(rustup which rustc 2>/dev/null || true)"
fi

if [ -z "$RUSTC_PATH" ]; then
  echo "Unable to locate a Rust compiler."
  exit 1
fi

ensure_rust_target() {
  local target="$1"
  if command -v rustup >/dev/null 2>&1; then
    if ! rustup target list --installed | grep -qx "$target"; then
      echo "Installing Rust target: $target"
      rustup target add "$target"
    fi
  else
    echo "rustup not found; assuming Rust target $target is already available."
  fi
}

build_target() {
  local target="$1"
  ensure_rust_target "$target"
  echo "Building comrak bridge for $target..."
  RUSTC="$RUSTC_PATH" \
    "${CARGO_CMD[@]}" build --manifest-path "$CRATE_DIR/Cargo.toml" --release --target "$target"
}

build_target "aarch64-apple-ios"
build_target "aarch64-apple-ios-sim"
build_target "x86_64-apple-ios"

DEVICE_LIB="$CRATE_DIR/target/aarch64-apple-ios/release/libsovereign_comrak_bridge.a"
SIM_ARM64_LIB="$CRATE_DIR/target/aarch64-apple-ios-sim/release/libsovereign_comrak_bridge.a"
SIM_X64_LIB="$CRATE_DIR/target/x86_64-apple-ios/release/libsovereign_comrak_bridge.a"

for lib in "$DEVICE_LIB" "$SIM_ARM64_LIB" "$SIM_X64_LIB"; do
  if [ ! -f "$lib" ]; then
    echo "Expected output missing: $lib"
    exit 1
  fi
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SIM_UNIVERSAL_LIB="$TMP_DIR/libsovereign_comrak_bridge_sim.a"
lipo -create "$SIM_ARM64_LIB" "$SIM_X64_LIB" -output "$SIM_UNIVERSAL_LIB"

HEADERS_DIR="$TMP_DIR/Headers"
mkdir -p "$HEADERS_DIR"
cp "$HEADER_SRC" "$HEADERS_DIR/"

rm -rf "$FRAMEWORK_OUTPUT"

xcodebuild -create-xcframework \
  -library "$DEVICE_LIB" -headers "$HEADERS_DIR" \
  -library "$SIM_UNIVERSAL_LIB" -headers "$HEADERS_DIR" \
  -output "$FRAMEWORK_OUTPUT"

echo "Created $FRAMEWORK_OUTPUT"
