#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLE_DIR="$REPO_ROOT/example"

run_android=0
run_ios=0
run_xcode_list=1

usage() {
  cat <<'EOF'
Verify the Flark example app native packaging harness.

Usage:
  ./scripts/verify_example_packaging.sh [options]

Options:
  --android             Build the example debug APK and inspect packaged JNI libs.
  --ios                 Verify the iOS example uses native-assets packaging.
  --all                 Run Android and iOS checks.
  --skip-xcode-list     Skip the xcodebuild project-parse check.
  -h, --help            Show this help.

When no platform is selected, --all is used.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --android)
      run_android=1
      ;;
    --ios)
      run_ios=1
      ;;
    --all)
      run_android=1
      run_ios=1
      ;;
    --skip-xcode-list)
      run_xcode_list=0
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

if [ "$run_android" -eq 0 ] && [ "$run_ios" -eq 0 ]; then
  run_android=1
  run_ios=1
fi

run() {
  echo
  echo "==> $*"
  "$@"
}

require_file() {
  if [ ! -f "$1" ]; then
    echo "Missing required file: $1"
    exit 1
  fi
}

require_dir() {
  if [ ! -d "$1" ]; then
    echo "Missing required directory: $1"
    exit 1
  fi
}

if [ "$run_android" -eq 1 ]; then
  require_dir "$EXAMPLE_DIR/android"
  echo
  echo "==> (cd example && flutter pub get)"
  (
    cd "$EXAMPLE_DIR"
    flutter pub get
  )
  echo
  echo "==> (cd example/android && ./gradlew :app:verifyFlarkComrakNativeLibs)"
  (
    cd "$EXAMPLE_DIR/android"
    ./gradlew :app:verifyFlarkComrakNativeLibs
  )
fi

if [ "$run_ios" -eq 1 ]; then
  project="$EXAMPLE_DIR/ios/Runner.xcodeproj/project.pbxproj"
  workspace="$EXAMPLE_DIR/ios/Runner.xcworkspace"
  hook="$REPO_ROOT/hook/build.dart"

  require_file "$project"
  require_dir "$workspace"

  # iOS ships through native assets: flark's build hook compiles the bridge and
  # Flutter bundles it as flark_comrak_bridge.framework at build time. There is
  # no per-app anchor or XCFramework link to verify statically, so instead
  # assert the old manual wiring is gone (a stale re-link would shadow the hook)
  # and that the hook declares the iOS build.
  if grep -q "FlarkComrakAnchor.c in Sources" "$project"; then
    echo "iOS project still builds the removed FlarkComrakAnchor.c."
    exit 1
  fi
  if grep -q "flark_comrak_bridge.xcframework in Frameworks" "$project"; then
    echo "iOS project still links the removed flark_comrak_bridge.xcframework."
    exit 1
  fi
  if ! grep -q "aarch64-apple-ios" "$hook"; then
    echo "Hook does not declare the iOS native-assets build."
    exit 1
  fi
  echo
  echo "==> iOS example uses native assets (no manual XCFramework/anchor wiring)."

  if [ "$run_xcode_list" -eq 1 ]; then
    run xcodebuild -list -workspace "$workspace"
  fi
fi

echo
echo "Flark example packaging harness checks passed."
