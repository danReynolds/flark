# Language infrastructure for Flark code regions

Research date: 2026-09-06. This is a recommendation and reproducible research
record, not an implementation or a new dogfood qualification.

Follow-up: the [small integration proof](code_region_spike_2026_09_06.md) confirms
that basic data-driven indentation is feasible, but rejects the tested Shiki
integration on dependency/API boundaries and typing cost. The user's clarified
snippet scope does not call for escalating to a larger parser/runtime project.

## Recommendation

Reuse language grammars and editing configurations. Stop expanding the current
collection of syntax-specific branches in Flark's code commands.

The leading approach is **TextMate tokenization plus VS Code-style language
configuration**, exposed through a small, Flutter-free Dart service. Flark keeps
the document, selection, input intent and undo transaction. A language service
supplies lexical ranges and proposes indentation within the code body.

This chooses an architecture, not a production dependency yet. The Dart port
behind `shiki_flutter` is worth qualifying first: its engine ran on the Dart VM
and as compiled JavaScript in our probes. Its current package boundary and
public token API prevent a clean direct adoption. Prefer an upstream pure Dart
engine package with raw lexical tokens over maintaining another tokenizer port.

**Tree-sitter plus an established indentation-query interpretation is the
alternative** if the lighter approach needs recurring language-specific code,
cannot handle embedded syntax accurately, or lacks a maintainable Dart runtime.
Neither candidate is a zero-integration solution. A full CodeMirror widget
becomes the lead if we deliberately choose a browser-only editor; that would
change the current shared Flutter/Fleury product contract.

## What established editors actually reuse

| Option | What it supplies | Fit for Flark |
| --- | --- | --- |
| TextMate + VS Code language configurations | Lexical grammar, brackets, Enter and indentation rules; the editor interprets those rules | Leading fit for shared Dart code regions, subject to runtime qualification |
| CodeMirror 6 | Editing commands, language packages, parser-aware indentation and highlighting | Strong behavioral reference; embedding its DOM editor creates a second host and editor integration |
| Tree-sitter + editor queries | Incremental syntax trees, highlighting queries and reusable indentation descriptions | Strong structural alternative; still requires a compatible query interpreter and native/web integration |
| `re_editor`, `flutter_code_editor` | Flutter code editor widgets and useful editing conveniences | Their inspected indentation implementations do not remove our language-engine problem |
| Highlight.js family, lowlight, Shiki, syntect | Syntax tokenization/highlighting | Useful components; none by itself is a general indent/outdent engine |
| Language server / formatter | Deeper language services and formatting | Separate future feature, unnecessary for the reported brace cycle |

VS Code's [language configuration guide](https://code.visualstudio.com/api/language-extensions/language-configuration-guide)
describes bracket pairs, context-sensitive autoclosing, indentation patterns and
ordered Enter rules. With no indentation rules, it falls back to opening/closing
brackets. [TextMate tokenization](https://github.com/microsoft/vscode-textmate)
provides scope ranges and state across lines; it does not implement indentation.
[On-type formatting](https://code.visualstudio.com/api/language-extensions/programmatic-language-features#incrementally-format-code-as-the-user-types)
is another provider layer. The inspected
[Dart-Code configuration](https://github.com/Dart-Code/Dart-Code/blob/4a166f7abbc077ac4d2706224a61ebceb4bf0a92/syntaxes/dart-language-configuration.json)
contains bracket/quote configuration but no custom indentation or Enter rules.
The ordinary Dart brace case does not inherently require a full language server.

Reusing JSON configuration still requires correct execution semantics. Grammar
regexes use Oniguruma; language configuration regexes follow the editor's
JavaScript conventions. We should not assume both are interchangeable with
Dart `RegExp`, or claim all VS Code extensions become supported.

## What Markdown and Flutter libraries do

[Tiptap CodeBlockLowlight](https://tiptap.dev/docs/editor/extensions/nodes/code-block-lowlight)
combines highlighting with optional Tab indentation. It does not establish
general language-aware indentation.
[Milkdown Crepe](https://milkdown.dev/docs/api/crepe) instead embeds CodeMirror.
Its inspected
[node view](https://github.com/milkdown/milkdown/blob/920cded3fc9fe956de87339e30507650a916cf56/packages/components/src/code-block/view/node-view.ts)
maps code edits and selections into ProseMirror, forwards undo, handles focus
and arrow exits, guards against update loops, and manages editor lifetime.
This is a credible web approach. It also demonstrates the integration work
hidden by the phrase "embed a code editor." Flutter and Fleury would need
equivalent behavior around a different rendering/input system.

The published, checksum-verified Flutter sources were less compelling as a
language backend:

- [`flutter_code_editor` 0.3.5](https://pub.dev/packages/flutter_code_editor/versions/0.3.5)
  uses the same `highlight` 0.7.0 dependency as Flark. Its Enter modifier increases
  indentation after `:` or `{`; its closing-block modifier removes one configured
  step before `}`. Those are local heuristics, not a general parser-backed service.
- [`re_editor` 0.10.0](https://pub.dev/packages/re_editor/versions/0.10.0) uses
  `re_highlight`. Its inspected newline path copies indentation and handles
  paired delimiters; its input layer has static delimiter and quote handling.
  It is useful as an editor reference, but adopting its widget would not make
  arbitrary-language indentation solved.
- [`syntect`](https://github.com/trishume/syntect) offers Rust highlighting from
  Sublime Text syntax definitions. It adds no indentation engine; its suitability
  for our Wasm build was not tested.

## Executed reference probes

The [saved probes and results](research/code_languages_2026_09_06/README.md)
contain exact versions, source hashes, source/caret traces and reproduction
instructions. These are small mechanism probes, not browser or product tests.

### CodeMirror

We executed 16 headless `EditorState` traces using the real
`insertNewlineAndIndent`, `indentOnInput`, `indentMore` and `indentLess` behavior.
Typed text was delivered character by character; paste was a separate transaction
intent. JavaScript, TypeScript, Python, YAML, HTML and JSON used language packages;
Dart, Ruby and shell used legacy stream modes.

| Trace | Observed behavior |
| --- | --- |
| Owner's Dart `for` loop: Enter, then `}` | Indents the blank line, then aligns the closer with the opener |
| Nested JavaScript and TypeScript braces | Aligns inner and outer closers |
| JavaScript regex, open comment, template interpolation | Handles the sampled contexts; the literal comment brace keeps its indentation |
| Pasted JavaScript `}` | Preserves the existing indentation |
| Python `if`, `pass`, `else:`, Enter | Aligns `else:` and indents the following body |
| Python list, YAML flow object, JSON object | Indents then aligns the closing delimiter |
| HTML closing tag and Ruby `end` | Aligns the closing construct |
| Dart Tab then Shift-Tab | Restores the initial indentation |
| YAML `settings:` then Enter | **Does not add indentation** in the tested configuration |
| Shell `if true; then` then Enter | **Does not add indentation** in the tested legacy mode |

These observations are not a universal pass/fail verdict on CodeMirror. They
show that a mature engine can cover much more than our current matcher while
still needing an explicit product acceptance suite. CodeMirror's
[indentation runtime](https://github.com/codemirror/language/blob/8e9700018446d46f23267f6e31da56628d5117c0/src/indent.ts)
and [JavaScript language support](https://github.com/codemirror/lang-javascript/blob/aac430a8d38669711cc2a4727fd6418b9634807d/src/javascript.ts)
also contain executable language behavior, not just grammar files that can be
imported unchanged into Dart.

### Dart TextMate feasibility

[`shiki_flutter` 1.1.0](https://pub.dev/packages/shiki_flutter/versions/1.1.0)
has a Flutter-free `engine.dart` entrypoint backed by a Dart TextMate runtime
and pluggable regex engine. Its published dependency graph nevertheless requires
the Flutter SDK. That is not a drop-in dependency for pure Dart `flark`.

Using its public engine API, six snippets covering Dart, YAML, JavaScript
interpolation/regex, a Python multiline string and HTML with JavaScript preserved
the exact input text. The Dart VM and Dart-to-JavaScript execution under Node
produced identical JSON token output. This establishes limited execution parity;
it does not establish grammar correctness, Flutter web input behavior, Dart Wasm
compatibility, incremental correctness, or acceptable typing latency.

A deliberate minimal theme exposed an important API distinction: adjacent lexical
tokens with the same style are returned as one themed token. Its explanatory
`scopes` concatenate the underlying scopes. For example, the entire Dart `for`
line became one themed token with several different lexical classifications.
Source inspection of `_tokenizeOneLine` confirms this behavior.

That is suitable for painting, but those ranges cannot be used as precise
editing authority. The runtime internally has `Grammar.tokenizeLine` with raw
ranges, which the public engine entrypoint does not expose. A supported raw-token
API and a pure Dart package boundary are the first adoption gates. Even raw
scopes need careful interpretation: code embedded in string interpolation must
not remain classified as literal merely because an ancestor scope is a string.

## Tree-sitter assessment

[Tree-sitter](https://tree-sitter.github.io/tree-sitter/) is an incremental,
error-tolerant parsing system. Its official native and
[web bindings](https://github.com/tree-sitter/tree-sitter/blob/master/lib/binding_web/README.md)
make it credible for our target platforms. They do not prove that adding it to
our current Comrak Wasm artifact will be trivial.

[Helix indentation](https://docs.helix-editor.com/master/guides/indent.html)
uses a grammar, per-language `indents.scm` queries, and editor logic interpreting
captures, alignment, literal regions and extension behavior. Its default hybrid
heuristic relates new indentation to existing text. These query conventions are
not a built-in Tree-sitter indentation standard. The inspected
[Helix runtime](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/helix-core/src/indent.rs)
is substantial and tied to its tree/query/rope abstractions. Reusing its queries
requires matching those semantics. Its repository is MPL-2.0; any reused source
must retain the applicable notices and terms.

The Dart package landscape also needs scrutiny:

- [`tree_sitter` 0.2.1](https://pub.dev/packages/tree_sitter/versions/0.2.1)
  is an FFI route requiring native libraries, not a direct web solution.
- [`tree_sitter_dart` 0.1.1](https://pub.dev/packages/tree_sitter_dart/versions/0.1.1)
  is a recent Dart implementation. Although described as pure Dart, the inspected
  published pubspec requires Flutter. Its parser/scanner port and parity need
  independent qualification; it is not simply the official C runtime in Dart.
- [`tree_sitter_language_pack` 1.16.2](https://pub.dev/packages/tree_sitter_language_pack/versions/1.16.2)
  supplies a broad grammar catalog and asynchronous Rust bridge APIs. It does
  not supply our synchronous editing transaction or an indentation engine.

No Tree-sitter runtime or indentation interpreter was executed in this research.
The comparison therefore supports a shortlist, not a measured claim that the
TextMate route is faster or more correct.

## Proposed implementation milestones

### 1. Qualify the language service boundary

Start with an isolated TextMate/configuration prototype, without changing the
current dogfood candidate. It must expose exact lexical ranges independent of
theme, preserve state across incomplete lines, and propose an indentation edit
for an identified code region. Language loading can happen before input; an
accepted edit must remain synchronous. Unknown, unloaded or budget-exhausted
languages keep literal input and explicit Tab/Shift-Tab behavior.

Qualify Dart, JavaScript/TypeScript, Python and YAML first. These cover the owner's
case, embedded syntax, keyword indentation and the current highlighter's known
YAML limitation. Require a maintainable pure Dart dependency path, correct raw
token ranges, equivalent native/web results and bounded work at the existing
code-region limits. Measure warm edits, initialization, malformed/long lines and
incremental invalidation separately. Do not raise the current 8,192-unit
highlighting limit on the strength of the six snippets.

**Switch criterion:** if integration requires owning a tokenizer port or recurring
language-specific branches to meet those cases, prototype the official
Tree-sitter runtime plus one compatible query interpretation instead. Do not
build and maintain both backends speculatively.

### 2. Integrate transactional editing

Replace the current highlight-token-driven closer decision and explicit syntax
branches with the qualified service. Keep Markdown recognition in Comrak.
Convert code-local proposals through existing source mappings and commit them
with the typed character in one Flark history transaction. The language service
does not own selection, clipboard, composition, focus or host widgets.

Automatic language detection remains a separate heuristic with a manual override.
Neither a grammar catalog nor a parser makes arbitrary language detection
reliable. Define when an inferred language may guide indentation, and ensure a
changing guess cannot reformat an existing region.

### 3. Qualify complete authoring journeys

Turn the reference traces into Flark expectations with exact source and caret
assertions after each action. Add next-character insertion, selection and
multiline indentation, Undo/Redo, literal paste, composition, tabs, mixed
whitespace, CRLF, Unicode before delimiters, malformed syntax, theme/language
changes, nested Markdown containers and crossing code/prose boundaries.

Replay those journeys in the real Flutter web host and native input path before
reopening the broader authoring contract. Expand the supported editing language
set only with declared behavior and those receipts. Broad highlighting coverage
can grow independently; it must not imply equally strong editing support.

The testing adjustment is as important as the dependency choice: borrow mature
language knowledge, but keep responsibility for the complete user interaction.
