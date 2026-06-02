#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PKG_ROOT"

flutter test \
  --platform chrome \
  test/v2/flutter/sovereign_markdown_web_smoke_test.dart \
  --reporter compact
