---
title: Release readiness
description: What the 0.5 preview on pub.dev covers, and the qualification still open.
---

Flark 0.5.0 is a preview, published on pub.dev as four packages:
[flark](https://pub.dev/packages/flark), the editing kernel;
[flark_codemirror](https://pub.dev/packages/flark_codemirror), code-block
highlighting and indentation; and the hosts
[flark_flutter](https://pub.dev/packages/flark_flutter) and
[flark_fleury](https://pub.dev/packages/flark_fleury). Add a host to your app:

```yaml
dependencies:
  flark_flutter: ^0.5.0
```

The parser is a native library on Android, iOS, macOS, Linux and Windows,
downloaded for the target you build and checked against the SHA-256 the package
pins, and a Wasm module bundled in the package on the web. No Rust toolchain is
needed.

## Implemented

- Automatic parser loading, owned widget sessions, optional controllers and retry.
- Matching Flutter/Fleury editor names, callable editing methods and state snapshots.
- Separate load and edit notifications, precise guarded source edits and undo.
- Dedicated read-only widgets sharing parsing and host rendering primitives.
- A live homepage composer, plus native and browser consumer entry points.

The source remains bounded: the default rich-rendering limit is 32 KiB of UTF-8
on desktop and in desktop browsers and 16 KiB on phones and tablets, plus shape
limits. Bigger or unsupported documents fall back explicitly. The
read-only fallback offers the complete Markdown for copying.

## Before a stable release

1. **Platform qualification:** attended native input methods, screen readers
   (VoiceOver, TalkBack), focus and lifecycle, and terminal gates. A browser
   test is not proof of those platform behaviours.
2. **Performance qualification:** frame budgets on phones and other intended
   production targets, including layout, paint and memory after unmount.
3. **Known editing residuals:** rare edge cases recorded in the repository's
   hardening review, each with a reproduction.

Existing advanced integrations can keep their API by importing
`flark_flutter_legacy.dart` or `flark_fleury_legacy.dart`. 0.x releases make no
stable-version compatibility claim.
