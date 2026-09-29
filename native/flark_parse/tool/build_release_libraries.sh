#!/bin/bash
# Builds the parser library for every target flark's build hook can download,
# for publishing as GitHub release assets.
#
#   native/flark_parse/tool/build_release_libraries.sh <out>
#
# Writes <out>/<triple>/<library> and, for upload, <out>/assets/<triple>-<library>
# with <out>/assets/SHA256SUMS. Then write_prebuilt_manifest.py pins the uploaded
# assets in packages/flark/hook/prebuilt.json.
#
# Runs on a Mac:
#   - Apple targets with Xcode, Android targets with the Android NDK (found as
#     the build hook finds it: ANDROID_NDK_HOME ... or ANDROID_HOME/ndk/<latest>);
#   - Linux targets in Docker, on an older glibc so the library loads on
#     current LTS distributions (LINUX_IMAGE overrides the image);
#   - Windows libraries are not built here: they come from the CI job's
#     `flark_parse-windows` artifact for the same commit (WINDOWS_DIR, or
#     fetched with `gh run download` when WINDOWS_RUN names the run).
# TARGETS limits the build to a space-separated subset.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$CRATE/../.." && pwd)"
OUT="${1:?usage: build_release_libraries.sh <out>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
TOOLCHAIN="$(grep -E '^channel' "$ROOT/rust-toolchain.toml" | cut -d'"' -f2)"
LINUX_IMAGE="${LINUX_IMAGE:-rust:$TOOLCHAIN-bullseye}"
ALL="aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-linux-android armv7-linux-androideabi x86_64-linux-android x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-msvc aarch64-pc-windows-msvc"
TARGETS="${TARGETS:-$ALL}"
# The lowest versions Flutter supports, so the library loads in any app.
export MACOSX_DEPLOYMENT_TARGET=10.15 IPHONEOS_DEPLOYMENT_TARGET=12.0
# Published libraries drop their symbol tables; the exported C ABI remains.
# (The crate's own profile is unchanged, so the committed Wasm module is too.)
export CARGO_PROFILE_RELEASE_STRIP=symbols
ANDROID_API=21

library() {
  case "$1" in
    *apple*) echo libflark_parse.dylib;;
    *windows*) echo flark_parse.dll;;
    *) echo libflark_parse.so;;
  esac
}

cargo_build() { # triple [env...]
  local triple="$1"; shift
  rustup target add --toolchain "$TOOLCHAIN" "$triple" >/dev/null
  # A Homebrew rustc on PATH has no cross-target std; use the toolchain's.
  env "$@" CARGO_TARGET_DIR="$OUT/.target" RUSTC="$(rustup which rustc --toolchain "$TOOLCHAIN")" \
    rustup run "$TOOLCHAIN" cargo build \
    --release --locked --lib --manifest-path "$CRATE/Cargo.toml" --target "$triple"
  mkdir -p "$OUT/$triple"
  cp "$OUT/.target/$triple/release/$(library "$triple")" "$OUT/$triple/"
}

apple() { # triple sdk
  local sdkroot clang upper snake
  sdkroot="$(xcrun --sdk "$2" --show-sdk-path)"
  clang="$(xcrun --sdk "$2" --find clang)"
  upper="$(echo "$1" | tr 'a-z-' 'A-Z_')"; snake="$(echo "$1" | tr '-' '_')"
  cargo_build "$1" SDKROOT="$sdkroot" "CARGO_TARGET_${upper}_LINKER=$clang" \
    "CC_$snake=$clang" "CFLAGS_$snake=-isysroot $sdkroot"
}

android() { # triple clang-prefix
  local ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK:-${ANDROID_NDK_ROOT:-${ANDROID_NDK_LATEST_HOME:-}}}}"
  if [ -z "$ndk" ]; then ndk="$(ls -d "${ANDROID_HOME:?set ANDROID_HOME or ANDROID_NDK_HOME}"/ndk/* | sort | tail -1)"; fi
  local bin; bin="$(ls -d "$ndk"/toolchains/llvm/prebuilt/*/bin | head -1)"
  local upper snake
  upper="$(echo "$1" | tr 'a-z-' 'A-Z_')"; snake="$(echo "$1" | tr '-' '_')"
  cargo_build "$1" "CARGO_TARGET_${upper}_LINKER=$bin/$2$ANDROID_API-clang" \
    "CARGO_TARGET_${upper}_AR=$bin/llvm-ar" "CC_$snake=$bin/$2$ANDROID_API-clang" "AR_$snake=$bin/llvm-ar"
}

linux() { # triple docker-platform
  docker run --rm --platform "$2" -v "$ROOT:/src:ro" -v "$OUT:/out" "$LINUX_IMAGE" bash -c "
    set -e
    CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_TARGET_DIR=/out/.target-$1 cargo build --release --locked --lib \
      --manifest-path /src/native/flark_parse/Cargo.toml --target $1
    mkdir -p /out/$1 && cp /out/.target-$1/$1/release/libflark_parse.so /out/$1/"
}

windows() { # triple
  local dir="${WINDOWS_DIR:-}"
  if [ -z "$dir" ] && [ -n "${WINDOWS_RUN:-}" ]; then
    dir="$OUT/.windows"
    [ -d "$dir" ] || gh run download "$WINDOWS_RUN" --name flark_parse-windows --dir "$dir"
  fi
  if [ -z "$dir" ] || [ ! -f "$dir/$1/flark_parse.dll" ]; then
    echo "no Windows library for $1: set WINDOWS_DIR or WINDOWS_RUN (the CI run for this commit)" >&2
    return 1
  fi
  mkdir -p "$OUT/$1" && cp "$dir/$1/flark_parse.dll" "$OUT/$1/"
}

for triple in $TARGETS; do
  echo "== $triple"
  case "$triple" in
    aarch64-apple-darwin|x86_64-apple-darwin) apple "$triple" macosx;;
    aarch64-apple-ios) apple "$triple" iphoneos;;
    aarch64-apple-ios-sim|x86_64-apple-ios) apple "$triple" iphonesimulator;;
    aarch64-linux-android) android "$triple" aarch64-linux-android;;
    armv7-linux-androideabi) android "$triple" armv7a-linux-androideabi;;
    x86_64-linux-android) android "$triple" x86_64-linux-android;;
    x86_64-unknown-linux-gnu) linux "$triple" linux/amd64;;
    aarch64-unknown-linux-gnu) linux "$triple" linux/arm64;;
    *-pc-windows-msvc) windows "$triple";;
    *) echo "unknown target $triple" >&2; exit 1;;
  esac
done

# Flat asset names for the release, and their checksums.
rm -rf "$OUT/assets" && mkdir -p "$OUT/assets"
for triple in $TARGETS; do
  lib="$(library "$triple")"
  cp "$OUT/$triple/$lib" "$OUT/assets/$triple-$lib"
done
(cd "$OUT/assets" && shasum -a 256 -- * > SHA256SUMS)
cat "$OUT/assets/SHA256SUMS"
