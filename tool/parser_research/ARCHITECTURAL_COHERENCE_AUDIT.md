# Architectural coherence audit

Status: **selected architecture GO; production composition and launch HOLD**,
2026-07-18.

This audit asks a different question from the feasibility spikes: if every
benchmark passes, does the resulting system still have one understandable
model, or is it a collection of local exceptions?  A shipping decision needs
both correctness evidence and a coherent answer to that question.

## Short verdict

The converged **system architecture is clean**. It has one exact source, one
Markdown authority, one candidate transaction, and explicit derived
lifetimes. Its main ideas are established editor/compiler/database patterns
rather than Flark-specific repair tricks:

- a persistent source rope with exact edit lineage;
- a Flark-owned, donor-correspondent block controller with bounded restartable
  continuation and an immutable packed-green root;
- MVCC-like revision roots and atomic derived-index adoption;
- actor/worker ownership with latest-wins cancellation;
- lazy, revision-scoped presentation queries that fail closed to source; and
- a bounded foreground input island whose edits do not copy the whole
  document while exact parsing remains worker-owned.

The physical direction is also selected: one source-ordered serialized green
stream in persistent balanced pages. Typed events are bounded build/query
scratch, not a second retained tree. The Flark controller is derived from and
differentially checked against Comrak/cmark-gfm algorithms, but Comrak's arena
and batch parser are not runtime editor state. This eliminates both the old
Dart prediction parser and the maintenance burden of making a private Comrak
checkpoint fork the product architecture.

The selection is now backed by composed mechanisms rather than representation
sketches:

- the exact donor-correspondent controller and writer substrate pass the
  current combined Rust lane with 392 library tests, every integration target,
  and three compile-fail doctests when the one explicitly open reference
  finalizer publication gate is skipped;
- Setext's parent-selected structural transaction is 44/44 green under the
  general rule that canonical identity is resolved before any structural
  action crosses the parent;
- Table validation/replay is 4/4 green with an authenticated projection cursor
  and no Paragraph snapshot or second projection tree;
- the source/projection composer is 22/22 green with a 224-byte, zero-payload
  line-boundary continuation; and
- the foreground Flutter mechanism is 66/66 green with clean analysis,
  including bounded input-island edits at a 10 MB host-model scale.

Those receipts select the architecture; they do **not** make it production
ready. The reference restart/re-winner index is still under active proof, and
the parser-owned reference finalizer plus the Table cursor still need their
real candidate-writer joins and atomic multi-root publication. Native/Wasm,
physical-device frame/IME/accessibility, mutation/fuzz, and no-skip release
matrices also remain launch gates.

The current `OutputAccumulatorCheckpoint` is likewise a proof witness, not the
selected seam: it still carries a complete `SemanticFrame` and growing proof
`String`, then re-derives the grammar projection during reconstruction.  The
production value must be genuinely disjoint typed output state over persistent
source runs.  Keeping the current duplicated frame because its tests pass
would be exactly the kind of hidden coupling this audit rejects.

That rejection has an executable authority-partition witness in
`restart_composer_gate`: `ControlContinuation`, `SemanticPrefixState`, stable
bindings, source and scheduler cursors are disjoint; control matching returns
an opaque witness with no attachment API; and only the top-level composer can
issue an adoption permit after lineage, both revisions and mapped boundaries,
suffix identity, exact semantic-root identity/generation, path shape, and a
typed list/table/setext/reference/raw recipe validate. Every action remains
paired with its stable capability and open depth. The representation-neutral
API requires a matching outer-to-inner capability path before the consuming
permit yields an action. Its state partition remains selected, but its
Paragraph action vocabulary is now known to be incomplete: the real grammar
can yield zero wrappers, one Paragraph/Heading, or a preface plus open Table
and can update a global reference index. These outcomes are specified by
[`LEAF_NORMALIZATION_GROUP_GATE.md`](LEAF_NORMALIZATION_GROUP_GATE.md).

The old `composed_adoption_storage_gate` is retained only as a historical
immutable-base property-rewrite witness. It no longer compiles against the
opaque manifest identity, and even its original successful surface proved only
an already-located same-root Enter rewrite. It echoed a caller-supplied storage
stamp, omitted arena identity, accepted a caller-assembled path, discarded the
Setext content recipe, attached no suffix, and retained the base source
revision after consuming the one-use permit. It must not be repaired into the
production endpoint piecemeal.

The older adoption witnesses below remain useful provenance, but they no longer
control the architecture verdict. Current gate state is owned by
[`ARCHITECTURE_PROOF_LEDGER.md`](ARCHITECTURE_PROOF_LEDGER.md), and the proposed
production composition is owned by
[`../../docs/architecture/rfc/rfc_023_incremental_live_markdown_engine.md`](../../docs/architecture/rfc/rfc_023_incremental_live_markdown_engine.md).
Any older statement in this audit that calls parser selection or the generic
serialized representation provisional is superseded by this 2026-07-18
update. Remaining HOLDs are concrete integration and launch proofs, not an
unresolved choice among competing architecture families.

## One dependency direction

```text
source revision + edit lineage
            |
            v
exact block continuation -----> transient write-only mutations
            |                              |
            |                              v
            +----------------> one persistent semantic transaction
                                           |
                         +-----------------+------------------+
                         |                                    |
                         v                                    v
              exact inline/reference query          structural/source query
                         |                                    |
                         +-----------------+------------------+
                                           v
                              revision-bound presentation
                                           |
                                           v
                          Flutter source/input/layout leases
```

Every arrow points away from source and grammar authority.  The semantic tree,
indexes, presentation facts, and mounted Flutter hosts may be absent or stale,
but none may feed Markdown decisions back into the parser.  That one-way rule
is the strongest evidence that this is not the old dual-parser design in a new
shape.

## Why the apparently multiple structures are not dual parsing

Multiple lifetimes are legitimate only when they answer different queries and
remain deterministic derivatives of one grammar result:

| State | Owns | Must not own |
| --- | --- | --- |
| `ControlContinuation` | exact future block-control state | semantic-prefix accumulators, rendering, source history |
| stable open bindings | identity/path capabilities for reused ancestors | Markdown classification |
| semantic prefix state | parser-owned composable runs, folds, descriptors, and finalizer cursors | published-tree lookup or independent control authority |
| semantic root | immutable structural and source-ownership facts | parser scratch or UI leases |
| presentation snapshot | complete requested inline/target/capability facts | partial or cross-revision truth |
| Flutter host | input, focus, element, and layout continuity | semantic authority |

`Unknown` is therefore not a prediction result or a second parse.  It means
that the current source is authoritative while an exact derived fact is not
yet certified.  It is clean only if ordinary active facts are measured to
arrive within the live-edit deadline; it cannot be used to excuse persistent
semantic lag.

## The incrementality seam

A complete paused parser value is deliberately not a convergence key. The
restart value is split into:

1. future-observable control continuation;
2. stable open semantic bindings;
3. separately composable, parser-owned semantic-prefix state;
4. a current source cursor; and
5. scheduler progress, which never authorizes reuse.

This avoids two opposite hacks:

- comparing a growing paragraph, raw literal, or child history merely because
  it appears in parser scratch; and
- replacing exact state with a hash, byte-equality guess, or Dart classifier.

Suffix reuse is earned only by operation-derived source alignment, exact typed
control equality, valid stable bindings, and a typed semantic-prefix adoption
proof owned by the top-level composer.
Changed list tightness or paragraph projection can update locally without
pretending that it changes future block grammar.  Unresolved table/setext
promotion cannot be skipped because it really does change grammar.

## Representation cleanliness tests

A representation is clean only if it satisfies all of these without an
exception table per Markdown feature:

1. one generic encoding represents heterogeneous leaves and containers;
2. source byte/UTF-16 lookup returns the exact owner and enclosing path,
   including gaps and continuation-line ancestor markers;
3. viewport iteration begins from source and remains bounded;
4. insertion, deletion, promotion, detach, and contiguous reparent are the
   same small set of sequence/tree operations;
5. changed child aggregates propagate along the open ancestry without scanning
   siblings or descendants;
6. exact distant suffix block/page identity survives a prefix edit;
7. no absolute ordinal is rewritten and no old source root is retained;
8. cancellation and allocation failure roll back one top-level transaction;
9. total retained and temporary memory includes directories, edge tables,
   arena slots, manifests, and builder journals; and
10. a new syntax feature normally adds typed facts and transition logic, not a
    new storage/indexing mechanism.

The tenth test is the architectural one.  A specialized Item-to-Paragraph
microtree, table-only side index, or per-feature repair pass can be useful as a
measurement witness, but it is not a selected architecture.

## Representation decision

### Normalized flat forest — retired mechanism witness

Architecturally conservative, but it coordinates record order, source
coverage, parent bindings, and child aggregates as distinct structures. Keep
its measurements and transaction mechanisms as evidence; do not carry it into
the next implementation lane without a concrete serialized-composition
failure.

### Packed Euler / serialized green stream — selected direction

The smallest coherent tested model because semantic Enter/Exit tokens and
coalesced source-coverage runs share one source-ordered persistent sequence.
Balanced-parentheses summaries can select direct children and skip whole
subtrees. Its production implementation remains on hold until packed-only
shared-arena pages, complete typed facts/source identities, streaming queries,
and base-root batch mutation compose without a directory, absolute rank, or
cursor-repair protocol.

### Hierarchical packed green tree — rejected representation

The shared packed-edge arena mechanics pass and remain reusable. The composed
tree representation does not: it cannot interleave ancestor-owned syntax with
an open descendant Paragraph, does not provide globally exact nested ranges or
nested viewport traversal, and its large receipts exercised flat entries. A
repair would add a mixed source/semantic local stream and converge toward the
selected serialized model.

## Things explicitly rejected as hacks

- keeping the Dart prediction parser for deadline misses;
- reparsing an guessed enclosing substring and treating it as document truth;
- retaining a Comrak arena/event history as the persistent editor model;
- content hashes or equal suffix bytes authorizing semantic identity;
- a complete old pause checkpoint restored after the new output prefix changed;
- absolute offsets/ranks embedded in reused suffix facts;
- committed-tree reads from the grammar to recover missing parser state;
- source-position or list-property repair walks after adoption;
- feature-specific storage paths that bypass the shared persistent sequence and
  ownership transaction; and
- UI element identity preserving stale semantic authority.

## Comrak boundary

Comrak is a semantic donor, differential oracle, and source of selected block
and GFM algorithms.  Its arena lifetime and public batch API are not the Flark
runtime architecture.  The current value-state extraction demonstrates that a
single Flark-owned transition spine can remain correspondent to Comrak without
shipping the experimental arena-checkpoint fork as the editor's state model.

This is cleaner than depending indefinitely on a private upstream patch, but it
creates a real maintenance obligation: record algorithm provenance, keep
unmodified Comrak/cmark-gfm differential lanes, isolate upstream intake, and
never let a compatibility adapter become a second grammar authority.

## Honest decision boundary

The following is selected and may guide production implementation now:

- source, grammar, worker, revision, authority, and source-first fallback
  boundaries;
- one Flark-owned donor-correspondent controller shared by unlimited and
  fuelled execution;
- the split among control continuation, semantic-prefix state, published
  immutable roots, and presentation state;
- a source-ordered serialized green stream in persistent balanced pages;
- exact checkpoint/state/binding/lineage convergence with suffix identity
  reuse;
- one actor-owned candidate journal and atomic multi-root publication;
- typed parser external-work rendezvous for reference and Table algorithms;
  and
- a bounded Flutter input island with exact work off the foreground isolate.

The following remains deliberately unfrozen until its named proof closes:

- the production reference occurrence directory/checkpoint details, pending
  the restart/re-winner falsification gate;
- the private API shape of the shared authenticated Paragraph projection
  cursor, pending both real reference and Table writer joins;
- the final block-ID lookup/cache policy, which is a query optimization rather
  than grammar authority; and
- device-specific scheduling constants, checkpoint cadence, and cache budgets,
  which must be selected from AOT/web/GC/frame measurements.

The architecture verdict is therefore **GO to implement, not GO to launch**.
Launch still requires the full no-skip normative and mutation/fuzz suites,
native/Wasm parity, sustained large-document memory/cancellation receipts, and
physical-device liveness/IME/accessibility evidence. A failure in the
reference restart or real projection-cursor joins reopens the affected
mechanism; it does not license a prediction parser, substring parse, or second
Markdown authority as a shortcut.
