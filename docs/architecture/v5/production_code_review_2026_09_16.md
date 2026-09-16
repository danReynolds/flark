# Production code review — 2026-09-16

Dedicated source review of `50e13d4..a252c5a` (the production audit and hardening
commits), including Fleury's prepared-image API at the pinned `ffb6d22` revision.
This is a further review by the implementing assistant, not independent peer
review. CI remains skipped at the owner's request.

The review found two correctness bugs. Both were reproduced before correction,
fixed, and covered by regressions that exercise the resulting behavior.

## Findings resolved

### P2: EXIF rotation must precede thumbnail sizing

Both image preparation paths used stored header dimensions to choose the
thumbnail size. JPEG decoders apply orientation metadata, so those dimensions
can differ from the actual pixels. An 800×600 JPEG rotated clockwise became
600×800 natively, exceeding the 640-pixel height limit. A rotated 1600×900 JPEG
was stretched to 960×540. The browser path also stretched smaller rotated images.

Native preparation now sizes the decoded first frame. Browser preparation first
obtains the oriented bitmap, then asynchronously resizes that bitmap if needed.
Both bitmap handles close on completion, failure or cancellation. The queue and
its cancellation/backpressure contract are unchanged. Header inspection still
precedes decoding; this is not a claim of process-level memory isolation.
The browser behavior follows the [ImageBitmap API's orientation and resizing
semantics](https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html).

The new portable tests run in the VM and Chrome. Three input sizes verify output
dimensions, the prepared PNG's dimensions, and left/right pixel colors after
rotation. They catch incorrect aspect ratio and omitted/double rotation, rather
than merely asserting that an image was produced. All three pass on both targets.

### P2: missing-cell preparation discarded formatting at the live limit

For a three-column table ending in `| x |`, first insertion into an omitted cell
privately adds delimiters before executing the edit. If that intermediate source
crossed the live-render admission limit, it switched to source mode too early.
Pending bold, italic and inline-code formatting were lost.

The delimiter preparation now remains a private live snapshot; the completed
semantic edit decides admission through the usual commit path. This permits one
transient parse of the already-admitted table plus the necessary delimiter
prefix, even if that small expansion crosses the live limit. The writable source
limit is checked before preparation, and normal final admission and validation
remain in force. Only the completed edit is published or recorded in history.

Tests cover each formatting style, one notification, exact Undo restoration,
and restoration of pending style. Fault injection verifies both extraction
deviation refusal and unexpected parser exceptions: the original snapshot,
virtual caret and history survive, and a subsequent valid edit succeeds.

Link/image insertion that would enter source mode is deliberately refused by
the existing resource-validation policy, including in explicitly written cells.
The review initially suspected that refusal too; comparison with explicit cells
showed it was intentional. A regression confirms identical refusal and rollback
for both written and omitted cells. That policy was not changed.

## Other reviewed paths

- Parser definition-offset binary searches and previous-sibling indexes were
  checked against the old boundary behavior, including multiline definitions
  and split text runs. No additional actionable finding was identified.
- Virtual cell identity was traced through pointer placement, Tab/Shift-Tab,
  arrows, Return, source mode, composition, resource lookup, history and Flutter
  semantics. A diagnostic exercised 264 admitted combinations of inline
  formatting, escaping, Unicode, optional pipes and LF/CRLF without finding
  another incorrect mutation or Undo result.
- Both hosts' resource presenter lifetime guards were checked for controller
  replacement and stale completions. Their existing local regressions pass.
- Fleury's prepared PNG cache invalidation, prepared/plain replacement,
  immutable-input contract and animated-source refusal were inspected. No new
  framework change was needed. Its 1,309-test evidence remains the earlier
  exact-revision run, not a new run during this review.

## Final local verification

The [receipts](receipts/production-code-review-2026-09-16/) retain red/green
regressions, complete suite logs and hashes of the reviewed production sources.

| Check | Result |
| --- | --- |
| Kernel analysis / full tests | Clean / 776 passed |
| Flutter analysis / full tests | Clean / 815 passed |
| Fleury analysis / full tests | Clean / 98 passed |
| Portable image regressions in Chrome | 3 passed |
| Focused table regressions | 34 passed |
| Fleury release web build | Passed; `/revisions/0fb322303ffa/` |

A temporary browser tab exercised the built candidate at 1280×720 through real
pointer/key/paste input. A rotated 1600×900 JPEG displayed at a portrait 9:16
ratio. Clicking the omitted third cell, toggling bold and typing `Z` produced
`| x | | **Z**|`; Undo restored the exact `| x |` source. Shift-Tab then typing
`Y` produced `| x | Y|` in the middle cell. The final table/image frame was
visually inspected. This journey does not exercise the live-size threshold;
the direct kernel regressions do. It also does not establish a frame budget.

## Status and remaining qualification

No unresolved actionable finding remains from this dedicated review's scope.
It is still not independent peer approval or a production-readiness declaration.
The earlier native table-reflow p99 of 19.445 ms misses the unchanged 16.667 ms
budget. Foreground accessibility activation, physical IME/device and lifecycle
qualification, terminal presentation costs and a real-consumer soak remain as
recorded in the [hardening report](production_hardening_2026_09_16.md).

The implementation and review fixes are local pending publication approval;
the already-merged Fleury API does not imply these Flark commits are merged.
