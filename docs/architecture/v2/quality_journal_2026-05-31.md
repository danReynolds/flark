# Sovereign v2 Quality Journal

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

- Made `SovereignTransaction` sort operations stably by original operation
  order when multiple operations have the same source range.
- Added regression coverage for same-offset multi-insert transactions.

Why:

- The core transaction model permits multiple collapsed insertions at one
  original offset. Without a stable tie-breaker, their relative insertion order
  depended on `List.sort` implementation details. This could make command,
  projection, or live-editor transactions nondeterministic.

Verification:

- `dart format lib/src/v2/core/transaction/sovereign_transaction.dart test/v2/core/sovereign_transaction_test.dart`
- `flutter test test/v2/core --reporter compact`: passed.

## Iteration 3 - Projection Construction Cost

Focus: projection/cursor-mask performance and range-input robustness.

What changed:

- Converted `SovereignCursorMask` to validate hidden and replacement ranges
  once, then build projection spans from the validated lists.
- Let `SovereignProjection` construct its `cursorMask` from already validated
  ranges and spans instead of redoing validation work.
- Added regression coverage that cursor masks can be built from single-pass
  range iterables.

Why:

- Projection construction is on the parser-adoption and predictive-edit hot
  path. Revalidating the same ranges added unnecessary allocation/work and made
  the public mask constructor less tolerant of valid iterable implementations.

Verification:

- `dart format lib/src/v2/projection/sovereign_projection.dart test/v2/projection/sovereign_projection_test.dart`
- `flutter test test/v2/projection --reporter compact`: passed.
- `flutter test test/v2/performance/sovereign_v2_performance_budget_test.dart --tags benchmark --reporter compact`: passed.

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

- `dart format lib/src/v2/markdown/source/sovereign_markdown_input_engine.dart test/v2/markdown/sovereign_markdown_input_commands_test.dart`
- `flutter test test/v2/markdown/sovereign_markdown_input_commands_test.dart --reporter compact`: passed.

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

- `dart format lib/src/v2/native/native_comrak_ffi.dart test/v2/native/sovereign_native_comrak_bridge_test.dart`
- `flutter test test/v2/native/sovereign_native_comrak_bridge_test.dart --reporter compact`: passed.

## Iteration 6 - Parse Scheduler Failure Containment

Focus: Flutter controller/scheduler architecture and async lifecycle safety.

What changed:

- Added an optional `onError` callback to `SovereignParseScheduler`.
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

- `dart format lib/src/v2/flutter/sovereign_parse_scheduler.dart test/v2/flutter/sovereign_parse_scheduler_test.dart`
- `flutter test test/v2/flutter/sovereign_parse_scheduler_test.dart test/v2/flutter/sovereign_flutter_controller_test.dart --reporter compact`: passed.

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

- `dart format lib/src/v2/flutter/sovereign_markdown_editor.dart lib/src/v2/flutter/sovereign_markdown_preview.dart test/v2/flutter/sovereign_markdown_surface_test.dart`
- `flutter test test/v2/flutter/sovereign_markdown_surface_test.dart --reporter compact`: passed.

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

- `dart format test/public_api/sovereign_editor_barrel_test.dart`
- `flutter analyze lib/src/v2/flutter/sovereign_markdown_editor.dart lib/src/v2/flutter/sovereign_markdown_preview.dart lib/src/v2/flutter/sovereign_parse_scheduler.dart test/public_api test/v2/public_api`: passed.
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

- `dart format test/v2/flutter/sovereign_parse_scheduler_test.dart`
- `flutter test test/v2/flutter/sovereign_parse_scheduler_test.dart --reporter compact`: passed.

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
