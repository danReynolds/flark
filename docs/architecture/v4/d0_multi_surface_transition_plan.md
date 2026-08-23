# D0 bounded multi-surface transition plan

**Status:** implementation contract for `SYNTAX-10`

**Owner:** [D0 dogfood milestone](../../../DOGFOOD_MILESTONE.md)

**Scope:** the frozen fenced-code construction journey only

## Problem

The current pre-edit dependency authority produces one presentation row. That
is sufficient for literal projection, local exact islands, and simple block
prefixes, but it cannot represent the clean parser result of a fence boundary.

For the frozen source `change this line\n\n**sentinel**\n`:

- after the third opening backtick, the clean parser replaces the two Plain
  rows with one unclosed fenced `CodeBlock` row whose visible content begins
  after the opener/info line; and
- after the third closing backtick, the clean parser replaces that unclosed
  row with a closed `CodeBlock` row followed by a separately projected Plain
  row whose `sentinel` content is Strong.

The transition therefore changes row partition, block shell, visible content,
and inline facts together. Retaining the predecessor rows, exacting the whole
viewport, or waiting for a later parser refresh all violate the frozen actual-
paint result.

## Selected seam

D0 adds one generic parser-authored **bounded pending-presentation plan**. It is
not a fence matcher in Flutter and it is not another controller state slot.

The plan contains:

1. one exact insertion sequence of at most eight ASCII bytes;
2. the exact zero-width trigger and activation prefix length;
3. the base affected source range;
4. one activation-result snapshot containing one to four ordered result rows;
5. each result row's byte/UTF-16 source and editable ranges, typed block
   presentation, and complete authoritative inline-fact set; and
6. the predecessor row ordinals replaced by that result snapshot.

All result coordinates describe the activation result revision. If the parser
declares additional bytes after activation, Core may advance only the exact
remaining sequence and transforms the result rows/facts mechanically through
those insertions. It may not add a row, fact, style, delimiter rule, or shell.

The existing `FlarkPendingPresentationSnapshot` remains the only host pending-
presentation state. Its dependency variant owns an ordered list of framework-
neutral rows instead of assuming exactly one. Structural receipts remain a
different post-commit input but materialize the same row-list publication and
use the same retirement/fresh-certification lifecycle.

## Layer ownership

- `flark-parser` selects the exact prefix, trigger, activation point, affected
  dependency range, result row partition, shells, and inline facts from a
  bounded clean counterfactual parse.
- `flark-runtime` validates revision/source ownership, maps byte and UTF-16
  coordinates, enforces caps, and omits the optional plan on any ambiguity.
- the ABI transports the generic plan and result snapshot. The record is not
  named for fenced code and introduces no syntax-specific query kind.
- Core matches the exact declared sequence, advances declared coordinates,
  and materializes typed result rows. Core does not classify Markdown.
- Flutter publishes those rows through `_pendingPresentation`, suppresses only
  the parser-declared predecessor ordinals, and maps selection/caret through
  the supplied source runs.

## Bounds and fail-closed rules

The first contract is deliberately small:

- the affected current source is at most 16 KiB and fully materialized;
- the exact sequence is 1–8 ASCII bytes;
- one row publishes at most four result rows and 128 total result inline facts;
- result rows are ordered, nonoverlapping, source-contained, and collectively
  cover the declared result affected range without partial fact sets;
- at most one plan may match an edit;
- optional plan bytes cannot evict ordinary current-revision inline facts from
  any row; and
- an unavailable, truncated, malformed, stale, ambiguous, nonmatching, or
  out-of-window plan falls back through the existing exact-source path.

The initial parser emitter is restricted to the frozen top-level BOF fixture:
the exact seven-byte opening prefix (three U+0060 BACKTICK bytes followed by
`dart`) and the matching three-backtick closer introduced at the semantic-
Return successor point. This is a bounded D0 denominator, not an assertion
that every arbitrary fenced block is covered.

## Rejected shortcuts

- No source/backtick scan in Dart or Flutter.
- No fence-specific controller state or precedence branch.
- No stale Strong fact retained while the clean parser classifies it as code.
- No whole-row or whole-viewport exact flash called a passing transition.
- No delayed notification that relies on native acknowledgement beating the
  next display frame.
- No ABI revision per opener, closer, or future syntax construct.

## Required proof

The implementation is complete only when:

1. parser tests compare every admitted prefix and carried successor with an
   independent clean parse, including complete row/fact partitions;
2. runtime/ABI tests prove caps, truncation priority, byte/UTF-16 geometry, and
   malformed-plan rejection;
3. Core tests prove unique matching, pre-activation and activated advancement,
   replaced-row ownership, revision binding, and fresh supersession;
4. the mounted `SYNTAX-10` test observes every actual paint through settlement
   and matches the clean result's source, caret, ordered rows, shells, exact
   ranges, and Strong/code styles; and
5. the existing simple-prefix, structural-Return, paging, and dense-document
   suites remain green.

The ABI minor carrying this general plan is the final D0 ABI. Any later ABI
change invalidates downstream D0 receipts and reopens architecture review.
