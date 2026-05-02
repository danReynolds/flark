# Sovereign Editor Markdown Support Matrix

**Status date**: 2026-05-02  
**Runtime backend**: native `comrak` (macOS/iOS/Android)  
**Scope**: live editor parse/projection/render/editing UX plus sovereign
read-only surface (`SovereignMarkdownView`) on focused post detail screens
(not HTML export UI)

## Legend

- `Supported`: implemented and covered by tests
- `Partial`: implemented in some layers but missing UX/rendering polish or not fully verified
- `Planned`: intentionally not yet implemented
- `Out of scope`: not targeted for current editor UX

## 1) Core/CommonMark + GFM Support Matrix

| Feature | Parse backend | Live rendering/projection | Editing UX | Status | Evidence / Notes |
| --- | --- | --- | --- | --- | --- |
| ATX headings (`#`) | Yes | Marker hidden + header styling | Standard typing | Supported | `block_markdown_rendering_test.dart`, `commonmark_syntax_engine_adapter_test.dart` |
| Setext headings (`===` / `---`) | Yes | Underline markers hidden | Standard typing | Supported | `commonmark_syntax_engine_adapter_test.dart` |
| Blockquotes (`>`) | Yes | Quote styling + rail | Continue/exit (double-enter + cursor escape) | Supported | `blockquote_editing_test.dart`, `native_live_editing_regression_test.dart` |
| Unordered lists (`- * +`) | Yes | Marker projection (bullet visual) | Continue/exit/backspace | Supported | `list_policy_editing_test.dart`, `block_markdown_rendering_test.dart` |
| Ordered lists (`1.`) | Yes | Marker projection (index visual) | Continue/exit/backspace | Supported | `list_policy_editing_test.dart`, `block_markdown_rendering_test.dart` |
| GFM task lists (`- [ ]`, `- [x]`) | Yes (GFM) | Checkbox marker hidden + checked styling | List flows + interactive checkbox tap toggle | Supported | `block_markdown_rendering_test.dart`, `list_policy_editing_test.dart`, `task_checkbox_interaction_test.dart` |
| Fenced code blocks (``` / ~~~) | Yes | Fence markers hidden; exclusion ranges; code styling | Strong UX: auto-open/close, exits, backspace rules, indent/tab/paste | Supported | many `fence_*` tests, `native_live_editing_regression_test.dart` |
| Fence language info string | Yes | Hidden for recognized tags; dropdown + highlighting integration | Picker changes info tag | Supported (curated) | `code_fence_language_picker*_test.dart`, `fence_info_string_*_test.dart` |
| Indented code blocks | Yes | Exclusion range behavior | Enter continuation/exit + Backspace indent-unit outdent | Supported | continuation, blank-line exit, space/tab outdent, and list/fence regressions covered |
| Inline bold / italic / code | Yes | Markers hidden + inline styling | Toolbar insertion + backspace re-entry | Supported | `predictive_inline_markers_test.dart`, `toolbar_markdown_insert_test.dart`, create-post toolbar tests |
| Links / autolinks | Yes (GFM autolinks in GFM lane) | Inline link styling + actions overlay | Typing + boundary-exit policy + link edit/open/copy actions | Supported | fixture/parity tests + `link_policy_editing_test.dart` + `link_actions_overlay_test.dart` |
| Strikethrough (`~~`) | Yes (GFM) | Inline styling | Standard typing | Supported | fixture/parity tests |
| Escapes / entities | Parser yes | Escaped delimiters and entities stay source-stable; escaped delimiters do not create hidden marker ranges | Standard typing + predictive cursor safety | Supported | parser, scanner, renderer, and predictive cursor-safety regressions |
| Thematic breaks (`---`, `***`) | Yes | Divider-style glyph rendering + marker hiding | Standard typing + horizontal-rule command | Supported | parser-backed classification, divider rendering, and command interaction covered |
| Tables (GFM) | Yes (GFM) | Monospace + source-aligned row formatting baseline (parser-backed table blocks) | Enter row continuation + Tab/Shift-Tab cell navigation baseline | Partial | no true grid layout/column metrics UI; row/column ops still pending |
| Images (`![alt](url)`) | Parser yes | Placeholder + inline/standalone preview card + actions overlay | Edit/open/copy actions + markdown text editing | Partial | `image_actions_overlay_test.dart`; image node remains markdown-source-first (not full embedded media block model) |
| Reference links/definitions | Parser likely yes | Definition marker prefix hiding baseline | None | Partial | `[id]:` marker projection hidden; broader ref-link UX still pending |
| Raw HTML blocks/inlines | Parser parses | Literal text only; no HTML execution | Text-only policy | Supported policy | `raw_html_policy_2026-05-02.md`; read/editor rendering tests keep tags literal |

## 2) Live-Editing / Projection Safety Matrix

| Area | Status | Evidence / Notes |
| --- | --- | --- |
| Single-flight parse scheduling | Supported | `syntax_parse_scheduler_test.dart`, `controller_engine_wiring_test.dart` |
| Stale parse drop | Supported | `controller_engine_wiring_test.dart` |
| Cursor snap out of hidden marker interiors | Supported | `snapshot_gap_cursor_safety_test.dart`, `controller_engine_wiring_test.dart`, native regression suite |
| Predictive inline marker hiding during async gap | Supported | `predictive_inline_markers_test.dart` |
| Predictive local fallback bounded work | Supported | `predictive_inline_markers_test.dart` |
| Fenced-code exclusion stability during predictive gap | Supported | `predictive_inline_markers_test.dart` (stale/ambiguous fenced exclusion regression) |
| Unicode UTF-8/UTF-16 offset mapping | Supported | `utf8_utf16_offset_mapper_test.dart`, native backend mapping tests |

## 3) Product-Prioritized Gaps (Recommended Next Work)

### Priority A (high product value)

1. **Tables (GFM)**  
   Build true structured rendering (column metrics/alignment independent of source spacing) and richer editing affordances (row/column ops). Baseline source formatting + Tab navigation now exist.
2. **Images / media markdown UX**  
   Expand placeholder support into inline preview/attachment widgets and media-specific interactions.
3. **Reference links / definitions**  
   Add full reference link rendering/cursor behavior (definitions now have baseline marker hiding).

### Priority B (polish / completeness)

4. **Thematic break visual treatment**  
   Supported: parser-backed classification, divider rendering, and horizontal-rule command interaction are covered.
5. **Indented code block editing niceties**  
   Supported: Enter continuation/exit and Backspace indent-unit outdent are covered for space and tab indents.
6. **Explicit coverage for escapes/entities and nested combinations**  
   Supported: escaped inline delimiters and entities have parser, scanner, renderer, and predictive cursor-safety coverage.

### Priority C (policy decisions)

7. **Raw HTML strategy**  
   Supported policy: raw HTML stays literal text in editor/read-only surfaces; apps that need rendered HTML must sanitize outside Sovereign.
8. **Extended GFM features (if needed)**  
   Footnotes, tables advanced alignment, task-list interactions beyond current text semantics.

## 4) Recommended Acceptance Gate for New Markdown Features

For each new feature, add all of:

- parser conformance/parity fixture (backend correctness)
- projection/hidden-marker test (cursor safety)
- live editing interaction regression (enter/backspace/selection behavior)
- rendering test (`buildTextSpan` / widget output)

## 5) Read-Only Sovereign Surface Status

| Area | Status | Notes |
| --- | --- | --- |
| `SovereignMarkdownView` package API | Supported | Exposed from `package:sovereign_editor/sovereign_editor.dart` |
| Parse/render parity (same sovereign pipeline) | Supported | Uses `SovereignController` + `SovereignTextRenderer` + `Tier1Painter` |
| Read-only parity regression suite | Supported | `sovereign_markdown_view_parity_test.dart` validates heading/quote/list/task/fence/link/image/thematic-break semantics |
| Focused post detail rollout | Supported | `view_post_screen.dart` now renders body via `SovereignMarkdownView` |
| Link open interaction | Supported | Tap-to-open via `onOpenLink` callback when enabled |
| Link/image copy/edit overlay actions | Supported | `SovereignMarkdownView` read overlay provides open/copy/edit actions for link and image targets |
| Feed/card rollout | Supported (current app surfaces) | Feed excerpt markdown previews now render through `PostReadMarkdown` with sovereign clamp + read-more overlay |
