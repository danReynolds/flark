# Sovereign v2 Projection Core

Status date: 2026-05-02

## Scope

The first projection slice defines headless models for hidden source ranges,
cursor masks, and source/display offset mapping.

## Current Contract

- `SovereignHiddenRange` identifies source ranges hidden from display.
- Parser-provided hidden ranges can build a `SovereignProjection` directly.
- Hidden ranges are sorted and must not overlap.
- `SovereignCursorMask` rejects cursor positions inside hidden ranges.
- Cursor normalization can snap upstream or downstream out of hidden ranges.
- `SovereignProjection.sourceToDisplayOffset` subtracts hidden source length
  before the given source offset.
- `SovereignProjection.displayToSourceOffset` maps display offsets back to
  source offsets after hidden ranges.
- `SovereignProjection.projectText` builds display text from canonical source
  text and hidden ranges.
- Ambiguity zones mark delimiter/link/table/raw-HTML spans where predictive
  projection should use a declared source affinity.
- `SovereignProjection.predictAfter` rebases hidden ranges and ambiguity zones
  through source transactions and flags edits that touched projection-sensitive
  ranges.
- `SovereignProjection.reconcileWith` compares predicted and authoritative
  projections for hidden-range, ambiguity-zone, and display-length changes.
- Selection helpers map source selections into display-space selections and
  display selections back into source offsets for cursor/selection overlays.
- Escaped delimiters, reference links, table syntax, image/media labels, and
  raw HTML tags are covered by parser-derived projection fixtures.

## Why This Matters

Projection is the core difficulty in a source-faithful live markdown editor.
The v1 implementation spread hidden ranges, cursor masks, and selection guards
across controller and syntax helpers. v2 makes these contracts headless and
testable before Flutter integration exists.

## Next Projection Work

- Broaden typed projection reasons from parser markers.
- Keep live block-widget source/display ranges covered as new block types
  become editable.
