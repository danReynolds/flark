# Flark v4 runtime and ABI contract, version 1

**Status:** implemented direct v4 boundary. All thirty-one header operations
are implemented. The checked-in manifest, C header, Rust encoder, and Dart
decoder are contract-tested together; later product milestones still determine
release readiness.

The machine-readable authority is
[`packages/flark_core/test/fixtures/v4/runtime_abi_v1.json`](../../../../packages/flark_core/test/fixtures/v4/runtime_abi_v1.json).
The C declaration is
[`packages/flark_core/native/comrak_bridge/include/flark_v4.h`](../../../../packages/flark_core/native/comrak_bridge/include/flark_v4.h).
Rust and C constants or layouts that disagree with the manifest fail the
contract tests.

## 1. Layering and host neutrality

The dependency direction is:

```text
host -> flark-abi -> flark-runtime -> flark-parser + flark-engine
```

`flark-abi` never calls parser or engine internals directly. `flark-runtime` is
the sole owner of source, revision, parser state, certification, anchors,
transactions, progress, continuations, and reversible history payloads.

The runtime contract has no Dart, Flutter, widget, IME, isolate, display-frame,
filesystem, platform-I/O, callback, or executor concept.

The ABI exposes fixed-width integer records, opaque generation-checked `u64`
handles, borrowed byte pointers, and caller-owned output buffers. It exposes no
Rust layout, parser tree, node pointer, recursive object graph, host object, or
filesystem path.

## 2. Version and capabilities

The direct ABI is major 4, minor 34. `NEGOTIATE` is the only ordinary operation
permitted without a session. ABI 4.32 also permits the explicitly flagged
process-global `SESSION_INSPECT` form without a session so post-close lifecycle
evidence cannot depend on a consumed handle. The host supplies its requested
version and all required capability bits. The runtime returns its supported
version, supported bits, and actual hard caps in `FlarkV4AbiInfo`.

A major or minor mismatch returns `UNSUPPORTED_ABI_VERSION`; this stateless ABI
accepts only its exact current minor because it cannot retain per-client
negotiation state and safely tailor later payloads. A missing required bit
returns `UNSUPPORTED_CAPABILITY`. A future minor may append behavior only behind
a capability and may use previously reserved fields; it may not reinterpret a
field or code.

All input records with `struct_size` must set it to the versioned size and set
reserved fields to zero. Every output sets its actual size. Unknown nonzero
reserved input is `INVALID_ARGUMENT`.

ABI 4.0 assigns no request flags: every `flags`, `SessionConfig.flags`,
`reserved_u32`, and reserved-array field must be zero. ABI 4.32 assigns
`SESSION_INSPECT.flags = GLOBAL_LIVE_STATE` behind its matching capability;
all other nonzero flags remain invalid. A later minor may assign a flag only
behind a negotiated capability and cannot reinterpret 4.0 zero behavior.

`SessionConfig.parser_profile` is mandatory and stable across hosts. Code 1 is
`COMMONMARK_0_31_2` (`commonmark-0.31.2`) and code 2 is the selected production
profile `FLARK_GFM_0_29_V1` (`flark-gfm-0.29-v1`). The latter is the versioned
Flark GFM extension set layered on CommonMark 0.31.2; its ID is a language
contract, not an alias for whatever options a parser dependency happens to
enable. CommonMark-only operation is explicit code 1. Zero or an unknown code
returns `INVALID_ARGUMENT`; a runtime that negotiated no matching profile bit
returns `UNSUPPORTED_CAPABILITY`.

## 3. Handles, ownership, and call discipline

Zero is invalid for every live handle, committed revision, pinned snapshot,
progress token, history token, and owner token. The only legal zero meanings
are frozen per operation in the manifest's `requestFieldRules`:

- create commit/abort use revision zero because no committed revision exists;
- an initial viewport query uses continuation zero and may use snapshot zero to
  atomically select and pin the latest snapshot;
- anchor create and the unused fields of transform/resolve/release use zero as
  explicitly listed;
- bounded commit, pump, anchor, coordinate, and history work uses progress token
  zero to begin, then requires the latest returned nonzero token to resume;
- close begin uses progress token zero, while close pump and finish require the
  latest nonzero close token.

Every other zero in those field classes is `INVALID_ARGUMENT` or the more
specific invalid-handle/status receipt. Handles encode a private generation and
kind. The caller never decodes them.

Ownership is exhaustive:

- `BORROWED_INPUT`: an input pointer remains caller-owned and valid only until
  that call returns;
- `CALLER_OUTPUT`: the caller owns the writable buffer and the runtime writes no
  more than its capacity;
- `RUNTIME_HANDLE`: a successful receipt creates runtime state released only by
  its matching release/abort/close operation;
- `RUNTIME_STAGED_BYTES`: a successful staging append has copied those bytes;
  commit, abort, or close releases them.

The runtime never retains an arbitrary host pointer.

Each session is non-reentrant and carries one opaque nonzero owner token. Every
session operation supplies the matching token. Concurrent calls return
`SESSION_BUSY`; a different token returns `OWNER_MISMATCH`.
`SESSION_TRANSFER_OWNER` changes the token only while the session is idle with
no active call, transaction, or continuation. Otherwise it returns
`MIGRATION_WHILE_ACTIVE`. Retained history tokens and live anchors survive an
idle migration; their stored owner authority follows the session's new token.
Owner tokens describe serialization domains, not OS threads, Dart isolates, or
executors.

## 4. Source creation and atomic edits

Source is exact valid UTF-8 and revisions increase monotonically. Invalid UTF-8
is rejected before commit. The runtime performs no normalization and preserves
LF, CRLF, CR, NUL, and all other valid bytes exactly.

Document creation is staged:

1. `CREATE_BEGIN` creates a provisional session and transaction and copies an
   optional first chunk of at most 64 KiB. Larger input is
   `INVALID_ARGUMENT`; it must use bounded `CREATE_APPEND` calls.
2. `CREATE_APPEND` accepts chunks of at most 64 KiB at explicit offsets.
3. Bounded `CREATE_COMMIT` validates completeness and UTF-8, then creates
   revision 1 atomically. If its work budget cannot finish validation it
   returns a changed progress token; it never performs an unbounded commit.
4. Bounded `CREATE_ABORT` releases the provisional session and staged bytes.

Before a successful commit, no new source revision exists.

`SMALL_EDIT` is a revision-checked atomic batch, not a single-splice shortcut.
It accepts 1–64 fixed 32-byte `FlarkV4EditDescriptor` records plus one packed
replacement-byte slice. Descriptors are sorted and non-overlapping in the named
base revision. Every source endpoint must be an in-range UTF-8 scalar boundary;
the replacement ranges form an exact descriptor-order partition of the packed
slice. The first starts at zero, each following offset equals the previous end,
and the final end equals the slice length. Gaps, overlap, reuse, out-of-order
ranges, and an unreferenced tail are invalid. Descriptor bytes, replacement
bytes, **and the sum of every deleted source range** must be at most 4 KiB. This
prevents a tiny descriptor from hiding a multi-megabyte synchronous replacement,
deletion, or inverse-history copy. Validation failure commits none of the edits.
An admitted batch consumes one runtime work unit, completes atomically in that
call, and returns exactly one new revision and, when retained under the
configured history budget, one opaque reversible token; `SMALL_EDIT` never
returns resumable progress. If bounded retirement capacity is full before
admission, it returns `BACKPRESSURE` with no source or revision change; the host
pumps bounded maintenance and retries the same edit.

Any edit whose aggregate synchronous envelope exceeds 4 KiB, including a large
deletion with an empty replacement, uses `BULK_BEGIN`, bounded 64 KiB
`BULK_APPEND` calls, then `BULK_COMMIT`. Append does not change source authority.
Commit performs one bounded rope splice and one revision change. Abort preserves
the old source and revision. `EDIT_TOO_LARGE` directs the host from the small
path to bulk; it never authorizes an unbounded call.

`SOURCE_READ` is valid only while the session is `OPEN`. Its requested range is
at most 64 KiB and it streams exact source into a caller buffer after the fixed
result-page header described in section 6. Its page always uses `SOURCE_BYTES`,
`NOT_APPLICABLE`, snapshot zero, and continuation zero. Save/export advances
explicit source ranges; source reads never create continuations and the runtime
never opens a path. This avoids a continuation that cannot name a snapshot and
prevents close from reclaiming source while a read still claims it is usable.

For `CREATE_APPEND`, `BULK_APPEND`, and `SMALL_EDIT`, duplicate lengths in the
fixed request record and C function arguments must match exactly. Mismatch is
`INVALID_ARGUMENT`; neither value silently wins.

## 5. Budgets and wall-clock measurement

`FlarkV4WorkBudget.max_work_units` is the runtime-enforced hard work bound.
`max_result_items` and `max_result_bytes` are runtime-enforced result bounds and
cannot exceed negotiated caps.

Every request that carries a work budget requires `max_work_units >= 1`; zero
is `INVALID_ARGUMENT` before the runtime consumes session state or borrowed
input. `QUERY_VIEWPORT` and `CONTINUATION_NEXT` additionally require nonzero
`max_result_items` and `max_result_bytes`, preventing a nonempty query from
entering a zero-progress result-cap loop. Other budgeted operations still
validate both result fields against the negotiated maxima even when they do not
produce a result page.

`advisory_max_micros` is deliberately not a runtime clock contract. It is a host
scheduling hint recorded by the host alongside its call receipt; it is not
echoed in `Outcome`. The host measures elapsed time around
each synchronous call and enforces the product frame/span gates. Runtime
correctness must never depend on observing wall time; work units remain bounded
even if a clock is absent, coarse, or paused. M2 must calibrate work units and
fail the measured hard-span gate if one unit is itself too large.

`BUDGET_EXHAUSTED` means bounded work advanced and more pump work remains.
`RESULT_CAP_REACHED` means a bounded page is complete and a continuation names
the remainder. Neither permits an unchanged anonymous pending state.

## 6. Revisions, snapshots, certification, and results

Every source, semantic, anchor, coordinate, and continuation request names an
explicit revision. An initial `QUERY_VIEWPORT` has continuation zero. Snapshot
zero on that operation means “select the latest snapshot atomically”; the
resulting page names the new or selected nonzero pinned snapshot. Later query
pages and both continuation operations echo the exact revision, snapshot, and
continuation authority from the prior page. An edit versions or invalidates old
snapshots and continuations; pages from different revisions or snapshots may
not be combined.

A snapshot ID is a non-owning immutable epoch identifier, not a resource
handle. It has no release operation and does not by itself retain page state.
Only a nonzero continuation retains result-generation resources. A snapshot is
usable only while its revision remains current and the runtime still has that
epoch; otherwise the request returns `STALE_SNAPSHOT`. Releasing the last
continuation permits immediate epoch reclamation.

Semantic output is usable only inside a range certified for the exact current
revision. A page can be wholly `CURRENT_CERTIFIED`, wholly `PENDING_NEUTRAL`,
or `MIXED_CURRENT`; pending spans paint exact source neutrally. Old semantics
are never mapped forward and called current merely because their coordinates
still fit.

Output buffers are hard-capped. `BUFFER_TOO_SMALL` reports the minimum required
bytes without partially encoding a record. For a page-producing operation this
minimum always includes the fixed 96-byte result-page header, so an empty page
still reports 96 bytes rather than zero. `RESULT_CAP_REACHED` returns a valid
bounded page plus an opaque continuation. `BACKPRESSURE` means the caller must
perform the named bounded release work before retrying: drain/release retained
output for a query, or pump retirement maintenance for a rejected `SMALL_EDIT`.

Every `SOURCE_READ`, `QUERY_VIEWPORT`, and `CONTINUATION_NEXT` result that
contains a page begins its buffer with the fixed 96-byte
`FlarkV4ResultPageHeader`; payload begins
at `header.struct_size`, and `Outcome.written_bytes` includes header plus
payload. The header carries ABI major/minor, `record_kind`, revision, snapshot,
the effective scalar-aligned requested byte range, the byte range actually
covered by this page, certification state, item count, payload-byte count, and
continuation. Viewport byte ranges are bounded window hints: for query kinds
1, 2, 3, 4, and 6 the ABI floors the requested start, requested end, and page
start to UTF-8 scalar boundaries once before runtime dispatch. Query kind 5
selects an exact semantic target and therefore remains strict rather than
silently changing its requested fact range.
The host-neutral runtime returns these values as a typed
`flark_runtime::ResultPageReceipt`; only `flark-abi` encodes the C header.
Therefore a host can validate a page at the ABI boundary without decoding an
opaque payload or trusting the separate `Outcome` alone. The request and header
revision/snapshot must agree, the covered range must be contained in the
effective requested range, and a continuation page preserves that same
effective range. Any mismatch is an invalid page and must not be painted or
merged.

Ordinary `SOURCE_BYTES` pages use `NOT_APPLICABLE` certification and carry exact
source. A semantic query that is not yet certified instead returns
`NOT_CERTIFIED` with a `SOURCE_BYTES`, `PENDING_NEUTRAL` page for the exact
revision and pinned snapshot; this page contains neutral source, never stale
semantic facts.
`SEMANTIC_FACTS` pages use `CURRENT_CERTIFIED` only when the covered range is
certified for the named revision and snapshot; otherwise the call returns
`NOT_CERTIFIED` and a `PENDING_NEUTRAL` page.

ABI 4.5 adds parser-authored inline projection without changing the 128-byte
row size. The final row word is now `inline_fact_count`, and row flag bit 3
means that count is authoritative, including an authoritative zero. The
payload contains all `item_count` row records first, followed by fixed 80-byte
inline records grouped in row order. Each record names its complete Markdown
source range and visible-content range in absolute UTF-8 byte and UTF-16 code-
unit coordinates. The initial kinds are emphasis, strong, simple code spans,
strikethrough, URI/email autolinks, and direct links. If a row contains an
unsupported transforming fact, reference/image fact, exceeds the bounded
inline envelope, or its complete fact group does not fit the caller's result
buffer, bit 3 stays clear and the count stays zero; the host must show exact
source neutrally. Partial inline sets never cross the ABI.

ABI 4.6 completes the bounded source-to-visible record shape. Escapes and hard
breaks carry parser-owned marker cuts; direct/reference links and images carry
their visible label cuts; and a replacement kind carries one or two cooked
Unicode scalars for character references and normalized code-span line
endings. Transforming code spans publish their already-trimmed content cut plus
explicit line-ending replacements, so Dart and Flutter never reinterpret
Markdown. Reference uses resolve against the live session-owned winner index
without rebuilding it. All replacement words must be zero on non-replacement
records, and a replacement record must carry a valid nonzero first Unicode
scalar.

ABI 4.7 adds bounded GFM table presentation without changing either fixed
record size. A Table Paragraph uses semantic-variant bit 26. Table-cell kind
14 carries alignment in flags bits 0-1, header in bit 2, row start in bit 3,
and an autocompleted empty cell in bit 4. Cell source and content cuts remain
absolute parser authority; table-specific escaped pipes are ordinary cooked
replacement facts. If the complete table group does not fit, the row remains
exact source and does not advertise table presentation.

ABI 4.8 assigns viewport-row flag bit 4 to parser-authored presentation
continuity for conservative plain-text insertions wholly inside a contiguous
editable row range. ABI 4.9 broadens that flag to conservative plain-text
edits. The host binds the capability to one exact transaction and revision and
retains the bounded exact row content. Deletion fails closed at the content
boundary, beside Markdown-sensitive source, when it empties the content, or
when it touches an inline fact. Syntax-shaped input, tables, and thematic
breaks remain unauthorized. The fixed row record remains 128 bytes.

ABI 4.26 adds capability `LITERAL_SAFE_ENVELOPES_V1`, inline-record kind 15,
and query kind `SEMANTIC_PROJECTED_LITERAL_SAFE`. Only that query kind may
append literal-safe-envelope records after a row's ordinary inline facts.
`SEMANTIC` and `SEMANTIC_PROJECTED` retain their pre-envelope payload vocabulary,
while the stateless runtime rejects a 4.25 negotiation rather than pretending
it can tailor every legacy flag and record. The landed word class is limited to
a complete non-empty content slice containing
only ASCII letters/digits (and zero code-normalization flags); the one-space
class is a zero-width row-end proof. Both are one-shot authorities.

ABI 4.27 adds capability `LITERAL_SAFE_ENVELOPE_CLOSURE_V1` without assigning a
new query kind, record kind, edit-class code, or record layout. The capability
extends the existing envelope vocabulary with parser-authored closure proofs.
The current reusable word/space bundle is limited to a canonical single-line
ATX heading with an authoritative empty inline-fact set, identity byte-to-UTF-16
geometry, and ASCII letter/digit-or-space content bounded at both edges by
ASCII letters/digits.
A non-empty `SINGLE_ASCII_SPACE_INSERTION` envelope authorizes one U+0020 only
at insertion positions `start < p < end`; a zero-width instance authorizes its
exact position and is consumed. Reusable space authority ends before any
existing trailing-space run, and a terminal zero-width proof is absent when the
row already ends in U+0020, so two trailing spaces cannot be carried into a hard
line break.

After a matching edit, a non-empty envelope grows in both coordinate systems.
Non-empty envelopes with exactly equal byte and UTF-16 geometry are one
parser-authored closure bundle and grow together. An unmatched envelope
strictly crossed by a foreign-class insertion is dropped; a range wholly before
the insertion stays, and a range at or after it shifts. Core may apply only
these transforms and edit-class matching. It may not inspect source, classify
Markdown, reconstruct a consumed envelope, or carry a proof after any failed
transform. The original ABI 4.26 authority remains one-shot; the stateless
runtime rejects a 4.26 negotiation rather than silently serving 4.27 closure
semantics.

ABI 4.28 adds capability `PROJECTION_EDIT_CELLS_V1` and semantic-record kind
16 on the existing query-kind-6 stream. A kind-16 record carries its affected
dependency closure in the source ranges and its edit trigger in the content
ranges. Flags name the matcher and the only presentation that may survive:
the block shell, the already-certified runs outside the closure when declared,
an exact transformed closure, and optional result-cell chaining. Replacement
words remain zero. Closures in one row must be disjoint or exactly equal;
partial overlaps are invalid.

The first broad cell is the complete editable content of a canonical top-level
single-line ATX heading with authoritative empty inline facts. It accepts any
non-noop splice without CR/LF, paints that whole cell exactly, retains the ATX
shell, and may chain the transformed cell. ABI 4.29 capability
`PROJECTION_EDIT_CELLS_V2` adds matcher codes 2, 4, and 5 rather than widening the
pushed 4.28 V1 contract in place. A chainable literal cell admits
nonempty ASCII-alphanumeric insertion/replacement or one U+0020 insertion
strictly inside its trimmed trigger. Its affected closure may include harmless
boundary spaces so equal-closure matchers form one partition, but those spaces
are not edit authority. The trigger excludes every neighboring inline-dependency
boundary (a physical row boundary may be included), its transformed literal
closure is exact, and certified outside facts remain projected. A separate
one-shot matcher admits exactly one empty-replacement ASCII/UTF-16-unit deletion
only when the parser proves every admitted position leaves an alphanumeric unit
in the cell. The first local
dependency cell is one-shot: for a conservatively isolated flat
`**ASCIIword**` Strong fact, one U+0020 insertion at the parser-authored
opener/content boundary paints only the Strong source closure exactly while
retaining the heading shell and independent outside facts. Any mismatch,
ambiguity, row change, failed range transform, or second edit after a one-shot
cell fails closed. A fresh result-revision row with complete inline facts always
supersedes the temporary cell presentation. A terminal append cell may cover
the final physical-line plain gap, including punctuation, with a zero-width
trigger at its end. It admits collapsed ASCII-alphanumeric appends, the bounded
ASCII prose punctuation `"',.:;?`, and one space after certified non-whitespace
terminal prose on a current Plain physical line whose first non-space character
is an ASCII letter; block-opener-shaped lines receive no terminal cell. That
space disables further space authority until another alphanumeric or admitted
punctuation append. A fresh parse ending in exactly one U+0020 republishes the
cell with `TERMINAL_SPACE_BLOCKED`; two spaces or any other terminal whitespace
suppress it. The carried proof therefore cannot create a hard line break.
Matcher codes are 1
`ANY_NO_CRLF_SPLICE`, 2 `ASCII_LITERAL_SPLICE_IN_LITERAL`, 3
`INSERT_SINGLE_ASCII_SPACE_AT_POINT`, and 4
`DELETE_ONE_ASCII_UNIT_IN_LITERAL`, and 5
`APPEND_ASCII_LITERAL_AT_LINE_END`.

ABI 4.30 capability `LITERAL_SAFE_ENVELOPES_V2` adds edit class 3,
`SINGLE_ASCII_ASTERISK_INSERTION`, on the existing kind-15 envelope record. It
authorizes exactly one collapsed `*` insertion strictly inside the complete
content range of one flat Strong fact; neither content boundary is included.
The parser emits this one-shot envelope only when
no other asterisk dependency or overlapping inline fact escapes the Strong
source. The host transforms the existing projected Strong run, preserving its
hidden delimiters and style, then consumes every same-geometry envelope so no
successor can reuse predecessor authority.

ABI 4.31 capability `STRUCTURAL_PRESENTATION_PROOFS_V1` adds edit-intent
receipt flag `PRESENTATION_PROVEN` (`0x8`). Rust sets it only while resolving a
semantic edit from a current Ready parser result. V1 admits bounded Plain
terminal paragraph splits and Plain paragraph merges whose parser-normalized
inline partitions are unchanged. The flag authorizes the typed transitional
presentation and, for the newly empty Plain successor of a proved terminal
split, exactly the zero-width chainable ASCII literal cell defined by this
minor. That cell may carry ordinary typing, but it cannot authorize another
structural split; every additional structural transition requires its own
current Ready `PRESENTATION_PROVEN` receipt. Pending, oversized, non-ASCII,
escape/entity/link/code/underscore/strike, or delimiter-crossing transitions
omit the flag.

Exact ABI 4.32 additionally permits `PRESENTATION_PROVEN` on the existing
`INDENT_LIST` and `OUTDENT_LIST` transition codes when a current Ready
parser-authored simple ListItem context resolves to a ListItem result through
one bounded 2..14-byte ASCII-space prefix insertion or deletion. The retained
row shell and inline runs are shifted through that exact prefix splice. This
post-commit proof grants no authority to a subsequent text or structural edit.

Exact ABI 4.31 also permits the existing `ASCII_WORD_INSERTION` record to cover
parser-authored maximal ASCII letter/digit word leaves inside an eligible
projected fact. Each leaf must be identity-mapped, bounded on both sides by the
fact edge or U+0020, and independent of every overlapping fact; Code leaves
still require zero normalization flags. Publication is capped at 128
literal-safe envelopes per row. When the page's complete baseline presentation
fits the caller buffer, kind-6 encoding reserves every row's ordinary inline
facts and required projection-segment group before admitting edit cells or
envelopes from the remaining shared 64 KiB payload, so optional continuity
vocabulary cannot evict a later rendered row. If the baseline groups themselves
do not fit, the ABI 4.5 complete-group fail-closed rule still applies.

ABI 4.32 capability `GLOBAL_LIVE_STATE_INSPECTION_V1` assigns
`SESSION_INSPECT.flags = GLOBAL_LIVE_STATE` (`0x1`). This form requires a zero
session reference and remains callable after close consumes the final handle.
The fixed `SESSION_INSPECTION` record sets session state, session, and revision
to zero; its four `live_*` fields report process-global transaction,
continuation, anchor, and history counts, while `reserved[0]` reports live
sessions and `reserved[1..2]` remain zero. This is bounded lifecycle evidence,
not document authority, and does not expose parser or allocator internals.

ABI 4.32 also adds capability
`PROJECTION_EDIT_CELLS_V3` without changing the kind-16 record layout. Matcher
code 6, `INSERT_EXACT_SCALAR_AT_POINT`, uses `replacement_first` as one valid
Unicode scalar parameter and requires `replacement_second == 0`; every older
matcher still requires both replacement words to be zero. Its content ranges
are one zero-width parser-authored trigger strictly inside the source-range
dependency closure. The host admits only a collapsed insertion of exactly that
scalar at exactly that point, retains only the declared block shell and outside
partition, presents the transformed closure as exact current source, and
consumes the record after one edit. The first bounded emitter covers `[` inside
one conservatively isolated flat Strong fact on a single physical-line Plain
row only when the parser's bracket
classification is exhaustive and the leaf contains no existing bracket
dependency. This is parser-owned proof data, not a Dart Markdown allowlist.

The same matcher may parameterize one of the frozen D0 prose punctuation
scalars (`.`, `,`, `;`, `:`, `!`, `?`, apostrophe, double quote, `(`, `)`,
hyphen, en dash, or em dash) at an ASCII-alphanumeric guard pair inside a
fact-free prefix before one authoritative Strong fact. The complete prefix is
the affected closure; the outside Strong fact remains retained. These records
are also one-shot, use the same V3 capability, and add no host punctuation
classification.

The same matcher encodes the frozen different-marker syntax set. Rust may emit
`*`, backtick, `[` or `]` beside one Emphasis fact and `_` or `~` beside one
Strong fact only when the complete prefix is fact-free ASCII prose, the trigger
is between ASCII-alphanumeric guards, and the inserted marker is absent from
the current source. `[` and `]` additionally require exhaustive bracket
classification. The prefix is the affected exact closure, the different-marker
fact is the retained outside partition, and the record is one-shot.

The same V3 capability also permits matcher code 2's existing guarded literal
cell to accept one nonempty ASCII-alphanumeric/U+0020 replacement when the edit
is strictly interior to its parser-authored trigger and contains at least one
alphanumeric unit. This closes a bounded multiword paste without granting line
boundaries, punctuation, deletion, or newline authority. The parser's first
emitter uses a complete fact-free physical-line gap as the affected closure
and a maximal ASCII prose run as its interior trigger; unchanged interior
guards isolate every retained outside fact. Older hosts reject this
additional safe shape, while a 4.32 host requires the V3 capability before it
can observe the record.

ABI 4.33 added capability
`PROJECTION_EDIT_CELLS_V4` without changing record kind 16 or query kind 6.
Matcher code 7, `EXACT_SPLICE_REPLACE_BLOCK_SHELL`, declares one exact
parser-authored insertion or deletion over a bounded physical-line closure.
Its source ranges carry that complete closure and its content ranges carry the
exact zero-width insertion point or nonempty deletion range.
`replacement_first` is the required inserted Unicode scalar, or zero for a
deletion. `replacement_second` packs a typed clean-result shell: bits 0–3 are
Plain, ATX heading, BlockQuote, or ListItem; bits 4–11 are the result prefix's
UTF-16 length; and bits 12–31 carry the heading level or quote depth when
applicable. Flags require `RETAIN_OUTSIDE`, `PRESENT_EXACT`, and
`REPLACE_BLOCK_SHELL`; retaining the predecessor shell and chaining are
forbidden.

Matcher code 8, `SIMPLE_BLOCK_PREFIX_PLAN`, covers the corresponding rapid
prefix sequence before a fresh parser publication can interleave. Its content
range is the zero-width physical-line start. `replacement_first` packs up to
three nonzero ASCII plan bytes little-endian in bits 0–23 and the parser-proved
1-based activation prefix length in bits 24–31; `replacement_second` carries
the same typed final shell. It requires `CHAIN_RESULT` in addition to the V4
replacement flags. Core advances only an exact nonempty prefix of the remaining
plan at the carried point. Before activation it presents Plain exact content;
from activation onward it presents the declared target shell using the consumed
prefix length. A unique specialized plan match takes precedence over a generic
literal cell for the same edit; multiple matching plans fail closed.

The first emitter reruns the bounded donor-backed physical-line classifier on
the exact counterfactual edit and publishes only a changed supported shell.
It covers top-level Plain, canonical ATX heading, depth-1 BlockQuote, and simple
depth-1 ListItem construction/removal. Core compares the declared splice and
range mechanically, then materializes the typed shell through the same pending
presentation lifecycle used by every other dependency authority. It neither
recognizes a Markdown opener nor widens the parser result. Fresh certified rows
supersede this authority over the prefix-inclusive physical range. The exact
splice form is one-shot; the prefix-plan form expires when the finite sequence
is complete or any different edit arrives.

The final D0 ABI 4.34 adds capability
`BOUNDED_PENDING_PRESENTATION_PLANS_V1` and inline-record kinds 17 `PLAN`, 18
`STEP`, and 19 `ROW`, all carried only by query kind 6. The new vocabulary
represents one bounded exact insertion sequence together with a complete clean
parser result snapshot for every admitted prefix. It is generic result
authority, not a host-visible fence grammar.

`PLAN.flags` packs sequence length in bits 0–7, step count in bits 8–15, and
the number of replaced predecessor rows in bits 16–23. Its source ranges name
the base affected range, its content ranges name the zero-width trigger, and
the two replacement words carry the 1–8 ASCII sequence in little-endian byte
order. Exactly one `STEP` follows for every sequence byte. `STEP.flags` packs
the 1-based prefix length in bits 0–7 and result-row count in bits 8–15; its
source ranges name that prefix result's affected range.

Each `ROW` follows its owning step. `ROW.flags` packs the viewport row kind in
bits 0–15 and ordinary inline-fact count in bits 16–31. Its source ranges name
the clean result row, its content ranges name the editable projection, and its
replacement words carry the ordinary row semantic variant/value. Exactly that
many ordinary fact records immediately follow the row. V1 permits only
complete Plain or fenced CodeBlock result rows, at most four rows per step,
128 facts across the plan, a 16 KiB affected source, and no segments or nested
plans. Rows are ordered and nonoverlapping; source gaps remain exact neutral
source owned by the same affected result.

The plan is encoded as one all-or-nothing optional group after all page rows'
ordinary facts and required segments have been reserved. It may be omitted for
capacity but cannot evict baseline presentation. Core matches only the next
exact scalar at the carried point and selects the supplied clean step. An
intermediate fresh parse does not discard still-declared successors; the plan
retires after the complete sequence is freshly certified or synchronously on
any mismatch, ambiguity, stale revision, malformed geometry, truncation, or
out-of-window source. The initial emitter covers only the frozen D0 opening
journey (three backticks, `dart`, Return) and closing journey (Return, three
backticks); other fenced construction remains fail-closed.

Exact ABI 4.32 also maps maximal ASCII-word triggers inside each physical line
of a parser-certified closed fenced-code body. The affected closure is that
authored code line without its line ending; the body publishes authoritative
empty inline facts, so the host retains the code shell and paints only the
changed line as exact current source. Neither fence is part of an admitted
range, and no new matcher or record shape is introduced.

The current implementation derives this bounded projection on the native
document actor while serving the viewport query, using the existing Rust
inline grammar and a maximum 4 KiB parser-row source, 512 facts per row, and
bounded parser transitions. Multi-physical-line paragraphs are split into
line-local literal cells; no cell contains a line ending. This establishes
functional authority, not final query-time performance: retained/cached inline
publication and demand scheduling remain a separate optimization gate.

ABI 4.4 adds typed block-structure presentation without changing the 128-byte
row layout. A parser-authored BlockQuote Paragraph uses bit 16, nesting depth
in bits 17–24, bit 25 for the bounded simple top-level continuation case, and
the exact presentation-prefix ranges. Indented and Fenced Code rows use bit
16; fenced rows additionally encode fenced/tilde/closed flags, the 0–3 fence
offset, and the minimum closing length in `semantic_value`. A ThematicBreak
row uses bit 16 with zero `semantic_value`. These meanings are disjoint by row
kind. Complex BlockQuote rows remain typed but do not receive the simple edit
capability, and empty marker-only quotes remain exact neutral source because
the parser publishes no renderable row for them.

ABI 4.3 extends the fixed viewport row record to 128 bytes. Zero remains the
default semantic variant. For a Heading row (`kind == 12`), bits 0–7 contain
the parser-authored level 1–6 and bit 8 distinguishes Setext from ATX. For a
parser-authored List Item Paragraph or terminal empty row (`kind == 5` or
`14`), bits 0–2 identify the bullet marker or ordered delimiter, bits 3–10
carry nesting depth, bits 11–12 carry marker offset, and bit 13 certifies the
simple top-level continuation case. Bit 14 states that the item opens its List,
which distinguishes direct marker removal from the blank-line boundary needed
to avoid CommonMark lazy continuation. `semantic_value` is the exact ordered
marker value and zero for bullets. The presentation-prefix start and end
fields name exact byte and UTF-16 ranges; all four are `UINT64_MAX` when
absent. Explicit ends preserve terminal empty-item geometry when a line ending
sits between the marker and the parser's zero-width caret row. Hosts use these
typed facts and exact ranges for presentation and editing behavior; they must
not inspect source markers to infer them. ABI 4.2 introduced the Heading
encoding in the then-final `u32`; ABI 4.3 preserves those bit meanings, and ABI
4.5 assigns the former final reserved word to `inline_fact_count`.

ABI 4.1 freezes the live `SOURCE_AND_SEMANTIC` payload. It contains
`item_count` ordered 40-byte `FlarkV4CertificationRangeRecord` values followed
by exact current source bytes for `covered_range`. The records form a gapless,
non-overlapping byte and UTF-16 partition of that range; each record is either
`PENDING_NEUTRAL` or `CURRENT_CERTIFIED`. The page header is
`PENDING_NEUTRAL` when every record is pending, `CURRENT_CERTIFIED` when every
record is certified, and `MIXED_CURRENT` otherwise. A mixed or pending page
returns `NOT_CERTIFIED`, because only its individually certified spans may
reuse bounded host-cached presentation facts. No parser identity, prior row
ordinal, or stale semantic record crosses this payload.

A zero
continuation means that no later page exists; a nonzero value is a
generation-checked runtime handle and is the only authority for the remainder.

Coordinates are never bare interchangeable integers. Requests name
`SOURCE_BYTE` or `UTF16_CODE_UNIT`, revision, and affinity where relevant.
Exact source-byte positions must be scalar boundaries. Viewport window hints
are the bounded exception described above: the ABI normalizes them and reports
the resulting effective range, so a host byte cap cannot manufacture an
invalid covered coordinate. An unpaired UTF-16 surrogate must normally be
rejected by the host binding before byte conversion;
`INVALID_UTF16_HOST_INPUT` is reserved as the cross-layer typed receipt so that
this failure cannot collapse into `INVALID_UTF8` or `INVALID_ARGUMENT`.

Anchors are source-stable opaque handles, not parser-node identities. Creating,
transforming, and resolving them carry a hard work-unit/result budget and a
zero-to-start/nonzero-to-resume progress token. Release detaches the anchor in
at most one work unit; any nontrivial destruction joins the session reclamation
queue. Anchor work may therefore span retained history without an unbounded
synchronous walk.

The current implementation keeps every anchor at the current revision by
transforming all of a session's anchors inside each committed small, bulk, and
history-replay splice. A position strictly inside a replaced span collapses to
the splice edge named by the anchor's creation affinity, and an insertion
exactly at an anchor moves it only for `DOWNSTREAM`. That eager maintenance is
bounded by the declared `MAX_LIVE_ANCHORS` cap (4096 per session; exceeding it
is `RESOURCE_LIMIT_EXCEEDED`, not yet surfaced in `FlarkV4AbiInfo`), so anchor
operations complete in their first bounded call and never issue a nonzero
resume token. Creation validates its position as a scalar boundary in the
requested coordinate kind; unreleased anchors are drained by close pumping and
counted by `CLOSE_FINISH`.

The host-neutral `Outcome` contains a typed `OperationResult` variant. The
manifest's `outcomeFieldRoles` maps every operation and typed variant into the
generic fixed-width C fields; unlisted fields are zero. Session inspection has
a separate fixed 64-byte record for state, revision, and all four live-handle
counts. Neither the runtime nor a language binding may invent another meaning
for `primary_handle`, `secondary_handle`, or `detail_code`.

Runtime `Outcome` byte counts exclude C record/header bytes. For page
operations, `flark-abi` adds the fixed page-header size when producing C
`required_bytes` and `written_bytes`; the runtime never hard-codes an upward ABI
layout.

## 7. Progress and every terminal condition

`FlarkV4Outcome` always identifies the operation, exact status, progress state,
revision, snapshot, progress token, bytes required/written, and an operation-
specific detail code. The status table in the JSON manifest is exhaustive.

Progress states are also exhaustive. The manifest's `statusProgressRules`
accounts for every status and its legal progress states. `NEEDS_INPUT`,
`NEEDS_OUTPUT_BUFFER`, `SESSION_CLOSED`, and `HISTORY_BUDGET_EXCEEDED` retain
their numeric values for ABI stability but are reserved and never returned by
ABI 4.0. Staged gaps use `NOT_READY_SOURCE_GAP`; insufficient caller memory uses
`BUFFER_TOO_SMALL`; a consumed close handle is stale or invalid; and history
budget pressure is reported by the successful commit's `HistoryDisposition`.
Every C entrypoint's scalar return equals `Outcome.status`.

The active progress states are:

- `ADVANCED`: token changed and work remains;
- `BUDGET_EXHAUSTED`: token changed and the hard work-unit budget ended;
- `RESULT_CAP_REACHED`: a capped page and continuation were produced;
- `PENDING_SOURCE_GAP`: a named staged source range is still required;
- `BACKPRESSURED`: the caller must perform bounded release/retirement work;
- `COMPLETE`, `CANCELLED`, `SUPERSEDED`, or `FAULT`.

If a nonterminal call repeats the same token without a named external
requirement, the runtime returns `PROGRESS_STALLED` with `FAULT`. Cancellation
and supersession occur at pump boundaries. A cancel naming an old token returns
`STALE_PROGRESS_TOKEN`; a successful `CANCEL` retires exactly the current
nonzero session progress token and echoes it with status `CANCELLED`.

A terminal `COMPLETE` pump echoes its final token in the receipt but
invalidates the stored progress: the session reads as idle for owner
migration, and the next pump chain begins from token zero. A completed
progress token can therefore no longer be cancelled or resumed.

Previously ambiguous terminal conditions have separate codes:

| Condition | Code |
| --- | --- |
| Work budget ended | `BUDGET_EXHAUSTED` |
| Result page cap reached | `RESULT_CAP_REACHED` |
| Staged source gap | `NOT_READY_SOURCE_GAP` |
| Caller must perform bounded release/retirement work | `BACKPRESSURE` |
| Caller buffer cannot hold one record | `BUFFER_TOO_SMALL` |
| Old progress generation | `STALE_PROGRESS_TOKEN` |
| Invalid host UTF-16 | `INVALID_UTF16_HOST_INPUT` |
| Parser-owned failure | `PARSER_FAULT` |
| Non-parser invariant failure | `INTERNAL_FAULT` |
| Caught unwind | `PANIC_CONTAINED` |
| Fallible bounded allocation failed | `ALLOCATION_FAILURE` |
| Anonymous unchanged progress | `PROGRESS_STALLED` |

`RESOURCE_LIMIT_EXCEEDED` is reserved for a declared negotiated resource cap;
it cannot substitute for any row above. `INTERNAL_FAULT` cannot substitute for
a parser, panic, allocation, progress, buffer, or backpressure failure.

ABI entrypoints contain panics and return `PANIC_CONTAINED`; unwinding never
crosses C. M2 uses fallible allocation on ABI-reachable bounded buffers so
allocation failure can return `ALLOCATION_FAILURE` rather than silently aborting
or masquerading as a parser fault.

## 8. History and close

A committed edit may return one opaque reversible token. Exact inverse bytes
remain runtime-owned under the session's history-byte budget. The host owns
grouping and selection snapshots, not inverse document text. Replay is
revision-checked and atomic. Evicted and stale tokens have distinct statuses.

Close is resumable:

1. `CLOSE_BEGIN` makes the session non-writable and enters `CLOSING`.
2. Each `CLOSE_PUMP` releases at most its work budget.
3. `CLOSE_FINISH` requires the latest nonzero close token and succeeds only
   after exactly zero live transactions, continuations, anchors, history
   tokens, source, and derived parser state. Success atomically consumes the
   sole closing session handle; there is no inspectable `CLOSED` tombstone.

When close begin/pump consumes its work budget after progress, it returns the
ordinary `BUDGET_EXHAUSTED` / `BUDGET_EXHAUSTED` pair and a changed nonzero
token. `CLOSE_FINISH` performs no hidden reclamation: if state remains, it
returns `CLOSE_INCOMPLETE` / `NONE` and echoes the latest token unchanged so the
caller can resume `CLOSE_PUMP`. It does not run a document-sized destructor
synchronously. The exact table is `closeStatusRules` in the manifest.

Once `CLOSE_BEGIN` succeeds, source reads and all writes are rejected. Close
pump may therefore reclaim canonical source incrementally without racing an
operation that still treats it as readable. `SESSION_INSPECT` accepts creating,
open, closing, and faulted live sessions only; a consumed handle returns the
appropriate invalid/stale handle status rather than a fabricated closed state.

Abort and release calls consume or detach their named handle in at most one
work unit. Larger destruction moves to the session reclamation queue and is
drained by ordinary or close pump work; these operations never create an
anonymous second release state machine.

History is byte-budgeted and commit-safe. Before retaining a new inverse, the
runtime evicts the oldest committed tokens until it fits. A zero budget returns
`HISTORY_DISABLED`; one inverse larger than the complete budget returns
`HISTORY_OVER_BUDGET`. In both cases the edit still commits, the history handle
is zero, and the typed disposition appears in `detail_code`. A retained inverse
returns `HISTORY_RETAINED` and a nonzero handle. Replay consumes its input token
and may retain one inverse token for redo under the same policy. If bounded
retirement capacity is full before replay admission, `HISTORY_REPLAY` returns
`BACKPRESSURE` without consuming the token or changing source or revision; the
host pumps bounded maintenance and retries. Source input is never rejected or
rolled back merely because undo storage is unavailable.

ABI 4.0 reserved the semantic result-kind numbers without assigning a mixed
payload. ABI 4.1 freezes the range-record framing above without reinterpreting
the 4.0 page header. A runtime advertises `RANGE_CERTIFICATION` only when it can
produce that current-revision partition and exact source payload.

## 9. Contract verification

The M0 tests enforce:

- all Rust codes equal the JSON manifest and C macros;
- names and numeric values are unique;
- all operation symbols and request records exist;
- every fixed-width Rust record size and critical offset matches the C header;
- every source/query/continuation output is declared with the fixed result-page
  header, including certification and requested/covered ranges;
- a C11 compiler accepts the header and its static layout assertions;
- every formerly ambiguous outcome has an explicit discriminant;
- the Rust runtime can only be implemented through the typed exhaustive request
  enum and caller-owned output slice.

These are interface receipts only. They do not demonstrate implemented runtime
behavior, conformance, performance, packaging, or a callable dynamic library.
