# CommonMark Fixtures

This directory contains two fixture lanes:

1. Curated lane (`core_cases.json`, `gfm_cases.json`) for fast targeted checks.
2. Upstream lane (`upstream/common_mark_tests.json`, `upstream/gfm_tests.json`)
   for broad conformance scoring.

Upstream fixture source:

- Copied from `package:markdown` tool data (`tool/common_mark_tests.json` and
  `tool/gfm_tests.json` in markdown `7.3.0`), which tracks CommonMark/GFM
  example corpora used by that package.

The active v4 product profile is `flark-gfm-0.29-v2`: official GFM 0.29-gfm
is normative, with the two omitted official task-list examples supplied by
`test/fixtures/v4/task_list_profile_cases_v1.json`. CommonMark 0.31.2 is kept as
a separate compatibility diagnostic and never changes GFM conformance status.
Live editor projection is independently versioned as `flark-live-v1`.

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

The JSON file remains historical source material after the direct v4 cutover.
Run the active v4 normative GFM and CommonMark compatibility ledgers with:

```sh
bash scripts/verify_v4_markdown_conformance.sh
```
