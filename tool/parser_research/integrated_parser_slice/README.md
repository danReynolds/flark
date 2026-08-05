# Integrated parser commitment slice

## Status

**In progress. No architecture or parser commitment pass is claimed yet.**

This crate tests the remaining composition risk in RFC 023. The earlier
experiments independently showed that selected Pulldown inline algorithms can
leave `Tree<Item>`, compact state can fit the adversarial memory envelope, and
checkpoint restart can converge after one changed source page. Those results
do not prove that the mechanisms compose.

This slice counts as evidence only when one runtime owns the complete path:

```text
persistent UTF-8 source root and stable anchors
  -> fuelled block state
  -> persistent segmented inline leaves
  -> packed lexical/code/link/emphasis/reference state
  -> exact state plus shared-source convergence
  -> packed facts and reference dependencies
  -> persistent output/dependency roots
  -> one sealed transport delta and atomic root-set commit
```

There may be multiple implementation phases, but there may not be multiple
grammar authorities, independently materialized source strings, or adapter-only
composition between the block and inline paths.

## Initial exact syntax slice

The first meaningful vertical slice is intentionally selected for difficult
ownership boundaries rather than broad syntax scores:

- paragraphs, blanks, block quotes, bullet-list items, lazy continuation and
  tab virtualization;
- setext promotion and GFM table header/delimiter/body rows;
- text, escapes, code spans, emphasis/strong;
- reference definitions plus shortcut/full uses, retained misses and
  first-definition-wins symbols.

Unsupported syntax must return an explicit unsupported-profile result. It must
never call Comrak or Pulldown as a fallback on the edit path.

### Current block commitment receipt

`block.rs` now executes the narrow paragraph/quote/bullet/lazy subset directly
over `PersistentSource`. It seals bounded 32-leaf pages into a persistent
forest, emits marker-free `SegmentedLeaf` inputs, and composes one real table +
emphasis path through `SharedLexer` and `GrammarJob`. A 1 MiB physical line is
read one byte transition at a time; line-prefix scratch is fixed at 256 bytes,
and known out-of-profile constructs fail explicitly.

This is not yet the final dense block representation. Every paragraph still
allocates `Arc<BlockLeaf>` plus its own `SegmentedLeafBuilder`, and every leaf
retains a handle to the complete source root. The 10,000-leaf stress receipt
accounts those structures separately from packed descriptors. Retaining one
leaf demonstrably pins the whole source revision, so independent leaf eviction
does not yet compose cleanly.

Block polls now report source, prefix-probe, frame, descriptor, allocation,
copy, and retained-memory dimensions. Transition fuel does not preflight those
dimensions against a scheduler permit, however. The API therefore declares
itself not measured-scheduler-admissible; prefix classification has a fixed
4,096-unit atomic ceiling, but a 4,096-transition poll is not itself a hard CPU
slice yet. Tab-stop residuals are tracked exactly enough to classify partial-tab
indentation; the supported paragraph subset consumes them. Emitting residual
virtual spaces becomes live when indented code/HTML leaves are added.

## Composition pass

The slice passes only when all of these are simultaneously true:

1. Source and output edits are persistent and `O(log pages + changed bytes)`;
   no flat prefix/suffix copies or document-scale preallocation occur at job
   creation, commit, suffix adoption, supersession, or reclamation.
2. Block leaves and inline logical segments share stable source anchors. Quote,
   list and table separators have one owner, and virtual bytes never become
   invented physical marker ranges.
3. Real code/link/emphasis/reference algorithms use fixed packed pages and
   resume inside scanning, resolution, emission, sealing and exact state
   comparison.
4. A local edit may attach old output only after complete parser state and an
   explicitly shared source/segment suffix converge. Facts spanning that
   boundary remain exact; changing their opener prevents false convergence.
5. One candidate job, cancellation token, resource ledger, delta and atomic
   commit drive both the Gate A and Gate B projections.
6. Reference-value edits update one symbol without consumer churn. Presence or
   winning-definition changes retain misses and schedule dependent leaves under
   fuel without enumerating a worker-to-Dart object per leaf.
7. External RSS, allocator traffic, wall time and native/WASM behavior confirm
   the self-reported audit trace. The 96 MiB research threshold is a kill
   ceiling, not an end-to-end product budget.

## Known contract correction

GFM table pipes must be escaped even inside code spans. The earlier Gate A/B
histories used an unescaped pipe and accidentally described a Flark deviation.
The corrected histories use `` `c\|d` `` and require visible code content
`c|d`, following GFM table example 200.
