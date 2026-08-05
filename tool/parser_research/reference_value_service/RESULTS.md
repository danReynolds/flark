# Reference value service result

Status: **GO for the bounded Comrak-correspondent value transform; terminal
projection/writer integration remains HOLD**, 2026-07-21.

This crate does not recognize Markdown. The block parser supplies an accepted
destination or title range. The service performs only the pinned donor's value
operations:

- destination ASCII trim or title delimiter removal;
- HTML entity decoding from the exact pinned entity table; and
- Markdown backslash unescaping.

The transform is resumable. It retains at most one 32-byte entity candidate,
one bounded replay buffer, one pending backslash, and one output chunk. Invalid
entity fallback replays its candidate through the same state machine, so a
later valid entity is not skipped. `Progress` is distinct from `NeedInput`;
the caller cannot overwrite a still-pending byte while replay work remains.

The build-generated table pins these exact limits:

- longest complete named-entity spelling: 33 bytes;
- largest named-entity output: 6 UTF-8 bytes; and
- worst output/source expansion: 6/5.

`entities` is pinned to `1.0.1`; Cargo resolves one copy for this service and
the vendored Comrak 0.54 donor.

## Receipt

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets

running 7 tests
test result: ok. 7 passed; 0 failed
```

The tests compare the cooked destination/title stored in Comrak's AST against
this service for every semicolon-terminated named entity in the selected
table, 2,000 randomized accepted reference values, numeric boundary cases,
Unicode, nested invalid/valid entity fallback, trimming, delimiter removal,
and backslash interactions. They also pin scratch/output bounds and prove that
a rejected second input offer does not mutate the pending byte.

## Still open

This receipt does not prove parser range provenance, random-access projection
replay, persistent blob ownership, terminal Paragraph replacement, reference
index publication, cancellation of the whole candidate, or native/Wasm
latency. Those belong to the actor-owned terminal join.
