# Native qualification — September 5

Later evidence and the current candidate are in the
[input-context continuation](input_context_review_2026_09_05.md). The measurements
below describe the earlier full-document platform mirror.

**D0 remains open.** The native sweep ran in a resumed Flutter lifecycle and
completed all 21 shape/site cases. Sixteen met the frame threshold; five missed
the unchanged 16,667 µs p99 limit. These are measurements of the local dirty
candidate, not CI, release or physical-phone proof.

| Shape, largest block | Insert p99 µs | Backspace p99 µs |
| --- | ---: | ---: |
| Dense | 20,639 | 19,468 |
| List | 19,496 | 19,545 |
| Table | 19,328 | 19,305 |
| Nested containers | 19,121 | 18,998 |
| Unique references | 20,079 | 20,375 |

All sources were 32,767 UTF-8 bytes before insertion, and all 4,200 measured
post-input paints passed source/revision/caret checks. The failing sites edit
the middle of the admitted 4,000-character paragraphs that fill each bounded
structural fixture. Start/end sites and the prose/Unicode largest-block sites
met the threshold. The first cases overlapped a short core/Rust correctness run;
the focused reproduction below ran without that concurrent work.

## Focused reproduction and interpretation

A second production-workbench run of dense/largest-block reproduced the miss:
insert 19,498 µs p99, Backspace 19,434 µs. The frame breakdown was:

- command: 2,789 µs p99;
- input to frame build start: 17,475 µs p99;
- build: 1,759 µs p99;
- raster: 569 µs p99.

Most of the delay precedes frame build. This narrows the next investigation to
frame scheduling and work performed before the frame begins; it does not prove
that parsing, persistence, the input bridge, or the Flutter embedder is the
cause. Do not add paragraph splitting or rewrite layout merely because the
slow case happens to contain a long paragraph. Do not relax the live byte/shape
envelope or frame limit to close this B1.

The focused diagnostic retains 200 linked raw samples and reports nominal
display refresh of 120 Hz and device pixel ratio 2. Its original viewport field
used `Size.toString()`, which is not descriptive in this profile build; future
receipts now record width and height as numbers. The sweep's exact application
and library hashes were preserved before the next build. The focused timing
diagnostic did not preserve a separate executable hash, so it is supporting
diagnosis rather than a sealed candidate receipt.

## Longer workload and environment

The first workbench workload exposed a harness assumption: a dense heading's
initial legal caret is offset 3, rather than inside its hidden prefix at zero.
The mounted scenarios now assert explicit opening offsets for headings, lists,
tables and quotes. Reference definitions remain editable source rows at zero.
This corrected the test's expected behavior; no production editor change was
needed.

The corrected run completed its 100 measured open/edit/close cycles and reached
the sustained typing loop, then failed the foreground guard at 3:59 with an
inactive lifecycle. It produced no complete performance or memory receipt and
does not qualify the workload. The failure location establishes entry to the
sustained loop; the exact accepted-input count was not retained in that version.

A minimal-surface comparator and an extended trace attempt subsequently failed
their initial foreground gates. CUA then reported that the Mac was locked and
automatic unlock was unavailable. A one-second native stack sample and VM trace
from the comparator captured startup/waiting, not the failing measured edit,
and must not be used to attribute its delay. The locked/inactive attempts are
environment rejections, separate from the completed sweep's performance miss.

Both harnesses now retain raw samples in redirected stdout even if a later
assertion rejects the run. The workbench reports completed cycles and sustained
input counts; the frame harness supports shape/site/iteration filters for
diagnosis. Filters do not qualify the unsampled cases. Lifecycle checks remain
unchanged.

## Other current evidence

The rerun passed 249 core tests (including 1,000 generated histories, seed 2026),
84 host tests, 24 workbench tests, all 40 Rust tests, schema freshness,
native/bundled/fresh Wasm identity across 1,322 cases and the Rust-free consumer.
Analysis passed in all three Dart/Flutter packages. The same dense headless
facade diagnostic measured insert 2,263 µs p99 and Backspace 2,020 µs p99 at
16,694 bytes/769 blocks. It remains below the 4 ms kernel gate and is not native
frame evidence.

The normal `lib/main.dart` macOS profile target and Flutter web/Wasm build were
rebuilt without diagnostic Flark defines. All native profiling processes and
their bounded awake holds exited. The normal binary still needs its native
canaries; launching a rebuilt binary is not a dogfood pass.

Browser checks on the rebuilt workbench observed exact source, caret and visible
style after deleting the last character from italic, strong, strike, code and
nested formatting, then immediately typing `y`. Starting outside the owner
correctly resumes plain text; starting inside recreates its formatting.
Whitespace after emptying the nested owner exited that context, and Undo/Redo
restored the observed source and caret. Ordinary `alpha beta  gamma!` preserved
both spaces and caret 18. Browser clipboard attempts did not produce an observed
paste, so clipboard qualification remains open; those attempts are not passes.

## Next work

On an unlocked Mac, establish actual foreground before sampling. Compare the
same large-paragraph edit with and without the workbench, capturing the failing
interval's engine timeline and input/command/vsync timestamps. Resolve the cause,
then repeat the whole sweep and uninterrupted workbench workload. Finish real
AppKit keyboard/menu/clipboard, focus, resize, scrolling and process lifecycle
canaries on the final normal binary. No commit or push has been made.

Local records: [candidate and validation](receipts/2026-09-05-native/candidate.json),
[full sweep](receipts/2026-09-05-native/frame_sweep.json), and
[focused diagnostic samples](receipts/2026-09-05-native/frame_diagnostic.json).
