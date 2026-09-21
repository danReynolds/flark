# Merge review receipts

- `analysis.log.gz`, `example-tests.log.gz`: clean analysis and 38 passing
  example tests, including the profile-driver receipt validator regressions.
- `conformance.log`: both Rust corpus tests passed (1,322 cases each).
  The trailing blank line was trimmed during integration; result text is unchanged.
- `coverage-probe.dart.txt`, `coverage-probe.log`: temporary mounted Fleury
  inspection of rules, footnotes, HTML and Setext headings. To reproduce, copy
  the probe into `packages/flark_fleury/test/`, run `dart test` on that file, then
  remove it. Its empty-rule observation documents a gap, not desired behavior.

The successful native measurements and their exact build/source provenance stay
in `../native-attended-2026-09-20/`. The new validator replayed those actual driver
results without rerunning the foreground workload. No CI result is claimed.
