# Storage checkpoint authority mechanism

Status: **storage/source boundary mechanism in progress; parser checkpoint and
candidate binding HOLD**, 2026-07-16.

This slice closes the raw-offset authority hole in the Stage-0 source-lineage
jobs without pretending the packed green manifest already has a restart index.

## What this slice may prove

A storage boundary mechanism can bind all of the following to one decoded and
revalidated `SerializedGreenDocument`:

- the arena-scoped manifest identity;
- exact base source revision, root, byte length, and UTF-16 length;
- syntax profile, grammar revision, parse generation, and semantic epoch;
- a UTF-8 scalar boundary that is also a physical-line start;
- an explicitly biased observation of the Coverage run immediately before or
  after the source coordinate.

Scalar and line-start validation is bounded. Crop answers the scalar-boundary
query from its persistent tree. The resolver reads at most the adjacent byte on
each side to distinguish LF, lone CR, and the forbidden cut between CR and LF.
It never scans from the beginning of the document to recover a line number.

The storage resolver does not use the generic green `seek` cursor. It descends
source-metric summaries with scalar route state and scans exactly one bounded
leaf through the same canonical scalar event decoder used by `decode_leaf`.
There is no second tag/facts/logical-contribution codec to drift when the packed
format changes. The resolver neither constructs a decoded-event vector nor
reconstructs a structural open path. One decoded `Enter` can temporarily own a
bounded inline-facts allocation; the receipt reports the maximum logical bytes
of that transient allocation separately from inline scanner state. The returned
observation retains zero heap bytes. Parser-state recovery remains a separate
future checkpoint-root operation.

A source coordinate is not a unique green sequence cut. Zero-metric `Exit` and
`Enter` events may lie between `AfterPreceding(coverage A)` and
`BeforeFollowing(coverage B)` at the same byte/UTF-16 coordinate. The mechanism
therefore persists the chosen adjacent-Coverage side from its private harness
permit but exposes `has_sequence_cut_authority() == false`. The adversarial
fixture demonstrates both observations at one source cut with structural
events between them. A future checkpoint-index entry must persist and
revalidate the exact event-side sequence cut; source descent may corroborate
it, but may not choose it.

The mechanism produces distinct, non-cloneable restart and convergence role
wrappers. Those wrappers are the only inputs accepted by the high-level
restart-selection and one-pass adoption-lineage adapters. The adapters derive
the Stage-0 descriptor and byte cuts internally and recheck the current
`SourceStore` descriptor before every poll and before consuming a proof.

## What it deliberately does not prove

An arbitrary scalar/line/source-run boundary is not a Markdown parser
checkpoint. The current serialized-green manifest has no restart-state root,
checkpoint sample entry, physical-line ordinal, control state, semantic state,
or parser open-path commitment. `seek` therefore cannot mint a production
`StoredRestartCheckpointCapability` or
`StoredConvergenceCheckpointCapability`.

Until that storage exists, executable role wrappers are gated by a
crate-private linear proof-harness permit. There is no public constructor and
no public API accepting a raw byte offset or event locator. The future
restart-index resolver must mint the permit from an actual manifest entry; a
caller-supplied integer must never become that entry.

This slice also does not define `CandidateStartBinding` or
`CandidateBoundaryCapability`. Those require one actor-owned operation that
combines:

- a consumed restart selection;
- the sole current Crop parser cursor seeded at the nonzero restart;
- exact retained-prefix coverage and incremental source-ledger metrics;
- the active `LiveCandidateEpoch`, base-output lease, and arena build; and
- parser control/semantic state restored from the stored checkpoint.

Creating those types from separately supplied `SourceStore` and epoch values
would be caller composition, not actor authority. They remain HOLD until the
nonzero cursor/ledger integration is implemented inside `LiveDocumentStore`.

## Required next storage root

The packed candidate manifest needs a restart/checkpoint root whose entries
are resolved by source-directed descent and bind at least:

```text
base manifest/source identity
physical line ordinal and scalar byte/UTF-16 cut
prefix/suffix sequence cut
syntax profile + grammar revision
parser control state + semantic restart state
parser open-path commitment
projection reset capability (distinct role)
```

Restart and convergence entries should remain role-distinct even if they share
one encoded record format. A projection reset or semantic-envelope end may be
joined with a checkpoint entry, but neither is independently sufficient to
authorize restart or suffix adoption.
