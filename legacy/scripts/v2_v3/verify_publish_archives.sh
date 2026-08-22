#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FLUTTER_PACKAGE_ROOT="$REPO_ROOT/packages/flark_flutter"
FIXTURE_ROOT="$REPO_ROOT/tool/archive_consumer"
DART_BIN="${DART_BIN:-dart}"
FLUTTER_BIN="${FLUTTER_BIN:-flutter}"

run_native_aot=1
run_browser_runtime=1

usage() {
  cat <<'EOF'
Build and consume the exact Flark pub archives outside the repository.

Usage:
  ./scripts/verify_publish_archives.sh [options]

Options:
  --skip-native-aot       Skip the host-native AOT relocation receipt.
  --skip-browser-runtime  Skip the real Chrome Worker/Wasm runtime receipt.
  -h, --help              Show this help.

The Dart-only source receipt and Flutter Web build/asset inspection always
run. Skip flags are intended only for platform-limited lanes; the release gate
runs every receipt.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-native-aot)
      run_native_aot=0
      ;;
    --skip-browser-runtime)
      run_browser_runtime=0
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
  shift
done

for command in "$DART_BIN" "$FLUTTER_BIN" tar rg cmp diff awk; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Missing required command: $command" >&2
    exit 1
  fi
done

WORKSPACE="$(mktemp -d "${TMPDIR:-/tmp}/flark-publish-archives.XXXXXX")"
HOSTED_SERVER_PID=''
cleanup() {
  if [ -n "$HOSTED_SERVER_PID" ]; then
    kill "$HOSTED_SERVER_PID" 2>/dev/null || true
    wait "$HOSTED_SERVER_PID" 2>/dev/null || true
  fi
  if [ "${FLARK_KEEP_ARCHIVE_WORKSPACE:-0}" = "1" ]; then
    echo "Retained archive verification workspace: $WORKSPACE"
    return
  fi
  chmod -R u+w "$WORKSPACE" 2>/dev/null || true
  rm -rf "$WORKSPACE"
}
trap cleanup EXIT

ARCHIVE_DIR="$WORKSPACE/archives"
PACKAGE_DIR="$WORKSPACE/packages"
CONSUMER_DIR="$WORKSPACE/consumers"
LOG_DIR="$WORKSPACE/logs"
ROOT_ARCHIVE="$ARCHIVE_DIR/flark.tar.gz"
FLUTTER_ARCHIVE="$ARCHIVE_DIR/flark_flutter.tar.gz"

mkdir -p "$ARCHIVE_DIR" "$PACKAGE_DIR/flark" \
  "$PACKAGE_DIR/flark_flutter" "$CONSUMER_DIR" "$LOG_DIR"

run_logged() {
  local label="$1"
  local log="$2"
  shift 2
  echo
  echo "==> $label"
  if ! "$@" >"$log" 2>&1; then
    cat "$log" >&2
    return 1
  fi
}

assert_archive_entry() {
  local listing="$1"
  local entry="$2"
  if ! rg -Fxq "$entry" "$listing"; then
    echo "Publish archive is missing required entry: $entry" >&2
    exit 1
  fi
}

assert_archive_excludes() {
  local listing="$1"
  local pattern="$2"
  local description="$3"
  if rg -n "$pattern" "$listing"; then
    echo "Publish archive contains forbidden $description." >&2
    exit 1
  fi
}

assert_no_checkout_reference() {
  local target="$1"
  if rg -a -l -F "$REPO_ROOT" "$target"; then
    echo "External consumer output references the source checkout: $target" >&2
    exit 1
  fi
}

assert_no_pubspec_path_overrides() {
  local pubspec="$1"
  if rg -n '^dependency_overrides:' "$pubspec"; then
    echo "Published pubspec contains dependency_overrides: $pubspec" >&2
    exit 1
  fi
  if awk '
    /^[[:alnum:]_]+:/ {
      section = $1
      sub(/:.*/, "", section)
    }
    (section == "dependencies" || section == "dev_dependencies") &&
      /^[[:space:]]+path:/ {
      found = 1
    }
    END { exit found ? 0 : 1 }
  ' "$pubspec"; then
    echo "Published pubspec contains a path dependency: $pubspec" >&2
    exit 1
  fi
}

archive_sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
    return
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
    return
  fi
  echo 'A SHA-256 utility (shasum or sha256sum) is required.' >&2
  exit 1
}

export DART_SUPPRESS_ANALYTICS=true
export CI=true

publish_warning_args=()
if [ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
  # Pub's only expected warning during local implementation is the dirty Git
  # checkout. A clean release checkout remains warning-strict, preserving the
  # old dry-run gate's behavior.
  publish_warning_args+=(--ignore-warnings)
  echo '==> Dirty checkout: publish warnings are not fatal in this local run.'
fi

run_logged \
  "Validate the root flark publish contract" \
  "$LOG_DIR/root-publish-validation.log" \
  "$DART_BIN" pub publish --dry-run "${publish_warning_args[@]}" \
  -C "$REPO_ROOT"

run_logged \
  "Validate the flark_flutter publish contract" \
  "$LOG_DIR/flutter-publish-validation.log" \
  env GIT_CEILING_DIRECTORIES="$REPO_ROOT/packages" \
  "$FLUTTER_BIN" pub publish --dry-run "${publish_warning_args[@]}" \
  -C "$FLUTTER_PACKAGE_ROOT"

run_logged \
  "Create the exact root flark pub archive" \
  "$LOG_DIR/root-publish.log" \
  "$DART_BIN" pub publish --skip-validation \
  --to-archive="$ROOT_ARCHIVE" \
  -C "$REPO_ROOT"

# The adapter is a nested package excluded by the root archive. Prevent pub's
# Git discovery from inheriting that ancestor exclusion while it enumerates
# the adapter's own .pubignore.
run_logged \
  "Create the exact flark_flutter pub archive" \
  "$LOG_DIR/flutter-publish.log" \
  env GIT_CEILING_DIRECTORIES="$REPO_ROOT/packages" \
  "$FLUTTER_BIN" pub publish --skip-validation \
  --to-archive="$FLUTTER_ARCHIVE" \
  -C "$FLUTTER_PACKAGE_ROOT"

tar -tzf "$ROOT_ARCHIVE" >"$WORKSPACE/root-archive.list"
tar -tzf "$FLUTTER_ARCHIVE" >"$WORKSPACE/flutter-archive.list"

assert_archive_entry "$WORKSPACE/root-archive.list" pubspec.yaml
assert_archive_entry "$WORKSPACE/root-archive.list" lib/flark_v3.dart
assert_archive_entry "$WORKSPACE/root-archive.list" hook/build.dart
assert_archive_entry \
  "$WORKSPACE/root-archive.list" \
  native/comrak_bridge/Cargo.toml
assert_archive_entry \
  "$WORKSPACE/root-archive.list" \
  lib/assets/wasm/flark_comrak_bridge.wasm
assert_archive_entry \
  "$WORKSPACE/root-archive.list" \
  lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo
assert_archive_entry \
  "$WORKSPACE/root-archive.list" \
  lib/assets/worker/flark_v3_parser_worker.js

assert_archive_entry "$WORKSPACE/flutter-archive.list" pubspec.yaml
assert_archive_entry \
  "$WORKSPACE/flutter-archive.list" \
  lib/flark_flutter.dart
assert_archive_entry \
  "$WORKSPACE/flutter-archive.list" \
  lib/assets/wasm/flark_comrak_bridge.wasm
assert_archive_entry \
  "$WORKSPACE/flutter-archive.list" \
  lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo
assert_archive_entry \
  "$WORKSPACE/flutter-archive.list" \
  lib/assets/worker/flark_v3_parser_worker.js

for listing in "$WORKSPACE/root-archive.list" \
  "$WORKSPACE/flutter-archive.list"; do
  assert_archive_excludes \
    "$listing" \
    '(^/|(^|/)\.\.(/|$))' \
    'absolute or parent-traversing path'
  assert_archive_excludes \
    "$listing" \
    '(^|/)\.dart_tool(/|$)' \
    '.dart_tool state'
  assert_archive_excludes \
    "$listing" \
    '(^|/)pubspec_overrides\.yaml$' \
    'pubspec override'
  assert_archive_excludes \
    "$listing" \
    '(^|/)target(/|$)' \
    'native build output'
done
assert_archive_excludes \
  "$WORKSPACE/root-archive.list" \
  '^packages(/|$)' \
  'repository workspace package'
assert_archive_excludes \
  "$WORKSPACE/root-archive.list" \
  '^tool/archive_consumer(/|$)' \
  'archive-verification fixture'

tar -xzf "$ROOT_ARCHIVE" -C "$PACKAGE_DIR/flark"
tar -xzf "$FLUTTER_ARCHIVE" -C "$PACKAGE_DIR/flark_flutter"

if find "$PACKAGE_DIR" -type l -print -quit | rg -q .; then
  echo 'Publish archives must not contain symbolic links.' >&2
  exit 1
fi
assert_no_pubspec_path_overrides "$PACKAGE_DIR/flark/pubspec.yaml"
assert_no_pubspec_path_overrides "$PACKAGE_DIR/flark_flutter/pubspec.yaml"
assert_no_checkout_reference "$PACKAGE_DIR"

cmp \
  "$PACKAGE_DIR/flark/lib/assets/wasm/flark_comrak_bridge.wasm" \
  "$PACKAGE_DIR/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm"
cmp \
  "$PACKAGE_DIR/flark/lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo" \
  "$PACKAGE_DIR/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm.buildinfo"
cmp \
  "$PACKAGE_DIR/flark/lib/assets/worker/flark_v3_parser_worker.js" \
  "$PACKAGE_DIR/flark_flutter/lib/assets/worker/flark_v3_parser_worker.js"

# Keep the independently extracted reference packages immutable. Consumers
# resolve separate copies through the loopback hosted-package protocol below.
chmod -R a-w "$PACKAGE_DIR"

ROOT_SHA256="$(archive_sha256 "$ROOT_ARCHIVE")"
FLUTTER_SHA256="$(archive_sha256 "$FLUTTER_ARCHIVE")"
HOSTED_URL_FILE="$WORKSPACE/hosted-url"
"$DART_BIN" "$FIXTURE_ROOT/hosted_server.dart" \
  "$ROOT_ARCHIVE" "$ROOT_SHA256" "$PACKAGE_DIR/flark/pubspec.yaml" \
  "$FLUTTER_ARCHIVE" "$FLUTTER_SHA256" \
  "$PACKAGE_DIR/flark_flutter/pubspec.yaml" \
  >"$HOSTED_URL_FILE" 2>"$LOG_DIR/hosted-server.log" &
HOSTED_SERVER_PID=$!
for _ in {1..100}; do
  if [ -s "$HOSTED_URL_FILE" ]; then break; fi
  if ! kill -0 "$HOSTED_SERVER_PID" 2>/dev/null; then
    cat "$LOG_DIR/hosted-server.log" >&2
    echo 'Archive hosted-package server exited before startup.' >&2
    exit 1
  fi
  sleep 0.05
done
if [ ! -s "$HOSTED_URL_FILE" ]; then
  cat "$LOG_DIR/hosted-server.log" >&2
  echo 'Timed out starting the archive hosted-package server.' >&2
  exit 1
fi
export PUB_HOSTED_URL
PUB_HOSTED_URL="$(head -n 1 "$HOSTED_URL_FILE")"
export PUB_CACHE="$WORKSPACE/pub-cache"

cp -R "$FIXTURE_ROOT/dart_consumer" "$CONSUMER_DIR/dart"
mkdir -p "$CONSUMER_DIR/dart/tool" "$WORKSPACE/dart-web"
cp "$FIXTURE_ROOT/verify_package_config.dart" \
  "$CONSUMER_DIR/dart/tool/verify_package_config.dart"

run_logged \
  "Resolve the Dart-only consumer from the extracted root archive" \
  "$LOG_DIR/dart-pub-get.log" \
  "$DART_BIN" pub get -C "$CONSUMER_DIR/dart"
(
  cd "$CONSUMER_DIR/dart"
  "$DART_BIN" analyze
  "$DART_BIN" run bin/source_smoke.dart
  "$DART_BIN" compile js -o "$WORKSPACE/dart-web/source_smoke.js" \
    bin/source_smoke.dart
)
ROOT_VERSION="$(awk '$1 == "version:" { print $2; exit }' \
  "$PACKAGE_DIR/flark/pubspec.yaml")"
ROOT_CACHE="$(find "$PUB_CACHE/hosted" -type d \
  -name "flark-$ROOT_VERSION" -print -quit)"
if [ -z "$ROOT_CACHE" ]; then
  echo "The hosted cache does not contain flark $ROOT_VERSION." >&2
  exit 1
fi
diff -qr "$PACKAGE_DIR/flark" "$ROOT_CACHE"
(
  cd "$CONSUMER_DIR/dart"
  "$DART_BIN" run tool/verify_package_config.dart flark "$ROOT_CACHE"
)
# A pub cache is immutable package input. Prove the native hook writes its
# Cargo and code-asset products under the consumer even when that is enforced.
chmod -R a-w "$ROOT_CACHE"
assert_no_checkout_reference "$CONSUMER_DIR/dart/.dart_tool/package_config.json"
assert_no_checkout_reference "$WORKSPACE/dart-web"

case "$(uname -s)" in
  Darwin|Linux)
    if [ "$run_native_aot" -eq 1 ]; then
      echo
      echo '==> Build and relocate the archive-backed native AOT bundle'
      (
        cd "$CONSUMER_DIR/dart"
        "$DART_BIN" build cli \
          --target=bin/native_smoke.dart \
          --output="$WORKSPACE/native-aot"
      )
      native_bundle="$WORKSPACE/native-aot/bundle"
      case "$(uname -s)" in
        Darwin) native_library=libflark_comrak_bridge.dylib ;;
        Linux) native_library=libflark_comrak_bridge.so ;;
      esac
      if [ ! -f "$native_bundle/lib/$native_library" ]; then
        echo "AOT bundle is missing lib/$native_library." >&2
        exit 1
      fi
      mkdir -p "$WORKSPACE/relocated" "$WORKSPACE/unrelated-cwd"
      cp -R "$native_bundle" "$WORKSPACE/relocated/flark-consumer"
      assert_no_checkout_reference "$WORKSPACE/native-aot"
      (
        cd "$WORKSPACE/unrelated-cwd"
        env -u DYLD_LIBRARY_PATH -u LD_LIBRARY_PATH \
          "$WORKSPACE/relocated/flark-consumer/bin/native_smoke"
      )
    else
      echo
      echo '==> Skipping native AOT relocation by request.'
    fi
    ;;
  *)
    echo
    echo "==> Native AOT relocation is not supported by this Bash lane on $(uname -s)."
    ;;
esac

cp -R "$FIXTURE_ROOT/flutter_consumer" "$CONSUMER_DIR/flutter"
mkdir -p "$CONSUMER_DIR/flutter/tool"
cp "$FIXTURE_ROOT/verify_package_config.dart" \
  "$CONSUMER_DIR/flutter/tool/verify_package_config.dart"

run_logged \
  "Resolve the Flutter consumer from both extracted archives" \
  "$LOG_DIR/flutter-pub-get.log" \
  "$FLUTTER_BIN" pub get -C "$CONSUMER_DIR/flutter"
(
  cd "$CONSUMER_DIR/flutter"
  "$FLUTTER_BIN" analyze
  "$FLUTTER_BIN" build web --release --no-pub
)
FLUTTER_VERSION="$(awk '$1 == "version:" { print $2; exit }' \
  "$PACKAGE_DIR/flark_flutter/pubspec.yaml")"
FLUTTER_CACHE="$(find "$PUB_CACHE/hosted" -type d \
  -name "flark_flutter-$FLUTTER_VERSION" -print -quit)"
if [ -z "$FLUTTER_CACHE" ]; then
  echo "The hosted cache does not contain flark_flutter $FLUTTER_VERSION." >&2
  exit 1
fi
diff -qr "$PACKAGE_DIR/flark_flutter" "$FLUTTER_CACHE"
(
  cd "$CONSUMER_DIR/flutter"
  "$DART_BIN" run tool/verify_package_config.dart \
    flark "$ROOT_CACHE" \
    flark_flutter "$FLUTTER_CACHE"
)
chmod -R a-w "$FLUTTER_CACHE"
assert_no_checkout_reference \
  "$CONSUMER_DIR/flutter/.dart_tool/package_config.json"

FLUTTER_BUILD="$CONSUMER_DIR/flutter/build/web"
FLUTTER_ASSET_ROOT="$FLUTTER_BUILD/assets/packages/flark_flutter/lib/assets"
cmp \
  "$PACKAGE_DIR/flark_flutter/lib/assets/wasm/flark_comrak_bridge.wasm" \
  "$FLUTTER_ASSET_ROOT/wasm/flark_comrak_bridge.wasm"
cmp \
  "$PACKAGE_DIR/flark_flutter/lib/assets/worker/flark_v3_parser_worker.js" \
  "$FLUTTER_ASSET_ROOT/worker/flark_v3_parser_worker.js"
rg -q \
  'assets/packages/flark_flutter/lib/assets/wasm/flark_comrak_bridge\.wasm' \
  "$FLUTTER_BUILD/main.dart.js"
rg -q \
  'assets/packages/flark_flutter/lib/assets/worker/flark_v3_parser_worker\.js' \
  "$FLUTTER_BUILD/main.dart.js"
assert_no_checkout_reference "$FLUTTER_BUILD"

if [ "$run_browser_runtime" -eq 1 ]; then
  echo
  echo '==> Boot archive-backed Worker and Wasm assets in Chrome'
  (
    cd "$CONSUMER_DIR/flutter"
    "$FLUTTER_BIN" test --platform chrome \
      test/web_runtime_test.dart --reporter compact
  )
else
  echo
  echo '==> Skipping Chrome Worker/Wasm runtime by request.'
fi

echo
echo 'Flark publish archives passed external-consumer verification.'
