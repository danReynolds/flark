# Owner code-region input follow-up — 2026-09-07

The owner reported an uncolored Ruby method, broken paste and Select All
expanding to the document from inside a fence. These observations exposed three
different gaps: missing language coverage, an untested keyboard route, and a
selection contract that did not match the expected code-region interaction.

## Ruby coverage

`def test` followed by `end` is a valid empty method (`ruby -c`: `Syntax OK`).
The shipped catalog did not include Ruby. Automatic detection could therefore
choose another language, while an explicit `ruby` tag remained plain.

Ruby now uses the unmodified grammar already supplied by the pinned
`highlight` dependency, with a manual picker entry and `rb` alias. The reported
snippet is recognized in Automatic mode; tests assert keyword and method-name
scopes and exact text preservation. This adds no dependency or bespoke Ruby
syntax rules. The existing generic delimiter behavior has a declared Ruby
bracket case. Ruby-specific `def`/`end` indentation is still outside the four
qualified Tree-sitter languages (Dart, JavaScript, Python, YAML).

## Clipboard route

The direct ClipboardEvent path worked, but web Cmd+V was consumed by the
editor's key handler, which attempted an asynchronous `Clipboard.getData` call.
The real DOM-key regression reproduced both prevention of the normal browser
paste event and `PlatformException(paste_fail, Clipboard.getData failed.)`.

Simply returning `ignored` was insufficient: ancestor Flutter shortcuts then
consumed the key and invoked the same clipboard API. Web Cmd/Ctrl+C, X and V
now return `skipRemainingHandlers`, which stops the Flutter shortcut chain while
allowing the browser default action. The existing clipboard binding owns the
resulting copy/cut/paste event and commits the semantic command exactly once.
Native Flutter clipboard actions retain their existing platform route.

The regression sends DOM modifier/key down/up events through the Flutter
engine and focused editor, verifies the browser default is allowed, then sends
the clipboard data. Synthetic keys cannot trigger an OS clipboard default on
their own. Both Cmd and Ctrl paste routes are checked; copy/cut also check their
keyboard route before the existing exact-content and single-commit assertions.
Typed closers remain distinct from literal pasted closers.

## Select All contract

The shared Dart kernel now owns `SelectAll()`. Within one rendered fence, the
first invocation selects its projected body; the second selects the document.
Further invocations leave the whole-document selection stable. Prose, source
mode and cross-region selections select the document immediately. Other kernel
commands restart this sequence. This policy belongs in the kernel so Flutter
and later Fleury hosts do not implement divergent region rules.

An empty fence's first invocation stays at its empty body. A small transient
sequence flag distinguishes that intent from a second invocation, since both
empty-body selection and an ordinary caret are collapsed. It is not a second
document or history owner. The command uses parser-owned projection ranges;
it does not scan delimiters. Replacing scoped contents keeps the fence and
neighboring prose, translates nested quote/list prefixes and CRLF, places the
next character correctly and undoes to the original selected body.

Tests cover partial/reverse selections, empty and unclosed fences, a document
consisting only of one empty fence, quoted/list-contained code, CRLF, Unicode,
source mode, repeated selection and replacement/undo. The random command matrix
also includes Select All. The existing declaration boundary still counts the
closed command set as one concept; its ceiling was not raised.

## Verification and reflection

The reported Ruby and Select All behaviors were reproduced by failing tests
before implementation. The clipboard regression failed on the real DOM-key
path before correction. Final checks: **446 kernel tests, 380 Flutter host
tests, 25 workbench tests and 10 Chrome browser tests (861 total)**, clean kernel
and Flutter analysis, and the release/Wasm build. Logs and source/build hashes
are in [`receipts/owner-code-2026-09-07/`](receipts/owner-code-2026-09-07/).
The existing CupertinoIcons build warning remains. Tree-sitter/Rust grammar and
transport code did not change in this follow-up.

Hands-on checks used the normal release/Wasm workbench in an isolated embedded
browser tab. The exact reported example displays `Auto · Ruby` and colors `def`,
the method name and `end`. Cmd+A copies only the code body; Cmd+V replaces it
while retaining the fence and following paragraph. Subsequent text and two undo
actions restore the original selected body. Repeated Cmd+A expands to the
document. In a separate empty-fence journey, one Cmd+A followed by keyboard
paste fills the fence and preserves the paragraph below. The observed browser
error/warning log was empty. The user's draft was not edited during these checks.

The previous browser pass demonstrated clipboard-event behavior but did not
prove that a user's keyboard shortcut reached it. Passing a lower-level input
path must not stand in for keyboard, menu and browser delivery. Keep the DOM
shortcut stage in the browser regression. The Ruby report also reinforces that
a language's absence needs to remain explicit; four qualified grammars are not
twenty-language coverage. Select All was an interaction-contract correction,
not a parser edge case. Full-frame, real IME, native-device and broader language
qualification remain separate from this browser follow-up.
