# V5 macOS qualification

These are predeclared D0 limits, not measured passes. Both profile targets run
the production workbench with its parser, text surface, toolbar, preferences
writes, and exit-save handler. Native OS input and lifecycle canaries qualify
the remaining AppKit routes on the rebuilt normal application.

## Workloads and limits

| Measurement | D0 limit |
| --- | --- |
| Live edit to matching engine raster finish, including return from source mode | p99 below 16,667 µs |
| Source-mode edit or entry to source mode | p99 below 50,000 µs through 256 KiB UTF-8 |
| First exact editable viewport after a document is available to open | p99 below 200,000 µs |
| Live editor reflow when inspection opens/closes | p99 below 16,667 µs |
| Source editor reflow when inspection opens/closes | p99 below 50,000 µs |
| Parser calls for the measured plain insert/delete/history operation | at most one; zero when byte/line preflight already excludes live mode |
| Process peak RSS above warmed baseline | at most 64 MiB |
| Retained RSS above warmed baseline after close and five seconds idle | at most 16 MiB |

The 32 KiB live floor and existing edit-frame limit are unchanged. Opening,
source-mode and memory limits make the previously unnamed D0 requirements
explicit before obtaining results. Source mode has a separate responsiveness
limit; it does not inherit the live tier's no-jank claim.

Percentiles use nearest rank (`ceil(.99 * n) - 1`). Small groups therefore
require every sample to meet the limit. Keep sample counts and maxima beside
p99; an aggregate across easy and hard shapes cannot hide a failing group.

`frame_profile_test.dart` measures 100 insert/delete pairs after 20 warmup pairs
at the start, largest block and end of each admitted 32 KiB shape: prose, dense
blocks, lists, tables, nested containers, unique references and Unicode. Its
kernel/parse/projection diagnostics are separate from the complete frame result.

`workbench_profile_test.dart` warms all thirteen cases twice, then measures 100
open/edit/close cycles and 3,000 character edits with 100 ms between edits
(at least five minutes). It adds the source ceiling and line width, line count,
block count, run count and nesting violations. The live fixtures start one byte
below the boundary, reach the boundary, cross it, return by deletion, and cross
again through Undo/Redo. A caret excursion explicitly ends the preceding typing
group before the separately undoable deletion. Every command's proving paint
must have the exact source and caret, and close must drain the actual save queue.

The ordinary sustained sequence types `alpha beta  gamma!` and deletes it,
checking exact source order and caret after every character. The inspection
toggle exercises the production reflow path in both directions, at a logical
window width above 650 px. It does not substitute for native window dragging.

The RSS baseline is sampled after warmup has opened every shape twice and
disposed the last surface. Peak uses the OS process high-water mark, including
startup and workload setup, so it is conservative. Retained RSS includes the
test binding and saved measurement records; no forced GC makes the result look
better. RSS is an observed growth guard, not proof that every object was freed.

## Required provenance and environment

Record the Git head, dirty input manifest, executable/native-library hashes,
Flutter/engine versions, CPU, OS, display cadence, logical viewport and device
pixel ratio. A profile may not silently qualify a different build or envelope.
The workbench profile retains raw start timestamps linked to engine frame
numbers, latency samples, parse counts and RSS readings in the driver's JSON
result. Preserve that result with the build receipt.

Both harnesses require a resumed Flutter lifecycle with frames enabled. They
wait up to sixty seconds for initial foreground activation and reject any later
loss of foreground. A capturable window, posted key, awake display or manually
pumped hidden test binding does not establish a valid run.

Opening measures the document-to-editable-viewport path after preferences are
loaded; it excludes OS process startup and initial backend loading. The normal
app's startup remains a native canary. Profile commands enter through the Dart
host/kernel boundary; keyboard, clipboard and AppKit menu delivery remain native
canaries even when these measured paths pass.

## Remaining attended checks

Run ordinary multiword typing first, then native Return/Delete/Backspace,
selection and arrows, clipboard/menu Undo/Redo, focus recovery, wheel scrolling,
window resize and sustained wrapped editing. Verify source and next input after
every family. Repeat ten normal process reopens and ten background/foreground
cycles, checking saved source and input recovery each time. These are separate
from the 100 widget open/edit/close cycles; neither count substitutes for the
other. No simulated lifecycle event closes an OS lifecycle gate.

After profiling, rebuild `--profile --target=lib/main.dart` without diagnostic
defines, record its hashes and run the native canaries on that exact application.
D0 stays open until this whole set passes for one candidate.
