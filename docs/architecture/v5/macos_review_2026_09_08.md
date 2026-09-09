# macOS qualification checkpoint — 2026-09-08

Candidate: merged `25e6190aa4fe9bfc7588d12b5f39d87ed658be72`. The measured runs
used unchanged tracked source; only the earlier qualification records were
untracked. **D0 remains open.** This is local macOS evidence, not CI or device
qualification. No production editor change was made in this continuation.

## Complete foreground frame sweep: passed

The retry completed all 24 shape/site cases and 4,800 measured insert/delete
paints in the production workbench with the actual parser, Tree-sitter editing
and controller-owned coloring workers. Every case met the unchanged 16,667 µs
p99 limit. Worst per-case insertion p99 was **13,790 µs**; deletion p99 was
**14,182 µs**. Source, revision and caret checks passed. There were no lifecycle
transitions. This is a complete fresh sweep, not a combination of partial runs.

The earlier interrupted run remains archived separately: it completed 22 cases
before macOS became inactive during code/largest-block and was correctly rejected.

## Longer foreground workbench run: one performance miss

The run completed all 100 measured open/edit/close cycles and 3,000 sustained
character edits. Exact source/caret, parser-call bounds, saving, opening, mode
transitions, inspection reflow and memory checks passed. Foreground remained
stable throughout. The overall performance gate **failed**:

| Measurement | Worst group p99/result | Limit | Result |
| --- | --- | --- | --- |
| Sustained insertion, 1,506 samples | 16,887 µs | <16,667 µs | Failed |
| Sustained deletion, 1,494 samples | 16,550 µs | <16,667 µs | Passed |
| Return to live mode | 15,044 µs | <16,667 µs | Passed |
| Live inspection reflow | 10,641 µs | <16,667 µs | Passed |
| Source inspection reflow | 16,494 µs | <50,000 µs | Passed |
| Open to editable viewport | 51,768 µs | <200,000 µs | Passed |
| Peak RSS above warmed baseline | 24.02 MiB | ≤64 MiB | Passed |
| Retained RSS above warmed baseline | −70.63 MiB | ≤16 MiB | Passed |

Twenty-two insertions and nine deletions exceeded the live threshold. The slow
insertions were spread across the sustained run, rather than confined to startup.
A 220 µs p99 miss is still a miss; the budget has not been relaxed and a repeat
run is not substituted for diagnosis. Lower retained RSS is an observed growth
guard result, not proof of complete object reclamation.

The prior transition/reflow failure is no longer reproduced. The remaining
performance investigation is narrower: sustained insertion with real workbench
persistence. After this run, the workbench harness was extended to retain action
duration, input-to-build wait, build/raster duration and engine phase timestamps,
as the frame-sweep harness already does. The latency formula, workload, first-paint
assertions and limits are unchanged. Example analysis and all eight qualification
tests passed after these changes. The extra fields have not yet been exercised by a new foreground performance run.

## Native input observations: incomplete, attribution needed

The normal application was rebuilt without diagnostic defines and its app hashes
recorded before native input. The existing Draft was preserved, then used for
temporary canaries with the source-inspection panel open:

- Rapid `alpha beta  gamma!` input left `alpha beta gamma!` (17 bytes).
  The missing-space observation repeated with two explicit Space key events.
- Sending one Space, observing the state, then sending the second Space and the
  following character preserved exact `x  y` (4 bytes).
- A native clipboard replacement produced the expected source. CUA nevertheless
  timed out waiting for clipboard acknowledgement; record the observed content
  separately from the automation error.
- The original Draft was restored and its exact source/byte count checked before
  quitting the normal app.
- In the existing minimal comparison target, both Flark and standard Flutter
  `TextField` preserved the same rapid double-space string with caret 18 on their
  first attempt. The Mac locked before the explicit-Space repeat could complete.

The missing space is an observed native input symptom. These attempts do not yet
isolate the full workbench, Flutter/AppKit or automation delivery as its cause.
Do not add an editor workaround based on the comparison's single successful trial.
A full-workbench trace and repeated matched standard-control trials are next.

Native Return/Delete, styled and fenced editing, clipboard/menu history,
composition/IME, VoiceOver, wheel/resize, ten process reopens and ten OS
background/foreground cycles remain unqualified. Kernel/widget results and the
successful profile cannot substitute for them.

## Provenance and stopping point

Machine and window: Apple M1 Pro, macOS 26.2, Flutter 3.44.4 / Dart 3.12.2,
120 Hz display, 800 × 600 logical viewport, device-pixel ratio 2. Source/build
manifests, linked frame samples and logs are in the
[receipt directory](receipts/macos-2026-09-08/README.md).

Native interaction stopped when CUA reported a locked Mac and unavailable
automatic unlock. The diagnostic runner was stopped; the normal app was rebuilt
successfully without diagnostic defines and its final hashes preserved. The
task-owned qualification processes and awake holds were confirmed stopped. Resume the focused input comparison and the richer performance
trace on an available Mac, preserving both outstanding findings. Existing
[qualification limits](macos_qualification.md) remain authoritative.

## Landing review

A fresh implementation self-review found no blocking issue in the profiling-only
change. The helper retains the same raster-finish-minus-input-start latency and
correlates diagnostic phases to the same proving frame. Action duration excludes
the explicit following `pump`; workload assertions and thresholds are unchanged.
The complete frame and workbench samples were audited against their summaries: all
sample counts and nearest-rank percentiles match, including the one failed gate.
Local VM-service URLs were removed from logs before publication. This is not an
independent peer review or a claim that the native findings have been resolved.
