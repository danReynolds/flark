# Flark v4 build plan

**Execution contract for
[RFC 026](../rfc/rfc_026_flark_v4_product_architecture.md).** 2026-08-08.

This plan builds a headless Dart `flark_core` over the selected incremental
Rust engine, then builds the Flutter product `flark` on top. The first proof and
all initial performance work run on the available Mac. Android and iOS claims
wait for physical devices; Windows follows later.

## 2026-08-09 execution update

The v4 implementation is a new path. Legacy v3 packages remain read-only
reference material; they are not renamed, repaired, imported, or used as a
migration scaffold by v4. The mechanical rename sequence later in this
document is therefore superseded. The active path now exists directly under
its final product identities:

- Rust `flark-runtime` owns a serialized document actor on a Rust-sized native
  stack and retains `flark-parser` plus `flark-engine`.
- Rust `flark-abi` implements open/chunked admission, bounded pump, one-edit
  admission, UTF-8/UTF-16 conversion, source reads, and first-page semantic or
  pending-neutral viewport queries.
- Dart `packages/flark_core` owns the native session from one persistent worker
  isolate and exposes revisioned open/edit/pump/query/source APIs with no
  Flutter dependency.
- Flutter `packages/flark` is a direct custom `RenderBox` surface using delta
  text input, a bounded 16 Ki UTF-16 active input window, optimistic local
  source/caret/selection painting, and certified-row reconciliation.
- `packages/flark/example` builds and launches as a macOS application.

This is an end-to-end vertical slice, not the complete product boundary. ABI
continuation and bounded close are now implemented through Rust, C, and Dart;
the custom Flutter surface pages forward and backward without a scroll widget.
The full GFM/live-projection behavior matrix and claim-eligible multi-shape,
multi-size performance receipts remain open.

Focused vertical-slice checks pass for the Rust runtime, fixed ABI, Dart actor,
UTF-8/UTF-16 edits, Flutter custom surface, and optimistic one-frame state
update. A release-mode ordinary-prose development curve (not a claim-eligible
performance receipt) initially measured exact 1/2/5/10 MiB sources. Pending
source was queryable in 4.6-6.0 ms, warmed edit admission remained 2.9-3.8 ms,
and local edit recertification was 17.9-26.1 ms. Full initial certification was
linear at roughly 2.8 seconds/MiB and the first certified 4 KiB viewport query
was roughly 34-36 ms.

A targeted sample then found that every parser work unit moved an enormous
inline state enum, with `memmove` dominating the native actor. Boxing the large
state variants removed that accidental copy. Product-style cooperative pump
measurements after the fix certified 1 MiB in 551.029 ms with a 1.014 ms
maximum pump turn, and 10 MiB in 4959.602 ms with a 2.608 ms maximum pump turn.
The 10 MiB local edit was admitted in 3.691 ms and recertified in 5.335 ms; its
maximum pump turn was 3.183 ms. An edit submitted immediately after opening a
10 MiB source committed revision 2 in 4.194 ms, while initial certification was
still pending, and the updated neutral viewport returned in 0.128 ms.

A 200-sample release-mode profile showed that the old 256-row certified query
was overfetching linearly. The product page is now 32 rows: on the 1 MiB
ordinary-prose fixture, full-range certified queries measured 8.927 ms p50,
9.834 ms p99, and 11.787 ms maximum on this Mac. A profile-mode macOS product
run then applied 120 rapid optimistic insertions to a 1 MiB document with no
pending edits or faults: input-to-frame measured 8.233 ms p50, 8.948 ms p99,
and 9.224 ms maximum; build measured 1.148 ms p99/3.870 ms maximum and raster
1.178 ms p99/3.142 ms maximum. This is a development receipt, not the full
provenance-bearing M4/M6 certification artifact.

That run also closed two real burst-input failures rather than masking them.
`SMALL_EDIT` retirement saturation now returns contract-valid backpressure,
bounded pumps reclaim source/parser state, and Dart retries without changing
the rejected revision. Semantic recertification is scheduled once after a
32 ms input-idle window instead of restarting after every key; exact source,
caret, and selection still paint optimistically on the input frame. A headless
120-edit 1 MiB burst measured native edit acknowledgement at 0.043 ms p50,
0.143 ms p99, and 3.185 ms maximum.

The direct Flutter controller now fails closed immediately on every optimistic
edit: it updates the bounded source projection and UTF-16 row positions on the
input turn, drops stale semantic kinds to neutral, and disables paging until a
current certified viewport is installed. A focused structural edit regression
proves that removing a heading marker never paints the old heading semantics,
keeps the distant source exact at its shifted position, and restores certified
rows after bounded parser work. The single post-change 1 MiB macOS profile
regression completed 120 rapid edits with zero pending edits: input-to-frame
was 8.290 ms p50, 9.703 ms p99, and 10.117 ms maximum; build was 1.890 ms p99
and 8.241 ms maximum; raster was 1.375 ms p99 and 4.273 ms maximum.

The next range-certification checkpoint is now implemented end to end. Rust
publishes only authenticated current-revision prefix/suffix ranges plus explicit
pending gaps. ABI 4.1 introduced those ordered ranges independently of row
ordinals, followed by exact current source. `flark_core` decodes that bounded
partition, and the Flutter controller reuses a cached row's presentation only
when its edit-mapped range is wholly inside a certified current range. The
edited structural row remains neutral while an unchanged row in the same
viewport can regain styling before whole-document convergence. Retained parser
trees are never queried as current source, and no stale row identity crosses
the ABI.

Focused Rust runtime, fixed-ABI, Dart actor, and Flutter structural regressions
cover the mixed path. This is not completion of the live-edit matrix: suffix
certification is withheld until parser convergence proves it, and the bounded
Flutter cache is not yet a complete virtualized multi-block layout. The next
execution order is the selected GFM/live-edit matrix, then dense, giant-line,
and multi-MiB Mac profile shapes. Clean-build throughput receives more work
only if those product receipts identify it. No mobile claim advances without
physical Android and iOS hardware.

The post-checkpoint 1 MiB macOS profile completed the same 120-edit burst with
zero pending edits or faults. Input-to-frame was 8.389 ms p50, 9.160 ms p99,
and 10.200 ms maximum; build was 1.395 ms p99/9.878 ms maximum and raster was
1.389 ms p99/5.504 ms maximum. This preserves the under-16-ms development
curve on this Mac; it remains a local receipt rather than device certification.

The first selected GFM/live-edit behavior slice is also implemented through the
product path. Parser-authored ATX/Setext heading level and style now cross
`flark-runtime` and fixed ABI 4.2 into typed `flark_core` row metadata; Flutter
uses the level for distinct H1-H6 presentation. At the projected content start,
Backspace uses the certified parser-owned source/editable ranges to remove the
whole ATX prefix atomically and demote the heading to a paragraph, without a
Dart Markdown scanner. Focused Rust runtime, ABI layout/native, Dart boundary,
and Flutter interaction regressions pass. This advances the behavior matrix but
does not create a new performance claim.

The next behavior slice carries parser-authored bullet/ordered marker facts,
literal ordered values, nesting/offset data, exact marker-prefix byte and
UTF-16 ranges, and the top-level continuation and List-start boundary facts
through `flark-runtime`, fixed ABI 4.3, and typed `flark_core` models. Flutter
projects passive list markers without scanning Markdown, preserves exact source
markers while active, continues bullet and incrementing ordered items on Enter,
demotes items on Backspace, and exits a terminal empty item on Enter. Explicit
prefix ends and a corrected empty-item caret cut preserve the case where the
parser's zero-width row follows a terminal line ending.

This slice exposed a consequential CommonMark rule: deleting a later item's
marker can leave its text as a lazy continuation of the preceding item. The
parser-owned List-start fact now selects direct prefix removal only for the
opening item and inserts the required blank-line boundary for later-item
demotion. Focused Rust runtime, C/manifest ABI, Dart FFI/boundary, Flutter
analysis, list interaction, and adjacent heading regressions pass. This is a
functional behavior checkpoint, not a new throughput or frame-time claim.

The block-structure tranche is implemented through fixed ABI 4.4. Parser-owned
block-quote prefix geometry, nesting depth, and a bounded simple-continuation
fact now reach typed `flark_core` and Flutter rows. Flutter projects passive
quotes, preserves exact source while active, and only performs Enter/Backspace
continuation for the certified single-line form. Nested or multi-line quote
shapes remain exact-source and interaction-neutral until the runtime publishes
a general segment map; Dart does not reconstruct one by scanning Markdown.

Fenced and indented code-block style, fence character/length/offset/closure,
and thematic-break facts cross the same boundary. Certified code-body cuts are
editable, passive code remains monospace, and passive thematic breaks render as
rules while active rows preserve source. Focused Rust runtime/ABI, Dart
boundary, Flutter analysis/interaction, and adjacent heading/list regressions
pass. This is another functional checkpoint, not a new scale or frame-time
claim.

The first inline-projection tranche is implemented through fixed ABI 4.5.
`flark-runtime` reuses the selected Rust inline grammar to publish complete
source/content byte and UTF-16 geometry for emphasis, strong, simple code
spans, strikethrough, URI/email autolinks, and direct links. `flark_core`
decodes that typed geometry without Flutter, and the Flutter surface hides
only parser-owned marker cuts, paints styled text runs, and maps hit positions
back to exact source offsets. Activating a row restores its unmodified Markdown
source and markers.

The slice fails closed by row. Character-reference, escape, hard-break,
transforming code, image, and reference-link shapes remain exact-source and
interaction-neutral; an output buffer that cannot hold a complete row fact set
also receives no partial set. Inline derivation is currently bounded and runs
on the native document actor during viewport queries. It has not yet earned a
performance claim or a final caching strategy. Focused runtime, ABI/header,
Dart boundary, Flutter analysis, and passive/active projection regressions pass;
no legacy v3 product path was revived.

The second inline-projection tranche is implemented through fixed ABI 4.6. The
single inline record is now 80 bytes and carries parser-authored backslash
escape and hard-break cuts, one- or two-scalar character-reference
replacements, code-span trimming and physical-line-ending replacement, plus
direct/reference link and image label geometry. Reference uses resolve through
the completed live reference-winner index; viewport queries do not rebuild or
scan a document-wide definition table. The same bounded 64-fact/8-KiB-leaf
fail-closed limits remain in force.

`flark_core` exposes these as typed facts without a Flutter dependency. The
Flutter surface applies non-identity replacement runs with explicit
source-endpoint hit mapping, hides only parser-owned syntax, styles resolved
links, uses textual alt-label fallback for images, and restores exact Markdown
when the row becomes active. Link activation and media loading remain M6
behavior; neither belongs in the parser or Dart core.

Focused Rust runtime/parser/ABI, C contract, Dart boundary, and Flutter
passive/active projection tests pass. Two profile-mode Mac development runs
then applied 120 rapid edits to a starting 1-MiB dense-inline fixture. Both
finished with zero pending edits: input-to-frame was 8.574/8.582 ms p99 and
8.601/8.635 ms maximum. Build p99 was 2.390/1.495 ms; the first run had one
26.243 ms build outlier and the repeat's maximum was 12.726 ms. Raster p99 was
0.901/1.470 ms and both maxima were below 4.5 ms. This validates the direction
as a local development checkpoint, but the isolated over-budget build frame
means it is not a "never jank" certification receipt. Query-time inline
derivation is still uncached; caching is justified only if the remaining
dense, giant-line, and multi-MiB product shapes identify it as material.

The first editor-transaction tranche is now implemented on that same direct
path. `flark-abi` advertises reversible history for small edits and retains
inverse source only in a byte-budgeted native store. Replay is revision- and
logical-state-checked, consumes its input token, and returns the opposite token
for redo/undo when retention succeeds. Explicit release, typed evicted/stale
outcomes, bounded eviction tombstones, and resumable close reclamation are
wired. `flark_core` exposes only opaque one-shot tokens and source-length /
revision receipts; Dart does not retain deleted document text.

Flutter now supports an exact-source selection window across visible rows,
neutralizes selected projected rows so selection geometry cannot guess through
non-identity runs, replaces the selected source optimistically, and restores
source-anchored before/after selections through undo and redo. Mac `undo:` and
`redo:` selectors route to the same transaction path. Focused native, Dart,
Flutter, ordinary typing, projected-prefix, and cross-row replace/undo/redo
checks pass.

The next transaction tranche now fills the frozen staged-bulk seam. Edits over
the 4-KiB synchronous envelope begin a native transaction, append replacement
input in at most 64-KiB chunks, validate and capture any retainable inverse in
bounded commit work units, and change source authority/revision exactly once.
Abort leaves source unchanged; close reclaims detached live transactions.
`flark_core` selects small versus bulk without exposing native handles, and
bulk undo/redo retains inverse source only in Rust under the same history
budget.

Flutter accepts an exact 32-KiB platform delta without retaining that complete
replacement in its input or visible caches: both remain at most 16 Ki UTF-16
code units, while the worker stages the full replacement. Focused direct-core
checks prove exact 32-KiB paste plus native undo/redo and an 8-KiB delete/undo;
the Flutter transaction check proves the same source lengths, revisions,
selection restoration, and bounded windows through paste, undo, redo, delete,
and undo.

This is deliberately not the completed editor-history or release-performance
claim. Source selection is still limited to the current 16-KiB visible/input
window, so deletion beyond that selection window is not yet admitted as one
gesture. Input-isolate transfer optimization also remains open. The focused
functional receipt removes the silent 32-KiB failure on this path, but no new
scale or frame-time claim follows until the Mac product benchmark exercises it.

The focused Mac product profile now has a selectable exact 32-KiB paste/reset
workload over the 1-MiB ordinary fixture. Its first run exposed a Flutter-side
virtualization defect: while parser certification was pending, the transient
surface laid out all 643-1,287 physical lines in its bounded 16-KiB cache and
spent roughly 30-35 ms building those frames. The surface now paints a
caret-centered maximum of 32 neutral rows and at most 2 Ki UTF-16 code units of
the active input while retaining the separate 16-KiB platform/source windows.
Focused transaction checks still prove bounded exact paste, undo, redo, and
large deletion behavior.

After that fix, a profile run with two warmups and ten measured paste/reset
cycles recorded 0.156 ms input handling p50/0.466 ms maximum, 1.106 ms measured
input-frame build p50/1.342 ms maximum, 2.917 ms build maximum across all 27
frames, and 3.761 ms raster maximum. Every cycle returned to the base byte and
UTF-16 lengths with zero pending edits or faults. The Flutter driver explicitly
failed to foreground the macOS app, however, and wall-clock input-to-frame
samples alternated between roughly 7-8 ms and 50-51 ms. Those wall samples are
therefore rejected as a jank claim rather than explained away. A foreground
run is still required before the 32-KiB product gate can pass.

The next focused behavior tranche is now implemented. Rapid single-grapheme
insertions coalesce across a one-second idle window into one undo/redo unit;
newline and explicit edit commands break the group. Backward and forward delete
use Dart's `characters` policy and remove one extended grapheme,
including emoji ZWJ sequences. Mac `copy:`, `cut:`, and `paste:` selectors now
route through the exact bounded source selection; paste retains the existing
bulk-capable transaction path. Simulated composing-range updates remain live
source edits but coalesce into one history unit, and semantic refresh defers
while a composing range is active. The grouped replay exposed and closed an ABI
contract defect: `HISTORY_REPLAY` retirement pressure is now a contract-valid
`BACKPRESSURE` result, so Dart performs bounded maintenance and retries instead
of receiving a fabricated internal fault. Focused core and Flutter checks cover
three adjacent undo/redo insertions, rapid typing, grapheme deletion, exact
clipboard commands, composition preservation, and composition undo/redo.

This is simulated composition evidence on this Mac, not live dead-key,
autocorrect, dictation, or third-party IME certification. Those live input
paths, selections beyond the bounded window, and complete command/navigation
behavior remain open.

A post-commit verification pass on the vertical-slice commit found its focused
Flutter suite not green: the mixed-partition regression waited on
`pendingEdits`, which retires at edit admission, then asserted against the
installed viewport, which exists only after the post-admission query installs.
A probe measured roughly six milliseconds between the two on this Mac, and the
assertion deterministically lost that race. The controller was fail-closed the
whole time — the installed page is revision-stamped and `semanticsCurrent`
stays false through the window — so this was an untruthful test barrier, not
stale paint. The gate now waits for the admitted revision's installed page and
asserts page/document revision agreement.

The same pass exposed a real torn observable in paging: both page navigations
advanced the page index before installation awaited a bounded source read, so
a consumer could observe the new page index with the old rows. Viewport
installation is now synchronous — the bounded source read completes first,
page state mutates adjacent to the swap, and the refresh path releases its
fresh continuation when an edit supersedes it mid-flight. Five consecutive
solo runs of the paging regression and the complete v4 gate pass.

`scripts/verify_v4.sh` is now the local gate of record: it builds `flark-abi`,
exports `FLARK_V4_LIBRARY_PATH`, and runs the Rust, Dart analyze/test, and
Flutter analyze/test v4 suites with no pipeline masking an exit code. Without
that variable the Dart and Flutter suites skip silently, which is how a red
suite could previously read as green. Continuous-integration wiring for v4 is
explicitly deferred by decision while core development remains local-first;
the script is the gate.

Two sequencing amendments are recorded. First, the bounded input-window/IME
contract implementation must land together with the ABI surface it depends on
that is currently declared but unimplemented — the anchor
create/transform/resolve/release family, cancellation, create-abort, owner
transfer, and session inspection — and with the migration of canonical
selection, grapheme policy, and history ordering/grouping out of the Flutter
controller into `flark_core`, where RFC 026 section 5 places them. That
correction is part of the milestone, not later cleanup; the
admission-versus-installation distinction above becomes a typed `flark_core`
concern in the same move. Second, macOS foreground performance certification
follows that milestone; the remaining order is unchanged.

The first tranche of that milestone is implemented: all twenty-nine header
operations now exist. Anchors are source-stable byte positions with
creation-time affinity, validated to scalar boundaries in either coordinate
kind and transformed eagerly inside every committed small, bulk, and replay
splice under a declared `MAX_LIVE_ANCHORS` cap of 4096, so anchor operations
complete in one bounded call and close pumping drains unreleased anchors.
Owner transfer requires an idle session and carries retained history-token
authority to the new owner; cancellation retires exactly the current progress
token and returns the stale-token status otherwise; session inspection
reports state, revision, and all four live-handle counts through the fixed
64-byte record. `STABLE_ANCHORS` and `CANCELLATION` capability bits are now
advertised.

Implementing the idle-migration rule exposed a real liveness defect: a
completed pump retained its progress token forever, so a session that had
ever pumped to readiness could never satisfy owner migration, and a fresh
zero-token pump chain was rejected as stale. A terminal pump now echoes its
final token but clears the stored one, and the Dart facade mirrors that
terminal-token rule. Five new focused ABI regressions cover anchor stability
through edits and replay in both coordinate kinds and affinities,
cancellation authority, idle-only migration with history carriage,
provisional-session abort, and lifecycle inspection. The complete verify_v4
gate passes. This is the ABI substrate for canonical `flark_core` selection;
no Dart selection policy has moved yet and no new performance claim follows.

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
- Current-revision range certification: implemented for authenticated
  incremental prefix/suffix regions with explicit pending gaps; the complete
  behavior matrix remains open.
- 32 KiB paste: exact and reversible on the direct core/Flutter transaction
  checks; product-path frame timing and device coverage remain release gates.
- Custom surface: the direct path now proves bounded ordinary input plus one
  exact cross-row replace/undo/redo transaction slice and bounded 32-KiB bulk
  paste plus 8-KiB delete/undo; explicit clipboard commands, IME, selections
  beyond the 16-KiB window, complete virtualization, and scale certification
  remain open.

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
- [x] Implement true requested-range current-revision certification. Pending
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
