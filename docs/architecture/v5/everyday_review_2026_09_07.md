# Everyday editor qualification — 2026-09-07

Candidate: working tree on `codex/v5-editor-qualification`, based on PR #41's
merge `a236eb2483d0208ec6ab6b91eae57bada0a7810b`. CI remains skipped at the
owner's request. This checkpoint qualifies the existing fourteen-language
Flutter editor before expanding languages or starting another host.

## Findings and changes

The native qualification targets had diverged from the normal workbench: they
did not load Tree-sitter or create its coloring workers. Both now use the same
code service and controller-owned workers as the normal application. The shape
sweep includes Dart, Ruby and JSON fences, and receipts identify both services.
The complete fixture set remains inside the predeclared admission limits.

That normal macOS build path exposed a packaging failure: the native-assets
hook's sanitized environment did not contain the SDK headers required by the
C grammars. Both parser packages now resolve the macOS SDK and compiler through
their existing Apple build helper and honor the configured deployment target.
The unmodified Flutter build path subsequently compiled and launched the app.

The longer workbench run found excessive work on Source-to-Rendered transitions
and inspection-panel reflow. The surface discarded shaped paragraphs whenever
width or mode changed. It now reflows unchanged paragraphs and keeps one bounded
layout for each mode, validating text, styles, language and coloring revision
before reuse. Controller/style/font changes and disposal clear both modes. A
direct first-paint scenario changes structure and inline styles in Source mode,
resizes, returns to Rendered mode, and types again; all 797 host tests pass.

This was a testing-methodology gap, not an obscure language edge case. Package
tests could pass while the production application's build and service ownership
were untested. The correction is to keep qualification on the consuming app's
real configuration, and test sustained input across context changes as well as
short, already-decoded editing commands.

## Local evidence

Before the paragraph-reuse change, the foreground macOS frame sweep passed all
24 shape/site cases, with 4,800
measured inputs. Worst per-case insert p99 was 13,725 µs and delete p99 was
12,795 µs, below the unchanged 16,667 µs limit. Worst raster-work p99 was 605 µs;
that diagnostic is distinct from the full input-to-raster measurement. The run
had no foreground transitions.

The first longer native run completed 100 open/edit/close cycles and 3,000
sustained character edits with exact source/caret and save checks. Sustained
insert/delete p99 were 15,608/15,719 µs. Peak RSS growth was 29.03 MiB and retained
RSS was 7.89 MiB below the warmed baseline. Opening stayed below 46 ms. The run
failed 17 transition/reflow groups, reaching 37,295 µs, and therefore is **not a
workbench qualification pass**. The subsequent rerun with paragraph reuse was
rejected before measurement because macOS reported a hidden lifecycle for its
entire sixty-second activation wait. No timing gate has been relaxed.

The real browser input/color-worker sequences passed 1,500 edits each for
Automatic Ruby and a larger explicit Dart fence. They exercised five and six
actual input contexts, respectively, restored the entire source exactly, kept
the following heading outside the fence, verified every edited paint and caret,
and continued through Undo/Redo. Coloring adopted 1,472 and 1,486 current
revisions without worker errors. Together they ran for more than six minutes.
These Chrome debug-browser measurements qualify transport and paint behavior,
not release browser raster latency.

Current active-package analysis is clean. The 797 host tests, 27 workbench tests,
25 Chrome regressions and two sustained Chrome tests passed; the normal
`flutter build web --wasm` build passed. Hands-on checks on that release build
at 1280 × 720 verified source-mode formatting changes, reflow in both directions,
Ruby colors, double spaces, fence-scoped Copy, Cut/Undo/following input, exact
whole-document clipboard export, and reload followed by typing. The console
reported no errors or warnings. These checks used the isolated 8815 origin;
the owner's 8813 draft was not edited. The owner preview was refreshed to the
same build, and its 1,020 UTF-16 units of saved source were compared exactly
before and after reload. The temporary test tab and server were closed.

The normal macOS application was rebuilt again with `--profile --target
lib/main.dart` and no diagnostic defines. Its separate final binary manifest
does not claim completion of the outstanding attended checks.

Machine: Apple M1 Pro, macOS 26.2, Flutter 3.44.4, 120 Hz display, 800 × 600
logical viewport, device-pixel ratio 2. This is profile-mode local desktop
evidence; it does not transfer to browsers, phones or release consumers.

The [receipt directory](receipts/everyday-2026-09-07/) retains raw logs, engine
frame samples, build inputs and binary provenance. The frame target's cached
AOT output is identified by its compiler input stamp; the workbench build is a
separate application artifact.

## Qualification still in progress

The final candidate still needs the foreground native workbench rerun and
normal input/lifecycle canaries. The first native run reported accessibility-tree
update errors after accessibility inspection. A normal application session
initially showed a rapid double-space discrepancy, but later OS input-field
observations and rendered frames diverged after the UI tool timed out opening a
comparison app. The hidden-lifecycle rejection prevents attributing that later
behavior to the editor. The user's original native Draft was restored and
visually verified after relaunch. The attended `input_comparison.dart` diagnostic
provides Flark and standard Flutter controls with exact source/caret readouts
to resolve delivery versus editor behavior when foreground control is available.

Physical-device, assistive-technology and cross-browser qualification, actual
consumer adoption and daily-use evidence remain separate gates. `flark_fleury`,
image previews, link/image editing affordances and release packaging remain
future work.
