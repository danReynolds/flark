#!/bin/bash
# M1 exit check: a fresh Dart app depending on flark builds and parses with
# no Rust toolchain on PATH, using a prebuilt library the consumer names in
# its pubspec user-defines (the hook runner sanitizes environment variables,
# so that is the supported channel besides a prebuilt bundled in the package).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
CRATE="$(cd "$PKG/../../native/flark_parse" && pwd)"
TRIPLE="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"; TRIPLE="${TRIPLE:-aarch64-apple-darwin}"
case "$TRIPLE" in *darwin*) LIB=libflark_parse.dylib;; *) LIB=libflark_parse.so;; esac
WORK="$(mktemp -d)"
PREBUILT="$WORK/prebuilt/$TRIPLE"; mkdir -p "$PREBUILT"
if [ ! -f "$CRATE/target/release/$LIB" ]; then cargo build --release --lib --manifest-path "$CRATE/Cargo.toml"; fi
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
  final m = createParseBackend().parse('hello *there* **friend**\n\n- item');
  print('blocks=${m.blockCount} runs=${m.runCount} schema=${m.runAt(1).kind == RunKind.emph}');
}
DART
# Strip Rust from PATH: keep only directories without cargo/rustup/rustc.
CLEAN_PATH=""
IFS=: read -ra PARTS <<< "$PATH"
for d in "${PARTS[@]}"; do
  if [ -x "$d/cargo" ] || [ -x "$d/rustup" ] || [ -x "$d/rustc" ]; then continue; fi
  CLEAN_PATH="${CLEAN_PATH:+$CLEAN_PATH:}$d"
done
cd "$APP"
PATH="$CLEAN_PATH" dart pub get >/dev/null
if PATH="$CLEAN_PATH" command -v cargo >/dev/null 2>&1; then echo "cargo still on PATH"; exit 1; fi
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"
echo "$OUT" | grep -vE '^\s*$' | tail -12
echo "$OUT" | grep -q 'blocks=5 runs=7 schema=true' && echo "prebuilt consumer OK (no Rust on PATH)"
