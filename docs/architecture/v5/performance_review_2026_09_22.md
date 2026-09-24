# Performance and architecture review — 2026-09-22

Base: `7b37201` plus this uncommitted working tree. Apple M1 Pro, macOS 26.2,
Dart 3.12.2, Flutter 3.44.4. Other workloads kept the load average between 4
and 40, so only same-session HEAD/working-tree comparisons are reported. Kernel
and Fleury timings are JIT (`dart run`); Flutter host figures are shaped-row
counts from debug widget tests; browser figures come from Chromium in the
desktop app. All evidence is local. No CI, profile-mode or device run backs it.

## Changes made

These needed no product, API or packaging decision. Behavior is unchanged
except for the corrected quote style.

| Area | Change | Evidence |
| --- | --- | --- |
| Kernel projection | Inline rows walked every leaf run per line and every delimiter per gap; blank lines in a container scanned all its content records. Cursors and binary searches make both linear. | Projection, legality at every offset, legalization and anchors byte-identical to HEAD over 6,654 inputs (1,322 corpus cases × LF, CRLF, quoted, indented, list-nested, plus Unicode and container shapes) |
| Caret legality | `_atomicIntervals` segmented every grapheme of every row per keystroke. Only segment boundaries can join graphemes; an ASCII check settles most. | Same equivalence run |
| Commit preflight | Validation, two UTF-8 counts and the line limits walked the source four times per commit. One pass (`_SourceStats`) now serves all. | Kernel suite: 854 passed |
| Flutter row reuse | Layouts were reused by index, so a row inserted or removed near the top reshaped every row after it. Unchanged rows now match from both ends. Reuse also compares quote membership: a paragraph gaining `>` kept an unquoted painter and missed a themed quote style. | `row_reuse_test.dart`: both regressions failed before the fix. 30 KiB, 1,312 rows: Enter 743 → 3 shaped rows, joining Backspace 741 → 1, Undo 743 → 3, Redo 741 → 1 |
| Flutter paint and rebuilds | Paint builds observation data only with an observer and keeps visible list-marker painters. Scrolling no longer rebuilds the editor widget; `update()` marks only what changed, so scrolls do not rebuild the document's semantic text. The resolved theme is cached per dependency change. | Host 828 passed, example 40, Chrome (dart2js) web suites 29, Tree-sitter package 360; a scroll regression fails when the visible-fence report is removed |
| Fleury layout | Every edit laid out every grapheme again. A layout now reuses unchanged rows from its predecessor, matched from both ends and checked against every `_addText` input. Image previews use the kernel's images-only view instead of materializing every link. | A reused layout equals a fresh one after 12 edit kinds at 12/40/100 columns; removing the prefix check fails it. Fleury 111 passed, example 7 |
| Web transport | `utf8.encode` plus a copy into the Wasm heap cost milliseconds under dart2wasm. `TextEncoder.encodeInto` writes into Wasm memory directly. | Native FFI, dart2js and dart2wasm models identical over 6,625 cases |

`tool/bench_editor.dart --paragraph` adds the formerly quadratic shape.

## Measurements (HEAD → working tree)

Kernel facade, `tool/bench_editor.dart`, insert p50:

| Fixture | HEAD | Now |
| --- | ---: | ---: |
| Dense 16 KiB (pinned structural fixture) | 2.02 ms | 1.70 ms |
| Dense 32 KiB | 4.26 ms | 3.34 ms |
| Prose 32 KiB | 1.45 ms | 0.77 ms |
| One paragraph, 16 KiB | 7.00 ms | 1.41 ms |
| One paragraph, 32 KiB | 43.4 ms | 2.76 ms |

Fleury `tool/perf_audit.dart`, prose 32 KiB insert: 5.18 → 3.27 ms p50 and
13.7 → 6.3 ms p99. Tables are unchanged at about 5.1 ms because table
cells are still laid out again on every edit.

The dart2wasm parse of a 32 KiB mixed document took 9.8 → 6.5 ms median in
alternating page loads. The browser measurements were noisy, so treat this
delta as indicative.

## Observations for discussion

- The frame gate measures vsync phase. In the attended receipts, about 6.5 ms
  of the 11.2 ms median input-to-raster is waiting for vsync. On a 60 Hz
  display the <16.667 ms gate cannot pass. The harness also never inserts or
  removes rows near the document start. (Resolved in round two.)
- Under `flutter test --platform chrome --wasm`, the Tree-sitter color worker
  never answers: all 17 tests in `tree_sitter_web_test.dart` and the "Automatic
  Ruby" and "large Dart" sustained tests hit the service's 15 s timeout, on
  pristine HEAD as well. The other 10 dart2wasm tests pass, and dart2js passes
  all 29. The cause (test asset serving or the worker itself) is not yet known.
- The Fleury package was already over its 3,000-line budget at HEAD (3,294);
  reuse brings it to 3,451.
- Deferred because they need a decision: mimalloc, render-model slimming,
  mapping code colors through edits, prebuilt binary distribution, optional
  Tree-sitter, the `FlarkEditor` name clash, curated `flark_flutter` exports
  and Fleury publication. Round two settles several of these.

## Round two: decisions applied

The owner's answers: gate on work with a 16 ms budget, adopt mimalloc if small,
map code colors through edits, rename the kernel facade and curate host
exports. Render-model and code-highlighting follow-ups are analysed below.

| Item | Change | Evidence |
| --- | --- | --- |
| Frame gate | UI work (command + build) and raster must each stay below 16,667 µs at p99, and the edit must reach the next frame (1 ms vsync jitter, at most 1% late). Input-to-raster latency is reported, not gated. The frame profile adds Enter, a multi-line paste and Undo of each at the document start, with 64 bytes of live headroom. The validator recomputes every gate from linked samples. | Contract in `macos_qualification.md`. Under the new gate the 2026-09-20 workbench receipt passes (worst live UI work p99 9.3 ms, raster 1.1 ms, 4 of 3,348 live samples late). The 09-20 frame receipts lack per-sample command times and are rejected, so the frame profile needs attended 120 Hz and 60 Hz runs. |
| Allocator | mimalloc as Rust's global allocator on native targets; the Wasm module is unchanged. The crate's default v3 segfaults in `mi_thread_init` under `dart test` isolate groups, so `v2` is pinned. | Stripped macOS library 831,576 → 898,832 bytes (+67 KB, +8%); five crates added to the lock, one of them the C library. Through FFI, alternating system/v2 builds: dense 32 KiB parse p50 1.87–1.90 → 1.41–1.46 ms, prose 32 KiB 0.57–0.59 → 0.47–0.48 ms. Peak RSS +3–4 MiB. Retained RSS after the loop varied 102–215 MiB (system) and 182–220 MiB (v2) between runs, and eager purging (`MIMALLOC_PURGE_DELAY=0`) did not lower it, so this harness cannot separate allocator retention from Dart GC; the attended RSS gates decide. iOS, iOS simulator and Android cross-builds succeed; 49 Rust tests pass; transports stay identical. |
| Code colors | RFC 031 amended: while an exact result is pending, a fence paints its previous colors mapped through the edit (`shiftCodeHighlight`). | 7 mapping tests; Flutter and Fleury color tests updated to the new policy |
| API names | Kernel facade `FlarkEditor` → `FlarkSession`, `FlarkEditorSnapshot` → `FlarkSnapshot`. Both hosts name their widgets `FlarkEditor` and `FlarkMarkdown`. `package:flark/session.dart` exports the host-facing kernel API with an explicit `show` list, and the hosts re-export only that. Superseded before landing: PR #51's approved consumer API keeps the kernel facade `FlarkEditor`, adds a consumer `FlarkSession` and curates the host barrels itself, so this rename and `session.dart` were dropped when the review was rebased onto it. | All suites analyze clean and pass: kernel 854, Tree-sitter 367, Flutter 828, Flutter example 53, Fleury 111, Fleury example 7; Rust-free consumer check passes |
| Web transport | The model is copied out of Wasm memory as 32-bit words into a `Uint32List`, not as bytes. dart2wasm copies JS arrays with one call per element, and a `Uint32List` view over a byte array reads each word as four byte loads. | dart2wasm, Chromium 152 in the desktop app, after a 4 s warm-up, keystroke p50: dense 32 KiB 12.7 → 9.75 ms, prose 32 KiB 1.85 → 1.30 ms, one paragraph 32 KiB 10.4 → 7.4 ms. Dense-model copy 3.6 → 1.0 ms; reading every word 0.6 → 0.1 ms. dart2js is unchanged. Native, dart2js and dart2wasm models identical over 6,625 cases (digest `314deb7b`). |

Browser timings taken before adding the warm-up were up to 18× too slow for
some loops, because V8 runs Wasm functions unoptimized until a later call.
Measure dart2wasm only after warming up.

## Render model analysis

Sizes are exact, computed from real v4 models (`model_stats.dart`, local).

| Documents | Source | v4 | P1: UTF-16 + side table | P2: packed | P3: P2 without implied runs |
| --- | ---: | ---: | ---: | ---: | ---: |
| Dense 32 KiB | 32.3 KB | 470 KB (14.6×) | 261 KB | 156 KB (4.8×) | 129 KB |
| Prose 32 KiB | 32.3 KB | 102 KB (3.1×) | 53 KB | 29 KB (0.9×) | 21 KB |
| One paragraph 32 KiB | 32.0 KB | 540 KB (16.9×) | 247 KB | 116 KB (3.6×) | 64 KB |
| RFC 030 spike 32 KiB | 32.4 KB | 334 KB (10.3×) | 196 KB | 124 KB (3.8×) | 106 KB |
| Repository docs (133 files) | 1,754 KB | 6,358 KB (3.6×) | 3,452 KB | 2,092 KB (1.2×) | 1,396 KB |
| CommonMark + GFM examples | 30.3 KB | 677 KB (22.4×) | 452 KB | 323 KB (10.7×) | 299 KB |

- **Composition.** Runs are 38–95% of the model at 80 bytes each. 59–71% of
  runs are text; 39–56% are top-level plain text; soft breaks are 29% of runs
  in hard-wrapped documents such as this repository's.
- **Value ranges.** Hidden delimiters never exceed 4 units before content and
  138 after (a link destination); parents are at most 15 runs back. Sixteen
  bits hold all of them with room to spare.
- **Where bytes cost.** dart2wasm pays per element to copy and paid per word
  to read; the word copy fixed most of that. Natively the model is one memcpy
  plus per-keystroke garbage. dart2js copies natively.
- **Extraction is half the native parse.** Dense 32 KiB: comrak 0.68 ms of
  1.22 ms; prose 32 KiB: 0.17 of 0.37 ms. `LineIndex::new` builds a per-byte
  UTF-16 table one push at a time: 0.11 ms at 32 KiB, 9% of dense and 29% of
  prose parse time.

### Proposal: schema v5, packed

Fixed-width u32 words, so every record stays randomly addressable:

| Record | v4 | v5 | Fields |
| --- | ---: | ---: | --- |
| Line | 8 B | 4 B | UTF-16 start |
| Block | 64 B | 36 B | kind·flags·attr0 (8/8/16 bits), parent, start, end, first line, line count, content offset, content count, first run |
| Content | 32 B | 16 B | start, end, prefix start, line·virtual spaces (28/4 bits) |
| Run | 80 B | 16 B | start, end, hidden before·after (16/16), kind·flags·parent distance (8/8/16) |
| Definition | 32 B | 24 B | start, end, label start/end, destination start/end |

- **UTF-16 only.** Every Dart byte read either converts back to UTF-16, by
  walking source (`projection.dart` `_u16`) or through `lineOfByte`, or is an
  ASCII-only difference. The extractor emits UTF-16 for those ranges instead.
- **Blocks own their runs.** A block's `first run` replaces each run's
  `block` word and the binary search in `firstRunOfBlock`.
- **Side table for sparse data.** Link, image and autolink ranges and
  resolved strings, replacement and code display text, footnote labels, info
  strings, task symbols, list start numbers and table alignments move to one
  table keyed by record index (the "extra data" array of Zig's AST). A value
  that overflows its packed width (a delimiter over 65,535 units, say) sets a
  flag and moves there too.
- **Result.** 2.7–4.7× smaller than v4 on generated documents, 3.0× on this
  repository's docs and 2.1× on the spec examples; P1 reached 1.5–2.2×. At
  the measured copy rate, the dense dart2wasm copy falls from 1.0 ms to about
  0.35 ms (estimate). Native memcpy and per-keystroke garbage shrink by the
  same factor.

Also in the same change, because it rebuilds the Wasm asset anyway: an ASCII
fast path in `LineIndex::new`, and writing records straight into the output
buffer instead of copying them in `encode`.

`package:flark/render_model.dart` is public, so this is a breaking change.
Make it before 1.0, or mark the render model experimental. With the word copy
landed, v5's speed gain is modest: about 0.65 ms of a 9.75 ms dense dart2wasm
keystroke, plus a third of the native garbage. The stronger reason is fixing a
lean public format before 1.0.

### Optional: implied runs (P3)

Omit top-level plain-text runs ("content no run claims is plain text") and
soft breaks ("a line end with no break run is soft"). The projection already
renders unclaimed content as plain text and already handles line ends without
a break run. This removes a further 15–28% on the generated documents, 33%
on this repository's docs and 44% on one long paragraph. It changes what the
extraction claims about comrak, so the Rust conformance checks and the kernel
equivalence harness (6,654 inputs) must show identical projections first.

Decision (2026-09-23): dropped. After schema v5 it would save about 0.1 ms
of a web keystroke, which does not justify changing what the extraction
claims.

### Considered and not proposed

- **Varint or delta encoding.** Smaller still, but binary searches such as
  `lineOfUtf16` need random access, and decoding would cost Dart time on every
  platform.
- **Zero-copy native models** (`asTypedList` with a finalizer). Saves one
  memcpy of about 25 µs; it needs a fourth C ABI function.
- **Incremental models.** Relative widths, as rowan, Tree-sitter and Lezer
  use, would make unchanged blocks byte-identical between keystrokes, enabling
  delta transport and row reuse in the kernel. That is the path to
  per-keystroke cost proportional to the edit, but it reshapes the kernel.
  Current costs fit the native budget.

After the word copy, a dense 32 KiB dart2wasm keystroke is about 1.8 ms
Wasm parse, 1.0 ms copy, 3.4 ms projection and 2.0 ms document indexing. The
projection's object construction is now the largest web cost, not the model.

## Round three: schema v5 implemented

The owner approved the packed schema. `schema/render_model_v5.json` replaces
v4; the implied-runs step was not taken.

| Record | v4 | v5 | Fields |
| --- | ---: | ---: | --- |
| Line | 8 B | 4 B | start |
| Block | 64 B | 40 B | kind·flags, parent, start, end, first line, line count, content offset, first run, attr, extra |
| Content | 32 B | 16 B | start, end, prefix start, line·virtual spaces (30/2 bits) |
| Run | 80 B | 16 B | start, end, hidden before·after (16/16), kind·flags·parent distance (8/8/16) |
| Definition | 32 B | 24 B | start, end, label, destination |
| Run-extra index | — | 8 B | run, extras offset, for runs with extra records |

Changes from the proposal:
- A block keeps `attr` as a whole word (40 bytes, not 36), so no attribute
  ever needs an escape. Its content count is implied by the next block's
  offset, because content records are now in block order.
- A table cell's column and alignment fields are gone: the extractor never
  set them, and the kernel counts cells and reads alignment from the table.
- A code span's backtick count is gone; nothing read it.
- `LineIndex::new` takes ASCII runs in one step, and the encoder writes the
  packed records directly.

| Documents | v4 | v5 |
| --- | ---: | ---: |
| Dense 32 KiB | 470 KB | 164 KB (2.9× smaller) |
| Prose 32 KiB | 102 KB | 30 KB (3.4×) |
| One paragraph 32 KiB | 540 KB | 116 KB (4.7×) |
| RFC 030 spike 32 KiB | 334 KB | 130 KB (2.6×) |
| Repository docs | 3.6× source | 1.24× source |
| CommonMark + GFM examples | 677 KB | 345 KB (2.0×) |

Verification, all local:
- **Rust:** 49 tests pass. Conformance extracts every corpus case with zero
  deviations, and the invariants are checked on each published model after
  `records::expand` rebuilds the byte-level records from it. One regression
  expectation changed, for block-ordered content.
- **Lossless:** for 6,611 documents (the corpus in five forms, plus a large
  mixed document), every expanded v5 model equals the v4 model word for word,
  apart from content order and the dropped backtick count.
- **Kernel:** projection, caret legality, legalization and anchors are
  byte-identical to pre-v5 over 6,654 inputs on the VM, and the same dump
  hashes identically under dart2js and dart2wasm in Chromium.
- **Transports:** native, dart2js and dart2wasm models agree over 6,625 cases
  (digest `73e0bbb6`); `verify_transports.sh` finds the rebuilt Wasm module
  identical to native on 1,322 cases.
- **Suites:** kernel 854, Tree-sitter 367, Flutter 828, Flutter example 53,
  Fleury 111, Fleury example 7; analysis clean.

Performance, M1 Pro with the load average at 18–34 from other workloads, so
each comparison alternates old and new:
- **Rust parse and extraction**, alternating per sample: dense 32 KiB 1.67 →
  1.61 ms, prose 0.345 → 0.315 ms, one paragraph 1.24 → 1.13 ms.
- **dart2wasm**, warm, two rounds each: parse including the copy, dense 2.8 →
  2.1 ms, prose 0.8 → 0.6 ms, one paragraph 2.2 → 1.4 ms. Keystroke, dense
  9.55–9.95 → 9.25–9.3 ms, prose 1.45 → 1.0–1.05 ms, one paragraph 6.9–8.4 →
  6.6–6.75 ms.
- **dart2js** already copied natively, so parsing did not change. The first v5
  build was 10–20% slower on one long paragraph, because each run's content
  range is now derived from two words and the per-line walk reread it. The
  projection now reads each leaf's range once, and the model's layout offsets
  are plain fields instead of late ones. After that, keystrokes match v4:
  one paragraph 4.7 ms (v4 4.3–5.15 ms), dense 8.55 ms (v4 8.45–9.05 ms).
- **VM** timings were too noisy under this load to report.

`RenderModel` now has named accessors (`blockKind`, `runContentStart`,
`itemMarkerEnd`, `linkDestination`, …). The views keep their UTF-16 getters.
Byte getters, `attr0`–`attr2`, `aux0`–`aux3` and `lineOfByte` are gone.

## Round four: profiling the remaining keystroke

The owner asked to test three guesses from round three: trimming allocation in
the projection and the caret index (guessed at 20–40%), incremental
projection, and Rust extraction (guessed at 10–30%). Profiles come from the
VM's sampling profiler over a dense 32 KiB keystroke loop. Timings alternate
old and new builds, because other workloads kept the load average at 10–35.

Where a dense 32 KiB VM keystroke went before this round: Rust parse through
FFI 45%, projection 30%, caret legality index 20% (12% of it sorting).

| Change | Result | Evidence |
| --- | --- | --- |
| Caret legality index | Every keystroke rebuilt the document's hidden and atomic intervals as records and sorted them twice to legalize one caret. They are now built already sorted (a stack emits closing delimiters in order; sorted streams merge linearly) as flat integer lists. The public `hiddenIntervals` list is built only when read. | VM keystroke, four alternating runs: dense 32 KiB 3.05–3.06 → 2.57–2.60 ms (−15%), prose 0.73 → 0.70 ms. Browser, best clean pair: dart2wasm dense 3.9 → 3.25 ms (−17%), one paragraph 3.6 → 2.7 ms (−25%); dart2js dense −9%, one paragraph −13%. |
| Projection | Line-break tables are arrays over the block's own lines, not two hash maps per row; `lineOfUtf16` tries its last answer before a binary search. | Projection alone, dense 32 KiB: 0.83 → 0.79 ms (−5%); prose unchanged. |
| FFI transport | Source is encoded as UTF-8 straight into native memory, validating surrogates in the same pass, instead of `utf8.encode` plus a copy. | Parse through FFI: dense 32 KiB 1.29–1.32 → 1.23–1.26 ms, prose 0.39 → 0.34 ms. Same exception and message for unpaired surrogates. |

All three keep behavior identical: the equivalence dump (projection,
legality at every offset, legalization, anchors; 6,654 inputs) is
byte-identical on the VM, dart2wasm and dart2js; the native transport digest
is unchanged (`73e0bbb6`); every package's suite passes.

What the profiles say about the guesses:
- **Trimming the projection: mostly wrong.** Its cost is building row strings
  (about 16%) and list growth (about 13%), which every rebuild pays. The easy
  items gave 5%.
- **The caret index: right, and bigger than guessed.** It dropped from 20% of
  a keystroke to about 9%.
- **Rust extraction: little left.** Parse phases for dense 32 KiB (1.20 ms):
  comrak 0.71 ms (59%), leaf extraction 0.30 ms (25%), AST drop 0.05 ms, the
  rest under 0.05 ms each. Vector growth is about 1%, and mimalloc's aligned
  path with macOS thread-local lookups about 2.4%. A few percent at most.
- **Incremental projection: the largest remaining lever.** Dense 32 KiB has
  1,387 rows. A full projection costs 0.81 ms, a shifted copy of every row
  0.22 ms, and comparing every block's model records 0.15 ms. Reusing rows
  whose block did not change would save roughly 0.45–0.65 ms of a 2.6 ms
  keystroke, and proportionally more on the web. It changes how rows are
  owned and needs a differential test against full rebuilds; not started.

## Round five: incremental projection

The owner approved reusing unchanged rows, with a differential test against
full rebuilds.

How it works:
- `Projection.of(model, source, previous:)` takes the projection being
  replaced; `FlarkEditor` passes it for every live edit, undo and redo, and
  `FlarkReadDocument` passes it when its markdown changes. The
  unchanged prefix and suffix of the source are found by comparing 256-unit
  substrings, then code units.
- A leaf row (paragraph, heading, table cell, code or HTML block) is reused
  when its block, with its lines, lies wholly in either part and its model
  records match the old block's relative to the block's start: kind, flags,
  attributes, line count, code info, content records, runs (kind, flags,
  extent, hidden widths, parent) and replacement text. Schema v5 stores parent
  distances and hidden widths relatively, so records compare as words.
- A reused row is a new object with source offsets and run indexes shifted. It
  shares its text and lists when nothing moved. Container shells and table
  fields are always recomputed, because they depend on blocks outside the row.
- Run styles are computed per block as its inline row is built, so reused rows
  skip them. An unused per-run link table was deleted.

Verification:
- `checkInvariants` now compares every session projection against
  `Projection.of` from scratch, in every direct scenario, journey and matrix
  step (857 kernel tests).
- `test/incremental_projection_test.dart` edits every corpus document with 21
  insertions and 3 deletions at a spread of offsets (more than 50,000 edits).
  A second edit follows each one and reuses rows that were already reused. A
  mixed document takes every insertion at every line start. Rows away from an
  edit must keep their text object (more than 90%).
- Mutation checks: stale shells, skipping the run comparison, skipping content
  prefixes, and skipping run kinds each fail the test.
  - The run-kind case needed a new fixture: a lone `[^1]` paragraph is one text
    run without a definition and one footnote-reference run with it, over the
    same range.
  - The hidden-width comparison is a backstop the test cannot reach, because
    widths only change together with a kind or run count.
- Fresh projections are unchanged: the equivalence dump (6,654 inputs) is
  byte-identical to round four.

Results. VM rows are local runs on an M1 Pro with alternating builds, JIT, and
the same native parse library in both builds.

| Measure | Without reuse | With reuse |
| --- | --- | --- |
| VM keystroke, dense 32 KiB (six runs) | 2.59–3.15 ms, median 2.65 | 2.08–2.98 ms, median 2.20 (−17%) |
| VM keystroke, prose 32 KiB (six runs, load 13–19) | 0.64–0.68 ms, median 0.66 | 0.53–0.56 ms, median 0.55 (−16%) |
| VM projection alone, dense, one insertion at 10/50/90% | 0.74–0.76 ms | 0.37–0.46 ms |
| VM projection alone, prose | 0.15 ms | 0.07–0.08 ms |

The prose keystroke row predates the substring scan, which costs the same as
the code-unit scan on the VM (0.03–0.05 ms per 32 KiB); a later run was
discarded at load 28.

Browser rows come from Chromium in the desktop app's browser pane with
cross-origin isolation, so the timer resolves 5 µs. Each figure is the median
single call over ten rounds of 60-call blocks, alternating the two variants.
The document rows are the kernel's whole per-keystroke build after parsing:
the projection plus the caret index, which legalizing the caret forces.

| Measure | dart2js | dart2wasm |
| --- | --- | --- |
| Projection, dense 32 KiB | 1.82–1.89 → 0.86–1.16 ms | 2.11 → 1.02 ms |
| Document, dense 32 KiB | 1.60–1.78 → 1.42–1.47 ms | 2.98 → 1.74 ms |
| Projection, prose 32 KiB | 0.175 → 0.090 ms | 0.185 → 0.125 ms |
| Document, prose 32 KiB | 0.255 → 0.170 ms | 0.250 → 0.185 ms |

The first browser build compared code units one at a time. That scan measured
0.07 ms on both compilers and 0.11–0.29 ms on dart2js for prose, while
substrings took 0.005–0.015 ms. Before the change, reuse made prose slower on
dart2js (0.18 → 0.38 ms). One long paragraph has nothing to reuse and pays
only the scan.

Code size: the reuse path adds about 4 KB to a dart2js build and 5 KB to a
dart2wasm build (A/B harness: 244,753 against 241,079 characters, and 164,488
against 159,749 bytes).

What remains in a reusing projection is shells for every row, the shifted
copies of rows after the edit, and the scan. An edit shifts every later row by
its length. Row offsets stored relative to the row would make that free, but
would change the row API that hosts use.

### Landing on main after PR #51

PR #51's consumer API landed first, so the review was rebased onto it:
- the kernel facade rename and `session.dart` were dropped in favour of
  that API's names and barrels;
- the one-pass preflight and row reuse pass through its shared
  `_projectSnapshot`, and its read-only `FlarkReadDocument` reuses rows
  when its markdown changes, so appended or streamed content lays out
  only what is new;
- Fleury's new `FlarkCellController.colorsFor` returns shifted colors;
- the web build's embedded parser (`bundled_wasm.g.dart`) is regenerated
  with each Wasm change.

After the rebase every package's suite passes again: kernel 870,
Tree-sitter 367, Fleury 113 and its example 6, Flutter 833 and its
example 51, and the example's 29 browser tests under dart2js.

## Correctness findings from longer runs

Verifying this round with the random-edit matrix at 5,000 sequences per seed
found a failure on seed 12. It fails identically at HEAD.

- **Root cause, in comrak 0.54.** `adjust_node_newlines` indexes the
  paragraph's line offsets by the number of lines an inline node crosses. It
  should use the node's line within the paragraph, as `parse_inline` does.
  - An HTML tag or code span that starts after the paragraph's first line, and
    ends on a line with a different prefix, gets an end column off by the
    difference. Lazy continuation lines and partial tabs cause this.
  - `> x\n> <a\nb> q` extracted the tag as 6..13 instead of 6..11. That
    overlapped the following text, and the row showed ` q` twice.
- **Fix, in the parse crate.**
  - A multiline HTML tag ends at its literal's last line, placed at the
    content start of the line it ends on, after that line's virtual spaces.
    The first and last lines must match the source, otherwise the extraction
    reports `html-inline-end`.
  - Single-line tags must equal their literal (`html-inline-literal`).
  - Multiline code spans always locate their closing run in the source. The
    wrong column could land just after an unrelated backtick run and refuse a
    valid document.
  - A register row was added. The regression
    `a_multiline_tag_or_code_span_ends_on_its_own_line` covers quote, list,
    nested quote, ordered list, a partial tab, a non-lazy control, CRLF, and
    two and three lines.
  - The committed Wasm grew from 534,809 to 538,062 bytes. It is identical to
    native across 1,322 cases.
- **New Rust invariant.** A run lies inside its parent and after its previous
  sibling. No existing check compared sibling runs, so this overlap passed
  them all.

Fuzzing well past the default 2,000 iterations, with the new invariant, finds
older problems at 20–36 per million random documents (five million scanned).
None is caused or changed by this round: the same scan against the crate
without the fix gives the same failures. By family:

- **Link destination or title starting on the next line** (`[x](\n…)`, LF
  and CRLF). Runs after the link land on the wrong line and overlap. The Dart
  matrix shows it too: seeds 15 and 19 at 5,000 sequences fail identically at
  HEAD. The extraction's "leaf contains a CR" exemption, meant for bare CR,
  also accepts such mispositioned text in CRLF documents.
- **An HTML block after a partial tab** (`>\t<div>`, `- x\n\t<div>`).
  Sibling blocks overlap, which breaks an existing invariant.
- **An empty footnote definition in a list item** (`1. [^1]:\n\n…`).
  Content falls outside its block.
- **Refusals of valid documents.** These are `code-content`, `code-literal`,
  `text-mismatch`, `block-range`, `emph-delims`, `strike-delims`,
  `definition-gap` and `uncovered-lines`. They are safe, but such a document
  opens in source mode.

The overlaps render wrongly in the product today, because the extraction
checks deviations at runtime but not these invariants. These need a dedicated
pass on the parse crate before release: each family gets a register row, a
correction and a regression. Refusing a run that overlaps its sibling at
runtime is an interim option, but it trades a wrong render for source mode;
the owner decides. The default fuzz stays at 2,000 iterations and passes;
`FLARK_FUZZ_ITERATIONS=100000` reaches the first failure, at iteration 72,627.
None of this repository's 261 Markdown files trips any of these checks.

Decision (2026-09-23): the extraction checks the model's structural
invariants at runtime and refuses a model that fails them. Then each family
is corrected, before the code highlighting work.

## Code highlighting size

The Tree-sitter library is 14 MB natively and 13.1 MB as Wasm (1.8 MB
gzipped), instantiated twice on the web (page and color worker). Compiled
grammar sizes, macOS arm64 release:

| Grammar | Size | Grammar | Size |
| --- | ---: | --- | ---: |
| TypeScript (+TSX) | 2,795 KB | JavaScript | 404 KB |
| SQL | 2,376 KB | Go | 213 KB |
| Ruby | 2,057 KB | YAML | 185 KB |
| Bash | 1,334 KB | CSS | 115 KB |
| Dart | 1,243 KB | XML | 75 KB |
| Rust | 1,093 KB | HTML | 18 KB |
| Python | 444 KB | JSON | 6 KB |
| Runtime | 195 KB | | |

Six grammars are 88% of the grammar bytes. The package is about 1,500 lines
of Dart, 1,550 of Rust and 200 of queries, plus a worker per host and a build
hook per platform.

Options, all keeping one engine:

1. **A small Dart lexer as the only engine.** Per-language token rules
   (keywords, strings including raw, template and triple-quoted, comments,
   numbers, call and type heuristics) plus bracket and keyword indentation.
   Estimated at tens of KB of rules and 100–200 KB compiled for all 14
   languages, and well under 1 ms for an 8K snippet; both need measuring. RFC
   031 moved colors to a worker because Tree-sitter took 23.9 ms for such a
   snippet under dart2js. The lexer runs synchronously on the edited fence, so
   the color worker, RFC 031's asynchronous path and shifted colors all go
   away. Costs:
   grammar-accurate scopes in edge cases; Automatic detection becomes a small
   heuristic, or untagged fences stay plain; indentation for Python, Ruby,
   Bash, YAML and HTML becomes keyword- and indent-based. Existing authoring
   scenarios become its acceptance tests and show which expectations change.
2. **Tree-sitter as an opt-in package.** Hosts paint fences plain by default,
   and apps that want structure add the package. Zero cost by default, and
   the full 13–14 MB plus per-platform binaries when enabled.
3. **Trimmed or lazily loaded grammars.** The eight smallest total 1.5 MB, but
   Dart and Rust are 2.3 MB between them, and Automatic needs every grammar.
   Per-language Wasm modules and native libraries multiply release artifacts.
4. **Restore the highlight.js port.** About 75 KB of definitions, but it is
   regex-driven, was last released in March 2021, and brings back a second
   integration for indentation.

### Existing lightweight options, measured

The owner asked whether something existing could replace Tree-sitter before
writing our own. Measured locally, compiled Dart, M1 Pro:

| Option | Finding |
| --- | --- |
| `re_highlight` 0.0.3, the highlight.js 11.9 port behind `re_editor` | Depends on the Flutter SDK, so Fleury cannot use it; last published February 2024 |
| `highlighting` 0.9.0 and `highlight` 0.7.0, pure-Dart highlight.js ports | Link all 193 languages even when one is registered: +3.1 MB native and +1.43 MB dart2js (355 KB gzipped). Regex-based and no faster than Tree-sitter: 6.6K of Dart 3.4 ms, 8K of Rust 6.7 ms, 2K of JavaScript 3.3 ms. Automatic detection called Dart "Go"; the library prints exceptions to stdout |
| `syntax_highlight` 0.5 (TextMate grammars) | Depends on Flutter and `super_clipboard` |
| Prism, Lezer, Shiki | JavaScript only |
| A 160-line single-pass Dart lexer (prototype) | The same 6.6K Dart snippet in 0.035 ms; +65 KB native |

Tree-sitter's own numbers for about 8K units are 6.9 ms for Dart and 3.4 ms
for JavaScript natively (`PERFORMANCE_REVIEW.md`).

The recommendation moved from new lexers to porting CodeMirror 5's language
modes: MIT-licensed, hand-written, line-based tokenizers with saved state
and indentation rules, about 5,100 lines of JavaScript for today's fourteen
languages, much of it keyword data. The port is checked token by token
against CodeMirror 5 itself running in Node.

Decision (2026-09-23): port CodeMirror 5's modes to Dart as the single
engine, as its own package, starting with JavaScript and TypeScript end to
end and measuring size and speed before the rest. It follows the parse-crate
pass. Both hosts depend on `flark_tree_sitter` directly today, so every app
pays its 13–14 MB until the port replaces it.

## Live limits

A document over its live limits switches to source mode: raw, still
editable. Two limit sets exist:

| | Bytes | Lines | Blocks | Runs | Units per block |
| --- | ---: | ---: | ---: | ---: | ---: |
| Kernel defaults (`FlarkLiveLimits`) | 16 KiB | 2,048 | 2,048 | 8,192 | 16,384 |
| Workbench candidate profile | 32 KiB | 1,024 | 512 | 2,048 | 4,096 |

Kernel keystroke cost by shape at 16 KiB, VM JIT, local, loaded machine:

| Shape | Blocks | Insert p50 | Candidate profile |
| --- | ---: | ---: | --- |
| Prose | 49 | 0.4 ms | fits |
| Checklist (394 items) | 788 | 0.9 ms | over on blocks |
| Dense mixed | 769 | 1.6 ms | over on blocks and runs |
| Tables | 2,523 | 2.5 ms | over on blocks |
| List with blank lines | 2,694 | 2.6 ms | over on lines and blocks |

Cost grows about 1 µs per block now that projection is linear, so the
candidate's count caps are much tighter than the frame budget requires. Of
this repository's 176 Markdown files, 162 fit the candidate profile; most of
the rest exceed 32 KiB. Kernel work under dart2wasm is 2–3× the VM's.

Decision (2026-09-23): adopted as recommended, with a Flutter-web keystroke
measured at 32 KiB to settle the web byte limit.

Recommendation:
1. Give the candidate profile the kernel's count caps (2,048 blocks, 8,192
   runs, 2,048 lines, 16,384 units per block).
2. Set byte limits per platform: 32 KiB on desktop, where it is qualified;
   16 KiB on web and phones until each is measured.
3. Let hosts choose platform defaults; apps may override them.
4. Rerun qualification with list- and table-heavy documents at the new caps.
