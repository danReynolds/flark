# Resource and theming landing review

Reviewed the complete candidate on `codex/v5-links-images` against
`46304ddd6522fa974376161a1af9072ecc900f28` (PR #42). The theming implementation
depends on the still-local resource commands, resolved parser metadata and
image preview support, so they are reviewed and landed together. This is a
fresh implementation self-review, not an independent peer-review claim.
CI is skipped at the owner's explicit request.

## Result

No blocking findings remain in the reviewed changes. The resource commands
retain Comrak as the recognition authority and validate canonical serialization
before one source/caret/history transaction. The host owns theme resolution,
layout, image-stream lifetime, focus and presentation. Custom controls receive
guarded actions; they do not reproduce editing rules. The single-editor
playground uses public APIs and an example-only color-picker dependency.

Review covered combined inline styles, theme precedence and geometry
invalidation, bounded image loading, default/custom control lifetimes and stale
requests, source preservation, undo, parser schema/transport identity and
example configuration export. Earlier no-op reference-save and code-fence
Cmd-K findings have dedicated regressions and remain corrected.

## New finding and correction

The browser failure after **Enable accessibility** was actionable. The surface
declared a semantic text field without the enabled flag or focus action;
Flutter's engine created a disabled textarea. Once enabled, an ancestor `Focus`
also advertised focus, pulling browser focus away from the field. The surface
now owns enabled/focused text-field semantics and routes focus to the existing
host focus node. The wrapper no longer publishes duplicate focus semantics.

The new Chrome test first failed on the disabled field. It now proves DOM focus,
actual input into bold text, two successive characters, exact source and caret,
bold presentation on the first edited paint, and Undo through Flutter's real
semantics text transport. It does not use the default semantics-disabled web
test lane. The normal release Wasm build also passed an attended embedded-browser
canary with accessibility enabled: typing, form-to-editor focus recovery,
styled insertion, link popover/form opening and cancellation, subsequent input,
and Undo. No browser warning/error logs were observed.

## Fresh local checks

| Check | Result |
| --- | --- |
| Kernel, Flutter host and example analysis | Passed |
| Kernel tests | 685 passed |
| Flutter host tests | 843 passed |
| Example tests | 31 passed |
| Exported public-API consumers | 2 passed |
| Chrome input, semantics, code-worker and font tests | 27 passed |
| Rust conformance, extraction and fuzz tests | 44 passed |
| Committed and freshly built Wasm versus native | Identical on 1,322 cases |
| Clean consumer using a prebuilt with Rust absent from PATH | Passed |
| Normal release web/Wasm and macOS profile builds | Passed |
| Schema regeneration and whitespace check | Passed |
| Pinned 16 KiB-class kernel benchmark | Insert p99 2.777 ms; backspace p99 2.615 ms (both <4 ms) |

The benchmark ran after the normal builds, on committed candidate
`93891d801d3292aba3edb4ea87156b158f12a471` with a clean working tree. It used
the pinned dense fixture (16,694 UTF-8 bytes, 769 blocks, 2,144 runs, 705 rows),
100 warmup pairs and 400 measured pairs. This is a kernel receipt, not a host
input-to-paint measurement.

The semantics test and normal web build were repeated after removing a redundant
deprecated focus flag; the host analysis/tests passed on that final source.
Logs, source hashes, line counts and normal build hashes are under
`receipts/theming-merge-2026-09-08/`. The ordinary macOS build still reports
native-framework filename warnings across architectures; it is a build receipt,
not a distribution, runtime or performance qualification.

## Boundaries and next priorities

This closes the observed web semantics input defect, not full screen-reader,
dictation, device, native-frame or lifecycle qualification. External-window
navigation still needs an observable consuming-application canary. The Fleury
host and its example remain unimplemented. No CI pass or tagged-release claim
is inferred from this merge.

The [revisited priorities](priorities_after_theming.md) put native qualification
and real consumer adoption next, then the Fleury host/example, broader device
qualification and release packaging. Theme-option and language expansion are
deferred while those concrete proofs are completed.
