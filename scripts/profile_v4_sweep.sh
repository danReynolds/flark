#!/usr/bin/env bash
# Development frame-receipt sweep across document sizes and content shapes.
#
# Each cell drives the foregrounded macOS typing profile once; a cell whose
# receipt fails validity (throttled or quiet display on this shared bench)
# is retried once. Receipt JSON lines append to the output file; the sweep
# never fabricates a passing cell.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT="${1:?usage: profile_v4_sweep.sh <output.jsonl>}"
SIZES=(1048576 5242880 10485760)
SHAPES=(ordinary giant-line tiny-blocks)

: > "$OUTPUT"
for size in "${SIZES[@]}"; do
  for shape in "${SHAPES[@]}"; do
    for attempt in 1 2; do
      log="$(mktemp)"
      echo "sweep: size=$size shape=$shape attempt=$attempt"
      FLARK_PROFILE_SOURCE_BYTES="$size" FLARK_PROFILE_SHAPE="$shape" \
        "$ROOT/scripts/profile_v4_macos.sh" > "$log" 2>&1
      status=$?
      receipt="$(grep -o 'FLARK_PROFILE_RECEIPT {.*}' "$log" | head -1 | cut -d' ' -f2-)"
      if [ -n "$receipt" ]; then
        echo "{\"sourceBytesRequested\":$size,\"shape\":\"$shape\",\"attempt\":$attempt,\"driveExit\":$status,\"receipt\":$receipt}" >> "$OUTPUT"
        if [ "$status" -eq 0 ]; then
          rm -f "$log"
          break
        fi
      else
        echo "{\"sourceBytesRequested\":$size,\"shape\":\"$shape\",\"attempt\":$attempt,\"driveExit\":$status,\"receipt\":null}" >> "$OUTPUT"
      fi
      echo "sweep: cell failed (exit $status); log tail:"
      tail -5 "$log"
      rm -f "$log"
    done
  done
done
echo "sweep: complete -> $OUTPUT"
