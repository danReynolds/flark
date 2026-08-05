# BlockId lookup caller audit

Status: **current product/API audit**, 2026-07-16.

The Euler and hierarchical-green challengers exposed a hidden assumption in
the flat record forest: that production needs arbitrary persistent
`BlockId -> record/tree position` lookup.  That assumption materially changes
the representation result, so it must be justified by real callers rather
than by implementation convenience.

## Current finding

No current public Flark API or v2 behavioral test requires an arbitrary
document-wide parser `BlockId` lookup.

The v2 Flutter implementation has UI-local rendered-block IDs for host
reconciliation, but those are assigned and searched inside the currently
materialized entry list.  They are not a public parser-node lookup contract.
RFC 023 requires stable IDs in deltas and semantic targets, but its product
operations begin from a source position/range, a visible viewport, a parser
continuation, or a reference occurrence.  None inherently requires searching
the semantic document by ID.

This does **not** prove that a global directory is unnecessary.  It changes
the bakeoff rule: a candidate without one must demonstrate every internal
caller below using exact source/path capabilities, and a new global directory
must name the caller that earns its retained and update cost.

## Caller inventory

| Caller | Required starting capability | Clean resolution without arbitrary ID lookup |
| --- | --- | --- |
| viewport materialization | current byte/UTF-16 range | descend source/subtree metrics, then iterate bounded neighboring leaves; emit stable IDs found in those records |
| tap, caret, selection, command context | current source position and affinity | total coverage/source descent returns owner, enclosing path, markers, targets, and capabilities |
| parser changed-region build | restart source boundary plus parser open path | builder retains typed current-root cursors for the open path and newly emitted handles |
| convergence | mapped old/current physical-line boundary | compare typed continuation and stable open bindings already present in the two restart states |
| suffix attachment | source-derived split boundary | persistent sequence/tree split capability comes from the mapped boundary, not from searching a block ID |
| list/item aggregate propagation | parser open ancestry | update the changed child contribution along the retained builder path |
| detach, promote, reparent | typed parser event handle/path | the transient write-only sink maps revision-local handles to current candidate cursors; no committed-tree search is permitted |
| active inline/presentation request | visible/active source range | source descent locates the inline-bearing block; its stable ID keys the bounded fact cache after it is found |
| Flutter host reconciliation | stable IDs carried in a local delta | the mounted/overscan host table is viewport-bounded; host identity is not a semantic lookup service |
| semantic target action | revision-scoped target plus exact source anchor/range | validate the target against the current source/semantic query at its anchor; stale targets fail closed |
| reference presence invalidation | occurrence/dependency index entry | retain a stable coverage/source cursor and leaf ID in the occurrence; schedule by cursor, then validate the discovered leaf ID |
| sampled restart after later edits | sampled source coverage anchor and typed state | map the source anchor through edit lineage and reconstruct the current structural path by source descent |
| binary delta validation | IDs and relationships inside one bounded delta/page | use a request-local bounded ID table; this does not justify a retained document-wide directory |
| export/find/full traversal | root capability | ordered traversal; these cold operations already visit document content |

## Necessary stable identities

Removing arbitrary lookup does not remove stable identity.  The following IDs
remain distinct and appear in deltas, caches, or target validation:

- source revision and parser/semantic generation;
- stable coverage/source unit;
- stable semantic block;
- semantic subtarget derived from block identity plus typed local identity;
- reference symbol and occurrence;
- presentation request/fact lease; and
- Flutter host/layout/input leases.

An unchanged block must keep its stable ID across exact suffix reuse even if
the immutable page/node version containing it changes.  A bounded local page
repack may update locators for records on that page; it may not rewrite the
distant suffix.

## Cases that would require a global directory

A persistent BlockId directory becomes justified if Flark adds or discovers a
real requirement such as:

- a public API that accepts an arbitrary long-lived parser block ID without a
  source anchor;
- offscreen random semantic mutation addressed only by block ID;
- a reference/dependency representation that stores only leaf IDs and cannot
  carry stable source cursors; or
- a chosen structural encoding whose own normal update algorithm cannot retain
  exact path/boundary capabilities.

In those cases the directory is a derived current-root index, not a second
semantic authority.  Its packed bytes, page-split locator updates, root
navigation metadata, transaction ownership, and stale-generation rejection
must be included in the representation receipt.

## Gates for an indexless candidate

Before accepting a representation without arbitrary lookup, prove:

1. every product query in the table starts from a bounded source/path
   capability in the real integration, not an oracle absolute rank;
2. reference presence changes can enumerate and schedule offscreen consumers
   from stable occurrence cursors after prefix edits;
3. restart samples beyond a changed prefix recover their current open path
   without retaining an old semantic/source root;
4. detach/promotion/reparent handles remain valid across candidate page splits
   and fail closed after cancellation;
5. semantic actions with an old ID and current-looking source range are still
   rejected by revision/generation validation; and
6. Flutter never expands its mounted-host map into a document-wide semantic
   cache merely to compensate for the missing worker index.

Until those tests exist, arbitrary lookup is **not a required operation and
not proven unnecessary**.  The representation bakeoff must report both
variants rather than silently charging or omitting the directory.
