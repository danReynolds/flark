#!/bin/bash
# M1 exit check: a fresh Dart app depending on flark builds and parses with
# no Rust toolchain on PATH, using a prebuilt library named by the consumer's
# pubspec user-defines (the hook runner sanitizes environment variables, so
# that is the supported channel besides a prebuilt bundled in the package).
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
CRATE="$(cd "$PKG/../../native/flark_parse" && pwd)"
TRIPLE="$( (rustc -vV 2>/dev/null || true) | sed -n 's/^host: //p')"
if [ -z "$TRIPLE" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) TRIPLE=aarch64-apple-darwin;; Darwin-x86_64) TRIPLE=x86_64-apple-darwin;;
    Linux-x86_64) TRIPLE=x86_64-unknown-linux-gnu;; Linux-aarch64) TRIPLE=aarch64-unknown-linux-gnu;;
    *) echo "unknown host"; exit 1;;
  esac
fi
case "$TRIPLE" in *darwin*) LIB=libflark_parse.dylib;; *) LIB=libflark_parse.so;; esac
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
PREBUILT="$WORK/prebuilt/$TRIPLE"; mkdir -p "$PREBUILT"
if [ ! -f "$CRATE/target/release/$LIB" ]; then cargo build --release --locked --lib --manifest-path "$CRATE/Cargo.toml" || exit 1; fi
cp "$CRATE/target/release/$LIB" "$PREBUILT/$LIB"
APP="$WORK/consumer"; mkdir -p "$APP/bin"
cat > "$APP/pubspec.yaml" <<PUB
name: consumer
publish_to: none
environment:
  sdk: ^3.10.4
dependencies:
  flark:
    path: $PKG
hooks:
  user_defines:
    flark:
      prebuilt_dir: $WORK/prebuilt
PUB
cat > "$APP/bin/main.dart" <<'DART'
import 'package:flark/flark.dart';
void main() {
  final backend = createParseBackend();
  final empty = backend.parse('');
  final m = backend.parse('hello *there* **friend**\n\n- item');
  print('empty=${empty.blockCount} blocks=${m.blockCount} runs=${m.runCount} emph=${m.runAt(1).kind == RunKind.emph}');
}
DART
# A minimal PATH: the Dart SDK's directory plus the system directories. This
# is more robust than filtering the caller's PATH (runners reach cargo through
# entries that do not hold the binary themselves).
DART_DIR="$(dirname "$(command -v dart)")"
CLEAN_PATH="$DART_DIR:/usr/bin:/bin:/usr/sbin:/sbin"
cd "$APP" || exit 1
PATH="$CLEAN_PATH" dart pub get >/dev/null || { echo "pub get failed"; exit 1; }
if FOUND="$(PATH="$CLEAN_PATH" command -v cargo 2>/dev/null)"; then echo "cargo still on PATH at $FOUND (PATH=$CLEAN_PATH)"; exit 1; fi
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"; STATUS=$?
echo "$OUT" | grep -vE '^\s*$' | tail -12
if [ $STATUS -eq 0 ] && echo "$OUT" | grep -q 'empty=1 blocks=5 runs=7 emph=true'; then
  echo "prebuilt consumer OK (no Rust on PATH)"
else
  echo "prebuilt consumer FAILED (status $STATUS)"; exit 1
fi
