# flark_core

`flark_core` is Flark's headless Dart API. Canonical source, incremental GFM
parsing, certification, anchors, transactions, and bounded history live in the
Rust runtime behind the fixed C ABI; this package owns the isolate and safe
Dart boundary without importing Flutter.

The native runtime is compiled and bundled automatically by the Dart build
hook. Consumers normally open a document without supplying a library path:

```dart
final document = await FlarkCoreDocument.open(markdown);
```

An explicit `libraryPath` remains available as a test and embedding override.
