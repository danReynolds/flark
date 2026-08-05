# G3 — in-frame synchronous pump: first results

**Status:** partial. 1 KB complete; 100 KB and 1 MB did not finish inside a
10-minute run. 2026-08-05.

Harness: `example/lib/g3_headless_probe.dart` + `g3_inframe_engine.dart`.
Command:

```
dart run lib/g3_headless_probe.dart --lib <abs path>/libflark_comrak_bridge.dylib
```

Drives the existing synchronous, fuel-bounded FFI endpoint from a pump loop
instead of the isolate, on plain-paragraph fixtures.

## Verbatim output (1 KB, 1,170 bytes, 7 paragraphs)

```
g3 cold      frames=13 wall=75ms maxframe=7215us p50frame=5901us exact=true
g3 edit-unbounded  n=120 oneshot=120/120 p50=2706us p90=5208us p99=18788us
                   max=23813us iters_p50=62 iters_max=64
                   under2ms=12/120 under4ms=96/120 under8ms=116/120
g3 edit-budgeted   budget=4000us n=120 oneframe=113/120 frames_p50=1
                   frames_p99=2 frames_max=2 firstpump_p50=1760us
                   firstpump_max=5164us
g3 sustained       pumps=240 budget=4000us exact_at_pump_end=240/240
                   p50=1648us p90=2258us p99=3531us max=3733us
g3 abort           paste=32789 budget=1000us frames=100000 exhausted=1
                   maxframe=1616us exact=false source_intact=true
g3 native          dispatch=6257 poll=19234 candidate=18753 encode=4329
                   encoded_bytes=2929112 native_us=552044
```

## What this establishes

**The core RFC 024 claim holds at 1 KB.** Driving the parser synchronously from
a frame callback with a 4 ms budget: **113 of 120 single-character edits reach
exact structure in one pump**, and no edit ever needs more than two. Under
sustained typing, all 240 pumps reached exact with p99 3.53 ms and max 3.73 ms —
inside the 4 ms budget and comfortably inside an 8 ms frame.

**Budgeting is what makes it safe.** Unbudgeted, pumping to completion has a
p99 of 18.8 ms and a max of 23.8 ms — it would drop frames. The budget converts
that tail into a second frame instead of a dropped one. The mechanism, not the
raw speed, is what delivers the contract.

**Fuel-abort works.** A 32 KB paste against a 1 ms budget stayed within budget
every pump (max 1.62 ms) and left the source intact.

## What this exposes

1. **The wire protocol is the bottleneck, and it is expensive.** 62 poll
   iterations per single-character edit, and **2.9 MB encoded for a 1 KB
   document** across the run. That is pure boundary overhead — encode, poll,
   decode — for work the parser does in microseconds. It is also why 100 KB and
   1 MB did not finish. This is direct evidence for RFC 024 §8 D3's plan to
   replace the endpoint protocol with a lean direct FFI: the isolate's protocol
   is not just unnecessary in-process, it is the dominant cost.
2. **The 32 KB paste did not converge — and the failure mode is a silent
   stall, not slow progress.** Diagnosed 2026-08-05.

   The receipt reads `frames=100000 exhausted=1 maxframe=1616us p99frame=0us`.
   Only **one** pump of a hundred thousand ever exhausted its budget, and the
   99th percentile pump took **zero microseconds**. So 99% of those pumps did
   no work at all.

   `pump()` (`g3_inframe_engine.dart:563`) advances the scheduler and the
   endpoint, and breaks out the moment neither reports progress. The outer
   probe loop then re-enters, finds nothing to do again, and spins. Meanwhile
   `document.failure` is never set — the engine reports no error.

   So after the paste the engine goes **quiescent while `isExactCurrent` is
   still false**: it believes it has no work left, but the document is not
   reparsed. The source is byte-intact (`source_intact=true`), so nothing is
   corrupted; it simply stops converging and says nothing.

   That is the same class of defect the earlier defect review found twice
   already — a terminal or stalled state that is never surfaced to the caller.
   It is very likely the engine took a fail-closed path that requires a fresh
   command the harness never issues, rather than an engine-internal hang.

   **Next diagnostic step:** log the endpoint's state and last publication
   outcome at the moment it goes quiescent, and check whether a clean-rebuild
   command is expected but unscheduled. Until then G3 is *not* passed — the
   in-frame pump is proven for ordinary edits, unproven for large pastes.
3. **Cold open at 1 KB takes 13 frames / 75 ms**, with a 7.2 ms max frame. Fine
   at this size, but it is 13 frames for a trivial document — consistent with
   per-round-trip overhead dominating.

## Next

- Diagnose the paste non-convergence.
- Re-run 100 KB and 1 MB with a longer budget, or after reducing round-trips.
- Then G2 for `FrameTiming` under a real Flutter surface.

The headline for RFC 024: **the in-frame pump works and the budget mechanism
does its job — but the protocol overhead must go, and the case for the lean FFI
is now measured rather than argued.**
