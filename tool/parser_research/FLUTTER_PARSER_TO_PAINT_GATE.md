# Flutter parser-to-paint gate

Status: **bounded authority and frame-adoption mechanism pass; production
input, layout, adapter, and device gates pending**, 2026-07-18.

## Decision

Flutter must not own or materialize a second document-sized Markdown model.
It owns canonical source/input state, selection, composition, and a bounded
editable input island. The Rust host store remains the only persistent
measured-green structural representation. Flutter receives only bounded
viewport data for presentation.

The input island is an exact source range, not a whole-document
`TextEditingValue`. Its local UTF-16 edit, selection, and composing offsets are
translated to canonical-source coordinates using `globalStartUtf16`. This
keeps a 10 MiB or 100 MiB document out of Flutter's per-keystroke string and
diff path while retaining exact input authority at the edit site.

The first executable gate is in `lib/src/v3/flutter/` with focused tests under
`test/v3/flutter/`. It is internal v3 prototype code and does not alter v2.

## Frame-coherent authority model

Each source edit advances canonical source and host source authority
synchronously. The editable controller's exact bounded value, selection, and
composition advance in the same transaction. Parser/host completion never
rewrites that value and never remounts the editable subtree.

Presentation adopts only on a scheduled frame callback:

1. Before any exact structure exists, paint is a typed `SourceGap`.
2. After source advances while an old ACK is retained, old output is allowed
   only as `stablePaint`. It can identify inert visual cache material, but its
   Markdown semantics, accessibility semantics, semantic actions, hit targets,
   and structural selection maps are invalid.
3. A stale or wrong-source ACK cannot regain authority.
4. An exact-current ACK is queried under explicit viewport budgets and becomes
   `exactStructural` atomically at a frame boundary.
5. An exact-current bounded query that cannot satisfy its structural budget
   remains a typed `SourceGap`; it does not guess structure.

The controller coalesces rapid source, selection, and composition changes into
one pending presentation callback. Its live authority guard invalidates old
semantics synchronously, even during the interval before the next frame adopts
the visual state.

## What the executable gate proves

The fake store is intentionally only an adapter/ownership witness. The focused
widget tests prove:

- a Unicode edit and an IME composing value remain exact and UI-authoritative;
- rapid edits coalesce to one presentation frame;
- input-island local UTF-16 offsets map to exact global source and query
  coordinates, including UTF-8 byte conversion;
- selection-only movement schedules a new bounded query without changing the
  source revision;
- a caret that divides a UTF-16 surrogate pair yields `SourceGap` without
  querying a guessed structural position, then recovers on the next valid
  caret position;
- old ACK paint remains inert while the host is pending;
- a stale ACK is rejected and cannot enable semantics or interaction;
- exact-current publication enables bounded structural presentation,
  semantics, and hit targets together at a frame boundary;
- typed host `SourceGap` output fails closed; and
- the same `EditableTextState`, `TextEditingController`, text, selection, and
  composing range survive every host and paint transition.

The test does not call `source.toString()` as part of controller attachment or
editing. Whole-source string comparisons in the small fixture are assertions
about canonical-source correctness, not runtime architecture.

## Deliberate prototype limits

`FlarkV3LiveEditorPrototype` uses a read-only `EditableText`. Tests inject exact
delta-shaped transactions and therefore prove value-level selection and IME
ownership, not a production platform text-input connection. The `Column` that
shows a paint witness above the editable is also not the final interpolated
Markdown layout. It proves stable subtree identity and authority-controlled
semantics/hit testing only; it does not prove caret geometry, mixed rich/source
line layout, scrolling, or visual polish.

The fake host does not prove packed-green decoding, persistent splice
complexity, viewport reconstruction, retirement bounds, FFI/Wasm ABI
correctness, or parser-to-paint latency. Those mechanisms belong to the Rust
store and real adapters, not to a duplicate Dart tree.

## Foreground bounds are sealed but not yet device-calibrated

The first audit found that the prototype named bounded values but trusted its
internal caller to choose them. `FlarkV3InputIslandSnapshot.maximumUtf16` had
no platform ceiling, `FlarkV3HostQueryBudget` checked only positivity, and
`pollHost`/the structural query execute synchronously on the calling isolate.
A caller could therefore configure a document-sized input island or viewport
without violating the Dart types.

The executable gate now closes that configuration hole with one privately
constructed `FlarkV3FlutterForegroundProfile`. Attachment rejects an oversized
island or structural query, ordinary edits reject an oversized replacement
before `String.replaceRange` or source mutation, and host grants/publication
chunks fail with a typed `foregroundBoundExceeded` result before calling the
store. Focused tests prove rejection is side-effect free.

The profile currently caps:

- editable-island UTF-16 extent and neighboring context;
- ordinary synchronous operation count and replacement payload;
- structural query encoded bytes, leaves, open depth, decoded facts, and
  projection runs;
- publication bytes/records adopted in one UI callback; and
- layout/shaping work scheduled from one structural viewport.

The values are conservative host-informed prototype ceilings, not launch
constants. The shipping foreground adapter must select a floor-device-
calibrated profile internally; application code may not raise it. Oversized
input must route **before** `String.replaceRange` or a whole-string
`TextEditingValue` is constructed. Large paste/open remains an exact
provisional source intent, but the platform-facing editable value contains
only a bounded active slice; certification, UTF-8/hash construction, and the
remaining source/layout pages stay worker-owned. Raising a budget is a product
profile change requiring native/web frame receipts, not a widget parameter.

The real adapter must also make its execution context explicit. Parser and
source certification polls never run on the UI isolate. A main-context host
query or publication adoption is allowed only as a measured bounded persistent
read/copy/decode kernel; if its tail misses the foreground deadline, it becomes
resumable across callbacks or moves off-isolate. The current fake store cannot
establish that property.

The prototype now exercises the previously missing bulk/island control shape:

- an oversized replacement routes into the exact provisional source before
  UTF-8 encoding or a document-scale `String.replaceRange`;
- a fixed-work preflight chooses a scalar/CRLF-safe island containing the
  global selection extent and active composing range;
- only that bounded source range becomes the next `TextEditingValue`;
- cross-island selection remains document-coordinate authority while
  `EditableText` receives a collapsed local extent proxy; and
- a source-free island move preserves active composing text and global offsets
  exactly, while an impossible composition fails before source mutation.

The focused widget gate is 17/17 green, and the complete v3 Dart/Flutter gate
is 66/66 green. The 100,000-code-unit paste receipt reports zero replacement
UTF-8 bytes/chunks on the foreground route and retains a 64-code-unit editable
island. Typed Flutter insertion, replacement, deletion, and non-text deltas now
map directly to the ordinary or bulk transaction without applying a giant
delta to the old island string. This is executable control evidence, not a
device latency result.

A same-process scaling receipt deliberately warms the VM path at 10,000 code
units, then measures 100,000, 1,000,000, and two 10,000,000-code-unit
insertions. Across the latest two runs, warmed calls ranged from 116 to 917 us;
the isolated cold/JIT call was about 8 ms. The absence of monotonic
payload-size growth supports the constant-adoption/bounded-island model. A
160-edit randomized ordinary/bulk trace also stays exactly equal to a String
oracle across repeated rebases. These host-VM numbers are diagnostic evidence
only: AOT native, web, GC pressure, and frame scheduling still decide launch
readiness.

The remaining production HOLD is therefore connecting this typed adapter to
the production `DeltaTextInputClient`, document-owned cross-island selection
paint/commands, and platform timing—not the bulk/island architecture itself.

## Remaining production gates

The direction should advance only through these concrete gates:

1. Connect Flutter's delta-input protocol to the proven exact ordinary and
   bulk/handoff transactions without diffing full strings. Exercise automatic
   handoff as caret/composition crosses an edge and implement document-owned
   paint/commands for selections spanning islands or source shards.
2. Join the UI-owned native FFI store and the separately instantiated web
   main-context Wasm store to the same Dart interface. Exercise transferred
   publication buffers and bounded copied viewport DTOs.
3. Decode and render a real structural viewport around the active island while
   keeping one editable subtree. Prove interpolated delimiter reveal, inline
   styling, fenced-code transitions, caret/selection geometry, scroll
   anchoring, and no duplicate visual text.
4. Prove hit testing, accessibility, semantic selection, and keyboard actions
   are enabled only by an exact-current structural viewport and degrade
   coherently through pending and `SourceGap` states.
5. Run randomized parser delay and rapid-edit traces at 60 Hz and 120 Hz. Record
   input latency, frame build/raster tails, bounded query time, copies,
   allocations, GC, Wasm memory growth, and recovery from superseded work on
   floor devices and representative browsers.
6. Run physical-device CJK and marked-text IME cases, emoji/grapheme and
   malformed-scalar-boundary cases, hardware selection/caret movement, large
   paste, undo/redo, accessibility traversal, and focus/keyboard lifecycle.
7. Repeat the above on large documents where document size is deliberately
   decoupled from input-island, publication-chunk, and viewport sizes. Any
   operation whose foreground cost grows with the whole document is a gate
   failure.

Passing the current gate strengthens the ownership and scheduling model. It
does not yet establish production UX or end-to-end performance; those claims
require the real adapter, renderer, and device receipts above.
