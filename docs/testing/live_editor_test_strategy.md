# Live-editor test strategy

**Status:** canonical portable runner and smoke corpus implemented; native
actuation, device delivery, and standard-depth expansion remain

**Date:** 2026-08-12

**Scope:** v4 `flark_core` and `flark` live editing

## Decision

Flark will use a small number of complementary test systems. No one runner is
expected to prove parser correctness, editor behavior, platform integration,
and performance.

1. Put a behavior at the lowest layer that can prove it.
2. Use ordinary Rust, Dart, and Flutter tests for exhaustive mechanism
   coverage.
3. Use a bounded portable scenario catalog for important multi-step product
   journeys and cross-layer regressions.
4. Run the same catalog at different evidence levels: headless first, then a
   real host application, then simulators and physical devices as those become
   available.
5. Keep performance and physical-input qualification separate from functional
   scenario assertions.

`smoke`, `standard`, and `extended` describe *which scenarios run*.
`headless`, `widget`, `host`, `simulator`, and `physical` describe *where they
run*. These dimensions are deliberately independent.

In this document, `headless` means the existing Flutter-test, no-window
controller path through real Core and Rust. It does not mean a pure Dart unit
test and it does not observe Flutter layout or paint.

The old CGEvent runner remains a useful macOS routing diagnostic, but it is not
the portable contract: it still parses source JSON independently and uses
fixed-pixel hints. The canonical v1 contract is the strict plan compiler, shared
executor, no-window driver, and mounted Flutter-surface driver. Both drivers
consume identical plan objects and barrier semantics; the surface driver adds
render-bound observations emitted by the production render object.

## Current implementation

As of 2026-08-12:

- the closed JSON compiler, canonical plan hash, plan-bundle validation,
  negative schema tests, shared executor, and typed driver boundary are live;
- no-window and mounted Flutter-surface tests run every named plan, with exact
  source/selection/resync/fault checks and bounded actual-paint predicates;
- the eight-scenario smoke set is implemented, plus the original
  paragraph/ordered-list boundary regression;
- a single portable list journey found an inaccurate synthetic platform
  lineage and two real product defects: structural-splice reconciliation for a
  zero-length input window, and list exit that allowed the next paragraph to
  become a CommonMark lazy continuation. Each now has focused lower-layer
  coverage;
- the warm corpus remains cheap enough to grow: the latest debug/test run put
  300 canonical cases through no-window in 3.95 seconds and through the mounted
  Flutter surface in 5.31 seconds.
- thematic-break atom deletion now has one cross-layer portable journey in
  both lanes; parser fixtures own the marker/newline/BOM/nesting matrix, and
  focused widget tests own focus, literal typing, Return, and divider paint.

Still outstanding are native source-offset actuation behind the Dart executor,
artifact/provenance receipts, device-safe plan-bundle delivery, the remaining
standard scenarios, and real platform/device canaries. None of those are
silently credited by the portable green runs.

## What each layer owns

| Layer | Owns | Does not own |
|---|---|---|
| Rust engine tests | GFM/CommonMark behavior, incremental-versus-clean equivalence, UTF/range correctness, bounded work, caps, properties and fuzzing | Flutter layout, native callbacks, gestures |
| ABI and `flark_core` tests | Transaction receipts, anchors, selection, history, queues, failures, stale/full-value/delta/composition observation arbitration | Pixels, hit testing, actual platform routing |
| Flutter widget/render tests | Projection/layout mapping, hit testing, selection geometry, scrolling, virtualization, accessibility semantics, focused goldens | Real OS text services or hardware behavior |
| Portable scenarios | Exact multi-step user outcomes crossing Core, projection, and presentation; important regressions | Parser permutations, raw callback matrices, exhaustive geometry |
| Platform-adapter tests | Native event or gesture maps to the intended portable primitive; lifecycle and platform plumbing | General Markdown semantics |
| Performance receipts | Latency by layer, frame work, memory, allocation, convergence, document shape/size | Functional coverage by scenario count |
| Physical qualification | Real IME, autocorrect, touch handles, menus, clipboard, accessibility, lifecycle, thermal and device performance | Proof supplied by Mac or a simulator |

This keeps failures local. A delimiter-range bug should fail a Rust or Core
test, a caret-rectangle bug a widget test, a macOS event-routing bug an adapter
test, and a complete Return-then-type regression a portable scenario.

## What we mine from v2 and v3

The legacy suites are reference material, not a contract to transplant.

- v2's `InlineSequence` and `LiveRenderSequence` harnesses supply the useful
  interaction vocabulary: activate, type, select, replace, paste, press keys,
  undo/redo, and verify source/display/selection after every step.
- v3 supplies the important authority invariants: only current-revision facts
  may style or accept edits; unsupported or stale regions fail closed; active
  and passive surfaces hand off without changing source, selection, or the
  logical input client; mounted work stays bounded.
- v2/v3 parser matrices, projection algebra, protocol state machines, geometry,
  goldens, and performance cases stay ordinary tests. Encoding them all as JSON
  would add indirection without adding evidence.
- Legacy expectations are candidates, not automatically v4 policy. Smart
  paste, tables, atom deletion, list behavior, and other product choices are
  pinned by the v4 edit contract before becoming portable expectations.

The useful legacy scenario families are inline edits, structural Return and
Backspace, selection replacement, paste and history, projection authority,
active/passive handoff, Unicode, IME observations, virtualization, and
large-document scheduling. We take one representative cross-layer journey per
mechanism rather than every construct permutation.

## Portable scenario contract

Portable scenarios are a product behavior contract, not a universal test
language and not a recording of raw platform callbacks.

### Architecture

- One strict Dart compiler validates a closed JSON schema, resolves source
  anchors, expands named cases, and emits a canonical plan with a SHA-256 hash.
- One shared executor owns operation order, barriers, portable assertions, and
  normalized results.
- Drivers implement a small typed interface: start, activate at a UTF-16 source
  point, insert text, press a named key, set a source selection, paste text,
  await a named barrier, snapshot, and optionally observe paint.
- A native helper is a primitive actuator. It may launch/focus an app, ask the
  app for the screen point corresponding to a source offset, post native input,
  and return observations. It does not interpret high-level scenarios or own
  product assertions.
- Unsupported capabilities are explicit (`supported`, `knownGap`, or
  `notApplicable`). Nothing silently skips or weakens an assertion.

The native driver can be reached by a small request/reply process boundary;
that preserves one Dart executor without building a general RPC framework.

### Closed v1 vocabulary

The initial contract contains only what the first catalog needs:

- initial source, selection, and optional composing range;
- unambiguous source anchors: absolute UTF-16 offset or needle plus an explicit
  occurrence and offset;
- `activate`, `insertText`, `key` (`enter`, `backspace`, `delete`, `undo`,
  `redo`), `selectSourceRange`, and `pasteText`;
- `await(editSettled)` and `await(paintSettled)` as distinct barriers;
- explicit checkpoints for exact source, selection, composing range, authority
  state, pending-edit/fault/resync counters, settled display, and painted-frame
  predicates where supported.

There are no expressions, loops, branching, macros, arbitrary callbacks, raw
platform deltas, runner-owned pixel coordinates, or performance thresholds.
New operations are admitted only when at least two real scenarios need them.

`editSettled` has one meaning across drivers: every injected logical action is
acknowledged, the Core mutation tail is complete, no edit is pending, and the
canonical source and selection are readable. `paintSettled` additionally binds
an actual visible frame to its source revision and visible range.

Wall-clock pauses are allowed only in scenarios explicitly classified as
`stress`. They run with declared repetition and report every outcome; they are
not deterministic conformance evidence.

### Bounded observations and provenance

Paint checks evaluate named predicates online. Receipts store counts, hashes,
frame/revision identifiers, and bounded failure witnesses—not a full document
string for every frame.

Every retained receipt records scenario and plan hashes, commit and dirty
state, app/Core artifact hashes, build mode, platform/toolchain, capabilities,
and PASS/FAIL. Stale or reused binaries are either rejected or marked
non-claim-eligible.

## Initial catalog

The starter catalog is intentionally bounded. Eight scenarios form the
permanent smoke set; standard cases are admitted one by one after they expose a
distinct cross-layer risk. We do not transplant the roughly two dozen useful
legacy candidates because the harness must prove leverage before its vocabulary
grows.

The initial catalog bootstraps the runner; it is not the intended size of the
behavioral acceptance corpus. After the contract is trustworthy, compact parameterized
families may compile into hundreds of named cases. Every named case runs through
both the no-window and Flutter-surface drivers. Model-generated exploration runs
primarily no-window, and every minimized failure is promoted into the named,
universally targetable corpus.

### Smoke: every capable runner

1. `projected-inline-rapid-typing` — type quickly adjacent to and inside
   certified emphasis; source and caret are exact and no raw marker or empty
   surface is painted.
2. `paragraph-split-rapid-successor` — press Return and immediately type the
   successor text; no loss, duplicate, resync, relay-out, or flicker.
3. `paragraph-join-backspace-rapid-successor` — join blocks with Backspace and
   immediately continue typing with the same invariants.
4. `styled-selection-replace-undo-redo` — replace across styled/plain inline
   runs, then restore exact source and selection through history.
5. `cross-block-selection-replace-undo` — replace a source-global selection
   spanning blocks without corrupting delimiters or block ownership.
6. `multiblock-paste-undo-redo` — paste Markdown containing multiple blocks and
   round-trip it as one logical history action.
7. `unicode-grapheme-delete-replace` — emoji ZWJ sequences and decomposed text
   are never split at invalid UTF-16/scalar boundaries.
8. `simple-list-continue-exit-type` — Return continues a simple item, Return on
   an empty item exits, and immediate plain-paragraph typing is preserved.

### Standard candidates

9. `distant-selection-replace-undo` — a source-global selection spanning
   distant blocks remains exact and reversible; widget/host tests additionally
   prove virtualization and cross-page geometry.
10. `inline-syntax-break-recover` — incomplete syntax becomes an exact local
    source island, completing it restores projection, and breaking it again
    never paints an incorrect certified state.
11. `paragraph-list-boundary-newline` — Return at the boundary between ordinary
    paragraph text and an ordered list creates the intended separation and
    preserves immediate successor typing.
12. `ordered-list-renumber-history` — structural continuation and renumbering
    remain exact and reversible across neighboring list items.

Read-only render parity stays a focused Flutter surface test because a
no-window editing driver cannot add independent evidence for UI input rejection.

Next candidates are admitted by shipped behavior: quote continuation/exit,
nested-list outdent, tasks, headings, fenced code, tables, links/images,
distant-reference recertification, active-island handoff, and authority-gap
failure. Composition and autocorrect start as Core/adapter tests plus the
physical IME protocol; they become portable only when two meaningful drivers
can execute the same contract without pretending synthetic callbacks are real
IME evidence.

## Initial platform canaries

Before each macOS dogfood handoff, a small native canary pack proves the
platform mappings that headless execution cannot:

1. wheel/trackpad scrolling does not start or mutate a selection;
2. app-reported source-offset activation receives real typing, Return,
   Backspace, and Delete events;
3. pointer drag selection crosses blocks and replacement reaches the intended
   source range;
4. system clipboard copy, cut, paste, undo, and redo use the real pasteboard and
   shortcut paths;
5. focus loss/regain preserves a usable single input client and does not fault.

The same categories later become touch, clipboard/menu, lifecycle, and
accessibility canaries on mobile. Real composition, autocorrect, dictation,
selection handles, magnifier behavior, VoiceOver/TalkBack UX, thermal behavior,
and performance remain physical-device qualification.

## Execution matrix

The following are initial engineering budgets. They make feedback fast; they
do not relax correctness, and they are revised from measurements rather than by
adding hidden skips.

| Catalog/environment | When | Target budget | Evidence |
|---|---|---:|---|
| Schema and driver conformance | Every fixture/driver change; later every PR | under 30 s | Strict plans, capability honesty, identical primitive/barrier meanings |
| One focused headless scenario | During implementation/debugging | under 2 s | Fastest complete Core/Rust transaction diagnosis |
| Headless smoke (8) | Normal local gate; later every PR | under 15 s after native library exists | Cross-layer semantics without OS routing or paint proof |
| Headless standard | Before dogfood; later every PR | under 30 s initially; cap at 2 min as catalog grows | Full portable semantic contract |
| Targeted widget/render tests | With affected surface code | under 30 s targeted | Geometry, layout, hit testing, semantics |
| macOS host smoke/canaries | Before every dogfood handoff | under 60 s warm execution; under 5 min with cached app setup | Real app, keyboard/pointer/clipboard routing, actual paint predicates |
| macOS host standard | Milestone/nightly or before broad dogfood | under 5 min | Broader real-host integration |
| Extended headless/host | Nightly/milestone, targeted on regressions | declared per workload | Long documents, repetition, unusual structures; not routine churn |
| iOS simulator / Android emulator smoke | When runners exist; packaging changes and pre-device qualification | under 5 min per platform | Packaging, lifecycle, functional adapter preparation—not physical proof |
| Physical automated smoke | Milestone/release on named devices | under 10 min per platform | Real platform integration baseline |
| Physical qualification matrix | Release candidate, OS/keyboard changes | receipt-driven | IME, touch, a11y, lifecycle, thermal, memory and performance |

Until Android/iOS hardware exists, Mac closes only Tier A evidence. Simulator
or emulator results prepare Tier B but cannot close it. Windows later uses the
same catalog/executor boundary plus a thin platform driver and has its own
named-hardware qualification.

Performance remains a separate controlled lane using the versioned document
size/shape matrix in the v4 build plan. Functional scenario cadence may expose
a regression, but it cannot establish p99 latency, frame, memory, or scale
claims.

Every portable scenario runs headlessly. Higher environments select scenarios
only when their capabilities can add evidence; we never run every scenario on
every OS/device/keyboard combination. A higher-tier pass cannot rescue a
headless failure, simulator green is required preparation rather than device
proof, and missing or stale receipts mean unexecuted rather than passed.

## Corpus-throughput feasibility experiment

On 2026-08-12, the canonical shared executor repeatedly ran a synthetic
interaction shape through real Core/Rust transactions and the production
Flutter render surface. Each case opened a fresh document/session, parsed it,
mounted the surface where applicable, activated a source point, inserted text,
issued semantic Return, immediately inserted successor text, settled, asserted
exact source/caret/presentation/fault state, unmounted, and closed the session.

The cases had zero artificial typing delay. The measurements describe warm
runner throughput in a debug/test build, not product latency or heterogeneous
scenario complexity.

| Runner | Cases | Corpus time | Case p50 | Case p95 | Result |
|---|---:|---:|---:|---:|---|
| No-window controller | 25 | 1,187 ms | 14.96 ms | 17.14 ms | PASS |
| No-window controller | 100 | 1,499 ms | 13.68 ms | 16.59 ms | PASS |
| No-window controller | 300 | 3,949 ms | 12.35 ms | 13.87 ms | PASS |
| Mounted Flutter surface | 25 | 678 ms | 18.26 ms | 25.40 ms | PASS |
| Mounted Flutter surface | 100 | 2,225 ms | 17.31 ms | 23.02 ms | PASS |
| Mounted Flutter surface | 300 | 5,311 ms | 15.64 ms | 18.00 ms | PASS |

The original deliberate-cadence regression still took about 0.5 seconds per
case because it sleeps 35 ms between characters. Timing schedules therefore
belong only on cases where cadence is the property under test.

The first 25-case no-window group includes a 781 ms cold-start outlier; its
100- and 300-case groups show steady-state throughput. The experiment supports
a larger universal semantic corpus: hundreds of named cases run through both
portable drivers in seconds when the process stays alive and documents reset
in-process. It does not support launching a new application per case. Native
event canaries therefore reuse one app process and remain a deliberately small
adapter suite.

Consequences:

- every named portable case should run through the no-window and Flutter
  surface drivers;
- generated exploration can remain no-window, with minimized failures promoted
  into the named corpus;
- platform-native event, IME, touch, clipboard, and accessibility canaries stay
  small and replay the relevant named cases rather than the entire generated
  search space;
- native/product runners must reuse one application process and reset sessions
  in-process;
- the experiment is rerun at 25/100/300 after the canonical compiler/executor
  lands and again after the corpus becomes heterogeneous.

Run the experiment with:

```sh
./scripts/benchmark_v4_live_editor_scenario_scaling.sh all 25,100,300
```

## Admission and regression rules

A portable scenario is added only when all of these are true:

1. It protects a user-observable, multi-step or cross-layer invariant, or a
   real frame/event-ordering regression.
2. The no-window controller and at least one product-surface driver can execute
   it without changing the action or assertion meaning.
3. A lowest-layer ordinary test cannot by itself prove the experience.
4. It covers a named mechanism not already represented by another scenario.

The indented-code scenario is one such mechanism: one source of parser-owned
hidden indentation drives both the no-window controller and mounted render
surface through immediate Return/Backspace, exact final source/caret checks,
zero resynchronization, and a painted-prefix exclusion. Tab/BOM/CRLF geometry
stays in the faster Rust matrix because it does not gain evidence by repeating
every lexical permutation through Flutter.

When a real-platform bug is found:

1. capture the smallest raw native event trace;
2. add a focused adapter/Core regression for that trace;
3. add or extend one portable journey only if the failure crossed layers or was
   visible to the user;
4. add a performance case only if measurement identifies a budget regression.

This avoids both failure modes: a portable catalog too small to catch product
breakage and a duplicate end-to-end matrix that is slow and hard to diagnose.

## Implementation sequence

### H0P — Make the portable scenario trustworthy — complete

1. Add the strict compiler/validator, canonical plan, hash, deterministic
   ordering, and negative schema tests.
2. Extract the shared executor and typed driver boundary.
3. Define and prove identical `editSettled` behavior with a
   small adapter-conformance suite.
4. Bind paint observations to the render object's actual frame, revision, and
   visible range; record bounded predicate summaries.
5. Migrate `paragraph-split-rapid-successor`; it must pass no-window and the
   mounted Flutter surface without runner-specific assertion meanings.

The compiler, executor, barriers, render observer, and migrated scenario are
complete. Capability/provenance fields remain intentionally absent rather than
partially implemented; they land with the first native driver that needs them.

### H0N — Make native actuation trustworthy — macOS core slice complete

1. Convert the macOS helper to primitive request/reply actuation behind the
   shared Dart executor; remove high-level product assertion logic from Swift.
   **Complete.** Dart owns canonical plans, scheduling, barriers, and
   assertions; Swift owns process/window and OS-event actuation only.
2. Replace pixel activation with source-offset lookup supplied by the app.
   **Complete.** The Flutter surface resolves a visible source offset to
   app-relative geometry; the actuator converts it to the foreground window.
3. Deliver a validated canonical plan bundle without relying on host filesystem
   paths, so the same integration test can target macOS and later simulators or
   devices.
4. Add capability and normalized provenance receipts, rejecting or explicitly
   labelling stale artifacts.
5. Prove the small native keyboard/pointer/clipboard canary pack; do not replay
   the full semantic matrix through raw platform events. **Core slice
   complete:** one reused profile app process runs both rapid Return schedules,
   Unicode Backspace, source-offset drag selection, system pasteboard
   copy/cut/paste, undo, and redo with exact source/caret, zero resyncs, and
   bounded paint assertions. Wheel scrolling is also green. Explicit focus
   loss/regain remains the next distinct macOS canary; automated focus loss was
   observed, but macOS 14+ refused cross-app reactivation from the CLI harness,
   so no native cross-app focus-regain pass is claimed. The mounted Flutter
   adapter now separately proves ordinary focus loss/regain and inbound
   `onConnectionClosed`: the client acknowledges the closed connection,
   unfocuses, reattaches, republishes exact editing state, and accepts two
   subsequent edits with zero resynchronization. Composition commit on focus
   teardown is owned by the existing headless transaction/history test.

### H1 — Establish the portable smoke set — complete

Add scenarios 1–4, measure diagnostic value and runtime, then add 5–8. Add only
the closed operations those scenarios require. Each scenario must first expose
or protect a specific mechanism; a passing duplicate is not enough reason to
keep it.

### H2 — Establish standard depth — in progress

Add the remaining distinct standard mechanisms, run the full portable set and
the H0N macOS canaries before the next broad dogfood handoff, and review failures
by owning layer. `paragraph-list-boundary-newline` is already live. This is the
first point at which portable depth plus real routing provides a meaningful
product checkpoint rather than only semantic confidence.

Current portable depth also includes ordered-list continuation/history,
depth-three successive outdent, nonuniform ordered-parent container geometry,
task and quote continuation/exit, fenced-code Return/type/history, and
conservative table-cell typing. Pure nested block quotes now add a bounded
continue/outdent/exit journey shared by the no-window and mounted-surface lanes;
exact prefix-width, CRLF, lazy-continuation, and fail-closed permutations stay
in the faster Rust/Core matrices. Those additions found and closed shared
duplicate-action, successor-history, parser-refresh generation, and structural
recertification bugs rather than adding construct-local handlers.

Rendered task-checkbox activation now uses the same portable plan as other
cross-layer behavior. The no-window lane invokes the framework-neutral target
action, the mounted lane observes its receipt-backed projection through
undo/redo, and the macOS actuator resolves the source target to current painted
geometry before sending a real click. A focused editor widget test separately
owns hit-testing and selection preservation, so the portable corpus does not
duplicate pointer mechanics in every runner.

A focused Flutter adapter test additionally pins the cross-device gesture
boundary: touch drag scrolls without changing canonical selection, touch tap
activates an exact painted source position, and the existing mouse gesture test
still reaches the rendered task action. This is host-routing evidence only;
mobile physics, selection controls, IME, menus, and accessibility stay in the
simulator/physical canary matrix.

Keyboard-adapter evidence pins both macOS selector delivery and a logical
hardware-arrow shortcut. The focused cases cover rendered/source delimiter
crossing, extended-grapheme movement, Shift selection, selection collapse, and
paint-geometry vertical movement. Broader word/line/document, bidi, page, and
platform convention matrices are added only with their corresponding product
behavior; they are not multiplied through the portable Markdown corpus.

The read-only widget has one focused host test proving touch scroll over the
shared bounded render object, unchanged source, zero edit traffic, and no
`EditableText`. Editing scenarios are intentionally not replayed through it.

The first bounded accessibility checks assert visible heading and task nodes,
checked-state refresh after a semantic action, the absence of that action in
read-only mode, a 48-pixel touch target, and omission of an offscreen sentinel
from the semantics tree. These widget receipts prevent unbounded semantic
publication; physical screen-reader UX remains device evidence.

Focused command tests now cover a real Meta+A shortcut with exact full-source
selection/readback and macOS line-boundary selectors with Shift extension over
rendered inline Markdown. Existing controller tests retain ownership of large
selection replacement/history; the widget layer tests only routing and
source/display geometry. Word-command coverage uses Flutter's Unicode boundary
policy, checks hidden-delimiter mapping and a real Apple Option-arrow route,
and proves a 600-unit word crosses multiple internal paint fragments without a
false stop. The adapter performs at most one bounded presentation-row layout;
this is not an off-page navigation claim.

Focused touch long-press and real mouse double-tap cases select the visible
content of a styled word and assert that neither source endpoint enters its
hidden delimiters; the long-press case replaces the selection while retaining
the Markdown style. The existing touch-drag case must remain green in the same
suite so the new recognizers cannot regress scroll arbitration. Selection
handles, menus, Apple floating-cursor behavior, and platform-specific
whitespace policy remain simulator/physical evidence rather than additional
portable scenarios.

### H3 — Grow from evidence

Promote shipped construct families from the candidate list as their v4 product
semantics land. Add bounded case matrices and model-generated interaction
sequences; shrink failures and retain the minimized plan as a named regression.
Add simulator/emulator drivers when platform packaging work begins and physical
qualification when hardware is available. Do not build sharding, dashboards, a
generic RPC service, or a broad expression DSL until measured suite cost or
repeated authoring friction proves the need.

## Fixed boundaries

The implementation continues under these boundaries:

- portable JSON expresses user intent and outcome, never raw platform callback
  sequences;
- raw input permutations stay focused Core/adapter tests;
- actual paint assertions cannot fall back to controller snapshots;
- simulator and Mac evidence cannot substitute for physical-device proof;
- performance has its own receipts;
- native/device work extends the driver boundary rather than forking scenario
  semantics or assertions.

References: [RFC 028](../architecture/rfc/rfc_028_source_authoritative_edit_transactions.md),
[v4 build plan](../architecture/v4/build_plan.md), and
[physical IME matrix runbook](ime_device_matrix_runbook.md). The runbook is
currently a v2/reference gate and must be adapted explicitly for v4 before it
is used as v4 qualification evidence.
