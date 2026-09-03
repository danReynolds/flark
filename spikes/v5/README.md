# Flark v5 spikes (RFC 030 §14)

> The parse spike graduated into `native/flark_parse` (M1). The harnesses here still point at the spike crate for reproducibility of the M0 numbers.


Measured 2026-09-02 on a MacBook Pro M1 Pro. All spikes use **unmodified
comrak 0.54** from crates.io plus a ~600-line extraction (`parse_spike/src/model.rs`)
that walks the AST once and writes one flat little-endian render model.

## Results

| Spike | Pass line | Result |
| --- | --- | --- |
| End-to-end keystroke, FFI, 25 KB dense | < 3 ms | **0.97 ms p50 / 1.19 ms p99** (v2 measured 8.5 ms) |
| End-to-end keystroke, FFI, 64 KB / 100 KB | informative | 2.4 ms / 3.9 ms p50 |
| Marshal alone (decode + project), 100 KB | < 1 ms | 0.54 ms p50 no memo, 0.43 ms with per-block memo |
| End-to-end keystroke, Wasm under dart2js, 25 KB, warm | informative | 1.2 ms p50 / 2.0 ms p99; parse 0.8 ms = native |
| Wasm instantiate (462 KB module) | < 100 ms | 15–29 ms |
| Sourcepos + line-content differential, 1,322 cases | zero unregistered | 15 cases, 4 classes, all registered (see SOURCEPOS_REGISTER.md) |
| Phone end-to-end, iPhone 16 (A18), iOS 18.7.3, profile | < 4 ms at 64 KB | **25 KB 0.69 / 0.76, 64 KB 1.75 / 1.83, 100 KB 2.78 / 3.84 ms p50 / p99**; first pass after launch shows ~200 ms stalls (launch interference), steady state clean |

Per-stage at 25 KB over FFI (ms p50): splice 0.01, utf8 encode 0.06, copy 0.00,
parse+extract 0.76, decode+project 0.14. The parse is ~78% of the keystroke;
marshal is no longer the dominant cost it was in v2 (68%).

Render model size is ~7× source (u32 fields, byte and UTF-16 offsets for
every range). That is fine for the tier; halving it with u16 deltas is an
optimization, not a requirement.

## Layout of the model

Header (9 × u32) → line table (start byte, start UTF-16) → blocks (12 × u32:
kind, parent, source range in bytes and UTF-16, first line, line count,
content-table offset, two attrs, flags) → per-line content ranges (5 × u32)
→ inline runs (13 × u32: kind, block, parent run, source range, content range,
both coordinate systems, two aux words) → string table (replacement text).

Hidden bytes of a run are exactly `source − content`. Dart never inspects a
delimiter character.

## Run

```sh
cd parse_spike && cargo build --release && ./target/release/bench
./target/release/differential ../../../test/fixtures/commonmark/upstream
cd ../dart_harness && dart run bin/keystroke.dart
cd ../web_harness && RUSTC=$(rustup which rustc) rustup run stable cargo build --release --target wasm32-unknown-unknown --manifest-path ../parse_spike/Cargo.toml \
  && cp ../parse_spike/target/wasm32-unknown-unknown/release/flark_parse_spike.wasm web/ \
  && dart compile js -O2 -o web/main.dart.js web/main.dart && (cd web && python3 -m http.server 8765)
cd ../phone_bench/example && flutter run --profile -d <iphone-udid>   # prints FLARKBENCH lines
```

Note: the Homebrew `cargo` on PATH lacks cross targets; use
`RUSTC=$(rustup which rustc) rustup run stable cargo …` for wasm32 and iOS.
