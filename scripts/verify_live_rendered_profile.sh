#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXAMPLE_ROOT="$PKG_ROOT/example"

DEVICE="${FLARK_PROFILE_DEVICE:-macos}"
BLOCKS_LIST="${FLARK_PROFILE_BLOCKS_LIST:-40 80}"
EDITS_LIST="${FLARK_PROFILE_EDITS_LIST:-end start}"

END_MEDIAN_BUDGET_MS="${FLARK_PROFILE_END_MEDIAN_BUDGET_MS:-8}"
END_P95_BUDGET_MS="${FLARK_PROFILE_END_P95_BUDGET_MS:-12}"
START_MEDIAN_BUDGET_MS="${FLARK_PROFILE_START_MEDIAN_BUDGET_MS:-16}"
START_P95_BUDGET_MS="${FLARK_PROFILE_START_P95_BUDGET_MS:-24}"

echo "Flark live-rendered profile gate"
echo "Repo: $PKG_ROOT"
echo "Device: $DEVICE"
echo "Blocks: $BLOCKS_LIST"
echo "Edit positions: $EDITS_LIST"

to_micros() {
  local value="$1"
  case "$value" in
    *us) echo "${value%us}" ;;
    *ms)
      awk -v ms="${value%ms}" 'BEGIN { printf "%d", ms * 1000 }'
      ;;
    *)
      echo "unsupported duration: $value" >&2
      return 1
      ;;
  esac
}

budget_micros() {
  local ms="$1"
  awk -v ms="$ms" 'BEGIN { printf "%d", ms * 1000 }'
}

median_budget_for_edit() {
  case "$1" in
    end) echo "$END_MEDIAN_BUDGET_MS" ;;
    start) echo "$START_MEDIAN_BUDGET_MS" ;;
    *)
      echo "unknown edit position: $1" >&2
      return 1
      ;;
  esac
}

p95_budget_for_edit() {
  case "$1" in
    end) echo "$END_P95_BUDGET_MS" ;;
    start) echo "$START_P95_BUDGET_MS" ;;
    *)
      echo "unknown edit position: $1" >&2
      return 1
      ;;
  esac
}

run_profile() {
  local blocks="$1"
  local edit="$2"
  local output line median p95 median_us p95_us median_budget_us p95_budget_us

  echo
  echo "==> blocks=$blocks edit=$edit"
  output="$(
    cd "$EXAMPLE_ROOT"
    flutter run \
      --profile \
      -d "$DEVICE" \
      -t lib/perf_harness.dart \
      --dart-define=FLARK_PROFILE_BLOCKS="$blocks" \
      --dart-define=FLARK_PROFILE_EDIT="$edit"
  )"
  printf '%s\n' "$output"

  line="$(printf '%s\n' "$output" | grep 'flark_profile ' | tail -n 1 || true)"
  if [[ -z "$line" ]]; then
    echo "missing flark_profile output line" >&2
    return 1
  fi

  median="$(printf '%s\n' "$line" | sed -n 's/.*build_median=\([^ ]*\).*/\1/p')"
  p95="$(printf '%s\n' "$line" | sed -n 's/.*build_p95=\([^ ]*\).*/\1/p')"
  median_us="$(to_micros "$median")"
  p95_us="$(to_micros "$p95")"
  median_budget_us="$(budget_micros "$(median_budget_for_edit "$edit")")"
  p95_budget_us="$(budget_micros "$(p95_budget_for_edit "$edit")")"

  if (( median_us > median_budget_us )); then
    echo "build median $median exceeds budget for edit=$edit" >&2
    return 1
  fi
  if (( p95_us > p95_budget_us )); then
    echo "build p95 $p95 exceeds budget for edit=$edit" >&2
    return 1
  fi
}

for blocks in $BLOCKS_LIST; do
  for edit in $EDITS_LIST; do
    run_profile "$blocks" "$edit"
  done
done

echo
echo "Flark live-rendered profile gate passed."
