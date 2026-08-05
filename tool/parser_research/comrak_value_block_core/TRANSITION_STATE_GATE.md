# Physical-line transition-state gate

Status: **variant-local grammar partition GO; generated reference cursor,
oversized-line integration, and composed adoption HOLD**, 2026-07-15.

## Decision

The clean split is executable:

- `BlockCheckpoint` remains the complete pause/output-restoration witness.
- `BlockTransitionCheckpoint` is a separate, typed physical-line convergence
  key.
- `ParagraphTransitionState` retains two exact semantic answers rather than
  paragraph bytes: setext-visible content after leading definitions, and GFM
  last-line table eligibility/column count.
- output accumulators may differ while grammar continuation is equal; suffix
  adoption must compose those accumulators rather than make them parser state.

This key is not an arbitrary packed fingerprint. Every retained field has a
future parser read, and mutation tests provide both equality and sensitivity
witnesses. It is only valid between complete physical lines. The cooperative
mid-line scheduler must retain its `LinePhase`, scanner DFA, offsets, and
ephemeral line state; using this key for a mid-line suspension is a correctness
error.

## Exact field matrix

| Boundary fact | Transition key | Reason / current read |
| --- | --- | --- |
| syntax profile | global | enables GFM table grammar |
| document-start bit | global | controls first-line BOM handling |
| current frame | global | `current` may be a table row while the materialized open path ends in a cell; it controls lazy continuation and close order |
| open frame discriminants | per frame | container matching, containment, and text dispatch |
| list kind | `Bullet(marker)` or `Ordered(delimiter)` | `lists_match`; stored start is not read |
| item indentation | `marker_offset + padding` | the prefix matcher observes only the sum |
| item child presence | one boolean on `Item` only | a blank line continues an item only after it has a child |
| fenced code | character, minimum run, fence offset | close recognition and later-line logical indentation |
| indented code | discriminant only | continuation uses the fixed four-column rule |
| HTML block | block type | blank continuation and terminator family |
| paragraph | setext-visible-content result plus, only under GFM before `table_visited`, last-line table eligibility/column count | these are the only paragraph-derived values read by later block transitions; definitions, preface/header bytes, origins, and occurrences are output state |
| table | column count plus `min(autocompleted, 500001)` | future row shape and hostile-short-row guard; all already-over-cap histories are equivalent |
| heading, thematic break, row, cell | discriminant only | their other fields are output-only at a physical-line boundary |

Excluded from convergence equality:

- `last_line_blank` and the five blank/looseness child-fold bits;
- list start, tightness, list-node marker offset/padding, and every Item list
  display field;
- code info/literal/closed projections, HTML literal projection, heading
  level/setext/closed, table alignment values, and row-header output metadata;
- paragraph logical payload, reference occurrences/cursor output, preface and
  header-cell source segments, origin runs, and line offsets;
- pending raw code/HTML/heading/table-cell payload;
- source positions, handles, IDs, materialization cursors, and reference
  winners.

These facts are not disposable. They belong to the output accumulator/property
root and must be spliced or updated exactly when equal grammar state makes
suffix reuse eligible. The composer, not grammar equality alone, authorizes the
semantic attachment.

## Table counter finding

The pinned donor-visible `TableData::num_nonempty_cells` is incremented after
padding short rows, so `(columns * rows) - num_nonempty_cells` remains zero and
cannot implement the stated hostile-short-row ceiling. Changing this public
metadata broke the full Comrak differential.

The gate therefore keeps donor-visible `TableData` exact and adds a
parser-only `table_autocompleted_cells` counter. It increments by
`columns - source_present_cells`, is serialized in the complete checkpoint, and
enters the variant-local key only as the exact saturated future-observable
class. Full corpus output remains donor-exact while the intended safety branch
is now live.

## Executable receipt

```text
cargo test --test transition_state -- --nocapture

running 8 tests
test child_presence_enters_grammar_equality_only_for_an_item ... ok
test document_start_profile_and_current_frame_remain_in_transition_equality ... ok
test full_width_table_row_history_does_not_block_suffix_convergence ... ok
test giant_fence_and_html_payloads_do_not_enter_transition_equality ... ok
test item_transition_observes_only_effective_content_indent ... ok
test paragraph_key_retains_exact_future_decisions_not_payload_or_provenance ... ok
test raw_and_heading_output_fields_are_absent_but_future_read_fields_are_present ... ok
test table_autocomplete_state_is_counted_and_saturated_at_the_observable_cap ... ok
```

The raw-block adversary parses two differing aggregate payloads of at least one
MiB for both fenced code and HTML. Their full checkpoints differ, their typed
transition keys are equal, and closing/outside suffixes have the same normalized
block-transition trace. The table adversary proves a full-width row insertion
does not poison the rest of an open table, while exact-cap/over-cap states still
diverge on the next row. Item tests prove sum-equivalent marker indentation and
the child-presence kind restriction. Paragraph tests prove that different
payload/provenance with equal setext/table answers converges, while changed
column count, setext visibility, or `table_visited` does not.

```text
cargo test --test paragraph_continuation -- --nocapture

running 5 tests
test all_1322_spec_fixtures_have_exact_bounded_paragraph_decisions ... ok
test giant_leading_reference_is_source_visible_but_conservatively_nonconvergent ... ok
test giant_paragraph_projection_is_bounded_and_oversized_last_line_is_refillable ... ok
test grammar_plus_changed_output_reconstructs_the_new_list_prefix_only ... ok
test multiline_gfm_header_keeps_preface_output_out_of_grammar ... ok
```

The full CommonMark/GFM corpus certifies every open paragraph boundary. A
multi-line GFM paragraph keeps its preface and exact last-line projection in
`ParagraphOutputAccumulator`, while the grammar key retains only the header
column count. A one-MiB multi-line paragraph with a short last line inspects a
bounded window and retains zero paragraph payload. An oversized final physical
line is `Unknown` until the refillable table scanner supplies its scalar result.
An oversized leading reference stays source-visible and deliberately cannot
authorize reuse, including against itself.

`BlockCheckpoint::into_reuse_parts` moves, rather than clones, payload into an
`OutputAccumulatorCheckpoint`. `reconstruct_checkpoint` validates the selected
grammar and paragraph output cursor against that accumulator. The list witness
combines an old equal grammar root with a changed output root and rebuilds only
the new start number and paragraph payload; incompatible grammar or a stale
paragraph cursor is rejected.

The existing child-fold gate separately proves all 33 reachable output-fold
states, exact associative composition, and 22,737 split comparisons.

## Production cursor forms

The non-paragraph variants are suitable as the semantic shape of a packed
production key, with bounded integer encodings and validated enum domains.

Raw code/HTML output should retain a persistent source-run sequence plus the
existing scalar projection fold. That root is output state, not transition
equality; an interior edit splices its changed run range while the parser may
converge as soon as fence/HTML continuation fields agree.

The paragraph grammar shape is no longer a hold: it contains no owned String,
hash, source identity, or unbounded vector. Production output still needs:

1. an immutable segmented logical cursor over source/Crop leases;
2. a generated resumable reference-prefix DFA kept on the output side,
   distinguishing provisional end-of-grant from decisive rejection and
   supporting repeated definitions without losing a lookahead byte;
3. the last physical line's source projection and exact table-row scanner,
   reducing to eligibility/column count in grammar and retaining cell/preface
   ranges in output; and
4. output-side reference occurrences, origins, transforms, and paragraph source
   runs.

The existing handwritten `ReferencePrefixJob` proves bounded lexical state but
does not yet expose a safe repeated-definition/provisional-EOF cursor. The gate
therefore marks oversized leading-reference paragraphs `Unknown`. This is an
integration/generation hold, not evidence that block grammar requires unbounded
prior paragraph bytes.

No hash may authorize equality. If the exact generated recognizer cannot expose
a bounded semantic state, the honest fallback is conservative non-convergence
inside that one open paragraph plus source-visible `Unknown`, not a prediction
parser.

## Remaining adoption gates

1. Generate and integrate the refillable reference/table scanners into the one
   line kernel, then differential every grant split and eliminate the current
   atomic facade's over-cap rejection.
2. Replace the proof parser's owned paragraph `LeafContent::logical` with Crop
   leases/segmented output cursors; the grammar key itself is already bounded.
3. Compose the key directly with the one exact block transition authority; do
   not leave a second checkpoint-only interpreter.
4. On 100,000-item lists, 100,000-row tables, one-MiB raw blocks, and giant
   paragraphs, prove bounded restart, stable suffix IDs/pages, exact output-fold
   splice, and fresh-parse equality.
5. Prove table autocomplete counter behavior at and above the ceiling without
   constructing unbounded output synchronously.
6. Run persistent arena retirement, worker cancellation, WASM/native, and
   physical-device deadlines.

Until those are green, this result selects the state partition but does not
select a shipping parser implementation.
