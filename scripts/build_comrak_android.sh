#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$PACKAGE_ROOT/native/comrak_bridge"
ANDROID_JNI_OUTPUT="${ANDROID_JNI_OUTPUT:-$PACKAGE_ROOT/native/comrak_bridge/dist/android/jniLibs}"
if [ ! -f "$CRATE_DIR/Cargo.toml" ]; then
  echo "Could not locate sovereign comrak bridge Cargo.toml at $CRATE_DIR."
  exit 1
fi

if [ -z "${ANDROID_NDK_HOME:-}" ] && [ -n "${ANDROID_HOME:-}" ]; then
  ANDROID_NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -n 1)"
fi

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  echo "ANDROID_NDK_HOME is not set."
  echo "Set it directly or set ANDROID_HOME so the latest NDK can be auto-detected."
  exit 1
fi

HOST_TAG=""
case "$(uname -s)" in
  Darwin)
    if [ -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-arm64" ]; then
      HOST_TAG="darwin-arm64"
    elif [ -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64" ]; then
      HOST_TAG="darwin-x86_64"
    fi
    ;;
  Linux)
    if [ -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64" ]; then
      HOST_TAG="linux-x86_64"
    fi
    ;;
  *)
    echo "Unsupported host OS: $(uname -s)"
    exit 1
    ;;
esac

if [ -z "$HOST_TAG" ]; then
  echo "Unable to find a supported NDK prebuilt toolchain in $ANDROID_NDK_HOME/toolchains/llvm/prebuilt"
  exit 1
fi

TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
API_LEVEL="${ANDROID_API_LEVEL:-24}"
HOST_ARCH="$(uname -m)"

echo "Using crate: $CRATE_DIR"
echo "Using NDK: $ANDROID_NDK_HOME"
echo "Using toolchain: $TOOLCHAIN"
echo "Using Android API level: $API_LEVEL"

if [ "$(uname -s)" = "Darwin" ] && [ "$HOST_ARCH" = "arm64" ] && [ "$HOST_TAG" = "darwin-x86_64" ]; then
  echo "Warning: using x86_64 Android NDK toolchain on Apple Silicon host."
  echo "If builds fail with SIGKILL/tool execution errors, install Rosetta or switch to a darwin-arm64 NDK."
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
  local triple="$1"
  local abi="$2"
  local linker="$3"
  local triple_upper
  triple_upper="$(echo "$triple" | tr '[:lower:]-' '[:upper:]_')"
  local triple_snake="${triple//-/_}"
  local cxx="${linker}++"
  local ar="$TOOLCHAIN/llvm-ar"

  ensure_rust_target "$triple"

  if [ ! -x "$linker" ]; then
    echo "Android linker not found: $linker"
    exit 1
  fi
  if ! "$linker" --version >/dev/null 2>&1; then
    echo "Android linker is present but cannot execute: $linker"
    if [ "$(uname -s)" = "Darwin" ] && [ "$HOST_ARCH" = "arm64" ] && [ "$HOST_TAG" = "darwin-x86_64" ]; then
      echo "Detected Apple Silicon host with x86_64 NDK toolchain."
      echo "Install Rosetta: softwareupdate --install-rosetta --agree-to-license"
      echo "or install/select an NDK that provides prebuilt/darwin-arm64 and set ANDROID_NDK_HOME."
    fi
    exit 1
  fi
  if [ ! -x "$cxx" ]; then
    echo "Android C++ compiler not found: $cxx"
    exit 1
  fi
  if [ ! -x "$ar" ]; then
    echo "Android archiver not found: $ar"
    exit 1
  fi

  echo "Building comrak bridge for $triple ($abi)..."
  env \
    "RUSTC=$RUSTC_PATH" \
    "CARGO_TARGET_${triple_upper}_LINKER=$linker" \
    "CARGO_TARGET_${triple_upper}_AR=$ar" \
    "CC_${triple_snake}=$linker" \
    "CXX_${triple_snake}=$cxx" \
    "AR_${triple_snake}=$ar" \
    "TARGET_CC=$linker" \
    "TARGET_CXX=$cxx" \
    "TARGET_AR=$ar" \
    "CC=$linker" \
    "CXX=$cxx" \
    "AR=$ar" \
    "${CARGO_CMD[@]}" build \
      --manifest-path "$CRATE_DIR/Cargo.toml" \
      --release \
      --target "$triple"

  local lib_src="$CRATE_DIR/target/$triple/release/libsovereign_comrak_bridge.so"
  local lib_dst_dir="$ANDROID_JNI_OUTPUT/$abi"
  local lib_dst="$lib_dst_dir/libsovereign_comrak_bridge.so"

  if [ ! -f "$lib_src" ]; then
    echo "Expected output missing: $lib_src"
    exit 1
  fi

  mkdir -p "$lib_dst_dir"
  cp "$lib_src" "$lib_dst"
  echo "Staged $lib_dst"
}

build_target "aarch64-linux-android" "arm64-v8a" "$TOOLCHAIN/aarch64-linux-android${API_LEVEL}-clang"
build_target "armv7-linux-androideabi" "armeabi-v7a" "$TOOLCHAIN/armv7a-linux-androideabi${API_LEVEL}-clang"
build_target "x86_64-linux-android" "x86_64" "$TOOLCHAIN/x86_64-linux-android${API_LEVEL}-clang"

echo "Done. Android JNI libs are staged under $ANDROID_JNI_OUTPUT."
