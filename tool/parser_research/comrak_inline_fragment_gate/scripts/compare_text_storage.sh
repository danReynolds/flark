#!/bin/sh
set -eu

cargo build --release --bin inline_document_bench --features research-owned-text
for shape in plain ordinary; do
  "${CARGO_TARGET_DIR:-target}/release/inline_document_bench" 10485760 96 "$shape" retain
done

cargo build --release --bin inline_document_bench --no-default-features
for shape in plain ordinary; do
  "${CARGO_TARGET_DIR:-target}/release/inline_document_bench" 10485760 96 "$shape" retain
done
