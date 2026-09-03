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
  to a flat render model. The schema is `schema/render_model_v1.json`;
  `tool/gen_schema.py` derives `src/schema.rs`, `SCHEMA.md`, and the Dart
  constants. Three-function C ABI on cdylib, staticlib, and wasm32.
- `packages/flark`: pure Dart, must not import Flutter. M1 holds the render
  model views and the parse transports (FFI on the VM, Wasm through
  `dart:js_interop` on the web). M2 adds the projection, caret, commands,
  history, and the `FlarkEditor` facade.
- `test/fixtures/commonmark`: the upstream CommonMark 0.31.2 and GFM corpora
  and the deviation register.

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
  (the build hook compiles the crate).
- Rust-free consumer: `packages/flark/tool/verify_prebuilt_consumer.sh`.
- Wasm asset: `packages/flark/tool/build_wasm.sh` after any crate change,
  then commit `packages/flark/lib/assets/wasm/flark_parse.wasm`.
- Schema: edit the JSON, run `python3 native/flark_parse/tool/gen_schema.py`,
  commit all three outputs (CI diffs them).
- Rust toolchain: `rust-toolchain.toml` is the single selector. Scripts and
  the hook run `rustup run <active toolchain>`; the Homebrew `cargo` on a
  developer's PATH lacks cross targets, so use those scripts for wasm and iOS.

## Architecture notes

- Markdown source is the document. Rust is the only Markdown authority: Dart
  consumes ranges from the render model and never inspects a delimiter. A
  range the model lacks is a parse-crate bug, not a Dart workaround.
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
- No test asserts on an edited frame after settling. Kernel journeys assert
  the visible transcript per step; budgets are single frames.
- No concept enters the kernel without a journey, a conformance case, or a
  named consumer (Dune, Fleury). Line budgets in the build plan are gates.
- Commit and push only when asked; watch CI to green after a push.
