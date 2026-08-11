#!/usr/bin/env bash
# Deterministic semantic receipts for the normative GFM 0.29-gfm profile and
# the separate CommonMark 0.31.2 compatibility ledger.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo test --locked \
  --manifest-path "$ROOT/native/comrak_bridge/Cargo.toml" \
  --package flark-parser \
  --test block_core_commonmark_ledger \
  -- --nocapture

echo "verify_v4_markdown_conformance: GFM normative + CommonMark compatibility receipts passed."
