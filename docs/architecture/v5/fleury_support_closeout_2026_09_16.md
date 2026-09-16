# Fleury support closeout — 2026-09-16

This closes the current Fleury integration as Flark's second supported host.
The shared kernel still owns Markdown parsing, document state, edit semantics,
selection and undo. Fleury owns cell layout, input/focus, rendering, scrolling
and resource presentation. Flutter retains its own pixel layout and controls.

The closeout includes table cell editing, bounded image previews and resource
actions, common block-edge alignment in both hosts, and per-level Fleury heading
styles with playground controls. Headings use the accepted quiet typography:
accent title, slate H2, bold italic H3, then italic, regular and muted levels.
No default title band or underline is imposed.

Fleury commit `41967d499a6bd0fe53e5f5f07b31649df0adcc83` contains the required
image occlusion/background and outer-edge glyph support. It is based on current
main `d6701677`, with the remote client regenerated from the combined sources.
Flark migrates the newer Button API and pins core/widgets/web together. There
are no ignored local Fleury overrides in the installation under review.

Review found a retained image-hole change that left the final cell grid and
recorded placements identical. A failing regression demonstrated the missed
frame; Fleury now compares visible presenter slices. The pre-existing diff
property oracle was updated to that presentation contract. The original Fleury
checkout and unrelated work are excluded from this closeout.

Local browser dogfooding checks table padding-click append and undo, image
actions/edit dialog occlusion, dark/light backgrounds, scrolling, a 420×760
layout and the heading sample. The viewport is restored afterward. No console
warnings or errors were observed on the reviewed candidate.

## Remaining qualification

This locks the architecture and implemented host baseline, not every platform
claim. Remaining work includes physical terminal/IME and lifecycle journeys,
large-document sustained input-to-present profiling, and a real consuming app.
Terminal graphics protocol coverage remains distinct from browser proof. Fleury
uses character-cell wrapping; word-aware wrapping is a possible follow-up.
Image alignment is currently centered contain-fit, and custom builders remain
the extension point for application assets/loading/presentation.

## Local closeout receipts

Clean worktree at Flark `d33b742` with no ignored overrides; all Fleury package
roots resolved to Git cache revision `41967d499a6bd0fe53e5f5f07b31649df0adcc83`.

- Fleury host: analysis clean; 91 tests passed. Example: analysis clean; six
  tests passed.
- Flutter host: analysis clean; 812 tests passed. Example: analysis clean;
  31 tests passed. This includes the shared block-alignment pixel/edit fixture.
- Fleury web compiled with both Wasm engines and its bundled image. The clean
  candidate `/revisions/8789d010bf12/` was inspected in the browser; checkbox
  activation/undo were verified again with the Git-installed framework.
- `dart build cli --target bin/main.dart --output build/native` produced a
  terminal bundle containing both native libraries. It rendered the sample in
  a background PTY and exited cleanly through Ctrl+Q. `dart compile exe` does
  not package build-hook assets on this SDK; the CLI bundler is required.
- Framework full `fleury_dev check` passed across packages, Chrome, docs
  examples, dart2js and all 64 terminal/remote integration cases. On final
  commit `41967d49`, the core suite also passed 3,564 tests with repaint-cache
  verification enabled (one existing skip). The diff property oracle follows
  visible placement semantics; its earlier recorded-placement expectation was
  corrected and rerun before that final full core pass.
- Framework all eight fast gates, terminal wire gate and live serve-wire gate
  passed without baseline changes. The regenerated embedded client passes its
  freshness check. Analysis has no errors/warnings; pre-existing informational
  lints remain in several framework packages.

Framework PR: [Fleury #260](https://github.com/danReynolds/fleury/pull/260).
CI is intentionally skipped at the owner's request. These receipts establish
local host behavior and dependency installation, with the limits above.
