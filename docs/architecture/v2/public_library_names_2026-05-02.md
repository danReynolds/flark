# Flark v2 Public Library Names

Status date: 2026-05-02

## Decision

Expose the experimental v2 surface from:

```dart
import 'package:flark/flark_advanced.dart';
```

Keep v1 as the stable default:

```dart
import 'package:flark/flark.dart';
```

## Rationale

- v2 is a source-first rewrite and should be easy to trial without changing the
  stable v1 import path.
- A separate v2 library lets examples, oracle tests, and early adopters exercise
  the new runtime before v2 becomes the default.
- Exporting from one library avoids public consumers importing `src/v2/*`
  implementation paths.

## Promotion Rule

`flark_advanced.dart` can become the default export only after v2 has:

- native parser protocol parity,
- editable/read-only widget parity,
- v1/v2 oracle coverage,
- release docs and migration notes,
- package-level API review.
