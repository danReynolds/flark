# Markdown theming and controls review

The later [landing review](theming_merge_review_2026_09_08.md) supersedes this
initial working-tree status and records the corrected web semantics input path.

Base: `46304ddd6522fa974376161a1af9072ecc900f28`, merged PR #42. The theme and
resource changes are local on `codex/v5-links-images`, not committed or merged.
This is an implementation self-review with local evidence, not an independent
peer review. CI is skipped at the owner's request.

## Playground refinement after owner feedback

The owner removed the side-by-side viewer requirement. The playground now
contains a theming panel and one live editor; narrow screens stack the panel
above it. The extra controller, document mirroring and preview tabs are gone.
Exported configurations also use one editor. Host viewer/theme parity remains
covered by the existing host tests.

Color swatches open an inline wheel with opacity control, alongside exact hex
entry. This uses FlexColorPicker 3.8 in the example only, compatible with the
installed Flutter SDK. The host and kernel gain no dependency. Picker, hex and
inherited preset values stay synchronized without resetting focus on each valid
hex keystroke. The original hex-only UI was a usability omission, not an API
limitation; exposing a setting is not enough to make experimentation easy.

The updated example passed analysis, all 31 example tests (including four
playground checks), both separately compiled single-editor exports, and the
release Wasm build. The gesture-based
color check verifies actual wheel input, hex synchronization (including alpha
and invalid input), retained source/caret and Undo after theme changes.
Follow-up logs and source hashes are under
`receipts/theming-2026-09-08/playground-refinement/`.

With the Mac unlocked, the updated embedded browser visibly showed one editor
beside the panel. The normal canvas journey passed wheel and opacity dragging,
hex input, dark/Notebook presets, typing after changing colors, Undo, branded
link popover and sheet editing, the next typed character, and Undo back to the
original source. Configuration view and clipboard contents were verified for
custom controls and then a different plain Dark preset, both containing one
editor and no viewer. No warning/error logs were observed. The sample was
restored through Undo and the browser left open in Dark mode with the picker.

One separate limitation remains reproducible in that session: after explicitly
pressing Flutter's **Enable accessibility** button, clicks/typing did not reach
the custom editor although ordinary form fields worked. The DOM hit target was
a full-viewport semantics node. Reloading without enabling that overlay restored
normal editor input. No cause is assigned or accessibility pass claimed; that
route needs a focused host/browser investigation. External-window opening and
native/device/performance gates also remain open. The original candidate
receipt below is retained as historical evidence, not a claim that its source
hashes include this refinement.

## Original candidate result and scope

Flutter T1 and T2 are implemented. `FlarkThemeData` resolves toolkit defaults,
ambient application overrides and editor/viewer overrides. Typed role maps
cover supported Markdown text, decorations, layout and Tree-sitter token
colors. The compatibility body style is applied last; specific role styles
still override it. Default links use the app primary color plus an underline.
The default body now inherits localized Material typography (16 pixels in the
default test theme) instead of forcing 17 pixels. Both surfaces use the current
text scaler.

Ordinary link clicks place the caret and show contextual actions. Modified
clicks and read-only links retain the application's opening callback. Dragging
does not open the popover. Shift+F10 opens keyboard-accessible actions; Escape,
editing, scrolling and stale target changes dismiss them.

Two presentation hooks are sufficient for the current default and branded
examples: `linkPopoverBuilder` receives guarded actions, and
`presentResourceEditor` receives a bounded editing session. The host retains
selection/revision guards, placement and focus restoration. Forms do not
reimplement source edits or Markdown recognition. Saving unchanged values
closes without normalizing reference syntax or adding undo history.

The Flutter example includes Light, Dark and Notebook presets, live controls
for every exposed role, ambient/per-instance themes, default/custom controls,
an editable mixed Markdown sample and a matching viewer. Separate Reset theme
and Reset sample actions preserve the distinction between appearance and
writing. Its Dart exporter includes public API wiring and the actual custom
control source; generated consumers compile and reproduce both theme scopes.
The playground is isolated from saved workbench drafts.

At this initial checkpoint, T3's implementation and automated checks were
complete, but its hands-on browser acceptance remained open: computer use
reported the Mac locked and could not unlock it. The refinement above records
the subsequent browser run. T4 remains scheduled with
the existing M4 Fleury host milestone. `flark_fleury` and its promised example
are not implemented; this is not completion of the two-host plan.

## Ownership and review findings

All new styling and control code stays in the Flutter host and example. There
is no new kernel style schema, Markdown parser, highlighter integration or
Fleury compatibility layer. Theme changes do not issue editing commands.
Resolved theme equality preserves normal paragraph reuse. Color changes
refresh text without revealing a scrolled-away caret; font/metric changes
invalidate geometry. Paragraph invalidation no longer disposes image streams,
so changing colors does not refetch loaded images. Metric validation rejects
invalid dimensions, and interpolation cannot introduce negative geometry.

Mounted testing exposed a popover transform read during incomplete layout.
The corrected overlay uses Flutter's layout-provided surface transform and
current glyph rectangles. Review also found two ordinary interaction gaps:
saving an unchanged reference link rewrote its Markdown, and Cmd+K opened a
resource form inside code while the toolbar was disabled. Both reproduced as
failing regressions before correction; they now preserve source/history and
reject the inappropriate command entry point respectively.

These are coverage obligations across presentation paths and entry points,
not unusual parser edge cases. Tests exercise source, caret, style and next
input through default and custom controls, stale requests, drag versus click,
keyboard action focus, first-frame geometry, image retention, color-only
scroll stability and narrow-screen example behavior. Built-in body, link and
syntax palette contrast is checked against actual resolved backgrounds. This
is not a complete accessibility or performance certification.

## Local evidence

The final pass after both review corrections completed successfully:

| Check | Result |
| --- | --- |
| Flutter host analysis | No issues |
| Flutter host tests | 843 passed |
| Example analysis | No issues |
| Example tests | 30 passed |
| Exported ambient/per-instance consumers | 2 passed |
| Chrome input, Tree-sitter worker and code-font tests | 26 passed |
| Normal Flutter web Wasm build | Passed |
| Normal macOS profile build | Passed, 108.6 MB |
| Flutter production Dart line budget | 4,003 / 10,000 |
| Diff whitespace check | Passed |

Raw compressed logs, reproducible commands, source hashes and build hashes
are in [receipts/theming-2026-09-08](receipts/theming-2026-09-08/).
Core/parser/Wasm sources still match the preceding resource receipt; its 685
kernel tests, 44 Rust tests and 1,322 native/Wasm comparisons were not repeated
for this host-only slice. The macOS build reports framework filename warnings
across architectures for the native parser/code assets; successful compilation
does not qualify distribution packaging or either architecture's runtime.

The dedicated local preview is `http://127.0.0.1:8818/?theme=1`; it uses fresh
content-derived asset paths. Serving the build does not count as inspecting
it. The final browser tour must still exercise presets and individual roles,
custom controls, link editing, the next input, undo, reset and configuration
copy on the release build. External-window navigation in the embedded browser
was not observable in the preceding resource canary and remains unverified.
Native foreground timing, OS input/clipboard/composition, device and lifecycle
gates remain open.
