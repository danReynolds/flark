# Changelog

## 0.5.0

First public preview.

- `FlarkEditor` and `FlarkMarkdown`: live Markdown editing and read-only
  display that keep the source exactly as written, on Flutter's native
  platforms and the web (Wasm and JavaScript).
- Touch input: a tap places the caret when the finger lifts, so scrolling never
  moves it; long press and double tap select words; the platform's handles,
  magnifier and Cut / Copy / Paste / Select all menu act on the selection.
- Code blocks highlight and indent through `flark_codemirror`. Links and
  images open replaceable controls, and theming covers typography, blocks and
  syntax colors.
