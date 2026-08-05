# Composer-permit to packed Enter-rewrite gate

Status: **immutable Enter-rewrite mechanism GO; storage-authorized adoption
HOLD**, 2026-07-16.

This crate mechanically joins the real `restart_composer_gate` permit to the
real packed `SerializedGreenDocument`. It is deliberately narrow: it extracts
the property portion of a Setext Paragraph-to-Heading recipe that selected
storage can express as an immutable Enter+facts rewrite. It does not consume
the recipe's content runs or attach its suffix, so its returned document is not
evidence of complete semantic adoption.

The current adapter:

1. maps the green manifest's arena slot index/generation to composer root
   scalars;
2. consumes the non-cloneable adoption permit;
3. checks a caller-supplied outer-to-inner stable binding path against the
   permit;
4. pairs each action by open depth with a caller-supplied
   `GreenEnterCapability`;
5. validates manifest, BlockId, and old kind without a BlockId lookup; and
6. sends all completed rewrites through one immutable-base arena transaction.

The three executable tests prove only:

- successful Setext promotion retains BlockId, changes the exact Enter/facts,
  leaves the old root queryable, and preserves a distant leaf ArenaId;
- a wrong concrete leaf capability fails as `StaleCursor`, returns to the same
  live-node count after reclaim, and preserves the old root; and
- a numerically changed manifest scalar is rejected before the rewrite.

They do not prove that lineage/revision/boundary/tail values came from storage,
that manifest identity cannot alias across `PageArena` instances, that the
physical capabilities form one nested open path, or that a partially allocated
multi-action failure rolls back. The focused failure is rejected before a
replacement page is allocated.

Validation:

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
RUSTC=/Users/dan/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  cargo check --target wasm32-unknown-unknown --all-targets
```

All are green.

This gate does **not** yet attach a suffix, apply the Setext content runs, bind
the result to the current source revision, or execute list, raw-run, table,
reference-detach, or coverage-range actions. Those need the unified
source/projection schema, an unforgeable storage-owned base proof, and the
generic range mutation API. Unsupported actions fail before `rewrite_enters`
opens a candidate transaction; adding ad hoc BlockId lookup or a second mutable
action path is not an allowed extension. The required replacement contract and
adversarial gates are recorded in
[`../COMPOSED_STORAGE_AUTHORITY_AUDIT.md`](../COMPOSED_STORAGE_AUTHORITY_AUDIT.md).
