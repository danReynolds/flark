# Flark v5 build plan

**Execution contract for [RFC 030](../rfc/rfc_030_synchronous_core.md).**
2026-09-02. One milestone is complete when every exit line has a receipt in
the repo: a test run, a number with commit and device, or a merged change.
Milestones are sequential except where noted. Line budgets are gates.

| Package | Budget (production lines) |
| --- | ---: |
| `native/flark_parse` (on top of comrak) | 3,000 |
| `flark` kernel | 8,000 |
| `flark_flutter` | 10,000 |
| `flark_fleury` | 3,000 |

Cross-cutting rules: no concept without a journey, a conformance case, or a
named consumer (Dune, Fleury); no performance claim without a receipt; no
test that settles before asserting on the edited frame.

## M0 — Spikes · done 2026-09-02

Sourcepos differential, end-to-end keystroke over FFI, marshal, Wasm under
dart2js, iPhone. All passed; results in RFC 030 §14 and `spikes/v5/`.
Fleury native packaging moved into M1.

## M1 — Parse crate · done 2026-09-02

`native/flark_parse` and `packages/flark` (parse transports) on branch
`v5/m1-parse-crate`. Receipts: `cargo test --release` (conformance 1,322/1,322
with zero deviations, invariants, fuzz), `tool/verify_transports.sh`
(native and wasm byte-identical, 1,322 cases), `packages/flark` `dart test`
through the build hook, and `tool/verify_prebuilt_consumer.sh` (a fresh
Dart app with Rust removed from PATH builds and parses via a prebuilt).
Native parse plus extraction at 25 KB dense: 1.05 ms.

- Render-model schema as a versioned file: field tables, kinds, attrs,
  invariants. Rust constants and the Dart decoder both derive from it.
- Complete the block vocabulary: per-line prefix ranges for quotes, items,
  footnotes; setext underline and ATX closing sequence as hidden lines;
  fence lines; table delimiter row and cell pipes; task marker range from
  comrak's `symbol_sourcepos`; thematic break; HTML block; link and image
  destination and title ranges.
- The three register corrections: partial tab expansion attr, escaped pipe
  mapping in table cells, inline shift after stripped reference definitions
  using the salvaged v2 scanner.
- Both transports: FFI through the salvaged `hook/build.dart`, wasm32
  through the salvaged build script. Prebuilt binaries fetched by the hook
  so a consumer without Rust builds, which is the deferred Fleury packaging
  spike.
- Fault containment: every panic becomes a typed error; a fuzz target proves
  arbitrary bytes never panic or read out of bounds.

Exit: 652 CommonMark and 672 GFM cases produce byte-identical render models
on FFI and Wasm; the differential reports zero unregistered deviations; a
scaffolded Dart app with no Rust toolchain builds and parses.

## M2 — Kernel

Pure Dart `flark`.

- `FlarkDocument`, typed-data model decoder, `FlarkProjection` with hidden
  ranges, offset map, and per-block memo.
- `FlarkCaret` anchor model with the navigation rules from RFC 030 §6.
- `FlarkCommand` closed set including formatting toggles, heading level,
  task toggle, indent and outdent, paste.
- Command semantics for every rule in `edit_profile_v1`, as range
  arithmetic over the model.
- History with v4's grouping rules.
- `FlarkEditor` facade with `typingContext`; parse backend interface with
  FFI, Wasm, and a test stub.
- Journey fixture format and runner; invariants asserted on every step;
  the four rapid sequences from the dogfood milestone as journeys.

Exit: every edit-profile rule has a journey; invariants hold on all
journeys; the boundary test proves no Flutter import; public exports at or
under fifteen; 25 KB dense keystroke under 1.5 ms on the M1 Pro through the
facade, receipt in repo.

## M3 — Flutter surface and macOS dogfood

- Prune the v4 surface: remove lineage, reconciliation, certification
  barrier, pager, generation counters; keep render surface, delta client,
  16 KB window, semantics.
- Real `Scrollable` over a multi-child layout; row protocol.
- Block rows: text, heading, code with background and highlighting, table
  with borders and cell navigation, image, list and quote shells, thematic
  break, task checkbox. Salvage v2 widgets, theme, popovers.
- Hide-model affordances: formatting shortcuts, typing-context exposure,
  link and image popovers, source mode above the tier with a notice.
- `FlarkMarkdownView` on the same rows.
- Actual-paint suite rewritten under the no-settle rule with single-frame
  budgets; rapid unpumped bursts on the input bridge.
- One attended session with a real phone keyboard and CJK composition.
- Dune migrated to `flark_flutter` for message view and composer.

Exit: dogfood milestone sections 1, 2, 3, and 5 pass on macOS with a
receipt; Dune runs on v5; two weeks of daily dogfood with every bug landing
as a journey before its fix.

## M4 — Fleury surface

- `flark_fleury` editor over Fleury's `TextInput` and controller; view.
- Cell rendering with box-drawing shells; Fleury table widget for tables.
- Terminal transport through the M1 hook; browser transport through the
  dart2js loader.
- A Fleury sample app and a test surface that runs the M2 journeys.

Exit: the kernel journeys pass on the Fleury test surface; browser and
terminal receipts; the sample app installs from a clean machine.

## M5 — Flutter web and the envelope

- dart2wasm build with the js_interop loader, asset bundling, and the demo
  site on v5.
- Parity run: render models byte-identical between native and the Flutter
  web build.
- Receipts on desktop Chrome, mobile Safari, and one mid-range Android
  device, which is the floor device the phone tier still lacks.

Exit: envelope limits published from receipts, not from the RFC's
provisional numbers.

## M6 — Hardening and release

- Mobile selection handles and magnifier, or an explicit deferral with the
  platform fallback documented.
- Accessibility pass with VoiceOver and TalkBack.
- Clipboard, dictation, and composition qualification on devices.
- Decisions from dogfood: fence auto-close, reveal-at-caret toggle.
- CI gates: conformance on both transports, journeys, actual paint,
  receipts, binary hosting release workflow.
- Repo: land on main, archive `codex/*` branches and stale worktrees, move
  v2 to `legacy/` once Dune is migrated, prune docs to the four active ones
  plus this plan, CHANGELOG and README.

Exit: `verify_release.sh` green; a tagged release consumed by Dune and
Fleury from the released artifact.

## Testing approach

Why every previous version reached dogfood with visible bugs, and the v5
answer to each:

| Failure in v2–v4 | v5 answer | Built in |
| --- | --- | --- |
| Suites settled before asserting and never saw the edited frame | Journeys assert the visible transcript per step: display text with style runs, caret visible position, typing context. No settle before an assertion anywhere | M2 (kernel), M3 (paint) |
| Coverage shaped by what the code handled; the v4 raw-frame receipt typed only inside existing bold | Generated journeys from a tracked matrix: every inline kind × boundary position × command, every block kind × Return and Backspace at start, middle, end; a user simulator that types delimiters, words, and spaces the way people do; invariants on every generated step; the matrix reported as a denominator | M2 |
| Real input deferred to the last milestone; v4 canaries never ran | Attended session with real macOS and iOS keyboards and CJK composition before dogfood, using v4's canary scripts; a fixed hundred-keystroke script that must be clean by hand | M3 |
| A dogfood bug needed a mounted, timed test to reproduce | Journey recorder in the editor: sessions capture source, commands, and transcript; a noticed bug becomes a fixture with one action and replays exactly headlessly | M3 |
| Budgets of 75–250 ms per step | Single-frame budgets with receipts naming commit, device, and display rate | M2, M3, M5 |

Invariants checked on every step of every journey, hand-written or
generated: display text contains no byte from a hidden range; display text
equals source minus hidden ranges plus replacements; the caret is never
inside a hidden range; offset mapping round-trips; undo restores exact
source and selection; the projection of the resulting source equals a
projection from scratch.

Dogfood readiness: the generated matrix is fully green, the hundred-
keystroke script is clean by hand on macOS and on a phone, and the paint
suite passes under the no-settle rule. Every dogfood bug lands as a
recorded journey before its fix; a bug that is a class adds a rule to the
generator.

## Later, only with a named consumer

Async tier for documents above the sync limit. Windows. Any collaborative
or provenance hook.

## Sizing

Order of magnitude, with agent-driven development at the cadence of the
last program: M1 about a week, M2 two, M3 three to four plus the two
dogfood weeks of calendar time, M4 one to two, M5 one, M6 two to three.
Roughly a quarter to a tagged release. The previous program withdrew its
own estimates as unsupported; treat these the same way until M1 and M2
have landed and set the real pace.
