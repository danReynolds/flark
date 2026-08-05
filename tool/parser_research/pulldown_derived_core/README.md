# Pulldown-derived persistent-core spike

This crate tests one narrow question: does Pulldown-Cmark provide a better
algorithmic starting point for Flark's parser if Flark owns the state,
incremental scheduling, facts, and output representation?

It is not a wrapper around `pulldown_cmark::Parser` and it is not a claim of
production completeness. The stock parser eagerly creates a first-pass node
stream before its event iterator starts. That representation is one of the
things this experiment is trying to avoid.

## Implemented slice

- Pulldown-derived tab-aware indentation and container scanning;
- explicit and lazy blockquote continuation;
- bullet and ordered list-item containers, including marker indentation;
- fenced-code open, content, and close transitions;
- single- and multi-line paragraph state;
- retroactive setext promotion with a paragraph-content digest preventing
  false convergence;
- exact source ranges for chunks and quote/list/fence/setext marker facts;
- Flark-owned, pointer-free semantic state apart from shared immutable
  container snapshots;
- `advance(Fuel)` with a hard bound on newly examined source bytes;
- a 512-byte syntactic-prefix cap plus constant-size summaries for arbitrarily
  long physical lines;
- sparse closed-output checkpoints, edit restart, state convergence, stable
  suffix IDs, and a direct output-delta description;
- clean-versus-resumed differential testing over 250 sequential edits.

The block output deliberately represents a paragraph or leaf as one
source-backed range. It does not create one token/node per byte and it does not
run inline parsing on construction. Inline output must be a separate lazy,
fuel-bounded leaf service.

## Donor correspondence

The donor is `pulldown-cmark` 0.13.4, package VCS revision
`38e4d08f14ec4bd9783270e9623db7681ebed968`, MIT licensed.

| Spike code | Pulldown 0.13.4 donor |
| --- | --- |
| `LineCursor` indentation/tab state | `src/scanners.rs`, `LineStart`, `scan_space_inner`, `scan_all_space` |
| Quote marker | `src/scanners.rs`, `LineStart::scan_blockquote_marker` |
| List marker and post-marker indentation | `LineStart::scan_list_marker_with_indent` and `finish_list_marker` |
| Setext recognition | `scan_setext_heading` plus `FirstPass::parse_setext_heading` ordering |
| Fence open/close | `scan_code_fence`, `scan_closing_code_fence`, and `FirstPass::parse_fenced_code_block` |
| Existing/new-container ordering and lazy paragraphs | `FirstPass::parse_block`, `parse_paragraph`, and `parse::scan_containers` |

The Flark-specific changes are structural, not cosmetic:

- scanners consume a bounded prefix plus composable tail summaries;
- source-byte fuel is first-class;
- state is a value snapshot rather than a partially built `Tree<Item>`;
- output is compact chunks/facts, emitted directly by the transition;
- setext promotion mutates the direct output chunk and its dependency digest;
- checkpoints are sparse and only retained when no later setext line can
  rewrite output before the checkpoint;
- edit convergence and output replacement are owned by Flark.

That means future maintenance is a semantic-port workflow, not a mechanical
rebase of Pulldown.

## Reproducible receipts

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release --bin pulldown_derived_receipt
```

The current release receipt is:

```text
giant_line_10mb source_bytes=20000000 chunks=1 checkpoints=2 transient_state_bytes=848 max_advance_source_bytes=4096
dense_million_lines input_bytes=2000000 lines=1000000 chunks=1 checkpoints=2 parser_metadata_under_32KiB max_advance_source_bytes=65536
edit_history edits=250 exact=250 converged=250 reused_suffix_chunks=1475462 max_reparsed_bytes=4120 max_delta_chunks=527
```

The 20 MB giant-line source capacity comes from the spike's flat `String`
growth and is explicitly not counted as a parser-state success. Replacing that
source and the eager `Vec` suffix shift with persistent trees remains a gate.

For contrast, the stock Pulldown first pass produced the following separate
falsification receipts:

- a 1 MB dense-inline input produced roughly 336,000-432,000 nodes and 24.2 MB
  of node capacity;
- a 2 MB, one-million-`a\n` paragraph produced 2,000,001 nodes and 192 MB of
  node capacity;
- an 11.2 MB giant paragraph exposed only four first-pass receipts, and a
  midpoint edit consumed the entire 11.2 MB leaf in one approximately 4.6 ms
  parser call.

Wrapping stock `FirstPass` is therefore a killed direction. The favorable
result here comes from retaining selected scanner/transition algorithms while
rejecting its eager block-plus-inline node representation.

## Executable limitations

`require_production_ready()` always returns `PRODUCTION_GAPS`; tests assert the
gaps. Important omissions are:

- the source is a flat `String`, so edit construction copies the document;
- output/checkpoints use `Vec`; suffix IDs are reused but suffix records are
  eagerly cloned and shifted rather than attached as a persistent subtree;
- edits inside a giant physical line restart at its preceding semantic
  checkpoint, though every continuation slice remains bounded;
- the rest of CommonMark/GFM block grammar, list tightness, container range
  facts, inline grammar, reference dependencies, GFM autolinks/tagfilter,
  stable order-key stress, and native/WASM parity are not implemented;
- a syntactic prefix requiring more than 512 bytes or more than 64 containers
  returns a visible error instead of silently changing semantics.

## Donor/seam conclusion

The best direction is hybrid by **algorithm provenance**, not two callable
parser cores:

1. Flark owns one persistent source, state machine, checkpoint format, fact
   model, and delta stream.
2. Pulldown's block scanners are credible donors for that machine.
3. Pulldown's inline algorithms are potentially useful donors for a lazy
   source-backed leaf resolver.
4. cmark-gfm/Comrak remains the best source for GFM bare-autolink behavior and
   an independent semantic oracle.

A Comrak-derived block service calling a stock Pulldown inline parser is not a
good seam. Pulldown's `Parser::new_with_callbacks` first runs `run_first_pass`,
and its inline passes mutate `Tree<Item>` chains; delimiter/link stacks store
`TreeIndex`. Those algorithms cannot consume a Flark leaf span directly today.
They would need extraction into a bounded token buffer and direct inline-fact
sink. Likewise, retaining Comrak's arena as the block machine would reintroduce
the representation problem this spike avoids.

So the promising hybrid is a single Flark-owned runtime with deliberately
ported algorithms at block and inline module seams. Whether Pulldown wins the
inline seam remains unproved; the next smallest falsification is one oversized
paragraph with emphasis, code spans, links/references, and GFM autolinks,
resolved incrementally into direct facts without constructing either donor's
general AST.

