# V5 local merge review — 2026-09-07

This review covers the accumulated V5 kernel, Flutter host/workbench, and
single-engine code-region implementation on PR #41. The owner authorized review,
local testing, and merge, explicitly requesting that CI be skipped. New commits
and the merge subject carry `[skip ci]`; repository branch rules do not require
a status check. No workflow configuration is changed.

The implementation is split into language-service, parser/kernel, Flutter-host,
and documentation/evidence commits. The final local receipt is
[`receipts/merge-2026-09-07/receipt.json`](receipts/merge-2026-09-07/receipt.json).
It names the committed implementation and hashes the tested source and assets.
Earlier dated receipts retain their original scope and source hashes.

## Independent review and corrections

Two independent reviewers examined the current implementation, with separate
parser/kernel and Tree-sitter/Flutter scopes. The root agent reviewed their
changes and ran the combined qualification. Four concrete findings were fixed:

| Finding | Correction and regression boundary |
| --- | --- |
| Return after a tab-padded list marker duplicated content or corrupted numbering. | Schema V3 publishes exact marker endpoints from the parser's column-aware container cursor. Dart consumes source ranges; tabs, ordered items, tasks, nesting, CRLF, next input, and undo/redo are covered. |
| A combining character across hidden formatting admitted an interior caret; deletion could leave a stray list marker. | Legal source ranges now account for graphemes in projected text across segment boundaries. Deletion includes fully consumed formatting owners. Pointer placement, movement, entities, next input, and history are covered. |
| Pasted fence delimiters could close the code block and pull following content into a new fence. | Literal code edits grow the original delimiters when required and preserve parser-authenticated container prefixes. Cases include partial tabs, CRLF, reversed selection, source-mode admission, first paint, next input, and history. |
| The new literal-paste route crashed on a source-authored fence with no body line; such rows also lacked a caret anchor when other content followed. | Bodyless fences receive one source-end caret anchor and create their body on first insertion. Dedicated pointer/typing and populated/bodyless browser clipboard cases verify following content, history, and first paint. |

The tests assert the intended source, caret, style/container membership,
rejection where appropriate, and the following typed character. The generated
matrix supplements these direct cases. Parser conformance alone could not
detect these editing-intent failures; passing final syntax colors could not
detect clipboard ownership or first-frame failures.

One narrow unsupported edit is explicit: an imported bodyless fence with a
partially consumed prefix tab requires its unpublished blank-body scaffold to
fit live admission. Otherwise its first insertion rejects atomically with
`unsupportedEdit`; source mode remains available. Two boundary tests cover this
without bypassing configured parsing limits.

## Qualification scope

Implementation commit `603433c44634157cdb15b056343bbc9e9b269b27` passes 494 core,
796 Flutter host, 25 workbench, 25 real-Chrome, and 307 Tree-sitter Dart tests.
The native suites pass 44 Markdown and 9 Tree-sitter tests. All active package
analyses are clean. The documentation commit adds evidence without changing
the qualified implementation; the receipt verifies every source hash.

The receipt records Rust conformance/fuzz and extraction checks, current schema
outputs, all 1,322 Markdown transport cases, a Rust-free consumer, Dart analysis
and tests, 1,000 generated command sequences, Flutter host/workbench checks,
real Chrome input/paint cases, and the release Flutter web/WASM build. The
Tree-sitter verifier covers all 390 transport cases and the native coloring
worker. Its unchanged language sources retain the earlier browser-worker
qualification in the single-engine receipt; browser-worker timings are not
reclaimed from this merge run.

The keystroke diagnostic uses the pinned 16,694-byte dense structural fixture,
with explicit admission above the product's 16 KiB fallback. Its receipt names
the source commit, machine, schema, fixture hash, and insert/Backspace p99. It
measures the kernel facade; it does not measure an entire Flutter frame.
On the Apple M1 Pro with Dart 3.12.2, the final committed run reports insert
p99 2.448 ms and Backspace p99 2.498 ms, both below the 4 ms local gate.

CI was deliberately not run. Physical-device/IME, sustained color transitions,
whole-editor frame budgets, Fleury adoption, and release qualification remain
open. The fourteen-language catalog and its explicit limitations remain in
[`SINGLE_ENGINE_REVIEW.md`](../../../packages/flark_tree_sitter/SINGLE_ENGINE_REVIEW.md).
This merge does not declare M2–M6 or the twenty-language expansion complete.
