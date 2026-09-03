#!/bin/bash
# M1 exit check: the render model is byte-identical on the native transport
# and on the COMMITTED wasm module for every conformance case. A stale
# committed module fails here. Pass --rebuild to also check a fresh build.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
PKG="$(cd "$CRATE/../../packages/flark" && pwd)"
CORPUS="$(cd "$CRATE/../../test/fixtures/commonmark/upstream" && pwd)"
COMMITTED="$PKG/lib/assets/wasm/flark_parse.wasm"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
if command -v rustup >/dev/null 2>&1; then
  TOOLCHAIN="$(cd "$CRATE" && rustup show active-toolchain | awk '{print $1}')"
  CARGO=(rustup run "$TOOLCHAIN" cargo)
else
  CARGO=(cargo)
fi
"${CARGO[@]}" build --release --locked --features tools --manifest-path "$CRATE/Cargo.toml" >/dev/null
"$CRATE/target/release/model_hashes" "$CORPUS" > "$TMP/native"
node "$HERE/wasm_model_hashes.mjs" "$COMMITTED" "$CORPUS" > "$TMP/committed"
if ! diff -q "$TMP/native" "$TMP/committed" >/dev/null; then
  echo "COMMITTED WASM IS STALE: render models differ from native"; diff "$TMP/native" "$TMP/committed" | head -20; exit 1
fi
echo "committed wasm identical to native: $(wc -l < "$TMP/native" | tr -d ' ') cases"
if [ "${1:-}" = "--rebuild" ]; then
  "$PKG/tool/build_wasm.sh" "$TMP/fresh.wasm" >/dev/null
  node "$HERE/wasm_model_hashes.mjs" "$TMP/fresh.wasm" "$CORPUS" > "$TMP/fresh"
  diff -q "$TMP/native" "$TMP/fresh" >/dev/null && echo "fresh wasm identical to native" || { echo "FRESH WASM DIFFERS"; exit 1; }
fi
