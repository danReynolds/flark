# Changelog

## 0.1.0-dev.1

- Cut over to the v4 package split: `flark_core` owns the headless Dart/Rust
  runtime and `flark` owns the Flutter product surface.
- Added bounded, source-authoritative opening, editing, certification, semantic
  viewport, history, and lifecycle APIs through ABI 4.28.
- Added parser-authored literal-safe insertion envelopes, ABI 4.27's bounded
  closure/carry proof for immediate word/space successors, and fail-closed exact
  source rendering whenever result-revision semantics are not proven current.
- Added ABI 4.28 parser-authored projection edit cells: canonical plain ATX
  content supports arbitrary non-newline splices without losing the heading
  shell, and the first one-shot inline dependency cell keeps unrelated inline
  projection while exposing only an invalidated Strong closure exactly.
- Replaced the active release, archive, platform, and documentation entry points
  with v4-only equivalents. Superseded release notes are preserved at
  `legacy/docs/v2_v3/CHANGELOG.md`.
