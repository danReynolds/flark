# Local theming candidate receipts

Captured 2026-09-08 from `codex/v5-links-images`, base
`46304ddd6522fa974376161a1af9072ecc900f28`. Changes are local and uncommitted.
No CI was run. Flutter 3.44.4 / Dart 3.12.2 on macOS.

Commands ran with `/Users/dan/Coding/flutter_arm64/bin/flutter`:

From `packages/flark_flutter`:

```sh
flutter analyze --no-pub
flutter test --no-pub
```

From `packages/flark_flutter/example`:

```sh
flutter analyze --no-pub
flutter test --no-pub
flutter test --no-pub .dart_tool/flark_theme_export_test.dart
flutter test --no-pub --platform chrome test/web_input_test.dart test/tree_sitter_web_test.dart test/code_font_web_test.dart
flutter build web --wasm
flutter build macos --profile
```

The ordinary example test run generates the exported public consumers and
export test in ignored `.dart_tool`; the next command compiles and runs them.
The final host/example/consumer/Chrome counts are 843 / 30 / 2 / 26.
`review-red` and `fence-link-red` preserve the failing unchanged-reference save
and code-fence Cmd-K cases. `review-green` is the targeted 19-test resource and
theme pass after correction. The complete final pass followed that correction.

Logs are gzip compressed. `source-sha256.json` covers host/example code,
tests, assets, tooling and dependency manifests; `build-sha256.json` records
normal release-web entry points and the normal profile-app executable.

All 33 core/parser entries in the preceding resource source receipt
still matched at capture. Its kernel/parser/conformance evidence is retained;
those suites were not repeated for the theme-only edits.

Hands-on release-browser acceptance is pending a Mac unlock. Headless Chrome
regressions and successful builds do not close that gate or native performance,
OS input, lifecycle, physical-device, packaging or release qualification.
