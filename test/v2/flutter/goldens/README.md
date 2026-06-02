# Flark v2 Visual Goldens

These PNG baselines cover visual regressions that ordinary widget assertions do
not catch well: paint, spacing, wrapping, block affordances, and overlay
composition. Parser semantics, projection mapping, command behavior, and
render-plan metadata should stay in code assertions.

## Scenario Inventory

- `sovereign_v2_surfaces.png`: broad source/projected/preview smoke coverage.
- `sovereign_v2_live_rendered_editing.png`: rendered-in-place editing with
  hidden markers, styled inline spans, editable task checkboxes, an editable
  language-labelled code block, quote chrome, and an editable table grid.
- `sovereign_v2_inline_wrapping.png`: narrow inline wrapping with bold, italic,
  strikethrough, inline code, links, and escaped markers.
- `sovereign_v2_code_fences.png`: source/projected/preview code-fence regions
  with and without language metadata.
- `sovereign_v2_blockquotes.png`: quote rails, continued quote lines, nested
  quote marker projection, and inline styling inside quotes.
- `sovereign_v2_tasks_tables_overlays.png`: task checkboxes, table grid
  rendering, link/image/action overlay chips, and task/table overlay labels.
- `sovereign_v2_compact_mixed.png`: compact preview layout with wrapping,
  overlays, quote rail, code block, and task rows in a narrow viewport.

## Update Command

```bash
flutter test --update-goldens test/v2/flutter/sovereign_v2_visual_golden_test.dart
```
