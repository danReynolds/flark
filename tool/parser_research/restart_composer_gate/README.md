# Restart/composer authority gate

Status: executable architecture proof; not a parser or storage-selection gate.

This crate makes the checkpoint split from
`COMPOSED_BLOCK_ADOPTION_CONTRACT.md` concrete:

```text
RestartState
  ControlContinuation
  StableOpenBindings
  SemanticPrefixState
  SourceCursor
  SchedulerCursor
```

There is no complete semantic frame inside `ControlContinuation`, and no
semantic payload is a growing string or contiguous vector. Text/raw payloads
are stable source spans joined by immutable run nodes. Child state is an
associative scalar fold. Reference definitions use persistent occurrence
nodes plus an exact persistent label trie.

`Composer::match_control` returns only a `ControlWitness`, which deliberately
has no attachment API. `Composer::compose` additionally validates exact edit
lineage, old/current revisions, mapped boundaries, immutable suffix-tail
identity, live binding capabilities, aligned typed semantic paths, and a
variant-specific output recipe. Only it can construct an `AdoptionPermit`.
The permit retains the exact semantic-root identity/generation, revisions,
lineage, mapped boundaries, suffix-tail identity, and every action paired with
its validated open binding capability. It is non-cloneable. Before yielding a
single action, its consuming storage handoff checks the selected transaction's
exact base stamp and outer-to-inner capability path. Storage therefore needs no
`BlockId` lookup or source rediscovery, and one proof cannot be replayed into
multiple candidate roots.

The suffix side exposes suffix contributions, never an old complete semantic
frame. A changed prefix is therefore combined with the retained suffix; there
is no API by which composition can restore the old prefix output.

## Covered adversarial cases

- identical ordered-list control with a changed displayed start and child fold;
- identical two-column GFM table control with whole-paragraph versus
  split-preface promotion recipes;
- setext promotion preserving the stable paragraph binding;
- reference-only paragraph detach, duplicate first-winner replacement, and
  exact consumer invalidation;
- raw source-run splicing with retained suffix-subtree identity;
- incompatible and expired binding capabilities;
- wrong storage root/generation, lineage, revision, boundary, suffix tail,
  capability, path shape, and capability order;
- stale revision, wrong lineage, wrong mapped boundary, and wrong suffix-tail
  identity;
- scheduler progress being irrelevant at a line boundary, while a mid-line
  scheduler pause is ineligible for convergence; and
- changed paragraph source runs never restoring old-prefix output despite
  equal control.

## Run

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo test --release --all-targets
cargo clippy --all-targets -- -D warnings
RUSTC="$(rustup which --toolchain stable rustc)" \
  cargo check --target wasm32-unknown-unknown --all-targets
```

The 2026-07-16 run is green: 14/14 debug tests, 14/14 release tests, one
compile-fail replay test, warning-free strict clippy, clean formatting, and a
successful `wasm32-unknown-unknown` all-targets check. The explicit rustup `RUSTC` is
needed on this machine because Homebrew Cargo otherwise selects Homebrew Rust,
whose sysroot does not contain the installed rustup WASM target.

## Verdict boundary

Passing this gate is a **GO for the representation-neutral authority seam**.
It is a **HOLD for production adoption** until the selected serialized or
hierarchical green representation implements these recipes transactionally,
the composer is polled under fuel for adversarial open depth, and every-line
restart is differentially equal to a clean parse over the full block corpus.
