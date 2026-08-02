# Parser and Platforms

The supported v2 engine uses Comrak as its default Markdown grammar backend.
Flark v3 keeps one Flark-owned grammar authority, using Rust on native and web,
but its long-lived isolate/Worker runtime is still an implementation preview.

| Consumer | Default backend and delivery |
| --- | --- |
| Dart VM on macOS/Linux | Native FFI bridge built by the `flark` package hook |
| Flutter on macOS/iOS/Android/Linux | The same native bridge, received transitively through `flark_flutter` |
| Dart web | Prebuilt Wasm supplied by the application as a URI, bytes, or lazy byte loader |
| Flutter web | The same Wasm engine; `flark_flutter` resolves its packaged asset into the engine loader contract |
| Windows | Not supported yet |

Load failures are surfaced directly. Flark does not silently switch to another
Markdown implementation.

## Build prerequisites

On native targets the engine package build hook (`hook/build.dart`) compiles
the bundled Rust bridge crate during a Dart or Flutter build:

- **Rust toolchain.** `cargo` or, preferably, `rustup` must be on `PATH`.
  Install it from <https://rustup.rs>.
- **Android additionally needs an NDK.** The hook locates it through the
  standard Android/NDK environment variables and SDK installation.
- **Web needs no Rust toolchain at application-build time.** The repository
  ships a prebuilt Wasm module.
- **Windows is not supported yet.** Supported native bridge targets are macOS
  arm64/x64, Linux arm64/x64, Android arm/arm64/x64, and iOS.

CI or container builds of native targets must install Rust. There is no
independent Dart Markdown parser fallback.

## Web module delivery

Dart web applications own the URL or bytes used to load the packaged module;
the engine does not import a Flutter asset API:

```dart
final backend = FlarkNativeComrakParseBackend.withNativeBridge(
  wasmSource: NativeComrakWasmUriSource(
    Uri.parse('/assets/flark_comrak_bridge.wasm'),
  ),
);
```

Use `NativeComrakWasmBytesSource` when the application already owns the
bytes, or `NativeComrakWasmBytesLoaderSource` for lazy delivery.
`flark_flutter` performs the Flutter asset-bundle translation automatically.

## Native preflight

```dart
final preflight = FlarkNativeComrakParseBackend.preflight();
if (!preflight.isAvailable) {
  print(preflight.error);
}
```

## Scheduling and widget tests

In the supported v2 Flutter adapter, native documents at or above the
configured isolate threshold parse away from the UI isolate. Smaller documents
may parse inline because the isolate round trip costs more than the parse.
Direct Dart callers own their own v2 scheduling policy.

A `testWidgets` body that drives a real large native parse must use
`tester.runAsync(...)`, or a test can temporarily raise the threshold through
`package:flark_flutter/flark_flutter_advanced.dart`.

The supported v2 web bridge runs in the calling JavaScript context. The v3
production gate replaces that with a long-lived Web Worker that owns Wasm; the
architecture does not count the current main-thread bridge as large-document
closure.

## Custom parser

Flutter apps with a custom v2 parser policy can implement
`FlarkMarkdownParseBackend` and pass it to `MarkdownEditor` or `Markdown`:

```dart
MarkdownEditor(
  initialMarkdown: markdown,
  parseBackend: myBackend,
)
```

This is a v2 compatibility extension point. A v3 document session always has
one grammar authority and does not combine custom predictions with parser
facts.

## Native artifact names

The Rust bridge artifact uses the `flark_comrak_bridge` ABI and symbol names.
Treat them as internal package artifacts unless you are working on native
packaging.
