#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENGINE_ROOT="$REPO_ROOT"
FLUTTER_PACKAGE_ROOT="$REPO_ROOT/packages/flark_flutter"

echo "Flark benchmark lane (enforced budgets)"
echo "Repo: $REPO_ROOT"
echo "Engine package: $ENGINE_ROOT"
echo "Flutter package: $FLUTTER_PACKAGE_ROOT"

echo
echo "==> (cd . && dart run -DFLARK_BENCHMARK_ENFORCE_BUDGETS=true test:test --tags benchmark test/v2/performance --reporter compact)"
(
  cd "$ENGINE_ROOT"
  dart run -DFLARK_BENCHMARK_ENFORCE_BUDGETS=true test:test \
    --tags benchmark \
    test/v2/performance \
    --reporter compact
)

echo
echo "==> (cd packages/flark_flutter && flutter test --tags benchmark test/v2/performance --dart-define=FLARK_BENCHMARK_ENFORCE_BUDGETS=true --reporter compact)"
(
  cd "$FLUTTER_PACKAGE_ROOT"
  flutter test \
    --tags benchmark \
    test/v2/performance \
    --dart-define=FLARK_BENCHMARK_ENFORCE_BUDGETS=true \
    --reporter compact
)

echo
echo "Flark benchmark lane passed."
