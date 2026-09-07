# Ruby owner indentation follow-up — 2026-09-07

The owner typed `def hello`, Enter, `print 'test'`, Enter, `end` in an
Automatic fence. Coloring identified Ruby, but the body did not indent.
The previous owner follow-up added Ruby to the fallback highlighter only;
the host's Tree-sitter resolver still admitted Dart, JavaScript, Python and
YAML. This was a capability gap in a normal authoring sequence.

## Change and capability

Ruby 0.23.1 is now a pinned, unmodified upstream grammar in the same native/Wasm
engine. `CodeLanguage.ruby` is appended as ABI language ID 5; existing IDs and
protocol v3 remain unchanged. The host resolves Ruby for both synchronous edits
and worker coloring. Explicit `ruby`, `rb` and Automatic are exercised.

The shared capture interpreter adds keyword block pairs and middle branches.
The Ruby query supplies parser-owned tokens for `def`, class/module, ordinary
conditionals and loops, `begin`, case and `do` blocks. Enter adds one configured
unit (two spaces in the host); completing a line-leading `end`, `else`, `elsif`,
`when`, `rescue` or `ensure` aligns with the active block. Branch headers,
including pattern `in`, indent their bodies on Enter. Delimiters retain the
existing selection-aware behavior. No Ruby text scanner or Flutter-only rules
were introduced.

Postfix conditionals, endless methods, closed one-line blocks, strings,
comments, regex punctuation and heredoc bodies have explicit negative cases.
Existing indentation, tabs, Unicode and CRLF are preserved. Typing a keyword
can outdent; paste and composition remain literal host operations. This is
snippet authoring support, not a formatter: it does not repair existing body
indentation, align arbitrary hanging arguments, or claim every Ruby error
recovery/embedded-language form.

## Evidence

The [receipt](../../docs/architecture/v5/receipts/ruby-indent-2026-09-07/manifest.json)
identifies this dirty working tree and served assets. The underlying base commit
is recorded there; no commit or push was made.

- 55 Ruby component cases join the shared corpus. The host runs applicable
  cases inside ordinary, quote/list and nested-quote CRLF fences, checking
  source, selection, code membership, next character, undo and redo.
- Three empty-fence owner journeys type every character using `ruby`, `rb`
  and Automatic. A separate highlighting case checks every incomplete prefix,
  keyword/method/string scopes and exact Unicode source.
- 8 Rust, 180 Dart component, 548 Flutter host, 25 workbench and 12 actual
  Chrome tests pass. Dart/Flutter analysis is clean and the release Wasm web
  build succeeds. Chrome checks Ruby Enter and typed-end first paint, real
  worker coloring, stable caret, following input and undo/redo; clipboard and
  real-font regressions also pass.
- 200 results match across native JIT, native AOT, dart2js/Node and Dart-Wasm/Node.
  Native and both real-browser worker runtimes each pass 280 direct/worker
  comparisons and a 40-request supersession burst.
- Hands-on release browser check at isolated origin 8815: type `def hello`,
  Enter, `print 'test'`, Enter, then `en` and `d`. Automatic reports Ruby;
  body indentation and `end` alignment are visible and source-exact. Appending
  ` # done`, undoing, and scoped Cmd+A/Cmd+C preserve the code and following
  paragraph. Browser error/warning log was empty. Screenshots were inspected
  inline, not archived as files.

The preview was rebuilt with prefix `/_flark/17cdf611b4424a93/` and refreshed
at port 8813 after the owner's 1041-byte draft showed Saved locally. Existing
unindented source is retained; the corrected behavior applies to new typing.
The Wasm engine is 6,272,909 bytes; adding a grammar has a packaging cost.
These are local functional and component timing receipts, not closure of the
sustained full-frame performance, physical IME or device gates.

## Reflection

Recognizing a language and coloring its tokens did not prove its authoring
behavior. Exposing Ruby beside the other languages invited the reasonable
expectation that Enter would also work. The missing case was ordinary, not an
obscure Ruby edge case. Future language additions need an empty-fence typing
journey under Automatic and explicit selection, alongside literal negatives,
container mapping and undo. Adding a parser or seeing colored output alone
cannot qualify the language for the workbench.

This repair advances one owner-requested language; the existing performance
gate and broader twenty-language expansion remain open. The extension stayed
in grammar data plus the shared keyword-block vocabulary, which is consistent
with the chosen architecture and does not require an editor runtime per language.
