# Sovereign v2 Render Plan

Status date: 2026-05-02

## Scope

The first render-plan slice defines platform-neutral block and inline render
descriptors. Flutter spans, painters, overlays, and widgets should adapt from
this model rather than recalculate parser/projection details.

## Current Contract

- `SovereignRenderPlan` contains render blocks and metadata.
- Render blocks preserve markdown block kind and raw type.
- Render inline runs preserve markdown inline kind and raw type.
- Source ranges come from parser payloads.
- Display ranges are derived through `SovereignProjection`.
- Render-plan construction derives projection from parser hidden ranges by
  default and still accepts an explicit projection override.
- Inline runs attach to the deepest owning block; container blocks do not
  duplicate descendant inline runs.
- Table blocks expose typed column alignment descriptors.
- GFM task-list items expose typed checked-state descriptors.
- Fenced code blocks expose typed language descriptors.
- Link and image inline runs expose typed action descriptors with destination,
  title, and label/alt text.
- Blocks and inline runs expose renderer-neutral text style tokens instead of
  Flutter `TextStyle` values.
- Render plans expose overlay-oriented queries for all blocks, all inline runs,
  link/image runs, table/task/code blocks, and display-offset lookup.
- `SovereignRenderOverlayPlan` turns render descriptors into stable overlay
  targets for links, images, task list items, tables, and code blocks.
- Unknown parser node types remain available as unknown render node types with
  raw type strings preserved.

## Why This Matters

The v1 editor and read-only preview now share much of the same pipeline, but
the rendering model is still Flutter-shaped. v2 needs one typed plan that can
feed editable and read-only Flutter adapters and can be tested without widgets.

## Next Render Work

- Broaden semantic theme tokens where consumer styling needs more control than
  the current renderer-neutral defaults.
- Keep new preview and live-editing widgets consuming the render plan rather
  than reparsing Markdown locally.
- Continue richer image/media and reference-link UI from the existing action
  descriptors rather than adding widget-local Markdown parsing.
