# Gate B inline/reference harness

This crate is a donor-neutral executable acceptance contract for RFC 023's
inline and reference phase. It contains no Markdown parser and makes no claim
that a current prototype passes. A candidate implements `GateBEngine`; the
runner checks the candidate against pinned syntax/profile fixtures and against
its own clean parse after every intermediate editor revision.

The normative corpus is CommonMark 0.31.2's inline, code-span, link, image,
autolink, reference-definition, escape, entity, raw-HTML, break, and textual
sections plus only GFM 0.29's strikethrough, extended-autolink, and disallowed
raw-HTML (tagfilter) sections.
Fixture JSON is the authority. Donor renderers are differential peers, not
oracles.

The product profile is explicit:

- GFM bare URL/email autolinks and strikethrough are enabled;
- footnote-shaped shortcuts and definitions remain ordinary visible source;
- GitHub alert labels remain ordinary blockquote text;
- raw HTML is classified for styling but remains source-visible and is never
  an executable editor rendering; and
- bare autolinks have no hidden marker ranges, while angle and bracket syntax
  exposes its exact physical markers.

The executable contract additionally requires:

- segmented physical-source maps through stripped quote/list prefixes and
  virtual tab/line normalization, with direct facts, semantic parents, and
  exact marker ranges;
- candidate-clean equivalence at every scalar-safe typing and erasing revision;
- byte and transition fuel no larger than 4 KiB per poll, including a 10 MiB
  token-dense single leaf;
- cancellation and supersession so a stale revision can never commit after a
  newer edit is accepted;
- coalesced plain-text runs and bounded auxiliary/output memory rather than an
  object per scalar, byte, or soft break;
- exact non-overlapping peak-live accounting: shared source backing, committed
  output, pending output, checkpoints, block context, lexical tape, resolution
  stacks, history, and scratch must reconcile to one total while work is live;
- a 96 MiB total kill ceiling for 10 MiB adversarial leaves, including a
  24 MiB lexical-tape sub-cap that rejects 12-16 byte records per source byte;
- stable replayable deltas whose contents, not a global snapshot replacement,
  reconstruct committed ID maps and leaf order through compact splices, while
  immutable output-sequence roots replay changed fact order lazily;
- symbol indirection for value-only definition edits, plus retained unresolved
  dependencies and a 5,000-distinct-leaf defined-to-undefined-to-defined lane;
- compact persistent output-sequence/dependency-generation root adoption for
  global recognition changes, so Dart receives no synchronous 5,000-ID list
  and lazily requests only visible leaf facts from the new root; and
- zero grammar-sensitive side scans and zero general batch-tree materialization
  on `begin_edit`, `supersede_edit`, `poll`, and `commit`.

`open`, `snapshot`, fixture HTML, and `clean_snapshot` are test-only batch
views. They are not liveness evidence. Production work begins at `begin_edit`;
all grammar work must occur in fuelled `poll` calls. Every phase returns both a
receipt and an event trace, and the harness reconciles the two. Final acceptance
must still inspect the implementation and measure wall time, allocator traffic,
and RSS in isolated subprocesses: a candidate can lie through any Rust adapter,
so self-reported counters alone never constitute proof.

The owned `String`s in `Snapshot` and `SegmentedText` are test-only
materializations for exact comparison. They are not a permitted production
storage strategy and are excluded from candidate resource receipts; production
state must retain source pieces and compact virtual descriptors instead.

The pinned corpus currently contains 398 normative fixtures. The 11 edit
histories expand to 687 scalar-safe intermediate revisions. The reference
presence lane uses 5,000 distinct dependent leaves in each direction; it must
yield during re-resolution and replay a delta no larger than 64 KiB without
enumerating the affected facts, uses, dependencies, or leaf IDs.

Run the contract self-tests and fixture pin with:

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
```

The tests intentionally exercise validators with dishonest mock receipts,
global snapshot deltas, copied reference destinations, discarded unresolved
dependencies, enumerated multi-leaf fanout, per-byte text facts, bad prefix
mappings, stale commits, and hidden work. Passing these self-tests only means
the gate is capable of rejecting those cheats.

Validation receipt on 2026-07-14: 16 contract self-tests and two corpus/history
tests pass in release mode; formatting and Clippy with warnings denied pass.
