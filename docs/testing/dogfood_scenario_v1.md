# D0 macOS dogfood scenario ledger v1

**Status:** frozen Phase 0 denominator

**Milestone:** [D0: Ready for Dan's macOS dogfood](../../DOGFOOD_MILESTONE.md)

**Baseline:** merge commit `9629b708440281f82c6ae40a539b98fbbb349671`

This ledger is the complete D0 product denominator. It is a coverage table, not
a scenario language. Tests remain ordinary parameterized Rust, Dart, Flutter,
and native tests.

A row may be added before D0 only for a B0, a reproducible B1 in an existing
journey, or an explicit scope decision from Dan. An implementation convenience,
hypothetical edge case, or reviewer preference does not expand this ledger.

## Candidate configuration

### Product shell

D0 covers the macOS editor-core workbench. It does not cover arbitrary-file
open/save, autosave, crash recovery, or edited-document persistence.

The default product-tour fixture is imported from
`packages/flark/example/lib/dogfood_documents.dart`. Tests may locate an edit
with a unique source substring and a relative UTF-16 offset; copied fixture text
does not count as product-tour evidence.

### Native feature set and ABI

- Starting ABI: `4.31`.
- Final frozen D0 ABI after Phase 1: `4.32`.
- Phase 1 is expected to consolidate host authority state. It may change the
  ABI only when the unified typed contract or global live-state inspector
  cannot be represented by 4.31.
- Phase 1 records and freezes the final D0 ABI minor. The draft 4.32 contract
  was reopened before any downstream Phase-2 receipt and refrozen at the same
  minor with capability `PROJECTION_EDIT_CELLS_V3`; any later ABI change
  invalidates Phase 1 and all downstream receipts.
- The D0 app uses the default Cargo feature set.
- `opening-session` remains an independently tested feature, not a D0 app
  capability.

### Enabled presets

The candidate app hash binds this exact menu state:

| Preset | D0 state | Exact scope |
|---|---|---|
| Product tour | enabled | complete interaction journey |
| Prose · 1 MiB | enabled | profile matrix plus local edit/undo/scroll/close |
| Prose · 5 MiB | enabled | local edit/undo/scroll/page/close |
| Prose · 10 MiB | enabled | typing, inline typing, scroll/page/close |
| Giant line · 5 MiB | enabled | local edit/navigation/close |
| Dense blocks · 1 MiB | enabled | local edit/scroll/page/close |
| Streamed · 10 MiB | visible but disabled | item says `Needs a library built with the opening-session cargo feature`; excluded from D0 performance and D1 |

The streamed item becoming selectable changes this ledger and makes the
opening-session functional and five-fresh-process profile lanes mandatory.

### Required exact-commit CI

The D0 receipt requires these job names with `head_sha` equal to the candidate:

- `v4-integration-gate`
- `macos-smoke`

`v4-opening-session-gate` remains required for changes touching the feature,
but it is not a D0 app gate while the streamed preset is disabled. iOS and
Android build smokes are PR/package evidence, not Mac dogfood product evidence.

### Watchdogs and native route

- One functional or mounted action: 30 seconds.
- Complete native canary: 180 seconds.
- One profile cell: 15 minutes.
- Final certification stress: 15 minutes.
- Complete cold dogfood-ready orchestrator: two hours.
- Native sustained edit: Product Tour, 1569 by 906 logical pixels, caret after
  `locally.`, exact string ` Testing is somewhat useful but lik`, 35 accepted
  edits, 80 milliseconds per edit.

The native artifact is the `flark_abi` binary embedded under the profile app's
`Contents/Frameworks/flark_abi.framework`. A standalone dylib is diagnostic
unless its bytes hash identically.

## Outcome vocabulary

Every mutation row names one required pending result:

- **P — projected:** the current rendered presentation is complete.
- **I(anchor-start, anchor-end) — local exact island:** the half-open current
  source range between the two named anchors is exact and unstyled. All facts
  outside it remain projected and styled.
- **S(name) — structural:** the named current-revision transitional surface is
  authoritative until fresh certification supersedes it.
- **A(name) — action:** the named current-revision action record is present and
  works. When the row says `A(none)`, hit geometry, semantics, and action are
  absent.

Any broader or different exact range is B1. A row shell, outside fact, action,
or caret may not be retained merely because its previous-revision geometry is
cached.

### Paint applicability

Every visually sensitive mutation has a per-edit-pump variant. Text-producing
rows also run at 80-millisecond cadence. The exact true-burst sequences are:

1. `keep what` at the Product Tour paragraph prefix;
2. `ke` inside the second word of `**bold text**`; and
3. Return followed immediately by `x` after `Before **bold**.`.

The identical sequence runs per-edit before its burst variant. A burst may
paint only its final generation.

Every editable collapsed-selection paint requires a non-null caret whose source
offset equals the canonical extent. Range-selection rows require selection
geometry for the canonical base/extent and no caret. Read-only rows require
neither unless explicitly stated. Semantics, hit geometry, and actions are
checked only on rows naming `A(...)`.

## Product-tour prose and inline editing

`PRESENT` means the current tree already contains the named evidence.
`PARTIAL` means lower/final-state evidence exists but the ledger's actual-paint
or native proof is missing. `GAP` means the behavior or proof must be added.

| ID | Exact seed and command | Required result | Cadence | Current evidence |
|---|---|---|---|---|
| PROSE-01 | Product Tour, caret immediately before `This`; insert `keep what` | P; Strong `Rust → Dart → Flutter` remains styled; source/caret current | per-edit, 80 ms, true burst | PRESENT: product-tour prefix matrix row plus unpumped replay |
| PROSE-02 | Product Tour, caret immediately after `locally.`; insert ` Testing is somewhat useful but like.` | P; earlier Strong remains styled | per-edit and 80 ms | PRESENT: `north_star_paint_matrix_test.dart`, terminal scenario |
| PROSE-03 | `Before **bold**\nplain terminal.¦`; insert ` Testing.` | P; distant Strong remains styled; EOF caret owns final row | per-edit and 80 ms | PRESENT: `north_star_paint_matrix_test.dart`, EOF scenario |
| PROSE-04 | `Alpha¦Beta and **bold**.\n`; independently insert `.`, `,`, `;`, `:`, `!`, `?`, `'`, `"`, `(`, `)`, `–`, `—`; separately insert `-` in `Alpha¦Beta` | P; Strong remains styled | per-edit and 80 ms | PRESENT: parser-parameterized exact-scalar differential plus parameterized actual-paint family |
| PROSE-05 | Product Tour first paragraph, caret after `is`; Backspace one UTF-16 unit | P; current visible source/caret; Strong remains styled | per-edit and 80 ms | PRESENT: dogfood Backspace matrix row |
| PROSE-06 | Same anchor before `is`; forward Delete one UTF-16 unit | P; current visible source/caret; Strong remains styled | per-edit and 80 ms | PRESENT: forward-Delete paint matrix row |
| PROSE-07 | Product Tour first paragraph, select `is`; replace rune-by-rune with `was` | P; current range/caret identity; Strong remains styled | per-edit and 80 ms | PRESENT: dogfood selection-replacement row |
| PROSE-08 | Product Tour first paragraph, select `temporarily pending`; paste `briefly pending` | P; current range/caret; Strong remains styled | one paste | PRESENT: one-delta actual-paint paste row |
| PROSE-09 | PROSE-08 final state; undo, redo | P after each action; exact source/selection lineage | one action each | PRESENT: actual-paint history replay row |
| INLINE-01 | `Before **bo¦ld** after.\n`; insert `ke` | P; inserted text is Strong | per-edit and 80 ms | PRESENT: Strong word scenario |
| INLINE-02 | `Before **bold te¦xt** after.\n`; insert `ke` | P; complete `bold tekext` source maps to one Strong fact without delimiters | per-edit, 80 ms, true burst | PRESENT: second Strong leaf scenario |
| INLINE-03 | `Before _ri¦ght_ after.\n`; insert `ke` | P; inserted text is Emphasis | per-edit and 80 ms | PRESENT: resolved-style paint matrix row |
| INLINE-04 | `Before ~~ri¦ght~~ after.\n`; insert `ke` | P; inserted text is Strikethrough | per-edit and 80 ms | PRESENT: resolved-style paint matrix row |
| INLINE-05 | ``Before `ri¦ght` after.\n``; insert `ke` | P; inserted text retains inline-code style | per-edit and 80 ms | PRESENT: resolved-style paint matrix row |
| INLINE-06 | `Before [ri¦ght](https://example.com) after.\n`; insert `ke` | P; label projection/link fact remains current | per-edit and 80 ms | PRESENT: resolved-style paint matrix row |
| INLINE-07 | `**left** mi¦ddle _right_\n`; insert `ke` | P; Strong and Emphasis remain styled | per-edit and 80 ms | PRESENT: independent-facts scenario |
| INLINE-08 | `# **¦left** middle _right_`; insert one space | I(start of opening `**`, end of closing `**` after edit); Heading shell and outside Emphasis remain rendered | per-edit and 80 ms | PRESENT: `inline_dependency_island_paint_acceptance_test.dart` |
| INLINE-09 | `Before **bo¦ld** and _right_.\n`; insert `*` | P; inserted literal asterisk is Strong content; outside Emphasis remains styled | per-edit | PRESENT: safe Strong asterisk row |
| INLINE-10 | `Before **bo¦ld** after.\n`; insert `[` | I(start of opening `**`, end of closing `**` after edit); `Before ` and ` after.` remain projected; paragraph shell retained | per-edit | PRESENT: parser-parameterized Strong dependency cell plus actual-paint acceptance |

`INLINE-10` uses the generic parser-owned exact-scalar cell. Its same detector
observes exactly the declared Strong-source island and retained outside runs;
existing-bracket and non-exhaustive parser shapes remain fail-closed negatives.

## Syntax construction

Every inline syntax row uses a different-marker styled sibling so the expected
outside fact is parser-provably independent for the frozen case.

| ID | Exact seed and command | Required result | Current evidence |
|---|---|---|---|
| SYNTAX-01 | `ab¦cd _right_\n`; insert `*` | I(start of `ab`, end of `cd` after edit); outside Emphasis stays styled | GAP |
| SYNTAX-02 | `ab¦cd **right**\n`; insert `_` | I(start of `ab`, end of `cd` after edit); outside Strong stays styled | GAP |
| SYNTAX-03 | `ab¦cd **right**\n`; insert `~` | I(start of `ab`, end of `cd` after edit); outside Strong stays styled | GAP |
| SYNTAX-04 | `ab¦cd _right_\n`; insert one backtick | I(start of `ab`, end of `cd` after edit); outside Emphasis stays styled | GAP |
| SYNTAX-05 | `ab¦cd _right_\n`; insert `[` and, in a fresh case, `]` | I(start of `ab`, end of `cd` after edit); outside Emphasis stays styled | GAP |

Block construction uses `change this line\n\n**sentinel**\n`, caret at physical-
line start. Every accepted prefix is compared with a clean parse. Literal
intermediate characters paint as current literal text; completing the delimiter
publishes the named shell without source marker relay from `**sentinel**`.

| ID | Exact command sequence | Final shell | Current evidence |
|---|---|---|---|
| SYNTAX-06 | insert `# `; then remove it | ATX heading, then Plain | PARTIAL: ordinary marker-transition tests; add actual-paint ledger case |
| SYNTAX-07 | insert `> `; then remove it | depth-one BlockQuote, then Plain | PARTIAL |
| SYNTAX-08 | insert `- `; then remove it | bullet ListItem, then Plain | PARTIAL |
| SYNTAX-09 | insert `1. `; then remove it | ordered ListItem, then Plain | PARTIAL |
| SYNTAX-10 | at line start type three backticks followed by `dart`, then Return; after `change this line` type Return followed by three backticks | fenced CodeBlock containing `change this line`, then following Plain sentinel | PARTIAL |

## Block shells, actions, and structural edits

| ID | Exact seed and command | Required result | Current evidence |
|---|---|---|---|
| BLOCK-01 | `# Te¦st is here\n`; insert ` now` | P; ATX heading level 1 retained | per-edit/80 ms paint coverage PRESENT |
| BLOCK-02 | `- fi¦rst **bold**\n`; insert `ke` | P; bullet list shell and Strong retained | PRESENT in north-star matrix |
| BLOCK-03 | `> fi¦rst **bold**\n`; insert `ke` | P; quote depth 1 and Strong retained | PRESENT in north-star matrix |
| BLOCK-04 | `\| f¦oo \| **bold** \|\n\| --- \| --- \|\n`; insert `x` | P; table shell/cells and Strong retained | PRESENT in north-star matrix |
| BLOCK-05 | fenced Dart block whose body is `final value = 'a¦';`; insert `x` | P; code shell retained and authored code stays exact | PARTIAL: code editing tests; add actual-paint row |
| BLOCK-06 | `Before **bold**.¦\n`; Return | S(paragraph split with one active empty successor); predecessor Strong retained | PRESENT |
| BLOCK-07 | BLOCK-06 Return immediately followed by `x` before pump | S(paragraph split with successor `x`); predecessor Strong retained | PRESENT true-burst row |
| BLOCK-08 | `Before **bold**.\n\n¦After.\n`; Backspace | S(paragraph merge); Strong retained and caret at join | PRESENT |
| BLOCK-09 | `- parent\n- child¦\n`; Tab, settle, then Shift-Tab | A(list-indent) then A(list-outdent), exact source and selection | PARTIAL: mounted final-state coverage; actual-paint/action ledger row missing |
| BLOCK-10 | `- parent\n- chil¦d\n`; insert composing `x`, then press Tab before certification | A(none); Tab is consumed without indenting or escaping to focus traversal | PARTIAL: authority tests exist; mounted semantics/action assertion required |
| BLOCK-11 | `\| a \| b \|\n\| --- \| --- \|\n\| c¦ \| d \|\n`; Tab then Shift-Tab | A(table-navigation); current mapped target cell | PARTIAL: mounted final-state coverage; actual-paint/action ledger row missing |
| BLOCK-12 | same table and caret as BLOCK-11; insert `xyz`, then press Tab before certification | A(none) until current mapping exists; no focus traversal leak | PRESENT suppression regression; add explicit semantics assertion if absent |
| BLOCK-13 | `Selection stays here.¦\n\n- [ ] todo\n`; click the task checkbox once | A(task-toggle); source becomes checked, task semantics becomes checked, paragraph selection is unchanged | PARTIAL: action tests exist; actual-paint transition row missing |
| BLOCK-14 | `- [ ] to¦do\n`; insert composing `x`, then attempt the checkbox action before certification | A(none); task shell may remain, but no hit box, onTap, or checked action semantics is published | PARTIAL: semantics coverage exists; bind it to unified pending snapshot in Phase 1 |

## Selection, clipboard, navigation, focus, and modes

| ID | Exact journey | Required result | Current evidence |
|---|---|---|---|
| NAV-01 | Product Tour first paragraph; Left/Right, Option-Left/Right, Up/Down across two wrapped visual lines | canonical/display caret identity and no source change on every paint | PARTIAL: navigation tests exist; one actual-paint product fixture row missing |
| NAV-02 | Shift-Right then Shift-Down across a row boundary; collapse selection | exact base/extent geometry; no continuity migration to another row | PARTIAL: selection tests exist; product fixture actual-paint row missing |
| NAV-03 | double-click `Rust`; drag selection across the paragraph; copy, cut, undo | exact source and selection; Strong styling restored after undo | PARTIAL: mounted/native final-state evidence; paint sequence missing |
| NAV-04 | focus another control, refocus editor, close/reopen platform input connection, type `x` | one accepted `x`, current caret, no rehome or hang | PARTIAL: focused mounted regression exists; add product-fixture paint assertions |
| NAV-05 | Product Tour long paragraph; resize 1569x906 → 1000x700 → 1569x906 | current source/display caret and grapheme-safe wrapping on every paint | PARTIAL: layout tests exist; moving-surface/actual-paint row missing |
| NAV-06 | Prose · 1 MiB; move forward two viewport pages, select/edit locally, move back, undo | current page/source/caret; no torn row/window; backward/forward remains truthful | PARTIAL: active viewport controller tests exist; mounted large-doc paint row missing |
| NAV-07 | Prose · 5 MiB; scroll at least two viewports away and back without selection input | scroll changes, selection/source do not; returning caret geometry is current | PARTIAL: native small-doc scroll and virtualization tests exist; large-preset path missing |
| MODE-01 | Product Tour in Edit, switch to Read, then Edit without source mutation | clean-parse-equivalent text/style/shell; no editing caret in Read; original selection restored or deterministically clamped in Edit | PARTIAL: parity tests exist; exact workbench journey missing |
| MODE-02 | capture exact Product Tour source, close app, relaunch default candidate | pristine Product Tour clean parse; no stale session, task overlay, selection, continuation, or resource | GAP: D0 lifecycle harness |

## Unicode and native macOS input

Japanese/CJK IME, autocorrect, predictive text, and dictation are outside D0.

| ID | Exact seed and command | Required result | Current evidence |
|---|---|---|---|
| UNICODE-01 | Product Tour Unicode line; insert `👩‍💻`, `🧑🏽‍🚀`, and `👨‍👩‍👧‍👦` at named grapheme boundaries | exact source/caret; no split grapheme; surrounding row stays rendered | PARTIAL: grapheme/unit tests exist; product actual-paint row missing |
| UNICODE-02 | replace `café` with `café`, then undo/redo | exact source form and grapheme-safe selection/caret; surrounding rendering stable | PARTIAL |
| UNICODE-03 | move and extend selection through `English العربية עברית English` | canonical/display geometry stays source-correct; no source mutation | PARTIAL |
| UNICODE-04 | native macOS `Option-E`, then `E`, in `caf¦\n` | source `café\n`, caret 4, no fault/resync | PRESENT settled native canary; add per-generation receipt only if the OS exposes an intermediate accepted generation |
| NATIVE-01 | exact 35-edit sustained Product Tour route declared above | every accepted generation has exact source/presentation/style/caret identity | PRESENT in `macos_native_canary_test.dart`; D0 orchestrator is GAP |
| NATIVE-02 | native Return/Backspace, pointer select+cut+undo, and wheel scroll sequences named in `macos_native_canary_test.dart` | settled exact source/selection and no fault/resync | PRESENT; exact embedded-artifact orchestration is GAP |

## Scale and performance journeys

All large-preset edits use a unique anchor and one insertion `x`, followed by
undo. A scroll/page step moves at least two visible viewport heights before
returning. The numeric denominator, latency endpoints, RSS procedure, and
watchdogs are frozen in `DOGFOOD_MILESTONE.md`.

| ID | Preset and anchor | Required result | Current evidence |
|---|---|---|---|
| SCALE-01 | Prose · 1 MiB, first block after `ordinary prose`; typing, inline typing inside first `**Flark**`, structural burst, 32 KiB paste/undo, scroll/page/close | all D0 profile gates | development profile only; dogfood receipt GAP |
| SCALE-02 | Dense blocks · 1 MiB, first `Short bounded paragraph 000001.` after `bounded`; edit/undo/scroll/page/close | all applicable open/edit/frame/memory/resource gates | GAP |
| SCALE-03 | Prose · 5 MiB, same first-block anchor; edit/undo/scroll/page/close | all applicable gates | development evidence only |
| SCALE-04 | Giant line · 5 MiB, after first `giant-word`; edit/navigation/undo/close | all applicable gates | development evidence only |
| SCALE-05 | Prose · 10 MiB, first-block anchor and first `**Flark**`; typing/inline typing/scroll/page/close | all applicable gates | development evidence only |
| SCALE-06 | Product Tour cold launch, five fresh OS processes | complete visible+overscan exact editable paint below 200 ms | GAP: actual app timestamps/receipt |
| SCALE-07 | Product Tour lifecycle: after `locally.` insert `x`, undo, close; 100 controller/session cycles in one warmed process plus 10 distinct OS processes | zero global native live state after every close; retained RSS within budget | GAP: inspector, harness, receipt |

## Known D0 blockers and proof gaps at freeze

### B0

No reproducible B0 is known at this baseline. This is not a universal claim;
the D0 gates must still prove the exact candidate.

### B1

No open B1 is recorded at this checkpoint. `B1-001` was closed by the
parser-parameterized `INLINE-10` dependency cell and actual-paint detector;
the aggregate candidate gates must still be rerun before D0 can pass.

### Architecture blockers

- Four controller authority stores (`_projectionContinuity`,
  `_committedParagraphSplit`, `_committedStructuralSurfaces`, and
  `_committedTaskChecks`) independently broaden/retire row authority. Phase 1
  replaces them with one sealed pending-presentation snapshot lifecycle.
- Literal envelopes and edit cells have parallel Core bind/advance paths.
  Phase 1 normalizes them into one pre-edit dependency-authority variant.

### Evidence/tooling blockers

- No checked-in dogfood-ready orchestrator can run the native canary without a
  skip and bind it to the app-embedded ABI.
- No `dogfood_performance_v1` schema/replay validator or actual-app timestamp
  instrumentation exists.
- The actual-paint gaps marked above can false-green through final-state tests.
- Historical performance prose is not current-candidate evidence.

## Explicit outside-D0 ledger

These do not block D0 unless investigation exposes a B0 or an in-envelope B1:

- Japanese/CJK IME, autocorrect, predictive text, dictation, and universal
  composition;
- arbitrary-file persistence, autosave, crash recovery, and document
  management;
- syntax-marker combinations beyond `SYNTAX-01` through `SYNTAX-10`;
- nested list/quote structural editing beyond the depth-one rows above;
- physical iOS/Android interaction and performance;
- peer comparison, competitor-derived scale tiers, publication, and alpha
  release;
- Web, Linux, Windows, collaboration, themes, final accessibility
  qualification, and unrelated visual polish; and
- complete background certification of a 10 MiB document within 500 ms. The
  D0 500-ms gate covers only the complete visible-plus-overscan range.

Each B2 allowed through handoff must add a reproducer, scope rationale, safe
workaround, owner/backlog ID, and proposed route to this section or the D0
receipt.

## Phase 0 exit

Phase 0 is complete when:

- this file is reviewed and committed;
- every D0 operation has an exact seed, command, required result, cadence, and
  evidence state;
- enabled presets, starting ABI posture, CI jobs, watchdogs, native route, and
  performance denominator are fixed;
- B0/B1, architecture, evidence, and outside-D0 ledgers are explicit; and
- later implementation is rejected unless it closes a named row/blocker or Dan
  explicitly changes scope.
