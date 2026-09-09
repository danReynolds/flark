# flark_tree_sitter

Shared code snippet language services with a pure Dart API and a Rust engine.
The Rust crate is inside this package at `native/`. It uses the official
Tree-sitter runtime/highlighter and unmodified, pinned language grammar crates.
Flutter and Fleury can consume the same source ranges and edit proposals.
The package does not own Markdown fences or editor state.
`package:flark_tree_sitter/flark.dart` provides the shared `FlarkTreeSitter`
adapter to Flark's command contract. Flutter adds its asset-loading convenience
in `flark_flutter/code.dart`; Fleury uses the same adapter directly. Its edit,
closer and highlighting scenarios now run here without Flutter bindings.
The [architecture decision](ARCHITECTURE.md) explains the Rust/Dart boundary and
the alternatives considered.

**Current checkpoint: one engine for all 14 selectable code languages.**
Dart, JavaScript, TypeScript, Python, Ruby, Rust, Go, JSON, YAML, CSS, Bash,
HTML, XML and SQL use the same package for highlighting and snippet edits.
Automatic uses parser/query evidence from the first 128 UTF-16 units; manual
selection is authoritative. Plain text is the behavior for unsupported,
unrecognized or oversized snippets. There is no parallel Dart highlighter or
lexical language indenter. See [SINGLE_ENGINE_REVIEW.md](SINGLE_ENGINE_REVIEW.md)
for capabilities, verification and remaining limits. Earlier reviews describe
historical checkpoints.

```dart
import 'package:flark_tree_sitter/flark_tree_sitter.dart';

final code = CodeAnalyzer();
final result = code.analyze(
  'void main() { print("hello"); }',
  language: CodeLanguage.dart,
);
for (final span in result.spans) {
  final text = result.source.substring(span.start, span.end);
  // Render text using span.scopes and the host's theme.
}
```

Propose one replacement and a resulting selection for the host to apply:

```dart
const source = 'for (final x in items) {\n  ';
final edit = code.proposeEdit(
  source,
  language: CodeLanguage.dart,
  base: source.length,
  extent: source.length,
  action: CodeEditAction.insert,
  text: '}',
);
if (edit != null) {
  final nextSource = edit.applyTo(source);
  // The host applies nextSource and edit.base/edit.extent in one transaction.
}
```

Dispose the analyzer when its host closes. `applyTo` rejects a different source
revision. `proposeEdit` returns null outside its input/candidate size budget;
the host then performs its ordinary edit. `indentUnit` and `newline` are explicit
options. Paste and IME composition use the host's literal edit path.

For web, load once before editing and pass the backend to `CodeAnalyzer`:

```dart
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/wasm.dart';

final backend = await WasmCodeBackend.load();
final code = CodeAnalyzer(backend: backend);
```

Flutter bundles the package-declared Wasm asset. A plain Dart web application
serves `flark_tree_sitter.wasm` itself and passes its URL to `load(uri: ...)`, or calls
`fromBytes`. Native builds use the code-asset hook. It checks bundled
`prebuilt/<triple>/`, then `hooks.user_defines.flark_tree_sitter.prebuilt_dir`, then
builds the co-located Rust crate. Rust-free release packaging is a later gate.

## Contract

- No Flutter dependency. Both dart2js and dart2wasm use `dart:js_interop` to call
  the same Rust Wasm artifact that supplies native behavior through FFI.
- Input is exact valid Unicode. Lone UTF-16 surrogates are rejected before
  conversion. CRLF is not normalized. Returned spans cover the entire input
  contiguously in both UTF-8 bytes and UTF-16 code units, and are immutable.
- Scope names come from upstream highlight queries and are theme-independent.
  Editing uses separate syntax queries for literal/interpolation context.
- `detect(source)` returns a catalog language or plain. Its exact-prefix cache
  holds 32 samples. Inference is deliberately conservative and can be ambiguous;
  long comment headers may require manual selection. Embedded-language
  injections are outside the current contract.
- Over 8,192 UTF-16 units, analysis returns plain spans with `status == limit`.
  Source-size admission is not a latency or parser-work bound. Full-snippet
  highlighting still reparses the complete snippet. Editing/context detection
  reuse Tree-sitter's supported incremental trees. Production frame budgets
  remain open; use the experimental worker to evaluate deferred code colors.
- Each analyzer owns and disposes its backend. Native result buffers are freed
  after copying. Browser memory is re-read after calls that may grow it. A Wasm
  trap invalidates that backend; create a fresh one rather than reuse its state.
- Flark will apply suggestions as one source/selection/history transaction.
  Markdown container prefixes and fences never enter the language engine.
- Native executing threads retain grammar/highlighter scratch and at most the
  last admitted snippet's two context trees. Complete source/language inputs
  determine every result; changing language invalidates those tree caches.
  This bounded scratch may outlive an individual native analyzer. A disposed
  browser backend releases its instance, including its scratch.
- The [ABI is version 4](ABI.md). Older native/Wasm assets fail initialization.
  See the [editing query contract](native/queries/CONTRACT.md) for supported
  rules, provenance and limitations.

## Verification

From this directory, with Dart, Node, Python, Rust/rustup and an LLVM Clang with
the wasm32 target installed:

```sh
dart pub get
python3 tool/verify.py
```

The verifier runs Rust and Dart tests, builds the Wasm asset, and compares the
same catalog detection, Unicode/range analyses and 206 authoring cases on the Dart VM, native AOT,
dart2js/Node and dart2wasm/Node. Each authoring case checks the next character too.
These are component receipts, not Flutter input/paint or physical-device proof.
Build products and local receipts go to `build/`.

The verifier also runs the native AOT worker probe and compiles both browser
probes. Run `python3 tool/serve_probe.py`, then open
`http://127.0.0.1:8814/tool/browser_probe.html` and the same URL with
`?runtime=wasm`. Each page saves its synthetic receipt in `build/` after passing.
These probe pages exercise actual Web Workers, with no Flutter rendering.

The optional API lives in `package:flark_tree_sitter/highlight_worker.dart`:
start a `CodeHighlightWorker`, call `analyze`, and dispose it with its host.
Superseded requests return null. A returned result can only decorate its exact
source and language. The Flutter workbench uses this worker for colors and the synchronous analyzer
for edits; see RFC 031 for its publication contract.

`tool/build_wasm.py` locates C headers in the pinned `tree-sitter-language`
dependency and passes them to upstream grammar build scripts. On macOS it finds
Homebrew LLVM if Apple's Clang lacks wasm32. Set `CC_wasm32_unknown_unknown` and
`AR_wasm32_unknown_unknown` explicitly on other installations when needed.

## Dependencies

Runtime/highlighter: Tree-sitter 0.27.0 (MIT). The complete pinned grammar table
is in [SINGLE_ENGINE_REVIEW.md](SINGLE_ENGINE_REVIEW.md); `native/Cargo.lock` pins
the dependency graph. Highlight and locals queries come from upstream grammar
crates (TypeScript combines its query with JavaScript's). Flark owns its small
edit and detection queries and common indenter, not a language parser.

CSS stays on 0.23.2 because 0.25.0's build script expects the removed 0.26 Wasm
libc. `native/wasm_compat.h` supplies two missing C-header definitions through
Clang and Tree-sitter's own libc. Parser/scanner sources remain unmodified.
The [query contract](native/queries/CONTRACT.md) records semantics and limits.
Dependency license texts are retained in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
