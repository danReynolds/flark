# Source-lineage projection reset falsifier

Status: executable feasibility result, 2026-07-16.

## Decision

The real `SourceStore` lineage probe strengthens the flat
`SourceProjectionRun` design. It does not justify promoting each large
projection `Program` to a second persistent mini-sequence.

The earlier maximum-fill falsifier remains valid: a projection envelope with no
stable reset can cascade to EOF after a one-byte edit. The new result shows that
source-proven resets plus bounded local split/merge stop that cascade under the
requested adversarial edits.

This is an algorithm/identity-locality gate, not yet the source-bound production
composer or a CPU-locality benchmark. The probe deliberately materializes a
clean comparison layout and maps every old reset and page to measure the exact
upper bound on reuse. Production must do neither global operation.

## Executable model

`v3_runtime_slice/tests/projection_source_resets.rs` uses the actual
`SourceStore`, accepted edit records, fuelled `LineageMapJob`, and consumed
`LineageMappingProof` values. It never simulates an offset delta.

The comparison layout uses:

- 4 KiB minimum, 8 KiB target, and 16 KiB hard-maximum physical source groups;
- the existing 4 KiB projection codec page;
- history-mapped internal reset identities;
- local underflow merge and overflow split with hysteresis;
- scalar-aligned resets that also refuse the interior of CRLF, an indivisible
  typed atomic transform; and
- `BoundaryAffinity::After` for newly minted internal resets.

Each group streams source-derived Identity, NUL, tab, CRLF, and lone-CR pieces
through the real `ProjectionProgramChunker`. Page eligibility requires an
actual source-lineage range proof, the exact mapped range, equal physical
byte/UTF-16 metrics, and equal logical contribution bytes.

## Receipts

| Probe | Result |
| --- | --- |
| 24 repeated 1 KiB prefix inserts into a 64 KiB NUL-dense document | resets 6 to 8; 2 locally minted; worst 2 changed groups and 19 changed new pages; largest group 16 KiB / 17 pages; minimum mapped-suffix page reuse 78.5% on this short fixture |
| 1 KiB insert exactly at an internal reset, `Before` | reset remains before inserted bytes; 1 changed group, 10 changed pages, 74.3% mapped-suffix page reuse |
| same insert, `After` | reset moves after inserted bytes; 1 changed group, 2 changed pages, 100% mapped-suffix page reuse |
| 33,768-byte deletion across four reset anchors | four intersected anchors rejected by lineage; 1 changed group, 8 changed new pages; 35/42 mapped suffix pages reusable; largest group 16 KiB / 17 pages |
| 40,960-byte NUL/tab/CRLF insertion | four replacement resets minted; 5 changed groups and 36 changed pages, proportional to inserted data; 89.4% mapped-suffix page reuse; largest group 16 KiB / 12 pages |
| prefix underflow followed by 20 KiB growth | one reset removed on underflow; later growth minted two resets; growth changed 3 groups / 34 pages; hard group bound retained |
| mapped reset made illegal by forming CRLF across it | reset discarded; change stayed within at most two groups / 20 pages |
| Unicode source with byte length unequal to UTF-16 length | exact total metrics retained; 1 changed group / 4 pages; 90.0% mapped-suffix page reuse |

The suffix percentages are fixture-size dependent. The architectural receipt is
the absolute changed-group/page bound. A longer document increases the suffix
percentage without increasing the local bound.

## What the production design should be

Treat reset grouping like bounded B-tree leaf rebalancing, not content-defined
chunking and not a global absolute-offset index:

1. A source-bound composer mints a reset only after consuming exact source and
   certifying that the cut is a UTF-8 boundary, does not split an Atomic packet,
   and has no unresolved Virtual/terminal projection state.
2. Flat `SourceProjectionRun` records on each side live in the existing
   persistent serialized-green sequence. Reset identity is an output edge
   capability, not a public `(revision, offset, affinity)` tuple.
3. An edit rebuilds the enclosing bounded group. Overflow splits and underflow
   merges affect only adjacent groups. The unchanged suffix runs and sequence
   subtrees attach by identity after the ordinary restart/convergence proof.
4. Purely internal resets use `After` affinity by default. The exact-reset probe
   shows why: it assigns inserted bytes to the left changed group and leaves the
   right suffix at its old reset, reducing 10 changed pages to 2 in this case.
5. The hard gate should cover physical bytes and emitted Program pages/pieces,
   not bytes alone, so a future transform profile cannot inflate one group with
   unbounded zero-physical or metadata pieces.

The probe's whole-layout rebuild, split search, and per-reset/per-page lineage
jobs are measurement oracles only. A production implementation that performs
any of them globally after every edit would fail the architecture even though
these identity receipts look good. Production should stream clean resets once,
map the bounded restart/convergence/tail capabilities in one lineage bundle,
splice the changed flat runs, and retain the suffix persistent subtree without
visiting it.

## Remaining falsifiers before commitment

- Integrate a non-forgeable source/projection reset capability with the
  candidate writer. The current public scalar APIs do not prove that a reset
  came from consumed source or a parser-valid projection state.
- Exercise a pending Virtual exactly at a proposed reset. A source boundary
  alone cannot decide right-interior versus left-EOF ownership.
- Prove the persistent sequence splice preserves every complete suffix
  `CoverageId`/`ArenaId`, with only boundary leaves repacked.
- Count end-to-end work and peak memory. The reset planner may inspect only the
  changed group plus persistent-tree depth; it may not scan this test's global
  reset/page vectors.
- Add schedule, cancellation, history-expiry, and multiple-edits-while-building
  cases through the live-document actor.

Promote `Program` to a persistent mini-sequence only if one of those gates shows
that parser-certified resets cannot be supplied densely, flat-run fanout is too
large, or unchanged suffix run identity cannot survive the real splice. None of
the source-lineage/edit-locality evidence currently requires that added root,
edge, traversal, adoption, and retirement surface.
