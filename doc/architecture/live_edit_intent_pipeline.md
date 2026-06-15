# Live-edit intent pipeline — recognizer matrix & convergence plan

**Status:** Stages 1–2 complete (classifiers extracted, standalone library,
table-tested). Stage 3 behavioral convergence is gated on a manual IME pass.
**Code:** `lib/src/v2/flutter/flark_live_edit_classifier.dart`
**Tests:** `test/v2/flutter/flark_live_edit_classifier_test.dart`

## Why this exists

Flutter text input is a round trip with the platform IME. When Flark
intercepts an edit (Enter on a fence opening line, a language shortcut, an
auto-closed fence) and rewrites the document itself, the platform may still
deliver its own version of the change afterwards — an *echo* that must be
recognized and swallowed or it corrupts the document.

Echo handling used to be open-coded as two long if-chains inside two widget
states. It is now two pure classifier functions, each resolving one incoming
`TextEditingValue` into a single typed intent:

- `classifyFlarkHostEdit` — the projected **host** surface (one field
  holding the whole projected document; also the live-rendered fallback
  while no authoritative plan exists).
- `classifyFlarkLiveBlockEdit` — the **block** surface (a field editing one
  block's display slice or direct source range).

The classifiers live in a standalone library that cannot import the editor
widgets, so their purity is compiler-enforced. Widgets resolve a context,
classify, and execute the intent's side effects.

### Why two functions, not one parameterized pipeline

The surfaces see fundamentally different text: the host sees the whole
document, a block sees a slice. Almost every recognizer is specific to one
granularity (see the matrix), and the safety-critical property of each chain
is its *ordering* — recognizer N is only correct because 1..N-1 did not
match. A merged, parameterized pipeline would interleave two orderings and
obscure both. Shared *steps* are shared functions; the chains stay separate
and explicit.

## Recognizer matrix

Order within each column is execution order. **I** = intentional asymmetry
(structural reason given). **C** = convergence candidate (needs the device
pass below before changing).

| # | Step | Host | Block | Verdict |
| --- | --- | --- | --- | --- |
| 1 | Whole-value auto-closed fence echo (raw value, caret at end) | ✓ | — | **I** — only the host's field ever contains the whole document, so only it can receive a whole-document echo. |
| 2 | Pure-insertion caret normalization (`flarkTextValueWithPureInsertionSelection`) | ✓ | ✓ | shared function |
| 3 | Whole-text auto-closed fence echo (inside normalization) | ✓ | ✓ | shared function |
| 4 | Code-body line-break echo normalization (`normalizeLineBreakInsertionValue`) | — | ✓ | **I** — requires a block's fence context; the host has no single block. |
| 5 | Standalone auto-closed fence echo against source markdown | ✓ | — | **I** — catches echoes recognizable only against the *source* markdown when the projected display text diverges from it (#3 compares against the display text, and #1 requires the caret already at the end of the raw value; this probe runs after #2 has repaired a stale caret). Host-only for the same reason as #1. |
| 6 | Pending code-body echo (`consumePendingEcho`) | — | ✓ | **I** — pending echoes are recorded when a policy-handled structural edit routes through a block's code body; the host never records one. |
| 7 | Fence opening-line platform Enter (caret move / Enter dispatch) | — | ✓ | **I** — the opening line is only separately editable as a block slice. |
| 8 | Opening-empty-unclosed newline echo | — | ✓ | **I** — as #7. |
| 9 | Language shortcut echo + promotion (`languageShortcutEdit`) | — | ✓ | **I** — typing a language into a bare fence's first body line is a block-slice interaction. |
| 10 | Trailing line-break platform echo | — | ✓ | **I** — block-slice bookkeeping for code bodies. |
| 11 | Platform line-break normalization + source-equality resync | — | ✓ | **I** — as #10; also the source of the block's `resyncWhenHandled` flag. |
| 12 | Typed-closing-fence policy bypass (`shouldHandleTypedClosingFence`) | — | ✓ | **C** — the host offers every change to the input policy unconditionally. Believed moot on the host (the policy only consumes Enter/Backspace-shaped diffs, and a host surface showing an unclosed fence is a brief pre-parse state), but porting or rejecting the gate needs device confirmation. |
| 13 | Markdown input-policy offer with explicit fallback | ✓ | ✓ | shared shape (`…PlatformTextChangeIntent` with nested fallback) |
| 14 | Completed standalone fence opener | ✓ display-text form | ✓ local-value form | **I** — same recognition, different output shape because the surfaces apply edits differently (whole-document projected edit vs block value adoption). |
| 15 | Immediate-parse heuristic for newly renderable lines | ✓ | — | **C** — the host requests an immediate parse when an edit creates a renderable block line (`- `, `> `, fence). The block's projected-edit fallback only does so after a completed fence opener. Converging would make new markers render one debounce-window sooner when typed inside a block, at the cost of more parses; measure churn on devices. |
| 16 | Selection mapping | projected only | source-range and projected | **I** — only blocks can own a direct source-edit range. |

Every intentional asymmetry above that is cheaply expressible as a pure
classification is pinned by a test in
`flark_live_edit_classifier_test.dart` (see the "asymmetry pins" group), so
converging one later forces a deliberate test change.

## Inline-run caret affinity (trailing-edge model)

A styled inline run (`code`, **strong**, *emphasis*, ~~strikethrough~~) hides
its markers, so two source caret positions render at the same display
position at the run's trailing edge: *inside* the run (before the hidden
closing marker) and *outside* it (after the marker). The pipeline treats
that pair as distinct caret states and keeps the user in control of which
side typing lands on:

1. **Placement maps inside.** A collapsed display selection at the trailing
   edge maps to the inside position
   (`FlarkProjection.displayCaretToSource`, used by
   `FlarkFlutterController.applyProjectedSelection` when no explicit
   affinity is given). Tapping at the end of a run and typing continues the
   run's style. Closing markers are identified at parse adoption
   (`FlarkHiddenRange.closesInlineRun`, derived from styled inline tokens)
   and survive predictive mapping.
2. **Closing a run keeps you inside it.** When the typed character itself
   completes a run's closing marker (the second backtick of `` `this` ``),
   the controller snaps the caret inside the run at parse adoption
   (`selection.inlineRunMarkerCompletion`), so typing continues in the
   style that just appeared — the Slack/Notion conversion behavior. To make
   the snap land before the next keystroke, both surface classifiers
   request an immediate parse for pure insertions of inline marker
   characters (`` ` `` `*` `_` `~`), extending the matrix-row-15 heuristic
   to inline runs.
3. **Plain horizontal arrows step through both states.** Right-arrow at the
   inside-end exits past the closing marker without visible caret movement;
   left-arrow from the outside re-enters
   (`FlarkProjection.inlineRunBoundaryStep`, intercepted by both editing
   surfaces before `EditableText` moves the caret).
4. **Typing the marker character exits.** A platform insertion of the run's
   own marker character (or a prefix of a multi-character marker, e.g. one
   `*` against a closing `**`) at the inside-end is classified as the exit
   gesture: the edit adapter emits a selection-only transaction past the
   marker instead of inserting a literal marker character.
5. **Ambiguous diffs anchor at the caret.** A platform edit arrives as an
   old → new display-text pair; typing a character identical to the one
   after the caret (a space before an existing space) lets the
   prefix-greedy diff slide the edit window forward, past the hidden
   closing marker. When the same change is expressible exactly at the old
   caret, the edit adapter prefers that interpretation
   (`_DisplayTextDiff.between(anchor: …)`), so writing multi-word styled
   text (`code with spaces`) never escapes the run on a space or
   backspace.
6. **Trailing whitespace keeps its highlight.** Flutter does not paint
   `TextStyle.backgroundColor` over line-trailing whitespace, so a code run
   that currently ends in a space (mid-typing `` `multi word ` ``) would
   lose its highlight on the last character. Both surfaces paint
   inline-code run backgrounds in a chrome underlay from layout selection
   boxes (`flarkPaintInlineCodeRunBackground`), which do include trailing
   whitespace.
7. **Enter steps out before splitting.** A line break with the caret at a
   run's inside-end would split the run's source and orphan its markers as
   literal text (a code span cannot contain a blank line). Inline runs are
   line-scoped, so the input policy steps the caret past the closing marker
   before dispatching the paragraph split
   (`selection.inlineRunLineBreakExit`).
8. **Range selections are content-symmetric, and deletions are marker-aware
   through one resolver.** A display range selection maps to exactly the
   visible content (start downstream past hidden markers, end upstream before
   them). Typing over a fully selected run replaces only the content, so the
   replacement stays styled (`` `x` ``), matching rich-text type-over.

   Backspace is the one source-space path that holds a *caret* next to hidden
   markers (the physical key dispatches against the controller's source
   selection, which deliberately preserves the inside/outside-the-close
   distinction — see #9 — so a display-space diff never gets a chance to make
   the deletion marker-aware for it). It routes through a single
   boundary-aware resolver, `FlarkProjection.resolveBackspaceSelection`
   (`dispatchBackspace` → `selection.inlineRunDeletion`), which adjusts the
   effective selection so the deletion never splits or orphans a marker:
   - **Whole-content deletion** (select-all + delete over `` `test` ``, or
     backspacing a run's last content character) expands over the now-orphaned
     marker pair so the backticks go too — built on the shared
     `expandDeletionOverInlineRunMarkers` primitive.
   - **A caret just past the close** (outside the run) re-enters it so the last
     *content* character is removed, not a marker char (`**bold**|` → `**bol**`
     via `inlineRunBoundaryStep(forward: false)`); without this a naive delete
     cut a single `*`, leaving the unbalanced `**bold*`.
   - **A caret just past the open** (the run's interior start) steps before the
     whole marker so a marker char is never split; the delete then targets the
     character before the run, or merges lines at its start.
   The same `expandDeletionOverInlineRunMarkers` primitive is applied directly
   by the projected-edit adapter, whose deletions arrive in display space and
   already target the visible character — so they need only the orphan rule,
   not the re-entry steps. **Gap:** forward `Delete` is not yet routed through
   the resolver (it falls back to Flutter's display-space default, which is
   marker-safe for the common case but can split a marker when the caret sits
   just before a hidden opening marker); a symmetric `forward: true` pass is a
   clean follow-up on the same primitive.
9. **Edit anchors never round-trip the caret through display space.** The
   input-policy/shortcut selection appliers on both surfaces skip the
   re-application when the controller's source selection already renders at
   the requested display position, because a display round trip cannot
   represent the inside/outside distinction.
10. **A trailing space inside an emphasis/strong/strikethrough run stays
   rendered.** CommonMark forbids a closing delimiter preceded by whitespace,
   so the parse of `**foo **` carries no styled run and the markers would flash
   into view for the moment a space sits at the run's end (mid-typing
   `**foo bar**`). At parse adoption a pure, stateless reconciler
   (`FlarkStickyInlineRun.reconcile`, invoked from
   `FlarkFlutterController.applyParseResult`) re-hides the marker pair and
   re-styles the content when the collapsed caret sits inside such a run —
   mirroring a real run's `opensInlineRun`/`closesInlineRun` hidden ranges and
   inline-run style token. It holds no state and is recomputed each adoption,
   so it releases the instant the caret leaves the run or the run becomes valid
   markdown again; code spans never trigger it (their trailing space is already
   valid) and ambiguous `***`/nested runs are skipped (kept at today's
   behavior). Phase 2 (not yet shipped) would normalize a dangling trailing
   space outside the markers (`**foo **` → `**foo** `) on caret exit so the
   resting source stays valid-and-bold.

Block markers (headings, quotes, lists) are never trailing edges; their
hidden markers are prefixes, so caret placement after them is unaffected.
The leading edge keeps the long-standing downstream default: typing at the
display start of a run inserts inside it (pinned by the adapter tests).

Related: a heading whose markers are fully hidden but whose content is empty
(`### `) renders as an *empty styled heading* block rather than falling back
to a raw synthetic source line, so committing the marker with a space styles
the line immediately instead of jumping when the first content character
arrives.

## Stage-3 device protocol (the remaining gate)

The convergence candidates (#12, #15) and any merge of the recognizer sets
change behavior under real IMEs, which simulated input cannot vouch for.
Before landing either:

1. macOS: type through each fence flow (open, language shortcut, body Enter,
   typed closer) with a CJK input source (e.g. Hiragana/Pinyin) active —
   composition must never be dropped mid-flow.
2. iOS: same flows with autocorrect and predictive text enabled; verify no
   doubled characters after auto-close echoes.
3. Android (Gboard): same flows; Gboard's aggressive composing regions are
   the historical source of the echo recognizers — watch for resync loops.
4. Web (desktop Safari + Chrome): the WASM-parser path plus browser IME.

Each platform pass should run the example app's Scratch document and the
`code fence` flows from `example/test/widget_test.dart` manually.
