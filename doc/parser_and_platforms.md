# Parser and Platforms

Flark uses Comrak as the default Markdown parser.

| Platform | Default backend |
| --- | --- |
| macOS | Native FFI bridge |
| iOS | Process-linked XCFramework bridge |
| Android | Native FFI bridge packaged in the app APK |
| Linux | Native FFI bridge |
| Web | Packaged Comrak WASM bridge |
| Windows | Not supported yet |

The public widgets use `FlarkNativeComrakParseBackend` when `parseBackend` is
omitted. Load failures are surfaced directly; Flark does not silently switch to
a different Markdown implementation.

## Build Prerequisites

On native targets the package build hook (`hook/build.dart`) compiles the
bundled Rust bridge crate during `flutter build`/`flutter run`:

- **Rust toolchain.** `cargo` (or `rustup`, which is preferred — the hook
  resolves toolchains and adds missing cross-compile targets through it) must
  be on `PATH`. Install via <https://rustup.rs>.
- **Android additionally needs an NDK.** The hook locates it through
  `ANDROID_NDK_HOME`, `ANDROID_NDK`, `ANDROID_NDK_ROOT`,
  `ANDROID_NDK_LATEST_HOME`, or `ANDROID_HOME` (with an installed NDK).
- **Web needs no extra tooling** — the WASM bridge ships prebundled in the
  package.
- **Windows is not a supported target yet.** The hook fails with an explicit
  unsupported-target error; supported native targets are macOS arm64/x64,
  Linux arm64/x64, Android arm/arm64/x64, and iOS.

CI or container builds of native targets must install Rust in the build
image; there is no pure-Dart parser fallback.

## Native Preflight

```dart
final preflight = FlarkNativeComrakParseBackend.preflight();
if (!preflight.isAvailable) {
  debugPrint(preflight.error.toString());
}
```

## Threading and Widget Tests

On FFI platforms (macOS, iOS, Android, Linux), documents of 4 KiB of UTF-8
or more parse on a worker isolate so the synchronous native parse cannot
block the UI isolate; smaller documents parse inline because the isolate
round trip would cost more than the parse. The browser/WASM bridge is
unaffected.

One consequence for **widget tests**: `flutter_test`'s fake-async zone never
drains a worker isolate's reply port, so a `testWidgets` body that drives a
real native parse of a large document will hang. Either wrap the parse in
`tester.runAsync(...)`, or raise the threshold to force synchronous parsing
for the test (import `package:flark/flark_advanced.dart`):

```dart
final previous = flarkNativeParseIsolateThresholdBytes;
flarkNativeParseIsolateThresholdBytes = 1 << 30; // force inline parsing
addTearDown(() => flarkNativeParseIsolateThresholdBytes = previous);
```

## Custom Parser

Apps with a custom parser policy can implement `FlarkMarkdownParseBackend` and
pass it to `FlarkMarkdownEditor` or `Markdown`.

```dart
FlarkMarkdownEditor(
  initialMarkdown: markdown,
  parseBackend: myBackend,
)
```

Parser output feeds projection and render-plan generation, so replacement and
hidden ranges must stay source-offset accurate.

## Native Artifact Names

The native Rust bridge artifact uses the `flark_comrak_bridge` ABI and symbol
names. Treat those as internal package artifacts unless you are working on
native packaging.
