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
