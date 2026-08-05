# Bounded exact presentation-output gate

Status: **GO as a semantic/lifetime gate; HOLD as the final production codec and composite output root.**

## Contract proved

- A mounted host/layout identity is revision-independent and carries no semantic authority.
- Semantic facts are owned by one non-clone arena lease and one immutable manifest.
- Adoption is atomic across source revision, grammar revision, parse generation, composite semantic-root generation, request identity, request scope, queried range, and requested authority dimensions.
- The composite semantic-root generation must advance for lazy reference-resolution changes even when source and block-parse generations do not.
- Authority completeness distinguishes an exact empty dimension from a dimension that was not produced. A query asking for several dimensions exposes none unless all are certified.
- Only `Viewport` and `ActiveEdit` requests exist. Budgets are hard-clamped to 8 pages, 584 facts, and 32,864 total arena payload bytes.
- Facts are scalar tags, IDs, capabilities, and source-relative ranges. No fact or manifest owns source text, a Crop root, child vectors, or transformed strings.
- Cap overflow clears the bounded in-memory candidate and produces one `PresentationUnknownRange`; it allocates no partial arena output.
- Packed fact pages are never independently renderable. Query begins at the manifest, validates the entire page chain/count/byte total, then returns facts.
- Build ownership uses the same `ArenaBuildTransaction` rollback boundary as the persistent sequence. Manifest retirement uses the arena's strictly fuelled queue.

## Fact surface

The proof codec round-trips compact forms for:

- inline hidden ranges;
- replacements by interned symbol ID;
- style ranges;
- ambiguity ranges;
- run edges;
- table, fence, and task interaction targets; and
- command capability masks.

These are data facts from an exact parser. This module contains no Markdown transitions or prediction rules.

## Receipts

`cargo test --test presentation -- --nocapture` passes 6 focused tests.

The dense cross-dimension witness stores 146 facts in two completely full 4 KiB pages:

- fact-page payload: 8,192 bytes;
- immutable manifest: 96 bytes;
- total retained arena payload: 8,288 bytes;
- retained payload per fact including manifest: 56.77 bytes;
- maximum individual page payload: 4,096 bytes.

The page boundary deliberately splits one logical inline-hidden/command-capability pair. The matching composite query receives all 146 facts; stale or partially certified queries receive only `Unknown`, never one side of the pair.

## Production boundaries still open

1. The 56-byte fixed-width manual codec is proof scaffolding, not the selected production representation. Production should generate encoders, decoders, validators, and fuzz cases from one schema. Delta-coded anchors and fact-family pages should be measured against this exact baseline.
2. Block-structure authority is owned by the record forest, not this standalone presentation leaf. The selected composite root must own both the matching forest and presentation manifest so the scalar semantic-root binding cannot be mis-issued by integration code.
3. The generic tags prove transport sufficiency, not product completeness. Existing editor/Flutter tests must name the exact tag vocabulary and urgency policy before freezing the schema.
4. Host stability is proved as a lifetime split, not as Flutter behavior. Mounted-key, selection, IME, layout-cache, and source-paint fallback behavior still need runtime tests.
5. Reference updates must have one enforced semantic-root-generation source. If different subsystems can mint or forget that generation, exact adoption is not proven.
6. Query is bounded by the hard cap but currently decodes the whole request manifest. Production can add a tiny per-page range summary only if device measurements show this bounded scan matters.
