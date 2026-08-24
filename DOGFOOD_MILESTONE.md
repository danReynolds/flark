# D0: Ready for Dan's macOS dogfood

**Status:** executable stop contract; D0 remains red until one exact candidate
produces the complete machine-validated receipt below

**North Star:** [NORTH_STAR.md](NORTH_STAR.md)

**Target:** a clean, exact Flark candidate that is demonstrably ready for Dan
to dogfood on macOS.

This is a finite engineering milestone. It is not a claim that the North Star
is complete, that Flark is release-ready, or that it leads its peers.

## The two stops

Flark separates engineering readiness from human acceptance:

- **D0 — dogfood ready:** engineering closes the frozen scope and evidence
  below, produces one exact candidate, and stops changing it.
- **D1 — dogfood accepted:** Dan completes the representative journeys below
  without a blocker or must-fix defect and explicitly accepts the candidate.

Codex can close D0. Only Dan can close D1. Peer comparison and the alpha-release
milestone begin after D1, not before it.

## D0 in one checklist

D0 is GO only when all of these are true on one clean candidate:

1. the exact scenario ledger and permitted pending ranges are frozen;
2. one pending-presentation snapshot/lifecycle owns every ledger case and the
   final ABI minor is frozen;
3. semantic, controller, and actual-paint gates pass through final settlement;
4. the non-skipped native macOS canary passes against the app's embedded ABI;
5. the fixed Mac performance/lifecycle receipt validates every budget;
6. exact-commit CI plus one architecture and one evidence review pass;
7. open B0 and B1 counts are both zero; and
8. the exact profile app, hashes, receipt, and B2 ledger are handed to Dan.

One false or missing item is NO-GO. When all eight are true, engineering stops
and hands off; it does not add another feature, refactor, benchmark, or audit.

The current example is an editor-core workbench with product and scale presets,
not a daily-document application. D0 therefore covers the editing engine and
the macOS dogfood workbench. Arbitrary-file open/save, autosave, recovery, and
document management are separate product-shell work and are not silently added
to this milestone.

## The frozen D0 envelope

The implementation phase starts with the checked-in
[dogfood scenario ledger](docs/testing/dogfood_scenario_v1.md), a compact table
with stable IDs, exact fixtures, operations, cadences, and the permitted
pending presentation for each transition. The tests import the real
product-tour fixture; copied approximations do not count.

The ledger is not a generic scenario language. It is a human-readable coverage
table whose rows point to ordinary parameterized tests.

Phase 0 expands these rows in place with exact fixture locations, caret ranges,
expected outcomes, and test names. After that review, the denominator is
frozen. A new row may enter D0 only for a B0, a reproducible B1 in an existing
journey, or an explicit scope decision from Dan; other discoveries go to the
post-D1 backlog.

| ID | Surface | Required operations | Required visible result |
|---|---|---|---|
| D0-PROSE | Ordinary product-tour prose | type words and spaces at the start, middle, and end; type `. , ; : ! ? ' " ( ) – —` and an interior hyphen; Backspace, forward Delete, selection replacement, and paste | fully rendered prose, current source and caret, and no unrelated marker exposure |
| D0-INLINE | Text before, inside, between, and after strong, emphasis, strikethrough, inline code, and links | type, delete, replace, paste, then undo and redo | unaffected facts keep their resolved style; a syntax-changing edit may expose only its parser-authored dependency island |
| D0-SYNTAX | Incomplete or changing Markdown syntax beside unrelated styled facts | insert and remove `*`, `_`, `~`, backtick, `[` and `]` in inline prose; insert and remove `# `, `> `, `- `, `1. `, and triple backticks at a physical-line start | the typed delimiter may remain exact only in the ledger-named parser-authored dependency island; unrelated delimiters, facts, and shells remain rendered |
| D0-BLOCK | Paragraph, ATX heading, simple ordered/bullet/task item, depth-one quote, table cell, and fenced-code boundary | edit content; Return; rapid Return plus typing; Backspace merge/lift; list indent/outdent; table Tab/Shift-Tab navigation | current block shell and action authority remain coherent; a structural transition never relays unrelated source markers |
| D0-HISTORY | Product-tour selections and clipboard/history paths | keyboard and pointer selection, copy, cut, paste, undo, and redo | byte-exact source, canonical selection, visible selection, and caret remain one lineage |
| D0-NAV | Wrapped and paged surfaces | arrow/word/vertical movement, range extension, focus loss/reconnect, resize, scroll away/back, and page crossing | no caret rehome, stale geometry, focus leak, source mutation, or torn visible page |
| D0-UNICODE | Product-tour Unicode prose | emoji and joined sequences, combining text, bidirectional text, and the real macOS Option-E/E dead-key route | exact source and caret identity with stable surrounding rendering |
| D0-MODES | Editable and reading surfaces | switch modes; toggle a certified task; use table Tab/Shift-Tab; verify those actions are absent while the ledger declares no current action authority; capture exact source; close; reopen the same named preset | equivalent parser-owned Markdown presentation; no stale or inert action is advertised; the named preset resets and matches a clean parse of its pristine source because persistence is outside D0 |
| D0-SCALE | Every enabled and selectable preset bound into the D0 app hash | open, reach an editable viewport, edit locally, undo, scroll/page, and close | the applicable numeric paint, latency, memory, lifecycle, fault, and resync gates in this document pass |

Japanese/CJK IME composition is explicitly outside D0. This does not weaken
the source, selection, or no-corruption invariants: any observed data loss,
input reordering, crash, hang, or unrecoverable fault remains a blocker
regardless of input language.

### Cadence denominator

Every mutation sequence has a per-edit-pump variant that paints every accepted
generation. Text-producing journeys additionally run at:

1. human cadence, exactly 80 milliseconds per input; and
2. every true-burst sequence named by the frozen ledger, in which several
   accepted edits occur before Flutter is pumped.

Phase 0 names one exact burst seed and command sequence for each distinct
chaining mechanism: literal prose, styled inline content, and structural Return
plus its immediate successor. Scroll, focus, copy, and mode-switch actions do
not multiply across text cadences unless their own ledger row names a transient
paint mechanism. A true-burst variant may paint only its final generation, but
it replays an identical sequence whose per-edit-pump variant proves that frame
coalescing did not hide an invalid intermediate generation.

Correctness may not depend on Flutter coalescing an invalid intermediate state.
Any phase-sensitive defect fixed during D0 is repeated 100 times once before
landing; that repetition is a stability receipt, not part of the routine gate.

### Pending-presentation denominator

Every ledger row declares one of three outcomes:

- **projected:** the complete current presentation remains rendered;
- **local exact island:** only a named parser-authored affected range is exact,
  while independent presentation stays rendered; or
- **structural transition:** one parser-authored current-revision transitional
  surface owns the change until fresh certification supersedes it.

An incomplete Markdown construct can legitimately show literal source. D0 does
not require impossible universal projection. The mechanical rule is that every
D0 paint must match the ledger-declared projected result, exact half-open island
`[start, end)`, or structural surface. A broader or different exact range is a
B1.

## What every accepted paint must prove

For each visually sensitive in-envelope action, every actual paint from
acceptance through settled recertification universally proves:

- exact accepted source, source generation, and canonical selection;
- visible source identity for the active input window;
- expected rendered text, resolved inline styles, and block shell;
- no unrelated Markdown delimiter or structural-marker exposure;
- exact source confined to the predeclared affected island, when applicable;
- no fault, resync, stale publication, or missing active surface; and
- terminal byte-exact source and presentation equivalence with a clean parse.

The ledger marks geometry and interaction applicability. A collapsed editable
selection requires a visible caret at the canonical extent and no range
selection geometry. A range selection requires geometry for its canonical base
and extent; a read-only row may require neither. Hit behavior, semantics, and
actions are asserted only for the action-bearing rows named by the ledger.

A controller publication, marker-free text without the expected style, or a
correct final frame does not satisfy this requirement. The mounted test lane
keeps one unsupported-edit control that must visibly paint raw pending source;
it proves the observation method can detect the transient frames that D0 cases
forbid.

## Architecture closure before D0

The current direction is correct, but D0 is not permission to keep adding
construct-specific matchers and parallel continuity paths. The bounded
architecture work is:

1. **One host pending-presentation state.** Literal envelopes and edit cells
   normalize into one sealed pre-edit dependency-authority model. Committed
   structural receipts remain a distinct post-commit input, but both paths
   materialize the same revision-bound pending-presentation snapshot.
2. **One transition lifecycle.** Core exposes pre-edit authority bind/advance
   and committed structural-receipt adoption. Both produce the same snapshot
   and share one retire/fresh-certification supersession lifecycle. The
   controller owns one pending-presentation slot instead of resolving
   precedence among independently authoritative state slots. A length-neutral
   semantic-action result, such as a committed task check, remains a typed
   snapshot field rather than a separate source of row authority.
3. **Parser-owned policy.** `flark-parser` authors Markdown dependency and
   retention proof. `flark-runtime` validates session/revision ownership and
   maps that proof into bounded product records. Core may implement the exact
   protocol-defined edit predicates, but may not invent or widen their
   Markdown/product meaning. Flutter does not add marker scans or product-policy
   character allowlists.
4. **Atomic publication.** Source, generation, viewport, presentation,
   mapping, selection, and every available current semantics/action record
   named by the ledger advance as one coherent snapshot. Older asynchronous
   acknowledgements cannot overwrite a newer optimistic generation. This does
   not claim final assistive-technology qualification.
5. **Bounded optional authority.** Record counts, byte payloads, matching,
   transformation, and query work are capped. Optional continuity metadata
   may be truncated without evicting ordinary inline facts from later rows.
6. **ABI stabilization.** Phase 1 records one exact final ABI minor for the D0
   candidate. Any later ABI change invalidates the Phase 1 exit and every
   downstream receipt, and requires explicit architecture review and re-freeze.
   Construct-by-construct ABI growth is not accepted.

This requires semantic and lifecycle consolidation, not risky wire-format
churn. The planned sealed Core artifact is
`FlarkPendingPresentationSnapshot`; existing pre-edit wire records may only
decode at the ABI/Core adapter boundary into its dependency-authority variant,
while committed structural, paragraph-gap, and semantic-action results adopt
their typed snapshot variants. The controller owns one `_pendingPresentation`
field, and a boundary test rejects additional controller authority slots. One
exhaustive test enumerates every variant, retirement path, precedence rule, and
fresh-cert supersession; a static boundary test rejects host Markdown-policy
scans.

The names may change once during Phase 1 if review finds a clearer API, but the
receipt records their final names and tests. Structural commands may remain
distinct Rust operations. The controller and runtime need only extract the
policy seams required to establish this single ownership; file length, module
count, wire cleanup, and a wholesale rewrite are not D0 exits.

## Evidence plan

Each assertion belongs at the lowest layer that can observe it. A case does not
need to be duplicated in every lane.

### 1. Parser and Core proof

- Every emitted D0 authority record is compared with a clean parse after every
  admitted edit and carried successor.
- The comparison covers the complete block shell and every fact outside the
  transformed affected island, not one selected fact.
- Matcher domains are disjoint, boundaries are exercised, and adversarial
  delimiters either expand the parser-owned island or fail closed.
- ABI payload/cap tests prove optional records cannot evict baseline facts from
  later rows.

### 2. Controller transition proof

- Every synchronous source, selection, generation, viewport, pending-authority,
  acknowledgement, history, and semantic-action publication is sampled.
- Rapid edits, older asynchronous results, paging, close, and reconnect cannot
  regress generation, reorder input, or pair stale facts with current source.
- Current action authority is proved separately from retained visual shell.

### 3. Mounted actual-paint proof

The existing North-Star matrix remains compact and data-driven. D0 closes these
currently missing mechanisms:

- forward Delete and paste/history in a styled product row;
- keyboard navigation and focus reconnect where an intermediate frame matters;
- scrolling and page crossing on a large document; and
- current task/table action exposure while transitional presentation exists.

Tests are admitted only when they add an actual-paint, geometry, semantics, or
interaction fact. D0 must not grow a second semantic corpus or a generic
journey DSL.

### 4. Native macOS proof

The native canary runs against the exact candidate app and native library. It
must not be reported as green when environment-gated and skipped. It proves:

- real character and punctuation routing;
- the declared Latin dead-key path;
- Return, Backspace, pointer selection, cut, undo, and wheel scrolling;
- frontmost PID, accessibility focus, window geometry, and pre-input selection
  preconditions; and
- an app-authored, stable input-delivery acknowledgement after every injected
  key, text batch, structural burst, and paste, before settlement is accepted;
  and
- every-generation source, presentation, style, and caret identity for the
  sustained wrapped product-tour edit.

Semantic breadth remains in the mounted and parser lanes; the native canary
does not replay it.

The sustained native cell is frozen to the real product-tour fixture, a
1569-by-906 logical-pixel window, the exact 35-edit string
` Testing is somewhat useful but lik`, and 80 milliseconds between edits. The
remaining native operation strings and source offsets stay checked in beside
that test. Changing any of them changes the ledger revision and invalidates the
prior receipt.

The app-embedded Native Asset is authoritative. The orchestrator locates the
`flark_abi` binary inside the candidate bundle's
`Contents/Frameworks/flark_abi.framework`, runs the canary with that path, and
records its size and SHA-256. A standalone test dylib is recorded separately
and cannot be claimed as the app artifact unless its bytes hash identically.

### 5. Mac performance and lifecycle proof

D0 uses fixed internal fixtures, not peer-derived size tiers. Measurements run
through the actual hashed dogfood app in profile mode on the named benchmark Mac
and record hardware, macOS, Flutter, display refresh/period, commit, app bundle
manifest, embedded native-library hash, and raw samples. A separate integration-
test/profile bundle may provide diagnostic decomposition, but it is separately
hashed and cannot supply D0 app metrics unless the receipt proves it imports the
same fixture/editor code and embedded ABI. Prefer instrumenting and driving the
actual dogfood app.

Required workloads are:

- product tour: ordinary and styled typing plus structural Return/typing;
- 1 MiB ordinary: typing, inline typing, structural burst, and 32 KiB
  paste/undo;
- 1 MiB dense blocks: open, scroll/page, local edit, and close;
- 5 MiB ordinary: open, scroll/page, local edit/undo, and close;
- 5 MiB giant line: open, local edit/navigation, and close;
- 10 MiB ordinary: typing, inline typing, scroll/page, and close; and
- streamed 10 MiB opening for five runs if that feature is enabled in D0.

The whole interaction matrix runs on the product tour; large shapes exercise
their distinct scale mechanism rather than multiplying every operation across
every size.

The D0 sampling denominator is fixed:

| Cell | Warmups per run | Measured samples per run | Runs | Cadence |
|---|---:|---:|---:|---:|
| product-tour cold launch | 0 | 1 | 5 fresh OS processes | unthrottled |
| product-tour and 1 MiB typing, inline typing, and deletion | 20 | 120 | 3 | 60 edits/second |
| product-tour and 1 MiB structural burst | 20 | 120 | 3 | 30 immediate Return-plus-`x` pairs/second; 60 accepted edits/second |
| 32 KiB paste/undo | 2 | 10 | 3 | unthrottled |
| each buffered large-preset open/edit/undo/scroll/close journey | 0 | 1 | 5 fresh preset sessions | unthrottled |
| streamed 10 MiB open/edit/close, when enabled | 0 | 1 | 5 fresh sessions | unthrottled |
| lifecycle | 0 | 100 same-process cycles plus 10 distinct OS processes | 1 fixed sequence per cycle/process | unthrottled |

The 60-edit-per-second cells are input-throughput measurements on the actual
display, not a demand for an impossible one-frame-per-input phase lock. A
generation may be recorded as `superseded-before-frame` only when the next
declared generation is accepted before the first real `FrameTiming` build begins
after it and that exact successor paints on that frame. The superseded
input still counts toward the fixed denominator and its synchronous/native work
stays attributed to that frame, but it contributes no duplicate visibility
latency sample. A frame opportunity before supersession, a missing successor
paint, or a successor that misses that first frame is B1. The separate
per-edit-pump actual-paint lane remains the proof that each individual
generation is renderable and correct when it has a frame opportunity.

Each structural run contains two distinct measurements against the identical
fixture and anchor. The latency phase sends one immediate Return-plus-`x` pair
and waits for the successor's proving paint before the next pair; the Return
paint may coalesce only when no FrameTiming opportunity occurs before `x`; the
`x` callback begins within 30 milliseconds of Return and must paint on its next
real frame. The continuous phase sends all 140 pairs at the fixed
30-pair/second schedule under that same immediate-successor bound, permits an
intermediate Return generation to coalesce only when no frame opportunity
exists, requires all 280 ordered input/engine receipts and exact transitions,
and requires the terminal generation to paint and certify. Run zero also
replays all 140 pairs with a pump after both Return and `x`, requiring all 280
generations to paint. Each phase begins with an app-acknowledged reset and
activation into a distinct canary session. The receipt records the exact frozen
actuator transcript plus every app-returned setup, settle, and close
acknowledgement; raw frame, input, engine, and paint observations retain the
app-authored session identity rather than a runner-applied phase label. The
control is correctness-only; all first
20-pair warmups are excluded from phase metrics, and latency and continuous
burst budgets are evaluated independently before aggregation.

Phase 3 adds `docs/testing/dogfood_performance_v1.schema.json` and
`scripts/verify_v4_dogfood_receipt.dart`, a machine schema and replay validator
for `dogfood_performance_v1`. Its raw-sample identity, display validation,
percentile/max calculation, outlier handling, proving-frame join, and warmed
RSS baseline procedure reuse Section 2, Section 3's raw identity/replay/
distribution/outlier/proving-frame semantics **excluding Section 3's frozen
sampling table**, and Section 4's numeric gates from the
[performance evidence contract](docs/architecture/v4/contracts/performance_evidence_v1.md).
It does not reuse peer resolution, M0 sample counts, or claim eligibility. Its
fixed workload matrix and denominator are the ones above.

Gate applicability is explicit:

- next-frame visibility, engine/Flutter p99, hard-span, dropped-frame, and
  projection gates apply to every measured edit cell;
- the 200-millisecond first-editable-viewport gate applies to every fresh
  buffered and streamed preset session;
- the 500-millisecond visible-certification gate applies after every fresh open
  and measured edit, not to full-document background completion; and
- peak/retained RSS applies to every large preset, while zero live resources
  applies to the lifecycle cell.

Latency start/end events cannot exclude inconvenient work:

- product-tour cold start begins at fresh OS process launch and ends only when
  the complete visual viewport has painted exact editable content and the
  declared layout-overscan range is laid out from the same current source;
- a preset-open sample begins when the app accepts the preset-selection command,
  includes fixture generation and native admission, and ends at that same
  complete visible paint plus current layout-overscan readiness;
- visible certification ends only when the complete visual viewport and its
  declared layout-overscan range are current for the accepted source revision;
- every fresh-session and process-reopen count above names a distinct OS
  process, while the 100 lifecycle cycles are explicit controller/session
  cycles within the named warmed process.

The profile app launches at the frozen 1569-by-906 window geometry, and the
cold-launch cell retains the initial engine paint stream rather than clearing it
and forcing a later observer-only repaint. Accepted input and process-launch
times use wall-clock epoch microseconds, while Flutter `FrameTiming` and paint
frame stamps use the engine's monotonic clock. Every raw frame and paint carries
a tight epoch/monotonic clock anchor. Each paint also carries the render-derived
painted-fragment count and source coverage, so a structural or opening proving
paint must include the active canonical caret within the complete observed
surface. The validator replays the clock mappings, paint-to-`FrameTiming` join,
session identity, and surface coverage before applying any latency or frame
budget. Directly comparing the two clock domains, trusting a prejoined ordinal,
or calling a runner-labelled/partial surface complete is invalid D0 evidence.

Every lifecycle sample uses Product Tour, activates immediately after
`locally.`, inserts `x`, undoes it, closes the session, and records a global-zero
native state before the next cycle or process exits. The ten process samples
each launch a distinct OS process and execute that same sequence once.

A single functional or mounted action has a 30-second watchdog; the complete
native canary has a 180-second watchdog; each profile cell and the final
certification-stress lane have a 15-minute watchdog; and the cold complete
dogfood-ready orchestrator has a two-hour watchdog. A watchdog expiry is a
failure, never a skipped or inconclusive result.

The D0 budgets are:

- accepted source, caret, and selection are visible together by the next real
  rendered frame and within the lesser of 16 milliseconds or the measured
  display period;
- Rust engine work is at most 4 milliseconds at p99;
- Flutter frame work is at most 8 milliseconds at p99;
- every editor-attributed frame and synchronous span is below 16 milliseconds;
- editor-attributed dropped frames are zero;
- first exact editable viewport paint is below 200 milliseconds;
- visible current-revision certification is below 500 milliseconds;
- every supported-continuity sample paints exactly the ledger-declared
  projected result or half-open exact island;
- after 100 open/edit/close cycles and 10 process reopens, native documents,
  transactions, continuations, and handles are zero;
- peak RSS over the warmed baseline is at most the greater of 64 MiB or eight
  times the source bytes; and
- retained RSS after close is at most 16 MiB over the warmed baseline.

The current stopwatch around `tester.pump` is development evidence, not this
receipt. `dogfood_performance_v1` requires exact acceptance, source-paint,
caret-paint, selection-paint, build, and raster timestamps; the complete frame
stream; measured display provenance; real product-tour fixture import and
declared edit positions; scroll/page/navigation events; and ordered warmed,
peak, close, and post-close RSS samples. It also requires a process-global
native live-state inspector established before the Phase 1 ABI freeze to prove
zero documents, transactions, continuations, and handles after session
disposal.

## The one dogfood-ready gate

D0 adds `scripts/verify_v4_dogfood_ready.sh`. It orchestrates or verifies, for
one exact clean commit:

1. `bash scripts/verify_v4.sh`;
2. `FLARK_V4_FEATURES=opening-session bash scripts/verify_v4.sh` when streamed
   opening remains enabled in the dogfood build;
3. `bash scripts/verify_v4_certification_stress.sh` once for the final
   candidate;
4. a profile macOS app with its embedded native framework;
5. the non-skipped native canary against that exact embedded binary;
6. the fixed profile/lifecycle matrix and numeric budgets above; and
7. a machine-readable completion receipt.

The final receipt is produced by
`scripts/verify_v4_dogfood_completion.dart`. It recomputes the app-bundle
manifest, replays the performance receipt, binds every profile fragment to the
candidate/app/fixture/measurement host, replays the native Flutter machine log
and the structured default/stress/actual-paint gate receipts, and requires live
GitHub job metadata for exact-head CI, two independent reviews, the
moving-surface capture, and the reviewed B2 ledger. The orchestrator must
report `INCOMPLETE`, not `PASS`, when that candidate evidence is absent.

The gate fails on a required skip, missing raw sample, invalid display, hash
mismatch, open blocker, or exceeded budget. It records:

- Git commit and tree plus clean state before and after the run;
- a versioned canonical app-bundle manifest digest built from sorted relative
  paths, file sizes, and file SHA-256 values;
- the main executable and embedded ABI binary path, size, and SHA-256;
- any separate test dylib path, size, and SHA-256;
- toolchain, OS, hardware, and display provenance;
- scenario-ledger path, schema/version, Git blob hash, and fixture identity;
- every functional/native/profile result and skip count;
- numeric distributions, maxima, frame misses, memory, and live resources;
- every FrameTiming interval from accepted input through the proving paint,
  including frames before the first generation paint;
- raw evidence, receipt, schema, and validator paths, versions, sizes, and
  SHA-256 values;
- required CI workflow run/job URLs whose reported `head_sha` equals the
  candidate commit; and
- moving-surface capture/checklist plus reviewer signoff artifacts; and
- the reviewed out-of-scope/known-limitation ledger.

The orchestrator replays machine test output and requires the named native
canary and both actual-paint files to execute with skip count zero. Default and
stress success are carried by structured receipts bound to the exact command,
absolute hashed toolchain, controlled environment, working directory, runner
artifact, raw log, observed process exit, commit, and tree rather than by
copyable log substrings. The certification-stress command proves its named
dense-runtime capacity case only; it does not substitute for any D0 scale or
profile cell.

Historical prose and a prior artifact do not count. CI must be green on the
same commit, but merge to `main` is not required for D0. Required CI build
smokes establish packaging only; they do not become mobile product evidence.

Before handoff, one reviewer also watches the scripted product-tour journey in
the exact profile app. This is a moving-surface sanity check, not a substitute
for the receipts above.

That journey is fixed and completes within three minutes: launch Product Tour
at 1569 by 906; append ` Testing is somewhat useful but lik` after `locally.`
at 80-millisecond cadence; replace `somewhat` with `reasonably`; undo and redo;
perform Return plus an immediate `x` and Backspace merge at the paragraph
boundary; scroll to the GFM section; toggle the certified task; edit a table
cell and use Tab; scroll to the long paragraph; resize to 1000 by 700 and back;
cycle focus; then close. A frame capture or screen recording, command log, and
review checklist are receipt artifacts.

## Severity and reopening rules

### B0 — blocker

Any source loss/corruption, crash, hang, fault or resync loop, lost/duplicated/
reordered input, source-selection-caret identity break, or inability to recover
exact source. Any reproducible observation in the exact exposed D0 candidate is
D0 NO-GO. D0 requires zero known B0; it does not claim universal proof over
unexercised products or platforms.

### B1 — must fix for D0

Any reproducible in-envelope:

- actual-paint North-Star violation;
- a broader or different exact range than the frozen ledger declares;
- lost style or wrong block shell outside the affected island;
- stale, inert, or incorrect semantic action;
- clipboard, history, navigation, focus, or paging error; or
- performance-budget failure.

One valid actual-paint receipt is sufficient. “Known limitation” cannot waive
an in-envelope B1.

### B2 — dogfood follow-up

A cosmetic issue or explicitly unsupported, safely local fail-closed edit
outside the frozen envelope. It may accompany D0 only when listed with a
reproducer, scope rationale, safe workaround, owner/backlog ID, and workable
route. It becomes B1 if it repeatedly interrupts Dan's representative journeys
or investigation reveals an authority/correctness defect.

A fix invalidates the affected receipts and reruns the impacted lane plus the
aggregate gate on the new commit. Architecture reopens only when a finding
shows an authority escape, divergent transition ownership, an inadequate typed
contract, host Markdown inference, or measured unbounded work. An isolated bug
that fits the existing contract reopens implementation and regression coverage,
not the architecture.

## Explicit non-goals for D0

D0 does not wait for:

- Japanese/CJK IME, autocorrect, dictation, or universal composition;
- peer baselines, competitor-derived scale tiers, or a “leading editor” claim;
- physical iOS/Android interaction, mobile performance, or touch qualification;
- Web, Linux, Windows, collaboration, or new product features;
- arbitrary-file persistence, autosave, recovery, or document management;
- universal projection for every obscure or deliberately unsupported Markdown
  mutation;
- a complete controller/runtime rewrite, target line counts, or removal of old
  wire records that already normalize into the unified authority model;
- publication, archives, package release, or alpha-release claims; or
- unrelated cleanup, themes, and final visual polish.

These items may become later goals. They may not silently expand D0.

## Work plan and phase exits

### Phase 0 — freeze the denominator

- Check in the scenario ledger and allowed pending outcome for every row.
- Name every enabled/selectable preset and bind that set to the candidate app
  hash. A streamed preset is excluded only when the UI marks it unavailable and
  the receipt records `DISABLED`; otherwise its opening-session lane is
  mandatory.
- Record the starting ABI minor and whether Phase 1 is expected to change it,
  plus the sampling/applicability table, native inputs/geometry/timeouts, RSS
  baseline procedure, and watchdogs.
- Freeze the exact D0 CI check names. The initial set is
  `v4-integration-gate`, `v4-opening-session-gate` when streamed opening is
  enabled, and `macos-smoke`; other platform checks are not Mac product proof.
- Record current B0/B1 defects and all known out-of-scope fallbacks.

**Exit:** no ledger row contains `representative`, `where supported`,
`currently`, `responsive`, or another implementation-dependent qualifier; no
implementation work is admitted without an exact ledger row or a B0.

### Phase 1 — seal authority ownership

- Normalize current authority records into one Core model and one controller
  transition lifecycle.
- Remove overlapping host authority paths and centralize retirement/
  supersession.
- Land the process-global live-state inspection contract and its zero-after-
  close test, whether through a predeclared test-only hook or the final ABI.
- Freeze the candidate ABI.

**Exit:** the final ABI minor is recorded; every D0 ledger row consumes the
sealed snapshot lifecycle; executable cases for projected,
local-exact-island, structural, paragraph-gap, and semantic-action outcomes all
pass through it; boundary/static tests reject extra authority slots and host
Markdown policy; and every remaining row needs parser proof data and tests, not
a new host mechanism, state slot, or syntax-specific ABI capability.

### Phase 2 — close the behavior matrix

- Implement every ledger row with exactly its declared projected, local-island,
  or structural result.
- Land only parser-owned dependency proof needed by the frozen denominator.
- Close source/generation, structural-action, paging, focus, history, and
  lifecycle races found by those rows.
- Preserve explicit out-of-envelope fail-closed behavior.

**Exit:** zero open B0/B1 and every scenario has a clean-parse terminal oracle.

### Phase 3 — close false-green gaps

- Add only the missing lowest-layer and actual-paint evidence identified above.
- Add the non-skippable native orchestrator.
- Add a versioned, machine-validated performance/lifecycle receipt.
- Keep fixture imports tied to the real dogfood document.

**Exit:** a deliberately raw negative control fails the North-Star assertions,
required native tests cannot skip, and invalid/missing performance evidence
fails closed.

### Phase 4 — make the exact candidate

- Run the dogfood-ready gate on a clean commit.
- Run required CI on that exact commit.
- Complete one independent architecture review and one evidence review.
- Fix only concrete B0/B1 findings, contract contradictions, or invalid proof.

**Exit:** the completion receipt below is entirely green. Hypothetical breadth,
style preferences, and out-of-envelope improvements go to the backlog rather
than starting another “final audit” cycle.

Each independent review is one checklist pass by a reviewer other than the
implementing turn or agent. Reviewers validate the frozen scope; they cannot add
D0 breadth. Concrete B0/B1 or invalid-evidence fixes receive at most one focused
verification pass before the aggregate gate is rerun. A new valid blocker after
that pass keeps D0 red, but does not authorize an unbounded audit loop or scope
expansion.

### Phase 5 — stop and hand off

- Launch the exact profile app for Dan.
- Provide commit, hashes, receipt, scenario ledger, and B2 limitations.
- Stop implementation until Dan reports a D0-reopening defect or completes D1.

## D0 completion receipt

Every line must be filled for the same candidate:

```text
Candidate commit and tree:
Worktree clean:                         PASS / FAIL
Dogfood scenario-ledger path/blob:
Profile app bundle path/manifest digest:
Main executable path/size/SHA-256:
Embedded ABI path/size/SHA-256:
Separate test dylib, if any:
macOS / hardware / display:
Flutter / Dart / Rust toolchains:

Default v4 aggregate gate:             PASS / FAIL
Opening-session gate or disabled:      PASS / FAIL / DISABLED
Final certification stress:            PASS / FAIL
North-Star actual-paint matrix:        PASS / FAIL
Native macOS canary, skipped count 0:  PASS / FAIL
Profile and lifecycle budgets:         PASS / FAIL
Required exact-commit CI:              PASS / FAIL
Moving-surface review:                 PASS / FAIL
Independent architecture review:       PASS / FAIL
Independent evidence review:           PASS / FAIL

Open B0:                               0 required
Open B1:                               0 required
Reviewed B2 / out-of-scope ledger:
Receipt artifact path:
Handoff date:
```

Any unchecked or failing line is D0 NO-GO.

## D1: Dan's acceptance journey

D1 is representative use, not an arbitrary hour count. Dan performs these
journeys in the exact D0 app:

1. write and revise ordinary prose around and inside styled Markdown;
2. create and restructure headings, paragraphs, lists/tasks, quotes, and a
   table cell;
3. select, copy/cut/paste, undo/redo, navigate wrapped text, resize, refocus,
   and scroll away/back;
4. edit and navigate at least one large preset; and
5. capture the exact edited source, close the workbench, reopen the same named
   preset, verify it intentionally resets to the preset source with no stale
   session state, and confirm clean lifecycle, caret behavior, and
   responsiveness. Edited-source persistence across restart is not part of D1.

Any B0 or B1 report reopens D0 and receives a regression at the layer that can
observe it. Otherwise Dan explicitly accepts D1. Only then does the program
move to peer comparison and the separate alpha-release goal.
