#!/usr/bin/env bash
# Development frame-receipt sweep across document sizes and content shapes.
#
# Each supported cell drives the foregrounded macOS typing profile once; a
# cell whose receipt fails validity (throttled or quiet display on this shared
# bench) is retried once. Receipt JSON lines append to the output file; the
# sweep never fabricates a passing cell. The 10 MiB tiny-block cell is outside
# the documented 5 MiB extreme-density envelope of the fixed 64 MiB arena and
# is recorded as such instead of being misreported as an editor failure.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT="${1:?usage: profile_v4_sweep.sh <output.jsonl>}"
SIZES=(1048576 5242880 10485760)
SHAPES=(ordinary giant-line tiny-blocks)

: > "$OUTPUT"
for size in "${SIZES[@]}"; do
  for shape in "${SHAPES[@]}"; do
    if [ "$shape" = "tiny-blocks" ] && [ "$size" -gt 5242880 ]; then
      echo "{\"sourceBytesRequested\":$size,\"shape\":\"$shape\",\"skipped\":\"outside-supported-density-envelope\",\"maximumSourceBytes\":5242880}" >> "$OUTPUT"
      continue
    fi
    for attempt in 1 2; do
      log="$(mktemp)"
      echo "sweep: size=$size shape=$shape attempt=$attempt"
      FLARK_PROFILE_SOURCE_BYTES="$size" FLARK_PROFILE_SHAPE="$shape" \
        "$ROOT/scripts/profile_v4_macos.sh" > "$log" 2>&1
      status=$?
      receipt="$(grep -o 'FLARK_PROFILE_RECEIPT {.*}' "$log" | head -1 | cut -d' ' -f2-)"
      rejection="$(grep -o 'FLARK_PROFILE_REJECTION {.*}' "$log" | head -1 | cut -d' ' -f2-)"
      if [ -n "$receipt" ]; then
        echo "{\"sourceBytesRequested\":$size,\"shape\":\"$shape\",\"attempt\":$attempt,\"driveExit\":$status,\"receipt\":$receipt,\"rejection\":null}" >> "$OUTPUT"
        if [ "$status" -eq 0 ]; then
          rm -f "$log"
          break
        fi
      elif [ -n "$rejection" ]; then
        echo "{\"sourceBytesRequested\":$size,\"shape\":\"$shape\",\"attempt\":$attempt,\"driveExit\":$status,\"receipt\":null,\"rejection\":$rejection}" >> "$OUTPUT"
      else
        echo "{\"sourceBytesRequested\":$size,\"shape\":\"$shape\",\"attempt\":$attempt,\"driveExit\":$status,\"receipt\":null,\"rejection\":null}" >> "$OUTPUT"
      fi
      echo "sweep: cell failed (exit $status); log tail:"
      tail -5 "$log"
      rm -f "$log"
    done
  done
done
echo "sweep: complete -> $OUTPUT"
