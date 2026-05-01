#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATE_DIR="$PACKAGE_ROOT/native/comrak_bridge"
IOS_SCRIPT="$SCRIPT_DIR/build_comrak_ios.sh"
ANDROID_SCRIPT="$SCRIPT_DIR/build_comrak_android.sh"

if [ ! -f "$CRATE_DIR/Cargo.toml" ]; then
  echo "Could not locate sovereign comrak bridge Cargo.toml at $CRATE_DIR."
  exit 1
fi

usage() {
  cat <<'EOF'
Build sovereign comrak native artifacts with one command.

Usage:
  ./scripts/build_comrak_all.sh [options]

Options:
  --host-only      Build host desktop artifact only.
  --ios-only       Build iOS XCFramework only.
  --android-only   Build Android JNI libs only.
  --skip-host      Skip host desktop artifact build.
  --skip-ios       Skip iOS XCFramework build.
  --skip-android   Skip Android JNI libs build.
  --strict         Fail when a selected platform is skipped.
  -h, --help       Show this message.
EOF
}

run_host=1
run_ios=1
run_android=1
strict_mode=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --host-only)
      run_host=1
      run_ios=0
      run_android=0
      ;;
    --ios-only)
      run_host=0
      run_ios=1
      run_android=0
      ;;
    --android-only)
      run_host=0
      run_ios=0
      run_android=1
      ;;
    --skip-host)
      run_host=0
      ;;
    --skip-ios)
      run_ios=0
      ;;
    --skip-android)
      run_android=0
      ;;
    --strict)
      strict_mode=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
  shift
done

if [ "$run_host" -eq 0 ] && [ "$run_ios" -eq 0 ] && [ "$run_android" -eq 0 ]; then
  echo "Nothing selected to build."
  exit 0
fi

if command -v rustup >/dev/null 2>&1; then
  CARGO_CMD=(rustup run stable cargo)
else
  CARGO_CMD=(cargo)
fi

skip_count=0

build_host() {
  local host_os
  host_os="$(uname -s)"
  local target_name
  case "$host_os" in
    Darwin) target_name="macOS" ;;
    Linux) target_name="Linux" ;;
    *)
      echo "Skipping host build: unsupported host OS ($host_os)."
      return 1
      ;;
  esac

  echo "Building host bridge artifact ($target_name)..."
  "${CARGO_CMD[@]}" build --manifest-path "$CRATE_DIR/Cargo.toml" --release
}

can_build_android() {
  if [ -n "${ANDROID_NDK_HOME:-}" ]; then
    return 0
  fi
  if [ -n "${ANDROID_HOME:-}" ]; then
    local detected_ndk
    detected_ndk="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$detected_ndk" ]; then
      return 0
    fi
  fi
  return 1
}

if [ "$run_host" -eq 1 ]; then
  if ! build_host; then
    skip_count=$((skip_count + 1))
  fi
fi

if [ "$run_ios" -eq 1 ]; then
  if [ "$(uname -s)" != "Darwin" ]; then
    echo "Skipping iOS build: requires macOS host."
    skip_count=$((skip_count + 1))
  else
    bash "$IOS_SCRIPT"
  fi
fi

if [ "$run_android" -eq 1 ]; then
  if ! can_build_android; then
    echo "Skipping Android build: set ANDROID_NDK_HOME or ANDROID_HOME."
    skip_count=$((skip_count + 1))
  else
    bash "$ANDROID_SCRIPT"
  fi
fi

if [ "$strict_mode" -eq 1 ] && [ "$skip_count" -gt 0 ]; then
  echo "Build completed with skipped targets ($skip_count) and --strict enabled."
  exit 1
fi

echo "Comrak build complete."
if [ "$skip_count" -gt 0 ]; then
  echo "Skipped targets: $skip_count (rerun with --strict to fail on skips)."
fi
