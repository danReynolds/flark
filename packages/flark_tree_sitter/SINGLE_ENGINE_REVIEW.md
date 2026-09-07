# Single-engine code regions — 2026-09-07

The owner asked to finish the migration and stop maintaining two integrations.
All existing choices now use `flark_tree_sitter`: one catalog, aliases,
Automatic detection, upstream highlighting and the common query-based indenter.
`highlight` 0.7.0, its registration/cache/detector and Flark's lexical closer and
newline rules have been removed. The existing authoring scenarios moved to tests
which install the actual Tree-sitter service; language assertions were not
replaced by plain-text expectations.

Flark still owns Markdown, source, caret, history and generic whitespace edits.
The language package owns syntax decisions. Flutter paints the returned scopes
and applies proposed edits. The synchronous API and coloring worker use the same
Rust library and grammar/query data. Missing services, unrecognized languages,
worker failure and size limits produce plain text/ordinary whitespace behavior;
none invoke another parser or highlighter.

## Qualified catalog

Every row has upstream highlighting plus Enter, typed-closer, literal-negative,
selection indent/outdent and following-character cases. All languages preserve
existing indentation; explicit Tab/Shift-Tab respects the selection direction.
This table describes snippet assistance, not a complete source formatter.

| Language | Pinned grammar | Highlighting | Automatic indent/outdent coverage |
| --- | --- | --- | --- |
| Dart | 0.2.0 | Yes | Delimiter bodies/pairs; typed closers; statement fragments and interpolation |
| JavaScript | 0.25.0 | Yes | Delimiter bodies/pairs; typed closers; template interpolation |
| TypeScript | 0.23.2 | Yes, combined with upstream JS query | Delimiter bodies/pairs; typed closers; strings/templates |
| Python | 0.25.0 | Yes | Colon suites, delimiters, compatible branch alignment; string/interpolation context |
| Ruby | 0.23.1 | Yes | Keyword blocks, `end`, branches, delimiters; postfix/endless-method and literal exclusions |
| Rust | 0.24.2 | Yes | Delimiter bodies/pairs; typed closers; quoted/raw literals |
| Go | 0.25.0 | Yes | Delimiter bodies/pairs; typed closers; strings/runes/raw literals |
| JSON | 0.24.8 | Yes | Object/array bodies and pairs; typed closers; quoted literal exclusions |
| YAML | 0.7.2 | Yes | Mapping/sequence context, scalar headers and literal bodies; flow delimiters |
| CSS | 0.23.2 | Yes, with upstream error-recovery limits | Rule/declaration fragments, delimiters, quoted literal exclusions; see limitation below |
| Bash | 0.25.1 | Yes | `then`/`fi`, `do`/`done`, `case`/`esac`, `else`/`elif`, delimiters and command substitution |
| HTML | 0.23.2 | Yes | Paired tags; typed end tags; void/self-closing exclusions |
| XML | 0.7.0 | Yes | Paired tags; typed end tags; self-closing/quoted-attribute/CDATA exclusions |
| SQL (`tree-sitter-sequel`) | 0.3.11 | Yes | Delimiter bodies/pairs and typed closers; literals/comments excluded |
| Plain text | No grammar | Plain | Preserve whitespace and explicit Tab/Shift-Tab |

All versions above are unmodified upstream crates, pinned by Cargo.lock. License
texts and exact source provenance are in THIRD_PARTY_NOTICES.md. TypeScript uses
the TypeScript grammar; JSX/TSX and language injections are not additional
qualified choices. HTML script/style bodies do not acquire another language's
indenter. SQL procedural `BEGIN`/`END`, dialect formatting, hanging argument
alignment, Python dedent after `return` and YAML automatic dash continuation
remain outside this contract.

## Review findings and adjustments

The migration exposed gaps beyond grammar loading:

- Enter immediately before JSON's first string was incorrectly classified as
  being inside that string. Caret-side context now keeps the preceding code
  active; the shared corpus and real browser first-frame test cover it.
- Migrated Dart triple-quote scenarios exposed incomplete literal captures.
  The edit query now marks unfinished triple-quoted error tails as opaque.
- Bash branches needed the existing Ruby block-family behavior generalized.
  This added query data and common family matching, not a Bash-specific host
  indenter. `else`, `elif` and closing `fi` have authoring cases.
- CSS 0.25.0's Wasm build assumes Tree-sitter 0.26's obsolete libc interface.
  Pinning the compatible 0.23.2 release avoids a fork. XML and Bash need two C
  header definitions supplied by `native/wasm_compat.h`, using Clang and the
  upstream Tree-sitter libc. Native/Wasm identity is checked after those changes.
- The expanded module exceeded Chrome's 8 MB synchronous instantiation limit.
  Both compilation and instantiation now use asynchronous WebAssembly APIs.
  Normal editing remains synchronous after initialization.
- Hands-on Automatic Bash typing exposed a changing guess: an unfinished `fi`
  was classified as Ruby. Weak opener and strong shell-keyword query evidence
  now keep unfinished Bash headers in Bash. Component prefix cases, a complete
  host typing journey and a real-browser final-character test cover the failure.
- A 1,024-unit Automatic sample took tens of milliseconds across 14 parsers.
  The admitted sample is now the first 128 UTF-16 units, clipped without splitting
  a surrogate. An exact-prefix LRU cache means typing later in a snippet does not
  repeat detection. The probe measures changing samples, not cache hits.

Automatic requires positive syntax-query evidence and discounts parser errors.
Ambiguous beginnings can choose a neighboring language (`if (...) {` defaults to
JavaScript) or remain plain. Headers beyond 128 units, including long introductory
comments, may require manual selection. Editing the opening prefix recomputes
the guess; manual choices bypass inference. No guessed language rewrites a
Markdown fence's info string.

The pinned CSS parser can expose braces inside a double-quoted string as error
syntax. This is related to upstream issue 51 / PR 52, whose scanner fix handles
single quotes. The current `opaque.tail` query preserves whitespace while this
error remains unresolved, so automatic outdent can be deferred. This is an
explicit upstream recovery limit, not a second lexical parser hidden in Flark.

The module is 13,123,001 bytes; gzip level 9 produces 1,798,080 bytes locally.
The loopback preview does not serve gzip. Bundling all current choices costs
more download/memory than the original five; lazy grammar distribution would
be a separate measured optimization, not another language integration.

## Verification and proof boundary

Evidence is stored in
[the migration receipt](../../docs/architecture/v5/receipts/single-engine-2026-09-07/receipt.json).
It identifies the dirty checkout and hashes the actual sources and served assets.
Historical receipts remain intact.

The checks cover component transport identity, real native/browser workers,
Flark source/caret/container/undo behavior, browser input/first paint and hands-on
release editing. They do not establish CI, physical-device/IME qualification or
sustained whole-editor frame budgets. Worker color latency is measured separately
from synchronous input cost; a passing source assertion does not prove either.

Passed locally: 9 Rust tests, 307 Dart component tests, 399 Flark core tests,
795 Flutter host tests, 25 workbench tests and 23 real Chrome tests. The 390-case
transport corpus is identical on VM, native AOT, dart2js/Node and dart2wasm/Node.
Native, browser JavaScript and browser Dart-Wasm workers each pass 412 exact
analysis comparisons and the 40-request supersession/disposal checks.

The final worker probes report maximum per-language detection p95 of 1.953 ms
native, 2.4 ms browser Dart-Wasm and 2.3 ms browser JavaScript. Maximum input-path
p95 is 3.393/4.0/3.9 ms respectively, with colors measured separately. An earlier
browser JavaScript run reached 30 ms detection p95 and 33.7 ms input p95; that
receipt is retained alongside the repeat. The cause of this variation has not
been established. These local probes do not close the sustained frame gate.

Hands-on release journeys used an isolated document and real keyboard/clipboard
commands: Automatic Ruby Enter and character-by-character `end`; Automatic Bash
Enter and final `i` outdent; manual Shell selection; HTML adjacent-tag splitting;
selected Tab/Shift-Tab; code-scoped Cmd+A/copy; multiline paste, next input and two
undos; source/rendered mode and reload. The following paragraph remained outside
the fence. Bulk text insertion is literal; smart keyword outdent is tested with
one character per input event.

The served preview uses `/_flark/0d6874d791f160fb/`. Its fetched engine bytes match
the verified asset hash. The owner's 1,041-byte document was compared exactly
before/after refresh through source-mode copy (1,037 UTF-16 units), then restored
to rendered mode. No browser warnings/errors were observed after refresh.

The testing lesson is to make inferred language and incomplete typing part of
the authoring contract. A grammar loading successfully and a final highlighted
program are insufficient: the Bash regression required inspecting the guess
before the last keystroke, while the browser loader failure could not be exposed
by the Node parity run. Both are now represented at the boundary that failed.
