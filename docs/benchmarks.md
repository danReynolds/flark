# Benchmarks

Flark v4 performance claims are receipt-based. Source code, local unit tests,
foregrounded profile runs, committed benchmark JSON, and physical-device runs
are different proof levels and must not be conflated.

## macOS frame receipts

Run one foregrounded profile workload:

```sh
bash scripts/profile_v4_macos.sh
```

Sweep the supported source sizes and shapes into JSONL:

```sh
bash scripts/profile_v4_sweep.sh /tmp/flark-v4-sweep.jsonl
```

These scripts use `flutter drive --profile` against the v4 example. They wake
and hold the display because a background or sleeping display cannot produce
valid frame timing evidence. A rejected or throttled receipt is not a pass.

## Streamed-open receipt

Build the feature-gated ABI in release mode, then run:

```sh
FLARK_V4_LIBRARY_PATH=/absolute/path/to/libflark_abi.dylib \
  bash scripts/profile_v4_streamed_open_macos.sh
```

The native library must include the `opening-session` feature. This receipt
measures first certified paint separately from completion of source admission.

## Certification stress

```sh
bash scripts/verify_v4_certification_stress.sh
```

The stress lane exercises the supported document-size and density envelopes;
it is deliberately outside the everyday gate.

## Committed evidence

Machine-readable schemas, workload definitions, baselines, and device results
live under [`benchmark/v4/`](../benchmark/v4/). Qualification tests validate
their schema and provenance. Historic v2/v3 benchmark prose is archived under
[`legacy/docs/v2_v3/`](../legacy/docs/v2_v3/).
