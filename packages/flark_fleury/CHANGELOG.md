# Changelog

## 0.5.1

- A standalone image's label shows only where the editor paints a caret:
  `FlarkView` and an editor without focus show a leading image without its
  label, and no longer offer it to accessibility as a link or to a press.

## 0.5.0

First public preview. Requires fleury 0.1.

- `FlarkEditor` and `FlarkMarkdown` for Fleury terminal and browser apps over
  the shared Flark kernel, with native cell styles, a composer toolbar,
  theming, and CodeMirror highlighting through `flark_codemirror`.
