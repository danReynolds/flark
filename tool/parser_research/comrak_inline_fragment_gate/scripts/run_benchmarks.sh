#!/bin/sh
set -eu

cargo build --release --bins --no-default-features
rustc="${RUSTC:-$(rustup which --toolchain stable rustc)}"
RUSTC="$rustc" rustup run stable cargo build --release --target wasm32-unknown-unknown \
  -p flark-comrak-inline-fragment-wasm-probe --no-default-features

for shape in dense unmatched links; do
  for bytes in 1024 4096 8192 16384; do
    "${CARGO_TARGET_DIR:-target}/release/inline_fragment_bench" "$bytes" "$shape" 2000
  done
done

"${CARGO_TARGET_DIR:-target}/release/inline_document_bench" 10485760 96 ordinary drain
"${CARGO_TARGET_DIR:-target}/release/inline_document_bench" 10485760 96 ordinary retain

wasm="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/flark_comrak_inline_fragment_wasm_probe.wasm"
for shape in 0 1 2; do
  for bytes in 1024 4096 8192 16384; do
    node scripts/bench_wasm.mjs "$wasm" "$bytes" "$shape" 2000
  done
done

cargo build --release --bin inline_fragment_bench --features research-large-inline
for shape in plain ordinary dense; do
  for bytes in 16384 65536; do
    "${CARGO_TARGET_DIR:-target}/release/inline_fragment_bench" "$bytes" "$shape" 1000
  done
done

# Leave the default urgent-path ceiling in the final binary.
cargo build --release --bin inline_fragment_bench --no-default-features
