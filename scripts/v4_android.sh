#!/usr/bin/env bash
# Build, run, or verify the current v4 Android dogfood slice. This is an
# arm64 physical-device qualification lane; final package-native delivery and
# the wider ABI matrix belong to the package identity cutover.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"
EXAMPLE="$ROOT/packages/flark/example"
ACTION="${1:-verify}"
DEVICE="${2:-${FLARK_ANDROID_DEVICE:-}}"
API_LEVEL="${ANDROID_API_LEVEL:-24}"

if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  if [[ -z "${ANDROID_HOME:-}" ]]; then
    echo "v4_android: set ANDROID_NDK_HOME or ANDROID_HOME" >&2
    exit 1
  fi
  ANDROID_NDK_HOME="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1)"
fi

case "$(uname -s)" in
  Darwin)
    if [[ -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-arm64" ]]; then
      HOST_TAG="darwin-arm64"
    else
      HOST_TAG="darwin-x86_64"
    fi
    ;;
  Linux) HOST_TAG="linux-x86_64" ;;
  *)
    echo "v4_android: unsupported host OS $(uname -s)" >&2
    exit 1
    ;;
esac

TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
LINKER="$TOOLCHAIN/aarch64-linux-android${API_LEVEL}-clang"
ARCHIVER="$TOOLCHAIN/llvm-ar"
TARGET="aarch64-linux-android"
OUTPUT="$BRIDGE/target/$TARGET/release/libflark_abi.so"
STAGED="$EXAMPLE/android/app/src/main/jniLibs/arm64-v8a/libflark_abi.so"

build() {
  if [[ ! -x "$LINKER" || ! -x "$ARCHIVER" ]]; then
    echo "v4_android: incomplete NDK toolchain under $TOOLCHAIN" >&2
    exit 1
  fi
  rustup target add --toolchain stable "$TARGET" >/dev/null
  local rustc
  rustc="$(rustup which rustc --toolchain stable)"
  env \
    RUSTC="$rustc" \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LINKER" \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$ARCHIVER" \
    CC_aarch64_linux_android="$LINKER" \
    AR_aarch64_linux_android="$ARCHIVER" \
    rustup run stable cargo build \
      --manifest-path "$BRIDGE/Cargo.toml" \
      --locked \
      --release \
      --target "$TARGET" \
      --package flark-abi
  mkdir -p "$(dirname "$STAGED")"
  cp "$OUTPUT" "$STAGED"
  echo "v4_android: staged $STAGED"
}

require_device() {
  if [[ -z "$DEVICE" ]]; then
    echo "v4_android: pass a Flutter Android device id or set FLARK_ANDROID_DEVICE" >&2
    exit 1
  fi
}

case "$ACTION" in
  build) build ;;
  verify)
    require_device
    build
    (cd "$EXAMPLE" && flutter test integration_test/android_device_smoke_test.dart -d "$DEVICE")
    ;;
  run)
    require_device
    build
    (cd "$EXAMPLE" && exec flutter run -d "$DEVICE" --profile)
    ;;
  *)
    echo "usage: $0 [build|verify|run] [flutter-device-id]" >&2
    exit 64
    ;;
esac
