#!/usr/bin/env bash
# Slow v4 certification checks intentionally excluded from the everyday gate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"

cargo test \
  --manifest-path "$BRIDGE/Cargo.toml" \
  -p flark-runtime \
  --test fault_containment \
  exhausting_the_payload_budget_is_a_typed_error_not_a_crash \
  -- --exact --ignored

echo "verify_v4_certification_stress: full payload-budget stress passed."
echo "verify_v4_certification_stress: historical M0 receipt drift remains a separate audit."
