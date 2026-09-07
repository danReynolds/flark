# Code-region integration proof

**Decision: keep the small, data-driven direction, but do not adopt this
TextMate backend.** The user's simplicity constraint is useful: the proof exposed
costs before they became production dependencies or another round of patches.

The [runnable spike](../../../spikes/v5/code_regions/README.md) implements a
TextMate lexical adapter and one generic indentation adapter. Existing grammars
supply lexical context; pinned upstream configuration supplies bracket pairs,
basic Enter rules and increasing-indentation patterns. Flark production packages
and the currently served dogfood candidate are unchanged.

## What the proof establishes

The final **69 checks produce identical results on the Dart VM, native AOT and
compiled JavaScript under Node**. There are **67 passing checks and two visible
misses**, with no skips. Analysis passes. These are isolated snippet-local edit
and lexical checks, not Flutter input, paint, selection/history or browser proof.

Passing cases include the owner's Dart loop through Enter, typed `}` and the next
character; nested brackets; splitting an existing pair over three lines; Python
colon/bracket indentation; YAML mapping/flow indentation; tabs; CRLF and Unicode;
literal strings, comments and regexes; blank lines in multiline strings; Dart
and JavaScript interpolation; trailing comments; and conservative unknown-language
and mismatched-bracket behavior. Raw token coordinates preserve source exactly
and remain identical after theme changes and re-editing earlier context.

The expanded cases caught a generic distinction between a trailing line comment
and an unterminated block comment. The adapter now uses the grammar's end-of-line
state to make that distinction. It needed no language-specific branch.

Two ordinary editing expectations remain unmet:

- Typing Python `else:` after an indented body does not align it with `if`.
- Enter after YAML `settings: |` does not indent the block scalar body.

Those are gaps in this selected configuration subset and interpreter, not a
claim that VS Code itself lacks those behaviors. The proof deliberately does not
claim full VS Code configuration/provider compatibility or patch individual
languages to turn these receipts green.

## Why this backend does not pass the simplicity gate

1. **Dependency boundary:** `shiki_flutter` 1.1.0 requires Flutter and Dart 3.12.
   Flark's core is a pure Dart package with a 3.10.4 SDK floor. Executing a
   Flutter-free entrypoint does not remove that package dependency.
2. **Supported API:** precise tokens and grammar state require six imports from
   the package's internal TextMate implementation. Public themed tokens merge
   ranges by style and are unsuitable for these editing decisions. Shipping the
   proof as-is would depend on unsupported internals.
3. **Typing cost:** the deliberately simple complete-snippet tokenization is too
   expensive at the existing code-region limit. Making it production-appropriate
   would require a different runtime or qualified incremental state/caching,
   alongside resolving the first two boundaries.

The last point is measured rather than inferred from package size. On **Apple
M1 Pro, macOS 26.2, Dart 3.12.2 / Node 22.23.0**, the final sequential diagnostic
measured these median complete-snippet tokenization times:

| Grammar | Native AOT, 512 units | Native AOT, 8,192 units | Compiled JS/Node, 8,192 units |
| --- | ---: | ---: | ---: |
| Dart | 1.07 ms | 17.70 ms | 7 ms |
| JavaScript | 4.20 ms | 68.55 ms | 27 ms |
| TypeScript | 3.76 ms | 61.81 ms | 27 ms |
| Python | 2.19 ms | 35.79 ms | 14 ms |
| YAML | 1.71 ms | 28.24 ms | 14 ms |

Each cell is the median of 12 warm runs over repetitive ASCII fixtures; for these
fixtures UTF-16 units equal bytes. These costs exclude Markdown parsing, host
input and painting. The Node clock is millisecond-granularity. Cold construction
and each grammar's first tokenization are recorded separately in the receipts.
This is not a benchmark of an optimized Shiki integration or every TextMate
runtime, and does not establish device or Flutter web performance.

The checkout is dirty at base commit
`9fcb092f34a1ecd3a2b97d67b9bd3456e04ab410`; the
[verification receipt](../../../spikes/v5/code_regions/receipts/verification.json)
hashes the actual spike inputs and results. No performance claim attaches to
that base commit alone.

## Direction after the proof

The reusable part of this work is the behavior contract and the generic editing
mechanism. Standard snippet indentation does not inherently need a syntax tree
or a language server. Most of the sampled behavior came from small upstream
definitions interpreted once in shared Dart.

The earlier research placed TextMate first for dependency qualification. This
proof does **not** justify making TextMate, Tree-sitter or a new incremental
runtime a prerequisite for Markdown code snippets. Under the clarified scope,
retain the current lightweight production implementation while treating its
declared language limits honestly. Use these acceptance cases for subsequent
small improvements; do not import the spike's unsupported API accesses.

A replacement highlighter should be adopted when it offers a supported Dart
boundary and meets the code-region budget with modest integration. The current
evidence does not qualify one. A future public raw-token API/pure Dart package
or another compatible runtime can reopen that choice, using the same tests.
This closes the agreed integration proof with a negative adoption decision; it
does not claim that the broader code-region feature or its remaining limitations
have been fixed by research.
