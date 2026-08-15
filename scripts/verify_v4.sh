#!/usr/bin/env bash
# Fast local gate for the active direct v4 path (macOS, per the Mac-first plan).
# Builds the flark-abi library and runs the active implementation and contract
# suites with
# FLARK_V4_LIBRARY_PATH exported: without it the Dart and Flutter suites
# silently skip, so a run without this script can look green while executing
# nothing. The immutable 2026-08-08 M0 receipt drift audit and full-scale
# payload-budget stress are historical/certification lanes, not everyday gates.
# No pipeline here may mask an exit code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE="$ROOT/native/comrak_bridge"
PROFILE="${FLARK_V4_PROFILE:-debug}"

if [[ "$PROFILE" == "release" ]]; then
  cargo build --manifest-path "$BRIDGE/Cargo.toml" --package flark-abi --release
else
  cargo build --manifest-path "$BRIDGE/Cargo.toml" --package flark-abi
fi
LIBRARY="$BRIDGE/target/$PROFILE/libflark_abi.dylib"
if [[ ! -f "$LIBRARY" ]]; then
  echo "verify_v4: missing $LIBRARY" >&2
  exit 1
fi

cargo test --manifest-path "$BRIDGE/Cargo.toml" -p flark-runtime -p flark-abi
"$ROOT/scripts/verify_v4_markdown_conformance.sh"

(cd "$ROOT" && dart test test/v4/contracts --exclude-tags historical-receipt)
# A fresh worktree has no package-local .dart_tool state. Resolve each v4
# package explicitly so this gate does not depend on a previous developer run.
(cd "$ROOT/packages/flark_core" && dart pub get)
(cd "$ROOT/packages/flark" && flutter pub get)
(cd "$ROOT/packages/flark_flutter" && flutter pub get)
(cd "$ROOT/packages/flark_core" && dart analyze)
(cd "$ROOT/packages/flark_core" && FLARK_V4_LIBRARY_PATH="$LIBRARY" dart test)
(cd "$ROOT/packages/flark" && FLARK_V4_LIBRARY_PATH="$LIBRARY" flutter analyze)
# The package loads one native worker library per test process. Serial execution
# is the stable, still-fast contract (about 20 s warm); concurrent flutter_test
# processes can deadlock during native-worker teardown after all assertions.
(cd "$ROOT/packages/flark" && FLARK_V4_LIBRARY_PATH="$LIBRARY" flutter test --concurrency=1)
(cd "$ROOT/packages/flark_flutter" && flutter test test/v4/contracts)

echo "verify_v4: active rust + dart + flutter v4 suites executed and passed."
echo "verify_v4: run scripts/verify_v4_certification_stress.sh for slow stress lanes."
