# Serialized green source contract

Status: **physical representation selected; logical-projection schema
correction under executable falsification**, 2026-07-16.

This contract defines the strongest packed-Euler candidate in the structural
representation bakeoff.  It is intentionally representation-specific but
grammar-neutral.  Its purpose is to determine whether one source-ordered green
sequence can cleanly implement semantic order, source ownership, direct-child
folds, and local persistent edits without a document-wide BlockId directory.

## Token model

The committed sequence contains final semantic structure and coalesced source
ownership, not parser-event history:

```text
OpenBlock {
  stable_block_id,
  block_kind,
  compact finalized structural facts
}

CloseBlock

SourceProjectionRun {
  stable_coverage_id,
  byte_length,
  utf16_length,
  owner_up,
  source_part,
  logical_contribution,
  boundary_affinity when required
}

LogicalContribution =
  None
  | Identity
  | Atomic(transform_kind, logical_byte_length, logical_utf16_length)
  | Program(typed_page_edge, logical_byte_length, logical_utf16_length)
```

`owner_up == 0` names the innermost open block at that token; larger values
name an ancestor.  This represents a continuation-line blockquote/list marker
that occurs in source order while a descendant paragraph remains open without
creating a second coverage tree.  Invalid owner depths are corruption.

`source_part` is a small grammar-free source-ownership class such as terminal
content, container marker, block marker, or editable gap. It does not also
encode logical transformation. `logical_contribution` records the exact block
parser result needed to reconstruct one terminal block's logical input without
rerunning Markdown classification: no contribution, exact identity, one
indivisible typed replacement such as a partially consumed tab, or a relative
typed program for compound transforms such as table-cell trim/unescape.

A Program partitions both its owning physical range and logical output into
Identity, Hidden, Atomic, and Virtual pieces. It contains no source text or
absolute coordinate. Interior queries of an Atomic replacement return an
explicit ambiguity rather than a guessed mapping. Code/HTML facts name
validated logical slices over the same stream; aggregate logical strings are
not retained.

Inline semantic targets and edit capabilities remain requested presentation
facts rather than structural tokens. Exact logical projection is different: it
is parser output and must be committed with physical ownership under the same
green transaction.

Adjacent runs coalesce only when owner, source part, logical contribution, and
boundary semantics are compatible. Identity/None common cases remain compact;
Atomic boundaries never disappear, and Program pages share only through exact
lineage. Runs do not preserve physical-line mutation history. A 100,000-line
plain paragraph should therefore use one or a small bounded number of
Identity runs even though the source rope and restart index retain their own
physical-line boundaries.

The sequence stores no source text, absolute byte/UTF-16 offset, absolute token
rank, old Crop root, parser scratch, or Flutter/layout identity.

## Page summaries

Every persistent sequence subtree has one associative summary containing at
least:

- token and block counts;
- byte and UTF-16 source metrics;
- net semantic depth and minimum/maximum relative depth;
- the exact minimum-Enter direct-child fold already proved by the Euler
  challenger; and
- enough forward/reverse balanced-parentheses navigation to find unmatched
  opens and a matching close while skipping complete pages.

The summary is constant size.  It may not grow with semantic depth, syntax
kind count, or the number of blocks in the page.  Feature-specific facts live
in typed `OpenBlock` payloads or separate sparse derived indexes, not in the
sequence algebra.

## Source-first queries

### Position to owner and ancestry

1. Descend byte or UTF-16 prefix metrics to the containing
   `SourceProjectionRun`.
2. Resolve upstream/downstream affinity at an exact run boundary.
3. Recover the open semantic path at that cursor by reverse
   balanced-parentheses navigation, skipping whole summarized subtrees.
4. Select `path[path.length - 1 - owner_up]` as the exact source owner.

The query must report pages/nodes/tokens visited.  It may be proportional to
`log pages + open depth`; it may not scan from document start, consult a hidden
BlockId map, or rely on an oracle token rank.

### Block range and subtree

A source-derived path result contains a revision-scoped cursor for each
enclosing `OpenBlock`.  From that cursor, forward balanced-parentheses search
finds its matching `CloseBlock` and permits bounded subtree traversal.  Product
callers first locate a block through source, viewport, parser continuation, or
a typed delta handle.  An arbitrary long-lived `BlockId -> cursor` API is not
part of this candidate.

Canonical block ranges are coverage-relative hulls.  An ancestor-owned marker
may lie between the first and last paragraph-owned runs without becoming the
paragraph's source owner.  Parent containment, byte/UTF-16 agreement, and the
canonical-range oracle must still hold.

### Viewport

Descend to the first intersecting source run, recover the one enclosing path,
then stream neighboring tokens/pages until the requested source window and
overscan are covered.  Zero-source structural tokens do not force whole-tree
materialization.

### Terminal logical cursor

Starting from a source-derived terminal Enter capability, stream its bounded
subtree and execute each run's typed logical contribution against the bound
source revision. Identity slices are borrowed, Atomic replacements are emitted
as typed chunks, and Program pages are traversed under fuel. Paragraph,
Heading, and TableCell expose their complete logical stream. Code and HTML
facts expose validated info/literal slices over it.

The cursor reports physical and logical bytes/UTF-16 visited and every mapping
ambiguity. It may not look up richer state through CoverageId, materialize an
aggregate String, or rerun table/reference/fence/HTML recognition.

## Mutation capabilities

Committed-tree mutation entry points are derived from current source/path
queries, mapped restart boundaries, or revision-local parser builder handles:

- splice a source/semantic interval;
- rewrite compact facts on one known open ancestor;
- promote or detach one contiguous semantic range;
- cut and reinsert one balanced subtree; and
- attach an immutable suffix at an exact source/convergence boundary.

Coverage and logical contribution are cut, replaced, coalesced, and adopted as
one record. Reference-prefix drain, Setext/table promotion, raw finalization,
and source edits may not update a parallel projection tree later.

The parser sink may keep transient page/cursor capabilities while constructing
one candidate.  They expire on cancellation or commit and cannot keep a retired
root alive.  A prefix edit must retain exact distant suffix page and block IDs;
no cursor table may rebase all later entries.

Changing a descendant may also rewrite an ancestor `OpenBlock` fact such as
list tightness.  Two bounded path-copy edits are acceptable.  Scanning all
children or copying the intervening source is not.

## Partial and open roots

An incomplete worker publication still describes the entire current source as
certified structure, an explicit `UnknownRange`, and an optionally converged
suffix.  The unfinished parser path is a grammar-free open overlay containing
only stable identities, kinds, ancestry, and source anchors.  Parser
continuation remains worker-private and never enters an `OpenBlock` payload or
page equality.

The executable candidate must choose one of two honest encodings:

- permit an intentionally unbalanced prefix plus typed unknown token whose
  root summary validates the open overlay; or
- commit only closed green regions and join them with the overlay/unknown range
  in the composite manifest.

It may not fabricate closing structure or parse the unknown range with another
classifier.

## Derived indexes

A separately persistent index is allowed only when a real query earns it and
the architecture authority rule is met.  In particular:

- reference symbol/occurrence indexes are expected because dependency lookup
  is not a structural source query;
- restart samples are expected because parser resumption has a different
  lifetime;
- requested inline/presentation pages are expected because they are
  leaf-complete and revision-scoped; and
- a global BlockId directory is absent unless an audited caller cannot start
  from source/path/delta capabilities.

Any admitted derived index is included in retained bytes, update work,
transaction ownership, and cancellation receipts.  It cannot repair or
override semantic structure.

## Genericity gate

The codec is rejected if it requires a different structural storage mechanism
for List/Item microtrees, tables, paragraphs, raw blocks, or references.
Compact typed payload variants and common-case varint forms are fine; separate
feature-owned order/coverage structures are not.

The minimum heterogeneous witness contains Document, BlockQuote, List, Item,
Paragraph, Heading, fenced/indented code, HTML, Table/Row/Cell, thematic break,
editable gaps, ancestor continuation markers, and reference-only paragraph
detach.  Promotion and reparent use the same balanced range operations as
ordinary replacement.

## Required receipts

The candidate reports total retained and peak temporary bytes for:

1. 100,000 top-level paragraphs;
2. one 100,000 Item-to-Paragraph list;
3. a 10 MiB/100,000-line plain paragraph and fenced block, including logical
   projection pages and cursor scratch;
4. nested continuation markers and blank gaps;
5. table/setext promotion and reference-only detach;
6. a tightness-changing interior list edit;
7. 10,000 insertions in one gap; and
8. one prefix edit retaining a distant suffix page exactly.

The count includes token payloads, page/branch headers, arena slots or allocator
overhead, root/manifest state, typed projection/large-fact edges, optional
directories, the maximum streaming page/program buffer, and the top-level
ownership journal. Compare against an executable packed-flat baseline
implementing the same facts and operations; arithmetic for structure-only
payloads is not a selection receipt.

## Stop conditions

Reject this candidate if it needs:

- an absolute token rank or document-wide locator update after prefix edits;
- a global BlockId map merely to perform normal parser or viewport operations;
- scanning from document start to recover an enclosing path;
- per-line coverage tokens for ordinary same-owner paragraph content;
- an owner-depth vector in page summaries;
- feature-specific repair passes or independently mutable parent truth;
- a parallel CoverageId-to-projection directory or aggregate logical String;
- renderer-side block/table/reference/fence/HTML reclassification;
- guessed physical coordinates inside an Atomic replacement;
- persistent parser events/checkpoints in the green sequence; or
- non-transactional adoption of structure, source ownership, and logical
  projection.

Passing this contract would select the physical structure/output seam, not the
parser implementation.  The exact block machine must still stream directly
into it and match a clean parse after every revision before the architecture is
frozen.
