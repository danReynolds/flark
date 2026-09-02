# API surface

Flark v4 exposes two supported library barrels and no compatibility barrel.
Anything under `lib/src/` is implementation detail.

## Flutter applications

```dart
import 'package:flark_flutter/flark_flutter.dart';
```

The `flark_flutter` barrel exports its Flutter surface and the complete
supported headless Dart surface. Primary types are:

- `FlarkEditorController`: document/controller lifecycle, selection, history,
  edits, paging, streaming open, source reads, and a typed snapshot listenable.
- `FlarkEditorSnapshot`: one immutable bounded state for Flutter layout,
  paint, hit testing, semantics, status, and command capabilities.
- `FlarkEditor`: the continuously rendered editable custom render surface.
- `FlarkMarkdownView`: the read-only surface sharing the same controller.
- `FlarkEditorStatus`, `FlarkViewportRow`, and the presentation/receipt models
  re-exported from Core.

Open with `FlarkEditorController.open` or `openUtf8Stream`, and always finish
with `close`. The optional `libraryPath` argument is a test/embedding override;
normal consumers use the automatically bundled native asset.

## Headless Dart

```dart
import 'package:flark/flark.dart';
```

`FlarkCoreDocument` is the source-authoritative document API. It exposes exact
source reads, UTF-16 edits and transactions, bounded viewport queries,
certification, anchors, history, semantic edit intents, and streamed open.
`FlarkCoreEditorSession` adds host-neutral editing/history policy.

Open with `FlarkCoreDocument.open` or `openUtf8Stream`, and always finish with
`dispose`.

## Authority boundary

Application code owns values, layout, focus placement, and visible error UI.
Rust owns canonical source, GFM grammar, source-to-projection identity,
certification, and semantic mutation receipts. Dart and Flutter do not infer
Markdown syntax to authorize projected edits.

The internal ownership and dependency rules are recorded in
[Flutter editor runtime boundaries](architecture/v4/flutter_runtime_boundaries.md).
