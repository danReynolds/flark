# Exact block-spine donor ranking

Status: decision-gate evidence, 2026-07-15. This document ranks block-parser
donors for the RFC 023 architecture. It does not select a shipping parser and
does not weaken Gate A or Gate B.

## Decision

Use **function-correspondent Comrak block orchestration** as the leading donor
for one exact, Flark-owned Rust block-spine gate.

This is not a recommendation to retain Comrak's arena AST, patch stock
`Parser` into a persistent runtime, or introduce a second parser. It means:

- preserve Comrak's exact line/block ordering, containment, finalization,
  table promotion, list tightness, and reference-definition interactions;
- replace the arena, owned leaf strings, and mutable node corrections with
  Flark's persistent value state and direct fact/output sink;
- execute a bounded ordinary physical line atomically, with a cancellation
  boundary between lines;
- use explicit resumable scanners only when a physical line is too large for
  the current atomic grant; and
- keep the narrow pinned-Comrak scanner/inline facade, so table, reference,
  HTML, and inline semantics do not cross unrelated donor lineages.

The ranking is:

1. **Comrak-derived exact Rust block spine — conditional GO for one Gate A
   implementation slice.** It is the best combination of exact selected-profile
   behavior, Rust/native/Wasm fit, existing lexical/inline seams, and mechanical
   update provenance. Persistent representation and Gate A exactness remain
   unproved.
2. **cmark-gfm — GO as independent oracle, upstream algorithm/scanner lineage,
   and provenance cross-check; NO-GO as a direct production runtime or a new
   standalone C-to-Rust port.** It is exact and slightly smaller, but Comrak is
   already its maintained idiomatic Rust descendant.
3. **MD4C — useful compact comparative donor; NO-GO for the first primary
   implementation lane, GO as the explicit stop-condition challenger.** Its
   earlier 1,289-line estimate did omit exact finalization, but an equal-seam
   recount ranges from 1,568 conservative lines to 1,112 selected-profile lines
   when it shares the existing Comrak HTML facade. Size therefore does not
   eliminate it. Comrak still leads the first gate because one donor lineage
   owns exact block ordering, table promotion, finalization, and the existing
   Rust lexical/inline seams.
4. **Lezer Markdown — GO as an incremental-tree architecture reference;
   NO-GO as the production block donor.** Its fragment reuse is credible, but
   its grammar and output contract are deliberately not CommonMark-authoritative,
   and its scheduler is leaf-atomic rather than physical-line-atomic.

No candidate has passed Gate A. The recommendation is deliberately narrower:
the refined scheduling model removes the strongest measured objection to an
exact Comrak-derived implementation, making it the most rational next
falsification target.

Between the two challenged alternatives, **A (mechanically correspondent
cmark-gfm block semantics) wins over B (Lezer as production donor)**. The
practical implementation of A should start from Comrak's Rust correspondence,
not fund a second hand-maintained cmark C-to-Rust port.

## The model being ranked

The earlier Comrak-derived seam converted nearly every source-byte loop into an
explicit coroutine. That proved cancellation but expanded 718 mapped upstream
lines into 2,117 Rust lines. The `Stage`/`LineWork::tick` region alone spans
about 1,176 lines (`comrak_derived_core/src/lib.rs:475..1650`). That expansion
was evidence against the scheduling transformation, not necessarily against
Comrak's block algorithm.

The refined model is smaller:

```text
revisioned Crop source + line index
  -> exact line/block state machine
     -> ordinary line: one bounded exact transition
     -> oversized line: resumable structural classifier
  -> persistent checkpoint + compact facts/origin runs
  -> exact bounded inline service for changed/visible leaves
```

### Ordinary lines

For an ordinary line, the donor's exact `process_line` ordering remains one
function-correspondent transition. Flark charges all bytes, copies, allocations,
and structural work, then checks cancellation/revision supersession before the
next physical line. No per-byte program counter is added to every ordinary
parser function.

There are two honest fuel contracts to choose between:

1. **4 KiB atomic cap.** A line is atomic only when it fits the current Gate A/B
   byte fuel. Stream anything larger. This requires no harness change.
2. **Up-to-8 KiB granted kernel.** Preflight line length from Crop's line index,
   request an atomic grant for the whole line, charge every byte exactly once,
   and certify native and Wasm floor-device wall-time/allocation p99. A smaller
   caller grant routes the line to the resumable path. This requires an explicit
   Gate A/B contract change; it cannot be an uncharged fuel overrun.

The existing 8 KiB Comrak facade cap is a lexical-helper limit, not proof that
an 8 KiB atomic transition satisfies today's 4 KiB `WorkFuel`. Pre-scanning a
line in one poll and rereading it uncharged in another is rejected.

### Oversized lines

Only whole-line-sensitive work becomes resumable. The initial exact families
are:

1. indentation and repeated container-prefix matching;
2. thematic-break and setext full-line validation;
3. fence opener/closer runs, whitespace tail, and backtick-info validation;
4. HTML start classes and type 1–5 terminator scans, with blank termination for
   types 6/7;
5. table header/delimiter/body escaped-pipe reduction, column counts,
   alignment, and body normalization metadata;
6. reference label/destination/title recognition, exact consumed-prefix state,
   and ordered definition-occurrence emission; winner aggregation is separate;
7. ATX trailing-marker/source-fact trimming; and
8. plain/raw source-span drain and exact origin recording.

Source-visible fallback may suppress expensive syntax detail inside an
oversized construct only if product policy allows it. It may not change the
post-line block state. A giant reference definition must still be consumed (or
retained as paragraph text) exactly and emit the same occurrence facts; table
delimiters, fence closers, and HTML terminators can affect later block parsing.
Each slow path must therefore produce the same block continuation and
structural output as the clean donor. Reference winner values are a separate
document aggregate, not block continuation.

This changes implementation expansion, not semantic ownership. The full donor
ordering still matters.

## Equal-boundary source accounting

LOC is not a forecast of shipping size. It is useful only when the semantic
boundary is the same. Persistent source pages, checkpoints, convergence,
packed outputs, reclamation, cancellation protocol, and Dart deltas are
excluded from every row below.

| Candidate | Comparable selected semantic surface | Direct representation coupling | Exact lexical seam | Important qualification |
| --- | ---: | ---: | --- | --- |
| Comrak 0.54/current | 55 functions / 1,816 lines | 29 functions / 1,252 lines; 84 tree sites; 44 owned-content sites | Existing ~234-line bounded facade plus small exports | Rust source; exact Gate A oracle; semantic surface unchanged at current HEAD |
| cmark-gfm current | 56 functions / 1,518 lines | 39 direct tree sites; 49 content/buffer sites | 457 authored scanner lines plus C table/reference helpers | Exact lineage, but C mutable-node/content model must be ported or linked |
| MD4C current, table/reference facade only at true lexical boundaries | 30 functions / 1,568 lines | Mutable tape, `CH`/`STR` flat-source access, container back-references | Existing Comrak table/reference facade | Conservative full-source closure includes the disabled footnote branch |
| MD4C selected Flark profile, table/reference facade | 25 functions / 1,382 lines | Same representation rewrite | Existing Comrak table/reference facade | Footnotes are disabled; exact corrections and runtime infrastructure remain |
| MD4C selected Flark profile, all existing block lexical facades | 20 functions / 1,112 lines | Same representation rewrite | Existing Comrak HTML/table/reference facade | Equal ordinary-line runtime seam; oversized resumable scanners remain separate work |
| Lezer Markdown 1.7.2 | 2,318 TypeScript lines for all Markdown/GFM source modules | Mutable partial parse and leaf strings; compact immutable output tree | No exact Comrak-compatible facade | Source includes inline parser and extensions; block-only comparable LOC is not isolated |

### Comrak

The reproducible audit selects exactly 46 functions / 1,465 lines from
`parser/mod.rs` and nine functions / 351 lines from `parser/table.rs`:

```text
selected_functions=55
selected_upstream_function_lines=1816
representation_coupled_functions=29
representation_coupled_function_lines=1252
direct_tree_operation_sites=84
direct_owned_content_state_sites=44
direct_source_position_state_sites=301
generated_scanner_call_sites=25
```

The 1,252 representation-coupled lines are the lower bound that cannot be
copied unchanged into a persistent value spine. The remaining 564 lines are an
upper bound on bodies without *direct* tree/content terms, not a claim that
they are zero-work: many still need segmented-source, exact-origin, error, and
scheduler contracts.

The line-atomic model does not reduce the 55-function semantic surface. It
avoids multiplying it into a per-byte stage machine. This is the crucial
change from the earlier 3x seam.

The narrow facade remains valuable because it exposes generated HTML scanners,
table row/delimiter lexing, reference label/URL/title scanners, normalization,
and cleaning without retaining the arena. It does not own table promotion,
list finalization, paragraph/reference consumption, source origins, or
persistent symbol generations. Those remain in the exact block spine.

Pinned Comrak 0.54.0 (`172c2ee7d2c5c262a28be3e407aadf705daea2b7`)
and current HEAD (`9e10bf2458c9a1bf92a14feb39c548d7d23bfced`)
produce identical audit rows. Current HEAD has no diff in the selected parser,
table, inline, or node files.

### MD4C correction

The earlier MD4C report quoted 23 functions / 1,289 lines for “orchestration
with Comrak table/reference boundaries.” Its static audit stopped recursion at:

```text
boundaries=md_end_current_block,md_is_table_underline
```

That is not an equal boundary. The current Comrak facade exposes bounded
reference-definition lexical facts; it does not implement MD4C's
`md_end_current_block`, consume leading definitions from a pending paragraph,
downgrade a setext underline after definition consumption, or finalize the
owned block state.

Moving the boundary to the actual lexical recognizers—
`md_is_link_reference_definition` and `md_is_table_underline`—produces the
conservative full-source closure:

```text
surface=md4c_orchestration_with_actual_table_ref_lexical_boundaries
functions=30 function_loc=1568 CH=82 STR=7
boundaries=md_is_link_reference_definition,md_is_table_underline
```

That closure now includes `md_end_current_block`,
`md_consume_link_reference_definitions`, table promotion in `md_process_line`,
and container finalization. It still excludes the separate Comrak facade,
persistent representation, and exact corrections. It conservatively includes
footnote-related calls behind disabled profile flags. Flark's actual profile
does not enable footnotes, so treating that recognizer as out of profile yields
25 functions / 1,382 lines. The already-patched Comrak facade also exposes HTML
start/end classification. Treating that existing ordinary-line seam equally
yields 20 functions / 1,112 lines:

```text
surface=md4c_selected_profile_all_existing_lexical_boundaries
functions=20 function_loc=1112 CH=42 STR=1
boundaries=md_is_footnote_definition,md_is_html_block_end_condition,
           md_is_html_block_start_condition,md_is_link_reference_definition,
           md_is_table_underline
```

The correction invalidates the old 1,289-line argument, but it does **not**
eliminate MD4C on size. At the actual existing facade boundary it remains
materially smaller than the selected cmark-gfm and Comrak source surfaces.
Those lines still write a flat mutable tape and omit persistent source/output,
checkpoint, convergence, and oversized-scanner machinery, so the result is not
a shipping-size prediction. It makes MD4C the explicit challenger if the
function-correspondent Comrak slice again expands toward a broad fork.

The semantic patch list is also at the ownership boundary:

- GFM table example 203 needs exact equal header/delimiter counts;
- table cell boundaries must not call MD4C's inline analyzer; and
- exact reference consumption and occurrence emission, separate first-wins
  symbol aggregation, and table body normalization must use the Comrak/Flark
  contracts.

The previously cited eight list-family “terminal softbreak” differences are
not parser differences. Comrak XML contains zero `softbreak` nodes in examples
9, 272, 274, 287, 299, 300, 301, and 303; paragraph/text source positions end
on their final visible character. cmark-gfm/Comrak serialize `foo\n<ul>` while
MD4C serializes `foo<ul>`, and the HTML canonicalizer folded that formatting
newline into the text token. The inline service is correct to rtrim terminal
LF/CRLF/spaces. Exact HTML serialization, if required, inserts a separator at
the structural leaf-to-child boundary rather than manufacturing inline state.

These are tractable differences. They are nevertheless exactly the sort of
cross-seam interactions for which preserving one donor's block ordering is
safer than composing two donors.

### cmark-gfm

The exact C source audit selects:

```text
selectedFunctions=56
selectedFunctionLines=1518
src/blocks.c=35 functions / 1070 lines
extensions/table.c=21 functions / 448 lines
directTreeSites=39
directOwnedContentSites=49
```

The authored scanner sources add 365 lines in `src/scanners.re` and 92 lines in
`extensions/ext_scanners.re`; `parser.h` and `node.h` add 59 and 167 lines of
mutable parser/node contracts. Those are not included in the 1,518 function
lines.

cmark-gfm is attractive because it is the canonical exact GFM lineage, its
block code is 16.4% smaller than the audited Comrak surface, and its selected
block/table/scanner files have no diff from release `0.29.0.gfm.13`
(`587a12bb54d95ac37241377e6ddc93ea0e45439b`) to current HEAD
(`499789b49373bfa045d0e7547e5ee63444c77bca`). The last change to those files
landed on 2023-07-20.

That does not justify a direct production adoption:

- linking C preserves its mutable node/content/arena assumptions and creates a
  second native/Wasm toolchain beside the Rust bridge;
- a mechanical C-to-Rust translation preserves unsafe pointer and buffer
  semantics rather than producing Flark's persistent value state; and
- a hand port would duplicate work already embodied in Comrak, while Flark
  would still use Comrak-derived inline/table/reference services.

Use cmark-gfm to cross-check Comrak provenance, generated-scanner lineage,
security/pathological fixes, and exact output—not as a second Rust port.

## Lezer current-source challenge

The audit uses the canonical repository, not the lagging GitHub mirror:

- repository: <https://code.haverbeke.berlin/lezer/markdown>
- version: 1.7.2
- commit: `f847886185e9262235ed3cf35b32bbb31bbe2f6d`
- commit time: 2026-07-15 11:07:38 +02:00
- upstream tests: 737 passing

### What is genuinely strong

Lezer's incremental mechanism is real and compact:

- edits split/move `TreeFragment` unchanged ranges;
- composite context is hashed into `NodeProp.contextHash`;
- a fragment is reused only when its stored context hash matches the current
  block context; and
- 458 canonical histories produced zero incremental-vs-clean tree mismatches.

This is good evidence for persistent fragments, context fingerprints, compact
trees, and clean/incremental grammar identity. Those ideas should influence
Flark's checkpoint/output design.

### Semantic distance

The structural probe intentionally compares only a shared block vocabulary. It
does not render Markdown, validate exact source facts, model list tightness, or
claim CommonMark conformance.

Current 1.7.2 matches Comrak's projected block structure on 184/189 Gate A
fixtures. The five divergences are material block behaviors:

| Fixture | Lezer behavior | Exact behavior |
| --- | --- | --- |
| Tabs 10, `#\tFoo` | paragraph | ATX heading |
| HTML 171, `<textarea>...` | splits raw block around blank lines | one HTML block through closing tag |
| List item 280, empty item then indented `foo` | paragraph inside item | empty item followed by paragraph |
| GFM table 202 | short body row has one cell | auto-complete to header width |
| GFM table 204 | short/long rows keep one/three cells | normalize both to header width |

Across the same 458 histories, 16 revisions differ structurally from Comrak
(12 table-typing, four HTML-typing) even though incremental equals Lezer clean.
The canonical 1.7.2 release improved the earlier mirror result from 176 to 184
structural matches by fixing empty-line leaf creation. That is evidence of an
active, useful project; it is also evidence that a downstream Rust port would
need deliberate grammar replay.

Lezer's own README explicitly says it produces no HTML and knowingly accepts
reference links without validating that a definition exists. Its tree does not
carry Flark's definitive renderer facts, list-tightness property, table
alignment/normalization facts, reference-symbol generations, or total source
coverage.

### Scheduler and memory distance

`BlockContext.advance()` checks `stopAt` only at its entry. It then may consume
an entire multiline leaf. `LeafBlock.content` concatenates the whole paragraph,
and `finishLeaf` immediately invokes inline parsing. The GFM `TableParser`
similarly accumulates and finishes a whole table leaf. This is not the refined
physical-line scheduler seam.

Canonical 1.7.2 receipts:

| Shape | Input units | First `advance()` | Position after first call | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| one giant paragraph | 10,485,760 | 849 ms | 10,485,760 | 60.8 MB |
| one paragraph, 1,000,000 `a\n` lines | 2,000,000 | 342 ms | 2,000,000 | 133.2 MB |
| one HTML comment | 10,485,760 | 0.31 ms | 10,485,760 | 50.9 MB |
| two giant table rows | 10,485,760 | 2,025 ms | 10,485,760 | 524.4 MB |

The HTML regex happens to be fast, but it is still atomic. At the 8 KiB scale,
one 16,388-unit two-row table took about 7.9 ms in one call on this host. These
are host diagnostics, not floor-device acceptance numbers.

A qualifying Lezer-derived Rust core would therefore have to split block/leaf
orchestration, separate inline work, replace UTF-16 string positions, add total
source/origin facts, add exact reference and table semantics, and introduce
oversized-line states. Its most valuable properties would be rewritten rather
than adopted. No Lezer advantage is strong enough to reverse the donor choice.

## Restart, finalization, and downstream state

Line-boundary restart is not just an open-container stack. An exact checkpoint
must include or persistently reference:

- ordered open quote/list/item descriptors and indentation/marker values;
- pending leaf kind and exact source-origin run builder;
- blankness/list-looseness prefix aggregates;
- fence character/length and HTML class/terminator state;
- pending paragraph state needed for setext/table promotion;
- pending paragraph bytes/state needed to accept or reject a leading
  multi-line reference definition;
- table header width, alignment, row counters, and normalization limits;
- syntax-profile/version identity; and
- persistent structural output immediately before the checkpoint where needed
  to authorize a page splice.

Reference definition occurrences and the first-definition-wins symbol map are
persistent output aggregates, not next-line continuation. Comrak consumes a
recognized duplicate definition even when it does not replace the existing
winner. A winner value/presence edit must therefore be allowed to converge
structurally; symbol and dependent-inline invalidation happens in its separate
occurrence/dependency indexes.

Likewise, a persistent fact must not retain the current Crop root plus an
absolute offset. Facts and origin runs are relative to immutable
coverage/output leaves; current absolute positions are derived through ordered
prefix sums. This lets a prefix edit reuse a suffix page without rebasing every
fact or retaining a retired Crop revision.

Convergence requires exact continuation equality (fingerprint plus
collision-safe comparison), exact edit-lineage mapping to an unchanged source
suffix, and output pages that can be reused without coordinate rebasing. A
source-visible giant line may omit detailed syntax only after producing this
same continuation and required structural facts.

Lezer's context hashes are a useful reuse filter, not a serializable restart
contract. MD4C's deep-clone probe proves its line-boundary scalar grammar state
can resume four curated cases, but the cloned prefix tape and repaired pointers
are not persistent convergence. Comrak's earlier patch proves useful local
continuations, but its open-arena checkpoint loses closed-prefix list semantics
and can retain megabytes of paragraph content. All candidates still require the
Flark value-state rewrite.

The complete partition and executable receipts are recorded in
[`ARCHITECTURE_STATE_PARTITION.md`](ARCHITECTURE_STATE_PARTITION.md).

## Authority and seam test

The runtime must make every grammar decision once.

| Composition | Authority result |
| --- | --- |
| Comrak-correspondent block order + Comrak lexical/inline algorithms on Flark value contracts | One owned runtime, one coherent semantic lineage |
| cmark block port + Comrak inline/table/ref | Closely related lineage, but unnecessary second Rust port |
| MD4C orchestration + Comrak HTML/table/ref/inline | Exactness patches sit at donor boundaries; higher risk of overlapping assumptions |
| Lezer blocks + Comrak reference/table/inline | Removes Lezer leaf/table/reference machinery and rewrites its incremental seam |
| Any stock donor plus a predictive Dart parser | Two runtime authorities; rejected |

“One lineage” is not itself a correctness proof, and mixed provenance is allowed
for localized algorithms. The deciding distinction is whether an exact rule is
implemented once on Flark state or split across two donor state models. The
Comrak-correspondent route has the cleanest answer.

## Native, Wasm, FFI, and Dart

An owned Rust spine keeps native and Wasm behavior in one implementation and
uses the existing bridge/service boundary. cmark-gfm or MD4C as linked C would
add C build/sysroot tooling and cross-language callbacks precisely at table,
reference, output, and cancellation seams. Lezer as JavaScript would fit web
but require a separate native JS runtime or a second implementation.

None of this requires grammar work on Dart's UI isolate. Dart should splice
source, send revisioned edits, adopt compact deltas, and project visible facts.
The parser worker/isolate owns the atomic line kernel and streamed oversized
work. The atomic cap still needs floor-device p99 certification; “it runs on a
worker” is not permission for unbounded work or memory.

## Extraction and update strategy

### Recommended extraction

1. Pin exact Comrak and cmark-gfm commits.
2. Create a function-level provenance manifest for all 55 selected Comrak
   functions: upstream path/name/body hash, local correspondent, retained
   ordering, and intentional differences.
3. Mechanically translate one upstream function at a time onto Flark's value
   state. Keep names/order close enough for review. Do not create a generic AST
   backend and do not keep stock Comrak output in this path.
4. Keep generated scanners and bounded inline/table/reference entry points in
   the narrow facade. The ordinary block spine consumes their facts; it does
   not call a second parser.
5. Implement the 4 KiB model first, or formally change the gate before using an
   up-to-8 KiB atomic grant. Add oversized states family-by-family with exact
   post-line-state differential tests.
6. Preserve pristine Comrak and cmark-gfm binaries as independent clean/diff
   oracles. The shipping core never invokes them.

A broad supported fork API is not the preferred lifetime boundary. The
existing narrow facade is supportable; the block ownership rewrite is easier
to update when it lives in Flark modules with explicit provenance than when it
conflicts inside upstream's arena parser. An in-place fork may be used
temporarily to refactor/prove one function before extracting it.

### Upgrade replay

For every donor update:

1. Fetch the pinned successor without changing the production pin.
2. Fail CI if a selected body, generated scanner source, node invariant, or
   narrow-facade signature hash changed.
3. Produce a report classifying each change as semantic/security,
   pathological/performance, source-position, refactor, or irrelevant profile.
4. Replay relevant changes mechanically onto correspondent local functions;
   never auto-merge grammar changes.
5. Run all 652 CommonMark 0.31.2 fixtures, the selected 670 GFM profile,
   Gate A/B clean and revision histories, fuzz/pathological lanes, giant-line
   post-state equivalence, and native/Wasm resource receipts.
6. Require a normative fixture/adjudication for every intentional behavioral
   difference.

Current replay evidence:

- cmark-gfm's selected block/table/scanner sources are byte-diff clean from
  `0.29.0.gfm.13` to current HEAD;
- Comrak 0.54.0 and current HEAD produce identical 55-function audit output;
- the earlier Comrak 0.50-to-0.54 maintenance rehearsal had two state-field
  conflicts and one AST-shape adaptation, then passed the upstream and focused
  suites—useful but not proof for the derived spine; and
- MD4C has 77 commits after release 0.5.3, with 1,442 insertions and 593
  deletions across `md4c.c`/`md4c.h`. Active maintenance is good, but its
  monolithic donor diff is materially more work to classify.

## GO/NO-GO gate

Proceed with exactly one implementation slice. It must include the real
selected-profile block order for tabs, setext, all HTML classes, quotes, lists,
tables, and leading reference definitions—not another simplified parser.

**GO to continue** only if the slice demonstrates:

- 189/189 normative Gate A fixture HTML and facts;
- clean-vs-resumed equality at every canonical history revision;
- exact post-line restart state for every oversized classifier family;
- persistent state/output within Gate A memory and delta caps;
- no batch tree and no grammar-sensitive side scan on the edit path;
- charged 4 KiB work, or an explicitly approved/certified up-to-8 KiB atomic
  grant;
- native and Wasm parity; and
- a provenance replay report against a changed donor function.

**NO-GO / reopen the architecture** if exact function correspondence again
expands toward a broad arena-fork-sized rewrite, if common real-world lines
regularly hit source-visible fallback, if giant-line downstream state cannot
remain exact, or if persistent output/checkpoint ownership forces a second
grammar pass.

## Missing evidence

- No production-shaped value spine implements all 55 selected functions.
- The reduction from the old 3x coroutine expansion is architectural reasoning,
  not a measured final LOC result.
- The eight oversized classifier families have not been implemented and
  differentially proved.
- No floor-device native/Wasm p99 certifies a 4 or 8 KiB full line transition.
- Lezer comparison is structural, not normalized HTML/fact equivalence.
- The corrected MD4C range (1,568 conservative, 1,382 selected profile, 1,112
  with every existing block lexical facade) is not a shipping LOC estimate; a
  profile-specialized exact value-state port has not been built.
- The proposed function-body provenance manifest/update bot does not yet exist.

These gaps are why the verdict is conditional. They are also narrow enough to
make the next gate decision-bearing.

## Reproduction

Pins:

```sh
git -C /tmp/flark_block_donor_audit/cmark-gfm rev-parse HEAD
git -C /tmp/flark_block_donor_audit/comrak-current rev-parse HEAD
git -C /tmp/flark_block_donor_audit/md4c rev-parse HEAD
git -C /tmp/flark_block_donor_audit/lezer-canonical rev-parse HEAD
```

Comrak equal-surface audit:

```sh
cargo run --release \
  --manifest-path tool/parser_research/comrak_in_place_block_challenger/Cargo.toml \
  --bin comrak_in_place_block_challenger -- \
  /tmp/flark_block_donor_audit/comrak-current
```

cmark-gfm surface:

```sh
node tool/parser_research/block_donor_probe/cmark_surface_probe.mjs \
  /tmp/flark_block_donor_audit/cmark-gfm
```

Corrected MD4C finalization-aware lexical-boundary closures:

```sh
python3 tool/parser_research/md4c_block_donor_challenge/source_audit.py \
  --source /tmp/flark_block_donor_audit/md4c/src/md4c.c
```

Nested-list softbreak correction:

```sh
python3 tool/parser_research/md4c_block_donor_challenge/terminal_break_audit.py \
  --spec /tmp/flark-cmark-gfm-gate/test/spec.txt \
  --comrak tool/parser_research/comrak_inline_fragment_gate/target/debug/comrak
```

Canonical Lezer build/tests and Gate A structural differential:

```sh
git clone https://code.haverbeke.berlin/lezer/markdown \
  /tmp/flark_block_donor_audit/lezer-canonical
npm --prefix /tmp/flark_block_donor_audit/lezer-canonical install
npm --prefix /tmp/flark_block_donor_audit/lezer-canonical test

node tool/parser_research/block_donor_probe/lezer_gate_a_probe.mjs \
  test/fixtures/commonmark/upstream \
  /tmp/flark_block_donor_audit/lezer-canonical/dist/index.js \
  /tmp/flark_block_donor_audit/comrak-0.54.0/target/release/comrak
```

Lezer liveness examples:

```sh
/usr/bin/time -l node \
  tool/parser_research/block_donor_probe/lezer_liveness_probe.mjs \
  /tmp/flark_block_donor_audit/lezer-canonical/dist/index.js \
  giant-paragraph 10485760

/usr/bin/time -l node \
  tool/parser_research/block_donor_probe/lezer_liveness_probe.mjs \
  /tmp/flark_block_donor_audit/lezer-canonical/dist/index.js \
  table-row 5242878
```

Supporting local evidence:

- [`PARSER_DONOR_BAKEOFF.md`](PARSER_DONOR_BAKEOFF.md)
- [`COMRAK_IN_PLACE_BLOCK_ENGINE_CHALLENGE.md`](COMRAK_IN_PLACE_BLOCK_ENGINE_CHALLENGE.md)
- [`md4c_block_donor_challenge/RESULTS.md`](md4c_block_donor_challenge/RESULTS.md)
- [`comrak_derived_core/RESULTS.md`](comrak_derived_core/RESULTS.md)
- [`gate_a_harness/README.md`](gate_a_harness/README.md)

Primary upstreams:

- [Comrak](https://github.com/kivikakk/comrak)
- [cmark-gfm](https://github.com/github/cmark-gfm)
- [Lezer Markdown](https://code.haverbeke.berlin/lezer/markdown)
- [Lezer reference](https://lezer.codemirror.net/docs/ref/)
- [MD4C](https://github.com/mity/md4c)
