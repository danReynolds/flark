# Source backend bakeoff results

Date: 2026-07-15. Host: Apple M1 Pro, arm64 macOS 26.2, Rust 1.93.1.

## Recommendation: GO, with a revised boundary

Use Crop as the leading source-backend candidate behind a Flark-owned
`SourceSnapshot` contract. Do **not** make Crop's private tree part of the
parser architecture, and do not delete the custom `PersistentSource` yet.

The next gate should adapt the real owned parse job to:

1. one leased Crop root per grammar snapshot;
2. plain `(root_identity, start, end)` leaf descriptors;
3. Flark-minted root identities and operation-derived edit provenance; and
4. a chunk cursor that remains live across cooperative parser polls.

Then run exact parser-output, convergence, cancellation, native/WASM, and real
edit-trace comparisons. If that adapter needs private subtree identities or a
second persistent sidecar source tree to remain exact, stop and retain the
custom source. The source bakeoff alone does not select the complete parser.

The performance evidence makes continued investment in a custom rope the
burden-of-proof position. Crop was roughly 5-8x faster on random one-byte edits,
about 2x faster on the 4 KiB range-scan seam, and used less process peak RSS.
The 100 MiB build was effectively tied when both copied from one complete
`String`.

## Correctness receipts

`cargo test --features custom` passes seven focused tests:

- descriptors reject a snapshot with a different exact root identity;
- the old snapshot remains unchanged after COW edits;
- edit provenance maps only exact unchanged prefix/suffix ranges, including
  both positive and negative byte deltas;
- UTF-16-to-byte round trips remain exact at every scalar boundary;
- CRLF spelling is preserved across snapshots and edits;
- 10,000 leaf descriptors are `Copy`, require no drop, and are at most three
  machine words each; they do not clone or capture a source root; and
- both backends match after 256 deterministic edits to a 1 MiB Unicode/CRLF
  document; and
- both backends match a `String` oracle through 1,024 mixed insertions,
  deletions, Unicode replacements, CRLF replacements, and Markdown-shaped
  replacements, with periodic internal-invariant audits.

The full-scan hashes also match at every measured size:

| Size | Crop | Custom |
|---:|---:|---:|
| 1 MiB | `ce48990ce4db5bb3` | `ce48990ce4db5bb3` |
| 10 MiB | `27993cc501ad874a` | `27993cc501ad874a` |
| 100 MiB | `10f6a4d53a481849` | `10f6a4d53a481849` |

Strict clippy also passes:

```sh
cargo clippy --all-targets --features custom -- -D warnings
```

## Warmed release receipts

Each row is a direct release-binary process after a warm-up run, with 1,000
deterministic random one-byte replacements, 64 retained history roots, and
4 KiB scan polls. Times shown are one representative low-preemption
`/usr/bin/time -l` rerun. The scan hashes every source byte.

| Backend | Size | Build ms | Scan total ms | Scan p50 / p99 us | Edit p50 / p99 us | Owned 4 KiB suffix us | 64-root drop us | Final two-root drop ms | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Crop 0.4.3 | 1 MiB | 0.278 | 1.412 | 5.458 / 5.833 | 0.542 / 2.334 | 1 | 11 | 0.053 | 5.05 |
| Custom | 1 MiB | 0.692 | 2.764 | 10.208 / 29.459 | 4.083 / 8.000 | 11 | 19 | 0.084 | 6.00 |
| Crop 0.4.3 | 10 MiB | 3.036 | 15.276 | 5.583 / 13.833 | 0.833 / 3.458 | 3 | 12 | 0.587 | 24.77 |
| Custom | 10 MiB | 4.339 | 28.676 | 10.208 / 30.458 | 5.750 / 9.667 | 11 | 25 | 0.890 | 30.84 |
| Crop 0.4.3 | 100 MiB | 63.790 | 186.187 | 5.625 / 10.042 | 1.250 / 4.084 | 2 | 11 | 12.425 | 210.00 |
| Custom | 100 MiB | 62.979 | 313.513 | 10.125 / 26.834 | 7.375 / 28.833 | 11 | 27 | 13.165 | 242.58 |

The binary drops the generator `String` before clone, scan, edit, suffix-read,
and retirement measurements. `/usr/bin/time` cannot reset its high-water mark,
so peak RSS still includes the construction interval in which the complete
input `String` and the new rope coexist. It is an ingest peak, not steady parser
RSS.

That distinction is material. A supplementary 100 MiB Crop build through
`RopeBuilder`, using a 47 KiB reusable ingress chunk and never allocating a
complete generator, produced the same hash with:

| Build | Build ms | Scan ms | Edit p50 / p99 us | Final drop ms | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| Crop streamed | 21.867 | 152.503 | 1.125 / 6.875 | 10.348 | 110.16 |

This supports chunked worker/file ingress. It also confirms that choosing a
rope does not itself solve large paste/open transfer; construction must avoid a
whole-document intermediate. Crop's `From<String>` copies the string, while
`RopeBuilder` provides the needed streaming API today.

## What the result proves—and what it does not

### Snapshot leases do not require Crop subtree identity for correctness

Crop exposes an O(1) COW root clone but keeps its root and `TinyArc` private.
The prototype wraps it with a monotonically minted `RootIdentity`. Every leaf
descriptor carries that identity and can bind only while the exact snapshot
lease is present. An edit made exclusively through the wrapper produces exact
unchanged prefix/suffix mappings by construction.

That is enough for stale-result rejection and exact coordinate lineage. It is
not yet proof that parser convergence is fast enough without subtree identity.
Syntax nodes may carry root-bound ranges and compose edit provenance; if they
instead need O(1) source-subtree equality, Crop cannot provide it through its
public API.

### The scan ratio is directional, not a parser-to-paint forecast

The harness repositions both backends at each 4 KiB range poll. Crop then walks
contiguous chunks; the custom backend walks anchored bytes. The current custom
block job can retain one `SourceCursor` across polls, and a Crop adapter should
likewise retain a chunk iterator under a longer-lived snapshot lease. The real
owned parse job must be benchmarked before treating the roughly 2x scan lead as
a complete-parser result. Real insertion/deletion traces and paste workloads
are also still missing.

### Fuelled destruction remains unsolved for both choices

Dropping the final 100 MiB roots took 12.4 ms for Crop and 13.2 ms for the
custom source; streamed Crop still took 10.3 ms. Crop recursively releases its
private B-tree through a custom `TinyArc`, so Flark cannot meter node retirement
without forking it. The current custom source also recursively releases
`std::sync::Arc` nodes, so this is not presently a reason to prefer the custom
implementation.

Hot cancellation normally drops one clone while the canonical snapshot still
exists, which is O(1). Unique large candidates, document replacement, and final
close remain the dangerous cases. The next gate must test a dedicated native
reclaimer and the single-Web-Worker behavior. If a hard sub-frame destruction
bound is required on web, either source backend needs architectural work; Crop
is harder to change because its tree is private.

## Dependency and maintenance audit

Crop is a compact, single-purpose dependency with a mature public rope API,
UTF-16 metrics, line metrics, cheap snapshots, a streaming builder, fuzz tests,
and active-but-sparse maintenance. Its repository shows 472 commits and March
2026 fixes, while the latest crates.io release remains 0.4.3 from April 2025.

That released version is behind two upstream iterator correctness fixes:

- [`Units*` initialization fix](https://github.com/noib3/crop/commit/669a96d802ae1940fe4572f9a28a18ac3666ae2c)
- [`UnitsBackward::remainder()` offset fix](https://github.com/noib3/crop/commit/d0234ce772eb34c7a3878d4ed57dc864da291cfb)

Crop also contains 23 source lines with `unsafe` operations or unsafe trait
implementations, concentrated in its private `TinyArc`, gap buffers/strings,
iterators, builder, and unchecked child lookup. Flark's `unsafe_code =
"forbid"` lint does not audit dependencies.

If the real adapter gate passes, production should therefore:

1. pin an audited upstream commit that includes the 2026 fixes, or vendor that
   exact source while waiting for a release;
2. run upstream tests plus Flark's UTF-8/UTF-16/CRLF/adversarial edit corpus,
   Miri where supported, and sanitizer/fuzz lanes over every API Flark uses;
3. keep Flark's root identity, provenance, descriptor, and history limits in a
   wrapper so no private Crop representation leaks into RFC 023; and
4. avoid a Crop fork for subtree IDs or fuelled destruction unless the next
   gate demonstrates a product requirement that cannot be solved above the
   rope.

This is a smaller maintenance surface than continuing the custom source tree,
but it is not a zero-maintenance dependency.

## Reproduction

```sh
cargo build --release --features custom
/usr/bin/time -l ./target/release/flark-source-backend-bakeoff crop 100 1000 4096
/usr/bin/time -l ./target/release/flark-source-backend-bakeoff custom 100 1000 4096
/usr/bin/time -l ./target/release/flark-source-backend-bakeoff crop-stream 100 1000 4096
```
