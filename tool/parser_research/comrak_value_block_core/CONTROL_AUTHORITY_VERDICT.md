# Transition control authority verdict

Status: **single control authority GO; output production and retirement HOLD**,
2026-07-16.

The cooperative gate now has one parser-owned physical-line and EOF control
authority. The exact Comrak-correspondent parser can pause within a
pathological physical line and EOF close without serializing its open frame
path or introducing a second syntax classifier, while the unlimited and
fuelled callers execute the same typed phase machines.

## What is shared now

`ValueBlockParser` owns:

| Shared primitive | Responsibility |
|---|---|
| `begin_line_transition` | initialize the revision-local physical-line cursor once |
| `step_line_transition` | own `CheckOpen`, `OpenNew`, text preparation, ancestor clearing, unmatched close, and text dispatch ordering |
| `begin_finish_transition` | create the EOF cursor once |
| `step_finish_transition` | own open-frame close, root close, and postorder list-position fold ordering |

`process_line` and `finalize_document` are unlimited-step wrappers over those
machines. `FuelledValueBlockParser` owns only budgets, cancellation, event
delivery, and final extraction; it retains the parser-owned transition value
between polls. The previous independent `check_open_blocks_inner`,
`open_new_blocks`, `add_text_to_container`, `propagate_list_sourcepos`,
`step_line`, and `step_finish` orchestration loops have been deleted.

## Executable receipt

After the cut:

- `cargo test --all-targets` and `cargo test --release --all-targets` are green
  (53 tests), including the full 1,322-fixture donor differential and
  every-line checkpoint/resume corpus;
- the fuelled full-corpus and focused setext/table/list/lazy-continuation tests
  remain green against the unlimited wrapper;
- `python3 scripts/generate_provenance.py --check` reports all 55 functions,
  with removed donor orchestration correspondents mapped to the shared phase
  machines;
- `cargo check --target wasm32-unknown-unknown --all-targets` is green; and
- source search finds no remaining duplicate orchestration functions.

The pinned donor projection remains the independent semantic oracle. There is
no longer an atomic parser algorithm serving as a second local authority.

## Output and retirement boundary

The current delivery cursor hard-caps consumer-visible events to the grant,
and separately reports generation. A 300-column GFM promotion delivers at most
256 events per poll but creates 304 events in one parser transition. Therefore
backpressure is proven, while dense event *production* is still atomic. The
persistent event-page integration must make row/cell construction resumable or
preflight a measured output allocation grant; merely queuing the 304 events is
not the final solution.

Likewise cancellation abandons the phase cursor and copies zero open frames,
but the receipt reports all flat-tree nodes still awaiting reclaim. The arena
owner must retire those pages under fuel. Dropping a large `Vec<BlockNode>` on
the worker callback is not covered by this gate.

Other explicitly separate atomic kernels remain the fixed long-line scanners,
reference classification, list-finalization subtree work, compatibility
closing inside `add_child`, and whole-literal code/HTML assembly.
