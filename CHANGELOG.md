# Changelog

## 0.1.0-dev.1

- Cut over to the v4 package split: `flark_core` owns the headless Dart/Rust
  runtime and `flark` owns the Flutter product surface.
- Added bounded, source-authoritative opening, editing, certification, semantic
  viewport, history, and lifecycle APIs through ABI 4.26.
- Added parser-authored literal-safe insertion envelopes and fail-closed exact
  source rendering while result-revision semantics are pending.
- Replaced the active release, archive, platform, and documentation entry points
  with v4-only equivalents. Superseded release notes are preserved at
  `legacy/docs/v2_v3/CHANGELOG.md`.
