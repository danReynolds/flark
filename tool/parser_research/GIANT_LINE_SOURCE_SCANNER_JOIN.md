# Giant-line source/scanner join

Status: **ownership selected; source-backed donor composition HOLD**, 2026-07-18.

This note records the production-shaped seam selected by the giant physical
line investigation. It is a proposed architecture and proof plan, not evidence
that every Markdown scanner or the end-to-end editor already passes.

## Decision

Keep one donor-correspondent grammar machine for every line size. Replace the
owned `String` line boundary with a source-bound, progressively readable line
view. Generated lexical continuations are private substates of that grammar
machine; they never classify Markdown beside it.

```text
immutable Crop revision + physical-line index
          |
          | query-only typed line descriptor
          v
ledger-owned recognition byte cursor ---- shared UTF-8/CRLF metrics + digest
          |
          | bounded byte peeks, no claim capability
          v
donor-owned line transition
  CheckOpen -> OpenNew handler stages -> text/tail dispatch
                |
                | generated exact scanner substates
                v
          typed DirectCommand stream
                |
                | independent authoritative source replay
                v
       projection + packed green writer
```

The two source traversals have different authority, not different grammar:

1. Recognition lets the one parser inspect the immutable source and decide its
   branch. It cannot mint a writer boundary.
2. Authoritative replay validates the parser-issued typed ranges against the
   same source revision and is the only route into projection/green storage.

This preserves the source/writer trust boundary without retaining a giant line
or a source-proportional action recipe.

## Non-negotiable invariants

- Small and oversized lines execute the same grammar transitions and scanner
  artifacts. A contiguous small-window optimization may change source access,
  never rule ownership or results.
- Scanner rules are mechanically derived from pinned donor sources and checked
  against the donor facade. There is no handwritten large-line approximation.
- The parser owns context, rule precedence, semantic commit, and commands. A
  generated scanner returns lexical cuts only.
- A scanner receives the full typed source/build/line identity. A scalar
  `source_key`, offset, digest, or line descriptor grants no claim authority.
- Admission, each poll, cancellation admission, and retirement are independent
  of physical-line length.
- One actor poll has a composed budget covering source first reads, decoder
  transitions, scanner transitions/peeks, parser actions, and output draining.
  Bounded source delivery with an unmetered sink is insufficient.
- UTF-8, tabs, NUL, LF, CRLF, lone CR, BOM, and bare EOF remain byte exact. In
  particular, lone-CR lookahead cannot leak the next line into a scanner view.
- A physical line may suspend many times but becomes a restart/convergence cut
  only after its complete command/source transaction is acknowledged.
- Deadline or cancellation failure exposes exact current source or explicitly
  unknown presentation; it never publishes guessed syntax.

## Evidence already obtained

### Donor control seam

`comrak_value_block_core` now represents the `OpenNew` precedence chain as
explicit Start, block quote, ATX, fence, HTML, Setext, thematic break, list,
code, and table stages. Each transition invokes at most one existing handler
family. A `cfg(test)` copy of the former atomic short-circuit scheduler remains
an oracle only.

The new scheduler matches every `DirectCommand`, in-memory and encoded durable
pause, block kind, and fail-closed exit in focused tests. The 1,322-document
corpus, CommonMark 652/652 scorecard, and existing GFM authority-version result
are unchanged. This is the parser-owned suspension point required by generated
scanner substates.

### Generated lexical state

`generated_scanner_gate` mechanically extracts Comrak 0.54's ATX rule and uses
pinned re2rust 4.3.1 to produce a storable source-free DFA. It matches 20,000
random donor-facade lines across tiny fuel grants. A 10 MiB candidate yields
more than 2,500 times, inspects at most 4,096 source bytes per poll, and retains
zero source bytes in the scanner.

The raw `Vec` source host test completes in about 0.03 s release. The same DFA
now runs over the real Crop source shape in about 0.11 s on this host, with a
4 KiB Crop chunk. Its instrumented request stream never moves backwards; the
16-byte ATX test cache is therefore an upper-bound harness, not an observed
requirement. These numbers are mechanism comparisons, not floor-device SLAs.

A second mechanically extracted generated family now covers Comrak's
`open_code_fence` trailing-context rules. Valid and invalid 10 MiB candidates
produce an at-least-10 MiB **logical** marker/context restore, but the restore
is terminal and the physical source request stream remains strictly forward
with zero retained source bytes. This hardens the continuation contract:
scanner cursor/accepted cut and physical source high-water are different
coordinates, and `YYMAXFILL` is not a rewind bound. Every additional scanner
family still needs the same instrumentation because a nonterminal restore
could have different source-access behavior.

### Source lineage and decoding

`v3_runtime_slice` now pumps a source-bound recognition cursor in hard-capped
4 KiB windows, stops at the first physical line boundary, preserves byte and
UTF-16 metrics for 10 MiB ASCII and mixed-width Unicode, rejects foreign
epochs before invoking the sink, and poisons a partially advanced candidate if
the sink fails. The five release tests, including both 10 MiB fixtures, take
about 0.46 s together on this host.

That pump proves lineage, decoder, cancellation, and failure semantics. Its
scalar-atom callback is deliberately not selected as the production grammar
feed: decode/callback/re-encode work is avoidable, and its receipt does not
bound arbitrary scanner work.

The persistent `CommonMarkLineIndex` now also resolves a query-only exact
physical-line descriptor without scanning the line. Six focused debug/release
tests cover every ending, Unicode, edited revisions, invalid cuts, and a naive
250-edit oracle. Both 10 MiB BOF fixtures resolve with zero tree nodes visited
and zero boundary bytes scanned; later starts visit one index path and scan at
most one 4 KiB boundary leaf. The release suite takes about 0.04 s. The
descriptor carries no source lease or writer/parser authority.

The actor now joins that descriptor to the active writer's opaque recognition
checkpoint only at an untouched line start. The joined value binds the full
source descriptor, candidate build, line ordinal, start, content end, physical
end, and ending. A foreign epoch fails before the index is queried, and the
same request after one recognition atom fails closed. Four focused tests,
including a 10 MiB CRLF line with zero endpoint scanning, are green. This is
the correct admission identity for the byte-view work; it still mints no
claim or publication authority.

### Exact ATX output protocol

The donor and v3 writer now agree on the complete owned-line ATX transaction:
the opener is `BLOCK_MARKER/None`, visible content is `CONTENT/Identity`, an
accepted closing sequence is `BLOCK_MARKER/None`, non-closing trailing space
is `CONTENT/HiddenUpstream`, and the ending is `TERMINAL/None`. Final heading
level is present on `Open`, survives both pause forms, and reaches typed green
facts. Accepted close, non-close, empty heading, every ending, durable restart,
and packed projection are green. This proves the semantic/output contract;
the current execution still owns an 8 KiB-capped `String`, runs the reverse
tail helper atomically, and replays arbitrary ranges one atom/action at a time.

### Admission and cancellation

Direct recipe admission no longer reserves from physical-line length. Its
fixed/depth-only allocation request is identical for a one-byte and 10 MiB
line at the same open depth. Candidate cancellation and donor-checkpoint heap
retirement are cooperatively fuelled rather than hidden in actor calls.

## Selected production shape

### 1. Indexed physical-line descriptor

At an exact line-start recognition checkpoint, the source actor derives a
query-only descriptor from the persistent line index:

```text
source descriptor + build + line ordinal
absolute start + content end + physical end + ending kind
```

The lookup must be index-local (`O(log n)` plus bounded tree work), not a scan
to discover the end of a 10 MiB line. Wrong revisions, roots, builds, or
non-line-start offsets fail before a source byte reaches the parser.

### 2. Ledger-owned byte view

The existing recognition Crop cursor becomes the sole source reader for the
parser. It serves sequential first reads plus bounded repeated peeks from
scratch. Every first-read byte enters the shared decoder/digest state exactly
once. Scanner lookahead may be ahead of its semantic cursor only inside the
explicit bounded scratch contract. The source-owned physical high-water is
never inferred from a scanner's logical cursor after a marker restore.

If an intended generated scanner requires unbounded rewind, the architecture
does not retain an unbounded prefix to accommodate it. That scanner is emitted
as an exact push DFA or rewritten as another mechanically checked bounded
continuation.

### 3. Donor-owned scanner substates

Each significant handler stage may hold a generated scanner continuation. A
`NeedMore` result changes no grammar/output state. `Matched` commits through the
existing donor handler/action path; `NoMatch` advances to the next precedence
stage without replaying earlier handlers.

Generated lexical artifacts belong inside `comrak_value_block_core` (or an
unpublished leaf used only by it), not in v3. V3 owns source identity, decoding,
writer authority, and scheduling; the donor owns rule precedence, generated
scanner state, ATX-tail classification, semantic mutation, and commands. Letting
`ExactBlockJob` invoke a proof crate and report a classification back to the
donor would recreate the dual-authority design this work is replacing.

ATX opener recognition alone is not the full ATX transaction. Trailing closing
hash classification, hidden trailing whitespace, visible content, and the line
ending each need exact resumable cuts before oversized ATX is accepted.

### 4. Typed commands and authoritative replay

The parser emits a bounded typed recipe, usually ranges rather than per-byte
events. The writer replays and validates those ranges with its authoritative
cursor, derives exact projection metrics, and appends bounded packed pages.
Dense tables and references must page their output; accepting a line and then
allocating a source-proportional command vector is a failed gate.

## Remaining falsification gates

1. Complete the candidate-owned raw-byte session behind the now-actor-bound
   line descriptor. It must use the existing decoder/digest, carry exact
   physical UTF-16 metrics, reject crossed same-root lines, and resolve lone CR
   without leaking the next line.
2. Move the generated ATX artifact behind the donor-owned stage and prove no command
   or grammar mutation occurs while it is suspended.
3. Replace the atomic reverse ATX tail helper with its donor-owned, provenance-
   checked forward continuation and use the already-proven exact ATX command
   partition.
4. Generalize the existing bounded identity-line replay into exact range replay
   for `None`, `Identity`, and `HiddenUpstream`, so a 10 MiB range is a constant
   recipe and bounded polls rather than one writer action per scalar.
5. Compose source, decoder, scanner, parser, command, and replay fuel into one
   receipt capped at the actor's calibrated slice.
6. Replace `ExactBlockJob.recognition_line` and the 8 KiB hard rejection for the
   supported slice. No temporary complete `String`, scalar vector, or line-
   sized reserve may remain.
7. Differential-test full commands, source partitions, durable pauses, packed
   green/projection, restart, cancellation at every suspension, source identity
   substitution, and randomized fuel schedules.
8. Run 10 MiB single-paragraph, ATX/fence candidates, multi-block documents,
   latest-wins edit storms, native/Wasm memory, and floor-device parser-to-paint
   traces.

The current direct command ranges use `u32`, so a single physical line above 4
GiB remains a separate explicit limit or schema migration. It is not hidden by
the 10 MiB gate.

## Reopen conditions

Reconsider the source-backed donor approach if any of these occur:

- required scanner families cannot be generated or mechanically provenance-
  checked without maintaining a second grammar;
- bounded rewind/push forms cannot prevent source-proportional scanner state;
- exact typed output necessarily materializes line-sized recipes;
- the complete two-pass source/parser/writer path misses device liveness or
  memory gates after batching and calibration; or
- a donor upgrade repeatedly requires broad semantic rewrites rather than
  localized provenance-reviewed intake.

Absent those failures, this is cleaner than either a stock Comrak runtime fork
or a new clean-room Markdown grammar: Flark owns the resumable state, source,
output, and scheduling contracts while the lexical/semantic algorithms remain
pinned, generated, differential-tested donor correspondents.
