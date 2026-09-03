# flark_parse

The Flark v5 parse crate: **unmodified comrak** plus a single-pass extraction
that writes a flat render model (see [SCHEMA.md](SCHEMA.md), generated from
`schema/render_model_v1.json`). It is the only place Markdown is interpreted;
the Dart kernel consumes ranges and never inspects a delimiter.

## ABI

Three functions on both targets (`cdylib` for FFI, `wasm32-unknown-unknown`
for the web), plus a schema version query:

```c
int32_t  flark_parse(const uint8_t* src, uint32_t len, uint8_t** out, uint32_t* out_len); // 0 ok, 1 null, 2 utf8, 3 contained panic
uint8_t* flark_parse_alloc(uint32_t len);
void     flark_parse_free(uint8_t* ptr, uint32_t len);
uint32_t flark_parse_schema_version(void);
```

## Derive, then validate

comrak does not expose per-line content ranges or reference definitions, and
its inline positions are wrong in two known situations. Each derivation is
checked against comrak's own output in report mode, and the conformance test
asserts zero deviations across the 652 CommonMark and 670 GFM upstream cases:

| Derived | Validated against |
| --- | --- |
| Per-line content start after container prefixes (`>`, list markers, footnote indents), with partial tabs carried as virtual spaces | inline node starts never precede it; code block content equals comrak's literal |
| Reference definitions stripped from the start of a paragraph, mirroring comrak's `resolve_reference_link_definitions` | every Text literal equals its corrected source slice |
| Definitions that left no paragraph behind (v2's textual scanner with block coverage) | no block covers them |
| Escaped-pipe shift inside table cells | delimiter and literal checks |
| Block ranges widened to their content where comrak reports a one-byte range (indented code inside containers) | the content itself is validated |

## Commands

```sh
cargo test --release                                   # conformance + invariants + fuzz
cargo build --release --features tools                 # differential, bench, model_hashes
./target/release/differential ../../test/fixtures/commonmark/upstream
./target/release/bench
tool/verify_transports.sh                              # native vs wasm byte identity (needs node)
python3 tool/gen_schema.py                             # after editing the schema JSON
```

The Homebrew `cargo` on PATH lacks cross targets; scripts use
`rustup run stable cargo` with `RUSTC=$(rustup which rustc)` for wasm32 and iOS.
