#!/usr/bin/env bash
set -euo pipefail
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$ROOT/build/dogfood-ready}"
EXAMPLE="$ROOT/packages/flark/example"
APP="$EXAMPLE/build/macos/Build/Products/Profile/Flark Dogfood.app"
MAIN="$APP/Contents/MacOS/Flark Dogfood"
ABI="$APP/Contents/Frameworks/flark_abi.framework/flark_abi"
NATIVE_OUT="$OUT_DIR/native"
MANIFEST="$NATIVE_OUT/app_bundle_manifest.json"
FRAGMENTS="$OUT_DIR/profile-fragments"
PERFORMANCE_RECEIPT="$OUT_DIR/dogfood_performance_receipt.json"
NATIVE_RECEIPT="$NATIVE_OUT/dogfood_native_receipt.json"
DEFAULT_LOG="$OUT_DIR/default-gate.log"
STRESS_LOG="$OUT_DIR/certification-stress.log"
ACTUAL_PAINT_LOG="$OUT_DIR/actual-paint.log"
COMPLETION_RECEIPT="$OUT_DIR/dogfood_completion_receipt.json"
CANDIDATE_COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
CANDIDATE_TREE="$(git -C "$ROOT" rev-parse 'HEAD^{tree}')"
CANDIDATE_MARKER="dogfood-candidate: commit=$CANDIDATE_COMMIT tree=$CANDIDATE_TREE"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo 'verify-v4-dogfood-ready: macOS is required' >&2
  exit 64
fi
if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
  echo 'verify-v4-dogfood-ready: a clean worktree is required' >&2
  exit 1
fi
if [[ -e "$OUT_DIR" ]]; then
  echo "verify-v4-dogfood-ready: output already exists: $OUT_DIR" >&2
  exit 1
fi

frontmost="$({ osascript -e 'tell application "System Events" to get name of first application process whose frontmost is true'; } 2>/dev/null || true)"
if [[ "$frontmost" == "loginwindow" || "$frontmost" == "LoginWindow" ]]; then
  echo 'verify-v4-dogfood-ready: unlock the interactive macOS session first' >&2
  exit 1
fi
mkdir -p "$FRAGMENTS"

echo '==> Active functional gate'
bash "$ROOT/scripts/verify_v4.sh" 2>&1 | tee "$DEFAULT_LOG"
echo "$CANDIDATE_MARKER" | tee -a "$DEFAULT_LOG"

echo '==> Certification stress gate'
bash "$ROOT/scripts/verify_v4_certification_stress.sh" 2>&1 | tee "$STRESS_LOG"
echo "$CANDIDATE_MARKER" | tee -a "$STRESS_LOG"

echo '==> Exact profile dogfood app'
(
  cd "$EXAMPLE"
  flutter build macos --profile
)
for artifact in "$APP" "$MAIN" "$ABI"; do
  if [[ ! -e "$artifact" ]]; then
    echo "verify-v4-dogfood-ready: missing app artifact: $artifact" >&2
    exit 1
  fi
done

echo '==> Non-skipped North-Star actual-paint gate'
(
  cd "$ROOT/packages/flark"
  FLARK_V4_LIBRARY_PATH="$ABI" \
    flutter test \
      test/north_star_paint_matrix_test.dart \
      test/inline_dependency_island_paint_acceptance_test.dart \
      --concurrency=1
) 2>&1 | tee "$ACTUAL_PAINT_LOG"
echo "$CANDIDATE_MARKER" | tee -a "$ACTUAL_PAINT_LOG"

echo '==> Non-skipped native macOS canary'
FLARK_DOGFOOD_PREBUILT_APP=1 \
  bash "$ROOT/scripts/verify_v4_native_canary.sh" "$NATIVE_OUT"

echo '==> Fixed profile and lifecycle matrix'
while IFS=$'\t' read -r cell_id run_count; do
  for ((run_index = 0; run_index < run_count; run_index += 1)); do
    fragment="$FRAGMENTS/${cell_id}.run-${run_index}.json"
    echo "    $cell_id run $run_index/$((run_count - 1))"
    (
      cd "$ROOT"
      /usr/local/bin/timeout 900 \
        dart run scripts/dogfood_profile_run.dart \
          "$cell_id" "$run_index" "$MAIN" "$ABI" "$MANIFEST" "$fragment"
    )
  done
done < <(cd "$ROOT" && dart run scripts/dogfood_profile_run.dart --list)

echo '==> Replay and seal performance receipt'
(
  cd "$ROOT"
  dart run scripts/dogfood_performance_receipt.dart \
    "$ROOT" "$APP" "$MANIFEST" "$MAIN" "$ABI" "$FRAGMENTS" \
    "$PERFORMANCE_RECEIPT"
  dart run scripts/verify_v4_dogfood_receipt.dart \
    "$ROOT" "$PERFORMANCE_RECEIPT"
)

if [[ -n "$(git -C "$ROOT" status --porcelain)" ]]; then
  echo 'verify-v4-dogfood-ready: worktree changed during the gate' >&2
  exit 1
fi

if [[ -z "${FLARK_D0_CANDIDATE_EVIDENCE:-}" ]]; then
  echo 'verify-v4-dogfood-ready: INCOMPLETE candidate evidence is required' >&2
  echo "artifacts are preserved at $OUT_DIR" >&2
  echo 'set FLARK_D0_CANDIDATE_EVIDENCE to the exact-CI/review/moving-surface JSON and run the completion validator' >&2
  exit 1
fi

echo '==> Bind final D0 completion receipt'
(
  cd "$ROOT"
  dart run scripts/verify_v4_dogfood_completion.dart \
    "$ROOT" "$DEFAULT_LOG" "$STRESS_LOG" "$ACTUAL_PAINT_LOG" \
    "$NATIVE_RECEIPT" "$PERFORMANCE_RECEIPT" \
    "$FLARK_D0_CANDIDATE_EVIDENCE" "$COMPLETION_RECEIPT"
)

echo "verify-v4-dogfood-ready: PASS receipt=$COMPLETION_RECEIPT"
