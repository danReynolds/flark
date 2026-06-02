# Fenced Code Support Plan

Date: 2026-05-10

## Goal

Flark v2 should make fenced code blocks feel like a strong Markdown editor,
not a language IDE. The source remains Markdown, the parser remains Comrak, and
the editor adds only portable text-editing behavior that is expected inside
code fences.

## Supported Editing Contract

- Hide fence markers in live editing while preserving exact source text for the
  opener, info string, body, and closer.
- Keep the language selector and syntax highlighting as rendered chrome over
  the fence, not as part of editable body layout.
- Treat the code body as plain editable text with selectable inline content,
  preserved blank lines, multiline paste, and normal undo.
- Enter inside a fence preserves the current line indentation and adds one
  sensible indent unit after common block openers.
- Enter on a trailing blank body line exits the fence, including unclosed fences.
- Typing `}`, `]`, or `)` on an indentation-only code line outdents by one
  inferred unit.
- Tab and Shift+Tab indent or outdent the current line or selected body lines.
- Up on the first visible code line and Down on the last visible code line move
  out to adjacent live blocks; arrows stay native inside multiline code bodies.

## Explicit Non-Goals

- No autocomplete, diagnostics, linting, bracket matching engine, comment
  toggles, format-on-paste, or language-specific indentation engine.
- No IDE-specific model inside the Markdown editor. Code fences are still source
  text ranges with semantic chrome, not embedded editors.
- No per-language parsing beyond syntax highlighting and the small
  language-aware colon indentation whitelist already used by the source policy.

## Architecture

- `FlarkMarkdownFencedCodeScanner` is the single source-level fence scanner
  for v2 editing. It identifies opener info, body range, closer, and shared line
  helpers without depending on Flutter.
- `FlarkMarkdownFencedCodePolicy` owns source mutations for fenced code:
  Enter, closer outdent, multiline paste indentation, Tab indentation, and
  Shift+Tab outdent.
- Flutter adapters map local editable selections into source ranges and call
  the source policy. Widgets do not parse fence syntax or own code semantics.
- Rendered interactions such as language selectors remain overlay chrome driven
  by `FlarkMarkdownInteractions` and source commands.
- Comrak remains the authoritative parse layer. The scanner exists only for
  edit-time local context where waiting for a parser cycle would make keyboard
  input unstable.

## Verification Matrix

- Scanner tests cover opener parsing, info string language extraction, tilde
  fences, longer closers, non-closing info-string fence lines, invalid backtick
  info strings, and opener/closer boundary null contexts.
- Headless policy tests cover Enter, blank-line exit, closer outdent, multiline
  paste indentation, trailing-newline paste indentation, and Tab/Shift+Tab
  operation construction.
- Source widget tests cover platform text insertion paths for Enter, Backspace,
  closer outdent, multiline paste, and IME undo grouping.
- Live rendered widget tests cover code fence rendering, language selector
  overlay, syntax highlighting, Enter height/blank lines, Tab/Shift+Tab,
  vertical boundary arrows, and multiline paste.
- Full confidence runs must include focused code-fence tests, markdown/flutter
  suites, `flutter analyze`, and `./scripts/verify_package_confidence.sh`.
