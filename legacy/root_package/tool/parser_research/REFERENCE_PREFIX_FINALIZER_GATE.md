# Reference-prefix finalizer gate

Status: **HOLD — resumable recognition, shared label semantics, actor work
lifecycle, and pre-work checkpoint chronology are green; authenticated writer
root publication and the real v3 join are not yet green**, 2026-07-18.

This gate is the reference-definition proof for the selected Flark-owned block
controller. It complements
[`PARSER_CONTROL_RENDEZVOUS.md`](PARSER_CONTROL_RENDEZVOUS.md) and the shared
[`REFERENCE_LABEL_NORMATIVE_GATE.md`](REFERENCE_LABEL_NORMATIVE_GATE.md).

## Current executable boundary

The scanner/control half is now green: the shared label service, inline lookup,
fuelled block scanner, multi-output rearm, terminal resume, projected CRLF/tab
metrics, duplicate ordering, malformed-title fallback, and direct parser
chronology all pass their focused suites. A real lookahead defect that consumed
the next definition's opening `[` was fixed by retaining one authenticated
replay unit; item acknowledgement now explicitly returns `Rearmed` or
`Complete`.

The parser request now also seals the interrupted terminal chronology as
`ParagraphFinalization` or `SetextCandidate`; the actor can execute that typed
policy but cannot derive a different mutation from scanner output. The focused
reference rendezvous is 7/7 green and the complete block-core suite is 176/176
green after this change. This proves the control distinction, not the writer
mutation or final publication join.

That is not yet integrated reference support. The exact v3 driver still maps
the parser's `ReferencePrefixFinalizer` request to a typed
`ReferenceExternalWork` failure. The remaining join must close all of these
authority gaps together:

- the parser request must carry an actor-minted opaque cursor bound to the live
  Paragraph, source snapshot/revision, logical projection root/end, and pending
  terminator rather than a caller-supplied identity plus arbitrary source
  trait;
- the candidate writer must consume each one-shot occurrence capability into
  an ordered occurrence root and first-definition winner root before minting
  the item acknowledgement;
- the terminal acknowledgement must join the selected Paragraph mutation,
  green/source/projection/reference roots, request/work lineage, and parser
  resume point; and
- the exact end-to-end test must inspect those candidate roots and prove that
  cancellation, staleness, crossed work/source, duplicate use, and wrong
  projection provenance publish nothing.

The current dependency crate exposes public convenience `acknowledge` methods
because it is an isolated control proof. Those calls are not the production
authority boundary. The selected production extraction places parser control,
actor rendezvous, and candidate writer in one Flark-owned crate so the actual
constructors are module-private; the cross-crate prototype must wrap them and
prove the same end-to-end flow before this gate turns green.

## Required control chronology

The block controller does not scan for definitions when it merely sees a `[`.
It accepts ordinary Paragraph lines in normal CommonMark priority, retaining a
compact possible-prefix hint and a writer-owned logical-projection anchor.
Immediately before Paragraph finalization or Setext promotion it raises one
typed `FinalizeReferencePrefix` request.

That request has one actor-owned lifecycle:

```text
parser request
  -> non-cloneable logical-cursor work
  -> poll
  -> one occurrence output token
  -> candidate writer acknowledgement
  -> same work rearms
  -> ...
  -> terminal token
  -> reference/green/source/projection join
  -> parser resumes exactly the interrupted grammar stage
```

Publishing an occurrence must not drop or recreate the DFA. No second parser
request is raised for each definition. The parser cannot resume on an item
acknowledgement; only the terminal join resolves its original request.

Ordinary line-boundary checkpoints must retain the compact possible-prefix
hint and projection anchor across an arbitrarily long Paragraph. A live work
cursor or unacknowledged output token is not a quiescent checkpoint. The first
production slice may recover those by replaying from the last pre-work
checkpoint; a later mid-work checkpoint must occur only after an item ack and
must bind the exact occurrence/candidate roots plus DFA scalar continuation.

## Source and output authority

Input is a writer-authenticated `LogicalProjectionCursor`, not a contiguous
physical range. Quote/list prefixes, tabs, CRLF normalization, and source-zero
projection operations make `base + logical offset` invalid provenance.

Each occurrence contains only:

- one bounded, spec-normalized label key;
- logical cuts for the full definition, raw label, destination, and optional
  title;
- deferred clean-destination/title transforms; and
- request/work/source/sequence bindings.

Destination and title payloads remain source-backed. The work retains no
Paragraph `String`, destination/title `String`, definition vector, or donor
tree/reference map.

The terminal result distinguishes:

- no definitions: replay the unchanged Paragraph once;
- reference-only: apply the parser-selected terminal mutation —
  `RemoveWrapper` on ordinary/EOF close or `RetainEmptyForSetext` after Setext
  priority has already recognized an underline — while retaining exhaustive
  physical coverage and ordered occurrences; and
- visible remainder: remove the accepted prefix, retain a Paragraph beginning
  at the last committed projection cut, and replay donor lookahead exactly
  once.

The writer publishes ordered occurrences and a separate first-definition
winner aggregate. A trace vector or test-only counter is not that root.

## Normative adjudications already discovered

Donor parity is evidence, not authority. Red-team comparison found at least
these pinned Comrak 0.54 divergences:

1. **Label bound and whitespace normalization.** Comrak counts bytes and admits
   1,000; the profile permits at most 999 Unicode code points. Comrak also
   collapses a broader whitespace set. The shared Flark label service owns
   both block and inline behavior.
2. **Invalid second-line title fallback.** For
   `[foo]: /url\n"title" ok`, trailing junk invalidates the title but not the
   destination-only definition. The occurrence must have no title. Comrak and
   the first DFA draft retained the invalid title, contrary to CommonMark
   0.31.2 Example 210's stated semantics.
3. **Escaped line ending in an angle destination.** A line ending is forbidden
   inside `<...>`. Backslash may escape ASCII punctuation; it cannot make CR or
   LF legal. `[x]: <u\\\n  v>` is not a definition even though Comrak accepts
   it.
4. **Projected character metrics.** The 999-character limit counts physical
   source code points, not canonical logical units. A CRLF-backed LF contributes
   two; the first logical space of a projected tab contributes one and its
   continuation spaces contribute zero; true synthetic units contribute zero.

Each disagreement becomes a pinned profile fixture. Donor differentials must
exclude or explicitly expect the adjudicated result; they may never redefine
the fixture by majority vote.

## Executable acceptance matrix

The gate turns green only after one integrated implementation proves:

- zero, one, and many definitions with one-at-a-time publication and terminal
  resume;
- duplicate occurrences in source order and first-definition winner identity;
- no-definition false candidates and exactly-once ordinary replay;
- reference-only `RemoveWrapper` and `RetainEmptyForSetext` mutations, plus
  visible-remainder canonical normalization, including `[x]: /u\n---` and
  `[x]: /u\n===` at nested and EOF boundaries;
- Paragraph-finalization, EOF, and pre-Setext chronology;
- up-to-three-space indentation, multiline destination/title forms, malformed
  title fallback, blank-line boundaries, and angle/bare destination rules;
- the shared 999-codepoint/normalization matrix, including raw CRLF and
  projected-tab contribution plus Unicode-whitespace counterexamples;
- one-byte UTF-8 and projection refills, giant destination/title values, and
  bounded label-only retention;
- nested quote/list projections with no synthesized physical range;
- cancellation at every scanner/output/writer/terminal boundary;
- stale, crossed-work, crossed-source, duplicate, and post-cancel
  acknowledgements failing before publication;
- checkpoint/resume while a multiline candidate Paragraph is still open;
- clean parse versus every-fuel/every-refill output equality; and
- the package's selected CommonMark/GFM fixtures plus mutation/fuzz cases.

The exact v3 test must inspect the published green/source/projection/reference
roots. Merely observing that `ExactBlockJob` stopped returning
`DirectUnsupported` or `ReferenceExternalWork` is not a passing result.

## Stop conditions

Stop and reopen the controller design if reference support requires any of:

- an actor-side Markdown classifier;
- a reference-specific parser driver lifecycle that Table cannot share;
- aggregate Paragraph materialization or a definition vector;
- caller-authored absolute source ranges or normalized labels;
- a public method that fabricates a writer acknowledgement without a candidate
  root receipt; or
- checkpoint prohibition for the whole potentially unbounded Paragraph.

Passing this gate through the shared request/work/output-token/ack/terminal
protocol is positive evidence for a Flark-owned donor-correspondent controller.
It is not evidence for keeping Comrak's runtime state model.
