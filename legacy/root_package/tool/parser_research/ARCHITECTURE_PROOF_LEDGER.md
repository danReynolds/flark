# Live Markdown architecture proof ledger

Status: active evidence ledger for the RFC 023 selected architecture,
2026-07-29. This is not a production or launch claim. It records which
contracts are executable, which are provisional, and which conditions should
reopen the selected direction.

## Candidate being tested

```text
Dart caller isolate / application isolate
  exact source, transactions, revision and worker lifecycle
  selection/composition values when an editor supplies them
             |
             | bounded revisioned source intents
             v
native parser isolate / Web Worker
  exact pending Crop root: revision + byte/UTF-16 dimensions + atomic lineage
  one derived SourceFacts index; CertifiedSource only at exact clean EOF
  exact Flark-owned block continuation + parser-site typed output port
  packed source-ordered serialized green with unified source/projection runs
  bounded inline/reference services over certified logical projections
  reference global-occurrence, exact-label sequence/checkpoint, and dependency aggregates
  immutable revision roots + exact restart state + exact Unknown ranges
             |
             | bounded revision-tagged publication chunks
             v
platform-neutral Dart host/query store
  atomically adopts current-revision facts
  exposes bounded source, structure, projection, semantic and presentation queries
             |
        +----+-----------------+
        v                      v
Flutter adapter          Dart-only consumer
  active input island      CLI/server/indexer/linter/exporter
  layout and paint
  source/caret paint never waits for Markdown
```

Block recognition, logical projection, inline/reference interpretation, and
presentation are separate lifetimes, not separate competing parsers. Each
Markdown rule is implemented once against Flark-owned state. Donor algorithms
may come from Pulldown, Comrak, cmark-gfm, or the normative specifications, but
no donor runtime tree or Dart classifier gets independent authority.

## Evidence state

The following rows control the current decision. “GO” applies only to the
named mechanism; it does not inherit into production or launch readiness.

### User review checkpoint ledger

| Checkpoint | Current evidence state | Demo/evidence required at review | Approval criterion |
| --- | --- | --- | --- |
| **A — Responsiveness diagnosis** | **Engineering evidence ready; user review pending.** Release native/Web receipts prove bounded foreground apply, a roughly 13.6 ms observed browser heartbeat gap for the earlier 100,000-reference engine case, latest-wins cancellation, exact convergence, and truthful close. The production-spacing 100,000-Paragraph public path replaces one middle edit in 20.994–26.977 ms native and 29.9–30.1 ms Chrome with foreground apply below 8 ms. The standalone real Flutter Web/Worker/Wasm marker-free 100,000-reference gate is green: seven zero-cadence platform deltas retain one `EditableTextState` and input client, preserve exact source, and converge the final revision; the latest measured preceding-build standalone Chrome receipt records a 4.2 ms maximum synchronous callback and 7.6 ms total across the burst. The preceding-build combined small-widget→100,000-reference-widget sequential reopen gate was green after the Web module-loader cache-lifetime correction; that Chrome receipt was a 5.1 ms maximum synchronous callback and 8.8 ms total callback time, separate from the standalone receipt. Grammar revision 6 has current functional/freshness/parity/checkpoint evidence, not a new timing receipt. | Review the release lab across small, 1 MiB, 10 MiB, and 100,000-reference fixtures, then inspect the production-spacing 4,096- and 100,000-Paragraph receipts plus the standalone marker-free 100,000-reference receipt with source/certified/structure revisions, foreground time, exact convergence, and close visible. Preserve exact gate commands and rebuilt-asset identity beside the demo. | The user accepts or requests changes to the off-caller scheduling/lifecycle and observability model, and agrees that the bounded Paragraph route closes that latency case without generalizing to broader block grammar. The workstation timing is not floor-device `FrameTiming` or launch-SLO proof. No approval is recorded yet. |
| **B — Persistent SourceFacts identity-reuse edit** | **Engineering evidence ready; user review pending.** The production probe covers prefix, middle Unicode, tail, and split-CRLF edits; exact clean equality; retained page identity/digest; bounded planning/splice work; unsafe-lineage rejection; cancellation/fallback; zero-residency close; and one shared native/real-Wasm parity digest. | Run the fixed proof from the lab and inspect the changed crop, retained prefix/suffix identities, replacement pages, work receipts, lifecycle checks, and cross-platform digest. | The user finds direct persistent reuse understandable and sufficiently robust to authorize promotion of the storage contract. No approval is recorded yet. |
| **C — Role-root delta and live editor integration** | **Partial production-shaped evidence; reviewable, not passed.** Authenticated exact-base SourceFacts reuse and packed block-page splice are complete. Ordinary Paragraph checkpoints bracket at-most-64-KiB interior Paragraph, ATX-content, Setext Paragraph↔H1↔H2, Paragraph↔thematic-break, and fenced-code-body crops in definition-free top-level documents. The 4,096-block thematic transition fixture stays inside one bounded parser crop and its exact packed splice deletes/replaces at most 64 records. The corresponding Setext fixture stays on `ParsingOrdinaryExact` -> `ExactBaseDelta`, transfers and replaces at most 64 records, preserves exact first/middle/last queries, and remains exact for the next revision; an over-4-KiB same-block Paragraph→Setext promotion takes the clean fallback. Strictly after the last definition-bearing leaf checkpoints carry the exact frozen count: a 2,048-definition / 2,048-Paragraph fixture publishes a length-changing local `ExactBaseDelta`, transfers at most 64 records while retaining all References, preserves exact first/middle/last queries, and completes a second public revision. Separate 4,096-Paragraph definition-free BOF/EOF fixtures prove length-changing first- and final-block crops enter `ParsingOrdinaryExact`, publish `ExactBaseDelta` with at most 64 transferred and replacement records, preserve exact first/middle/last geometry, and reach public revision 3. BOF reuses suffix checkpoints; EOF reuses prefix checkpoints and correctly mints zero fresh EOF checkpoints. A bounded 128-leaf/2,048-fact-record Dart cache preserves current-revision inline facts after the singleton host sidecar moves. Paragraph, `FencedCode`, ATX Heading, Setext Heading, atomic `ThematicBreak`, and inline `HardLineBreak` cross parser, publication, independent host, public Dart, native/Chrome parity, real managed Flutter, and the release demo. The standalone 100,000-reference real Flutter Web/Worker/Wasm gate now applies seven zero-cadence platform deltas to the marker-free live tail while retaining one `EditableTextState` and input client, exact canonical source, and final exact convergence. The combined small-widget→100,000-reference-widget sequential reopen gate is green after the Web module-loader cache-lifetime correction; its current Chrome receipt is a 5.1 ms maximum synchronous callback and 8.8 ms total callback time. | Review the current selected-leaf and standalone 100,000-reference checkpoints, then extend incremental restart/convergence to edits of or before definitions, definition-bearing BOF, typed unsupported leaves, missing anchors, containers, lists, quotes, indented code, HTML, tables, and other block grammar; complete virtualized visible-set materialization and layout plus floor-device interaction receipts. New tail definitions/unsupported constructs, over-cap crops, and lost convergence continue to fail clean. | The user accepts both the delta architecture and the editor's instantaneous, fluid, trustworthy feel before broad grammar or public API promotion. C does not claim reference-link UI, incremental definition mutation, a complete virtualized multi-block UX, full grammar, 100 MiB viewport closure, floor-device `FrameTiming`/SLO evidence, accessibility, launch, or release readiness. |

Checkpoint C now also has a narrow grammar-revision-6 character-reference
vertical. Kind 9 crosses the parser, unchanged 20-byte `IFO2`/`IFP2`
publication, independent host, Dart projection/editing, native and Chrome
public runtime, managed Flutter, and the runnable checkpoint. It does not
admit direct or bracketed links/images, broader bracket hazards, or full
CommonMark/GFM.

### 2026-07-29 grammar-revision-8 strict bare-autolink addendum

This addendum preserves the revision-7 direct-media record above and advances
only the strict bare-autolink slice. The classifier admits exact lowercase
`http://`, `https://`, and `ftp://`; exact lowercase `www.` only at BOF or
after TAB, LF, CR, space, `*`, `_`, `~`, `(`, or `[`; and email with an ASCII
`[A-Za-z0-9.+_-]+` local part and a valid dotted domain. Uppercase letters in
an email remain valid. URI and `www.` candidates take precedence over email
candidates. GFM examples 621–631 supply terminal punctuation, entity-like
suffix, `<`, and excess-closing-parenthesis trimming.

Each fact retains source-relative markerless content geometry and chooses one
target recipe rather than carrying a copied cooked target:

- schemed bare URI → `exactContent`;
- lowercase `www.` → `httpPrefixedExactContent`;
- email → `mailtoExactContent`.

The whole-leaf job uses a resumable source-relative range cursor, reads source
in 256-byte chunks, and fuels scan, classification, precedence filtering, and
merge. Before invoking the synchronous classifier it charges one transition
per token byte. Tokens over 8 KiB, unknown or unresolved bracket context,
candidate overlap, or invalid state make the whole leaf unsupported with no
partial facts. Code, angle-autolink, direct-link/image, and bracket precedence
maps shield candidates; escaped brackets are respected. URI/`www.` candidates
are merged before email candidates in source order without overlap.

Exact lowercase `mailto:` and `xmpp:` are intentionally declined until their
target recipe and wire support are admitted. Uppercase URI/`www.` prefixes,
other schemes, relaxed autolinks, reference/collapsed/shortcut links, and the
rest of CommonMark/GFM are also outside this revision.

Same-ordinal activation is now idempotent when the exact ordinal is already
active and no activation is pending. This prevents replaying the handoff that
could replace the certified projected value with canonical source even though
block authority and caret intent had not changed. The example checkpoint
leaves the autolink block active after the direct-media handoff, then proves
that repeated activation preserves the projected value.

The verified revision-8 gates are:

- Rust exact-clean 46/46;
- promotion audit 2/2;
- engine 251/251;
- `cargo test -p flark-parser`: 309 non-doc tests and one compile-fail doctest
  passed, three manual scaling receipts ignored by design, and zero failed;
- packaging 12/12 and freshness 2/2;
- Dart inline facts/projection 68/68;
- native sidecar end-to-end 7/7;
- Web Chrome sidecar end-to-end 3/3;
- Flutter presentation/surface 24/24;
- example Chrome checkpoint 3/3 after the idempotent activation fix; and
- focused exact bare-classifier large-paragraph 1/1.

Broader bare cases are covered within the current green parser suite without a
separate aggregate here. Browser visual inspection retained styled pixels on
the first frame without a raw or blank frame; manual browser typing was not
driven in that inspection, while automated Chrome and Flutter tests cover
typing.

Root and Flutter assets are byte-identical at version
`dfcce276df7954a9-714e23750091d226-bba3dc0f34f51964`. The Wasm is
3,506,644 bytes with SHA-256
`dfcce276df7954a97a11f3faef4f93217adddba0d4b620db5e4942a8a2e4c930`;
the Worker is 33,195 bytes with SHA-256
`bba3dc0f34f51964fe55bf67363b75fdc68a1387ce28f1771529c44ad7493a60`.
These receipts do not claim full grammar, release readiness, floor-device
behavior, or new performance timing.

| Claim | Current state | Executable receipt | Still open |
| --- | --- | --- | --- |
| The engine can be Dart-first rather than Flutter-owned | **production package plus native/Web document-open-query vertical GO** | root `flark` has no Flutter SDK dependency or Flutter/UI imports; `flark_flutter` depends on it; `package:flark/flark_v3.dart` exposes one pure-Dart document runtime with binding-derived `open`, automatic bounded execution, exact-structure initial readiness, semantic source/certified/structure watermarks, synchronous bounded structural query, small apply/undo results, exact reads/export, recoverability, and truthful close. The normal barrel is an explicit allow-list; a negative analyzer receipt proves host/session/certification/parser-binding types and runtime attachment are unavailable. Public-only fixtures produce identical empty/reference/Unicode/unsupported/gap/edit/close semantics plus exact multi-block split/merge and atomic thematic-break semantics on native and real Chrome. Exact root/Flutter pub tarballs resolve through an isolated hosted cache with no path overrides, match extracted package trees, include and compare the Wasm buildinfo, compile external Dart source and JS plus Flutter Web, relocate/run a macOS arm64 native AOT bundle, boot/edit/close the real packaged Worker/Wasm runtime in Chrome, and prove no generated or packaged artifact retains the absolute checkout path. Rebuilt asset version `6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964` and Wasm SHA-256 `6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4` are byte-identical across root and Flutter and pass freshness checks against the current Rust inputs | exercise the implemented native archive branch on Linux CI, retain the narrowed surface and archive/parity gates, choose final unversioned names/rendered-node taxonomy at Checkpoint C, and retain Flutter only in the dependent adapter |
| Production source/ownership substrate is bounded and endpoint-safe | **M1.0 plus native/Web M1.1 parser/publication/host/query vertical GO** | the current grammar-revision-6 full Rust workspace all-targets suite is green. Character-reference fact kind 9 crosses parser recognition and cooking, unchanged 20-byte `IFO2`/`IFP2` publication, independent-host validation, Dart decoding/projection, managed Flutter editing, and native/Chrome public runtime. The focused Dart core gate is 60/60 including digest parity, Flutter active/source is 17/17, the native and Chrome public-runtime entity-edit cases are green, and Chrome asset/reopen is 3/3. Hard-line-break kind 8 retains its focused Rust and exact LF/CR/CRLF Dart/Flutter receipts. Schema-3 canonical `SourceFacts` pages plus terminal proof promote the exact same provisional revision through both endpoints; public receipts cover document open, exact parser work, credited candidate publication, independent-host install/query, edit-derived replacement, latest-wins supersession, and truthful close. Existing native receipts retain finalizer/revocation, fresh/recovery, close races, rapid-edit source coalescing, deterministic startup abandon, relocated CLI discovery, and an exact publish-archive AOT consumer. Focused Web receipts retain strict CSP, proof-based close, host separation, exact asset identity, an archive-backed real Chrome consumer, and a real terminal parser fault through public recovery, endpoint replacement, exact multi-page reseed, current query, and truthful Worker close. Fresh admission remains capped at 2,048 below a 4,096-slot resident ceiling with one create-before-revoke replacement reserved per admitted endpoint | retain these receipts in combined CI; close broader physical-Worker-death/GC/exit/capacity/floor-device stress. Source ACK remains transport/install proof only |
| UI source edits stay bounded independently of document size | **real selected-block managed-input verticals plus standalone 100,000-reference Web gate GO; floor-device/product breadth HOLD** | The managed binding owns a real `DeltaTextInputClient` over one stable `EditableText`, maps display-space edits and multi-stage composition to exact source, retains parser-certified projection provisionally without semantic guessing, and atomically adopts current authority without replacing the controller, focus, or input client. Paragraph, ATX Heading, and Setext Heading render marker-free certified inline content through a generic Heading Dart API; Setext hides the underline, excludes only the terminal content-line EOL, and retains internal softbreaks. Fenced code uses parser-owned body geometry and a bounded literal body path. ThematicBreak maps caret affinity to the exact boundary of an empty projection, paints one semantic divider, revokes stale semantic action authority synchronously, and maps Backspace/Delete to whole-atom deletion while preserving the same `EditableTextState` and platform client. HardLineBreak kind 8 hides only the Rust-certified marker, normalizes LF/CR/CRLF to one display newline, retains exact canonical source, and expands replacement or deletion to the complete marker-plus-EOL atom. CharacterReference kind 9 replaces the exact certified source token with Rust-cooked one- or two-scalar text, derives nested URI-autolink label/destination mechanically, consumes the complete token for partial replacement, and rejects UTF-16 edit endpoints inside surrogate pairs. Oversized leaves retain scalar/CRLF-safe bounded source-visible islands. A bounded current-revision Dart cache retains 128 leaves/2,048 fact records and invalidates atomically on edit, recovery, close, or fault. The real Flutter Web/Worker/Wasm 100,000-reference gate applies seven zero-cadence platform deltas to the bounded marker-free tail, retains the same `EditableTextState` and input client, preserves exact source, and reaches exact final convergence; the latest measured preceding-build workstation maxima are 4.2 ms synchronous callback and 7.6 ms total burst time. The preceding-build small-widget→100,000-reference-widget sequential reopen gate was green after the Web module-loader cache-lifetime correction; that Chrome receipt was a 5.1 ms maximum synchronous callback and 8.8 ms total callback time. Grammar revision 6 has current functional/freshness/parity/checkpoint evidence, not a new timing receipt. | virtualized visible-set materialization and multi-block layout; document-wide selection/commands, undo/paste/device IME, touch/accessibility/shaping, GC/frame, sustained floor-device evidence, and an explicit floor-device `FrameTiming`/launch-SLO decision |
| Exact block control can feed one source/projection/packed-green writer | **narrow every-size production exact-clean controller plus Paragraph, fenced-code, ATX-heading, Setext-heading, and ThematicBreak publication/query GO; full Table HOLD** | the move-only controller consumes only a pinned Comrak `0.54.0` lexical facade, checks every competing root opener in normative order, and uses one forward segmented line/reference path at every size. One parse transition accounts at most one 4 KiB aggregate quantum. Exact-clean output includes authoritative `FencedCode` geometry, ATX opener/content/optional-closer/EOL geometry, Setext H1/H2 content/underline/EOL geometry under structured role variant 5, and ThematicBreak marker/count/indent/BOM/envelope/EOL geometry under structured role variant 6 with an empty zero-run projection. Inline-bearing Paragraph and heading-content projections reuse one resumable strong/emphasis/code service; Setext excludes exactly the terminal content-line EOL while retaining prior line endings as softbreaks. Unsupported winners remain typed source-backed `Unknown`; donor-drift receipts forbid a hidden full-parser/oracle path. Exact-clean output and the admitted interior/BOF/EOF crops use one persistent packed block-page cut/splice path. Ordinary restart authority authenticates the exact top-level block count; segmented state is only `count > 1`. Exact clean output mints reference-frozen ordinary checkpoints only after the last definition-bearing leaf. Parser split/merge receipts prove exact +2/-2 count and block-ordinal changes across blank-boundary topology changes | restart/convergence and suffix adoption beyond the definition-free interior/boundary and reference-frozen Paragraph-anchored at-most-64-KiB subsets; the exact source-backed GFM Table detector; lists, quotes, indented code, HTML, containers, wider inline grammar, and later remaining grammar through the same authority |
| Bounded Paragraph edits can reparse and republish locally | **definition-free interior/BOF/EOF plus reference-frozen Paragraph, Setext-transition, and thematic-transition native/Chrome/Dart/Flutter verticals GO; broader restart authority HOLD** | the parser's exact edit envelope is distinct from the storage-aligned SourceFacts page range. Adjacent rapid edits compose into one authenticated envelope; distant edits fail closed. A 4,096-block fixture transitions one local middle block Paragraph→Setext H1→Setext H2→Paragraph; every phase enters `ParsingOrdinaryExact`, publishes `ExactBaseDelta`, transfers and replaces at most 64 records, preserves exact first/middle/last queries, and retains next-revision authority. An over-4-KiB same-block Paragraph→Setext promotion cleanly takes exact-clean fallback rather than reusing a stale checkpoint. A separate 4,096-block fixture promotes and demotes one middle Paragraph as ThematicBreak in a crop bounded to 16 KiB / 512 lines / 4,096 transitions; its packed splice deletes and replaces at most 64 records. The production-spacing Paragraph edit parses 4,116 bytes over 168 lines in three transitions and transfers 8 of 16,386 records. A 671,794-byte fixture places ATX Heading and fenced code inside 4,096 Paragraphs: both an ATX-content edit and a fence-body edit enter `ParsingOrdinaryExact`, publish successive `ExactBaseDelta` revisions, and retain exact first/middle/last queries on native and Chrome. The same mixed Flutter slice stays marker-free and retains one `EditableTextState` and platform input client across both edits. A 2,048-definition / 2,048-Paragraph fixture proves a length-changing middle Paragraph `ExactBaseDelta` with at most 64 transferred records, all References retained, exact first/middle/last queries, and a second exact public revision. Separate 4,096-Paragraph definition-free first/final-block edits enter `ParsingOrdinaryExact`, publish `ExactBaseDelta` with at most 64 transferred/replacement records, preserve exact first/middle/last geometry, and reach exact public revision 3. BOF reuses suffix checkpoints; EOF retains prefix checkpoints and correctly mints zero fresh EOF checkpoints | edits to or before definitions, definition-bearing BOF, typed unsupported leaves, missing ordinary Paragraph anchors, a new tail definition or unsupported tail, a crop over 64 KiB, or lost convergence take the definitive exact-clean fallback. Containers, lists, quotes, indented code, HTML, tables, reference-link UI, incremental definition mutation, and remaining block grammar still require explicit admission |
| Retained Setext restart/normalization preserves exact authority and identity | **GO for the selected transaction family** | focused Setext is 44/44, including 10 MiB parent-bound retained suffix splices, nested ownership, exact clean equality, stale/crossed authority, fuelled cancellation, EOF and non-Paragraph Open; deferred whole identity is acknowledged before non-Paragraph Open, parent/ancestor Close, or Finish | consume this family inside the final reference/Table and multi-root candidate publication; device parser-to-paint remains open |
| Table validation/replay can be bounded without a Paragraph snapshot | **authenticated cursor mechanism GO; direct grammar/writer integration HOLD** | private cursor gate 4/4; isolated scanner receipts add 7 differential, 5 downstream, 4 two-pass, and 4 Table/reference/Setext/list priority tests; `TableReady` is non-cloneable and the actor retains the only packed-green/Program/Crop cursors | sequential scanner adapter, parser priority, authenticated prefix retain/body continuation, real writer/final manifest |
| Source/projection checkpoint continuation is compact and derivable | **line-boundary mechanism GO; composite checkpoint HOLD** | source composer module 22/22; continuation retains zero source/heap payload, is capped at 224 bytes, and derives composer generation from sealed-run count plus one | join parser pause, open writer bindings, source lineage, normalization state, semantic roots, exact green cut, tail adoption, and host publication |
| Active-Paragraph semantic ranges can be replayed exactly without flattening | **projection and reference-finalizer integration GO** | focused cursor 7/7 and source-session 2/2; the 257-leaf far-range case performs one root descent, decodes one page, holds one live cursor, and makes two source reads with no prefix scan; the terminal path also splits an exact coalesced run, preserves reset ownership on the suffix, and restores the surviving Paragraph from a typed composer origin | apply the same authenticated cursor contract to Table and inline; broader crossed-authority, fault, and scale matrices are production hardening |
| Reference winner changes need not enumerate the unchanged suffix | **integrated architecture GO; production lifetime/full-manifest HOLD** | one production-shaped restart streams one replacement spool, promotes untouched suffix winners, resolves cooked values, and enumerates no suffix; the parser-owned finalizer completes reference-only removal and visible-remainder rewriting through CandidateWriter; Green plus the reference index commit as two child edges under one live parent owner; duplicate receipt: two occurrences, one exact label | flatten the interner's owning donor-manifest witness, then extend the same atomic owner to source/projection/checkpoint/inline/Table/host roots before production |
| One clean revision can assemble and independently install the fixed five-role owner without document-sized reference buffers | **exact-clean native/Web vertical, multi-block structural roles, selected-leaf sibling facts, authenticated exact-base reuse, and packed block-page splice GO; 100 MiB directory HOLD** | Exact `CertifiedSource` plus parser output produce canonical ordered multi-page SourceFacts, Green, Projection, cooked References, and CleanEof roles. A self-contained unique-node DAG reconstructs in independent native/main-context-Wasm host arenas, validates every role, and atomically swaps roots. Green and Projection bind one authority-free persistent measured block sequence; a revision/fence-bound hot-inline sibling installs independently. Exact-base reuse and the packed block-page cut/splice serve exact-clean output plus the admitted definition-free interior/boundary and reference-frozen Paragraph-anchored interior/EOF crops, allocating fresh wrappers/manifests with no old-manifest ancestry | replace the 128-page M1.1 directory with a multi-level directory for the 100 MiB gate; extend authenticated restart/convergence and suffix adoption beyond the admitted subsets; add bounded viewport/range query families and a visible-set materializer |
| M1.1 engine work stays off the caller and exposes its remaining backlog honestly | **Checkpoint A evidence ready; bounded anchored mixed restart and M1.2 storage mechanics supported; standalone 100,000-reference product receipt green; product acceptance pending** | Bounded worker/arena kernels and foreground mechanisms challenge the global-work model. Production-spacing blank-separated Paragraph edits retain a byte-exact parser envelope independent of the SourceFacts page cut: 4,096 Paragraphs parse 4,116 bytes and transfer 8 of 16,386 records, while the roughly 3.2 MB/100,000-Paragraph public path replaces in 20.994–26.977 ms native and 29.9–30.1 ms Chrome with foreground apply below 8 ms. The Setext vertical adds a 4,096-block local Paragraph↔H1↔H2 receipt bounded to 64 transferred/replacement records and an explicit clean fallback for over-4-KiB same-block promotion. The thematic vertical adds a separate 4,096-block Paragraph↔thematic-break crop bounded to 16 KiB / 512 lines / 4,096 transitions and a splice bounded to 64 deleted/replacement records. The separate 671,794-byte mixed path proves local ATX-content and fence-body deltas on native/Chrome and stable marker-free same-client Flutter. The 2,048-definition / 2,048-Paragraph public path proves a length-changing reference-frozen local delta and exact next revision. The 4,096-Paragraph definition-free BOF/EOF paths prove bounded length-changing first/final-block deltas, complementary suffix/prefix checkpoint reuse, exact first/middle/last geometry, and public revision 3. The standalone real Flutter Web/Worker/Wasm gate adds seven zero-cadence platform deltas behind 100,000 leading references with stable marker-free input ownership, exact source, exact final convergence, a 4.2 ms maximum synchronous callback, and 7.6 ms total on the latest measured preceding-build workstation run. The preceding-build combined small-widget→100,000-reference-widget sequential reopen gate was green after the Web module-loader cache-lifetime correction; that Chrome receipt was a 5.1 ms maximum synchronous callback and 8.8 ms total callback time, separate from the standalone receipt. Grammar revision 6 has current functional/freshness/parity/checkpoint evidence, not a new timing receipt. Exact-clean multi-block publication/query, exact-base SourceFacts reuse, packed block-page splice, and the bounded current-revision Dart inline cache remain complete | Checkpoint A/B user decisions; restart/convergence to edits of or before definitions, definition-bearing BOF, unsupported/unanchored regions, new tail definitions/unsupported constructs, over-cap or lost-convergence crops, containers, lists, quotes, indented code, HTML, tables, references, and remaining block grammar; virtualized visible-set materialization/layout and 100 MiB directory/viewport closure; real device IME/touch/accessibility/shaping; physical-Worker/GC hardening; and sustained floor-device native/Web `FrameTiming`, memory, maximum-slice, and launch-SLO gates |

### 2026-07-29 standalone 100,000-reference Web product and archive closure

The standalone large-reference product receipt and publish-archive receipt are
now green without passing Checkpoint C:

- the real Flutter Web release Worker/Wasm gate applies seven zero-cadence
  platform deltas to the bounded marker-free live tail behind 100,000 leading
  definitions, retains one `EditableTextState` and platform input client,
  preserves exact canonical source, and reaches exact final convergence;
- that rebuilt-byte workstation run recorded a 4.2 ms maximum
  synchronous callback and 7.6 ms total across the burst. These are regression
  receipts, not floor-device `FrameTiming` evidence or an accepted launch SLO;
- the combined small-widget→100,000-reference-widget sequential reopen gate was
  green, with a 5.1 ms maximum synchronous callback and 8.8 ms total callback
  time in that Chrome run. The prior stall was a Web module-loader cache
  lifetime defect: resolved modules are now cached as values, only active loads
  share an in-flight future, and resolved reuse completes in the reopening
  caller's async context. This is a separate receipt from the standalone run
  above;
- exact root and Flutter-adapter publish archives now include the Wasm
  buildinfo and compare the root/adapter Wasm, buildinfo, and Worker bytes. The
  isolated external-consumer lane resolves the archives with no path override,
  runs external Dart source, compiles external Dart to JavaScript, relocates and
  runs the macOS arm64 AOT bundle, builds Flutter Web, boots the real Chrome
  Worker/Wasm runtime, and proves that package/generated output contains no
  absolute checkout path; and
- at that closure snapshot, the asset version was
  `6b4fa631ad1ebe85-63aeeaed89225bfc-bba3dc0f34f51964`; the Wasm SHA-256 is
  `6b4fa631ad1ebe85ffc67e4e496de438c0ac7b6cde0f1c74024c31413e5183e3`.

This closed the standalone 100,000-reference workstation product gate and that
macOS/Web archive receipt. It did not close the 100 MiB
directory/viewport gate, broader grammar, Linux AOT execution, floor-device
latency/frame/memory evidence, user checkpoint decisions, accessibility,
launch, or release readiness.

### 2026-07-29 escaped-punctuation vertical closure

Grammar revision 4 publishes ASCII escaped punctuation as inline projection
kind 7. Parser-certified backslashes are hidden without adding a semantic
style, the two-byte source pair edits atomically, nested facts retain parser
preorder, cache reuse is ACK-bound, and malformed materializer records fail
closed.
Public semantic parity is green 1/1 on native and 1/1 on Chrome; the managed
Paragraph batch is 3/3 on both platforms; and passive-to-active handoff is 1/1
on both.

### 2026-07-29 character-reference vertical closure

Grammar revision 6 publishes parser-certified `CharacterReference` as inline
fact kind 9. The `IFO2` stream, `IFP2` page, and schema-2 canonical 20-byte fact
record remain unchanged. Kind 9 uses the existing kind byte, the scalar-count
byte, the exact source range, and the final two `u32` words for the
parser-cooked one- or two-scalar value. Rust is the only recognizer: named,
decimal, and hexadecimal references are decided in the inline lexer using the
pinned Comrak donor decoder; invalid or unterminated candidates remain literal,
escaped candidates retain escape authority, and code spans shield their
contents.

Dart does not recognize Markdown or entity names. It validates the parser
record and mechanically builds the replacement text, source/display mapping,
and link target from the certified source range and scalar payload. A URI
autolink can therefore contain a nested character-reference fact. The earlier
idea of decoding entities again in the autolink scanner—or rejecting an
otherwise valid entity-bearing URI—was removed; the one parser fact now cooks
both the displayed URI and destination. Email autolinks reject such a child.

Marker-free active and passive presentation replace the complete source token
with its cooked one- or two-scalar value. Replacement of any part of that value
consumes the whole source token; insertion re-emits untouched cooked
prefixes/suffixes as literal source, and source/display coordinate maps reject
UTF-16 endpoints inside a surrogate pair. These are editing mechanics over
parser authority, not a second grammar.

Verified gates for this milestone are:

- the full Rust workspace all-targets suite is green;
- focused Dart core is 60/60, including digest parity;
- Flutter active/source editing is 17/17;
- the public-runtime entity edit is green on native and Chrome; and
- the Chrome asset/reopen gate is green 3/3.

The rebuilt root and Flutter assets are byte-identical at version
`6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964`, with Wasm SHA-256
`6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4`.
This closes only the selected-leaf character-reference vertical. Direct and
bracketed links/images, broader bracket hazards, reference-link UI, and full
CommonMark/GFM remain open.

### 2026-07-29 hard-line-break vertical closure

Grammar revision 5 publishes parser-certified `HardLineBreak` as inline fact
kind 8. Rust is the sole recognizer; Dart and Flutter do not scan trailing
markers or infer break semantics. The admitted odd-backslash and
at-least-two-space forms cover the marker plus physical EOL, retain exact LF,
CR, or CRLF as content, and use a collapsed closer. Marker-free presentation
hides only the marker and maps the physical ending to one display newline while
canonical source/export remains byte-exact.

Display-space replacement or deletion expands to the complete
marker-plus-ending atom; insertion is authorized only at the certified
boundary. Nested emphasis retains the hard break as a non-style fact, code
shielding remains opaque, malformed records fail closed, and fuel partitions,
cancellation/reclaim, host persistence, and CommonMark differentials are
covered. An unshielded candidate followed by continuation indentation fails
the whole inline leaf closed rather than delegating recognition to another
layer.

At that hard-line-break snapshot, the Worker/Wasm asset version was
`6b4fa631ad1ebe85-63aeeaed89225bfc-bba3dc0f34f51964`, with Wasm SHA-256
`6b4fa631ad1ebe85ffc67e4e496de438c0ac7b6cde0f1c74024c31413e5183e3`.
The standalone 100,000-reference Chrome edit receipt was 4.2 ms maximum
synchronous apply and 7.6 ms total across seven zero-cadence edits. The
combined small-widget→100,000-reference-widget sequential reopen gate was green;
that Chrome run recorded a 5.1 ms maximum synchronous callback and 8.8 ms
total callback time. At that snapshot, bracketed/direct links and images,
entities, and broader bracket hazards remained pending. The
character-reference closure above supersedes only the entity item; full
CommonMark example 14 is not claimed.

### 2026-07-28 bounded Paragraph-anchored mixed restart and publication closure

This slice proves the incremental architecture for the definition-free,
Paragraph-anchored interior subset without claiming general block-grammar
restart:

- the parser consumes a byte-exact edit envelope distinct from the
  storage-aligned SourceFacts replacement page. Adjacent rapid edits compose
  into one authenticated envelope; distant edits fail closed;
- under production SourceFacts spacing, a middle edit in 4,096 Paragraphs
  parses 4,116 bytes over 168 physical lines in three transitions and transfers
  8 of 16,386 canonical records;
- the roughly 3.2 MB/100,000-Paragraph public path keeps foreground apply
  below 8 ms and reaches exact-current replacement in 20.994–26.977 ms native
  and 29.9–30.1 ms Chrome;
- exact point semantics remain correct at the first, edited middle, and last
  Paragraph, and the Dart, Flutter, and Chrome matrix is green;
- a separate 671,794-byte source inserts ATX Heading and fenced code between
  4,096 Paragraphs. The ATX-content edit and the following fence-body edit are
  each bracketed by ordinary Paragraph checkpoints, stay within the 64 KiB
  crop cap, enter `ParsingOrdinaryExact`, publish successive
  `ExactBaseDelta` revisions, and remain exact on native and Chrome;
- removing the closing fence destroys convergence and therefore takes the
  definitive exact-clean fallback; definitions, typed unsupported leaves,
  missing anchors, and boundary edits were fallback cases at this snapshot.
  The later BOF and EOF closures below supersede the definition-free boundary
  limitations; and
- the marker-free mixed Flutter checkpoint retains the same
  `EditableTextState` and platform input client while moving from ATX content
  to the literal fenced-code body and editing both.

### 2026-07-28 reference-frozen ordinary restart closure

Reference records still own absolute byte and UTF-16 ranges, so an exact
unchanged suffix alone cannot authorize reuse after a length-changing edit.
The clean parser now identifies the last definition-bearing leaf, discards
every ordinary checkpoint at or before it, and binds every survivor to the
exact frozen definition count. A crop strictly after that prefix can therefore
reuse the unchanged References root; accepting a new definition or editing the
prefix fails closed.

The endpoint fixture contains 2,048 definitions followed by 2,048 Paragraphs.
A length-changing middle Paragraph edit enters `ParsingOrdinaryExact`,
publishes `ExactBaseDelta`, transfers at most 64 records, retains all 2,048
References, and preserves exact first, edited-middle, and shifted-last
structural queries. The target retains source-bound ordinary checkpoints with
the same frozen count, and the shared public Dart/native/Chrome fixture proves
a second edit reaches exact-current revision 3. In contrast, the dense
8,192-Paragraph late-definition regression still proves that a
length-changing edit before the last definition takes a fresh `FullSnapshot`.
This closes local work after a frozen definition prefix; general definition
editing/restart and reference-link UI remain open.

### 2026-07-28 segmented BOF ordinary restart closure

This closes the definition-free first-block boundary without generalizing it to
definition-bearing BOF; the complementary EOF authority is recorded below:

- a 4,096-Paragraph blank-separated fixture applies a length-changing edit in
  its first block. The endpoint selects the exact BOF-to-ordinary convergence
  window, enters `ParsingOrdinaryExact`, and publishes `ExactBaseDelta`;
- the crop starts at byte zero, stays within the 64 KiB cap, reuses a
  document-sized suffix plus nonzero downstream restart checkpoints, and
  retains those target checkpoints as authority for the next edit;
- publication transfers at most 64 canonical records and emits a nonempty
  block replacement of at most 64 records. Independent-host queries preserve
  exact first, middle, and shifted-last geometry;
- the shared public Dart/native/Chrome fixture applies the same first-block
  length change and a second local edit, reaching exact-current revision 3;
  and
- separate parser-level split and merge cases insert or delete a blank
  boundary and prove that the converged checkpoint's block ordinal shifts by
  the exact topology delta.

A BOF crop in a definition-bearing document, a crop over 64 KiB, or lost
convergence still fails clean to definitive parsing. The EOF closure below
supersedes the former final-block limitation.

### 2026-07-28 segmented EOF ordinary restart closure

This closes the complementary final-block boundary while retaining exact
topology authority:

- ordinary restart collections now authenticate the exact top-level block
  count. The old segmented flag is derived only as `count > 1`; it is not
  sufficient authority for a boundary crop;
- a 4,096-Paragraph blank-separated fixture applies a length-changing edit in
  its final block. The endpoint selects the exact ordinary-to-EOF window,
  enters `ParsingOrdinaryExact`, and publishes `ExactBaseDelta`;
- the crop retains a document-sized authenticated prefix and nonzero prefix
  checkpoints. Because there is no source beyond EOF, it correctly mints zero
  fresh crop checkpoints and reuses zero suffix checkpoints;
- publication transfers at most 64 canonical records and emits a nonempty
  block replacement of at most 64 records. Independent-host queries preserve
  exact first, middle, and final geometry;
- the shared public Dart/native/Chrome fixture applies the same final-block
  length change and a second final-block edit, reaching exact-current revision
  3;
- BOF and EOF parser split/merge cases insert or delete a blank boundary and
  prove exact +2/-2 changes to the authenticated top-level count and relevant
  convergence/restart ordinal; and
- a frozen definition prefix remains safe for an ordinary EOF crop because
  every definition remains in the authenticated unchanged prefix.

A new tail definition, typed `Unsupported` tail, over-cap edit, or lost
convergence fails clean to definitive parsing. Definition-bearing BOF and
general definition editing remain open.

### 2026-07-28 cache, exact-clean block splice, and ATX closure

This slice closes three previously open mechanics without closing Checkpoint C:

- a bounded Dart cache retains at most 128 current-revision leaves and 2,048
  inline fact records, and invalidates atomically on edit, recovery, close, or
  terminal fault;
- authenticated exact-base SourceFacts reuse, blank-separated Paragraph
  restart/convergence, and the packed block-page cut/splice path are complete.
  The later mixed receipt above supersedes the old ATX/fence limitation for
  definition-free, Paragraph-anchored interior edits within 64 KiB;
- ATX Heading crosses parser, publication, independent host, public Dart,
  native and Chrome parity, real Flutter, and the runnable demo. Its exact
  level, opener, content, optional closer, indentation/BOM validation, EOL,
  CRLF, inline content facts, and live recertification are covered; and
- that dated slice's earlier receipt counts and asset identity are superseded by
  the Setext closure receipt below.

At that dated snapshot, virtualized visible-set materialization and layout, the
standalone 100,000-reference product gate, broader grammar, floor-device
evidence, and launch were unclaimed. The 2026-07-29 closure above supersedes
only the standalone 100,000-reference item.

### 2026-07-28 Setext production-vertical closure

This slice completes Setext through the current selected-leaf and local-delta
seams without passing Checkpoint C:

- exact-clean parsing emits `SetextHeading` with source and UTF-16 content,
  underline marker, underline EOL, H1/H2 level, indentation, and reference-count
  geometry. The inline content range excludes exactly the terminal content-line
  EOL and preserves preceding line endings as softbreaks;
- publication encodes Setext as structured block role variant 5. The independent
  host validates and translates it, while the public Dart surface reports the
  generic Heading kind and retains exact Setext-specific geometry;
- native, Chrome, real Flutter, and the runnable demo render parser-authored
  heading typography while hiding the underline and certified inline markers;
- a 4,096-block definition-free fixture transitions one middle block
  Paragraph→H1→H2→Paragraph. Every phase enters `ParsingOrdinaryExact`,
  publishes `ExactBaseDelta`, transfers and replaces at most 64 records,
  preserves exact first/middle/last queries, and leaves exact next-revision
  authority; and
- a same-block Paragraph→Setext promotion whose prior content exceeds 4 KiB
  rejects the narrow crop and succeeds through definitive exact-clean
  publication, proving the lane does not reuse a checkpoint invalidated by the
  new underline.

These dated Setext counts are superseded by the thematic-break closure receipt
below.

At that dated snapshot, virtualized visible-set materialization/layout, broader
grammar, the standalone 100,000-reference product gate, floor-device evidence,
Checkpoint C, and launch were open. The 2026-07-29 closure above supersedes
only the standalone 100,000-reference item.

### 2026-07-28 thematic-break production-vertical closure

This slice completes one atomic non-text block through the current clean,
incremental, public-runtime, Flutter, and demo seams without passing Checkpoint
C:

- exact-clean parsing emits `ThematicBreak` with exact marker kind/count,
  opening indent, BOF-BOM flag, marker envelope, line ending, source, and
  UTF-16 geometry. CommonMark precedence cases and `*`, `-`, and `_` markers
  remain parser-owned;
- publication encodes the atom as structured block role variant 6. Green and
  Projection retain the canonical source span, while visible/projected spans
  are empty and projection run count is zero. The independent host and public
  Dart decoder reject contradictory facts or any fake visible run;
- a definition-free 4,096-block fixture promotes and demotes one middle
  Paragraph as ThematicBreak inside one crop bounded to 16 KiB, 512 physical
  lines, and 4,096 parser transitions. Its packed splice deletes and replaces
  at most 64 records and retains exact next-revision authority;
- the shared public fixture proves exact atomic facts and the live
  Paragraph→ThematicBreak→Paragraph revision sequence on native and Chrome;
- managed Flutter collapses the empty input island to the affinity-selected
  boundary, paints a one-pixel semantic divider, synchronously revokes stale
  semantic actions, and maps Backspace and Delete to whole-canonical-atom
  deletion while retaining the same `EditableTextState` and platform input
  client; and
- the runnable Worker/Wasm lab exposes the exact `  * * * \r\n` canonical
  source separately from the empty editable projection and exercises both
  deletion keys.

At that dated snapshot, the full gates were green: 213 bridge tests passed with
one deliberate ignored scale receipt, the root Dart v3 aggregate passed 282,
Flutter v3 passed 48, shared thematic semantic parity passed on native and
Chrome, and the full Web adapter CI lane passed. The rebuilt Worker/Wasm asset
version was
`3dee517743e44167-a77d33ff29128daf-fe9ef141863df4eb`, with Wasm SHA-256
`3dee517743e44167f1be316cbd526138f7056df55310b49819ad68b0f1fc8cb8`.

At that dated snapshot, this did not claim lists, quotes, indented code, HTML,
tables, virtualized visible-set materialization/layout, the integrated
100,000-reference product gate, floor-device evidence, Checkpoint C, or launch
readiness. The 2026-07-29 closure above supersedes only the integrated
100,000-reference item and asset identity.

### 2026-07-28 selected-Paragraph hot-inline and Flutter update

This slice closes one demanded Paragraph leaf through the production seams; it
does not claim simultaneous viewport materialization or close Checkpoint C:

- the parser resolves the exact published Paragraph fence at a demanded point,
  runs the resumable inline projection job, and publishes strong, emphasis, and
  code facts as a revision-bound sibling sidecar;
- the independent host installs one latest sidecar and joins it only to the
  matching block ordinal, physical range, visible range, and source authority.
  Moving middle → tail → first does not leak facts to equal-looking neighbors;
- host close now latches a legal `active + retiring predecessor` state, hides
  queries, drains the predecessor, and then drains the active owner. The
  focused regression failed before this serialization fix and passes after it;
- Dart applies the 8 KiB whole-inline-envelope policy to
  `projection.projectedSource`, not the physical leaf containing leading
  definitions. A 1,024-definition native/Web parity fixture proves the
  >8 KiB physical leaf remains eligible for its eight-byte visible tail;
- a projected Paragraph above 8 KiB remains exact and editable through a
  scalar/CRLF-safe bounded source-visible island without issuing an unusable
  whole-leaf demand;
- the real Flutter binding and packaged Worker/Wasm lab move one stable
  `EditableText` and platform input client among first, middle, and tail
  Paragraphs. The selected leaf renders without delimiters, canonical
  instrumentation retains them, edits recertify exactly, and close completes
  after all three sidecar replacements; and
- this dated slice's earlier receipt counts are superseded by the closure
  receipt above.

### 2026-07-28 fenced-code Structured and marker-free Flutter update

This slice closes the first representative Structured block through the real
editor seams; it does not close Checkpoint C:

- the exact-clean controller recognizes backtick and tilde fences with a
  constant-state opener/closer scanner, authoritative closed and unclosed EOF
  outcomes, exact UTF-8/UTF-16 geometry, and literal body semantics. Fixed
  cases plus 20,000 randomized samples match the pinned Comrak donor;
- the generic `Structured` block entry carries fixed Green and Projection
  records through the packed measured sequence. Publication, an independent
  Rust host, the bounded public Dart decoder, native FFI, and rebuilt Wasm
  validate matching whole-block/body facts and reject tampering;
- the managed Flutter coordinator hands the stable `EditableText` to the exact
  body when caret/composition is inside it. The opener, info string, and closer
  never enter normal body display; `**`, `_`, and backticks in code remain
  literal because no inline grammar lane is invoked;
- a body larger than 8,192 UTF-16 units retains or selects a scalar/CRLF-safe
  caret-local shard inside the parser-authorized body. The planner cannot widen
  into fence syntax or neighboring blocks;
- focused Flutter receipts cover closed body edits, unclosed authority,
  Markdown-looking literals, closer removal/restoration with the same
  `EditableTextState` and platform client, a 25K-unit middle shard, and an
  initially syntax-crossing large island; and
- the packaged Chrome Worker/Wasm lab opens the fenced-code fixture, displays
  only its monospace body, applies a real delta, reaches exact-current
  structure, and closes truthfully.

The cache, Paragraph-anchored mixed interior restart, and block-page splice were
closed by later sections. At this dated snapshot, virtualized general
multi-block editing, restart/convergence beyond the definition-free
interior/BOF and reference-frozen bounded subsets, the integrated
100,000-reference Web product loop, broader grammar, and floor-device
edit-to-paint evidence were open. The 2026-07-29 closure above supersedes only
the integrated product-loop item.

### 2026-07-27 multi-block structural publication/query update

This slice closes the clean multi-block structural path, not Checkpoint C or a
production editor claim:

- the exact-clean parser total-partitions nonempty source into Paragraph,
  Blank, DefinitionsOnly, and typed Unsupported leaves with exact byte and
  UTF-16 ranges. The no-flat-cap receipt produces 259 leaves—130 Paragraph
  plus 129 Blank—and the split/merge fixture compares merged `one\ntwo`
  with split `one\n\ntwo` without losing coverage;
- the persistent block sequence packs at most 64 semantic entries per page.
  Exhaustive point/affinity queries at 1, 64, 65, 128, 129, and 260 entries
  meet the descriptor's tight header bound: zero for an empty tree, two for a
  height-one tree, and `3h + 1` for height `h >= 2`. A separate 600-entry
  receipt packs into at most ten pages and inspects one bounded page after the
  measured-tree descent;
- independent-host receipts return exact Paragraph, Blank, DefinitionsOnly,
  and Unsupported viewports, preserve byte/UTF-16 boundary affinity including
  Unicode and CRLF, retain the empty-document viewport, reject insufficient
  preflight budgets before writing output, and keep the legacy flat
  Green/Projection query route unchanged;
- the same public-only test passes through the native runtime and real Chrome:
  `p\n\n**q**` queries as Paragraph/Blank/Paragraph at the exact boundaries,
  deleting the blank byte produces `p\n**q**`, revision 2 becomes exact
  current, and the merged Paragraph covers the full `0..7` source;
- focused Flutter receipts preserve the caret when the selected island becomes
  DefinitionsOnly, keep the same `TextEditingController`,
  `EditableTextState`, focus, and platform input client across exact leaf
  handoff, and rebase a same-range authoritative refresh before the next
  projected edit; and
- the root-package and Flutter-package Wasm assets were rebuilt from the
  current Rust inputs, their buildinfo freshness checks pass, and both binaries
  are byte-identical at SHA-256
  `620ab45aa45041bb1f11266cc99aef0733db095ee2f4c37ca47b2c39e51b34b9`.

At this dated snapshot, per-leaf inline facts and block-sequence splice/reuse
were open. The 2026-07-28 closures above supersede those items plus
Paragraph-anchored mixed interior restart, fenced code, and ATX Heading.
Marker-free virtualized multi-block UX, restart/convergence beyond the
definition-free and reference-frozen at-most-64-KiB interior subsets, the
standalone 100,000-reference product gate, broader grammar, and floor-device
latency/frame/memory gates were open at that snapshot. The 2026-07-29 closure
above supersedes only the integrated product-gate item. This historical
evidence neither passed Checkpoint C nor claimed production or launch readiness.

### 2026-07-22 native M1.1 vertical update

The native join exposed integration defects, not a contradictory architecture:

- a fresh endpoint establishes lifecycle but does not contain source, so every
  document open now sends an explicit seed, including the empty `0..0` seed;
- candidate scheduling was joined to the same one-event-credit loop as source
  work, while schema-2 host results remain a distinct lane from schema-3 event
  receipts;
- publication size negotiation now treats `BufferTooSmall` as the bounded
  retry contract rather than an internal fault;
- Green and Projection records have named exact 80/56-byte contracts, and the
  Dart host proves there is no trailing record with an explicit end probe;
- host removal or emergency destruction is proven before a finalizer token is
  released, and Dart scratch storage is one finalizer-owned aligned arena; and
- the real public path now proves `open -> source -> parser -> publication ->
  host install -> query -> edit -> newer install -> query -> close`.

Those findings harden the selected ownership boundaries. They do not add a
second parser, document-sized bridge payload, mutable shared AST, or
presentation-side grammar. The former root-ordinal adapter has since been
removed: native and Web now call one Rust-authored bounded point-query ABI with
exact source/version/range receipts, typed resource gaps, fixed 64 KiB scratch,
and no exported raw role enumeration.

### 2026-07-22 Web scheduling and Checkpoint A update

The first visible browser checkpoint rejected an initially green-looking
vertical. A 1 MiB source certified and synchronized quickly but did not install
structure after 90 seconds. Direct release work was only about 100 ms: the
exact-clean driver reported individual source bytes as endpoint transitions,
and the Worker scheduled only 32 transitions per `setTimeout(0)` turn. The
result was about 65,537 clamped browser timers, not a parser, host, Dart, or
event-credit deadlock.

Two production corrections now compose:

- one parse transition spends at most one 4 KiB aggregate accounted-work
  quantum across physical-line discovery, explicitly charged admission/commit
  boundaries, and lexical/classifier work. Counted discovery handles partial
  CR, zero-byte EOF, finish, and cancel without hiding state work;
- the external Worker begins with one hard 32 KiB source grant, calls the
  candidate ABI in fuel-64 microgrants, stops at a hard aggregate grant of
  4,096 transitions or a four-millisecond target, and stops immediately for an
  event or zero progress. Receipt transitions may never exceed their grant;
  the hard quota remains the bound when one atomic call overshoots the clock;
  and
- mid-discovery and mid-lexical supersession publish no partial candidate,
  retain installed source authority, reset certification, and leave the
  cancelled candidate at zero progress/no event.

The direct release parser now reports 513/557 transitions for 1 MiB giant and
80-byte-line sources, versus 5,121/5,569 at 10 MiB; the results and total
transitions are invariant under fuel 1 and 32. Actual Chrome reaches current
structure in about 0.25-0.36 seconds at 1 MiB and 2.7-3.3 seconds at 10 MiB for
those ordinary shapes. Active 10 MiB close is about 6.5 ms, and rapid edits
install only the newest revision. Durable browser coverage includes giant,
ordinary-line, and newline-dense 1 MiB sources, a coarse three-second ordinary
line scheduler-regression ceiling, active close, latest-wins supersession, and
the packaged Flutter lab's 1 MiB typing/convergence/truthful-close interaction.

The packet-only ABI-v2 combined lane also crosses a 1,788,903-byte source with
100,000 reference definitions. Its 3,307,535 candidate transitions exposed the
old 8/64 policy as excessive orchestration: roughly 413,000 Worker-to-Wasm calls
and at least 51,000 clamped timer turns exceeded 60 seconds. The retained
64/4,096/four-millisecond calibration completes with a roughly 13.6 ms maximum
caller heartbeat gap and 0.5 ms foreground edit. At that dated snapshot, exact
clean convergence still took roughly 17 seconds cold and after the edit for the
reference-specific witness. The current 100,000-leading-reference path instead
replaces in roughly 15.8 ms native and 14.5 ms Chrome after roughly 1.6-second
native and 1.5-second Chrome cold opens; neither result describes the separate
blank-separated Paragraph route.

The adversarial `x\n` shape remains explicit rather than optimized away for the
narrow M1.1 grammar. Its 10 MiB/five-million-line witness takes about 3.4
seconds in the direct release parser and 13 seconds through the current Web
cold path, but remains off-main, bounded, and promptly cancellable. Full
grammar, broader-grammar restart, floor-device maximum-slice calibration, and
100 MiB source-ingress/directory work remain open.

The selected reference restart design is now an executable isolated mechanism:
one global persistent occurrence sequence plus an exact-label directory whose
leaves own per-label persistent occurrence sequences. A committed checkpoint
authenticates each
label sequence's prefix rank before the active Paragraph. For one authenticated
contiguous restart-to-convergence replacement, old changed occurrences are
deleted forward at the fixed prefix rank and new occurrences are inserted in
reverse at that same rank. The first per-label element is the winner; deleting
it therefore promotes an untouched suffix occurrence without suffix
enumeration or rebasing. Arbitrary move/reorder is not authorized. Published
occurrence descriptors own exact interned-label and persistent cooked
destination/title blob roots. The parser-authenticated source/projection
ranges that selected those values are transaction-only witnesses: an
authenticated random-access projection cursor replays them through the pinned
Comrak-correspondent cleaner and blob writer before terminal Paragraph
mutation. The old projection can retire once the manifest join commits, while
unchanged suffix occurrences reuse immutable blob roots by identity. Crop
exposes no public stable leaf identity, and finite scalar lineage is not
treated as one. Durable source navigation remains a separate stable-anchor or
lazy-coordinate-index gate. The decisive receipt replaces fixture numeric
labels and the changed-interval `Vec` with committed-interner adoption, one
persistent replacement spool, bounded reverse traversal, and committed
exact-label winner lookup. It covers insertion, winner deletion/promotion,
relabel/value change, duplicate order, cooked queries, donor retirement,
bounded suspension/reclaim, and zero suffix enumeration.

The source-authority decision is closed independently of those parser results.
The immutable Crop root, revision, dimensions, and atomic transition lineage
are the exact replica authority. A source ACK proves only generation-bound
transport/install. The one SourceFacts index derives contiguous source facts
and a versioned rolling-128 convergence/corruption guard from that exact lease;
the guard is not equality or reuse authority. M1.1 builds SourceFacts with a
bounded scan in the native isolate or Web Worker and mints `CertifiedSource`
only with full coverage, the configured grammar/profile, and clean EOF. M1.2
now persistently splices that same index from an authenticated exact certified
base. The selected design adds neither a Crop hash fork nor a parallel summary
tree co-mutated with Crop. Schema 3 transports exact source stamps/intent
chains, canonical
SourceFacts pages and terminal proof, installed dimensions, full publication
binding, and causal host-poll tickets without turning transport receipt into
certification. The bounded page/completion lane now promotes only the exact same
revision. The generation-checked registry, FFI ABI, long-lived native isolate,
automatic Dart executor, managed source runtime, close/recovery races, and
truthful disposal are executable. Native parser scheduling, credited
publication, independent-host installation, public structural query, edit
replacement, and close are executable on native and Web. The external classic
Worker owns the parser endpoint while a separate main-context Wasm instance
owns the independent Web host. Terminal Web parser fault-to-restart-to-reseed-
to-query recovery is executable. Exact publish-archive consumers are green on
macOS arm64 and Chrome; the Linux branch is implemented but unexecuted.
Physical JavaScript Worker death and broader crash/GC/floor-device stress
remain open.

M1.2 authority stratification is executable for the completed reuse paths.
Reusable canonical role pages and content digests exclude ephemeral
publication, source-root, revision, and generation authority. Fresh role-root
wrappers bind reused pages to the target `CertifiedSource`, and a fresh
manifest wrapper binds all five roles without retaining an old manifest
ancestor.

Canonical SourceFacts pages store relative page-local checkpoints and
associative byte/UTF-16/line/split-CRLF/rolling-hash summaries. Authenticated
exact-base edits splice those pages through the persistent measured
representation and derive absolute coordinates through bounded prefix work.
The exact-clean structural path likewise cuts and splices packed block pages.

These storage and publication mechanics now compose with authenticated
incremental parsing for definition-free top-level interior edits bracketed by
ordinary Paragraph checkpoints, definition-free first/final-Paragraph boundary
crops, and ordinary Paragraph edits strictly after the last definition-bearing
leaf. In the latter case each checkpoint carries the exact frozen definition
count. Each restart collection also authenticates the exact top-level block
count; segmented state is merely derived from it. The parser's exact edit
envelope stays distinct from the storage-aligned SourceFacts page, and
unchanged regions retain authority behind the same host protocol. Paragraph,
ATX-content, Setext Paragraph↔H1↔H2 transitions, and fenced-code-body edits
within the 64 KiB definition-free crop cap use this path; the reference-frozen
and BOF/EOF receipts cover length-changing Paragraph edits. Edits to or before definitions,
definition-bearing BOF, typed unsupported leaves, missing anchors, new tail
definitions/unsupported constructs, over-cap crops, lost convergence,
containers, lists, references, and the remaining block grammar still take the
fail-closed definitive path.

This closes the architecture-selection prototype, not production or launch.
The parser-finalizer/CandidateWriter join consumes the sealed projection
session, materializes cooked values, performs both terminal Paragraph outcomes,
and publishes Green plus reference roots atomically. The production ownership
shape is still open: each adopted interner manifest retains its parent manifest
as a witness, which would keep an unbounded revision chain live. Production
must flatten that witness into candidate-owned exact roots plus bounded
lineage/adoption facts and extend the proven parent transaction across every
document root. Broader grammar, fault, crossed-authority, scale, parser-to-paint,
and device matrices remain production hardening.

### Prior detailed snapshot (2026-07-16)

The rows below preserve the detailed evidence lineage. Where a state or next
step differs, the current rows above supersede it.

| Claim | State on 2026-07-16 | Evidence recorded then | Missing proof recorded then |
| --- | --- | --- | --- |
| Worker source revisions can be cheap at 1/10/100 MiB | evidenced mechanism | `integrated_parser_slice/CROP_OWNED_ADAPTER_RESULTS.md` and v3 source-lineage receipts | integrate the selected one-current-root Dart source, Crop mirror, retirement, and Web Worker path |
| Persistent parser output need not retain a Crop root | evidenced mechanism | source-relative identity/lifetime gates and packed arena ownership tests | prove it in the direct parser-to-green transaction and retired-revision audit |
| Dart can preserve exact source spelling with compact local edits | host mechanism GO; device composition open | v3 source tests, lazy-bulk candidate/certified-worker gates, and one-current-root AOT/GC receipts across 10/100 MiB | integrate the selected source object, byte-bounded inverse log, bulk payload leases, bridge flow control, and floor-device combined-memory traces |
| Exact block parsing fits a maintainable Flark-owned value core | semantic, bounded-driver, ordinary-container, fenced-code, and Setext composition GO; Table composition open | one correspondent line machine passes the 1,322-document differential corpus, scratch-discard/resume, source-backed large raw literals, canonical byte/UTF-16 coverage, and fuelled deep-container transitions; the direct parser composes Document/Paragraph/Quote/List/Item/FencedCode through one typed port into `CandidateWriter`, with exact source partitions, typed facts, close-time folds, arbitrary ancestor ownership, acknowledgement-driven stack effects, and no parser/output ID crossing the seam; fenced close slices are derived solely by the writer's constant-size ledger metric fold from parser semantic marks; fresh Setext now consumes a private provisional Paragraph capability, rewrites the canonical packed Enter in place or through one bounded page repack, preserves primary identity and distant page/Program IDs, and retypes the source ledger only after storage acknowledgement; wrong/replayed authority and an injected post-storage failure fail poison-only; the full v3 crate passes 159 unit tests plus every integration target | prove the retained-base 10 MiB restart/convergence transaction; replace the 8 KiB recognition-line ceiling with segmented fuelled work; add reference/table/inline roots and floor-device composition evidence |
| Oversized block-significant input can remain exact and cancellable | ownership/mechanism GO; source-backed donor composition open | handwritten all-family witnesses; pinned generated ATX and trailing-context fence DFAs; 10 MiB real-Crop cursors with strictly forward physical requests; explicit donor transition stages; a complete typed owned-line ATX command/projection transaction; an index-derived line endpoint with zero BOF giant-line scanning; and an actor join that admits it only for the exact live build at an untouched recognition-line start | finish the ledger-owned raw-byte session and donor-owned forward ATX tail; move generated state behind the donor stage; add batched exact range replay and typed tab/NUL text recipes; then compose fuel, cancellation, direct commands, packed output, restart, and floor-device parser-to-paint receipts before lifting the 8 KiB hold |
| Exact inline parsing can remain one authority | algorithm seam plausible; production service open | Pulldown inline algorithms were extractable onto segmented value state; bounded lazy-cache/context gates exist; the first retained tree representation was rejected for dense memory | implement the selected bounded inline service over `SourceProjectionRun`, retain only compact facts/dependencies, and differential-test against CommonMark/GFM and donor peers |
| Reference edits can avoid global invalidation | evidenced mechanism | stable symbol/presence, winner, occurrence, dependency, and fan-out adoption gates | integrate these roots into the same candidate manifest and exercise real edit histories |
| Ordinary local edits converge and reuse suffix output by identity | restart selection, one-pass lineage bundle, storage-boundary roles, bounded direct-parser pause/resume, real resumable sequence split/retained-range/owned-splice, build-local exact green cuts, and the single reusable green working-prefix/tail forest GO; authoritative composite checkpoint HOLD | v3 proves balanced persistent sequence identity and non-copy arena ownership; `restart_composer_gate` proves disjoint continuation/prefix/binding state and typed recipes; restart selection and the storage-owned lineage bundle map restart, retained prefix, convergence, and tail in one frozen pass; the direct donor now captures only an acknowledged post-`FinishLine` open path, cursor, child folds, and deferred terminator/blank-gap state, reconstructs fresh node IDs, excludes source payload/positions, uses fallible depth-proportional reserves, and produces exactly the same suffix command stream across paragraph, list, blank-gap, fence, Setext, BOM, and EOF cases; its scale receipt is 120 bytes at depth 2 and 1,656 bytes at depth 66 with zero retained source bytes; the real `ArenaBuildSession` sequence jobs follow only AVL boundary paths, reuse preflighted split/join scratch, allocate at most one branch per poll, preserve exact leaf/aligned-subtree `ArenaId` values, and release a deleted range as one journal owner before joining prefix + optional replacement + suffix; all 171 delete ranges over 17 leaves, every insertion/replacement boundary, empty/full/aligned cases, wrong generations, saturation, forced join failure, and every-phase cancellation pass; the one-prefix forest preserves the same 12 leaf pages and AVL height across 2,000 reductions while 1,989 unchanged cuts allocate nothing; a forced packed-green leaf barrier mints distinct leaf/event cuts even when adjacent structural cuts have the same zero source metric, and extraction requires the matching live build session | pair the opaque parser pause with storage-authorized writer bindings, provisional normalization-group/fence state, ledger/composer/source continuations, one parser-authorized projection reset, and the exact adjacent leaf in one same-build composite entry; then derive cross-build restart authority from the committed manifest rather than reconstructing it from a source coordinate or final green alone |
| Candidate authority, edit admission, and cancellation are bounded | worker/arena authority, resumable packed journaling, AVL final reduction, and bounded retirement transfer GO; offer-input heap preflight, host disposal scheduling, and production writer binding open | `LiveDocumentStore` exclusively owns exact source/candidate/arena clocks; revision-zero generation-one admission, linear build-scoped IDs, prepared source+coordinator atomic edits, real partial-journal cancellation, and strictly fuelled cleanup; the seven-leaf falsifier is now a required regression rather than an exception: final bins fold through the existing height-aware join/rotation semantics as one explicit task per poll, and a recursive witness proves source order plus `abs(left.height - right.height) <= 1` for every leaf count 1..257 and 511/512/513/1,023/1,024/1,025 while suspending after every task; bin storage fallibly reserves the fixed 64-slot `u64` envelope before input, each join fallibly reserves the logical `max(left.height, right.height) + 4` task slots and exactly two value slots before execution, pushes guard those logical limits rather than allocator overcapacity, successful polls assert unchanged actual capacity, partial leaves preflight 4,096 payload bytes and 128 Program-owner slots, and branch payloads use fixed arrays; persistent-sequence tests pass 12/12 debug and 12/12 release, while the resumable serialized-green suite passes 6/6 debug and 6/6 release including fuel-one mid-rotation abort, generation isolation, sole-manifest commit, exact byte+UTF-16 binding, and a 12,000-event suspend-every-boundary run; `offer_event` remains separately and honestly accounted because it creates one fresh encoded-descriptor `Vec` per event and a second facts `Vec` for Enter-with-facts; `CROP_ROOT_DROP_RESULTS.md` measures direct final 100 MiB old-root destruction at 5.301 / 6.943 / 11.789 ms p50/p95/max, while one retained owner reduces assignment to 41 / 125 / 250 ns; the implemented slice transfers old or unpublished roots into an allocation-free four-owner FIFO with a 256 MiB pessimistic logical-byte bound, preflights before mutation, rejects saturation without changing clocks/candidate, and exposes only a borrowed non-owning live-document query view; 7/7 focused retirement tests pass debug/release and strict library Clippy is green | replace or fallibly preflight the per-event descriptor/facts buffers and calibrate their input-kernel budget; enforce a floor-device-calibrated small scalar-lineage cap; make the source-complete ledger seal the exclusive root-spec/manifest input; connect the writer to live candidate commit; run the full shared regression/Clippy/Wasm matrix; wire native disposer and Wasm post-response/idle drains, whole-document close retirement, and sustained-edit byte/backpressure gates on floor devices |
| Dense projection construction is bounded and locally reusable | flat Program codec/query mechanisms and reset-certified exact-cut design GO; composite drain and suffix adoption HOLD | typed Program v2 codec and 4 KiB scratch cap; the greedy 200,000-byte falsifier gives 0 reusable suffix pages, while the 7/7 real-`SourceStore` reset suite bounds repeated prefix edits to 2 groups/19 pages, a 33,768-byte cross-reset deletion to 1 group/8 pages, and Unicode to 1 group/4 pages; the current run-attached reset bit and 6/6 debug/release suite prove the older storage-only join, including Virtual behavior and cross-manifest rejection; the integrated audit now shows that a checkpoint-specific virtual-safe composer drain plus exact event/source cut is a strictly stronger reset authority, directly indexed by the already-required composite checkpoint and able to represent source-zero and zero-metric structural sides without retroactive page mutation or predecessor scan | implement `flush_for_line_boundary_checkpoint`, explicitly reject a pending right-biased Virtual before mutation, mint `CheckpointProjectionResetAtCut` only in the full parser/source/composer/green join, and prove exact suffix `CoverageId`/`ArenaId` splice; keep the run bit only as temporary standalone-storage compatibility until the indexed restart gate passes |
| Parser work cannot jank the UI | architectural invariant, not yet accepted | dedicated-worker/latest-wins model and bounded-kernel workstation receipts | full bridge backpressure plus real UI traces on floor native/web devices |
| Ordinary large documents remain fully live | physical representation plausible; logical representation not accepted | packed-only serialized green accounts 6,996,635 bytes for 200,002 blocks (34.98 B/block) and preserves source-order identity; full event history was rejected | finish the unified logical projection codec, large facts, range splice/boundary repacking, combined source+parser+presentation RSS, and parser-to-paint trace |
| Giant constructs fail honestly and recover | specified, not accepted | scheduling/degradation contract and isolated resumable scanners | integrate exact source-visible Unknown/degradation behavior and recovery histories |
| Donor maintenance is bounded without a runtime fork | direction selected; evidence incomplete | Flark-owned contracts isolate donor algorithms; pinned Comrak/Pulldown/cmark-gfm lanes exist as differential peers | complete provenance inventory, full corpus, mutation/fuzz lanes, and one source-changing donor intake rehearsal without adopting a donor runtime tree |
| The whole editor feels live | open | disconnected input-lease, layout, selection, parser, and presentation probes | composed parser-to-paint workload with IME, undo, paste, supersession, shaping, viewport motion, and backlog |

“Evidenced mechanism” means the isolated claim survived its probe. It does not
mean the composed editor inherits the property automatically.

The hidden arena-metadata copy has now been removed from the prototype rather
than waived. `PageArena` fallibly pre-reserves its complete slot-segment
descriptor directory and its strict active-build directory at admission. Page
slots grow in 64-entry segments, so one page allocation initializes at most 64
new `Slot` values (`64 * size_of::<Slot>()`, included in the storage receipt)
and moves zero old slots or descriptors. The default logical envelope is
1,048,576 slot IDs,
16 active builds, and 512 MiB of live encoded node storage; all three are
explicit `ArenaLimits`, and saturation is rejected before reference/page state
changes. Resumable build journals keep a separate 2,048-owner default and grow
in 16-entry segments behind 64-descriptor directory blocks; both boundaries
fallibly preflight and report zero prior owner entries/descriptors moved. The
non-yielding compatibility transaction has a distinct configurable 131,072
owner envelope solely to preserve the 65,536-page oracle/stress paths; it is
not a production recommendation, and its synchronous rollback remains barred
from the yielding writer. `bounded_arena_metadata` crosses three page-slot
segments, 65 journal segments/two journal-directory blocks, active-build and
storage limits, reuse, generation retirement, candidate cancellation, and
committed-root coexistence. The 512 MiB default is a safety envelope, not proof
that every 100 MiB root-role coexistence trace fits: worker-current,
acknowledged/offered roots, active candidate, allocator overhead, source, and
presentation memory still require floor-device calibration and an explicit
product fallback policy. Per-event descriptor/facts buffers also remain a
separate allocation-preflight HOLD. The full crate is green in debug and
release across all targets and in the explicit Wasm check; the shared strict
all-target, all-feature Clippy invocation is also green.

The same distinction now applies to structural paths and checkpoint cuts.
`CandidateSourceLedger` still grows a flat open-binding `Vec`, while the
general `GreenStreamCursor::seek` reconstructs an allocation-owning open path
and may walk old structural events to discover it. The new storage-only source
boundary descent avoids that work, but it proves only an adjacent coverage
boundary. A byte boundary is not a unique green-sequence boundary: zero-metric
`Enter`/`Exit` events can lie between `After(previous coverage)` and
`Before(next coverage)`. The production restart index must therefore retain
and revalidate the exact event-side cut together with persistent parser,
semantic-prefix, open-binding, and projection continuation state. Neither a
source offset nor a freshly inferred coverage side may mint adoption
authority. Production acceptance also requires a segmented or persistent
parser path whose growth does not copy prior depth; pathological one-line
nesting remains fuelled and source-visible rather than entering one atomic
`Vec::push`/path-recovery kernel.

The capability slices are proof instruments, not permission for a distributed
production protocol. The integrated lane passes only if one candidate-owned
writer contains the source ledger, projection composer, checkpoint builder,
packed-green builder, and poison/commit state. Parser algorithms may call a
small typed action port, but they may not manually shuttle replayable IDs or
synchronize parallel generation/source cursors between subsystems. At-most-one
admission booleans and copyable completion receipts are insufficient: the sole
manifest commit must consume the non-cloneable source-composition completion
and the sole arena root together. If the current mechanisms cannot collapse
behind that writer without duplicate state or a large cross-module protocol,
the composition gate fails even when every isolated test remains green.
That writer must also drain and sink the preceding projection envelope before
it journals an intervening `Exit`, `Enter`, or other structural action. Merely
noticing the changed owner key when the next source piece arrives preserves
metrics but can serialize coverage on the wrong side of a zero-metric event.
Structural flush, storage-reset flush, semantic-envelope finish, and EOF are
therefore distinct writer transitions, all over the same composer state.

## Architecture acceptance gates

### 1. Semantic authority

The selected engine must provide:

- exact CommonMark 0.31.2 and the pinned Flark GFM profile for all normative
  fixtures, with every difference adjudicated rather than majority-voted;
- clean-versus-incremental equality at every revision of typing, deletion,
  paste, undo, reference-precedence, list-tightness, setext/table, fence, HTML,
  CRLF/lone-CR, Unicode, and incomplete-syntax histories;
- exact total source coverage and logical-to-physical origins; and
- no stock full-document parser or predictive grammar fallback on the edit
  path.

The 189 block fixtures are the first milestone, not sufficient final coverage.
After they pass, run all 652 CommonMark and 670 GFM fixtures plus the package's
existing behavioral corpus and generated edit histories.

### 2. Incremental ownership

A prefix edit followed by semantic convergence must prove:

- unchanged suffix output pages are reused by identity;
- work is proportional to changed pages plus persistent-tree depth;
- no suffix fact is rewritten to shift an absolute offset;
- no output, checkpoint, origin, reference, or dependency record retains the
  retired Crop root; and
- current absolute UTF-8/UTF-16 ranges reconstructed by prefix sums match a
  clean parse.

Stable projection resets are source-relative edges in that same persistent
run sequence. They are not a document-wide absolute-offset table. An edit may
map the bounded restart/convergence/tail capabilities needed for its splice;
it may not visit or remap every reset or Program page in the unchanged suffix.
The latter would make the apparently local design O(document groups).

Reference winners and list tightness use aggregate/property indirection. A
value-only definition edit must change one symbol without consumer reparse. A
defined/undefined transition reparses only dependent inline leaves.

### 3. Scheduling and cancellation

Every grammar operation must be one of:

- a measured, preflighted atomic kernel whose complete input/output/allocation
  cost was granted before entry; or
- an explicit resumable state machine with byte/transition/output accounting.

Cancellation before and after atomic kernels is honest; cancellation inside
one is not claimed. Latest-wins queue collapse, stale-result rejection, and
root retirement must be exercised under sustained edits. Large line and leaf
thresholds are calibrated independently for urgent, worker-exact, and
pathological source-visible work.

Final Crop-root destruction is explicitly outside the edit-publication kernel.
One retained owner reduced the measured 100 MiB replacement assignment from
5.301 / 6.943 / 11.789 ms p50/p95/max to 41 / 125 / 250 ns while moving the
same destruction cost into a retirement lane. The v3 actor now transfers that
owner through a four-slot, 256 MiB pessimistic logical-byte FIFO; both bounds
are checked before preparation, and post-prepare rejection retires the
unpublished next root rather than destroying it in admission. Its public query
view borrows the actor and owns no Crop `Arc`, so external observation cannot
become the accidental last owner. Platform disposal scheduling, actor close,
and sustained floor-device backpressure remain acceptance gates. Scalar
lineage instead remains a strict recent-history mechanism: a fully divergent
1,000-record snapshot drop measured 0.039 / 0.043 / 0.052 ms; a 10,000-record
snapshot had a 4.169 ms host maximum. Expiry performs an exact clean restart,
so the simpler calibrated cap is selected unless floor-device evidence
requires the existing arena reclaimer.

### 4. Product liveness

Provisional targets for the floor supported device/browser are:

- source, caret, selection, and IME composition are visible at the next paint;
- editor synchronous work remains below 4 ms at p99 during ordinary typing;
- ordinary visible-leaf authoritative facts arrive within one 60 Hz frame at
  p95 and within 50 ms at p99 while typing continuously;
- parser work causes zero missed UI frames and never grows an unbounded stale
  queue; and
- a local edit in a warm 10 MiB ordinary document has the same interaction
  behavior as a small document.

These are acceptance targets, not claims from workstation parser-only timing.
If device evidence shows a different perceptual boundary, revise the numbers
explicitly rather than silently loosening a test.

### 5. Large and pathological inputs

Required independent subprocess receipts include:

- 1, 10, and 100 MiB ordinary many-block documents;
- dense facts, deeply nested containers, million-line input, and high reference
  dependency fan-out;
- a giant paragraph, fence line, HTML terminator, table row, list prefix, and
  reference definition; and
- large paste, whole-document replace, close/reopen, undo, and rapid
  supersession.

Large ordinary documents remain structured and locally live. A giant inline
leaf may remain exact-source-visible, but oversized block syntax must still
produce exact downstream continuation state. Memory must be measured through
allocator traffic and external RSS/linear-memory growth, not only candidate
counters.

### 6. Renderer and layout

Parser acceptance is incomplete until the presentation layer proves:

- changed and visible facts can be materialized without a document-scale Dart
  object graph;
- block/fact deltas rebuild only affected viewport/layout shards;
- styling transitions do not move the caret or corrupt selection/IME ranges;
- Unicode shaping, bidi, grapheme clusters, wrapping, links, code, tables, and
  cross-block gestures match whole-layout oracles; and
- oversized layout regions use a bounded source-visible treatment.

### 7. Native, Wasm, and donor-maintenance parity

The same safe Rust semantic core and fixture corpus run on native and Wasm.
Platform-specific scheduling may differ, but semantic facts may not. Donor
code is admitted only as a localized algorithm inside Flark-owned input,
continuation, output, scheduling, and lifetime contracts. Maintenance requires:

- exact CommonMark/GFM/profile pins and function-level donor provenance;
- unmodified Comrak, Pulldown, and cmark-gfm differential lanes where they are
  relevant, with every disagreement adjudicated against the normative profile;
- generated-scanner source hashes and deterministic regeneration where code is
  mechanically derived;
- complete unit/doctest, corpus, formatting, strict Clippy, native, and Wasm
  lanes for the owned core; and
- at least one source-changing donor-intake rehearsal before launch, proving
  that an upstream change does not require adopting the donor's runtime tree or
  state model.

## Stop and reopen conditions

Reopen the Flark-owned correspondent-core decision if any of these occur:

- exact block semantics require retaining or cloning the donor arena/tree as
  the persistent representation;
- clean exactness requires a second block parse or document-scale finalization
  after every edit;
- the exact grammar core becomes materially larger or less reviewable than the
  donor algorithms it replaces, or provenance becomes untraceable;
- suffix output cannot be reused without absolute rebasing or old-root
  retention; or
- ordinary real-world lines frequently hit the pathological source-visible
  tier.

Reconsider the selected inline algorithm donor or service granularity if dense
memory cannot fit the product ceiling, a donor intake requires its runtime
tree/state model, or floor-device atomic tails cannot meet the worker deadline
at a useful visible-leaf size. Do not recover by adding a second parser.

## Next decision-bearing sequence

The architecture-selection question is closed at prototype level. Exact block
control meets the candidate writer/source/projection/packed-green substrate;
retained Setext normalization, bounded active-Paragraph replay, streamed
reference restart, both terminal reference outcomes, atomic Green/reference
ownership, authenticated Table replay, and the bounded Dart foreground island
are executable. **Recommendation: continue with this architecture.** The
remaining sequence is production implementation and product composition, not a
return to the donor-runtime or dual-parser bakeoff. Reopen the parser choice
only if the full production join requires a competing grammar authority, a
flattened Paragraph/document snapshot, document-sized transaction state, or
non-atomic publication.

1. **Retain the completed native and Web source/ownership boundaries.** The production
   crate has an arena-owned rollback journal, count-and-byte source retirement
   admission, starvation-free source/arena and build/node reclamation, and
   explicit fuelled close. The runtime and owned capabilities are `Send + !Sync`
   with sequential cross-thread and external-serialization receipts. Schema 3
   preserves exact source lineage and canonical page/completion certification.
   The generation-checked registry, FFI ABI, long-lived isolate, finalizer
   tokens, automatic executor, managed Dart runtime, immediate/credited close,
   deferred-command invalidation, retired-frame recovery, and truthful disposal
   now cross both production boundaries. Retain deterministic startup-timeout
   reclamation, reserved native recovery headroom, source-first bounded Worker
   turns, strict-CSP startup, proof-based close, and separate Worker-parser and
   main-context-host ownership as regression gates. ACK proves install only;
   SourceFacts derives rolling-128 off the Dart caller isolate. Rust `Drop`
   still cannot yield, so normal close/drain remains structural and emergency
   release remains unmetered.
2. **Retain completed archive consumers and close remaining combined M1.1
   gates.** Exact
   root and Flutter-adapter publish tarballs now resolve through an isolated
   hosted cache. They include and compare the root/adapter Wasm buildinfo beside
   the exact Wasm and Worker bytes, run and compile the external Dart consumer
   to JavaScript, relocate the native asset/AOT bundle on macOS arm64, build the
   external Flutter Web consumer, and open, edit, query, and close the packaged
   Worker/Wasm assets in real Chrome. Package, cache, and generated outputs
   contain no absolute checkout path.
   Run the implemented AOT branch on Linux CI. Retain those receipts plus the
   real Web terminal-fault-to-recovery-to-reseed-to-query receipt, completed
   exact-clean parser, credited five-role publication, independent hosts,
   persistent multi-block structural roles, bounded point query, latest-wins
   edit replacement, cancellation, and truthful close in one combined
   acceptance lane. Admission remains a post-parse restriction over the exact
   block controller's typed result, never a second safe-Paragraph classifier.
3. **Extend the bounded query and directory shapes for scale.** Keep exact
   source position, affinity, requested revision, and all budgets in the
   native/Web query contract. Extend the existing Rust-authored point query
   with self-describing viewport/range families or native-authored gaps without
   document-scale validation or Dart-side grammar. Build the multi-level role
   directory required by the 100 MiB gate and prove that query latency and
   copied bytes depend on the requested viewport rather than document size.
4. **Extend authenticated parser restart and block control beyond the admitted
   bounded subsets.** Exact-base SourceFacts reuse, the byte-exact parser edit
   envelope, ordinary Paragraph restart/convergence, and persistent packed
   block-page splice are complete behind the unchanged host protocol for
   definition-free Paragraph, ATX-content, Setext Paragraph↔H1↔H2 transitions,
   Paragraph↔thematic-break transitions, and fenced-code-body edits within
   64 KiB, plus length-changing ordinary Paragraph edits strictly after the
   last definition-bearing leaf with an exact frozen count, and
   definition-free first-Paragraph edits from BOF to an authenticated ordinary
   suffix plus final-Paragraph edits from an authenticated prefix to EOF. The
   paired 4,096-Paragraph boundary receipts retain complementary suffix/prefix
   checkpoints, bound transferred/replacement records to 64, preserve exact
   first/middle/last geometry, and stay exact through revision 3. EOF correctly
   mints zero fresh checkpoints. Restart collections authenticate the exact
   top-level block count, and parser split/merge receipts change the count and
   relevant ordinal by +2/-2. Retain their relative page-local facts,
   composable summaries, fresh `CertifiedSource` wrappers, and
   no-old-manifest-ancestry gates. The 4,096-block Setext transition receipt
   bounds every local delta to 64 transferred/replacement records; over-4-KiB
   same-block promotion fails the narrow plan cleanly. The 4,096-block thematic
   receipt bounds both parser crop and packed splice for promotion and
   demotion. Next, authenticate
   restart and suffix adoption for edits to or before definitions,
   definition-bearing BOF, typed
   unsupported leaves, missing anchors, new tail definitions/unsupported
   constructs, over-cap/lost-convergence cases, Table, containers, lists,
   quotes, indented code, HTML, references, and the remaining block constructs through the same
   scheduler, writer, projection, and publication authority.
   No path may expose a Paragraph `String`, cloneable document snapshot, or
   construct-specific fallback parser.
5. **Extend the completed selected-leaf parser-to-paint slice into a
   viewport.** Paragraph, fenced code, ATX Heading, Setext Heading, and atomic
   ThematicBreak now cross authenticated parser, publication, host,
   public-Dart, native, Chrome, real Flutter, and demo seams without giving
   Flutter grammar authority. Both heading forms use the generic Heading Dart
   API; ThematicBreak retains an empty text projection and whole-atom actions.
   The bounded current-revision Dart cache retains 128 leaves/2,048 facts and
   invalidates atomically. Build the
   visible-set materializer and virtualized range layout above it without a
   document-sized Dart graph or payload. Retain honest `Unknown` treatment and
   the proven input-island adapter on native and Wasm.
6. **Broaden from architecture proof to production proof.** Run the normative
   CommonMark/GFM corpus, inherited package regressions, clean-versus-
   incremental equality at every edit, randomized mutation/repacking, giant
   constructs, deep containers, reference fan-out, generated scanners,
   ownership/failure fuzzing, and sustained 1/10/100 MiB memory/backpressure
   workloads.
7. **Run launch gates and cut over.** Exercise real native/web floor hardware
   for input-to-paint latency, IME, touch selection, caret stability, shaping,
   bidi, accessibility, undo/paste/supersession, viewport motion, and sustained
   backlog. Freeze launch limits and migrate behind the existing package API
   only around contracts that survive those receipts.
