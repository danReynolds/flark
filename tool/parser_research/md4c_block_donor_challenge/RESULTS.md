# MD4C block-donor gate results

Date: 2026-07-15

## Decision

There are two materially different proposals hidden behind “use MD4C for
blocks”:

1. **Embed or patch MD4C's C block engine over Crop:** **NO-GO.** Its private
   block phase assumes one contiguous document, accumulates a mutable
   whole-document tape, retains source pointers in reference state, finalizes
   lists by mutating earlier tape records, and has no callback/cancellation
   point until that analysis has finished. Making that a persistent value
   spine is a representation rewrite plus a C/Rust/native/Wasm integration,
   not a small adapter.
2. **Use MD4C's compact line/container algorithm as the primary code donor for
   an owned Rust value spine:** **not selected by this gate.** The first 1,289
   line estimate incorrectly treated `md_end_current_block` as a lexical facade
   boundary. The finalization-aware closure is 1,568 lines conservatively,
   1,382 lines for Flark's footnotes-disabled profile, or 1,112 lines if the
   already-existing Comrak HTML facade is also treated as the ordinary-line
   lexical boundary. Those are bounded reference surfaces, but none includes
   the persistent representation rewrite or proves an exact mixed-donor core.
   MD4C remains useful comparative prior art, not a size-based winner.

The strongest resulting model is therefore:

- Crop owns revisioned source;
- an owned Rust block state machine owns persistent checkpoints and packed
  structural facts;
- one function-correspondent donor owns line/container ordering and
  finalization; Comrak currently leads that next gate because it is already the
  exact Rust lineage, while MD4C and cmark-gfm remain measured challengers;
- the bounded Comrak facade remains the exact authority for HTML, table-row,
  and reference lexing, and the bounded Comrak inline hook remains the inline
  authority;
- specialized oversized-line states are exact streaming forms of those
  scanners, not a predictive parser;
- stock Comrak/cmark-gfm remain clean-parse oracles.

This is cleaner than a broad Comrak arena refactor and cleaner than a patched C
MD4C runtime. It is also much closer to “write the owned state machine” than to
“adopt MD4C.”

## What the prototype reached

`block_probe.c` includes pinned `md4c.c` in one translation unit and calls its
private seam:

1. `md_analyze_line`;
2. `md_process_line`;
3. `md_end_current_block`;
4. `md_build_ref_def_hashtable`;
5. `md_leave_child_containers`.

It stops before `md_process_all_blocks`, where MD4C performs inline parsing and
callbacks. This gives a real view of the block tape, retained origins,
reference state, memory, per-line work, and the cancellation seam. It does not
claim that upstream exposes or supports this API.

`full_parse_probe.c` calls public `md_parse` with no-op callbacks to provide
whole-document baselines. `source_audit.py` computes conservative dependency
closures for the private seams.

Pinned revisions:

- MD4C `65c6c9d72cebd9a731aaa5597414ce04d9ea5de3`;
- cmark-gfm `499789b49373bfa045d0e7547e5ee63444c77bca`.

MD4C's current `MD_DIALECT_GITHUB` also enables footnotes and admonitions.
Flark does not. The probes explicitly use tables, task lists, strikethrough,
and permissive autolinks only.

## Semantic result

### Block structure is surprisingly close

Against the 670 upstream GFM fixtures in this workspace, a structural HTML
signature that preserves paragraphs, headings, quotes, lists and tightness,
ordered starts, code blocks, and table structure matched **668/670**.

The two failures were:

- GFM tag filtering, which belongs to inline/render projection rather than the
  block spine;
- table example 203:

  ```markdown
  | abc | def |
  | --- |
  | bar |
  ```

  cmark-gfm/Comrak keep this as a paragraph because the two header cells do not
  match the one delimiter cell. MD4C activates a one-column table. This is a
  materially block-owned divergence.

The result is evidence that MD4C's container orchestration is a useful donor.
It is not evidence that MD4C is an exact target parser.

### Classification of the canonical 27 output differences

The canonical command against pinned cmark-gfm produced 643/670 and 27
failures. They divide cleanly:

| Ownership | Count | Failures | Consequence |
|---|---:|---|---|
| Inline-only | 17 | 9 emphasis/strong, 7 autolink, table example 200's escaped pipe inside code | Do not import MD4C inline parsing; the bounded Comrak inline hook owns these. |
| HTML serializer whitespace | 8 | Tabs 9, list items 272/274, lists 287/299/300/301/303 | Structural nesting, tightness, and inline ASTs match. cmark-gfm writes a formatting newline between tight paragraph text and a nested block; MD4C does not. This is not block or inline grammar state. |
| Materially block-owned | 1 | Table 203 activation | Compare exact header and delimiter cell counts using the Comrak table-row facade. |
| Renderer/projection | 1 | GFM tagfilter 652 | Apply tag filtering in the Comrak inline/render projection. |

The workspace JSON fixture produced 18 rather than 27 canonical failures
because it does not expose the nine newer cmark-gfm emphasis expectations. The
ownership conclusion is unchanged.

The original interpretation of the eight list-family rows was wrong. Comrak's
XML AST for every cited fixture contains **zero** `softbreak` nodes, and each
paragraph/text source position ends on its final visible character. The HTML
difference is only:

```text
cmark-gfm/Comrak: foo\n<ul>
MD4C:             foo<ul>
```

`canonical_spec_diff.py` collapses whitespace inside non-empty HTML text
chunks, so `foo\n` becomes the apparently semantic token `foo `. That makes a
serializer formatting choice look like a Markdown AST difference. The bounded
inline service's tested contract is the correct one: terminal LF, CRLF, and
trailing spaces are rtrimmed and emit no `SoftBreak`; only an interior line
ending can do so. If exact cmark-gfm HTML bytes matter, the HTML serializer must
insert its block-boundary newline from tree/event structure. The block spine
must not manufacture an inline softbreak or extend the leaf's logical inline
origin to achieve it.

### Exact table facade removes the bad dependency

MD4C discovers table cell boundaries by calling `md_analyze_inlines` from
`md_process_table_row`. Importing that path would make MD4C's inline engine a
second authority and expands the donor surface dramatically.

The existing bounded Comrak block-spine facade already exposes:

- exact `table_row` ranges and escaped-pipe handling;
- exact delimiter alignments;
- an 8 KiB fail-closed cap.

Table activation belongs to the owned block state: tokenize the pending header
and delimiter with the same facade, require equal non-zero cell counts, then
promote the pending paragraph. This rejects example 203. Table-cell contents
then go through the Comrak inline hook, which fixes example 200.

This is not overlapping parser authority if MD4C's table classifier and inline
boundary path are removed rather than used as a fallback. The seam is:

```text
owned block state
  -> exact Comrak row/delimiter lexical fact
  -> activation decision from pending-block context
  -> exact Comrak inline facts per retained cell range
```

For a C MD4C runtime this would require C-to-Rust callbacks or duplicated table
logic. For an owned Rust state machine it is a direct function call. That is a
major reason to use MD4C as a donor, not a linked library.

## Source and maintenance surface

Pinned source sizes:

| File | Lines |
|---|---:|
| `src/md4c.c` | 7,240 |
| `src/md4c.h` | 478 |
| `src/md4c-html.c` | 646 |
| `src/entity.c` | 2,185 |

Conservative `md_*` function dependency closures:

| Candidate seam | Functions | Function LOC | `CH` accesses | `STR` accesses |
|---|---:|---:|---:|---:|
| MD4C blocks + references | 45 | 2,192 | 106 | 9 |
| MD4C table boundary via inline engine | 65 | 2,563 | 82 | 8 |
| Union of both | 96 | 4,322 | 162 | 17 |
| Orchestration with actual table/reference lexical boundaries | 30 | 1,568 | 82 | 7 |
| Same, selected Flark profile (footnotes disabled) | 25 | 1,382 | 70 | 6 |
| Same profile, using the existing HTML lexical facade too | 20 | 1,112 | 42 | 1 |

The 4,322 line union is the early-stop result for “take MD4C blocks including
its exact cells”: it imports roughly 60% of `md4c.c`'s function bodies and the
wrong inline authority.

The historical 1,289-line row was invalid: it stopped dependency traversal at
`md_end_current_block`, but the Comrak facade supplies only lexical reference
facts. It does not consume definitions from the pending paragraph, perform the
setext downgrade, or finalize owned state. Stopping at the actual lexical
recognizers yields 1,568 lines and retains those responsibilities. Flark does
not enable footnotes, so the selected-profile reference surface is 1,382 lines.
The facade also already exposes exact HTML start/end classification; using all
of its existing ordinary-line lexical seams reduces the MD4C orchestration
reference to 1,112 lines.

These counts still include MD4C's tape-writing and backward-mutation functions,
which must be rewritten to emit owned packed facts. They also do not price the
exact table-promotion substitution, source origins, output hierarchy,
checkpoints, convergence, or oversized-line scanner states. The 1,112--1,568
range is therefore a provenance/reference measurement, not a copy-paste or
shipping-size estimate. It is enough to show that an exact line-atomic
milestone is bounded; it is not enough to show that mixing MD4C orchestration
with Comrak lexical/inline semantics is cheaper to make exact and maintain than
a correspondent Comrak port.

The source-access count matters. `CH(off)` and `STR(off)` are macros over
`ctx->text`, one flat pointer. Ordinary-line scratch does not make the current C
code source-agnostic: container close reads an old task marker by absolute
offset, reference finalization revisits prior lines, and the tape's line ranges
refer back to the original flat allocation.

## Why the current representation is not a persistent spine

The internal types measured on arm64 are:

| Type | Bytes | Problem |
|---|---:|---|
| `MD_CTX` | 664 | Contains source, buffer, mark, tape, container, and reference pointers plus mutable capacities. |
| `MD_BLOCK` | 8 | Compact, but embedded in a mutable byte tape and overloaded for container values. |
| `MD_LINE` | 8 | One absolute source range per retained physical line. |
| `MD_CONTAINER` | 24 | Contains `block_byte_off`, a back-reference used to mutate an earlier list opener. |
| `MD_REF_DEF` | 40 | Mixes source offsets with pointers into source or owned merged strings. |

`md_process_doc` first analyzes the entire input into `block_bytes`, then builds
the reference hash, closes containers, and only then calls
`md_process_all_blocks`. User callbacks—including aborts—do not happen during
the first pass.

Specific persistence conflicts:

1. `current_block` is a pointer into a reallocating tape.
2. container `block_byte_off` points backwards into that tape; a later blank
   line sets `MD_BLOCK_LOOSE_LIST` on the earlier opener.
3. reference definitions store labels/titles either as pointers into the flat
   input or as separately allocated merged strings.
4. `md_end_current_block` reparses accumulated paragraph line records to
   consume leading reference definitions.
5. table rows are retained as lines and split only in the later inline pass.
6. `md_process_all_blocks` reuses the container allocation for a second
   tightness/render traversal and then zeroes the tape.

A safe checkpoint cannot be `memcpy(MD_CTX)`. A production checkpoint needs
value state designed for restart:

- container descriptors with marker characters, indentation, task metadata,
  and a persistent list-looseness prefix fold;
- current leaf kind and source-origin run builder;
- pivot kind, fence character/length, HTML type/terminator state, and the two
  blank-line/list flags;
- pending local reference-definition recognition only while the current leaf
  is unfinished; recognized occurrences and the first-definition-wins symbol
  index belong to the output/dependency aggregate, not block continuation;
- packed structural output page identity/digest;
- no pointers into scratch, old source revisions, or mutable output pages.

Structural convergence requires equality of that block state plus a matching
source suffix and structural output-page digest. It must **not** depend on the
global reference map: duplicate definitions are still consumed regardless of
which value wins, so a reference-value edit should converge structurally and
invalidate only dependent inline leaves. The state partition and evidence are
recorded in [ARCHITECTURE_STATE_PARTITION.md](../ARCHITECTURE_STATE_PARTITION.md).
MD4C has good clues about the necessary scalar grammar state, but its actual
context is not the checkpoint representation.

### Deep-clone restart result

`checkpoint_probe.c` challenged the stronger claim that pointerful state is
intrinsically unresumable. It parses an unchanged prefix, deep-copies every
allocated capacity, fixes `current_block`, rebases source-backed reference
pointers onto an edited document with the same prefix, resumes, and compares
semantic tape records and reference definitions with a clean parse.

```text
case=list      checkpoint=23 prefix_tape=40 retained_clone_bytes=896 exact=1
case=html      checkpoint=10 prefix_tape=20 retained_clone_bytes=512 exact=1
case=table     checkpoint=10 prefix_tape=16 retained_clone_bytes=512 exact=1
case=reference checkpoint=22 prefix_tape=0  retained_clone_bytes=832 exact=1
summary cases=4 failures=0
```

This hardens, rather than overturns, the recommendation:

- MD4C's line-boundary grammar state is sufficient to resume lists, an open
  HTML block, a pending table header, and already-consumed references;
- the current checkpoint must copy allocation capacities and the entire prefix
  tape, then repair source and heap pointers;
- it has no persistent sharing, compact equality digest, or suffix-convergence
  primitive;
- raw tape bytes are not deterministic because `MD_BLOCK.data` is populated
  from unused/uninitialized line-analysis data for block kinds where the field
  has no semantics. The comparison must normalize value fields.

So an owned value checkpoint is feasible, but it should extract the meaningful
grammar fields rather than serialize or retain `MD_CTX`.

## Refined ordinary-line and oversized-line model

The refined model materially improves feasibility. A universal suspendable
coroutine at every byte is not necessary.

For ordinary physical lines up to 8 KiB:

1. materialize one exact line/window from Crop;
2. call bounded exact lexical helpers synchronously;
3. translate returned local ranges to revisioned source origins;
4. commit one grammar transition and packed facts;
5. poll cancellation/yield before the next line.

For oversized lines, only the classifications whose answer depends on the
whole line need resumable state:

- HTML types 1-5: streaming terminator matcher with a tiny cross-chunk suffix;
- fenced closing lines: fence-run count followed by streaming
  whitespace-only validation;
- thematic/setext candidates: marker count plus invalid-character flag;
- table header/delimiter candidates: resumable Comrak-equivalent row/cell
  reduction and cell-count/alignment state;
- reference-definition candidates: bounded label state plus streaming
  destination/title terminators;
- ordinary prose: source range extension; no byte-by-byte block parse once the
  line cannot interrupt the current block.

These states should be derived from/tested against the same Comrak scanners.
They are exact slow paths, not provisional prediction. Keeping the input and
output on packed persistent pages makes a yield cheap.

This model removes the earlier objection that every loop in MD4C must become a
general coroutine. It does **not** make the existing C context persistent or
Crop-owned. It makes a Rust derivation of the useful orchestration practical.

## Stress results

### Stock whole-document MD4C baseline

Each row is a fresh process and a fresh `md_parse`; the source was already read
into the required flat buffer before the internal timer. Callbacks were no-ops.
Three cold runs:

| 10 MiB document | `md_parse` time | Peak RSS | Notes |
|---|---:|---:|---|
| Tiny list items (`- x\n`) | 205-218 ms | 97.9-99.6 MB | 2,621,440 list items and very high tape density. |
| Ordinary prose paragraphs | 32.6-34.4 ms | 22.6 MB | Low structural density. |
| One large three-column table | 87.6-91.7 ms | 17.7-19.5 MB | Header/delimiter plus repeated body rows. |

This is good C throughput but not an interactive hot-path result. Even prose is
above a 16 ms frame on this desktop, and the flat-source read/copy cost is not
inside the parser timer. It reinforces the need for incremental restart and
cooperative work rather than arguing against it.

### Block tape density

For 10 MiB of four-byte list items, the private block phase reported:

```text
lines=2621440
leaf_blocks=2621440
container_records=5242882
leaf_lines=2621440
source_runs=2621440
block_bytes=83886096
block_capacity=98173885
elapsed_ns=200112000
peak_rss=97878016
```

The density is semantically real—millions of list items require millions of
facts—but a monolithic reallocating tape is the wrong retention form. The owned
spine needs packed persistent pages and page-level reuse/cancellation, not Rust
objects per fact and not an 84 MiB contiguous tape.

### Cancellation

The private loop can poll before each physical line. On the same 10 MiB list,
canceling after 1,000 lines took 116 microseconds of measured block work and
retained only 32,000 tape bytes. This proves line-boundary polling is a viable
seam once the loop is owned.

Stock public `md_parse` cannot use that seam: the first callback occurs before
the document wrapper and then block callbacks occur only after whole-document
block analysis. A callback abort therefore does not cooperatively cancel the
block pass.

A single 10 MiB prose line took 4.75 ms in one block transition on this desktop.
`--cancel-after-lines 1` did not cancel it because there was no second line at
which to poll. This is exactly the case addressed by specialized oversized-line
states. The measurement is not a mobile latency guarantee.

## Origin runs and handoff contract

MD4C retains byte offsets, which is better than copying every leaf string, but
those offsets are valid only against its one flat `ctx->text`. Persistent facts
must not replace that with a `{Crop revision, absolute offset}` anchor, because
suffix reuse would then rebase every fact or retain the retired root. The owned
spine should retain immutable-coverage-leaf-relative origin runs such as:

```text
(coverage_leaf_id, relative_start, relative_end, line_ending_kind, transform)
```

The ordered coverage/output index carries subtree source lengths. Current
absolute positions are materialized by prefix sums only for queries and visible
deltas; a reused suffix page keeps its identity and payload after a prefix edit.

Important rules exposed by this gate:

- retain every *interior* physical line ending in a multiline inline leaf;
  terminal line endings may remain in raw source coverage, but Comrak-style
  logical inline input rtrims them and emits no terminal break fact;
- represent the leaf-to-nested-block boundary structurally. An exact HTML
  serializer may write formatting whitespace there without changing the inline
  origin or AST;
- table cells retain the exact ranges returned by the Comrak row facade;
- code and raw HTML retain indentation/line-ending metadata without copying;
- reference label, destination, and title facts retain source ranges plus the
  normalized lookup key;
- coalesce adjacent compatible prose runs, but do not lose physical-line
  boundaries required by soft/hard breaks or restart checkpoints.

## Native, Wasm, and upstream maintenance

Native C compilation is simple. Browser Wasm is a separate toolchain concern:
the installed clang cannot compile MD4C for `wasm32-unknown-unknown` because no
C libc/sysroot is present (`stdio.h` is missing). Emscripten/WASI or a bundled C
sysroot could solve that, but it would add a second native/Wasm build pipeline
beside the existing Rust bridge.

More importantly, a production C wrapper would need private internal exposure,
source callbacks or rebasing, event callbacks, cancellation, and Rust calls for
exact table/reference facts. That patch would live in the monolithic
`md4c.c`. Its maintenance cost is architectural, not just build scripting.

If MD4C-derived code is used:

- port only the compact orchestration into Rust;
- retain the MIT notice and function-level provenance to the pinned commit;
- document every intentional semantic substitution with a Comrak facade;
- differential the owned result against stock Comrak and cmark-gfm on every
  upgrade;
- treat future MD4C releases as reviewable donor changes, not a patch that must
  merge mechanically.

This avoids a second runtime dependency and makes native/Wasm behavior one Rust
implementation.

## Recommended next implementation slice

Do not build a C MD4C bridge, and do not expand the paused MD4C-derived Rust
draft on the strength of the old 1,289-line estimate. The next decision-bearing
slice should:

1. keep the compact value checkpoint, persistent source, and packed output
   contracts donor-neutral;
2. transplant Comrak's exact ordinary-line block ordering and finalization
   function by function, but keep each ordinary physical line atomic instead
   of expanding every byte loop into a coroutine;
3. use the existing bounded Comrak HTML/table/reference facade and inline
   service from the start;
4. pass Gate A clean structure before adding checkpoint/convergence machinery,
   including the eight nested-child cases with **no** terminal inline break;
5. then add persistent restart, suffix-page identity reuse, reference-output
   partitioning, and randomized edit/clean-parse equality;
6. implement resumable oversized HTML/fence/thematic/table/reference states
   only where the ordinary-line cap requires them; and
7. retain the corrected 1,112--1,568-line MD4C surface as a stop-condition
   challenger. If Comrak correspondence again expands toward a broad fork or
   fails the value-state contracts, run the same thin Gate A slice from the
   MD4C ordering before funding more parser surface.

The donor decision is therefore not “MD4C or Comrak as the runtime parser.” It
is which upstream ordering gives the simplest exact provenance for one owned
state machine. The corrected evidence makes Comrak the leading next
falsification, not an unconditional lifetime selection.

## Reproduction receipts

Source audit:

```sh
python3 tool/parser_research/md4c_block_donor_challenge/source_audit.py \
  --source /tmp/flark-md4c-gate/src/md4c.c
```

Focused exact Comrak facade tests:

```sh
cargo test \
  --manifest-path tool/parser_research/comrak_in_place_block_challenger/Cargo.toml \
  --test block_seams -- --nocapture
```

Receipt: 5 passed, 0 failed, including table activation context, escaped pipes,
HTML state, ordered reference occurrences, list prefix looseness, and the 8 KiB
fail-closed cap.

Canonical cmark-gfm differential:

```sh
python3 tool/parser_research/canonical_spec_diff.py \
  --renderer /tmp/flark-md4c-gate/cmark-gfm-wrapper.sh \
  --spec /tmp/flark-cmark-gfm-gate/test/spec.txt \
  --extensions table strikethrough autolink tagfilter tasklist \
  --show 30
```

Receipt: 670 tests, 643 passed, 27 failed, classified above.

Nested-list AST correction:

```sh
python3 tool/parser_research/md4c_block_donor_challenge/terminal_break_audit.py \
  --spec /tmp/flark-cmark-gfm-gate/test/spec.txt \
  --comrak tool/parser_research/comrak_inline_fragment_gate/target/debug/comrak
```

Receipt: all eight cited examples report `softbreaks=0`; paragraph and text
source positions end on the same visible character. The canonical HTML
difference is serializer whitespace, not inline origin state.

Native builds:

```sh
mkdir -p /tmp/flark-md4c-block-probe
cc -O2 -std=c11 -Wall -Wextra \
  -I /tmp/flark-md4c-gate/src \
  tool/parser_research/md4c_block_donor_challenge/block_probe.c \
  -o /tmp/flark-md4c-block-probe/block_probe
cc -O2 -std=c11 -Wall -Wextra \
  -I /tmp/flark-md4c-gate/src \
  tool/parser_research/md4c_block_donor_challenge/full_parse_probe.c \
  /tmp/flark-md4c-gate/src/md4c.c \
  -o /tmp/flark-md4c-block-probe/full_parse_probe
cc -O2 -std=c11 -Wall -Wextra \
  -I /tmp/flark-md4c-gate/src \
  tool/parser_research/md4c_block_donor_challenge/checkpoint_probe.c \
  -o /tmp/flark-md4c-block-probe/checkpoint_probe
/tmp/flark-md4c-block-probe/checkpoint_probe
```

The 10 MiB generators and exact timing commands are intentionally recorded in
the shell history/output for this research turn; the probes accept stdin so the
same corpora can be regenerated without checking large artifacts into the
repository.
