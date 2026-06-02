# Sovereign v2 Markdown Test Matrix

Status date: 2026-05-14
Runtime target: Comrak-required source-first editor and previewer. The native
FFI bridge and browser WASM bridge are the supported parser implementations.

This matrix is the coverage contract for markdown behavior. A feature is not
considered fully covered unless the relevant rows below point to concrete tests
for parser output, projection/render-plan behavior, and the user-facing editing
or preview surfaces that expose the feature.

## Coverage Lanes

| Lane | Required evidence | Primary tests |
| --- | --- | --- |
| Parser contract | Real Comrak output produces valid ranges, no diagnostics, expected blocks/tokens/metadata, and deterministic output. | `test/v2/markdown/sovereign_markdown_feature_matrix_test.dart`, `test/v2/markdown/sovereign_native_comrak_parse_backend_test.dart`, `test/v2/markdown/sovereign_v2_native_upstream_contract_test.dart` |
| Projection/render plan | Hidden markers, display text, descriptors, action runs, overlay targets, and predictive descriptor preservation build from parser output. | `test/v2/markdown/sovereign_markdown_feature_matrix_test.dart`, `test/v2/projection/sovereign_projection_test.dart`, `test/v2/render_plan/sovereign_render_plan_test.dart` |
| Commands | Toolbar/API commands mutate canonical markdown through transactions. | `test/v2/markdown/sovereign_markdown_block_commands_test.dart`, `test/v2/markdown/sovereign_markdown_inline_commands_test.dart`, `test/v2/markdown/sovereign_markdown_link_commands_test.dart`, `test/v2/markdown/sovereign_markdown_table_commands_test.dart` |
| Keyboard policy | Enter, Backspace, Tab, and Shift-Tab route through markdown-aware source transactions. Shift+Enter is explicitly covered as a soft line break that bypasses structural continuation/exit behavior. | `test/v2/markdown/sovereign_markdown_input_commands_test.dart`, `test/v2/flutter/sovereign_markdown_input_policy_contract_test.dart`, `test/v2/flutter/sovereign_projected_editable_text_test.dart`, `test/v2/flutter/sovereign_live_rendered_editable_text_test.dart` |
| Read-only preview | Default preview widgets render the typed render plan. | `test/v2/flutter/sovereign_read_only_preview_test.dart`, `test/v2/flutter/sovereign_v2_visual_golden_test.dart` |
| Live edit surface | Live edit displays parsed structure while editing canonical markdown, keeps structured surfaces mounted during predictive edits, preserves source boundaries for block widgets, keeps focus/caret ownership aligned with the canonical selection, gives parser-omitted source lines editable hosts, and routes document-level intents through the canonical runtime. | `test/v2/flutter/sovereign_live_rendered_editable_text_test.dart`, `test/v2/flutter/sovereign_v2_visual_golden_test.dart`, `example/test/widget_test.dart` |
| Transitional state invariants | Each semantic feature that renders as live chrome or a block widget stays semantically stable between text input and parser adoption; multi-block edits must also prove the active local editor follows the canonical source selection after structural changes and cannot trap document-level selection/deletion inside one block. Structural exits or spacing edits that land in parser-omitted blank/gap positions must prove synthetic source-line hosts preserve visible rows, focus, and continued typing. | `test/v2/markdown/sovereign_markdown_transition_matrix_test.dart`, `test/v2/render_plan/sovereign_render_plan_test.dart`, `test/v2/flutter/sovereign_flutter_controller_test.dart`, `test/v2/flutter/sovereign_live_rendered_editable_text_test.dart`, `example/test/widget_test.dart` |
| Spec edge contracts | CommonMark/GFM precedence and boundary cases that commonly regress live editing stay pinned: 0-3 vs 4-space indentation, lazy continuation, setext vs thematic-break precedence, list marker digit limits, fenced info-string validity, code-span delimiter runs, intraword emphasis, reference links, GFM autolinks, GitHub alerts, and unsupported footnote syntax. | `test/v2/markdown/sovereign_markdown_transition_matrix_test.dart`, `test/v2/markdown/sovereign_v2_native_upstream_contract_test.dart` |
| Parser-backed fuzzing | Random command/source edits must keep controller invariants, parse through native Comrak, build projection/render-plan output, and adopt authoritative parse results without invalid ranges or diagnostics. | `test/v2/markdown/sovereign_markdown_fuzz_invariants_test.dart` |
| Web/example | Browser WASM and promoted example surfaces exercise the same parser path. | `test/v2/flutter/sovereign_markdown_web_smoke_test.dart`, `example/test/widget_test.dart` |

## Feature Matrix

| Feature | Parser/projection/render evidence | Editing and preview evidence | Status |
| --- | --- | --- | --- |
| ATX headings | Feature matrix, native adapter tests, render-plan heading style tests, projection hidden-range tests. | Block command tests, heading input tests, projected/live Backspace tests, visual goldens. | Covered |
| Setext headings | Feature matrix proves Comrak heading mapping and render-plan construction. | Source/projected editing preserves canonical markdown; no dedicated setext command by design. | Covered as parser/render; source-first editing only |
| Blockquotes | Feature matrix covers normal, empty, nested quote/list cases; native adapter tests cover marker-only and multi-line quote normalization; render-plan prediction tests preserve blockquote descriptors through edits. | Block command tests, Enter/Backspace input tests, projected/live policy tests, quoted empty-list exit tests, live surface stability tests, example Chrome scratch tests, preview tests, goldens. | Covered |
| Unordered lists | Feature matrix covers list marker hiding and list descriptors; render-plan prediction tests preserve unordered item descriptors through edits. | Bullet command tests, Enter/Backspace input tests, Shift+Enter soft-break tests, live marker rendering and stability tests, structural Enter focus-handoff tests, empty final item exit and repeated blank-line insertion through parser-omitted source-line hosts, select-all deletion of hidden markers, example Chrome scratch tests. | Covered |
| Ordered lists | Feature matrix covers ordered marker hiding and ordered descriptors; render-plan prediction tests preserve ordered item descriptors through edits. | Ordered-list command/capability tests, Enter/Backspace input tests, live marker rendering and stability tests, structural Enter focus-handoff tests, empty final item exit into parser-omitted source-line hosts, example Chrome scratch tests. | Covered |
| GFM task lists | Feature matrix covers checked metadata and task overlay targets; render-plan prediction tests preserve task descriptors through edits. | Task command/capability tests, Enter/Backspace input tests, live checkbox toggle and stability tests, empty final item exit into parser-omitted source-line hosts, example Chrome scratch tests, preview checkbox tests, goldens. | Covered |
| Fenced code blocks | Feature matrix covers language metadata, hidden fences, code descriptors, and overlay targets; native adapter tests cover closing-fence boundary projection; render-plan prediction tests preserve code descriptors through edits. | Fence command tests, Enter indentation tests, source-bounded live code body edit and stability tests, language selector tests, copy-action tests, Tab/Shift-Tab tests, preview/golden tests. | Covered |
| Indented code blocks | Feature matrix covers code-block parsing and render-plan construction. | Enter continuation/exit and Backspace outdent input tests. | Covered |
| Inline strong/emphasis/code | Feature matrix covers Comrak tokens and marker projection. | Inline command tests, live inline styling tests, preview inline styling tests, wrapping golden. | Covered |
| Strikethrough | Feature matrix covers GFM Comrak token emission and marker projection. | Render-plan style token support and visual wrapping golden. | Covered |
| Links and autolinks | Feature matrix covers link actions, escaped destination/title markers, nested image links, link destination hiding, and projection/render construction. | Link command tests, overlay-control tests, preview/golden action rendering. | Covered |
| Images | Feature matrix covers image action metadata, destination hiding, and image-as-link-label nesting. | Preview image card tests, open/copy/edit-source action tests, and overlay-control tests. Image editing remains source-first while the rendered card exposes common media affordances. | Covered core |
| Reference links and definitions | Feature matrix covers resolved link metadata plus hidden reference definitions. | Projection reference-link tests and link render action tests. | Covered |
| Thematic breaks | Feature matrix covers Comrak block mapping and render-plan construction. | Thematic-break insertion command tests. | Covered |
| GFM tables | Feature matrix covers table descriptors and alignment metadata; render-plan prediction tests preserve table descriptors through edits. | Table command tests, live table-cell editing and stability tests, irregular-row tests, preview table tests, goldens. | Covered |
| Raw HTML blocks/inlines | Feature matrix covers raw HTML block/inline mapping and hidden raw-HTML ranges. | Projection raw-HTML policy tests. Rendering remains literal-text/non-executable by policy. | Covered by policy |
| Escaped delimiters and UTF offsets | Feature matrix and projection fixture tests cover escaped delimiter mapping and UTF offset safety. | Source/projected editing keeps canonical source; no special WYSIWYG command. | Covered core |
| HTML entities | Feature matrix plus native bridge/adapter tests cover decoded `htmlEntity` replacement ranges, escaped entities, hidden-overlap filtering, and literal code/raw-HTML exclusions. Projection and render-plan tests cover replacement span display/source mapping. | Projected edit-adapter and live rendered widget tests prove decoded display text edits map back to canonical source ranges. | Covered core |
| GitHub-only extensions outside current parser profile | GitHub alert syntax remains a literal blockquote; GitHub footnote shorthand and definitions remain source-visible and do not become half-mapped reference links. Mermaid/GeoJSON/TopoJSON/STL fences remain code fences with language metadata, not diagram widgets. Mentions, issue references, emoji shortcodes, color chips, math, frontmatter, and admonition variants remain literal/source-first until first-class extensions are added. | Transition matrix and native adapter tests cover alert and footnote safety; future extension work must add parser/render/edit evidence before changing this row. | Covered by explicit unsupported-extension policy |

## Regression Rule

When a manual test exposes a markdown behavior bug, add or update one of these
rows before treating the fix as complete:

1. Parser/projection/render-plan assertion when Comrak output or marker
   elision is involved.
2. Headless command/input assertion when source transactions are involved.
3. Widget assertion for projected/live/read-only behavior.
4. Browser/example assertion when the failure was only visible through web or
   the example app.
5. Transitional-state assertion when the failure happens between input and
   parser adoption; semantic descriptors and mounted live surfaces must be
   checked before and after parse reconciliation.
6. Spec-edge assertion when the failure depends on CommonMark/GFM precedence or
   on unsupported GitHub-only syntax being kept source-visible.

The matrix must stay current with the implementation. Stale rows are bugs: they
can hide unsupported behavior as effectively as missing tests.
