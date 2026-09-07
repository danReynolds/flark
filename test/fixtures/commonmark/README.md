# CommonMark Fixtures

This directory contains two fixture lanes:

1. Curated lane (`core_cases.json`, `gfm_cases.json`) for fast targeted checks.
2. Upstream lane (`upstream/common_mark_tests.json`, `upstream/gfm_tests.json`)
   for broad conformance scoring.

Upstream fixture source:

- Copied from `package:markdown` tool data (`tool/common_mark_tests.json` and
  `tool/gfm_tests.json` in markdown `7.3.0`), which tracks CommonMark/GFM
  example corpora used by that package.

V5's parse crate runs the 652 CommonMark and 670 GFM upstream cases through the
versioned render model and requires native/Wasm byte identity. The V4 profile,
its two supplied task-list cases, and the live-projection profile remain
historical evidence; they do not define V5's completion count.

Deviation register:

- `deviation_register.json` stores approved exclusions keyed by lane.
- Each entry should include:
  - `example` (numeric fixture id),
  - `owner`,
  - `reason`,
  - `targetMilestone`.

## Historical v3 coverage ledger

`v3_coverage_ledger.json` accounts for every CommonMark 0.31.2 fixture without
turning corpus inventory into a conformance score. Its statuses distinguish:

- numbered authoritative semantic probes;
- numbered intentional fail-closed behavior;
- intentional GFM extension/divergence; and
- fixtures for which v3 has no numbered conformance claim yet.

The final category includes both likely-working and incomplete grammar. It is
not a pass. The ledger deliberately does not credit fragment-only, synthetic,
or legacy-v2 coverage to v3.

The JSON file remains historical source material. The V4 ledger can still be
run with:

```sh
bash scripts/verify_v4_markdown_conformance.sh
```

Run the active V5 conformance and transport parity lanes from
`native/flark_parse` with `cargo test --release --locked` and
`./tool/verify_transports.sh`.
