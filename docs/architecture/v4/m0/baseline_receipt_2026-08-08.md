# Flark v4 M0 baseline receipt — 2026-08-08

**Status: INCOMPLETE. M0 is not complete, this worktree is not release-ready,
and this document is not a conformance or performance claim.** The
machine-readable receipt is
[`benchmark/v4/m0_baseline_receipt_2026-08-08.json`](../../../../benchmark/v4/m0_baseline_receipt_2026-08-08.json).

This receipt records the evidence available against planning base
`47692297661489bcbc2a2af4574a6a422cf68ef7` (`Accept Flark v4 product
architecture and execution plan`). `HEAD` still names that commit, but the
observed worktree is dirty with concurrent M0 repairs and contract work. No
diff hash or immutable-revision claim is made.

## Bottom line

The targeted baseline is materially healthier than the first audit: root Dart
analysis passes, Rust formatting passes, the `flark-parser` library passes
170/170 tests, the CommonMark structural guard passes, and the new v4 contract,
competitor, and bounded-input-window tests pass. Those are useful receipts, but
they do not close M0.

The current WASM freshness guard fails again after later Rust/Cargo changes.
The root and `flark_flutter` WASM files are still byte-identical, but their
buildinfo manifest is stale. Full Rust workspace, native packaging, publish
archive, Mac profile, and full Flutter runs have not been completed and remain
explicitly `PENDING`.

## Anonymized environment

- MacBook Pro (`MacBookPro18,1`), Apple M1 Pro, 10 cores (8 performance and
  2 efficiency), 16 GB RAM, arm64.
- macOS 26.2 (25C56); Xcode 26.6 (17F113).
- Dart 3.12.2 stable at `/opt/homebrew/bin/dart`.
- Local Flutter 3.44.4, revision `ad70ec4617`, Dart 3.12.2. CI pins Flutter
  3.41.9, so local Flutter evidence does not substitute for CI.
- rustc/cargo 1.93.1; rustfmt 1.8.0; stable aarch64 Apple toolchain.

Device identifiers are deliberately excluded.

## Original baseline blockers and present disposition

| Blocker | First audit | Repair | Current receipt |
|---|---|---|---|
| Stale committed WASM | `verify_package_confidence.sh --skip-native` stopped at its first real gate | Root/nested assets and buildinfo were rebuilt and synchronized; the web asset version changed | **FAIL, reopened.** Later Rust/Cargo changes made the source manifest stale again. |
| Missing `expectedStructuralAck` | Root analysis reported six missing required arguments | Added all six arguments and a deterministic structural-ack fixture | **PASS.** Root analysis reports no issues. |
| Rust formatting drift | `cargo fmt --check` emitted workspace diffs | Applied rustfmt | **PASS.** Current formatting check is clean. |
| Fatal Flutter `avoid_print` | Full nested analysis found one diagnostic | Replaced `print` with `debugPrint` in the large-document checkpoint | **PASS, targeted only.** That file analyzes cleanly; full nested analysis is still pending. |

## Targeted checks run

| Check | Result | Boundary |
|---|---:|---|
| `cargo fmt --all --manifest-path native/comrak_bridge/Cargo.toml -- --check` | PASS | Formatting only. |
| `cargo test --locked --manifest-path native/comrak_bridge/Cargo.toml --package flark-parser --lib` | PASS — 170 passed, 0 failed | `flark-parser` library tests only. |
| `cargo test ... --test block_core_commonmark_ledger -- --nocapture` | PASS — 5 passed, 0 failed | Structural admission guard, not semantic/incremental conformance. |
| `dart analyze hook lib test` | PASS — 0 diagnostics | Root Dart targets. |
| v3 CommonMark coverage-ledger contract | PASS — 3 passed | Inventory/classification integrity, not conformance. |
| root v4 contract suite before adding this receipt contract | PASS — 23 passed | Fixture/data contracts only. |
| competitor baseline contract | PASS — 5 passed | Historical non-claim seed and a specified-but-unrun Mac protocol. |
| Flutter bounded input-window contract | PASS — 3 passed | Fixture state-machine contract only; no runtime implementation receipt. |
| targeted Flutter checkpoint-file analysis | PASS — 0 diagnostics | One repaired file only. |
| WASM freshness contract | **FAIL — 1 passed, 1 failed** | Assets match each other, but not current Rust/Cargo source inputs. |

## Markdown denominators — keep these claims separate

CommonMark structural admission is **652/652**. That says the production
controller admits every fixture into a terminal structural outcome. It does
not say that the HTML/semantic result is exact, and it says nothing about
incremental histories.

The selected profile owns four separate ledgers:

| Ledger | Exact | Missing | Divergent | Approved deviation | Denominator |
|---|---:|---:|---:|---:|---:|
| CommonMark semantic | 384 | 262 | 6 | 0 | 652 |
| GFM semantic | 0 | 672 | 0 | 0 | 672 |
| CommonMark incremental | 0 | 652 | 0 | 0 | 652 |
| GFM incremental | 0 | 672 | 0 | 0 | 672 |

Each row accounts for its denominator exactly. The v3 diagnostic inventory
(60 authoritative supported probes, 19 intentional fail-closed, 2 extension
divergences, and 571 unclassified) is also separate and is not laundered into
the four v4 ledgers.

The selected profile is CommonMark 0.31.2 (652 cases) plus GFM 0.29-gfm. The
upstream GFM fixture contains 670 cases and the pinned supplement restores
examples 279 and 280, yielding a 672-case GFM denominator.

## Frozen input hashes

The JSON receipt pins SHA-256 for the CommonMark/GFM corpora, v3 diagnostic
ledger, v4 Markdown profile and ledgers, task-list supplement, live-projection,
input-window, M0-regression and Unicode matrices, v4 workload catalogue,
competitor baseline, and `Cargo.lock`. The receipt contract recalculates every
listed hash. In particular:

- CommonMark 0.31.2 fixture:
  `d431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20`.
- GFM 0.29-gfm fixture:
  `ce09eea1c15b61235868465468f6281ec82ab177998e404d9143e1641c4e5b55`.
- GFM task-list supplement:
  `8a735bd2ce45b2cea42a687f6425d0519f8c9b2a62f77d3cb37b9e404c3e9a69`.
- Four-ledger file:
  `4898b72d18cec3682e2a121ef1b2e3423254ae9728f55a22d539341fe9cbd1f0`.

## Historical non-pass evidence remains part of the baseline

### G2 — blocked Flutter viewport run

Zero of eight planned configurations completed and no frame timings were
produced. Dense 5 KB content timed out before paint and the runtime subsequently
faulted; dense 25 KB hit an uncaught out-of-authority range receipt (`0x0111`).
A later direct-Dart bisect passed 22/22 cases, including the mixed 25 KB fixture
in 32 ms. That clears the parser core for this reproduction and localizes the
observed failure to the Flutter viewport integration; it does not turn G2 into
a pass.

### G3 — useful 1 KB seed, failed large-paste liveness

At 1,170 bytes, 113/120 ordinary edits became exact in one 4 ms pump, no edit
needed more than two pumps, and 240/240 sustained pumps were exact (p99
3,531 us, max 3,733 us). The 32,789-byte paste then attempted 100,000 pumps,
exhausted the budget only once, remained non-current, and reported no terminal
reason while preserving exact source. The 100 KB and 1 MB cases did not finish
inside the ten-minute run. G3 therefore remains partial and non-passing.

### Range certification — safe, but too coarse

All four invalidating-edit probes returned `PENDING` rather than stale semantic
facts, which is the required safety behavior. The untouched distant range also
became `PENDING` in every case, demonstrating document-wide rather than
per-range certification. This remains an M0 input, not a completed runtime
contract.

## Gates still pending

These are deliberately not inferred from the targeted passes:

- full locked Rust workspace/all-target run;
- full `verify_native_editor_ci.sh` native/packaging lane;
- full package-confidence lane (currently blocked by WASM freshness);
- immutable publish-archive consumers;
- current Mac profile-mode run and raw samples;
- both Mac-first competitor profile runners and their comparable evidence;
- full nested Flutter analysis, tests, and profile build.

M0 requires honest denominators and named non-passing regression/status inputs;
it does not require the later M2/M4/M6 implementation work to advance the four
ledgers or repair G2/G3. This receipt remains incomplete because stable
workspace/native/package gates, fresh WASM, immutable archive consumers, the
current Mac profile evidence, full Flutter gates, and the Mac-first competitor
protocol receipt are still outstanding.
