# Source-bound streaming ledger: Stage 1 result

Status: **single- and multi-line source spine proven; composed parser integration HOLD**, 2026-07-16.

## Result

The source-ledger contract fits the selected worker-owned runtime authority
model, but the initial “one consuming cursor does recognition and emission”
model does not. A giant Setext, table, or reference-definition candidate may
need to be scanned to its decision boundary before the prior terminator or
branch policy is known. Retaining every atom until that decision would make
parser memory proportional to the candidate line.

The corrected model has two cursor roles over the same immutable Crop root and
one decoder implementation:

1. A non-authoritative recognition cursor advances under fuel and returns only
   query atoms plus opaque checkpoints. Neither is accepted by a claim API.
2. After a grammar branch commits, the authoritative cursor replays the exact
   source range through the same `AtomDecoder` kernel. Only this pass mints
   source boundaries and typed Tab, NUL, CRLF, and lone-CR atom capabilities.
3. Semantic owner and logical-target handles still derive from the existing
   `LiveCandidateEpoch`, `ArenaBuildId`, monotonic permits, and live open path.
   No source-root nonce or second identity vocabulary was introduced.

This is two source-reader roles, not two Markdown parsers. The grammar scanner
that recognizes a table/reference/Setext branch must also own the typed replay
recipe for that same committed branch; the sink must not classify Markdown.

## Current executable proof

`v3_runtime_slice/src/source_bound_ledger.rs` and
`tests/source_bound_ledger.rs` prove the following mechanics:

- raw byte mode cannot be mixed with ledger mode after the cursor advances;
- both roles are bound to the same candidate source descriptor and arena build;
- one shared decoder derives scalar boundaries and byte/UTF-16 metrics under
  tiny fuel with at most four decoder bytes and one CR lookahead byte;
- a recognized physical line installs one O(1) replay expectation and blocks
  further recognition until authoritative replay finishes it;
- a multi-line recognition candidate installs one O(1) range expectation with
  no retained line/atom queue; its scanner-family kind does not classify the
  eventual source claims;
- a malformed reference-definition-like candidate replays exactly as ordinary
  paragraph content and terminals, proving that recognition does not grant
  semantic or span authority;
- replay compares exact descriptor, build, source range, metric, line ending,
  atom count, and a debug-only sequence checksum; exactness rests on the same
  immutable root/range and shared deterministic decoder, never on the digest;
- claims cannot supply raw metrics, source ranges, root specifications,
  `BlockId`, or `CoverageId`;
- typed atomic transforms require the exact source-minted atom capability;
- claims are ordered, disjoint, and total before a line can finish or EOF can
  seal;
- prior terminators may remain pending during recognition lookahead, but later
  claims cannot pass them;
- arbitrarily many adjacent blank lines coalesce in O(1), and the first gap's
  binding-generation ceiling prevents a later-opened block from capturing old
  bytes;
- cancellation and recycled build generations reject stale source, owner, and
  target capabilities;
- cursor scratch and open-path high-water marks are explicit receipts;
- the final source seal independently matches replay-derived UTF-16 totals to
  Crop's O(1) whole-root UTF-16 metric.

## Bounded multi-line recognition contract

Reference definitions can span physical lines, so production cannot queue one
recognized-line receipt per line. The source spine now implements one range
scanner state:

```text
begin_recognition_range(scanner_family)
poll_recognition(fuel) -> query atom | need fuel | physical line boundary | eof
continue_range_line()
finish_recognition_range()
  -> one candidate-owned ExpectedReplayRange

authoritative_poll(fuel)
  -> stream exact source atoms to parser-owned typed actions
  -> compare exact root/build/range/metric/line-count/atom sequence at range end
```

The implemented ledger range state is O(1): start/end offsets, scanner family,
rolling byte/UTF-16 totals, line/atom counts, and diagnostics. It owns no line
`String`, atom `Vec`, cell-cut `Vec`, or queued line receipts. The returned
range receipt is query/debug output; no API accepts it back as source, replay,
claim, reset, or adoption authority.

The grammar automaton and typed action producer are intentionally not in this
module. The real parser must keep their resumable state bounded and reproduce
table cells/reference actions while authoritative atoms stream to the sink.
Any recipe that needs source-proportional cuts fails the remaining composition
gate rather than storing them.

Recognition checkpoints remain non-authoritative observations. A future
restart/fork hook may create another actor-owned query cursor at a certified
Crop scalar boundary, but a raw `(revision, offset)` tuple must never become a
claim, reset, or adoption capability.

## Verification receipts

From `v3_runtime_slice` on 2026-07-16:

- `cargo fmt --all -- --check`: passed;
- `cargo test --test source_bound_ledger`: 8 passed, 0 failed (5.72 s test
  time);
- `cargo test --release --test source_bound_ledger`: 8 passed, 0 failed
  (0.26 s test time);
- `cargo clippy --lib --test source_bound_ledger -- -D warnings`: passed;
- `RUSTC=/Users/dan/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/rustc
  /Users/dan/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/cargo check
  --target wasm32-unknown-unknown`: passed. The default shell's Homebrew 1.93
  compiler could not see rustup's target standard library, so that default
  invocation is not a valid Wasm receipt.

## Immediate HOLD obligations

1. Instrument the real block parser's Setext/table/reference sites with typed,
   resumable recognition-and-replay recipes; the current module proves the
   source spine, not those grammar recipes.
2. Feed validated claims directly into the resumable packed green builder.
   `ValidatedSourceClaim` remains debug/test data and must never be replayed as
   authority. The production ledger must instead yield a non-cloneable
   `ConsumedSourcePiece` without a `CoverageId`; the projection composer mints
   one fresh coverage identity only when it seals an inline run or bounded
   Program chunk. Otherwise dense typed atoms would become one retained green
   event each and defeat the Program-page bound.
3. Issue projection reset edges only after authoritative source consumption
   and projection-safe sink state; recognition checkpoints and debug digests
   are insufficient.
