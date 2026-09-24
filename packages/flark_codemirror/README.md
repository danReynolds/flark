# flark_codemirror

Code snippet highlighting and indentation for Flark, from CodeMirror 5's
language modes ported to pure Dart. It runs synchronously on the edited fence,
with no native library, Wasm module or worker, on the VM, dart2js and
dart2wasm alike.

`FlarkCodeMirror` implements the kernel's `CodeEditingDelegate`:

```dart
final editor = FlarkEditor(backend, codeEditing: FlarkCodeMirror(), text: markdown);
```

Status: JavaScript, TypeScript and JSON are ported (CodeMirror's `javascript`
mode). Other fence languages are plain, and their edits take the kernel's
defaults. A fence without a language is plain: there is no automatic
detection yet. The hosts still use `flark_tree_sitter`; this package is meant
to replace it once the remaining modes are ported.

## Layout

- `lib/src/stream.dart`, `lib/src/mode.dart`: CodeMirror's `StringStream` and
  the mode interface, with a `runMode` driver.
- `lib/src/modes/`: one file per ported upstream mode, keeping its structure.
- `lib/src/highlight.dart`: styles to the kinds hosts theme, as tokens that
  tile the snippet.
- `lib/src/edit.dart`: Enter, typing and Tab proposals from a mode's
  indentation.
- `tool/reference/generate.cjs`, `tool/corpus/`, `test/reference_test.dart`:
  the check against CodeMirror itself.

## Checked against CodeMirror

`test/fixtures/reference.json` holds CodeMirror 5.65.21's own tokens and
indentation for the corpus, two of CodeMirror's sources and seeded mutations
of them, recorded in Node by `tool/reference/generate.cjs`. The port must
match every token's range and style and, for each line, the indentation for
the line as written and for an empty line in its place. Regenerate it after
changing the corpus:

```sh
npm pack codemirror@5.65.21 && tar xzf codemirror-5.65.21.tgz
node tool/reference/generate.cjs package test/fixtures/reference.json
```

A third argument runs another corpus directory; point
`FLARK_CODEMIRROR_REFERENCE` at its output to test the port against it.

## Editing

Enter indents the new line with the mode's indentation, as CodeMirror's
`newlineAndIndent`; between `[]` or `{}` it opens an empty line and indents
both, as its `closebrackets` addon, except inside a string or comment. Enter
on an indented empty line keeps its whitespace. Typing re-indents a line when
the mode's electric input matches it, or when `)` or `]` starts it. Tab and
Shift-Tab shift whole lines by the snippet's unit. Snippets indented with tabs
get tabs, measured at four columns.

`tool/bench.dart` times highlighting and proposals; `bench_core.dart` serves
web builds with the same snippets embedded.

Third-party notices are in `THIRD_PARTY_NOTICES.md`.
