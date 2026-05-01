# CommonMark Fixtures

This directory contains two fixture lanes:

1. Curated lane (`core_cases.json`, `gfm_cases.json`) for fast targeted checks.
2. Upstream lane (`upstream/common_mark_tests.json`, `upstream/gfm_tests.json`)
   for broad conformance scoring.

Upstream fixture source:

- Copied from `package:markdown` tool data (`tool/common_mark_tests.json` and
  `tool/gfm_tests.json` in markdown `7.3.0`), which tracks CommonMark/GFM
  example corpora used by that package.

Deviation register:

- `deviation_register.json` stores approved exclusions keyed by lane.
- Each entry should include:
  - `example` (numeric fixture id),
  - `owner`,
  - `reason`,
  - `targetMilestone`.
