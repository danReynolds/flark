# Serialized-green projection sufficiency audit

Status: **physical coverage GO; current logical-projection codec FAIL;
serialized-green representation remains selected with a mandatory schema
correction**, 2026-07-16.

## Verdict

`CoverageRun { id, metric, owner_relative_depth, part }` is sufficient to
index a total physical source partition and recover its semantic owner. It is
**not** sufficient to reconstruct the exact parser-logical byte stream used by
inline/reference parsing or the logical projections used by code and HTML.

That is a codec omission, not evidence against the source-ordered
serialized-green representation. The minimal clean correction is to make each
physical coverage run also describe its contribution to its owner's one
logical-content stream, with an atomically owned typed projection program for
the uncommon non-identity case. Source ownership and logical projection remain
orthogonal fields on one authority; there must not be a second coverage tree
or a renderer-side Markdown classifier.

Direct parser-to-green composition must stop until this correction has an
executable corpus gate. The current 34.98-byte-per-block receipt remains valid
for the facts it measured, but it is not a complete retained-memory receipt
for the production semantic root.

## What the current packed stream proves

The current packed token has only:

```text
CoverageRun
  CoverageId
  physical byte length
  physical UTF-16 length
  owner-relative depth
  source-ownership part
```

This is the complete Rust shape in
`v3_runtime_slice/src/serialized_green.rs:207-240`, and its encoder writes only
those values at `:685-699`. `GreenCoverageView` consequently returns a
physical byte/UTF-16 range, owner/path, part, and validator capability, but no
logical length, transform, logical channel, or projection recipe (`:1578-1588`,
`:1672-1703`).

That model already supports the important structural operations:

- one physical byte/UTF-16 prefix descent;
- exact source owner and open ancestry;
- ancestor-owned continuation markers interleaved with an open descendant;
- source-first viewport streaming; and
- stable physical-run identity without an absolute-offset directory.

The focused packed tests are green: five serialized-green integration tests,
including the interleaved-owner/unequal-UTF-16 witness and the 100,000-item
receipt. Those tests construct metrics directly; none supplies a source root,
asks for parser-logical bytes, or exercises code, HTML, table-cell, heading, or
reference projections.

The source contract currently overstates the implemented schema in one place:
it says `part` may represent hidden/replacement source
(`SERIALIZED_GREEN_SOURCE_CONTRACT.md:40-43`), while the codec exposes only
Content, ContainerMarker, BlockMarker, Gap, and Terminal
(`serialized_green.rs:98-107`). More importantly, even adding a `Hidden` part
would not encode a tab replacement, table-cell normalization, synthetic byte,
or logical slice boundary. `part` is an edit/ownership class, not a projection
program.

## Information the exact block parser actually produces

The Comrak-correspondent value core demonstrates that final block output needs
strictly more information than physical coverage metrics.

### Paragraphs and headings

`ValueBlockParser::add_line` removes block/container prefixes before appending
logical content. A partially consumed physical tab appends between one and
four logical spaces while retaining the one-byte tab as its origin
(`parser.rs:1122-1187`). Thus this physical run is ambiguous under the current
codec:

```text
physical: "\t"       bytes=1, utf16=1, part=Content
logical:  " " | "  " | "   " | "    "
```

The result depends on the exact parser column at that transition. Neither the
physical metrics nor `owner_relative_depth` records the surviving space count.
Recomputing it from ancestor facts would reproduce block-prefix parsing in the
projection layer and restore a dual authority.

ATX opening/trailing hashes and a Setext underline similarly occupy physical
source without belonging to the heading's inline input. Exact run cuts can
make those bytes noncontributing, but the current packed tests do not define or
verify those cuts.

### Leading reference definitions

Paragraph finalization parses leading definitions, records occurrences, and
drains the exact consumed logical prefix; it may then detach the now-empty
Paragraph (`parser.rs:1459-1484`). The restart work correctly models this as
output state: `ParagraphOutputAccumulator` retains the logical, preface, last
line, and a certified `consumed_prefix`/visible-remainder result
(`checkpoint.rs:248-405`).

The block-projection corpus tests prove the resulting survivor and origin
behavior over all 1,322 fixtures, including multiline definitions, Setext
interaction, CRLF/lone-CR, and duplicate definitions. Physical ownership alone
does not say which logical prefix was removed. The direct sink must turn the
parser's certified result into an atomic projection-range replacement (and a
reference-root update), rather than asking the presentation layer to recognize
definitions again.

The canonical-range gate currently classifies a detached definition as a
Document-owned Gap. That is valid final physical ownership, but it does not by
itself prove the surviving paragraph's logical stream or the occurrence's
origin partition.

### Table cells and promoted table prefaces

GFM table cells are the decisive counterexample. The exact parser stores the
donor-transformed cell string and labels its origin
`TrimAndUnescapePipes` (`table.rs:150-175`, `:198-219`). Table promotion can
also trim and unescape a preface before reparenting it as a Paragraph
(`table.rs:250-276`). For example:

```text
physical cell: " a\|b "
logical cell:  "a|b"
```

Both transformed and untransformed interpretations have the same BlockId,
kind, physical metrics, owner depth, and `Content` part. A consumer can choose
the correct result only by receiving the parser's transform result/recipe or
by rerunning the table grammar.

Serializing the existing `source.rs::OriginRun` unchanged is not a sufficient
fix. `LeafContent::transformed_slice` keeps the transformed aggregate in an
owned `String`, and `LogicalProjectionCursor` explicitly rejects
`TrimAndUnescapePipes`, entity/backslash normalization, and synthetic
transforms (`source.rs:294-316`, `:461-540`). A direct zero-copy green sink
therefore needs a finer transform program, not the proof parser's owned-string
fallback.

### Fenced/indented code and HTML

Raw blocks avoid aggregate literal copies only because `LeafContent` retains
source-relative origin runs plus the constant-size `SourceBackedContent` fold:

- total logical length;
- first-line content end and first-line end;
- last nonblank end and line index;
- last-line logical length; and
- line count (`source.rs:69-96`, `:158-224`).

Finalization consumes those values to derive:

- fenced info and literal logical ranges;
- indented-code trailing-blank trimming plus a synthetic final newline; and
- HTML literal and source-end projection (`parser.rs:1343-1393`).

The packed `CODE` and `HTML` fields currently have only shallow size checks
(`serialized_green.rs:588-655`), no typed decoder or projection query, and no
test instantiates them. Coverage `Content` cannot distinguish opener info,
opener newline, literal body, trimmed trailing blanks, closing fence, or the
synthetic newline.

The existing 1/10 MiB literal tests prove the desired no-aggregate-copy
behavior in the value core. They do not prove that the selected green codec
can retain and query the same logical projection after `LeafContent` is
removed.

## Sufficiency matrix

| Requirement | Current packed fields | Verdict |
| --- | --- | --- |
| Physical byte/UTF-16 prefix lookup | Exact subtree metrics | **Sufficient as an index**, once build actions validate them against the bound source revision |
| Physical owner and open ancestry | Owner depth + balanced path | **Sufficient** |
| Exact paragraph/heading inline bytes | No logical contribution or transform | **Insufficient** |
| Exact table-cell inline bytes | No trim/unescape program; no logical length | **Insufficient** |
| Reference-prefix removal | No consumed logical range or replacement recipe | **Insufficient** |
| Fenced-code info/literal | No logical stream boundary/range facts | **Insufficient** |
| Indented-code trim/synthetic newline | No hidden/synthetic logical contribution | **Insufficient** |
| HTML logical literal/source-end fold | No logical projection descriptor | **Insufficient** |
| Logical byte/UTF-16 to physical mapping | Physical aggregate only | **Insufficient**, except for a separately certified identity run |
| Physical byte to logical mapping | No identity/hidden/atomic/ambiguity class | **Insufficient** |

`CoverageId` cannot close any of these gaps. The architecture deliberately
treats it as a lineage validator rather than a locator. Making it a key into a
hidden projection directory would create the parallel authority the selected
representation was meant to avoid.

## Minimal coherent correction

Keep the balanced source-ordered sequence and upgrade `CoverageRun` into one
source-and-projection record:

```text
SourceProjectionRun {
  coverage_id,
  physical_metric { bytes, utf16 },
  owner_relative_depth,
  source_part,
  logical_contribution,
  hidden_boundary_affinity_when_required
}

LogicalContribution =
  None
  | Identity
  | Atomic {
      transform_kind,
      logical_metric { bytes, utf16 }
    }
  | Program {
      logical_metric { bytes, utf16 },
      typed_page_edge
    }
```

Each terminal block has one source-ordered logical-content stream. Paragraph,
Heading, and TableCell feed the whole stream to inline/reference services.
Code and HTML facts contain validated logical slices over that same stream
(info/literal for code, literal/source-end fold for HTML). This follows the
working `LeafContent + LogicalProjection` model without retaining its owned
aggregate `String`.

The contribution meanings are:

- `None`: physical marker/gap/trimmed/definition source contributes no logical
  bytes; source-to-logical affinity is explicit where a hidden span borders
  editable content;
- `Identity`: borrow the exact bound-source slice; logical and physical
  byte/UTF-16 metrics agree;
- `Atomic`: one typed indivisible replacement, initially tab-to-N-spaces and
  any explicitly selected newline normalization; an interior mapping returns
  a typed ambiguity zone rather than a guessed coordinate; and
- `Program`: a packed relative sub-run sequence of Identity, Hidden, Atomic,
  and Virtual pieces for dense/compound transforms such as table-cell
  trim/unescape or an indented-code synthetic newline.

The program is relative to its owning physical run. It must partition the run's
physical bytes exactly, partition its logical output exactly, and contain no
source text or absolute coordinates. Common identity/none cases remain inline
one-byte descriptors; only a real transform earns an arena edge. The edge is a
typed child of the same green page/candidate transaction and the composite
manifest adopts it atomically. It is not a second coverage index.

Table trim/unescape should normally compile to granular runs: leading/trailing
space and an escape backslash are Hidden, retained characters are Identity,
and only genuinely many-to-one replacements are Atomic. A whole cell may not
be labeled one opaque atomic transform, because a fact or selection inside
`a|b` must not claim the entire physical cell. The already-green inline-origin
gate demonstrates the right contract: identity maps precisely, atomic ranges
are indivisible, and physical prefix gaps are never claimed.

Coalescing becomes exact and simple:

- adjacent Identity runs may coalesce only when owner, source part, affinity,
  and projection semantics all match;
- None runs may coalesce only when their boundary behavior matches;
- Atomic boundaries never disappear; and
- a Program remains self-contained and is shared only through exact source
  lineage.

The parser sink must emit these descriptors at the moment it already knows the
corresponding `OriginTransform`, reference-prefix result, table-cell transform,
or raw-block fold. A later presentation query only executes a small typed
transducer over source slices. It never classifies Markdown.

## Why this remains one clean architecture

This correction does not add a second parse or a second independently mutable
tree:

```text
exact parser action
  -> source ownership + logical contribution
  -> one packed green candidate transaction
  -> one revision-bound manifest
  -> bounded logical cursor / inline service
```

Physical coverage answers where bytes live and who owns them. The orthogonal
logical contribution answers what the exact parser already decided those bytes
mean as block-leaf input. Inline facts, reference winners, presentation spans,
and layout remain derived indexes with separate lifetimes; none is allowed to
repair or reinterpret the projection program.

The existing `comrak_derived_core::origin_runs` prototype already proves the
useful four-part algebra (Identity, AtomicTransform, Hidden, Synthetic) for
container gaps, CRLF/lone-CR, and partial tabs. The
`comrak_inline_fragment_gate` proves that a multiline inline fact can map to
disjoint physical parts without claiming hidden prefixes and that partial
atomic mappings fail closed. Those focused tests are green. They are mechanism
evidence, not yet a table/reference/raw-block-complete production codec.

## Required executable tests

### Codec and corruption

1. Round-trip every contribution variant and reject unknown critical transform
   kinds, mismatched logical metrics, nonpartitioning Programs, bad edge types,
   stale generations, and nonminimal encodings.
2. Validate every physical run against a capability from the exact bound source
   revision; hand-authored byte/UTF-16 metrics may not become trusted truth.
3. Prove that roots differing only in transform count, hidden affinity, logical
   range, or synthetic output do not compare/adopt as equal.

### Exact parser differential

4. For every one of the 1,322 CommonMark/GFM fixtures, compare the new green
   logical cursor against `BlockDocument` for every Paragraph, Heading,
   TableCell, fenced/indented code info/literal, and HTML literal.
5. Compare the full logical-origin partition, not just rendered HTML: every
   logical byte/UTF-16 boundary maps to exact physical parts or an explicit
   atomic ambiguity, and every physical byte is Identity, Hidden, Atomic, or
   outside that leaf by construction.
6. Compare reference occurrences, consumed prefixes, surviving Paragraph
   streams, definition-only detach, duplicate first-winner behavior, Setext
   interaction, and GFM table-preface reparenting.

### Adversarial cases

7. Nested quote/list prefixes with partially consumed tabs, CRLF, lone CR,
   multibyte UTF-8, and astral UTF-16 characters.
8. ATX opening/closing hashes, Setext underlines, multiline reference
   definitions, and a definition followed by visible content in the same and a
   later coverage leaf.
9. Table cells with leading/trailing whitespace, escaped pipes, empty/padded
   cells, dense escapes, Unicode, and a promoted multiline preface. Test every
   logical and physical boundary, not only final cell text.
10. Fenced code with info, body, closer, nested container prefixes, fence
    offset, partial tabs, CRLF, and an unclosed EOF fence; indented code with
    trailing blanks and its synthetic final newline; every HTML block class.

### Incremental and scale

11. Prefix edits retain an unchanged distant SourceProjectionRun and its
    Program page by exact ArenaId with no retired source root or absolute
    rebase.
12. Edits inside a coalesced identity run split/rejoin coverage and projection
    together under the CoverageId survivor rules.
13. Reference-prefix drain, Setext promotion, and table promotion replace
    structure, ownership, projection programs, and reference roots in the same
    base-root transaction.
14. A 10 MiB plain paragraph/fence/HTML path owns no aggregate logical `String`,
    keeps identity runs coalesced, streams bounded chunks, and reports Program
    pages plus scratch in retained/peak memory.
15. Cancellation/allocation failure at every Program page and composite
    manifest boundary reclaims the candidate while the old logical cursor
    remains queryable.

## Stop conditions

Stop and redesign the projection seam if any implementation requires:

- rerunning block, table, reference-definition, fence, or HTML classification
  from a source slice in the projection/UI layer;
- treating `source_part` as both edit ownership and logical-transform truth;
- looking up richer projection state through a global `CoverageId` directory;
- retaining paragraph, table-cell, code, or HTML aggregate strings in green
  state;
- storing absolute source/logical offsets that rebase after a prefix edit;
- committing projection programs separately from structure/source ownership;
- mapping an interior atomic replacement to a guessed physical byte;
- one opaque whole-cell transform that cannot map internal facts/selections;
- per-line Identity records for an otherwise coalescible plain paragraph or
  raw block; or
- claiming the packed representation is production-complete from a structural
  memory receipt that excludes projection-program pages.

## Recommendation

Proceed with serialized green, but make projection sufficiency the next codec
gate before broad parser composition. Implement the common inline
None/Identity/TabAtomic descriptor, one typed relative Program page, and a
streaming logical cursor first. Then wire only Paragraph/Heading, table cells,
reference-prefix drain, and raw-block finalization through it and run the
1,322-fixture differential.

If those features require a parallel source map or aggregate logical strings,
reject the correction and revisit the representation. If they fit the same
run/edge/transaction model, this closes the largest remaining losslessness gap
without changing the selected architecture.
