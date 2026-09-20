# Native qualification follow-up — 2026-09-20

Production baseline: merged `e7136388dd0cb5d080b05adfba2ef4099b570bf5`.
The production renderer and the existing performance workloads/budgets were
unchanged. CI remains skipped at the owner's request. This is a diagnostic
follow-up, not a production qualification pass.

## Workbench performance

The first attempt was rejected before measurement: Flutter remained hidden
through the initial foreground deadline. In the second attempt, the launcher
again failed automatic activation; raising/clicking the actual window established
foreground during preflight. The unchanged harness then completed all 100
open/edit/close cycles and 3,000 sustained edits. It recorded `lostForeground:
false` and one initial `resumed` transition. Native accessibility inspection
occurred during startup, not as a controlled VoiceOver usability exercise.

All source/caret checks passed. The process log contains zero AXTree errors.
The logical viewport was 800×600 at DPR 2 and reported 120 Hz. The machine was
running other CPU-intensive workloads, so no quiet-run claim is made.

| Measurement | Current result | Unchanged limit |
| --- | ---: | ---: |
| Table inspection reflow, 14 samples | 13.042 ms p99/max | <16.667 ms |
| Sustained insertion, 1,506 samples | 15.356 ms p99; 27.375 ms max | p99 <16.667 ms |
| Sustained deletion, 1,494 samples | 15.296 ms p99; 16.141 ms max | p99 <16.667 ms |
| Peak RSS growth above warm baseline | 19.11 MiB | ≤64 MiB |
| Retained RSS growth | −74.08 MiB | ≤16 MiB |

The earlier table miss of 19.445 ms did not reproduce, but **the complete gate
still failed**. Small groups use nearest-rank p99 and therefore do not discard
their slowest sample:

| Failing operation group | p99/max |
| --- | ---: |
| Nested containers: return to live | 50.984 ms |
| Nested containers: inspection reflow | 22.574 ms |
| References: return to live | 17.473 ms |
| References: redo boundary | 20.273 ms |
| References: deletion | 19.032 ms |
| Unicode: insertion | 20.108 ms |

The worst sample spent 48.721 ms from input to build start, 1.763 ms building
and 0.449 ms rasterizing; its 2.903 ms action time is included in the first
interval. Scheduling delay dominates that outlier. The nested reflow miss had
a 16.044 ms build interval, and other misses include both action and build cost.
This is insufficient to attribute every failure to contention or to choose a
renderer optimization. Preserve the failed samples and repeat under controlled
conditions before changing code or declaring the gate closed.

## Accessibility comparison and harness correction

The previous widget probe enabled framework semantics but did not establish
native AX activation. A temporary attended variant exposed two test-harness
constraints: live widget tests drop device pointer events by default, and their
leak accounting treats a platform-owned semantics handle created during the
test as a leak. Forcing a framework tree before the native bridge exists also
does not prove that the native accessibility tree is available.

With real pointer delivery and OS-driven semantics, the temporary comparison
completed both hosts: 50 replacement cycles and 1,000 edits each, with resumed
lifecycle, enabled frames and native semantics checked throughout. Native AX
inspection exposed each Start button and the focused text field. Zero AXTree
errors appeared. However, the widget runner rejected the final platform-owned
handle, so this is **not a passing integration-test receipt** and does not close
the native accessibility gate.

The retained solution is `semantics_native_probe.dart`, a separate attended app
using the normal Flutter binding. Each host requires native activation and
foreground, checks exact source and caret after each painted edit, disposes
each mounted control, and records counts, lifecycle transitions and failures.
It neither forces semantics nor modifies drafts/preferences. The existing
framework-only probe remains unchanged. A new guard regression verifies that
framework semantics alone cannot produce a successful native result. Review
tightened that regression to establish resumed lifecycle and enabled frames
first; otherwise an unset test lifecycle could mask the native-activation check.

The standalone app built and launched, but the Mac locked before its native
interaction could begin. The computer-use tool explicitly reported that it
could not unlock the Mac. This final native comparison remains pending. The
temporary diagnostic process was stopped and the ordinary app build restored.

## Verification and next step

Example analysis was clean and all 32 example tests passed, including the new
native-activation refusal check. The ordinary macOS profile build also passed.
Results are recorded with the
[local receipts](receipts/native-followup-2026-09-20/). No production renderer,
kernel or parser code changed. Review checked native-activation refusal,
foreground-loss detection, exact source/caret checks, controller disposal and
failure reporting; the final standalone positive path still needs its native run.

When the Mac is unlocked, first finish the standalone AX comparison. Then use a
quiet 7–10 minute period for the unchanged full workbench gate. A passing table
group alone cannot close it. The separate full frame-profile workload, ordinary
app OS-input/lifecycle canaries, VoiceOver usability and physical IME/device
checks remain required by the macOS qualification contract.
