# Why a Dart API with a Rust adapter

Keep a small Rust adapter around the upstream Tree-sitter runtime and highlighter.
Expose snippet results through a pure Dart API. This choice buys direct reuse of
upstream parser, grammar and highlighting behavior while keeping editor state in
Dart. It is not a measured claim that Rust is intrinsically faster than Dart.

## What runs where

[Tree-sitter's parsing runtime](https://tree-sitter.github.io/tree-sitter/)
is C11. The Rust crate uses its official bindings, the upstream
`tree-sitter-highlight` crate and unmodified grammar crates. Our adapter owns
range conversion, result encoding and the narrow ABI. The same implementation
is compiled into a native library and a browser Wasm module.

Tree-dependent indentation analysis runs beside the parser and query engine:
keep syntax trees local and return a proposed code-body edit instead of exporting
a general tree API across the bridge. The fourteen-language authoring corpus
exercises that layer through the Flutter host and native/Wasm transports.
Tree-sitter does not include a universal indenter.

The [performance review](PERFORMANCE_REVIEW.md) records the decision to keep editing
synchronous and use a native-isolate/browser-worker lane for code colors. RFC 031 specifies its source/language checks and visual acceptance
gates. The Flutter workbench now uses it. Full-frame and physical-device qualification
remain separate gates; see [SINGLE_ENGINE_REVIEW.md](SINGLE_ENGINE_REVIEW.md).

Dart owns the public types, lifecycle and transport. Flark supplies the editing
intent and indentation preferences, and owns source, Markdown containers,
selection and undo. The Flutter and Fleury hosts consume these results without
acquiring separate language rules. A pure Dart API does not mean a Dart-only
runtime: native libraries and a Wasm asset remain package requirements.

## Alternatives considered

**Dart bindings directly to C** can eliminate our Rust adapter, but retain a
native dependency and the web bridge. The inspected
[`tree_sitter` 0.2.1](https://pub.dev/packages/tree_sitter/versions/0.2.1)
expects separately built dynamic libraries. The upstream highlighter's C API
in our pinned version produces HTML; the Rust API provides the source-range
events that Flutter and Fleury need. Direct C integration is possible, but
does not yet show a smaller overall ownership burden for native plus web.

**An all-Dart Tree-sitter implementation** can remove the native/Wasm engine
dependency. [`tree_sitter_dart` 0.1.1](https://pub.dev/packages/tree_sitter_dart/versions/0.1.1)
implements a compatible parser, query runtime and external scanner ports in Dart.
Its README describes a standalone runtime, while its published package declares
Flutter and web dependencies. Rechecked on 2026-09-06. It needs independent parser,
query, incomplete-input and scanner parity measurements before substitution; its
grammar count alone does not establish those properties. No comparative runtime
benchmark against it has been run here.

**Dart highlighting plus lexical indentation rules** was considered for a
reduced syntax contract. That integration has now been removed; no parallel
implementation is maintained. The previous local TextMate prototype exposed
dependency/API constraints and Python branch/YAML scalar failures. Those results
reject that tested integration, not every possible Dart implementation. Writing
our own cross-language parser would add substantially more maintenance than this
adapter and is outside the snippet package's purpose.

## Decision and limits

Retain Rust for upstream reuse and local syntax/query work, with Dart as the
consumer API and editor integration language. Native asset distribution, web
loading, UTF-8/UTF-16 conversion, module size and typing cost are real costs of
this choice. The current four-runtime comparison verifies transport identity;
it does not compare Rust against Dart implementations or qualify editor latency.

Revisit this decision if a Flutter-free Dart runtime passes the same parser,
query and authoring corpus with acceptable native/browser performance and a
smaller maintenance burden, or if the adapter cannot meet the package's typing
and distribution budgets. Do not expand a general editor framework inside Rust.
