# Attended macOS qualification — 2026-09-20

The native frame and workbench performance workloads passed after correcting
accessibility activation in the test harness. The normal-app accessibility
replacement comparison also passed. Production renderer, kernel, parser and
workload sizes/budgets were unchanged; no CI was run.

This closes those local native measurements, not the complete D0 qualification.
Ordinary-app OS-input/lifecycle canaries, VoiceOver usability and physical
IME/device checks remain separate requirements.

## Results

| Check | Result |
| --- | --- |
| Normal-app TextField comparison | 50 mount/edit/unmount cycles, 1,000 exact source/caret checks |
| Normal-app Flark comparison | 50 mount/edit/unmount cycles, 1,000 exact source/caret checks |
| Frame profile | All 24 shape/site cases passed, 4,800 measured edits |
| Worst frame-profile per-case insert/delete p99 | 13.612 ms; limit <16.667 ms |
| Workbench | All 90 timing groups passed; 100 open/edit/close cycles and 3,000 sustained edits |
| Sustained insertion | 1,506 samples, p99 14.399 ms, max 15.197 ms |
| Sustained deletion | 1,494 samples, p99 14.339 ms, max 15.295 ms |
| Slowest live workbench group | Unicode return to live, p99/max 15.481 ms; limit <16.667 ms |
| Slowest source-mode group | Source ceiling deletion, p99/max 20.652 ms; limit <50 ms |
| Slowest opening group | Reference opening, p99/max 47.608 ms; limit <200 ms |
| Table inspection reflow | 14 samples, p99/max 9.712 ms |
| Peak RSS growth | 32.516 MiB; limit ≤64 MiB |
| Retained RSS growth | −78.016 MiB; limit ≤16 MiB; not proof of object reclamation |

All three successful native runs retained OS-requested accessibility and resumed
lifecycle. Both performance harnesses checked enabled frames and rejected loss
of foreground; the final workbench recorded no lifecycle transitions and
`lostForeground: false`. Their logs contain zero AXTree errors. Full driver
results were preserved: 24 frame receipts and 3,732 linked workbench samples.
The viewport was 800×600 logical pixels, DPR 2, reported display cadence 120 Hz.

A background Python process remained near one CPU core in the captured snapshots,
alongside WindowServer and Codex activity. These are measured passes under the
recorded conditions, not an idle-machine claim. No unrelated process was stopped.

## Harness correction and review

The first unchanged workbench run completed all source/caret checks but produced
3,858 AXTree errors and missed reference Redo at 17.406 ms. The normal-app
comparison had already completed without errors. Those failed results are kept.

The widget runner previously forced a framework semantics tree before the native
bridge was activated. The corrected profile setup waits for foreground and the
OS accessibility request in `setUpAll`, before the runner records its handle
baseline. `testWidgets(semanticsEnabled: false)` removes its synthetic request;
it does **not** disable native accessibility. Native semantics are asserted on
every measurement, reported in receipts, and no handles or errors are suppressed.
Frame delivery remains an in-test requirement. Both full workloads then completed
with zero AXTree errors, supporting a harness-startup cause for the observed flood.

Review checked that no fixtures, sample counts, parser-count expectations, timing
budgets, source/caret assertions or memory limits were weakened. The guard test
rejects framework-only semantics and background lifecycle, and exercises native
activation using a test-only platform override with explicit cleanup. That mock
is not counted as native evidence. Final example analysis, formatting and all
33 example tests passed.

The first revised preflight incorrectly required frames during suite setup,
where the live binding intentionally disables them. It was rejected before any
measurement and then corrected. Notably, the integration driver printed success
and exited zero despite that setup failure. Qualification therefore requires
complete receipts and clean error logs, not merely the driver's final banner or
exit code. The rejected attempt remains in the receipts.

## Provenance and restoration

The runs started from `ef73d76` (production baseline `e713638`) plus the retained
profiling-harness correction. Receipts record exact executable, App framework,
Flutter engine, parser and Tree-sitter binary hashes, Flutter/engine versions,
CPU/OS/architecture, dirty input names, and final source hashes. This is profile
mode on the Apple M1 Pro / macOS 26.2 host.

The ordinary app was rebuilt with `--profile --target=lib/main.dart` after the
runs, without diagnostic defines. Its build receipt is separate: restoring that
binary does not imply the remaining OS-input or physical-device checks passed.

See [retained receipts](receipts/native-attended-2026-09-20/) and the
[qualification contract](macos_qualification.md) for the remaining requirements.
