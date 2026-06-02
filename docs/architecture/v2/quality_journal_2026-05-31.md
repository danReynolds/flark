# Flark v2 Quality Journal

Status date: 2026-05-31
Scope: iterative code-quality, correctness, architecture, and performance
hardening on the current v2-only worktree.

## Iteration 1 - Baseline and Journal

Focus: establish the current health of the package before making any code
changes.

What changed:

- Added this journal so the continuing quality work has a durable audit trail.

Why:

- The current worktree is already far into the v2 migration. A journal keeps
  later improvements tied to concrete evidence instead of implicit context.

Verification:

- `flutter analyze hook lib test`: passed with no issues.

## Iteration 2 - Core Transaction Determinism

Focus: headless source transaction correctness.

What changed:

- Made `FlarkTransaction` sort operations stably by original operation
  order when multiple operations have the same source range.
- Added regression coverage for same-offset multi-insert transactions.

Why:

- The core transaction model permits multiple collapsed insertions at one
  original offset. Without a stable tie-breaker, their relative insertion order
  depended on `List.sort` implementation details. This could make command,
  projection, or live-editor transactions nondeterministic.

Verification:

- `dart format lib/src/v2/core/transaction/flark_transaction.dart test/v2/core/flark_transaction_test.dart`
- `flutter test test/v2/core --reporter compact`: passed.

## Iteration 3 - Projection Construction Cost

Focus: projection/cursor-mask performance and range-input robustness.

What changed:

- Converted `FlarkCursorMask` to validate hidden and replacement ranges
  once, then build projection spans from the validated lists.
- Let `FlarkProjection` construct its `cursorMask` from already validated
  ranges and spans instead of redoing validation work.
- Added regression coverage that cursor masks can be built from single-pass
  range iterables.

Why:

- Projection construction is on the parser-adoption and predictive-edit hot
  path. Revalidating the same ranges added unnecessary allocation/work and made
  the public mask constructor less tolerant of valid iterable implementations.

Verification:

- `dart format lib/src/v2/projection/flark_projection.dart test/v2/projection/flark_projection_test.dart`
- `flutter test test/v2/projection --reporter compact`: passed.
- `flutter test test/v2/performance/flark_v2_performance_budget_test.dart --tags benchmark --reporter compact`: passed.

## Iteration 4 - Markdown List Input Fidelity

Focus: source-first Markdown input correctness for list Enter/Backspace.

What changed:

- Preserved the actual whitespace padding after unordered and ordered list
  delimiters in `_ListContinuation`.
- Tracked the task-marker start separately so task-list handling can keep using
  full source ranges without normalizing the preceding list marker.
- Added Enter coverage for padded unordered and ordered list markers.
- Added Backspace coverage for spaced, tab-padded, and padded task-list
  markers.

Why:

- The input engine previously parsed list padding but stored a normalized
  marker (`- ` or `1. `). Structural Backspace could leave extra spaces behind,
  and Enter could unexpectedly rewrite user-authored list spacing. The engine
  should preserve canonical source choices unless a command explicitly
  reformats the source.

Verification:

- `dart format lib/src/v2/markdown/source/flark_markdown_input_engine.dart test/v2/markdown/flark_markdown_input_commands_test.dart`
- `flutter test test/v2/markdown/flark_markdown_input_commands_test.dart --reporter compact`: passed.

## Iteration 5 - Native Payload Boundary Hardening

Focus: native Comrak JSON payload validation.

What changed:

- Clamped decoded native byte offsets to non-negative values inside
  `NativeComrakPayloadCodec`.
- Added codec coverage for negative block, marker, and diagnostic offsets.

Why:

- The Rust bridge emits unsigned byte offsets, but the Dart boundary should
  still defend against corrupt or future malformed JSON payloads. Negative byte
  ranges should not leak into native value models or downstream range mapping.

Verification:

- `dart format lib/src/v2/native/native_comrak_ffi.dart test/v2/native/flark_native_comrak_bridge_test.dart`
- `flutter test test/v2/native/flark_native_comrak_bridge_test.dart --reporter compact`: passed.

## Iteration 6 - Parse Scheduler Failure Containment

Focus: Flutter controller/scheduler architecture and async lifecycle safety.

What changed:

- Added an optional `onError` callback to `FlarkParseScheduler`.
- Made scheduled background parses catch failures and report them through that
  callback instead of allowing unhandled async errors.
- Kept explicit `parseNow()` awaitable so direct callers can still observe
  thrown parser failures.
- Added scheduler coverage proving a failed in-flight parse is reported and a
  later controller revision still reparses successfully.

Why:

- The scheduler owns background parsing for promoted widgets. A parser backend
  failure should leave the render plan stale and observable, not escape as an
  unhandled asynchronous error that can destabilize an app.

Verification:

- `dart format lib/src/v2/flutter/flark_parse_scheduler.dart test/v2/flutter/flark_parse_scheduler_test.dart`
- `flutter test test/v2/flutter/flark_parse_scheduler_test.dart test/v2/flutter/flark_flutter_controller_test.dart --reporter compact`: passed.

## Iteration 7 - Promoted Widget Parse Errors

Focus: widget-level access to parser/scheduler failures.

What changed:

- Added `onParseError` callbacks to `MarkdownEditor` and
  `Markdown`.
- Reconfigured each widget's scheduler when the callback changes.
- Added promoted-surface widget coverage proving editor and preview parse
  failures are reported without surfacing as unhandled widget-test exceptions.

Why:

- The scheduler can now contain background parse failures, but app-level
  integrations need a promoted-widget hook to log, display, or recover from
  parser backend failures.

Verification:

- `dart format lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_markdown_preview.dart test/v2/flutter/flark_markdown_surface_test.dart`
- `flutter test test/v2/flutter/flark_markdown_surface_test.dart --reporter compact`: passed.

## Iteration 8 - Public API and Docs Alignment

Focus: promoted API contract after parser-error callback changes.

What changed:

- Updated the top-level public API widget smoke test to compile
  `onParseError` on both promoted widgets.
- Documented promoted-widget parse-error handling in the README.
- Updated the v2 public API inventory and changelog for the new callbacks.

Why:

- Public widget parameters are part of the package contract. The test and docs
  should describe the supported way for applications to observe parser backend
  failures instead of relying on implementation details.

Verification:

- `dart format test/public_api/flark_editor_barrel_test.dart`
- `flutter analyze lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_markdown_preview.dart lib/src/v2/flutter/flark_parse_scheduler.dart test/public_api test/v2/public_api`: passed.
- `flutter test test/public_api test/v2/public_api --reporter compact`: passed.

## Iteration 9 - Scheduler Contract Coverage

Focus: tests for explicit versus background parse failure behavior.

What changed:

- Added scheduler coverage proving `parseNow()` surfaces parser failures to
  awaiters.

Why:

- Iteration 6 intentionally distinguishes scheduled background parses from
  explicit parsing. Background parses are contained and reported through
  callbacks; direct `parseNow()` calls remain observable by the caller. This
  regression test pins that contract.

Verification:

- `dart format test/v2/flutter/flark_parse_scheduler_test.dart`
- `flutter test test/v2/flutter/flark_parse_scheduler_test.dart --reporter compact`: passed.

## Iteration 10 - Confidence Gate Closeout

Focus: verify the cumulative work as one integrated package change.

What changed:

- No additional production code changes in this loop.
- Closed the journal with broad package-level verification evidence.

Why:

- The previous iterations touched core transactions, projection, markdown input,
  native payload decoding, scheduler behavior, promoted widgets, public API
  tests, and docs. The final loop verifies those changes together rather than
  only through focused tests.

Verification:

- `./scripts/verify_package_confidence.sh`: passed. This covered package
  analysis, v2 core/markdown/projection/render-plan suites, selected Flutter
  controller/editor/preview/live-rendered/visual golden suites, Chrome web
  smoke coverage, example widget tests, native bridge tests, native packaging
  contracts, native parse adapter tests, and upstream CommonMark/GFM native
  contracts.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.

## Iteration 11 - Review Follow-Up and Large-Document Benchmarks

Focus: correctness issues from library review plus measurable performance
evidence for large markdown documents.

What changed:

- Routed live-rendered immediate parses through the editor's configured parse
  backend, markdown profile, and parse-error callback instead of a private
  Comrak-only singleton.
- Added regression coverage for standalone fence edits proving immediate
  live-rendered parses use the caller's backend/profile and report immediate
  parse failures through `onParseError`.
- Made `MarkdownEditor.cursorColor` theme-aware by default through
  `DefaultSelectionStyle`, while still preserving explicit caller colors.
- Made redo clearing explicit when recording a new history transaction and
  collapsed duplicated transaction offset-mapping branches into one documented
  rule.
- Added a large-document benchmark file covering text-buffer rebuilds,
  projection prediction with dense markers, render-plan construction, and a
  native Comrak parse/decode sample.
- Reduced Dart-side native parse-result mapping work by reusing mapped marker
  ranges, replacing repeated overlap scans with a sorted source-range index,
  and avoiding repeated marker-only block checks during block mapping.

Why:

- Immediate live-rendered parse adoption is part of the editing correctness
  path. It must honor the same parser configuration as scheduled parsing or
  tests/custom backends see different behavior from real editing.
- The review correctly identified large-document performance as the next risk.
  The new benchmarks show the current shape: buffer/projection/render-plan hot
  paths are in the sub-millisecond to low-millisecond range on this machine,
  while native parse/decode remains the dominant cost for 177K-character
  documents.

Benchmark evidence:

- Before the native mapper range-index pass, a one-shot
  `native_comrak_parse_decode_177540_chars` sample measured 4555.66ms.
- After the range-index pass, the same sample measured 1151.48ms in the final
  benchmark lane.
- Final large-document tracking results:
  - `text_buffer_replace_126k_middle`: median 322us, p95 759us.
  - `projection_predict_5000_markers`: median 3.64ms, p95 7.44ms.
  - `render_plan_5000_blocks_5000_inlines`: median 10.88ms, p95 25.54ms.
  - `native_comrak_parse_decode_177540_chars`: one-shot 1151.48ms.

Verification:

- `flutter analyze lib/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_projected_editable_text.dart lib/src/v2/core/history/flark_history_stack.dart lib/src/v2/core/transaction/flark_transaction.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/performance/flark_v2_large_document_benchmark_test.dart`: passed.
- `flutter test test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/flutter/flark_markdown_surface_test.dart --reporter compact`: passed with 37 tests.
- `flutter test --tags benchmark test/v2/performance --dart-define=FLARK_BENCHMARK_ENFORCE_BUDGETS=true --reporter compact`: passed with 8 tests.
- `flutter analyze lib test`: passed.
- `flutter test test --exclude-tags benchmark --reporter compact`: passed with
  545 tests.
