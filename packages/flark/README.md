# Flark

`flark` is the Flutter product surface. Its headless Dart API lives in
`flark_core`; parser and source authority live in the Rust runtime behind the
fixed v4 ABI.

The editor uses a custom `RenderBox`, Flutter delta text input, a bounded
16 Ki UTF-16 input window, next-frame exact-source painting, and certified
incremental viewport rows.

Native Rust assets are built and bundled automatically by `flark_core`. A
consumer does not configure a library path:

```dart
final controller = await FlarkEditorController.open(markdown);
```
