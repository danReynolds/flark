#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build/web
"${DART:-dart}" compile js -O2 web/main.dart -o build/web/main.dart.js
cp web/index.html build/web/
cp ../../flark/lib/assets/wasm/flark_parse.wasm build/web/
cp ../../flark_tree_sitter/lib/assets/wasm/flark_tree_sitter.wasm build/web/
cp ../../flark_tree_sitter/lib/assets/highlight_worker.mjs build/web/
# Each candidate has a new asset base URL, including both Wasm modules/workers.
# Reload alone can keep old subresources in the embedded browser's cache.
build_id=$(shasum -a 256 build/web/index.html build/web/main.dart.js build/web/*.wasm build/web/*.mjs | shasum -a 256 | cut -c1-12)
candidate="build/web/revisions/$build_id"
mkdir -p "$candidate"
cp build/web/index.html build/web/main.dart.js build/web/*.wasm build/web/*.mjs "$candidate/"
echo "Candidate URL path: /revisions/$build_id/"
