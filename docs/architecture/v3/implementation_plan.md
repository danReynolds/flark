# Flark v3 production implementation plan

**Status:** Active, updated 2026-08-01. The Dart-first package foundation,
packet-only native/Web M1.1 vertical, and first multi-block structural slice are
complete. Checkpoints A and B have engineering evidence ready for review.
Checkpoint C has authenticated exact-base role reuse and a real marker-free
selected-Paragraph, fenced-code, ATX-heading, Setext-heading, and indented-code
Flutter editing slice, plus parser-certified escaped-punctuation,
hard-line-break, character-reference, direct-inline-link/image, and
grammar-revision-9 full/collapsed/shortcut reference-link/image slices, an
atomic thematic-break Flutter editing slice, a
depth-one single-Paragraph BlockQuote slice with separately demanded schema-4
path/physical-line projection, marker-free Flutter rail/Enter/recertification,
and green native/Chrome parity and sidecar lifecycle. It also has a top-level
depth-one tight BulletList slice with exact variant-9 structure, the established
separately demanded schema-5 selected-item projection, marker-free item editing,
exact canonical source, and terminal-empty exit; its current-byte
native/Chrome managed gates and focused Chrome lab receipt are green. Admitted
local list edits now use checkpoint-free source-rope line rank/select to derive
base and target predecessor/changed/successor windows, parse only those bounded
windows, and publish `ExactBaseDelta`. The matching narrow top-level depth-one
tight `OrderedList` vertical is green through exact parsing, a 20,000-item
bounded local delta, distinct schema-7/payload-kind-6 selected-item geometry,
public Dart, managed Flutter, and focused Chrome.
Checkpoint C also has a
bounded current-revision inline cache and exact-clean packed block-page splice,
plus a green anchored top-level interior crop/splice vertical for Paragraph,
ATX-content, Setext-transition, Paragraph↔thematic-break, and fenced-code-body
edits bracketed by ordinary Paragraph checkpoints within 64 KiB. Ordinary checkpoints
strictly after the last
definition-bearing leaf now carry the exact frozen definition count and keep
length-changing local Paragraph edits on `ExactBaseDelta`. Definition-free
first- and final-Paragraph edits can now crop from BOF or to EOF through the
same delta path. The restart collection authenticates the exact top-level block
count rather than a loose segmented flag. The standalone 100,000-reference real
marker-free Flutter Web/Worker/Wasm product gate is green on rebuilt bytes:
seven zero-cadence platform deltas preserve one `EditableTextState` and
platform input client, exact canonical source, and final convergence. The
latest measured preceding-build standalone Chrome receipt is 4.2 ms maximum
synchronous callback and 7.6 ms total callback time. The preceding-build
combined small-widget→100,000-reference-widget sequential reopen gate was also
green after correcting the Web module-loader cache lifetime; that Chrome run
recorded a 5.1 ms maximum synchronous callback and 8.8 ms total callback time.
Those measurements come from bytes preceding grammar revision 6. Revisions 6
through 9 have not rerun the performance gate, and neither receipt is
floor-device `FrameTiming` or SLO proof. The revision-7 proof remains recorded
below; the revision-8 and current revision-9 addenda record only functional,
freshness, parity, ownership, and checkpoint evidence.
Checkpoint C is not passed. Edits to or restart
through definitions, definition-bearing BOF, unsupported or unanchored regions,
over-cap or lost-convergence crops, authenticated restart/convergence for quote
or indented-code edits, broader quote/list shapes and block grammar, inline
emphasis/code composition inside projected quotes, virtualized multi-block
materialization, 100 MiB scale closure, user decisions, the Linux AOT CI
receipt, floor-device evidence, and production hardening remain.

This plan implements the
[definitive architecture](architecture_summary.md).
[RFC 023](../rfc/rfc_023_incremental_live_markdown_engine.md)
contains the rationale, while the
[proof ledger](../../../tool/parser_research/ARCHITECTURE_PROOF_LEDGER.md)
controls executable evidence and reopen conditions. A mechanism-level test is
not a milestone until it crosses the production package, runtime, ownership,
and consumer seams named here.

## 1. Delivery rules

Build one production-shaped vertical at a time. Every milestone must:

- preserve the Dart-only `flark` -> optional `flark_flutter` dependency
  direction;
- use one exact Markdown grammar authority;
- run through the real session, byte protocol, worker ownership, publication,
  host, and query contracts appropriate to that milestone;
- keep caller-isolate and bridge work bounded;
- make parser/index/publication/reclamation work fuelled and cancellable;
- publish no mixed revision and retain no unbounded revision ancestry;
- fail unsupported grammar closed as source-backed `Unknown`; and
- add durable contract, correctness, resource, and product evidence.

Do not build a broad parser in isolation and integrate it later. Do not retain
v2 prediction or whole-document paths inside v3 to make an intermediate demo
look complete.

## 2. Package and repository shape

The existing repository root remains the published Dart engine so its native
hook, Rust workspace, Wasm build, and release infrastructure do not move without
product value:

```text
repo root / package: flark
  lib/                         pure Dart public API and implementation
  hook/                        native-assets build hook
  native/                      production Rust engine and FFI
  lib/assets/                  engine Wasm for package/test delivery
  test/                        package:test engine tests

packages/flark_flutter/
  lib/                         input, controller, viewport and presentation
  lib/assets/                  Flutter packaging/staging conveniences
  test/                        flutter_test and device/widget gates

example/
  imports flark_flutter and transitively exercises flark
```

Mechanical enforcement:

- root `pubspec.yaml` has no Flutter SDK dependency;
- root `lib` and hook code import neither Flutter nor `dart:ui`;
- `flark_flutter` depends on `flark`, never the reverse;
- the adapter imports a narrow supported SPI, not arbitrary engine internals;
- native and web runtime configuration below the adapter uses bytes/URI/platform
  loaders rather than Flutter asset APIs; and
- Dart-only and Flutter verification lanes resolve independently.

## 3. Current implementation boundary

The boundary and checkpoint tables preserve time-of-run wording from their
milestone receipts. Within those historical rows, “current,” “current-byte,”
and “current rebuilt-byte” refer to the recorded revision-6-or-earlier
artifact, not the current revision-9 bytes. The revision-7 through revision-9
addenda following the tables are the controlling current receipts.

| Area | Confirmed current state | Remaining gate |
| --- | --- | --- |
| Package split | Root `flark` is Dart-only; `flark_flutter` is the dependent adapter; root and adapter graphs/import guards are green. The normal v3 barrel is now an explicit document-facade/value allow-list, while host, session, source-certification, parser-binding, and attachment choreography live only in the unstable adapter SPI; a negative analyzer receipt proves those names are unavailable from the normal import. Exact root and Flutter publish tarballs resolve through an isolated hosted cache with no path dependencies/overrides; each package tree is verified and their Wasm buildinfo is identical, while external standalone Dart/JS, relocated macOS AOT, Flutter Web, and real Chrome Worker/Wasm receipts pass without any absolute checkout path | Exercise the implemented Linux AOT branch in Linux CI and keep the v3 root-kind taxonomy explicitly preview-only until Checkpoint C promotion and representative rendered multi-block coverage |
| Persistent source | Rust owns immutable Crop revisions, exact UTF-8/UTF-16 metrics, bounded cursors, atomic edit lineage, retirement, and fuelled reclamation; the exact-clean candidate consumes the resulting `CertifiedSource` capability through endpoint-owned parser scheduling and publication on native and Web. The Checkpoint B path splices persistent SourceFacts content with exact identity reuse. A separate private parser edit envelope is derived before SourceFacts page widening, composes adjacent retained edits, and drops to the wider/clean fallback for a distant edit | Retain the same authority and bounded-work invariants while extending restart/convergence beyond the definition-free interior/boundary and reference-frozen Paragraph-anchored at-most-64-KiB subsets, then carry them through 100 MiB lifecycle stress |
| Source certification | Schema 3 canonical `SourceFacts` pages and terminal proof cross Rust/Dart on native and Web; pages remain provisional and only exact completion installs current source-fact authority. Reusable incremental-base authority advances separately, only when the independent host commits the matching structural manifest. Rust and Dart now retain the same committed-vs-active split through zero-delay supersession; both public-runtime verticals publish every canonical page rather than a digest surrogate | Retain the two-phase authority chain through archive, recovery, and large-scale stress gates |
| Endpoint FSM | One global event credit, strict receipts, source flow, deferred close, recovery, and retired-frame drop are implemented; ordinary edits use source synchronization as their sole cancellation authority and never enqueue a separate `Supersede` | Keep every parser/publication/query family under the same one-command unseen-credit invariant |
| Native registry/FFI | Generation-checked registry and C ABI are compile/test green; stale handles cannot alias reused slots; 2,048 advertised fresh endpoints reserve another 2,048 resident slots so every admitted endpoint can recover create-before-revoke | Retain the capacity, generation-exhaustion, header, and packaging receipts in final combined gates |
| Native isolate | Finalizer ownership, detached ports, truthful `done`, fresh/recovery, immediate/credited close, terminal deferred-command invalidation, rapid-edit source coalescing, executor synchronization, large provisional certification, deterministic startup-timeout reclamation, and truthful OS support reporting are proven; the exact root publish archive builds a native CLI whose whole bundle relocates and runs from an unrelated directory with loader overrides unset and package input read-only on macOS arm64 | Add broader GC/unexpected-exit/capacity stress receipts and run the implemented archive-AOT branch on Linux CI |
| Public Dart session | `FlarkV3DocumentRuntime.open` owns source, binding derivation, the platform byte endpoint, bounded executor, independent host, semantic source/certified/structure revision watermarks, synchronous bounded structural query, small edit results, exact range/cold export access, recovery, and truthful close without pumping on native and Web. `initialReady` now guarantees exact-current structure rather than only source synchronization. A real browser receipt drives a terminal Rust parser fault through public `recover()`, endpoint replacement, exact multi-page reseed, currentness, query, and truthful close. Normal status/results expose no Worker generation, host revision, raw failure code, host receipt, or public `attach`. One shared public-only fixture now proves identical native/Web semantics, and the same facade executes from exact root/Flutter publish archives | Retain the surface, parity, archive, and recovery receipts in combined CI; defer final unversioned naming plus extensible rendered-node taxonomy until Checkpoint C |
| Parser | The private production crate owns a fuel-bounded exact-clean controller, complete competing-opener order, both leading-reference terminal outcomes, one segmented every-size lexical/reference path, resumable source-backed cooked reference values, typed fail-closed results, 10 MiB resource/cancellation receipts, executable Comrak 0.54 donor-drift guards, and a line-shape-independent 4 KiB aggregate parse quantum with fuel-partition and supersession receipts. Nonempty source is partitioned into exact byte/UTF-16 `Paragraph`, `Blank`, `DefinitionsOnly`, `FencedCode`, `IndentedCode`, `AtxHeading`, `SetextHeading`, `ThematicBreak`, depth-one single-Paragraph `BlockQuote`, narrow top-level depth-one tight `BulletList` or `OrderedList`, or typed `Unsupported` leaves; empty input has zero leaves. IndentedCode publishes exact four-column/BOM/line-count/projected-length/terminal-EOL summary facts through structured role variant 7; its at-most-8-KiB selected-leaf projection is separately demanded from the same segmented lexical authority, fuelled, cancellable, and reclaimable. BlockQuote records exact marked/lazy physical-line geometry and publishes its selected-leaf path and line payload only on separate demand; nested and multi-child quote forms remain typed `Unsupported`. BulletList admits homogeneous `-`, `+`, or `*` markers with exactly one Paragraph per nonempty item and an optional terminal empty item; loose, task, nested, mixed-marker, and multi-block BulletList forms remain typed `Unsupported`. OrderedList admits only a top-level depth-one tight list with one physical line and one Paragraph per nonempty item, an optional terminal empty item, homogeneous `.` or `)` delimiters, and 1–9 digit markers. It preserves zero padding, nonsequential literal ordinals, Unicode, and CRLF; loose, task, nested, multiline/lazy, mixed delimiter/type, container-wrapped, multi-block, and 10-digit-marker forms remain typed `Unsupported`. Their shared local-delta controller uses persistent source-rope line rank/select to parse only independent base and target predecessor/changed/successor windows, with no list checkpoint index or list-wide scan. Setext records exact H1/H2 underline geometry and publishes inline content that excludes only the terminal content-line EOL while retaining internal line endings as softbreaks. ThematicBreak records exact marker/count/indent/BOM/envelope/EOL facts and publishes no visible or projected source. In definition-free top-level documents, ordinary Paragraph checkpoints bracket interior Paragraph, ATX-content, Setext Paragraph↔H1↔H2, Paragraph↔thematic-break, and fenced-code-body edits; first- and final-Paragraph edits may instead use BOF-to-ordinary or ordinary-to-EOF boundary plans. The restart collection authenticates the exact top-level block count, from which segmented state is derived, and every plan validates that count. After an exact clean definition-bearing parse, every checkpoint at or before the last definition-bearing leaf is discarded and every survivor carries the exact frozen definition count. All admitted lanes cap the crop at 64 KiB and require exact ordinary Paragraph authority. Parser split/merge receipts prove blank-boundary topology changes adjust the exact count and relevant block ordinals by +2/-2. A crop that accepts a new tail definition or typed unsupported tail, begins at BOF in a definition-bearing document, exceeds the cap, or loses convergence fails clean; an over-4-KiB same-block Paragraph→Setext promotion also rejects stale restart authority and takes the exact-clean fallback | Extend authenticated restart/convergence to edits of or before definitions and the other fallback regions, including edits inside IndentedCode and BlockQuote; then broaden lists beyond the admitted tight bullet/ordered shapes, including loose, task, nested, multiline, mixed, container-wrapped, and multi-block forms, before HTML and tables. Broaden quote shapes, bracket hazards, definition-mutation semantics, and reference-media editing afterward. Unsupported constructs stay typed and source-backed until admitted |
| Publication/host/query | Certified parser records form ordered multi-page SourceFacts, Green, Projection, cooked References, and CleanEof roles. Structural leaves publish through a packed persistent measured block tree with at most 64 semantic entries per page; exact-clean and the admitted crop paths use its bounded persistent cut/splice path. Setext uses structured role variant 5 and the public Dart surface reports the generic Heading kind with exact Setext geometry. ThematicBreak uses structured role variant 6 with exact atomic facts and an empty zero-run projection. IndentedCode uses structured role variant 7 for its exact compact summary, followed only on demand by a viewport-schema-3 payload of canonical 20-byte per-line projection records; native and Chrome agree on both phases. BlockQuote uses structured role variant 8 and separately demanded viewport schema 4 for its selected path and canonical 20-byte physical-line payload; native and Chrome agree, and cancellation, root release, and zero-residency close are covered. BulletList uses structured role variant 9 for exact-clean list facts; its established selected-item vertical separately demands viewport schema 5 for the exact path, parser-authored editing inputs, canonical source projection, and canonical 28-byte per-item records. The compact successor demands one schema-6 item geometry record first and then inline facts for that parser-certified content range; rebuilt-Wasm/freshness is 2/2, public-runtime semantic parity is 1/1 on Chrome, and the managed compact BulletList batch is 3/3 on native Flutter and 3/3 on Chrome. OrderedList uses structured role variant 10 and a distinct viewport-schema-7/payload-kind-6 constant selected-item projection: 20 bytes of ordered metadata followed by the canonical 28-byte item record. It carries selected ordinal, canonical EOL, opening-marker span, and literal marker value; geometry is demanded before inline facts. The local-delta publication receipts stay fixed at four records/two packets and 262,149 SourceFacts bytes for 20,000 and 100,000 items, preserve clean-oracle equality, accept two consecutive deltas from the valid underfilled 109-checkpoint/three-page topology, and close to zero. The separate 20,000-item ordered-list receipt discovers and parses exactly three base and three target physical lines, matches the clean oracle, survives cancellation and a second sequential edit, accepts nine-digit markers, and fails closed on 10 digits, delimiter changes, boundary edits, or stale authority. A 4,096-block fixture transitions one middle block Paragraph→H1→H2→Paragraph; every phase uses `ParsingOrdinaryExact` -> `ExactBaseDelta`, transfers and replaces at most 64 records, preserves exact first/middle/last queries, and remains exact for the next revision. A separate 4,096-block fixture promotes and demotes one middle Paragraph as ThematicBreak through one bounded crop and a splice deleting/replacing at most 64 records. The production-spacing Paragraph proof crops 4,116 bytes / 168 lines in three parser transitions and publishes an `ExactBaseDelta` replacing 8 of 16,386 records. A 671,794-byte fixture with 4,096 Paragraphs around an ATX Heading and fence proves successive ATX-content and fence-body `ParsingOrdinaryExact` -> `ExactBaseDelta` edits on native and Chrome; removing the closer loses convergence and takes the clean fallback. A 2,048-definition / 2,048-Paragraph fixture proves a length-changing middle edit transfers at most 64 records through `ExactBaseDelta`, retains all References, preserves exact first/middle/last queries, and leaves the public runtime exact for a second revision. Separate 4,096-Paragraph definition-free boundary fixtures lengthen the first and final blocks, enter `ParsingOrdinaryExact`, publish `ExactBaseDelta` with at most 64 transferred and replacement records, preserve exact first/middle/last geometry, and stay exact through public revision 3. BOF reuses suffix checkpoints; EOF reuses prefix checkpoints and correctly mints zero fresh EOF checkpoints. A frozen definition prefix remains safe for the EOF route. A sibling hot-inline sidecar binds one exact leaf fence and revision; a bounded 128-leaf/2,048-fact-record Dart cache retains decoded current-revision facts after that singleton moves. Separate point and structural-range lanes are now implemented: the latter performs one seek plus a consecutive authenticated page walk, returns fixed `FLKVR001` records with an opaque structural-ACK-bound continuation, and defaults to 4,096 encoded bytes / 24 blocks / 25 pages / depth 16 / 320 nodes because persistent splices do not preserve page density. Dart consumes one bounded quantum per advance and caps a visible window at 256 blocks | Extend restart/convergence beyond the admitted definition-free interior/boundary and reference-frozen interior/EOF subsets, including edits inside IndentedCode and BlockQuote; replace the separate 128-page M1.1 role fanout with a multi-level directory for the 100 MiB gate |
| Web Worker/Wasm | An external classic Worker owns one Wasm parser endpoint; a separate main-context Wasm instance owns the independent host. Exact transferred endpoint packets, stable scalar exports, explicit asset URLs, strict-CSP startup, install/query, failure propagation, proof-based close, Flutter asset delivery, generic abandonment, source-first 32 KiB turns, 64-transition candidate microgrants, a four-millisecond target slice, a hard aggregate cap of 4,096 candidate transitions, terminal in-protocol parser fault/recover/reseed/query, native/Web semantic parity, the broad example/Checkpoint A lane, and exact publish-archive consumers are green with packet-only ABI-v2 asset version `6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964` and Wasm SHA-256 `6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4`. The prior full Web adapter CI receipt is dated; the current focused Chrome asset/reopen gate is green 3/3, including the public-runtime character-reference edit on the rebuilt bytes. Focused native/Chrome parity also includes indented-code, depth-one BlockQuote, narrow tight BulletList semantics, and the narrow schema-7 OrderedList selected-item vertical. BlockQuote demand/cancellation/close lifecycle is covered. Current-byte BulletList gates include 3/3 managed cases on both native and Chrome plus the 1/1 focused Chrome engine-lab checkpoint; the focused OrderedList Chrome gate covers paint-only `007)` and same-client CRLF continuation to `008)`. The public roughly 3.2 MB 100,000-Paragraph replacement completes in 20.994-26.977 ms native and 29.9-30.1 ms Chrome with foreground work below 8 ms. The latest measured standalone 100,000-reference real marker-free Flutter Web/Worker/Wasm receipt is from the preceding build: seven zero-cadence platform deltas reached exact source/final convergence with a 4.2 ms maximum synchronous callback and 7.6 ms total callback time. Grammar revision 6 has current functional/freshness/parity/checkpoint evidence, not a new timing receipt | Add physical JavaScript Worker crash/GC stress, evaluate the clamped-timer continuation as a later optimization seam, and calibrate slices with `FrameTiming` on named floor devices |
| Flutter product | The real v3 managed binding drives one stable `EditableText` and platform input client from exact source through parser-certified marker-free strong/emphasis/code/strikethrough/escaped-punctuation/hard-line-break projection. A production Worker/Wasm fixture moves that client among Paragraphs while the bounded Dart cache retains revisited current-revision facts. Fenced code retains its exact body-only island. ATX and Setext Headings use the generic Heading Dart API and render parser-authored typography without certified markers. Setext hides its underline, excludes exactly the terminal content-line EOL from inline content, retains internal softbreaks, and recertifies after live edits through real Flutter and the demo. Indented code hides parser-certified four-column prefixes, retains residual indentation and literal Markdown-looking content, uses code typography, maps Enter to a canonical line ending plus four-space continuation, and recertifies exact-current structure and projection without replacing the `EditableTextState` or platform client. The first BlockQuote slice hides parser-certified quote prefixes, paints a quote rail, maps Enter to canonical `> ` continuation, and recertifies exact-current structure/projection on the same client; inline emphasis/code composition inside that projected quote remains pending. The narrow BulletList slice hides the selected item's certified marker/prefix, paints its marker in the gutter, preserves exact canonical source, hands the same bounded island between items, uses parser-authored canonical Enter continuation and terminal-empty exit, removes only the exact authorized prefix on column-zero Backspace, and composes selected-content inline facts after compact schema-6 geometry. The managed compact batch is green 3/3 on native Flutter and 3/3 on Chrome. The narrow OrderedList slice consumes the distinct schema-7/payload-kind-6 constant selected-item projection, installs geometry before selected-content inline facts, omits the certified marker from editable text while painting it in the gutter, and preserves that marker in canonical source. The focused managed case keeps one `EditableTextState` and platform input client while Enter continues `007)` to `008)` with CRLF. ThematicBreak collapses the active island to the affinity-selected atom boundary, keeps the canonical marker line out of the empty `EditableText` projection, paints one semantic divider, and maps Backspace/Delete to whole-atom source deletion without replacing the `EditableTextState` or platform client. A mixed 4,096-Paragraph checkpoint moves the same `EditableTextState` and platform input client from marker-free ATX content to the literal fence body, edits both, and is green on native and Chrome. Display-space selection, IME composition, atomic delta batches, stale-base rejection, provisional topology maintenance, literal fallback, and exact-current recertification remain covered. The visible-block coordinator translates a layout source range into demand and advances at most one bounded range quantum per frame; the 4,096-reference Chrome checkpoint reaches exact before and after a marker-free edit. The standalone 100,000-reference product gate keeps marker-free editing on the same `EditableTextState` and platform input client through seven zero-cadence deltas and exact final convergence; its 4.2 ms workstation callback maximum and 7.6 ms seven-edit total are workstation receipts, not floor-device frame-tail results | Build virtualized multi-block layout and height indexes above the completed structural materializer; compose inline styles inside projected BlockQuotes; and run floor-device caret/selection/undo/paste/composition and input-to-paint `FrameTiming`/SLO gates before promotion |

Revision 7 extends the table's current implementation boundary as follows:

- the parser's one resumable bracket resolver admits direct inline links/images
  with pinned Comrak tail-cut differential checks, parser-cooked
  destination/title values, nested-style preservation, code shielding, and
  inner-link precedence; reference, collapsed, shortcut, and incomplete forms
  remain fail-closed;
- inline publication uses persistent bundle schema 5 (`IPB5`): child zero
  retains the fixed schema-2 fact tree and child one is the optional
  variable-width value tree. Host queries return facts plus authenticated
  `FLKIV001`, within independent 64 KiB fact/value ceilings, a 128 KiB combined
  selected-leaf ceiling, and a 256 KiB default viewport-page ceiling;
- the current mirrored Worker/Wasm asset is
  `a868f652dbdd5e5d-5f412bffe731e227-bba3dc0f34f51964`, with SHA-256
  `a868f652dbdd5e5d22431e4e5d5401ea5c46855e5b02a905077ade9a1adb55f7`.
  The revision-6 asset and every timing figure in the table remain historical
  receipts rather than current-byte measurements; and
- active direct links and image alt text are marker-free and non-actionable.
  Passive links activate only through the supplied callback; passive images
  render only through an explicit builder or safe labelled fallback, never
  implicit I/O. A nested link in image alt text cannot activate, while an image
  inside a surrounding link retains only the outer action.

During pending recertification, the last exact bounded passive pixels, render
objects, and geometry remain stable, and the same active `EditableText`/input
client keeps its mechanically updated projection. Hit testing, link actions,
and accessibility semantics fail closed until exact authority returns. This is
the stable-paint anti-flicker policy, not provisional parser authority.

The shared admitted tight-list local-delta lane deliberately adds no second
list-checkpoint index. It asks the persistent source rope for independent base
and target predecessor/changed/successor line windows, validates only those
bounded windows with the exact parser, and publishes their replacement through
the existing block-tree `ExactBaseDelta` path.

This table deliberately distinguishes an architecture decision from a completed
consumer vertical. In particular, native FFI success does not prove native
adapter reclamation; protocol bytes do not prove publication; and a selected
Web Worker boundary does not mean it exists.

The demanded selected-leaf inline resolver now authoritatively projects
emphasis, strong, inline code, and Comrak-compatible one- or two-tilde GFM
strikethrough. `*`, `_`, and `~` participate in one source-ordered,
fuel-bounded delimiter walk; the resolver preserves original source-run lengths
for the CommonMark mod-three rule and passes a deterministic 1,000-case Comrak
donor differential. Mismatched, unclosed, and over-two-tilde runs remain
literal, while code spans shield contained tildes. Projection kind 4 crosses
the persistent engine schema, independent host validation, native/Web wire,
public Dart decoder, and active/passive Flutter presentation under grammar
revision 2. Normal editing hides the certified tilde markers, paints
strikethrough, and combines that decoration with IME composition underlining.

Grammar revision 4 adds ASCII escaped punctuation as projection kind 7. The
projection hides the certified backslash, carries no semantic style, and edits
the source pair atomically. Public semantic parity is green 1/1 on native and
1/1 on Chrome; the managed Paragraph batch is 3/3 on both platforms; and the
passive-to-active handoff is 1/1 on both.

Grammar revision 5 adds `HardLineBreak` as projection kind 8. Recognition
remains exclusively in Rust: Dart decodes the parser certificate and plans
source edits from its geometry, while Flutter only presents that current
authority. The admitted odd-backslash and at-least-two-space forms cover their
complete marker plus physical EOL; content retains exact LF, CR, or CRLF and
the closer is collapsed. Marker-free display hides the certified marker and
normalizes the physical ending to one visible newline without changing
canonical source or export. Replacement/deletion expands to the complete atom,
and insertion is authorized only at its certified boundary. An unshielded
candidate followed by continuation indentation makes the whole inline leaf fail
closed.

Grammar revision 6 adds parser-certified `CharacterReference` as projection
kind 9 while retaining the existing schema-2 `IFO2`/`IFP2` family and canonical
20-byte fact record. Rust is the sole recognizer and emits the exact source
range plus its cooked one- or two-scalar value. Dart validates that payload and
mechanically builds marker-free replacement runs, source/display maps, and edit
plans; it does not scan for `&...;` or name entities. A URI autolink may carry
nested character-reference children. The same parser-authored values cook its
visible label and destination, replacing the earlier rejected direction of
decoding or rejecting entities separately in the autolink path. Partial
replacement consumes the complete source token, untouched cooked
prefixes/suffixes are re-emitted literally around insertions, and edit
endpoints inside a surrogate pair fail closed. Bracketed/direct links and
images plus broader bracket hazards remained pending at the revision-6
milestone; that entity vertical did not claim them or full CommonMark example
14.

Grammar revision 7 adds parser-certified `DirectLink` and `DirectImage` as
projection kinds 10 and 11. Rust alone resolves direct `(…)` tails and bracket
precedence, preserves nested inline facts, and cooks destination and optional
title values. The fixed schema-2 fact records retain exact source and label
geometry; an atomically paired persistent `IPB5` value root emits the
self-framing `FLKIV001` companion keyed by raw fact ordinal. Missing or orphan
values, corrupt cooked UTF-8, reference, collapsed, shortcut, or incomplete
link/image forms, and broader bracket hazards fail the whole inline leaf
closed. The fact and value lanes keep independent 64 KiB ceilings, the combined
selected-leaf query is bounded to 128 KiB, and the default public viewport page
is bounded to 256 KiB.

The older bounded Pulldown-based inline-publication helper remains wired to
legacy role-record derivation and deliberately reports strikethrough as
unsupported. It is not the demanded sidecar or viewport authority proven by
the product vertical above. Consolidating or retiring that duplicate authority
is the next inline architecture gate; new product grammar must not be
implemented independently in both paths.

The historical grammar-revision-6 full Rust workspace all-targets suite was
green. Its focused Dart core gate was green 60/60, including digest parity; the
Flutter active/source gate was green 17/17; the public-runtime
character-reference edit was green on native and Chrome; and the Chrome
asset/reopen gate was green 3/3. Those receipts cover parser cooking,
fixed-record engine/host validation, one- and two-scalar Dart projection,
nested URI-autolink cooking, source-token-atomic editing, and surrogate-safe
active editing. That revision-6 asset version was
`6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964`, with Wasm SHA-256
`6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4`.
Prior focused ordered-list native, public-Dart, managed-Flutter, and Chrome
gates remain historical evidence from the preceding bytes, including
paint-only `007)` and same-client CRLF continuation to `008)`. Those prior
gates include the
Setext, thematic-break, and first BlockQuote verticals, the 671,794-byte mixed
native/Chrome case,
the marker-free same-client Flutter checkpoints, and the shared public
next-revision reference-frozen, segmented-BOF, and segmented-EOF fixtures; they
are evidence for these bounded slices, not Checkpoint C completion.

The grammar-revision-7 proof remains the historical receipt for the dual-root
`IPB5`/`FLKIV001` publication and query path, strict Dart joining, marker-free
active presentation, safe passive direct-media presentation, and the
stable-paint pending policy. The virtualized-surface focused suite is green
10/10; the Chrome live checkpoint is green 2/2 for passive and active direct
link/image presentation, three recertified direct-media edits, exact hidden
values, the existing live autolink edit, one stable input client, and the
first-frame flicker regression. The byte-exact nonzero-value Dart wire-codec
gate is green 6/6, focused native and Web direct-media runtime gates are green
1/1 each, and the packaging/freshness gate is green 12/12. The Rust workspace
all-targets release build and Wasm rebuild are green. Root and Flutter Wasm
bytes and buildinfo are identical at asset version
`a868f652dbdd5e5d-5f412bffe731e227-bba3dc0f34f51964`, with Wasm SHA-256
`a868f652dbdd5e5d22431e4e5d5401ea5c46855e5b02a905077ade9a1adb55f7`.
These receipts do not reattribute the preceding-build timing figures, and no
new performance timing is claimed. The previously red active sidecar exposed a
Dart/Rust Begin-layout mismatch for nonzero link values; the shared Dart codec
now follows the Rust `u32 entry count`, `u32 encoded bytes`, `u64 storage-page
count` order and a distinct-value byte-offset fixture prevents zero-value tests
from hiding it again.

### Grammar revision 8 evidence addendum

Revision 8 adds only strict GFM bare URI, `www.`, and email autolinks.
Exact lowercase `http://`, `https://`, and `ftp://`, boundary-gated lowercase
`www.` with a dotted domain, and ASCII
`[A-Za-z0-9.+_-]+` local-part emails with dotted domains are admitted. URI and
`www.` recognition precede email recognition, and GFM examples 621–631 govern
terminal trimming. Markerless exact-source facts use the target recipes
`exactContent`, `httpPrefixedExactContent`, and `mailtoExactContent`.

The whole-leaf implementation remains bounded and resumable: a source-relative
range cursor reads at most 256 source bytes at a time, work is fuelled, and
classification has an 8 KiB token cap. Code, angle-autolink, direct
link/image, and bracket-context maps shield false candidates. Over-cap input,
unknown or unresolved bracket context, overlap, or invalid state fails the
whole leaf closed without partial facts. Explicit `mailto:`/`xmpp:`, uppercase
URI/`www.` prefixes, relaxed forms, reference/collapsed/shortcut links, and
the rest of CommonMark/GFM remain future admissions.

The presentation source also makes same-ordinal activation idempotent when the
exact ordinal is already active and no activation is pending. That closes the
path which could replace certified projected content with canonical source
despite unchanged authority and caret intent. Current gates are Rust
exact-clean 46/46, promotion audit 2/2, and engine 251/251.
`cargo test -p flark-parser` is green: 309 non-doc tests and one compile-fail
doctest passed, three manual scaling receipts were ignored by design, and zero
tests failed. The remaining gates are packaging 12/12, freshness 2/2, Dart
inline facts/projection 68/68, native sidecar end-to-end 7/7, Web Chrome
sidecar end-to-end 3/3, Flutter presentation/surface 24/24, example Chrome
checkpoint 3/3, and the focused exact bare-classifier large-paragraph gate 1/1.

Root and Flutter assets are byte-identical at version
`dfcce276df7954a9-714e23750091d226-bba3dc0f34f51964`. The 3,506,644-byte
Wasm has SHA-256
`dfcce276df7954a97a11f3faef4f93217adddba0d4b620db5e4942a8a2e4c930`;
the 33,195-byte Worker has SHA-256
`bba3dc0f34f51964fe55bf67363b75fdc68a1387ce28f1771529c44ad7493a60`.
No full-grammar, release, floor-device, or new performance-timing conclusion
follows from these receipts.

### Grammar revision 9 evidence addendum

Revision 9 adds parser-certified full, collapsed, and shortcut reference
links and images. Kinds 12 and 13 are additive in the existing `IFO2`/`IFP2`
and `FLKIN002`/`FLKIP002` fact family and reuse the authenticated `FLKIV001`
companion-value root, so those wire schemas do not advance. Grammar authority
does advance because a leaf's meaning can now depend on the document's first
winning normalized reference definition.

The exact reference root owns a fuelled, resumable winner index that is built
once for that root and retained across exact-base inline requests. Leaf parsing
receives only a root-bound resolver; it does not rescan the document or ask Dart
to classify Markdown. Resolved facts keep leaf-relative use-site geometry while
carrying document-absolute destination/title source cuts and parser-cooked
values from the winning definition.

The resolver capability is now cloneable without duplicating or transferring
winner-index ownership: the retained exact publication owns the indexed pages,
and each hot-inline or viewport-leaf job receives a cheap root-bound clone. A
completed leaf can therefore consume its job-local resolver without depriving a
later direct- or reference-media leaf of the same revision's authority.

Undefined or malformed uses remain literal, including CommonMark's explicit
tail replay rules. A real reference whose cooked value exceeds the bounded
companion lane, a reference-shaped use without authenticated resolver authority,
or an over-cap tail revokes the exhaustive bracket certificate and fails the
whole leaf closed. Revision 9 therefore expands definitive behavior without
introducing a prediction path or weakening bounded-resource admission.

The current real-Chrome checkpoint is green 3/3 on asset version
`c8d79f20ac3ffce4-76c8745528303a41-bba3dc0f34f51964`. Its Worker/Wasm case
materializes full, collapsed, and shortcut reference links plus a reference
image beside the existing direct media, verifies cooked destinations/titles,
marker-free passive semantics, and the labelled no-I/O image fallback, then
recertifies three direct-media label/alt edits on one input client. A separate
focused Chrome regression is green 1/1 for a length-changing direct-link label
edit before later reference definitions. These are functional and ownership
receipts, not a new performance measurement, general definition editing, or a
complete reference-link interaction claim.

### CommonMark coverage and recursive-container gate

The pinned CommonMark 0.31.2 corpus contains 652 examples. The executable
`test/fixtures/commonmark/v3_coverage_ledger.json` currently records 60
numbered authoritative semantic probes, 19 numbered intentional fail-closed
cases, two intentional GFM-profile divergences, and 571 examples with no v3
numbered conformance claim. The last group is not passing coverage. The ledger
test pins the corpus digest, assigns every example exactly once, and verifies
that every positive or fail-closed claim remains tied to live Rust test
anchors. Both the fast package-confidence gate and native editor CI select it
explicitly.

Grammar breadth now proceeds by falsifying the hardest structural assumption
before adding isolated rules. The first probe uses a multi-block list item with
a quote and fenced code (CommonMark example 321) and a nested loose-list
counterexample (example 325). It must preserve exact byte and UTF-16 source
ownership, work under arbitrary fuel partitions, and update a local edit in a
large surrounding document without scanning or rewriting unrelated siblings.

The narrow `M11CleanLeaf::{BlockQuote,BulletList,OrderedList}` variants remain
valid staged product slices, but they are not the full-container storage model.
Broader containers must lower into the already-selected source-ordered packed
Euler/serialized-green representation: generic Enter/Exit structure,
source-ordered coverage atoms, adjacent typed facts, and associative ancestor
summaries. A feature-specific nested-list/quote variant, document-wide block
directory, absolute-rank repair, substring parser, or second Markdown
classifier fails this gate even if its fixture renders correctly.

The existing `tool/parser_research/comrak_value_block_core` is the grammar
promotion source, not a discarded experiment: its current release suite is
green 183/183 and its scorecard remains 652/652 exact for CommonMark 0.31.2.
That suite also includes the full 1,322-fixture block-projection and every-line
restart lanes, direct recursive commands, refillable 10 MiB source lines, and
fuelled deep-container scheduling. Its Flark-owned line/finish control machine
and donor-proven transition ordering should be moved behind the current
refillable source and fuelled writer boundary after the container probe. Its
mutable vector tree, per-line `String` ownership, test renderer, and proof
materializer must not move with it. This separates the already-proven grammar
algorithm from the production resource, persistence, and publication work and
avoids rebuilding CommonMark feature by feature in `M11CleanLeaf`.

## 4. Immediate execution order

The current shortest path to decisive product evidence is:

1. **Completed — consolidate grammar revision 9.** Keep the rebuilt native and
   Wasm artifacts, reference-resolver ownership fix, package/freshness checks,
   real Chrome checkpoint, and focused native/Web reference receipts green.
2. **Completed — install an honest CommonMark ledger.** Keep all 652 fixtures
   classified exactly once and refuse to convert inventory or safe fallback
   into a conformance percentage.
3. **In progress — compose the hardest container slice.** Stream CommonMark
   321 into generic serialized green, keep 325 as the nested/loose discriminator,
   and prove fuel invariance, exact source ownership, and local mutation in a
   large surrounding document.
4. **Promote the proven grammar control.** Move the 652-exact value-block
   line/finish machine onto the current refillable source, fuelled worker, and
   candidate-writer APIs without its vector tree, copied source strings, or
   test renderer.
5. **Join incrementality and publication.** Prove exact restart/convergence,
   changed-prefix plus retained-suffix composition, atomic multi-root adoption,
   cancellation, and reclamation through the production native and Wasm host.
6. **Close product grammar breadth.** Drive the official ledger from the real
   v3 runtime, expose complete typed facts through Dart and marker-free Flutter,
   and add coarse milestone liveness gates rather than per-rule review pauses.
7. **Finish policy and launch gates.** Resolve raw/inline HTML, separate strict
   CommonMark from optional GFM extensions, then complete device-frame, IME,
   accessibility, packaging, and no-skip release evidence.

### Prior checkpoint ledger

The detailed checkpoint history below remains evidence for mechanisms already
built; it no longer defines the order of new work:

1. **Review Checkpoint A — responsiveness diagnosis.** The native/browser lab
   and packet-only ABI-v2 combined native/Web gates are green. Review the
   responsive caller behavior plus the public roughly 3.2 MB
   100,000-Paragraph replacement: 20.994-26.977 ms native, 29.9-30.1 ms Chrome,
   and below 8 ms foreground. No user review or approval has occurred yet.
2. **Completed: close external-consumer and release gates.** Exact publish
   tarballs now pass isolated hosted resolution, package-tree and Wasm
   buildinfo identity, external Dart/JS execution, relocated macOS AOT, Flutter
   Web build, and real Chrome Worker/Wasm open/edit/query/close outside the
   repository, with no absolute checkout path. Exercise the implemented AOT
   branch on Linux in CI rather than treating macOS as Linux proof.
3. **Review Checkpoint B — persistent SourceFacts identity reuse.** The
   production-path proof now covers prefix, middle, tail, Unicode, and
   split-CRLF edits; clean equality; unchanged page identity; bounded splice
   work; false-lineage rejection; cancellation/fallback; reclamation; and
   native/Wasm parity. Record the user decision before treating the storage
   model as promoted.
4. **Complete Checkpoint C — role-root delta and live editor integration.**
   Authenticated exact-base SourceFacts splice, unchanged References reuse,
   fresh target wrappers, stale-base rejection, and the first real
   marker-free `flark_flutter` editing slice are implemented. The first
   multi-block structural cut is also green through parser, shared-root
   publication, independent-host point query, native/Chrome public parity, and
   exact Flutter leaf handoff. A demanded nonzero Paragraph now receives a
   revision-bound inline sidecar, renders marker-free, and moves among first,
   middle, and tail leaves on the same input client. Fenced-code breadth is
   also visible through the production native/Web and Flutter seams. The bounded
   current-revision inline cache, exact-clean packed block-page splice, and ATX
   and Setext Heading verticals through native/Chrome, real Flutter, and the demo
   are now complete. Revision 7 also carries direct link/image geometry and
   parser-cooked values through the atomic `IPB5`/`FLKIV001` pair. The current
   product checkpoint proves marker-free passive and active direct media, three
   recertified label/alt edits with exact hidden values, safe image fallback,
   passive semantics/action, and one stable input client on real Worker/Wasm.
   The atomic ThematicBreak
   vertical is also green through
   structured role variant 6, native/Chrome public parity, managed Flutter, and
   the demo. The IndentedCode vertical is green through exact variant-7
   structure, separately demanded schema-3 per-line projection, native/Chrome
   runtime parity, managed Flutter Enter/recertification, cancellation and
   reclaim, and the new demo seed. Setext uses structured role variant 5 behind
   the generic
   Heading Dart API; its inline range omits only the terminal content-line EOL
   and retains internal softbreaks.
   The first depth-one single-Paragraph BlockQuote vertical is green through
   exact-clean structure, separately demanded schema-4 path/physical-line
   projection, native/Chrome parity and lifecycle, and marker-free managed
   Flutter rail/Enter/recertification. Nested or multi-child quotes remain
   typed unsupported; inline emphasis/code composition inside the quote and
   authenticated restart/convergence for quote edits remain pending.
   The first top-level depth-one tight BulletList vertical is also complete
   through exact-clean structured role variant 9, the established separately
   demanded viewport-schema-5 selected-item projection, and marker-free managed
   Flutter item editing with exact canonical source, item handoff, canonical
   Enter continuation, terminal-empty exit, and exact prefix removal. Loose,
   task, nested, mixed-marker, and multi-block BulletList forms remain typed
   unsupported. Current-byte schema-5 native and Chrome managed gates, the
   focused Chrome engine-lab checkpoint, and visual release-demo inspection are
   green. The checkpoint-free source-rope local-delta path now parses only the
   base and target predecessor/changed/successor windows and publishes
   `ExactBaseDelta`. Compact schema-6 selected-item geometry followed by an
   exact inline demand is green through rebuilt-Wasm/freshness 2/2, Chrome
   public-runtime semantic parity 1/1, and managed compact BulletList batches
   3/3 on native Flutter and 3/3 on Chrome.
   The narrow OrderedList sibling is complete for top-level depth-one tight
   lists with one physical line per item, homogeneous `.` or `)` delimiters,
   1–9 digit markers, zero padding, nonsequential ordinals, Unicode, CRLF, and
   an optional terminal empty item. It publishes structured role variant 10;
   the host then demands one constant schema-7/payload-kind-6 selected-item
   geometry projection before selected-content inline facts. Its 20,000-item
   local-delta receipt parses only three base and three target physical lines
   and matches the clean oracle. Native, public-Dart, managed-Flutter, and
   focused-Chrome receipts cover paint-only marker hiding, exact source, and
   same-client `007)`→`008)` continuation. Loose, task, nested,
   multiline/lazy, mixed delimiter/type, container-wrapped, multi-block, and
   10-digit-marker forms remain typed unsupported.
   In definition-free top-level documents,
   Paragraph checkpoints now bracket bounded interior Paragraph, ATX-content,
   Setext Paragraph↔H1↔H2, Paragraph↔thematic-break, and fence-body edits
   through the same `ParsingOrdinaryExact` and `ExactBaseDelta` splice path. A
   4,096-block thematic receipt keeps promotion and demotion in one bounded
   crop with at most 64 deleted/replacement records. A 4,096-block Setext
   receipt keeps every local transition within 64 transferred and replacement
   records while preserving exact first/middle/last queries. An over-4-KiB
   same-block Paragraph→Setext promotion rejects stale restart authority and
   completes through exact-clean fallback. The
   671,794-byte public fixture is green on native and Chrome, and the
   marker-free mixed Flutter checkpoint retains one input client across both
   Structured leaves. Definition-bearing documents now retain the same ordinary
   crop lane only strictly after the last definition-bearing leaf: every
   surviving checkpoint carries the exact frozen count. The
   2,048-definition / 2,048-Paragraph fixture applies a length-changing middle
   edit through `ExactBaseDelta`, transfers at most 64 records while retaining
   all References, proves exact first/middle/last queries, and completes a
   second public-runtime revision. The 4,096-Paragraph definition-free BOF
   fixture lengthens the first block, enters `ParsingOrdinaryExact`, publishes
   `ExactBaseDelta` with at most 64 transferred and replacement records,
   preserves exact first/middle/last geometry, reuses suffix checkpoints, and
   stays exact through public revision 3. Its final-block EOF twin enters the
   same route with the same 64-record bounds, retains prefix checkpoints,
   correctly mints zero fresh checkpoints beyond EOF, preserves exact
   first/middle/last geometry, and stays exact through public revision 3.
   Restart collections now authenticate the exact top-level block count;
   parser split/merge receipts change it and the relevant ordinal by exactly
   +2/-2. A frozen definition prefix remains safe. Generic edits to or before
   definitions, definition-bearing BOF, a new tail definition or typed
   unsupported tail, over-cap crops, and lost convergence still rebuild
   definitively; this does not implement incremental definition mutation or
   complete reference-link interaction/editing. Restart/convergence for edits inside an indented-code
   block is likewise not proven. The distinct standalone 100,000-reference real
   marker-free Flutter Web/Worker/Wasm product gate is now green: seven
   zero-cadence platform deltas retain one `EditableTextState` and platform
   input client, exact canonical source, and final convergence. Its rebuilt-byte
   workstation maximum synchronous callback is 4.2 ms and total callback time
   is 7.6 ms, not floor-device `FrameTiming` or SLO proof. Next extend the same
   authority through the named fallback regions and broader block grammar,
   build virtualized multi-block layout above the implemented bounded
   structural-range materializer, and close 100 MiB and floor-device gates.
   The selected producer, parser, wire, and host contract remains the
   [Checkpoint C exact-base delta design](checkpoint_c_exact_base_delta.md).

This order front-loads the remaining architectural seams. It avoids spending
months on grammar breadth before knowing that revision truth, publication,
queries, and lifecycle work end to end.

The parser extraction, independent Rust ownership vertical, Dart public facade,
bounded host-query ABI, packet-only rebuilt assets, Checkpoint A
scheduling/lifecycle gates, and Checkpoint B persistent-splice proof are
implemented. Checkpoints A and B await explicit user decisions. Checkpoint C is
now reviewable as a narrow live-rendered slice, not as a passed architecture or
launch gate; no product-facing promotion follows without its remaining named
evidence and user decision.

## 5. Product feedback checkpoints

These are user-review stops, not presentations assembled after the system is
finished. Each checkpoint must be runnable, instrumented, explicit about what
is still synthetic or unsupported, and cheap enough to revise after feedback.
A checkpoint cannot close an architecture milestone, but negative product
feedback can reopen the relevant public contract before the next checkpoint.
Backend work may continue while feedback is pending; crossing the next named
product/API boundary requires the preceding checkpoint's decision record.

No checkpoint may use the v2 prediction parser or a second Markdown grammar to
make v3 appear more complete.

| Checkpoint and status | Concrete demo | Evidence supplied for review | Approval question | Deliberately not claimed |
| --- | --- | --- | --- | --- |
| **A — Responsiveness diagnosis. Evidence ready; user review pending.** | Run the release native/browser engine lab on small, 1 MiB, 10 MiB, and the public roughly 3.2 MB 100,000-Paragraph fixture; type, supersede an active candidate, query currentness, and close. Show its below-8 ms foreground work and 20.994-26.977 ms native / 29.9-30.1 ms Chrome structural replacement. | Source/certified/structure revision traces; foreground `apply`, heartbeat-gap, convergence, latest-wins, cancellation, and close receipts; exact native/Web gate commands and rebuilt asset identity. | Is the off-caller scheduling/lifecycle model acceptable, and does the supported narrow Paragraph path feel live? Record requested observability or interaction changes before Checkpoint C. | Broad-grammar incrementality, complete parser-to-paint, full CommonMark/GFM, or launch readiness. |
| **B — Persistent SourceFacts identity-reuse edit. Evidence ready; review pending.** | Through the production source/lineage/storage path, apply prefix, middle, tail, Unicode, and split-CRLF edits to a multi-page fixture. Visualize the persistent SourceFacts tree before and after each edit, including unchanged page identities and the bounded changed path. | The fixed native/Wasm proof covers clean-vs-incremental page bytes, summaries, root fingerprint, absolute-coordinate equality, retained identities outside the changed range, bounded planning/splice work, false-lineage rejection, cancellation/fallback, and zero-residency close. Its parity digest excludes process-local identities and agrees across platforms. | Does the evidence make direct persistent reuse understandable and credible enough to authorize role-root/publication integration? If not, revise the storage model before UI work depends on it. | Role-root delta, reference-role reuse, visible Markdown rendering, editor feel, or M1.2 acceptance. |
| **C — Role-root delta and live editor integration. Partial vertical evidence; reviewable, not passed.** | Edit selected Paragraphs, the fenced-code body, the ATX and Setext Headings, the indented-code seed, the depth-one single-Paragraph BlockQuote seed, the top-level tight BulletList and narrow OrderedList selected-item seeds, the atomic thematic-break seed, and the hard-line-break Paragraph seed while canonical instrumentation retains every hidden delimiter, indentation prefix, quote/list prefix, and CRLF byte. Run the standalone 100,000-reference real marker-free Flutter Web/Worker/Wasm gate across seven zero-cadence deltas. Revisit prior Paragraphs and list items to exercise bounded handoff. Inspect the separate public-runtime Setext/thematic/BlockQuote receipts, reference-frozen, segmented-boundary, and BulletList locality receipts; they are structural evidence, not a complete reference-link UI or a broad editor demo. | Exact-base reuse, packed block-page splice, and the definition-free Paragraph-anchored interior crop cross parser, publication, independent host, public Dart, native/Chrome parity, real Flutter, and the runnable demo. The current grammar-revision-9 Chrome checkpoint is green 3/3 for passive full/collapsed/shortcut reference links and a reference image alongside direct media, with exact cooked values, semantics, and a no-I/O image fallback; the focused later-definition direct-media recertification regression is green 1/1. IndentedCode publishes exact variant-7 structure, separately demands an at-most-8-KiB viewport-schema-3 payload of canonical 20-byte physical-line records, agrees across native and Chrome, and stays marker-free through managed Flutter Enter and exact-current recertification on the same client; in-flight cancellation, root release, and zero-residency close are covered. BlockQuote publishes exact variant-8 structure, separately demands schema-4 selected-path and canonical 20-byte physical-line payload, agrees across native and Chrome, and stays marker-free through managed Flutter rail, canonical Enter continuation, and exact-current recertification; sidecar cancellation/release/close is covered. BulletList publishes exact variant-9 structure and the established vertical separately demands schema-5 selected-path, editing inputs, canonical source projection, and canonical 28-byte item records. Current-byte schema-5 native and Chrome managed gates cover marker-free selected-item display, exact source, item handoff, canonical Enter continuation, terminal-empty exit, and exact prefix removal; the focused Chrome Worker/Wasm checkpoint and visual release demo are green. The checkpoint-free source-rope local-delta path derives only base and target predecessor/changed/successor windows. At both 20,000 and 100,000 items, target local parse is 295 transitions and stream is 20; build is 18/21, publication is four records in two packets after 262,149 SourceFacts bytes, output equals the clean oracle, lifecycle authority is restored, two consecutive `ExactBaseDelta` edits work from the underfilled 109-checkpoint/three-page base topology, and close reaches zero. Compact schema-6 selected-item geometry followed by a separate inline demand is green through rebuilt-Wasm/freshness 2/2, public-runtime semantic parity 1/1 on Chrome, and the managed compact BulletList batch 3/3 on native Flutter and 3/3 on Chrome. The first combined Chrome run had one transient timeout; its immediate isolated rerun and full rerun passed, so these receipts do not define a deterministic performance budget. The narrow OrderedList sibling supports only top-level depth-one tight one-physical-line items with homogeneous `.`/`)` delimiters and 1–9 digit markers, preserving zero padding, nonsequential ordinals, Unicode, CRLF, and a terminal empty item. Its 20,000-item local delta parses only three base and three target lines and matches the clean oracle. Structured role variant 10 is queried through a distinct constant viewport-schema-7/payload-kind-6 selected-item projection, then a separate inline demand. Native, public-Dart, managed-Flutter, and focused-Chrome receipts cover paint-only marker hiding, exact canonical source, and same-client `007)`→`008)` continuation. Grammar revision 5 hard-line-break fact kind 8 keeps recognition in Rust, hides only the certified marker, retains exact LF/CR/CRLF source, and makes replacement or deletion atomic; an unshielded indented continuation fails the whole inline leaf closed. Setext uses structured role variant 5 behind the generic Heading API, retains internal softbreaks while excluding only the terminal content-line EOL, and proves local Paragraph↔H1↔H2 transitions among 4,096 blocks with at most 64 transferred/replacement records per revision; an over-4-KiB promotion falls back cleanly. ThematicBreak uses structured role variant 6 with exact atomic facts and an empty projection; its 4,096-block Paragraph↔thematic transition remains bounded, while Flutter preserves affinity, paints a semantic divider, and deletes the whole atom with Backspace/Delete on the same client. The 671,794-byte public fixture proves local ATX-content and fence-body revisions; the mixed Flutter fixture proves marker-free display and same-client handoff. The 2,048-definition / 2,048-Paragraph fixture proves a length-changing local `ExactBaseDelta`, at most 64 transferred records, all References retained, exact first/middle/last queries, and a second exact public revision. The 4,096-Paragraph BOF/EOF fixtures prove `ParsingOrdinaryExact` -> `ExactBaseDelta`, at most 64 transferred/replacement records, exact first/middle/last geometry, complementary suffix/prefix checkpoint reuse, zero correctly fresh EOF checkpoints, and public revision 3; parser split/merge receipts prove exact +2/-2 topology. The standalone 100,000-reference product receipt preserves one `EditableTextState` and platform input client, exact source, and final convergence; on the current rebuilt-byte workstation its maximum synchronous callback is 4.2 ms and total callback time is 7.6 ms. The combined small-widget→100,000-reference-widget sequential reopen gate is green after the Web module-loader cache-lifetime correction; its Chrome receipt is 5.1 ms maximum synchronous callback and 8.8 ms total callback time, separate from the standalone receipt. | Does the current checkpoint feel instantaneous, fluid, and trustworthy, and does the role-root delta remain architecturally clean? | Edits to or restart through definitions, definition-bearing BOF, unsupported/unanchored regions, new tail definitions/unsupported constructs, over-cap or lost-convergence crops, restart/convergence for edits inside indented-code or BlockQuote blocks, or broader grammar; loose, task, nested, multiline/lazy, mixed delimiter/type, container-wrapped, multi-block, and 10-digit-marker list forms; nested/multi-child BlockQuotes, HTML, tables, complete reference-link interaction and active reference-media editing, inline emphasis/code composition inside projected quotes; a complete virtualized multi-block editor; full CommonMark/GFM; 100 MiB viewport closure; floor-device `FrameTiming`/SLO proof; launch accessibility; or release readiness. |

The Checkpoint C demo now also includes named, two-scalar, and URI-autolink
character references. Grammar revision 6 publishes these as kind 9 through the
unchanged 20-byte `IFO2`/`IFP2` fact record. Rust remains the only recognizer;
Dart mechanically projects the cooked scalars and URI destination from
certified facts, and Flutter edits the complete source token with scalar-safe
UTF-16 boundaries. The historical revision-6 Rust, 60/60 Dart, 17/17 Flutter,
native/Chrome public-runtime, and 3/3 Chrome asset/reopen receipts prove that
narrow vertical only.

The Checkpoint C document now also contains direct
`[label](destination "title")` links and
`![alt](destination "title")` images. Grammar revision 7 publishes kinds 10 and
11 through the existing fixed fact records and joins parser-cooked values from
`FLKIV001`. Active presentation is marker-free and non-actionable; passive
links are actionable through the supplied callback, while images remain an
explicit consumer-resolver or safe labelled-fallback decision. The current
Chrome checkpoint proves passive direct-media presentation, the existing live
autolink edit, and first-frame stable paint. Revision 9 adds marker-free passive
full, collapsed, and shortcut reference links plus a reference image resolved
from the document's first winning definitions. The same 3/3 checkpoint proves
their cooked destinations/titles, semantics, and labelled no-I/O image fallback,
then proves whole-label and final-boundary direct-link-label edits, direct
image-alt editing, exact hidden values, same-client recertification, and the
updated passive direct action/semantics. Incomplete forms, broader bracket
hazards, active reference-media editing, general definition mutation, and full
CommonMark/GFM remain outside the vertical.
The 4.2/7.6 ms standalone and 5.1/8.8 ms combined figures in the table are
preceding-build receipts; grammar revisions 6 through 9 have not rerun that
performance gate.

For BulletList, the proven local path is now checkpoint-free. Persistent
source-rope rank/select derives the base and target
predecessor/changed/successor line windows without traversing the list, and the
exact parser publishes their replacement through `ExactBaseDelta`. The
20,000- and 100,000-item receipts both spend 295 target local-parse transitions;
build spends 18/21, stream spends 20, and publication stays at four records in
two packets after examining 262,149 SourceFacts bytes. Exact clean-oracle
parity, the restored local-edit lifecycle, two consecutive deltas from an
underfilled 109-checkpoint/three-page base topology, and close-to-zero are
covered.

Later large-document interaction, learned-behavior, and release-candidate reviews
remain required during viewport/grammar expansion and M6. They do not replace
or weaken Checkpoints A-C.

Each review produces a short decision record: observed behavior, accepted
trade-offs, required changes, and whether the next checkpoint may proceed.
Automated latency and ownership receipts remain beside the demo so subjective
smoothness cannot hide a large-document or lifecycle regression.

## 6. Milestones

### M0 — Dart-first foundation

**Status:** Complete.

**Outcome:** `flark` is the Flutter-independent engine package and
`flark_flutter` is a dependent adapter.

Closed gates:

- root Dart analysis/tests run without a Flutter SDK dependency;
- import guards enforce the core boundary;
- adapter and example resolve independently and receive the native asset
  transitively;
- runtime-neutral Wasm bytes/URI loading exists below the adapter; and
- root and adapter package contents have separate archive checks.

M0 closes dependency direction, not the v3 public API. `flark.dart` remains the
supported v2 surface during migration until the later promotion gate.

### M1.0 — Source authority, endpoint, and native runtime substrate

**Status:** Native source/runtime vertical and deterministic capacity/lifecycle
substrate green; final combined hardening remains.

**Outcome:** A Dart source revision reaches one persistent Rust endpoint,
derives bounded canonical facts, promotes only on an exact terminal proof,
recovers without cross-generation receipts, and closes with truthful
reclamation.

Implemented substrate:

- a parser-independent Rust document runtime with immutable Crop source roots,
  checked atomic multi-operation edits, bounded segmented cursors, retirement
  backpressure, arena journals, and fuelled reclamation;
- `Send + !Sync` exclusive document ownership that may migrate sequentially
  across host threads;
- a common bounded little-endian frame envelope and strict schema-3 session
  wire;
- explicit fresh/recovery open, opened gating, source seed/edit/supersede,
  source facts, event receipt, failure, and close/drain transitions;
- canonical 64-checkpoint `SourceFacts` pages invariant to poll cuts and one
  exact terminal certification proof;
- Dart staging that cannot promote on pages, echoes, dimensions, or fingerprint
  alone, and atomically promotes source plus the Dart host-source view on
  accepted exact completion;
- one globally credited event and exact binding/correlation checks;
- close deferred across one credited event, followed by a distinct bounded
  close-latch action;
- validation and no-receipt drop of already queued retired-generation frames;
- generation-checked Rust endpoint registry and C ABI;
- a 4,096-slot hard resident ceiling with fresh admission limited to 2,048,
  reserving simultaneous create-before-revoke recovery capacity for every
  admitted endpoint;
- a long-lived native-isolate byte endpoint plus automatic bounded Dart session
  executor;
- detached caller-port cleanup, generation-checked native finalizer tokens,
  truthful endpoint disposal completion, and unexpected-exit failure semantics;
- an early native-isolate control handshake that forbids handle creation before
  initialization is authorized, and startup abandon that reports success only
  after a truthful disposal receipt;
- exact close behavior before Opened, during source certification, and while a
  source command is backpressured behind event credit; and
- one-command ordinary-edit pacing: Dart revokes stale host staging locally,
  coalesces edits behind one live source lease, and sends only the next source
  synchronization, whose accepted install cancels older derived work; and
- a Dart-first managed runtime with one-shot `initialReady`, revision-current
  status, typed protocol-failure readiness, close-intent write exclusion,
  source access, restart, and no manual pump.

Remaining hardening:

1. Add broader GC-abandon and unexpected-exit coverage plus repeated
   startup-timeout and capacity/recovery stress lanes.
2. Keep the immediate-close, close-during-`SourceFacts`, deferred-source/close,
   one-command rapid-edit pacing, typed protocol failure,
   retired-frame recovery, and multi-page provisional regressions in CI.
3. Rerun format, analysis, Rust workspace/clippy/header/Wasm, focused Dart, and
   combined package gates at each native ABI change.

M1.0 does not claim parser output, publication, host queries, or Web Worker
support.

### M1.1 — Exact clean vertical through final seams

**Status:** Complete for the initial exact-clean ownership vertical. Green
receipts cross both production endpoints, independent native/main-context-Wasm
hosts, the bounded public Dart query, one shared native/Web semantic fixture,
and exact publish-archive consumers. The packet-only ABI-v2 native and Wasm
assets are rebuilt and the combined resource/lifecycle lanes are green.

**Outcome:** One actual parser document accepts exact source, performs bounded
clean parsing, atomically publishes a self-contained revision into an
independent host, answers a bounded Dart query, and closes on native and web.

`Clean` means the parser computes the result from its persistent worker-side
replica without suffix reuse. It does not mean Dart sends or materializes the
whole document. All work is already resumable and uses the final ownership and
query seams.

Current narrow grammar contract:

- empty documents publish zero structural leaves;
- every nonempty blank-separated source byte belongs to exactly one ordered
  `Paragraph`, `Blank`, `DefinitionsOnly`, `FencedCode`, `IndentedCode`,
  `AtxHeading`, `SetextHeading`, `ThematicBreak`, or typed `Unsupported`
  leaf;
- every leaf carries exact byte and UTF-16 boundaries, including CRLF and
  Unicode; and
- definition runs retain both proven terminal outcomes and leaf-local reference
  counts.

The selected CommonMark block controller must produce each typed result before
admission. A typed `Unsupported` leaf remains exact source coverage and queries
as source-backed `Unknown`; invalid definition-looking text left literal by the
normative grammar remains Paragraph text. Strong/emphasis/code inline authority
is implemented for explicit inline-bearing Paragraph, ATX-heading, and
Setext-heading content projections. Setext excludes only the terminal
content-line EOL and preserves earlier line endings as softbreaks. ThematicBreak
is a non-inline atomic leaf with exact facts and an empty projection.
The same admitted inline-bearing leaves carry parser-certified direct and
full/collapsed/shortcut reference links/images through fixed geometry facts
plus the authenticated `FLKIV001` cooked-value companion. Undefined, malformed,
or incomplete forms remain literal when definitive; missing resolver authority,
over-cap tails, and unrepresentable winning values fail the whole leaf closed.
This does not admit broader bracket hazards, general definition editing, or a
complete reference-link interaction surface.
IndentedCode publishes an exact variant-7 summary, while its physical-line
projection remains a separate at-most-8-KiB demand returned through viewport
schema 3 as canonical 20-byte records. The derivation reuses the same segmented
lexical facts, is resumable and cancellable, and has fuelled release plus
zero-residency close evidence. This contract does **not** claim broad grammar,
virtualized multi-block rendering, or authenticated restart/convergence for
edits inside IndentedCode.

Implementation:

1. Create one private production parser crate/module with a move-only segmented
   source capability and opaque block lifecycle.
2. Pin Comrak exactly to `0.54.0`, commit
   `172c2ee7d2c5c262a28be3e407aadf705daea2b7`, and crates.io checksum
   `0d5910408554659ed848ff469e67ec83b30f179e72cec286cfdae64d1616f466`.
   Coherently extract the root controller and reference finalizer with
   provenance; do not align v3 to the legacy bridge's unproved `0.50` baseline.
3. Preserve same-controller segmented giant-line handling at admitted
   quiescent roots. Oversized continuation in unsupported open containers fails
   closed rather than taking a special parser path.
4. Retain every competing root opener in normative order. Unsupported winners
   return typed `Unknown`; in particular, GFM Table candidates cannot become
   Paragraph while the source-backed Table detector is being composed.
5. Build one candidate containing certified SourceFacts, compact Green,
   physical/logical projection, paged reference/cooked-value facts, and a
   `CleanEofOnly` checkpoint.
6. Canonically digest records into bounded frames and stream consecutive
   frames through credited raw `FPK3` packets. Packet credit reports the next
   frame ordinal; commit authenticates actual frame count and encoded frame
   bytes. The manifest binds full document/publication identity and sibling
   roots.
7. Decode and install the complete candidate in an independent host store.
   Transport delivery alone is not installation acknowledgement.
8. Add bounded source/structure/projection/reference queries over a lightweight
   revision snapshot.
9. Expose the vertical through the public Dart session and narrow adapter SPI.
10. Run the unchanged endpoint under the native isolate and Web Worker/Wasm.

Acceptance:

- supported clean opens and edit-derived histories produce identical canonical
  results;
- reference-only and visible-remainder transitions cross source, parser,
  publication, host query, and close;
- stale, superseded, cancelled, malformed, and fault-injected candidates
  publish nothing and preserve the prior current root;
- native and Wasm semantic digests match;
- 10 MiB single-line and 100,000-line/reference witnesses keep packet payloads
  and derived output bounded and reclaim to the configured ownership floor;
- repeated revisions retain no recursive manifest/interner ancestry; and
- a Dart-only consumer performs open -> edit -> observe -> bounded query ->
  close without constructing or manually pumping a driver; and
- a clean consumer outside the repository runs that flow from the package
  archive with `dart run`, then from a relocated AOT executable and unrelated
  working directory, without discovering repository build artifacts.

Checkpoint A pre-review scheduling observations (updated 2026-07-28):

These are engineering observations and packet-only ABI-v2 acceptance receipts,
not user-review results. Checkpoint A feedback remains pending.

- parser fuel now counts at most 4 KiB of aggregate discovery,
  admission/commit, and lexical/classifier work per parse transition, with
  explicit nonzero charges for zero-byte state changes;
- fuel 1 and fuel 32 produce identical terminal results and transition totals
  across giant, 80-byte, newline-dense, CRLF/Unicode, empty, and CR-boundary
  sources;
- the external Worker performs one source-first 32 KiB grant, candidate
  microgrants of 64 transitions, a hard per-turn candidate grant of 4,096
  transitions, and a four-millisecond target slice. Events and zero progress
  stop the turn immediately; hard grants remain authoritative when elapsed
  time overshoots one atomic call;
- the prior 8-transition/64-transition scheduling policy made the
  3,307,535-transition 100,000-reference candidate require roughly 413,000
  Worker-to-Wasm calls and at least 51,000 clamped timer turns. It timed out at
  60 seconds despite a responsive caller thread. The 64/4,096 calibration
  removes that orchestration failure; a 256/16,384 trial improved convergence
  by only about 3.4 seconds while enlarging the non-interruptible call, so it
  was rejected;
- actual release Chrome reaches current structure in about 0.25-0.36 seconds
  for 1 MiB giant/ordinary-line witnesses and 2.7-3.3 seconds for 10 MiB. A
  bounded 1 MiB edit spends about 1.1 ms in foreground `apply()` and reaches
  exact current structure in about 0.26 seconds;
- active close during a 10 MiB candidate completes in about 6.5 ms, while
  rapid supersession installs only the latest revision and reaches it in about
  0.22 seconds;
- the public definition-free 100,000-Paragraph witness is roughly 3.2 MB,
  spends less than 8 ms in foreground work, and completes structural
  replacement in 20.994-26.977 ms native and 29.9-30.1 ms in Chrome. This is a
  realtime narrow incremental result, not evidence for definitions or broader
  block grammar;
- the Rust 100,000-reference lifecycle publishes 100,010 records in 452
  packets, with a maximum outer event of 71,019 bytes and maximum raw packet of
  70,967 bytes, then replaces, queries, and reclaims to zero; and
- a 10 MiB `x\n` adversary remains an explicit roughly 13-second cold backlog:
  its more than five million real physical lines take about 3.4 seconds in the
  direct release parser before Wasm, source transfer, and scheduling overhead.
  It remains off the caller, bounded, and promptly cancellable. This is not
  hidden behind an M1.1-only early-`Unknown` shortcut.

### M1.2 — Narrow authenticated incrementality

**Status:** In progress. Definition-free segmented top-level interior
restart/crop/splice is green for Paragraph, ATX-content, Setext
Paragraph↔H1↔H2, Paragraph↔thematic-break, and fenced-code-body edits bracketed
by ordinary Paragraph checkpoints within 64 KiB, through the production
endpoint and public native/Chrome runtime. Checkpoint A's user review remains
pending; Checkpoints B and C have not passed.

Keep exactly M1.1's narrow grammar, publication, host, and query API. Add
and then broaden restart, convergence, and unchanged-suffix adoption without
changing consumer contracts.

The first production foundation is now executable:

- every committed byte or atomic UTF-16 edit mints move-only scalar lineage
  containing exact old/new source authority and ordered byte/UTF-16 spans,
  while retaining no Crop root, lease, or source text;
- the document actor preallocates and retains a bounded 64-transition lineage
  chain. Expired, foreign, crossed, or closed lineage produces a clean-parse
  fallback rather than reuse authority;
- unchanged ranges and affinity-bearing restart/convergence boundaries map
  exactly through insertions, replacements, deletions, Unicode, and touching
  multi-operation edits;
- the parser edit envelope is derived from the first retained lineage before
  SourceFacts page widening. Adjacent retained edits compose through it;
  distant edits remove only that narrow authority and fall back to the existing
  wider or clean lane. Empty insertion ranges remain valid;
- canonical SourceFacts page content now uses page-local relative checkpoints,
  a position-independent digest, and an associative summary over byte, UTF-16,
  line, split-CRLF, and rolling-hash state. The terminal-lone-CR plus appended-LF
  case is an explicit regression; and
- exact certification installs the active target facts while only a matching
  independent-host structural commit advances the reusable delta base. Rapid
  supersession therefore continues from the last committed base on both Dart
  and Rust rather than minting authority from an uncommitted candidate;
- the production arena can journal a retained immutable node into a new build
  with generation, owner-cap, cancellation, and fuelled-reclamation checks; and
- structural leaves can be packed into an arena-backed measured tree with at
  most 64 semantic entries per page, one shared immutable root behind paired
  Green/Projection wrappers, bounded affinity-aware point lookup, and a
  fuel-bounded persistent exact-clean cut/splice path; and
- in a definition-free segmented top-level document, the crop parser expands
  only its exact edit envelope to the preceding and following authenticated
  ordinary Paragraph checkpoints and admits at most 64 KiB. A definition-bearing
  clean result also mints ordinary authority strictly after its last
  definition-bearing leaf, with the exact total definition count frozen into
  every surviving checkpoint. At production
  4,096-UTF-16 SourceFacts spacing the Paragraph endpoint crop is 4,116 bytes /
  168 lines / three parser transitions, and `ExactBaseDelta` replaces 8 of
  16,386 records; and
- the 671,794-byte mixed fixture proves that an ATX-content edit and a
  fenced-code-body edit between those anchors both enter
  `ParsingOrdinaryExact` and publish successive `ExactBaseDelta` revisions.
  Removing the closer makes the fence consume the former convergence suffix
  and therefore falls back to definitive exact-clean parsing; and
- a 4,096-block Setext fixture proves Paragraph→H1→H2→Paragraph transitions at
  one middle block all enter `ParsingOrdinaryExact`, publish `ExactBaseDelta`,
  transfer and replace at most 64 records, preserve exact first/middle/last
  queries, and retain next-revision authority. A same-block Paragraph→Setext
  promotion whose prior content exceeds 4 KiB rejects the narrow restart and
  uses definitive exact-clean parsing; and
- a separate 4,096-block fixture proves Paragraph↔thematic-break promotion and
  demotion remain in one parser crop bounded to 16 KiB / 512 physical lines /
  4,096 transitions, publish the exact structured role variant 6, and splice
  at most 64 deleted and replacement records; and
- the 2,048-definition / 2,048-Paragraph fixture proves that a length-changing
  middle Paragraph edit uses `ExactBaseDelta`, transfers at most 64 records,
  retains all References, preserves exact first/middle/last queries, and
  leaves enough public authority for a second exact revision; and
- the definition-free 4,096-Paragraph BOF fixture proves that a length-changing
  first-block edit enters `ParsingOrdinaryExact`, publishes `ExactBaseDelta`
  with at most 64 transferred and replacement records, preserves exact
  first/middle/last geometry, reuses suffix checkpoints, and reaches exact
  public revision 3; and
- the definition-free 4,096-Paragraph EOF fixture proves the symmetric
  final-block route with the same phase, publication mode, and record bounds.
  It retains prefix checkpoints, correctly mints zero fresh EOF checkpoints,
  preserves exact first/middle/last geometry, and reaches exact public revision
  3. Separate parser split/merge cases adjust the exact authenticated top-level
  block count and relevant ordinal by +2/-2.

This is not yet broad M1.2 acceptance. The production path now has authenticated
restart, bounded crop, exact-base splice, and public native/Chrome replacement
for the definition-free interior/boundary and reference-frozen
Paragraph-anchored interior/EOF subsets. Edits to or before definitions,
definition-bearing BOF, typed unsupported leaves, missing ordinary Paragraph
anchors, new tail definitions or unsupported constructs, over-cap crops, lost
convergence, nested/multi-child containers, and the remaining block grammar do
not inherit that incremental authority and take the definitive exact-clean
fallback. In
particular, exact-clean IndentedCode structure and selected-leaf projection do
not yet authorize restart or convergence for an edit inside that block. The
same is true for the first exact-clean depth-one BlockQuote vertical. An
admitted top-level tight BulletList now has a distinct checkpoint-free
source-rope local-delta path. It validates bounded predecessor/changed/successor
windows in the base and target and reuses the existing authenticated
crop/splice publication path without traversing list-wide source.

Implementation:

- retain the completed relative page-local `SourceFacts`, composable subtree
  summaries, and split-CRLF/rolling-hash boundary state;
- retain the completed exact-base SourceFacts splice rather than adding a
  second source-summary authority;
- make that measured sequence the canonical published SourceFacts role, with a
  versioned compositional strong commitment that permits an independent host
  to validate fresh changed paths without rewalking exact-base subtrees;
- exclude ephemeral publication/root/revision/generation identity from
  reusable canonical role pages and digests;
- bind reused pages to the target certified source through fresh role wrappers
  and a fresh manifest; and
- retain the completed Paragraph-anchored mixed interior crop/splice path while
  extending its restart authority through the named fallback regions and then
  construct by construct, adopting suffixes only after exact source,
  parser-state, profile, and canonical convergence checks.

Acceptance proves clean-vs-incremental equality after every edit, identity
reuse of unchanged pages, bounded changed work, false-convergence rejection,
cancellation, reclamation, native/Wasm parity, and no old-manifest ancestry.

### M2 — Complete block control

**Outcome:** All selected CommonMark/GFM block constructs use the same parser,
restart, writer, and publication transaction now proven first for
definition-free top-level Paragraph-anchored interior crops containing
Paragraphs, ATX Headings, Setext Headings, ThematicBreak, fenced code, and Blank
leaves, plus ordinary Paragraph crops strictly after a frozen definition prefix and
first-Paragraph crops from BOF or final-Paragraph crops to EOF in definition-free
segmented documents.

Blank/gap coverage and blank-separated Paragraph/DefinitionsOnly/typed
`Unsupported` segmentation, fenced code, and top-level ATX and Setext Headings
and ThematicBreak are complete through their current verticals. IndentedCode is
also complete for exact-clean variant-7 structure and separately demanded
selected-leaf projection. A depth-one single-Paragraph BlockQuote is complete
for exact-clean variant-8 structure, separately demanded schema-4 path/line
projection, and marker-free Flutter editing; nested/multi-child quote shapes
remain typed unsupported. One top-level depth-one tight BulletList is complete
for exact-clean variant-9 structure, the established separately demanded
schema-5 selected-item projection, marker-free Flutter editing with
terminal-empty exit, and checkpoint-free source-rope local deltas; loose, task,
nested, mixed-marker, and multi-block BulletList forms remain typed unsupported.
The compact schema-6 selected-item geometry demand and its
subsequent selected-content inline demand are complete. One top-level depth-one
tight `OrderedList` is complete for exact-clean variant 10
structure, a constant schema-7/payload-kind-6 selected-item projection,
geometry-then-inline demand, marker-free same-client editing, and a bounded
20,000-item local delta. This does not admit broader ordered-list grammar:
loose, task, nested, multiline/lazy, mixed delimiter/type, container-wrapped,
multi-block, and 10-digit-marker forms remain typed unsupported. Add
authenticated IndentedCode/BlockQuote restart/convergence alongside Paragraph/container
continuation, then broaden the list family, HTML blocks, and the authenticated
GFM Table control path.
Prove every-line
restart differential fixtures, deep/giant constructs, cancellation,
clean-vs-incremental equality, bounded reclamation, and native/Wasm parity.

No construct may add a second scanner or document snapshot outside the single
parser authority.

### M3 — Inline, semantics, and Dart presentation queries

**Outcome:** Bounded inline parsing and document-scoped semantics provide the
facts required by Dart-only consumers and the Flutter live editor.

The first authenticated projection cursor now resolves emphasis, strong,
inline code, GFM strikethrough, accepted angle autolinks, ASCII escaped
punctuation, hard line breaks, character references, and direct plus
full/collapsed/shortcut reference links/images with parser-authored geometry
and cooked scalar, destination, and title values. Continue with broader bracket
hazards, reference occurrence/dependency and mutation indexing beyond the
retained first-winner root; stable structure/source identities;
diagnostics and edit capability queries; and explicit cold/streaming exports.

Acceptance covers undefined-to-defined transitions, winner
deletion/promotion, large semantic fanout, dense delimiter resources, mutation
histories, and exact clean/incremental/export convergence. A local edit in a
10 MiB Paragraph must reuse bounded inline state; block suffix reuse alone does
not satisfy this gate.

### M4 — Flutter parser-to-paint vertical

**Outcome:** `flark_flutter` renders only current-revision facts from the public
Dart session.

The selected-Paragraph, fenced-code body, ATX-heading, Setext-heading,
indented-code, narrow tight-BulletList/OrderedList selected items, and atomic
thematic-break input islands now cross the real managed binding; the bounded
inline cache preserves revisited current-revision facts. Selected Paragraph
editing and passive viewport runs now hide certified emphasis, strong,
inline-code, and strikethrough markers while retaining their semantic styles;
strikethrough also composes with IME underlining. Escaped punctuation hides its
certified backslash without adding a style and remains marker-free across the
passive-to-active handoff. Direct links and image alt text are also marker-free
in the active island and carry no active gesture recognizer. Passive direct and
reference links use parser-certified cooked targets; passive direct and
reference images use an explicit builder or safe labelled fallback and never
perform implicit I/O. Active edit/recertification evidence remains specific to
the direct-media paragraph.
Links nested inside image alt text cannot activate, while an image inside a
surrounding link retains only the outer link action. Both heading forms use
the generic Heading Dart API. The indented-code
island consumes the separately demanded schema-3 line recipe, hides certified
prefixes, maps Enter to canonical indentation, and recertifies exact-current on
the same client. The BulletList island currently consumes the established
separately demanded schema-5 selected-item record, hides its certified
marker/prefix, paints the marker in a gutter, retains exact canonical source,
hands off between items, and applies parser-authored continuation,
terminal-empty exit, and prefix removal operations. Its compact successor
explicitly resolves schema-6 selected-item geometry before demanding inline
facts for that exact content range. That combined
native/Dart/Flutter/browser path is green: rebuilt-Wasm/freshness 2/2, Chrome
semantic parity 1/1, and the managed compact batch 3/3 on native Flutter and
3/3 on Chrome. The first combined Chrome run had one transient timeout; the
immediate isolated rerun and full rerun passed, so this is not a deterministic
performance budget. The OrderedList island consumes the distinct
schema-7/payload-kind-6 constant item projection, installs geometry before
inline facts, omits its certified marker from editable text while painting it
in the gutter, and retains the exact marker in canonical source. The focused
native/Chrome managed case continues `007)` as `008)` with CRLF on the same
`EditableTextState` and platform input client. The thematic-break island is
affinity aware, contributes no
fake editable characters, paints one semantic divider, and deletes its whole
canonical source atom with Backspace or Delete on the same client. The mixed
4,096-Paragraph checkpoint edits the ATX content and
fence body while preserving marker-free presentation, exact source, one
`EditableTextState`, and one platform input client on native and Chrome. The
pure-Dart visible-block materializer now consumes one authenticated structural
range quantum per advance with a hard 256-block window cap. Its Flutter
coordinator advances at most once per frame, and the 4,096-reference Chrome
checkpoint reaches exact range state before and after a marker-free edit. The
standalone 100,000-reference real marker-free Flutter Web/Worker/Wasm product
gate is green across seven zero-cadence platform deltas while preserving one
`EditableTextState` and platform input client, exact canonical source, and
final convergence. The latest measured preceding-build workstation receipt is
a 4.2 ms maximum synchronous callback and 7.6 ms total callback time;
grammar-revision-7 current-byte performance, floor-device `FrameTiming`, and
SLO evidence remain open. A giant top-level block remains one structural
record. This is structural viewport-demand closure, not a virtualized
multi-block editor or a 100 MiB product gate.

Implement the production `DeltaTextInputClient`, bounded active input island,
composition-preserving handoff, cross-island commands, revision-safe viewport
handoff into virtualized layout/height indexes, shaping-safe windows, overlays,
hit testing, and current-revision accessibility semantics.

Acceptance requires direct input-to-paint receipts for formatting, fences,
paste, undo, supersession, selection/caret stability, IME, viewport motion, and
stale-paint fallback. While recertification is pending, mechanically updated
active projection and last-exact bounded passive geometry remain stable, but
stale semantic actions, Markdown hit targets, and accessibility semantics are
suppressed. Active direct-media label/alt editing is now green on the real
Worker/Wasm checkpoint; mobile-Web soft-keyboard, floor-device, broader
selection/IME, and remaining grammar receipts stay pending.

### M5 — Behavioral inheritance and grammar completion

**Outcome:** V3 replaces learned v2 behavior, not merely its parser.

Classify and port the existing suite: transaction ordering/inversion, undo
grouping, selection mapping, edit commands, source fidelity, exports,
CommonMark/GFM fixtures, incomplete-input transitions, tables/tasks/code/HTML/
links, and historical regressions. Replace representation-coupled assertions
with named v3 invariants and run clean-vs-incremental comparison at every edit.

V2 remains a separately selected compatibility engine during this milestone.
A session never combines v2 prediction with v3 facts.

### M6 — Scale, platform, and launch closure

**Outcome:** The architecture earns the live-editor promise on floor native and
web hardware.

Required gates:

- sustained 1/10/100 MiB ordinary and adversarial edit workloads;
- input-to-authoritative-paint and frame-tail percentiles;
- latest-wins backlog, cancellation latency, and starvation freedom;
- source, history, parser, projection, publication, host, layout, and retained
  root memory;
- IME across supported keyboards/scripts and touch/mouse/keyboard selection;
- joining scripts, ligatures, bidi, wrapping, and long unbroken text;
- screen-reader traversal and semantic actions;
- worker crash/restart, stale replies, close/reopen, and leak checks; and
- package dry runs, native linkage, Wasm freshness, Worker packaging, and CSP
  behavior.

Only after these pass does v3 become the default `flark.dart` engine. Keep v2
for one explicitly bounded compatibility cycle, then remove it.

## 7. Public Dart API promotion gate

Before M1.1 closes, ordinary Dart use must be this simple in shape:

```dart
final document = await FlarkV3DocumentRuntime.open(markdown);
await document.initialReady;
document.apply(transaction);
final facts = document.queryAtUtf16(offset, budget: budget);
// Render, index, lint, export, or otherwise consume exact revisioned facts.
await document.close();
```

The real API may refine names and sync/async details, but it must not expose:

- driver pumping;
- source-page or terminal-proof ACK choreography;
- endpoint generations or FFI handles;
- publication frame/packet/commit/delivery receipts;
- Flutter values; or
- an apparently cheap getter that materializes the whole document.

`package:flark/flark.dart` becomes this Dart engine API.
`package:flark_flutter/flark_flutter.dart` becomes the normal Flutter import and
may re-export shared engine values. One-shot parse/export helpers use this same
engine and disclose whole-document cost.

`FlarkV3DocumentRuntime.open` now provides this document-owning shape and derives
binding authority from the owned source session. Attachment is no longer a
member of the normal runtime: `FlarkV3DocumentRuntimeAdapter.attach` remains an
explicitly unstable adapter/test seam outside `flark_v3.dart`. The versioned
preview surface is narrowed and guarded; final unversioned naming and the
rendered-node taxonomy wait for Checkpoint C. The same facade now runs from
exact archive-native macOS and archive-Web Chrome consumers; Linux CI must
exercise the implemented native archive branch before cross-platform release.

## 8. Ownership map

| Concern | Owner |
| --- | --- |
| Exact source, transactions, history, canonical ranges/anchors | Dart `flark` session |
| Source replica, grammar, persistent facts, restart/convergence | Rust parser document |
| Endpoint generation, event credit, recovery, close/drain | shared Rust/Dart schema-3 FSM |
| Candidate admission and atomic root publication | Rust parser/publication transaction |
| Installed revision and bounded structural queries | independent host store + Dart value API |
| Native FFI and dynamic library lifetime | long-lived native Dart isolate |
| Parser Wasm instance and web endpoint lifetime | Web Worker |
| Independent Web host/query Wasm lifetime | browser main context |
| Text input, layout, paint, geometry, semantics, widgets | `flark_flutter` |
| Normative and behavioral truth | shared fixture and acceptance corpus |

No convenience API may cross these boundaries by recreating grammar, exposing
mutable worker state, or hiding document-sized work.

## 9. Verification policy

Every milestone maintains four evidence lanes:

1. **Contract:** import, type, wire, ABI/header, ownership, and compile-fail
   boundaries.
2. **Correctness:** normative, differential, cross-language golden,
   clean-vs-incremental, mutation, fault, and retained-behavior oracles.
3. **Resource:** copied bytes, messages, allocations, pages/roots, fuel,
   cancellation latency, retirement, finalization, and reclamation.
4. **Product:** source-to-query and input-to-paint latency, IME, selection,
   shaping, accessibility, viewport behavior, and floor-device frame tails.

Focused tests are iteration evidence. A milestone closes only on its named
cross-boundary acceptance lane. Test counts alone are not acceptance evidence.

## 10. Architecture control

Implementation may refine internal types, scheduling budgets, record layouts,
and milestone task grouping. It may not quietly change the decisions in the
definitive summary. If a named reopen condition is observed, record the failing
receipt in the proof ledger before proposing a new parser or topology.

The current plan is narrowing, not reopening: review Checkpoint A, prove
persistent SourceFacts identity reuse at Checkpoint B, then prove role-root
delta publication and authoritative live-editor feel at Checkpoint C behind the
already-established source, publication, independent-host, query, and
public-runtime seams.
