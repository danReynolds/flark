# Native qualification merge review — 2026-09-20

Reviewed the diff from `e713638` through `f22b776`, including diagnostic lifecycle,
controller disposal, native preflight, measurement assertions, receipts and the
claimed qualification boundary. Production renderer/parser/kernel behavior is
unchanged. This record describes the review performed in this task; it is not
an independent reviewer approval or a CI result.

## Findings resolved before merge

- **P2: generic driver false success after suite-setup failure.** The retained
  rejected preflight had no measurements but the upstream integration driver
  printed success and exited zero. Added `test_driver/profile.dart` for full
  qualification: preserve response data even on failure, then reject missing
  data, missing/duplicate frame cases, incomplete linked sample sets, absent
  native activation, failed frame limits, failed workbench limits and incomplete
  sustained/open workloads. The generic driver remains for explicitly filtered
  diagnostics; README commands now select the validating driver for full runs.
  Native/framework error logs must still be inspected.
- **P3: stale Fleury dependency documentation.** The README named #260's revision
  while both package/example manifests pin the reviewed #262 commit. Updated
  the README to the actual merged pin and linked the current coverage audit.

Five new tests replay the real passing native driver receipts, then reject
missing setup data, partial/duplicate cases, truncation, missing native semantics
and a timing/memory failure. Full example analysis is clean and all 38 tests pass.
The postprocessing change does not alter the native workload, runtime binary,
source/caret assertions or timing budgets, so the retained passing native runs
remain applicable; another foreground performance run was not needed.

No further blocking issue was found in this branch. The standalone native probe
still checks exact source/caret after every frame, retains foreground/native
activation guards, disposes controllers after unmount, and prevents a failed host
from advancing to a success screen. Native handles are owned by the OS; the test
harness does not synthesize or dispose them to make leak checks pass.

## Coverage audit and cleanup

The current Markdown report separates recognition/editing from host presentation.
A temporary Fleury mounted probe confirmed its missing horizontal-rule paint and
partial footnote presentation; neither is silently counted as complete support.
Those pre-existing feature gaps are recorded for follow-up rather than folded
into this qualification-driver change. The probe was removed from executable
tests; its source/output are retained for reproduction.

The Rust conformance/extraction target passed over the 1,322-case corpus, with
one registered CommonMark deviation and no registered GFM deviations. This does
not establish complete UI authoring coverage. All receipts remain local/native;
CI is deliberately skipped and D0 still has the platform checks listed in the
[attended report](native_attended_2026_09_20.md).

[Review receipts](receipts/native-review-2026-09-20/) include final local analysis,
tests, conformance output and the exploratory paint probe.
