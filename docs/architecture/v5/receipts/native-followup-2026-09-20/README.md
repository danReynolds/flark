# Native follow-up receipts

See the [follow-up report](../../native_followup_2026_09_20.md). Baseline:
`e7136388dd0cb5d080b05adfba2ef4099b570bf5`; production code and performance
budgets/workloads were unchanged. These receipts do not establish a full pass.

- `workbench-hidden.log.gz`: rejected initial foreground preflight.
- `workbench-active.log.gz`: complete manually activated run; six operation
  groups failed their existing timing limits. No AXTree errors.
- `workbench-summary.json` and `workbench-samples.json.gz`: all reported groups
  and raw samples linked to engine frame numbers; 100 cycles / 3,000 edits.
- `workbench-build.json`: recorded executable/App framework hashes and host
  metadata. Its modified-input entry names the temporary, unused semantics
  probe edit present while the workbench built; that file was later restored.
  Native parser-library binary hashes were not captured for this diagnostic.
- `ax-widget-runner.log.gz`: temporary attended widget-runner comparison. Both
  host workloads completed with native activation and zero AXTree errors, but
  test teardown rejected the newly platform-owned semantics handle. Not a pass.
- `ax-app-launch.log.gz`: normal-binding diagnostic built/launched; the Mac
  locked before native interaction. No successful host results are claimed.
- `guard-test.log.gz`, `analysis.log.gz`, `example-tests.log.gz`: final local
  refusal regression, clean analysis and 32 passing example tests.
- `restore-normal.log.gz`: successful restoration of the ordinary profile app.
- `source-sha256.txt`: reviewed diagnostic/test and unchanged workload inputs.

Local VM service and DevTools connection URLs are redacted from retained logs.
The driver's result file was null after the failed workbench test; the structured
records above come from the harness's explicit receipt and teardown markers.
No CI, physical IME or VoiceOver usability result is implied.
