# Flark v5 build plan

**Execution contract for [RFC 030](../rfc/rfc_030_synchronous_core.md).**
Revised 2026-09-04 after the architecture and testing review. One milestone is
complete when every exit line has a receipt in
the repo: a test run, a number with commit and device, or a merged change.
Milestones are sequential except where noted. Line budgets are gates.

| Package | Budget (production lines) |
| --- | ---: |
| `native/flark_parse` (on top of comrak) | 3,000 |
| `flark` kernel | 8,000 |
| `flark_flutter` | 10,000 |
| `flark_fleury` | 3,000 |

Cross-cutting rules: no concept without a direct case, a conformance case, or a
named consumer (Dune, Fleury); no performance claim without a receipt; no
test that settles before asserting on the edited frame.

The separate [flark_tree_sitter workstream](../../../packages/flark_tree_sitter/PLAN.md)
owns snippet highlighting and indentation, with a pure Dart API over a shared
Rust engine. Its package/transport checkpoint precedes four-language authoring,
Flark host adoption and twenty-language qualification. It does not close the
editor or platform gates below.

## Current implementation sequence

The 2026-09-07 merge review is qualified locally at the owner's request; CI is
explicitly skipped for this merge. The [merge review](merge_review_2026_09_07.md)
records independent findings, their regressions, and the candidate's local
checks. This exception does not close M2b's CI/performance gate or the native,
physical-device, sustained-frame, and release gates below.

M0 and M1 are complete. M2 remains open. The M2a semantic corrections and the
first Flutter host are implemented and tested locally; M2b named-commit CI and
M3 native/device qualification are not closed. Current evidence and remaining
work are in the [implementation review](implementation_review_2026_09_04.md).
The table below retains the milestone gates, not a claim that later host work
closes the earlier qualification gates.

| Checkpoint | Deliverable | Gate before moving on |
| --- | --- | --- |
| M2a — semantic closeout | Correct common edits and trustworthy direct tests | Supported behavior families pass; regressions demonstrated failing before their fixes |
| M2b — kernel qualification | Reproducible correctness and performance evidence | Final candidate clears local gates, CI, and the desktop kernel budget |
| M3a — first working host | Paragraph/inline Flutter editor, native input and early Flutter-web smoke | Real input, actual paint, recovery, and slice-specific device budgets pass |
| M3b — complete surface | Full block rows, scrolling, matching viewer, qualified admission limits | Complete supported shape set clears the macOS and phone floors |
| M3c — application dogfood | Dune composer and message view on V5 | D0 passes; two weeks of daily use with no open B0/B1 issues |
| M4 — Fleury | Terminal and browser editor/view | Shared semantics, host behavior, and clean installation verified |
| M5 — web qualification | Full Flutter-web editor and published platform limits | Browser/device matrix and packaged transport parity pass |
| M6 — release | Accessibility, lifecycle, packaging, released consumers | Release verification and consumption from the tagged artifact pass |

M4 may overlap M3c after the shared input and editing contracts stabilize. The
Flutter-web smoke in M3a moves integration risk earlier; full web qualification
remains M5. The package direction stays `flark_flutter` / `flark_fleury` →
Flutter-independent `flark` → the same Rust parser through native FFI or Wasm.

## M0 — Spikes · done 2026-09-02

Sourcepos differential, end-to-end keystroke over FFI, marshal, Wasm under
dart2js, iPhone. All passed; results in RFC 030 §14 and `spikes/v5/`.
Fleury native packaging moved into M1.

## M1 — Parse crate · done 2026-09-02

`native/flark_parse` and `packages/flark` (parse transports) on branch
`v5/m1-parse-crate`. Receipts: `cargo test --release` (spec HTML conformance 1,321/1,322 with
example 354 registered; extraction zero deviations plus schema invariants;
fuzz; regressions), `tool/verify_transports.sh` (committed wasm and native
byte-identical, 1,322 cases), `packages/flark` `dart test` through the build
hook, and `tool/verify_prebuilt_consumer.sh` (a fresh Dart app with Rust
removed from PATH builds and parses via a prebuilt). Reviewed on PR #40
(15 confirmed findings, all fixed before merge). Native parse plus
extraction at 25 KB dense: 0.96 ms on the M1 Pro at the head of PR #40; a
300,000-iteration random fuzz over the CRLF-inclusive alphabet is clean.

- Render-model schema as a versioned file: field tables, kinds, attrs,
  invariants. Rust constants and the Dart decoder both derive from it.
- Complete the block vocabulary: per-line prefix ranges for quotes, items,
  footnotes; setext underline and ATX closing sequence as hidden lines;
  fence lines; table delimiter row and cell pipes; task marker range from
  comrak's `symbol_sourcepos`; thematic break; HTML block; link and image
  destination and title ranges.
- The three register corrections: partial tab expansion attr, escaped pipe
  mapping in table cells, inline shift after stripped reference definitions
  using the salvaged v2 scanner.
- Both transports: FFI through the salvaged `hook/build.dart`, wasm32
  through the salvaged build script. Prebuilt binaries fetched by the hook
  so a consumer without Rust builds, which is the deferred Fleury packaging
  spike.
- Fault containment: every panic becomes a typed error; a fuzz target proves
  arbitrary bytes never panic or read out of bounds.

Exit: 652 CommonMark and 670 GFM cases produce byte-identical render models
on FFI and Wasm; the differential reports zero unregistered deviations; a
scaffolded Dart app with no Rust toolchain builds and parses.

## M2 — Kernel · correction pass 2026-09-04

Pure Dart `flark`. Closeout detail: [m2_kernel_plan.md](m2_kernel_plan.md).

- `FlarkDocument`, typed-data model decoder, `FlarkProjection` with hidden
  ranges and bidirectional offset maps.
- Source-offset selection and anchor mapping with the navigation rules from
  RFC 030 §6.
- `FlarkCommand` closed set including formatting toggles, heading level,
  task toggle, indent and outdent, paste.
- Command semantics for every rule in [`edit_profile_v1`](edit_profile_v1.md), as range
  arithmetic over the model.
- History with v4's grouping rules.
- `FlarkEditor` facade with `typingContext` and a sealed live/source snapshot;
  parse backend interface with FFI, Wasm, and a test stub.
- Direct typed editing scenarios with invariants asserted on every step;
  the four rapid sequences from the dogfood milestone as named cases.

Exit: every supported edit-profile rule has a readable direct case; invariants
hold on all cases; deterministic regressions cover every minimized generated
failure; the boundary test proves no Flutter import; the public-surface gate is
green; and insert and Backspace on the pinned 16 KiB-class structural fixture
stay under 4 ms p99 on the M1 Pro through the facade, with a receipt in the repo.

### M2a — semantic closeout · corrections implemented locally

Work in small behavior families: Return around styled content and container
exit followed by typing; partial-owner range edits; then delete-to-empty from
inside and outside formatting. Each begins with the expected product result
and a failing direct regression, followed by a fix at the owning layer.

Repair per-command assertions, expected style/container checks, replacement
checks, and uninterrupted generated histories alongside those cases. Complete
the small supported-case matrix and calibrate the assertions with representative
faults. [The M2 plan](m2_kernel_plan.md#closeout-order) defines the concrete work.

Exit: all four known failure families are corrected, supported commands are
accepted, unsupported edits reject atomically, and immediate follow-up input
and history preserve the intended result. No new Flutter surface is required
to close this checkpoint.

### M2b — kernel qualification

Make the benchmark verify successful mutations and emit a small receipt naming
the exact fixture and shape, source revision/tree, parser artifact, runtime,
machine, and timings. Diagnose the Backspace budget miss without weakening the
4 ms gate or silently replacing the structural workload with easier prose.

Exit: analysis, direct cases, bounded discovery, Rust conformance/fuzz, schema
freshness, native/Wasm parity, and the Rust-free consumer pass on the final
candidate. Record insert and Backspace below 4 ms p99 for the pinned 16 KiB-class
structural fixture, including its exact UTF-8 size; the fixture may exceed the
16 KiB product fallback and is explicitly admitted only by this diagnostic.
Tie the receipt to a named commit and verify CI at that commit before closing
M2. Local green, committed evidence, and CI are reported separately.

### Evidence before closeout

Baseline receipts (branch `v5/m2-kernel`, commit `a7bea47`, Apple M1 Pro): 51
JSON scenarios, the original bounded-surface check, 1,375 production Dart
lines, and a 25 KiB facade measurement of insert 1.50–1.58 ms p50 / 2.2–2.5 ms
p99. These describe that commit only. The correctness review changed the wire
schema, snapshot API, caret legality, projection, history, and hot path, so the
baseline timing and fixture format are not current M2 exit evidence.

Correction-pass verification (2026-09-04 working tree; rerun on a named commit
before calling it an exit receipt):

- 51 directly named typed Dart scenarios (inline 19, structure 18, navigation
  10, history 4), with no serialized command language or replay framework.
- 99 Dart tests and analysis green; 1,000 seeded 40-command discovery sequences
  green with explicit command logs; native Rust tests green; the working-tree
  bundled Wasm and native render models byte-identical across all 1,322 corpus
  cases.
- The recursive boundary test sees 26 concepts per platform, 27 in the union of
  conditional FFI and Wasm exports, against a gate of 27. The render model and
  schema constants remain in `package:flark/render_model.dart`.
- Schema V2 makes extraction deviations a typed fail-closed transport result.
  The editor preserves CRLF exactly, rejects malformed host UTF-16 and bare CR,
  publishes nonmutable live/source snapshots transactionally, keeps grapheme
  and replacement ranges atomic, and bounds history.
- The 16 KiB-class structural fixture (16,750 bytes) measured insert 1.78/2.18 ms
  p50/p99 and Backspace 2.60/3.14 ms. At 65,538 bytes a flat list measured
  14.68/20.94 ms insert and 20.11/25.52 ms Backspace. That run cleared the revised
  small-tier desktop-kernel diagnostic and showed that a byte-only 64 KiB cap
  is not a no-jank envelope; layout, paint, AOT, and floor-device proof remain M3.
- 2,799 production Dart lines in `packages/flark/lib` against the 8,000 budget.

Later review on 2026-09-04 reproduced the M2a semantic failures despite the green
suite. A standard local benchmark on the dirty tree based on `9fcb092`, using
Dart 3.12.2 on the M1 Pro and a 16,694-byte dense fixture, measured insert
3.38 ms p99 and Backspace 5.61 ms p99. This is a diagnostic, not a committed
receipt or proof of a stable regression; it does not clear M2b. The earlier
passing timing and different fixture size cannot close the current milestone.

## M3 — Flutter surface and macOS dogfood

September 5 delivery adjustment: first qualify the existing standalone web/Wasm
workbench for [D0-web](web_dogfood.md) in Codex's embedded browser. This permits
owner feedback without waiting for native foreground availability. The native
and phone requirements below, Dune migration, and sustained application dogfood
remain separate milestones; D0-web does not close them.

### M3a — first working host

- Build paragraph/inline editing through the real input bridge, immutable
  publication, layout, and first paint. Salvage the V4 render surface and input
  client while removing its parser reconciliation and certification machinery.
- Prove composition update/cancel/commit with one logical Undo, duplicate and
  stale callback handling, wrapped-line navigation, pointer selection, focus
  recovery, formatting shortcuts, and visible typing context. Let these cases
  justify small shared API changes rather than adding host-owned Markdown rules.
- Make byte/line admission and post-parse model-count admission kernel-enforced,
  with host-configured limits. Check model counts before building a projection;
  use the same admission path for open, edit, Undo, and Redo. Define observable
  rejection reasons, a source-edit escape, and atomic mode changes that preserve
  source, selection intent, history, focus, and subsequent input.
- Declare a bounded writable source-mode size and latency target separately.
  The live tier's no-jank budget does not imply unlimited source editing.
- Calibrate the input/paint tests with representative faults. Assert every
  accepted command and every actual paint, including a required post-input
  paint. Begin brief attended macOS and phone sessions as soon as the paragraph
  is writable; repeat after input, selection, or composition changes.
- Run the same slice in Flutter web through its packaged Wasm parser and
  dart2wasm/JS-interop loading path: asset loading, input, editing, and Undo.
- Compare the current collapsed style toggle with deferred next-input formatting
  in this small editor. Decide the behavior from user experience and direct
  tests, including cancellation and Undo, before expanding the surface.

Exit: the named slice passes real input and actual-paint cases on macOS and a
floor Android, including CJK composition; the Flutter-web smoke passes in a
real browser. Measure 32 KiB on macOS and 16 KiB on the phone across the slice's
supported shapes, with single-frame budgets and a recorded 32 KiB phone stretch
result. This qualifies only those shapes; it cannot qualify tables or images
that the host does not yet render. If the required floor fails, resolve the
bottleneck or explicitly revisit the product envelope before adding more rows.

### M3b — complete surface and envelope

- Real `Scrollable` over a multi-child layout; row protocol, wrapped selection,
  hit testing, and caret visibility across row boundaries.
- Block rows: text, heading, code with background and highlighting, table
  with borders and cell navigation, image, list and quote shells, thematic
  break, task checkbox. Salvage v2 widgets, theme, popovers.
- Hide-model affordances: formatting shortcuts, typing-context exposure,
  link and image popovers, source mode above the tier with a notice.
- `FlarkMarkdownView` on the same rows.
- Actual-paint cases for each new visible failure mechanism and its transitions;
  continue real-input sessions throughout construction.

Exit: the complete supported shape set, representative Dune documents, and
adversarial boundary presets pass input-to-raster qualification at 32 KiB on
macOS and 16 KiB on the floor phone. Record the 32 KiB phone stretch result and
source-mode size/latency limits. Test open, paste, deletion, Undo/Redo, scrolling,
resize, and sustained input at both byte and shape boundaries. Repeated fallback
on ordinary intended use fails the product gate. Raising a host's limit above
the kernel's 16 KiB default requires its own byte-and-shape receipt.

### M3c — Dune integration and sustained dogfood

Migrate Dune's message view and composer to `flark_flutter`. Pass dogfood
milestone sections 1–5 on one macOS candidate, including native canaries and the
performance/lifecycle receipt, then run two weeks of daily use. Keep testing and
fixing during that period; formal D0 is not the first real-input session.

Exit: Dune runs V5, the daily-use period is complete, no B0/B1 issues remain,
and relevant receipts cover the final candidate. Every fixed issue has a direct
regression at the owning layer and a paint/native case where that adds evidence.

## M4 — Fleury surface

May begin during M3c once shared contracts stabilize. Changes discovered through
Fleury must preserve the same kernel semantics and revalidate the Flutter host.

- `flark_fleury` editor over Fleury's `TextInput` and controller; view.
- Cell rendering with box-drawing shells; Fleury table widget for tables.
- Terminal transport through the M1 hook; browser transport through the
  dart2js loader.
- A Fleury sample app and direct host cases for shared editing behavior, cell
  geometry, selection, input delivery, and history. The kernel suite remains
  shared; do not build a universal cross-host replay driver.

Exit: shared semantic cases and Fleury-specific input/rendering cases pass; browser and
terminal receipts; the sample app installs from a clean machine.

## M5 — Flutter web and the envelope

- Extend M3a's Flutter-web smoke to the complete editor: dart2wasm build,
  js_interop loader, packaged assets, and the demo site on V5.
- Parity run: render models byte-identical between native and the Flutter
  web build.
- Receipts on desktop Chrome, mobile Safari, an older iPhone, and the qualified
  mid-range Android device.

Exit: envelope limits published from receipts, not from the RFC's
provisional numbers.

## M6 — Hardening and release

- Mobile selection handles and magnifier, or an explicit deferral with the
  platform fallback documented.
- Accessibility pass with VoiceOver and TalkBack.
- Clipboard, dictation, and composition qualification on devices.
- Reveal-at-caret toggle. Fence creation has been pulled forward into the
  current D0-web authoring gate after owner dogfooding; see the
  [implemented fence follow-up](fence_authoring_review_2026_09_05.md). Code selection,
  syntax coloring, manual language choice, indentation and typed-closer outdent are also implemented
  in the [code editing follow-up](code_closer_review_2026_09_06.md).
- CI gates: conformance on both transports, kernel scenarios, actual paint,
  receipts, binary hosting release workflow.
- Repo: land on main, archive `codex/*` branches and stale worktrees, move
  v2 to `legacy/` once Dune is migrated, prune docs to the four active ones
  plus this plan, CHANGELOG and README.

Exit: `verify_release.sh` green; a tagged release consumed by Dune and
Fleury from the released artifact.

## Testing approach

The [testing strategy](../../testing/live_editor_test_strategy.md) owns the
method. The product contract defines expected outcomes; a clean parse or a
consistent projection alone cannot establish correct command semantics. V4
already included immediate-paint tests. V5's smaller state space helps, but
confidence requires demonstrated fault detection and early real use.

M2a makes the supported behavior families executable and calibrates the kernel
assertions. M3a calibrates input/paint observation and begins real-device use.
M3b–M5 extend coverage only where new blocks, routes, or platforms add risk.
Each checkpoint reports separately what was implemented, tested locally,
verified by CI, and exercised on devices. Missing or skipped required checks
are outstanding work, never passing evidence.

When another variant escapes a purported family-level fix, pause expansion in
that area. Identify the general rule, why detection failed, and the one owning
layer for the correction before adding another exception. A new test framework
or more isolated examples cannot substitute for that review.

## Later, only with a named consumer

Windows. Any collaborative or provenance hook. Asynchronous live rendering is
not part of V5; it would require a separate RFC and must not be enabled merely
by crossing the source-mode limit.

## Sizing

The two weeks of daily dogfood are calendar time. Estimate remaining engineering
work after M2b and again after M3a, using the corrected kernel and real-input
results. Historical code-production pace is not an estimate of device
qualification or usability work.
