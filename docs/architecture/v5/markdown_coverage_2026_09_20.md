# Markdown coverage — 2026-09-20

Flark implements a broad CommonMark/GFM editing baseline through one shared
parser and Dart kernel, with Flutter and Fleury presentation. It does not yet
have complete presentation parity or every Markdown extension. Parsing a construct
and preserving its source are weaker claims than a finished editing UI for it.

## Current feature coverage

| Feature | Flutter | Fleury | Scope and limits |
| --- | --- | --- | --- |
| Paragraphs, escapes, entities, soft/hard breaks | Supported | Supported | Shared source/projection mapping; Fleury wraps in character cells. |
| ATX H1–H6 and Setext headings | Supported | Supported | Flutter has per-level typography/sizes; Fleury uses one size with weight, italic, color and optional level labels/decorations. |
| Bold, italic, strikethrough, inline code and nested styles | Supported | Supported | Editing, selection, typing context and Undo are shared; terminal style availability affects appearance. |
| Ordered/unordered lists and nesting | Supported | Supported | Markers, continuation, indentation and structural editing; common marker gutters. |
| Task lists | Supported | Supported | Clickable checked/unchecked markers; text remains editable; read-only activation disabled. |
| Blockquotes and nested containers | Supported | Supported | Shared structure; each host paints rails and its own layout. |
| Fenced and indented code blocks | Supported | Supported | Editable body, selection, Tab/Shift-Tab, scoped Select All, shared optional Tree-sitter assistance. |
| Links, reference links and autolinks | Supported | Supported | Open/Edit/Remove controls, keyboard access, application opener callbacks and relative-resource resolution. |
| Raster images | Supported | Supported | Bounded previews, loading/error states, resource editing and custom resolution/presentation. Fleury previews use reserved cell rows and centered contain-fit; terminal graphics depend on protocol support. |
| GFM pipe tables | Supported | Supported | Aligned/wrapped cells, headers/borders, cell navigation, missing trailing cell insertion and Undo. Not a spreadsheet UI for inserting/deleting/reordering columns and rows. |
| Horizontal rules | Painted | Painted | Fleury now paints a themed rule within its container gutter; source and caret mapping remain shared. |
| Footnotes | Partial | Partial | Recognized and source preserved. Reference markers remain literal and definition bodies stay in document order; no dedicated numbering, superscript, jump/backlink or footnote-editor UI. |
| Raw inline/block HTML | Literal source | Literal source | HTML is preserved as editable text, not rendered or executed as browser HTML. |
| Math, diagrams, front matter, definition lists, custom directives | No dedicated support | No dedicated support | These extensions are not enabled in the parser; their text may still match ordinary CommonMark syntax. |

Both hosts also expose read-only views, source mode, Markdown theming and
replaceable resource controls. That is host functionality, not additional syntax.
Image previews do not imply SVG or arbitrary embedded HTML rendering.

## Code languages

The single Tree-sitter integration covers **14 languages** for highlighting and
snippet indentation: Dart, JavaScript, TypeScript, Python, Ruby, Rust, Go, JSON,
YAML, CSS, Bash, HTML, XML and SQL. Automatic detection is conservative; manual
language selection is authoritative. Both hosts now supply language pickers using the same catalog and shared
`SetCodeLanguage` command. Fleury opts into its built-in controls with
`showToolbar: true`; its composer example enables them.
Unsupported/unrecognized languages and
oversized snippets use plain text and ordinary whitespace editing.

This is snippet assistance, not formatting or an IDE. In particular, JSX/TSX,
embedded-language injections, SQL procedural block indentation, Python dedent
after `return` and YAML automatic dash continuation are outside the current
contract. The [language capability table](../../../packages/flark_tree_sitter/SINGLE_ENGINE_REVIEW.md)
details the per-language rules and exclusions. There is no second fallback
highlighter to maintain.

## Evidence and confidence

- The current parser enables tables, strikethrough, task lists, autolinks and
  footnotes in `native/flark_parse/src/model.rs`.
- Local Rust conformance/extraction tests passed again for **1,322 upstream
  CommonMark/GFM cases**. The register contains one accepted CommonMark emphasis
  deviation (example 354) and no registered GFM deviations. Extraction has zero
  deviations against Comrak's model. This is not a claim of 1,322 UI journeys.
- Shared tests cover source/caret invariants, edit semantics, structural edits,
  table cells, resource edits, fences, history and admission. Mounted host tests
  add painting, hit testing, selection and resource controls. Existing whole-host
  suites were not rerun merely to write this report; the merge's new executable
  changes are in the qualification driver, which received focused and full
  example testing.
- A temporary Fleury mounted probe confirmed Setext headings, literal HTML,
  literal footnote references/definition text, and the missing horizontal-rule
  paint in the initial audit (fixed in the composer follow-up). Its source and output are retained with the review receipts. It checked
  source preservation and observed real cells; it is not a permanent test that
  locks in the missing rendering as intended behavior.
- The [native performance results](native_attended_2026_09_20.md) apply to the
  measured Flutter/macOS workbench. They do not establish Fleury terminal frame
  budgets, physical IME behavior, or all accessibility journeys.

## Composer follow-up and priorities

The authorized composer follow-up closes both small Fleury gaps: horizontal-rule
paint and a built-in code-language picker. Both existing playgrounds now open
as document composers with formatting toolbars and optional theme panels.
Blank documents accept a heading choice before typing. The shared kernel
remains the only command/history owner. See [the composer review](composer_2026_09_20.md).

The owner's updated priorities explicitly defer ordinary-app native/device
qualification, table structure controls and dedicated footnote UX. They remain
open work, not release qualifications waived by the composer improvements.
Math, diagrams and custom directives remain separate feature decisions.

The core defaults to a 16 KiB live-rendering budget; the measured Flutter workbench
uses the separate 32 KiB live / 256 KiB source candidate. Larger accepted source
or a configurable limit does not imply equivalent live rendering or latency.
The packages remain unpublished development packages rather than a tagged,
fully qualified production release.
