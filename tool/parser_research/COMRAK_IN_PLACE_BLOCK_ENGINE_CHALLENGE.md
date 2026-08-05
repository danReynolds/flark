# In-place Comrak block-engine challenge

Status: disposable architecture evidence, 2026-07-15. This work does not edit
RFC 023 or `FINDINGS.md`.

## Verdict

**NO-GO as a distinct shipping architecture.** Once Comrak's arena and owned
`Ast.content` are removed, the result is a Flark-owned value-state block spine
regardless of whether its file lives inside the Comrak fork. Refactoring the
existing `Parser` in place is not materially smaller than a function-correspondent
derived spine, and it makes upstream upgrades conflict with the ownership rewrite.

There is one **conditional GO**: keep a very narrow pinned-Comrak lexical and
bounded-inline seam. The current block facade is 232 lines plus roughly eleven
one-line module/export/visibility edits. It reuses Comrak's generated HTML
scanners, table-row lexer, label/URL/title scanners, normalization, and cleaning
without retaining the parser arena. That seam is worth maintaining if it stays
hard-capped, differentially tested, and version-pinned.

An in-place rewrite is still useful as a *temporary development technique*:
mechanically refactor one upstream function at a time, prove equality, and then
extract the result behind Flark's source/event/checkpoint contracts. It should
not become the long-lived broad fork.

The red-team result also rejects a premature declaration that the current
facade-driven commitment spine is exact. It has meaningful corpus and large-list
failures described below. The recommendation is therefore not “ship the current
owned prototype”; it is “continue the owned architecture using stricter
Comrak-function correspondence and do not commit until Gate A passes.”

## What was challenged

Three counterfactuals were separated:

1. **Event tap over stock Comrak.** Hook `add_child`, `add_line`, finalization,
   and table correction events while retaining the arena.
2. **Generic in-place backend.** Keep the existing all-profile `Parser`
   orchestration but abstract its mutable AST behind arena and value-state
   backends.
3. **Profile-specific in-place rewrite.** Rewrite only Flark's CommonMark/GFM
   block profile inside a fork, keeping generated scanners but replacing the
   arena with an open stack, aggregates, source ranges, events, and checkpoints.

The first is small but retains the already-falsified memory, checkpoint, and
delta representation. The second crosses most of Comrak's block/tree contract.
The third is the same runtime architecture and similar code surface as the
Flark-owned derived spine; putting it in the fork changes provenance packaging,
not capability.

## Reproducible source-surface audit

The executable audit is
[`comrak_in_place_block_challenger`](comrak_in_place_block_challenger/). Against
pristine Comrak 0.54.0 it selects the exact functions needed by Flark's current
block profile and reports:

| Surface | Result |
| --- | ---: |
| Selected functions | 55 |
| Selected upstream function lines | 1,816 |
| `parser/mod.rs` functions / lines | 46 / 1,465 |
| `parser/table.rs` functions / lines | 9 / 351 |
| Directly representation-coupled functions / lines | 29 / 1,252 |
| Direct mutable-tree operation sites | 84 |
| Direct owned-content/state sites | 44 |
| Source-position/line-state sites | 301 |
| Generated-scanner call sites | 25 |

The 1,252 lines are a lower bound, not a predicted patch size. Methods without a
direct tree operation still call coupled methods and need new scheduling/error
contracts. Persistent source roots, compact outputs, checkpoints, convergence,
cancellation, reclamation, and protocol integration are additional modules.

If the existing Comrak `Parser` must continue supporting every upstream
extension, the immediate ownership-sensitive neighborhood is
`parser/mod.rs` (2,953 lines), `parser/table.rs` (392), and `nodes.rs` (1,236):
4,581 lines before persistent runtime infrastructure. The 2,721-line inline
parser is avoidable only because the bounded inline hook is a separate choice.
This is smaller than the old full block-plus-inline 7.3k-line headline, but it
is still a deep fork rather than an API hook.

The important table result is structural: the reusable `row` lexer is about 62
lines, while header promotion, paragraph splitting, row/cell allocation,
autocompletion limits, counters, and source corrections occupy the rest of the
351-line selected table surface and directly mutate arena nodes. HTML scanner
DFAs are reusable, but HTML lifecycle state is not. Reference lexical helpers
are reusable under the cap, but paragraph finalization, first-definition-wins,
symbol generations, and dependent-leaf invalidation are document state.

Run the audit with:

```sh
cd tool/parser_research/comrak_in_place_block_challenger
cargo run --release -- /path/to/comrak-0.54.0
```

## Checkpoint and locality reproduction

The existing incremental patch was applied to a fresh Comrak 0.54.0 copy and
augmented with two focused private-module tests. It reproduced:

```text
full_tight=false resumed_tight=true
loose/tight continuation state equal=true
open_depth=4

checkpoint_depth=25002
checkpoint_ast_bytes=3400272
source_bytes=50002
paragraph_retained_content=510000
paragraph_retained_line_offsets=10000
```

This is not evidence that locality is impossible. The same patched parser
converged after 83 bytes in a 1 MiB list and 37 bytes in a 1 MiB table, reusing
large suffixes. It proves the opposite: Comrak's grammar has useful local
continuations, but an open-arena-spine clone does not contain exact closed-prefix
semantics and its cost follows `Ast`, owned paragraph content, and line-offset
vectors.

Exact value checkpoints therefore require native list-prefix aggregates,
table/HTML state, pending paragraph state, source-backed leaf segments, and
persistent reference generations. Adding an event tap does not create those
facts after the fact.

## Cancellation and oversized work

Stock Comrak schedules one whole physical line through `process_line`. Generated
scanner calls and handwritten loops do not accept a cancellation token. A fresh
10 MiB host probe requested cancellation after approximately 1.5 ms and observed
the parser return only after the atomic parse completed:

| Shape | Parse time | Time after cancellation request |
| --- | ---: | ---: |
| Fenced raw block | 6.4 ms | 4.9 ms |
| Type-1 HTML block | 25.2 ms | 23.8 ms |
| GFM table-shaped input | 10.3 ms | 8.8 ms |

These are host timings, not device acceptance numbers. The architectural result
is the absence of a poll boundary, not a particular millisecond threshold.

The bounded facade correctly rejects every HTML/table/reference slice above
8 KiB. A source-owned outer machine can scan to a line boundary with fuel and
then either call the bounded donor helper or emit an explicit source-visible
opaque region. Refactoring storage under stock `process_line` alone cannot make
that guarantee.

## Construct traces

Five focused challenger tests pass:

- list tightness distinguishes a closed loose prefix from an otherwise equal
  open suffix;
- table row lexing preserves escaped pipes, while activation still depends on
  a promotable preceding paragraph and equal column counts;
- all seven CommonMark HTML block start classes and types 1–5 terminators use
  Comrak's generated scanners;
- reference scanning emits ordered normalized occurrences while leaving
  first-definition-wins to persistent document state; and
- HTML, table, and reference helpers fail closed above the 8 KiB cap.

Run them with:

```sh
cd tool/parser_research/comrak_in_place_block_challenger
cargo test --release
cargo run --release --bin cancellation_probe
```

## Red-team result against the current owned candidate

The current 889-line `commitment_spine.rs` is valuable seam code, not an exact
block authority yet. Its seven curated tests pass, including tightness, table,
HTML, references, restart, source ranges, and giant-line yielding. The pinned
corpus diagnostic nevertheless records these structural divergences from
pristine Comrak:

- HTML blocks: examples 120, 141, 142, 143, 144, 152, 153, and 160;
- lists: examples 282, 284, 290, 291, 292, 293, 298, and 305;
- tables: example 201; and
- link reference definitions: examples 169, 180, 183, and 187.

The table and reference tests currently fail; the HTML/list tests assert the
known divergence lists. More importantly, the 10 MiB ordinary-list benchmark
made 1,747,627 tiny list lines but returned `facts=2 opaque=1`: its pending
paragraph crossed sibling item boundaries and the entire list payload exceeded
the paragraph cap. This violates the bounded-leaf premise even though every
physical line was only five bytes.

That failure is evidence for *how* the owned parser must be built. Scanner reuse
does not preserve Comrak's container/finalization semantics. The production
candidate should mechanically transplant and refactor the exact upstream block
ordering and containment rules, with a function-level provenance ledger, rather
than accumulate a simplified parallel orchestration.

## Maintenance comparison

### Broad in-place fork

- Merge conflicts land in the same 29 representation-coupled functions where
  upstream grammar/security/pathological fixes arrive.
- Keeping stock Comrak output in the same parser requires a generic dual backend
  or reconstructing an arena AST from events. Both add another representation
  path.
- Checkpoint, persistent output, source, and cancellation code remains Flark
  specific even though it is stored in the fork.
- Upstream extension additions enlarge a generic backend even when Flark does
  not enable them.

### Flark-owned derived spine plus narrow donor seam

- Pristine pinned Comrak remains an independent clean-parse oracle.
- Only generated scanners and bounded lexical/inline entry points are exposed by
  the fork; the current block-side patch is auditable at roughly 243 touched/new
  lines rather than more than a thousand ownership-sensitive lines.
- Upstream upgrades compare the selected function/provenance ledger and rerun
  normative differentials before deliberately porting semantic fixes.
- Runtime ownership, cancellation, stable identities, and output contracts stay
  in one Flark-controlled module rather than leaking into Comrak's AST API.

The owned lane still carries a substantial derived-parser maintenance burden.
The claim is only that the burden is explicit and isolated, not that it
disappears.

## Recommendation

1. **Do not fund a broad in-place Comrak backend refactor as a separate product
   lane.** It reaches the same value-state architecture with a worse upstream
   merge boundary.
2. **Keep and harden the narrow pinned-Comrak facade** for generated HTML/table/
   reference scanners and the bounded inline service. Keep caps and unsupported
   outcomes explicit.
3. **Use Comrak's block parser as a line-by-line donor, not merely a behavioral
   oracle.** Port/refactor containment and finalization in upstream order and
   record exact function provenance.
4. **Do not select the current commitment spine yet.** First eliminate all 21
   recorded corpus divergences, fix large ordinary-list leaf boundaries, and
   pass Gate A clean/resumed equality, direct-delta, cancellation, and resource
   lanes.
5. **Keep a stop condition.** If exact function correspondence plus persistent
   state expands toward the previously falsified broad-fork surface or cannot
   satisfy the cap without common real-world Markdown becoming opaque, reopen
   the product/SLA decision rather than hiding the miss behind the facade.

