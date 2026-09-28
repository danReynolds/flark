# Changelog

## 0.5.0-dev.1 (unreleased)

- Render-model schema V5: UTF-16 offsets only, packed line, block, content and run records, and an extras section behind a sorted run-extra index. Models are 2.0–4.7× smaller. `records::expand` rebuilds the extractor's byte-level layout for tests and the dump tool.
- Faster keystrokes: linear projection, a caret index built already sorted, projection rows and host layouts reused across edits and read-only updates, mimalloc on native targets, and source text encoded straight into native and Wasm memory. Measurements are in `docs/architecture/v5/performance_review_2026_09_22.md`.
- Live limits per platform: `flarkDefaultLiveBytes` renders 32 KiB live on desktop and in desktop browsers and 16 KiB on phones and tablets, and the workbench candidate uses the kernel's count caps. `FlarkController`, `FlarkSession` and `FlarkReader` accept `syncLimit` and `liveLimits`. A browser frame profile (`web_frame_profile.dart`) gates edits at 32 KiB against the frame budget.
- While exact code colors are pending, a fence paints its previous colors mapped through the edit (RFC 031). The macOS frame profile gates UI work and raster at 16 ms p99 and requires each edit to reach the next frame.
- Fixed a multiline inline HTML tag that ended on a lazy continuation line displaying the text after it twice. The parse crate's test invariants now reject overlapping sibling runs.
- Added the V5 Flutter editor/viewer and a local-draft workbench for macOS and Flutter web/WASM, with selection, clipboard, source inspection, keyboard input, and first-paint regression coverage.
- Added `flark_tree_sitter`: one fourteen-language service for syntax highlighting, automatic/manual language selection, and snippet indentation through shared native and WASM transports. Removed the active Dart highlighter and language-specific lexical fallback.
- Hardened semantic editing, history, and source-mode admission. Review regressions cover tab-padded list continuation, graphemes spanning hidden formatting, and literal fence delimiters pasted into code regions. Render-model schema V3 publishes exact list-marker source endpoints.
- v5 M2: the pure-Dart kernel in `packages/flark`: projection with hidden ranges and caret spans, document with anchors, the closed command set, grouped history that restores typing intent, and the `FlarkEditor` facade; journeys, a generated command matrix, and a keystroke bench. The parse crate records each line's innermost prefix start and splits text nodes into exact and replacement pieces.
- v5 M1: `native/flark_parse` (unmodified comrak plus a flat render model, three-function ABI on native and wasm32) and `packages/flark` (render model views, FFI and Wasm transports, build hook with prebuilt resolution). v4 moved under `legacy/v4/`. See RFC 030.


## 0.1.0-dev.1

- Cut over to the portable v4 package split: `flark` owns the headless
  Dart/Rust runtime and `flark_flutter` owns the Flutter product surface.
- Added bounded, source-authoritative opening, editing, certification, semantic
  viewport, history, and lifecycle APIs through ABI 4.34.
- Added parser-authored literal-safe insertion envelopes, ABI 4.27's bounded
  closure/carry proof for immediate word/space successors, and fail-closed exact
  source rendering whenever result-revision semantics are not proven current.
- Added ABI 4.28 parser-authored projection edit cells: canonical plain ATX
  content supports arbitrary non-newline splices without losing the heading
  shell, and the first one-shot inline dependency cell keeps unrelated inline
  projection while exposing only an invalidated Strong closure exactly.
  ABI 4.29's `PROJECTION_EDIT_CELLS_V2` extends that record with
  punctuation-free plain literal segments that retain unrelated inline
  projection during chainable ASCII word/interior-space typing and replacement,
  one parser-proved Backspace, and safe terminal word/space/prose-punctuation appends
  after punctuation without exposing earlier Markdown, including the mounted
  product-tour dogfood paragraph.
- Added ABI 4.30's `LITERAL_SAFE_ENVELOPES_V2`: a parser-authored, one-shot
  proof for a single `*` insertion inside a conservatively isolated flat Strong
  span. The Strong delimiters stay hidden, its style remains rendered, and the
  proof is consumed before any successor edit.
- Added ABI 4.31's `STRUCTURAL_PRESENTATION_PROOFS_V1`: Ready parser results
  may certify bounded terminal paragraph splits and paragraph merges whose
  inline partition is unchanged. Rapid Return successors and Backspace merges
  therefore keep the rendered Strong run, exact source, and caret identity;
  every unsupported structural transition still fails closed.
- Extended the same typed receipt proof in the final ABI 4.32 contract to
  parser-certified simple list indent/outdent actions. Their exact prefix
  splice retains the current list shell and mapped inline runs on every paint
  without granting ordinary input any structural authority.
- Added ABI 4.32's `GLOBAL_LIVE_STATE_INSPECTION_V1`: after close consumes the
  final session handle, qualification can still prove that the process owns no
  native sessions, transactions, continuations, anchors, or history tokens.
- Added ABI 4.32's `PROJECTION_EDIT_CELLS_V3`: Rust may parameterize a generic
  one-shot exact-scalar edit predicate on the existing projection-cell record.
  The first parser-owned dependency component keeps `[` insertion local to an
  isolated Strong span, retaining the paragraph shell and outside projection
  without teaching Dart bracket grammar. The same parser seam now supplies a
  guarded fact-free prose component for a strictly interior multiword ASCII
  paste, preserving the Product Tour's earlier Strong run through paste and
  history replay. The parameterized scalar path now also covers the frozen D0
  punctuation set at parser-guarded prose points, exacting only the fact-free
  prefix while retaining its outside Strong fact. The same one-shot seam now
  covers the frozen different-marker syntax constructions (`*`, `_`, `~`,
  backtick, `[` and `]`) when the current source contains no matching marker,
  keeping the certified Strong or Emphasis sibling projected.
- Added ABI 4.33's `PROJECTION_EDIT_CELLS_V4` after the D0 actual-paint block
  construction matrix found that the final space in `# `, `> `, `- `, and
  `1. ` still painted the predecessor Plain shell. The parser now publishes
  one bounded exact splice or finite parser-declared rapid prefix plan plus its
  typed clean-result block shell; Core compares only that declared sequence,
  Flutter materializes it through the existing
  pending-presentation snapshot, and fresh prefix-inclusive certification
  retires it. Construction and structural Backspace removal keep exact source,
  caret identity, outside Strong styling, and the clean heading/quote/list
  result on every observed paint at human cadence and in true unpumped bursts
  without adding host Markdown rules.
- Added the final D0 ABI 4.34 `BOUNDED_PENDING_PRESENTATION_PLANS_V1`
  contract. The parser now owns one bounded insertion sequence plus the full
  clean multi-row presentation for every admitted prefix. Core validates and
  materializes that data through the existing pending-presentation lifecycle,
  allowing the frozen opening/closing fenced-code journey to remain identical
  to a clean parse on every paint without adding fence recognition or another
  authority slot in Flutter.
- Kept closed fenced-code typing rendered on every paint by publishing an
  authoritative empty inline-fact set plus bounded physical-line ASCII-word
  cells; the code shell remains projected while only the changed authored line
  becomes exact, and neither fence is part of the admitted edit range.
- Replaced the active release, archive, platform, and documentation entry points
  with v4-only equivalents. Superseded release notes are preserved at
  `legacy/docs/v2_v3/CHANGELOG.md`.
