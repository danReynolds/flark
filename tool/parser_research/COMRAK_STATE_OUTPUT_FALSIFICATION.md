# Comrak state/output falsification audit

Status: temporary-fork evidence, 2026-07-14. The probes ran against the
production-shape Comrak 0.54 extraction in a disposable checkout. They test
whether RFC 023 can be reached through a narrow adapter; they are not claims
that stock Comrak is an incorrect batch Markdown parser.

## Outcome

The public handle's ordinary top-level fast path is real, but the narrow-fork
hypothesis fails. Meeting the oversized-container, giant-inline,
bounded-resumption, and compact-delta contract requires replacing arena-backed
block and inline ownership with persistent source-backed values. That result is
best described as a Comrak-derived Flark parser core, not a surgical fork.

The current decision is therefore to use Comrak as the primary grammar and
algorithm donor for a Flark-owned persistent core, while keeping an unmodified
pinned Comrak path as its clean-parse oracle. The ownership boundary and gates
are described in
[`PARSER_AUTHORITY_REASSESSMENT.md`](PARSER_AUTHORITY_REASSESSMENT.md).

## Exact checkpoint state

The cloned open AST spine loses closed-prefix semantics:

```text
checkpoint_prefix_gap full_tight=false resumed_tight=true open_depth=4
```

The existing continuation hash is also identical after an edit that changes
the final list from loose to tight:

```text
checkpoint_false_convergence state=8082866648161049234 old_tight=false new_tight=true
```

These particular failures are repairable with a persistent list-prefix
aggregate. The architectural consequence is that every closed-prefix semantic
dependency must become native exact continuation/convergence state; hashing the
open arena spine is insufficient.

## Checkpoint size

A 25,000-deep quote checkpoint clones 25,002 `Ast` values:

```text
checkpoint_depth_cost depth=25002 ast_bytes=3400272 source_bytes=50008
```

A midpoint checkpoint in an open multiline paragraph retains the entire source
prefix and its line offsets:

```text
checkpoint_paragraph_cost retained_content=510000 retained_line_offsets=10000 checkpoint_offset=510000
```

The fix is not another hash. Checkpoints need compact immutable container
frames, source-backed leaf cursors, external line indexes, and structural
sharing across revisions.

## Public-handle delta shape

`cargo run --release --example public_handle_shapes` measured one-word edits:

| Shape | Apply | Reparsed bytes | Estimated delta bytes |
| --- | ---: | ---: | ---: |
| Many ordinary blocks | 65 us | 13 | 102 |
| 1 MB fence | 6.43 ms | 1,000,024 | 1,040,230 |
| 1 MB list | 96.97 ms | 1,000,012 | 14,141,795 |
| 1 MB table | 70.86 ms | 1,000,017 | 3,553,017 |
| 1 MB paragraph | 0.17 ms | 1,000,008 | 1,000,101 |

The paragraph time excludes inline parsing. The delta estimates are large
because the adapter debug-formats node kinds, clones node content, and
recursively copies arena trees. Those are prototype choices, but bounded nested
deltas require persistent item/row/leaf fragments and a direct parser event
sink; compacting the top-level wrapper is not enough.

## Giant inline representation

A syntax-dense 10 MB paragraph parsed through full Comrak in approximately
1.03–1.57 seconds in the reproduced run and reached about 761 MB maximum RSS.
Earlier uncontended samples in the same disposable checkout were faster but
still hundreds of milliseconds with roughly 725 MB peak memory.

This does not show that Comrak's scanning algorithms are poor. It shows that
creating a heavyweight arena node/string representation and then converting it
to editor chunks already violates the intended giant-leaf envelope. The
production inline machine needs source cursors, index-based delimiter/bracket
state, explicit scan/resolve/output phases, bounded cancellation points, and a
compact persistent sink.

## Minimum deep-fork surface

An exact Comrak-derived implementation would need to change:

- block parser ownership and finalization in `parser/mod.rs`;
- inline ownership, delimiter/bracket state, and resolution in
  `parser/inlines.rs`;
- table/list aggregate state and stable nested fragments;
- `nodes.rs` source/content ownership;
- exact source facts currently reconstructed by bridge classifiers; and
- the incremental service, chunk, reference, budget, cancellation, and binary
  protocol layers.

The hot ownership-sensitive Comrak files total roughly 7,300 lines before the
3,316-line incremental module and roughly 2,100 lines of current bridge
fact/conversion code. Not every line changes, but this is not an API-hook-sized
maintenance commitment.

## What remains valuable

- Complete pinned CommonMark/GFM behavior and mature fuzz/pathological cases.
- Ordinary top-level block locality.
- Proven algorithms for blocks, delimiters, brackets, tables, and extensions.
- Independent full-parse differential output.
- A contingency base if the product explicitly accepts weaker giant-input
  behavior or chooses to own a deep derived fork.

Those assets should reduce semantic risk in the Flark-owned core; they do not
justify retaining the arena as the product's persistent runtime model.
