# flark_codemirror

Code snippet highlighting, indentation and language detection for Flark, from
CodeMirror's language modes ported to pure Dart. It runs synchronously on the
edited fence, with no native library, Wasm module or worker, on the VM,
dart2js and dart2wasm alike.

`FlarkCodeMirror` implements the kernel's `CodeEditingDelegate`:

```dart
final editor = FlarkEditor(backend, codeEditing: FlarkCodeMirror(), text: markdown);
```

An app that highlights only some languages names them, and the other modes
stay out of its build:

```dart
FlarkCodeMirror.only([CodeMirrorLanguages.python, CodeMirrorLanguages.bash]);
```

The hosts still use `flark_tree_sitter`; this package is meant to replace it.

## Languages

| Languages | Port | Upstream |
| --- | --- | --- |
| JavaScript, TypeScript, JSON | `javascript.dart` | CodeMirror 5.65.21 `javascript` |
| Python | `python.dart` | legacy-modes `python` |
| C, C++, Java, C#, Kotlin, Dart | `clike.dart` | legacy-modes `clike` |
| PHP | `php.dart`, with PHP's `clike` configuration | CodeMirror 5.65.21 `php` |
| HTML, with CSS and JavaScript inside | `htmlmixed.dart` | CodeMirror 5.65.21 `htmlmixed` |
| XML | `xml.dart` | legacy-modes `xml` |
| CSS, SCSS, LESS | `css.dart` | legacy-modes `css` |
| SQL, PostgreSQL, MySQL | `sql.dart` | legacy-modes `sql` |
| Bash | `shell.dart` | legacy-modes `shell` |
| YAML | `yaml.dart` | legacy-modes `yaml` |
| Go | `go.dart` | legacy-modes `go` |
| Ruby | `ruby.dart` | legacy-modes `ruby` |
| Rust | `rust.dart` on `simple_mode.dart` | legacy-modes `rust`, `simple-mode` |
| PowerShell | `powershell.dart` | legacy-modes `powershell` |

legacy-modes is `@codemirror/legacy-modes` 6.5.4, CodeMirror 6's collection of
its version 5 modes. Each port keeps its upstream's structure and names its
source file.

A fence's info string names its language through aliases and extensions
(`js`, `py`, `sh`, `yml`, `rs`, `ps1`, `pgsql`, `htm` and others,
`codeMirrorAliases`). A fence in a language without a port is plain but
indents by its brackets (`BracketsMode`); `text` and its aliases are plain
and take the kernel's editing defaults.

## Detection

A fence without a language takes the language it looks like
(`lib/src/detect.dart`). Each language carries signs (`lib/src/signs.dart`):
patterns its code shows and other text rarely does, such as `def f():` for
Python or `err != nil` for Go, each weighted by how sure a sign it is, and
some counting against it. The language with the most evidence wins if it
reaches 2.5 points (half a point less a line for one or two lines), and
ties go to the language listed first. TypeScript, C++ and PHP carry their
base language's signs and win on their own; a dialect (SCSS, LESS and the
SQL dialects) needs its base's signs to reach half the threshold. JSON is a
strict shape check. A sample that is mostly sentences stays plain.
Detection reads the fence's first 512 code units.

Signs run only where they can match: one pass lists the sample's tokens and
each line's first, and each pattern's possible first tokens, read from its
source, decide which lines it tries or whether it scans at all.

`tool/detect_eval.dart` measures detection on windows of files in each
language, as fences excerpt them, and on the repository's Markdown
paragraphs, which should stay plain. Wider local directories can be added
as `language=dir`. Over real files on one machine (12,540 windows with at
least 30 characters of code), it named the right language for 67% and the
right family (JavaScript for TypeScript, C for C++, a SQL dialect for SQL)
for 76%; when it named one, it was right 97% of the time, and it claimed 13
of 4,684 paragraphs.

## Checked against CodeMirror

Every port is compared with its upstream running in Node: each token's range
and style and, for each line, the indentation for the line as written and
for an empty line in its place (what Enter asks). The cases are the corpus
files in `tool/corpus/` and seeded mutations of them (deleted characters,
stray brackets and quotes, tabs, CRLF).

- `tool/reference/generate.cjs` records CodeMirror 5.65.21's `javascript`
  mode (`test/fixtures/reference.json`, 429 cases).
- `tool/reference/generate_legacy.mjs` records the legacy modes listed in
  `tool/reference/legacy.json` (`test/fixtures/legacy/`, 41 or 82 cases a
  language). It runs them on CodeMirror 6's own `StringStream`, lifted from
  `@codemirror/language` 6.12.4.
- `tool/reference/generate_mixed.mjs` records the composed modes, mixed HTML
  and PHP, from the same parts (`test/fixtures/mixed/`, 164 cases).

Wider local runs over real files matched too: JavaScript, TypeScript and
JSON in 5,860 cases, Python 1,000, Rust, XML and the xml mode's HTML 4,000
each, mixed HTML 600, PHP 3,005 (its two local files and the corpus, under
600 mutations each), LESS 8 (the only local files) and 80 to 600 for each
other language, besides 1,600 generated edge cases for each CSS and SQL
mode. Upstream differences left: Node's Unicode 17 tables call three
characters (U+A7CE, U+A7D2, U+A7D4) upper case where the Dart VM does not,
which C and C++ read for reserved identifiers, and Ruby's indentation is
null where upstream's is NaN.

Regenerate a fixture after changing its corpus:

```sh
npm pack codemirror@5.65.21 @codemirror/legacy-modes@6.5.4 @codemirror/language@6.12.4
# unpack each tarball into its own directory
node tool/reference/generate.cjs <codemirror dir> test/fixtures/reference.json
node tool/reference/generate_legacy.mjs <legacy-modes dir> <language dir> [language...]
node tool/reference/generate_mixed.mjs <legacy-modes dir> <language dir> <codemirror dir>
```

Each generator also runs one language over another directory (`--corpus`,
`--out`); point `FLARK_CODEMIRROR_REFERENCE`,
`FLARK_CODEMIRROR_LEGACY_REFERENCE` or `FLARK_CODEMIRROR_MIXED_REFERENCE` at
the output to test the port against it.

## Editing

Enter indents the new line with the mode's indentation, as CodeMirror's
`newlineAndIndent`; between `[]` or `{}` it opens an empty line and indents
both, as its `closebrackets` addon, except inside a string or comment. Enter
on an indented empty line keeps its whitespace. Typing re-indents a line when
the mode's electric input matches it, or when `)` or `]` starts it. Tab and
Shift-Tab shift whole lines by the snippet's unit. Snippets indented with tabs
get tabs, measured at four columns.

## Speed and size

`tool/bench.dart` times highlighting, Enter and a typed closer at the end of
each corpus file repeated to 2K and 8K, and detection of the file as an
untagged fence; `bench_core.dart` serves web builds with the same snippets
embedded. Local AOT on an M1 Pro, machine loaded by other work, median of
41 runs, commit `e75b9110`:

| 8K snippet | Highlight | Enter |
| --- | ---: | ---: |
| XML | 0.3 ms | 0.3 ms |
| JSON, Kotlin, Dart, Bash, SQL and its dialects | 0.5–0.6 ms | 0.4–0.5 ms |
| JavaScript, TypeScript, CSS, SCSS, LESS, PHP | 0.7–0.8 ms | 0.6–0.7 ms |
| Java, C#, Go, HTML, C, Ruby | 1.0–1.2 ms | 0.8–1.1 ms |
| C++, YAML, Rust | 1.4–1.9 ms | 1.2–1.7 ms |
| PowerShell, Python | 2.3–2.4 ms | 2.1–2.3 ms |

Detection takes 0.2 to 0.9 ms. Highlighting results are cached per fence.

Size, dart2js and dart2wasm at `-O4`, of a program that highlights and
proposes through `FlarkCodeMirror`, over the same program with a stub
delegate:

| Languages | dart2js | gzipped | dart2wasm | gzipped |
| --- | ---: | ---: | ---: | ---: |
| All 25 | 241 KB | 78 KB | 252 KB | 90 KB |
| JavaScript, TypeScript, JSON | 83 KB | 26 KB | 87 KB | 30 KB |
| Python alone | 51 KB | 18 KB | 54 KB | 22 KB |

## Layout

- `lib/src/stream.dart`, `lib/src/mode.dart`: CodeMirror's `StringStream`
  and the mode interface, with a `runMode` driver.
- `lib/src/modes/`: one file per ported upstream mode, keeping its structure.
- `lib/src/languages.dart`: the ported languages, their aliases and modes.
- `lib/src/signs.dart`, `lib/src/detect.dart`: detection.
- `lib/src/highlight.dart`: styles to the kinds hosts theme, as tokens that
  tile the snippet.
- `lib/src/edit.dart`: Enter, typing and Tab proposals from a mode's
  indentation.
- `tool/reference/`, `tool/corpus/`, `test/*reference_test.dart`: the checks
  against CodeMirror.

Third-party notices are in `THIRD_PARTY_NOTICES.md`.
