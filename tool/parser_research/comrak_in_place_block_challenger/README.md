# Comrak in-place block challenger

This disposable crate contains two pieces of evidence:

- `src/main.rs` audits the exact Comrak 0.54 block-parser functions that Flark's
  current GFM profile would need to preserve or refactor; and
- `tests/block_seams.rs` plus `src/bin/cancellation_probe.rs` exercise the narrow
  scanner facade and stock whole-line scheduling boundary.

The architectural verdict and captured receipts are in
[`../COMRAK_IN_PLACE_BLOCK_ENGINE_CHALLENGE.md`](../COMRAK_IN_PLACE_BLOCK_ENGINE_CHALLENGE.md).

```sh
cargo test --release
cargo run --release -- /path/to/comrak-0.54.0
cargo run --release --bin cancellation_probe
```

