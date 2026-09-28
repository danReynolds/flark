# V5 macOS qualification

These are predeclared D0 limits, not measured passes. Both profile targets run
the production workbench with its parser, text surface, toolbar, preferences
writes, and exit-save handler. Native OS input and lifecycle canaries qualify
the remaining AppKit routes on the rebuilt normal application.

## Workloads and limits

| Measurement | D0 limit (UI work and raster, each) |
| --- | --- |
| Live edit, including return from source mode | p99 below 16,667 µs, and the next frame |
| Source-mode edit or entry to source mode | p99 below 50,000 µs through 256 KiB UTF-8 |
| First exact editable viewport after a document is available to open | p99 below 200,000 µs |
| Live editor reflow when inspection opens/closes | p99 below 16,667 µs, and the next frame |
| Source editor reflow when inspection opens/closes | p99 below 50,000 µs |
| Parser calls for the measured plain insert/delete/history operation | at most one; zero when byte/line preflight already excludes live mode |
| Process peak RSS above warmed baseline | at most 64 MiB |
| Retained RSS above warmed baseline after close and five seconds idle | at most 16 MiB |

### The frame gate (revised 2026-09-22)

Each edit is measured by the work on its critical path, not by wall-clock
latency. **UI work** is the command plus the build of the frame that shows it;
**raster** is that frame's raster duration. Each must meet the budget at p99.
Live edits and reflows must also reach the **next frame**: at most 1% of their
samples may reach the screen after one or more further vsyncs once the command
has finished. `example/test_driver/frame_gate.dart` implements the gate for
both harnesses and the receipt validator.

Input-to-raster latency is still recorded and reported, but it is not gated.
In the 2026-09-20 receipts, about 6.5 ms of the 11.2 ms median was waiting for
vsync: the harness's fixed input cadence landed each input just before a vsync
that the command then missed. On a 60 Hz display that wait alone approaches the
old 16.7 ms limit, so latency measured display timing rather than Flark's work.
Re-evaluated under this gate, the 2026-09-20 workbench run passes: worst live
UI work p99 9.3 ms, worst raster p99 1.1 ms, 4 of 3,348 live samples late.

Qualify at both 120 Hz and 60 Hz. On a ProMotion Mac, set the display to
60 Hz in System Settings → Displays for the second run. Receipts record the
display rate, and the gate derives the frame interval from it.

The 32 KiB live floor and existing edit-frame budget are unchanged. Opening,
source-mode and memory limits make the previously unnamed D0 requirements
explicit before obtaining results. Source mode has a separate responsiveness
limit; it does not inherit the live tier's no-jank claim.

Percentiles use nearest rank (`ceil(.99 * n) - 1`). Small groups therefore
require every sample to meet the limit. Keep sample counts and maxima beside
p99; an aggregate across easy and hard shapes cannot hide a failing group.

`frame_profile_test.dart` measures 100 insert/delete pairs after 20 warmup pairs
at the start, largest block and end of each admitted 32 KiB shape: prose, dense
blocks, lists, tables, nested containers, unique references, Unicode and code
regions (24 shape/site cases). At the start site it also inserts rows before
most of the document, with Enter and a multi-line paste, each followed by the
Undo that removes them. These edits move every later row, which typing never
does. The start-site fixture keeps 64 bytes of headroom so those edits stay
live; the other sites still type at the byte boundary. The code fixture contains Dart, Ruby and JSON.
Both production harnesses load the same Tree-sitter service and controller-owned
color workers as the normal workbench; receipts report their presence. Its
kernel/parse/projection diagnostics are separate from the complete frame result.

`workbench_profile_test.dart` warms all fourteen cases twice, then measures 100
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
wait up to sixty seconds for initial foreground and native accessibility
activation before the widget runner records its semantics-handle baseline.
They use OS-requested semantics rather than a forced framework tree, require
native semantics throughout measurement, and reject any later loss of foreground.
Frame delivery is checked inside the test because the live binding disables
frames during suite setup. Inspect the native accessibility tree and activate
the title bar during preflight; do not click an unmounted test surface.
A capturable window, posted key, awake display or manually
pumped hidden test binding does not establish a valid run.

Use `test_driver/profile.dart` for full qualification runs. It preserves the
response data and rejects missing/incomplete receipts before returning success.
A setup failure can leave the generic integration driver reporting success
without having executed the widget test. Deliberately filtered diagnostics use
the generic driver and do not qualify a full run. Inspect the process log for
framework/native errors as well as the validated driver's exit status.

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
