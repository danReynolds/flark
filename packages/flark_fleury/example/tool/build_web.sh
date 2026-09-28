#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build/web
"${DART:-dart}" compile js -O2 web/main.dart -o build/web/main.dart.js
cp web/index.html build/web/
cp assets/demo.png build/web/
cp ../../flark/lib/assets/wasm/flark_parse.wasm build/web/
# Each candidate has a new asset base URL, including the parser's Wasm module.
# Reload alone can keep old subresources in the embedded browser's cache.
assets=(build/web/index.html build/web/main.dart.js build/web/flark_parse.wasm build/web/demo.png)
build_id=$(shasum -a 256 "${assets[@]}" | shasum -a 256 | cut -c1-12)
candidate="build/web/revisions/$build_id"
mkdir -p "$candidate"
cp "${assets[@]}" "$candidate/"
echo "Candidate URL path: /revisions/$build_id/"
