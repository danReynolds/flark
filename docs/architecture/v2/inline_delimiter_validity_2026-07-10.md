# Inline delimiter validity (2026-07-10)

## Invariant

**Editor-generated edits never commit inline delimiter placements that
CommonMark refuses. Editing intent that valid markdown cannot express lives in
controller state, never in the source.**

Concretely: an emphasis/strong/strikethrough delimiter never sits against
whitespace on its content side. `**hello world **` is unrepresentable through
the editing surfaces; the canonical form of "the user typed `hello world `
with bold on" is `**hello world** ` **plus** the strong style kept armed
(`pendingInlineStyles`) so the next styled keystroke re-enters the run.

This replaces the previous model, where the source could hold invalid
transients while the caret was inside the run and a caret-local render pass
(`FlarkStickyInlineRun`, now deleted) re-hid the markers. That model leaked at
every boundary the compensation could not reach: `markdown`/`markdownChanges`
consumers mid-typing (autosave persisted invalid CommonMark), the standalone
preview (no caret context), caret-out states (the hold went stale — selection
changes trigger no re-parse), the muted-exit paths, deletions, Enter, and
selection wraps. It also lied outright in edge cases (holding markers inside
an indented code block). The bug class was structural: four write sites
composed `open + text + close` with no flanking validation, and one narrow
read-side patch covered a single member of the family.

## Where placement happens

All delimiter placement flows through one headless module,
[`FlarkInlineDelimiterPlacement`](../../../lib/src/v2/markdown/inline/flark_inline_delimiter_placement.dart):

- `armedWrap` — pending-style wraps: the wrap hugs the typed text's core;
  edge whitespace stays outside; whitespace-only text commits unwrapped and
  stays armed; typing in a re-entry gap (`**hello** |`) extends the run
  instead of opening a sibling.
- `contentEditRepair` — one rule for insertions, deletions, and replacements
  inside a recognized run's content whose plain application would strand a
  delimiter against whitespace: the delimiter relocates to hug the surviving
  core; blank content dissolves the run (cascading outward through emptied
  enclosing runs); edge whitespace bubbles out through *flush* nested
  delimiters and parks as legal interior content at the first non-flush
  level.
- `markerCrossingRepair` — selection edits whose source range covers exactly
  one half of a hidden marker pair keep the pair balanced instead of
  orphaning the survivor. A covered close relocates to the edit boundary
  with the typed text joining the run (style inherited from the selection
  start, the Docs/Word convention); a covered open relocates past the typed
  text, which stays outside; covering one run's close and a same-marker
  neighbor's open merges the two into one run absorbing the text;
  different-marker both-covered edits rebalance both pairs, switching the
  second run to its alternate delimiter character (`*` ↔ `_`) when the
  rebalanced clusters would fuse. Code spans participate here (and only
  here — an orphaned backtick swallows the rest of the document), with no
  whitespace splitting since code whitespace is content. Every produced
  edit is verified against the flanking rules on the resulting text;
  unverifiable shapes (stacked multi-run crossings, dissolves inside an
  enclosing run, tilde fusions with no alternate character) fall through to
  the plain edit.
- `joiningDeletionRepair` — a deletion that consumes the entire gap between
  two runs merges same-marker neighbors (`**a** **b**` minus the space →
  `**ab**`, never the fused literal `**a****b**`; stacked neighbors merge
  cluster-chain-wise, `***a*** ***b***` → `***ab***`). Same-character
  different-marker neighbors would also fuse (`**a***b*` leaves `*b*`
  literal), so one side rewrites to its alternate delimiter character
  (`**a**_b_`); different-character neighbors (`**a**~~b~~`) are valid
  adjacency and the plain deletion stands.
- `runSplit` — muted-exit middle splits move whitespace straddling the split
  point between the closing and reopening delimiters.

Call sites: the projected-edit adapter (armed wraps, plain insertions,
deletions, selection replacements), the controller (muted exits, the
type-a-delimiter-over-selection recognizer), the inline toggle command
(selection wraps hug the core; whitespace-only selections reject), the input
engine (Enter at a run edge lands outside the delimiters), and the keyboard
deletion path (below).

## Two deletion paths, one repair set

A deletion reaches the source two ways, and both canonicalize through the
same repairs:

- **Display-space edits** — the block editables and IME/paste deliver a
  changed display string; `FlarkProjectedTextEditAdapter.resolveDisplayEdit`
  maps it to a source range and runs the repair pipeline.
- **Source-space keyboard edits** — on the whole-document host surface,
  Backspace and forward Delete are intercepted by `FlarkMarkdownInputPolicy`
  and resolved to a source range by
  `FlarkProjection.resolveBackspaceSelection` /
  `resolveForwardDeleteSelection` (which step the caret past hidden marker
  chains). That resolved range would otherwise be plain-deleted by the
  engine — stranding edge whitespace (`**foo x**` backspacing `x` →
  `**foo **`) or fusing adjacent runs (`**a** **b**` minus the gap →
  `**a****b**`). The policy now routes it through
  `FlarkFlutterController.applyResolvedInlineDeletion`, which applies the
  same `contentEditRepair` → `markerCrossingRepair` → `joiningDeletionRepair`
  pipeline (with authored-marker hiding) before falling back to the plain
  delete. A collapsed resolver step that lands the caret before an opening
  marker (`**a** **|b**` Backspace) is canonicalized as the single grapheme
  before the step, unless that grapheme is a line break (a line merge stays
  with the engine's block-aware Backspace). So the two keys stay symmetric
  and both keep the source valid.

## Authored markers hide predictively

Typing-path placements (`armedWrap`, `contentEditRepair`, `runSplit`, and the
muted-exit code split) additionally report the delimiter ranges they author
(`FlarkInlinePlacementEdit.authoredMarkers`, post-edit coordinates with
open/close roles). The controller folds these into the *predicted* projection
inside the adoption chokepoint — before any listener fires — so just-written
markers are hidden on the very frame of the keystroke. Two effects: no raw
delimiter ever flashes while the immediate parse is in flight, and the
platform editable's text never changes out from under an active IME
composition (previously the first marker-creating keystroke cleared the
composing region and cancelled composition on real keyboards; pinned by the
strict predictive test in `flark_ime_input_test.dart`). This is
provenance-pure — only ranges the machinery itself wrote this instant are
hidden — and the authoritative parse re-derives the identical ranges a beat
later. Selection-path repairs (`markerCrossingRepair`,
`joiningDeletionRepair`) currently report none: composition is never active
across a selection edit, so their one-frame transient is acceptable; extend
them the same way if that ever changes.

## Two notions of "a run", used asymmetrically

- **Relocation decisions** (moving existing delimiters) resolve runs from the
  parser's own pairing: `FlarkProjection.inlineRunScans` pairs the
  projection's `opensInlineRun`/`closesInlineRun` hidden ranges. This is
  comrak's truth, so hand-typed literal text is never rewritten — if the
  parser didn't recognize a run, nothing touches it. The textual scanner
  approximates CommonMark's delimiter algorithm and *does* diverge on
  adversarial shapes (comrak discards unmatched delimiters between a matched
  pair); the randomized sequence gate found exactly such a divergence, which
  is why relocation never trusts it.
- **Detection-only decisions** (toolbar active state, arming/muting routing,
  Enter's insertion-point shift) may use the flanking-aware textual scanner
  ([`FlarkInlineRunScanner`](../../../lib/src/v2/markdown/inline/flark_inline_run_scanner.dart) /
  [`FlarkInlineFlanking`](../../../lib/src/v2/markdown/inline/flark_inline_flanking.dart)),
  which never edits anything — a false positive can only mislabel a toolbar
  chip or place a newline suboptimally, never corrupt text.
- **Muted code-span exits** are the deliberate crossover: code spans are
  absent from `inlineRunScans` (their backticks must never be
  whitespace-relocated), so the exit finds its run through the textual
  backtick scan instead — safe because a code exit only writes *around* the
  markers (and its middle split is the plain `` `x` `` insertion, never the
  whitespace-moving `runSplit`).

Consequences worth remembering:

- A muted exit only engages when the projection carries parse-derived runs.
  Editing surfaces parse on attach; headless drivers should `parseNow()`
  first.
- A muted **middle split** additionally requires the muted run to be the
  innermost run at the caret — splitting an outer run through an inner one
  would create overlapping delimiters (found by the randomized gate).

## Exemptions and known edges

- **Code spans** are exempt from whitespace relocation: `` `x ` `` is valid
  CommonMark and relocating the space would change the code text. They are
  visible only to the marker-crossing balance repair (via
  `inlineRunScans(includeCodeSpans: true)`), which never splits whitespace —
  it only keeps the backtick pair together.
- **Block reinterpretation is not a leak.** Leading whitespace bubbling to
  line start can accumulate into an indented code block, which then renders
  raw — the same thing that happens typing four spaces before plain text.
  The sequence gates accommodate exactly this case and nothing else.
- **Armed wraps with mixed edge whitespace at an existing run's edge**
  (paste-shaped text like `" x"` typed while armed, with the caret exactly on
  another run's delimiter) route the fresh wrap's edge whitespace correctly
  relative to the new wrap, but do not bubble it through the *pre-existing*
  run's delimiter. Whitespace-only armed typing does (it routes through
  `contentEditRepair`). If a real-world report hits the mixed case, extend
  `armedWrap` the same way.
- **Marker-balance repairs bail conservatively.** `markerCrossingRepair` and
  `joiningDeletionRepair` (see above) fixed the former adjacent-run-join and
  marker-crossing leaks, but they verify every produced edit against the
  flanking rules and return null — today's plain-edit behavior, which can
  still leak literals — for the shapes they cannot guarantee: a crossing
  that covers more than one run's cluster on a side (stacked `***` edges),
  a dissolve or edge-whitespace park inside an enclosing run, replacement
  text that itself contains delimiter characters, tilde neighbors of
  different cluster lengths (`~a~ ~~b~~` has no alternate character), an
  alternate-character rewrite whose content already contains the alternate
  character, and a merge whose content seam would abut delimiter characters
  (`**x~** **~y**`). If a real-world report hits one, extend the repair the
  same generate-and-verify way.
- The scanner's flanking predicates approximate the spec's Unicode classes
  (common `Zs` code points; ASCII punctuation). The authoritative oracle is
  the comrak parse, which is what the tests assert against.

## The gates

[`flark_inline_style_sequence_test.dart`](../../../packages/flark_flutter/test/v2/flutter/flark_inline_style_sequence_test.dart)
drives a real controller + real comrak parse through user-level sequences and
asserts after **every step**:

1. **Display fidelity** — the projected text equals exactly what the user
   typed (block reinterpretation excepted, see above). Editor-authored
   delimiters leaking into the display fail here.
2. **Export round-trip** — a fresh, caret-free controller parsing
   `controller.markdown` renders the identical display. Any state whose
   styling depends on editor-local compensation fails here. This is the gate
   the sticky-run era could not pass.
3. Selection bounds sanity.

The suite covers the reported repro and its family (leading/trailing/
whitespace-only, all styles, stacked and nested runs, muted exits and splits,
deletions at edges, Enter, selection wraps, hand-typed literals, undo/redo,
continuation re-entry) plus seeded randomized sequences with a journaled
failure reporter — the journal reproduces any failure as an explicit op list.
Unit coverage for the placement/scanner modules lives in
`test/v2/markdown/flark_inline_delimiter_placement_test.dart` and
`flark_inline_run_scanner_test.dart`.

On top of the harness sits a declarative one-string layer
([`render_spec.dart`](../../../packages/flark_flutter/test/v2/flutter/support/render_spec.dart),
specs in
[`flark_markdown_render_spec_test.dart`](../../../packages/flark_flutter/test/v2/flutter/flark_markdown_render_spec_test.dart)):
`expectRendered(source, rendered)` asserts a parsed document's display as one
annotated string (`'hello *world*'` → `'hello <em>world</em>'`; literal
shapes appear literally), and `expectTypedRendered` types the same source
keystroke-by-keystroke through a live editor — round-trip gated per key —
then proves the typed document equals the loaded one, source and render both.
On failure the actual annotation is printed, so authoring a spec is "run
once, paste the verified actual".

During this work the gates caught, beyond the reported bug: five sibling
write-path leaks, a projection mapping defect (upstream display→source at a
stacked hidden-marker chain resolved between two markers, so a backspace in
`*~~ff~~*` deleted half the `~~` pair), the outer-run middle-split overlap,
the whitespace-only sibling-wrap leak (`__ __`), and the sticky pass hiding
markers inside an indented code block.
