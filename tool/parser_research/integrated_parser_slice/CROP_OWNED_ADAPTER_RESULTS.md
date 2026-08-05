# Crop-backed owned-parser adapter gate

Date: 2026-07-15. Host: Apple M1 Pro, arm64 macOS 26.2, Rust 1.93.1.

## Verdict

**GO for Crop as the leading source backend behind a Flark-owned snapshot
contract.** This gate does not select the full owned parser over the bounded
Comrak challenger. It shows that choosing the owned/donor-derived parser no
longer implies maintaining Flark's custom persistent source tree.

The result changes the source-side design:

- one outer `CropSnapshotLease` object owns the immutable Rope for a parse job;
- sealed leaves retain a weak binding and packed, root-bound coordinates, not
  a per-leaf source fragment or a second sidecar tree;
- Flark, not Crop, mints revision identities and derives exact edit provenance;
- unchanged-range convergence is authorized by operation lineage plus parser
  state, never by Crop pointer/subtree identity or a hash; and
- one long-lived cursor and reusable chunk scratch survive cooperative polls.

The last point is a real cost, not an implementation detail to hide. Crop's
safe chunk/byte iterators borrow the Rope. A self-contained parser job cannot
store an owned Rope and its borrowed iterator without a self-referential
abstraction. The adapter therefore copies each traversed Crop chunk into one
reused cursor scratch. Block, lexer, and inline telemetry report every chunk
load and copied byte. The measurements below show that this copy does not erase
Crop's edit, memory, or end-to-end advantage on the tested shapes, but a later
safe owning cursor upstream would be valuable.

Each scratch refill currently creates a fresh indexed `byte_slice` and takes
its first chunk. The refill count is exact, but Crop does not expose the number
of private B-tree nodes visited. The adapter therefore reports zero index nodes
rather than inventing a receipt. This seam is O(n log n) in the worst case,
not the final ideal O(n) borrowed iterator. The 100 MiB result shows it is
viable; an upstream owning cursor or outer borrowed-job lifetime is still a
worthwhile production optimization.

## Exactness and integration receipts

The feature-gated adapter runs the existing, real:

`BlockJob -> SharedLexer -> InlineMachine -> OwnedParseJob -> PageArena`

It does not duplicate block or inline grammar. Four focused tests prove:

1. Crop and the custom source produce the same identity-independent summary
   and every canonical output-page byte for CRLF, Unicode, quotes/lists,
   escapes, code spans, emphasis, and strong emphasis.
2. Prefix, interior, Unicode, container-prefix, and suffix edits match a clean
   custom-source parse of the same final bytes.
3. Unchanged suffix reuse maps only through exact edit provenance, rejects an
   edited overlap, and rejects a descriptor from the wrong root.
4. A 10,000-leaf CRLF/Unicode document retains zero source-fragment nodes,
   zero source-fragment handles, zero capture piece runs, zero buffer-handle
   clones, and zero fragment payload copies.

The differential work exposed a pre-existing custom-path defect: the block
prefix classifier could decide after the first byte of a multibyte leading
scalar and commit a capture at a non-scalar boundary. The subsequent capture
trim panicked. Block capture now waits for the same sequential cursor to certify
the end of the scalar. The complete default suite remains green.

Validation:

```text
cargo test --lib --tests
  all non-ignored tests pass

cargo test --lib --tests --features crop-research
  all non-ignored tests pass, including 4 Crop owned-path tests

cargo clippy --all-targets --features crop-research -- -D warnings
  PASS

RUSTC=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  $HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo \
  build --target wasm32-unknown-unknown --features crop-research --lib
  PASS
```

The explicit `RUSTC` is necessary on this machine because `/opt/homebrew/bin`
precedes rustup and Homebrew's otherwise matching compiler lacks the installed
rustup WASM standard library.

## Release performance receipts

The dependency is pinned exactly to audited Crop commit
`d0234ce772eb34c7a3878d4ed57dc864da291cfb`. Each run:

- drops the generator `String` after source construction;
- performs 1,000 deterministic one-byte edits while retaining 64 history
  roots;
- parses through the real block, shared lexer, and cooperative inline machine;
- reports source capture/cursor work; and
- is a separate `/usr/bin/time -l` process for peak RSS.

### Dense live-Markdown leaf

The dense shape is `*a* ` repeated to the requested byte size. Both backends
produced the same span count and canonical digest.

| Backend | Size | Block | Lexer | Inline | Parser total | Edit p50 | Peak RSS | Source payload copies |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Crop | 1 MiB | 11.4 ms | 16.3 ms | 301.9 ms | 329.9 ms | 0.54 us | 9.5 MiB | 3 x 1 MiB |
| Custom | 1 MiB | 13.1 ms | 14.5 ms | 342.3 ms | 370.3 ms | 3.96 us | 9.9 MiB | 0 |
| Crop | 10 MiB | 125.1 ms | 198.2 ms | 3.706 s | 4.036 s | 1.08 us | 63.5 MiB | 3 x 10 MiB |
| Custom | 10 MiB | 152.6 ms | 156.0 ms | 3.744 s | 4.062 s | 6.13 us | 66.8 MiB | 0 |

“3 x” is one sequential scratch copy in each of block, lexer, and inline. It is
not three retained documents: each cursor owns one reusable roughly 1 KiB Crop
chunk scratch. RSS remains lower because Crop avoids the custom per-leaf
fragment tree and uses a smaller source root.

### Large plain leaf

The plain shape has no delimiter runs, so inline exits after the event stream
proves empty. Block and lexer still scan every source byte.

| Backend | Size | Parser total | Edit p50 | Peak RSS | Block source fragment |
|---|---:|---:|---:|---:|---:|
| Crop | 10 MiB | 268.7 ms | 1.67 us | about 25 MiB in earlier low-contention run | 0 nodes / 0 handles |
| Custom | 10 MiB | 302.8 ms | 6.13 us | about 27 MiB in earlier low-contention run | 5,120 nodes / 1 handle |
| Crop | 100 MiB | 3.03 s representative warmed | 4.25 us in that contended run | 209.7 MiB | 0 nodes / 0 handles |
| Custom | 100 MiB | 3.21 s representative warmed | 11.33 us in that contended run | 238.5 MiB | 51,202 nodes / 1 handle |

Wall-time tails and edit maxima varied materially with host preemption, so these
are architecture-direction receipts, not device SLAs. CPU instruction counts,
semantic outputs, and retained-shape counters were stable. Physical-device
parser-to-paint and p99/p999 slice gates remain required.

### Cancellation and retirement

At 100 MiB after scanning 1 MiB:

| Backend | Cancel parser while canonical root remains | Drop final source root |
|---|---:|---:|
| Crop | 4 us | about 20-29 ms warmed |
| Custom | 21 us | about 16-22 ms warmed |

Hot latest-wins cancellation is safe and cheap because it releases only shared
handles. Final last-owner destruction is not cooperatively bounded for either
backend. Crop's private `TinyArc` makes that harder to fix locally. The worker
must retire final roots off the UI thread; a hard bounded Web Worker shutdown
or document-close gate remains open.

## Code-surface result

The temporary dual-backend experiment is intentionally larger than a selected
production backend:

- `crop_source.rs`: 551 lines including identity/provenance, cursor telemetry,
  descriptor-only capture, docs, and errors;
- existing block/frontier/owned/source files: roughly 550 lines of temporary
  enum dispatch, feature gates, metrics, and constructor plumbing;
- focused differential tests: 280 lines; and
- the release receipt binary: 385 lines.

This is nontrivial but it did **not** duplicate grammar or create a sidecar
source. The dual plumbing exists only to run both backends through the same
pipeline in one crate.

A Crop-only production cut should be materially smaller:

- the current custom `source.rs` is 2,081 lines;
- Crop replaces its balancing, buffers, split/concat, anchors, fragments, and
  capture forest with a wrapper whose current fully instrumented version is
  551 lines;
- direct Crop types remove most of the temporary backend enums in `block.rs`,
  `frontier.rs`, and `owned_parse.rs`; and
- operation provenance replaces exact buffer-layout comparison for restart
  mapping.

That projects to a net runtime reduction around 1,000 lines, but it is a
projection, not yet a reviewed deletion patch. Do not merge the dual-backend
shape as the production architecture. If Crop is selected, make a separate
Crop-only cut and judge its actual diff and contracts.

## Remaining gates before selection

1. Integrate the Crop provenance descriptor with the real checkpoint/restart
   state comparison, not only the owned full-parse job.
2. Run Native and Web Worker edit supersession with real transport and verify
   root retirement never occurs on the UI isolate/thread.
3. Run physical-device parser-to-layout-to-paint p50/p99/p999 gates on dense,
   many-leaf, giant-block, paste, IME, and undo traces.
4. Decide whether the reusable indexed chunk copy is acceptable, use an outer
   borrowed job lifetime, or pursue an audited safe owning-cursor abstraction.
5. Make the Crop-only deletion patch and rerun all source/frontier/convergence,
   fuzz, Miri/sanitizer, native, and WASM lanes.

## Reproduction

```sh
cargo build --release --features crop-research --bin crop_owned_gate

./target/release/crop_owned_gate pipeline crop 10 1000
./target/release/crop_owned_gate pipeline custom 10 1000

FLARK_GATE_SHAPE=dense \
  /usr/bin/time -l ./target/release/crop_owned_gate pipeline crop 10 1000
FLARK_GATE_SHAPE=dense \
  /usr/bin/time -l ./target/release/crop_owned_gate pipeline custom 10 1000

/usr/bin/time -l ./target/release/crop_owned_gate pipeline crop 100 1000
/usr/bin/time -l ./target/release/crop_owned_gate pipeline custom 100 1000

/usr/bin/time -l ./target/release/crop_owned_gate cancel crop 100
/usr/bin/time -l ./target/release/crop_owned_gate cancel custom 100
```
