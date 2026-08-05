# Child-fold continuation partition gate

Status: **mechanism GO; composed spanning-list adoption still open**, 2026-07-15.

## Question

Can an edit near the start of a document-spanning list converge before the
list closes, or does accumulated list-tightness state force parsing the entire
remaining document?

## Result

`ChildSequenceFold` contains two different kinds of state:

- `had_child`, which is observed by later block transitions; and
- blank/looseness facts, which determine finalized output properties.

The variant-local transition key retains the former only on an `Item`, the one
kind whose blank-line prefix matcher reads child presence. It composes the
latter with `ChildSequenceFold::followed_by`. The composition is the exact
associative summary of adjacent child ranges.

`BlockCheckpoint::transition_checkpoint` is a typed equality projection. Its
`BlockTransitionKind` retains only fields read by later block transitions:
list matching/effective indentation, Item child presence, fence scanning, HTML
type, exact paragraph setext/table decisions, and saturated table row-cap state.
It excludes list start/tightness and other display metadata, open-frame
`last_line_blank`, code/HTML/heading pending payload, and accumulated looseness.
Paragraph payload and reference/header/preface output are absent from grammar;
uncertified oversized recognizer states cannot authorize reuse. Equality may
never be replaced by a hash or opaque `u64` fingerprint. See
[`TRANSITION_STATE_GATE.md`](TRANSITION_STATE_GATE.md) for the complete field
matrix and table/raw-block adversaries.

## Executable receipt

```text
cargo test --test child_fold_partition -- --nocapture

running 6 tests
test code_projection_metadata_does_not_block_convergence ... ok
test child_output_fold_is_an_exact_associative_range_summary ... ok
test every_child_output_bit_is_absent_from_list_and_item_transition_state ... ok
test historical_child_presence_is_irrelevant_when_the_open_path_retains_a_child ... ok
test list_display_metadata_and_last_blank_output_do_not_block_convergence ... ok
test list_looseness_changes_output_but_not_suffix_block_transitions ... ok
```

The range-summary test checks every split across all sequences of up to four
children drawn from all eight blank/looseness input combinations: 22,737 exact
composition comparisons and asserts the complete 33-state reachable closure,
identity, and all reachable-state associativity triples. Transition tests flip
each of the five output-only bits on list and item frames across blank/outdent
suffixes. They also delete the complete historical child prefix while keeping
the same retained open child. The typed transition checkpoint remains equal
and the later structural trace is identical after normalizing only the final
output metadata. Additional mutation cases cover ordered-list start/display
fields, open-frame blank output, and code info/literal/closed projections. A
separate case verifies that differing looseness does produce a different
`ListData.tight` result.

## What this does not prove

The architecture cannot select the compact output from this isolated result.
The composed gate must still demonstrate on a 100,000-item list that:

1. ordinary and tightness-changing prefix/interior edits reach typed exact
   convergence without scanning to the list close;
2. normalized `BlockOrder` and the list-property aggregate splice in bounded
   work;
3. distant suffix pages and stable block IDs retain exact identity;
4. one parent/property result changes rather than every descendant record;
5. the adopted result, source coverage, references, and rendered output equal
   a fresh exact parse; and
6. repeated edits do not fragment the persistent sequence into unbounded tiny
   pages.

Until that composition is green, this is a mechanism proof rather than the
large-document decision receipt.
