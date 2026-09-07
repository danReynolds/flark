# Native qualification continuation — 2026-09-04

D0 remains open. This session resumed after the owner's “go ahead.” Native
diagnosis was followed by browser dogfood, which found and corrected three
families of editing defects. The normal app has been rebuilt from those fixes;
the previous candidate receipt is historical. The refreshed local record is
[candidate.json](receipts/2026-09-04-typing/candidate.json).

The [September 5 qualification extension](receipts/2026-09-05-workbench/qualification.json)
records subsequent test/workload changes against the same normal application.
The later [native profile session](native_profile_2026_09_05.md) records the
completed sweep, its five performance failures, and the interrupted longer run.

## Candidate verification

Before launch, every entry in the local candidate input manifest and every
recorded macOS/Web/Wasm artifact hash matched
`receipts/2026-09-04-local/candidate.json`. The normal macOS candidate launched
from its recorded build path. Its initial Draft contained the earlier synthetic
canary `osabca`; the resumed session added synthetic punctuation/characters.
No Dune message was sent and no application data outside this workbench was edited.

## Observed native condition

The native text-field accessibility value could advance while the rendered
text and byte-count label stayed at a previous value. A fresh diagnostic build
with `FLARK_TRACE_INPUT=true` established that a physical-key route reached
Dart exactly once as KeyDown, one delta delivery to an active input client, and
KeyUp. The app was not paused in the debugger; its main thread was idle rather
than stuck in a kernel edit.

Read-only VM-service inspection of the live `WidgetsFlutterBinding` found:

- `AppLifecycleState.hidden`;
- `_framesEnabled == false`;
- `_hasScheduledFrame == false`;
- the first frame had already been sent, with no deferred first-frame count.

The current Flutter macOS embedder derives this state from AppKit application
visibility/occlusion. A captured window and an input-accepting target do not
prove that AppKit considers the app visible and foregrounded. Flutter's launcher
also reported `Failed to foreground app; open returned 1`. CUA Raise, native
clicks, and a Finder activation attempt did not establish a resumed lifecycle.
The Finder attempt was abandoned when its UI state changed.

The owner was asked to bring Flark to the front on an awake display. Until
Flutter reports resumed with frames enabled, neither the apparent lag nor frame
timings can qualify or falsify the final live envelope. Do not patch the editor
to ignore the platform lifecycle as a way to produce a passing receipt.

## Profile correction

`example/integration_test/frame_profile_test.dart` now waits for an actual
resumed lifecycle after the first mounted workbench, requires foreground state
and enabled frames before input and after its proving paint, and rejects a run
that loses foreground status. Receipts include lifecycle transitions. Analysis
passes after this change. The guarded run waited for foreground activation and
failed with `AppLifecycleState.inactive` after 82 seconds. This is a negative
calibration of the environment guard, not an editor performance failure.
Profile preferences and timing listeners now clean up through test teardown,
including when a run fails.

The Mac subsequently locked again. CUA explicitly required manual unlock. The
owned profiling process was stopped before rebuilding the normal `lib/main.dart`
target. Its new hashes are recorded, with no diagnostic Dart defines. The normal
binary must still pass native canaries before owner dogfood handoff. There is no
new native pass.

## Browser dogfood and corrections

The rebuilt Flutter/Wasm workbench was exercised through actual browser keys,
with source inspection, caret offsets and screenshots checked separately.
This found defects that the preceding green local checkpoint had missed:

- Typing a space at a paragraph's end moved the caret before that space, so the
  next word appeared before it. Rust content records now retain editable
  trailing whitespace, including table padding and spaces before existing
  LF/CRLF breaks. Space-based hard-break markers expose their spaces as content;
  deleting across the break still removes the full parser-authenticated marker.
- The larger generated run then exposed a multiline link whose exported range
  ended before its title. The extractor now derives the complete link/image
  closure and following break from its authenticated content, including quote
  prefixes. Projection inserts only parser-reported inline breaks, so physical
  lines inside an owner do not create extra rendered lines or expose its title.
- The table browser retest found a second cause of dropped spaces: a greedy
  full-value diff placed an insertion after an identical neighboring space and
  rejected it as unrelated. The Flutter bridge now authenticates a replacement
  at the current selection first, including repeated-character insertions and
  forward/backward deletions. Stale-input rejection remains covered.

The first whitespace family failed all 15 cases before correction; table and
interior-break variants extended it. The multiline-link history was minimized
to a direct failing case. All ten repeated-character bridge cases failed before
their fix. Expectations that previously hid editable table padding were changed
as an explicit behavior correction, not silently weakened assertions.

The new local checks pass: 249 kernel tests, 84 host tests, 11 workbench tests,
40 Rust tests, 1,000 generated histories of 40 commands (seed 2026), and identical
native/bundled/fresh Wasm models across 1,322 corpus cases. The final dense facade
diagnostic measured insert 2.140 ms p99 and Backspace 1.936 ms p99 at 16,694 bytes
and 769 blocks. That headless diagnostic still exceeds the workbench shape
budget and does not qualify the 32 KiB production input-to-raster envelope.

Observed browser results after rebuilding the corrections:

- `alpha beta  gamma!` preserved both word order and exact spaces; Undo/Redo
  restored the observed before/after sources and selections.
- Typing two spaces before `alpha\nnext` kept the caret at offsets 6 then 7;
  subsequent `beta` produced `alpha  beta\nnext`, with the caret at 11.
- Inserting beside table padding produced `| alphabeta  |`, then
  `| alphabeta next |`; the corresponding caret offsets were 12 and 16.
  Undo/Redo restored the table states, and the painted cell matched inspection.
- A multiline link with a title rendered only `link` and the following
  `: next` line; the destination and title remained in the source inspector.

These are functional browser smoke results, not browser timing or full web
qualification. Native AppKit routes, production frame/lifecycle budgets,
source/opening transitions and final normal-binary interaction remain open.

## Milestone reflection

The architecture still kept each repair in its owning layer, without a second
Markdown recognizer in Flutter. The earlier passing test counts did not establish
dogfood readiness. Ordinary multiword typing now starts the browser/native
checklist, and direct sequences assert source order and caret after every
character. Generated invariant runs supplement that explicit behavior oracle.

## Dune integration location

The sibling Git worktree `/Users/dan/Coding/dune-pr42`, on
`tailscale-library-overhaul`, contains the existing application. Its
`lib/widgets/inputs/desktop_message_input.dart` accepts a caller-owned
`TextEditingController`, `FocusNode`, attachments and a submission callback;
`mobile_message_input.dart` has its own submission path. The current
`MarkdownSyntaxController` paints regex-based syntax over source text.

This resolves where the older composer implementation lives, but does not
establish that this networking worktree is the intended migration branch. It
was inspected read-only. Native qualification comes before changing that
consumer, and its existing attachment/submission ownership should remain with
the application when replacing the editor.

## September 5 continuation

Before launch, the 171 candidate input hashes and all six recorded artifacts
matched the typing candidate. The Mac was unlocked and the normal app opened.
CUA key delivery changed the native text-field value, but the captured Flutter
surface and byte label retained an earlier state; a toolbar mode change also
failed to produce an observed new frame. This does not close the foreground or
native input gate. The owner was asked to bring the app forward manually. No
valid resumed-lifecycle performance run was obtained.

Review found that D0 named opening, source transitions, reflow, memory and
sustained-use requirements without a complete V5 workload and numeric limits.
The [macOS qualification contract](macos_qualification.md) now states those
limits before measurement. A complementary production-workbench profile adds
100 measured open/edit/close cycles after warming all shapes, source fallback,
boundary Undo/Redo, inspection reflow, real save draining and at least five
minutes of ordinary multiword typing/deletion. Its raw measurements link input
timestamps to engine raster frames and record parse calls and process RSS. It
has been analyzed but has not run in a valid native foreground session.

Thirteen new mounted cases exercise all seven live 32 KiB shapes and six
source-fallback fixtures. They check every edited paint, the exact caret,
source-page disclosure, visible text at the caret, the next command and saved
source. The full workbench suite now passes 24 tests and analysis is clean.
Earlier core/host/parser results still refer to unchanged inputs; they were not
rerun or newly claimed as native proof.

The new tests needed two oracle corrections: adjacent typing and deletion share
an undo group, so a real caret excursion isolates the deletion being undone;
and disclosed source paging can split a line, so visible text is compared with
the active page while canonical source is checked in full. These were test
assumptions, not new editor fixes. The production editor and normal application
artifacts are unchanged. D0 remains open, and no commit or push was made.
