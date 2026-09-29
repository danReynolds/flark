# Changelog

## 0.5.0

First public preview.

- Highlighting and indentation for code blocks in 25 languages: CodeMirror 5
  and `@codemirror/legacy-modes` ported to pure Dart and checked token for
  token against the upstream modes.
- Untagged code blocks are detected from per-language signs; fence names
  resolve through common aliases (`js`, `py`, `sh`, `yml`, …).
- `FlarkCodeMirror.only([...])` keeps the languages an app does not name out
  of its build.
