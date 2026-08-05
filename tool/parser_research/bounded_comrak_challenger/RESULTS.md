# Bounded Comrak challenger results

Status: disposable architecture evidence, 2026-07-15. This crate does not
modify the production bridge, the Comrak fork patch, or the v3 prototype.

## Verdict

There is a credible middle architecture, but it is **not** “send each enclosing
Markdown block to stock Comrak”:

1. Flark owns one grammar-exact, persistent, source-backed **block spine**.
2. That spine owns containers, list tightness, table activation/cells, setext
   promotion, HTML/raw-block termination, definitions, footnotes, restart state,
   and stable identities.
3. It hands only bounded inline-bearing leaves (paragraphs, headings, list-item
   paragraphs, table cells) to a small exposed Comrak inline API.
4. Raw blocks such as fenced code remain exact and source-backed without an
   inline parse.
5. A single inline-bearing leaf above the cap remains source-visible but does
   not receive complete live inline semantics until it falls below the cap.

That architecture is promising enough to keep as the primary lower-maintenance
challenger to the fully owned parser. It supports a genuinely large document
when the document consists of ordinary bounded leaves. It changes the SLA for
a pathological giant paragraph, not for every large document.

It does **not** make the parser fork surgical overall. The Comrak inline hook is
likely surgical; the exact persistent block spine is still a deep block-parser
ownership commitment. The remaining choice is therefore crisp:

- accept an explicit roughly 8 KiB live-inline leaf cap and avoid owning the
  complete resumable inline grammar; or
- require universal live formatting inside arbitrarily large single leaves and
  fund the fully owned donor-derived parser.

The current evidence favors building the real bounded inline hook and one exact
block-spine gate before committing to the fully owned inline machine. It does
not yet select the hybrid for production.

## What the prototype proves

The crate has two deliberately separate modes:

- `build_bounded_index` groups top-level semantic envelopes. It falsifies the
  naive proposal because a 10 MiB list becomes one opaque region even though
  each item is tiny.
- `build_bounded_leaf_index` keeps structural ownership in the spine and sends
  each inline leaf to a bounded Comrak stand-in. A 10 MiB list then has 189,232
  bounded delegated leaves and no over-cap inline leaf.

Stock Comrak has no public inline-fragment entry point, so the stand-in parses a
bounded logical leaf as a tiny document. This adds per-leaf overhead and is not
the proposed production API. It is sufficient to test the allocation boundary,
cap behavior, and local edit envelope.

The prototype drains block records continuously, retains source ranges rather
than leaf text, drops the bounded logical-content range vector as soon as a
leaf crosses the cap, and never calls Comrak before a region is closed and
measured. Every run reported `premature_comrak_calls=0`.

The current block spine is only the existing Comrak-derived research subset.
Known unsupported constructs fail closed. This is not a second heuristic parser
being proposed as an authority.

## Why an after-the-fact Comrak cap fails

In Comrak 0.54.0:

- `Parser::add_line` appends every accepted line to `Ast.content` and records a
  `line_offsets` entry before finalization (`parser/mod.rs`, around 1995–2012).
- inline parsing runs after the document block pass (`finalize_document` and
  `process_inlines`, around 2015–2034 and 2264–2293).
- each GFM table row and cell is allocated in the arena while the table is
  scanned (`parser/table.rs`, especially `try_opening_row`).
- list items and their paragraph children are likewise arena nodes needed by
  the current open-container control flow.

Marking a region opaque after parsing therefore hides output but does not avoid
the content, line-offset, row, cell, item, delimiter, or inline allocations.

A paragraph/fence-only patch could stop appending content once a byte counter
crosses the cap. That does not solve lists and tables: continuing exact block
recognition while suppressing their per-item/per-cell arena nodes requires a
compact value-state block representation. Running that representation beside
the arena branch would be two block parsers. The honest implementation is one
source-backed block core, derived from Comrak's algorithms, that emits both
structural facts and inline-leaf handoffs.

## Boundary and dependency falsifications

The tests intentionally cover places where “enclosing leaf” is not a complete
semantic unit:

- **List tightness:** must be an aggregate on the structural list. It does not
  require parsing every item inline as one giant list region.
- **GFM tables:** header promotion and row/cell splitting belong to the block
  spine. Once exact cell spans exist, each cell can be inline-parsed under the
  cap. The current subset lacks this and honestly makes the table opaque.
- **Setext headings:** the underline promotes the preceding paragraph. The
  spine must issue the promotion; the inline API parses only the title text.
- **References:** independently parsing `[use][label]` and a distant definition
  produces different HTML from a whole Comrak parse. The block spine needs a
  first-definition-wins symbol index and the inline API needs a resolver input.
- **Footnotes:** independent fragments lose global numbering, backreferences,
  and definition placement. Those are document projection/dependency facts,
  not leaf-local inline results.
- **HTML blocks:** block-class terminators can close without a blank line. The
  subset spine conservatively swallowed a later suffix into one opaque HTML
  envelope, proving that blank-line heuristics are not acceptable.
- **Loose-list suffix:** Comrak ends a generated list before a later unindented
  paragraph while the current subset retained that line in the list envelope.
  This is a direct boundary mismatch, not a theoretical warning.

A closed fence can recover a later suffix and a giant fence can remain an exact
source-backed raw region, but a debug-only invariant in the current derived
probe also observed one fence transition inspecting two bytes in one nominal
one-byte tick. Release output remained correct in the focused case. The hybrid
cannot be selected until the block machine itself passes its strict work and
differential gates.

## Benchmarks

Host receipts are release builds on the current development Mac with a 64 KiB
prototype cap. Times varied under concurrent research processes; ranges below
use the captured uncontended or CPU-consistent runs. They are architecture
receipts, not device acceptance numbers.

### Bounded hybrid stand-in

The command performs an initial build, a full exact-rescan fallback after a
one-byte edit, and a separate local-edit path. RSS therefore includes the old
and edited full `Arc<str>` sources and, transiently, a copied edited `String`.
A persistent rope would share almost all source storage.

| Shape | Size | Initial scan/delegate | Max RSS | Local ordinary-byte edit | Live result |
| --- | ---: | ---: | ---: | ---: | --- |
| One paragraph | 1 MiB | 4.4–5.2 ms | 5.0 MB | <1 us classifier | opaque inline leaf |
| One paragraph | 10 MiB | 47–80 ms | 33.4 MB | <1 us classifier | opaque inline leaf |
| Fenced code | 1 MiB | 6.9–11.8 ms | 5.3 MB | <1 us classifier | exact source-backed raw block |
| Fenced code | 10 MiB | 73–112 ms | 33.5 MB | <1 us classifier | exact source-backed raw block |
| Many list items | 1 MiB | 58.4 ms | 7.9 MB | 4.7 us leaf reparse | 19,269 bounded leaves; block subset not yet exact |
| Many list items | 10 MiB | 561 ms | 57.3 MB | 22.3 us leaf reparse | 189,232 bounded leaves; block subset not yet exact |
| GFM table | 1 MiB | 6.8 ms | 5.2 MB | <1 us classifier | unsupported/opaque in current spine |
| GFM table | 10 MiB | 75–86 ms | 33.6 MB | <1 us classifier | unsupported/opaque in current spine |

The 10 MiB list result is intentionally pessimistic on opening: it calls a
complete tiny-document Comrak parse 189,232 times. A direct inline hook removes
the repeated block-parser setup. The useful receipt is that the operation is
memory-bounded and a local item edit remains tens of microseconds.

For a 100 MiB document containing 16,606 approximately 6.3 KiB paragraphs:

- bounded prototype full initial CPU was about 0.8 seconds;
- a local leaf reparse was 48–86 microseconds;
- the compact region delta replaced one 6,311-byte semantic envelope and
  estimated an 80-byte protocol header before inline payload;
- max RSS was 315–319 MB, dominated by three transient 100 MB flat strings in
  the benchmark's edit setup; and
- a stock one-shot Comrak parse took 258 ms and reached 379 MB RSS.

The stock parser wins initial throughput. The hybrid is buying persistent
locality and bounded semantic allocations, not a faster clean parse. The source
backend and packed index determine whether the 100 MiB steady-state memory is
acceptable; this flat-string harness cannot answer that.

### Stock Comrak clean-parse comparison

| Shape | Size | Parse | Nodes | Max RSS |
| --- | ---: | ---: | ---: | ---: |
| One paragraph | 1 MiB | 52.8 ms | 54,240 | 39.7 MB |
| One paragraph | 10 MiB | 515.7 ms | 542,370 | 406.9 MB |
| Fenced code | 1 MiB | 1.1 ms | 2 | 4.3 MB |
| Fenced code | 10 MiB | 19.9 ms | 2 | 26.8 MB |
| Many list items | 1 MiB | 29.3–31.4 ms | 115,610 | 41.8 MB |
| Many list items | 10 MiB | 341.1 ms | 1,135,382 | 395.6 MB |
| GFM table | 1 MiB | 37.5 ms | 149,919 | 50.4 MB |
| GFM table | 10 MiB | 432.9 ms | 1,441,468 | 468.7 MB |

The fence row is a useful challenge to our assumptions: stock Comrak already
handles inert raw payload very efficiently. The source-backed spine is justified
there by eliminating duplicated literal ownership and enabling persistent
restart/deltas, not by clean-parse speed.

### Bounded inline latency

`inline_cap_bench` repeatedly uses stock Comrak full parsing as a conservative
stand-in for the proposed direct inline API. Warm host results:

| Input | Cap | p50 | p99 | Max |
| --- | ---: | ---: | ---: | ---: |
| Dense matched inline syntax | 8 KiB | 0.221 ms | 0.383 ms | 2.15 ms |
| Unmatched delimiter/bracket syntax | 8 KiB | 0.332 ms | 0.504 ms | 0.518 ms |
| Links | 8 KiB | 0.199 ms | 0.277 ms | 0.336 ms |
| Dense matched inline syntax | 16 KiB | 0.452 ms | 2.25 ms | 5.44 ms |
| Unmatched delimiter/bracket syntax | 16 KiB | 0.708 ms | 1.75 ms | 5.04 ms |
| Links | 16 KiB | 0.401 ms | 0.517 ms | 0.603 ms |
| Dense matched inline syntax | 64 KiB | 1.85 ms | 2.97 ms | 3.06 ms |
| Unmatched delimiter/bracket syntax | 64 KiB | 2.98 ms | 4.36 ms | 4.62 ms |
| Links | 64 KiB | 1.65 ms | 2.08 ms | 2.23 ms |

The 16 KiB maxima were scheduler/allocator tails rather than monotonic parser
behavior, but they still count for a liveness policy. An 8 KiB launch cap is the
defensible starting point. The work occurs on the parser worker, so it does not
consume the Flutter frame budget directly; projection, serialization, message
transfer, and paint must still be added to the end-to-end gate.

## Maintenance surface

### Bounded inline hook: plausibly surgical

Comrak's private parser module already contains public-within-crate `Subject`,
`RefMap`, `FootnoteDefs`, `parse_inline`, `process_emphasis`, and
`clear_brackets`. A sibling `inline_fragment` module can:

1. accept one bounded logical leaf plus line/source mapping and a reference
   resolver snapshot;
2. construct the temporary paragraph/heading AST and `RefMap`;
3. run the existing inline algorithm unchanged;
4. convert the bounded arena subtree directly to Flark's compact spans; and
5. drop the arena before returning.

The expected upstream integration is a few re-export/module lines plus roughly
150–300 lines of Flark-owned wrapper/conversion code. It can keep Comrak's
owned `String`, arena nodes, pointer delimiters, and synchronous emphasis pass
because the hard leaf cap bounds all of them. This estimate still needs the
dedicated implementation/differential gate; it is not a completed patch.

### Definitive block spine: not surgical

Comrak 0.54's block control flow is about 2,951 lines in `parser/mod.rs`, with a
further 392-line table module. Its arena `AstNode` is documented as 176 bytes on
64-bit builds before child strings/vectors. The existing derived subset already
needs 2,117 lines to correspond to 718 upstream block-line spans and still
omits exact tightness, tables, HTML, footnotes, references, ATX/indented code,
and several extensions.

Therefore the hybrid still owns/refactors most of the block grammar's runtime
representation. It is materially smaller than the fully owned plan because it
does not need to own the approximately 2,721-line inline algorithm, its packed
delimiter/bracket representation, or cooperative resumption inside an
unbounded leaf. It is not the earlier 53-existing-line incremental adapter.

## Exact solution versus SLA change

The following can be exact with this architecture:

- arbitrarily large documents made of bounded inline leaves;
- giant lists whose individual item paragraphs are bounded;
- giant tables whose individual cells are bounded, once the exact table spine
  exists;
- giant fences, indented code, and HTML raw blocks as source-backed structural
  regions; and
- distant references/footnotes through block-emitted indexes and inline lookup
  dependencies.

The explicit SLA change is one inline-bearing leaf over the cap. The editor
still shows and edits exact source immediately, but does not promise complete
bold/emphasis/link/code-span interpretation throughout that leaf. A structural
edit whose boundary effect is not narrowly certified must resume the exact
block machine until convergence; it cannot use a heuristic local classifier.

If that degradation is unacceptable, the hybrid is ruled out and the owned
resumable inline machine is justified. If it is acceptable, the hybrid has a
meaningful maintenance advantage and should be the next direction to prove.

## Required next gate

1. Implement the real Comrak inline-fragment wrapper with source mapping,
   resolver injection, compact output, CommonMark/GFM differential tests, and
   8 KiB native/WASM latency/RSS measurements.
2. Extend one exact source-backed block-spine slice through list tightness,
   GFM table cells, HTML terminators, definitions, and footnotes. It must emit
   structure, restart state, and dependency facts from one transition.
3. Differential-test boundary convergence after adversarial edits, including
   the loose-list and HTML suffix failures found here.
4. Replace flat edited strings and `Vec<Region>` receipts with the chosen
   persistent source and packed stable-ID indexes; prove a one-leaf delta rather
   than the prototype's fingerprint replacement.
5. Measure parser-to-paint liveness on physical native and web targets.

## Reproduction

```sh
cargo test
cargo test --release
cargo clippy --all-targets -- -D warnings

/usr/bin/time -l target/release/bounded_bench many-small 100 65536
/usr/bin/time -l target/release/bounded_bench paragraph 10 65536
/usr/bin/time -l target/release/bounded_bench fence 10 65536
/usr/bin/time -l target/release/bounded_bench list 10 65536
/usr/bin/time -l target/release/bounded_bench table 10 65536

/usr/bin/time -l target/release/stock_bench paragraph 10
/usr/bin/time -l target/release/stock_bench fence 10
/usr/bin/time -l target/release/stock_bench list 10
/usr/bin/time -l target/release/stock_bench table 10

target/release/inline_cap_bench dense 8192 500
target/release/inline_cap_bench unmatched 8192 500
target/release/inline_cap_bench links 8192 500
```

Debug tests pass 8 rows; release tests pass all 10 rows, including the two
fence cases gated out of debug because they currently trip the derived probe's
one-byte-tick assertion. Strict Clippy and formatting checks pass.

