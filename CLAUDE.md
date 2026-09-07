# Flark repository guide

Flark is a live Markdown editor and read-only renderer for Flutter, Flutter
web, and Fleury, being rebuilt as **v5** on a synchronous core: the whole
document is parsed by unmodified comrak on every keystroke and the frame
paints the parser's answer. The controlling decision is
[RFC 030](docs/architecture/rfc/rfc_030_synchronous_core.md); the execution
contract is the [v5 build plan](docs/architecture/v5/build_plan.md). The
repository root is a non-publishable workspace.

Active code:

- `native/flark_parse`: Rust. Unmodified comrak plus a single-pass extraction
  to a flat render model. The schema is `schema/render_model_v3.json`;
  `tool/gen_schema.py` derives `src/schema.rs`, `SCHEMA.md`, and the Dart
  constants. Three-function C ABI on cdylib, staticlib, and wasm32.
- `packages/flark`: pure Dart, must not import Flutter. `src/parse` holds the
  render model views and the parse transports (FFI on the VM, Wasm through
  `dart:js_interop` on the web); `src/kernel` holds the projection (rows,
  segments, hidden ranges, caret spans), the document (legal caret offsets,
  anchors, owners), the closed command set, grouped history, and the
  `FlarkEditor` facade that applies `edit_profile_v1` semantics. The facade
  library is `package:flark/flark.dart`; the render model and schema
  constants are `package:flark/render_model.dart`.
- `test/fixtures/commonmark`: the upstream CommonMark 0.31.2 and GFM corpora
  and the deviation register.
- `packages/flark_tree_sitter`: separate snippet language package under development.
  Pure Dart API, co-located `native/` Rust engine using official Tree-sitter,
  FFI and Wasm transports. Fourteen-language highlighting and edit proposals now
  cover Enter, typed outdent, branch alignment and selections. Automatic, aliases
  and the language catalog live here too; there is no Dart highlighter/lexical
  indenter fallback. See `SINGLE_ENGINE_REVIEW.md`. Combined typing
  performance remains open in the package's `PLAN.md`. The Flutter workbench
  now opts into synchronous snippet edits and controller-owned coloring workers.
  `HOST_INTEGRATION_REVIEW.md` records source/container/history, first-frame,
  real native-worker and browser-worker checks. `BROWSER_DOGFOOD_REVIEW.md` records
  hands-on browser journeys and host fixes. RFC 031 remains conditional on
  sustained color-transition/full-frame qualification; automated tests and
  observed screenshots do not close those performance or physical-device gates.

The new `packages/flark_flutter` host owns text input, focus, glyph geometry,
painting and bounded source inspection. Its runnable local-draft workbench is
`packages/flark_flutter/example`. Run analysis and tests in both directories;
build the example with `flutter build macos --profile` or `flutter build web --wasm`.
The immediate development loop is D0-web in Codex's embedded browser. The latest follow-up adds visible code selection, syntax coloring, language
selection, indentation and typed-closer outdent after fixing fence creation.
The rebuilt candidate is ready for exploratory browser dogfooding. See
`docs/architecture/v5/code_closer_review_2026_09_06.md`; the acceptance
contract is `docs/architecture/v5/web_dogfood.md`. D0-macOS native/performance
qualification remains separately open. Browser success cannot close native gates.

Superseded code is under `legacy/` (v2/v3 in `legacy/root_package` and
`legacy/native_v3_comrak_bridge`, v4 in `legacy/v4`). It is historical
evidence, not a dependency, and its scripts no longer run from the root.
The later v4 tip is on the `codex/editor-runtime-boundaries` branch.

## Commands

- Rust gates: `cargo test --release --locked --manifest-path native/flark_parse/Cargo.toml`
  (spec HTML conformance, extraction with zero deviations plus schema
  invariants, fuzz, regressions).
- Transport identity: `native/flark_parse/tool/verify_transports.sh [--rebuild]`
  (needs node; compares the committed wasm, and optionally a fresh build,
  to native across all 1,322 cases).
- Dart: `cd packages/flark && dart analyze --fatal-infos && dart test`
  (the build hook compiles the crate). Direct typed scenarios live under
  `packages/flark/test/journeys/` and are run by `test/journey_test.dart`; the
  generated discovery matrix is `test/matrix_test.dart`
  (`FLARK_MATRIX_ITERATIONS`, `FLARK_MATRIX_SEED`). Minimize a failing matrix
  log into a directly named regression rather than storing a replay fixture.
- Keystroke diagnostic: `cd packages/flark && dart run tool/bench_editor.dart 16`
  (the current dense, 16 KiB-class structural fixture; add `--spike` only to
  compare with the historical M0 document).
- Rust-free consumer: `packages/flark/tool/verify_prebuilt_consumer.sh`.
- Wasm asset: `packages/flark/tool/build_wasm.sh` after any crate change,
  then commit `packages/flark/lib/assets/wasm/flark_parse.wasm`.
- Schema: edit the JSON, run `python3 native/flark_parse/tool/gen_schema.py`,
  commit all three outputs (CI diffs them).
- Rust toolchain: `rust-toolchain.toml` is the single selector. Scripts and
  the hook run `rustup run <active toolchain>`; the Homebrew `cargo` on a
  developer's PATH lacks cross targets, so use those scripts for wasm and iOS.

## Architecture notes

- Markdown source is the document. Rust is the only Markdown-recognition
  authority: Dart consumes ranges from the render model and does not infer
  structure by scanning source. Explicit semantic commands may synthesize
  canonical Markdown, but their candidate result must validate through Rust.
  A range the model lacks is a parse-crate bug, not a Dart workaround (content
  records carry each line's innermost prefix start for exactly this reason).
- The caret is a source offset that is never strictly inside a hidden range
  and always on a row's caret span; several legal offsets can share one
  display position, and which one the caret holds is its typing context.
  An explicit noncollapsed whole-document selection retains `0..source.length`
  so Select All includes block syntax; collapsing it restores a legal caret.
  Movement keeps the context it came from; row edges take the outermost
  anchor; pointer placement uses the glyph half.
- The extraction derives what comrak does not expose and validates each
  derivation against comrak's own output; `native/flark_parse/REGISTER.md`
  lists every known comrak quirk and its correction.
- Two conformance claims are kept separate: comrak's HTML matches the spec
  fixtures (minus the registered deviations), and the extraction is faithful
  to comrak (zero deviations, schema invariants). Do not report one as the
  other.
- Coordinates are explicit everywhere: every range carries UTF-8 bytes and
  UTF-16 code units, and hidden bytes of a run are source minus content.
- The build hook resolves a bundled `prebuilt/<triple>/` library, then a
  consumer's `hooks: user_defines: flark: prebuilt_dir:`, then a cargo build.
  The hook runner sanitizes environment variables; they are not a channel.
- Web packaging: the package declares `lib/assets/wasm/flark_parse.wasm` as a
  Flutter asset; a dart2js page serves the module itself.

## Conventions & quality bar

- Every performance claim names a commit, machine, and number; local runs,
  CI, and device receipts are different proof levels and are labeled as such.
- No test asserts on an edited frame after settling. Direct kernel scenarios assert
  the visible transcript per step; budgets are single frames.
- No concept enters the kernel without a direct scenario, a conformance case, or a
  named consumer (Dune, Fleury). Line budgets in the build plan are gates.
- Commit and push only when asked; watch CI to green after a push.
