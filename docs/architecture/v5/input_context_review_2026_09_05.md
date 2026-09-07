# Native input context — September 5 continuation

**D0 is still open.** The long-paragraph latency investigation now has a
specific native cause and a tested host-level correction. A complete native
sweep and normal-binary canaries have not yet qualified the new candidate.

## Evidence and correction

The minimal host reproduced the original failure at 21,270 µs insert and
21,141 µs Backspace p99 over 960 measured inputs. Workbench persistence is
therefore not the sole cause. Input-to-vsync p99 was 19,326 µs; build p99 was
1,358 µs. The source remained 32,767 bytes before insertion.

A five-second VM trace and two-second native stack sample were captured during
the production workbench's foreground edit loop. Of 1,524 main-thread samples,
445 included `FlutterTextInputPlugin.setEditingState`; 346 included AppKit
insertion-point layout and 280 included caret-offset enumeration. The native
text view was doing substantial TextKit/CoreText work to maintain its mirror
of the document. These are stack-sample counts, not exact percentages of CPU
time. The longer traced run subsequently lost foreground, so it is diagnostic
evidence rather than a qualification pass.

The Flutter host now gives the platform input method exact surrounding source
instead of ordinarily mirroring the whole document. `InputContext` is private
to the host. The controller, parser, history and public full-value/delta APIs
still use complete source and global offsets.

- Ordinary contexts start with roughly 256 UTF-16 units on either side, rounded
  to grapheme boundaries. They remain stable while typing and ordinarily rebase
  before exceeding 1,024 units.
- Complete selections and active compositions take precedence over that soft
  bound. Large selections and unusually large graphemes can still be larger;
  this is not an unconditional platform-buffer ceiling.
- Incoming values expand into the exact full document before kernel validation.
  Canonical formatting corrections are sent back to the platform. Accepted
  unchanged deltas are acknowledged without an unnecessary full-state echo.
- A rebase opens a new input client, making callbacks with the old coordinate
  system inert. Focus/document replacement continues to use the same ownership
  boundary. Native queued input during this transition remains an explicit
  canary; rejecting stale callbacks alone does not prove that no queued OS
  keystroke can be lost.

The first native run with this correction completed all 200 measured edits of
dense/largest-block at **12,748 µs insert and 12,290 µs Backspace p99**, below the
unchanged 16,667 µs limit. Command p99 was 2,775 µs, build 2,194 µs and raster
595 µs. Real native mirroring remained enabled. However, inspecting the app
during this run enabled accessibility, produced native AX-tree errors and
tripped the test framework's outstanding `SemanticsHandle` check at teardown.
The timings support the correction; the run is not a clean pass. Normal-app
accessibility behavior must be checked separately.

Two temporary no-mirror controls were rejected for foreground loss. Neither
provides a successful counterfactual. That diagnostic switch was removed from
the final harness. The subsequent full sweep produced no completed case and
was rejected after a delayed startup with a hidden lifecycle. Desktop state
also changed during CUA activation attempts; native UI actions were paused
rather than competing for foreground.

## Review and current tests

This preserves the V5 source-authority design. It adds one small input adapter
at the Flutter boundary rather than changing parsing, splitting paragraphs or
relaxing the live envelope. The remaining risk is platform input continuity,
particularly composition, accessibility and keys queued at a context change.

All 93 host tests and 24 workbench tests passed. New cases cover exact source
expansion, Unicode/CRLF boundaries, wide reverse selections, composition growth
and commit, stale deltas/clients, formatting correction followed by typing, and
continued Unicode typing across context changes with exact first-paint source
and caret checks. Existing core/Rust/parity evidence remains attached to
unchanged inputs. Both host and example analysis passed.

Browser dogfooding used the rebuilt web/Wasm application and the 5,000-byte
long-line source fixture. Copy Markdown provided an independent full-source
oracle. At 769 inserted characters the platform context changed: the actuator
stopped because its textarea target changed, while the new textarea was already
focused. The complete source matched every delivered character. Resuming the
remaining input reached 800 inserted characters, with exact 5,800-byte source,
global caret 800 and local caret 287. Undo and Redo reproduced the expected
complete source. Select All exposed the complete 5,800-character selection.

Clipboard replacement still produced no observed paste; it is not a pass.
Undo restored the exact original `word ` repeated 1,000 times, verified through
Copy Markdown, and the prior browser clipboard was restored. These checks
qualify this browser interaction subset, not AppKit, native timing, IME or the
full web surface.

Both native profile harnesses now wait for an actual resumed lifecycle with
frames enabled **before** their first `pumpWidget`. Previously a hidden binding
could wait for a frame before reaching its sixty-second foreground guard. The
new ordering was analyzed; its next native startup run remains outstanding.
No fake lifecycle/frame delivery or weaker timing limit was introduced.

## Next gate

On an available foreground Mac, run the full 21-case sweep, the uninterrupted
100-cycle/five-minute workload, then the rebuilt normal application's AppKit
and process-lifecycle canaries. Include rapid typing/repeat keys across the
input-context boundary, large selection replacement, composition and
accessibility activation. Record the final candidate's exact hashes. D0 and the
original performance B1 stay open until those pass together.

The normal macOS and web/Wasm applications were rebuilt after diagnostics. No
commit or push was made. Local evidence is in
[the continuation receipt](receipts/2026-09-05-input-context/candidate.json).
