# Checkpoint C exact-base delta contract

**Status:** Normative exact-base contract, updated 2026-07-28. Producer/host
splice and transactional-lifetime proofs are implemented; public candidate
selection and the remaining presentation gates are still open. It is not
Checkpoint C approval.

## 1. Decision

Checkpoint C uses one typed exact-base transaction with two schema-owned
operations:

1. `ReuseReferences` retains the exact canonical References root from the
   currently installed, host-acknowledged base; and
2. `SpliceSourceFactsV2` replaces a certified page range in the exact base
   persistent SourceFacts sequence and derives the target root.

The host replays the same measured-sequence splice as the producer from its
opaque exact-installed-base capability. It validates only the fresh
replacement pages and fresh changed path, then fresh target role wrappers and
the fresh target manifest refer directly to the resulting canonical roots.
Neither operation retains an old role wrapper or manifest.

This is deliberately not a generic "reuse arbitrary graph node by digest" or
path-selector protocol. A role may skip full transfer and validation only when
its schema defines an exact, independently replayable operation. Future Green
or Projection deltas must add their own proven operation rather than inherit
authority from this transaction.

## 2. Why the References-only slice is not the production delta

The implemented References slice proves useful mechanics:

- a distinct exact-base program;
- an opaque current-installed-base capability and typed base mismatch;
- a virtual ordinal for canonical content omitted from transfer;
- fresh target References and manifest wrappers;
- metadata equality before reused-root validation is skipped;
- atomic install, abort preservation, no old-manifest ancestry, and fuelled
  reclamation; and
- work, bytes, stale-base, abort, and ancestry receipts.

Those mechanics remain. The hard-coded References mode, single virtual ordinal,
and `begin_references_delta` shape do not.

The production parser still projects every SourceFacts page into fresh M1.1
role records. A References-only delta would therefore rescan, allocate,
transfer, hash, and validate work proportional to the large unchanged source.
That fails Checkpoint C even if the omitted References closure itself is
constant-size. The exact-base program must cover both the unchanged References
root and the changed-path SourceFacts root.

## 3. Canonical SourceFacts prerequisite

Checkpoint B's persistent measured SourceFacts sequence becomes the canonical
published SourceFacts role. A candidate retains that authority-free root
directly under a fresh target role wrapper; it does not copy all pages into a
`Vec` of publication records.

SourceFacts certification is not itself exact-base authority. It may install
the active target facts before that target's structural candidate reaches the
host. The reusable base remains the last matching host-acknowledged structural
publication and advances only on structural commit. Both producer and Dart
session enforce this two-phase boundary, so supersession cannot pair a rolled
back root with a merely certified target proof.

Before delta reuse, SourceFacts needs a versioned, compositional strong
commitment. The current sequence branch summary authenticates profile,
checkpoint count, aggregate source facts, leaf count, and height, but has no
strong subtree commitment. An independent host can validate a full imported
closure by traversal, but it cannot soundly skip an unchanged subtree from the
root summary alone.

The SourceFacts v2 role therefore requires:

- canonically encoded authority-free leaf and branch nodes;
- a strong ordered-sequence commitment that is identical for clean and
  incremental construction of the same canonical pages;
- local branch verification from direct child commitments and measures;
- a resumable full-snapshot validator for the cold path; and
- a delta validator that trusts only subtrees already validated under the
  exact installed base, validates every replacement page, and recomputes every
  fresh changed-path commitment.

The commitment design is the first implementation gate. It must preserve the
sequence summary's associative composition; placing an ordinary
non-associative Merkle hash inside the existing semantic `combine` operation is
not valid.

## 4. Move-only witnesses

### 4.1 Persistent SourceFacts delta

`DocumentRuntime` mints one move-only `PersistentSourceFactsDeltaWitness` as
part of the successful incremental scan and splice. It contains:

- runtime identity;
- exact base and target `SourceVersion`;
- parser and SourceFacts profiles;
- exact base page range replaced;
- exact target page range containing the replacement pages;
- the target persistent root identity and commitment;
- changed-page, inspection, allocation, retain, and seal receipts; and
- no source text, old role wrapper, or old manifest owner.

The witness is valid only while the runtime still owns the exact target root.
Using it rechecks the runtime, source, profiles, root identity, and retained
candidate base. Supersession or mismatch consumes it into clean fallback or
typed cancellation; a raw `ArenaId` is never sufficient authority.

### 4.2 Leading-References restart checkpoint

A qualifying clean parse mints one move-only
`LeadingReferencesRestartCheckpoint`, later bound to its delivered producer
publication. It contains:

- exact base source and publication identity;
- syntax profile and grammar revision;
- paragraph content start;
- the byte and UTF-16 end of the complete leading-definition prefix;
- the next physical-line ordinal at that cut;
- the exact reference-definition count; and
- a typed `LeadingReferencesAwaitingRemainder` parser-state tag.

It contains no definition vector, source lease, cooked value, role root,
wrapper, or manifest edge. Once publication finishes, its count and authority
must equal the published References metadata before the endpoint retains it as
an eligible base.

### 4.3 Exact unchanged-prefix authority

For a target attempt, `DocumentRuntime` folds its retained consecutive
`SourceEditLineage` capabilities from the checkpoint source to the exact
target. It must prove in both byte and UTF-16 coordinates that
`0..definition_prefix_end`:

- intersects and crosses no edit;
- maps to the identical absolute range, not merely equal bytes at shifted
  coordinates; and
- ends at the same scalar and physical-line boundary.

This mints a one-use `ExactUnchangedPrefixWitness`. No hash, nearby-text
comparison, or reference-role digest can replace the lineage proof because
canonical reference facts contain absolute source ranges.

### 4.4 Combined transaction lifetime

The endpoint moves the one retained base publication, its acknowledged host
base, the parser checkpoint, the SourceFacts delta witness, and target
certification into one `ExactBaseTailAttempt`.

- Clean fallback or cancellation restores the still-installed base.
- Successful writer construction directly retains the base canonical
  References root and the target persistent SourceFacts root.
- The target delta stream owns the one-use splice description until transfer
  completes or aborts.
- Host acknowledgement installs the target and fuel-closes the old producer
  base.
- Rejection, base mismatch, or abort reclaims only target staging and preserves
  the base on both producer and host.
- Close fuel-drains every active, retained, and staging owner.

The target manifest owns canonical roots directly. The transaction may
temporarily own the base publication as authority, but the target graph never
owns the old wrapper or manifest, so repeated edits cannot form an ancestry
chain.

## 5. Parser crop and target roles

The narrow parser gains a source scanner starting at an exact physical-line
cut and a controller constructor seeded from
`LeadingReferencesAwaitingRemainder`. The seed recreates:

- the target source authority;
- paragraph-open state;
- the armed segmented-reference scanner at the prefix end;
- the already committed definition count; and
- the checkpoint's next byte and line ordinal.

It scans from the prefix end through target EOF. For the Checkpoint C fixture,
this is the small visible tail. It does not scan the definition prefix,
enumerate definitions, or cook reference values.

If the crop accepts any new definition, reaches `Unknown`, or cannot reproduce
an exact Empty/Paragraph terminal state, reuse is not authorized and the
endpoint starts the ordinary clean parse. On success, fresh Green and
Projection records are derived from the target source dimensions, exact
terminal variant, crop-derived visible range, and authenticated reused
definition count. They are never copied from or patched against base Green or
Projection records. CleanEof is fresh target certification.

### Grammar limitation

The current BOF-to-EOF paragraph grammar can soundly prove this focused
fixture: complete leading definitions followed by a small visible tail that,
after replay, remains an admitted Empty/Paragraph result. It cannot soundly
claim the full Checkpoint C editor demo:

- emphasis, strong, and inline code have no authoritative inline Green facts;
- fenced code is explicitly returned as `Unknown`; and
- the current Green/Projection records are document summaries, not the
  parser-to-paint structure required by `flark_flutter`.

Before Checkpoint C review, the same parser authority must add the named inline
slice, fenced-code block support, corresponding persistent Green/Projection
facts and bounded queries, and the real Flutter paint path. A Dart/Flutter
classifier or v2 prediction layer is not an acceptable shortcut.

## 6. Wire and host program

The exact-base Begin frame carries a versioned operation table and binds the
target to the exact base authority. For this transaction the table contains:

- `ReuseReferences` with expected base role metadata; and
- `SpliceSourceFactsV2` with the base page range, replacement page count, and
  expected target SourceFacts commitment and metadata.

Replacement SourceFacts page frames precede ordinary target node frames. The
host:

1. authenticates the opaque current-installed base;
2. resolves the base References and SourceFacts canonical roots without
   traversing their closures;
3. validates and stages each replacement SourceFacts page;
4. resumably applies the measured-sequence splice against the exact base;
5. verifies the derived target measure and strong commitment;
6. assigns virtual ordinals to the reused References root and derived target
   SourceFacts root;
7. imports only fresh target wrappers, Green, Projection, CleanEof, and
   manifest nodes;
8. seals and atomically swaps the target root; and
9. releases the old manifest only after the target owns every shared canonical
   subtree.

Abort releases retained edges, replacement staging, and fresh changed-path
nodes. It never changes the installed base.

The packet layer must make frame-to-node ordinal accounting program-aware:
operation frames are not node records, and fresh node ordinals start after all
virtual roots. Target semantic record counts still describe the complete
manifest; transfer counts describe only material sent or rebuilt.

## 7. Honest work gate

For a fixed small tail edit:

- reference scan, enumeration, cooking, transfer, and validation perform zero
  work per unchanged definition;
- wire bytes contain changed SourceFacts pages plus fixed operation and fresh
  wrapper/role bytes, not unchanged pages or reference records; and
- producer and host SourceFacts mutation work is
  `O(changed pages + measured-sequence height)`.

Literal equal transition counts at every document size are not a truthful gate
for the current balanced sequence: changing a leaf must rebuild its root path.
Checkpoint B already selects changed-path work. Checkpoint C must measure all
producer and host edges, allocations, hashing, sealing, installation, and
reclamation and prove no linear dependence on unchanged reference count. If
exact size-invariant transition counts are required instead, the persistent
tree itself must change to a fixed-depth representation before integration;
the References-only receipt cannot establish that property for the complete
transaction.

## 8. Fallback and failure conditions

Use the clean path when any semantic reuse prerequisite is absent:

- no exact acknowledged retained base;
- source, publication, syntax, grammar, or profile mismatch;
- missing, expired, foreign, crossed, or shifted source lineage;
- an edit intersects the definition prefix;
- SourceFacts base/range/target mismatch;
- a restart cut is not an exact scalar and physical-line boundary;
- reference checkpoint count or metadata mismatch;
- crop acceptance of a new definition;
- crop `Unknown`, blank-boundary, or unsupported-opener result; or
- a target no longer current before writer construction.

Allocation, corruption, commitment mismatch, impossible counters, or a host
derivation mismatch are typed faults, not semantic clean fallbacks. A host
base mismatch preserves the installed base and routes through the existing
supersession/recovery policy rather than overwriting a newer host with a full
snapshot.

## 9. Required receipts

The implementation is not complete until tests record:

- clean-vs-crop terminal, Green, Projection, SourceFacts, References, query,
  and target-manifest equality;
- unchanged producer and host canonical References-root identity under fresh
  wrappers;
- clean-vs-spliced SourceFacts commitment and absolute-coordinate equality;
- no old wrapper/manifest reachability;
- stale/crossed/shifted lineage, wrong publication/profile, metadata mismatch,
  new-definition, unsupported-tail, and host-base-mismatch rejection;
- cancellation at scan, splice, build, transfer, seal, install, and retirement;
- native/Wasm byte, commitment, query, and lifecycle parity; and
- zero residual owners after target/base close.

Scale receipts compare 1, 4,096, and 100,000 definitions and expose at least:

- lineage transitions;
- crop source bytes, physical lines, lexical work, and newly accepted
  definitions;
- reference facts enumerated, cooked bytes, and cooker polls;
- SourceFacts scanned bytes, replacement pages, inspected nodes, retained
  edges, fresh leaves/branches, hashed bytes, and seal transitions;
- candidate retained roots, allocations, and poll transitions;
- wire operation/page/node frames and encoded bytes;
- host base-resolution nodes, validated fresh nodes, retained edges,
  allocations, splice/seal/install transitions, and retirement polls; and
- peak retained bytes and final resident owners.

Reference-specific counters must be exactly count-invariant. SourceFacts and
host counters must fit a named `changed pages + height` envelope and must not
traverse or hash an unchanged subtree.

## 10. Implementation order

1. Add the canonical SourceFacts v2 published role and compositional strong
   commitment; prove clean construction and full independent-host validation.
2. Preserve a move-only SourceFacts splice witness through the production
   incremental runtime and prove producer changed-path bounds.
3. Add host `SpliceSourceFactsV2` replay and prove clean target equality,
   bounded edges/allocations/reclamation, and abort preservation.
4. Generalize the References-only exact-base encoder/host seam to the typed
   operation table and multiple virtual roots.
5. Add the leading-References checkpoint, exact-prefix lineage admission,
   crop parser, fresh Green/Projection derivation, and clean fallback matrix.
6. Join the real endpoint, native/Web wire, host query, and lifecycle path.
7. Add the authoritative inline/fence/query/render slice, then run Checkpoint C
   performance and product review.
