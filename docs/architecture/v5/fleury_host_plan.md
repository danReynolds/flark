# Fleury host: test the second target

Started 2026-09-08 at the owner's request, alongside the separately open native
Flutter qualification. This does not close any Flutter native gate.

## First milestone

Deliver a terminal and locally compiled browser example with one live editor
and a Fleury theme panel. Exercise typed text, paste, pointer/keyboard selection,
wrapped vertical movement, empty lines, Unicode cell geometry, undo/redo,
Markdown styles/containers, code indentation and asynchronous code colors.
The edited frame is checked before waiting for asynchronous decorations.

The first package must have no Flutter import and no second Markdown recognizer.
FlarkEditor owns source, selection, composition transactions and history. Fleury
owns input routing, cell geometry, scrolling, paint, focus and host semantics.
Use Fleury's public input claimant SPI rather than mirroring a TextArea's
independent text controller and undo stack.

The first concrete sharing correction is moving the nonvisual Tree-sitter
command adapter out of flark_flutter into flark_tree_sitter/flark.dart.
Flutter retains its own asset loader and API entry point.

## Subsequent milestones

The first milestone and the tables/images presentation milestone are implemented.
The September 16 closeout reviews the framework changes, replaces development
overrides with a merged Git revision, and verifies both Flark hosts from a clean
checkout. Heading hierarchy and per-level theme controls are included. This
locks the second-host architecture and current APIs, not all platform gates.

1. Tables through Fleury's table surface, replaceable link/image controls,
   resource activation and terminal-appropriate image presentation.
2. Large-source viewport qualification, terminal protocol/IME/lifecycle journeys,
   sustained input-to-present profiling, and clean dependency installation.
3. Host parity review and consumer integration. Do not declare M4 complete on
   headless tests or a browser screenshot alone.

## Architecture review questions

- Does a user edit require changing the kernel for cell coordinates or Fleury?
- Can both hosts share command semantics and snippet services without sharing
  widgets or duplicating document state?
- Do first-frame tests catch incorrect text, selection, styles and the next
  character, including at hidden-markup and wrapping boundaries?
- Is each remaining gap a host responsibility or evidence that the core
  contract lacks information? Record the distinction with a reproducer.

The adjacent Fleury checkout has unrelated uncommitted work. Keep it separate.
Host and example dependencies must resolve to the reviewed Git pin for closeout;
ignored local overrides are only for development, never installation evidence.
