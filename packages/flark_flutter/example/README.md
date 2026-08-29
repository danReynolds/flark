# Flark dogfood

This app is the first hands-on surface for the real Flark v4 path: Rust runtime,
`flark`, and the Flutter custom editor surface.

From the repository root, build the optimized native runtime and open the app:

```sh
./scripts/run_v4_dogfood.sh
```

The document menu contains:

- A compact product tour with GFM, Unicode, incomplete syntax, and wrapping.
- Ordinary prose at 1, 5, and 10 MiB.
- A 5 MiB giant-line stress document.
- A 1 MiB dense-block stress document.

Focus feedback on typing latency, scrolling, selections, clipboard actions,
undo/redo, source correctness, live Markdown projection, wrapping, and hit
testing. The **Feedback guide** button includes a copyable report template.

The status strip exposes document size, revision, pending edits, parser state,
input-window state, resync count, document-generation time, and engine-open
time. Mobile controls, final accessibility, themes, and visual polish are not
part of this first Mac dogfood pass.
