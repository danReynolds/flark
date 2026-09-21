# Consumer API and release-preparation review — 2026-09-21

The approved widget/controller design is implemented at `c25d3fb`. Both hosts
have `FlarkEditor`, `FlarkController` and a dedicated `FlarkMarkdown`. The homepage
uses this API. No global initialization or consumer-owned parser is needed.
This is a local implementation review and validation record by the implementing
agent, not an independent peer approval, CI result, or stable-release certification.

## Scope and compatibility

The candidate also reconciles the six previously local commits for composer
playgrounds, formatting state, fence exit and native qualification tooling.
Their historical receipts remain dated and scoped to the original runs. The
current core, host and example suites were rerun after integration.

Existing advanced consumers can retain the engine/controller model by changing
the host import to `flark_flutter_legacy.dart` or `flark_fleury_legacy.dart`.
The default host imports now expose the new names; this is a source-level API
change in unpublished development packages. Parser and snippet services remain
available through the explicit advanced imports.

## Review findings and corrections

| Concrete scenario | Correction and coverage |
| --- | --- |
| A note fetch completes while parsing is loading | Latest explicit `loadMarkdown` wins, without a save event; session tests cover readiness, retry and late disposal. |
| An autosave listener performs another edit | Queued, non-replaying change publication preserves order and avoids synchronous-stream reentry; nested edits after a load still emit. |
| A rejected command arrives during IME preedit | Admission/revision checks precede mutation; composition and history restore on rejection. Tests preserve source, caret, revision and undo state. |
| Exact source replacement splits a surrogate or uses a stale target | Reject without snapping or altering history; valid replacement preserves directional selections and one undo step. |
| A custom link dialog outlives its document | The mounted host supplies the existing guarded resource session; stale submission rejects and unattached controllers report unavailable. |
| The toolbar moves focus or switches documents during loading | The host attaches input when a pre-focused view mounts; homepage tests cover switching, independent histories, and next input. |
| `updateImage` is called outside an image | Reject instead of silently inserting. Link capability now uses the same structural eligibility check as the command, including multiline/code selections. |
| Read-only rendering was just an editor with editing disabled | A dedicated parsed document shares projection/layout but has no history, composition, indentation lane or IME claimant. Selection reuses parsing; oversized source gets an explicit complete-source fallback. |
| Accessibility tries to select or copy read-only content | Flutter exposes selection/copy without a text-mutation action. Fleury exposes document text and optional copy semantics. Native accessibility usability still requires qualification. |
| A disposed browser parser remains referenced | Owned buffers and instance/export function references are released; compiled module sharing remains independent of document ownership. |
| API examples require setup that the guide omits | Minimal native/browser entry points and the actual homepage now use the public API. A fresh external Flutter app built with only the package dependency and widgets. |

The review followed ownership through loader, session, input adapter, mounted view,
read-only projection, resource dialog and disposal. It also checked capability
state against editing admission and checked README/guide samples against the
implemented signatures. The generated Wasm is verified byte-for-byte against the
existing parser asset; Markdown recognition remains in Rust/Comrak.

## Local validation

Flutter 3.44.4 / Dart 3.12.2, Apple M1 Pro, macOS 26.2:

- Strict analysis: core, Flutter host, Fleury host, both examples and homepage.
- Full suites: **866 core**, **830 Flutter host**, **111 Fleury host**,
  **38 Flutter example**, **6 Fleury example**, **3 homepage** tests passed.
- Subsequent capability-wiring edits passed **19 Flutter** and **24 Fleury**
  focused consumer/toolbar/resource tests; final analysis remained clean.
- `python3 packages/flark/tool/embed_wasm.py --check` and `git diff --check` passed.
- Production homepage/docs build passed under `/flark/`, including hashed assets.
- Minimal Fleury native bundle built; standalone dart2js consumer loaded at a
  nested browser URL with no Wasm fetch/copy, and accepted typing, undo and source mode.
- Clean external Flutter app: web JavaScript, web Wasm and macOS release builds
  passed with no parser initialization or app-managed parser assets. Native launch
  visibly rendered both the read-only article and editor. The first macOS attempt
  ran out of disk copying Flutter symbols; task-owned build caches were removed
  and the retry passed. Framework-name warnings remain below.
- Homepage browser journey: blank draft, typing, bold, exact Markdown source,
  switching starter tabs and typing into the preserved draft. Tests also cover
  copy, undo/reset, theme preservation and a 320-pixel control surface.

Browser accessibility briefly reported the previous document's input value after
switching drafts, while the painted document and counter were correct; refocusing
showed the correct value and the next edit reached the right document. This is an
open accessibility qualification issue, not evidence of document corruption.
The native app's automation accessibility tree did not expose useful editor
semantics in this launch; the screenshot establishes rendering only.

## Performance findings

At `c25d3fb` on the machine above, the local AOT diagnostic compares the same
Comrak-backed core with and without the public session. At 14,750 source bytes,
engine insertion p50/p99 was **1.192/1.758 ms**; session insertion was
**1.452/2.848 ms** (200 samples each). The state API has measurable publication
cost. These exclude layout/paint and do not qualify a full frame or a device.

Read-only construction of one 4,731-byte document was essentially equal to the
engine (**0.595 vs 0.596 ms** median); a batch of 100 was **69.410 vs 62.885 ms**.
There is no demonstrated parsing/construction speedup. The dedicated path removes
editing state and reuses unchanged parsing; retained-memory and full-host list
measurements remain open. No document limits were raised.

See [diagnostic receipts](receipts/consumer-api-2026-09-21/README.md) for workloads
and raw values. Earlier native workbench timings do not qualify the new API path.

## Remaining release gates

1. Publishable dependency graph and native artifacts for claimed targets; clean
   archive consumption with Rust absent. Current development builds use local
   crates, and Fleury still needs its documented Git/core override.
2. Resolve native build warnings about architecture-dependent framework names
   for parser and snippet assets before distributing binaries.
3. Attended OS input, lifecycle, VoiceOver/accessibility, physical IME/device and
   terminal qualification. Existing native/profile receipts close only their
   specific workloads; the new browser/native observations above do not close these.
4. Public-session host-frame performance and read-only layout/paint/retained-memory
   qualification on target hardware, retaining the measured bounded document model.
5. Choose a version, inspect exact package archives, tag and publish. No package
   publication or version change is part of this candidate.

The repository merge and Pages deployment are delivery steps tracked by the PR;
this document does not pre-claim either. CI is intentionally skipped for the
code commits per the working preference; the Pages deployment runs separately.
