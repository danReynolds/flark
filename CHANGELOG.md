# Changelog

## 0.1.0-dev.1

- Cut over to the v4 package split: `flark_core` owns the headless Dart/Rust
  runtime and `flark` owns the Flutter product surface.
- Added bounded, source-authoritative opening, editing, certification, semantic
  viewport, history, and lifecycle APIs through ABI 4.32.
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
- Replaced the active release, archive, platform, and documentation entry points
  with v4-only equivalents. Superseded release notes are preserved at
  `legacy/docs/v2_v3/CHANGELOG.md`.
