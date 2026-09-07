# Four-language authoring checkpoint — 2026-09-06

Historical ABI 2 checkpoint. The subsequent optimization and worker proposal,
current test counts and receipts are in [PERFORMANCE_REVIEW.md](PERFORMANCE_REVIEW.md).

The functional part of milestone 2 is implemented. The shared Rust adapter now
returns a single edit proposal through native FFI and Wasm; the Dart API validates
its ranges and resulting selection. The package still has no Flutter dependency.
Combined typing performance remains open, and the running Flark editor has not
adopted this package.

## Behavior and evidence

The four-language corpus contains 84 hand-authored scenarios. Each checks the
exact resulting source, selection and the following typed character. It covers:

- Enter and typed bracket outdent, including the owner's Dart loop.
- Splitting an adjacent delimiter pair into a blank indented line.
- Python `else`, `elif`, `except` and `finally` alignment, including nesting,
  stale earlier branches and colons inside expressions.
- YAML mapping headers, sequence mappings, block/folded scalars, explicit scalar
  indentation and literal scalar bodies.
- Finished/unfinished literals, comments, raw strings, regex and interpolation.
- Selection replacement, reverse selections, Tab/Shift-Tab, empty lines, tabs,
  Unicode and CRLF. End-of-selection at the next line's start excludes that line.

Six Rust tests, 113 Dart tests, strict Dart analysis and Rust Clippy pass.
The same 45 highlighting analyses plus 84 authoring cases produce identical
results on native VM, native AOT, dart2js/Node and dart2wasm/Node. See the current
[verification receipt](receipts/verification.json) for source hashes and tool
versions. The ABI is now version 2 and rejects stale version-1 assets.

Release tooling on this machine emitted an optional debug-info stripping warning
because rust-objcopy could not find libLLVM. Builds and executable checks completed
successfully; this is not a cross-platform release-packaging qualification.

## Findings that changed the implementation

The original range-preservation tests could pass while a standalone Dart `for`
loop was classified as a malformed declaration. We now check actual keyword
highlighting and compare the normal parse against a temporary function-body
context when actual error nodes warrant it. Missing punctuation alone does not
trigger that second parse. Wrapper text and synthetic tokens are excluded from
all published ranges and edits.

Unfinished strings/comments do not always appear as literal nodes. Explicit
error-recovery captures conservatively preserve their whitespace. Editing context
comes from syntax queries rather than inferring it from painted highlight scopes.

The first indentation implementation repeatedly searched literal regions while
matching delimiters. A bounded literal-position mask removed those repeated
searches. Ordinary insertion skips parsing unless the language's configured
character can trigger outdent. Query files are tracked as build dependencies,
and unsupported capture names and custom predicates/properties fail initialization.

This is a small Flark query vocabulary and shared interpreter. It does not claim
compatibility with arbitrary Helix/Zed indentation files. The
[query contract](native/queries/CONTRACT.md) records provenance and exclusions.

## Performance gate remains open

On the recorded Apple M1 Pro, near 8K UTF-16 units:

| Operation | Native AOT warm median | dart2js + Rust Wasm warm median |
| --- | --- | --- |
| Ordinary insertion proposal | 0.22–0.28 ms | Below or around 1 ms clock resolution |
| Enter proposal | 2.1–4.7 ms | 3–7 ms |
| Complete highlighting | 6.8–15.4 ms | 12–27 ms |

These repeated-corpus measurements include the bridge and result validation but
exclude Flark transactions, Flutter layout and paint. Cold initialization costs
more. They are not worst-case bounds. Highlighting a large Dart fence on every
keystroke cannot yet be assumed to fit the combined frame budget.

The [stage diagnostic](receipts/stages.jsonl) separates parsing, editing-query
execution, highlighter events and JSON encoding. Highlighter events include their
own parse, so the stage numbers must not simply be summed. Parsing and highlighting
remain the main work; the extra layer alone does not explain their cost. Our
Dart fragment-context check also currently adds a parse before highlighting.

See [benchmark context](receipts/benchmark-context.json),
[native edit timings](receipts/edit-bench-native.json),
[Wasm edit timings](receipts/edit-bench-js.json),
[native highlight timings](receipts/bench-native.json) and
[Wasm highlight timings](receipts/bench-js.json).

## Next focus

Close the combined typing-cost decision before host adoption or adding languages:
evaluate supported parser/highlighter reuse and where decoration work belongs
in the host's frame budget. The current upstream highlighter API does not accept
an already parsed tree; sharing the context-check parse is not a one-line option.
Do not fork private upstream internals or lower the snippet limit silently.

After that gate, integrate Flark's exact code-body/container mapping and test
actual undo, composition, paste, Flutter input/paint and browser/native dogfooding.
Package proposals and source snapshots cannot prove those host behaviors.
