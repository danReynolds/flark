# Leaf normalization group gate

Status: **normalization transaction model selected; Setext and authenticated
Table-cursor mechanisms GO; reference/Table writer composition and the final
multi-root manifest HOLD**, 2026-07-18.

The original standalone model remains rejected, but the architectural question
it posed has since been answered on the real v3 substrate. The selected unit is
the actor-owned active-Paragraph normalization transaction: provisional green,
source/projection, parser continuation, stable parent bindings, semantic side
roots, and candidate publication either advance together or abort together.
It is not required to survive as a public type named `LeafNormalizationGroup`;
the invariant and ownership boundary are what are selected.

Setext now proves the hard spanning-restart case on the real source, writer,
packed-green, lineage, and arena journal: its focused 44/44 lane includes the
10 MiB parent-bound suffix splice, exact clean-parse equality and stable whole
identity, nested ownership, stale/crossed authority, cancellation, and the
general parent-boundary rule for Open, Close, and Finish. The authenticated
Table projection cursor is separately 4/4 green without a Paragraph snapshot
or second projection tree. The remaining falsifier is not whether the
transaction unit is coherent; it is whether reference prefix removal/re-winner
publication and Table promotion can join the same real candidate writer and
final manifest without weakening that boundary.

## Rejected proof instrument

The representation-neutral `leaf_normalization_group_gate` crate is not
decision evidence. It allows callers to supply future retained suffixes at a
checkpoint, accepts arbitrary source runs and arenas as certified partitions,
asserts compatible prefixes instead of deriving them, does not enforce the
latest generation, and has no real candidate cancellation, publication, or
index atomicity. Its 10 MiB witness is one synthetic length leaf rather than a
restart through real source and packed green. Repairing it far enough to become
honest would duplicate the v3 mechanisms it is supposed to validate.

Only its finite outcome/identity truth table is retained as a specification.
Its fake run arena, capability seals, tail catalog, generation registry, and
copy counters must not be cited as proof or promoted into production. The next
gate is implemented directly in `v3_runtime_slice`.

## Decision

The pinned CommonMark 0.31.2 plus selected GFM block profile has one useful
locality property: every retroactive block transition targets the currently
open leaf transaction. No selected transition reaches into an unrelated,
previously closed sibling.

That does **not** make one open-leaf binding sufficient. One provisional
Paragraph can normalize to:

- one Paragraph with the same stable identity;
- one Setext Heading with the same stable identity;
- no semantic wrapper after reference-only finalization;
- a closed preface Paragraph plus a newly open Table; or
- a newly open Table with the Paragraph identity retired.

The working semantic unit is therefore an opaque `LeafNormalizationGroup`. It owns the
provisional lineage and the exact structural, source/projection, fact,
checkpoint, and reference transaction needed to normalize those exhaustive
outcomes. It is not a semantic block and promises no single final `BlockId`.

Fresh parsing and restart from immutable retained output use the same semantic
operation:

```text
begin/restore one LeafNormalizationGroup
  -> stream exact source decisions and persistent run capabilities
  -> suspend or converge while unresolved
  -> consume one certified Paragraph outcome
  -> atomically install canonical balanced green plus side indexes
```

Storage may internally append a fresh group or path-copy a retained base range.
That distinction stays below one typed authority. The parser never receives a
root, arena ID, token rank, arbitrary split range, or generic mutation API.

Architecturally this is a persistent form of an ordinary parser reduction:
Paragraph control is the unresolved nonterminal; the decisive line reduces it
to the canonical small forest. The only unusual requirement is retaining
enough sealed reduction provenance to resume *before* that decision from an
already-finalized immutable document. It is not a second AST or a repair layer.

## Rejected simpler models

### Whole-terminal deferred fragment

This is elegant during a clean parse, but restart may begin inside a 10 MiB
Paragraph whose canonical `Enter` and earlier prefix already live in the
retained base. Deferral then either reparses from the opener, needs the exact
retained-range transaction anyway, or requires a different hierarchical
representation. A fresh-only fragment plus retained rewrite would be two
mechanisms for one transition.

### Retained `PromoteSetext` event

An append-only promotion makes checkpoint chronology look convenient, but
leaves mutation history in final green and makes source-first queries before
the underline depend on future overlay interpretation. Table promotion then
requires a much richer event-sourced split/reparent system. Final green stays
canonical instead: Paragraph control state and finalized semantic kind have
different lifetimes and are explicitly separate.

### One generic late-bound `Enter`

A late kind can express Paragraph versus Heading only. It cannot express zero
wrappers, two siblings, nested Row/Cell fan-out, parent-owned definition
source, or a returned open Table continuation.

## Opaque provenance

Every restart sample inside an unresolved Paragraph retains one scalar group
reference, not a copied prefix or a semantic-kind guess:

```text
ParagraphCheckpoint
  group
  exact source boundary
  Paragraph control snapshot
  captured open-parent path
```

When a revision seals a group that has at least one persisted interior restart
sample, storage creates one private manifest shared by all of its samples:

```text
SealedNormalizationManifest
  group identity
  exact source extent and lineage
  final typed outcome
  sampled provisional semantic-prefix recipe
  structural footprint (possibly empty or multi-block)
  source/projection footprint and certified interior partitions
  fact and aggregate footprint
  restart/checkpoint footprint
  reference effects
  outcome-specific identity manifest
```

Small groups with no sampled interior checkpoint may elide it and restart from
their preceding closed boundary. Sampling is page/work-budget based, not one
record per physical line. Once an interior sample exists, the manifest is
required even though it is not ordinary semantic output. A
canonical reference-only result has no Paragraph wrapper to rediscover. A
canonical split table has two siblings and a body subtree. Reconstructing the
old provisional Paragraph by heuristically inverting final green is forbidden.

The manifest is generation-bound and storage-private. A resumed parser gets a
non-cloneable `ResumeParagraphLease`; stale nested capabilities fail by
generation when an outer normalization is replaced.

The provisional recipe is not inferred from the final logical projection.
Reference-only source is final `Gap/None`, and a table header may have final
cell trim/unescape programs even though the pre-delimiter Paragraph state was
raw Inline input. The recipe therefore retains source-backed run expressions
and bounded scanner/checkpoint values that share exact source pieces; it never
copies an aggregate Paragraph string or mirrors final green pages as a second
AST.

## Exhaustive Paragraph outcomes

`Continue` suspends the unresolved group; it is not a canonical final outcome.
The finalizer accepts only scanner-certified variants:

| Outcome | Identity and structure | Source/projection result | Returned state |
| --- | --- | --- | --- |
| ordinary Paragraph | Primary Paragraph ID survives | persistent visible runs; final terminator is `Terminal/None` | closed Paragraph |
| visible Setext | same primary ID becomes typed Heading | definitions become parent `Gap/None`; visible runs survive; underline is Heading `BlockMarker/None`; ending is `Terminal/None` | closed/open Heading as chronology requires |
| definitions-only Setext continuation | Paragraph ID survives and remains open | definitions become parent `Gap/None`; underline is Paragraph `Content/Identity`; its terminator remains pending; never retry as thematic break | same Paragraph transaction |
| reference-only | primary ID retires; zero wrappers | every definition range remains exactly covered as parent `Gap/None`; ancestor markers keep their owners | captured parent path |
| visible remainder | primary Paragraph ID survives | definition prefix becomes parent `Gap/None`; surviving runs are rebased Paragraph content | closed Paragraph |
| whole Table | Paragraph ID retires; mint Table/HeaderRow/Cells | certified header cells become typed projection programs; delimiter is Table marker; no fresh reference finalization | open Table transaction |
| split preface + Table | primary ID survives on preface; mint Table/HeaderRow/Cells | preface and header partitions use certified source cuts; delimiter is Table marker | closed preface plus open Table transaction |

Table activation precedes ordinary Paragraph reference finalization in the
pinned donor. A reference-looking split preface therefore remains visible and
does not produce fresh definition occurrences. Reconciliation may still retire
stale reference facts from the replaced base group.

One contiguous structural replacement envelope is sufficient for the selected
profile. One contiguous source range is not: quote/list markers interleave,
reference runs change owner, and tables need preface/header/cell partitions.
The group carries scanner-certified persistent run capabilities for those
interior pieces rather than caller-provided offsets.

## State machine

```text
Open
  -> LineInFlight
  -> Open                         accepted continuation
  -> Normalizing
       -> Sealed                  Paragraph/Heading/removed
       -> HandedToTable           Table continuation
  -> Poisoned                     any authority or allocation failure
```

The transaction retains only values whose next transition or final output
needs them:

- provisional group start and exact current source cursor;
- Paragraph control, visible-content state, and lazy-container outcome;
- captured quote/list/item path and parent insertion capability;
- pending terminator/gap ownership;
- reference-prefix scanner state and pending reference delta;
- exact last-line capability and bounded table recognizer state;
- persistent source/projection run expressions, not aggregate text; and
- candidate revision/build/generation authority.

The writer consumes a typed `ParagraphLineDecision` and performs all source
claiming. Recognition buffers never become output authority.

## Convergence inside a large open group

This is the decisive incremental requirement. A current parse may converge at
a line boundary inside the provisional Paragraph. The leaf-specific recipe
must atomically compose:

```text
current exact changed prefix
+ retained old provisional continuation
+ old typed normalization outcome
```

The recipe proves control equality, mapped unchanged source lineage, stable
parent path, persistent source/projection suffix capabilities, compatible
reference state, and the sealed old group manifest. It cannot simply adopt the
old canonical Heading/Table because the new prefix may differ.

An edit in the middle of a 10 MiB Paragraph must converge without work
proportional to distance from the Paragraph opener or eventual closer. Failure
of that witness rejects the architecture even if clean parsing is fast.

## Canonical output and liveness

Committed packed green contains only final balanced structure and exact
source/projection runs. It retains neither parser events nor normalization
history. Restart control remains provisional Paragraph state even when the old
committed group finalized as a Heading or Table.

While a group is unresolved in the new candidate, that group is an `Unknown`
reuse barrier. Old semantics are not relabeled as current. Flutter still
commits source and selection immediately and source-paints the affected range;
the grammar-free open overlay may expose exact current ancestry/identity but
cannot classify Markdown. Normalization or typed retained-tail convergence
must resolve the barrier before candidate publication.

This keeps liveness independent from parse completion without introducing a
Dart prediction parser.

## Atomic scope

One normalization commit owns all of:

- canonical packed structural green;
- total source ownership and logical projection;
- stable identities, facts, and child aggregates;
- the sealed group and restart manifests;
- reference occurrences, winner changes, and consumer invalidations;
- the parser/writer open-stack transition; and
- the revision-bound presentation delta or its explicit invalidation.

Reference consumers live in a separate revisioned symbol/consumer index. If
resolved destinations are eagerly copied into closed inline nodes, this local
transaction is insufficient and the architecture must change.

## Profile boundary

The active-group locality result applies to the pinned CommonMark plus selected
GFM profile. Extensions that reach outside the active group require separate
typed transactions or remain unsupported. Known counterexamples include
description lists reparenting an already-closed preceding Paragraph and
footnote postprocessing moving definitions to the root. Front matter needs a
document-prefix recognizer. Alerts require their own conformance/fact gate.

## Executable reject gates

1. Valid -> invalid -> valid Setext from a checkpoint immediately before the
   underline preserves the primary ID and never exposes stale Heading truth.
2. Definitions-only and visible-definition Setext exactly match donor source,
   reference, projection, and block results.
3. Restore a canonical reference-only group, edit the definition into visible
   text, and create exactly one Paragraph without reconstructing source.
4. Invalidate a split-table delimiter before a very large body; retire the
   whole old Table footprint with no orphan Row/Cell identities or generic
   range search.
5. Distinguish whole-table and split-preface identity policies.
6. Edit inside a 10 MiB Paragraph and converge before its close with bounded
   changed-page work and exact distant suffix identity.
7. Preserve nested quote/list marker ownership and total byte/UTF-16 coverage
   through every outcome.
8. Atomically update an earlier reference consumer when the group changes the
   winning definition.
9. Cancel or inject failure at every normalization allocation, index, manifest,
   and attachment phase; publish nothing and reclaim every candidate owner.
10. Keep one scalar group reference per sampled checkpoint and one sealed group
    manifest, with no aggregate source string or per-checkpoint fragment copy.

Invalidating a delimiter can genuinely change the grammar of the entire old
table body. The architecture does not promise constant total semantic work for
a nonlocal semantic edit. It promises constant-time candidate invalidation,
fuelled off-main-thread reparsing/reclamation, immediate source-first UI, and
convergence as soon as exact state permits. The storage normalization itself
may not synchronously enumerate the old body merely to detach it.

## Current receipt and next proof

The five-step Setext vertical slice previously requested here is complete.
Resumable persistent-sequence split/retain/splice, sparse exact barriers,
fresh and retained writer normalization, parser/ledger/composer/binding/green
resume, and the checkpointed 10 MiB convergence case now run through the real
arena journal. The focused suite is 44/44 green and preserves exact distant
page identity, byte/UTF-16 coverage, clean-parse equality, and the primary
Heading identity under cancellation and crossed authority.

The Table cursor proof also validates the same transaction shape for the
multi-block outcome: one non-cloneable actor session joins the builder leaf,
composer high-water, Crop cursor, provisional Paragraph, delimiter ownership,
grammar configuration, and writer epoch. It validates then replays the exact
projection with no cloneable green snapshot; the remaining work is the real
parser/writer attachment, not a new transaction family.

The smallest honest remaining vertical slice is reference finalization:

1. finish the persistent restart/re-winner index using the checkpoint's
   authenticated per-label prefix ranks, with no unchanged-suffix enumeration;
2. expose one build-local, non-cloneable logical cursor over the force-sealed
   active Paragraph in the existing packed-green prefix, rather than retaining
   a Paragraph `String` or second projection tree;
3. change `ExactBlockJob`'s `ExternalWorkReady` path into an actor-owned
   reference-finalizer state and mint `DirectReferencePrefixWork` from that
   cursor identity;
4. resolve each parser-authenticated logical definition range back through a
   transaction-local random-access cursor over that same projection; run the
   destination trim probe where required, stream the selected destination and
   title bodies through the pinned Comrak-correspondent cleaner into persistent
   cooked byte blobs, stage the occurrence/directory roots, and return the
   one-shot item acknowledgement only after storage accepts all of them;
5. apply `NoDefinitions`, `VisibleRemainder`, or `ReferenceOnly` as one
   canonical active-Paragraph replacement, then join the terminal token back
   into the interrupted parser transition; and
6. publish source, green, reference occurrence/directory/cooked-value,
   checkpoint, interner, lineage/adoption, and host roots in one candidate
   manifest—or abort them all. The transaction-local projection cursor and its
   old-root lease must retire before publication escapes.

That slice must prove zero/one/many definitions, duplicate re-winner behavior,
giant streamed destinations/titles, nested projection ownership,
reference-only zero-wrapper and visible-remainder identity policy, Setext
chronology, every-phase cancellation/fault, crossed/stale capabilities, exact
clean-parse equality, and bounded work/memory on a large unchanged suffix. A
failure which requires Paragraph materialization, a second classifier, or an
independent projection structure reopens this transaction model. Until such a
failure appears, the evidence supports one coherent active-Paragraph
transaction rather than a stack of feature-specific repairs.
