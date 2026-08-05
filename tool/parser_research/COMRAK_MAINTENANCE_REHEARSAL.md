# Comrak fork maintenance rehearsal

Status: Phase 1 decision evidence, 2026-07-14. This is an upgrade rehearsal
against a disposable checkout, not a production fork or an upstream release.

Current-decision note: the later symmetric state/output probes supersede this
document's front-runner judgment. They retain the useful upgrade receipt but
reject the narrow arena-backed fork as the lifetime representation for RFC
023's unchanged oversized-construct contract. See
[`PARSER_AUTHORITY_REASSESSMENT.md`](PARSER_AUTHORITY_REASSESSMENT.md).

## Decision at the time of the rehearsal — superseded

At this stage Comrak remained the parser front-runner. The research algorithm
survived a representative four-release upgrade from Comrak 0.50.0 to 0.54.0
with small, understandable integration work. This was enough to continue the
maintenance proof, but not enough to approve the current patch as production
architecture.

The present patch is intentionally shaped as a research test:

- exact upstream base: Comrak `v0.50.0`, commit
  `b36ba6cff01fae07196f31f2bb4e761b73a1aed0`;
- patch: `0001-Prototype-incremental-checkpoints-and-persistent-suf.patch`;
- patch footprint: 3,177 insertions and 15 deletions over 13 hunks;
- touched upstream files: `src/parser/inlines.rs` and `src/parser/mod.rs`;
- roughly 3,000 lines are one `#[cfg(test)]` probe module appended to
  `parser/mod.rs`; the remaining hooks collect reference dependencies and
  occurrences only in test builds.

That shape makes the algorithms reproducible, but it also makes the raw line
count a misleading estimate of the production fork. Production work must
extract a small stateful parser API and Flark-owned incremental modules instead
of removing `cfg(test)` from the patch wholesale.

## Rehearsal target

At the time of the rehearsal, crates.io reported Comrak 0.54.0 as current. The
target tag resolves to commit
`172c2ee7d2c5c262a28be3e407aadf705daea2b7`.

Between 0.50.0 and 0.54.0, the relevant upstream surface changed materially:
1,197 insertions and 83 deletions across `Cargo.toml`, `src/nodes.rs`, and the
parser modules. Changes included AST attributes, block directives, source
column behavior, inline parsing changes, and new extension work. This was not a
no-op version bump.

## Procedure

The rehearsal used a clean upstream clone and preserved the research patch as
one downstream commit:

```sh
git checkout v0.50.0
git apply /absolute/path/to/0001-Prototype-incremental-checkpoints-and-persistent-suf.patch
git add src/parser/inlines.rs src/parser/mod.rs
git commit -m "Prototype incremental checkpoints and persistent suffix reuse"
cargo test --release incremental_checkpoint_probe -- --nocapture

git rebase --onto v0.54.0 v0.50.0
cargo test --release incremental_checkpoint_probe -- --nocapture
cargo test --release \
  checkpoint_chunks_remain_exact_across_ten_thousand_large_document_edits \
  -- --ignored --nocapture
```

## Results

### Pinned baseline

The patch applied cleanly to 0.50.0. Fourteen focused incremental tests passed;
the explicitly ignored 10,000-edit soak was not part of that focused command.

### Upgrade integration

The rebase onto 0.54.0 produced two conflict regions, both in the `Parser`
state declaration/constructor where upstream added `heex_block_depth`. The
resolution retained both the upstream field and the research-only reference
occurrence field.

Compilation then exposed one semantic adaptation: the probe's manually
constructed `Ast` needed the new conditional `attrs: None` field introduced by
the upstream attributes extension.

After those changes, all fourteen focused incremental tests passed on 0.54.0,
including randomized checkpoint convergence, adversarial fence/HTML/list/table
and reference transitions, persistent list/reference summaries, in-container
list/table checkpoints, exact suffix reuse, and 500 sequential edits.

The complete upgraded upstream suite also passed: 663 unit tests and 203
doctests, with only the two upstream/intentionally ignored unit rows skipped.
The focused incremental suite separately passed with Comrak's default features
disabled, matching Flark's lean shipping dependency configuration.

The 500-edit receipt remained local on the upgraded parser:

- reparsed bytes p50: 68;
- reparsed bytes p95: 194;
- reparsed bytes p99: 204;
- maximum reparsed bytes: 347;
- parser time p95: 12 microseconds in this release-mode host run;
- splice time p95: 27 microseconds;
- replaced chunks p95: 2.

The ignored release-mode soak also passed all 10,000 edits against a
1,003,383-byte document while comparing the incremental result to a clean full
parse after every edit:

- reparsed bytes p50: 70;
- reparsed bytes p95: 211;
- reparsed bytes p99: 213;
- maximum reparsed bytes: 512;
- parser time p50/p95/max: 15/34/1,738 microseconds;
- splice time p50/p95/max: 35/69/7,218 microseconds;
- delta bytes p50/p95/max: 161/1,059/2,346;
- replaced chunks p95: 2;
- result: 10,000 full-vs-incremental comparisons passed in 577.65 seconds.

The rare millisecond-scale tails are a reminder that production budgets and
continuations remain required even though ordinary locality is excellent.

## What this proves

- The checkpoint/state assumptions used by the research engine were not
  invalidated by four Comrak releases and substantial parser evolution.
- The downstream integration points exposed by this patch are few enough that
  a maintainer could understand the observed conflicts.
- The differential corpus caught an upstream AST-shape change at compile time
  rather than silently accepting divergent output.

## What this does not prove

- The research module is not a public stateful native/WASM parser service.
- Test-only access to private parser state is not the production fork boundary.
- Enabled-extension checkpoint coverage is incomplete until the 0.54 additions
  and every Flark-enabled option receive full-vs-incremental cases.
- Cancellation, byte/node/time budgets, revision supersession, protocol
  corruption, and memory retention are not exercised by this upstream patch.
- Native/WASM parity and cmark-gfm/Flark-corpus equivalence have not yet run on
  the rebased fork.
- One successful upgrade rehearsal does not establish ownership or cadence.

## Required production shape

Do not merge the 3,300-line research patch into the shipping bridge. The next
fork change should instead:

1. Pin an exact upstream commit and carry a machine-readable patch inventory.
2. Keep the smallest necessary parser hooks in upstream-owned files.
3. Move checkpoint state, persistent chunks, dependency indexes, and delta
   construction into dedicated Flark-owned modules.
4. Expose a narrow stateful document-handle API with revision/hash validation,
   explicit work budgets, cancellation, and supersession.
5. Make full parsing the differential oracle in native and WASM CI.
6. Add every enabled extension and the newly introduced upstream parser shapes
   to checkpoint serialization/resumption tests before Phase 2.

The parser-service and binary-delta contracts remain independent of Comrak so
this fork can still be rejected without discarding the Dart source substrate or
the editor architecture.

## Production-shape extraction follow-up

The next maintenance experiment extracted the research module on Comrak 0.54.0
instead of removing `cfg(test)` in place. The reproducible net patch is
`0002-Extract-stateful-incremental-parser-API.patch` and applies cleanly to the
unmodified `v0.54.0` tag. `COMRAK_PATCH_INVENTORY.json` pins its upstream
commit, SHA-256, per-file ownership/counts, validation receipts, and known gaps
for automated upgrade review.

The resulting diff is 3,435 insertions and 5 deletions, but the integration
surface in existing upstream files is only 53 insertions and 5 deletions:

- `src/lib.rs`: four re-export lines;
- `src/parser/inlines.rs`: 21 insertions and one deletion for opt-in reference
  dependency capture;
- `src/parser/mod.rs`: 29 insertions and four deletions for an optional
  reference-definition observer and the child-module declaration;
- `src/parser/incremental.rs`: 3,316 new Flark-owned lines;
- `examples/incremental_spike.rs`: a 65-line external-consumer receipt.

Ordinary full parses leave both observers disabled. A reference-heavy 1 MB
full-parse comparison measured stock/fork p50 at 21,465/21,520 microseconds and
p95 at 23,389/23,242 microseconds over 101 warmed samples. That is measurement
noise rather than evidence of a regression.

The extracted module now exposes a revision/hash-validated `IncrementalDocument`
and edit request. Its delta includes the replaced chunk range, actual stable-ID
inserted chunks with source-relative nodes, reference presence/value changes,
and work measurements. The clean external-crate run is important: unlike unit
tests, it does not compile the full-parse oracle into the library. Across 10,000
one-byte edits in a 1,000,065-byte document it measured:

- end-to-end apply p50/p95/p99: 11/16/20 microseconds;
- maximum apply: 126 microseconds in the final warmed run;
- reparsed bytes p95/max: 63/63;
- initial document construction: 19.6 milliseconds in that run.

Validation after extraction passed:

- 663 upstream/fork unit tests, with the two intentional ignored tests skipped;
- 203 doctests;
- 14 focused incremental tests with default features disabled;
- a clean `wasm32-unknown-unknown` release build with default features disabled;
- clean application and compilation of the generated patch against a second
  `v0.54.0` checkout.

This clears the **minimal-hook maintenance-surface gate**. It does not clear the
production-parser gate. Remaining extraction debt is explicit:

- test helpers are still colocated in the new module and temporarily covered by
  `allow(dead_code)`; they must move under a test-only child module;
- the integrated document handle still checkpoints only at safe block
  boundaries; the separately proved in-list, in-table, and raw-block
  continuation checkpoints have not yet been incorporated into this API;
- inline leaf deltas and the production binary codec are not yet returned by
  the handle;
- node kinds are temporary Comrak debug strings rather than a versioned wire
  enum;
- cancellation, supersession, and byte/node/time budgets remain absent.

The maintenance conclusion is therefore narrower and stronger than the first
rehearsal: Comrak can be isolated behind a small, understandable fork boundary.
The open question is no longer whether the patch can be maintained; it is
whether completing the editor-specific parser engine on top of Comrak is a
better lifetime investment than owning a purpose-built grammar core.
