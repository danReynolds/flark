# Attended native receipts

See the [result and review](../../native_attended_2026_09_20.md).

- `ax.log.gz`, `ax-results.json`, `ax-build.json`: normal-app native comparison;
  both hosts completed 50 cycles and 1,000 edits with zero AXTree errors.
- `workbench.log.gz`, `workbench-summary.json`, `workbench-samples.json.gz`,
  `workbench-build.json`: initial unchanged harness; complete workload but one
  timing failure and 3,858 AXTree errors. Retained as failed evidence.
- `frames-preflight-rejected.log.gz`: invalid initial setup check, no measured
  cases. The driver's success banner/exit code did not make this a pass.
- `frames.log.gz`, `frames-summary.json`, `frames-samples.json.gz`,
  `frames-driver.json.gz`, `frames-build.json`: corrected native preflight, all
  24 cases passing with no AXTree errors. Driver data contains full raw samples.
- `workbench-corrected.log.gz`, `workbench-corrected-summary.json`,
  `workbench-corrected-samples.json.gz`, `workbench-corrected-driver.json.gz`,
  `workbench-corrected-build.json`: corrected startup, full workload and all 90
  timing groups passing with zero AXTree errors and no foreground loss.
- `cpu-start.txt`, `cpu-frames.txt`: CPU snapshots; process names only. A Python
  process was still busy. No idle-machine claim is made.
- `format.log.gz`, `analysis.log.gz`, `tests.log.gz`: final local verification.
- `restore.log.gz`, `restore-build.json`: ordinary profile app restored afterward.
- `source-sha256.txt`: source inputs for the diagnostic, profiles and normal app.

Local VM-service/DevTools connection URLs are redacted. No CI, VoiceOver usability,
physical IME or ordinary-app OS-lifecycle result is implied by these receipts.
