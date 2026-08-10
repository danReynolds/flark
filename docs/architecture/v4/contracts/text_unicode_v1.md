# Flark v4 text and Unicode contract v1

This contract fixes the text rules at every v4 boundary. Rust owns canonical
source as valid UTF-8 bytes. A committed edit preserves those bytes exactly:
Flark does not normalize Unicode and does not rewrite LF, CRLF, or bare CR.
Dart and Flutter use document UTF-16 coordinates only through revision-bound
conversion APIs; raw integers from different coordinate spaces are not
interchangeable.

Invalid UTF-8, unpaired host UTF-16 surrogates, and edits that split a Unicode
scalar are rejected before source or revision changes. NUL remains an exact
source byte even when the selected Markdown profile uses a replacement
character for semantic interpretation. Export always returns the original
committed byte sequence.

Host UTF-16 validation walks every code unit: a high surrogate must be followed
by exactly one low surrogate, and a low surrogate is never valid on its own.
Edit endpoints are rejected when they fall between that pair. The contract test
executes both unpaired-surrogate cases and the split-pair coordinate case; they
are not inventory-only labels.

Line starts are independently indexed in source bytes and document UTF-16.
LF, CRLF, and bare CR each terminate one line, with CRLF treated as one unit.
The executable matrix includes non-ASCII content so the byte and UTF-16 arrays
provably diverge and both arrays are recomputed rather than trusted as metadata.

Editor grapheme behavior is pinned to `characters` 1.4.1 and Unicode 16.0.0.
Grapheme queries are bounded. When the configured context cannot prove a
cluster boundary, the result is `needs_more_context`; it never guesses and
never scans the rest of the document synchronously. The executable cases and
invalid-input inventory live in
`test/fixtures/v4/unicode_matrix_v1.json`.
