#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="$REPO_ROOT/packages/flark_core/native/comrak_bridge/Cargo.toml"
USER_ID="$(id -u)"
LEASE_DIR="/tmp/flark-native-focus-$USER_ID.lease"
LOG_DIR="/tmp/flark-native-focus-logs-$USER_ID"

usage() {
  cat <<'EOF'
Run one exact focused Rust test with a shared Cargo lease and compact receipt.

Usage:
  ./scripts/run_native_focus.sh PACKAGE EXACT_TEST_FILTER [CARGO_TEST_FLAGS...]

Examples:
  ./scripts/run_native_focus.sh flark-engine \
    recursive_green::tests::active_terminal_fragment_cursor_and_visible_suffix_rewrite_preserve_projection_and_source \
    --lib
  ./scripts/run_native_focus.sh flark-parser \
    block_core::reference_rendezvous::tests::same_paragraph_reference_work_has_linear_doubling_slope \
    --lib --locked

The complete Cargo output is retained in a uid-scoped directory under `/tmp`;
this helper removes only its own log files older than seven days.
Only one invocation may run at a time; contention exits immediately with 75.
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
esac

if [ "$#" -lt 2 ]; then
  echo "error: PACKAGE and EXACT_TEST_FILTER are required" >&2
  usage >&2
  exit 64
fi

package="$1"
test_filter="$2"
shift 2

if ! mkdir "$LEASE_DIR" 2>/dev/null; then
  owner_pid="unknown"
  if [ -r "$LEASE_DIR/pid" ]; then
    owner_pid="$(sed -n '1p' "$LEASE_DIR/pid")"
  fi
  printf 'BUSY lease=%s owner_pid=%s another focused Cargo run is active\n' \
    "$LEASE_DIR" "${owner_pid:-unknown}" >&2
  exit 75
fi

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  rm -f "$LEASE_DIR/pid" 2>/dev/null || true
  rmdir "$LEASE_DIR" 2>/dev/null || true
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

printf '%s\n' "$$" >"$LEASE_DIR/pid"

mkdir -p "$LOG_DIR"
find "$LOG_DIR" -type f -name '*.log' -mtime +7 -delete 2>/dev/null || true
timestamp="$(date '+%Y%m%dT%H%M%S')"
safe_name="$(printf '%s' "$package-$test_filter" | LC_ALL=C tr -c 'A-Za-z0-9._-' '_' | cut -c 1-120)"
log_file="$LOG_DIR/$timestamp-$safe_name-$$.log"
started="$(date '+%s')"

{
  printf 'repository=%s\n' "$REPO_ROOT"
  printf 'package=%s\n' "$package"
  printf 'exact_test_filter=%s\n' "$test_filter"
  for flag in "$@"; do
    printf 'cargo_flag=%s\n' "$flag"
  done
} >"$log_file"

printf 'RUN package=%s test=%s log=%s\n' "$package" "$test_filter" "$log_file"

set +e
(
  cd "$REPO_ROOT"
  cargo test \
    --manifest-path "$MANIFEST" \
    --package "$package" \
    "$@" \
    "$test_filter" \
    -- \
    --exact \
    --nocapture
) >>"$log_file" 2>&1
status=$?
set -e

if [ "$status" -eq 0 ] && ! grep -Eq '^test result: ok\. [1-9][0-9]* passed;' "$log_file"; then
  status=65
  printf '%s\n' \
    'error: exact test filter matched no passing test; use its fully qualified name' \
    >>"$log_file"
fi

finished="$(date '+%s')"
elapsed=$((finished - started))

if [ "$status" -eq 0 ]; then
  printf 'PASS package=%s test=%s elapsed=%ss\n' "$package" "$test_filter" "$elapsed"
  result_line="$(grep -E '^test result: ' "$log_file" | tail -n 1 || true)"
  if [ -n "$result_line" ]; then
    printf '%s\n' "$result_line"
  fi
  printf 'log=%s\n' "$log_file"
else
  printf 'FAIL package=%s test=%s status=%s elapsed=%ss\n' \
    "$package" "$test_filter" "$status" "$elapsed" >&2
  printf 'log=%s\n' "$log_file" >&2
  printf '%s\n' '--- failure tail (last 60 lines) ---' >&2
  tail -n 60 "$log_file" >&2
fi

exit "$status"
