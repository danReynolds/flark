# Generated resumable scanner gate

This gate tests whether Flark can keep one pinned lexical rule authority while
producing both Comrak's ordinary fixed-input scanner and a resumable scanner
for pathological physical lines.

It is intentionally two materially different scanner families, not a shipping
scanner module. `atx_heading_start` is a simple prefix recognizer;
`open_code_fence` uses trailing context and can restore its logical cursor over
an arbitrarily long line. The handwritten oversized-line gate established the
semantic and scheduling shape; this gate tests the missing generation and
maintenance mechanism. It generates the original ATX prefix-grant proof plus
source-free cursor artifacts that run against a caller-owned physical-line
source while retaining only scalar DFA/cursor/marker state.

## Reproduce

Build or obtain **re2rust 4.3.1**, then run:

```sh
RE2RUST=/path/to/re2rust python3 scripts/generate.py --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

Without `--check`, `generate.py` updates the extracted rules, storable Rust
DFAs, and provenance hashes. It first regenerates all of Comrak's ordinary
scanners from the vendored `scanners.re`, formats them, and requires byte
equality with Comrak's checked-in `scanners.rs`. It then mechanically extracts
the selected ATX rule and both accepting `open_code_fence` rules into
grammar-free wrapper templates and generates the storable DFAs.

The generator-isolation tests use a `Vec<u8>`, and `tests/crop_cursor.rs` now
runs the generated ATX DFA directly over the runtime slice's real Crop cursor
while preserving its existing 16-byte test cache. The stricter fence adapter
retains zero source bytes and accepts only the exact next Crop byte. Both
adapters instrument first reads and maximum requested rewind; both generated
cursor scanners retain zero source bytes.

The fence proof distinguishes the scanner's logical match cursor from the
physical source cursor. The DFA reads trailing context to the line terminator,
then performs a terminal restore to the fence-end marker. On a 10 MiB line that
is a 10 MiB **logical** rewind, but it makes no backwards source request: the
Crop cursor stays at its physical high-water mark. Production must preserve
both values rather than reconstructing physical progress from the returned
match offset.

The Crop adapters are still query/test cursors. Production must place this
contract behind the ledger-owned physical-line view, bind the full
source/build/line identity, observe every first-read byte in the shared decoder,
and compose its poll budget with parser transition/output budgets.
