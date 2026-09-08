# Priorities after Markdown theming

Reviewed 2026-09-08. The kernel, Flutter editor/viewer, Flutter web/Wasm path,
fourteen-language code service, link/image editing, bounded image previews and
Flutter theming playground are implemented. More palette options or more code
languages are not the current bottleneck. The remaining work is proving normal
application use and delivering the second host.

| Order | Focus | Concrete outcome and stopping point |
| --- | --- | --- |
| 1 | Close the Flutter input and performance evidence gaps | Run both foreground macOS profiles with the actual code service, then ordinary-app typing, clipboard, composition, focus and lifecycle canaries. Verify browser semantics input, then real assistive-technology behavior rather than treating a DOM regression as a VoiceOver/TalkBack pass. Resolve failures or explicitly narrow the supported envelope. |
| 2 | Put V5 into a real consumer | Integrate the Dune composer and message viewer using public APIs, real persistence, theme, link opening and image resolution. Start the planned two weeks of daily use; turn observed failures into source/caret/style/next-input regressions. Verify external URL opening in the consumer. |
| 3 | Build `flark_fleury` and its example (M4/T4) | Implement the real Fleury editor/viewer over the shared kernel, native cell styles, focus/input and overlays. Deliver a theming panel plus one live editor, visual or terminal-appropriate color controls, replaceable resource controls and copyable Dart configuration. Keep viewer parity in host tests. This may overlap consumer dogfooding once shared contracts are stable. |
| 4 | Qualify the advertised device/browser envelope | Exercise desktop Chrome and Safari, mobile Safari/older iPhone, and the floor Android device with IME, selection, image loading and code coloring. Publish byte/shape and source-mode limits from actual input-to-paint measurements. |
| 5 | Package and release | Prove clean native/web installation and binary distribution, update the stale Pages/release path and consumer docs, finish accessibility/mobile-selection decisions, and consume a tagged artifact in both named clients. Revisit CI with the owner before reinstating it; the present local-only exception is not a CI pass. |

The next implementation focus should be the first row, followed by real consumer
adoption. Fleury is the next substantial feature milestone. Do not reopen the
core architecture or add another parser/highlighter unless those concrete
consumer scenarios demonstrate a missing boundary.

The merge review's fix to the enabled/focused semantic text node closes the
observed web input defect only. It does not qualify screen-reader navigation,
dictation, device composition, native frames or lifecycle behavior. Existing
[milestone exits](build_plan.md) and [D0 platform gates](../../../DOGFOOD_MILESTONE.md)
remain authoritative.
