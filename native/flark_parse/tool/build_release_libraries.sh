#!/bin/bash
# Builds the parser library for every target flark's build hook can download,
# for publishing as GitHub release assets.
#
#   native/flark_parse/tool/build_release_libraries.sh <out>
#
# `rk release parser` runs it from a clean copy of the release commit, and
# says which one in RK_SOURCE_COMMIT (and the repository in RK_REPOSITORY).
# Run by hand, it builds the checked-out commit.
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
#     `flark_parse-windows` artifact, from a successful push or manual CI run
#     of this exact commit. The script finds that run; WINDOWS_RUN names one.
#     (WINDOWS_DIR takes a local directory instead, for trying the script; it
#     is not tied to a commit.)
# TARGETS limits the build to a space-separated subset.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$CRATE/../.." && pwd)"
OUT="${1:?usage: build_release_libraries.sh <out>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
HEAD="${RK_SOURCE_COMMIT:-$(git -C "$ROOT" rev-parse HEAD)}"
REPO="${RK_REPOSITORY:-}"
# The libraries are pinned against this commit. rk's copy is the committed
# source by construction; a working tree must match it.
if [ -z "${RK_SOURCE_COMMIT:-}" ] &&
  ! git -C "$ROOT" diff --quiet HEAD -- native/flark_parse ':!native/flark_parse/tool' rust-toolchain.toml; then
  echo "the parse crate or rust-toolchain.toml has uncommitted changes" >&2
  exit 1
fi
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
  # An @rpath install name rather than this machine's build path, with header
  # room for the absolute install name Dart's bundler rewrites it to.
  cargo_build "$1" SDKROOT="$sdkroot" "CARGO_TARGET_${upper}_LINKER=$clang" \
    "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=-Wl,-install_name,@rpath/libflark_parse.dylib -C link-arg=-Wl,-headerpad_max_install_names" \
    "CC_$snake=$clang" "CFLAGS_$snake=-isysroot $sdkroot"
}

android() { # triple clang-prefix
  local ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK:-${ANDROID_NDK_ROOT:-${ANDROID_NDK_LATEST_HOME:-}}}}"
  # Android Studio's default SDK location on a Mac, when nothing names one.
  local sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
  if [ -z "$ndk" ]; then ndk="$(ls -d "$sdk"/ndk/* 2>/dev/null | sort | tail -1)"; fi
  if [ -z "$ndk" ]; then echo "no Android NDK: set ANDROID_HOME or ANDROID_NDK_HOME" >&2; return 1; fi
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
  local dir run
  # A pull request's run builds the pull request merged into main, not this
  # commit. A push to main or a manual run checks out the commit itself.
  if [ -z "${WINDOWS_RUN:-}" ] && [ -z "${WINDOWS_DIR:-}" ]; then
    WINDOWS_RUN="$(gh run list ${REPO:+--repo "$REPO"} --commit "$HEAD" --workflow ci.yml \
      --status success --json databaseId,event \
      --jq '[.[] | select(.event == "push" or .event == "workflow_dispatch")][0].databaseId // empty')" || return 1
    if [ -z "$WINDOWS_RUN" ]; then
      echo "no successful CI run of $HEAD yet: let CI finish on main, or run it on" \
        "this commit (gh workflow run ci.yml --ref <branch>)" >&2
      return 1
    fi
  fi
  if [ -n "${WINDOWS_RUN:-}" ]; then
    run="$(gh run view "$WINDOWS_RUN" ${REPO:+--repo "$REPO"} --json headSha,conclusion,event \
      --jq '.headSha + " " + .conclusion + " " + .event')" || return 1
    if [ "$run" != "$HEAD success push" ] && [ "$run" != "$HEAD success workflow_dispatch" ]; then
      echo "CI run $WINDOWS_RUN is \"$run\", not a successful push or manual run of $HEAD" >&2
      return 1
    fi
    # One directory per run, completed before it is used.
    dir="$OUT/.windows-$WINDOWS_RUN"
    if [ ! -d "$dir" ]; then
      rm -rf "$dir.partial"
      gh run download "$WINDOWS_RUN" ${REPO:+--repo "$REPO"} --name flark_parse-windows --dir "$dir.partial" || return 1
      mv "$dir.partial" "$dir"
    fi
  elif [ -n "${WINDOWS_DIR:-}" ]; then
    echo "warning: WINDOWS_DIR is not tied to a commit; a release uses WINDOWS_RUN" >&2
    dir="$WINDOWS_DIR"
  fi
  if [ -z "${dir:-}" ] || [ ! -f "$dir/$1/flark_parse.dll" ]; then
    echo "no Windows library for $1 in CI run ${WINDOWS_RUN:-} of $HEAD" >&2
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
