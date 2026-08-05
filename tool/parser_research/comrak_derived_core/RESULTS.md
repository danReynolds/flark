# Comrak-derived core seam results

Status: research-only falsification, updated 2026-07-15. This directory does
not alter the production bridge or architecture documents. The commitment-gate
addendum below supersedes the earlier "continue for one bounded gate" verdict.

## What was built

The probe ports a coherent slice of Comrak v0.54.0's block loop while changing
the representation rather than wrapping the arena:

- persistent `Arc`-chained block quote/list/item frames;
- compact leaf values for paragraphs, setext headings, and fenced code;
- source-backed line chunks and markers rather than `Ast.content` strings;
- direct open/close/promote events rather than AST conversion;
- an explicit coroutine whose tick inspects at most one source byte;
- O(1)-clone checkpoints at physical-line boundaries; and
- fingerprint-first, collision-safe, cooperatively budgeted state comparison.

The exact upstream function map is in `PROVENANCE.md`. Fourteen named Comrak
functions cover 718 upstream source-line spans. The probe core is 2,117 Rust
lines excluding tests and binaries. That crude correspondence density is only
34%, or roughly 3x expansion over the mapped upstream spans. It is not a claim
that 718 lines were copied verbatim: most of the expansion is explicit
coroutine, value-state, event, and fuel plumbing. This is already a maintenance
warning for a narrow subset.

## Validation receipts

`cargo test --release` passes 9 tests. Seventeen selected fixtures compare the
probe's normalized block structure with unmodified Comrak 0.54.0, including
nested quotes/lists, lazy continuation, list-marker changes, paragraph
interruption, fence closure, invalid backtick info, and setext promotion. This
is useful seam evidence, not CommonMark conformance coverage.

`cargo clippy --all-targets --all-features -- -D warnings` and
`cargo fmt --all -- --check` pass.

The 10 MB probes use 4,096 work-unit polls:

| Shape | Polls | Total work | Bytes inspected | Maximum bytes/poll | Time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Paragraph | 2,442 | 10,000,007 | 10,000,003 | 4,096 | 39 ms |
| Setext underline | 2,442 | 10,000,019 | 10,000,011 | 4,096 | 38 ms |

The parser therefore really yields inside a physical line and a retroactive
setext scanner. The corresponding tests also exercise a 2,000-deep quote and
make exact state comparison yield every 31 frames.

Setext is represented as a `PromoteLeaf` event targeting the existing paragraph
ID. No side scanner or second AST is needed. A separate generated-scanner probe
at `/private/tmp/flark-resumable-scanner/setext_probe.re` found that Comrak's
re2c-derived scanner source can also be generated as a storable-state lexer.
Production should preserve that generated lineage rather than grow handwritten
scanner coroutines like the one in this probe.

## Dense-line memory result

The trace `LineRecord` is 192 bytes before heap allocations. Retaining one
record for each of one million `a\n` lines is therefore intentionally a losing
sink shape:

| 2 MB / 1,000,000 lines | Parser time | Maximum RSS | Retained records |
| --- | ---: | ---: | ---: |
| Retain research records | 74 ms | 197,771,264 bytes | 1,000,000 |
| Drain into an external sink | 57 ms | 5,799,936 bytes | 0 |

The block machine itself does not need per-newline persistent syntax nodes. In
this input every continuation shares one paragraph leaf ID and only its first
line emits `StartLeaf`; a production sink can coalesce contiguous source spans
and keep adaptive checkpoints. The current research `Vec<LineRecord>` must not
become that sink. Until the coalescing/index layer is implemented and measured,
the low-memory row is a parser-core receipt, not an integrated-document claim.

## What the representation did and did not eliminate

`src/lib.rs` contains no Comrak `Ast`, arena `Node`, owned `String`, or copied
leaf-content buffer. Chunks retain ranges into one `Arc<str>` source and events
retain only value metadata. This eliminates the arena/conversion duplication
that falsified the surgical adapter.

It does **not** yet use Flark's editable rope/input abstraction. Replacing the
whole `Arc<str>` after an edit would be unacceptable. It also does not yet have
adaptive checkpoint storage or a persistent coalescing syntax sink.

## Semantic limits that still decide the architecture

The probe now covers quote/list/fence/paragraph/setext mechanics, but it still
omits or simplifies:

- exact list tightness and closed-prefix aggregates;
- the full `add_child` containment/finalization matrix;
- exact tab/partial-tab behavior in every branch;
- thematic breaks, ATX headings, indented code, HTML, and GFM tables;
- reference-definition extraction and dependency invalidation;
- source-backed inline handoff, delimiter/bracket state, and inline deltas; and
- edit-local convergence and stable nested output IDs.

The first differential matrix caught a real hidden coupling: changing `-` to
`+` must finalize the old List before opening its sibling. The setext test also
caught that an apparently empty `- ` list line can retroactively promote the
preceding paragraph. Those fixes stayed clean in value/event form, but they show
why a loose rewrite of the algorithm is unsafe.

## Comparison with Pulldown

Pulldown-cmark's flat indexed tree and two-pass organization would make arena
removal, serialization, and index-based state easier. Its current first pass,
however, eagerly emits inline candidates and newline nodes: the companion probe
measured about 2,000,001 nodes / 192 MB capacity for the same 2 MB million-line
shape, and about 24 MB for a 1 MB syntax-dense inline input. It would also need a
lazy source-backed inline/event-sink refactor.

Comrak's strongest donor advantage is its existing block-before-inline phase
boundary. Once `Ast.content` becomes source spans, a paragraph can remain one
lazy leaf instead of per-token or per-newline syntax. Its scanners also have a
plausible generated resumability path.

Comrak's strongest donor disadvantage is visible in the 2,117-line result:
arena containment and finalization behavior is spread across control flow, so
preserving semantics while changing ownership creates substantial explicit
machinery. List tightness and inline handoff may expand that machinery enough
that this ceases to be a maintainable correspondence-preserving port.

## Verdict and next falsification

This seam is genuinely cleaner than the arena adapter: its runtime state,
source ownership, fuel, and outputs have the right *shape*, and retroactivity
did not require a second parser. It is not yet enough evidence to commit the
whole parser to this donor.

The next and final donor gate should implement only three integrated pieces:

1. exact Comrak-derived list tightness with closed-prefix aggregates;
2. a source-backed paragraph-to-inline handoff using resumable generated
   scanners and no per-newline/token output; and
3. a rope-backed input plus coalescing/adaptive sink that keeps the million-line
   case under a stated memory ceiling.

If those remain function-correspondent and compact, Comrak is the stronger
donor because it preserves lazy block/inline separation. If they force another
large hand-translated state machine or duplicate semantics, stop the port and
run the same integrated gate against Pulldown's indexed core. That bounded gate
has now run; its result follows.

## 2026-07-15 exact block-spine commitment gate

### Variants evaluated

The gate separated three previously conflated designs:

1. Flark owns block semantics and state: translate Comrak into a new coroutine
   parser.
2. Flark owns state/output but reuses pinned Comrak lexical helpers.
3. Refactor Comrak's arena/content ownership in place.

Variant 2 produced a useful primitive: a 234-line pinned internal facade for
exact HTML start/end DFAs, table-row/escaped-pipe tokenization, and multiline
reference label/URL/title/case-fold handling. Every call is capped at 8 KiB.
The patch is one new module, two exports, and visibility-only changes to
`table::row`/`Row`/`Cell`.

It did **not** produce a small exact parser. The 921-line
`src/commitment_spine.rs` still had to own list aggregates, container
precedence, table activation, HTML open state, paragraph finalization,
first-definition-wins generations, checkpoints, and opaque degradation. A
companion audit found that variant 3 is not smaller: 55 relevant Comrak block
functions / 1,816 lines, including 29 functions / 1,252 lines directly coupled
to arena/content ownership (84 tree operations and 44 content sites).

### Exactness result

Seven focused differential tests pass against pristine Comrak 0.54.0. The
pinned upstream GFM seam matrix is the decisive result:

| Seam | Examples | Exact normalized facts | Divergent | Remaining coupling |
| --- | ---: | ---: | ---: | --- |
| List structure/tightness | 26 | 20 | 6 | relative indentation, fenced children, descendant blankness |
| HTML type/literal | 43 | 35 | 8 | container indentation, lazy/open-block context, literal prefix ownership |
| Table columns/rows/alignment | 8 | 7 | 1 | block-quote interruption precedence |
| Reference symbol snapshot | 28 | 26 | 2 | ATX/container finalization and quote-prefix ownership |
| **Total** | **105** | **88** | **17** | block orchestration, not lexical scanners |

`tests/spec_commitment.rs` intentionally asserts those divergence IDs, so its
green status is a stable falsification receipt, not conformance. Exact donor
scanners removed lexical duplication; they did not remove the surrounding
Markdown block semantics.

### Bounded latency and large-document receipts

Ten thousand warmed release calls at the full 8,192-byte cap measured:

| Exact donor helper | p50 | p99 | p99.9 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Table row | 18.7 us | 20.4 us | 53.5 us | 361 us |
| HTML type-7 start | 7.9 us | 8.7 us | 22.2 us | 47.7 us |
| Reference definition | 16.3 us | 23.9 us | 65.6 us | 181 us |

The release benchmark drains facts after every poll. All 10 MiB ordinary
shapes used 4,096-byte cooperative polls, emitted no opaque regions, and used
about 22.4--23.3 MiB maximum RSS:

| Shape | Lines | Time | Facts | Opaque | Maximum atomic input |
| --- | ---: | ---: | ---: | ---: | ---: |
| List | 1,747,627 | 344 ms | 1,747,628 | 0 | 5 B |
| Loose list | 1,429,878 | 234 ms | 953,253 | 0 | 14 B |
| Table | 898,782 | 396 ms | 898,781 | 0 | 12 B |
| HTML | 1,429,878 | 209 ms | 953,252 | 0 | 10 B |
| References | 911,806 | 313 ms | 455,903 | 0 | 22 B |

The initial large-list benchmark incorrectly accumulated all sibling items in
one pending paragraph and emitted `ParagraphOverCap`. Closing the leaf at every
item marker fixed that falsification: the final 10 MiB list row above emitted
1,747,628 streamed facts, zero opaque regions, and used 23.3 MiB maximum RSS.

A 10 MiB single physical table, HTML, or reference line took 20--26 ms to scan
cooperatively, used about 33.2 MiB maximum RSS including source, never entered a
donor scanner (`maximum_atomic=0`), and emitted exactly one explicit
opaque/source-visible fact. This is honest bounded behavior, not exact live
semantics for arbitrarily large single blocks.

### Exact block-to-inline origins

`src/origin_runs.rs` adds the source-mapping contract the earlier virtual-byte
model lacked. Its 440 lines distinguish:

- identity content;
- atomic transforms such as CRLF-to-LF and tab-to-spaces;
- hidden block/container prefixes; and
- genuinely synthetic bytes with no physical source.

Three tests preserve actual CRLF spellings, tab expansion, quote prefixes,
disjoint physical parts for multiline inline facts, and bidirectional origin
classification. A virtual newline attached to one boundary cannot safely
round-trip deletion of the physical `\r\n`, and hidden `> ` prefixes must not
be silently claimed by inline facts. This origin-run protocol is
parser-independent and should survive the donor decision.

### Commands

```sh
cargo test --test commitment_spine --test spec_commitment --test origin_runs
cargo test --lib --tests
cargo clippy --all-targets --all-features -- -D warnings
cargo run --release --bin facade_latency -- 10000
cargo build --release --bin commitment_bench
/usr/bin/time -l target/release/commitment_bench list 10 4096
```

### Recommendation

**NO-GO on continuing the separate simplified spine.** It has become a partial
second block parser and is still wrong on 17/105 pinned seam examples. Preserve
the small scanner facade as a bounded integration option and preserve the
origin-run contract. Then make the product-level choice explicitly:

- universal exact live semantics for arbitrarily large single blocks requires
  accepting a deep donor-derived/in-place ownership refactor; or
- if oversized blocks may stay source-visible, the 8 KiB scanner cap and
  explicit opaque region are a credible, far smaller launch design.

There is no evidence-supported middle path where a small Flark block spine
becomes exact merely by calling a few Comrak scanners.
