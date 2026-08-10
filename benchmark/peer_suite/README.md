# macOS peer-suite coordinator

This directory is the cross-peer authority for
`m0-mac-competitor-profile-v1`. The Quill and SuperEditor packages deliberately
remain peer-local harnesses; neither may promote its own output into a cohort
claim.

The frozen full plan contains 234 fresh profile processes in three groups of
78. Each exact size/workload/location/replicate case runs as an adjacent Quill
and SuperEditor pair. Peer-first order alternates, while size order rotates by
a three-row Latin square. Each group begins only after a recorded five-minute
idle interval. The full runner refuses to start unless the operator explicitly
attests exclusive machine use.

## Cheap wiring check

The dry run creates and validates the complete plan without building or
launching either GUI app. Its receipt is intentionally non-claim evidence:

```sh
dart run benchmark/peer_suite/tool/run_peer_suite.dart --dry-run
```

## Full profile protocol

Do this only when the Mac can remain plugged in, with Low Power Mode off and no
concurrent build, test, benchmark, indexing, screen-recording, or agent work:

```sh
dart run benchmark/peer_suite/tool/run_peer_suite.dart \
  --execute \
  --exclusive-machine-attested \
  --flutter=/absolute/path/to/flutter
```

The coordinator builds both profile apps before the first idle interval, then
interleaves their fresh processes. It checkpoints an explicitly ineligible
`suite-state.json` after every process; an interrupted run must be restarted so
all three group-level idle intervals remain honest.

The final validator fails closed on duplicate process IDs or artifact paths,
hash drift, inexact fixture bytes, final-export byte/hash mismatch, source
fidelity, missing raw input evidence, and any input whose selected frame did
not begin building strictly after model acceptance. For paste, it also requires
both peers to prove the identical 2-warmup/20-measured sequence of base source,
one 32 KiB paste, and complete platform-backspace reset; the final source must
be the unchanged fixture. Paste/reset requests must be uniquely linked and
strictly interleaved through input ingress, model acceptance, raster, and timing
callback before the next transition may begin. It reports two separate
decisions:

- `completionEnvelopeEligible` controls whether the local two-peer completion
  boundary can be resolved and may remain true when longest-synchronous-span
  capture is unavailable.
- `performanceClaimEligible` additionally requires full measurement fields,
  including longest-synchronous-span evidence, and a coordinator-produced
  cross-process p50/p90/p99/max aggregate for cold/open and input latency.
  Coordinator v1 deliberately does not materialize that aggregate, and the
  current peer runners do not capture longest synchronous span, so a completed
  suite is expected to remain performance-claim-ineligible.

The next competitor probe is mechanical: 1 MiB to 5 MiB, 5 MiB to 10 MiB, and
10 MiB to 20 MiB. That derived probe never changes Flark's independently fixed
10 MiB product target. Aggregate `claimEligible` remains false for a dry or
incomplete run, and a completed run can resolve only the scoped leading
embeddable-Flutter editor-SDK boundary—not a market-wide editor claim.
