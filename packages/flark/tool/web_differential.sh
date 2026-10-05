#!/bin/bash
# The web differential: replay the same seeded command sequences
# (test/web_differential_test.dart) on the Dart VM, through the FFI parser,
# and under node with dart2js and dart2wasm, through the bundled Wasm parser,
# then compare every sequence's digest of the states it reached. A sequence
# whose digests disagree is replayed in full on the platforms involved and
# the first differences are printed. A sequence that ended in an error (the
# editor threw, or a check of a host action failed) is digested by its kind,
# so the digests are still compared, and is reported after them. Exits 0 when
# every platform agrees and no sequence ended in an error on any of them.
#
#   tool/web_differential.sh [-n SEQUENCES] [-s SEED] [-k STEPS] [-j SHARDS]
#                            [-p "vm dart2js dart2wasm"] [--release] [--contract]
#
#   -n  sequences per platform (default 2000)
#   -s  master seed (default 2026)
#   -k  commands per sequence (default 40)
#   -j  processes per platform, each running a slice (default 2)
#   -p  platforms; the first is the reference (default "vm dart2js dart2wasm")
#   --release   compile as Flutter web release builds do: dart2js at -O4 and
#       dart2wasm at -O2 without asserts. Otherwise the test runner's levels:
#       dart2js -O1, dart2wasm -O0, both with asserts.
#   --contract  also run matrix_test.dart and corpus_contract_test.dart, which
#       check their invariants on every platform, under both web compilers
#
# Needs dart on PATH (the workspace resolved with `flutter pub get`) and node
# 20.16 or 22.3 or later. The VM side builds the parser through the package's
# build hook, as `dart test` does.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$PKG/../.." && pwd)"
TEST=test/web_differential_test.dart

SEQUENCES=2000 SEED=2026 STEPS=40 SHARDS=2 PLATFORMS="vm dart2js dart2wasm"
RELEASE=0 CONTRACT=0
while [ $# -gt 0 ]; do
  case "$1" in
    -n) SEQUENCES="$2"; shift 2;;
    -s) SEED="$2"; shift 2;;
    -k) STEPS="$2"; shift 2;;
    -j) SHARDS="$2"; shift 2;;
    -p) PLATFORMS="$2"; shift 2;;
    --release) RELEASE=1; shift;;
    --contract) CONTRACT=1; shift;;
    -h|--help) sed -n '2,28p' "$0"; exit 0;;
    *) echo "unknown option $1" >&2; exit 2;;
  esac
done
command -v node >/dev/null || { echo "node is required" >&2; exit 2; }

WORK="$(mktemp -d)" pids=""
# shellcheck disable=SC2086
trap 'kill $pids 2>/dev/null; rm -rf "$WORK"' EXIT
cd "$PKG" || exit 2

# Compile the differential with dart2wasm at -O2 into $WORK/wasm, laid out as
# `dart test --precompiled` runs it: the test runner itself compiles at -O0
# and takes no dart2wasm options. The entry point and loader are the runner's
# own (package:test 1.31).
precompile_wasm() {
  local dir="$WORK/wasm"
  mkdir -p "$dir/test"
  cat > "$dir/entry.dart" <<DART
import "package:test/src/bootstrap/node.dart";
import "file://$PKG/$TEST" as test;

void main() {
  internalBootstrapNodeTest(() => test.main);
}
DART
  if ! dart compile wasm -O2 -Dnode=true \
      --packages="$ROOT/.dart_tool/package_config.json" \
      -o "$dir/$TEST.node_test.dart.wasm.wasm" "$dir/entry.dart" \
      > "$dir/compile.log" 2>&1; then
    cat "$dir/compile.log"; exit 1
  fi
  cat > "$dir/$TEST.node_test.dart.js" <<JS
const { readFileSync } = require('fs');
(async () => {
  const { instantiate, invoke } = await import('./$(basename "$TEST").node_test.dart.wasm.mjs');
  const bytes = readFileSync(String.raw\`$dir/$TEST.node_test.dart.wasm.wasm\`);
  invoke(await instantiate(WebAssembly.compile(bytes), {}));
})();
JS
}

# `dart test` arguments for the differential on a platform.
platform_args() {
  case "$1:$RELEASE" in
    vm:*) echo "";;
    dart2js:0) echo "-p node";;
    dart2js:1) echo "-p node --dart2js-args=-O4";;
    dart2wasm:0) echo "-p node -c dart2wasm";;
    dart2wasm:1) echo "-p node --precompiled $WORK/wasm --js-trace";;
    *) echo "unknown platform $1" >&2; return 2;;
  esac
}

# run PLATFORM OUT [NAME=VALUE...]: one `dart test` of the differential.
run() {
  local platform="$1" out="$2" args; shift 2
  args="$(platform_args "$platform")" || exit 2
  # shellcheck disable=SC2086
  env FLARK_WEB_DIFF_SEED="$SEED" FLARK_WEB_DIFF_STEPS="$STEPS" \
    FLARK_WEB_DIFF_OUT="$out" "$@" dart test $args -r expanded "$TEST"
}

start=$(date +%s)
case " $PLATFORMS " in *" dart2wasm "*) [ "$RELEASE" -eq 1 ] && precompile_wasm;; esac
per=$(( (SEQUENCES + SHARDS - 1) / SHARDS ))
for platform in $PLATFORMS; do
  for shard in $(seq 0 $((SHARDS - 1))); do
    first=$((shard * per)); count=$per
    [ $((first + count)) -gt "$SEQUENCES" ] && count=$((SEQUENCES - first))
    [ "$count" -le 0 ] && continue
    log="$WORK/$platform.$shard.log"
    run "$platform" "$WORK/$platform.$shard.txt" \
      FLARK_WEB_DIFF_FIRST="$first" FLARK_WEB_DIFF_ITERATIONS="$count" \
      > "$log" 2>&1 &
    pid=$!; pids="$pids $pid"
    # One start at a time: `dart test` copies the package's native library
    # into .dart_tool/lib as it starts, and concurrent copies corrupt it.
    while kill -0 "$pid" 2>/dev/null && ! grep -q ' loading ' "$log"; do
      sleep 1
    done
  done
done
for pid in $pids; do wait "$pid"; done
pids=""
# A run that ended its sequences prints its summary, and fails when any of
# them ended in an error: its digests are complete, and compared below. A run
# that failed otherwise (a crash, a timeout) leaves nothing to compare.
broken=0
for log in "$WORK"/*.log; do
  grep -h 'web differential [a-z0-9]*: [0-9]' "$log" | sed 's/^/  /'
  grep -q 'All tests passed' "$log" && continue
  grep -q 'web differential error: ' "$log" && continue
  broken=1
  echo "--- $(basename "$log" .log) failed:"; tail -40 "$log"
done
[ "$broken" -ne 0 ] && exit 1

reference=""
status=0
for platform in $PLATFORMS; do
  cat "$WORK/$platform".*.txt 2>/dev/null | sort -n -k1,1 > "$WORK/$platform.txt"
  got=$(wc -l < "$WORK/$platform.txt" | tr -d ' ')
  if [ "$got" -ne "$SEQUENCES" ]; then
    echo "$platform recorded $got of $SEQUENCES sequences"; status=1
  fi
  if [ -z "$reference" ]; then reference="$platform"; continue; fi
  diff "$WORK/$reference.txt" "$WORK/$platform.txt" > "$WORK/$platform.diff"
  differing=$(grep '^<' "$WORK/$platform.diff" | awk '{print $2}' | sort -un)
  if [ -z "$differing" ]; then
    echo "$platform agrees with $reference on all $got sequences"
    continue
  fi
  status=1
  # shellcheck disable=SC2086
  echo "$platform differs from $reference on $(echo "$differing" | wc -l | tr -d ' ') sequences: $(echo $differing | cut -c1-200)"
  trace=$(echo "$differing" | head -3 | paste -sd, -)
  run "$reference" "$WORK/$reference.trace" FLARK_WEB_DIFF_TRACE="$trace" > /dev/null 2>&1
  run "$platform" "$WORK/$platform.trace" FLARK_WEB_DIFF_TRACE="$trace" > /dev/null 2>&1
  echo "--- first differences in sequences $trace ($reference -> $platform):"
  # Each hunk names its sequence and step (#sequence.step).
  diff -u -F '^#[0-9]' "$WORK/$reference.trace" "$WORK/$platform.trace" |
    sed 1,2d | head -80
done

# Errors fail the run whether or not the platforms agree on them.
errors=$(grep -ho 'ms, [0-9]* ended in an error' "$WORK"/*.log |
  awk '{n += $2} END {print n + 0}')
if [ "$errors" -gt 0 ]; then
  status=1
  echo "$errors sequences ended in an error, over all runs (each lists its first 20):"
  grep -h 'web differential error: ' "$WORK"/*.log |
    sed 's/.*web differential error: /  /' | head -40
fi

if [ "$CONTRACT" -eq 1 ]; then
  for platform in $PLATFORMS; do
    case "$platform:$RELEASE" in
      dart2js:0) args="-p node";;
      dart2js:1) args="-p node --dart2js-args=-O4";;
      dart2wasm:*) args="-p node -c dart2wasm";;
      *) continue;;
    esac
    # shellcheck disable=SC2086
    if dart test $args test/matrix_test.dart test/corpus_contract_test.dart \
        > "$WORK/contract.$platform.log" 2>&1; then
      echo "$platform passes the matrix and corpus contract"
    else
      echo "$platform fails the matrix or corpus contract:"
      tail -40 "$WORK/contract.$platform.log"; status=1
    fi
  done
fi
echo "web differential: $SEQUENCES sequences of $STEPS commands (seed $SEED) on $PLATFORMS in $(( $(date +%s) - start ))s"
exit $status
