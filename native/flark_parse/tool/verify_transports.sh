#!/bin/bash
# M1 exit check: the render model is byte-identical on the native and wasm
# transports for every conformance case.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE="$(cd "$HERE/.." && pwd)"
CORPUS="${1:-$(cd "$CRATE/../../test/fixtures/commonmark/upstream" && pwd)}"
cargo build --release --features tools --manifest-path "$CRATE/Cargo.toml" >/dev/null
"$CRATE/../../packages/flark/tool/build_wasm.sh" >/dev/null
NATIVE="$(mktemp)"; WASM="$(mktemp)"
"$CRATE/target/release/model_hashes" "$CORPUS" > "$NATIVE"
node "$HERE/wasm_model_hashes.mjs" "$CRATE/../../packages/flark/lib/assets/wasm/flark_parse.wasm" "$CORPUS" > "$WASM"
if diff -q "$NATIVE" "$WASM" >/dev/null; then
  echo "transports identical: $(wc -l < "$NATIVE" | tr -d ' ') cases"
else
  echo "TRANSPORT MISMATCH"; diff "$NATIVE" "$WASM" | head -20; exit 1
fi
