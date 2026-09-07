# Typing performance and worker proposal — 2026-09-06

Retain the official Tree-sitter runtime/highlighter and the thin Rust adapter.
The smallest measured synchronous optimizations substantially reduce cost, but
complete highlighting still consumes too much of a frame for large snippets.
The proposed next step is synchronous edits/indentation with code colors in an
owned worker. [RFC 031](../../docs/architecture/rfc/rfc_031_code_color_worker.md)
specifies that product change and its host gates. The worker is implemented as
an optional component; Flark has not adopted it and the dogfood preview is unchanged.

## Changes retained

- Reuse the upstream highlighter's parser/cursor scratch, as its public API
  recommends. This alone did not materially improve the typing benchmark.
- Use public `Tree::edit` and incremental parsing for editing and Dart's fragment
  context selection. Retain only the last snippet's normal/wrapped trees per
  executing thread; reset them on language changes. The complete source and
  language still determine every result, with no caller-owned parser session.
- Keep the larger highlight/locals query initialization lazy. A first Enter
  compiles editing rules without paying for first-time colors.
- In ABI 3, send contiguous range ends and intern scope stacks. The large Dart
  journey's largest response shrank from 440,067 to 72,821 bytes. Dart validates
  UTF-8/UTF-16 agreement without allocating a substring and byte array per span.
  The public immutable span/edit meanings are unchanged; old assets are rejected.
- Skip syntax work for a typed closer that cannot outdent because other text
  precedes it on its line. Python branch alignment only runs at its configured
  colon trigger; a condition's closing parenthesis must wait for that colon.

No grammar fork, private upstream API or second highlighting interpreter was
introduced. The upstream highlighter still parses its complete input.

## Testing and measured scope

Eight Rust tests and 123 Dart tests pass, with strict Dart analysis and Rust
Clippy. The four synchronous transports agree on 130 cases: 45 highlighting
analyses and 85 hand-authored edit scenarios, each checking the next character.
Incremental parsing is compared with fresh parsing for every prefix of five
snippets and 800 seeded replacements, including context changes, CRLF and
multibyte characters. The comparison checks node kinds, hierarchy and ranges.

The native AOT isolate and both real browser runtimes each match direct analysis
on 170 post-edit/following-character snapshots. A 40-request burst returns only
the latest result. Controlled tests cover coalescing, stale malformed output,
language/plain/limit changes, current and stale failures, invalid input and
disposal while work is running. Browser probes also exercise custom relative
asset URLs. There is no Flutter input/paint or physical-device claim here.

All timings below are local observations on the Apple M1 Pro, macOS 26.2,
Dart 3.12.2 and Rust 1.98.0, from dirty base commit
`9fcb092f34a1ecd3a2b97d67b9bd3456e04ab410`. Node is 22.23.0; the in-app browser
reports Chromium 152. Source/asset hashes, full timing summaries and provenance
are in [the receipt directory](receipts/performance-2026-09-06/context.json).

The synchronous benchmark replays evolving source, including Enter and typed
closers, instead of repeatedly analyzing an identical string. Browser timings use
`performance.now` rather than the millisecond-resolution Dart/JS stopwatch.
The worker benchmark issues events about every 8 ms, records edit/application/
enqueue cost, and measures color delivery separately. It deliberately warms
grammars before timed journeys. Tables are p50/p95 in milliseconds and exclude
Flark transactions, layout and paint. These short runs are not p99 bounds.

### Synchronous edit plus highlighting near 8K units

| Language | Baseline Node/Wasm | Final Node/Wasm | Final native AOT |
| --- | --- | --- | --- |
| dart | 23.94/34.59 | 10.30/11.48 | 6.90/7.82 |
| javascript | 9.33/11.63 | 5.11/6.19 | 3.41/4.22 |
| python | 7.97/12.19 | 5.80/9.58 | 3.93/6.61 |
| yaml | 10.13/14.03 | 6.11/8.02 | 4.24/5.50 |

### Worker input path near 8K units

| Language | Native input | Browser dart2js input | Browser dart2wasm input | Browser dart2wasm color delivery |
| --- | --- | --- | --- | --- |
| dart | 0.30/2.02 | 0.40/2.10 | 0.40/2.20 | 8.80/10.20 |
| javascript | 0.32/1.40 | 0.40/1.60 | 0.40/1.60 | 4.80/5.90 |
| python | 0.35/1.28 | 0.50/3.80 | 0.50/4.10 | 5.60/9.20 |
| yaml | 0.30/1.62 | 0.40/2.10 | 0.50/2.20 | 5.70/7.60 |

Only completed, current results contribute to color-delivery timing. The 40-request burst independently checks supersession. Small snippets in these runs are approximately 373–388 units; large snippets begin at 8,062–8,073 units and grow through the typing journey.

Worker startup in these single observations was 197.8 ms native, 16.3 ms in dart2js and 10.6 ms in dart2wasm. This is worker readiness, not first-language query preparation.

A separate first-Enter observation for a 504-unit Dart snippet, after ordinary insertion, was 3.39 ms native and 13 ms under Node/Wasm. The earlier checkpoint recorded 21.42 and 49 ms respectively. The JS cold measurement has millisecond resolution. This improvement does **not** remove the first-use gate: prepare editing grammars during host initialization, before accepting input, and verify that path in the actual host.

The final Rust Wasm asset is 4,151,230 bytes (819,869 bytes gzip). The worker also adds a small shipped module and a separate runtime instance. Startup, memory and CSP packaging remain host measurements.

The stage diagnostic now changes source while measuring incremental parsing. Highlighter-event time includes its own parse, so the diagnostic stages must not be summed. Full raw timing summaries, including maxima and any noisy outliers, are retained in the receipts.

## What this changes in the plan

The original all-synchronous coloring approach has not cleared the combined
frame gate. The worker prototype gives a concrete alternative to take into host
qualification; its measured input path has room for evaluation without replacing
the upstream highlighter. Keep the 8,192-unit admission limit and the four-language
scope while doing that integration. Do not infer twenty-language readiness.

The next milestone must test the first Flutter frame and the later color-only
frame, actual selection and undo, containers, IME, switching/deleting fences,
worker errors and editor disposal. Host adoption must compare the current fence,
source and language, clear invalid colors immediately, and prevent colors from
changing text metrics. Review the plain-to-colored transition during continuous
typing; it could flicker or leave a busy snippet plain until typing pauses.

Also measure first use without prewarming, multiple visible fences, worker memory,
asset/CSP packaging, and difficult admitted shapes such as deeply nested or
heavily malformed code. Python's incremental parse remains noticeably more
expensive than the other three; a worker does not remove that synchronous
indentation cost. These are explicit acceptance gates, not covered by a green
component or median-timing result.

The methodology correction is to measure the full edit-plus-analysis sequence,
keep cold and warm paths separate, compare cached syntax against fresh syntax,
and check delayed-result ownership. Component parity supports integration;
the visible authoring contract still needs its own direct scenarios and dogfooding.
