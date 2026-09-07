# Foundation checkpoint — 2026-09-06

Historical foundation checkpoint. The current code and receipts also include
the subsequent [authoring checkpoint](AUTHORING_REVIEW.md).

The package boundary and shared Rust engine are viable. `flark_tree_sitter` has a pure
Dart API, a co-located Rust crate, native code assets and a browser Wasm backend.
Four released grammar crates supply highlighting without forks: Dart,
JavaScript, Python and YAML. Markdown ownership stays in Flark.

## Evidence

- Five Rust tests and eighteen Dart tests pass; Dart analysis and Rust Clippy
  pass without warnings.
- Forty-five shared analyses produce identical results on the Dart VM, bundled
  native AOT, dart2js/Node and dart2wasm/Node. Cases cover all four languages plus
  plain text, Unicode, CRLF, incomplete syntax and the source-size boundary.
- Tests check exact source coverage, UTF-8/UTF-16 correspondence, invalid input
  rejection, disposal and independence from previously analyzed snippets.
- Native AOT uses `dart build cli` so the Rust code asset accompanies the
  executable. `dart compile exe` alone is insufficient for that packaging test.
- The Rust Wasm module requires no external imports. It is approximately
  4.0 MB uncompressed and 0.76 MB gzip for the current four-language build.

The [verification receipt](receipts/verification.json) records tool versions,
source hashes and the Wasm artifact hash. These are component checks on macOS
ARM64 and Node, not Flutter browser input/paint, mobile, or Fleury qualification.

## What the baseline changed in the plan

The implementation currently reparses each complete snippet and returns JSON.
On an Apple M1 Pro, warm analyses near the 8K UTF-16 limit cost approximately
6–12 ms through native AOT and 8–16 ms through dart2js/Node. Approximately
500-unit snippets cost 0.3–1.0 ms native and 1–2 ms through dart2js/Node.
These timings include the bridge and result validation but exclude host editing,
layout and paint. First use also includes grammar/query initialization and costs
more. The small repeated corpus is a baseline, not a worst-case latency bound.

See the [benchmark context](receipts/benchmark-context.json),
[native measurements](receipts/bench-native.json) and
[JavaScript/Wasm measurements](receipts/bench-js.json).

Typing performance is therefore an explicit gate in the next milestone.
Separate parse, query, encoding and transport costs before choosing the smallest
supported reuse strategy. An 8K source limit alone cannot establish responsiveness.

## Next focus

Implement and qualify shared indentation for the four-language corpus before
adding more grammars or adopting this engine in the editor. Tree-sitter provides
syntax structure; it does not supply a universal ready-made indenter. The next
step must choose supported indentation-query semantics and preserve upstream
provenance, with tests for typed closers, Python branch alignment, YAML block
scalars, literals and incomplete snippets.

Highlight scopes are not yet qualified as an editing-context classifier.
Automatic language detection, embedded languages, host transactions, undo and
Flutter input remain open. The existing preview is not using this package yet.
Expansion to twenty languages follows the authoring and host gates in
[PLAN.md](PLAN.md).
