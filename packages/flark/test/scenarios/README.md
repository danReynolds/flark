# Live-editor scenarios

Each JSON file defines one user interaction once: initial Markdown, a
source-relative activation point, timed actions, schedule variants, and an
exact final outcome. The same file is consumed by two intentionally different
adapters:

- `live_editor_scenario_test.dart` is the fast inner loop. It drives the real
  controller, Core worker, and Rust engine without a platform window. It owns
  exact source, selection, revision, resync, fault, and settled-presentation
  assertions.
- `live_editor_scenario_macos.swift` launches the profile dogfood app and sends
  real mouse and keyboard events. It owns platform routing and assertions over
  states sampled on actual Flutter frame boundaries.

Run the fast lane after editing input or projection machinery:

```sh
./scripts/run_v4_live_editor_scenario.sh headless
```

Run both lanes before a dogfood handoff:

```sh
./scripts/run_v4_live_editor_scenario.sh all
```

The native app is rebuilt by default. Reuse an already-current profile build
with `FLARK_SCENARIO_REUSE_APP=1`. The Rust library is reused when present;
set `FLARK_SCENARIO_REBUILD_CORE=1` after native changes.

When a real interaction fails, first reduce its trace to one scenario and a
small number of timing schedules. Add broad matrices only after the reduced
case proves that another dimension changes the result. Controller samples are
not painted frames: transient presentation is a native-adapter assertion, not
a headless failure.
