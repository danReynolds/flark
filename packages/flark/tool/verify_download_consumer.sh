#!/bin/bash
# A copy of this package outside the repository has no crate beside it, as the
# published package does not, so its build hook downloads the parser library
# named in hook/prebuilt.json and checks its SHA-256. This serves a freshly
# built library from a local HTTP server and checks that a consumer with no
# Rust toolchain on PATH parses through it, that a second build uses the cache,
# and that a library whose hash does not match is refused.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
CRATE="$(cd "$PKG/../../native/flark_parse" && pwd)"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
case "$TRIPLE" in
  *darwin*) LIB=libflark_parse.dylib;;
  *windows*) LIB=flark_parse.dll;;
  *) LIB=libflark_parse.so;;
esac
WORK="$(mktemp -d)"
SERVER=""
stop_server() { [ -n "$SERVER" ] && { kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null; }; SERVER=""; }
trap 'stop_server; rm -rf "$WORK"' EXIT

cargo build --release --locked --lib --manifest-path "$CRATE/Cargo.toml" || exit 1
mkdir -p "$WORK/served"
ASSET="$TRIPLE-$LIB"
cp "$CRATE/target/release/$LIB" "$WORK/served/$ASSET"
HASH="$(shasum -a 256 "$WORK/served/$ASSET" | cut -d' ' -f1)"

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$WORK/served" >/dev/null 2>&1 &
SERVER=$!
for _ in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done

# The package as published: lib, hook and pubspec, no crate two levels up.
COPY="$WORK/pub/flark"
mkdir -p "$COPY"
cp -R "$PKG/lib" "$PKG/hook" "$COPY/"
grep -v '^resolution: workspace$' "$PKG/pubspec.yaml" > "$COPY/pubspec.yaml"
manifest() {
  cat > "$COPY/hook/prebuilt.json" <<JSON
{
  "release": "local",
  "files": {
    "$TRIPLE": {"url": "http://127.0.0.1:$PORT/$ASSET", "sha256": "$1"}
  }
}
JSON
}

APP="$WORK/consumer"
mkdir -p "$APP/bin"
cat > "$APP/pubspec.yaml" <<PUB
name: consumer
publish_to: none
environment:
  sdk: ^3.10.4
dependencies:
  flark:
    path: $COPY
PUB
cat > "$APP/bin/main.dart" <<'DART'
import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
void main() {
  final m = createParseBackend().parse('hello *there* **friend**\n\n- item');
  print('blocks=${m.blockCount} runs=${m.runCount} emph=${m.runAt(1).kind == RunKind.emph}');
}
DART

DART_BIN="$(command -v dart)"
while [ -L "$DART_BIN" ]; do
  LINK="$(readlink "$DART_BIN")"
  case "$LINK" in /*) DART_BIN="$LINK";; *) DART_BIN="$(dirname "$DART_BIN")/$LINK";; esac
done
CLEAN_PATH="$(cd "$(dirname "$DART_BIN")" && pwd):/usr/bin:/bin:/usr/sbin:/sbin"
cd "$APP" || exit 1
hash -r
if PATH="$CLEAN_PATH" command -v cargo >/dev/null 2>&1; then echo "cargo still on PATH"; exit 1; fi

manifest "$HASH"
PATH="$CLEAN_PATH" dart pub get >/dev/null || { echo "pub get failed"; exit 1; }
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"
if ! echo "$OUT" | grep -q 'blocks=5 runs=7 emph=true'; then
  echo "$OUT" | tail -12; echo "download consumer FAILED"; exit 1
fi
echo "downloaded, verified and parsed (no Rust on PATH)"

# A second build is served from the hook's cache: stop the server first.
stop_server
rm -rf .dart_tool/hooks_runner/flark/*/output  2>/dev/null
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"
if ! echo "$OUT" | grep -q 'blocks=5 runs=7 emph=true'; then
  echo "$OUT" | tail -12; echo "cached build FAILED"; exit 1
fi
echo "rebuilt from the cache with the server stopped"

# A library that does not match the pinned hash is refused.
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$WORK/served" >/dev/null 2>&1 &
SERVER=$!
for _ in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done
manifest "0000000000000000000000000000000000000000000000000000000000000000"
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"; STATUS=$?
if [ $STATUS -eq 0 ] || ! echo "$OUT" | grep -q 'Nothing was used'; then
  echo "$OUT" | tail -12; echo "hash mismatch was not refused"; exit 1
fi
echo "a mismatched hash is refused"
echo "download consumer OK"
