#!/bin/bash
# A copy of this package outside the repository has no crate beside it, as the
# published package does not, so its build hook downloads the parser library
# named in hook/prebuilt.json and checks its SHA-256. This serves a freshly
# built library from a local HTTP server and checks that a consumer with no
# Rust toolchain on PATH parses through it, that a second build uses the cache,
# that a library whose hash does not match is refused, that an error response
# fails the build at once even when its body stalls, and that failed downloads
# leave nothing in the cache.
#
# With --pinned it uses the committed manifest instead: a consumer downloads
# this machine's library from the published release. Run it after pinning.
set -uo pipefail
PINNED=0
[ "${1:-}" = "--pinned" ] && PINNED=1
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

if [ "$PINNED" = 0 ]; then
  cargo build --release --locked --lib --manifest-path "$CRATE/Cargo.toml" || exit 1
  mkdir -p "$WORK/served"
  ASSET="$TRIPLE-$LIB"
  cp "$CRATE/target/release/$LIB" "$WORK/served/$ASSET"
  HASH="$(shasum -a 256 "$WORK/served/$ASSET" | cut -d' ' -f1)"

  PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
  python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$WORK/served" >/dev/null 2>&1 &
  SERVER=$!
  for _ in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done
fi

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

[ "$PINNED" = 0 ] && manifest "$HASH"
PATH="$CLEAN_PATH" dart pub get >/dev/null || { echo "pub get failed"; exit 1; }
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"
if ! echo "$OUT" | grep -q 'blocks=5 runs=7 emph=true'; then
  echo "$OUT" | head -20; echo "download consumer FAILED"; exit 1
fi
echo "downloaded, verified and parsed (no Rust on PATH)"
if [ "$PINNED" = 1 ]; then
  echo "pinned download consumer OK ($(sed -n 's/.*"release": "\(.*\)".*/\1/p' "$COPY/hook/prebuilt.json"))"
  exit 0
fi

# A second build is served from the hook's cache: stop the server, then
# delete the hook's recorded output so the runner has to run it again.
stop_server
shopt -s nullglob
OUTPUTS=(.dart_tool/hooks_runner/flark/*/output.json)
shopt -u nullglob
if [ ${#OUTPUTS[@]} -eq 0 ]; then echo "no recorded hook output to invalidate"; exit 1; fi
rm -f "${OUTPUTS[@]}"
OUT="$(PATH="$CLEAN_PATH" dart run bin/main.dart 2>&1)"
if ! echo "$OUT" | grep -q 'blocks=5 runs=7 emph=true'; then
  echo "$OUT" | tail -12; echo "cached build FAILED"; exit 1
fi
for output in "${OUTPUTS[@]}"; do
  [ -f "$output" ] || { echo "the hook did not run again ($output)"; exit 1; }
done
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
# Nothing of the refused download is kept: no partial file, and no directory
# for its hash.
LEFT="$(find .dart_tool/hooks_runner/shared/flark \( -name '*.partial' -o -path '*/prebuilt/0000*' \) -print 2>/dev/null)"
if [ -n "$LEFT" ]; then
  echo "$LEFT"; echo "a refused download left files in the cache"; exit 1
fi

# An error response fails the build without waiting for its body, which here
# never finishes. The run gets 50 seconds, room for compiling the hook but
# less than the hook's 60-second stall bound, and a hook that read the body
# would wait forever.
stop_server
python3 - "$PORT" >/dev/null 2>&1 <<'PY' &
import http.server, sys, time
class Stalling(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(503)
        self.send_header('Content-Length', '100000')
        self.end_headers()
        self.wfile.write(b'unavailable')
        self.wfile.flush()
        time.sleep(600)
    def log_message(self, *args):
        pass
http.server.ThreadingHTTPServer(('127.0.0.1', int(sys.argv[1])), Stalling).serve_forever()
PY
SERVER=$!
# The server is up once a request times out on its body instead of failing
# to connect.
for _ in $(seq 50); do curl -s -m 1 -o /dev/null "http://127.0.0.1:$PORT/"; [ $? -eq 28 ] && break; sleep 0.1; done
manifest "$HASH"
# Without the cached library the hook has to download again.
rm -rf .dart_tool/hooks_runner/shared/flark/build/prebuilt
PATH="$CLEAN_PATH" dart run bin/main.dart > "$WORK/stalled.log" 2>&1 &
RUN=$!
for _ in $(seq 500); do kill -0 "$RUN" 2>/dev/null || break; sleep 0.1; done
if kill -0 "$RUN" 2>/dev/null; then
  kill "$RUN"; tail -12 "$WORK/stalled.log"; echo "an error response with a stalled body hung the build"; exit 1
fi
wait "$RUN"; STATUS=$?
if [ $STATUS -eq 0 ] || ! grep -q 'failed with HTTP 503' "$WORK/stalled.log"; then
  tail -12 "$WORK/stalled.log"; echo "an error response was not reported"; exit 1
fi
if [ -n "$(find .dart_tool/hooks_runner/shared/flark/build/prebuilt -type f 2>/dev/null)" ]; then
  echo "a failed download left files in the cache"; exit 1
fi
echo "an error response fails the build at once and caches nothing"
echo "download consumer OK"
