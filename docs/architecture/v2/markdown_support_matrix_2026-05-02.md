# Flark v2 Markdown Support Matrix

Status date: 2026-05-14
Runtime target: source-first v2 architecture with required Comrak parsing,
headless projection/render-plan layers, and Flutter controller/widgets.

See also:
`docs/architecture/v2/markdown_test_matrix_2026-05-08.md` for the coverage
contract that maps each feature to concrete tests.

## Legend

- `Supported`: implemented in v2 and covered by v2 tests.
- `Partial`: meaningful support exists, but specialized UX polish remains.
- `Policy`: v2 has a defined behavior boundary rather than a full WYSIWYG
  feature.

## Core/CommonMark + GFM

| Feature | Parse | Projection/render plan | Editing UX | v2 status | Evidence |
| --- | --- | --- | --- | --- | --- |
| ATX headings | Comrak maps headings | Heading style tokens | `setHeadingLevel` command, Enter exit, Backspace unwrap | Supported | Feature matrix, native adapter, render-plan, block command, input command, projected/live widget tests |
| Setext headings | Comrak maps setext headings | Heading style tokens | Source-first editing; no dedicated setext command | Supported parser/render, source-first UX | Feature matrix |
| Blockquotes | Comrak maps quote blocks; adapter normalizes marker-only and multi-line quote ranges | Quote style/chrome and marker projection | `toggleQuote`, Enter continue/exit, Backspace remove/unwrap | Supported | Feature matrix, native adapter, block command, input command, preview, live widget, example Chrome, and golden tests |
| Unordered lists | Comrak maps list blocks/items, with synthetic editable item recovery when native output omits item spans | List item descriptors and marker projection | `toggleBulletList`, Enter continuation/exit, Backspace marker removal | Supported | Feature matrix, block command, input command, live widget, example Chrome tests |
| Ordered lists | Comrak maps ordered list blocks/items | Ordered list item descriptors and marker projection | `toggleOrderedList`, Enter continuation/exit, Backspace marker removal | Supported | Feature matrix, block command, capability, input command, live widget, public API tests |
| GFM task lists | Comrak maps checked task item metadata | Task descriptor, overlay target/control, live checkbox block widget | `toggleTaskList`, Enter/Backspace task marker policies, live checkbox source toggle | Supported | Feature matrix, native metadata, render-plan/control, block command, input, preview, live widget, golden tests |
| Fenced code blocks | Comrak maps code blocks and info strings | Code descriptor, overlay target/control, live editable code widget | `insertFence`, Enter indentation, source-preserving body edits, language picker, syntax highlighting, copy action, Tab/Shift-Tab indentation | Supported | Feature matrix, render-plan/control, block command, input, preview, live widget, golden tests |
| Indented code blocks | Comrak maps indented code blocks | Code descriptor when parser marks block as code | Enter continuation/exit and Backspace indent-unit outdent | Supported | Feature matrix and input command tests |
| Inline strong/emphasis/code | Comrak maps inline styles | Inline style tokens and marker projection | `toggleInlineStyle` command | Supported | Feature matrix, inline command, render-plan, capability, live styling, preview, golden tests |
| Strikethrough | Comrak maps GFM strikethrough | Strikethrough style token and marker projection | Standard source/projected editing | Supported | Feature matrix, render-plan style token support, visual golden |
| Links/autolinks | Comrak maps link/autolink styles and action metadata | Link action descriptors, escaped destination/title handling, nested image-link labels, destination hiding, overlay controls | Insert/apply link commands | Supported | Feature matrix, native metadata, upstream contract, link command, render-plan action/control tests |
| Escaped delimiters and UTF offsets | Comrak marker mapping preserves literal escaped delimiters | Escaped delimiter projection hides backslash markers while keeping literal delimiters | Standard source editing | Supported core | Feature matrix, projection escaped-delimiter, and UTF mapping tests |
| HTML entities | Comrak bridge emits decoded entity replacement ranges outside literal code/raw HTML and preserves escaped entities as literal text | Replacement-capable projection displays decoded text while preserving source ranges for edits | Standard projected/live editing maps display edits back to the source entity range | Supported core | Feature matrix, native bridge, native adapter, projection, edit-adapter, render-plan, and live widget tests |
| Thematic breaks | Comrak maps thematic break blocks | Render-plan block construction | `insertThematicBreak` command | Supported core | Feature matrix and block command tests |
| GFM tables | Comrak maps table descriptors and alignments | Table descriptor, overlay target/control, live editable table grid | Insert row/column/table commands and live table-cell source edits | Supported | Feature matrix, table command, render-plan/control, projection fixture, preview, live widget, golden tests |
| Images | Comrak maps image action metadata | Image action descriptor/control; projection exposes accessible label; nested image-link labels build without overlapping hidden ranges; read-only preview renders default image cards with open/copy/edit-source actions | Source-first editing with rendered media affordances | Supported core | Feature matrix, native metadata, upstream contract, render-plan image action/control, projection image fixture, preview image-card tests |
| Reference links/definitions | Comrak resolves reference links and hides reference definition lines | Link descriptor exposes resolved action metadata | Source-first editing | Supported core | Feature matrix, native hidden-range tests, projection reference-link tests |
| Raw HTML blocks/inlines | Comrak maps HTML variants and raw-HTML hidden ranges | Raw HTML style token; projection hides tags by policy | Literal-text/non-executable policy | Policy | Feature matrix, native hidden-range tests, projection raw HTML fixture |

## GitHub-Only Extension Policy

Flark's required parser profile is CommonMark plus the GFM extensions that
Comrak exposes for tables, task lists, strikethrough, and autolinks. GitHub.com
also supports product-level features that are not part of that core profile.
Until a feature has a typed parser/render/edit contract, v2 keeps it
source-visible and editable instead of partially rendering it.

| Feature class | Current v2 behavior | Evidence |
| --- | --- | --- |
| Alerts/callouts such as `> [!NOTE]` | Render as normal blockquotes with the alert marker visible. | Transition matrix |
| Footnotes such as `Text[^1]` and `[^1]: Note` | Stay literal/source-visible; adapter suppresses accidental shortcut-reference link mapping and does not hide footnote definitions. | Transition matrix, native adapter test |
| Mermaid, GeoJSON, TopoJSON, and STL code fences | Remain code fences with language metadata; no diagram/map/model renderer is mounted by default. | Fenced-code feature matrix and language metadata tests |
| Mentions, issue/PR references, emoji shortcodes, color chips, math, frontmatter, and admonition variants | Remain source-first/literal unless a future extension registers typed parser/render/edit behavior. | Unsupported-extension policy; future work must add tests before changing behavior |

## Live Editing and Projection Safety

| Area | v2 status | Evidence |
| --- | --- | --- |
| Canonical source transactions | Supported | Core transaction/runtime/history tests |
| Undo/redo grouping | Supported headless | History stack tests |
| Parser result adoption | Supported | Flutter controller tests and performance budget |
| Parser scheduling | Supported | `FlarkParseScheduler` tests and example app wiring |
| Comrak on web | Supported | Chrome WASM smoke tests and rebuilt example web bundle |
| Stale parse rejection | Supported | Flutter controller tests |
| Hidden marker source/display mapping | Supported | Projection tests and feature matrix |
| Boundary affinity at hidden markers | Supported | Projection affinity tests |
| Projected text editing | Supported | `FlarkProjectedTextEditAdapter` tests and `FlarkProjectedEditableText` tests |
| Live rendered editing | Supported | `FlarkLiveRenderedEditableText` tests, visual goldens, example widget/browser-path tests |
| Raw-source caret synchronization | Supported | `FlarkEditableText` selection behavior tests |
| Read-only/edit render-plan parity | Supported | Flutter render-plan parity test |
| Render-plan overlay controls | Supported | `FlarkRenderPlanOverlayControls` tests |
| Native bridge packaging contract | Supported | v2 packaging contract test |
| Performance budgets | Supported | v2 performance budget test |

## Remaining Release Boundaries

1. First-class render/edit extensions for GitHub-only product features such as
   footnotes, alerts, diagrams, mentions, issue references, color chips,
   emoji shortcodes, math, and frontmatter.
2. Owner-controlled external release decisions: license, canonical URLs,
   screenshots, `publish_to`, and pub metadata.
