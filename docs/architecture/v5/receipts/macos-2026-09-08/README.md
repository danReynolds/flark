# macOS qualification receipts — 2026-09-08

The complete retry and workbench run used unchanged tracked source at
`25e6190aa4fe9bfc7588d12b5f39d87ed658be72`. `retry-candidate.json` records the
prior untracked documentation separately. Local macOS profile evidence; CI skipped.

From `packages/flark_flutter/example`:

```sh
caffeinate -dis flutter drive --profile -d macos \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/frame_profile_test.dart \
  --dart-define=FLARK_PROFILE_BOUNDED=true \
  --dart-define=FLARK_PROFILE_APP=true
caffeinate -dis flutter drive --profile -d macos \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/workbench_profile_test.dart
flutter build macos --profile --target lib/main.dart
flutter run --profile -d macos --target integration_test/input_comparison.dart \
  --dart-define=FLARK_TRACE_INPUT=true
```

- `frame-interrupted.*`: earlier 22-case run rejected on foreground loss.
- `frame-retry.*`: complete 24-case pass; the result JSON is the fresh driver
  output, copied before the next build. Summaries omit raw frame samples.
- `workbench.*`: 100 cycles and 3,000 edits; one sustained-insertion p99 failure.
  Result JSON is reconstructed from the full receipt and teardown samples in
  redirected stdout. A failed integration run does not replace the driver JSON,
  so that file was not reused as workbench evidence.
- `*-build.json`: app contents, AOT frameworks, native libraries and build-stamp
  hashes captured before the next build. `normal-final` is the ordinary app used
  for native input; `input-comparison` is the separate traced diagnostic.
- `native-observations.json`: exact synthetic source/caret observations and
  limits; the original Draft was restored before opening the comparison.
- `diagnostics-analysis.log.gz`: analysis of the later profiling-only timing
  fields. These changes do not retroactively add timing detail to the earlier run.

Local VM-service URLs are redacted from the process logs; sample timestamps,
frame numbers and measurements are preserved.

The Mac locked during the comparison, before its repeat and remaining native
canaries. This is not completed native input, IME, VoiceOver or OS lifecycle
qualification. The predeclared limits remain unchanged; see
[the review](../../macos_review_2026_09_08.md).

The diagnostic runner was stopped and `flutter build macos --profile --target
lib/main.dart` passed again. `normal-restored-build.json` and its log identify
the final ordinary app. The eight qualification tests passed;
`diagnostics-source.json` binds their analysis/test validation to the later
profiling-only file. No timing gate has been rerun or closed by those checks.

The landing self-review and fresh local analysis/eight-test rerun are retained in
`merge-review.json`, `merge-analysis.log.gz` and `merge-tests.log.gz`.
