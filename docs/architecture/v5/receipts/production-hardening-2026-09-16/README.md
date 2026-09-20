# Production hardening receipts

See [the review](../../production_hardening_2026_09_16.md) for claims and limits.
Logs are gzip-compressed without timestamps; inspect with `gzip -dc FILE.log.gz`.

- `source-sha256.txt`: Dart production sources and Fleury Git dependency pins.
- `core-*`, `flutter-*`, `fleury-*`: analysis and package tests. The Flutter
  complete suite preceded extension of its missing-cell test to accessibility;
  `flutter-table-semantics` is the passing final targeted run.
- `fleury-framework-tests`: 1,309 widget tests on the exact dependency branch.
- `fleury-framework-current-main-tests`: the same suite on `19a8978c` plus the
  image API, before PR #262 merged. No unrelated checkout was changed.
- `web-build`: the final Git-pinned release browser build. Browser actions and
  the visually inspected frame are recorded in the review and task history.
- `native-semantics`: programmatic TextField/Flark comparison, not foreground
  qualification; the ephemeral local VM-service URL is redacted.
- `native-main-build`: the ordinary macOS entry point restored after diagnosis.
- `image-responsiveness.json`: one synchronous and three queued native samples.
  `image_responsiveness.dart` reproduces the diagnostic; its optional argument
  writes the synthetic PNG fixture to a path supplied by the caller.

Commands ran using Flutter 3.44.4 / Dart 3.12.2 on macOS 26.2 (M1 Pro). CI was
intentionally skipped. Framework analysis had four existing informational
diagnostics in unrelated files; all three Flark packages analyzed cleanly.

The old failing workbench gate remains in the separate production-audit receipt
directory. None of these receipts replaces that native failure with a pass.
