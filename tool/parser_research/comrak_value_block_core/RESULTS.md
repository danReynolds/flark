# Comrak-correspondent value block core: decisive gate results

Status: **same-parser semantic, ordinary live-continuation, source-backed raw
literal ownership, output-side list-repair aggregate, and parser-internal
pathological-transition mechanism GO; persistent pages/edit convergence and
end-to-end event-page scheduling remain open**, 2026-07-17.

This disposable crate answers a narrower question than “is RFC 023 proven?”:
can the complete selected Comrak 0.54 block ordering be translated to
Flark-owned value state without calling Comrak's stock block parser? The answer
is yes. The same grammar now also survives scratch destruction and serialized
restart behind a write-only sink. It does not yet prove all production
resource bounds, edit convergence, or persistent output pages.

## Exactness receipts

- The machine-readable provenance map pins all 55 selected upstream functions
  by path, name, body SHA-256, and local correspondent. All 55 local statuses
  are implemented and `scripts/generate_provenance.py --check` passes.
- Gate A normalized HTML is 189/189 exact: 11 Tabs, 27 Setext headings, 44
  HTML blocks, 25 Block quotes, 48 List items, 26 Lists, and 8 GFM Tables.
- A separate pre-inline donor projection compares reachable preorder block
  kind/metadata, parent index, source range, logical leaf, and line offsets.
  It is also 189/189 exact. Candidate origin runs are checked for contiguous
  logical coverage, valid coverage-leaf-relative ranges, and exact identity
  materialization.
- The direct projection caught three defects that normalized HTML hid:
  raw-HTML end positions, zero-end-column repair for list descendants, and
  table first-cell line offsets/counter updates. They were fixed before this
  result was recorded.
- The full candidate scorecard is CommonMark 0.31.2 652/652 exact. The pinned
  GFM 0.29 corpus is 661/670 fixture-exact; the other nine exactly match the
  selected modern Comrak 0.54 profile rather than the older GFM authority (five
  HTML-block cases and four core-autolink cases). There are zero candidate
  block, output/inline, or parser-error divergences after fixing adjacent-text
  coalescing in the bounded inline service.
- A focused interaction combines CRLF, lone CR, duplicate definitions, setext
  promotion, and table activation. It preserves physical line endings and
  leaf-relative origins; both reference occurrences remain ordered, while the
  renderer derives the first winner separately.
- A second full-corpus projection runs all 1,322 CommonMark/GFM fixtures and
  compares reachable kind/metadata, parentage, source ranges, logical leaves,
  and exact coverage-relative origins directly with the donor. All 1,322 pass.
- Six focused GFM task-list contexts now match Comrak, including loose lists,
  nested tasks, and the rule that only the first paragraph child of an item
  owns the task marker.
- Multiline 1 MiB and 10 MiB fenced-code and HTML cases match the selected
  Comrak oracle in both direct and live materialization. Raw block kinds retain
  only range descriptors; finalization, live append events, and cancellation
  own/copy zero aggregate literal bytes. The exact fenced-info transform is
  deferred to rendering and preflight-capped at 8 KiB before allocation.

The candidate `src/` path has no call to Comrak's `parse_document`. Inline
leaves call only the bounded `parse_inline_fragment`; HTML/table/reference and
small block scanners use the narrow facade. The donor block projection is
oracle-only test support.

## Surface and ownership audit

Current physical Rust lines:

| Surface | Lines | Disposition |
|---|---:|---|
| Function-correspondent grammar plus cooperative control gate (`parser.rs` + `table.rs`) | 2,177 | Candidate grammar seam with one shared line/finish authority |
| Value/source scaffolding (`tree.rs` + `source.rs`) | 1,161 | Source-backed raw literal mechanism; must be adapted to persistent pages/sink |
| Same-parser checkpoint/write-only-sink/position-overlay gate (`checkpoint.rs`) | 2,427 | Architecture proof; vector test materializer is not production |
| Normalized renderer (`render.rs`) | 998 | Test-only streaming projection |
| Cooperative exactness/resource tests (`fuelled_transition.rs`) | 393 | Proof-only |
| Selected donor seam | 1,816 | Comparison baseline |

The cooperative prototype originally duplicated the control driver around the
same grammar handlers to differential-test atomic and fuelled execution. That
maintenance hazard has been removed: one parser-owned
`LineTransition`/`FinishTransition` machine now owns phase ordering, the
ordinary API drives it without a budget, and the fuelled API only schedules
it. The former atomic check/open/text/finish loops were deleted after the full
corpus stayed exact. [CONTROL_AUTHORITY_VERDICT.md](CONTROL_AUTHORITY_VERDICT.md)
records the cut and its remaining output-production/retirement holds.

Allocation/ownership is still intentionally non-production outside the raw
literal gate:

- the test source and materializer each own one `String` per physical line;
- the one-shot test driver transiently clones all of those line strings into
  an iteration vector, while the live borrowed-line adapter copies each new
  line once into the materializer's source store; a real Crop
  `PhysicalLineView` must lease/move page slices instead;
- paragraph, heading, table-cell, and reference-classification paths still own
  aggregate logical `String`s in addition to source leaves;
- raw code/HTML leaves no longer own aggregate logical/literal strings, but the
  prototype still retains roughly one `OriginRun`, one line-offset entry, and
  one source `String` per physical line; adjacent identity coverage spans have
  not yet been coalesced into source/output pages;
- `LeafContent` represents owned versus source-backed storage as an asserted
  `String`/`Option<SourceBackedContent>` invariant for the disposable gate; the
  shipping model should make those mutually exclusive states an enum so an
  invalid mixed owner cannot be constructed;
- the mutable tree owns a `Vec` node plus a `Vec` of child IDs per node;
- paragraph/reference finalization and table promotion still clone
  kinds/content in several paths; and
- the test renderer clones child lists and constructs an inline fact forest.

Consequently the receipts prove bounded parser continuation copying, not the
whole editor's memory, jank, or persistence budget.

## Continuation/output gate ledger

The rejected first checkpoint spike reimplemented a subset of paragraph,
quote, setext, table, and reference grammar. It has been deleted. The accepted
gate wraps the exact 55-function-correspondent `ValueBlockParser`; clean and
resumed parsing invoke the same transition functions.

The implemented continuation split is:

```text
BlockCheckpoint = parser/profile identity
                + canonical open semantic frames
                + list looseness folds
                + pending logical leaf/origins

PauseToken       = BlockCheckpoint
                + OpenOutputBindings   (excluded from checkpoint equality)
                + RevisionCursor       (excluded from checkpoint equality)
```

| Gate | Receipt | Result |
|---|---|---|
| Same grammar | No second checkpoint grammar remains; resume reconstructs fresh scratch for `ValueBlockParser` | **pass** |
| Serialized restart | Pause after every physical line, JSON round-trip canonical state, discard scratch, resume | **pass, 1,322/1,322** |
| Exact output | Source leaves, references, kind/metadata, parentage, source ranges, logical leaves, line offsets, origins, and HTML equal one-shot | **pass, 1,322/1,322** |
| Canonical-state purity | Serialized JSON asserts absence of `NodeId`, `BlockTree`, `Position`, absolute/source start/end, line number, and revision root | **pass** |
| Runtime separation | Output handles and revision-local `Position`s exist only in non-serializable `OpenOutputBindings`/cursor receipts | **pass** |
| Historical grammar folds | Each finalized direct child is committed once to constant-size `ChildSequenceFold`; 1k/5k/20k deep-quote finish receipts rule out recursive close-summary recomputation. Item age uses ephemeral `opened_this_line`, not source coordinates | **pass** |
| Physical-line transition equality | A variant-local key retains only future grammar inputs: list marker/delimiter, item effective indent plus child presence, fence delimiter facts, HTML type, paragraph setext-visible-content and GFM last-line eligibility/column count, table width plus saturated autocomplete guard, and global profile/start/frame path. All 1,322 fixtures certify bounded paragraph decisions; giant raw histories, multi-line GFM prefaces, item/table mutations, refillable oversized-table certification, and conservative leading-reference `Unknown` have executable witnesses. Grammar plus a separately selected output accumulator reconstructs the new list prefix and rejects stale/incompatible roots | **grammar partition pass; generated reference/oversized line-kernel and persistent-page composition open** |
| Write-only output | Parser receives an `emit`-only sink trait and has no output query method | **pass** |
| Live scratch bound | 2,000 lines with one persisted JSON pause: at most 4 retained frames, 7 transient nodes, and repair payload no larger than transient scratch | **pass** |
| Live pending/output copying | Open frames are consumed/moved; sink receives append/drain/finalization deltas. A 512-byte open paragraph reports 0 pending bytes and 512 logical bytes copied, replacing the prior 65,792/65,794 result | **pass** |
| MiB live scaling | See the table below: paragraph/list/table copies are linear in new owned payload; fence/HTML copy 0 pending, logical, and kind payload bytes while copying bounded per-line origin metadata | **raw-literal mechanism pass; other many-line constructs linear** |
| List source-position materialization | Eager oracle scans 16,385 nodes at close and 16,384 at final propagation; the exact overlay records one repair scope, touches/scans 0 descendants, and performs 0 final list scans on the same 1 MiB/8,192-item list | **mechanism pass; persistent-page integration open** |
| Pathological parser transitions | One parser-owned line/finish phase machine drives both unlimited and fuelled wrappers; all 1,322 fixtures plus focused cross-construct cases remain exact, ordinary lines finish in one poll, and 20k quote work stays at 256 transitions/events/index operations with zero open-frame copies per poll | **control authority pass; persistent output integration open** |
| Persistent edit restart/convergence | No persistent source/output page splice or exact edited-suffix convergence in this crate | **open** |

The 1 MiB live receipts (no persisted JSON pause during the run) are:

| Shape | Source bytes | Pending copied | Logical copied | Kind payload copied | Events | Max transient nodes |
|---|---:|---:|---:|---:|---:|---:|
| Open paragraph | 1,048,576 | 0 | 1,048,576 | 0 | 4,097 | 2 |
| Fenced code | 1,048,584 | 0 | 0 | 0 | 4,107 | 2 |
| HTML block | 1,048,595 | 0 | 0 | 0 | 4,108 | 2 |
| 8,192-item list | 1,048,576 | 0 | 1,032,192 | 0 | 98,313 | 6 |
| 1,024-row table | 1,048,600 | 0 | 1,044,492 | 2,054 | 14,361 | 7 |

Code and HTML `BlockKind` values now contain only constant-size
`LogicalProjection` ranges. Their source-backed leaf keeps scalar
`SourceBackedContent` folds plus coverage-relative runs, so finalization and
event delivery copy no literal payload. Separate 1 MiB/10 MiB tests materialize
and stream those projections exactly against Comrak, and cancelling an open
1 MiB fence reports zero aggregate literal ownership. This removes the atomic
whole-literal kernel; it does not remove the prototype's per-physical-line
source `String`, origin run, line offset, or hash-index entry.

The giant paragraph deliberately remains open for the scaling receipt: final
reference-definition classification still returns the known 8 KiB facade
`OverCap` error. That is the oversized-line/classifier gate, not a
continuation-copy regression. Fenced info uses the same cap intentionally: the
exact Comrak normalization allocates only after a preflight range check, with
near-cap and over-cap tests making the bounded transform explicit.

List source-position repair exposed an important boundary. Its exact Comrak
timing differs across fixtures, so a document-final subtree repair is wrong.
The parser emits a write-only repair event at the original finalization point;
the eager materializer remains the differential oracle. A second materializer
now records the repair as a lazy scope. Runtime bindings fold the list's final
preorder interval without reading materialized output, and one global
point-update/range-maximum index supplies descendant maxima without duplicating
every position into every ancestor list.

That overlay matches the eager oracle over all 1,322 fixtures, including the
different repair/overwrite timing in CommonMark 255 and 257. On the 1 MiB,
8,192-item list it records one repair scope, performs zero repair-descendant
touches, zero eager/final list scans, and resolves a 32-node visible page
without widening the query. A complete `BlockDocument` request naturally
resolves every output node; that is an explicit full-output read, not a hidden
repair rewrite. The maximum per-node lazy resolution receipt is 8 steps.

The first overlay draft retained every raw position update. That assumption
was rejected. It now keeps only the current raw `{position, write_seq}` plus a
sparse prior value when a later write crosses a repair whose older snapshot is
still observable. Ordinary per-line paragraph updates and the 8,192-item list
retain zero such snapshots in the measured lanes. A
256-detached-reference-paragraph adversary initially exposed a 273-step reverse
sibling scan; indexed active-child history reduces it to 10 maximum resolution
steps. A 100-level nested-list case initially caused 2,705,604 per-ancestor
multiset updates; the range index reduces that to 10,399 point updates, with
maximum point resolution of 201 steps under the grammar's 100-list nesting cap.

The current flat vector segment tree is still a feasibility representation.
Its point operations are logarithmic, but capacity doubling rebuilds old
leaves. Production persistent output pages must supply the same range-maximum
contract without resize spikes and prove page splice/reuse under edits.

Deep-container receipts separately meter transition, close/finalize, a
one-node position read, and a full output read:

| Quote depth | Source bytes | Nodes | Push | Finish | One-node read | Full read |
|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 2,002 | 1,002 | 0.90 ms | 1.12 ms | <0.01 ms / 2 steps | 0.04 ms |
| 5,000 | 10,002 | 5,002 | 11.66 ms | 3.29 ms | <0.01 ms / 2 steps | 0.22 ms |
| 20,000 | 40,002 | 20,002 | 34.00 ms | 9.42 ms | <0.01 ms / 2 steps | 0.97 ms |

An independent release rerun on the same workstation produced 1.71/0.42 ms,
8.50/2.54 ms, and 37.29/18.19 ms for push/finish at those depths. That red led
to the cooperative control gate rather than accepting the faster sample as a
worker-deadline claim.

The new `begin_line`/`poll_line` and `begin_finish`/`poll_finish` path retains
one live parser/tree and moves only small phase and traversal cursors between
polls. It does not checkpoint, rebuild, serialize, or copy the open frame
stack. A delivery cursor hard-caps downstream `BlockEvent` visibility, while
the receipt separately records generated events and maximum per-transition
fan-out so an atomic producer cannot be hidden by the delivery cap. Deep quote
generation, delivery, transitions, and position traversal all stay within the
256 grants. Cancellation stops scheduling and abandons the phase cursor
without walking or copying the tree; its receipt reports the nodes still
awaiting owner-side reclaim, because destruction of that flat tree is not yet
fuelled. Exact differential passes all 1,322 fixtures and focused
list/setext/table/task/quote/reference interactions.

The focused 20,002-node cancellation receipt measures phase abandonment at
less than 1 us and the later flat-tree `drop` at 208 us on this workstation.
That is encouraging but not an accepted bound: the drop is outside the poll
budget and must become fuelled page retirement before production/device claims.

Release receipts with 256 transition/event/index grants are:

| Shape | Polls (line / finish) | Max line poll | Max finish poll | Max transitions/events/index per poll |
|---|---:|---:|---:|---:|
| 1,000 quotes | 8 / 11 | 113 us | 24 us | 256 / 256 / 256 |
| 5,000 quotes | 40 / 59 | 138 us | 57 us | 256 / 256 / 256 |
| 20,000 quotes | 157 / 235 | 224 us | 132 us | 256 / 256 / 256 |
| 20,000 quotes, continued then deindented | 315 / 235 | 234 us | 215 us | 256 / 256 / 256 |

The list challenge exposes two deliberately separate limits rather than a
false green. Comrak's selected grammar caps list opening depth at 100, so a
1,000-marker request creates at most 100 list levels; it remains exact and
fuelled, but one list-finalization kernel reaches 346 us because subtree
position repair is not yet decomposed. The 5,000- and 20,000-marker physical
lines are rejected by the current fixed 8 KiB facade before list parsing. The
generated refillable-scanner gate must remove that rejection; it is not
silently bypassed here.

A 300-column GFM table adversary proves the output accounting is honest: the
delivery cursor never exposes more than 256 events in a poll, while the receipt
reports 304 events generated by one table-promotion transition. Thus the
consumer backpressure mechanism passes, but table construction itself remains
an atomic 304-event producer and is explicitly red for the persistent event
page integration.

Operation receipts are linear or `N log N`: child summaries fold once,
materialization is iterative, point resolution stays two steps, and a full
read performs two resolution steps per node. This falsifies the suspected
depth-squared close/finalize path, and the cooperative parser seam removes the
deep quote transition/close as an intrinsically atomic grammar operation.

This is intentionally not yet an end-to-end `ResumableValueBlockParser` pass.
That wrapper still materializes and compacts the whole open path at a physical
line boundary, and the vector sink applies emitted events/index changes
synchronously. Production must drain the same exact events into persistent
pages under output/index fuel and retain live open frames instead of rebuilding
them each line. Other honest atomic kernels remain: fixed facade scanners,
dense table-row fan-out, `add_child` compatibility closing, reference
classification, paragraph/reference aggregate strings, and list repair
snapshots/subtree folds. Raw code/HTML finalization is no longer on that list:
it folds constant-size projections over source-backed runs. None has acquired a
second classifier or syntax-specific fallback in this gate.

The deferred repair event also still carries scratch-position snapshots for
the bounded live scratch tree. That is useful as the timing oracle, but the
100-level nested-list receipt produces 9,800 sparse crossing snapshots. The
shipping event lane should emit write-sequenced position deltas plus the folded
preorder interval directly; it must not retain the snapshot-vector witness.

The JSON path remains deliberately copy-based as a hidden-state falsifier.
Live scheduling does not serialize it: internal compaction consumes/moves each
open `BlockKind`/`LeafContent`, and output receives append/drain/finalization
deltas. Persisted restart frequency must therefore be sparse/adaptive for giant
open constructs, or its checkpoint payload must retain immutable source leases
rather than serializing a growing prefix.

## Refillable physical-line feasibility slice

The donor crate now contains a production-shaped but deliberately standalone
bounded-window proof. A non-cloneable `RefillableSourceLine` carries the
existing private root/revision/snapshot/line identity and certified byte/UTF-16
metric without borrowing source. `RefillableLineJob` asks an external source
capability to fill at most its bounded destination, validates arbitrary UTF-8
refill splits and the claimed metric under fuel, retains no aggregate source
copy, and emits at most three exact relative/absolute provenance claims.

| Gate | Receipt | Result |
|---|---|---|
| Giant paragraph | 10 MiB content plus CRLF, 4 KiB maximum read/inspection, one prefix byte then streamed body | **pass** |
| Giant fenced literal | 10 MiB content, exact two-space deindent/content/canonical-newline partition, bounded scratch | **pass** |
| Refill boundaries | UTF-8 scalar, opening marker, and closing marker split across one-byte windows | **pass** |
| Cancellation/source authority | Cancellation emits zero completed claims and zero retained aggregate source bytes; wrong snapshot and bad metric fail before completion | **pass** |
| Donor correspondence | Focused lines plus 512 randomized admitted document/fence lines match `DirectValueBlockParser` coverage ranges/actions | **pass** |
| Significant other constructs | ATX, quote, list/thematic, HTML, reference-definition, and setext-leading lines fail closed to the donor | **explicitly open** |
| Direct parser composition | `DirectValueBlockParser::begin_line(String)` still rejects over 8 KiB and `LineTransition` still takes whole `&str` | **open** |

The differential receipt corrected the proof's first ownership assumption:
leading spaces before paragraph content belong to a Document-owned `Gap`, not
to Paragraph identity content. That is now frozen by direct-command comparison.

This slice therefore challenges the claim that an arbitrarily long ordinary
line must be synchronously materialized, but it does not remove the shipping
ceiling. The remaining seam is a single trusted donor transition entry point
that consumes certified recognition facts/ranges and streamed metrics, then
performs the existing grammar mutation without a source rescan. Wiring each
standalone recognizer directly to mutable output instead would create a second
grammar and is not an acceptable completion.

## Known profile/resource gaps

- The checked-in 670-example GFM corpus does not include task-list items. Six
  focused task-list contexts now pass, but the 1,322-example score still must
  not be described as exhaustive selected-Flark-profile coverage.
- This crate's direct parser and lexical facade calls remain hard-capped at
  8 KiB. The donor-local refillable proof now covers definitive ordinary
  paragraphs, fenced openers, and tab/NUL-free lines inside an open fence; the
  separate oversized-block-line gate still supplies the broader exact scanner
  witnesses, and the generated-scanner gate derives one storable ATX DFA from
  pinned Comrak rules. Significant prefix families and, more importantly, the
  one parser-owned refillable transition/mutation hook are not yet integrated.
- Fenced-code and HTML finalization now derives coverage-relative projections
  without scanning or copying the aggregate literal. The 1 MiB/10 MiB direct,
  live, render, and cancellation receipts pass. Remaining composition work is
  to coalesce the prototype's one identity run/line offset/source `String` per
  physical line into persistent coverage pages and prove the same counters
  through the real `PhysicalLineView`/Crop adapter.
- The direct `SourceDocument::new` test adapter still assigns line-ordinal
  coverage IDs and clones one `String` per physical line. The live API and
  lookup index accept arbitrary stable IDs, but the real source layer must
  supply them without rebuilding this test representation after an edit.
- Paragraph/reference-definition content remains aggregate-owned, and the
  facade reference parser still consumes its bounded input atomically. Those
  paths need the same persistent-page/copy-accounting audit before a composed
  memory claim.
- Reference occurrences are correctly outside block continuation, but the
  test renderer's sequential symbol IDs are not a production-stable symbol
  index.

## Decision

Continue with this single Flark-owned, Comrak-correspondent block spine rather
than the Comrak arena-checkpoint fork or a second independent grammar. The
decisive semantic-continuation concern is now answered: exact behavior survives
write-only output and canonical restart without hidden tree state.

Do not yet call the implementation production-ready. The output-overlay gate
removes the list-repair scan as a reason to reject the architecture, but it is
not yet the persistent editor index. The next discriminator is composition:
integrate the point resolver/aggregate with stable source/output pages and
immutable leases, then prove edit restart, suffix convergence/reuse,
cancellation/reclamation, per-line run coalescing, paragraph/reference
ownership, oversized-line yielding, and native/WASM resource bounds. If those
changes require restoring parser output reads or duplicating grammar, stop and
reopen the donor decision.
