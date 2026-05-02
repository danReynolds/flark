# Changelog

## 0.1.0 - Unreleased

Initial standalone `sovereign_editor` package hardening release.

- Extracted the editor and read-only previewer into a standalone Flutter
  package workspace.
- Established the public `package:sovereign_editor/sovereign_editor.dart`
  barrel for editor, preview, controller, command, theme, syntax, and native
  diagnostics APIs.
- Added native `comrak` parsing through the package FFI/native-assets flow, with
  iOS XCFramework and Android JNI packaging verification.
- Added the example app with editable and read-only Sovereign surfaces.
- Hardened controller, rendering, syntax, native bridge, markdown structure, and
  command module boundaries.
- Completed the Phase 4 markdown support policy for thematic breaks, indented
  code, escapes/entities, raw HTML text-only behavior, reference links,
  images/media previews, and source-first GFM tables.
- Added release, confidence, benchmark, native packaging, and example packaging
  verification scripts.

This package remains unpublished while `publish_to: none` is set and release
owner decisions are pending.
