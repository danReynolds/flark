# Flark v4 build plan

**Execution contract for
[RFC 026](../rfc/rfc_026_flark_v4_product_architecture.md).** 2026-08-08.

This plan builds a headless Dart `flark_core` over the selected incremental
Rust engine, then builds the Flutter product `flark` on top. The first proof and
all initial performance work run on the available Mac. Android and iOS claims
wait for physical devices; Windows follows later.

## 1. Destination and current state

The destination is fixed:

```text
flark (Flutter)
  -> flark_core (Dart, no Flutter)
       -> flark-abi
            -> flark-runtime
                 -> flark-parser + flark-engine
```

The migration starts from different package names and a broader legacy bridge:

| Current | Destination | Treatment |
| --- | --- | --- |
| Rust `flark-engine` | Rust `flark-engine` | Keep |
| Rust `flark-parser` | Rust `flark-parser` | Keep and complete |
| Rust `flark_comrak_bridge` | `flark-runtime` + `flark-abi` | Replace after parity |
| Dart `flark` | Dart `flark_core` | Rename mechanically after baseline |
| Flutter `flark_flutter` | Flutter `flark` | Rename mechanically after Dart |

After the M0 baseline, the package identities change first because all three
candidate pub.dev names returned not-found on 2026-08-08 and the project has no
hosted compatibility promise to preserve. Runtime work, each package rename,
legacy deletion, and directory moves remain separate reviewable checkpoints.

### Starting receipts that must remain honest

- CommonMark structural admission: 652/652.
- Semantic replay: 384 exact, 262 typed missing, 6 divergent.
- Selected GFM profile: not yet covered by one complete executable lane.
- Live projection: selected behavior exists in prototypes, but there is no
  complete versioned matrix covering incomplete syntax, marker transitions,
  selection, edit histories, and certification states through the final path.
- Incremental/locality engine: selected and retained.
- Current-revision range certification: incomplete; whole-document pending is
  still used.
- 32 KiB paste: can silently fail to converge and is a release blocker.
- Custom surface: selected, but not yet proven through the final direct engine
  path.

Fixture admission, GFM semantic conformance, incremental edit coverage, and
live-projection/product behavior are separate ledgers. A total in one ledger
must never be presented as a total in another.

## 2. Rules of execution

1. **One engine.** Whole reparse is a benchmark control only. No fallback,
   backend selection, or document-size switch enters production.
2. **One source.** Rust owns canonical source. Dart and Flutter may cache
   bounded views, never a second authoritative document.
3. **One grammar.** Markdown decisions and certification stay in Rust.
4. **Bound everything synchronous.** Edit admission, pump, query, conversion,
   cleanup, layout, and paint need explicit units and caps.
5. **Fail visibly and specifically.** A stalled progress token, uncategorized
   status, stale semantic range, over-cap result, or leaked handle fails a gate.
6. **Measure the product path.** Parser microbenchmarks diagnose; only
   input-to-paint editor receipts support editor performance claims.
7. **Keep commits logical.** Do not mix runtime behavior, public renames,
   filesystem moves, generated artifacts, or broad cleanup.
8. **Stop at gates.** A milestone advances on executable receipts and review,
   not a plausible status summary.

The legacy and direct boundaries may coexist only as a pre-release migration
scaffold between M2 and M5. The legacy path is baseline/test-only for the new
v4 work; the M4 surface has no runtime selector and uses the direct path only.
The renamed packages MUST NOT be published until M5 makes the direct path the
sole public/default reachability and removes the scaffold. M5 is necessary but
not sufficient for release: first public product publication also requires the
M6 Mac product/conformance checkpoint and an explicit platform-support scope.

Every milestone writes a checked-in receipt containing the commit SHA,
hardware/runtime/toolchain, exact commands, fixture hashes, sample counts,
predeclared thresholds, observed values, and PASS/FAIL. A narrative summary is
not a receipt. A failed milestone reverts only its own commits; it never leaves
a second runtime strategy in production.

## 3. Evidence matrix used from M0 onward

Every performance checkpoint uses versioned fixtures and records machine,
build mode, engine revision, Flutter revision, display refresh rate, warmup, and
sample count.

M0 assigns exact fixture/case IDs to each milestone. M4 uses a minimum
architecture subset across all size tiers; M6 runs the complete shape matrix
after grammar and editor behavior exist. “Matrix” never means an unstated or
post-hoc subset.

Evidence is tiered and never substituted upward:

- **Tier A — Mac:** architecture, headless Dart, Flutter product behavior, and
  Mac performance on the named machine;
- **Tier B — Android/iOS:** physical-device input, touch, lifecycle, thermal,
  memory, and performance certification;
- **Tier C — Windows:** Windows packaging, input, accessibility, lifecycle, and
  named-hardware performance certification.

Passing Tier A authorizes Mac claims only. Passing one Tier B platform does not
qualify the other, and no simulator result closes Tier B.

### Sizes

- 1 KiB: interactive floor and fixed-overhead detector;
- 25 KiB: ordinary product document;
- 100 KiB: large ordinary-document tier;
- 1 MiB: first scale waypoint, not a ceiling;
- 2 MiB, 5 MiB, and 10 MiB: editor scale tiers;
- the best comparable competitor boundary and the next larger meaningful tier;
- at least four times the selected editor envelope, engine-only: hidden
  document-sized work detector.

### Shapes

- ordinary prose;
- Markdown/delimiter-dense prose;
- one giant paragraph and one giant physical line;
- many tiny blocks;
- nested lists and block quotes;
- GFM tables and task lists;
- many references plus edits to a referenced definition;
- an unclosed fence/container extending to EOF;
- sustained typing, deletion, streaming append, undo/redo, and 32 KiB paste.

### Recorded metrics

- source-visibility and input-to-paint latency;
- p50, p90, p99, and maximum foreground time by layer;
- longest synchronous span, build/raster timing, and missed frames;
- total and per-frame FFI calls and returned bytes;
- work units to certification, convergence latency, and uncertified
  character-frames;
- allocations, peak/retained memory, document-close and process-reopen state.

The provisional Mac targets from RFC 026 are development gates: accepted
source/caret/selection visible by the next frame with no input backlog older
than one frame; engine p99 at or below 4 ms; Flutter frame workload at or below
8 ms p99 as stretch headroom; no editor-attributed frame or synchronous span
reaching the hard 16 ms budget; zero editor-attributed dropped frames; and, at
the selected multi-MiB editor envelope, exact editable viewport paint below
200 ms and visible projection certification below 500 ms. Results from this Mac
cannot close a mobile gate. Actual frame misses are evaluated at the named
display mode.

## 4. Milestones

### M0 — Freeze the decision, contracts, and baseline

**Status:** active.

Purpose: make the selected architecture falsifiable before replacing the
boundary.

Work:

- [x] Record the product/package architecture in RFC 026.
- [x] Make RFC 024 and RFC 025 historical evidence rather than competing
  execution plans.
- [ ] Record a clean baseline for the Rust workspace, Dart public boundary,
  Flutter tests, native packaging, archive consumers, conformance ledgers, and
  a profile-mode build/run artifact of the current Mac application.
- [ ] Check in a rename manifest covering package URIs, public barrels,
  pubspec/override/lock files, examples, build scripts, archive consumers,
  generated metadata, and hard-coded package asset URLs. Classify logical
  package/library/asset names separately from unchanged physical repository
  paths and historical evidence that must not be rewritten.
- [x] Check the pub.dev package API for `flark`, `flark_core`, and
  `flark_flutter`; all returned not-found on 2026-08-08. Recheck before the
  first M1 commit and before first publication, recording endpoint, timestamp,
  status, and response. A not-found result is evidence, not name reservation.
- [ ] Version the workload matrix and result schema above.
- [ ] Select and record a leading relevant editor cohort. On the same Mac, run
  comparable source-fidelity, open, fast-typing, and scale workloads; record
  each competitor's largest passing envelope and fidelity differences. Flark's
  minimum Mac envelope is at least the best comparable result, with the next
  larger tier retained as the stretch target.
- [ ] Freeze Tier A thresholds and provisional Tier B minimum thresholds before
  implementation, including p99 and maximum synchronous/frame spans, maximum
  missed-frame rate, cold paint, pump/wall-time convergence, uncertified
  character-frames, memory, fast-input backlog, and the competitor-derived
  multi-MiB envelope. Changing a threshold later requires an explicit RFC
  amendment and a fresh run; it cannot turn a failed run green.
- [ ] Pin the CommonMark version and selected GFM profile, including an explicit
  deviation policy and separate semantic/incremental ledgers.
- [ ] Version the separate live-projection matrix: incomplete syntax,
  reveal/hide behavior, caret/selection states, edit histories, neutral pending
  output, current certification, and transitions between them.
- [ ] Specify the direct runtime contract: revisions, transactions, progress
  tokens, budgets, certification, anchors, coordinate types, source reads,
  query caps, small-edit limit, staged bulk admission, faults, ownership,
  concurrency, snapshot continuations, bounded reversible-edit tokens/history
  bytes, and resumable close.
- [ ] Pin valid UTF-8/no-normalization behavior, invalid host-input rejection,
  exact line-ending preservation, and the Unicode grapheme version/library.
- [ ] Specify the bounded input-window state machine separately: represented
  source range, composition rules, window movement, cross-boundary edits,
  resynchronization, and oversized selection behavior.
- [ ] Add failing regression fixtures for the 32 KiB stall, current-revision
  range certification, giant paragraph/line, and every ambiguous status code.

Exit evidence:

- baseline commands and immutable result artifacts are checked in or linked;
- contract tests compile against stubs and name every terminal state;
- performance results contain the full provenance schema;
- the four ledgers report their own denominators;
- no public performance or full-GFM claim exceeds those receipts.

**Review checkpoint:** approve the runtime/ABI and input-window contracts before
the M1 identity change and M2 implementation grow around them.

### M1 — Establish the final package identities

Purpose: make subsequent implementation land in the product structure the user
will actually consume. This milestone changes names and ownership declarations,
not runtime behavior.

#### M1A — Headless Dart rename

- [ ] Rename the root Dart package from `flark` to `flark_core`.
- [ ] Preserve `package:flark_core/flark_core.dart` as the existing narrow core
  barrel and map the former supported barrel to
  `package:flark_core/flark.dart`; export-set consolidation belongs to M3.
- [ ] Update Dart imports, tests, examples, build hooks, archive consumers,
  metadata, docs, and hosted-package keys mechanically.
- [ ] Update every dependent, including the still-named `flark_flutter`, to
  depend on/import `flark_core` so the repository is green at this commit.
- [ ] Preserve the existing export sets of `lib/flark.dart` and
  `lib/flark_core.dart`; changing the final `flark_core.dart` API belongs to M3,
  not the identity commit.
- [ ] Assert that `flark_core` has no Flutter SDK dependency or Flutter import.

M1A exits when the analyzer, Dart tests, the still-named `flark_flutter`
package, example, native build hooks, and an immutable archive-backed headless
consumer all pass using only the `flark_core` identity.

#### M1B — Flutter product rename

- [ ] Rename the Flutter package from `flark_flutter` to `flark`.
- [ ] Add a behavior-free `package:flark/flark.dart` forwarding barrel while
  retaining the old barrel for migration verification, and keep the dependency
  on `flark_core` explicit.
- [ ] Update Flutter imports, tests, example applications, scripts, assets,
  metadata, docs, and hosted-package keys mechanically.
- [ ] Assert that production Dart imports in Flutter reach engine APIs through
  `flark_core`, with no accidental self-import after `flark` takes the product
  name.

M1B exits when Flutter analyze/test/build, an archive-backed macOS Flutter
build/launch smoke, and an immutable product consumer pass with **only** a
direct `flark` dependency/import. Its generated package config must contain
`flark_core` transitively. The full root/nested/example suites pass at the clean
committed SHA without dirty-checkout warning suppression.

Constraints:

- M1A and M1B are separate green commits.
- Neither commit changes runtime behavior, parser behavior, ABI, or filesystem
  package-directory layout. Required public-barrel filename changes are part of
  the mechanical rename; broader directory moves are not.
- Existing Flutter-packaged Web/worker artifacts may remain as an explicitly
  inventoried legacy exception so the rename is behavior-free. No v4 code may
  adopt that ownership, and M5 removes the exception with the old runtime.
- M1A runs a standalone archive-backed `flark_core` browser-runtime migration
  receipt. Negative assertions require its legacy default asset paths to use
  `/packages/flark_core/` and never silently resolve duplicate assets from the
  newly named Flutter `/packages/flark/` package. This protects migration
  integrity only; Web is not a v4 product target.
- Exact M1 scans require: zero active `package:flark_flutter/` imports; zero
  core-source imports of `package:flark/`; Flutter production engine imports
  pointing to `package:flark_core/`; the exact root `flark_core`, nested
  `flark`, and nested dependency `flark_core` pubspec graph; and zero stale
  logical asset namespaces outside an explicit allowlist for unchanged
  physical paths and historical records.
- If one fails, revert that commit only; do not build v4 under a half-migrated
  identity.
- Recheck package-registry availability immediately before first publication.

**Review checkpoint:** inspect the two mechanical diffs separately and verify
their archive consumers before runtime implementation begins.

### M2 — Build the host-neutral Rust runtime and thin ABI

Purpose: turn the good incremental engine into a direct, bounded product core
without Dart, Flutter, or legacy endpoint concepts.

Work:

- [ ] Add `flark-runtime` beside `flark-engine` and `flark-parser`.
- [ ] Give one runtime session exclusive ownership of source, revision,
  incremental parser state, certification state, anchors, and progress.
- [ ] Implement revision-checked atomic edits, capped source reads, bounded
  pump, capped viewport queries, anchor operations, and explicit coordinate
  conversion.
- [ ] Implement the declared inline-edit maximum plus staged chunked bulk
  transactions whose commit alone changes revision/source authority.
- [ ] Return opaque reversible transaction tokens backed by a bounded Rust
  history-payload store; implement exact token replay and typed eviction.
- [ ] Implement true requested-range current-revision certification. Pending
  ranges return exact neutral source, not mapped-forward semantics.
- [ ] Fix the 32 KiB paste stall and make every quiescent/terminal outcome
  discriminated and observable.
- [ ] Add `flark-abi` as a small C-compatible native seam over the runtime.
- [ ] Add explicit ABI version/capability negotiation without freezing private
  parser record layouts or inventing second-language SDK abstractions.
- [ ] Retain generation-checked handles, panic containment, fixed-width
  records, explicit ownership, caller buffers, and hard result caps.
- [ ] Add a tiny C harness that opens, edits, pumps, queries, reads source, and
  closes without Dart assumptions.
- [ ] Make close and large buffer reclamation resumable; no document-sized
  destructor may execute on a foreground call.
- [ ] Audit dependencies/imports for OS, Dart, and Flutter leakage and compile
  the runtime/ABI for at least one iOS and one Android Rust target. The C
  harness is macOS execution evidence; target compiles are portability smoke,
  not device qualification.
- [ ] Keep the legacy bridge live only as a comparison path until parity is
  recorded.

Exit evidence:

- clean parse and incremental results agree for every exact M0 semantic fixture
  ID, with no denominator narrowing or regression;
- a requested viewport becomes independently certified for its current
  revision without requiring whole-document publication;
- the exact M0 large-edit case IDs, including 32 KiB paste, preserve source and
  converge within their predeclared pump/wall-time limits;
- no single ABI call can return unbounded data or perform unbounded work;
- cancellation, supersession, stale revision, cap exhaustion, panic, and close
  have distinct test-covered outcomes;
- invalid UTF-8 is rejected before commit; valid Unicode bytes, normalization
  form, and LF/CR/CRLF line endings round-trip unchanged;
- hard work/latency caps cover small edit, bulk staging/commit/abort, history
  replay/eviction, result release, cancellation, and every close pump;
- repeated open/edit/close and fault injection return exactly zero live
  documents, transactions, continuations, and handles; allocator/RSS variance
  is measured against a separate predeclared tolerance;
- the C harness and Rust benchmark matrix pass in release mode on macOS.

**Review checkpoint:** inspect the ABI surface and raw performance receipts. Do
not start the Dart binding or Flutter rewrite while Rust lifecycle, liveness,
or certification is still ambiguous.

### M3 — Build the direct headless Dart core

Purpose: prove the general Dart product independently from Flutter.

Implement this direct path inside the now-final `flark_core` package. Keep the
new API explicitly preview-scoped until its M3 review, but do not create a
second package or compatibility parser path.

Work:

- [ ] Consolidate the reviewed headless public surface at
  `package:flark_core/flark_core.dart`; explicitly remove or deprecate the
  transitional `package:flark_core/flark.dart` barrel before publication.
- [ ] Generate or hand-maintain private raw bindings to `flark-abi` behind a
  narrow reviewed boundary.
- [ ] Add safe Dart lifecycle, typed revisions/ranges/anchors/budgets/statuses,
  bounded values, source reads, and deterministic disposal.
- [ ] Expose schedule-neutral apply/pump/query operations; add no Flutter
  scheduler or generic executor framework.
- [ ] Provide explicit source-byte/UTF-16 conversion and reject accidental
  coordinate-space mixing. Rust performs conversion at a named revision; Dart
  exposes typed wrappers and validates the result rather than reimplementing
  canonical mapping.
- [ ] Implement canonical selection and grapheme policy plus history ordering/
  grouping over Rust opaque transaction tokens, without Markdown
  interpretation, inverse-text retention, or a full-source replica.
- [ ] Build a CLI/archive consumer that opens a document, edits, converges,
  queries a viewport, exports exact source, and closes.
- [ ] Run native JIT and AOT/profile paths on this Mac.

Exit evidence:

- the headless consumer depends on Dart only and imports no Flutter library;
- exact source and current-revision semantics match direct Rust oracle results;
- unpaired Dart surrogates fail before commit, and pinned Unicode/grapheme
  fixtures agree across Dart and Rust position wrappers;
- the direct path has no endpoint packet, JSON/wire graph, host-side parser, or
  authoritative Dart source copy;
- bounded-query and lifecycle tests pass under repeated edit/open/close loops;
- large replacement/undo/redo is byte-exact without retaining the deleted
  document payload in Dart;
- profiler receipts show coarse calls and capped data, not per-node FFI chatter;
- the M0 performance matrix passes through Dart with attributed overhead.

**Review checkpoint:** approve the provisional `flark_core` API shape before it
becomes the foundation of the Flutter surface.

### M4 — Prove the real Flutter surface on macOS

Purpose: build the smallest complete product-shaped path and measure it before
adding editor breadth.

Build the new surface inside the now-final `flark` package, consuming only the
direct Dart API from M3.

Work:

- [ ] Connect one custom own-painted document surface to one real Rust session.
- [ ] Add the frame scheduler with explicit mutation, pump, query, layout, and
  paint budgets.
- [ ] Implement minimum viewport-first open: exact visible source can paint
  before whole-document semantic convergence.
- [ ] Virtualize blocks/fragments and assert that offscreen layout is not built.
- [ ] Fragment oversized paragraphs/physical lines so a single visible block
  cannot force document-width or full-block layout on the frame path.
- [ ] Implement Mac keyboard, mouse, caret, source-anchored selection, basic
  clipboard, and bounded platform input.
- [ ] Paint only revision-matched certified structure; paint exact neutral
  source for pending ranges.
- [ ] Hide certified syntax markers except around the active edit context.
- [ ] Instrument the entire platform-edit-to-raster path in a profile app.
- [ ] Add product-shaped visual fixtures and inspect live scrolling, typing,
  selection, long lines, pending-to-certified transitions, and theme variants.

Exit evidence:

- typing edits Rust-owned source and save/export returns the exact expected
  bytes;
- accepted source, caret, and selection appear by the next frame with no input
  backlog older than one frame while non-local structure may converge over
  later bounded pumps using neutral exact-source fallback;
- stale semantics never flash after delimiter, fence, container, or reference
  edits;
- giant paragraph/line and many-block cases remain virtualized and bounded;
- the exact M4 case IDs at 1 KiB, 25 KiB, 100 KiB, 1 MiB, 2 MiB, 5 MiB,
  10 MiB, and any larger competitor-derived boundary meet the fast-typing,
  warmed edit, input-backlog, hard 16 ms frame/span, cold exact-source paint,
  and visible-projection certification gates in a profile-mode build/run
  artifact; on failure M4 fails, work stops, and any amended architecture or
  threshold requires a fresh contract and rerun;
- functional and visual editor acceptance tests use the real engine, not plain
  string stand-ins;
- the old v3 surface is not on this new product path.

**Review checkpoint:** live product review plus raw frame trace review. This is
the first point at which “jankless on this Mac” may be said, scoped to the
recorded fixtures and build.

### M5 — Remove the legacy boundary

Purpose: delete the superseded integration only after the direct Rust, Dart,
and Flutter path has independently earned replacement.

- [ ] Prove direct-path parity for the exact M0 source/semantic fixture IDs,
  lifecycle cases, packaging cases, and M4 surface cases; denominators may not
  shrink.
- [ ] Produce a capability-delta ledger for every frozen v2/v3 public behavior
  and fixture: carried forward, deliberately replaced, explicitly deferred, or
  intentionally dropped with rationale. No capability disappears through an
  unclassified test deletion.
- [ ] Remove endpoint/wire/publication/host-store runtime paths, Dart source
  replicas, worker/parser replicas, and duplicated artifacts that the direct
  path replaces.
- [ ] Keep `flark-engine` and `flark-parser`; do not delete working incremental
  machinery merely because it was hosted by the old root crate.
- [ ] Decide v2/v3 public API removal/deprecation explicitly rather than as
  incidental cleanup.

M5 exits when the capability-delta ledger has no unclassified row, both
immutable archive consumers pass, a zero-production-reachability scan proves
the legacy boundary cannot be selected or imported, and the deleted path has
no production reachability. The checked-in M5 receipt includes the deletion
diff, commands, scan allowlist, archive hashes, and capability ledger.

On failure, M4 remains the direct preview and the legacy baseline remains
test-only; neither renamed package may be published.

Any filesystem reorganization remains a later move-only checkpoint. It is not
part of legacy runtime deletion.

**Review checkpoint:** inspect the deletion receipt separately from all prior
identity and runtime changes.

### M6 — Complete the Mac product and selected GFM profile

After the vertical architecture passes, three lanes may proceed in parallel but
must close together for the Mac product checkpoint.

#### M6A — Grammar and incrementality

- [ ] Make the selected GFM profile executable and versioned.
- [ ] Close semantic CommonMark cases and GFM extensions using parser-owned
  logic only.
- [ ] Add edit histories for every construct: type, erase, split, merge, paste,
  incomplete syntax, and non-local dependency changes.
- [ ] Keep clean/incremental oracle parity and locality/resumability receipts
  separate from static conformance.
- [ ] Reach zero CommonMark semantic divergence and pass every assertion in the
  pinned GFM profile. Intentional exclusions are versioned out-of-profile cases,
  never explained failures counted inside a completed denominator.

Full conformance means semantic behavior against the pinned profile, not
652/652 structural admission.

#### M6B — Editor behavior

- [ ] Finish selection, multi-block replacement, clipboard, undo/redo, command
  routing, and source-anchored history.
- [ ] Harden composition, autocorrect, dead keys, dictation events, and input
  window resynchronization where macOS can exercise them.
- [ ] Cover grapheme deletion, emoji/ZWJ, combining marks, bidi, affinity,
  long lines, text scaling, and font fallback.
- [ ] Add link/media actions, tables/task interactions, marker reveal rules,
  keyboard navigation, focus, shortcuts, themes, and read-only behavior.
- [ ] Implement semantics and accessibility with bounded viewport exposure.
- [ ] Require visual inspection of the moving surface; widget/golden tests are
  regression evidence, not a substitute.

#### M6C — Scale and cold path

- [ ] Harden viewport-first open and oversized-fragment virtualization across
  the complete M6 workload matrix.
- [ ] Pass the selected multi-MiB envelope at or beyond the best comparable Mac
  competitor result, and report the next larger tier honestly as PASS or FAIL.
- [ ] Eliminate remaining document-sized foreground work in reference
  resolution, queries, conversion, destruction, layout, and paint.
- [ ] Enforce memory and allocation budgets for the declared envelope.
- [ ] Turn the versioned Mac workload matrix into a permanent CI/performance
  lane with noise policy and saved traces.
- [ ] Define explicit visible degradation beyond the verified envelope.

M6A writes the denominator-exact conformance/incremental receipt; M6B writes the
editor acceptance plus visual/live-inspection receipt; M6C writes the full
profile-mode performance, memory, and degradation receipt. M6 exits only when
all three lane receipts and one real-product integration receipt pass without
weakening the evidence contract.

On failure, the last green lane commits may remain behind unavailable preview
APIs, but the Mac product checkpoint and all full-GFM/scale claims remain
failed until the complete integration reruns green.

**Review checkpoint:** approve Mac product behavior and the exact scope of every
support/performance statement before device qualification.

### M7 — Qualify Android and iOS on physical devices

This milestone begins when representative hardware is available. Simulators
may prepare functional coverage but cannot pass performance or physical-input
gates.

Work:

- [ ] Choose and record a release floor device and a current device for each
  platform.
- [ ] Before Flark qualification, freeze the comparable competitor cohort and
  measured multi-MiB minimum for that platform/device; do not derive the target
  from Flark's result.
- [ ] Package the native Rust artifacts for all required architectures.
- [ ] Run the identical versioned performance matrix and save raw traces.
- [ ] Run physical keyboard, software keyboard, composition, autocorrect,
  predictive text, dictation, paste, selection handle, toolbar, magnifier,
  gesture, app lifecycle, and accessibility matrices.
- [ ] Measure thermal behavior, memory pressure, long sessions, background/
  foreground transitions, and repeated document lifecycle.
- [ ] If evidence requires a scheduling, input, ABI, or surface architecture
  change, fail M7, amend the owning contract, reopen and rerun affected M2–M6
  gates on Mac, then begin a fresh device qualification. Do not optimize inside
  a supposedly completed qualification run.
- [ ] State the public document/performance envelope only from passing named
  devices.

Exit evidence:

- all correctness, IME, touch, accessibility, lifecycle, memory, and frame
  gates pass on the named devices;
- the predeclared competitor-derived multi-MiB minimum and all Tier B
  thresholds pass for that platform; a smaller envelope is an explicit
  RFC/product-scope amendment followed by a fresh run, never an automatic PASS
  for the failed run;
- no Mac or simulator result is presented as device evidence.

Android and iOS each write a separate named-device receipt with raw traces,
commands, fixture hashes, OS/toolchain versions, input/interaction recordings,
and PASS/FAIL, followed by one cross-platform integration receipt. On failure,
the Mac product remains green and the failing mobile platform remains
explicitly unsupported.

### M8 — Windows qualification

Windows is an eventual target after M7, not a constraint to pre-build generic
abstractions now.

- [ ] Add and verify Rust DLL packaging/loading and CI.
- [ ] Cover Windows IME, keyboard/shortcut conventions, pointer selection,
  clipboard, accessibility, fonts, scaling, and window lifecycle.
- [ ] Run the same correctness and performance matrix on named hardware.
- [ ] Add platform code only where actual Windows behavior differs.

Before measurement, check in a versioned Windows gate table covering install,
native loading, conformance/source fidelity, input/IME, accessibility,
lifecycle, the exact workload IDs, memory, p99/max spans, frames, cold paint,
and convergence. M8 exits only when that table and its named-hardware raw-trace
receipt are entirely green. On failure, Windows remains explicitly unsupported
and existing platform contracts do not weaken.

## 5. Permanent release gates

After their milestone introduces them, these stay green:

- semantic CommonMark and selected GFM conformance;
- incremental edit-history oracle parity and convergence;
- revision/range certification and stale-result rejection;
- no-silent-stop progress and typed terminal states;
- ABI caps, handle ownership, panic/fault containment, exactly-zero live state
  after close, and separately bounded allocator/RSS variance;
- valid UTF-8/no-normalization/source-fidelity and typed coordinate contracts;
- bounded input-window, bulk-edit, history-token, continuation, and close state
  machines;
- `flark_core` has no Flutter dependency and no authoritative/full-source Dart
  replica;
- `flark` reaches Rust only through public `flark_core` APIs, apart from
  allowlisted platform packaging metadata;
- zero Markdown grammar scanner or semantic command implementation outside
  Rust;
- exact save/export bytes and large edit/undo source fidelity;
- headless Dart archive consumer;
- Flutter-only direct-dependency archive consumer with transitive
  `flark_core`;
- custom-surface behavior, accessibility, and visual regression suite;
- versioned end-to-end performance matrix on every qualified platform;
- every public performance/support claim names a passing device/build/fixture
  receipt and exact envelope.

## 6. Explicitly deferred

- Web and Linux product support;
- another language SDK;
- permanently stable third-party ABI guarantees;
- collaborative editing and multi-source provenance;
- a public parser AST or general editor framework;
- backend/plugin selection;
- filesystem/package layout cleanup not required by the selected boundary.

The first implementation task after M0 review is the two mechanical package
identity commits in M1. The first behavioral task is the `flark-runtime`
contract and direct Rust harness in M2; it is not a broad Flutter surface build.
