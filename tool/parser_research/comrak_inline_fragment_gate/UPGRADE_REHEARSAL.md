# Comrak inline patch upgrade rehearsal

Status: preliminary maintenance evidence, 2026-07-15.

This is deliberately separate from the pristine 0.54 replay gate. A patch can
replay perfectly on its donor version while still being expensive to carry
across upstream releases.

## Current upstream main

Checked commit: `9e10bf2458c9a1bf92a14feb39c548d7d23bfced`.

At the time of the rehearsal, upstream `main` differed from tag `v0.54.0` only
in `CHANGELOG.md` and `Cargo.lock`; there was no parser source change. The
isolated patch's Rust source hunks applied cleanly with `git apply --check` and
then applied without fuzz. `Cargo.toml.orig` no longer exists upstream and the
generated `Cargo.toml` feature insertion moved, so the two research feature
flags were added manually to current `Cargo.toml`. This replay includes the
selected post-inline task-list phase and its exact source-range handling.

After that metadata-only adaptation:

```text
cargo fmt --all -- --check
cargo check --lib --no-default-features
cargo test --lib tasklist --no-default-features
16 task-list tests passed; 473 filtered out
```

This proves the patch is not accidentally tied to the release tarball layout.
It is not a meaningful parser-source upgrade because upstream has not changed
parser source since 0.54.

## Previous source-changing release boundary

The `v0.53.0..v0.54.0` release changed 29 files by 1,447 insertions and 296
deletions. `src/parser/inlines.rs` changed by 162 insertions and deletions,
including signatures and call sites used by the Flark annotations.

Applying the 0.54 inline patch three-way onto 0.53 produced:

- clean application for `src/lib.rs`, `src/parser/mod.rs`, and the new
  `src/parser/inline_fragment.rs`;
- one conflicted file: `src/parser/inlines.rs`; and
- four conflict regions, all inside the expected inline-parser seam.

That reverse-boundary exercise is not a compiled 0.53 backport, so it cannot
stand in for a future forward upgrade. It does show that a substantial upstream
inline-parser release concentrates the manual reconciliation in the exact file
and functions pinned by the provenance inventory rather than spreading it
through the block parser or renderer.

## Maintenance judgment

The current evidence supports a conditional maintenance case:

- routine upstream metadata/dependency movement is cheap;
- a source-changing inline release should be expected to require human review
  and conflict resolution in the sensitive annotation seam;
- the six-file/12-function manifest makes that review bounded and fails closed;
  and
- the full pristine differential and upstream suites remain required after
  every port.

Do not call forward maintenance proven until the first upstream release after
0.54 changes parser source and the patch is actually ported, differentially
validated, and reviewed. If that port requires semantic redesign outside the
pinned inline seam, reopen the Comrak decision.
