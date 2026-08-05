# Selected runtime extraction audit

Status: prototype evidence, 2026-07-18. This is an extraction audit, not a
production implementation or an amendment to RFC 023.

## Verdict

The source-backed ATX -> candidate writer -> packed green -> bounded real
Comrak inline path is a valid walking skeleton.  Subsequent retained-restart,
Setext, canonical-fragment, Table-writer, host-publication, and Dart source
session gates show that the production engine is materially larger than that
skeleton.  It still does **not** justify shipping or depending on
`v3_runtime_slice`: that crate mixes selected mechanisms with proofs, rejected
representations, duplicate authority wrappers, and embedded test apparatus.

The smallest coherent production topology remains three crates:

1. `flark-markdown-semantics`: one Comrak-correspondent value-state block
   transition, generated/bounded lexical helpers, parser continuation values,
   and the narrow bounded Comrak inline service;
2. `flark-engine`: Crop source, exact source admission, packed arena/sequence,
   candidate composition, current revision actor, lazy presentation, and host
   publication; and
3. `flark-bridge`: native/Wasm transport and versioned wire codecs only.

`flark-markdown-semantics` must not depend on engine identities or storage.
`flark-bridge` must not be imported by either lower crate. The dependency graph
is therefore a DAG:

```text
patched Comrak narrow facade
        |
flark-markdown-semantics
        |
flark-engine
        |
flark-bridge
```

The Flutter/Dart session owns the current UI source root.  `flark-engine`
owns exactly one acknowledged worker replica plus at most one non-authoritative
candidate derived from that replica; parser, checkpoint, green, and
presentation modules only borrow source leases.  Replica acknowledgement,
source-fact certification, and structural publication are separate lineages.

There are now two different size claims and they must not be conflated:

- **36-43k maintained non-test Rust lines** remains a reasonable budget for a
  walking skeleton with fresh parsing, packed output, narrow inline parsing,
  and copied host publication.  It is not a production large-document engine.
- A production-capable selected CommonMark/GFM engine is expected to require
  **68-75k maintained non-test Rust lines**, with an honest planning range of
  **65-80k**.  Review the topology at 75k and rearchitect before crossing 80k.

The production budget includes restart/checkpoint authority, split
normalization, references, Table control, the source-worker journal and
certification seam, copied host publication, giant-line execution, and the
native/Wasm bridge.  Dart controller/source code is reported separately; its
initial planning range is 3-6k non-test lines and it must not be hidden in the
Rust number.  The maintained Comrak seam currently comprises about 1,589 added
Rust lines (and 19 deletions) plus roughly 217 Flark-local origin-map lines.
Every changed/local seam line counts toward the 80k ceiling even if it lives in
the fork or bridge.

## Measured current surfaces

Counts below are audit snapshots of physical source.  Prototype files often
embed tests; production targets exclude tests, benchmarks, generated fixtures,
and provenance data.  “Compiled” means the current crate/module declaration
includes the code even when the walking skeleton does not execute it.

| Surface | Current evidence | Extraction disposition | Production target |
| --- | ---: | --- | ---: |
| Comrak-correspondent block/control family | 15,971 at the earlier audit, before the latest direct-path work | Extract the value-state transition, command protocol, and resumable continuation. Drop batch rendering, test materializers, duplicate checkpoint proofs, and legacy document APIs. It must no longer wrap `ValueBlockParser`, `SourceDocument`, or `BlockTree`. | 8-10k |
| Generated and bounded scanners | 2,524 at the earlier audit | Retain source-backed selected scanner families, including resumable oversized-line entry points. Remove legacy duplicates as donor-backed cursors replace them. | 2-3k |
| Narrow Comrak additions and local origin seam | about 1,589 additions, 19 deletions, and 217 local origin-map lines | Keep pinned, inventoried, and upgrade-rehearsed together. Count every maintained line, regardless of repository placement. | 1.8-2.2k |
| Worker source replica and CommonMark line index | 4,538 at the earlier audit, excluding the Dart authority/session | Extract exact line descriptors/cursors, actor-owned byte sessions, revision/root identity, giant-line yielding, and bounded retirement. | 5-6k |
| Page arena and persistent sequence | 5,411 at the earlier audit | Retain the proven 4 KiB, generation-safe, COW, prefix-metric, fuelled-retirement mechanisms. | 5.5-6.2k |
| Packed green and canonical fragments | 17,603 compiled at the earlier audit | Extract codec, streaming builder, logical cursor, leaf/program access, current-root splice, canonical-fragment origin/rebase, and finite folds. Exclude proof-only and rejected representations, not production restart support. | 9-11k |
| Ledger/composer/writer/exact driver | 18,477 before embedded tests at the earlier audit | Extract one fresh pipeline plus normalization recipes. Merge duplicate capability wrappers only where the owning candidate already proves the invariant. | 11-13k |
| Current actor and copied host publication | previously split across live-document, coordinator, staging, and mirror proofs | Keep one worker-current root, at most one candidate, one latest queued edit, and one unacked copied offer. Include snapshot/delta sequencing, restart, nack, and retirement. | 6-8k |
| Inline leaf materializer and lazy cache | 2,019 at the earlier audit | Keep bounded green/source materialization and leaf-relative facts. Never retain absolute physical suffix facts. | 2-2.5k |
| Reference occurrence/winner/dependency root | mechanisms proven separately; integrated candidate root still open | Persist ordered occurrences, first-definition winner, consumers, and bounded invalidation under the same candidate lineage. | 3-4.5k |
| Composite checkpoint/restart/normalization | spread through proof modules and still entangled with writer/storage types | Retain source, parser, writer, green, reference, and canonical-fragment continuations behind one top-level manifest/join. | 6-8k |
| Native/Wasm bridge | versioned transport exists, production worker application remains open | Codecs and transport only; no grammar, source authority, or worker-root query API. | 3-4k |

The current `v3_runtime_slice/src` tree contains 119,721 physical Rust lines,
including embedded tests and proof-only code.  A conservative selected-v3
closure using only obvious whole-module pruning was approximately
73,774-74,428 lines; adding the donor parser/scanner and actual maintained
Comrak/local seam produced approximately 87,496-88,150 lines, before a real
reference root and the complete source-publication protocol.  Those closure
figures are not production targets: they measure how much coupling must be
removed by extraction.

The 68-75k expectation therefore requires real deletion and ownership cleanup,
especially inside packed green, the ledger/writer/driver family, and the
restart proofs.  A crate which merely wraps `v3_runtime_slice` has failed the
size and reviewability gate regardless of runtime benchmarks.  Re-run the
closure inventory after the reference and Table control gates; do not lower the
budget by excluding required mechanisms or moving code across crate lines.

## Selected module closure

The first production crate should contain this one-way module graph:

```text
model / identity
  |-- source (Crop, line index, byte session, edit lineage)
  |-- storage (arena -> persistent sequence)
  |-- green (model -> codec -> query -> current-root splice -> host export)
  |-- block pipeline
  |     source ledger -> projection composer -> candidate writer
  |     semantics commands --------------------------^        |
  |     exact driver -----------------------------------------^
  |-- references (order index, occurrences, winners)
  |-- checkpoint (semantics continuation + writer/source/green/reference roots)
  |-- presentation (green query + source + bounded inline + lazy cache)
  |-- publication (host bundle, dirty overlay, ack/backpressure)
  `-- actor (the only owner of source/current/candidate/outbound state)
```

Only the actor may join those roots and publish. Lower modules return owning
values and receipts; they do not import actor or bridge types.

### Walking-skeleton symbol families

The existing green ATX path establishes these concrete responsibilities:

- `SourceStore`, `CommonMarkLineIndex`, `CropSourceCursor`, the physical-line
  descriptor, `CandidateRecognitionByteSession`, and its source adapter;
- `DirectValueBlockParser`'s command/line transition and
  `FusedAtxLineScanner` donor facts, with no V3 ATX classifier or buffered
  fallback;
- fresh `CandidateSourceLedger`, `SourceBoundProjectionComposer`, and
  `CandidateWriter` operations for open/consume/close/finish;
- `PageArena`, the persistent measured sequence, packed `GreenEvent` codec,
  `SerializedGreenDocument` logical cursor, and exact leaf/program projection;
- `derive_inline_leaf_presentation` plus the bounded Comrak inline request and
  origin join; and
- cancellation/retirement for the actor-owned candidate.

This list describes responsibilities, not a clean extraction boundary.
`DirectValueBlockParser` still wraps `ValueBlockParser`, which owns a
`SourceDocument`, `BlockTree`, and reference state.  The direct path is also
CommonMark-only at that seam.  The semantics extraction is not complete until
the value transition can run without those lifetime representations and can
accept selected GFM control state through the same resumable protocol.

The selected runtime does **not** require `event_tape`, `generic_green`,
`green_tree`, `hierarchical_green_sequence`, the `record_forest`
representation, `storage_only_composite_document`, `projection_reset`, or the
current three-root `Coordinator`.  It does require the semantics proven by
checkpoint, committed-index, retained-Setext, and suffix-adoption modules, but
not their current cyclic proof implementations.  The finite child-fold
aggregate is extracted from `record_forest`; the representation is not.

## Host copied-object publication

The selected host contract further reduces the worker closure. The host never
receives sequence branches or a worker manifest and never queries an obsolete
worker root.

The copied unit is one closed envelope:

```text
ProjectionProgram { id, payload }
GreenLeaf { id, payload, ordered_child_object_ids }
```

An object ID is `(publication_session, arena_slot, generation)`. The host owns
the copied bytes, a measured leaf sequence, and its object store. Worker arena
IDs are only translated at export; arena owner handles never cross the bridge.

`GreenHostSpliceProof { base, target, common_prefix, old_changed,
new_changed, common_suffix }` is minted by the candidate join that already
knows the retained prefix/suffix. Publication must not rescan either document
to discover this proof.

Source edits do not mutate the structural base acknowledged by the host. The
actor maintains a cumulative `DirtyOverlay` and separate current-source,
structural-source, and structural-root clocks. Pending/stale spans remain
source-visible until a structural offer catches up.

Before publication, the candidate must:

1. finish and validate every green/reference/checkpoint root;
2. produce and validate the splice proof;
3. export all inserted leaf/program envelopes;
4. build the complete versioned wire bundle and checksum;
5. reserve the one outbound offer and all required tracking storage; and only
6. perform assignment-only current-root and outbound-offer swaps.

There is at most one unacknowledged structural offer. After copy admission,
the old worker root may enter fuelled retirement because the host no longer
depends on it. Ack advances the host base; nack or publication-session loss
causes a fresh current snapshot, not an obsolete-root query. Inline
presentation is a separately versioned derived stream and never blocks this
structural publication.

The first decisive host test uses two revisions and must prove all of the
following: the changed leaf/program envelopes are sufficient to reconstruct
the target host sequence, the reused suffix object IDs and bytes are identical,
the host performs no worker-root query, stale/base-mismatched bundles are
rejected, and nack/session loss recovers with a current snapshot.

## Dependency cycles to break

1. `candidate_writer -> live_document::DocumentIdentityAllocator` while
   `live_document -> candidate_writer`. Move clocks/allocators into `identity`;
   pass an owning `CandidateContext` into the fresh writer.
2. `exact_block_job -> LiveDocumentStore` while the live document owns the job,
   writer, source, and arena. Make the driver operate on one explicit mutable
   candidate; keep edit admission/publication in the actor.
3. `candidate_writer` imports `committed_checkpoint_index`, while committed
   checkpoint selection imports writer/ledger/green state.  Move the immutable
   checkpoint manifest and query interface above both; neither side may own or
   construct the other.
4. `candidate_writer` imports `storage_only_composite_document`, which in turn
   joins writer, ledger, green, checkpoint, and source roots.  The actor owns
   the composite candidate; the writer receives narrow owning capabilities.
5. `candidate_writer` and `setext_cross_build_restart` import one another's
   identity and normalization types.  Move canonical-fragment origin/rebase
   and normalization recipes into a lower model module; restart selection
   supplies capabilities without importing the writer.
6. Packed green imports child-fold types from `record_forest`, while the forest
   imports arena/green/presentation machinery. Move the approximately 100-line
   finite fold summary into `model` and delete the forest dependency.
7. Source-ledger checkpoint restoration imports serialized-green restart paths.
   Keep source admission independent; a top-level checkpoint join consumes
   opaque source, donor, green, reference, and writer continuations.
8. Presentation currently tends toward absolute mapped facts. Cache semantic
   facts leaf-relatively under root/leaf/content/context/reference identities;
   project to current physical ranges only on a current-root read.

## Exact first extraction sequence

1. Freeze the mechanism receipts, including BOM/indent, tiny/random fuel,
   10 MiB retained Setext, giant-line source completion, every-phase
   cancellation, canonical 1->3 and 3->1 replacement, real Comrak inline facts,
   copied host publication, and fail-closed unsupported cases.  Keep the
   split-Setext gate deliberately red until restart-crossing fragment authority
   exists.
2. Create `flark-engine::model`, `identity`, `storage`, and `source`; extract
   arena, sequence, finite folds, worker source replica, exact line cursors, and
   bounded retirement.  No parser, green node, checkpoint, or bridge message
   may own a second source root.
3. Create `flark-markdown-semantics`.  Extract the direct value transition,
   selected generated scanners, and resumable control state.  Prove it no
   longer constructs or retains `ValueBlockParser`, `SourceDocument`, or
   `BlockTree`.  Put all maintained Comrak seams under one pin, patch inventory,
   body/signature hash manifest, and upgrade rehearsal.
4. Extract packed green plus the fresh ledger/composer/writer/driver pipeline.
   The fixture `# **β😀** ###\r\n` must reach packed green and real Comrak
   without a caller-certified range, aggregate source string, or second
   classifier.
5. Extract one composite checkpoint/restart layer above the fresh pipeline.
   Add parent-authenticated restart-crossing canonical-fragment origin/rebase,
   then turn the split-Setext gate green without forcing identities.  Checkpoint
   samples bind parser, writer, source, green, projection, and fragment state.
6. Add selected GFM Table control and the reference
   occurrence/winner/dependency root to that same candidate manifest.  Prove
   whole/split Table, reference-only/visible-definition normalization,
   duplicate first-wins edits, cancellation, and clean-parse equivalence before
   declaring the grammar-control boundary settled.
7. Add the current-only actor and copied-object publication transaction.  Prove
   two-revision suffix identity, snapshot/delta sequencing, ack, nack/session
   reset, backpressure, cancellation, and fuelled old-root retirement with all
   roots present.
8. Integrate the Dart source session's bounded worker journal and staged fact
   certification with a real native worker and Wasm worker.  Replica ACK,
   grammar certification, and host publication remain three distinct
   protocols; none may manufacture another's acknowledgement.
9. Extract bounded inline materialization and lazy cache as a separately
   versioned presentation service, then wire Dart host adoption and Flutter
   parser-to-paint.  Source remains visible for pending, stale, cancelled, and
   over-cap semantics.
10. Run native/Wasm parity and physical-device latency, frame, backlog, RSS/GC,
    IME, touch-selection, and accessibility gates.  Re-run the closure/LOC/DAG
    audit before broad feature completion.

## Stop conditions

Reopen the architecture before production implementation if any of these
occurs:

- selected non-test maintained Rust reaches 75k without an explicit topology
  review, or would exceed 80k before profile completion;
- the direct semantics transition cannot be separated from `ValueBlockParser`,
  `SourceDocument`, or `BlockTree` without reimplementing a second classifier;
- the maintained Comrak seam grows beyond roughly 2.5k changed/local lines or
  begins spreading through additional semantic files without a new bakeoff;
- fresh pipeline extraction cannot remove cyclic proof/restart imports while
  preserving the production checkpoint semantics;
- host deltas require sequence branches, worker manifests, or obsolete-root
  queries;
- a local edit requires scanning old/new documents to discover the splice;
- source-backed donor support grows into per-construct Flark classifiers or a
  buffered fallback;
- source synchronization or fact certification requires whole-piece strings,
  unbounded main-isolate work, or certification-created worker credit;
- cached inline facts require absolute suffix rebasing; or
- a competent Rust maintainer cannot trace one keystroke from source edit to
  host offer in one working day.

The present evidence strengthens the architecture, but broad production
implementation remains **HOLD** on three composition gates: authenticated
restart-crossing fragments, selected Table control, and the integrated
reference root.  Once those gates pass through one candidate/checkpoint/host
lineage, the first six extraction steps constitute the production architecture
commitment.  Source certification and maintenance rehearsal can proceed in
parallel because they test orthogonal ownership and boundedness claims.
