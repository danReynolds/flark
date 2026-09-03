#!/usr/bin/env bash
# Deterministic semantic receipts for the normative GFM 0.29-gfm profile and
# the separate CommonMark 0.31.2 compatibility ledger.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/pinned_rust_env.sh"

cargo test --locked \
  --manifest-path "$ROOT/packages/flark/native/comrak_bridge/Cargo.toml" \
  --package flark-parser \
  --test block_core_commonmark_ledger \
  -- --nocapture

cargo test --locked \
  --manifest-path "$ROOT/packages/flark/native/comrak_bridge/Cargo.toml" \
  --package flark-parser \
  --test gfm_incremental_ledger \
  -- --nocapture

echo "verify_v4_markdown_conformance: GFM semantic + incremental and CommonMark compatibility receipts passed."
