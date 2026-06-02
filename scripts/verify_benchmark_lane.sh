#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$PKG_ROOT"

echo "Sovereign benchmark lane (enforced budgets)"
echo "Repo: $REPO_ROOT"
echo "Package: $PKG_ROOT"

echo
echo "==> (cd . && flutter test --tags benchmark test/v2/performance --dart-define=SOVEREIGN_BENCHMARK_ENFORCE_BUDGETS=true --reporter compact)"
(
  cd "$PKG_ROOT"
  flutter test \
    --tags benchmark \
    test/v2/performance \
    --dart-define=SOVEREIGN_BENCHMARK_ENFORCE_BUDGETS=true \
    --reporter compact
)

echo
echo "Sovereign benchmark lane passed."
