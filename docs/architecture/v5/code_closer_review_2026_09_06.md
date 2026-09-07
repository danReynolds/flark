# Automatic code-closer follow-up

Typing `}`, `]` or `)` on an indented blank code line now aligns it with its
matching opener. This completes the owner's reported loop: type an opening
brace, press Enter, type the closing brace, then continue typing. The previous
[code editing candidate](code_editing_review_2026_09_06.md) supplied Enter and
explicit Shift-Tab but did not implement automatic closer outdent.

## Mechanism and scope

One pure Dart delimiter matcher serves the registered language tokenizers. It
balances braces, brackets and parentheses while skipping string, comment and
regex token ranges, including inherited literal ancestry. It maps the opener
back through parser-owned code content to its actual leading whitespace. It
preserves quote/list prefixes, and the closer plus whitespace change publish
as one logical Undo action. The host paints the canonical caret immediately;
a duplicate platform delivery cannot restore the removed spaces.

This is systematic within a bounded contract, not arbitrary-language formatting.
Every registered grammar has a declared closer example in `code_closer_test.dart`.
Eleven demonstrate shared closer alignment; YAML deliberately stays literal
because its bundled grammar labels flow punctuation as string text. Plain text,
unknown languages, mismatched/unmatched delimiters and uncertain literal
contexts retain indentation. Python's colon rule remains explicit. Keyword
closures such as Ruby `end` and shell `fi` require language-specific support
and complete authoring sequences before claiming them. Detection is heuristic;
the manual language override remains available.

Paste, selection replacement, composition and source-mode edits are literal.
Browser dogfooding found that a pasted single closer could lose that intent in
the platform text diff. The existing focused-editor clipboard binding now owns
browser paste alongside copy/cut, dispatches the core Paste command, and prevents
the raw DOM insertion. The Chrome regression checks one revision, every paint,
next input, Undo and handler removal on unmount.

## Reflection and parser correction

The missing outdent was a product-contract and test-sequence gap. Our previous
tests established that Enter and Shift-Tab worked, but stopped before the normal
next action: typing the closing delimiter. The new tests cover that complete
cycle, nested pairs, literals, tab indentation, containers, history and host
redelivery. Checking generic language capabilities makes omissions explicit;
it does not prove a tokenizer recognizes every language construct.

Adding braces to the generated editing alphabet also exposed a separate table
coordinate defect. Comrak parses each body row at its own first nonspace byte,
but assigns cell and inline columns relative to the header's origin. The old
literal-based repair could accept a repeated character at the wrong position;
literal tabs then masked the mismatch as a replacement run. Display looked
plausible while distinct caret positions collapsed together.

The Rust adapter now translates that column origin before slicing literals or
accounting for escaped pipes. It replaces the old content-from-repaired-runs
heuristic. A minimized source/typing/Undo regression and 256 Rust combinations
cover piped/pipeless tables, indentation, quotes/lists, LF/CRLF and Unicode.
An older strong-range rejection is now an exact successful parse. ABI and Dart
transport rejection tests use the declared table-alignment capacity boundary;
the transactional edit-rejection test uses its existing synthetic backend.
No validation rule was disabled to accommodate the correction.

## Evidence

Final local suites: **432 core, 117 Flutter host, 25 workbench and 5 real Chrome
transport tests** (579 total), plus **43 Rust tests**. The core run retains the
expanded 1,000-history matrix at seed 2028. Native, bundled Wasm and freshly
built Wasm produce identical models across all 1,322 transport fixtures. All
three analyzers and changed-file formatting pass.

The normal Flutter web/Wasm build was exercised in the embedded browser using
the owner's exact untagged loop, next-character/Undo/Redo, nested closures,
literal comments, plain text and clipboard paste. Test edits use the disposable
Draft; it is cleared afterward and the owner's Tour is restored unchanged.
Build-specific served hashes and observations are recorded in the new
[candidate receipt](receipts/2026-09-06-closer/candidate.json). Earlier receipts
remain sealed.

This qualifies a dirty local candidate for exploratory D0-web dogfooding.
It does not close native/AppKit/IME, device performance, the browser matrix,
Dune, Fleury, CI or release gates.
