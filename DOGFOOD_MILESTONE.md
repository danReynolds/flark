# D0: Owner dogfood, by platform

**Product bar:** [NORTH_STAR.md](NORTH_STAR.md)

**Editing behavior:** [edit_profile_v1.md](docs/architecture/v5/edit_profile_v1.md)

**Testing approach:** [live_editor_test_strategy.md](docs/testing/live_editor_test_strategy.md)

**Latest checkpoint (2026-09-08 UTC):** the
[resource/theme landing review](docs/architecture/v5/theming_merge_review_2026_09_08.md)
covers parser-backed link/image editing, bounded image previews, Markdown themes,
replaceable resource controls and the single-editor Flutter theme playground.
Local automated checks and normal web/macOS builds pass; CI is skipped. The
observed web accessibility-mode input failure is corrected and covered through
Chrome's actual semantics transport. This is not a VoiceOver/TalkBack pass.
The foreground native timing, OS input, lifecycle and physical-device gates
remain open. The Fleury host/example remains M4/T4. The
[revisited priorities](docs/architecture/v5/priorities_after_theming.md) describe
the next work.

**Delivery status (2026-09-06 automatic outdent):** D0-web is ready for exploratory
owner dogfooding on the observed embedded browser. The
[code editing review](docs/architecture/v5/code_closer_review_2026_09_06.md)
records visible code selection, syntax coloring, language choice, indentation and typed-closer outdent
with actual browser and paint regressions.
The complete Flutter web/Wasm workbench remains the primary
development and dogfooding environment in Codex's embedded browser. Its
[browser acceptance contract](docs/architecture/v5/web_dogfood.md) covers the
common editing loop, browser input, persistence and bounded source mode.
D0-web can pass independently of D0-macOS. Neither status is a release claim.

The macOS requirements below remain unchanged and are now called D0-macOS.
Web evidence cannot close AppKit input or native frame-budget failures.

**Current native status (2026-09-05):** D0-macOS remains open. The
[continuation record](docs/architecture/v5/native_session_2026_09_04.md) records
browser-found corrections and passing local checks. The
[native sweep](docs/architecture/v5/native_profile_2026_09_05.md) now records an
open B1 performance miss in five largest-paragraph cases. The
[input-context correction](docs/architecture/v5/input_context_review_2026_09_05.md)
brings the focused case below the limit in diagnostic evidence. D0-macOS still needs
a complete foreground run and normal-app native canaries on the new candidate.

## Goal

D0 means the current macOS app is in a high-quality state for sustained owner
dogfooding of the core live Markdown experience. It need not be release-perfect,
but common editing must be responsive, continuously rendered, and free of known
high-severity interaction failures.

D0 is reached only when all five sections below pass for one locally built
candidate. Passing CI, parser conformance, or a final settled frame alone is not
enough.

## 1. Functional editing

Direct Core and controller tests must cover:

- ordinary typing, punctuation, whitespace, and source-delimiter completion;
- Backspace and Delete inside and at both edges of Emphasis, Strong,
  Strikethrough, Inline Code, and representative nested/literal cases;
- deleting the final styled grapheme, then immediately typing a character or
  whitespace;
- selection creation, collapse, replacement, arrow movement, and pointer
  placement at formatting boundaries;
- paragraph, heading, list, quote, table, and terminal-gap Return/Backspace;
- Undo and Redo across insert, delete, replacement, and structural edits;
- full-value, delta, key, and semantic-command delivery without duplication;
- repeated Return/Backspace followed immediately by typing; and
- every committed model and projection matching a clean parse.

Every accepted command must leave exact source, selection, history, and the next
writable state correct.

## 2. Visible-frame quality

Mounted tests inspect every actual paint for representative cases from each
functional family. They require:

- current visible text with no unrelated Markdown markers;
- retained inline style and block presentation;
- canonical selection, displayed selection, and painted caret identity;
- stable geometry, focus, hit testing, and accessibility semantics;
- no stale source generation or mixed old/new presentation; and
- terminal equivalence with a clean rebuild.

Required rapid sequences include delete-to-empty then type, repeated Return then
type, structural Backspace then type, and delete/insert Undo/Redo. Each runs at
human cadence and as a true unpumped burst where scheduling is part of the risk.

## 3. Native macOS interaction

The headless functional suite runs first. In one attended native session, the
candidate then proves:

- real character and punctuation input reaches the focused editor once;
- Return, Backspace, Delete, arrows, pointer selection, paste, cut, Undo, and
  Redo use the real AppKit routes;
- focus loss/recovery and input-connection reopening accept the next command;
- wheel scrolling and resize preserve source and selection;
- sustained editing across wrapped Markdown never rehomes the caret or exposes
  certified markers; and
- the actuator verifies the exact app, window, focus, and selection before
  sending input.

Posted input is not considered delivered until the app records a corresponding
event and reaches a stable state.

## 4. Live envelope and source mode

The initial macOS qualification floor is continuously rendered editing through 32 KiB of
UTF-8 source when the document is also inside the published shape budget. The
checked-in product tour plus representative plain, dense, long-list,
table-heavy, Unicode-heavy, and giant-line presets immediately below, at, and
above both boundaries cover open, local editing, scrolling, resize, Undo, and
sustained input.

Inside the configured byte-and-shape envelope, the complete input-to-raster path
must stay inside the measured frame budget. Outside it, the document opens
directly in a visibly identified source mode that remains exact and writable
without first projecting the full document. Cheap byte/line preflight must
avoid parsing plainly ineligible input. Paste, Undo, Redo, and deletion across
the boundary switch modes atomically. A 64 KiB ordinary-prose tier is a stretch
candidate, not a promise; the limit may be raised only by a production-path
receipt across the published adversarial shape set.

## 5. Mac performance and lifecycle proof

The production-path profile must pass the checked-in frame-latency, memory,
opening, parser-work, mode-transition, resize, and sustained-input budgets on the
benchmark Mac. The [V5 macOS qualification contract](docs/architecture/v5/macos_qualification.md)
names those limits and the two workloads. Measurements identify the exact commit, tree, app executable,
native library, host, display, and configuration.

The app must also survive repeated open/close, background/foreground,
focus/input-connection recovery, and document switching without stale state,
crashes, or material memory growth.

Functional success does not waive a performance miss, and performance success
does not waive a visible-frame failure.

## Blockers

- **B0:** data loss, corruption, crash, hang, dead input, or unrecoverable
  source/selection divergence.
- **B1:** a common in-scope action visibly violates the North Star, including
  marker flash, lost style, caret jump, wrong block presentation, duplicate or
  dropped input, or a material performance miss.
- **B2:** bounded roughness that does not break the core editing loop; record it
  for post-D0 work.

D0 requires zero open B0 and B1 issues. Every closed visible blocker has an
actual-paint regression, and every closed platform blocker has a native canary
when the OS route mattered.

## Current status

Current implementation and local evidence are in the
[2026-09-04 review](docs/architecture/v5/implementation_review_2026_09_04.md).
The merged V5 host and workbench pass local functional/paint checks and build on
macOS and Flutter web/Wasm. The September 7 merge passes 494 core, 796 host,
25 workbench, and 25 real-browser tests, plus 307 Tree-sitter Dart tests and
native/WASM parity. See the [merge review](docs/architecture/v5/merge_review_2026_09_07.md)
for source identity and proof boundaries. Attended
native input and the sealed production performance/lifecycle profile remain
open. A resumed foreground sweep completed with five frame-budget failures;
the longer workload lost foreground during sustained typing. After unlocking,
native stack samples identified TextKit work in the full-document input mirror.
The new bounded input context has focused timing and browser evidence, but
foreground changes and accessibility activation prevented a clean native
qualification run. The original B1 remains open pending the complete rerun.

D0-macOS is not yet passed on v5. Kernel and parser receipts are necessary inputs, but
the v4 large-document, controller, paint, and native receipts do not transfer to
the new synchronous surface. The remaining proof includes the complete v5
functional and actual-paint suites, the 32 KiB macOS floor and source-mode
transition receipt, the lifecycle profile, and attended native canaries.
Broader composition, arbitrary cross-owner ranges, deep structural editing, and
mobile qualification remain outside this milestone unless investigation
reveals a B0 or B1 in the supported core loop.

## Stop rule

Dogfood handoff is ready when one candidate has:

1. all focused and full local functional suites green;
2. every required actual-paint case green;
3. zero open B0/B1 issues after architecture review;
4. the attended native macOS canaries green;
5. the sealed performance/lifecycle profile green; and
6. a clean rebuild whose exact binary is the one opened for dogfooding.

New edge cases found only through dogfooding may become post-D0 work when they
are not B0/B1 and the supported core loop remains trustworthy.
