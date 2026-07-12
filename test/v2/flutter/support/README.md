# Flark v2 test harnesses & tiers

Editing bugs in Flark live at different layers, and each layer needs a
different kind of test. Pick the **lowest tier that can observe the bug** — it
is the fastest and the most precise — and only reach for a higher tier when
the bug is invisible below it.

## The tiers

### 1. Module units (headless, no controller)
Pure algorithm tests over `lib/src/v2/markdown/inline/` and friends
(`flark_inline_delimiter_placement_test.dart`,
`flark_inline_run_scanner_test.dart`). Use for the delimiter/flanking/placement
logic in isolation: given a source string and an edit, assert the produced
source. No parse, no widgets.

### 2. Source-model sequences & specs (headless, real controller + parse)
- **`InlineSequence`** (`inline_sequence_harness.dart`) — drives a real
  `FlarkFlutterController` through user-level edits (`type`, `backspace`,
  `toggle`, `moveCaret`, `paste`, …). After every step it gates on:
  1. **display fidelity** — `projectText` equals what the user typed, and
  2. **export round-trip** — a fresh caret-free parse of `controller.markdown`
     projects the same display (the source never depends on editor state).
  This is the tier for the **inline-validity** class: "does an
  editor-generated edit keep the source valid CommonMark". It found the
  original `**foo **` family and its siblings.
- **Render specs** (`render_spec.dart`,
  `flark_markdown_render_spec_test.dart`) — one-string `expectRendered(source,
  annotated)` / `expectTypedRendered` for parse/render *truth*
  (`'hello *world*'` → `'hello <em>world</em>'`).

These tiers are **blind to layout**: they assert the source and the projected
display *text*. A document whose source and display text are both correct but
which the widget lays out into the wrong number of rows passes here. That is
what tier 3 is for.

### 3. Rendered sequences (widget-pumped)
**`LiveRenderSequence`** (`live_render_sequence.dart`) — pumps the real
`FlarkLiveRenderedEditableText`, drives real key/text input, and after every
step snapshots the **rendered structure**: the ordered editable `rows`, which
block each is in (`expectRowInBlock`), and the focused row. It also re-runs the
tier-2 round-trip gate, so it is a strict superset for the flows it covers.

Use for the **rendering/layout** class: row counts, cursor placement, block
boundaries — bugs a headless gate cannot see. The exemplar is the
blockquote/list *exit* (`> q\n\n`): valid source, correct display text, but a
phantom empty row. See `flark_live_block_exit_sequence_test.dart`.

`pumpLiveEditor(...)` is the shared pump for any widget-level editor test
(real parse + `DefaultTextEditingShortcuts` so hardware keys route the
production way). Prefer it over per-file copies.

## Where the other widget suites sit

Tier 3 has more than one lens, because "the rendering is wrong" is several bug
classes. `LiveRenderSequence` owns the **row/focus structure** lens (how many
editable rows, which block owns each, where the cursor is). The rest each own a
different rendering property; reach for the one that names your bug:

| Suite | Property it guards | Reach for it when |
| --- | --- | --- |
| `flark_live_block_exit_sequence_test` (`LiveRenderSequence`) | row count, block membership, focused row | an edit produces the wrong number of rows, or focus/caret lands in the wrong block |
| `flark_live_rendered_transition_matrix_test` | typing continuity *at the moment* a typed construct activates into a rendered block | typing a marker (`- `, `> `, `` ``` ``) drops or reorders characters as the block flips from raw to rendered |
| `flark_live_rendered_visual_layout_test` | geometric layout invariants (blank rows stay visible, blocks don't overlap) | rows are structurally correct but positioned wrong — overlap, collapsed blanks, gaps |
| `flark_render_plan_parity_test` | editor and preview surfaces derive identical render state from one controller | the same document renders differently in the editable vs the read-only preview |
| `flark_live_rendered_a11y_test` | semantics tree (tap targets, checkbox/role exposure) | a control is visually right but wrong for screen readers / tap-target size |
| `flark_v2_visual_golden_test` | pixels (paint, spacing, wrapping), 0.5%-tolerant | paint/spacing/wrapping regressions no code assertion captures — the last resort, semantics belong in the tiers above |

Rule of thumb still holds: assert a bug at the **most specific** lens that
names it. A golden that "would have caught" a row-count bug is the wrong home —
it will also break for an unrelated font change and tell you nothing about
why.

## Authoring convention

- **Semantic claims are immutable.** If a test asserts a *correctness*
  property (valid source, styled-vs-literal, one row per Enter) and reality
  disagrees, that is a defect to file — mark the test `skip:` with a one-line
  reason and report it. Never weaken the expectation to match a bug.
- **Rendering facts are pinned from reality.** Row structure and block layout
  are observed, not predicted: write a best guess, run, and paste the actual —
  *unless* the actual is itself implausible, in which case it is a finding.
  On failure the harnesses print the actual, so this is one copy-paste.
- **Every step is gated.** Don't assert only the final state; the harnesses
  run their invariants after each op so a mid-sequence regression is caught
  where it happens, with the op that caused it.
- **Prove the gate has teeth.** When a harness test guards a fix, confirm it
  fails with the fix reverted before trusting it.
