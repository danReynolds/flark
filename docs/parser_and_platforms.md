# Parser and Platforms

Flark uses Comrak as the default Markdown parser.

| Platform | Default backend |
| --- | --- |
| macOS | Native FFI bridge |
| iOS | Process-linked XCFramework bridge |
| Android | Native FFI bridge packaged in the app APK |
| Linux | Native FFI bridge |
| Web | Packaged Comrak WASM bridge |

The public widgets use `FlarkNativeComrakParseBackend` when `parseBackend` is
omitted. Load failures are surfaced directly; Flark does not silently switch to
a different Markdown implementation.

## Native Preflight

```dart
final preflight = FlarkNativeComrakParseBackend.preflight();
if (!preflight.isAvailable) {
  debugPrint(preflight.error.toString());
}
```

## Custom Parser

Apps with a custom parser policy can implement `FlarkMarkdownParseBackend` and
pass it to `MarkdownEditor` or `Markdown`.

```dart
MarkdownEditor(
  initialMarkdown: markdown,
  parseBackend: myBackend,
)
```

Parser output feeds projection and render-plan generation, so replacement and
hidden ranges must stay source-offset accurate.

## Native Artifact Names

The Dart package is named `flark`, but the native Rust bridge artifact still
uses the existing `sovereign_comrak_bridge` ABI and symbol names. Treat those
as internal package artifacts unless you are working on native packaging.
