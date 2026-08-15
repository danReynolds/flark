# Flark v3 engine lab

This lab contains three deliberately separate feedback surfaces. Checkpoint A
is runtime observability, Checkpoint B is a fixed incremental-storage proof,
and Checkpoint C is the first authoritative live-rendered input slice. A newer
fenced-code slice now crosses the parser, publication, independent host, public
Dart query, native/Web runtime, and real managed Flutter editor. Its normal
body view hides the opening fence, info string, and closing fence while keeping
Markdown-looking body bytes literal. ATX Heading now crosses the same seams:
the review seed hides its opening and accepted closing hashes, applies
parser-authored heading typography, and reuses the same strong/emphasis/code
inline service as Paragraphs. Setext Heading crosses those seams through the
same generic Heading Dart API: the review seed hides its underline, applies
parser-authored heading typography, excludes only the terminal content-line EOL,
and retains internal line endings as softbreaks. The thematic-break slice
publishes exact atomic facts through structured role variant 6, keeps its
canonical marker line out of the empty editable projection, and paints one
parser-certified divider. Backspace or Delete removes the whole source atom on
the same input client. The indented-code slice publishes exact variant-7
structure, then separately demands a parser-authored schema-3 physical-line
projection that hides its four-column prefixes while preserving residual
indentation and literal code content. Enter inserts canonical indentation and
exact-current recertification retains the same input client. The narrow
BulletList slice publishes exact top-level depth-one tight-list facts through
structured role variant 9 and its established vertical separately demands
viewport schema 5 for the selected path and canonical 28-byte item records. Its
selected item is marker-free while the instrumentation retains exact markers,
indentation,
Unicode, and CRLF; handoff, canonical Enter continuation, terminal-empty exit,
and column-zero Backspace use parser-authored editing inputs. The separate
OrderedList slice publishes exact top-level depth-one tight-list facts through
structured role variant 10, then demands viewport schema 7 for one compact
selected-item record carrying the exact marker span and value. Its lab seed
paints `007)` outside marker-free `EditableText`; Enter inserts the
parser-authored `008) ` continuation while preserving CRLF and the input
client. Loose, task, nested, mixed-marker, and multi-block item forms remain
fail-closed. The
current-byte schema-5 managed gates are green on native and Chrome, and the
focused Chrome lab checkpoint plus visual release-demo review are green. Local
item edits now use checkpoint-free source-rope rank/select to derive only the
base and target predecessor/changed/successor windows. A compact schema-6
selected-item projection explicitly demands geometry and then inline facts for
that exact content range. Rebuilt-Wasm/freshness is green 2/2, public-runtime
semantic parity is green 1/1 on Chrome, and the managed compact BulletList
batch is green 3/3 on native Flutter and 3/3 on Chrome. The first combined
Chrome run had one transient timeout; its immediate isolated rerun and full
rerun passed, so this receipt is not a deterministic performance budget. The
selected-Paragraph slice moves one stable input client among first, middle, and
tail Paragraph leaves and hides each selected leaf's parser-certified
delimiters. Grammar revision 6 also carries parser-certified hard line breaks
as inline fact kind 8: their trailing marker is hidden, LF/CR/CRLF source is
retained exactly, and display/edit behavior comes only from Rust-authored
geometry. The same revision carries CommonMark character references as kind 9:
the small live-tail fixture cooks `&copy;` to `©` and the two-scalar `&ngE;`
to `≧̸` while the instrumentation retains both exact source tokens. Its URI
autolink fixture also derives the cooked visible label and destination from the
same parser-authored `&amp;` value. This is a narrow supported inline vertical,
not a claim of complete CommonMark or GFM coverage. Adjacent display runs with
the same parser-issued link annotation coalesce into one passive semantic link
label without merging unrelated styles or destinations. The historical focused
Chrome Engine Lab character-reference run was green on grammar-revision-6
asset version
`6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964`, whose mirrored Wasm
SHA-256 is
`6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4`.
Grammar revision 7 adds marker-free direct links and image alt text. Rust owns
their direct-tail and bracket precedence; exact destination/title geometry and
parser-cooked values cross the persistent `IPB5` fact/value pair and the
self-framing `FLKIV001` companion. Reference, collapsed, shortcut, and
incomplete link/image forms remain fail-closed. The current focused Chrome
checkpoint is green 2/2 for passive and active direct link/image presentation,
three recertified label/alt edits with exact hidden values, the existing live
autolink edit, one stable input client, and the first-frame flicker regression
on asset version
`a868f652dbdd5e5d-5f412bffe731e227-bba3dc0f34f51964`, whose mirrored Wasm
SHA-256 is
`a868f652dbdd5e5d22431e4e5d5401ea5c46855e5b02a905077ade9a1adb55f7`.
Broader performance receipts later in this document retain the asset version
on which they were actually measured rather than being silently reattributed.
A bounded Dart cache now
retains already-materialized current-revision inline facts after the singleton
host sidecar moves. Blank-separated Paragraph edits now use authenticated
restart/convergence and the persistent packed block-page splice. The same path
now carries interior ATX-content, Setext Paragraph↔H1↔H2 transitions, and
Paragraph↔thematic-break transitions, and fenced-code-body edits in
definition-free top-level documents when ordinary Paragraph checkpoints bracket
an at-most-64-KiB crop; exact-clean construction retains the same storage path.
An engine/public-runtime receipt also keeps ordinary Paragraph crops on that
path strictly after a frozen definition prefix: every surviving checkpoint
carries the exact definition count. Definition-free first-Paragraph edits can
also crop from BOF to an authenticated ordinary suffix, and final-Paragraph
edits can crop from an authenticated prefix through EOF. Restart collections
carry the exact top-level block count rather than relying on a loose segmented
flag. Edits to or before definitions, definition-bearing BOF, typed unsupported
leaves, missing anchors, new tail definitions/unsupported constructs, over-cap
crops, and lost convergence still fall back clean. Bounded visible structural
materialization is now implemented; virtualized multi-block layout and the
100 MiB product gate remain, so this is not yet a complete multi-block editor.
From this directory, run:

```sh
flutter run --release -d chrome -t lib/v3_engine_lab.dart
```

For the compact product-shaped multi-block checkpoint used by the focused
Chrome gate, run:

```sh
flutter run -d web-server --release --web-hostname 127.0.0.1 \
  --web-port 8765 -t lib/v3_live_editor_checkpoint.dart
```

That document includes a passive direct link with a cooked title and a passive
direct image with a cooked title. The link activates only through the
checkpoint callback. The image uses a labelled fallback and performs no
implicit network, file, or asset fetch.

Use a native Flutter device name instead of `chrome` to exercise the same
Dart runtime API through the native endpoint. The Web entrypoint explicitly
uses the Worker and Wasm mirrors packaged by `flark_flutter`. The lab displays
its build mode. A normal `flutter run` remains useful for functional debugging,
but compare liveness numbers only in release/profile builds.

Review the ATX Heading, Setext Heading, indented-code, top-level tight
BulletList and OrderedList selected-item, atomic thematic-break, and multi-block
selected-Paragraph seeds, the small live-tail seed, the 4,096- and
100,000-leading-reference live-tail seeds, and the 1 MiB and 10 MiB
giant-paragraph structural seeds.
Every projected fixture attaches the same small canonical tail through one
bounded marker-free editor; the reference prefix never enters
`TextEditingController`. Giant-paragraph neighborhoods reserve 256 units of
insertion headroom, so normal typing is accepted immediately. The lab reports
only public runtime facts: source, certified, and structure revisions;
semantic currentness; recovery availability; local pending-edit count; fixture
preparation, cold full-open, visible-tail-to-current, and truthful-close wall
times. The managed binding does not expose its synchronous foreground apply
duration, so that tile says so instead of inferring one. The lab does not
expose Worker generations, host revisions, parser queue depth, or hidden
per-stage timings.

Current release-Chrome reference observations on the development machine are
roughly 0.25-0.36 seconds to current structure at 1 MiB and 2.7-3.3 seconds at
10 MiB for giant/ordinary-line paragraphs. The interactive large seeds are the
single-line witnesses; ordinary-line and newline-dense figures are automated
browser receipts in
`test/v3/runtime/flark_v3_web_large_document_liveness_test.dart`, run by
`../scripts/verify_web_adapter_ci.sh`. These are checkpoint observations, not
launch SLOs.

The production-spacing incremental Paragraph receipts now keep the parser's
exact edit envelope distinct from the storage-aligned SourceFacts page. A
middle edit in 4,096 Paragraphs parses 4,116 bytes over 168 lines in three
transitions and transfers 8 of 16,386 canonical records. On the roughly
3.2 MB/100,000-Paragraph public path, exact-current replacement is
20.994–26.977 ms native and 29.9–30.1 ms Chrome, with foreground apply below
8 ms. Exact point semantics remain correct at the first, edited middle, and
last Paragraph, and the Dart/Flutter/Chrome matrix is green. Adjacent rapid
edits compose into one exact parser envelope; distant edits fail closed. This
also has a 671,794-byte mixed companion: 4,096 Paragraphs bracket an ATX Heading
and fenced-code block, and both an ATX-content edit and a fence-body edit enter
`ParsingOrdinaryExact`, publish successive `ExactBaseDelta` revisions, and
remain exact on native and Chrome. Removing the closing fence destroys
convergence and uses the definitive exact-clean fallback. A separate 4,096-block
fixture transitions one middle block
Paragraph→Setext H1→Setext H2→Paragraph. Every phase enters
`ParsingOrdinaryExact`, publishes `ExactBaseDelta`, transfers and replaces at
most 64 records, preserves exact first/middle/last queries, and retains exact
authority for the next revision. A same-block Paragraph→Setext promotion whose
prior content exceeds 4 KiB rejects the narrow restart and completes through
definitive exact-clean fallback instead of reusing stale checkpoint state.
The matching 4,096-block thematic fixture promotes and demotes one middle
Paragraph inside one parser crop bounded to 16 KiB, 512 physical lines, and
4,096 transitions. Structured role variant 6 carries exact marker
kind/count/indent/BOM/envelope/EOL facts and an empty zero-run projection; the
packed splice deletes and replaces at most 64 records and retains exact
next-revision authority.
The exact-clean indented-code path publishes structured role variant 7 with
the fixed four-column deindent recipe, BOF BOM fact, physical line count,
projected UTF-8/UTF-16 lengths, and terminal EOL width. Its visible range is
empty until a selected leaf separately demands the at-most-8-KiB projection.
Viewport schema 3 then carries canonical 20-byte records for every physical
line, distinguishing hidden prefix, source-backed content, EOL, and internal
blank lines without a Dart indentation recognizer. Real native and Chrome
runtimes agree on that two-phase result. The derivation is fuelled and
cancellable; focused evidence also releases a completed root and closes the
retained publication/runtime with zero residency. This is exact-clean and
selected-island evidence, not restart/convergence authority for edits inside
the block.
Another fixture places 2,048 definitions before 2,048 Paragraphs. Its
length-changing middle
Paragraph edit enters `ParsingOrdinaryExact`, publishes `ExactBaseDelta`,
transfers at most 64 records while retaining all References, preserves exact
first, edited-middle, and shifted-last queries, and stays exact through a
second public edit at revision 3. The admitted path therefore has two bounded
interior subsets: definition-free Paragraph-anchored crops, and ordinary
Paragraph crops strictly after a frozen definition prefix. A third
4,096-Paragraph fixture lengthens the first block: the endpoint enters
`ParsingOrdinaryExact`, publishes `ExactBaseDelta` with at most 64 transferred
and replacement records, preserves exact first/middle/last geometry, reuses
suffix checkpoints, and the public native/Chrome path stays exact through
revision 3. Its final-block twin takes the symmetric ordinary-to-EOF route,
retains prefix checkpoints, correctly mints zero fresh EOF checkpoints, obeys
the same 64-record bounds, preserves exact first/middle/last geometry, and
reaches public revision 3. Separate parser split/merge cases adjust the exact
authenticated top-level block count and relevant ordinal by +2/-2. A frozen
definition prefix remains safe for the EOF route. This is not general restart
through or incremental mutation of definitions, definition-bearing BOF,
reference-link UI, unsupported or unanchored regions, containers, quotes,
HTML, tables, other block grammar,
ordered-list restart/convergence, loose/task/nested/mixed-marker/multi-block
list forms, or
restart/convergence for edits inside indented-code blocks. A new tail
definition/unsupported construct, over-cap crop, or lost convergence still
fails clean.

The Checkpoint C live-tail, selected-Paragraph, ATX Heading, and Setext Heading
paths render parser-certified strong, emphasis, inline code, character
references, and direct inline links/images while hiding certified syntax.
Direct destination/title values arrive through the authenticated `FLKIV001`
companion rather than a Dart recognizer. The indented-code path uses its
separately demanded line
recipe to render marker-free monospace code while retaining literal
Markdown-looking content. The thematic-break path instead renders one atomic
divider from an empty text projection. The established BulletList path renders
one selected item without its certified marker/prefix, paints the marker in a
gutter, and retains exact canonical list source. The compact successor
separates selected-item geometry (schema 6) from the following inline demand,
and parser-certified strong/emphasis/code composition is green through that
combined path. The host retains one
revision-bound hot sidecar at a time;
the bounded current-revision Dart cache prevents revisited leaves from losing
facts. A separate structural-range lane now materializes consecutive top-level
blocks without repeated point queries. The 1 MiB and 10 MiB seeds remain
separate structural/liveness witnesses.

The automated checkpoint has standalone marker-free Worker/Wasm edit receipts
for the small, 4,096-reference, and 100,000-reference fixtures. The standalone
100,000-reference product gate applies seven zero-cadence platform deltas to
the bounded live tail, keeps one `EditableTextState` and input client, preserves
exact canonical source, and converges only the final revision. The historical
release-Worker Chrome regression observed a 4.2 ms maximum synchronous callback
and 7.6 ms total across the burst; these are preceding-revision-6 workstation
receipts, not current-byte or floor-device launch SLOs. The historical combined
small-widget→100,000-reference-widget sequential reopen gate is also green
after correcting the Web module-loader cache lifetime; that Chrome run records
a 5.1 ms maximum synchronous callback and 8.8 ms total callback time. Separate
revision-6 public-runtime gates cover the production-spacing 4,096- and
100,000-Paragraph paths across native and Chrome, the 671,794-byte mixed
ATX/fence path, and the
2,048-definition / 2,048-Paragraph and 4,096-Paragraph segmented-boundary
next-revision fixtures, plus exact thematic-break facts and the live
Paragraph→thematic-break→Paragraph transition, through rebuilt Worker/Wasm
asset version `6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964`, with Wasm
SHA-256
`6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4`.
Mirrored native and Chrome gates also demand and decode exact variant-7
indented-code structure and its viewport-schema-3 line projection.
Focused native and Flutter gates cover the established BulletList variant-9
structure, schema-5 selected-item payload, handoff, canonical source, Enter
continuation, terminal-empty exit, and exact prefix removal. Those current-byte
managed cases are green 3/3 on native and Chrome, and the focused Chrome
Worker/Wasm engine-lab checkpoint is green 1/1. The compact schema-6 path does
not rely on those older schema-5 receipts: rebuilt-Wasm/freshness is 2/2,
Chrome public-runtime semantic parity is 1/1, and the managed compact batch is
3/3 on native Flutter and 3/3 on Chrome.
A native gate still
drives a tail edit behind 100,000 definitions while checking bounded foreground
apply work, caller heartbeat, exact convergence, query preservation, and
truthful close. The historical grammar-revision-6 Rust workspace all-targets
run was green. Focused hard-line-break receipts cover Rust
recognition, engine/host validation, exact LF/CR/CRLF geometry, atomic source
edits, marker-free managed Flutter presentation, and whole-leaf fail-closed
behavior for an unshielded indented continuation. Older aggregate-suite and
full Web-adapter-CI results remain dated receipts rather than current
rebuilt-byte claims. The combined sequential reopen result above is also
historical.
The mixed Flutter checkpoint remains marker-free and on the same
`EditableTextState` and platform input client while moving from ATX content to
the literal fence body and editing both. The lab
exposes the 100,000-reference fixture for product review. Its automated
standalone Chrome Worker/Wasm timing receipt is from the same preceding-build
artifact: a 4.2 ms maximum synchronous apply callback and 7.6 ms total for the
seven-edit zero-cadence burst on that workstation.

The grammar-revision-7 functional gates remain the historical direct-media
receipt: the virtualized-surface suite is green 10/10; the Chrome live
checkpoint is green 2/2 for passive and active direct link/image presentation,
three recertified label/alt edits, exact hidden values, the existing live
autolink edit, one stable input client, and the first-frame flicker regression.
The byte-exact nonzero-value Dart codec gate is green 6/6, focused native and
Web direct-media runtime gates are green 1/1 each, and packaging/freshness is
green 12/12. The Rust workspace all-targets release build and Wasm rebuild are
green, and root/Flutter Wasm bytes and buildinfo are identical. No revision-7
performance timing is claimed here. The previously red active sidecar was a
Dart/Rust nonzero-value Begin-layout mismatch; Dart now uses the Rust field
order and a distinct-value byte-offset fixture prevents recurrence.

### Grammar revision 8: strict bare autolinks

Revision 8 adds a narrow marker-free bare-autolink checkpoint. Given canonical
source such as `https://commonmark.org`, `www.commonmark.org/help`, and
`hello@example.com`, the exact source span is also the visible content span.
The target is derived by recipe:

- a schemed URI uses `exactContent`;
- lowercase `www.` uses `httpPrefixedExactContent`;
- an email uses `mailtoExactContent`.

The admitted classifier is deliberately strict: exact lowercase `http://`,
`https://`, and `ftp://`; boundary-gated lowercase `www.` with a dotted
domain; and an ASCII `[A-Za-z0-9.+_-]+` email local part followed by a dotted
domain. URI and `www.` candidates win before email candidates. Terminal
punctuation, entity-like suffixes, `<`, and excess closing parentheses follow
the GFM examples 621–631 rules.

Classification runs as a bounded, resumable whole-leaf job. Its range cursor
reads source in 256-byte chunks, work is fuelled, candidate tokens are capped
at 8 KiB, and code, angle autolinks, direct links/images, and bracket context
shield candidates. An overlong token, unknown or unresolved bracket context,
overlap, or invalid state fails the entire leaf closed with no partial facts.
Explicit `mailto:`/`xmpp:`, uppercase URI/`www.` prefixes, relaxed forms,
reference/collapsed/shortcut links, and the rest of CommonMark/GFM are not
admitted by this revision.

The Chrome checkpoint also covers an idempotent same-ordinal activation after a
direct-media handoff. Repeating activation while the exact ordinal is already
active and no activation is pending now retains the certified projected value
instead of replaying canonical source despite unchanged authority and caret
intent. The browser first-frame inspection retained styled pixels without a
raw or blank frame. Browser typing was not manually driven in that inspection;
the automated Chrome and Flutter gates cover typing.

Current revision-8 receipts are Rust exact-clean 46/46, promotion audit 2/2,
and engine 251/251. `cargo test -p flark-parser` is green: 309 non-doc tests
and one compile-fail doctest passed, three manual scaling receipts were ignored
by design, and zero tests failed. The remaining gates are packaging 12/12,
freshness 2/2, Dart inline facts/projection 68/68, native sidecar end-to-end
7/7, Web Chrome sidecar end-to-end 3/3, Flutter presentation/surface 24/24,
example Chrome checkpoint 3/3, and the focused exact bare-classifier
large-paragraph gate 1/1.

Root and Flutter assets are byte-identical at version
`dfcce276df7954a9-714e23750091d226-bba3dc0f34f51964`. The Wasm is
3,506,644 bytes with SHA-256
`dfcce276df7954a97a11f3faef4f93217adddba0d4b620db5e4942a8a2e4c930`;
the Worker is 33,195 bytes with SHA-256
`bba3dc0f34f51964fe55bf67363b75fdc68a1387ce28f1771529c44ad7493a60`.
These are narrow functional and asset-identity receipts, not full-grammar,
release, floor-device, or new timing evidence.

Rapid supersession is also covered at zero edit delay. Certification of an
intermediate target does not advance the reusable SourceFacts base; only the
matching structural host commit does. Adjacent edits compose into the exact
parser envelope while a distant edit fails closed. This keeps the live editor
on a single exact-base lineage even when several edits arrive before any
candidate publishes.

## Multi-block structural status

The first production multi-block cut is complete below the visual layer:

- blank-separated source has total ordered coverage as exact `Paragraph`,
  `Blank`, `DefinitionsOnly`, `FencedCode`, `IndentedCode`, `AtxHeading`,
  `SetextHeading`, `ThematicBreak`, `BulletList`, `OrderedList`, or typed
  `Unsupported` leaves;
- leaves are packed into a persistent measured tree with at most 64 semantic
  entries per page;
- Green and Projection publish as distinct authenticated wrappers over one
  shared immutable block root;
- point queries require matching byte and UTF-16 coordinates, use affinity at
  boundaries, and budget both preflight and actual work with the derived
  `3h + 1` bound for tree height `h`;
- structural ranges use one seek plus a consecutive authenticated page walk,
  the fixed `FLKVR001` packet, and an opaque structural-ACK-bound
  continuation. The default quantum is 4,096 encoded bytes / 24 blocks /
  25 pages / depth 16 / 320 nodes; Dart advances one quantum at a time with a
  hard 256-block window cap, and Flutter advances at most once per frame. A
  giant top-level block remains one structural record;
- the public native and real-Chrome paths agree, including an edit from a
  segmented document into one sole Paragraph; and
- the managed Flutter binding hands off the exact queried leaf, including an
  empty `DefinitionsOnly` projection with a collapsed caret; and
- a selected nonzero Paragraph receives its own revision-bound inline sidecar,
  renders marker-free, moves among three leaves on one input client, and
  closes truthfully while a predecessor sidecar is still retiring;
- direct inline links/images retain fixed parser-authored geometry and join
  exact destination/title cuts plus cooked values from the authenticated
  `FLKIV001` companion. Passive direct links activate through the checkpoint
  callback; passive images use a labelled no-I/O fallback until an app supplies
  a resolver. Active direct-media presentation is marker-free and
  non-actionable; live label and alt edits preserve exact hidden values,
  recertify through the parser, and retain the same input client;
- current-revision inline facts remain available from a bounded 128-leaf /
  2,048-fact-record Dart cache after the host sidecar moves; and
- an ATX Heading publishes exact opener/content/optional-closer/EOL geometry,
  reuses the generic inline-bearing leaf path, and retains heading typography
  without flicker while exact structure catches up after an edit; and
- a Setext Heading publishes through structured role variant 5 and the generic
  Heading Dart API with exact H1/H2 underline geometry. Its marker-free content
  excludes only the terminal content-line EOL, retains internal softbreaks, and
  preserves heading typography through live recertification; and
- a ThematicBreak publishes through structured role variant 6 with exact
  marker/count/indent/BOM/envelope/EOL facts and an empty zero-run projection.
  Native and Chrome public queries agree, while managed Flutter uses affinity
  to choose the collapsed boundary, paints one semantic divider, and deletes
  the whole canonical atom with Backspace or Delete on the same client; and
- an IndentedCode publishes exact structured role variant 7 first, then a
  selected leaf separately demands its schema-3 canonical 20-byte physical-line
  records. Native and Chrome agree; managed Flutter hides certified prefixes,
  preserves monospace styling and literal content, maps Enter to canonical
  indentation, and recertifies on the same client; and
- a narrow top-level depth-one tight BulletList publishes exact structured role
  variant 9 first; the established vertical then separately demands its
  schema-5 selected path and canonical 28-byte item records. Managed Flutter
  hides the selected item's certified marker/prefix, retains exact source,
  hands off between items, and applies parser-authored continuation,
  terminal-empty exit, and exact prefix removal operations. Local list edits
  now use checkpoint-free source-rope rank/select to parse only the independent
  base and target predecessor/changed/successor windows and publish through
  `ExactBaseDelta`. The 20,000- and 100,000-item receipts both use 295 target
  local-parse transitions, build in 18/21, stream in 20, and transfer four
  records in two packets after examining 262,149 SourceFacts bytes. They match
  the clean oracle, cover lifecycle restoration and two consecutive deltas from
  the underfilled 109-checkpoint/three-page base topology, and close to zero.
  Compact schema-6 item geometry followed by an inline demand is green across
  rebuilt-Wasm, Chrome semantic-parity, and native/Chrome managed receipts; and
- a narrow top-level depth-one tight OrderedList publishes exact structured
  role variant 10 and separately demands one schema-7 selected-item record.
  Dart/Flutter keeps the parser-authored marker out of `EditableText`, paints
  its exact text, and maps Enter to the parser-authored next marker and
  canonical line ending. This is exact-clean selected-item evidence, not
  ordered-list restart/convergence; and
- a 4,096-block local Paragraph↔thematic-break transition stays inside one
  bounded parser crop and packed splice with at most 64 deleted/replacement
  records; and
- a 4,096-block local Paragraph↔Setext H1↔H2 transition stays on
  `ExactBaseDelta` with at most 64 transferred/replacement records per revision,
  while an over-4-KiB same-block promotion falls back cleanly; and
- an interior ATX-content edit and an adjacent fenced-code-body edit can reuse
  ordinary Paragraph checkpoints on both sides, stay within the 64 KiB crop
  cap, and publish through the same exact-base packed block splice; and
- ordinary Paragraph checkpoints strictly after the last definition-bearing
  leaf carry the exact frozen definition count, allowing a bounded
  length-changing Paragraph crop to retain all cooked References and publish
  through `ExactBaseDelta`; and
- a definition-free first-Paragraph edit can crop from BOF to an authenticated
  ordinary suffix, publish a bounded `ExactBaseDelta`, retain downstream
  checkpoints, and preserve exact first/middle/last geometry; and
- a definition-free final-Paragraph edit can crop from an authenticated
  ordinary prefix to EOF, retain upstream checkpoints, correctly mint zero
  fresh EOF checkpoints, and publish through the same bounded delta path.

This closes bounded structural viewport demand, not a virtualized multi-block
editor. Inactive visible leaves still need layout and height virtualization
above the range materializer and semantic cache. Fenced code, IndentedCode, ATX
Heading, Setext Heading, ThematicBreak, and the narrow BulletList are the
Structured blocks promoted beyond Paragraphs. OrderedList is also promoted
through its exact-clean selected-item vertical. Blank-separated
Paragraph edits and the admitted mixed interior Structured edits now use
authenticated restart/convergence and persistent packed block-page splice.
Direct inline links/images are promoted through exact parser geometry,
authenticated `FLKIV001` values, marker-free active/passive presentation, safe
passive actions, and the current static Worker/Wasm checkpoint. Reference,
collapsed, shortcut, and incomplete links/images are not promoted.
Restart through
or edits to definitions, definition-bearing BOF, typed unsupported leaves,
missing ordinary Paragraph anchors, new tail definitions/unsupported
constructs, over-cap or lost-convergence crops, containers, quotes, HTML,
tables, reference-aware links, other block grammar, restart/convergence inside
indented-code or ordered-list blocks, and loose, task, nested, mixed-marker, or
multi-block list forms remain the next latency gates.

## Feedback Checkpoint B

Select **Run Checkpoint B proof** near the top of the lab. The same bounded
Rust battery runs off the UI context:

- in a helper isolate through FFI on native Flutter;
- in the existing external Worker through Wasm on Flutter Web.

The receipt covers prefix insertion, middle Unicode replacement, tail
insertion, and a split-CRLF edit. Expand each row to inspect the exact target
byte crop, retained prefix/suffix page identities, replacement pages, and
bounded planning/splice work. The lifecycle chips additionally prove
cancellation after promotion, base restoration, rapid nearby lineage reuse,
explicit rejection of an unsafe distant lineage, clean fallback, and
zero-residency close.

The parity digest excludes process-local arena identities and must agree
between native and Wasm. Checkpoint B does **not** prove role-root deltas,
rendered Markdown, caret/selection behavior, or editor feel. Those are the
purpose of Feedback Checkpoint C rather than claims hidden inside this
storage-level gate.

## Feedback Checkpoint C

The instructions below exercise the selected-leaf marker-free path.
Checkpoint C has not passed because virtualized multi-block layout, the
100 MiB product viewport, floor-device interaction receipts,
restart/convergence beyond the definition-free and reference-frozen
Paragraph-anchored interior and definition-free segmented-boundary
at-most-64-KiB subsets, reference-link UI, broader grammar, and launch gates
remain open. Exact-clean indented-code structure and selected-island rendering
are proven, but restart/convergence for edits inside that block is not. The
narrow BulletList has exact-clean structure, the established schema-5
selected-item rendering vertical, and checkpoint-free local deltas. Source-rope
rank/select derives bounded predecessor/changed/successor windows instead of
traversing list-wide source. Compact schema-6 selected-item geometry
followed by inline facts is green; broader list forms remain open grammar work.
The narrow OrderedList has separate exact-clean structure, schema-7
selected-item projection, exact marker paint, and parser-authored continuation;
incremental restart/convergence for ordered-list edits remains open.

The compact product checkpoint separately proves marker-free passive direct
link/image presentation through real Worker/Wasm, cooked destination/title
values, callback-owned link activation, labelled no-I/O image fallback, and a
first-frame anti-flicker regression. While recertification is pending, it keeps
the last exact bounded passive pixels/render objects/geometry and the same
active `EditableText` with its mechanically updated projection. Stale hit
testing, link actions, and accessibility semantics remain disabled until exact
authority returns. After first readiness, the compact shell is latched so
transient parser work cannot expand diagnostics or resize the editor. A live
active direct-media receipt is now part of the gate: whole-link-label
replacement, insertion at the final visible label boundary, and image-alt
replacement each preserve the canonical destination/title, recertify exactly,
and return to updated passive semantics/fallback/action on the same input
client.

Open **Multi-block · selected Paragraph** first. The middle Paragraph renders
bold, emphasis, and inline code without delimiters while the instrumentation
view retains exact canonical Markdown. Use **Select first Paragraph**,
**Select middle Paragraph**, and **Select tail Paragraph** to move the same
bounded `EditableText` and platform input client. Closing after all three moves
also exercises serial retirement of the replaced sidecars.

Open **Small · 1 leading ref + live tail**, **4,096 leading refs + live tail**,
or **100,000 leading refs + live tail**, then edit the rendered tail after the
reference prefix. `**Bold**`, `_emphasis_`, and `` `code` `` remain canonical
Markdown in the document while the active field displays only their styled
content. The same field displays parser-cooked `©` and two-scalar `≧̸` instead
of the canonical `&copy;` and `&ngE;` tokens. Its
`<https://e.test/?q=&amp;>` autolink displays and activates the cooked
`https://e.test/?q=&` target while instrumentation retains exact source. All
three fixtures use the exact same source tail and projected input client; only
the authoritative prefix size changes. On the 4,096-reference
fixture, **Visible block range** should reach `exact` before the edit and again
after it, while **Visible range work** reports bounded quanta and the covered
UTF-16 span.

In the compact product checkpoint, inspect **Flark architecture notes** and
**Local architecture preview** before activating their Paragraph. The visible
document must contain neither direct-link brackets/tail nor image syntax. Tap
the passive link and confirm the checkpoint reports the parser-certified
`https://flark.dev/revision-7` destination. The image must remain a labelled
placeholder: no `Image` fetch occurs until an application supplies an explicit
resolver. Activate that Paragraph and edit the visible link label or image alt;
the destination and title stay hidden and exact in canonical Markdown while
the active projection recertifies on the same input client.

Open **Fenced code · marker-free body** to review the first block-level
Structured presentation. The field displays only the monospace code body: the
opening `` ```dart ``, closing fence, and info string remain canonical source
outside the active island. The visible `**literal Markdown**` stays literal
code rather than entering the inline emphasis lane. Body edits round-trip
through the same Worker/Wasm runtime and retain the same `EditableText` and
platform input client as the fence becomes closed or unclosed.

Open **Indented code · marker-free indentation** to review the variant-7
selected-island path. The field displays
`final message = '**literal Markdown**';` and `  print(message);` as monospace
code without the parser-certified four-column prefixes; instrumentation retains
the exact canonical source. Press Enter inside the first line. The editor adds
the physical line ending and four-space continuation prefix to source, keeps
the prefix hidden, and recertifies the new internal blank line without
replacing the `EditableTextState` or platform input client. This validates
exact-clean plus selected-leaf behavior, not incremental parser restart inside
the block.

Open **ATX heading · marker-free inline content** to review the first
inline-bearing Structured block. The H2 field displays `β😀 live heading`
without opening hashes, accepted closing hashes, strong markers, or emphasis
markers. The instrumentation view retains the complete canonical
`## **β😀** live _heading_ ###` source, including CRLF. Type or paste inside
the visible heading and watch it remain H2 during the parser-pending frame,
then recertify without replacing the `EditableText` or platform input client.

Open **Setext heading · marker-free inline content** to review the second
Heading source form through the same generic Dart and Flutter presentation
contract. The field displays `β😀 live heading` with heading typography but
without the `---` underline or certified inline markers. The canonical
instrumentation retains the full source and line endings. Multiline content
keeps internal line endings as softbreaks; only the final content-line EOL is
excluded from the inline range. Type inside the heading and watch the same
`EditableText` recertify without changing grammar authority.

Open **Thematic break · atomic marker-free divider** to review the first
non-text atomic block. The canonical instrumentation shows the exact
`  * * * \r\n` marker line, while `EditableText` contains zero characters and
the editor paints one semantic divider. Moving affinity across the atom chooses
its start or end boundary without inventing a caret inside the markers.
Backspace or Delete removes the whole canonical atom, then hands the same
`EditableTextState` and platform input client to the resulting Paragraph
boundary.

Open **Bullet list · marker-free selected item** to review the narrow list
vertical. The selected item's `-` marker and indentation stay in canonical
source while the field shows only item content and Flutter paints the gutter.
Use **Select first item**, **Select second item**, and **Select empty exit
item** to exercise exact selected-item handoff across the CRLF/Unicode fixture.
Enter in a nonempty item uses the parser-authored marker, prefix, and canonical
line ending for continuation. Enter on the terminal empty item exits the list;
Backspace at display column zero removes only the exact parser-authorized
prefix. This checkpoint admits one top-level, depth-one, tight homogeneous
bullet list with one Paragraph per nonempty item. Ordered lists use the
separate checkpoint below; loose, task, nested, mixed-marker, and multi-block
item forms fail closed. The runnable checkpoint
now composes parser-certified inline styles after compact selected-item
geometry. The parser locality gate is already intra-list incremental: a
list-local edit uses the
persistent source rope to parse only the base and target
predecessor/changed/successor windows. The compact schema-6 geometry demand and
subsequent selected-content inline demand are green through rebuilt-Wasm,
Chrome semantic-parity, and native/Chrome managed receipts.

Open **Ordered list · exact marker outside selected item** to review the narrow
ordered-list vertical. The sole `EditableText` initially contains `alpha\n`;
the exact parser-authored `007)` marker is painted beside it and remains in the
canonical instrumentation source. Press Enter after `al`. Canonical source
becomes `007) al\r\n008) pha\r\n9) beta\r\n`, the selected item stays
marker-free, and the same input client is retained. This checkpoint admits one
top-level, depth-one tight ordered list. Nested, loose, and task list forms and
ordered-list incremental restart/convergence remain pending.

The field is one stable `EditableText`. Its bounded display-space input lease
maps selection, edits, and multi-stage IME composition back to exact source
coordinates. Ordinary typing retains the hidden projection while parser
authority catches up, then atomically adopts exact-current facts without
replacing the controller, focus, or platform input client. A current
unsupported parser result fails closed to literal source.

The reference fixtures deliberately do not hide startup cost. Seed preparation
and cold full open are separate metrics, and the tail editor attaches only
after exact-current startup. The subsequent **Visible tail → exact** metric
starts after the managed binding has synchronously accepted the edit and
updated the marker-free projection. It therefore measures parser/authority
convergence visible to this example, not unexposed synchronous input work.
