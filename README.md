# sovereign_editor

Flutter package for the Sovereign live markdown editor and read-only markdown
previewer.

Native bridge crate path:

- `native/comrak_bridge`

## Read-only markdown view

`SovereignMarkdownView` is the package read-only sovereign surface. It reuses
the sovereign controller parse/render pipeline and Tier1 block painting
(quotes + fenced code) for edit/read visual parity.

```dart
SovereignMarkdownView(
  markdown: '# Hello\\nSome text',
  profile: MarkdownSyntaxProfile.commonMarkCore,
  selectable: true,
  theme: const SovereignEditorThemeData(),
  showLinkActionsOverlay: true,
  onOpenLink: (url) async {},
  onEditInlineTarget: (context, label, url, isImage) async {},
)
```

## Native comrak backend

The runtime parser is native comrak only on supported native platforms.
If the bridge cannot load, editor initialization fails fast.
Web is not supported by the native parser path.

### Startup preflight (recommended)

Before constructing editor/controller instances, apps can preflight the native
bridge and log/show actionable diagnostics:

```dart
final preflight = preflightNativeComrakBridge();
if (!preflight.isAvailable) {
  debugPrint(preflight.error.toString());
}
```

The error includes platform, candidate paths (desktop/debug), and remediation
steps (build script + platform-specific packaging checks).

## Theming (editor-first API)

Use `SovereignEditorThemeData` to style the editor as a cohesive unit (inline
markdown text, quote rails, code blocks, and the fenced language picker).

```dart
SovereignEditor(
  controller: controller,
  wrapText: true,
  theme: const SovereignEditorThemeData(
    textStyle: TextStyle(
      color: Color(0xFFEAECEF),
      fontSize: 15,
      height: 1.45,
    ),
    cursorColor: Color(0xFF7AA2F7),
    inlineText: SovereignInlineTextTheme(
      bold: TextStyle(fontWeight: FontWeight.w700, color: Color(0xFFFFD166)),
      link: TextStyle(
        color: Color(0xFF7AA2F7),
        decoration: TextDecoration.underline,
      ),
      inlineCode: TextStyle(
        fontFamily: 'JetBrainsMono',
        color: Color(0xFFB8F2E6),
      ),
    ),
    blockquote: SovereignBlockquoteTheme(
      railColor: Color(0xFF7AA2F7),
      railWidth: 3,
      railInset: 10,
      railRadius: 2,
    ),
    codeBlock: SovereignCodeBlockTheme(
      backgroundColor: Color(0xFF141A22),
      languagePicker: SovereignFenceLanguagePickerTheme(
        backgroundColor: Color(0xFF1E2632),
        borderColor: Color(0xFF3A4A60),
        textStyle: TextStyle(color: Color(0xFFF5F7FA), fontSize: 12),
        menuTextStyle: TextStyle(color: Color(0xFFF5F7FA)),
      ),
    ),
  ),
)
```

This replaces ad hoc code-fence/quote styling params. Prefer the theme object
for all visual customization.

## Native build workflow (recommended)

After **any** Rust/ABI change in `native/comrak_bridge`, run one command:

```bash
./scripts/build_comrak_all.sh
```

What this does:

- host build (`cargo build --release`) for desktop/native FFI testing
- iOS XCFramework build (on macOS hosts)
- Android JNI `.so` staging (when `ANDROID_NDK_HOME` or `ANDROID_HOME` is set)

Useful options:

```bash
./scripts/build_comrak_all.sh --strict
./scripts/build_comrak_all.sh --ios-only
./scripts/build_comrak_all.sh --android-only
./scripts/build_comrak_all.sh --host-only
```

`--strict` turns skipped targets into a failing build (good for CI/local release checks).

## One-shot CI gate (local)

Run the native editor verification gate (build + key tests + Android lib check):

```bash
./scripts/verify_native_editor_ci.sh
```

Useful options:

```bash
./scripts/verify_native_editor_ci.sh --skip-build
./scripts/verify_native_editor_ci.sh --android-verify
```

## Package confidence gate (fast local maintenance check)

For day-to-day editor work (Dart/UI/policy changes), use the package confidence
gate before jumping to the heavier native CI script:

```bash
./scripts/verify_package_confidence.sh
```

Useful options:

```bash
./scripts/verify_package_confidence.sh --skip-native
./scripts/verify_package_confidence.sh --full-suite
```

- `--skip-native`: useful when native artifacts are not built locally yet
- `--full-suite`: runs `flutter test test` in the package after the fast gate

## Tests

Core editor tests live in `test/`. Run them from the package root so local
fixture paths resolve correctly:

```bash
flutter test test/widgets/sovereign
```

Benchmarks are tagged and can be run separately:

```bash
flutter test --exclude-tags benchmark
flutter test --tags benchmark test/benchmarks
```

Enforced benchmark lane (budgets are failing assertions, not warnings):

```bash
./scripts/verify_benchmark_lane.sh
```

You can also run it from the confidence gate:

```bash
./scripts/verify_package_confidence.sh --benchmarks
```

App-level composer/route integration tests remain in the app root `test/` tree.

### Test stability guidance (widget tests)

- Prefer bounded polling helpers / explicit `pump(...)` loops for async editor
  reconciliation tests.
- Avoid broad `pumpAndSettle()` unless the test truly depends on all animations
  and microtasks quiescing.
- Prefer focus-node based activation over tapping arbitrary editor surfaces when
  keyboard shortcut behavior is the target (reduces hit-test warnings and flake).

## Public API surface (pragmatic contract)

Use `package:sovereign_editor/sovereign_editor.dart` for the supported package
surface:

- editor and preview widgets;
- `SovereignController`;
- command APIs and command result/capability models;
- theme types, including `SovereignMarkdownTheme`;
- syntax contracts for custom engines;
- native bridge preflight/load diagnostics.

Deep imports are acceptable in package tests. App code should avoid deep
imports unless a type has been explicitly documented as a supported secondary
library.

Rendering internals, scanners, parser adapters, undo/edit-diff internals, and
marker helpers are not considered stable app API and may move as the
projection/painter system evolves.

Phase 1 migration notes are tracked in
`docs/production_readiness/api_migration_2026-05-01.md`.

## Support matrix

Current markdown support coverage and prioritized gaps:

- `docs/architecture/sovereign_editor_how_it_works.md` (runtime walkthrough + module ownership)
- `docs/architecture/sovereign_editor_markdown_support_matrix.md`
- `docs/architecture/sovereign_editor_command_interface.md` (command API ownership + action catalog)

### When rebuild is required

- Rust bridge source/ABI/header changes: **yes**, run `build_comrak_all.sh`
- iOS-only native packaging updates: run `build_comrak_ios.sh`
- Android-only JNI packaging updates: run `build_comrak_android.sh`
- Dart-only editor/presentation logic: no native rebuild required

## Android native library build

Build and stage Android JNI libraries:

```bash
./scripts/build_comrak_android.sh
```

Apple Silicon note:

- if your selected NDK only has `prebuilt/darwin-x86_64`, install Rosetta or
  use an NDK with `prebuilt/darwin-arm64`.
- Rosetta install command:
  `softwareupdate --install-rosetta --agree-to-license`

Expected output paths:

- `native/comrak_bridge/dist/android/jniLibs/arm64-v8a/libsovereign_comrak_bridge.so`
- `native/comrak_bridge/dist/android/jniLibs/armeabi-v7a/libsovereign_comrak_bridge.so`
- `native/comrak_bridge/dist/android/jniLibs/x86_64/libsovereign_comrak_bridge.so`

Android prebuild should fail if those libs are missing.

## iOS XCFramework build

Build and stage the iOS XCFramework:

```bash
./scripts/build_comrak_ios.sh
```

Expected output path:

- `native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework`

### iOS link model (static, process symbols)

iOS does not support loading an arbitrary app-bundled `.dylib` at runtime for
this use case, so the bridge uses static linking:

- Rust produces `libsovereign_comrak_bridge.a` (inside the XCFramework slices).
- Xcode links that static archive into `Runner`.
- Dart FFI resolves symbols from the app image via `DynamicLibrary.process()`.

Project wiring details:

- The consuming app's Xcode project includes
  `native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework` in its
  Frameworks build phase.
- The consuming app includes an anchor C file that references
  `sovereign_comrak_bridge_version`, `sovereign_comrak_parse`, and
  `sovereign_comrak_response_free` to prevent dead-stripping of archive members.
