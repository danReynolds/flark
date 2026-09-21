---
title: Release readiness
description: Implemented APIs and the remaining gates before package publication.
---

Flark is a development package, not a published stable release. The homepage
and usage guide exercise the widget/controller API from this repository.

## Implemented

- Automatic parser loading, owned widget sessions, optional controllers and retry.
- Matching Flutter/Fleury editor names, callable editing methods and state snapshots.
- Separate load and edit notifications, precise guarded source edits and undo.
- Dedicated read-only widgets sharing parsing and host rendering primitives.
- A live homepage composer, plus native and browser consumer entry points.

The source remains bounded: the default rich-rendering limit is 16 KiB of UTF-8
plus shape limits. Bigger or unsupported documents fall back explicitly. The
read-only fallback offers the complete Markdown for copying.

## Existing integrations

The default host imports now expose the new widget/controller names. Existing
advanced integrations can retain their API by importing
`flark_flutter_legacy.dart` or `flark_fleury_legacy.dart`. These are unpublished
development packages; no stable-version compatibility claim is made.

## Before package publication

1. **Distribution:** replace development path/Git dependencies with published
   compatible versions. Ship and verify native parser artifacts for each claimed
   target; a local Cargo build does not prove a registry download works without Rust.
   The macOS consumer build also reports architecture-dependent framework-name
   warnings for parser/snippet assets; resolve those before distributing binaries.
2. **Platform qualification:** close the remaining attended native IME,
   accessibility, focus/lifecycle and terminal gates. A browser test is not proof
   of those platform behaviors.
3. **Performance qualification:** retain the measured live-document bounds.
   Dedicated rendering avoids editing state; it does not establish faster parsing
   or larger-document support. Profile layout, paint, and memory after unmount on
   intended production targets before making those claims.
4. **Release:** select a version, review the exact package archives, run clean
   native/web consumer checks against those archives, then tag and publish.

Local tests, repository merging, website deployment and package publication are
separate outcomes. The packages retain `publish_to: none` until distribution is
ready. The implementation/review receipts live under `docs/architecture/v5/` in
the repository.
