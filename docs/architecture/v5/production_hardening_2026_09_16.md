# Production hardening — 2026-09-16

Continuation of the [production audit](production_audit_2026_09_16.md), starting
at `cde7915`. CI remains skipped at the owner's request. This is implementation
self-review with local tests and browser dogfooding, not independent peer review
or a production qualification pass.

The subsequent [dedicated code review](production_code_review_2026_09_16.md)
found and corrected EXIF thumbnail sizing and pending formatting at the live
admission boundary. Its final test counts supersede the counts below; these
earlier receipts are preserved.

## Changes and review

**Imported short table rows now support editing their omitted cells.** A row such
as `| x |` under a three-column header has two unwritten cells. Both used to map
back to `x`. The kernel now retains a projected cell address alongside the source
offset for this particular collapsed caret. Pointer placement, Tab/Shift-Tab,
arrow navigation, Return and host caret paint preserve that identity. Navigation
does not rewrite Markdown. First insertion privately materializes the necessary
delimiters and publishes one final edit; Undo restores both exact original source
and the unwritten-cell caret. No second document or host-owned table state exists.

The shared `MoveTableCell` command replaces duplicate adapter navigation. Both
hosts use `PlaceCaret` for projected hits; Flutter semantics selection uses the
same route. Source mode discards the virtual address. Resource lookup cannot
mistake an unwritten cell for a resource ending in the previous cell. Selection
ranges remain source ranges; this does not introduce rectangular cell selection.

Reviewed refusal rollback, source admission transitions, pending formatting,
composition cancellation, history grouping and source-address validation. Tests
cover first/middle missing columns, explicit closing pipe or none, LF/CRLF, bare,
quoted and list-contained tables, resource insertion and final-source limits.
The corpus traversal oracle now orders equal-source virtual cell positions; it
still rejects cycles, stalls and nonmonotonic movement.

**Default Fleury image preparation no longer decodes, resizes or encodes PNG on
the native UI isolate.** A global queue permits two active preparations and eight
waiting jobs. Loads retain their 4 MiB encoded / 4 Mi-pixel input limits and
960×640 maximum thumbnail. Native jobs run in disposable isolates; stale jobs
are cancelled on replacement, disposal and timeout. Browser work uses native
bitmap/PNG promises after bounded header inspection, with bounded pixel readback.
Cancelled browser work retains its slot until its native operation settles.

Fleury's first placement paint also used to encode PNG synchronously, even for
already-decoded input. [Fleury PR #262](https://github.com/danReynolds/fleury/pull/262)
adds optional prepared PNG bytes to `ImageSource.decoded`. The pixels/bytes must
match and remain immutable while mounted; animated prepared sources are rejected.
Cache invalidation and prepared/plain/prepared replacement have regression tests.
It merged as `90f27e619deecbc14944ebbbee5df791086ab2a5`. Flark pins the exact tested
PR commit `ffb6d22d488a1979658578f4fc5634bb17b37b7f`; there are no local path overrides.

This does not make every image operation asynchronous. Header inspection,
bounded readback and display still cost time. Terminal protocol conversion and
clipped iTerm2 PNG preparation remain separate qualification work. Browser
formats depend on both header support and native decoding; no synchronous Dart
pixel-decoder fallback is used. Custom preview builders remain the host hook.

## Local evidence

The [receipts](receipts/production-hardening-2026-09-16/) contain complete local
logs, production-source hashes and the image diagnostic. The original audit's
red receipts and failed performance result are preserved separately.

| Check | Result |
| --- | --- |
| Kernel analysis / full tests | Clean / 770 passed |
| Flutter analysis / full tests | Clean / 815 passed |
| Final Flutter pointer, Tab, platform input, semantics and Undo checks | Both 240 px and 800 px passed after extending the original full-suite test |
| Fleury analysis / full tests against the Git pin | Clean / 95 passed |
| Fleury framework widgets on pinned base | 1,309 passed, including 57 image tests |
| Fleury framework widgets combined with current main `19a8978c` | 1,309 passed before merge |
| Fleury web release build | Passed; `/revisions/78a5085b0aac/` |
| Ordinary Flutter macOS profile app | Rebuilt successfully after the diagnostic |

Browser dogfooding used an isolated tab at 1280×720, real pointer/key input and
the hidden input bridge. A missing third cell received `Z`, Undo restored the
exact `| x |` row, and Shift-Tab plus `Y` edited the middle cell. On the final Git
build, a real 2048×2048 synthetic image rendered as a 640×640 prepared thumbnail;
typing beside it, opening image actions, removal, Undo/reload and subsequent
missing-cell insertion all worked. The resulting table/image frame was visually
inspected. These are targeted browser journeys, not browser frame-budget proof.

The real native decode test pumps and checks Fleury input/caret/paint while
preparation is outstanding. Additional tests cover saturation, active/queued
cancellation, recovery after cancellation, stale URI completion and disposal.

### Image responsiveness diagnostic

The same 2048×2048 RGB gradient (16,856 encoded bytes) was decoded, resized and
PNG-encoded synchronously, then through the native preparation queue three times.
A 5 ms UI-isolate timer measured opportunities to service other work.

| Mode | Total preparation | UI ticks during work | Largest timer gap |
| --- | ---: | ---: | ---: |
| Synchronous | 262.349 ms | 0 | 263.894 ms |
| Queue run 1 | 284.066 ms | 56 | 6.342 ms |
| Queue run 2 | 230.207 ms | 45 | 6.832 ms |
| Queue run 3 | 244.520 ms | 48 | 7.807 ms |

This demonstrates UI-isolate availability, not faster decoding or an OS
input-to-presentation guarantee. It is a diagnostic sample, not a new gate.
To repeat from `packages/flark_fleury`, after `dart pub get`:

```sh
dart --packages=.dart_tool/package_config.json ../../docs/architecture/v5/receipts/production-hardening-2026-09-16/image_responsiveness.dart
```

## Native investigation and remaining gates

The new `semantics_probe_test.dart` compares standard Flutter `TextField` and
Flark with semantics enabled: 50 mount/edit/unmount cycles each, 20 programmatic
edits per cycle. Both completed, with zero AXTree errors in the captured output.
However, Flutter reported failure to foreground the app, and the Mac was locked
when native automation attempted inspection. This does **not** reproduce the
accessibility activation path that produced the earlier 2,948 errors. The local
VM-service connection URL is redacted from the receipt.

The ordinary app was restored with `flutter build macos --profile --target
lib/main.dart`. No new clean foreground performance or VoiceOver pass is claimed.
The prior table-reflow p99 miss of 19.445 ms remains open. Once the Mac is
unlocked, activate native accessibility on the comparison, isolate the affected
phase, and repeat the unchanged complete workbench gate in a quiet foreground
session. Do not increase the 16.667 ms threshold to obtain a pass.

Remaining production work: that native gate; supported browser/terminal
presentation budgets; physical IME, accessibility, clipboard and OS lifecycle
qualification; a real consumer soak; and release-artifact consumption. The
audit's structural deletion behavior decisions remain explicitly deferred.
