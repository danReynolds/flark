# Snippet editing query contract

These query files are authored for Flark against the pinned upstream
grammars in `Cargo.lock`. Grammar/parser source is unmodified. This is a small
Flark capture vocabulary, not an implementation of every Helix or Zed query.
Unknown captures and custom predicates/properties fail grammar initialization.
The build hook tracks these files so changing a rule rebuilds the native asset.

| Capture | Meaning |
| --- | --- |
| `open.paren/bracket/brace`, `close.*` | Pair structural tokens. Missing parser tokens and literal tokens are excluded. Mismatches discard the current unmatched stack. |
| `open.block`, `close.block` | Pair keyword blocks using parser tokens, including incomplete headers. Postfix conditionals and endless methods are excluded by grammar shape. |
| `middle.family` | Align a completed branch keyword with the nearest unmatched opener in the same family; Enter after its header adds one unit. |
| `opaque` | Preserve whitespace inside a string or other literal. |
| `opaque.tail` | An unfinished literal represented by an error node: treat the remaining source as literal until a subsequent parse resolves it. |
| `opaque.body` | Preserve a literal's body after its header line, including its final caret position. |
| `code` | Allow syntax work inside interpolation. More specific nested literal captures override it. |
| `comment` | Ignore comment punctuation; permit a preceding structural opener/header to control Enter after a trailing comment. |
| `indent.end` | If this token ends the code before Enter, add one configured indentation unit. |
| `indent.scalar` | A literal header; a positive digit in its token specifies a relative space count, otherwise use one unit. |
| `indent.prefix` | Account for an inline container prefix before a newly indented body, such as a mapping after a sequence marker. |
| `anchor.family`, `branch.family[.family...]` | Match a branch to the nearest still-active compatible earlier line. Ordinary lines at an anchor's indentation retire that anchor. |

`languages.rs` holds the small language catalog, reindent trigger characters and
optional statement context. The common engine preserves existing whitespace,
adds the configured unit, splits adjacent delimiter pairs on Enter and aligns
typed closers with their opening line. Trigger data may include the final letters
of keyword tokens; a letter alone never authorizes an edit. The completed
parser capture must occupy the beginning of the line. It applies automatic outdent only when
the target is a shorter exact prefix of the current indentation. Tab/Shift-Tab
are explicit commands and preserve selection direction and end-line exclusion.
One tab is one indentation unit; Shift-Tab removes at most four leading spaces
when the configured unit is a tab. LF/CRLF and the caller's unit are explicit.

For Dart statement fragments and CSS declaration fragments, compare the normal grammar parse with a temporary function
body or CSS rule body only when actual error nodes indicate a possible context mismatch. Missing
closing punctuation alone does not trigger that second parse. Choose the lower
error-coverage result, using missing-token count as a tie-breaker. Clip all
captures/highlighting back to original source coordinates; wrapper text and
synthetic closing tokens cannot become edits or painted source.

## Provenance and limits

The design was checked against [Tree-sitter queries](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html)
and [Helix's indentation concepts](https://docs.helix-editor.com/master/guides/indent.html).
The Helix query review used revision
`079a789e8cb08ead67f19e1971a1b7438b37354b`; those MPL-2.0 files and its indentation
interpreter are not vendored or executed here. The Flark query files use the
package's MIT license. Their support is established by the named authoring
corpus, not by accepting arbitrary upstream query files.

The current contract covers delimiter/colon/header indentation and branch
alignment in fourteen languages. It does not claim full-file formatting, hanging
argument alignment, every braceless control-flow form, Python dedent after
`return`, YAML automatic dash continuation, embedded languages, or arbitrary
error recovery. New languages must add a capability record and authoring cases;
adding a parser alone is insufficient. If many new language-specific engine
branches are needed, revisit the contract before expansion.

## Catalog extensions

Matching `open.family`/`close.family` captures cover keyword blocks and tags.
HTML excludes void elements through an upstream text predicate; XML uses its own
grammar. Self-closing tags do not open a body. Bash `if`/`then`/`fi`,
`do`/`done`, `case`/`esac` and `else`/`elif` use the common family stack.
SQL supports delimiter indentation, not dialect-specific procedural formatting.
Helper captures beginning `_` are for query predicates only. Built-in text
predicates are evaluated by upstream QueryCursor; custom predicates/properties
are rejected for both editing and detection queries.

Automatic detection runs each grammar's `detect/*.scm` query on at most the first
128 UTF-16 units. Captures `signal.weak`, `signal`, `signal.strong` carry weights
1, 2, 8. The common ranker requires evidence, discounts parser error coverage,
and rejects error recovery which skips ordinary text before its first signal.
Equal scores retain catalog order. This is inference, not language recognition
with certainty; manual language selection bypasses it. The Dart cache keys the
exact bounded prefix, so edits later in a snippet do not re-run detection.

The pinned CSS grammar misparses some double-quoted literal braces. Its
`opaque.tail` error capture preserves whitespace until syntax recovers. That
conservative behavior can defer automatic outdent; it is not a second tokenizer.
Embedded JavaScript/CSS in HTML, JSX/TSX, semantic formatting and arbitrary broken
program recovery remain outside the qualified contract.
