---
title: DX review decisions
description: Constructive review of the proposed Flark API, before implementation.
---

This is a design review by three independent AI reviewers acting as DX personas,
plus a source-code check. It is not external user research, a code review of an
implementation, or a performance result. The [Using Flark proposal](/guides/using-flark/)
contains the revised consumer-facing examples.

## What the reviewers tested mentally

| Persona | Scenarios |
| --- | --- |
| First-time app integrator | Fetch a note, switch documents, autosave, recover a failed load, dispose the editor |
| Custom composer author | Build a toolbar, handle mixed formatting and read-only policy, edit a stale selection, save a link dialog |
| Cross-platform maintainer | Load native/Wasm assets, use a controller without a widget, render many read-only items, release resources |

All three supported the widget/controller model, automatic loading, typed style
methods, and a separate renderer. The first round found contract gaps that
needed resolution before implementation.

## Changes adopted

| Finding | Decision |
| --- | --- |
| Loading a fetched note could trigger autosave | `loadMarkdown` establishes external content, resets history/selection/typing intent, and emits state notifications but no edit/save event. `replaceMarkdown` remains undoable and emits when content changes. |
| Changing `initialMarkdown` after a fetch looked reactive | Show an editor after data arrives; a document key starts a fresh widget-owned session. Explicit controller loads work during preparation, with the latest accepted load winning. |
| Save subscriptions depended on a mounted widget | Expose `controller.changes`, a stream of Markdown strings for edits. Widget `onChanged` forwards it. Initial content and explicit document loads do not emit. |
| Failed initialization had no custom-control recovery API | Expose status/error and `retryLoading()`. Creation starts preparation; `ready` observes the current attempt. Retry preserves content. |
| “Surgical” ranges actually suggested the old normalized edit | Propose `replaceSourceRange` with an exact source splice and validation. Existing `ReplaceRange` legalizes offsets and can preserve wrappers, so it cannot simply be renamed. |
| A delayed edit could affect a different document | Add `state.revision` and optional `expectedRevision`. The token changes for edits, selection, typing intent, mode, and loads, and is monotonic across document changes. |
| State covered bold but not the rest of the toolbar | Include heading, link, code-language, and mode context with corresponding capabilities. All current built-in controls must be expressible through public APIs. |
| Read-only policy was easy to omit from a custom button | Apply the same application-owned `readOnly` flag in the example. It restricts user input; programmatic edits remain available. Capability is distinct from application policy. |
| A custom toolbar could style a resource dialog but could not launch it | Add `showLinkEditor()` and `showImageEditor()` to the host controller, using the same guarded target and presenter as the built-in toolbar. |
| Programmatic Select All depended on a previous call | Make `selectAll()` deterministic for the whole document, with explicit code-block scope. Keep the keyboard's fence-first behavior. |
| The advertised command suite mixed cleanup with new features | Stage list/quote conversion, clear-formatting, and new code/table structure operations separately. |
| Read-only rendering implied unlimited articles or cheap renaming | Define parent-owned layout/scrolling and optional selection. Retain existing source/shape bounds; measure the dedicated renderer rather than claiming a speedup. |

## Contract details

- **Lifecycle:** one controller may attach to one editor at a time. Unmounting
  an externally owned controller's view preserves the controller. Disposal is
  idempotent, releases owned native/worker state, closes subscriptions, and
  prevents late publications. Pending readiness completes with a disposed
  error; later mutations reject with reason `disposed`. Shared module work may
  finish for other consumers; disposing one controller cannot cancel their loader.
- **Retry:** all waiters share the current attempt. A failed attempt stays failed;
  retry creates the next attempt, or joins an already-running attempt. A retry
  when ready is a no-op. Loader errors are observed internally even when nobody
  awaits `ready`, so widget-only usage produces no unhandled asynchronous error.
- **Document loading:** a valid load clears pending composition/typing intent,
  moves selection to the document start, and resets undo/redo. An invalid load
  leaves the previous document intact. Loading during preparation replaces its
  seed; old preparation completions cannot republish an older seed. Applications
  remain responsible for discarding stale network responses.
- **Notifications:** `changes` is a broadcast, non-replaying stream. Widget
  `onChanged` forwards events only while attached; remounting never replays edits.
  State is updated before notifications. A document load notifies state listeners
  even when resetting a document to identical Markdown, but emits no edit event.
- **Precise edits:** source ranges must be ordered, in bounds, and on valid
  Unicode boundaries. Replacement Markdown must pass source/admission validation.
  No implicit snapping or wrapper preservation occurs. Selection before the
  splice is preserved, selection after it shifts, and an overlapping selection
  collapses after replacement. It is one undo step and does not request focus.
  Whole-document replacement collapses to its end and clears pending typing style.
- **Guards:** validate the revision before composition commits or other side
  effects. Content, selection, typing intent, mode, and document loads invalidate
  a captured editing target; color/theme/readiness updates alone do not.
  Resource dialogs keep the existing guarded target/session model.
- **Resource presentation:** `showLinkEditor` and `showImageEditor` return a
  future edit result and capture the target before opening the default or custom
  presenter. They require one attached editable view; unavailable/loading/read-only
  calls reject with an explicit reason. Cancelling leaves the document unchanged.
  Unmounting or loading another document invalidates the session. These UI
  requests are distinct from immediate data-only `setLink`/image mutations.
- **Snapshots:** state includes Markdown, selection, revision, readiness, mode,
  inline styles, and current toolbar contexts. A mixed heading selection is
  distinct from a paragraph; unavailable controls retain descriptive state.
  Capabilities describe structural eligibility; the edit result remains authoritative
  for destination validation and source-size limits.
- **Caching:** deduplicate module loading/compilation. Per-instance mutable
  parser state and document caches have bounded lifetimes; there is no unbounded
  global cache of note content. Sharing a loader never shares selection or history.

## Implementation sequence

1. **Consumer API:** owned loading/retry/disposal, controller ownership, typed
   methods/results/snapshots, explicit load versus edit notifications, guarded
   source edits, and matching host names. Migrate both built-in toolbars to this
   API so they exercise the same surface as consumers.
2. **Dedicated read-only rendering:** remove editing-only machinery while sharing
   the parser and host rendering primitives. Preserve default limits and supported
   rendering behavior. This is real refactoring, not a `readOnly` rename.
3. **Separate authoring work:** expanded list/quote/table operations and simplified
   code-service opt-in. Existing code-snippet integration remains available meanwhile.

## Gates before calling it implemented

- Starter apps work without initialization helpers on native Flutter, Flutter web,
  Fleury native, and Fleury web, including deployment under a non-root base path.
  Assets are packaged by the supported host build path; consumers do not hand-copy
  Wasm or worker files to satisfy the advertised default setup.
- Delayed/failing loading, concurrent editors, retry, disposal during loading,
  headless readiness, and document loads during preparation have direct tests.
- Autosave receives edits, undo/redo, and programmatic replacements once; loading
  a fetched document never masquerades as an edit. Widget remounts do not multiply
  save subscriptions or reset an externally owned controller.
- Mixed selection, next-character formatting, exact source boundaries, surrogate
  safety, stale targets, composition rejection, undo restoration, and keyboard
  versus programmatic Select All have shared behavioral tests.
- Both public-API toolbars are dogfooded for focus, mixed/disabled states, resource
  dialogs, and rejected operations. Rename-only analysis is insufficient.
- Compare read-only and read-only-editor rendering on one document and many items:
  cold/warm load, memory after unmount, parse reuse, layout/paint, and selection
  overhead. Confirm bounded cache/resource behavior. No performance claim precedes
  these measurements, and larger-document support stays separately scoped.
  Fallback must identify unsupported/oversized rendering and provide access to
  complete source; it cannot silently present a truncated article as complete.

## Review verdict

The second pass supported implementation of the staged design. Small follow-ups
made stream replay, shared-loader disposal, resource-dialog entry points, and
read-only fallback explicit. There is no requirement for an application-owned
backend or global initialization.

The revised design is approved for implementation. This change remains
documentation-only. Local site checks validate the document,
not the proposed APIs. Package implementation and performance qualification are
still ahead of us.
