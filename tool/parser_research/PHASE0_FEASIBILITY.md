# Phase 0 editor-engine feasibility receipts

Status: observational prototype evidence, 2026-07-13. This record supports
[RFC 023](../../docs/architecture/rfc/rfc_023_incremental_live_markdown_engine.md)
and does not itself define the product contract.

## Disposition

The Phase 0 work supports continuing with the persistent, incremental,
virtualized v3 engine, with one material correction: a layout shard is not
safe merely because the parser permits a source split. Exact text layout also
requires Unicode shaping and line-break continuity. Arbitrary independent
`TextPainter` shards are not a viable implementation for large complex-script
paragraphs.

The automated evidence lowers the risk of the active input island,
document-owned selection chrome, and bounded semantic paging. It does not
clear the real-keyboard, physical touch, or screen-reader gates. Those remain
Phase 0 exit criteria before production parser-fork work is funded.

## Test environment

- Flutter 3.44.4 on Apple silicon macOS.
- macOS desktop engine.
- iPhone 17 Pro simulator, iOS 26.5.
- Physical iPhone, iOS 18.7.3: signed/installed test build; the Runner process
  remained live on-device, but no Dart VM service connection was discovered and
  therefore no automated test completed.
- Chrome 150 for Flutter web widget tests.
- No Android target was attached.
- The iPad was not usable as a development target. No physical-device pass is
  claimed.

The simulator and widget-test input updates use Flutter's engine/test APIs.
They do not substitute for Gboard, Samsung Keyboard, Japanese/Korean IMEs,
dictation, or a physical iOS keyboard session.

## Results

| Risk spike | Receipt | Result | Consequence |
| --- | --- | --- | --- |
| Active-source input lease | Exact `**bold**` source was installed before focus; the same `EditableTextState` survived composition; parser reshaping remained queued until commit. Passed in the Flutter VM, Chrome widget runtime, macOS engine, and iOS 26.5 simulator. | Mechanism viable; real-IME gate open. | Keep one bounded Flutter-owned input host. Select active-source reveal before opening the connection and lease identity/representation through composition. |
| Cross-shard selection model | The current two-editable surface collapsed a requested `2..11` mouse drag to `11..11`. A document coordinator mapped `2..372` across a lazy 50,000-shard surface with only 22 shards mounted. Public `SelectionOverlay` accepted document-owned handle geometry, toolbar, magnifier, and handle-drag updates in VM and Chrome tests. | Current v2 gesture path fails; replacement model is API-feasible; physical behavior open. | Source selection belongs to the document. Shards remain geometry/input portals. Validate adaptive native handles, magnifier, menus, autoscroll, and clipboard on devices. |
| Virtualized accessibility | A 50,000-item semantic surface mounted 23–24 semantic widgets in VM/Chrome tests and 32–35 in macOS/iOS engine runs. It exposed scroll semantics and made paragraph 25,000 discoverable after semantic paging without retaining paragraph 0. | Bounded semantic paging feasible; screen-reader gate open. | Specify paged semantics with stable document anchors, rather than a whole-document semantic tree. Test VoiceOver, TalkBack, focus continuity, rotor/navigation, actions, and selection on devices. |
| Live parser-to-paint slice | Over a 50,000-block, 2.97 MB surface, active-source reveal had 0.59 ms urgent p95 and 3.71 ms pump p95 in the latest debug-VM run; hidden-syntax mode had 0.92 ms urgent p95 and 4.39 ms pump p95, with an 18.28 ms pump maximum. Earlier runs showed wider hidden-mode tails. | Same-refresh architecture plausible; release liveness unproved. | Use active-source reveal as the baseline. Feed the real incremental delta into the slice, then measure event-to-pixel p99/p999 in profile builds on floor devices. |
| Complex shaping seams | With native fonts, independently shaping `سل` + `ام` differed from shaping `سلام` by 7.7089 px; `of` + `fice` differed from `office` by 0.7588 px. A whitespace Arabic seam differed by 0 px. Eight UTF-16 units of naive overlap only halved the sampled Arabic error from 2.5064 px to 1.2532 px. macOS and iOS engine runs reproduced the same seam deltas. | Arbitrary independent shards fail exactness. | Replace “parser-approved safe boundary” with a syntax-safe **and** shaping/line-break-safe layout boundary. Preserve shaping context or share a shaping result; do not assume fixed overlap repairs joining. |
| Contextual layout checkpoints | Over seven 16 KB Latin/Arabic/Devanagari/Thai/CJK/mixed-bidi/emoji corpora, 128-unit work windows with two leading context lines and three trailing lookahead lines matched monolithic line breaks, boxes, metrics, and localized-insert results within 0.1 px. Contextual debug-host chunk p95s were roughly 0.15–0.35 ms. Removing leading context shifted sampled mixed-bidi boxes by 21 px. | Ordinary-prose mechanism passes the automated differential; certification/fallback boundary remains essential. | Build line checkpoints as layout-owned certified state, not arbitrary source chunks. An edit may still propagate globally, but each continuation slice is bounded and preemptible. |
| Adversarial checkpoint state | Long-lived bidi embedding and weak-direction runs kept line breaks but produced about 1,595–1,596 geometry mismatches when restarted from bounded Flutter context. One 8,195-unit grapheme forced an 8,195-unit window and 47–55 ms layout. A 16,384-unit unbroken Arabic sample remained exact with at most 256 units of context. | Fixed context is not a universal resumption API; “unbroken text” alone is not the classifier. | Detect paragraphs with uncertified bidi/grapheme state and use exact-source/plain bounded or no-wrap fallback, or later add an explicit Unicode bidi/shaping state service. Do not build a native text stack for ordinary prose. |
| Incremental wrapping | Independently wrapping a long Arabic paragraph disagreed with monolithic wrapping at 72 of 88 sampled splits. Full native layout of a 102,400-code-unit Arabic run took 51–57 ms across the captured debug-host runs. Existing convergence probes kept individual chunks below roughly 2 ms, but some edits propagated through 699,126–786,473 code units and consumed 55–79 ms in aggregate across runs. | Slicing is preemptible, not always local. | Keep resumable line breaking and prefix/suffix convergence. Ordinary prose may split at proven line/word boundaries; global propagation completes over multiple frames without blocking input. |
| Oversized paragraph layout | Full `TextPainter` relayout measured roughly 0.83–1.25 ms at 4 KB, 2.92–3.14 ms at 16 KB, 10.7–11.1 ms at 64 KB, 43–59 ms at 256 KB, and 175–278 ms at 1 MB across captured debug-VM runs. | A megabyte paragraph cannot use monolithic layout on the hot path. | Use certified contextual checkpoints and explicitly degrade regions whose Unicode layout state cannot be bounded. Building a full custom text stack is not justified by Phase 0 evidence. |

## Architectural correction

Parser, projection, shaping, line breaking, and viewport segmentation have
different boundaries:

1. The parser defines authoritative syntax and legal source coverage.
2. Projection defines hidden/replaced display spans and source/display maps.
3. A shaping boundary service chooses seams that preserve bidi, joining,
   grapheme, font-run, and ligature context.
4. Resumable line breaking carries incoming line state and advances until the
   outgoing state converges.
5. Viewport shards consume those results; they do not independently reshape
   arbitrary substrings and add their dimensions.

For normal prose, established line boundaries plus bounded leading and trailing
context produced exact differential results in the Phase 0 corpus. The
pathological case is more precise than “unbroken text”: it is any region where
the layout layer cannot certify bounded bidi, grapheme, shaping, and line-break
state. Long-lived bidi state and an oversized grapheme failed that
classification, while the unbroken Arabic sample passed. The honest interim
behavior is local exact-source/plain editing with bounded or no-wrap
presentation for an uncertified region; it must not block the UI isolate or
guess geometry.

## Automated layout gate result

The narrow differential spike passes for the ordinary-prose corpus and rejects
the tested unbounded-state fixtures. The production gate is therefore:

1. Emit a layout checkpoint only when its bidi/grapheme/shaping state is
   certified bounded; “two previous lines” is a prototype receipt, not the
   production definition of safety.
2. Retain sufficient trailing lookahead to keep dictionary and line-breaking
   decisions authoritative.
3. Differential-test line breaks, selection/caret geometry, bidi movement, and
   height after every edit over the full supported-script and font corpus.
4. Stop and schedule continuation when the next certified checkpoint cannot be
   reached within the UI-isolate deadline.
5. Use the exact-source/plain bounded or no-wrap fallback when state cannot be
   certified, including oversized graphemes and unresolved bidi runs.

This is sufficient to proceed without a custom native text stack for ordinary
documents. Full-featured support for the rejected bidi/grapheme shapes would
require a separate explicit Unicode-state/native-layout investment.

Flutter's public [`ParagraphBuilder`](https://api.flutter.dev/flutter/dart-ui/ParagraphBuilder-class.html)
is a one-shot builder: after `build`, the builder is no longer usable, and the
result exposes no checkpoint import/export API. The maintained Rust
[`unicode-bidi`](https://docs.rs/unicode-bidi/latest/unicode_bidi/) crate can
produce paragraph bidi classes and embedding levels, but it is also a
whole-paragraph analysis and Flutter does not accept those levels as input to
`TextPainter`. It may become a differential oracle or classifier, but adding it
now would not make Flutter paragraph layout resumable. Phase 1 should therefore
start with certified ordinary checkpoints plus fallback, and reopen a Unicode
layout service only if the rejected shapes must become full-featured.

## Remaining physical-device gates

The device portion of Phase 0 is not complete until the following have receipts
on supported floor devices:

1. The full existing IME protocol plus active-source activation and handoff on
   iOS and Android, including Japanese/Korean composition, autocorrect,
   dictation, and hardware-keyboard paths where supported.
2. Native touch selection handles, magnifier, context menus, edge autoscroll,
   clipboard, and selection across mounted and unmounted shards.
3. VoiceOver and TalkBack traversal over paged semantics, including focus
   preservation during page replacement and edits.
4. Profile-build input-to-pixel and UI-isolate p99/p999 measurements on the
   60 Hz floor device and a 120 Hz target.

The example app's responsive overflows were fixed with compact accessible
status and editor controls. The shipped-parser integration smoke now passes
3/3 on the iOS 26.5 simulator, including bold and fenced-code round trips. A
physical iPhone build signed, installed, launched, and remained live as an
on-device Runner process, but its Dart VM service was not discovered after 60
seconds, so no physical test result is claimed. This isolates the attempted-run
failure to harness discovery rather than build/install/launch. Real keyboard,
touch-selection, VoiceOver, and profile-frame receipts remain open.

## Executable probes

```sh
flutter test --reporter expanded \
  test/prototype/flark_layout_checkpoint_differential_probe_test.dart \
  test/prototype/flark_phase0_ui_feasibility_test.dart \
  test/prototype/flark_phase0_complex_shaping_native_test.dart \
  test/prototype/flark_cross_block_gesture_probe_test.dart \
  test/prototype/flark_document_selection_coordinator_probe_test.dart \
  test/prototype/flark_product_feel_vertical_slice_prototype_test.dart \
  test/prototype/flark_incremental_wrap_convergence_probe_test.dart \
  test/prototype/flark_wrapped_paragraph_layout_probe_test.dart

flutter test --platform chrome --reporter expanded \
  test/prototype/flark_phase0_ui_feasibility_test.dart \
  test/prototype/flark_document_selection_coordinator_probe_test.dart

cd example
flutter test integration_test/flark_phase0_engine_feasibility_test.dart \
  -d macos --reporter expanded
flutter test integration_test/flark_phase0_engine_feasibility_test.dart \
  -d 9FDA7E97-D310-4197-9668-61C954F9EB5F --reporter expanded
```
