# Packed inline-state micro-prototype

This disposable crate tests whether the remaining inline-parser risk is
intrinsic to incremental parsing or mostly a consequence of donor-owned trees
and per-token structs. It intentionally implements a **toy pairing grammar**,
not CommonMark or GFM.

## Executable shape

- immutable `Arc<str>` source segments and a cursor that crosses them without
  flattening;
- source-relative lexical pages with one-byte dense delimiter/bracket records,
  varint escapes, and one record per delimiter run;
- reverse-varint delimiter/bracket stacks storing packed event ordinals and
  source positions (two payload bytes per dense all-open bracket);
- a packed pending-match tape and source-relative packed fact pages;
- fixed 4 KiB packed-byte chunks with pre-sized pointer indexes, so appending
  and sealing never copy a document-sized flat buffer;
- fuel-charged scan, lexical-page seal, resolve, emit, fact-page seal, and EOF
  transitions; source/output digests accumulate while bytes are consumed, so
  EOF is constant work rather than a hidden whole-document hash;
- cancellation checked before every transition, including resolution and page
  sealing/EOF;
- immutable checkpoints carrying packed open-stack roots plus parser-state and
  document fingerprints;
- convergence checks at lexical page boundaries and `Arc` reuse of equivalent
  lexical/fact suffix pages **after a clean candidate parse**;
- conservative live-memory accounting that explicitly includes a retained old
  checkpoint plus the candidate source, lexical tape, pending matches, stacks,
  and fact output.

The stress binary keeps the old committed checkpoint alive, inserts one plain
byte at the midpoint, builds a candidate, and reports the combined accounted
peak and suffix adoption. Build it first, then use `/usr/bin/time -l` on macOS
so Cargo/compiler memory is not mixed into the receipt:

```sh
cargo build --release --bin packed_inline_stress
/usr/bin/time -l target/release/packed_inline_stress alternating
/usr/bin/time -l target/release/packed_inline_stress emphasis
/usr/bin/time -l target/release/packed_inline_stress brackets
/usr/bin/time -l target/release/packed_inline_stress run
```

## What this can and cannot decide

Passing the representation stress shows, within this spike's explicit 16 MiB
ceiling, that segmented input, fixed-size append work, fuelled finalization,
compact tapes/stacks/facts, cancellation, and immutable suffix payloads can
coexist without a general AST or a `u32`/struct per lexical event. It makes an
owned Pulldown-derived resolver mechanically plausible.

It **does not pass Gate B**. The pairing logic is deliberately wrong for real
Markdown. It does not implement delimiter flanking/rules-of-three, code spans,
links/images, reference normalization/dependencies, escapes/entities, raw HTML,
GFM bare autolinks/strikethrough, or table/task-list seams. Candidate parsing is
still clean-from-start. Although the checkpoint retains the packed open-stack
bytes, it has no restart API or persistent prefix sharing; therefore this does
**not** prove restart from a checkpoint, early suffix attachment, or
convergence with unresolved open state. The demonstrated `Arc` adoption happens
only after the candidate is complete. Hash matches would require collision-safe
confirmation in production.
See `PRODUCTION_GAPS` for the executable list.

## Validation

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Stress receipts are added here only after running the release binary; estimates
or debug-build numbers should not be treated as evidence.

## 10 MiB release receipts (2026-07-14, Apple Silicon/macOS)

Each row is a second parse with the old committed checkpoint retained and a
plain byte inserted at the midpoint. `Accounted peak` deliberately counts the
old and candidate source in full even when their `Arc` segments are shared, so
it is conservative. RSS is `/usr/bin/time -l` maximum resident set size.

| adversary | lexical records / payload | pending / facts | packed stack peak (payload / capacity) | accounted peak | external RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| `*_` repetition | 10,485,760 / 10,485,760 B (1.000 B/event) | 10,485,760 / 10,485,760 B | 16 / 1,245,232 B | 76,809,393 B | 68,714,496 B |
| `*a*` repetition | 3,495,254 / 6,990,507 B (2.000 B/event) | 3,495,254 / 3,495,254 B | 8 / 1,237,040 B | 48,863,121 B | 40,468,480 B |
| all-open `[` | 10,485,760 / 10,485,760 B (1.000 B/event) | 0 / 0 B | 20,971,520 / 22,200,368 B | 86,705,329 B | 78,495,744 B |
| one giant `*` run | 2 / 10 B | 5 / 5 B | 2 / 1,237,040 B | 23,711,177 B | 15,089,664 B |

The dense all-open stack payload is exactly 2 bytes/open (one reverse-varint
ordinal plus one reverse-varint source position). The larger allocated number
includes fixed pointer indexes for all six ordinal/position lanes, even when
five are empty. The combined peak retains those actual stack roots in the old
checkpoint; the all-open row is therefore the worst case and still remains
below the 96 MiB experiment threshold. A giant old source is one logical
lexical record across all source segments; the midpoint insertion splits the
candidate into two runs, which is why its row reports two records. The `*a*`
row reaches 2 bytes per record because adjacent repetitions create `**` runs
with an extra run-length varint.

The balanced `*_` edit converged with equal final state and reused 80 lexical
suffix pages (5,242,880 payload bytes) plus 1,280 fact pages (10,485,760 bytes).
The other adversaries expose the limit of the current convergence key:
absolute-position open state can prevent lexical suffix adoption even when
immutable page payloads match. That is a design item, not a hidden success.

The slowest case (`*_`) took about 1.18-1.19 s per clean 10 MiB parse in this
deliberately simple chunked implementation. This is a representation and
scheduling receipt, not a shipping throughput result; donor-quality scanning
and a real restart path still need their own performance gate.

Validation receipt: 13 tests passed; `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings` passed. The tests include exact
fuel=1 versus fuel=4096 equivalence, cancellation during resolution, fact-page
sealing and pre-EOF commit, two-byte dense packed-open stacks, run compaction,
source-segmentation independence, exact boundary/EOF edits, and local-edit
suffix reuse.
