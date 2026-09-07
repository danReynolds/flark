# Browser dogfood review — 2026-09-05

**D0-web is ready for exploratory owner dogfooding in the observed Codex embedded
browser. D0-macOS remains open.** This is a local dirty candidate, identified by
source and build hashes in the [receipt](receipts/2026-09-05-web/candidate.json),
not a release or a universal browser performance qualification.

## What browser use found

The Wasm parser and Flutter web application already existed. Running the normal
workbench exposed gaps that the decoded platform-input tests could not detect:

| Finding | Owning correction | Regression evidence |
| --- | --- | --- |
| Browser paste changed the DOM input but left the document unchanged when `beforeinput` metadata was absent. | Web requests full values from Flutter's bounded input context; native keeps delta input. | Real browser engine-message test fails before the fix, then checks exact source, every observed paint and the next key. |
| Select All followed by replacement across structured blocks was rejected; a leading heading marker was outside the selection. | Preserve the explicit full-source range and replace it through the existing atomic commit. Ordinary caret and partial-owner restrictions remain. | 49 core cases, mounted selection/clipboard/paint/history regression, interactive keyboard paste and Undo/Redo. |
| Rejection at the writable ceiling incorrectly asked for source mode. | Platform-value rejection uses the same reason mapping as commands. | Source-limit and recovery regression, plus normal-app ceiling canary. |
| Task markers depended on unavailable checkbox glyphs. | Paint the checkbox and check as geometry. | Actual marker pixels distinguish checked from unchecked without a symbol font. |
| Chinese, emoji and symbols remained missing after fallback fonts loaded. | Use Flutter's system-font lifecycle mixin to invalidate cached row layouts. | A late-loaded font changes pixels without changing the editor snapshot; final normal build visibly renders the Unicode sample. |

The changes keep source ownership in the kernel and platform/layout concerns in
the Flutter host. There is no second browser editor or test-only production
input route. The full-source selection exception is documented in the edit
profile; it does not enable arbitrary partial transformations across owners.

## Current evidence

- Core: **298 tests**, including 1,000 generated command histories with seed 2026.
- Flutter host: **96 tests**. Workbench: **25 tests**. Real Chrome transport: **2 tests**.
- All three analyzers and the normal `flutter build web --wasm` pass.
- Parser/Rust evidence is inherited only where the prior receipt's parser
  inputs still hash identically; it is separately recorded in the receipt.

The browser session used the normal application at `http://127.0.0.1:8813/`,
1280 × 720, on an Apple M1 Pro with macOS 26.2. Source checks use the application's
Copy Markdown export against independently constructed expectations, including
complete documents beyond the displayed source page. The receipt separates the
exploratory session from canaries repeated after the final rebuild.

Interactive coverage includes keyboard and browser paste, copy/cut, whole-source
replacement, the following character, history, bold entry/exit, deleting the
last styled character and retyping, list/quote exit and heading Return, source
mode transitions, pointer editing in a table, checkbox toggle/undo, scrolling,
inspection reflow, reload persistence and switching all presets. Dense presets
and the 5,000-character line remained exact and editable through source paging.
The 32 KiB live boundary transitioned both ways; the 256 KiB writable ceiling
rejected excess input atomically and allowed recovery.

Sustained typing crossed two bounded input-context replacements, with full
source checked at each of four batches. The final build repeats this with 64
chunks and 1,642 delivered characters. Automation retargets the active DOM input
between chunks and limits a chunk to the context boundary. This proves continued
editing across those replacements; it does not qualify an unretargeted queued
native key burst. Session timings include inspection and are not latency metrics.

## Reflection and remaining work

This session supports the decision to make browser dogfooding the primary
development loop. It found real transport and rendering failures despite green
lower-layer tests, and each fix now has a regression at the layer that missed it.
Continue pairing every-paint tests with the normal application and exact-source
oracles. More generated source cases alone would not have found these failures.

No known B0/B1 remains in the exercised browser loop. Remaining scope and
roughness are explicit:

- Choosing a document may leave focus on the chooser; click the editor to type.
  Focus recovery and the next character work after that click.
- The automation viewport override did not change the actual 1280-pixel browser
  width. Narrow layouts pass mounted workbench tests; physical narrow-browser
  resizing is not qualified by this session. Inspection reflow was exercised.
- Numeric web performance, other browser engines, real IME/mobile input and
  assistive-technology use need separate qualification. No visible editing-loop
  stall was observed during delivered-input canaries.
- D0-macOS still needs its complete foreground performance rerun, normal AppKit
  clipboard/composition/queued-input canaries and sustained lifecycle run. The
  earlier focused timing improvement does not close the native B1.
- Phone-floor devices, Dune integration and release evidence remain later gates.

The handoff opens the Tour in the final normal web build. Draft and presets use
browser-local storage; Copy Markdown exports a complete document. The candidate
limits remain provisional: 32 KiB plus the declared shape budget for live mode,
256 KiB for writable source mode.
