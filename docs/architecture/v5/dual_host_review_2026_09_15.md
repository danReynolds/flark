# Dual-host integration closure — September 15

The shared-editor boundary held: no kernel or Markdown parser changes were
needed for this follow-up. Flark owns source, selection, composition and history;
Flutter and Fleury own platform input, viewport geometry, paint and controls.
This is local integration evidence, not complete host or device qualification.

## Changes reviewed

The pending visual pass supplies configurable code inset, readable paired color
presets, a bordered and spaced customization panel, content-fit link controls,
and actionable checkbox/link semantic bounds. Current Fleury required adapting
pointer details, wheel bubbling and derived paint geometry. Custom caret
geometry now uses Fleury's public CaretHost interface, including identity-checked
detach on unmount and focus-node replacement. No private Fleury imports or
parallel document/history implementation were introduced.

Fleury PR #253 landed the scoped field shortcuts, ColorPicker API, wide-run DOM
baseline fix and public caret interface. Flark and its example pin the reviewed
commit `d37f5a56bcd164acb40ad7dd45ccd633b42cc0a3`; local path overrides are not
needed. Because the unpublished companion packages still declare hosted core
constraints, application roots carry the documented Git core override. This is
reproducible development packaging, not a claim of pub.dev release readiness.

Both hosts now use `FlarkCodeHighlighting` in `flark_tree_sitter`. The cache and
worker orchestration were duplicated despite using the same Tree-sitter engine.
The Fleury loop could evict a current snippet, then request it again forever
when more than 32 snippets competed for the cache. The shared lane bounds the
wanted set as well as retained colors, and each host supplies its visible rows.
It also skips oversized snippets, suppresses retry loops after null responses,
checks response source/language identity, and disposes the worker once. Current
text paints plainly until exact colors arrive. The existing synchronous code
edit delegate remains separate because it handles Enter/outdent immediately.

Read-only link clicks now open directly in both hosts. Editable clicks retain
replaceable controls, and modifier-click opens directly. Host-specific control
builders still determine presentation.

The final migration review found a clipped-cache regression: painting only the
current screen-visible rows into a retained surface left a subsequently revealed
row blank. A direct scroll/reveal test failed before the fix and passes after
removing screen clipping from local surface painting. Fleury's buffer/compositor
owns ancestor clipping; caret and semantic geometry remain clipped separately.

## Local evidence

- Flutter: 811 tests passed, analysis clean.
- Tree-sitter package: 360 tests passed, analysis clean. New controlled-worker
  cases cover 40 fences, total work bounds, oversized input, null results,
  source/language replacement, source mode, mismatched responses and disposal.
  The size-budget test explicitly expands the test-only live envelope; it does
  not expand the product's qualified envelope.
- Fleury host: 65 tests passed against Git-resolved dependencies, including a
  new real input/paint journey with held color responses, fenced Cmd+A, paste,
  undo, late delivery and teardown. Analysis clean.
- Fleury example: five tests passed against Git-resolved dependencies, including
  painted theme contrast, responsive geometry and pointer color selection.
- Framework validation is recorded in Fleury's
  `docs/audits/2026-09-15-flark-host-polish.md`: 3,529 unit tests, one new caret
  contract test, 536 VM/Chrome web tests, 16 picker tests, fast performance gates
  and live serve-wire passed. Its whole-repository check still reports 14
  existing info-level lints in unchanged files; changed-file analysis is clean.
- Browser candidate `17c6e5c502ed`: physical link click, content-fit controls,
  keyboard URL replacement/save/undo, checkbox toggle and pointer cursor,
  fenced select-all/paste/undo, visible syntax colors, light/dark themes and
  400-pixel narrow controls checked. No browser warnings/errors observed.
  The final Git-resolved build produces the same candidate asset hash.
  A semantic-mirror button click did not toggle the theme; the physical button
  did. This is not a complete accessibility-action qualification receipt.
- CI intentionally skipped. No new native/physical IME or sustained frame-time
  qualification is claimed.

## Remaining focus

| Area | Current boundary | Next work |
| --- | --- | --- |
| Shared editor semantics | One kernel; cross-host journeys cover ordinary edits | Keep minimizing actual dogfood defects into real host input/paint checks |
| Heading presentation | Flutter supports size hierarchy; Fleury uses one cell size | Choose per-level cell styles; variable-size rendering would be a separate Fleury feature |
| Tables | Flutter has a table surface; Fleury emits sequential cells | Design a cell-grid table surface, selection and narrow-width behavior |
| Images | Flutter has previews; Fleury shows styled text | Use host capabilities for previews and a useful text fallback |
| Resources and theming | Shared guarded actions; host-specific controls | Qualify accessibility actions and external URL dispatch on each real platform |
| Consumer proof | Package playgrounds work | Integrate a real composer/viewer consumer, then sustained dogfooding |
| Native input and performance | Existing macOS qualification remains incomplete | Resolve double-space attribution; finish IME, lifecycle, accessibility and sustained latency gates |
| Core edge cases | Earlier deletion/table-addressing and definition-heavy extraction findings remain | Follow the bounded issues in the September 8 review; no speculative parser rewrite |

The next product milestone should be a real consumer integration with an explicit
capability contract. Equal parser behavior does not imply equal host capability,
and green parser/service tests cannot substitute for pointer, input, geometry,
first-paint and physical-device evidence.

Final browser candidate after the cache-reveal correction:
`http://localhost:8820/revisions/5595326c820a/`. Loaded from the Git-pinned build;
editor rendering and link controls were checked again with no browser warnings
or errors. The host's 65 tests and all five playground tests pass on this final
paint implementation.
