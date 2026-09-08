# Single-editor playground and visual color picker

Follow-up to the initial theming receipt, 2026-09-08. No commit, merge or CI.

Commands, run in `packages/flark_flutter/example` with Flutter 3.44.4:

```sh
flutter analyze --no-pub
flutter test --no-pub test/theme_playground_test.dart
flutter test --no-pub .dart_tool/flark_theme_export_test.dart
flutter build web --wasm
flutter test --no-pub
```

Analysis passed; four playground checks, two exported consumers and all 31
example tests passed. Wasm build passed. Parent host source/test hashes remain
unchanged, so its 843-test qualification was retained without repetition.
The color picker is an example dependency only (FlexColorPicker 3.8.0).

Hands-on normal-browser canaries: single editor/panel, wheel, hex and opacity,
Dark/Notebook presets, color changes between typed characters, Undo, custom
link popover and editing sheet, next typed character, exact source restoration,
configuration view and verified clipboard contents for two different presets.
The final open page uses Dark with its color picker expanded; exploratory edits
were undone. No browser warning/error logs observed.

Explicit Enable accessibility revealed a separate blocked custom-editor input
route; form input still worked, and the DOM hit target was a full-viewport
semantics node. A reload without that mode restored normal input. This is an
open investigation, not a passed accessibility canary. External navigation and
native/device/performance gates remain open.
