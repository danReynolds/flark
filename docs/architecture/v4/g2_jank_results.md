# G2 — jank harness: first run

**Status:** BLOCKED. The harness works; the engine could not survive long
enough to be measured. 2026-08-05.

Harness: `example/lib/g2_jank_harness.dart`. Run with
`flutter run --profile -d macos -t lib/g2_jank_harness.dart`.
Eight configurations planned (5 KB / 25 KB / 100 KB / 1 MB × plain / dense),
10 keystrokes/sec, 2 s warm-up + 15 s measured, `FrameTiming`-based.

## What happened

```
g2| harness start  configurations=8 cps=10 warmup=2s measured=15s
g2| plain/5KB  open=1265.4ms paint=1455.1ms structure=1368.5ms
g2| dense/5KB  open=56.0ms   paint=timeout  structure=214.2ms
g2| keystroke rejected: Bad state: The Flark v3 document runtime is not
    writable (state: faulted, parserFailure: 4, hostRejection: closed).
    … repeated for every subsequent keystroke …
g2| dense/5KB done frames=12 keystrokes=17 mode=editable-delta
g2| dense/25KB open=167.8ms paint=1750.4ms structure=538.8ms
[ERROR] Unhandled Exception: FlarkV3NativeHostException(
    queryStructuralRangeReceipt, status=0x0111:
    native host returned an out-of-authority range receipt)
  at flark_v3_native_host_store.dart:1868  _decodeBlockRangeQueryOutcome
  …
  at flark_v3_managed_viewport_presentation_source.dart:374
        _queryStructuralWindowAtSourcePoint
  at flark_v3_managed_viewport_presentation_source.dart:156
        handleRuntimeProgress
```

**No frame timings were produced.** Zero of eight configurations completed.

## Three defects, all new

**D-A · Markdown-dense content faults the parser at 5 KB.**
`plain/5KB` opened and painted. `dense/5KB` — the same size, but with headings,
lists, bold/italic/code and links — reached `structure` yet **never painted**
(`paint=timeout`), then entered `state: faulted, parserFailure: 4` and rejected
every subsequent keystroke. Lines were kept well under 1 KB, so this is *not*
the known over-window defect. Realistic Markdown at a trivial size faults.

**D-B · The virtualized viewport query throws an unhandled exception at 25 KB.**
`queryStructuralRangeReceipt` returns "out-of-authority range receipt" and
nothing catches it, so it escapes to the Flutter error handler and takes the
app down. It is thrown from `handleRuntimeProgress` — the normal progress path,
not an edge case. Note this is **0x0111 again**, the same catch-all status that
has now stood for at least four distinct faults.

**D-C · Cold open is far slower than the recorded receipts.**
`plain/5KB` took 1,265 ms to open and 1,455 ms to first paint. For 5 KB. The
ledger's figures are 0.25–0.36 s at 1 MiB. Even allowing for first-configuration
warm-up, this is two to three orders of magnitude off the shape those receipts
imply, and it wants explaining before any cold-open contract is written.

## What this means for RFC 024

**The instrument is fine; the subject could not stand up.** That is itself the
most useful thing G2 has told us, and it is worse news than slow frame times
would have been. Marginal frame times would be a tuning problem. Faulting on
ordinary Markdown at 5 KB, and crashing the app from the routine viewport
progress path at 25 KB, is a robustness problem in the v3 *integration layer* —
precisely the layer RFC 024 §8 D3 already proposes to delete and rebuild.

It also confirms the four P0 defects found earlier were not exhaustive. Every
time this stack is driven through a genuinely new path, it produces new faults.

**And the recurring theme now has a fourth instance.** D-A closes the runtime
with `parserFailure: 4` and no surfaced reason; the G3 paste stall goes
quiescent with no reason; the earlier review found a 0x0111 terminal fault
visible only as the word "closed" in a diagnostic tile, and a surface parked in
`awaitingActivePresentation` forever. Four separate silent-stop states is not
four coincidences — it is a missing invariant. **RFC 024 should state it
explicitly: the engine must always be able to say that it has stopped, and
why.**

## Next

1. Diagnose D-A — what is `parserFailure: 4` on dense 5 KB? Smallest
   reproducing fixture.
2. Diagnose D-B — why does a routine viewport progress query produce an
   out-of-authority receipt, and why is it uncaught?
3. Give 0x0111 a discriminant. Four faults sharing one opaque code is why each
   of these took a separate investigation.
4. Re-run G2 once the stack survives, then measure what we actually came for.
