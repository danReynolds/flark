# Checkpoint/restart state micro-prototype

## Verdict

**Narrow pass for the incremental mechanism; not a parser or Gate B pass.**

This disposable crate demonstrates that a parser can keep an O(1) checkpoint
root at every immutable source page, restart before an edit, advance scanning
and resolution under fuel, and stop parsing when both the exact source suffix
and the exact parser state converge. At that point the old lexical and semantic
fact pages are attached without scanning the old source suffix again.

The useful result is not that this toy grammar is fast. It is that facts may
cross the convergence boundary safely when:

1. every fact endpoint uses a stable `(source_page_id, offset)` anchor;
2. the complete parser state needed to derive suffix facts is checkpointed;
3. the remaining source is the same immutable tail object; and
4. state equality is exact, not a hash comparison.

The tests also show the conservative counterpart: rebuilding the page that
owns an open delimiter changes its anchor, so state does not converge and the
later closing fact is reparsed rather than reused incorrectly.

## What is implemented

- Immutable 4 KiB source pages and persistent source-tail identities. An edit
  rebuilds the touched page range and prefix tail nodes while retaining the
  untouched suffix tail.
- A 24-byte `ParserState` checkpoint containing an `Arc` root, depth, and
  digest. The delimiter stack is an immutable cons list, so recording a
  checkpoint is O(1).
- A resumable exact-state comparator. Depth/digest are fast rejects only;
  equal digests still require exact frame comparison or shared-root identity.
  Deep equality consumes one frame per work unit. A forced digest-collision
  test proves the hash is not accepted as equality.
- A toy bracket grammar for `[]`, `()`, `{}`, and `*`. A closer resolves the
  stack one frame per work unit and emits pair, abandoned-open, or
  unmatched-close facts. Unclosed facts are emitted at EOF one frame per work
  unit.
- Stable lexical and semantic facts. Pair facts can begin on a page before an
  edit and close in an attached suffix page.
- Fixed-capacity linked fact chunks. Cross-page resolution and EOF output do
  not grow/copy a whole-document `Vec` in one parser step. Page tables and the
  tail index are pre-sized. Source scanning, exact comparison, close
  resolution, page sealing, suffix metadata attachment, EOF finalization, and
  EOF sealing are all explicit fuelled phases.
- Byte-for-byte canonical fact comparison and exact checkpoint comparison
  between resumed and clean parses across deterministic edits, 250 random edit
  revisions, spanning-fact cases, and open-state cases.

## Reproducible receipts

Run on macOS in release mode on 2026-07-14:

```text
$ cargo test --release
running 9 tests
test result: ok. 9 passed; 0 failed

$ cargo clippy --all-targets -- -D warnings
Finished `dev` profile
```

The receipt binary uses 4,096 work units per tick. Its balanced 10 MiB input
contains one balanced pair per source page. A one-byte edit is made in the
middle:

```text
balanced_clean: bytes=10485760 pages=2560 elapsed_us=25029
  scanned_bytes=10485760 scanned_pages=2560 ticks=2564
  retained_graph_bytes=11792408

balanced_resumed: bytes=10485760 pages=2560 elapsed_us=97
  scanned_bytes=4096 scanned_pages=1
  prefix_pages=1280 attached_suffix_pages=1279
  convergence_page=Some(1281) ticks=2
  retained_graph_bytes=11792408

balanced_exact_clean_match=true
maximum resident set size: 29818880 bytes
```

The 10 MiB open input has one `[` at the start of every source page. Removing
the first opener changes the state at every later boundary, so convergence
must not occur:

```text
open_clean: bytes=10485760 pages=2560 elapsed_us=22851
  scanned_bytes=10485760 scanned_pages=2560 eof_frames=2560
  retained_graph_bytes=11506328 retained_stack_bytes=102400

open_resumed: bytes=10485760 pages=2560 elapsed_us=21321
  scanned_bytes=10485760 scanned_pages=2560
  attached_suffix_pages=0 convergence_page=None eof_frames=2559
  retained_graph_bytes=11506288 retained_stack_bytes=102360

open_exact_clean_match=true
maximum resident set size: 30031872 bytes
```

The process-RSS runs retain the old, resumed, and clean results long enough to
compare them; `retained_graph_bytes` is a deterministic estimate for one live
result and excludes allocator/`Arc` bookkeeping. Timing is evidence of shape,
not a benchmark commitment.

Reproduce with:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo build --release --bin checkpoint_restart_receipts
/usr/bin/time -l target/release/checkpoint_restart_receipts balanced
/usr/bin/time -l target/release/checkpoint_restart_receipts open
```

## Assumptions challenged

### A hash is not a checkpoint identity

Hash equality alone would make suffix reuse probabilistically wrong. The
prototype uses source-tail pointer identity and a fuelled structural state
comparison. Digests only reject obvious mismatches. Shared persistent roots
make the common unchanged-state case O(1).

### Empty state is not required for convergence

The spanning-fact test edits a middle page while `[` remains open from an
earlier page. The entry and exit stack roots remain exactly the same, so the
later `]` page and its pair fact are attached. This is the important advantage
over requiring a blank-line or empty-stack restart boundary.

### Incremental work still has a worst case

Removing the first opener from the all-open document correctly scans the
remaining 10 MiB and finalizes every remaining open frame. Incrementality
bounds work when state converges; it cannot promise locality for an edit whose
semantic effect genuinely reaches EOF. A worker, supersession, and fuel remain
necessary.

### “Fuelled loop” is not enough if containers hide large copies

The first implementation used growable whole-output vectors. That allowed a
single `push` during EOF or cross-page resolution to reallocate and copy an
unbounded number of facts. It was replaced with 128-record linked chunks, and
page/checkpoint tables are pre-sized. This is why the final claim is about
bounded parser transitions rather than merely having an `advance(fuel)` API.

## Limits and remaining composition gaps

This is intentionally not evidence that the complete Flark parser is solved:

- `StackFrame` is 40 bytes. The 10 MiB open stress has only one opener per
  4 KiB page (2,560 frames), not one opener per byte. A dense all-opener input
  with this representation would be hundreds of megabytes. A shipping inline
  parser still needs packed delimiter runs/records and its own dense-input
  memory gate.
- Prefix `PageRecord`s and converged suffix `PageRecord`s are copied/attached
  one page per fuelled transition (1,280 + 1,279 transitions in the balanced
  receipt). The source editor also rebuilds prefix tail nodes. A shipping
  result/checkpoint index should use a persistent balanced sequence so prefix
  and suffix roots compose in O(log pages), rather than merely avoiding source
  reparse.
- Fact storage is bounded linked chunks, but each fact is still an unpacked
  Rust enum/struct. This does not prove the packed output memory target.
- Stable anchors are preserved, but this crate does not implement the
  production anchor-to-current-byte-offset index or delta protocol.
- The source edit locator and source-tail construction occur outside measured
  parser fuel. A real rope/piece-tree edit index must provide those operations
  with its own bounds.
- Dropping long `Arc` chains and reclaiming old revisions is not scheduled by
  parser fuel. Production needs bounded/iterative reclamation or an epoch
  strategy so cleanup cannot become a latency spike.
- The exact state contains only this toy delimiter stack. Real Markdown state
  includes block containers, lazy continuations, delimiter/bracket metadata,
  references, HTML scanners, tables, and profile flags. Suffix reuse is sound
  only after proving that the production checkpoint is complete for every
  emitted fact.
- There is no Unicode, CommonMark/GFM semantics, source projection, layout,
  worker transport, WASM boundary, or UI adoption path here.

Accordingly, this result supports continuing with immutable checkpoints plus
exact state/source convergence as the incremental spine. It does **not** select
Pulldown, Comrak, or a clean-room grammar, and it does **not** claim Gate B.
