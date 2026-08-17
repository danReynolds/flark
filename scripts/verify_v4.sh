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
BRIDGE="$ROOT/packages/flark_core/native/comrak_bridge"
PROFILE="${FLARK_V4_PROFILE:-debug}"
# Experiment opt-in: extra cargo features for the flark-abi library build
# (e.g. FLARK_V4_FEATURES=opening-session for the RFC 029 A3 streamed-open
# surface). Empty by default so the everyday gate exercises exactly the
# default feature set; the feature-gated Dart suites additionally key off
# FLARK_V4_OPENING_LIBRARY_PATH and skip unless it is exported below.
FEATURES="${FLARK_V4_FEATURES:-}"

build_args=(--manifest-path "$BRIDGE/Cargo.toml" --package flark-abi)
if [[ "$PROFILE" == "release" ]]; then
  build_args+=(--release)
fi
if [[ -n "$FEATURES" ]]; then
  build_args+=(--features "$FEATURES")
fi
cargo build "${build_args[@]}"
LIBRARY="$BRIDGE/target/$PROFILE/libflark_abi.dylib"
if [[ ! -f "$LIBRARY" ]]; then
  echo "verify_v4: missing $LIBRARY" >&2
  exit 1
fi
# The streamed-open Dart suites need the opening-session entry points, which
# only exist when the feature was compiled in. Exporting the path is the
# explicit signal that this library carries them; without it those suites
# skip rather than fail against a default-feature library.
if [[ ",$FEATURES," == *",opening-session,"* ]]; then
  export FLARK_V4_OPENING_LIBRARY_PATH="$LIBRARY"
fi

# Every first-party target must at least compile, so a broken engine/parser
# test or example cannot rot invisibly outside the runtime/abi suites.
cargo check --manifest-path "$BRIDGE/Cargo.toml" \
  -p flark-engine -p flark-parser -p flark-runtime -p flark-abi --all-targets

cargo test --manifest-path "$BRIDGE/Cargo.toml" -p flark-runtime -p flark-abi
"$ROOT/scripts/verify_v4_markdown_conformance.sh"

(cd "$ROOT" && dart test test/qualification/v4 --exclude-tags historical-receipt)

# A fresh worktree has no package-local .dart_tool state. Resolve each v4
# package explicitly so this gate does not depend on a previous developer run.
(cd "$ROOT/packages/flark_core" && dart pub get)
(cd "$ROOT/packages/flark" && flutter pub get)
(cd "$ROOT/packages/flark_core" && dart analyze)
(cd "$ROOT/packages/flark_core" && FLARK_V4_LIBRARY_PATH="$LIBRARY" dart test)
(cd "$ROOT/packages/flark" && FLARK_V4_LIBRARY_PATH="$LIBRARY" flutter analyze)
# The package loads one native worker library per test process. Serial execution
# is the stable, still-fast contract (about 20 s warm); concurrent flutter_test
# processes can deadlock during native-worker teardown after all assertions.
(cd "$ROOT/packages/flark" && FLARK_V4_LIBRARY_PATH="$LIBRARY" flutter test --concurrency=1)

echo "verify_v4: active rust + dart + flutter v4 suites executed and passed."
echo "verify_v4: run scripts/verify_v4_certification_stress.sh for slow stress lanes."
