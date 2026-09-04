# RFC 028: Source-authoritative edit transactions

**Status:** PROPOSED — H1 semantic seam and performance receipt implemented;
H2 framework-neutral structural transitions passed the full v4 gate and are
ready for focused dogfood. 2026-08-12.

**Reading rule:** this RFC records transaction architecture and its decision
history. Current user-visible behavior lives in
[edit_profile_v1.md](../v5/edit_profile_v1.md), test policy in
[live_editor_test_strategy.md](../../testing/live_editor_test_strategy.md), and
the handoff bar in [DOGFOOD_MILESTONE.md](../../../DOGFOOD_MILESTONE.md). This
RFC is not a second product contract or testing taxonomy.

**Amends:**
[RFC 026](rfc_026_flark_v4_product_architecture.md) and
[RFC 027](rfc_027_continuously_rendered_markdown.md).

**Scope:** the edit-command machinery between Flutter platform input,
`flark_core`, and the Rust v4 runtime. This RFC does not change the exact-source
document model, the continuously rendered product goal, or the package
boundaries selected by RFCs 026 and 027.

## 1. Decision

Markdown-aware edits are native source transactions. Flutter observes platform
input and presents results; `flark_core` owns ordered editor-session commands;
Rust alone decides what exact Markdown source mutation a semantic command means.

The command path is:

```text
Flutter platform observations
  -> flark input arbiter (one logical edit)
  -> flark_core serialized command gate
       -> apply_source_transaction       (literal edit already known exactly)
       -> try_apply_semantic_intent       (Rust resolves and commits semantics)
  -> committed receipt and canonical selection
  -> ordered flark_core publication
  -> Flutter input/projection state
  -> incremental parser certification
```

The first semantic vertical slice supports one contiguous splice per command.
That is a deliberate contract, not an accidental implementation limitation.
True multi-splice transactions require a coordinated runtime, history, anchor,
ABI, and test design and are deferred.

The first implementation measures the complete event-to-presented-frame path
before the command matrix expands. If semantic availability or resolve time
misses the pending-parser gate, section 8 selects a bounded Rust-authored hint;
scheduling, transport, publication, and rendering misses are fixed in their own
layers. It is never permissible to restore Markdown interpretation in Dart.

## 2. Why the earlier sketch was insufficient

The initial design direction was right but several implied contracts do not
exist in v4 today:

1. The ABI advertises an edit descriptor count, but the current implementation,
   runtime session, history record, and anchor transform support one splice.
   Multi-splice cannot be treated as plumbing.
2. Current Flutter code sometimes changes visible/input state before native
   admission. A rejected native command can therefore leave optimistic state
   labelled with an older authoritative revision.
3. Source commit, history bookkeeping, and replacement selection installation
   are currently separate asynchronous steps. Calling that sequence atomic
   would be inaccurate.
4. Flutter currently owns command serialization and some Markdown behavior,
   including list continuation and block splitting. That contradicts Rust-only
   grammar and edit-semantics authority.
5. Platform newline and delete may arrive through multiple Flutter entrances.
   Similar text deltas are not an adequate deduplication identity.
6. Parser semantics can legitimately be pending while the user types. A fused
   semantic call is correct, but its availability and latency are not yet
   proven in that state.
7. A revision-only receipt is insufficient. Flutter needs the exact committed
   replacement and result selection to adopt the result without reinterpreting
   the command.

This RFC makes each of those boundaries explicit.

## 3. Normative edit profile

GFM defines Markdown parsing. It does not define rich-editor behavior for
Return, Backspace, selection replacement, composition, or list continuation.
Those choices belong to a separate versioned profile:

```text
flark-edit-v1    source edit behavior
flark-live-v2    continuously rendered projection behavior
GFM profile      Markdown parse/render behavior
```

Every semantic intent request names its edit-profile version. Unknown versions
are rejected without mutation. Profile behavior is fixture-backed and may not
be inferred independently in Dart or Flutter.

The normative user-facing behavior of `flark-edit-v1` is now specified by the
[rendered-editing profile](../v5/edit_profile_v1.md). This RFC owns
the transaction, receipt, package-boundary, and failure mechanism used to
implement that behavior. Parser recipes prove a profile result; they do not
choose the product result.

`flark-edit-v1` initially pins these choices:

- exact line-ending policy, including LF, CRLF, CR, and mixed-source handling;
- collapsed and non-collapsed selection behavior;
- paragraph split and merge behavior;
- simple unordered and ordered list continuation;
- empty-list-item termination;
- list-item lift on Backspace at content start;
- marker indentation and ordered-marker increment policy;
- history boundary behavior; and
- behavior while composition is active or parser semantics are unavailable.

The initial line-ending rule is preserve-nearby: a structural insertion uses
the enclosing block's line ending when unambiguous, otherwise the document's
recorded dominant ending, otherwise LF. Existing source bytes outside the
committed range are never normalized as a side effect.

The dominant-ending fact is maintained as bounded document metadata. A command
may inspect its bounded row neighborhood but may not scan the document to choose
a line ending.

### 3.1 E1 implementation slice

The first end-to-end implementation slice covers only:

- Return at a collapsed caret in a plain paragraph;
- Return in a simple unordered or ordered list item;
- Return in an empty simple list item to terminate the list;
- Backspace at the content start of a simple list item to lift it;
- Backspace at a plain paragraph boundary to merge adjacent plain paragraphs;
  and
- ordinary literal insertion/deletion within text through the existing measured
  lane, whose destination migration is `apply_source_transaction` in H3.

All E1 semantic cases resolve to one contiguous replacement. Quote, heading,
task-list, nested-list, fenced-code, table, HTML, and cross-block-selection
behavior remain explicitly outside E1. Non-collapsed selection replacement is
the next selection increment; excluding it keeps E1 inverse size and anchor
behavior statically bounded. Existing temporary Dart handlers may be removed
only as their exact cases move into the native matrix; they are tracked
migration debt and may not gain new behavior.

This is a historical implementation checkpoint, not the complete
`flark-edit-v1` support envelope. Inline deletion, formatting continuity,
literal provenance, semantic closure, links, replacements, objects, and the
conformance rules live in the standalone profile and must not be inferred from
this smaller E1 list.

`notApplicable` means the named profile has no semantic handling for the exact
case and no state changed. It does not authorize Flutter to guess an equivalent
Markdown mutation.

## 4. State and atomicity model

### 4.1 Two explicit state planes

The editor distinguishes:

1. **Committed state:** the native source revision, canonical anchors, retained
   history, and parser/certification state.
2. **Provisional input echo:** bounded UI state tied to a committed base
   revision plus logical edit and connection generations.

Provisional state is never exported as authoritative source, labelled as
certified, or silently promoted by observing a later revision. It is either
matched to a committed receipt or discarded during typed resynchronization.

E1 semantic intents are receipt-first: Flutter does not speculate their source
replacement. Literal edits may retain a provisional echo only after the state
plane and rollback/resync rules above are implemented.

For a semantic observation, `flark` may retain the platform-supplied provisional
delta solely to keep the text-input protocol and successor coordinates coherent.
It is not painted, exported, or treated as the Rust-resolved replacement; the
terminal receipt reconciles or discards it.

### 4.2 Native linearization point

A successful source transaction has exactly one native source linearization
point under an expected revision. Before that point, all fallible work required
for the promised result must be complete:

- request and profile validation;
- current selection and semantic resolution;
- work/output-cap validation;
- inverse capture and required history reservation;
- result-selection validation; and
- caller-owned result-buffer validation.

After the linearization point, the native coordinator performs only
prevalidated, allocation-free anchor transformation, moves the captured inverse
into its reserved history slot, and fills the preallocated terminal and caller
receipt buffers.

Atomicity here means one native source commit with its required inverse and a
deterministic result selection. It does not claim crash-atomic publication
across Rust, a Dart worker isolate, the UI isolate, and Flutter's next frame.
`flark_core` provides ordered publication after the native commit.

### 4.3 Selection

`flark_core` owns canonical selection anchors and validates its selection
generation while holding the command gate. It sends the native anchor handles,
affinity, direction, and generation as correlation data. The native transaction
validates ownership and expected revision, then resolves both handles exactly
once inside its exclusive transaction. No preflight anchor-resolution worker or
FFI calls are permitted.

For E1, existing anchors transform through the single committed splice. Before
commit, a literal transaction compares that predicted transform with its
requested result; a semantic transaction compares it with the resolver-derived
result. The native coordinator admits the operation only when they agree. It
does not allocate replacement anchors after source commit. Selection generation
remains Core authority and is echoed, not native-validated. A later intent whose
result cannot be represented by mechanical anchor transformation requires an
atomic anchor-retarget primitive before that intent is admitted.

### 4.4 Post-commit uncertainty

Core preallocates the local pending-history/publication slot before dispatch.
Once a reply may represent a commit, receipt corruption, anchor mismatch, or
publication failure is never reported as command rejection. Core enters
`postCommitUnknown`: it disconnects input, freezes authoritative publication,
discards provisional descendants, retains every possibly related token, and
prohibits retry.

Because only one mutation may be native-in-flight, each native session retains
one bounded terminal receipt keyed by session epoch, logical edit ID, and request
digest. The next command acknowledges the previous receipt in its request, so
normal editing pays no extra acknowledgement call. A live worker can recover a
lost reply from that slot and publish one complete recovered snapshot. Reusing
an ID with another digest is an invariant fault.

During H1 migration, an ordered legacy literal/history mutation at the exact
terminal result revision implicitly acknowledges that terminal receipt. This
is safe only behind `FlarkCoreEditorSession`'s single command gate, because such
a caller can know that revision only after receiving the prior result. The
implicit rule is removed when literal edits converge on
`SOURCE_TRANSACTIONS_V1`.

Worker error and exit ports remain attached after startup. Loss of only a reply
is recoverable as above; loss of the worker or native session is typed fail-stop
in E1 unless a later native reattach protocol is explicitly implemented. The
editor never calls either case successful resynchronization. The UI isolate
retains a recovery capability limited to streaming exact authoritative source
and bounded contained close, so worker loss can preserve data and satisfy the
existing zero-live-state contract without permitting continued mutation.

## 5. Input arbitration and command ordering

### 5.1 One logical edit

The Flutter package owns a `PlatformInputObservation` arbiter. It normalizes:

- mutating text deltas or full editing values;
- `TextInputAction.newline` and related actions;
- macOS selectors such as `insertNewline:` and delete selectors;
- hardware-key shortcuts;
- clipboard commands; and
- composing-range changes.

The first mutation-capable observation opens one `logicalEditId`. A mutating
delta is primary on connections that supply it; an action, selector, or hardware
key may be primary where the platform adapter does not supply such a delta.
Later observations matched to the open ticket are acknowledgements, not new
mutations. A consumed shortcut cannot also reach text input as a second
mutation. Matching uses connection and callback generations, platform sequence
information where exposed, adapter ordering rules, and the expected before/after
input state—not text similarity alone.

The ID is assigned by the arbiter; Flutter does not expose a portable OS-event
identity. Correctness therefore comes from one mutation owner per connection
mode plus captured platform callback traces, not a claim that every platform
supplies a deduplication token.

Every logical edit ticket carries:

- document/session epoch;
- observed committed revision and provisional generation;
- predecessor logical edit ID;
- selection generation;
- composition generation;
- actual `TextInputConnection` generation;
- input-window epoch;
- before/after input-window identity; and
- logical edit ID.

Stale observations are rejected before command dispatch.

The common single-delta lane validates and applies the platform's exact range
and replacement once, then updates one result identity. It does not re-run the
batch transform or scan the whole window for common prefix/suffix. The slower
hash-chained path remains for genuine multi-delta batches.

### 5.2 Core command gate

`FlarkCoreEditorSession` admits exactly one native mutation at a time and owns
its authoritative source/history/selection order. `flark` owns the platform
observation buffer and input-window reconciliation; Core does not own Flutter
visible or input caches.

The ordinary H1 path has one native-in-flight mutation. Dependent platform
observations may occupy a simple FIFO for at most one served-frame target and a
terminal ceiling of eight logical observations. This is not a 32-node revision
dependency graph. Crossing the target fails the performance sample; crossing
the terminal ceiling disconnects input and enters typed overload recovery.
Nothing already admitted is silently reported as committed. Oversized paste
uses the existing bounded bulk path.

Mutation commands are never coalesced. Queued selection, viewport,
caret-geometry, and parser requests may be superseded. While a mutation waits,
Core dispatches no new background actor operation. An operation already running
cannot be preempted, so focused-editor pump turns have a measured 1 ms p99
target and remain cooperatively bounded.

Busy/backpressure retry is permitted only when the native result explicitly
guarantees that no linearization point was crossed. Hidden parser pumping inside
an edit retry is prohibited; queue wait and parser work remain separately
measurable. Cancellation succeeds only before native linearization. After that
point Core must adopt or recover the terminal receipt.

### 5.3 Burst materialization

A receipt-first semantic command is one ordering barrier. A later platform
observation stores its text range, selection, and composing range in its
immediate predecessor's provisional-after coordinates; H1 does not materialize
or remap a general dependency graph.

When the barrier receipt arrives, `flark` derives one bounded reconciliation map
from provisional-after to committed-after by composing the inverse of the
recorded platform splice with the exact native splice. It maps every stored
field once and promotes the next observation only when the complete mapped
before-state—text identity, selection, and composing range—is represented and
matches. Ambiguous or interior mappings discard that ticket and its suffix and
enter typed resynchronization. Semantic successors resolve native anchors only
after the predecessor publishes.

## 6. E1 native transaction contracts

The ABI gains minor-version capabilities: ABI 4.10 `EDIT_INTENTS_V1`, ABI 4.11
`SOURCE_TRANSACTIONS_V1`, and ABI 4.12 `NATIVE_COMPOSITE_HISTORY_V1`. The edit
operations use fixed-layout request/result headers plus a caller-owned bounded
byte buffer. None uses a generic argument blob.

Both operations share the linearization, required-history, anchor, and receipt
rules in this section. They differ only in who determines the splice:

- `apply_source_transaction` receives one exact literal splice already known by
  `flark_core`; and
- `try_apply_semantic_intent` receives an intent and resolves the one exact
  splice inside the runtime actor closure.

The existing sequence of small edit, later history retention, and later
selection replacement is not the destination API.

H1 implements only `EDIT_INTENTS_V1` and preserves the already measured
optimistic literal lane while the new semantic path is proved. Literal
transactions migrate to `SOURCE_TRANSACTIONS_V1` immediately after that
measurement; designing both envelopes now does not require landing both at
once.

The normal semantic hot path is exactly one Core dispatch, one worker message,
one FFI call, one native actor closure, one compact receipt publication, and one
Flutter visible-state installation. It performs no public coordinate, source,
anchor, history, prepare, or pump call before or after the transaction.

### 6.1 Literal source request

```text
SourceTransactionRequestV1 {
  session_epoch
  logical_edit_id
  request_digest
  acknowledge_previous_logical_edit_id
  expected_revision
  selection_base_anchor
  selection_extent_anchor
  base_utf16_range
  replacement_utf8_offset
  replacement_utf8_length
  expected_result_selection_utf16
  selection_affinity
  selection_direction
  selection_generation
  history_policy = required
  work_cap
  output_cap
}
```

This operation is for edits whose source meaning is already literal: typed text,
literal deletion, paste, and future exact-source mode. It cannot carry
Markdown-derived marker, block, or delimiter decisions. E1 admits only literal
cases whose canonical selection anchors mechanically transform to the expected
result. More general selection replacement waits for the atomic anchor-retarget
primitive described in section 4.3.

That description is the historical E1 migration lane, not the complete
rendered-editing destination. Once the complete `flark-edit-v1` command
capability is negotiated, ordinary rendered-mode keyboard input, paste,
replacement, and deletion use the rendered command below even when their final
splice happens to be literal. `apply_source_transaction` remains only for an
explicit exact-source surface or a runtime-authenticated exact/literal island
whose canonical source edit is already completely known. Core never selects
the literal route by scanning the payload for Markdown punctuation.

### 6.2 Semantic intent request

```text
EditIntentRequestV1 {
  session_epoch
  logical_edit_id
  request_digest
  acknowledge_previous_logical_edit_id
  expected_revision
  selection_base_anchor
  selection_extent_anchor
  profile_id = flark-edit-v1
  intent = insertParagraphBreak | deleteBackward
  selection_affinity
  selection_direction
  selection_generation
  composition_active
  history_policy = required
  work_cap
  output_cap
}
```

Core validates the selection generation while holding its gate and supplies the
two native handles without first resolving them. E1 semantic intents are
rejected without mutation while composition is active. Native validates handle
ownership and revision and resolves the handles once inside the transaction;
the generation is echoed correlation data. Caller work/output caps are clamped
by negotiated session maxima, so a host cannot opt out of bounded work.

#### 6.2.1 Complete rendered-command destination

`EditIntentRequestV1` is the finite structural E1 wire record. It is not
stretched into a generic flag bag. The next negotiated capability adds one
finite rendered-command request whose variants correspond to normalized
product commands rather than Markdown constructs:

```text
RenderedEditCommandRequest {
  common transaction identity, expected revision, anchors, selection,
  Core selection generation as correlation data,
  profile and support-domain identity, work/output caps

  command =
      keyboardText(exactPayload, orderedInputBoundaries)
    | replaceSelection(origin, exactPayload)
    | deleteSelection(direction)
    | deleteAdjacent(direction, granularity)
    | structuralReturn
    | plainTextPaste(exactPayload)
    | composition(update | commit | cancel, groupIdentity, exactPayload)
    | history(undo | redo, storedSemanticIntentDigest)
    | semanticAction(actionKind, optionalActionPayload)
    | prepareCut

  targetAuthority =
      collapsed(currentCaretContextAuthority,
                optionalEmptyOwnerIntentAndSourceRecipeAuthority)
    | range(authenticatedSelectionAnchors, rangeTopologyIdentity)
    | compositionRange(authenticatedRange, capturedStartIntentAndAuthority)
    | historyToken
    | semanticActionAuthority
}
```

The target-authority variant must match the command. Collapsed insertion and
adjacent deletion require collapsed authority. Range replacement, range
deletion, paste-over-range, and `prepareCut` require noncollapsed `range(...)`
authority, derive their longest common owner path from the authenticated range
topology, and forbid an empty-owner context. Composition
alone may carry its captured starting intent while its composing selection is
noncollapsed. History and semantic actions use only their native tokens plus
the common transaction identity.

Navigation, pointer placement, selection extension/collapse, and other
selection-only adoption do not enter this native source-mutation request. Core
mechanically adopts one parser-authored topology alternative and publishes no
source revision or history entry, as required by
`EP1-COMMAND-RESOLUTION-001`.

The enum is closed by the negotiated ABI/domain. Constructs do not become
command variants: the runtime resolves the command's rendered target and owner
path from current parser-authored topology, then runs the six-stage reducer in
`edit_profile_v1`. This prevents both a controller-side Markdown dispatcher and
an ever-growing per-bug recipe DSL.

Every rendered text request carries its exact origin and payload. Rust decides
whether that origin is source-authoring or rendered-content intent, applies
separator/owner lifecycle, realizes valid source, and returns one complete
result. The host cannot switch paths because the payload contains `*`, `_`,
backticks, brackets, whitespace, or any other delimiter-shaped text.

History carries Core's stored semantic-intent digest alongside the native
history token. Native validates it against the token's before/after intent,
resolves the restored revision, and returns freshly issued caret and optional
empty-owner authority before any restored state is published. Native authority
binds revision/topology/intent, not Core's selection-generation namespace; Core
mechanically wraps the fresh token with its newly adopted local generation.
Historical authority bytes are never accepted as current.

`prepareCut` is the one closed two-phase exception required by an external,
non-rollbackable side effect. It resolves the selected visible `text/plain`
payload and performs every fallible validation/reservation required for the
same range deletion, but does not mutate source or history. Its bounded result
contains the payload plus a single-use opaque `PreparedCutToken` bound to the
session, expected revision, exact range authority, request digest, edit profile,
support domain, reserved inverse/history capacity, and complete rendered-result
proof. Core keeps its ordinary command gate while `flark` writes that payload
to the clipboard, then sends exactly one of:

```text
commitPreparedCut(token) | releasePreparedCut(token)
```

The reservation is tentative capacity owned by the token: preparation creates
no history entry and evicts no existing history token. Commit revalidates only
token/session/revision identity and crosses the already prepared source
linearization point; it does not rerun Markdown policy or allocate. Release
consumes the token without mutation. A failed clipboard write
therefore leaves source unchanged. A commit rejection proved to be before
linearization is reported as explicit copy-only/no-mutation and is not retried.
Once dispatch may have crossed linearization, a lost/corrupt reply follows the
ordinary `postCommitUnknown` path: the consumed token's logical edit ID and
request digest retain one idempotent terminal receipt, Core freezes publication,
and recovery adopts the committed result or proves no commit without executing
a second deletion. The host never reports copy-only while source state is
uncertain. Only one prepared token may exist per session, it cannot cross a
command-gate release or revision, and timeout/session close consumes an
undispatched token. This is not a general construct recipe, speculative cache,
or public prepare API.

### 6.3 Applied receipt

```text
CommittedEditReceiptV1 {
  operation_kind
  semantic_disposition
  history_disposition
  replacement_payload_kind
  has_commit
  logical_edit_id
  request_digest
  base_revision
  result_revision

  committed_splice {
    base_byte_range
    base_utf16_range
    replacement_utf8_offset
    replacement_utf8_length
    result_byte_range
    result_utf16_range
  }

  result_selection_utf16
  result_selection_affinity
  result_selection_direction
  result_source_byte_length
  result_source_utf16_length
  affected_result_utf16_range
  parser_pending
  history_token
}
```

For a literal operation, `semantic_disposition` is the fixed `notPresent` value.
`has_commit` controls validity of the splice and history-token fields. For
`handledNoChange`, `notApplicable`, or `needsCurrentSemantics`, it is false,
base and result revisions are equal, splice fields are zero, and no history
token exists.

For a semantic commit, `replacement_payload_kind = semanticBytes` and the
caller-owned payload contains the exact committed replacement. For a literal
commit, `replacement_payload_kind = callerKnown`; the receipt echoes length and
digest and Core reuses its existing replacement rather than copying the same
bytes native-to-host. This is descriptive outcome data, not a recipe for Dart
to interpret.

The receipt does not claim certification. Parser-authored certification arrives
later through the normal incremental projection path.

That limitation is valid for the implemented structural E1 slice but is not
sufficient for the complete `flark-edit-v1` profile. Any accepted command that
creates, changes, or removes a projected construct or hidden syntax must
negotiate a new receipt capability and return, in the same logical result,
both a fresh typing-context directive and the bounded result-presentation
proof required by
[EP1-RESULT-PRESENTATION-001](../v5/edit_profile_v1.md#ep1-result-presentation-001).
The current `CommittedEditReceiptV1` and its reserved fields are not
reinterpreted.

Conceptually, the capability adds this typed companion result:

```text
CommittedRenderedEditResultV1 {
  edit_profile_id
  support_envelope_id
  predecessor_publication_id
  result_revision
  base_and_result_source_closure
  complete_affected_partition {
    row_shells
    rendered_runs_and_semantic_owner_paths
    source_rendered_mapping
    legal_caret_context_alternatives
    objects_and_action_authority
  }
  retained_partition_references {
    predecessor_fact_ids
    bounded_range_transforms
  }
  result_selection_anchors
  typing_context_transition {
    semantic_intent
    current_caret_context_authority
    preserve | set(fresh_empty_owner_recipe) | clear
  }
  inverse_and_history_identity
}
```

The affected and retained partitions must form a complete non-overlapping
surface for the command closure. There is no open-ended `outside facts` bag and
no truncation of required proof data: cap exhaustion fails before mutation.
Core validates this companion and installs it in the existing immutable
pending-presentation slot; the capability does not add a third controller
semantic state.

The exact fixed-layout wire record belongs to the next ABI minor. Session
negotiation must separately name `flark-gfm-0.29-v2`, `flark-edit-v1`, and the
supported envelope/capabilities; parser-profile code 2 remains permanently
`flark-gfm-0.29-v1`. A mismatch fails before session mutation. Native undo and
redo return the same typed companion with a fresh result-revision recipe;
history never replays an old authority token.

Likewise `preserve` means preserve semantic layer intent while Rust reissues
fresh authority whenever revision or topology stop changes; Core wraps that
authority with its newly adopted local selection generation. It never means
retain an earlier native or Core binding.

### 6.4 Semantic dispositions

- `applied`: one source commit occurred and the receipt is complete.
- `handledNoChange`: E1 consumed the command without changing source,
  selection, or history.
- `notApplicable`: the profile does not handle this exact case; nothing changed.
- `needsCurrentSemantics`: required parser facts were unavailable within the
  work cap; nothing changed.

`needsCurrentSemantics` never falls through to a platform default or a Dart
Markdown guess, but it is not a resumable H1 hot-path state. An occurrence in an
E1 ready, pending-parser, or burst lane fails the sample and triggers causal
diagnosis under section 8; the input queue does not wait behind a parser pump.

`notApplicable` permits only a separately declared semantics-independent literal
behavior. During migration, a Rust-authored capability chooses an old or new
handler before either mutates; a native `notApplicable` result cannot trigger a
second Markdown-aware attempt. In E1, `handledNoChange` consumes the ticket but
leaves revision, anchors, direction, affinity, selection generation, and history
unchanged.

Transport/contract statuses such as stale revision, invalid selection, invalid
profile, cap exceeded, precommit backpressure, and fault remain ABI statuses and
are not duplicated as semantic dispositions.

### 6.5 Runtime and ABI ownership

`flark_runtime::DocumentSession::apply_source_transaction` validates, captures
the inverse, and commits a literal splice in one actor closure when that
destination operation lands.

`flark_runtime::DocumentSession::try_apply_semantic_intent` performs semantic
resolution, inverse capture, and one-splice commit inside one actor closure. It
may reuse bounded current-row machinery internally; it may not compose a public
viewport query and a later public edit, which would create a time-of-check/time-
of-use gap.

One private native per-session transaction coordinator holds exclusive access
across anchor resolution, semantic resolution, exact inverse-size reservation,
inverse capture, source commit, anchor transformation, token installation, and
receipt completion. A private recipe may cross Rust module boundaries only
inside that coordinator; it is never returned to Dart. A no-change result makes
no history reservation.

The destination design uses the global handle registry only to find and
validate a per-session gate, releases it before actor work, and scans only that
session's anchors. The first ABI 4.10 implementation still holds the existing
global registry mutex across the actor call. That does not add foreground work
to one editor, but it serializes independent sessions and remains explicit H1
concurrency debt. H1 measures the configured maximum live-anchor case rather
than assuming its bounded scan is cheap.

Normal user edits use `history_policy = required`. E1's collapsed-caret matrix
rejects any candidate whose inverse plus fixed metadata exceeds the negotiated
small-edit ceiling (currently 4 KiB), so the session keeps one slot and that
maximum byte reserve out of reach of older history. The newest token can
therefore always be retained. If regular history capacity cannot absorb it, the
receipt reports `maintenanceRequired` and Core must release complete old units
before admitting another mutation. No source retry or silent token eviction
occurs. Explicit barriers and disabled history remain separate policies.

## 7. Core adoption and publication

For a transaction, `FlarkCoreEditorSession`:

1. enters its private mutation gate;
2. validates Core-owned ticket and selection generations;
3. sends one worker request containing the complete native operation;
4. receives and validates the bounded receipt;
5. installs the returned token in its preallocated pending-history slot;
6. publishes one compact immutable source/selection/history receipt in queue
   order; and
7. completes the logical edit future.

Any failure after step 3 that may follow a native commit enters
`postCommitUnknown`; it is not ordinary rejection. Core owns no Flutter input or
visible-source cache and does not apply the presentation splice twice.

Flutter installs the compact semantic receipt into its bounded input and surface
state once and emits one visible-state notification. It emits no layout-causing
"waiting" acknowledgement. A literal operation may use the explicit
provisional state plane because its exact replacement is already known; its
matching authority receipt updates metadata without a second layout when the
visible result already agrees.

Grouped undo/redo is not made atomic by replaying several native tokens with
compensating rollback. ABI 4.12 extends one adjacent native tail token for a
caller-named source-transaction group and reduces its inverse sequence to one
bounded replay splice. A composite is capped at 256 observations and 1 MiB of
replay materialization; Core starts a new user-visible unit at that boundary
instead of degrading to partial replay. E1 semantic commands remain standalone
history barriers.

## 8. Parser-pending latency and the prepared fast path

The E1 resolver works from exact current source and bounded native edit context;
it does not require full parser certification, wait for quiescence, or pump
inside the transaction. If it cannot prove an E1 case within its cap,
`needsCurrentSemantics` fails that H1 sample.

The edit context reuses parser-owned scanners, container facts, and revision
lineage. It is not a second regex grammar in the runtime. E1 may define a
bounded editing rule distinct from GFM output, but Markdown recognition still
has one Rust implementation.

A missed complete-path gate is diagnosed at the layer that caused it:

- unavailable or slow semantic resolution selects a prepared edit hint;
- worker or pump delay selects scheduling and smaller pump turns;
- FFI, allocation, or copy cost selects buffer reuse or further fusion;
- Core delay selects receipt/publication simplification; and
- Flutter build, layout, or raster delay selects narrower invalidation.

Current evidence makes that attribution important: the headless optimistic
literal lane measured 0.143 ms native acknowledgement p99, while the controlled
1 MiB Flutter lane measured 10.07 ms input-to-frame p99. H1 therefore preserves
the worker placement and attacks extra calls, publications, and layout first;
it does not add an executor abstraction or direct-FFI rewrite without contrary
semantic-path evidence.

Preparation cannot mask a scheduling or rendering defect. If semantic
availability requires it, the first prepared mechanism is only a
`PreparedEditHintV1` for Return or Backspace: single revision, single use, exact
replacement and result selection, opaque native validation token, and
piggybacked on the native edit or viewport receipt that establishes that
revision. It has no separate query, cross-revision carry policy,
connection/window generations, or general cache. If the measured literal ABI
cannot carry a required current-revision hint, H2 pulls forward only the
`SOURCE_TRANSACTIONS_V1` receipt migration; it does not add a prepare round trip.
Flutter generations remain admission checks in `flark`.

That latency rule is separate from the correctness-mandated `prepareCut`
handshake in Section 6.2.1. Cut spans a platform clipboard side effect that
cannot participate in the native source transaction; its closed token exists
only to order prevalidation, clipboard write, and source commit without
pretending those systems share rollback.

### 8.1 Structural projection transition

The current plain-text continuity policy does not cover paragraph splits or list
restructuring, and RFC 027 already proved that recertification-only presentation
can miss the active projection. Structural continuity is therefore mandatory
before the next dogfood; it is not conditioned on rediscovering flicker.

H2 extends the existing transaction-bound `EditPresentationContinuity` protocol
with the smallest operation-specific old-to-new source mapping needed for E1.
It does not create a second projection-transition subsystem and does not claim
certification. Rust classifies the transition in the committed receipt;
`flark_core` maps source-backed presentation runs and publishes a
framework-neutral transitional row or neutral gap. Flutter adapts that model to
selection, layout, and paint without switching on Markdown transition kinds.
Any other Dart frontend consumes the same Core model and otherwise shows the
smallest authenticated neutral island. The speculative `_pendingBlockSplit`
bypass is removed; only receipt-bound local geometry may survive an exact
committed splice, never a frontend-guessed source operation.

## 9. Composition, clipboard, and platforms

E1 suppresses structural intents during an active composition and uses one
normalized composing predicate. This is a containment rule, not the final IME
design.

The final composition model is a framework-neutral Core scope backed by the
native transaction and composite-history primitives:

```text
beginComposition -> updateComposition* -> commitComposition | cancelComposition
```

Core owns the lifecycle because only the frontend can normalize platform
composition observations. The first update retains its exact inverse in one
required native composite token and records the canonical base selection;
updates and commit extend that token through the ordinary source-transaction
gateway. Cancel replays the composite once, discards the generated redo
inverse, and restores the base selection. This avoids a second native edit
protocol while keeping exact source, inverse retention, rewind, and history
state native-authoritative. Real IME qualification requires physical Android
and iOS devices; macOS and widget tests cannot close that evidence gap.

Clipboard acquisition and platform menu wiring belong to `flark`; exact source
replacement, selection, and history adoption use the same `flark_core` command
gateway. The common editing behavior is not reimplemented once per platform.

## 10. Package ownership

- **`flark_runtime` / private native session coordinator**: edit profile,
  bounded resolver, exclusive transaction, inverse/history reservation, source
  linearization, anchor transformation, terminal receipt slot.
- **`flark_abi`**: capability negotiation, handle validation, fixed envelopes,
  one-call dispatch, and bounded result serialization; no general public
  prepare/commit choreography. The closed `prepareCut`/commit-or-release token
  is the sole external-side-effect ordering exception in this profile.
- **`flark_core`**: canonical selection generation, one authoritative mutation
  gate, worker request, receipt validation/adoption, history grouping, ordered
  compact publication, `postCommitUnknown` recovery state, framework-neutral
  source-mapped presentation rows, and receipt-backed transition state.
- **`flark`**: platform observation arbiter and small dependent-observation FIFO,
  input-window reconciliation, connection lifecycle, gestures, clipboard/menu
  adapters, Core-presentation adaptation, selection decoration, layout, and
  frame presentation.
- **`flark_parser` / projection runtime**: Markdown semantics, certification,
  bounded edit context, optional single-use prepared hint, and the existing
  continuity protocol's structural mappings.

No Flutter dependency enters `flark_core`. No Markdown recognizer, marker
incrementer, block-split policy, or delimiter-sensitive carry validator remains
authoritative in a frontend adapter at the destination. A non-Flutter Dart UI
must implement its own input, layout, hit-testing, accessibility, and paint
adapters, but not Markdown edit or structural-transition semantics.

## 11. Test and proof harness

The v4 suite uses ordinary layer-owned tests plus a compact typed temporal
probe. It does not serialize live-editor journeys or replay one universal
fixture through Rust, Core, Flutter, and native platforms: those layers expose
different facts, and universal replay duplicated coverage while obscuring the
owner of a failure.

Rust owns resolver, parser, incremental-versus-clean, cap, and property
matrices. `flark_core` owns transaction, selection, history, queue, and failure
receipts. The Dart controller transition probe records every synchronous
publication around one logical action and its deterministic mutation and
presentation barriers. Mounted Flutter tests add actual paint, layout, and
geometry observations. A few direct native canaries prove only OS input,
pointer, clipboard, and scroll routing.

Earlier Flark suites and public editor suites are mined for cases, but v4
expected behavior comes from `flark-edit-v1`, not from preserving superseded
implementations. The GFM/CommonMark data corpus remains an independent parser
reference; it is not an editor-interaction DSL.

Required perturbations include duplicate newline entrances, stale connection
events, parser pending, an in-flight pump, backpressure, reply loss, history
budget exhaustion, active-composition suppression, composition-end ordering,
non-ASCII window boundaries, mixed line endings, undo/redo, and rapid
alternating Return/Backspace.

## 12. Execution and gates

### 12.1 Implementation checkpoint (2026-08-12)

Implemented: the parser-owned bounded line classifier; the one-splice Rust
resolver/commit path in ready, parser-pending, and initial-building states; ABI
4.10 `EDIT_INTENTS_V1` with required history headroom, anchor transformation,
and one retained terminal receipt; one Dart worker message; the private Core
command gate and receipt adoption; persistent worker error/exit observation
with typed fail-stop; and receipt-first Flutter routing for E1 paragraph and
non-task simple-list Return/Backspace. That was the initial E1 boundary; the H4
checkpoint below records the structural handlers subsequently removed.

Focused evidence is green through Rust, ABI, `flark_core`, and the non-widget
Flutter controller. Release-mode synchronous resolve+commit probes measured
approximately 2.6-2.9 ms for 1-10 MiB ordinary documents and about 0.05 ms for a
1 MiB giant line on the benchmark Mac. These are development probes, not H1
p99 receipts.

The input arbiter now handles delta and full-value Return, treats the paired
macOS action as acknowledgement, admits selector Backspace and other terminal
commands as bounded successors, and reconciles full-value or delta typing in
the exact pre-edit input window. The FIFO is capped at seven dependent
observations and fails closed on overflow or a successor after a deferred
command. Focused regressions cover duplicate delta/action delivery, Return then
Backspace, whole-value typing, cap overflow, and cross-page selection without
viewport navigation mutating the canonical selection.

Injected reply loss recovers the retained terminal by replaying the exact
logical ID/digest, with one source revision and one undo unit. Injected worker
loss fail-stops Core, two independent editor sessions commit without cross-talk,
and a semantic transaction transforms all 4,096 negotiated live anchors while
the 4,097th allocation fails closed.

The reproduced 12,000-byte ordered-list creation failure is not a general open
or document-size cap. It is the parser's explicit `SegmentedLine` refusal when
a block-opening physical line exceeds its 4 KiB direct-classification window;
plain giant lines remain supported. Proving suffix-dependent block semantics
for segmented opener lines is parser-hardening work, not an input-arbitration
patch, and remains tracked for H5.

The valid foreground 1 MiB, 120-observation semantic burst measured 8.322 ms
callback-to-frame p50, 8.900 ms p99, and 8.982 ms max. Editor-attributed work
measured 1.512 ms p50, 2.142 ms p99, and 2.593 ms max; callback-to-receipt p99
was 2.031 ms, worker round-trip p99 1.878 ms, and native FFI p99 1.606 ms. No
sample crossed the frame budget. The headless 5 MiB semantic lane measured
approximately 0.297 ms p50 and 3.815 ms p99, with size-independent development
probes through 10 MiB. These receipts prove the benchmark-Mac H1 path, not
mobile, IME, or the full shape matrix.

H2 now carries Rust-authored split/continue/exit/merge/lift classifications
through ABI and Core. Core owns source-run remapping and neutral-gap geometry;
Flutter contains no switch over those transition kinds. Focused headless,
controller, and real-surface Return checks cover repeated Return plus typing,
Return then selector Backspace, immediate typing behind Backspace, paragraph
merge, list lift, undo, and no raw-marker/empty-surface intermediate state. A
no-Flutter fake frontend consumes the same Core transition model.

The final tree passed the unmasked `verify_v4.sh` gate. The single post-gate
foreground attempt was correctly rejected by the harness after macOS left the
display inactive for 29.6 seconds; its component medians remained 0.360 ms from
callback to receipt and 1.429 ms of editor-attributed latency, but it is not p99
evidence. The earlier valid H1 receipt therefore remains the performance record
for this checkpoint. Maximum-anchor and multi-session *contention
measurements* and proof of bounded zero-state reclamation after worker loss
remain hardening evidence rather than reasons to duplicate the now-green H2
behavior work. Literal source transactions, non-collapsed replacement, task
lists, full composition, and other structural constructs remain H3/H4 work.

### H1 — one-splice vertical slice

1. Freeze the collapsed-caret E1 fixtures and bounded line-ending metadata rule.
2. Add the pure Rust intent/result model and prove resolution from exact current
   source in ready and parser-pending states.
3. Add the one-closure native semantic coordinator, maximum-E1 history headroom,
   per-session gate, and one-slot terminal receipt.
4. Add only ABI 4.10 `EDIT_INTENTS_V1`, its fixed result header, and bounded
   semantic replacement payload.
5. Add one worker message, persistent error/exit observation, and the one-in-
   flight `FlarkCoreEditorSession` gate. Remove hidden edit-triggered pumping.
6. Add the Flutter observation arbiter, single-delta fast path, and small
   semantic-barrier FIFO with exact reconciliation.
7. Route only E1 Return/Backspace and remove only their Dart semantic helpers
   plus `_pendingBlockSplit`.
8. Instrument and measure the complete path before expanding either API.

H1 passes only if:

- every committed fixture produces one logical edit, one native commit, exact
  source/selection, and one required history unit; no-change fixtures produce
  no revision or token;
- duplicate platform entrances cannot duplicate source;
- every rejection is mutation-free and deterministic;
- lost-reply injection recovers the terminal receipt without replay, while
  worker loss is a typed fail-stop followed by bounded zero-state cleanup;
- the normal path performs one worker message, one FFI call, one actor closure,
  zero preflight anchor/coordinate calls, one Core publication, and at most one
  Flutter layout before the first correct frame;
- `needsCurrentSemantics` never occurs in an E1 ready, parser-pending, or burst
  fixture;
- history capacity never evicts or retries on the interactive path;
- anchor transformation passes at the negotiated maximum live-anchor count;
- queue and output caps fail closed;
- ordinary, parser-pending, and in-flight-pump lanes all reach the first correct
  presentation by the next eligible Flutter frame on the benchmark Mac;
- no input backlog is older than one served frame;
- synchronous Rust resolve/commit work is at most 4 ms at p99;
- profiled Flutter frame work is at most 8 ms at p99;
- every editor-attributed frame and synchronous foreground span is strictly
  below 16 ms, including burst samples, with zero hidden dropped frames; and
- latency/cost remains size-independent across 1, 2, 5, and 10 MiB ordinary,
  giant-line, and block-dense documents.

Instrumentation records platform callback, arbitration, queue wait, Rust
resolve, source commit, worker reply, Core installation, and presented-frame
timestamps separately. It also counts worker messages, FFI and actor calls,
coordinate conversions, anchor operations, request/result bytes, Dart/native
allocations, replacement copies, controller publications, layouts, and paints.
An unattended low-refresh display receipt is not claim-eligible.

The interaction lanes include sustained 120 Hz observations and the first edits
after a 32 KiB paste. These gates specialize rather than weaken RFC 026's full
performance evidence contract.

### H2 — stable structural presentation

Carry Rust-authored E1 transition classifications into a framework-neutral Core
presentation transition, then adapt that result in Flutter. Core must retain
only untouched source-mapped runs or a neutral exact-source gap; ambiguous
mapping fails closed. Flutter may decorate selection and paint the result but
may not infer a Markdown transition. A headless fake frontend and the Flutter
adapter must pass the same transition fixtures.

If and only if semantic availability or resolve time caused an H1 miss, add the
single-use piggybacked prepared hint and rerun. No broader dogfood occurs until
Return and Backspace are visually stable through pending parser, burst, and
transition lanes. H2 behavior and framework-boundary checks are green, and the
full v4 gate passed. The post-gate foreground attempt was environmentally
invalid, so its wall-clock tail is excluded rather than rerun as low-signal
work; the valid H1 receipt remains the checkpoint's performance authority.

### H3 — literal transaction convergence

Migrate the measured literal path to `SOURCE_TRANSACTIONS_V1`, eliminating
separate coordinate conversions, postcommit selection-anchor allocation, and
history-retention ambiguity. Add non-collapsed selection replacement, the
atomic anchor-retarget primitive, and native composite history before claiming
the general transaction gateway complete.

Implementation checkpoint: ABI 4.11 and the Core literal lane use one
receipt-bearing transaction for replacements and inverses bounded to one 64
KiB ingress chunk. It validates result selection before commit, reserves the
exact inverse before mutation, atomically retargets the canonical anchors, and
recovers a lost reply from the terminal slot. ABI 4.12 keeps rapid typing and
composition in one bounded native token and replays each group in one source
commit. ABI 4.22 extends the same authoritative receipt, required-history,
atomic-anchor, ordered-terminal, and lost-reply guarantees to replacements and
deletions larger than one ingress chunk. Bytes still enter through bounded
`BULK_BEGIN`/`BULK_APPEND` staging, but only
`STAGED_SOURCE_TRANSACTION_V1` may commit them. The compatibility bulk path now
remains only for the unadmitted case where a large edit requests a noncollapsed
result selection away from the inserted-range end. The common transaction
gateway is therefore complete for normal typing, deletion, paste, and undo.

### H4 — structural and production input matrix

Expand through nested lists, quotes, headings, task items, code blocks, tables,
and exact block-boundary behavior. Add scoped composition, clipboard/bulk
semantics, drag/drop, and accessibility actions. Each increment removes its
temporary Dart handler and shares the same native transaction and fixture path.

Implementation checkpoint: depth-1 GFM task items now use the parser-owned
list context in both ready and exact-pending states. Return creates an unchecked
successor, empty Return exits, and prefix Backspace lifts the item through the
same Rust transaction and presentation receipts as ordinary lists; Flutter no
longer selects its task-specific fallback for those cases. ABI 4.13 adds the
same receipt-backed continue/exit/lift transitions for isolated depth-1 block
quotes and split/exit/lift transitions for ATX headings. Exact quote prefixes
are preserved, empty ATX caret geometry is repaired inside the parser-certified
runtime viewport, and generated successor context remains usable while parsing
is pending. Certified unsupported rows (including Setext headings, nested
quotes, quoted headings, and structural commands inside multiline quotes) now
fail closed rather than falling through to line-local guessing; an absent
certified row may use exact source only for an isolated empty structural
marker.

ABI 4.14 extended the same lane to parser-certified depth-two list rows. Return
preserves the exact container indentation and next marker/task policy; an empty
successor or prefix Backspace removes one exact indentation span and returns an
`outdentList` presentation receipt. Empty-marker correlation uses the preceding
certified list row and exact marker columns so the GFM Setext/list ambiguity is
never guessed. The parser path and pending lineage now handle any bounded pure
list depth whose container contribution is exactly two spaces per level,
removing one level per command even without an intervening parser pump. Current
item marker offsets remain parser-authored.

ABI 4.17 removes that uniform-width restriction for bounded pure-list paths.
The parser packs each ancestor item's CommonMark padding width and publishes the
current marker column; the runtime validates that lineage against exact source,
then continuation and each pending outdent preserve or remove one exact
container contribution. Core exposes only the framework-neutral marker column,
so renderers do not infer indentation from nesting depth. The lineage is capped
at sixteen ancestors and 255 columns; tabs and richer mixed-container paths
remain fail-closed rather than acquiring host spacing policy.
The final Dart structural-newline and prefix mutation fallbacks are removed;
unsupported constructs now receive only literal input until Rust admits an
explicit profile.

ABI 4.15 adds an opt-in `SEMANTIC_PROJECTED` viewport query without changing
the fixed row record. It carries bounded ordered identity segments after the
inline-fact stream and activates the first discontinuous edit surface for
depth-one multiline block quotes. Parser certification, runtime validation,
Core worker serialization, source/display affinity mapping, and conservative
literal continuity are end-to-end; structural Return/Backspace remains a
separate multi-surface receipt problem rather than a frontend text heuristic.
The first structural subset is now closed: nonempty Return resolves one exact
physical quote line inside that certified logical row, inserts the parser-owned
prefix, and maps the committed splice into one marker-free Core surface through
recertification. Prefix Backspace on a later physical line commits in Rust and
Core publishes the two resulting quote/plain surfaces in source order. The
custom render surface consumes the same framework-neutral collection, and an
immediate literal successor maps only its owning surface while preserving the
unaffected peer. Delta/full-value Backspace observations enter the same
semantic arbiter, and a paired desktop selector is acknowledged rather than
committed twice. ABI 4.16 adds parser-authored zero-length rows for standalone
and trailing empty quote lines. The writer tracks an unrepresented container
marker, the viewport publishes its exact final-line prefix/caret, and Return
exits the quote through the ordinary authoritative transaction and presentation
receipt.

The first table slice keeps the parser-owned rendered table active for
conservative literal typing and deletion wholly inside one real cell. A
framework-neutral Core receipt binds the entire burst to that exact cell;
unescaped delimiters, autocompleted cells, cross-cell edits, and syntax-shaped
changes fail closed to exact source. This removes active-raw fallback for the
ordinary cell-text lane without pretending that two-dimensional navigation,
structural table commands, or nested-scroll arbitration are complete.

The bounded H4 table-navigation profile now routes Tab and Shift-Tab across the
ordered real cells in that same parser-authored table model. Flutter owns only
the visible caret destination; it neither counts pipes nor reconstructs table
shape. Autocompleted cells are skipped because they have no source-backed edit
position, and the first/last real cell retains Tab ownership instead of leaking
focus out of the editor. Automatic row creation remains deliberately
unadmitted: it requires a Rust semantic transaction that chooses exact source
style and atomically moves selection into newly inserted source, not a Flutter
pipe-string recipe. Paragraph boundary selectors likewise resolve against one
complete rendered block rather than a wrapped visual line and map back through
the existing hidden-marker-safe surface. Mounted receipts cover table
traversal, list-Tab nonregression, and directional paragraph selection.

Fenced-code content now exercises the same authoritative literal lane without
exposing its parser-owned fences. Duplicate platform action arbitration applies
to literal as well as semantic Return/Backspace observations, and typing
captured behind a structural receipt preserves the native typing-history group.
Portable headless and mounted journeys cover fenced-code Return/type/history,
ordered-list continuation/history, task continuation/exit, and quote
continuation/exit with zero resynchronization and bounded paint predicates.

ABI 4.18 extends that transaction lane to indented code without adding a host
indentation classifier. The parser marks only its exact CommonMark deindent
coverage as hidden-upstream projection, the runtime resolves one physical line
from certified source segments, and the existing one-splice/history/anchor path
commits Return, cross-line Backspace, or first-line lift. The receipt echoes
`continueIndentedCode`, `joinIndentedCode`, or `liftIndentedCode`; Core maps the
same source runs into a marker-free temporary surface for any Dart frontend.
Space, tab, mixed space-tab, residual visible indentation, CRLF, and BOF BOM
cases are pinned, including successive Return while certification is pending.
A repeated Return/type/join/Return burst also proves that a direct structural
receipt crossing the bounded input-window edge does not request successor
reconciliation when no provisional platform edit or successor exists.

ABI 4.19 adds `SEMANTIC_ATOMS_V1` and a forward-delete intent without creating
a second edit path. The first admitted atom is a top-level parser-certified
thematic break: its presentation publishes a zero-width editable boundary,
Backspace or Delete at that exact current boundary commits deletion of the
whole physical row, and Core removes the row using a framework-neutral
`deleteThematicBreak` receipt. Nested thematic rows, stale facts, and other
positions are not semantically admitted. Return and ordinary typing remain
literal edits, so this profile does not silently turn every key at an atom into
a structural command. The runtime owns newline and BOF-BOM preservation, while
the Flutter surface owns only focus routing and divider paint.

ABI 4.20 adds `NESTED_BLOCK_QUOTE_EDITING_V1`. The parser publishes a bounded
root-first lineage of exact physical quote-marker widths for pure quote paths;
the runtime validates that lineage against current source and removes one
innermost container per Return/Backspace command. Nonempty Return continues the
entire prefix, while an empty nested row outdents without an intervening parser
pump. A later nonempty physical-line outdent replaces the complete line prefix
with one line ending plus the remaining outer prefix: deleting only the inner
marker would be source-shaped but can remain nested through CommonMark lazy
continuation. The one committed splice drives anchors, history, Core temporary
surfaces, and Flutter paint. Mixed-container paths and unbounded prefixes remain
explicitly unadmitted rather than delegated to host Markdown inference.

ABI 4.21 adds `SEMANTIC_ACTIONS_V1` without creating a UI-specific mutation
path. A parser-certified task row contributes an exact checkbox byte range;
Core binds a temporary target anchor to that row, and Rust toggles only the
certified marker through the existing one-splice admission, history, anchor,
idempotency, and terminal-receipt machinery. Canonical selection is an
independent anchor set and is preserved, including directional ranges. The
Flutter surface maps its rendered checkbox hit target to this framework-neutral
action and holds only the receipt-backed checked value while parsing catches
up. A portable source-targeted scenario runs the same action headlessly and on
mounted Flutter; the macOS driver resolves that target to painted geometry and
performs a real pointer click. This is the general target-action seam, with task
toggle as its first admitted action rather than a task-specific second editor.

ABI 4.22 adds `STAGED_SOURCE_TRANSACTIONS_V1`. Large replacement bytes remain
bounded during ingress, UTF-8 validation, UTF-16 counting, and inverse capture;
source mutation occurs only after the exact inverse and standalone history
token fit the configured budget. The commit returns the ordinary source
transaction receipt, retargets canonical anchors in the same native critical
section, and preserves idempotent terminal replay if the worker reply is lost.
This closes the normal clipboard and large-deletion correctness gap without
placing clipboard policy or platform UI in Rust.

The H4 host adapter now fills the remaining framework hooks around that same
transaction lane. Flutter's platform toolbar request and secondary-click path
show bounded adaptive Copy/Cut/Paste/Select-All actions anchored by current
render geometry; the actions reuse the existing exact selection and clipboard
methods. In-app `String` drops resolve one painted source caret and enqueue an
ordinary source transaction, so drop data receives no Markdown-special
mutation path. Rich keyboard content and app-private IME commands are forwarded
only through explicit product callbacks and advertised MIME types; Core does
not guess how an image URI should become Markdown. Mounted tests pin drop
placement, configured content filtering, private-command forwarding, and a
real toolbar overlay. Cross-application OS file drops, selection handles,
magnifiers, and physical-device IME/menu behavior remain platform
qualification rather than inferred from these framework receipts.

The first scoped-composition slice now covers the cancellation boundary without
adding another mutation lane. Intermediate updates already form one required
native composite token; `flark_core` can rewind and discard that unit while
preserving every earlier undo token. Flutter recognizes both macOS
`cancelOperation:` and exact precomposition echoes from full-value or delta
input, so either event order converges on the same serialized Core command. A
rejected composing callback clears the platform composing state, commits the
already-accepted prefix as one undo unit, and unpins parser convergence. Core,
controller, and mounted-input regressions prove exact source, restored
directional selection, zero stray redo, and surviving prior history. This is
simulated composition evidence. Core now reserves an outward-affinity native
base-anchor pair before the first composition splice, so reservation failure is
pre-mutation and cancellation reuses the retained pair after replay instead of
allocating after source rewind. A cap test fills all 4,096 live-anchor slots and
proves exact cancellation, directional restoration, and reclamation without
headroom. Real dead keys/CJK/autocorrect/dictation and physical mobile IMEs
remain open before composition is complete.

Flutter focus teardown now commits any already-accepted composition prefix,
clears its adapter range, and releases the scoped base through the serialized
Core tail. The custom text client acknowledges an inbound platform closure via
Flutter's `connectionClosedReceived` contract before unfocusing; a later focus
reattaches one client and republishes exact current editing state. A mounted
regression proves explicit focus loss/regain, platform closure, two exact
post-reconnect edits, zero resynchronization, and no engine fault. This closes
widget connection-reopen choreography, not the still-missing native cross-app
macOS focus receipt or physical mobile lifecycle qualification.

A parser-certified projected-inline regression also proves that an active
composition inserted inside strong text remains exact in source, keeps its
local composing range, and stays marker-free and styled on the shared Core
surface before commit; commit and one undo preserve the same projection. This
closes the simulated hidden-delimiter interaction, not the live platform-IME
qualification boundary.

The standard inline editing cycle now carries a presentation receipt at each
settled checkpoint: an unmatched emphasis marker is an exact local source
island, completion restores marker-free projection, and inserting/removing a
visible space at the closing boundary breaks and restores that projection
without an extra mounted render or selection plan. Paint observations begin
once after initial activation, so later caret or selection moves cannot erase
the edit history being asserted.

Certified direct-link labels now have one portable label/history receipt.
Rapid plain-text insertion and Backspace remain inside the parser-authored
label continuity range; the hidden destination never enters a painted frame,
and two-unit undo/redo preserves exact destination bytes and canonical caret.
This proves rendered label editing, not link-target editing. The latter still
requires the parser's cooked destination/title value and source ownership to
cross the ABI into a framework-neutral Core action before Flutter or another
frontend may offer a popover or activation callback.

ATX heading split and lift now share one portable lifecycle with history:
Return exits the heading, undo restores it, prefix Backspace lifts it to a
plain paragraph, immediate typing remains ordered behind that receipt, and two
undo/redo units reproduce the exact source and selection. The mounted lane
observes exactly the five expected render/selection states and never paints the
hidden `##` prefix.

Flutter pointer routing now separates recognizers by intent instead of enabling
one pan policy on every device: a touch or stylus tap activates text or a
semantic surface action, a touch or stylus vertical drag scrolls without moving
selection, and mouse drag remains source selection. Gesture-arena acceptance,
not pointer-down speculation, decides activation. This closes the original
scroll-select coupling in the framework adapter; fling physics, selection
handles, magnifier, platform menus, and physical-device behavior remain H4/H5
qualification rather than implied by a widget test.

The custom Flutter input client now owns the minimum visual keyboard-navigation
adapter that `EditableText` previously supplied implicitly. Character movement
steps through rendered grapheme clusters and crosses hidden delimiters in one
source-mapped stop; vertical movement asks the bounded painted surface for the
nearest caret at a retained horizontal coordinate. macOS selectors and Flutter
logical arrow shortcuts share that adapter, including Shift extension, while
Core installs the resulting source selection through canonical anchors. This
does not move navigation semantics into Rust: layout-dependent destinations
belong to the frontend. When the current painted target exhausts a fully
materialized bounded page, the adapter now requests exactly one adjacent Core
viewport page, waits for its Flutter layout, and resolves the first or last
rendered caret stop at the retained horizontal coordinate. A reverse Shift+Up
receipt proves the selection base remains Core-anchored across the page swap.
Every adopted vertical caret now scrolls its already-laid-out fragment into
the visible surface and materializes the next bounded overscan region. Arrow
events arriving during a page query enter a 32-command FIFO and drain at one
move per Flutter frame; a three-event receipt proves they are not discarded.
If another interaction changes selection before adoption, the adapter clears
that FIFO and restores the prior page through the normal controller operation.
Word/line/document conventions beyond the implemented subset, bidi
permutations, overflow behavior beyond the bounded FIFO, and accessibility
traversal remain explicit H4 work.

The public `FlarkMarkdownView` shares the same bounded render object and now
shares the device-appropriate scroll boundary: wheel and pan/zoom signals on
desktop, vertical touch/stylus drag on mobile, and no input connection or
selection mutation in read-only mode. This is render-surface parity, not a
second parser or widget-per-block read path.

The render object now publishes stable semantics nodes only for rows inside the
painted viewport. Headings carry header state; task items carry checked state;
the editable surface routes semantic task activation through the same
target-anchor action, while the read-only surface intentionally omits it. Task
touch hit testing uses a 48-logical-pixel target without enlarging the painted
glyph. This first accessibility slice proves bounded traversal and one action,
not a complete editable-text semantics contract, off-page screen-reader
navigation, platform menus, or physical VoiceOver/TalkBack behavior.

The H4 semantics profile now supplies that bounded editable-text contract for
every visible source-mapped row. Its value and local selection are the rendered
projection; set-selection and character/word movement map back through the
same hidden-marker-safe geometry before Core adopts canonical anchors. Copy,
cut, paste, long-press menu, focused state, and accessibility scroll actions
reuse the host adapter's existing command paths. The root exposes only the
current bounded page and scrolls/paginates to reveal more rows rather than
materializing a multi-megabyte semantics value. Read-only Markdown retains
static heading/task labels with no edit actions. Mounted action-level receipts
prove projected selection, one grapheme move, and semantic scrolling;
VoiceOver/TalkBack focus order, announcements, selection handles, and physical
device menus remain H5 qualification.

The basic command adapter now restores two more behaviors that the custom
surface cannot inherit from `EditableText`: Cmd/Ctrl+A installs one exact
document-wide anchored selection through Core, and Home/End plus macOS
left/right-line selectors resolve against the current `TextPainter` line before
mapping back to source. Shift variants extend the canonical selection. Visual
line commands are deliberately not reused for word movement. Word commands use
Flutter's Unicode word-boundary policy over the complete bounded presentation
row, so hidden Markdown markers and internal 256-unit paint fragments do not
create false stops; Apple Option-arrow and Windows/Linux Control-arrow route to
that policy, including Shift extension. Paragraph commands and off-page
navigation remain distinct policies requiring their own evidence.

Touch/stylus long press and mouse double tap now use the same complete-row
Unicode word geometry to install one bounded source selection. Selection
mapping deliberately differs from cursor navigation: its downstream start and
upstream end stay flush with, but never inside, hidden Markdown delimiters.
Replacing a selected rendered word therefore preserves its enclosing source
style instead of exposing or orphaning syntax. The controller publishes the
local range and queues one canonical anchor installation rather than emitting a
transient collapsed selection first. Handles, magnifier, menus, Apple floating
cursor behavior, whitespace tailoring, and physical mobile behavior remain
separate platform work.

ABI 4.24 makes editor-created empty paragraphs an explicit authoritative
lineage. A nonempty paragraph Return retains the two source line endings needed
for a distinct Markdown paragraph; each further Return contributes exactly one
visible empty row, and Backspace reverses one command at a time even while
parsing is pending. Core maps the receipt to either a split, retained gap, or
merge without asking Flutter to count line endings. The retained prior contexts
share immutable state, so repeated blank-row commands do not copy the complete
lineage on every edit.

Literal syntax characters now use a separate presentation-only provisional
surface when parser continuity conservatively declines the edit. It may splice
one bounded exact source run and retain the surrounding source-mapped styles
until recertification, but it is never semantic authority and cannot admit a
Markdown command. This prevents a lone `*` from flashing an entire raw row
without teaching Flutter delimiter semantics. The render surface also aligns
its bounded 256-unit layout tiles to actual visual-line boundaries: a tile cut
can no longer appear as a newline, while giant-line virtualization and
grapheme-safe cuts remain bounded. Portable mounted regressions pin the blank
paragraph and syntax-character cases; a render-object regression pins the exact
257-unit boundary reported during dogfood.

The syntax-character gate now cycles representative emphasis, code, link,
strikethrough, angle-hazard, and escape punctuation through insert and delete.
That expansion found a second whole-row relay for an incomplete `<` opener:
the block row was certified, but its authoritative inline fact set was empty.
An empty fact set may supersede a provisional projection when the edit touched
the styled run itself; when the edit is confined to a plain exact run, the
mechanically unchanged sibling styles remain until nonempty inline facts or a
real block-kind change arrive. This is a general authority rule, not a
character-specific parser exception.

ABI 4.25 crosses the remaining semantic-target ownership boundary without
inflating the viewport hot path. Given one exact parser-authored link or image
fact, Core can query the cooked destination, optional title, syntax class, and
their authoritative source ranges on demand. The runtime resolves direct and
reference links/images plus URI, email, and `www` autolinks; Flutter never
reparses Markdown or decodes destinations. A lookup made while incremental
parsing has retired the prior facts returns no current target rather than an
input-path error, while actual native faults remain typed failures. The
read-only Flutter surface exposes target activation as a product callback;
editing stays caret-first so a frontend may offer its own target popover
without making ordinary label placement activate a link. Mounted geometry-to-
fact and headless fact-to-cooked-target receipts cover the portable boundary;
real pointer activation remains part of the final H4 dogfood pass.

### H5 — hardening and later architecture

Qualify memory envelopes, fault recovery, giant-line raster behavior, GFM and
live-projection matrices, physical devices, and Windows. Design true multi-
splice transactions and batched semantic actions as coordinated changes only
when a proven command requires them.

## 13. Falsification questions

The architecture remains provisional until H1 answers:

1. Can the runtime resolve the collapsed E1 profile from exact current source
   within bounded work while certification is pending?
2. Is the one-call worker path plus one publication fast enough under rapid
   typing, an in-flight pump, and large-document shapes?
3. Does fixed maximum-E1 history headroom preserve correctness without
   materially inflating the dense-document memory envelope?
4. Can the existing continuity protocol express E1 structural mappings without
   a parallel projection model or visible relay-out?
5. Can captured macOS callback traces be normalized into one deterministic
   logical edit without relying on an unavailable OS identity?
6. Does eager transformation of the maximum per-session live-anchor set remain
   inside the Rust p99 budget?

A negative answer selects the mechanism already named in this RFC; it does not
weaken exact-source authority, bounded work, history correctness, or Rust-only
Markdown semantics.

## 14. Rejected alternatives

- **Keep structural Return/Backspace in Flutter.** Rejected: it duplicates
  Markdown policy and has already produced divergent projection behavior.
- **Query row facts, then issue a normal edit.** Rejected: the revision can
  change between query and commit.
- **Optimistically guess semantic source and reconcile later.** Rejected: a
  rejection can expose source/selection state that never committed.
- **Treat the advertised ABI edit count as multi-splice support.** Rejected: the
  runtime, history, anchors, and implementation are one-splice today.
- **Retry an intent after an ambiguous reply.** Rejected: it can duplicate a
  command that committed before transport failure.
- **Make every input event wait for full parser quiescence.** Rejected: it makes
  interaction latency depend on unrelated document work.
- **Suspend an E1 command until a parser pump catches up.** Rejected: it creates
  head-of-line input blocking and cannot satisfy the pending-parser gate.
- **Build a general 32-command provisional rebase graph.** Rejected: ordinary
  success needs one semantic barrier and a tiny FIFO; broader mapping machinery
  waits for evidence.
- **Build prepared intents before measuring the simple fused operation.**
  Rejected: it adds a cache/protocol without evidence that the smaller correct
  mechanism fails.
- **Move foreground edits directly onto the UI isolate now.** Rejected: current
  worker/native acknowledgement is already sub-millisecond at p99; execution
  placement changes only if the semantic receipt attributes a real miss there.
