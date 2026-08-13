# Live-editor scenarios

> **Status:** canonical v1 portable corpus. The no-window and mounted Flutter
> runners compile the same JSON into the same hashed plan and execute it with
> one shared assertion engine. See the
> [live-editor test strategy](../../../../docs/testing/live_editor_test_strategy.md)
> for layer boundaries and the remaining real-platform work.

Each JSON file defines one user interaction once: initial Markdown, an
unambiguous source-relative activation point, typed actions, explicit barriers,
schedule variants, checkpoints, and an exact final outcome. A strict compiler
rejects unknown or ambiguous input and emits a canonical SHA-256 plan. Both
portable drivers consume that exact plan:

- `live_editor_scenario_test.dart` is the fastest inner loop. It drives the real
  controller, Core worker, and Rust engine without a platform window. It owns
  exact source, selection, revision, resync, fault, and settled-presentation
  assertions.

Semantic surface actions use source-relative targets as stable identities. For
example, `toggleTaskAtUtf16` names any offset inside a certified task row. The
no-window driver invokes the framework-neutral controller action; a native
driver resolves the same target to the painted checkbox and performs a real
pointer click.

- `live_editor_scenario_surface_test.dart` mounts the production render object
  and adds bounded observations emitted by actual paint calls. It uses the same
  actions, barriers, and assertions as the no-window driver.

The macOS CGEvent helper is a thin native actuator behind the same Dart
compiler/executor as the portable runners. It owns neither scenario semantics
nor assertion policy. Its small canary pack reuses one profile app process and
adds real keyboard, pointer, pasteboard, and paint evidence. Future simulator
and physical drivers reuse the same Dart plan/executor boundary; device-only
IME, touch, accessibility, and lifecycle canaries remain separate evidence.

Run both portable lanes after editing input or projection machinery:

```sh
./scripts/run_v4_live_editor_scenario.sh portable
```

Focus one scenario in either lane:

```sh
./scripts/run_v4_live_editor_scenario.sh headless packages/flark/test/scenarios/simple_list_continue_exit_type.json
./scripts/run_v4_live_editor_scenario.sh surface packages/flark/test/scenarios/simple_list_continue_exit_type.json
```

Run the native-input canary pack with `macos`, or include it after the portable
corpus with `all`. The native app is rebuilt by default; reuse an
already-current profile build with `FLARK_SCENARIO_REUSE_APP=1`. The Rust
library is rebuilt incrementally by default; `FLARK_SCENARIO_REUSE_CORE=1`
explicitly opts into the existing artifact.

When a real interaction fails, first reduce its trace to one scenario and a
small number of timing schedules. Add the smallest ordinary controller/Core
test that localizes the mechanism, and keep the portable scenario when the bug
crossed layers or was user-visible. Broad raw-callback and Markdown construct
matrices remain ordinary tests. Controller snapshots are not painted frames;
only the mounted driver's render-bound observations can prove paint predicates.
