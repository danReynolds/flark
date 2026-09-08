# Local resource/theme merge evidence

2026-09-08, Flutter 3.44.4 / Dart 3.12.2 / Rust 1.98.0, macOS. CI skipped.
Source and build hashes bind this receipt to the candidate; test logs are gzip.

From `packages/flark`: `dart analyze --fatal-infos`, `dart test`.
From `packages/flark_flutter`: `flutter analyze --no-pub`, `flutter test --no-pub`.
From `packages/flark_flutter/example`:

```sh
flutter analyze --no-pub
flutter test --no-pub
flutter test --no-pub .dart_tool/flark_theme_export_test.dart
flutter test --no-pub --platform chrome test/web_input_test.dart test/semantics_web_test.dart test/tree_sitter_web_test.dart test/code_font_web_test.dart
flutter build web --wasm
flutter build macos --profile
```

The ordinary example tests generate the exported consumers. The semantics test
and normal web build were repeated after removing a redundant deprecated flag.

From the repository root:

```sh
python3 native/flark_parse/tool/gen_schema.py
rustup run 1.98.0 cargo test --release --locked --manifest-path native/flark_parse/Cargo.toml
native/flark_parse/tool/verify_transports.sh --rebuild
rustup run 1.98.0 bash packages/flark/tool/verify_prebuilt_consumer.sh
git diff --check
```

Dart was on PATH for the prebuilt check; the check itself removes Rust from the
consumer PATH. This verifies the clean consumer hook, not published distribution.

The semantics-red log demonstrates the disabled DOM editor before correction.
The final normal release-browser canary enabled accessibility, typed into the
editor, changed a theme field, returned to styled text, opened/cancelled link
controls and continued input/Undo. No browser warning/error logs were observed.
This is not VoiceOver/TalkBack, native foreground, device or performance proof.
