# Scheduling and pathological-input contract

Status: working correction to the Gate A/B research assumptions, 2026-07-15.
The normative fixture/profile requirements remain unchanged. The byte-fuel and
giant-leaf mechanisms below must be reconciled before claiming either gate.

## Product invariant

The UI thread never waits for Markdown grammar work. A source edit, caret, and
IME composition become visible immediately. Derived presentation is adopted
only for the exact source revision it describes.

The worker may return one of three honest states for a changed region:

1. current authoritative facts;
2. an old presentation that the parser has proved remains valid; or
3. exact source-visible presentation while current facts are pending or the
   region exceeds a certified enrichment limit.

It may never return a guessed Markdown classification. Source-visible is a
resource policy, not a second parser.

## Byte fuel is accounting, not a semantic boundary

The original Gate A/B contracts require every poll to examine no more than
4 KiB. The bounded Comrak facades and inline service currently admit up to
8 KiB atomically. Calling an 8 KiB helper after spending or reporting only
4 KiB would be dishonest; pre-scanning and then rereading bytes without
charging both passes is also dishonest.

Two implementation models are valid candidates:

### Streaming at 4 KiB

Every operation above 4 KiB becomes an explicit continuation. This preserves
the current gate verbatim but expands donor code and state, including scanners
whose complete 8 KiB call measures only tens of microseconds on the research
host.

### Preflighted atomic kernels

Before an atomic helper starts, the scheduler knows its exact or maximum input
length and grants enough remaining budget for the entire call. The receipt
charges all inspected, copied, allocated, emitted, and hashed bytes. The
kernel has a hard input/output cap and is certified by native and Wasm
wall-time p50/p99/p99.9 measurements on floor devices. If the grant is not
available, the job yields before entering the helper.

This model changes the gate from “4 KiB under every circumstance” to “bounded
streaming work plus explicitly listed atomic kernels.” It does not permit an
unreported overrun. The product deadline, dedicated worker, cancellation
latency, and measured worst-case duration decide the maximum grant. The 8 KiB
research cap is a candidate, not a launch constant.

The simpler atomic model should win when floor native/Web Worker evidence
keeps a full kernel comfortably below the parser scheduling deadline. The
streaming model should win for a helper or platform whose tail does not.

## Separate limits by purpose

One threshold cannot simultaneously describe interaction urgency, worker
correctness, and pathological resource protection. The production profile
needs separately calibrated limits:

- **urgent enrichment limit:** facts likely to arrive before the next paint;
- **worker exact-enrichment limit:** a larger bounded leaf that may complete
  over subsequent frames without blocking input;
- **pathological enrichment limit:** beyond this, retain exact block/source
  structure but keep the inline/layout region source-visible;
- **physical-line atomic limit:** maximum line accepted by a one-call lexical
  helper before its exact streaming classifier is used; and
- independent output, fact-density, origin-run, dependency, nesting, and
  allocation limits.

Threshold crossings change presentation/scheduling, never the Markdown
meaning of surrounding structure.

## Large documents versus giant constructs

“A 10 MiB document is fully live” means a 10 MiB document composed of ordinary
certified lines, leaves, and layout shards remains locally editable and
authoritatively styled. It does not imply that a single 10 MiB token-dense
paragraph must allocate and render tens of millions of inline facts.

The old Gate B adversary requires exact incremental inline parsing of one
10 MiB leaf under 4 KiB polls. That remains useful stretch evidence, but it is
stronger than RFC 023's launch premise and would force a much larger inline
implementation than the bounded Comrak service. For launch, the giant-leaf
contract should instead require:

- exact source preservation and editing;
- exact surrounding block/container state;
- a bounded, explicit source-visible inline/layout treatment;
- no guessed partial emphasis/link/code facts;
- cancellable background enrichment only when a separately certified larger
  path exists; and
- recovery to authoritative styling as soon as the leaf returns below the
  certified limits.

The normative inline corpus and editing histories must still be exact for all
supported ordinary leaf sizes. A small document cannot hide a correctness bug
behind the pathological policy.

## Oversized physical lines still require exact block effects

Inline degradation cannot make block state approximate. A large fence closer,
HTML terminator, thematic/setext candidate, table delimiter, list prefix, or
reference-definition candidate can change later block interpretation.
Oversized-line classifiers therefore retain exact, resumable scalar state:

- fence character/count and whitespace-only tail;
- HTML class-specific terminator matcher;
- thematic/setext marker count and invalid-character state;
- table row/cell count, escape/code-boundary state, and alignment validity;
- bounded reference label plus destination/title terminator state; and
- indentation/container-prefix state.

Once ordinary prose is known not to interrupt or close the current block, the
block engine may extend a source range without running inline grammar.

## Revised executable acceptance

Before architecture selection, the gate harnesses should be split into:

1. semantic/profile conformance, unchanged and size-independent within the
   certified profile;
2. exact clean-versus-incremental block/reference/inline histories;
3. bounded atomic-kernel declarations and external wall-time receipts;
4. streaming oversized block-classifier receipts;
5. exact source-visible giant-inline/layout fallback receipts; and
6. native/Web Worker parser-to-paint traces with supersession, cancellation,
   IME, paste, undo, and revision-safe adoption.

Passing self-reported byte counters alone is insufficient. The final caps come
from external timing, allocator/RSS, and physical floor-device evidence.

