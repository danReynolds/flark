# Flark v4 bounded input-window contract v1

Flutter exposes one bounded source window to the platform text-input client.
The window is an adapter, never a second document. Rust owns canonical UTF-8,
revisions, and anchor transform/resolve. `flark_core` owns the canonical
selection as typed Rust anchor handles plus affinity and a nonzero selection
generation. Flutter owns only the active adapter's serialized selection shadow.

## 1. Callback authority

Revision alone is insufficient because a window can move or reconnect without
changing source. Every attached Flutter `TextInputClient` adapter has a
nonzero, immutable `connectionEpoch` tied to its platform client ID. It owns a
serialized shadow containing the represented source revision, exact global
UTF-16 range, nonzero `windowEpoch`, SHA-256 of the complete window text, and
exact global selection generation. `windowEpoch` increases within one
connection and resets to one on a new connection.

Flutter does **not** echo those fields. The `authority` object in the
executable matrix is the host snapshot attached when dispatch begins, not an
invented platform payload. The platform client ID selects the adapter and thus
the immutable connection epoch; the adapter supplies the rest from its current
serialized shadow. It processes one callback completely before dispatching
the next and advances the shadow before returning from an accepted callback.
Flutter's client-ID dispatcher, or the retired adapter if it is invoked by a
test double, ignores callbacks for retired clients without consulting local
offsets. It may record a typed `retired_connection_epoch` diagnostic, but it
cannot transition or close the active adapter because stock Flutter drops a
mismatched client ID before invoking that client.

Any host-originated change to the exposed range, text, or selection retires
the old client and opens a new `TextInputConnection`/client ID before sending
one `setEditingState`. This includes a same-revision window move and a gesture
or external selection change. Ordinary platform-originated typing and
selection updates stay on the active connection and advance its serialized
shadow in callback order. During composition, a host-originated move is
deferred so the active platform client is not replaced under the IME.

Within the active client, a mismatch in current source revision, sequential old
text, bounded range, or current shadow mutates nothing, retires that connection,
and enters `resync_required` with a typed reason. Reconnect/resync mints a new
connection epoch, so a callback from a retired connection is harmlessly
ignored and cannot address a new window at the same revision. Selection
generations are host authority, not platform metadata: a platform selection
callback is current only on the active serialized client; any host-driven
selection replacement first reconnects.

Delta batches are atomic. Flutter supplies each delta's actual `oldText`; the
adapter hashes it and requires the first hash to equal the serialized shadow,
then requires each delta's old hash to equal the prior delta's new hash. The
final hash must match the proposed full editing value. The adapter validates
every range, selection, composition, hash, and the **whole batch** runtime edit
envelope before choosing a commit path. That envelope is 32 bytes per edit
descriptor plus one exact descriptor-order packed replacement partition plus
every deleted source UTF-8 byte. It is not a per-delta or replacement-only cap.
A bad or over-cap second delta cannot leave the first applied.

If Flutter delivers only a full editing value, one bounded full-value diff is
allowed only on the active serialized client, with no unacknowledged
host-originated exposure. Its old value is the adapter's current shadow, not a
field claimed to come from Flutter. The replacement, selection, composition,
byte caps, and resulting complete hash are validated before one transaction.
Otherwise the callback triggers resynchronization; full-value fallback is
never a blind overwrite.

## 2. Selection authority

The exact selection is a `flark_core` snapshot `(baseAnchor, extentAnchor,
affinity, generation)`. The anchor handles are opaque Rust authorities resolved
at the named source revision; Dart owns their pairing, active extent, affinity,
generation, and history grouping. When it fits, the platform receives the
corresponding resolved local selection. When it exceeds the window,
`flark_core` retains the exact anchor snapshot and Flutter exposes only a
collapsed active-extent surrogate plus that generation. Typing, paste, or
deletion against a current surrogate replaces the complete exact global
selection atomically; it never inserts at the surrogate caret. A current
platform caret move asks `flark_core` to replace the canonical selection and
increment its generation. A stale selection callback resynchronizes without
changing the selection or source. A callback from the client retired by a host
selection change is ignored without disturbing the new active client.

`(sourceRevision, generation)` identifies exactly one canonical snapshot and
therefore exactly one resolved base/extent projection. It cannot be reused for
a different selection. Composition metadata retains the full precomposition
selection snapshot at its base revision, including its Rust anchor pair; cancel
never reconstructs authority from integer offsets alone.

## 3. IME lifecycle

Composition updates are atomic text + selection + composing-range editing
values. `ime_update` starts or replaces the current composition and retains the
precomposition source slice and selection under a composition generation.
Every accepted intermediate composing value is one canonical Rust source
transaction and revision: it is immediately visible to exact source reads,
paint, and export. There is no private Flutter text overlay and therefore no
second source authority.

`ime_commit` applies any final text change and clears composing. `ime_cancel`
restores the exact precomposition slice and selection in one new canonical
revision. `flark_core` groups the accepted composition-update tokens and final
commit into one user undo unit, or discards that group after a successful
cancel; intermediate updates are revisions but not separate user undo steps.
An application that does not want marked text saved must explicitly commit or
cancel composition before save rather than observing a hidden source.

Composition is capped and must remain wholly represented. A composition whose
UTF-16 extent exceeds the composition cap, an external revision, a stale
callback, or an unprovable boundary mutates nothing from that callback, sends
`clear_composing` followed by `close_connection`, and enters
`resync_required`. A composition that fits that cap but exceeds the 4 KiB
whole-edit envelope uses the staged bulk sequence below. Delta and full-value
IME paths use the same authority and atomicity rules.

Movement that would evict composition is deferred. There is exactly one pending
window demand; later demands replace it. Commit or cancel applies the latest
demand after its source transaction, adjusts it if needed to contain the final
selection, retires the composing client, and exposes only the final window on
a new connection. No intermediate deferred window or `setEditingState` reaches
the platform.

## 4. Unicode and bounds

Exposed window endpoints and accepted edit endpoints must be proven UTF-16
scalar boundaries, must not split CRLF, and use `characters` 1.4.1 / Unicode
16.0.0 extended-grapheme boundaries for editor movement and deletion. Exact
source is never normalized.

A bounded context query that cannot prove a grapheme edge returns
`needs_more_context`, mutates nothing, and queues a coalescible expansion. It
never guesses or scans the rest of the document synchronously. If one cluster
cannot fit the maximum window, the platform connection closes with typed reason
`grapheme_exceeds_window`; source remains exact and a higher-level bounded
source operation is required.

The window is bounded independently in UTF-16 and UTF-8. A small cross-edge
edit may expand/recenter within both caps. The synchronous small-edit path is
admitted only when the complete callback batch envelope—descriptor bytes,
packed replacement bytes, and deleted source bytes—is at most 4 KiB.

Anything larger, including a long selected-range deletion, an otherwise valid
IME value, or a multi-delta batch whose aggregate crosses the cap, follows one
staged sequence:

1. Validate the complete callback and begin/copy bounded bulk staging. Close
   the platform client and return `staged_bulk_required` without changing the
   canonical source revision, selection generation, or composition.
2. Append and pump only bounded chunks/work units. Each `bulk_progress` result
   carries the latest token and leaves canonical source and selection unchanged.
3. On successful completion, commit the entire original callback atomically as
   exactly one source revision and one selection generation, then reconnect and
   expose the final authoritative state once. Failure or abort preserves the
   old authority.

No stage or pump publishes a partial delta, selection deletion, or composing
value. Closing and faults are terminal.

The executable transition and negative-case denominator is
`test/fixtures/v4/input_window_matrix_v1.json`.
