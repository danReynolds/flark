# Bounded Comrak challenger

This disposable crate tests a deliberately weaker alternative to a fully
Flark-owned parser: keep a definitive incremental block spine, but invoke
stock Comrak only after a complete semantic region is known to be below a
fixed byte cap. Oversized regions remain editable source and are explicitly
opaque to live semantic styling.

The current spine is the existing research-only Comrak-derived subset. It is
exact only for its named quote/list/fence/paragraph/setext fixtures. The crate
therefore cannot validate full CommonMark/GFM correctness; it can validate the
allocation boundary and expose which missing grammar dependencies turn the
proposal into a deep block-parser fork or an SLA change.

Run:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release --bin bounded_bench -- many-small 100 65536
cargo run --release --bin bounded_bench -- paragraph 10 65536
```

`bounded_bench` arguments are `shape`, target MiB, and semantic-region cap in
bytes. Shapes are `many-small`, `paragraph`, `fence`, `list`, and `table`.

