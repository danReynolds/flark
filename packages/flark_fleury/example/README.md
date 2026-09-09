# Fleury theme playground

One live Markdown editor beside Fleury theme controls. Color pickers update
headings, links and snippet keywords; Copy theme Dart exports the current
configuration. Reset theme leaves editing state intact; Reset sample is undoable.
On narrow screens, Customize theme opens the controls in place of the editor.

Setup and commands: [package README](../README.md#develop-and-run).

`bin/main.dart` loads native parser/snippet libraries. `web/main.dart` loads
the same engines as Wasm and mounts a local Fleury DOM host. Neither entry point
imports Flutter. Documents are session-only; this is not a persistent workbench.
