# Flark v4 build plan

**Execution contract for
[RFC 026](../rfc/rfc_026_flark_v4_product_architecture.md) as amended by
[RFC 027](../rfc/rfc_027_continuously_rendered_markdown.md).** 2026-08-08.

This plan builds a headless Dart `flark_core` over the selected incremental
Rust engine, then builds the Flutter product `flark` on top. The first proof and
all initial performance work run on the available Mac. Android and iOS claims
wait for physical devices; Windows follows later.

## 2026-08-11 continuously-rendered product correction

The first real dogfood pass falsified the prototype's focus behavior. The
surface currently sends every pan gesture into selection and restores exact
Markdown whenever a row becomes active. Those are implementation facts, not
accepted product behavior.

[RFC 027](../rfc/rfc_027_continuously_rendered_markdown.md) and the normative
[live projection v2 contract](contracts/live_projection_v2.md) now control the
surface:

- valid current Markdown remains rendered while focused and edited;
- incomplete, composing, pending, source-only, and faulted ranges use local
  exact-source islands instead of a whole active-row reveal;
- source/display positions use typed legal caret stops plus affinity;
- desktop scroll gestures never mutate selection, and mobile gesture behavior
  is one shared Flutter policy with platform adapters only for real deltas;
- `FlarkEditor` and `FlarkMarkdownView` are separate public widgets over one
  internal projection, layout, and paint path; and
- the exact Markdown input window and Rust source authority remain unchanged.

`flark-live-v1` is frozen historical evidence for the passive-rendered/
active-source prototype. It is not a launch gate. T1 materializes
`flark-live-v2`; T2 proves continuously rendered inline editing and corrected
gesture arbitration; T3 closes input/selection truth; T4 covers blocks,
semantic objects, and tables; T5 performs scale, accessibility, and platform
hardening. Full `verify_v4.sh` runs at the T2, T4, and T5 integration
checkpoints rather than after every small implementation step.

Ordinary source edits inside rendered constructs must not flash an entire raw
row. T2 first measures the existing bounded commit/pump/query path. A new
Rust-authored edit-presentation continuity receipt is permitted only if that
measurement shows current projection cannot be served by the paint deadline;
Flutter may never infer continuity by carrying old Markdown facts forward.

### T2 execution result

The recertification-only spike was falsified and the permitted receipt is now
implemented end to end. Rust publishes conservative per-fact continuity,
`flark_core` validates and binds it to the exact transaction, and Flutter keeps
only that authorized content projected until a covering certified viewport
arrives. Syntax-like input and constructs with non-local validity fail closed.

The dedicated `FlarkMarkdownView` and editor share the same bounded render
surface. Focus no longer reveals inline markers, hidden-boundary deletion and
platform selection normalize through source/display mappings, and mouse versus
trackpad arbitration prevents scrolling from changing selection. The product
tour exposes Edit and Read modes for dogfood.

The T2 profile-mode Mac development receipt used 20 warmups and 120 measured
edits against a 1 MiB dense-inline fixture. It recorded zero raw or missing
projected frames; editor-attributed p99 was 3.481 ms with no editor-attributed
over-budget samples. Flutter failed to foreground the final harness run, so
wall-clock frame latency remains unclaimed until a controlled visible session.
This is a dogfood checkpoint, not the complete T5 matrix or mobile proof.
The authoritative `./scripts/verify_v4.sh` gate passes at this checkpoint.

## 2026-08-11 conformance-profile update

The active semantic product profile is now unambiguous: official GFM
0.29-gfm is the sole normative corpus, with all 672 numbered examples owned by
`flark-gfm-0.29-v2`. The imported 670-case corpus is supplemented by the two
official task-list examples it omitted. CommonMark 0.31.2 remains a separate
diagnostic compatibility ledger; it cannot change a GFM pass or failure.
Live editor projection is separately versioned and cannot be counted as
semantic conformance. `flark-live-v1` records the historical prototype;
`flark-live-v2` is the active product contract.

The production parser now executes the complete static GFM denominator through
one fail-closed receipt. The selected block controller, live reference
resolver, bounded typed inline projector, and bounded typed table projector are
672 exact, 0 missing, and 0 divergent. Projection leaves fail closed above
8 KiB and table output is capped at 512 semantic facts per leaf; no Markdown
recognition moved into Dart or Flutter. The runtime now carries typed table
cells and alignment through the C ABI and `flark_core` to the passive Flutter
surface. At this historical checkpoint an active table remained exact source
Markdown; RFC 027 explicitly rejects that as the final supported table UX. The
separate CommonMark compatibility receipt is 652 exact, 0 missing, and 0
divergent.

The normative incremental ledger now applies all six type, erase, split,
merge, paste, and incomplete-syntax histories to every GFM example: 4,032
edits. Every target matches a clean selected-profile Green semantic digest and
reference resolution. 4,027 edits converge through local adoption; five pinned
paste cases use the contract-required clean fallback because the edit precedes
retained reference-definition coverage. The maximum source read by a completed
local adoption in this corpus is 274 bytes.

The first GFM run also exposed and fixed a real coroutine replay defect: a
nested task-list opener could advance the line cursor, pause for reference
prefix work, then replay the same stage and underflow. List openers now resolve
that potentially pausing paragraph finalization before consuming the marker,
while preserving same-list reuse. The official nested task-list example and
the prior complete CommonMark receipt both pass. The completed incremental
matrix additionally found and fixed a multibyte reference replay probe that
could split a UTF-8 scalar, a deleted-newline convergence cut that landed
inside a target line, and unclaimed blank list continuation indentation inside
a fenced code block.

`scripts/verify_v4_markdown_conformance.sh` reproduces the semantic receipts,
and the everyday `scripts/verify_v4.sh` gate now includes it. The immutable M0
v1 profile and receipt remain historical evidence; the v2 manifests are the
active execution contract.

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
Static and incremental GFM receipts are complete. The live-projection behavior
matrix and claim-eligible multi-shape, multi-size performance receipts remain
open.

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

The second tranche moves canonical editing policy into `flark_core`, where
RFC 026 section 5 places it. The new headless `FlarkCoreEditorSession` owns
undo/redo ordering and grouping over opaque native history tokens — typing
coalescing inside the one-second idle window, composition grouping with
commit-joins-group semantics, grouped replay with exact rollback, and typed
replayed/dropped outcomes that carry the selection snapshot to restore — plus
the grapheme policy (`characters` 1.4.1, exactly pinned) as pure bounded-
context functions, and the anchor-backed canonical selection: collapsed
carets follow insertions at the caret, range edges exclude them, snapshots
are generation- and revision-stamped, and an opaque adapter payload rides
through history restoration. Dart bindings, worker-isolate protocol, and
typed wrappers now cover anchor create/resolve/release and session
inspection.

The Flutter controller is now an adapter over that session: its history
stacks, replay/rollback machinery, typing/composition grouping, and direct
`characters` use are deleted (354 lines replaced by delegation; the
controller no longer imports a grapheme library), while it keeps the bounded
input window, optimistic echo, and viewport mechanics that RFC 026 section 6
assigns to the surface. Seven new headless `flark_core` regressions prove
coalescing, epoch and idle breaks, composition grouping, composition-end
tracking, anchor-backed selection surviving edits and replay by affinity, and
disabled-budget honesty; the full verify_v4 gate passes with every existing
Flutter behavior test unchanged. Still open in this milestone: the
connection/window-epoch input state machine with hash-chained delta batches
and resynchronization from `input_window_matrix_v1`, adapter adoption of the
anchored selection for cross-window authority, and IME evidence beyond
simulated composition. No new performance claim follows.

The macOS foreground harness is now fixed and verified. The example
application fronts itself and holds a latency-critical, display-awake
activity; `scripts/profile_v4_macos.sh` additionally wakes the display and
wraps the drive in `caffeinate`. The frame harness instruments inter-frame
vsync gaps and hard-fails any run whose wall samples show a throttled cadence
or a quiet display, because diagnosis runs proved both failure modes real: a
first run recorded a 560-second hole and a later one an 827-second hole whose
vsync-gap instrumentation matched exactly — the display was sleeping, not the
editor stalling. Under the fixed harness the 1 MiB ordinary typing receipt is
clean for the first time: 120 samples, input-to-frame 8.34 ms p50, 10.07 ms
p99, 11.15 ms maximum with zero samples at or above 16 ms on the 120 Hz
display, build 2.09 ms p99, raster 1.39 ms p99, and a live vsync for the
whole run. This is a development receipt on this Mac, not the M4/M6
provenance artifact.

The same fixed harness then caught a real bounded-surface violation the
rejected runs could never isolate: after a 32-KiB single-line paste into the
1 MiB fixture, the ready-state full-range viewport refresh installed a
32-row page containing the pasted 32,768-character physical line, so
`visibleSource` reached 33,583 UTF-16 code units against the 16,384 cap.
Row count bounds a page; byte length did not. Every viewport page request
now enforces the visible-byte budget in every parse state, so a row crossing
the requested boundary stays exact-source neutral until giant-line
fragmentation lands; a probe confirmed the visible cache pins at the cap and
the full suite is unchanged.

The paste profile gate itself remains honestly red with the surface bound
fixed, and its remaining blockers are now characterized from four
instrumented runs. First, per-cycle phase timing proved the native path
innocent: undo settles stay under 14 ms and paste settles track frame
delivery almost exactly, so the wall cost is frame delivery, not engine
work. The early-cycle post-paste frames follow a warm-up curve — roughly
48-54 ms for the first cycles, decaying to a steady 7.3-9.9 ms once display
activity sustains — which matches an adaptive-refresh display serving
first-paint-after-idle from its low-power cadence and training back to full
rate only under sustained activity. Trained-state paste-during-active-
session receipts therefore sit in the same next-frame band as typing; the
first-paint-after-idle cost is a platform characteristic the certification
harness must either control with an engine frame-rate hint or evaluate
against the contract's actual-display-period rule rather than the 16 ms
constant. Second, repeated multi-second frame-quiet holes (34 s, then 56 s)
appeared mid-run even under caffeinate with display-sleep assertions held,
alongside inflated raster maxima: this bench is currently not a controlled
measurement environment, and the harness's validity gate correctly refuses
to bless those runs. Claim-eligible paste receipts are deferred to a
controlled bench session — which pairs with the live-IME evidence that
already requires a human at the machine.

A development sweep across three document sizes (1, 5, 10 MiB) and three
content shapes (ordinary prose, one giant physical line, many tiny blocks)
running the typing workload produced the first multi-shape evidence, and it
found real defects that 1 MiB ordinary-prose measurement had hidden.

The size-independence claim holds empirically: the body of the sample
distribution measured 8.31-8.33 ms p50 in every cell that produced a
receipt, independent of both size and shape. Ordinary prose is clean at all
three sizes (p99 8.92-9.93 ms). That is the architectural result the bounded
window, byte-capped visible cache, and fragmented layout were built to
produce.

Three findings came out of the hostile shapes. First, one giant physical
line degraded typing badly: at 5 MiB every one of 120 samples measured
50-62 ms, and at 10 MiB nineteen samples measured 48-57 ms with raster
p99 at 21.16 ms. The cause was a defect in the fragmentation work recorded
above: the below-fold layout budget was evaluated once per row, and a giant
line is a single row, so its whole visible length was laid out every frame
regardless of the viewport. The budget is now evaluated per fragment, with
unlaid fragments counted and height-estimated exactly like skipped rows, and
the regression suite now asserts the within-row property (a giant row
accounts for every fragment as laid out or skipped, skips at least one, and
materializes more when the viewport actually grows — the earlier test
asserted a fragment count that encoded the unbounded behavior, and its
"taller viewport" case was void because a widget test surface clamps to
800x600 unless resized explicitly). Second, many tiny blocks at 1 MiB
produced editor-attributed over-budget frames: p99 23.35 ms and maximum
29.32 ms with raster p99 15.35 ms, which no display artifact explains.
Third, many tiny blocks at 5 and 10 MiB fail outright, producing no receipt
on either attempt — a reproducible hard failure, not a timing problem.

Two process corrections are recorded with those findings. A mid-sweep claim
that the giant-line result was environmental was premature: it rested on one
contradicting retry, and the 5 MiB cell then reproduced the signature on
both attempts. And a threshold that labels any sample at or above 30 ms
"display-attributed" is not attribution — the 5 MiB giant-line cell had all
120 samples above it from a real defect. Per-sample attribution against the
frame stream, which the certification contract already requires in the form
of editor-attributed misses, is the instrumentation this harness still owes.

The per-fragment budget fix is confirmed by measurement rather than argued:
re-running the worst cell, 5 MiB giant-line, moved it from 120 of 120
samples at 50-62 ms to 8.34 ms p50 with 106 of 120 samples inside the frame
budget. Fourteen outliers remain in that cell and are the subject of the
attribution work below.

The harness now classifies every over-budget sample instead of thresholding
it. Each sample records the engine vsync stamp of the frame it proved, and
after the run each over-budget sample is joined to that frame and labelled
`editor` (the frame's own build plus raster reached the budget),
`display` (the vsync gap preceding it explains the wall time), or
`unexplained` (a frame was served promptly and cheaply, yet the sample still
waited). The third bucket is the one that would indicate scheduling
starvation, and it is now measured rather than assumed.

That attribution then falsified the scheduling-starvation hypothesis rather
than confirming it. On the re-measured 5 MiB giant-line cell, **zero**
over-budget samples were editor-attributed: the editor's own build plus
raster measured 1.3-1.8 ms on every one of them. All eleven outliers fell in
the first eleven samples, where the vsync record shows the display
delivering frame pairs at roughly ten hertz before ramping to full rate;
after the ramp every sample was inside budget. The editor was never the
cause, and no scheduler change is warranted.

A second attributed run then settled what the bench actually is. That run
served roughly ten hertz for its entire duration: 117 of 120 samples were
over budget, every one display-attributed, with the editor's own build plus
raster a flat 1.2-1.4 ms throughout. The preceding run, on identical code
and fixture, ramped to full rate and measured 8.34 ms p50. The difference is
the panel, not the editor. This hardware's adaptive refresh range bottoms
out at ten hertz, and an unattended machine driven only by synthetic input
never leaves that idle rate, so the display period alone can be 100 ms while
the editor is spending barely one.

Two consequences follow. First, a wall-clock input-to-frame gate is
unsatisfiable on an idling adaptive display through no fault of the editor,
because the evidence contract's own rule takes the minimum of 16 ms and the
actual frame period. Claim-eligible receipts therefore require a bench whose
served refresh rate is recorded and at least the budget rate, which an
unattended session does not provide; every such run is correctly rejected by
the validity gate rather than reported. Second, the receipt now carries a
display-independent editor-attributed latency — input handling plus the
proving frame's build and raster — together with the median served interval
and its implied refresh rate, so the editor's own cost remains measurable
and reportable even when the panel is idling.

Two further harness corrections followed from that evidence. The classifier had
labelled alternating samples `unexplained` purely because a burst-delivered
frame has a small immediately-preceding gap; it now also judges the rate
actually served over the window leading into the sample, which is the honest
measure when a display delivers in bursts. And the typing workload had no
warmups at all, so the display ramp landed inside the measured window: it
now runs the twenty warmups the evidence contract already prescribes for
sustained typing, excluded from the distribution. Neither change discards a
sample that the editor caused.

With that metric available and both runs served at full rate, the 5 MiB
typing comparison is the clearest editor-cost evidence to date. Ordinary
prose spends 1.333 ms p50 and 2.269 ms p99 of editor-attributed latency per
keystroke with zero over-budget samples and a valid run. One giant physical
line spends 2.325 ms p50 and 10.062 ms p99 with a single 49.438 ms outlier
and raster p99 at 17.815 ms: the layout defect above accounted for the
former uniform 50-62 ms wall behavior, and what remains is occasional
rasterization cost for tall wrapped fragments rather than layout work. That
residue is a candidate optimization, not a correctness gate.

Continuing on the render surface then exposed a defect well outside it. A
fragment cut is now placed on an extended-grapheme-cluster boundary rather
than merely between surrogates, so a ZWJ sequence or a combining mark can no
longer be rendered as two clusters; the boundary primitive lives in
`flark_core` beside the rest of the grapheme policy, because that decision
is not the render surface's to make. Writing a fixture out of family emoji —
eleven UTF-16 units and twenty-five UTF-8 bytes per cluster — then failed
before it could assert anything, with the viewport query returning
`RANGE_OUT_OF_BOUNDS`.

The cause was a byte cut landing inside a multi-byte scalar, in two places.
The visible-byte budget recorded above caps a viewport request at 16 KiB,
and `flark-abi` independently caps a source page against the caller's buffer
and result budget; both produced an arbitrary byte offset, and every ASCII
fixture in the suite hid it. Only the runtime knows where a cut is legal, so
the runtime now snaps a viewport request and a capped source page back to
the nearest scalar boundary instead of rejecting them: a page may cover less
than requested, which the page header already expresses, but a host capping
bytes against a budget can never be expected to know where scalars end.
Coordinate conversion is itself the boundary test, so the snap probes at
most four offsets rather than reading bytes, which a boundary-aligned read
could not have done anyway. A regression covers a mid-scalar cap through
both the certified and live projection paths.

This was a latent correctness bug for any document with non-ASCII text near
a window boundary — emoji, CJK, or accented prose — not merely for the
contrived fixture that surfaced it.

The tiny-blocks hard failure is now root-caused, and it is two defects
stacked. The engine reports a typed `PayloadBudgetExceeded` when a document
whose block count rather than byte count dominates exhausts the arena's live
payload budget: five mebibytes of four-byte blocks is over a million blocks,
and the default budget is 64 MiB, which is also exactly the memory ceiling
the evidence contract allows for that document size. The engine is therefore
reporting an honest capacity limit, and this content shape is a recorded
scale limitation rather than a crash to be patched away.

What was defective is everything after that report. The failed build was
dropped without cancellation, tripping a `Drop` assertion that panicked the
`flark-document` actor thread; the actor died silently, and every later ABI
call degraded to an anonymous `INTERNAL_FAULT` — the observed symptom was a
`coordinate_convert` internal fault with no mention of the real cause. That
also violated the ABI contract's panic-containment rule, which the actor
thread had never satisfied: `catch_unwind` guarded the ABI entrypoint, but a
panic on the actor's own thread crossed no barrier at all.

The actor now runs every job behind a panic barrier. A contained unwind
discards the session rather than reusing state whose invariants a partial
mutation may have broken, poisons that actor so later calls report the same
typed fault, and runs the discarded session's destructor behind the same
barrier so a failing destructor cannot abort the process either. `flark-abi`
maps the contained unwind to `PANIC_CONTAINED` as the contract requires
instead of collapsing it into an internal fault. Two regressions pin the
behaviour: the payload budget surfaces as a typed parser error naming the
budget, and a deliberately panicking job is contained, reported, and leaves
an actor that still answers and drops cleanly. End to end, the shape that
previously killed the actor now returns `PARSER_FAULT` with no panic at all.

Writing those regressions surfaced a related hazard worth recording: a
faulted `DocumentSession` cannot be dropped bare, because the runtime's own
destructor asserts that it must be explicitly closed and fuel-drained. In
production the actor is the only owner and now contains that, but it means
a faulted session's reclamation runs through the contained path rather than
the bounded close state machine, so exactly-zero-live-state after a fault is
not yet proven. Both regressions therefore drive the actor rather than a
bare session, which is the production ownership.

The remaining open items from the sweep are the giant-line raster spike, the
tiny-blocks over-budget raster cost, the engine's per-block payload overhead
for block-dense documents, and a now-falsified frame-scheduling suspicion
retained here only as a record: `_finishParsing` pumps the worker in
a free-running `while (!ready) await pump()` loop. That hypothesis is not
yet supported: each pump awaits a worker-isolate reply, which already yields
to the event loop, so frames have an opportunity to interleave. The
competing explanations are a single native pump turn that runs long on a
large document and an engine-level frame-delivery stall. RFC 026 section 6
assigns frame scheduling and work-budget allocation to `flark`, so bounded
per-frame pumping is the likely shape of the fix, but the attribution data
decides which mechanism is actually at fault before any scheduler change
lands.

The render surface now enforces its two remaining layout bounds. One laid-out
painter never holds more than 2,048 UTF-16 units: a row beyond that budget is
emitted as stacked surrogate-safe fragments with exact offset mapping, so
selection boxes, the caret, and hit testing stay correct across fragment
boundaries while a giant physical line can no longer force full-block layout
on the frame path. Rows starting below the viewport plus a 400-pixel overscan
are not laid out at all — their height is estimated until scrolling toward
them triggers materialization — so offscreen layout is no longer built for
the below-fold portion of a page. The active row keeps its separate
2 Ki transient paint cap. Two focused regressions prove the fragment bound
with deep hit-test monotonicity and caret placement inside a passive giant
line, and below-fold skip counts with scroll-driven materialization; the
existing 23 Flutter behavior tests are unchanged. Grapheme-cluster-perfect
fragment boundaries (beyond surrogate safety), lazy per-fragment layout
inside one giant row, and cross-page selection/navigation remain open in
this milestone.

One intermittent native fault also surfaced once and is under armed watch:
a single run's first bulk commit returned INTERNAL_FAULT and could not be
reproduced headlessly (paste-after-full-parse with a live viewport
continuation replays cleanly) or in two subsequent driven runs. Fault-path
diagnostics now name every internal-fault source on the bulk and retention
paths when they fire, and the Dart-side history-coherence failure carries a
distinct detail code so it can no longer be confused with a native fault.
The next occurrence will identify itself.

The third tranche implements the executable core of that input-window state
machine. The controller now maintains the contract's serialized shadow —
connection epoch, window epoch, represented revision, exposed range, SHA-256
window-text identity, and selection generation — reconciled at a single
notification choke point so no window-rewrite site can bypass the epoch
discipline: platform-accepted updates advance the window epoch on the active
connection, while any host-originated change to the exposed text, range, or
selection retires the connection and mints a strictly increasing epoch with
the window epoch reset to one. A platform delta batch is validated completely
before anything applies — the first delta's old-text hash against the shadow,
each later delta's old hash against the prior delta's new hash, every range
and selection against the simulated window, and the whole-batch small-edit
envelope — so a bad or over-cap second delta can no longer leave the first
applied, which the previous per-delta loop permitted. A rejected callback
mutates nothing and resynchronizes with a typed reason
(old-text/chain/range/envelope), re-exposing the unchanged window on a fresh
connection.

Cross-window selection authority now uses the anchored canonical selection: a
range larger than the 16 Ki input window installs `flark_core` anchors and
exposes only a collapsed active-extent surrogate; command-path replacement and
deletion against the surrogate resolve the anchors at commit time and replace
the complete exact global selection atomically as one revision, with the
caret restored through the bounded async window fetch; user activation
abandons the oversized selection and releases its anchors. Seven focused
regressions drive the real engine through shadow truthfulness, chain-reject
and stale-old-text atomicity, out-of-window rejection, host-retire and
full-value-fallback epochs, strictly increasing resynchronization epochs, and
the oversized-selection install/replace/undo cycle; the complete verify_v4
gate passes. Still open: platform-connection reopen choreography in the
widget layer, typing-at-surrogate and composition interaction with oversized
selections, the composition-pinned and bulk-staging window states with
deferred movement, grapheme needs-more-context expansion, and real IME
evidence. No new performance claim follows.

## 2026-08-10 execution update

The post-handoff corrective tranche closes the four concrete engine/surface
gaps that could be resolved unattended on this Mac.

- Faulted actors no longer fall back to a bare `DocumentSession` drop. The
  contained discard path fuel-drains `close`, and a reduced-budget regression
  proves a typed parser capacity fault followed by `Closed` with zero source
  bytes. Pre-cleanup arena counters remain available for diagnosis.
- Recursive-Green storage now uses canonical minimal varints for common event
  fields, permits up to 512 compact events per already-bounded 4 KiB page, and
  stores cached row geometry in a versioned variable-width trailer while still
  decoding the previous fixed-width form. Local splice/replay work remains
  page-bounded under the denser geometry.
- The separate certification lane now requires a five-mebibyte source made of
  four-byte blocks — more than 1.3 million blocks — to reach `Ready` within the
  unchanged 64 MiB admitted-payload contract and then close to zero state. That
  lane passed in debug mode in 324.98 seconds. Block density is therefore no
  longer a recorded 4-5 MiB capacity limitation; it remains a deliberately
  slow certification check, not part of the everyday gate.
- Passive giant-line fragments are now normally capped at 256 UTF-16 units on
  extended-grapheme boundaries; one indivisible cluster may exceed the cap.
  On the exact 5 MiB giant-line development case, raster moved from 17.815 ms
  p99 to 1.033 ms p99 (2.987 ms maximum), build measured 1.218 ms p99, and
  editor latency measured 2.370 ms p99 with zero editor-attributed misses. The
  panel served only 20 Hz and the foreground check failed, so this is defect-
  resolution evidence, not a claim-eligible jank receipt.

Cross-page selection, exact copy/cut, replacement, undo, grapheme-safe
fragmentation, atomic platform delta batches, and canonical core selection are
also complete in the active v4 path. The everyday gate remains
`scripts/verify_v4.sh`; the large density proof remains
`scripts/verify_v4_certification_stress.sh`.

The remaining evidence boundary is intentionally narrow: pin the exact
CommonMark/GFM/live-projection profile and execute its conformance lane; run a
controlled foreground Mac session for claim-eligible performance and real IME;
and keep the one unreproduced bulk `INTERNAL_FAULT` diagnostic armed until a
reoccurrence identifies its source or the controlled session clears the
targeted workload. Android/iOS device qualification and the eventual legacy
identity cutover remain later milestones.

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
- CommonMark semantic compatibility: 652 exact, 0 missing, 0 divergent.
- Normative GFM 0.29-gfm semantic replay: 672 exact, 0 missing, 0 divergent.
- Normative GFM incrementality: all six histories for all 672 cases are clean
  oracle-exact; 4,027 use local adoption and five use the explicit
  edit-before-reference-coverage clean fallback.
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
- [x] Pin official GFM 0.29-gfm as the normative product profile and CommonMark
  0.31.2 as a diagnostic compatibility profile, including an explicit
  deviation policy and separate semantic/incremental ledgers.
- [x] Version the separate `flark-live-v1` projection profile and matrix:
  incomplete syntax,
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
- [ ] Keep certified syntax markers hidden across focus and editing; expose
  only the local exact-source states permitted by `flark-live-v2`.
- [ ] Use one internal projection/layout/paint path for `FlarkEditor` and the
  dedicated `FlarkMarkdownView` read-only widget.
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

- [x] Make the selected GFM profile executable and versioned; its first receipt
  is an implementation-gap ledger, not a conformance claim.
- [x] Close semantic CommonMark cases and GFM extensions using parser-owned
  logic only.
- [x] Add edit histories for every construct: type, erase, split, merge, paste,
  incomplete syntax, and non-local dependency changes.
- [x] Keep clean/incremental oracle parity and locality/resumability receipts
  separate from static conformance.
- [x] Reach zero CommonMark semantic divergence and pass every assertion in the
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
- [ ] Add link/media actions, tables/task interactions, continuously rendered
  editing/local exact-island rules, keyboard navigation, focus, shortcuts,
  themes, and shared-render-plan read-only behavior.
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
