# Direct parser-to-packed-green feasibility audit

Status: **direct `StructuralEvent` adapter REJECT; exact parser algorithms
retain; parser output/continuation refactor required; composed vertical slice
GO**, 2026-07-16.

This audit pressure-tests
[`DIRECT_PARSER_GREEN_COMPOSITION_GATE.md`](DIRECT_PARSER_GREEN_COMPOSITION_GATE.md)
against the current `comrak_value_block_core`, `restart_composer_gate`, and
`v3_runtime_slice` implementations. It is a source audit, not a new Rust
prototype. No conclusion below relies on the intended architecture being
present merely because a type with a similar name exists.

## Executive verdict

The architecture's **topology remains sound**:

```text
exact parser control
  -> typed, capability-bound output work
  -> one resumable candidate transaction
  -> one immutable packed structural/source root
  -> one atomic revision manifest
```

But the current gate overstates how settled the seam is. The accurate status
is:

- **Selected:** one exact off-thread parser, source-ordered serialized green,
  immutable base roots, typed suffix composition, no Dart predictor.
- **Rejected:** implementing production by attaching a clever bounded adapter
  to `WriteOnlyBlockSink<StructuralEvent>`.
- **Required:** retain the exact line machine and donor-derived scanners, but
  structurally refactor their output ownership. Parser decisions must write to
  a direct typed port at the decision sites, and open continuation must retain
  source-run/scanner state rather than an aggregate logical `String`.
- **Still provisional:** the exact source/projection codec, transaction-local
  open/range capabilities, reference-definition source treatment, table
  promotion recipe, and composite manifest.

This is not a recommendation to rewrite CommonMark/GFM recognition. It is a
recommendation to replace the proof parser's AST-materialization boundary.
That is materially smaller and safer than a grammar rewrite, but materially
deeper than a downstream adapter.

## What the current parser actually does

The production-shaped proof path is currently:

```text
physical line text
  -> ValueBlockParser mutates a transient BlockTree
  -> BlockTree records chronological BlockEvent history
  -> ResumableValueBlockParser::materialize diffs reachable scratch
  -> StructuralEvent(handle, state, repair, copied content, ...)
  -> TreeMaterializer maintains a second mutable tree and handle directory
  -> compact_live destroys/rebuilds parser scratch around the open path
```

Concrete evidence:

- `StructuralEvent` carries copied `CoverageLeaf`, numeric handles, mutable
  `Open`/`Update` state, detach, position repair, append/drain, and references
  (`checkpoint.rs:770-854`).
- `push_line` copies every physical line, runs the whole line synchronously,
  materializes, then compacts (`checkpoint.rs:953-976`).
- `materialize` constructs `HashSet`/`HashMap` views of reachable and existing
  nodes, allocates numeric handles, detects detach by reachability, replays
  list-position repair history, copies content deltas, and emits another
  preorder/postorder update stream (`checkpoint.rs:1221-1509`).
- `compact_live` folds closed children, moves every open frame into a temporary
  vector, constructs a new `ValueBlockParser`, and rebuilds the open scratch
  path (`checkpoint.rs:995-1114`).
- `TreeMaterializer` retains the document-wide `handle -> NodeId` directory
  that gives those events meaning (`checkpoint.rs:2079-2250`).

This is strong feasibility evidence. In particular, every-line resume across
the 1,322 block fixtures proves that closed history can leave parser scratch.
It is not the storage seam we want to preserve.

## Why a downstream line-local adapter is not authoritative

### 1. The sink never receives an exhaustive source partition

This is the decisive blocker.

`SourceLeaf` gives the adapter one copied physical line. `AppendContent` gives
logical content origins for selected leaf content. Nothing tells the adapter,
without reinterpreting parser behavior, which exact source intervals were:

- block-quote/list indentation and markers;
- ATX opening or trailing markers;
- opening/closing fence markers and trailing terminal bytes;
- a Setext underline;
- a thematic break;
- a GFM delimiter row or pipe separators;
- blank-line gaps and line endings;
- the skipped BOM; or
- source bytes removed as leading reference definitions.

Those decisions exist only at the parser call sites. For example:

- quote and item prefixes advance the parser cursor in
  `parse_block_quote_prefix` and `parse_node_item_prefix`
  (`parser.rs:613-644`);
- closing fences consume the delimiter and finish the line before `add_line`
  (`parser.rs:647-697`);
- Setext and thematic-break handlers consume their marker lines without
  appending logical content (`parser.rs:829-909`);
- table delimiters and rows consume whole lines in `table.rs:135-142` and
  `table.rs:227-238`; and
- `add_line` reports only content that survives the current offset and, for an
  ATX heading, receives an already chopped line (`parser.rs:493-548`,
  `parser.rs:1122-1205`).

`Position { line, column }` cannot recover these intervals exactly. Columns
are not byte offsets in the presence of tabs and Unicode, and source positions
do not encode semantic part or logical transformation. Inferring the missing
ranges from the copied line would be the second Markdown classifier that this
architecture is intended to eliminate.

Therefore an adapter over today's sink can build a plausible tree, but it
cannot build the definitive structural/source stream.

### 2. Final output truth is not available at current event boundaries

The parser deliberately mutates provisional nodes:

- ATX headings are opened at level 1 and changed immediately afterward
  (`parser.rs:728-765`).
- a Paragraph becomes a Setext Heading only after reference-prefix
  finalization (`parser.rs:829-856`).
- code `info`/`literal`, HTML literal/end, list tightness, and reference-only
  disposition are finalized after `tree.close(node)` has already recorded a
  chronological Close (`parser.rs:1308-1419`).
- table construction opens detached nodes, inserts a possible preface sibling,
  constructs rows/cells, then replaces and detaches the old paragraph
  (`table.rs:39-147`, `parser.rs:1037-1054`).

`ResumableValueBlockParser::materialize` hides this by inspecting the final
scratch state and reordering it into updates and closes. A direct adapter below
that layer must either retain mutable output handles or reproduce the same
diff. A clean direct port instead needs an explicit parser finalization point
that produces final normalized storage facts plus the close contribution.

The packed representation's decision to put child contribution on `Exit` is
correct. The current `StructuralEvent::Close { handle }` does not carry that
contribution, however; it is computed from the scratch tree by
`closed_child_summary` (`tree.rs:526-548`).

### 3. “Line local” is not currently bounded or fuelled

`ResumableValueBlockParser::push_line` calls `process_line` to completion
before returning (`checkpoint.rs:953-970`). The separate
`FuelledValueBlockParser` moves a line through `LinePhase`, but it only meters
delivery from the existing `BlockEvent` vector (`parser.rs:1503-1599`). It does
not feed a direct sink or compact/restart state.

More importantly, table row/header construction is still one atomic producer:
it loops over every parsed and synthesized cell (`table.rs:124-133`,
`table.rs:198-225`). The existing
`dense_table_event_delivery_is_capped_and_atomic_generation_is_visible` test
explicitly proves that one transition generates more events than the delivery
budget. Thus `CurrentLineActions` in the composition gate cannot simply be a
bounded `Vec`; dense table scanners and action generation must become
resumable phases.

One physical line may also contain thousands of block-quote opens. The current
test suite deliberately exercises depths up to 20,000. “One line” is an exact
convergence boundary, not a complexity bound.

### 4. Open paragraph state still owns aggregate payload

`SemanticFrame.pending` is a full `LeafContent`, and ordinary paragraph/table
content owns `LeafContent.logical: String` (`checkpoint.rs:51-66`,
`source.rs:90-97`). `compact_live` moves it rather than copying it, which is a
good linearity result, but a long open paragraph still leaves parser
continuation owning document-sized logical bytes.

Actual paragraph finalization clones that whole string before reference
scanning (`parser.rs:1459-1484`). Table promotion clones the whole paragraph
content and creates transformed strings for preface and cells
(`table.rs:68-69`, `table.rs:165-170`, `table.rs:267-276`). The bounded
transition projection proves that future grammar can be scalar, but the live
parser has not adopted the production split yet.

The direct parser must retain persistent source runs plus resumable recognizer
state. Bounded scanners may borrow/materialize windows. Neither control nor
structural green storage should own the aggregate string.

### 5. Numeric handles are not the only identity gap

Replacing `u64` handles with `BlockId` would not make the adapter sound.

- The current parser has no stable edit-lineage BlockId allocation policy.
- `OpenOutputBindings` binds only numeric handles and absolute source
  positions (`checkpoint.rs:687-716`).
- table promotion creates a new table and may split a preface paragraph, so
  survivor/retirement rules are semantic, not a mechanical handle rename.
- reference-only detach must retire a semantic wrapper while preserving every
  source byte in the structural/source stream.

The production binding has to be an open-path lease tied to a base manifest or
candidate build, with BlockId carried as evidence inside it.

## Construct-by-construct composition audit

| Construct | Current exact behavior | Why `StructuralEvent` translation is insufficient | Direct action/data required |
|---|---|---|---|
| Ordinary paragraph | `add_line` appends copied logical bytes plus origin runs; paragraph stays open across lines | no exhaustive marker/gap/terminal partition; open accumulator is an aggregate string | source-run append from stable source lease; paragraph line-boundary projection; resumable reference/table recognizers |
| Quote/list prefix | cursor advances while matching/opening containers | consumed indentation/marker interval and owner are never emitted | parser-site `claim_source(span, owner_lease, ContainerMarker)` |
| Setext | resolves definitions, optionally drains prefix, mutates Paragraph kind, consumes underline | `Update` can show final Heading but cannot place definition source outside the heading or classify underline exactly | atomic paragraph-finalization recipe: source-prefix definitions, visible runs, underline run, stable BlockId promotion |
| Reference-only paragraph | closes, drains definitions, records occurrences, detaches node | removing the packed balanced range would also remove source coverage and violate total metrics | **unwrap semantic wrapper**, retain/reclassify source coverage under parent, publish occurrences/winners in same candidate |
| GFM table header | clones paragraph, optionally splits preface, creates detached Table/Row/Cells, then replaces paragraph | requires retroactive wrappers around prior source slices; cell transforms and delimiter ownership are absent from sink | range rewrite with cut capabilities, deterministic identity recipe, typed cell projection runs, alignment edge, delimiter coverage |
| GFM body row | creates row/cells and pads missing cells in one loop | event fan-out is unbounded; transformed content is copied | resumable row/cell action phase; source slices plus trim/unescape descriptors; zero-source empty cells |
| Fenced/indented code | raw payload is source-backed; projections become final at close | good source-run evidence exists, but opening/closing markers and terminal bytes are absent; final projection is a later mutable kind update | marker/content/terminal claims; final projection root attached at close without Enter backpatch |
| HTML block | raw payload is source-backed and scalar trim folds are maintained | same source partition gap; final end/literal arrives after chronological close | direct raw-run append and close-time projection facts |
| List tightness | historical children reduce to `ChildSequenceFold`; list tightness finalized at close | position-repair events are donor AST baggage; copying `tight` into every descendant would be wrong | emit each child's close aggregate on Exit; derive tightness from packed summaries/presentation |
| Source positions | list repair overlays historical output ranges | line/column positions are unnecessary in a relative source stream and invite global repair | derive query ranges from sequence metrics; do not port repair events |

## A correction required for reference “detach”

The current composition gate lists `DetachRange { balanced_range_capability }`
and says a reference-only paragraph detaches. That naming imports mutable-AST
semantics into the unified structural/source representation.

In serialized green, this input:

```markdown
[label]: /url
```

still has source coverage after its Paragraph disappears from the rendered
block tree. Deleting the entire balanced Paragraph range would reduce the
document metric. Keeping the range detached outside the manifest would create
a second source authority.

The required operation is closer to:

```text
FinalizeReferencePrefix {
  paragraph_lease,
  definition_source_runs,
  visible_source_runs,
  occurrences,
  winner_delta
}
```

If `visible_source_runs` is empty, storage removes only the Paragraph
`Enter`/`Exit`, leaves the enclosed coverage in source order, and therefore
naturally changes its owner to the still-open parent. It may reclassify the run
as a reference-definition/source-only part. If visible content remains, it
splits the source prefix from the surviving Paragraph wrapper.

Setext with leading definitions needs the same primitive before promotion.
This is an enumerable typed mutation, but it is not generic range deletion.

## A clean production seam

The smallest coherent seam is a **parser-owned typed output port**, not a
storage-aware parser and not a `StructuralEvent` translator.

### Keep

- `LinePhase`, exact container matching, handler ordering, donor-derived
  scanners, list-marker rules, HTML/fence recognition, table recognition, and
  the existing one-shot/resume differential oracles.
- parser-private `NodeId` scratch where it simplifies one in-flight
  transition. Scratch identity is allowed; it must not escape as persistent
  output identity.
- constant-size child folds and the separation between grammar continuation
  and output accumulators.

### Replace

- `BlockTree.events` as production output;
- `ResumableValueBlockParser::materialize` and `TreeMaterializer` in the
  production dependency path;
- document-wide output handles and source-position repair;
- aggregate paragraph/table logical strings in live continuation; and
- atomic dense-table producer loops.

### Port shape

`comrak_value_block_core` should define a representation-neutral trait whose
methods use typed scopes, for example:

```text
ExactBlockOutput
  begin_line(stable_source_lease)
  open_new(parent_lease, identity_recipe, normalized_open_facts) -> OpenLease
  claim_source(open_lease, source_slice, source_part, projection_recipe)
  finalize_paragraph(open_lease, paragraph_recipe)
  promote_table(open_lease, table_recipe) -> OpenLease
  close_open(OpenLease, final_projection_facts, closed_child_contribution)
  record_references(reference_recipe)
  finish_line(exhaustive_partition_receipt)
```

The trait should have associated, opaque lease types. It must not name
`PageArena`, `ArenaId`, or packed byte layouts. The integration crate implements
the leases with base-root or candidate-build capabilities.

Calls must occur as semantic decisions become exact. A parser transition may
yield between typed actions, but action application and the phase cursor must
advance together so retry cannot duplicate an action.

### Exhaustive per-line source ledger

At `begin_line`, the output port receives a stable, revision-bound source
slice—not owned text. Parser decision sites claim disjoint byte intervals with
an owner lease, part, and logical projection recipe. At `finish_line`, a ledger
must prove:

```text
claimed intervals are ordered, non-overlapping, and cover [0, line_bytes)
sum byte metric == physical line bytes
sum UTF-16 metric == source backend metric
every logical run maps to source or an explicit synthetic recipe
```

Unclaimed bytes are an error during development, not silently converted to a
Document gap. Explicit gap/terminal claims are necessary because ownership is
product behavior.

Partial tabs keep physical byte/UTF-16 metrics in the coverage run and logical
space count in a projection recipe. Table trim/unescape likewise stores the
source slice plus transform descriptor rather than the transformed string.

### Normalize facts before the codec

`BlockKind` is a donor-shaped mutable state bag; it should not be serialized
wholesale. The direct port should define final normalized facts so facts known
later do not force arbitrary Enter backpatching:

- List Enter: marker/delimiter/displayed start; tightness derived from Exit
  folds.
- Code Enter: fence configuration; info/literal are projection roots finalized
  at close.
- HTML Enter: block type; literal projection finalized at close.
- Table Enter: column count plus typed alignment edge; row/nonempty counts are
  derived summaries if the product needs them.
- Heading Enter: exact level/setext bit, constructed only after the handler has
  the final level.

Transaction-local Enter finalization remains necessary for Setext and table
promotion, but ordinary blocks can open with final bounded facts.

## Current storage and composer gaps

The direct port cannot be implemented honestly against today's public APIs.

### `v3_runtime_slice::serialized_green`

What exists:

- complete clean `build` from an iterator of already-final `GreenEvent`s
  (`serialized_green.rs:885-968`);
- source metric/owner/part records and exact source-first seek
  (`serialized_green.rs:207-265`, `serialized_green.rs:1552-1889`);
- one committed-base batch that rewrites complete Enter records
  (`serialized_green.rs:1002-1131`); and
- packed close-time child summaries.

What the composed parser still needs:

1. `CandidateBuildId` plus short-lived sessions that survive worker yield.
2. Transaction-local append/open/close leases.
3. Token/source cut capabilities and one-base-root range batches.
4. Wrapper removal that preserves coverage, Setext promotion, table
   replacement/reparent, and suffix attachment.
5. Boundary repacking rather than retaining a leaf per small mutation.
6. A logical-projection codec. Current `CoverageRun` has only ID, byte/UTF-16
   metric, owner depth, and part (`serialized_green.rs:207-240`).
7. Typed arena edges for alignments and projection/source-run vectors. The
   current facts comment promises external roots, but leaf decode rejects all
   child edges and only the bounded opaque envelope exists
   (`serialized_green.rs:152-204`, `serialized_green.rs:1519-1531`).
8. A composite manifest. The current manifest owns only the structural
   sequence and a few generations (`serialized_green.rs:719-728`,
   `serialized_green.rs:1207-1288`).
9. Candidate/base capability variants. `GreenEnterCapability` and
   `GreenCoverageCapability` currently address only a committed manifest leaf;
   there is no balanced-range or suffix-boundary capability
   (`serialized_green.rs:1552-1577`).
10. A test-only canonical event/projection visitor. `seek` is a viewport API,
    not a complete differential decoder.

### `restart_composer_gate`

The composer proves valuable logic: control equality is necessary but
insufficient, lineage and suffix identity are checked, actions are bound to an
ordered open path, and the permit is consumed before storage use.

It is not integrated storage authority yet:

- IDs and `CapabilityId` are local proof types, not v3 manifest/leaf/range
  capabilities (`restart_composer_gate/src/lib.rs:364-405`).
- `ControlFrame`/`BindingRole` cover the proof variants, not the full
  `BlockTransitionKind` path used by the real parser
  (`restart_composer_gate/src/lib.rs:320-405`).
- `SourceRuns`, reference folds, and child folds are proof-local persistent
  values, not decoded roots from the candidate/base manifest.
- `StorageAdoptionContext` validates caller-supplied proof bindings, but no
  current code derives those bindings from a v3 manifest capability
  (`restart_composer_gate/src/lib.rs:956-1075`).
- `PromoteTable` identifies the new table by BlockId but has no table range,
  cut, projection, or candidate-build lease (`restart_composer_gate/src/lib.rs:1343-1381`).
- no v3 operation consumes `StorageAdoptionPlan` or attaches an immutable
  suffix.

These are expected for a proof crate, but they mean the direct gate must not
describe the executable composition contract as selected yet.

## Smallest coherent first composed crate

Create one integration-only crate, provisionally:

```text
tool/parser_research/direct_green_composition/
```

with real path dependencies on:

```text
flark-comrak-value-block-core
flark-restart-composer-gate
flark-v3-runtime-slice
```

It should contain no copied parser, composer, arena, or green capability types.
Its job is to implement `ExactBlockOutput` for `PackedGreenCandidate` and make
type mismatches visible.

### Required dependency changes before the crate can be meaningful

1. **Parser core:** add the typed output port and source ledger; split table
   header/body fan-out into resumable phases; expose real control/output restart
   values without aggregate strings.
2. **V3 storage:** add resumable candidate lifecycle, transaction-local leases,
   cut/range/wrapper operations, typed projection edges, and a minimal composite
   manifest.
3. **Composer:** add an integration adapter that binds proof values to actual
   base-manifest capabilities and yields a storage-consumable suffix plan. Keep
   composer logic representation-neutral; the adapter belongs in the composed
   crate.

### First vertical acceptance slice

Do not call the entire production matrix the “first executable slice.” The
current ten requirements mix seam validation, adversarial scale, full-corpus
correctness, ownership fuzzing, and platform release gates. Use three levels.

#### Slice A: seam falsifier

The first crate is worth keeping only after it passes all of these:

1. Clean direct parse, without `TreeMaterializer`, for:
   - ordinary Paragraph;
   - nested quote/list marker ownership;
   - Setext with and without a leading reference definition;
   - reference-only paragraph wrapper removal;
   - fenced code including a closing fence and bare EOF fence;
   - GFM whole-header and split-preface table promotion;
   - escaped-pipe and padded-short table cells; and
   - BOM, tab, LF, CRLF, lone CR, and non-BMP UTF-16 metrics.
2. Every physical line has an exhaustive source-ledger receipt. Reference-only
   wrapper removal preserves the exact total byte/UTF-16 metric.
3. Canonical packed output equals the unlimited parser for structure, final
   normalized facts, logical projections/origins, references, and HTML on
   those fixtures.
4. One local edit starts from an immutable base, uses a real composer permit,
   adopts a balanced suffix, and preserves one distant suffix leaf by exact
   `ArenaId`.
5. Cancellation or injected allocation failure before each typed operation
   leaves the old root queryable and the candidate reclaimable.
6. The crate retains no `StructuralEvent`, `TreeMaterializer`, copied source
   leaf, aggregate logical string, or document-wide BlockId/handle directory.

This slice is deliberately small but attacks every hard seam: source truth,
retroactive wrapper mutation, transforms, typed capabilities, suffix adoption,
and ownership.

#### Slice B: architecture proof

After Slice A, expand to:

- every-line restart and permitted convergence over all 1,322 block fixtures;
- 100,000-item tightness mutation bounded by changed pages/path depth;
- 10 MiB raw block with shared source/projection runs;
- 300-column and oversized refillable table/reference scanners;
- 10,000 same-gap edit repacking;
- randomized edit histories and stable identity rules; and
- debug/release, strict Clippy, native/WASM, and ownership stress.

#### Slice C: product gate

Only after Slice B should worker scheduling, Dart bridge, Flutter adoption,
physical-device input-to-paint latency, memory peaks, and degraded/Unknown
behavior become acceptance gates.

## Identity rules the first slice must make explicit

The composed crate should fail compilation or tests until these are encoded,
not guessed:

- Setext: the Paragraph BlockId survives as Heading.
- Reference-only finalization: the Paragraph BlockId retires; source coverage
  IDs survive according to split rules.
- Table whole-header promotion: old Paragraph retires; new Table/Row/Cell IDs
  are minted deterministically.
- Table split-preface: the start/preface Paragraph fragment keeps the old
  Paragraph identity; Table/Row/Cell IDs are new.
- Coverage split: the start fragment keeps the old CoverageId; later fragments
  are minted. Merge keeps the left identity.
- Reclassification without logical-run survival retires the old CoverageId.
- IDs are validators inside capabilities and are never lookup keys.

## Changes required in the composition gate

The following claims should be tightened before implementation:

1. Change the status from “integration contract selected” to **“integration
   topology selected; exact output/codec contract provisional.”**
2. Explicitly reject a production adapter over `StructuralEvent`, while
   allowing a parser-owned bounded/fuelled current-transition staging area.
3. Replace `DetachRange` for reference paragraphs with an operation that
   removes/repositions semantic wrappers while preserving source coverage.
4. State that `OpenNew` receives normalized facts known at open; close-time
   projections and the enumerated Setext/table promotions finalize through
   typed operations. Do not serialize mutable `BlockKind`.
5. Add the exhaustive source-ledger invariant. Total metrics alone do not prove
   correct marker/gap ownership.
6. Record dense table fan-out as a known current counterexample to bounded
   line actions.
7. Split the current ten-item “first executable slice” into seam,
   architecture, and product gates.
8. Add a stop condition for reconstructing source roles from positions,
   origins, or copied line text after the parser decision.

## Final recommendation

Proceed with the architecture, but implement the next experiment at the
**parser decision/output boundary**, not under `WriteOnlyBlockSink`.

The exact parser is not a dead end. Its control machine, resume corpus, child
folds, and donor correspondence are the strongest assets in the current work.
The fragile part is the proof-era ownership model around them: mutable scratch
tree history, aggregate paragraph strings, copied source leaves, and the
materializer's handle/position protocol.

The best next move is therefore a surgical structural refactor:

```text
keep exact recognition
replace AST/history output
add exhaustive source claims
make large producers resumable
bind typed actions directly to a resumable packed candidate
prove one real suffix adoption before broadening the corpus
```

If Slice A requires a global output directory, reparsing source roles, or a
second source/projection tree, stop. If it passes, we will have validated the
clean architectural seam—not merely demonstrated that one mutable tree can be
translated into another.
