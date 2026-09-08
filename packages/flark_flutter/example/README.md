# Flark Flutter example

Run `flutter run -d macos --profile`, or use the web build instructions below.
Open **Theme playground** with the palette button in the workbench. On the web,
`?theme=1` opens the playground directly.

The playground puts a theming panel beside one live editor. On narrow screens,
the panel sits above the editor. Its sample document is separate from the
workbench's saved Draft and Tour documents.

Try this tour:

1. Choose Light, Dark or Notebook, then click the link-color or app-accent swatch
   to open a visual picker. The wheel, opacity slider and hex field update live.
2. Explore Typography, Blocks and Syntax colors using the Customize selector.
   Each setting updates the editor immediately.
3. Enable Custom link controls, click a link, and use its branded popover and
   editing sheet. Edit or remove the link, Undo, and keep typing.
4. Switch between ambient theming and per-editor overrides. Reset theme keeps
   your writing; Reset sample restores only the playground document.
5. View or copy the Dart configuration. It includes a public-API widget with
   editor wiring and the complete custom-control source when enabled. Supply
   your editor controller; `lib/backend.dart` handles native/web setup.

The example's application code imports public package exports only. The
playground configuration is in `lib/theme_settings.dart`; the replaceable
popover and editing sheet are in `lib/custom_controls.dart`. Defaults adapt to
brightness. Explicit custom colors remain explicit, so check their contrast
against the backgrounds you choose.

Visual color selection uses [FlexColorPicker](https://pub.dev/packages/flex_color_picker/versions/3.8.0)
in this example only. It adds no dependency to the Flark host or kernel.
The public read-only viewer still accepts the same theme; its parity is covered
by host tests without duplicating the playground document.

Local example checks include compiled consumers generated from the actual
configuration exporter:

```sh
flutter test test/theme_playground_test.dart
flutter test .dart_tool/flark_theme_export_test.dart
```

## Editing workbench and qualification

A standalone local-draft application for qualifying the editor before owner
handoff. **D0-web is ready for exploratory owner dogfooding.** Vertical navigation,
checkbox hover and immediate typed fence creation are fixed. Code now supports
visible selection, syntax colors, a language picker, indentation and typed-closer outdent. The complete
web/Wasm build runs in Codex's embedded browser. See the
[browser acceptance contract](../../../docs/architecture/v5/web_dogfood.md) and
[code editing review](../../../docs/architecture/v5/code_closer_review_2026_09_06.md).
The separate D0-macOS gate still requires native input, frame-budget and
lifecycle checks with the app foregrounded. This app is not a claim of Dune integration.

Links and images have toolbar dialogs. Click a link for Open, Edit and Remove,
use Cmd/Ctrl+K to insert/edit a link, or Cmd/Ctrl-click to open one. Shift+F10
opens keyboard-accessible link actions at the caret. Image previews keep their alt text editable;
click a preview to change its URL, alt text or title. Relative image URLs resolve
against the web workbench's origin. Loading failures retain a stable placeholder.

From this directory:

```sh
flutter pub get
flutter run -d macos --profile
```

The Draft and Tour documents and stress presets each retain their own edited
copy. Inspect Markdown shows the bounded source page around the caret; Copy Markdown exports the complete document. A normal
application exit waits for the latest queued save. If saving fails, the app
stays open and asks you to copy the Markdown. This prototype does not provide
crash-proof document storage or a file-management workflow.

The candidate settings are in `lib/qualification.dart`: 32 KiB UTF-8 live
source, 1,024 physical lines, 512 blocks, 2,048 runs, and 4,096 UTF-16 units per
line and leaf block, with at most eight nested quote/item containers. The writable source ceiling is 256 KiB. These values are
**candidates**, not a sealed performance promise. Dense presets can exceed the
shape budget and open in the disclosed source mode.

Build Flutter web with its packaged parser Wasm:

```sh
flutter build web --wasm
```

Run `python3 tool/serve_web.py` and open `http://127.0.0.1:8813/` in Codex's
embedded browser. Restart the server after each rebuild. It serves the normal
build with content-derived asset paths and no new service worker, so stale
cached code cannot hide a rebuild; the origin and saved drafts stay the same.
Run the real browser transport regression separately from the widget suite:

```sh
flutter test --platform chrome test/web_input_test.dart
```

It enables engine platform messages (suppressed by Flutter's browser unit-test
runner by default), then verifies DOM input, exact source, the next character
and actual paint. The normal Wasm workbench still receives interactive browser
canaries; the transport test alone is not D0-web.

The native profile correlates a required post-input paint with its engine raster
frame, with no settling before the proving paint. The full workbench path also
includes draft persistence and application UI:

```sh
caffeinate -dis flutter drive --profile -d macos \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/frame_profile_test.dart \
  --dart-define=FLARK_PROFILE_BOUNDED=true \
  --dart-define=FLARK_PROFILE_APP=true
```

Run it with the Mac unlocked and the app in the foreground. Reject a run with
failed foreground activation or a lock transition. The harness now requires
Flutter's resumed lifecycle and enabled frames, and rejects foreground loss. The bounded fixture fills
the byte envelope while retaining as much of each structural pattern as the
published count limit admits; the original unconstrained fixtures remain a
separate diagnostic. Prose, dense blocks, lists, tables, nested containers, unique references, and
Unicode are covered. Every shape is exercised near the start, inside its
largest laid-out block and at the end. Profile persistence uses a separate
`flark.profile.` preferences prefix and does not edit the workbench's drafts.

Run the complementary opening, boundary, save/close, memory and sustained-input
workload with the same driver and `--target=integration_test/workbench_profile_test.dart`.
It always uses the production workbench, keeps its drafts under
`flark.workbenchProfile.`, and requires a window wider than 650 logical pixels
so the inspection toggle actually reflows the editor. Its measured sequence runs
for at least five minutes after the repeated open/edit/close cycles. The
[macOS qualification contract](../../../docs/architecture/v5/macos_qualification.md)
predeclares numeric limits and the remaining native canaries. The September 5
foreground sweep completed and failed five largest-paragraph cases. The longer
workbench run lost foreground during sustained input and was rejected. See the
[native results](../../../docs/architecture/v5/native_profile_2026_09_05.md).
The subsequent [input-context correction](../../../docs/architecture/v5/input_context_review_2026_09_05.md)
has focused timing and browser evidence; the complete foreground rerun and
normal-app native canaries remain open.

Headless checks:

```sh
flutter analyze
flutter test
cd ..
flutter test
```

Functional, browser-smoke, native-input and performance evidence are separate.
See the repository's [D0 gate](../../../DOGFOOD_MILESTONE.md) and
[implementation review](../../../docs/architecture/v5/implementation_review_2026_09_04.md).
The [continuation record](../../../docs/architecture/v5/native_session_2026_09_04.md)
contains the subsequent browser dogfood fixes and current native blocker.
