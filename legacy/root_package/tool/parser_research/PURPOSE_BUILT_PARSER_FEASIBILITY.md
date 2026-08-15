# Purpose-built incremental Markdown parser feasibility

Status: decision evidence, 2026-07-14. This evaluates a Flark-owned parser as a
real alternative to the maintained Comrak fork in RFC 023. It does not claim
that the included 357-line kernel is a CommonMark parser.

## Decision

A purpose-built parser is technically viable and more credible than the first
parser bakeoff implied. Markdown's incremental mechanics do not require a large
general parser framework, the syntax specifications are stable, and several
mature implementations show that a complete core is measured in thousands—not
hundreds of thousands—of lines.

It does **not** yet displace the Comrak fork for launch. The extracted Comrak
path already has exact Flark semantics, 10,000-edit differential proof, a
53-addition/5-deletion existing-file integration surface, native/WASM builds,
and 16-microsecond p95 external apply time. A new parser cannot create a
meaningful user-visible speed improvement over that ordinary-edit budget. Its
case is architectural ownership: first-class persistence, checkpoints,
cancellation, and editor deltas without adapting a mutable arena parser.

The next decision should therefore be a bounded **conformance-kernel gate**, not
an immediate clean-room rewrite and not an automatic commitment to Comrak. A
Flark-owned Rust parser becomes the preferred direction only if it can reach
full CommonMark block/inline equivalence and the five enabled GFM extensions
without recreating a larger or less legible version of Comrak.

## What “our own parser” should mean

It should not mean inventing Markdown rules from memory. The practical route is
a Flark-owned Rust implementation that:

- uses the normative CommonMark/GFM examples as executable requirements;
- derives proven block and delimiter algorithms from BSD/MIT prior art with
  attribution where useful;
- adopts Lezer-style fragment reuse and explicit parser-context hashes;
- represents source, checkpoints, syntax chunks, reference symbols, and inline
  leaves persistently from the start;
- emits Flark's versioned editor delta directly instead of constructing a
  general mutable HTML-oriented AST;
- keeps Comrak/cmark-gfm as development and CI oracles, never as a second live
  grammar after cutover.

This is a purpose-built implementation, not necessarily a clean-room one.
CommonMark's reference cmark implementation is BSD licensed, Comrak is BSD
licensed, and Lezer Markdown is MIT licensed. Any derived implementation must
retain the appropriate notices and a provenance inventory.

## Scope Flark actually needs

The current bridge enables CommonMark plus exactly five extensions:

1. GFM autolinks;
2. strikethrough;
3. tables;
4. tag filtering;
5. task-list items.

Flark does not need Comrak's HTML renderers, CommonMark serializer, syntax
highlighting plugins, alerts, description lists, footnotes, math, shortcodes,
wikilinks, or Phoenix HEEx parsing in the parser authority. That substantially
narrows a purpose-built core.

It does still need all difficult CommonMark behavior:

- nested blockquote/list continuation and lazy lines;
- tabs, indentation, tight/loose list propagation, and source positions;
- seven HTML block forms and inline HTML;
- fenced/indented code and unclosed constructs;
- delimiter-run emphasis/strong rules, including the rule of three;
- bracket stacks, links, images, code spans, escapes, entities, and autolinks;
- normalized reference definitions, duplicate first-wins behavior, and missing
  reference dependencies;
- incomplete and malformed input at every keystroke;
- bounded behavior on adversarial nesting, delimiters, tables, and long lines.

The checked-in fixtures contain 652 current CommonMark examples and 670
cmark-gfm main examples. The largest semantic concentrations are 132 emphasis
and strong examples, 90 links, 74 list/list-item examples, and 64 block/inline
HTML examples. Those are a better estimate of the risk than counting syntax
features by name.

## Relevant implementation scale

Line counts below include comments and blanks and are only scale indicators.
They were measured from pinned primary-source checkouts:

| Implementation | Snapshot | Relevant implementation lines | Architectural lesson |
| --- | --- | ---: | --- |
| cmark | `9562204` | 4,380 for blocks, inlines, references, scanner source, parser/node core | Complete algorithms fit in a compact two-phase core; generated scanner and renderers excluded |
| cmark-gfm | `499789b` | 5,238 core plus 2,837 extension C/H lines | GFM and extension machinery add real but bounded surface |
| MD4C | `65c6c9d` | 7,718 parser/API lines | A fast callback-oriented complete parser can remain one compact native core |
| Lezer Markdown | `30fe1b1` | 2,313 TypeScript source plus roughly 4,300 test lines | Fragment reuse, context hashes, resumable parsing, and compact trees do not require a large engine |
| `markdown-rs` | `1506572` | 14,872 construct lines plus 2,151 tokenizer/state/subtokenize/resolve lines | Explicit state machines are modular but can become broad and allocation-heavy |
| Comrak 0.54 | `172c2ee` | 8,569 parser lines before the Flark module | Current semantic authority includes many extensions Flark does not need |
| Extracted Flark incremental module | `ac299c7` | 3,316 lines including colocated tests | Persistent source/tree/reference machinery is already a material fraction of a custom core |

Lezer is especially relevant. Its Markdown parser is custom rather than an LR
grammar. It advances incrementally, accepts reusable tree fragments, hashes the
surrounding block context before reusing a fragment, and exposes stop points.
Its known semantic shortcut is also precisely identified: it parses reference
link candidates without checking whether a matching definition exists.

Flark's already-proved reference model addresses that shortcut without a second
grammar. Inline leaves retain normalized labels and lookup misses; a persistent
definition index resolves candidates and invalidates only dependent leaves when
definition presence changes. This suggests a Rust design inspired by Lezer is
plausible, while copying Lezer unchanged is not.

## Executable kernel result

`src/bin/purpose_built_parser_spike.rs` is a 357-line dependency-free Rust
kernel. It intentionally recognizes only representative block states: ordinary
lines, headings, lists, quotes, definitions, table candidates, fenced blocks,
and HTML comments. It provides:

- explicit clonable parser state at every line boundary;
- state and unchanged-line-hash convergence;
- exact suffix checkpoint reuse for same-length local edits;
- a full self-parse oracle;
- adversarial propagation for an opened HTML comment and a changed fence;
- 10,000-edit locality measurement over approximately 1 MB.

The release run measured:

- initial parse: 2.124 milliseconds;
- apply p50/p95/p99: 208/292/333 nanoseconds;
- maximum apply: 71.0 microseconds;
- reparsed bytes p95/max: 46/46;
- reparsed lines p95: one;
- 47,182 naïve per-line checkpoints occupied 1,887,280 bytes by
  `size_of::<Checkpoint>()`—too expensive as the production density;
- all three targeted state/convergence tests passed.

These numbers are not a parser bakeoff: the kernel omits most of CommonMark and
uses fixed-length byte edits to isolate parser-state work from the already-proved
persistent source tree. They prove only that an explicit, Flark-shaped
checkpoint engine has negligible machinery overhead. Grammar correctness and
incremental inline output dominate the real program.

The checkpoint memory result is a useful negative constraint. A production
engine should keep compact state only at selected structural/byte intervals and
store per-line hashes or summaries in denser aggregate leaves. “Checkpointable
at every line” must not become “one heap-heavy checkpoint object per line.”

Run it with:

```sh
cargo test --release --bin purpose_built_parser_spike -- --nocapture
cargo run --release --bin purpose_built_parser_spike
```

## Proposed architecture

### Source and edit boundary

- Consume the existing v3 mirrored UTF-8 rope through a small `Input` trait;
  never flatten the document.
- Accept revision/hash-scoped edit batches and publish exactly one atomic parser
  delta per accepted revision.
- Keep checkpoint and node offsets segment-relative so suffix reuse never
  rewrites global positions.

### Block phase

- Use an explicit container stack with compact value-semantic state: container
  kind, marker/indent data, blank/lazy flags, open leaf state, pending paragraph
  facts, and extension state.
- Store checkpoints at syntax-safe line boundaries and periodically inside
  large containers. Each checkpoint has a stable state fingerprint and context
  summary.
- Reparse from the predecessor checkpoint until source alignment, parser state,
  and semantic summaries converge; then reuse the persistent suffix.
- Construct immutable source-relative block chunks directly. Avoid a mutable
  arena and whole-document finalization.
- Express list tightness, table column shape, and other delayed properties as
  composable aggregates rather than ancestor rescans.

### Inline phase

- Parse each changed inline-bearing leaf independently with an explicit
  delimiter/bracket stack.
- Store delimiter runs and nodes in compact persistent arrays; giant paragraphs
  receive internal checkpoints rather than one monolithic inline pass.
- Record every normalized reference-label dependency, including misses.
- Resolve reference candidates through a persistent first-definition-wins
  symbol index. Presence changes reparse dependent leaves; value-only changes
  update indirection without rebuilding them.
- Emit delimiter/source mapping facts while parsing so projection never
  re-discovers Markdown syntax in Dart.

### Output and scheduling

- Return the existing versioned binary delta: replaced chunk interval, stable-ID
  chunks/nodes, inline styles and markup ranges, reference changes, and local
  projection facts.
- Make `advance(work_budget)` a native operation. Check byte/node/time budget at
  line, delimiter, and output-chunk boundaries.
- Support cancellation and revision supersession without unwinding a borrowed
  arena.
- Use the same safe Rust core on native and `wasm32-unknown-unknown`.

## Work program and gates

### Gate A — complete block authority

Implement every CommonMark block construct, source range, and Flark table/task
extension on the persistent block engine. Compare canonical block signatures to
Comrak/cmark-gfm for all 652/670 fixtures plus Flark's behavioral corpus.

Pass requires zero unexplained structural or source-range differences,
checkpoint/full equivalence after adversarial edit sequences, and internal
checkpoints for oversized lists, tables, code/HTML blocks, and paragraphs.

### Gate B — complete inline authority

Implement code spans, escapes/entities, raw HTML/autolinks, delimiter emphasis
and strikethrough, links/images, and reference candidates. Add the persistent
reference index and dependency invalidation.

Pass requires full canonical equivalence for the Flark profile, including all
132 emphasis/strong and 90 link examples, sequential-edit differential fuzzing,
and no traversal-order global resource state.

### Gate C — production service

Integrate the rope, stable-ID chunks, binary delta, budgets, cancellation,
supersession, native/WASM ABI, and parser-to-paint harness.

Pass requires the same input-to-photon, memory, protocol-corruption, and
physical-device gates as the Comrak fork. Comrak runs only as an oracle in CI
and development tools; it is not shipped as a simultaneous live parser.

## Effort and ownership estimate

This is not a few-week parser swap. Existing implementation sizes and the
conformance distribution suggest approximately 6,000–10,000 lines of focused
production Rust plus 4,000–8,000 lines of fixtures, adapters, fuzz targets, and
state-transition tests. Reusing licensed algorithms can reduce invention but
not the certification burden.

For planning, treat it as roughly four to eight months of concentrated work for
an experienced single owner to reach a production candidate, with confidence
continuing to accrue after that. Parallel help can shorten elapsed time, but one
named maintainer still needs to understand the full block, inline, reference,
and incremental state machines. This estimate is deliberately a range; Gates A
and B should replace it with measured remaining work.

The long-term maintenance profile may be attractive. CommonMark's current
0.31.2 specification has been stable since January 2024, and Flark's enabled GFM
profile is small. Owning the parser exchanges recurring upstream merge and
internal-adaptation work for direct responsibility for correctness, security,
and any future syntax additions.

## Recommendation

Do not abandon the extracted Comrak fork: its maintenance-surface concern has
passed, and it is the lower-risk launch route today. Also do not dismiss the
purpose-built parser: its architecture could be materially cleaner, and the
bounded grammar/core sizes make it a legitimate strategic option.

Before substantial parser-specific Flutter integration, fund Gate A as the
final parser-choice experiment. Start from a Rust port/adaptation of the compact
Lezer/cmark block algorithms, build directly on the v3 rope and persistent
chunk contracts, and retain exact reference occurrences from day one. If Gate A
reaches complete block/source-range parity with a smaller, clearer core, proceed
to Gate B and prefer the owned parser. If it becomes a compatibility chase or
exceeds the expected core size, stop and ship the maintained Comrak fork.

This avoids a permanent dual parser. During the experiment Comrak is an oracle;
at runtime there is always exactly one grammar authority.

## Primary references

- [CommonMark 0.31.2 specification](https://spec.commonmark.org/)
- [GitHub Flavored Markdown specification](https://github.github.io/gfm/)
- [cmark reference implementation](https://github.com/commonmark/cmark)
- [cmark-gfm](https://github.com/github/cmark-gfm)
- [Lezer Markdown](https://code.haverbeke.berlin/lezer/markdown)
- [MD4C](https://github.com/mity/md4c)
