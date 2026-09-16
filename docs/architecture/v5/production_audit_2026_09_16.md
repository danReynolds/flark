# Production audit — 2026-09-16

Baseline: merged main `50e13d43d449bf1aef65780f39639d82606237e5`.
Working branch: `codex/production-audit`. This is an implementation self-review
and local diagnostic pass, not independent peer review or production approval.
CI remains skipped at the owner's request.

## Review result

The shared ownership model still holds. The Dart kernel owns source, selection,
commands, composition transactions and history. The adapters own input, layout,
paint and resource presentation. Code analysis is one shared service. This pass
found problems inside those boundaries; none requires a new document model,
parser, editor framework or host abstraction.

Reviewed the native extraction loops; source/live admission; bounded history;
controller/listener and coloring-worker lifetimes; layout/cache invalidation;
resource presentation replacement; preview loading; deferred editing findings;
and the current qualification tools/docs. Existing history caps bound undo and
redo together to 100 snapshots and 4 Mi UTF-16 source units. Coloring work and
retained results are bounded separately and reject stale responses. Fleury
geometry reuse is keyed by controller, source, mode, width, theme, width policy
and coloring revision. No broad refactor is warranted by these checks.

Raw production Dart line counts excluding generated `.g.dart` files were 5,643
in `flark`, 3,698 in `flark_flutter`, 2,835 in `flark_fleury`, and 1,374 in the
separate Tree-sitter package before this pass. The original package size budgets
are not currently exceeded. These counts describe scope, not quality proof.

## Fixed findings

| Finding | Correction | Evidence |
| --- | --- | --- |
| Definition-to-source mapping rescanned all preceding lines for each endpoint, causing quadratic extraction | Build one buffer line index and binary-search endpoints, preserving newline/prefix handling | 15 before/after diagnostic model fingerprints agree; multiline Unicode definitions in bare CR, CRLF, LF and nested quotes retain exact source/label/destination ranges |
| Every first inline child searched backwards through earlier runs for a sibling it did not have | Track the last emitted sibling per parent during the existing inline walk | Full native regression, conformance and fuzz suites; matching dense-inline models across the scaling sweep |
| Flutter could leave resource editing blocked after a document replacement while a custom presenter stayed pending | Retire the old session when its target is replaced | New real host test fails before the correction and passes afterward |
| A stale Fleury presenter completion could release the replacement dialog's guard | Only the identical current session can clear presentation state or restore focus; both hosts derive the guard from the session | New Fleury host test first produced a third dialog while the second was open; both hosts now reject that extra opening and recover after the correct completion |
| Root documentation still described V4 native-only behavior and Fleury as pending | Update root package/setup/limit guidance and current milestone sequence | Links point to the active V5 packages and evidence |

The parser's bundled Wasm is rebuilt. There is no schema or Markdown grammar
change, and no performance threshold was relaxed.

## Parser scaling diagnostic

Same machine, release Rust, five warmups and 25 recorded samples per shape/size.
Numbers below are median parse **plus** extraction, not host presentation time.
The tool also records parsing alone and output fingerprints.

| Workload | Approximate bytes | Before | After |
| --- | ---: | ---: | ---: |
| Contiguous reference definitions | 16 KiB | 1.029 ms | 0.396 ms |
| Contiguous reference definitions | 32 KiB | 3.212 ms | 0.773 ms |
| Contiguous reference definitions | 256 KiB | 154.310 ms | 6.157 ms |
| Dense inline paragraph | 16 KiB | 3.995 ms | 0.955 ms |
| Dense inline paragraph | 32 KiB | 14.008 ms | 2.005 ms |
| Dense inline paragraph | 256 KiB | 736.272 ms | 26.746 ms |
| Ordinary short paragraphs | 16 KiB | 0.651 ms | 0.720 ms |

The ordinary-prose case became modestly slower (roughly 5–10% across the sweep),
consistent with the added sibling index. This is a tradeoff against repeated
whole-leaf scans, not a claim of improvement on every input. Large sizes and the
long inline paragraph deliberately exceed product admission limits; these are
algorithm diagnostics, not an expanded live-rendering envelope.

The pinned kernel benchmark was also rerun, and its model hash is unchanged.
Absolute timings did **not** pass the 4 ms gate in this session. Other active
builds, a virtual machine and sustained CPU workloads were observed. The
before/after kernel p99 values and headless host timings are retained as
contended diagnostics; they cannot establish a clean performance regression or
close the earlier sustained macOS miss. Do not quietly replace them with a
favorable repeat.

## Open findings and release decisions

| Priority | Finding | Required next proof/change |
| --- | --- | --- |
| P1 | A parser-synthesized missing cell in a short table row shares a source anchor with the preceding cell. Placing in either missing column then typing `Z` edits `x` to `x Z` in the preceding column. | Design a shared cell-address/materialization command and first-paint pointer/Tab/typing/undo tests in both hosts. Do not claim arbitrary imported table-cell editing until this is fixed. Explicitly written empty cells are a separate, covered case. |
| P1 | Default Fleury image decoding and resizing is synchronous on the UI isolate. A permitted 2048×2048 PNG took about 313 ms in the native diagnostic, despite an encoded size of only 16,851 bytes. | Move decoding to a bounded background/native-browser path with stale cancellation, and prove uninterrupted typing during load. An async HTTP request and bounded output pixels do not make decoding asynchronous. Current custom preview builders can substitute application loading, but do not qualify the default. |
| P1 | Complete current-candidate input-to-presentation performance remains unqualified, including the prior macOS sustained insertion miss. | Diagnose the captured engine phases, eliminate workload contention for a qualification run, and qualify each host/browser/device separately. |
| P1 qualification | The current macOS workbench diagnostic emitted repeated native accessibility-tree update errors after accessibility inspection. | Minimize with a standard Flutter control and the ordinary app; establish whether the cause is Flark semantics, framework lifecycle, or the profiling harness. Do not claim VoiceOver qualification or ignore this noise in timings. |
| P2/profile decision | Backspace cannot remove an initial empty unclosed fence or an empty-label link. Table-boundary deletion also has deliberate refusals. | Define the intended structural deletion contract, then test the shared command. Source mode / Select All / RemoveLink remain explicit alternatives, not equivalent normal editing behavior. |
| P2 | Generated-command and corpus helpers remain duplicated between package tests. | Consider a dev-only shared test package when another change needs it; do not add testing machinery to the production public API solely to remove duplication. |

The image decoder measurement includes the mock response, decode and resize;
it is a diagnostic under the same machine conditions, not a universal image
latency bound. Its synchronous execution is confirmed by the implementation.
No image decoding change is bundled into the parser/session correction.

## Fresh validation

- Kernel analysis: clean. Full suite: **742 passed**.
- Flutter host analysis: clean. Full suite: **813 passed**.
- Fleury host analysis: clean after diagnostic lint fixes. Full suite: **92 passed**.
- Native parser: **49 tests passed** across library, ABI, conformance, fuzz and
  regressions. No registered conformance deviation was added.
- Native / bundled Wasm / freshly rebuilt Wasm: identical models on **1,322 cases**.
- Both new host presentation regressions were demonstrated failing before the
  fixes. The definition-coordinate regression passes with the new index.

The completed native workbench run and browser verification are recorded below.
The headless Fleury tool measures input dispatch through layout and
cell-buffer painting, validates the first edited character and caret, includes
real coloring workers, and preserves configured fallback behavior. It excludes
DOM/terminal presentation and uses image placeholders; its timings must not be
reported as an end-to-end or image-loading pass.

The final headless sweep completed all 11 shape/size cases. Prose, definitions,
code and tables remained live near both 16 and 32 KiB; code received real worker
colors before measurement. Image-heavy input stayed live near 16 KiB and fell
back to source at 32 KiB under the existing shape limits. The 256 KiB source case
also preserved its intended mode. Insert/delete occurred in the tail paragraph,
so this measures rebuilding those document shapes, not an in-fence or in-cell
editing latency guarantee. Selection and 40/80-column resize were included;
scroll presentation was not measured.

## Remaining release sequence

1. Correct missing-cell targeting and isolate bounded image decoding; minimize
   the macOS accessibility finding. Keep changes at the owning layer.
2. Complete quiet, current-candidate sustained performance for Flutter native,
   both web hosts and Fleury terminal; publish byte and shape envelopes.
3. Physical IME, accessibility, clipboard, focus and OS lifecycle qualification
   on only the platforms to be advertised.
4. Real consumer integration and daily-use acceptance, then package/tag and
   verify consumption of the actual release artifacts.

No new language or theme feature is needed to complete this sequence.

## Completed current-candidate macOS diagnostic

All 100 measured workbench cycles and 3,000 sustained edits completed at 800×600
logical pixels with no foreground loss or lifecycle transition. The actual
parser, Tree-sitter service, source/caret assertions and persistence path were
active. The complete gate **failed**, and this pass does not close D0.

| Measurement | Result | Limit |
| --- | ---: | ---: |
| Sustained insertion p99, 1,506 samples | 15.217 ms | <16.667 ms |
| Sustained deletion p99, 1,494 samples | 14.902 ms | <16.667 ms |
| Table inspection reflow p99, 14 samples | **19.445 ms** | <16.667 ms |
| Peak RSS above warm baseline | 10.42 MiB | ≤64 MiB |
| Retained RSS above warm baseline | −143.92 MiB | ≤16 MiB |

All other operation groups passed their unchanged budgets. The insertion maximum
was 50.994 ms and deletion maximum 23.200 ms; a p99 pass does not imply every
frame passed. Negative retained RSS is an observed growth check, not a proof that
all objects were reclaimed.

The failing table frame (1274) spent 0.748 ms in the action, 13.367 ms from input
to build start, 5.519 ms building and 0.519 ms rasterizing. Action time is included
in input-to-build; these fields must not be added twice. Most of the delay was
before the build, with a smaller elevated build cost. This narrows diagnosis to
scheduling and host rebuild work; it does not prove that the kernel, OS contention
or harness alone caused it. The other 13 table reflows ranged from 4.587 to
10.717 ms. Keep the failure instead of relaxing its small-sample percentile.

The original sustained insertion miss was not reproduced, but the run observed
2,948 native accessibility-tree errors and competing machine work. No clean
production-performance or assistive-technology pass is claimed. The next native
investigation should isolate table reflow and semantics activation, then rerun
the complete unchanged qualification under controlled conditions.

The [receipt directory](receipts/production-audit-2026-09-16/) includes raw linked
samples, build/source hashes, red/green tests, transport checks and contended
headless diagnostics. The profiling app is rebuilt with the ordinary entry point
after the run; synthetic preferences use the harness's separate namespace.

## Browser smoke and restored example

Built the updated parser/session code into the Fleury example at
`http://localhost:8820/revisions/9285a2689efb/`. In a fresh disposable browser tab:

- Clicked the rendered link, opened Edit, replaced its label through the actual
  input bridge, saved, and undid the change back to the original source.
- Typed inside the highlighted Ruby fence and undid the edit.
- Clicked an explicitly written table cell, typed and undid; Tab reached the
  next cell, where typing appeared at its start and undo restored the source.
- Confirmed the 512×512 demo image decoded, and clicking it opened its resource
  actions. Closed the actions and reset the disposable tab to the clean sample.
- No browser warning/error console entries were observed during this smoke pass.

This was a small-document interaction check, not browser performance, physical
IME, screen-reader or malformed-table qualification. Semantic DOM nodes are
read-only mirrors; typing was sent through Fleury's actual textarea bridge.

The Flutter macOS profile build using `--target lib/main.dart` completed after
the harness run. Its existing cross-architecture native-asset framework-name
warnings remain in the retained build log. The normal app was rebuilt, not
requalified or launched as another foreground dogfood run.
