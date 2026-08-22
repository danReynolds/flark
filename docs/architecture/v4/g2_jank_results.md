# G2 — jank harness: first run

> **Historical pre-cutover evidence (2026-08-05).** This blocked result and its
> command describe the retired v3 root package, not the active v4 product. The
> harness is preserved at
> [`legacy/root_package/example/lib/g2_jank_harness.dart`](../../../legacy/root_package/example/lib/g2_jank_harness.dart).

**Status:** BLOCKED. The harness works; the engine could not survive long
enough to be measured. 2026-08-05.

Historical command as recorded (not an active command):
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

## D-A bisected: the engine is innocent

Ran 2026-08-06 using the archived
[`g2_dense_bisect.dart`](../../../legacy/root_package/example/lib/g2_dense_bisect.dart),
which drives `FlarkV3DocumentRuntime` **directly — pure Dart, no Flutter layer**
— over the same constructs and sizes, opening, applying one edit, and waiting
for structure-current.

```
bisect heading/x1        bytes=    22 :: OK  5ms
bisect bold/x1           bytes=    42 :: OK  5ms
bisect link/x1           bytes=    60 :: OK  6ms
bisect bullet-list/x1    bytes=    39 :: OK  5ms
bisect ordered-list/x1   bytes=    42 :: OK  5ms
bisect all-inline/x1     bytes=   101 :: OK  6ms
bisect paragraph/5KB     bytes=  5128 :: OK 12ms
bisect heading/5KB       bytes=  5134 :: OK 21ms
bisect bold/5KB          bytes=  5146 :: OK 12ms
bisect link/5KB          bytes=  5144 :: OK  9ms
bisect bullet-list/5KB   bytes=  5123 :: OK 33ms
bisect ordered-list/5KB  bytes=  5146 :: OK 33ms
bisect all-inline/5KB    bytes=  5148 :: OK 10ms
bisect mixture/1KB       bytes=  1387 :: OK  8ms
bisect mixture/5KB       bytes=  5554 :: OK 13ms
bisect mixture/10KB      bytes= 10647 :: OK 20ms
bisect mixture/25KB      bytes= 25926 :: OK 32ms
```

**22 of 22 pass.** Every construct the dense fixture uses, alone and mixed,
at every size G2 failed on — including the full mixture at 25 KB in 32 ms.

So **D-A is not a parser defect.** `parserFailure: 4` was the state the runtime
*ended up in*, not where the fault began. The only difference between this
probe and the G2 run is the Flutter integration layer — the managed binding and
the viewport presentation source.

That collapses D-A and D-B into **one defect in the Flutter viewport layer**:
it issues queries the host rejects as out-of-authority, which then faults the
runtime. D-B caught it uncaught and fatal; D-A is the same wound reported one
step downstream.

**This is the most useful thing G2 has produced.** It clears the component
RFC 024 keeps and convicts the component RFC 024 deletes. The engine handles
realistic Markdown at 25 KB in 32 ms; the integration layer built around the
`EditableText` island cannot survive being driven.

## Next

1. ~~Diagnose D-A~~ — done: not the engine (above). Remaining work is to find
   why `FlarkV3ManagedViewportPresentationSource` requests a window the host
   considers out of authority. Worth only enough effort to confirm the rebuild
   is the right fix rather than a patch.
2. Diagnose D-B — why does a routine viewport progress query produce an
   out-of-authority receipt, and why is it uncaught?
3. Give 0x0111 a discriminant. Four faults sharing one opaque code is why each
   of these took a separate investigation.
4. Re-run G2 once the stack survives, then measure what we actually came for.
