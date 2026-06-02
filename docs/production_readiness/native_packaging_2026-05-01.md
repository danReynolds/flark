# Sovereign Native Packaging Plan

Status date: 2026-05-01

## Decision

Sovereign remains a Dart/Flutter FFI package, not a Flutter plugin, for this
release-planning pass.

Reasons:

- The package needs a Rust C ABI parser bridge, not platform channels.
- The public editor/preview API remains pure Dart/Flutter.
- Dart build hooks and code assets can compile and bundle native dynamic
  libraries for the supported dynamic-loading platforms.
- A Flutter plugin can be reconsidered only if iOS static linking or app-level
  project mutation cannot be handled cleanly by native assets and documented
  consumer wiring.

## Build Hook

`hook/build.dart` is the package native-assets entry point. It uses:

- `package:hooks/hooks.dart`
- `package:code_assets/code_assets.dart`
- the Rust crate at `native/comrak_bridge`

The hook emits the asset ID:

- `package:sovereign_editor/src/v2/native/native_comrak_ffi.dart`

This matches the Dart library that owns the native bridge contract.

When Rust is discovered through `rustup`, the hook resolves the `stable`
toolchain's `cargo` and `rustc` binaries directly, installs missing target
standard libraries for that same toolchain, and sets `RUSTC` for Cargo builds.
This avoids mixing a Homebrew `rustc` with rustup-installed Android targets.

Supported hook outputs:

- macOS arm64/x64: `DynamicLoadingBundled` `.dylib`
- Linux arm64/x64: `DynamicLoadingBundled` `.so`
- Android arm/arm64/x64: `DynamicLoadingBundled` `.so`
- iOS: `LookupInProcess` declaration only

iOS remains process-linked for now because the current bridge uses a static
XCFramework and Dart's `StaticLinking` code-asset mode is not supported by the
SDK yet. The existing `build_comrak_ios.sh` flow remains the iOS packaging path
until that changes or a plugin requirement is proven.

Unsupported targets:

- Web: no `dart:ffi` runtime.
- Windows/Fuchsia/RISC-V/IA-32: no committed artifact layout or CI coverage yet.

## FFI Binding Generation

Do not introduce `ffigen` in this phase.

The C ABI currently has one struct and five exported functions:

- `sovereign_comrak_bridge_version`
- `sovereign_comrak_input_alloc`
- `sovereign_comrak_input_free`
- `sovereign_comrak_parse`
- `sovereign_comrak_response_free`

The hand-written Dart FFI binding is smaller than a generated binding and is
covered by the native bridge tests. Revisit `ffigen` only if the ABI grows,
adds nested structs/unions, or starts sharing more generated declarations with
the Rust header.

## Artifact Layout

Hook-managed dynamic artifacts are copied to the hook output directory, for
example:

- `.dart_tool/hooks_runner/shared/sovereign_editor/build/<checksum>/sovereign_comrak_bridge/libsovereign_comrak_bridge.dylib`
- `.dart_tool/hooks_runner/shared/sovereign_editor/build/<checksum>/sovereign_comrak_bridge/libsovereign_comrak_bridge.so`

Developer scripts still produce local artifacts used by white-box tests and
manual mobile packaging:

- host: `native/comrak_bridge/target/release/libsovereign_comrak_bridge.dylib`
- host: `native/comrak_bridge/target/release/libsovereign_comrak_bridge.so`
- Android: `native/comrak_bridge/dist/android/jniLibs/<abi>/libsovereign_comrak_bridge.so`
- iOS: `native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework`

## Example Harness

`example/` is the package's mobile integration harness. It depends on
`sovereign_editor` through `path: ..`, imports only the top-level public barrel,
and exposes `SovereignMarkdownEditor` with `SovereignMarkdownPreview`.

Android verification:

```bash
./scripts/verify_example_packaging.sh --android
```

This runs the example Gradle task
`:app:verifySovereignComrakNativeLibs`, which builds the debug APK and fails
unless the APK contains `lib/**/libsovereign_comrak_bridge.so`.

iOS verification:

```bash
./scripts/verify_example_packaging.sh --ios
```

This checks that `Runner/SovereignComrakAnchor.c` references the exported
bridge symbols, that the Runner target compiles that source file, that the
project links `sovereign_comrak_bridge.xcframework`, and that Xcode can parse
the workspace. Use `--strict-ios` when the built XCFramework must already exist.

## Consumer Integration

For macOS, Linux, and Android app builds:

1. Depend on `sovereign_editor`.
2. Ensure the build machine has Rust available on `PATH`.
3. For Android, ensure `ANDROID_NDK_HOME`, `ANDROID_NDK`, `ANDROID_NDK_ROOT`,
   `ANDROID_NDK_LATEST_HOME`, or `ANDROID_HOME` points to an installed NDK.
4. Build the app normally. The Dart/Flutter build hook compiles and bundles the
   native library as a code asset.
5. Optionally call `SovereignNativeComrakParseBackend.preflight()` before
   constructing editor instances to surface bridge-load diagnostics.

For iOS app builds:

1. Build `native/comrak_bridge/dist/ios/sovereign_comrak_bridge.xcframework`
   with `./scripts/build_comrak_ios.sh`.
2. Link the XCFramework in the consuming app.
3. Include an anchor C file that references the exported bridge symbols so
   the static archive members are not stripped.
4. Rebuild/reinstall the app and use
   `SovereignNativeComrakParseBackend.preflight()` for diagnostics.

## References

- Dart hooks: https://dart.dev/tools/hooks
- `hooks` package: https://pub.dev/packages/hooks
- `code_assets` package: https://pub.dev/packages/code_assets
