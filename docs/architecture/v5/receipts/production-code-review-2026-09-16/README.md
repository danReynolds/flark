# Dedicated review receipts

Scope: Flark `50e13d4..a252c5a`, followed by the corrections documented in
[the review](../../production_code_review_2026_09_16.md). Local only; no CI.

- `table-red.log.gz`: three formatting regressions fail on the original code.
- `table-final.log.gz`: all 34 focused table tests pass after correction.
- `image-native-red.log.gz`: two of three orientation regressions fail.
- `image-browser-red.log.gz`: all three orientation regressions fail in Chrome.
- `image-native-green.log.gz`: orientation and image lifecycle/queue tests pass.
- `image-browser-green.log.gz`: all three orientation regressions pass in Chrome.
- `core`, `flutter`, `fleury`: final analysis and complete test-suite logs.
- `web-build.log.gz`: final release browser compilation and candidate path.
- `table-probe.log.gz`: diagnostic traversal of 264 admitted short-row cases.
- `source-sha256.txt`: hashes of changed production source/configuration/assets
  in the reviewed commits, including the final fixes; excludes receipts.

Permanent regressions are in `packages/flark/test/missing_table_cells_test.dart`
and `packages/flark_fleury/test/image_orientation_test.dart`. From their respective
package directories, run `dart test test/missing_table_cells_test.dart` and
`dart test test/image_orientation_test.dart`; run the latter with `-p chrome`
to exercise the browser decoder. No physical-device proof is implied.
