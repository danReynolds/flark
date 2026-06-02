#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLE_DIR="$REPO_ROOT/example"

run_android=0
run_ios=0
strict_ios=0
run_xcode_list=1

usage() {
  cat <<'EOF'
Verify the Flark example app native packaging harness.

Usage:
  ./scripts/verify_example_packaging.sh [options]

Options:
  --android             Build the example debug APK and inspect packaged JNI libs.
  --ios                 Verify iOS XCFramework/link-anchor project wiring.
  --all                 Run Android and iOS checks.
  --strict-ios          Fail the iOS check when the built XCFramework is absent.
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
    --strict-ios)
      strict_ios=1
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
  anchor="$EXAMPLE_DIR/ios/Runner/FlarkComrakAnchor.c"
  project="$EXAMPLE_DIR/ios/Runner.xcodeproj/project.pbxproj"
  workspace="$EXAMPLE_DIR/ios/Runner.xcworkspace"
  xcframework="$REPO_ROOT/native/comrak_bridge/dist/ios/flark_comrak_bridge.xcframework"

  require_file "$anchor"
  require_file "$project"
  require_dir "$workspace"

  if ! grep -q "flark_comrak_bridge_version" "$anchor"; then
    echo "iOS anchor does not reference flark_comrak_bridge_version."
    exit 1
  fi
  if ! grep -q "flark_comrak_parse" "$anchor"; then
    echo "iOS anchor does not reference flark_comrak_parse."
    exit 1
  fi
  if ! grep -q "flark_comrak_response_free" "$anchor"; then
    echo "iOS anchor does not reference flark_comrak_response_free."
    exit 1
  fi
  if ! grep -q "FlarkComrakAnchor.c in Sources" "$project"; then
    echo "iOS project does not build FlarkComrakAnchor.c."
    exit 1
  fi
  if ! grep -q "flark_comrak_bridge.xcframework in Frameworks" "$project"; then
    echo "iOS project does not link flark_comrak_bridge.xcframework."
    exit 1
  fi

  if [ -d "$xcframework" ]; then
    echo
    echo "==> Found $xcframework"
  elif [ "$strict_ios" -eq 1 ]; then
    echo "Missing iOS XCFramework: $xcframework"
    echo "Run ./scripts/build_comrak_ios.sh before strict iOS packaging checks."
    exit 1
  else
    echo
    echo "==> iOS XCFramework is not built yet; run ./scripts/build_comrak_ios.sh before device packaging."
  fi

  if [ "$run_xcode_list" -eq 1 ]; then
    run xcodebuild -list -workspace "$workspace"
  fi
fi

echo
echo "Flark example packaging harness checks passed."
