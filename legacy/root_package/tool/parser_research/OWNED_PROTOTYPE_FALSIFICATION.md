# Owned-prototype falsification audit

Status: temporary-probe evidence, 2026-07-14. This audits the current research
crate as a possible production seed. It does not reject Flark-owned persistent
parser state; it rejects treating the present clean-room mechanisms as though
they already compose into that core.

## Outcome

The product-shaped representation remains promising, but the current batch
parser, checkpoint/reference scanner, and unified slice are separate research
mechanisms. The production seed should be a Flark-owned, Comrak-derived
persistent parser core: preserve one mature grammar lineage while replacing
arena/pointer ownership, whole-line work, and copied output with native input,
value state, budgets, and persistent chunks.

## Work-budget falsification

A temporary binary applied a nominal one-line/64-byte `WorkBudget` to giant
single-line documents:

```text
bytes=1000000  elapsed_us=3542–4266  status=Converged
bytes=10000000 elapsed_us=37050–47631 status=Converged
```

`advance` currently always admits one whole line, and `SourceRope::line` then
materializes that line. The byte budget is therefore not a real cancellation
bound for giant paragraphs, table rows, HTML, or other long physical lines.
Production block and inline scanners need sub-line cursors and fuel checks.

## Dense-line memory falsification

`/usr/bin/time -l` on two million source bytes shaped as one million `a\n`
lines measured:

| Representation | Checkpoints/records | Maximum RSS |
| --- | ---: | ---: |
| `RevisionedDocument` checkpoint tree | 1,000,000 | 473,202,688 bytes |
| `UnifiedSliceDocument` line records | 1,000,000 | 209,387,520 bytes |

At 10 MB this shape is incompatible with the launch envelope. Checkpoints and
facts must be packed/adaptive rather than one heavyweight Rust object per
physical line. The density policy must be measured by both bytes and records.

## Stable-order falsification

Repeatedly inserting `x\n` before the same suffix exhausted the fractional
`u128` semantic order-key gap and panicked on the 307th insertion:

```text
semantic order-key space exhausted
```

A production persistent sequence needs relabeling, tree-native order, or IDs
whose correctness does not depend on finite midpoint space. The gate requires
at least 10,000 same-boundary inserts and long randomized histories.

## Semantic-composition gap

The current score is 343/652 CommonMark and 8/30 architecture-stress cases.
The missing behavior is concentrated in the mechanisms that decide the
architecture: lists 1/26, list items 8/48, HTML blocks 0/44, link definitions
6/27, links 14/90, and images 1/22.

The separate unified slice now demonstrates forward container state plus
setext/table promotion mechanics, but it is a bounded structural model with no
independent renderer oracle. Its claims do not raise the batch parser's score.
The reference slice proves a persistent dependency-index idea, not complete
Unicode/escaped/multiline CommonMark reference recognition.

## Production kill gates

The integrated seed must prove all of the following together:

1. Exact list/quote/tab/setext/table/HTML semantics and real persistent output
   from one machine, with no batch shadow or grammar-sensitive side scan.
2. Sub-line budgets and cancellation on 10 MB single-line constructs.
3. Packed/adaptive checkpoint memory under a measured 10 MB dense-line cap.
4. At least 10,000 same-boundary inserts and 100,000 randomized edit histories
   without key exhaustion or clean-oracle divergence.
5. Bounded stable-ID deltas inside giant lists, tables, HTML blocks, and
   paragraphs.
6. Complete Unicode, escaped, multiline, duplicate, and distant-reference
   behavior through the real block/inline machines.
7. Budget-independent native/WASM equivalence at every cancellation point.

These receipts are why the clean-room trial remains disposable evidence rather
than the production parser seed.
