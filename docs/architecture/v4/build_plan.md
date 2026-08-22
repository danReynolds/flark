# Flark v4 build plan

**Execution contract for
[RFC 026](../rfc/rfc_026_flark_v4_product_architecture.md) as amended by
[RFC 027](../rfc/rfc_027_continuously_rendered_markdown.md) and
[RFC 029](../rfc/rfc_029_large_document_architecture.md).** 2026-08-16.

This plan builds a headless Dart `flark_core` over the selected incremental
Rust engine, then builds the Flutter product `flark` on top. The first proof and
all initial performance work run on the available Mac. Android and iOS claims
wait for physical devices; Windows follows later.

## 2026-08-16 large-document architecture correction

The 10 MiB full-Green certification and memory receipts falsified a hidden
whole-document readiness premise. The subsequent compact-index/Green-fragment
probes established a promising replacement, but a lifecycle audit found three
more document-sized risks that must be resolved before production routing:

- the public Dart open path still encodes and commits the complete source
  before the first viewport query;
- the compact index has no proven revision-local update path and may not be
  rebuilt from BOF after every ordinary edit; and
- the current 32-row Flutter page surface is a bounded dogfood mechanism, not
  the final continuous virtual-scroll model.

[RFC 029](../rfc/rfc_029_large_document_architecture.md) now controls this
work. It selects bounded source admission, a persistent revision-shared compact
index, certified Green fragments, and a source-anchored continuous virtual
viewport. It also reconciles the continuously-rendered promise with constructs
whose final GFM meaning genuinely requires later source.

The next work is deliberately risk-first: Experiment A proves first viewport
publication before complete source admission, then Experiment B proves local
index convergence and suffix sharing across edits. No compact-session ABI or
Flutter production cutover begins merely because the existing first-slice and
cold-jump engine probes pass.

### Experiment A engine receipt (2026-08-17)

RFC 029 Experiment A1 and the parser half of A2 now have passing engine
receipts, feature-gated (`progressive-source-probe`, `m11-compact-probe`) and
unreachable from production sessions.

A1 source lifecycle: `OpeningSourceStore` publishes immutable exact prefix
roots through structural sharing under separate load/generation/edit-revision
identities; an admitted-prefix edit advances only the edit revision while the
append frontier re-anchors; append lineage is provable only through
store-minted move-only proofs; sealing promotes the current root without
copying or reparsing (`seal_reused_root` asserted). Release probe on the
development Mac: first 512 KiB frontier in 0.35–0.63 ms, total admission at
575–1,165 MiB/s across 1/10/40 MiB with one interleaved admitted-prefix edit.

A2 parser lifecycle (fixed-source frontier simulation): the one primary
parser suspends at cursor starvation (`CleanPhase::Starved`) with a
checkpoint and source baton instead of treating exhaustion as EOF, resumes
across authenticated line frontiers, and seals explicitly. Differential
suites prove starvation/resume equality with a clean parse across Setext
lookahead, a late reference definition, and a multi-starvation open fence;
early certification stays fail-closed behind the reference-hazard scan.
Release probe: certified slice with inline facts engine-ready in
0.33–0.52 ms at 2,240 admitted bytes, independent of 1/10/40 MiB total size;
EOF compact indexing 115.6 ms / 0.99 s / 4.01 s. The 10 MiB result straddles
the 1-second Mac gate within run-to-run noise, and 40 MiB costs 4.05x the
10 MiB time, so scaling is linear through the 4x detector tier.

Two caveats keep these engine-only: the parser probe slices a fully resident
source rather than consuming `OpeningSourceStore` appends (the
`adopt_progressive_opening_append` seam exists with no callers yet), and the
early-certification fixture is deliberately link-free. The A2 wiring tranche
must add the opening-store-driven differential — including a split-CRLF page
boundary — and the A3 ordinary fixture must contain links, which requires an
admitted-first-winner reference proof (a use whose label already has an
admitted definition is final under the GFM first-winner rule).

Two corrections landed with this receipt. An unsealed frontier ending in a
bare CR is now inadmissible: a later LF may still join it into one CRLF
ending, and the LF alone would scan as a phantom blank line, so
`is_unsealed_physical_line_frontier` answers the frontier question separately
from the sealed line-start rule. And the authenticated incremental-adoption
regression was restored as an in-crate unit test after the a210a12 cutover
had silently broken `cargo test -p flark-parser` compilation; the everyday
gate now checks every first-party crate target so a target that stops
compiling fails visibly.

The standing scale audit for this correction is
[scale_alignment_audit_2026-08-17.md](scale_alignment_audit_2026-08-17.md):
a work-class ledger over every existing and planned operation, the ranked
flags (nested cold-jump constant, page relocatability, accessibility gate,
whole-document query class), and the recurrence-prevention rules adopted
into section 2 below.

### Experiment B relocatability receipt (2026-08-17)

The rule-11 falsification probe for compact-page relocatability ran before
any convergence machinery was designed, and it falsified the risk in the
favorable direction. `compact_checkpoint_relocatability_under_bof_insertion`
(release mode, `--ignored`, in-crate) builds the compact checkpoint journal
for a 10 MiB fixture, rebuilds it for the same source with one byte inserted
at BOF inside the opening paragraph, aligns checkpoints by line ordinal, and
compares raw durable payload bytes plus manifest metadata.

Ordinary cell: 2,532 checkpoints in both builds, all aligned, zero
unmatched. Nested cell (5.5 KiB quotes and 5 KiB lists spanning the 4 KiB
stride, so 1,013 of 1,013 cuts sit inside open containers and 1,012 carry
the bounded writer restart record): all aligned, zero unmatched. In both
cells every parser payload and every writer payload is byte-identical
between the two builds, both encoded streams have identical length
(445,616 and 178,320 bytes), and every manifest entry differs exactly by
the uniform +1 byte/+1 UTF-16 shift with equal event cuts, row counts,
line ordinals, frame identities, and open depths.

Three conclusions bind Experiment B's design:

- The durable payload codec is already translation-invariant, including the
  writer open-frame record whose absolute block starts are reconstructed
  from manifest authority rather than baked into payload bytes. Suffix
  sharing may reuse payload pages as stored; no record re-encoding layer is
  required for these shapes.
- Checkpoint selection is shift-stable: both builds selected the same
  logical boundaries, so suffix sharing does not fight cadence churn.
- The remaining storage design work is the manifest, exactly as RFC 029
  section 5 prescribes: roughly 2,500 entries of absolute coordinates per
  10 MiB cannot be rewritten record-by-record per edit, so the entry index
  becomes the aggregate measure tree (per-entry deltas at leaves, summed
  measures at nodes, O(log n) updates) with structural sharing.

Both recorded follow-ups closed the same day. The multibyte cell (a
two-byte, one-unit scalar inserted at BOF) keeps all 2,532 checkpoint
payloads byte-identical while every manifest entry shifts by exactly +2
bytes and +1 UTF-16 unit: the coordinate dimensions relocate
independently. The reference cell measures the compact reference index
under the same edit: 22,901 records, zero unmatched, zero identical, zero
structural, 22,901 uniformly shifted with stable digests, labels, and
winner ordinals. Reference records therefore do carry absolute coordinates
in their payload — unlike checkpoint pages they cannot share as stored —
but the perfect uniformity proves the remap is well-defined, so Experiment
B's storage design splits cleanly: checkpoint payload pages share as
stored, and reference records take the same measure-tree/indirection
treatment as the entry manifest. The probe is retained `#[ignore]`d
in-crate as the regression check for any future durable-codec change.

### Experiment A loop-closure receipt (2026-08-17)

The append-adoption seam now has callers and a differential.
`build_m11_progressive_open_probe` drives the one compact primary parser
over source admitted for real through an `OpeningSourceStore`: the runtime
replica advances only through store-minted append proofs, the parser
frontier trails the admitted frontier at line granularity (unterminated and
CR-ambiguous tails wait for later input or the seal), an append with no
complete line advances source/writer authority while the parser stays
starved, and unknown-length streams (`expected_input_utf16: None`) end only
at an explicit seal. The store-driven differential proves clean-parse
equality across CRLF endings split between transport pages, mid-word page
cuts, a fence spanning pages, late reference resolution, and an
unterminated sealed tail. The early pre-EOF viewport remains
generation-bound: querying it after later appends fails closed with a
source-authority mismatch, which is the required behavior; carrying it
forward through append receipts is A3 continuity work, not a default.

Closing the loop surfaced two real defects, both fixed with regressions:
a first resume from an empty admission bypassed the controller's initial
document-open command (both resume sites now select the ControllerLine
phase until the initial boundary is captured), and per-append root
retirement was never drained, so a 128-page load failed with typed
`RetirementBackpressure` (the drive now drains one grant per adoption; a
failed drive cancels the build and releases the early viewport so the
typed error survives instead of a root-release drop assertion). The
production document actor must own the same reclamation work class.

True-path release numbers on the development Mac (8 Ki UTF-16 pages,
unknown length, admission included in the timer): first certified slice
0.358 ms at 1 MiB and 0.341 ms at 10 MiB; complete admission plus EOF
compact indexing 98.7 ms and 1,034.8 ms with 130 and 1,282 starvation
cycles — about 3% protocol overhead against the fixed-source simulation,
and the 10 MiB result again straddles the 1-second Mac gate with the known
~30% sink headroom still unexploited.

### Admitted-first-winner reference certification (2026-08-17)

Early certification no longer defers on every bracket. The compact
reference resolver now carries typed authority: `Final` (EOF), where a
missing label is authoritatively literal, and `CommittedPrefix`, where
present winners are final under the GFM first-winner rule — every earlier
position is already admitted, so a later duplicate always loses — but a
missing label returns the new fail-closed
`M11ReferenceResolution::Unknown`. Absence before EOF is never
literalness: the inline reference stage treats `Unknown` exactly like the
oversized-value case, revoking the whole-leaf bracket certificate so the
leaf fails closed to neutral, and a prefix resolver therefore cannot emit
literal-text facts a suffix could falsify.

Certification is now an audit, not a byte scan. Every inline-bearing row
of the candidate slice is captured against the committed-prefix resolver,
which shares one `Unknown` counter across clones; a capture that consumed
an `Unknown` lookup defers certification, other fail-closed captures
certify because the eventual viewport refuses them identically, and rows
without an inline-leaf fence defer only on bracket bytes not accounted
for by a committed definition's exact source range. Differentials: direct
links, escaped brackets, and inline code spans certify before EOF with
cooked link values equal to the eventual viewport (previously every such
slice deferred); the late-definition fixture still refuses certification;
and the opening-store path certifies direct-linked CRLF paragraphs across
real appends.

One pre-existing hole was exposed by the new differential and closed the
same day: a first slice whose range covered *leading* reference
definitions could not build a bounded Green slice, because a removed
reference window's events were discarded rather than flushed — including
the hidden coverage for the definition bytes. The window's rewritten
journal is already the final Green form (physical coverage preserved by
the rewrite invariant, no renderable row), so the fix flushes it into the
first-slice candidate before the buffer clears. Documents opening with
definitions now build their slice, certify early through the audit
(definition brackets are inside committed record ranges), and resolve
uses to the admitted first winner with a later duplicate losing — proven
by the flipped differential against the eventual full-authority viewport.
Error paths in the slice builder and the audit now cancel their builders,
so every failure in this area surfaces as its typed error rather than a
root or builder drop assertion. The early-path degradation for any future
coverage gap remains in place as fail-closed insurance.

### Session-layer progressive open receipt (2026-08-17)

The A3 vertical now reaches the runtime session layer. `DocumentSession`
gains a feature-gated opening mode (`opening-session`; the production
`begin()` path and the default build are unchanged): the incremental
parser open session from the previous receipt is driven by the ordinary
pump under the session state machine, transport pages adopt in one
authenticated step at parser starvation, `query_live_viewport` serves the
certified early viewport as a certified span with exact pending source
elsewhere, and `query_viewport` serves complete mapped
`DocumentViewportRow` payloads clamped to the certified range — with
`total_rows` reporting only the known prefix and `complete=false`,
because a pre-EOF count is never an exact total. Literal edits during
load mutate the opening store and rebuild the replica and parser session
from the post-edit snapshot (restart, not convergence: load-time edits
trade locality for correctness until Experiment B).

The headless dress rehearsal proves the full A3 lifecycle through real
session calls: paged admission, certified rows before EOF, a mid-load
edit with stale-revision rejection and recertification, sealing, and
final viewport rows byte-equal to the complete-source oracle including
the load-time edit.

Release probe, session layer, ordinary 10 MiB with 8 KiB pages: **first
certified 32-row viewport in 2.874 ms with 32,768 bytes admitted** —
the session, store-proof, pump, and query machinery add roughly 2.5 ms
over the raw engine number, leaving ~70x headroom against the 200 ms
public gate before the ABI, Dart, and Flutter layers take their share.
Pump-to-Ready measures 2.99 s because the post-seal path still runs the
old full-Green build for full editing semantics; that is background work
outside the first-visibility gate and is exactly what Experiment B
replaces. Remaining for the A3 receipt: the ABI opening transaction,
`flark_core` streaming admission, Flutter paint, and the frozen
five-run physical measurement.

### Hostile-shape sweep of the revision updater (2026-08-17)

The rule-12 detector pass over Experiment B's own results, run before any
integration consumed them: the same convergence gates against hostile
shapes instead of uniform prose, plus certification differentials for
nested and table first screenfuls.

Confirmed. Nested quote/list (10,369 bytes replayed — the open-container
closure widens the window, still 6x inside the 64 KiB gate), CRLF
endings, multibyte content (replace and a +2-byte/+1-unit insertion
through the remap), and GFM tables at mid and BOF (4,107–4,132 bytes)
all converge on the first candidate with zero checkpoints replaced, zero
pages appended, and entry-by-entry equality against clean rebuilds — the
uniform-fixture results were not fixture artifacts. Nested and table
heads certify before EOF with early facts equal to the eventual
viewport. The clustered-reference cell proves the reference carry path
at density when the replayed window holds no definition. The
spanning-fence adversary replays 525,534 bytes from its resumable
predecessor — the honest, printed cost of the no-restart-record class —
and still converges with full equality.

Two real findings, pinned typed in the probe rather than worked around:

- **Interleaved-definition rebuild cliff.** A document that interleaves
  definitions with prose has definitions in every possible replay
  window, so v1 pays the declared whole-document reference rebuild on
  every edit (~hundreds of ms at 2 MiB). Correct, honest, and the named
  section 5.2 fix is label-scoped invalidation: revalidate the window's
  unchanged definitions by range equality under the remap instead of
  keying the rebuild on window intersection alone.
- **Definition-run base-journal defect.** A consecutive definition run
  spanning checkpoint strides produces a base journal that violates its
  own metadata/stream monotonicity validation; the updater rejects it
  with a typed error instead of converging over a corrupt base. This is
  a pre-existing capture-discipline defect in the journal across
  reference windows, exposed by the updater's begin validation; those
  documents keep clean-rebuild behavior until the journal fix lands.

**Structural-edit stance, made explicit.** The v1 authority split is a
declared hybrid: compact convergence owns structure-preserving edits and
the load/scale path; persistent-Green adoption remains the editing
authority for structural edits (Enter, merges, paste), where its
4,032-edit ledger already proves locality. An edit whose declared deltas
change frame identities fails closed in the compact updater to a
bounded, still-equal full replay — proven by the declining structural
cell — so a wrong declaration can cost time, never correctness.
Frame-identity translation is the named B2 completion item; the
editing-authority cutover (and with it the full deletion of the
post-seal build) is gated on B2 plus the two findings above.

### Experiment B revision-locality receipt (2026-08-17): partial

The compact index now updates through bounded convergence instead of BOF
rebuilds. The build follows the measurement-determined design exactly: a
piecewise revision remap layer (the section 5 explicit remapping layer)
carries every absolute coordinate dimension; the convergence updater
selects the nearest resumable predecessor on the parser cut (walking back
past entries without a bounded writer restart), resumes the real primary
parser through the durable-decode machinery, and tests convergence at
every replayed line boundary against the next candidate's remapped cut —
convergence equality is encoded parser and writer payload byte equality
plus remap-consistent manifest metadata, which the relocatability
receipts proved is state equality through the durable codec. On
convergence the journal splices: prefix and suffix entries reuse their
payload records as stored under the remap.

The nine-cell development differential (1 and 10 MiB at BOF, middle,
and EOF; same-length replacements and a one-byte insertion; one
definition-bearing 2 MiB cell), independently re-run and reproduced:

- **Replay ceiling**: 144–4,144 bytes replayed per ordinary edit — 16x
  inside the frozen 64 KiB gate.
- **Perfect sharing**: zero checkpoints replaced and zero pages appended
  in every ordinary cell; every payload page shared as stored, with
  convergence on the first candidate every time.
- **Size independence**: update cost 0.22–7.8 ms with identical replay
  bytes at 1 and 10 MiB, against clean rebuilds of 100 ms and 1.03 s —
  a ~200x advantage at 10 MiB that does not trend with size.
- **Equality**: all nine cells entry-by-entry equal to a clean rebuild
  of the edited source through the remap, including carried reference
  records; the definition-bearing window converges its checkpoint index
  identically in 7.8 ms and pays the declared v1 whole-document
  reference rebuild (~380 ms), recorded as named future work per
  section 5.2 rather than silent cost.

Recorded design decisions: predecessor selection on the parser cut with
a resumability walk-back; convergence tested against remapped candidate
cuts (the post-resume cadence cannot land on them by accident,
especially under insertions, and window re-emission still applies the
production stride rule); non-position deltas are caller-declared and
verified by convergence — a wrong declaration can only suppress
convergence into a bounded, still-correct full replay, proven by the
structural-edit test; frame-identity translation under a nonzero frame
delta fails closed as declared future work. This closes the ordinary 1 and
10 MiB prototype cells only. RFC 029's frozen gate remains open: peer-next and
4x detector tiers have not run, reference-winner foreground work still performs
the named whole-document rebuild instead of proving locality with reference-use
count, and B2 frame-identity translation remains incomplete. Integration and
the post-seal authority cutover cannot treat this partial receipt as their
unlock.

### Dart-layer streamed open receipt (2026-08-17)

The A3 vertical now reaches the public Dart API.
`FlarkCoreDocument.openUtf8Stream(Stream<Uint8List>, {expectedBytes})` and
the lazy `openStreaming(String)` convenience drive the opening-query C ABI
through the existing worker-isolate protocol extended with append/seal
messages: chunks transfer to the worker without accumulation (scalar-split
carry mirrored at the boundary; chunks over 64 KiB split before staging),
the worker stages and pumps per chunk, queries work mid-load through the
ordinary machinery, and stream close seals through create_commit. The
feature lane is opt-in end to end: `FLARK_V4_FEATURES=opening-session`
for the library build, `FLARK_V4_OPENING_LIBRARY_PATH` keying the gated
Dart suites, defaults byte-identical.

Development cold-open receipt (10,485,776-byte ordinary fixture, chunk
sizes cycling 8–64 KiB, Dart VM, release feature dylib, five runs,
measured from before the `openUtf8Stream` call — isolate spawn, dylib
load, negotiate, and opening create inside — to the owner isolate
observing its first fully certified viewport reply with semantic rows):
**first certified viewport in 7.7–8.8 ms steady-state and 55.4 ms on the
true-cold first run (JIT plus first dylib load), with 57,344 bytes
admitted at certification.** Open-to-ready remains ~3.4 s (seal plus the
post-seal full-Green build; the Experiment B replacement target).
Suites: full `verify_v4.sh` with the feature lane exits green end to
end; flark_core 79/79; flark 137 passed; qualification 48/48;
runtime/abi green under both feature sets.

Two native findings are recorded as named defects rather than worked
around: an opening session smaller than its first compact slice cannot
seal (`create_commit` faults typed — the final viewport path requires a
captured slice; small and empty documents need a slice-free completion
path), and a pre-certification opening semantic query cannot surface
`NOT_READY_SOURCE_GAP` through the C ABI because `runtime_error` leaves
`ProgressState::None` where outcome coherence demands `PendingSourceGap`,
collapsing to an anonymous internal fault (the Dart layer structurally
avoids the state via a bounded certification probe before row fetches).

The remaining distance to dogfooding is exactly the Flutter controller:
`FlarkEditorController.open` accepts only a complete String; its finish
loop pumps to Ready before any viewport refresh, so streaming needs a
certification-aware loop plus a streaming status; the input-window
authority needs an explicit stance on admission growth; and the frame
receipt needs a workload mode correlating first-certified-paint to
`FrameTiming`. Four named items, no unknowns.

### Edit-presentation architecture correction (2026-08-18)

The first dogfood typing session falsified the T2 continuity design in three
keystrokes: typing an emphasis run and then an ordinary space reveals the
row's raw delimiters for a frame before settling back. The cause is
contractual, not a coding slip — `live_projection_v2` itself refuses "edits
that touch an inline fact", boundary-inclusive, and delegates the per-edit
decision to a host validator that must therefore classify Markdown-sensitive
characters in Dart. That is simultaneously the source of the flicker and the
one surviving breach of the one-grammar rule.

Two replacements were considered. *Presentation lag* — always paint the last
certified styling transformed through subsequent edits — removes the flicker
entirely and deletes code, but buys that by painting styling not yet proven
for the current revision. *Literal-safe envelopes* — the parser publishes, per
row, the exact ranges in which a literal edit of a declared class provably
cannot change published facts — keeps every painted frame provably correct at
the cost of a new payload and a new proof obligation. Envelopes were selected:
the correctness bar is not for sale, and the flicker then retreats to exactly
the edits whose semantics are genuinely in question, where showing literal
source is correct feedback rather than a defect.

[RFC 027 section 4.4.1](../rfc/rfc_027_continuously_rendered_markdown.md)
carries the amendment and `live_projection_v2` carries the normative change.
The ABI 4.26 minimum removes the old row-policy ABI field, inline-fact policy
flags, and host-side character classification from the active decision path.
It publishes only ASCII letter/digit insertion envelopes when an eligible
inline fact's complete content slice is one flat non-empty ASCII word, plus a
one-shot U+0020 envelope at an eligible outer closing boundary at row end.
Punctuation, whitespace, nesting, latent syntax, and code normalization fail
closed. The old inline/table authorization functions are removed. Deletion,
replacement, non-ASCII, table-specific and broader classes, and their exhaustive
differential evidence remain pending.

ABI 4.27 adds `LITERAL_SAFE_ENVELOPE_CLOSURE_V1`. Plain parser-proved ranges may
carry ASCII word/space insertion across immediate successors: matched non-empty
envelopes and exact byte-and-UTF16 same-geometry bundles grow, unmatched
foreign-class envelopes strictly crossed by the insertion drop, before/after
ranges stay/shift, and matched zero-width envelopes are consumed. A non-empty
space envelope admits positions strictly inside its range. Reusable space
authority stops before an existing trailing-space run, and a separate terminal
zero-width proof is published only when one terminal space is safe. This keeps
the second trailing space that can form a hard line break outside the proof.
The current emitter restricts this reusable bundle to canonical single-line ATX
content with authoritative empty inline facts, identity coordinates, and ASCII
word/space content bounded by word bytes.

Query kind 6, inline-record kind 15, and edit-class codes 1 and 2 are unchanged;
the stateless boundary instead requires exact minor 4.27 plus the new bit-27
capability.

ABI 4.28 adds `PROJECTION_EDIT_CELLS_V1`, record kind 16, and exact-minor 4.28.
The first broad cell keeps a canonical ATX shell while arbitrary non-newline
plain content edits paint only the complete editable cell exactly. The first
local cell admits one parser-proved space at a flat Strong opener/content
boundary, paints only that Strong closure exactly, and retains independent
outside facts. Mounted paint evidence pins the latter: `# ` and unrelated
`_right_` never reappear, and every frame carries the accepted source generation
and exact caret identity.

ABI 4.29 adds `PROJECTION_EDIT_CELLS_V2` rather than widening the pushed 4.28
matcher vocabulary in place. The next bounded cell uses kind 16 for
parser-authored plain
literal segments in top-level paragraphs, simple list/quote content, and plain
table cells. ASCII word insertion/replacement and spaces strictly inside the
trimmed trigger chain, while a separately proven one-unit Backspace is consumed after one
edit; the block shell and independent styled facts stay rendered. A mounted north-star
matrix now imports the exact product-tour dogfood document and checks it plus styled-gap, in-fact,
after-fact, list, quote, and table shapes at both zero and human cadence. Every
accepted rune must cause an actual paint with the current source generation,
canonical source and display caret, expected rendered text, and resolved paint
styles. A separate lane admits several edits before one frame. The
same mounted lane proves dogfood Backspace and selection replacement at both
cadences. Composition remains a separate authority-safe lane rather than a
claim of complete no-marker-flash coverage.

A zero-width terminal matcher covers the real dogfood append after `locally.`.
Its affected closure is only the parser-authored final physical-line plain gap,
so the earlier Strong run remains rendered. It admits ASCII words and bounded
ASCII prose punctuation separated by single spaces only on Plain lines beginning with an ASCII letter after ordinary
paragraph padding, republishes blocked-space state after exactly one trailing
U+0020, suppresses two spaces or other terminal whitespace, and cannot carry a
second terminal space that would create a hard line break.

ABI 4.30 adds `LITERAL_SAFE_ENVELOPES_V2` for one deliberately narrow delimiter
case. A flat Strong fact with no escaping asterisk dependency publishes its
complete content as a one-shot envelope; one `*` inserted strictly inside it transforms that
projected run without exposing its delimiters or losing its style. The proof
cannot chain, and unsupported delimiter or structural edits continue to fail
closed until the parser publishes a bounded dependency plan.

ABI 4.31 adds `STRUCTURAL_PRESENTATION_PROOFS_V1` for the bounded structural
cases exercised by ordinary dogfood: terminal paragraph Return (including a
rapid ASCII successor) and paragraph-merge Backspace. The runtime compares the
parser-normalized inline partition before authorizing retention; the controller
only applies typed range transforms. Crossing delimiters and every unsupported
shape remain exact-source fail-closed.

Exact 4.31 also extends the existing ASCII-word envelope to parser-authored
maximal word leaves inside eligible projected facts, which covers ordinary
multiword Strong content without making Dart classify Markdown. The runtime
caps these optional records at 128 per row; hitting the cap drops further
continuity optimization, never the authoritative inline fact set. When the
complete page baseline fits, the ABI page encoder reserves all ordinary facts
and required projection segments before spending its remaining payload on
cells or envelopes, preventing proof-heavy early rows from forcing later rows
to exact source. Oversized baseline groups retain the ABI's existing complete-
group fail-closed behavior.

ABI 4.32 is the final frozen D0 minor. It adds one process-global live-state
inspection mode to the existing fixed inspection record so close/lifecycle
qualification can prove zero sessions, transactions, continuations, anchors,
and histories after the last session handle has been consumed. Before any
downstream Phase-2 receipt was frozen, the same draft minor was explicitly
reopened and refrozen with `PROJECTION_EDIT_CELLS_V3`. That capability keeps
kind 16 and adds a generic one-shot exact-scalar matcher parameterized by Rust.
The first parser emitter closes the D0 `[`-inside-Strong case on one
single-physical-line Plain row with a complete Strong dependency closure and
zero-width guarded trigger; Core compares only
the scalar and transforms the supplied ranges. No Dart Markdown rule or
construct-specific wire record was added.

The generic exact-scalar path also carries the frozen D0 prose punctuation set
at parser-guarded alphanumeric points in a fact-free prefix before one Strong
fact. The full prefix is the affected exact closure and the outside Strong fact
remains projected. Records are one-shot and bounded by the existing component
cap; this is emitter breadth within V3, not another ABI revision.

The same bounded emitter closes the frozen different-marker syntax rows: `*`,
backtick, `[` and `]` beside one Emphasis fact, and `_` or `~` beside one
Strong fact. The parser requires a fact-free ASCII prefix, an alphanumeric
guard pair, and no current occurrence of the inserted marker; brackets also
require exhaustive bracket classification. The complete prefix is exact and
the outside sibling stays projected. These are one-shot V3 cells, not a
general delimiter-graph claim.

V3 also lets the same parser component seam supply a complete fact-free
physical-line gap as the affected closure and a maximal ASCII prose run as the
trigger for the existing ASCII-literal matcher. Core admits one nonempty
ASCII-alphanumeric/U+0020 replacement only strictly inside that declared
trigger, which closes the Product Tour multiword-paste row without widening to
punctuation or line-boundary edits.

The edit-cell seam is still bounded, not completion of continuously rendered
editing. When no cell or envelope matches, the controller safely paints the
whole active row as exact source. That can reveal unrelated markers and remains
a product gap against RFC 027's no-marker-flash target. General parser-authored
dependency components and exact composition islands remain required.

The soundness obligation replaces a measured frame count with a structural
guarantee — for every admitted position in a published envelope, an edit of the
declared class must leave published facts unchanged, including every carried
successor and closure bundle. Focused tests cover the landed minimum and the
bounded 4.27 closure; a corpus-wide differential is still required before the
class vocabulary expands. The T2 receipt's "0 raw projected frames" could
not have caught this: its workload types alphanumerics strictly inside an
already-certified strong run, the single shape the old permission retained.
The pinned regression in `packages/flark/test/emphasis_continuity_test.dart`
is the product acceptance test for the trailing-space class; it does not prove
the pending edit classes or broader corpus coverage.

### Flutter streamed-open paint receipt (2026-08-18)

The A3 vertical now reaches painted pixels. `FlarkEditorController` gains
the streamed-open entry points (`openUtf8Stream`, `openStreaming`, and a
bounded `streamedOpenSupported` capability probe), and startup no longer
discards the head: instead of pumping to Ready before the first refresh,
it interleaves bounded pump slices with a bounded head-window
certification probe, publishes the first certified viewport through the
ordinary refresh path — so the editor paints and accepts input for the
certified region while admission continues — republishes only on genuine
certification upgrades, and converges to the existing ready flow once the
stream seals. A new `FlarkEditorStatus.streaming` reports the live
mid-load state; ranges past certification present as pending exact source
under the unchanged live-projection contract.

The input-window stance is now declared and proven rather than implied:
**streamed admission is epoch-neutral.** An append only adds bytes after
the admitted frontier, the engine proves no byte inside the previously
admitted prefix changes, and the input window always lies inside
certified text — so admission never advances the window or connection
epoch and never forces a resync. It only grows the length mirrors, which
a new owner-side admission hook surfaces for length-derived UI. A literal
edit during load remains the separate case and flows through the ordinary
revision-resync machinery unchanged. The regression types into the
certified head while admission continues and asserts an unchanged resync
count and connection epoch.

Development receipt, five runs (profile mode, 10,485,776-byte ordinary
fixture, 8–64 KiB chunks): the open call returns in 2.2–5.1 ms, the first
certified viewport publishes in 60.6–77.0 ms, and **the first painted
frame carrying certified rows lands at 74.4–88.3 ms** with 32 certified
rows and 264–288 KiB admitted, while the status is still `streaming`.
Every run sits inside the frozen 200 ms first-editable-viewport gate with
better than 2x margin, and the measurement is pessimistic: `flutter
drive` launched the universal profile binary's x86_64 slice under
Rosetta. This is a development receipt; the frozen five-run claim still
requires the controlled bench.

Three defects surfaced and were fixed on the way, each of which would
have reached a user:

- The startup probe keyed on per-range certification, which a semantic
  row query never populates (that breakdown belongs to live-projection
  queries), so the loop could never publish and the certified head was
  unreachable.
- A streamed open's parse task deliberately runs until the stream seals,
  so the presentation barrier's await on `continueParsing` turned a
  bounded settle into a wait-for-the-whole-load; any caller settling
  mid-load hung until transport ended. The barrier now keys on the
  published certified head for the current revision, and every exit from
  the opening loop releases its waiters.
- The capability probe opened a stream that never emitted, so an
  unsupported library waited forever instead of answering false.

Dogfooding is live: the tour gains a "Streamed · 10 MiB" preset that
admits its fixture in transport-sized chunks so a person can type in the
certified head while the tail loads, gated behind the capability probe.
One operational note — Flutter's build-hook runner does not propagate
`FLARK_V4_CARGO_FEATURES`, so the bundled library is always a
default-feature build; point the app at a feature library explicitly with
`--dart-define=FLARK_V4_LIBRARY_PATH=<dylib>`. Universal (x86_64 + arm64)
libraries are needed for profile-mode drives, and cross-compiling on this
Mac requires rustup's toolchain explicitly (`RUSTC=$(rustup which rustc)
rustup run stable cargo build …`) because Homebrew's rustc carries only
the host target.

### Pixel 6a engine multiplier receipt (2026-08-17)

The compact engine suite ran on the physical Pixel 6a (release,
aarch64-linux-android, charging, no thermal throttling: status 0
throughout, battery 25.5–26.9 °C, repetition spread under 1.5%; three
repetitions per metric; same build measured on the M1 Pro within the hour
for apples-to-apples ratios).

- Ordinary 10 MiB EOF compact indexing: 1,954.6–1,979.9 ms (nested cell
  1,654.2–1,660.4 ms; the progressive probe path agrees at
  1,881.3–1,914.0 ms). **The frozen <3 s Pixel gate passes with ~34%
  headroom.** 1 MiB steady-state completes in ~193 ms.
- Early certification stays sub-millisecond on device (0.614–0.675 ms at
  10 MiB) at the identical 2,240-byte admission point: size-independence
  survives the device.
- The device multiplier is not flat. Bulk indexing throughput is a tight
  1.79–1.94x across sizes and shapes. Small-slice query latency scales
  worse: ordinary midpoint cold jump 28.5–29.4 ms (2.46x, inside the
  100 ms gate), reference cold jump 25.8–33.5 ms (2.40x, inside), nested
  depth-four 132.9–139.1 ms (1.92x apples-to-apples) — exceeding the
  100 ms request gate by ~1.35x exactly as projected before the slice-query
  optimization. Device phase split matches the Mac shape: rows_located
  48.7 ms plus inline_prepare 69.8 ms is ~87% of engine_ready, with the
  query half scaling 2.4–4.0x and the inline half 1.2–1.6x.
- Planning rule frozen from this receipt: budget 1.9x for throughput and
  2.5x for slice-query latency; a single flat multiplier under-predicts
  cold-jump work by ~25%.
- The relocatability probe ran on-device with byte-identical results
  (2,532/2,532 and 1,013/1,013 aligned, zero payload diffs), extending the
  Experiment B storage receipt to ARM.

### Slice-query optimization receipt (2026-08-17)

The named optimization from the decomposition below landed the same day.
The measured hot loop was per-visit leaf re-authentication:
`sequence_node_header` re-decoded and re-summarized every packed event of
a leaf page (~0.36 us/event, ~900 events per descent on the nested slice)
on every node visit of every descent, with a fresh descent set per row and
five ancestor boundaries recomputed per fence call. Two hoisting-only
changes remove it: a walk-scoped `SequenceNodeCache` memoizing node
headers that already passed the exact decode-and-validate path against the
same immutable arena during that walk (full arena identity key including
generation; failed decodes never cached; the historical uncached paths and
their receipt-exact point-query tests untouched), and an additive batch
fence API (`locate_renderable_row_fences_for_kinds` /
`prepare_m11_recursive_green_slice_inline_leaf_rows`) minting every
qualifying row's fence in one bounded window walk with per-row admission
identical to the per-point query.

Merged-tree release receipt: nested depth-four cold jump engine_ready
**84.6 ms → 7.4 ms** (captured 1.77 ms, green built 3.3 ms, rows plus all
32 fences 5.8 ms total, batch preparation residue 1.7 us, slowest complete
row 0.51 ms); the ordinary path improved from 16.4 ms to 3.7 ms with no
regression. At the frozen 2.5x device slice-query multiplier this projects
the nested jump to ~19 ms on the Pixel — inside the 100 ms request gate
with 5x margin, closing the one pre-optimization device exceedance. The
nested receipt now additionally asserts per-row equality between per-point
and batch preparations; GFM 672/672, the 4,032-edit incremental ledger,
all engine/parser/runtime/abi suites, and the byte-identical
relocatability receipts all hold on the merged tree.

### Nested cold-jump decomposition (2026-08-17)

The 89 ms depth-four nested cold-jump receipt threatened the frozen
100 ms p99 / 200 ms maximum request gate once the historical Mac-to-Pixel
factor is applied, so the phase split was measured before any optimization.
The instrumented release run divides 84.6 ms as: durable decode 0.027 ms,
replay to the certified slice 2.25 ms, bounded Green slice build 3.2 ms,
`locate_renderable_rows` 26.1 ms for the single 32-row window, and
`prepare_m11_recursive_green_slice_inline_leaf` 50.7 ms across 32 rows
(about 1.6 ms per row through
`resolve_m11_recursive_green_slice_inline_leaf_row_fence`), with the actual
inline capture at 2.3 ms and the slowest complete row at 2.5 ms.

The compact restart architecture is therefore exonerated: decode, replay,
and slice construction total 5.5 ms at depth four. More than ninety percent
of the time is the slice query layer, whose per-row fence resolution and
row-window location cost roughly nine times their ordinary-depth
equivalents. Because every deep-viewport materialization pays these same
two functions — not only cold jumps — optimizing them (for example, one
shared ancestor-context walk per materialization instead of a fresh
fence resolution per row) improves the ordinary path as well. This is a
named engine optimization target ahead of Pixel qualification, not an
architecture correction; no gate threshold changes.

## 2026-08-11 continuously-rendered product correction

The first real dogfood pass falsified the prototype's focus behavior. The
surface currently sends every pan gesture into selection and restores exact
Markdown whenever a row becomes active. Those are implementation facts, not
accepted product behavior.

[RFC 027](../rfc/rfc_027_continuously_rendered_markdown.md) and the normative
[live projection v2 contract](contracts/live_projection_v2.md) now control the
surface:

- valid current Markdown remains rendered while focused and edited;
- incomplete, composing, pending, source-only, and faulted ranges ultimately
  require local exact-source islands. The current conservative whole-active-row
  fallback is safe but remains an explicit product gap;
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
plus ABI 4.9 row continuity for safe plain-text edits. `flark_core`
validates and binds either capability to the exact transaction, and Flutter
keeps only authorized content and unaffected cached presentation projected
until a covering certified viewport arrives. Syntax-like input and constructs
with non-local validity fail closed.

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

`scripts/verify_v4.sh` is the local gate of record: it builds `flark-abi`,
exports `FLARK_V4_LIBRARY_PATH`, and runs the Rust, Dart analyze/test, and
Flutter analyze/test v4 suites with no pipeline masking an exit code. Without
that variable the Dart and Flutter suites skip silently, which is how a red
suite could previously read as green. Active-package release and archive gates
now live under `scripts/verify_v4_*.sh`; the checked-in platform-smoke script
and compatibility entry points also target the v4 packages. Workflows invoke
distinct default-feature integration, `opening-session` feature, and full
release lanes. This wiring is structural evidence only until it passes on a
committed SHA; in this worktree the local script remains the only executed gate
of record.

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

The first tranche of that milestone is implemented: all thirty-one header
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

### H5 hardening result (2026-08-15)

The machine-verifiable H5 boundary is now closed. The normative semantic lane
is 672/672 exact for GFM 0.29-gfm, and all six deterministic histories for
every case are exact against clean selected-profile results: 4,032 edits,
4,027 local adoptions and five pinned clean fallbacks. The receipt drift caused
when the temporal test cutover changed those five path labels is reconciled in
the active ledger, and the semantic profile now points at active
`flark-live-v2` rather than the frozen v1 prototype.

Every non-device live-v2 family now names an executable product-path test.
The compact temporal circuits cover syntax hazards, structural Return plus an
immediate successor, blank-line Return/Backspace geometry, sustained wrapped
typing, cross-range replacement/history, pending certification, GFM objects,
source gaps/oversized fallback, view parity, and bounded hostile shapes. The
Mac native canary remains the narrow OS-routing lane for real keys, clipboard,
pointer selection, scrolling, every-painted-frame caret identity, and a real
Option-E/E dead-key composition route. CJK IME, autocorrect, dictation, and
touch behavior remain physical-device receipts rather than simulated claims.

The fixed 64 MiB admitted-payload contract supports ordinary and giant-line
documents through 10 MiB and the deliberately pathological four-byte-block
shape through 5 MiB (more than 1.3 million blocks). The release certification
receipt for that 5 MiB dense case reached `Ready` in 16.98 seconds with
64,844,455 live payload bytes and closed to zero state. A 10 MiB four-byte-block
fixture is outside that explicit density envelope and fails with typed
`PayloadBudgetExceeded`; the sweep records the exclusion instead of turning a
known capacity contract into an anonymous missing receipt.

The controlled 1 MiB ordinary typing run is claim-eligible at 120 Hz:
input-to-frame p99 8.500 ms, editor latency p99 1.564 ms, build p99 1.581 ms,
raster p99 0.907 ms, and no editor, display, or unexplained over-budget sample.
The current 1 MiB giant-line run kept editor latency at 3.395 ms p99, build at
2.144 ms p99, raster at 0.950 ms p99, and zero editor-attributed misses. Its
panel served the synthetic test at 20 Hz, so all 120 wall delays are explicitly
display-attributed and the run remains defect-resolution evidence rather than
a wall-clock claim. A test-only animation failed to change that cadence and was
discarded rather than retained as ineffective harness machinery.

The previously one-off bulk `INTERNAL_FAULT` did not recur through a controlled
14-cycle 32 KiB paste/undo surface workload or the focused Core/Flutter lanes,
so its temporary diagnostic printing is removed. The workload exposed a
separate honest performance residue: after a 32 KiB paste into a 1 MiB file,
large undo waits about 0.9 seconds for full recertification while retaining the
old coherent frame. That is not typing jank or a correctness fault, but it is a
post-dogfood optimization target for receipt-backed history presentation.

Android and iOS source/build checks may run on this Mac, but interaction and
performance claims still require physical devices. Windows is outside the
current program scope. The source-layout identity cutover has landed: active
code lives only in `packages/flark_core` and `packages/flark`, while superseded
v2/v3 sources are inert under `legacy/`. Checked-in CI, release, archive, and
platform entry points now target those packages; operational cutover remains
unclosed until those gates pass on a committed SHA.

### Android qualification development update (2026-08-15)

A physical Pixel 6a running Android 16 (API 36, arm64) now has one explicit
v4 lane: `scripts/v4_android.sh verify|profile|run <device>`. The command
uses `flark_core`'s package build hook to compile and bundle the Rust ABI in the
Flutter artifact. It does not accept a manually staged `jniLibs` copy or need
`FLARK_V4_LIBRARY_PATH`. A separate clean macOS consumer that depends only on
the two product packages also built and launched through the package-native
asset path. The same clean consumer builds for the iOS simulator and bundles a
universal arm64/x86_64 `flark_abi.framework`; this is packaging evidence only,
not physical iOS interaction or performance qualification.

The profile driver now preserves its JSON receipt on both PASS and FAIL and
records whether foreground validity came from immediate input-wall evidence
or engine-vsync cadence. That distinction exposed a real Flutter cost rather
than blaming Rust: the first eligible 1 MiB structural burst measured 28.659 ms
editor-attributed p99 with 60 over-budget samples, while callback-to-receipt
remained bounded. The surface was rebuilding new `TextPainter` paragraphs for
unchanged visible rows on every controller notification and never disposing
the prior paragraphs. It now reuses only deep-identical spans at the same
width/direction, disposes every unused or terminal painter, and has a focused
reuse regression. On the same named device and workload after that change,
120/120 structural edits completed with 8.153 ms editor-attributed p99, zero
editor-attributed misses, 1.321 ms platform-callback p99, and 10.610 ms
callback-to-authoritative-receipt p99 at a served 60 Hz cadence.

The final package-native physical passes also cover real Gboard text and `*`
input, keyboard reshow on an already attached connection, touch scrolling
without accidental selection, long-press word selection with the adaptive
Copy/Cut/Paste/Select All toolbar, background/resume, meaningful editable and
read-only semantics, and display-only task checkboxes. The deterministic
device smoke covers source open, live projection, `*`, Backspace, structural
Return plus its immediate successor, undo, and the dogfood shell. The retained
checks remained input-synchronized with zero resyncs.

The compact profile matrix is strong for foreground editing. Across 1 MiB
ordinary, 10 MiB ordinary, 10 MiB giant-line, and 5 MiB tiny-block shapes,
editor p99/max stayed below 16 ms with zero editor-attributed or unexplained
over-budget frames. The final 10 MiB ordinary run served 60 Hz, painted no raw
Markdown projection frames, and measured 9.174 ms p99 / 9.368 ms max. Removing
two complete source ownership duplicates cut 10 MiB controller open from
249.778 ms to 138.319 ms and the absolute process peak by 13.3 MiB.

This is still development and defect-resolution evidence, not an M7 pass.
Complete 10 MiB background certification takes 93.136 seconds. Its 95.4 MiB
peak-over-warm-baseline exceeds the provisional 60.0 MiB mobile allowance, and
61.5 MiB remains above baseline after close versus the 8 MiB limit. A 1 MiB
same-process reopen diagnostic plateaued rather than growing linearly across
four additional full parse/close cycles, but it also missed the retained-RSS
gate. The raw receipts and evidence boundaries are recorded under
`benchmark/v4/android_pixel6a_2026-08-15/`.

Selection handles and magnifier, TalkBack, real Gboard composition/autocorrect,
predictive text, dictation, the competitor-derived Android size envelope,
release-floor/current-device coverage, and long thermal sessions remain open.
iOS remains entirely unqualified on physical hardware.

### Cold-certification investigation (2026-08-16)

The 93.136-second 10 MiB Pixel result is a real architecture signal, not Dart
isolate scheduling or source scanning. On the exact ordinary fixture, a
temporary phase diagnostic attributed a 6.082-second Mac clean build as
3.385 seconds in persistent Green writing, 1.377 seconds in the block
controller, 0.692 seconds in active/pending-line work, and only 0.074 seconds
in the source scanner. The build performed 7.42 million event-authentication
operations for 2.47 million Green events.

Two exact production optimizations survived the investigation. Commitment
arithmetic over the Mersenne prime `2^61 - 1` now uses shift-and-fold reduction
instead of software `u128` remainder on ARM, with a property test against the
old arithmetic over boundary values and 100,000 deterministic pairs. The
runtime also passes each remaining bounded pump grant directly to the clean
builder instead of moving its large state once per transition. The 10 MiB Mac
receipt moved from 5.315 seconds to 4.457 seconds. The Pixel 1 MiB receipt moved
from 7.28 seconds to 3.864 seconds, but 10 MiB moved only from 93.136 seconds to
88.923 seconds. These changes are worthwhile; they do not make a full tree a
viable foreground readiness gate.

A physical bounded-source curve established the useful foreground scale on
the Pixel 6a: 16 KiB certified in 163.105 ms, 64 KiB in 283.762 ms, and
256 KiB in 874.563 ms. A disposable same-parser differential then falsified
the obvious dual-document shortcut. A 4 KiB prefix produced the same first
1 KiB rows as the full source for ordinary prose, but differed when a later
reference definition resolved an earlier link and when fenced code, a lazy
list, or an HTML block crossed the prefix cut. A truncated parse is therefore
never current-revision certification, even when it looks plausible.

The preliminary scale correction was:

- keep exact source available and editable immediately;
- let the primary parser expose a bounded first viewport progressively from
  its own event stream, without a second grammar or truncated-document parse;
- publish only closed rows whose local and global dependencies are proven
  current; keep unresolved references and spanning constructs source-faithful
  and neutral;
- continue or replace the monolithic full-tree build outside the foreground
  path; ultimately retain compact restart/reference state and materialize
  bounded Green fragments on demand rather than requiring a 10 MiB Green root
  before semantic queries work.

The architecture challenge below supersedes the closure-only publication and
per-region storage parts of this preliminary correction.

The bounded-recorder experiment succeeded and changed the selected storage
architecture. On the exact primary parser stream, an ordinary 10 MiB document
closed its first row after 1.343 ms and its first 32 rows after 7.787 ms in an
unoptimized diagnostic build; those 32 rows covered only 3,007 source bytes.
A 2 MiB document with a late reference definition reached 32 closed block rows
after 6.719 ms. A 2 MiB document-spanning fenced block correctly produced no
closed row until EOF. These are architecture timings, not claim-eligible
foreground receipts, but they prove that useful work is available independently
of total document size and that the exact parser exposes one useful publication
boundary. They do not prove that boundary is sufficient for every legal shape.

A disposable differential replay then sealed only closed top-level regions
from that same event stream. Row geometry, editability, nested container facts,
and source coordinates matched the eventual full Green result for ordinary
prose, a bounded list, and a document whose early reference use was resolved by
a late definition. The fragment's synthetic storage envelope necessarily had a
shorter Document range; every parser-authored fact below that envelope matched.
The late reference remains an inline dependency and must stay neutral until its
winner is known. The spanning-fence case proved the complementary rule: an open
top-level construct cannot be certified merely because it intersects the
viewport.

A second disposable release-mode probe ran the same 10 MiB ordinary source
through the complete scanner, block controller, writer geometry, and reference
journal while replacing persistent Green construction with the existing local
event journal. It still retained all 557,756 high-level events in a `Vec`, yet
finished the document body in 791.832 ms over 5,912,211 bounded transitions.
The comparable persistent-Green path is 4.457 seconds on this Mac. The roughly
5.6x separation is large enough that per-event durable authentication and
whole-document Green storage are the wrong background representation, not just
an implementation detail to micro-optimize. The probes were removed after the
receipts were recorded.

#### Architecture challenge and corrected direction: sparse pages plus certified viewport slices

The initial compact-region proposal survived only at the strategic level. Two
independent read-only reviews and four disposable release-mode diagnostics
found that its literal fragment and publication rules were not implementable
or correct enough to begin a production migration.

The frontier diagnostic used approximately 256 KiB of each adversarial shape.
Ordinary prose retained 65 checkpoints and reached a Document-only cut at byte
4,160. A spanning fence retained 33 checkpoints, a quote 23, and a list 17;
all had useful interior open-path cuts but no closed Document-only frontier.
A type-6 HTML block retained only the BOF checkpoint, while one giant physical
line retained only BOF and EOF. An interior restart is therefore continuation
authority, not publication authority, and some legal GFM shapes expose no
interior restart at all.

A prefix-versus-full differential found stable broad styling but non-final row
authority. Extending the source changed the first fence row from `0..36872` to
`0..36876` and its `closed` fact from false to true; the quote row changed from
`2..47104` to `2..47125`; the list row changed from `2..40968` to `2..40989`
and its `item_end` changed; and the paragraph row changed from `0..49152` to
`0..49172`. A 110,613-byte prefix had no winner for an early `[late]` use,
while appending the definition at EOF produced winner ordinal zero. Stable kind
or typography is not final range, geometry, edit, or reference authority.

The storage diagnostic was encouraging only for a *sparse* index. On this Mac
compiler layout a current checkpoint is 496 inline bytes and each writer open
frame is 160 bytes, making a shallow 4 KiB cadence plausible. It does not
justify one record per block: the 5 MiB tiny-block receipt contains roughly
1.3 million block units, so even 16 bytes per unit would cost about 20.8 MiB.
Current reference storage is a similar trap for the many-reference workload.
These layouts are diagnostic, not a stable ABI or a retained-heap receipt.

The reviews also exposed four contract corrections:

- Closed does not mean bounded. A list, quote, paragraph, table, fence, or HTML
  block can close only after megabytes, so a cache miss may exceed its work cap
  even after the correct natural closure rule is known.
- Invalidations have backward edges. A later blank can change list tightness, a
  following line can promote Setext or a GFM table, a terminator can change a
  fence or HTML extent, and a definition can change earlier reference leaves.
- Today's restart checkpoint cannot outlive the monolithic Green root. Resume
  validates its Green identity, event cut, structural boundary, and base root.
  Root-independent materialization is a new authority and codec, not plumbing.
- Green equality is not the product oracle. GFM tables and inline/reference
  facts are produced after block Green queries. Differential tests must compare
  complete public viewport rows, inline facts, table cells, semantic targets,
  source/display coordinates, and edit capabilities.

The corrected architecture is deliberately austere:

- Source remains the sole mutation and coordinate authority. There is still one
  primary GFM parser and no truncated-document or second-grammar shortcut.
- Background work writes coarse, fixed-record parse pages, selected *before*
  capturing restart state. A page carries source/UTF-16 cuts, prefix row-count
  summaries, dependency epochs, and a durable parser/writer restart independent
  of any Green tree. The records are versioned and checksummed; delta coding is
  permitted only if retained-byte evidence later requires it. It does not
  allocate one object per block or retain the transient event stream.
- Reference state is source-backed and compact: normalized-label digests,
  winner links, source ranges, and immutable committed-prefix snapshots. A
  fixed-revision first winner becomes final only after a BOF-to-frontier commit;
  absence remains unknown until EOF.
- The renderer consumes one bounded viewport-slice contract. A slice always
  carries exact current source, revision-qualified byte/UTF-16 coverage, and
  source-anchor mapping. It may additionally carry complete
  `DocumentViewportRow` payloads only for certified ranges. `total_rows` is
  unknown until EOF; a known prefix count is not an exact total.
- There is no weaker `provisional` row. A range without complete current
  semantic authority uses the existing `pending_exact` or `source_gap_exact`
  state from `flark-live-v2`. It advertises no row ordinal, marker hiding,
  geometry, source/display projection, structural edit, inline, or target fact.
  The initial scale probe adds no new display-hint category. Any later hint must
  version an explicit allowed-fact set and pass its own differential oracle;
  it cannot masquerade as a partial `DocumentViewportRow`.
- Literal insertion, deletion, paste, caret, and selection remain source-anchor
  operations in exact ranges. Pending ranges are not structural-edit authority.
  A transaction-bound continuity receipt may preserve exactly the presentation
  already authorized by `flark-live-v2`; it is not a cold-open shortcut.
- Some legal inputs have no bounded semantic answer: a giant physical line, an
  open or megabyte-scale container, and Setext/table/reference ambiguity can
  force exact pending presentation until more work or EOF. The no-raw-source
  promise is therefore a falsifiable product gate for the ordinary 10 MiB
  fixture, not a universal claim. Pending exact source remains the correctness
  fallback required by M4 and the live-projection contract.
- Recursive Green remains the exact generic representation for naturally
  bounded, certified hot regions. Fragments have absolute byte/UTF-16/logical
  and row bases, immutable dependency epochs, authenticated start/end context,
  and a hard cache byte/page cap. A fragment that cannot reach certifying
  closure within budget is not silently called bounded.
- Edits invalidate by dependency footprint, including backward label and
  enclosing-structure edges. Immutable page storage may be shared only behind
  exact lineage plus parser, writer, reference, coordinate, and prefix-summary
  convergence. Equal local bytes do not make a page authoritative at a new
  revision. Revision-local indirection must avoid cloning or physically
  rebasing every later page without reusing stale coordinates or summaries.

This still rejects a persistent flat row model, a globally retained event
journal, and further optimization of the monolithic Green as the scale
solution. It retains a compact locator/dependency index plus hot generic Green,
but corrects “region” to mean sparse page summaries and “fragment” to mean only
work-bounded certified materialization.

The probes use a small frozen development contract. Rust parser/index work uses
an optimized release build; physical rendering uses a profile build. A cold run
starts a fresh process with no fragment cache. Before *each* cold jump, the
completed compact index remains but the 8 MiB hot-fragment cache is cleared,
destroyed, and observed at zero allocated capacity. Receipts name the exact Mac
and Pixel 6a build, OS, power, thermal, display, fixture and contract hashes.
Each Mac cold result repeats five times and each Pixel result three times; every
repetition must pass. Timers have explicit endpoints:

- every open timer begins at the named host-to-ABI open-request event, before
  any source copy, rope construction, parser construction, or indexing;
- first-slice engine time ends at publication of 32 consecutive certified rows
  from BOF for the ordinary fixture;
- first rendered viewport ends at the raster-complete frame with certified
  rows covering every source-owned glyph and layout row from BOF through the
  first row below the frozen viewport plus one overscan row;
- EOF index time ends only when the page/reference index for that revision is
  atomically queryable; and
- jump time runs from the qualified source-coordinate request through slice
  response, fragment/inline work, ABI, layout, paint and raster, with each layer
  also reported separately.

The ordinary 10 MiB fixture, 5 MiB tiny-block fixture, and 5 MiB many-reference
fixture must each retain at most 12 MiB for compact pages plus reference state
and at most 8 MiB for hot fragments. The accounting measure is allocated
capacity after EOF publication and one explicit scratch/reclamation drain, not
serialized logical length. It includes checkpoints and open paths, maps,
metadata, allocator-owned page/reference buffers, cached inline facts, and all
fragment-arena overhead; only exact source storage is excluded from the 12/8
MiB component caps, while the global physical RSS gate includes everything.
The two 5 MiB stress fixtures are the minimum supported density envelope: they
must publish a complete index, answer their certification-required cold jumps,
and, for references, resolve and retarget the declared winners. Their EOF index
gate is below 3 seconds on Mac and below 10 seconds on Pixel. The existing
global physical memory and close-state gates still apply. Thresholds,
fixture-coordinate dispositions, cache state and repetitions are frozen before
the first measured run; a result cannot become passing through post-hoc
eligibility.

No production cutover begins until four falsifiable probes pass:

1. **True compact-stream sink.** Consume primary-parser events directly into
   4 KiB-class fixed-record pages with compact references and no document event `Vec` or Green
   root. Run ordinary, tiny-block, nested, many-reference, giant paragraph and
   line, open fence, and every HTML termination family. Record first rendered
   slice, EOF time, attempted versus retained checkpoints, allocations, index
   bytes by component, peak RSS, and release state on Mac and the Pixel. Restart
   cloning/allocation occurs only at selected page cuts: retained restarts are
   at most `ceil(source_bytes / 4096) + 2`, and rejected physical-line cuts do
   not clone the open path. On ordinary 10 MiB, first certified slice is at most
   50 ms on Mac and 100 ms on Pixel, and EOF index publication is below 1 second
   on Mac and below 3 seconds on Pixel. Every adversarial cell must complete or
   return its predeclared typed cap outcome without an unbounded allocation.
2. **Green-independent restart and cache miss.** Release the monolithic root,
   decode durable restarts, and issue 100 fixture-manifested cold jumps from
   every declared family per repetition; the total is `100 * family_count`, not
   a fixed denominator. Coordinates are generated before implementation from a
   frozen seed: 70 are uniform source-coordinate samples rounded to legal
   grapheme boundaries and 30 are stratified across page cuts, construct
   starts/ends, reference uses/definitions, and closure-cap boundaries. Ordinary,
   tiny-block, nested-container, many-reference, bounded
   fence/paragraph/list/quote, and each bounded HTML termination family are
   certification-required: all 100 coordinates in each must certify.
   Document-spanning giant-line, paragraph/table, list, quote, open-fence, and
   over-cap HTML families are separately fallback-required. Classification
   comes before implementation from a clean full-root oracle plus a fixed
   closure/replay-cap calculation, not from the compact implementation's
   outcome. Every coordinate predeclares its exact expected disposition.
   Certified results compare complete public viewport rows, inline facts, table
   cells, targets, source/display coordinates and edit capabilities with a clean
   full-root oracle. Exact fallbacks compare complete current-source coverage
   and prove that no semantic/edit authority leaked.
   Every request has p99 below 100 ms and maximum below 200 ms; mismatches,
   timeouts and undeclared outcomes are zero. Separately report lookup, replay
   bytes, closure distance, fragment build, inline work, ABI, layout, paint and
   raster; never omit over-cap HTML or giant-line outcomes.
3. **Publication and invalidation oracle.** Attempt publication after every
   parser command/rendezvous boundary across Setext, tables, tight/loose lists,
   lazy continuation, fences, all HTML types, references/duplicates, CRLF, and
   Unicode. Certified output must equal the complete public full-root oracle;
   pending output must cover exact source, expose none of the forbidden row
   facts above, and reconcile atomically to one revision. After later edits,
   prove no stale earlier row or inline leaf is served. Green-only equality
   cannot satisfy this gate.
4. **Concurrent physical receipt.** On Pixel, run separate typing and scrolling
   passes while indexing an ordinary 10 MiB document. Typing uses 20 warmups and
   120 measured single-character insertions at the existing 2 ms requested
   cadence, preserving the exact delivered callback times. Scrolling uses 120
   predeclared uncached jumps, at most one per presented frame. Background grants
   request a 1 ms deadline as well as a transition cap and yield immediately to
   foreground work. Observed active grant wall time is at most 1.25 ms p99 and
   strictly below 4 ms maximum; requested and observed values are both retained.
   Accepted source/caret/selection, backlog, p99 and hard
   frame/span results must pass the existing Tier B contract; editor-attributed
   misses are zero. Against the same warmed workload with indexing paused,
   foreground p99 regresses by at most 10% and gains no hard miss. The index
   publishes within 3 seconds after input stops. Supersession detaches old
   authority synchronously; at most one retired generation may remain allocated,
   and its allocated capacity must be destroyed before a second replacement is
   admitted or within 50 ms, whichever occurs first. Allocator RSS need not fall
   until close, but it remains charged to peak RSS. The physical memory envelope
   passes and all live state is zero after close. Repeat memory/cancellation
   coverage for tiny-block and many-reference shapes even if their declared
   density envelope prevents the 3-second ordinary-document claim.

Gate-one structural implementation receipt (2026-08-16, release mode on the
development Mac): the primary parser now streams into a no-Green diagnostic
sink, retains only parser-proven reference Paragraph windows, writes selected
donor restarts through the existing versioned/checksummed durable codec into
owned 4 KiB pages, and keeps source-backed compact reference records. It
completed the frozen 16-shape matrix, including every CommonMark HTML block
termination family. Ordinary 10 MiB completed in 0.697 s with 0.612 MiB of
checkpoint pages; tiny-block 5 MiB completed in 1.941 s; many-reference 5 MiB
completed in 1.070 s with 0.308 MiB of checkpoint pages plus 9.713 MiB of
reference state. The maximum provisional reference window was 352 bytes.
Interior and EOF samples decode and canonically re-encode without a Green root.
This evidence rejects mandatory delta coding: fixed records already clear the
component budget with simpler corruption and random-access behavior.

This is not yet a complete Gate-one pass. The reported first-32-row time is a
parser structural counter, not a complete public viewport payload or rendered
frame; writer/output restart reconstruction, public row/inline/table/target
differential equality, peak RSS, explicit scratch drain, and physical frame
receipts remain open. The durable roundtrip exposed and fixed a latent codec
defect where GFM was encoded but only CommonMark decoded; both durable codecs
now have direct GFM regression coverage.

The first-slice mechanism has also passed its initial correctness experiment.
The compact primary stream retained exactly one candidate bounded by 32 rows,
64 KiB of source, and 8,192 events; Paragraph events remained private until
reference-prefix rewriting was final. Those same events built a distinct Green
slice tied to the full current source revision but covering only the certified
prefix, so it cannot be mistaken for or adopted as a whole-document root. For
an ordinary 96-block document, all 32 slice rows matched the eventual full root
for ordinals, kinds, byte/UTF-16 ranges, edit capabilities, editable ranges and
segments, nested path facts, inline facts, and cooked direct-link values. A
document-spanning fence crossed the fixed cap and returned no slice rather than
growing an unbounded buffer.

This experiment tightened the publication rule further. Complete public
viewport rows for an ordinary closed prefix matched the same rows after a later
independent suffix, while a later reference definition, GFM table delimiter,
or Setext underline each changed the earlier public row. Inline projection is
now explicitly fail-closed: unsupported inline HTML and a reference-shaped
slice without final winner authority return `None`/neutral exact source, not an
authoritative empty fact list. The runtime now shares one row mapper and one
inline-capture path between full-reference and no-reference authority, which
removes a future second semantic implementation.

The first end-to-end engine timing is also inside the provisional Gate-one Mac
budget. Five fresh release test processes opened the ordinary 10 MiB fixture
and produced the same 32-row, 2,751-byte slice. The primary-stream boundary was
available in 2.275-2.680 ms, and the bounded Green rows plus authoritative
inline facts and cooked direct-link values were ready in 13.557-13.733 ms,
timed from before source admission and before EOF indexing completed. This is
not the `<50 ms` rendered-slice receipt: it excludes the runtime/ABI payload,
Flutter layout, paint, and raster, and therefore remains engine-only evidence.

The first Gate-two cache-miss experiment now passes for ordinary blocks. A
compact session with `green == None` selected a Document-only durable
checkpoint near the middle of the same 10 MiB source, decoded the primary
parser restart, restored the writer's distinct accepted/parser cuts (including
its one deferred blank-line metric), and resumed the normal scanner,
controller, writer, and reference-rendezvous driver. It did not parse a cropped
source or invoke a second grammar. The resulting fragment was attached to the
full current source with absolute byte, UTF-16, and row bases. Its 32 public
rows, frame identities, editable geometry, inline facts, and cooked direct-link
values matched the eventual full-root oracle. A Unicode-prefix differential
also proved that nonzero byte and UTF-16 bases cannot be conflated; that test
exposed and fixed one convenience query that had bypassed slice-coordinate
translation.

One fresh release process measured the 10 MiB midpoint restart at byte
5,242,559. Durable decode plus primary-stream capture took 0.229 ms; bounded
Green rows and authoritative inline/link facts were engine-ready in 11.307 ms.
The slice covered 2,752 bytes and began at row 60,960. The entire differential
test, including compact EOF indexing and eventual full-root comparison,
finished in 2.63 seconds. This is strong evidence for cold random access, but
only for ordinary Document-level blocks. Full open-row/fence/HTML restart
serialization, reference-winner snapshots, tables/targets, the frozen
100-coordinate family matrix, ABI/layout/paint, and physical-device receipts
remain open; Gate two is not yet claimed complete.

The next bounded-container experiment removes the first item from that open
list. Compact checkpoints now join the donor restart with a separate
versioned/checksummed writer record: a 32-byte header plus 32 bytes per open
Document/list/item/quote frame. The record retains frame identity, authenticated
absolute block start, and the two close-time container folds; the parser record
remains the sole owner of semantic kinds and list/item properties. Decode
requires both paths to agree. A slice seeds the open event envelope from that
joined authority, replays the primary parser until the bounded containers
close, and substitutes authenticated ancestor starts into public path geometry.
Open rows, fences, and HTML are intentionally not encoded by this record and
therefore retain a typed cache-miss fallback.

One release-mode nested fixture restarted inside an open block quote, list, and
item at depth four. It replayed 16,879 source bytes to the certified container
close, captured in 3.349 ms, and materialized the frozen 32-row viewport plus
authoritative inline/link facts in 89.188 ms. Complete public rows—including
container properties, absolute paths, frame identities, editable geometry, and
inline values—matched the eventual full-root oracle. This clears the initial
bounded list/quote mechanism under the provisional 100 ms request target, but
is one development receipt rather than the required repeated family matrix.

The compact-reference dependency has now passed its first cold-slice
differential without introducing a second reference grammar. The EOF index
retains normalized labels, exact definition/value source ranges, sorted
first-winner ordinals, and the source revision. A cold inline lookup binary
searches that immutable authority and runs only the selected winner's bounded
destination/title ranges through the same parser-owned cleaner used during
full publication. A fixture whose viewport preceded a late definition matched
the eventual persistent reference root for complete inline facts and target
geometry: the first of two duplicate definitions won, the forward reference
resolved, case/whitespace and Unicode labels resolved, cooked Unicode
destination/title values matched, and an undefined label remained literal.
Two release-mode runs of the final escaped/entity fixture made the 32-row cold
result engine-ready in 15.760-20.428 ms.

One rejected variant usefully hardened the storage rule. Retaining every
cooked destination/title in the compact index increased the 5 MiB
many-reference fixture from 9.713 MiB to 16.005 MiB and failed the 12 MiB
component cap. Source-backed on-demand cooking restored 9.713 MiB of reference
state plus 0.544 MiB of checkpoint pages, retained exact behavior, and
completed EOF indexing in 1.073 seconds. Compact reference values therefore
remain source-backed; a later value cache, if any, belongs to the bounded hot
fragment budget rather than the revision-wide index.

The product-mapping seam has now been narrowed without adding a cold-only
renderer. `flark-runtime` maps a bounded row set through one production row
mapper while the caller supplies the exact inline-authority capture. A focused
slice-style differential (no reference authority) matched the complete public
viewport for an ATX heading, strong text, a character reference, a direct
link, an email autolink, a typed GFM table, and a task item, including every
byte/UTF-16 range, edit/continuity capability, table cell/replacement fact and
activated cooked direct-link target. The existing fixed ABI vertical slice now
also asserts the encoded table presentation/cell payload and semantic-target
record plus cooked destination/title bytes. An intentionally malformed direct
link failed closed to neutral inline output during development of the fixture,
which is the required behavior rather than an authoritative empty projection.

The crate-boundary gap is now closed for the runtime payload. A dormant
`m11-compact-probe` feature exposes one opaque correctness bridge without
making the sparse path selectable by production sessions. Through it, a real
compact first-slice root and the final source-backed compact reference resolver
fed the production runtime mapper. All 32 mixed rows matched the complete-root
`DocumentViewportRow` oracle exactly, including heading, paragraph, table,
task, quote, direct link, character-reference, autolink and missing-reference
behavior. A forward `[late]` use resolved to `/resolved` with `late title`, and
the definition was asserted to begin after the compact slice ended; its
activated `DocumentSemanticTarget` matched the full-session oracle. Both the
slice root and its `DocumentRuntime` were explicitly fuel-drained to zero.

This correctness probe intentionally waits for EOF reference authority before
mapping the mixed slice, so it does not replace the earlier pre-EOF timing
receipt for the ordinary no-reference fixture. Gate two also remains open for
the actual compact-session ABI routing and byte-for-byte encoded payload, the
frozen multi-coordinate/family matrix, and layout/paint. The runtime contract
must represent an unknown total row count until EOF; the ready-only
`DocumentViewport.total_rows` field cannot be reused as a progressive total.

The direction is accepted for prototyping, not yet for production migration.
The remaining Gate-one semantic proof is a frozen mixed-construct slice matrix
through the complete runtime/ABI payload, including tables and activated
targets; the current result proves ordinary structural and inline equivalence,
the dependency fallback, and the bounded failure mode but not a rendered
frame.
Its first product gate is five Mac and three Pixel cold runs with a physical
rendered, literal-editable first viewport below 200 ms for the ordinary 10 MiB
fixture. The entire frozen visible-plus-overscan range must be
`certified_projected` or `certified_literal`; `pending_exact`,
`source_gap_exact`, uncovered space, and mixed-revision coverage all fail. This
prevents one tiny certified row from satisfying the gate. “Editable” here proves
exact source, caret, selection, literal insert/delete/paste and receipt-backed
mapping; it does not grant pending content semantic Return/Backspace behavior.
Cold-jump and background gates are the explicit predicates above rather than
an undefined “eligible” subset. Only passing raw receipts can promote this
section from prototype direction to implementation architecture.

## 1. Destination and current state

The destination is fixed:

```text
flark (Flutter)
  -> flark_core (Dart, no Flutter)
       -> flark-abi
            -> flark-runtime
                 -> flark-parser + flark-engine
```

The direct package source-layout cutover from the broader legacy bridge has
landed; verification and automation closeout remain incomplete:

| Current | Destination | Treatment |
| --- | --- | --- |
| Rust `flark-engine` | Rust `flark-engine` | Keep |
| Rust `flark-parser` | Rust `flark-parser` | Keep and complete |
| Rust `flark_comrak_bridge` | `flark-runtime` + `flark-abi` | Replace after parity |
| Dart `flark` | Dart `flark_core` | Rename mechanically after baseline |
| Flutter `flark_flutter` | Flutter `flark` | Rename mechanically after Dart |

The intended sequence was to close M0 and then change the package identities
mechanically because all three candidate pub.dev names returned not-found on
2026-08-08 and the project had no hosted compatibility promise to preserve. The
user instead selected a direct cutover, and source moves, runtime work, generated
artifacts, and qualification landed together in `a210a12`. That decision is
recorded rather than rewritten as the original staged proof: committed-SHA CI,
archive consumers, capability/rollback classification, and publication gates
remain separate closeout evidence.

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
9. **Declare total work class before building.** A tranche that adds or
   reroutes any foreground or readiness operation states, in its plan entry
   before implementation, the operation's total work as a function of
   document size and shape, and names the frozen gate that falsifies it. A
   bounded pump is not a bounded total, and "background" is a scheduling
   claim, not a total-work claim.
10. **Arithmetic before receipts.** Any operation whose declared class is
    linear in a capacity dimension shows `units x unit cost x slowest-device
    factor` at the envelope on paper before its first implementation commit.
    A paper fail rejects the design; no receipt is required to kill it.
11. **Cheapest falsification first.** Within an experiment, the property
    most likely to invalidate the design is probed with a disposable
    diagnostic before dependent machinery is built.
12. **Every claim receipt carries its detector tier.** Claim-eligible
    performance receipts include the 4x size tier and the declared hostile
    shapes where applicable, so hidden linear foreground work cannot pass by
    fixture selection.
13. **New feature classes declare their work class at proposal time.** A
    product feature touching document content — find, export, accessibility
    summaries, text services — enters the RFC with its foreground work class
    and background contract stated before any UI work.

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

**Status:** incomplete. The direct source-layout cutover proceeded before this
milestone's baseline and evidence exits closed; the unchecked items below remain
real reconciliation work rather than retroactive checkmarks.

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

**Status:** package directories and logical identities landed; verification
closeout is pending. “Landed” does not mean the original mechanical-only
sequence or committed-SHA exit gates were satisfied.

Purpose: make subsequent implementation land in the product structure the user
will actually consume. This milestone changes names and ownership declarations,
not runtime behavior.

#### M1A — Headless Dart package

- [x] Establish `packages/flark_core` as the only active headless Dart package.
- [x] Expose the supported API through
  `package:flark_core/flark_core.dart`.
- [x] Move the Rust workspace and build hook under the package that owns them.
- [x] Reconcile checked-in active workflows, release/archive/platform scripts,
  and repository README/CLAUDE paths with the new physical paths.
- [ ] Reconcile remaining active fixture and metadata pointers; historical
  evidence paths must resolve to their archived `legacy/` locations.
- [ ] Record successful committed-SHA CI and external archive-consumer receipts.
- [x] Assert that `flark_core` has no Flutter SDK dependency or Flutter import.

M1A exits when analysis/tests and a clean external package-native consumer pass
using only the `flark_core` identity on the committed candidate SHA. Local
package analysis and the static extracted-archive consumer are green; runtime
archive-consumer and committed-SHA CI receipts remain closeout work, so M1A is
not yet closed.

#### M1B — Flutter product package

- [x] Establish `packages/flark` as the only active Flutter product package.
- [x] Expose the supported API through `package:flark/flark.dart` with an
  explicit `flark_core` dependency.
- [x] Reconcile checked-in active Flutter workflows, example entry points, and
  release/platform scripts with `packages/flark`.
- [ ] Reconcile remaining active Flutter fixture and metadata pointers.
- [ ] Record successful committed-SHA Flutter CI/build and external
  archive-consumer receipts.
- [x] Assert that production Flutter code reaches engine APIs through
  `flark_core`.

M1B exits when Flutter analyze/test/build and a clean macOS product consumer
build/launch with only a direct `flark` dependency pass on the committed
candidate SHA. Local static extracted-archive consumer evidence is useful, but
runtime archive-consumer and committed-SHA Flutter CI/build gates remain open,
so M1B is not yet closed.

Constraints:

- The user selected a direct cutover rather than staged compatibility aliases.
- Superseded sources may remain under `legacy/`, but active package resolution,
  build hooks, imports, tests, and scripts must not depend on them.
- `flark_core` owns Rust/native delivery and cannot import Flutter; `flark`
  owns the Flutter surface and depends on the headless package.
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
  window resynchronization where macOS can exercise them. The sound landed
  minimum makes the active row exact during composition; a narrower island
  requires parser-authored result-revision/dependency authority.
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
