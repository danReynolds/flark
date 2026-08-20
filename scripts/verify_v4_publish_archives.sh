#!/usr/bin/env bash
set -euo pipefail
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# Build the exact active package archives, inspect their contents, and consume
# extracted copies outside the checkout. Path dependencies point only at those
# extracted archives; no consumer can fall back to repository source.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CORE_PACKAGE_ROOT="$REPO_ROOT/packages/flark_core"
FLUTTER_PACKAGE_ROOT="$REPO_ROOT/packages/flark"
DART_BIN="${DART_BIN:-dart}"
FLUTTER_BIN="${FLUTTER_BIN:-flutter}"

run_runtime=1

usage() {
  cat <<'EOF'
Build and consume the active flark_core and flark pub archives.

Usage:
  bash scripts/verify_v4_publish_archives.sh [--skip-runtime]

--skip-runtime still builds, extracts, inspects, resolves, and analyzes both
archives. It skips Core JIT/executed-AOT and Flutter widget/native-assets
runtime smoke evidence. The desktop consumer is build-only in either mode.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-runtime)
      run_runtime=0
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

for command in "$DART_BIN" "$FLUTTER_BIN" tar rg awk; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Missing required command: $command" >&2
    exit 1
  fi
done

WORKSPACE="$(mktemp -d "${TMPDIR:-/tmp}/flark-v4-publish-archives.XXXXXX")"
cleanup() {
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
CORE_ARCHIVE="$ARCHIVE_DIR/flark_core.tar.gz"
FLUTTER_ARCHIVE="$ARCHIVE_DIR/flark.tar.gz"

mkdir -p "$ARCHIVE_DIR" "$PACKAGE_DIR/flark_core" \
  "$PACKAGE_DIR/flark" "$CONSUMER_DIR" "$LOG_DIR"

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

export DART_SUPPRESS_ANALYTICS=true
export CI=true

# Both packages are nested inside one repository. Stop pub's Git discovery at
# packages/ so the root repository's workspace layout cannot make a nested
# archive appear empty.
PUBLISH_ENV=(
  env
  DART_SUPPRESS_ANALYTICS=true
  GIT_CEILING_DIRECTORIES="$REPO_ROOT/packages"
)

run_logged \
  "Create the exact flark_core pub archive" \
  "$LOG_DIR/flark-core-publish.log" \
  "${PUBLISH_ENV[@]}" "$DART_BIN" pub publish --skip-validation \
  --to-archive="$CORE_ARCHIVE" -C "$CORE_PACKAGE_ROOT"

run_logged \
  "Create the exact flark pub archive" \
  "$LOG_DIR/flark-publish.log" \
  "${PUBLISH_ENV[@]}" "$DART_BIN" pub publish --skip-validation \
  --to-archive="$FLUTTER_ARCHIVE" -C "$FLUTTER_PACKAGE_ROOT"

tar -tzf "$CORE_ARCHIVE" >"$WORKSPACE/flark-core-archive.list"
tar -tzf "$FLUTTER_ARCHIVE" >"$WORKSPACE/flark-archive.list"

assert_archive_entry "$WORKSPACE/flark-core-archive.list" pubspec.yaml
assert_archive_entry "$WORKSPACE/flark-core-archive.list" lib/flark_core.dart
assert_archive_entry "$WORKSPACE/flark-core-archive.list" hook/build.dart
assert_archive_entry \
  "$WORKSPACE/flark-core-archive.list" native/comrak_bridge/Cargo.toml
assert_archive_entry \
  "$WORKSPACE/flark-core-archive.list" native/comrak_bridge/Cargo.lock
assert_archive_entry "$WORKSPACE/flark-archive.list" pubspec.yaml
assert_archive_entry "$WORKSPACE/flark-archive.list" lib/flark.dart

for listing in "$WORKSPACE/flark-core-archive.list" \
  "$WORKSPACE/flark-archive.list"; do
  assert_archive_excludes "$listing" '(^/|(^|/)\.\.(/|$))' \
    'absolute or parent-traversing path'
  assert_archive_excludes "$listing" '(^|/)\.dart_tool(/|$)' \
    '.dart_tool state'
  assert_archive_excludes "$listing" '(^|/)pubspec_overrides\.yaml$' \
    'pubspec override'
  assert_archive_excludes "$listing" '(^|/)target(/|$)' \
    'native build output'
  assert_archive_excludes "$listing" '(^|/)test(/|$)' \
    'package test source'
done

tar -xzf "$CORE_ARCHIVE" -C "$PACKAGE_DIR/flark_core"
tar -xzf "$FLUTTER_ARCHIVE" -C "$PACKAGE_DIR/flark"

if find "$PACKAGE_DIR" -type l -print -quit | rg -q .; then
  echo "Publish archives must not contain symbolic links." >&2
  exit 1
fi
assert_no_checkout_reference "$PACKAGE_DIR"
chmod -R a-w "$PACKAGE_DIR"

mkdir -p "$CONSUMER_DIR/dart/bin"
cat >"$CONSUMER_DIR/dart/pubspec.yaml" <<EOF
name: flark_core_archive_consumer
description: External consumer for the extracted flark_core archive.
publish_to: none
version: 0.0.0
environment:
  sdk: ^3.10.4
dependencies:
  flark_core:
    path: $PACKAGE_DIR/flark_core
EOF
cat >"$CONSUMER_DIR/dart/bin/main.dart" <<'EOF'
import 'package:flark_core/flark_core.dart';

Future<void> main() async {
  final document = await FlarkCoreDocument.open('# Archive consumer\n');
  try {
    final receipt = await document.applyEditUtf16(0, 0, '> ');
    if (receipt.revision != 2 ||
        await document.readSource() != '> # Archive consumer\n') {
      throw StateError('Archive-backed flark_core did not edit exact source.');
    }
  } finally {
    await document.dispose();
  }
}

// Public return types must remain nameable from the supported barrel.
void acceptTransactionReceipt(FlarkCoreSourceTransactionReceiptV1 receipt) {}
void acceptNativeReceipt(FlarkNativeEditReceipt receipt) {}
EOF

run_logged "Resolve the extracted flark_core archive" \
  "$LOG_DIR/dart-pub-get.log" \
  "$DART_BIN" pub get -C "$CONSUMER_DIR/dart"
(
  cd "$CONSUMER_DIR/dart"
  "$DART_BIN" analyze
  if [ "$run_runtime" -eq 1 ]; then
    "$DART_BIN" run bin/main.dart
    case "$(uname -s)" in
      Darwin|Linux)
        "$DART_BIN" build cli --target=bin/main.dart \
          --output="$WORKSPACE/native-aot"
        aot_executable="$WORKSPACE/native-aot/bundle/bin/main"
        if [ ! -x "$aot_executable" ]; then
          echo "Missing executable Dart AOT artifact: $aot_executable" >&2
          exit 1
        fi
        "$aot_executable"
        ;;
    esac
  fi
)
assert_no_checkout_reference "$CONSUMER_DIR/dart/.dart_tool/package_config.json"

run_logged "Create a disposable Flutter archive consumer" \
  "$LOG_DIR/flutter-create.log" \
  "$FLUTTER_BIN" create --no-pub --platforms=macos,linux \
  --project-name flark_archive_consumer "$CONSUMER_DIR/flutter"

cat >"$CONSUMER_DIR/flutter/pubspec.yaml" <<EOF
name: flark_archive_consumer
description: External consumer for the extracted flark and flark_core archives.
publish_to: none
version: 0.0.0+1
environment:
  sdk: ^3.10.4
dependencies:
  flark:
    path: $PACKAGE_DIR/flark
  flutter:
    sdk: flutter
dependency_overrides:
  # flark's published constraint normally resolves flark_core from its hosted
  # archive. This offline verifier binds that transitive identity to the
  # separately extracted core archive; the archives themselves are still
  # required to exclude overrides and path dependencies.
  flark_core:
    path: $PACKAGE_DIR/flark_core
dev_dependencies:
  flutter_test:
    sdk: flutter
  flutter_lints: ^6.0.0
flutter:
  uses-material-design: true
EOF
cat >"$CONSUMER_DIR/flutter/lib/main.dart" <<'EOF'
import 'package:flark/flark.dart';
import 'package:flutter/material.dart';

void main() => runApp(const MaterialApp(home: Text('Flark archive consumer')));

// The Flutter barrel re-exports Core types used by its public controller API.
void acceptRows(List<FlarkViewportRow> rows) {}
EOF
cat >"$CONSUMER_DIR/flutter/test/widget_test.dart" <<'EOF'
import 'package:flark/flark.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('archive-backed Flutter package opens through bundled Core', () async {
    final controller = await FlarkEditorController.open('# Archive consumer\n');
    try {
      expect(await controller.readSource(), '# Archive consumer\n');
    } finally {
      await controller.close();
    }
  });
}
EOF

run_logged "Resolve both extracted archives in Flutter" \
  "$LOG_DIR/flutter-pub-get.log" \
  "$FLUTTER_BIN" pub get -C "$CONSUMER_DIR/flutter"
(
  cd "$CONSUMER_DIR/flutter"
  "$FLUTTER_BIN" analyze
  if [ "$run_runtime" -eq 1 ]; then
    "$FLUTTER_BIN" test test/widget_test.dart --concurrency=1
    case "$(uname -s)" in
      Darwin) "$FLUTTER_BIN" build macos --debug --no-pub ;;
      Linux) "$FLUTTER_BIN" build linux --debug --no-pub ;;
    esac
  fi
)
assert_no_checkout_reference \
  "$CONSUMER_DIR/flutter/.dart_tool/package_config.json"

echo
echo "Active flark_core and flark archives passed external-consumer verification."
if [ "$run_runtime" -eq 1 ]; then
  echo "Core JIT/executed-AOT and Flutter widget/native-assets runtime smokes passed."
fi
echo "Desktop application evidence is build-only; no packaged-app launch is claimed."
