# Inline whitespace dogfood failure — 2026-09-07

Owner report: create bold `what`, Backspace to `wha`, then type a space.
The rendered editor displayed the literal markers. This is an ordinary editing
sequence and a missed acceptance case.

## Cause and missed evidence

The regression reproduces directly in `FlarkEditor`, independently of Flutter
and the recent paragraph-cache work. Insertion committed `**wha **`. The
Markdown parser correctly treats that spelling as literal text because the
closing emphasis delimiter follows whitespace. The merged kernel already had
the unguarded insertion path; this was not introduced by paragraph reuse.

Existing tests checked Backspace within bold and spaces at an outside anchor
separately. They also checked deletion that exposes existing whitespace and
deleting the entire styled owner. None required the nonempty formatted word,
partial deletion, newly inserted whitespace and next word as one journey.

The randomized matrix asserts legal carets and agreement between source,
parser and projection. It has no independent formatting-intent oracle, so
literal `**wha **` can satisfy all those invariants. Sustained code-fence input
does not cover this prose transition. Earlier green counts and exploratory
browser checks justified too much confidence in ordinary inline editing.

## Correction

Insertion, replacement and deletion share the existing normalization of
parser-owned emphasis/strong/strike edges. Edge whitespace moves outside
delimiters in the same source/caret/history transaction. A surviving span's
typing intent continues through spaces, while whitespace after an emptied
owner retains the established exit behavior. Erasing separating spaces can
return to the surviving span instead of creating adjacent marker pairs.
The Rust parser remains the recognition authority; source mode remains literal.

The regression starts with the actual formatting command and character-by-
character authoring. A behavior matrix covers seven spellings/style combinations,
prose/quote/list/heading contexts, both starting anchors, repeated spaces,
following letters and Undo/Redo at each step. It asserts independently expected
source, visible transcript, source/display caret, character styles and block
membership. Leading/interior whitespace, code/link spans, replacement and
literal source editing provide neighboring controls.

Mounted tests exercise both full-value and delta platform input. A Wasm browser
regression exercises the real shortcut and DOM input transport, checking every
affected paint. The browser acceptance checklist now explicitly requires the
whole formatting journey. This closes the reported case and its tested
neighbors; it does not establish exhaustive intent coverage for every Markdown
collision or close the separate native/device qualification gates.

## Validation

Local results: 572 kernel tests, 799 Flutter host tests, and all eight real
browser input regressions passed. The final expanded whitespace file passed
112 cases, including the additional Paste and ReplaceRange variants. Analysis
of the core, host and workbench passed; `git diff --check` passed. The normal
`flutter build web --wasm` build succeeded.

In an isolated workbench on the rebuilt release web application (1280 × 720,
Codex embedded browser), actual shortcut/typing actions reproduced the reported
journey and verified `**wha** ` with a visible bold word and caret after the
space. A further space, Backspace, `next`, Undo and Redo retained visible bold
`wha next` and exact source `**wha** **next**` within the surrounding sentence.
The owner's preview was refreshed; its reported line and 1,077-byte saved draft
remain present, with no observed startup warning/error. Existing malformed
Markdown is preserved on reload rather than silently rewritten.

Changes remain local on `codex/v5-editor-qualification`; CI is skipped at the
owner's request. The earlier native timing/lifecycle gates remain open.
