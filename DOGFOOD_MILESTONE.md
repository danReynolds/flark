# D0: Ready for macOS dogfood

**Product bar:** [NORTH_STAR.md](NORTH_STAR.md)

**Editing behavior:** [edit_profile_v1.md](docs/architecture/v4/contracts/edit_profile_v1.md)

**Testing approach:** [live_editor_test_strategy.md](docs/testing/live_editor_test_strategy.md)

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
- incremental results matching a clean parse.

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

## 4. Large documents

The candidate opens and edits the checked-in product tour plus the 1 MiB, 5 MiB,
10 MiB, giant-line, and dense-block presets. Tests cover local editing, paging,
scroll-away/return, resize, Undo, and sustained input while offscreen parsing is
active.

The visible viewport must remain semantically correct and writable. Document
size may not cause unbounded foreground work or require a complete document-wide
render model before useful editing begins.

## 5. Mac performance and lifecycle proof

The production-path profile must pass the checked-in frame-latency, memory,
opening, parser-work, paging, resize, and sustained-input budgets on the
benchmark Mac. Measurements identify the exact commit, tree, app executable,
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

D0 is not yet passed. The final-styled-grapheme deletion family—including the
reported italic Backspace case, forward Delete, delete-then-type, history, and
every-frame marker exposure—is implemented and covered across native, Core,
controller, and mounted-surface tests. The full local Core, controller, and
mounted-paint suites pass, including the headless 1 MiB/5 MiB navigation and
dense-block cases. The whole-candidate architecture review and separate 5 MiB
dense certification stress pass. The remaining proof is the clean-candidate
profile/lifecycle matrix and attended native canaries.
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
