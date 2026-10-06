# Fleury theme playground

One live Markdown editor beside Fleury theme controls. Color pickers update
headings, links and snippet keywords; Copy theme Dart exports the current
configuration. Reset theme leaves editing state intact; Reset sample is undoable.
On narrow screens, Customize theme opens the controls in place of the editor.

Setup and commands: [package README](../README.md#develop-and-run).

`bin/main.dart` loads the native parser library. `web/main.dart` loads the
parser as Wasm and mounts a local Fleury DOM host. Both color and indent code
fences with `flark_codemirror`, in Dart, with no snippet library or worker.
Neither entry point imports Flutter. Documents are session-only; this is not a
persistent workbench.

Dogfooding: `dart run bin/main.dart --repro=repro.dart` records the session
and writes it on exit as a Dart test that replays it
(`package:flark/recorder.dart`).
