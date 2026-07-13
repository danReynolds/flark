# Architecture position: flark as a robust live markdown editor

**Status:** POSITION, 2026-07-12 — written at the close of the RFC 022
hardening program (Phases 0–3 + 2b merged; Phase 4 gated). Input to the
launch/Phase-4 sequencing decision; merging this document does not by itself
adopt §5.
**Author:** architecture review follow-through (see PRs #27, #29, #30–#34)
**Related:** `rfc/rfc_022_parser_grammar_monopoly.md` (the accepted contract
and phase log), `../production_readiness/audit_2026-05-01.md` (the pre-split
audit this supersedes in spirit), `../../doc/architecture/live_edit_intent_pipeline.md`,
`../../doc/architecture/live_rendered_rebuild_isolation.md`,
`../testing/ime_device_protocol.md`, `rfc/rfc_021_flark_surface_edit_convergence.md`.

## 1. Position

Flark's architecture — markdown source string as the single truth, comrak as
the sole grammar authority, geometry-only derivation below the parse
boundary, block-keyed rendering — is **fit for purpose as a robust live
markdown editor and does not need a rethink.** This is now an evidential
claim, not a design opinion: the hardening program pushed a protocol redesign
through the FFI boundary, inserted a pre-commit judge at the dispatch
chokepoint, replaced the rendering substrate under three surfaces, and
deleted the freelance-parsing layer — and upstream conformance held to the
digit (core 643/644, GFM 661/662) through every phase. Architectures that
need replacing do not absorb surgery like that.

Robustness for an editor of this kind is **not a property of the document
model; it is a property of the enforcement lattice around an adequate
model.** The program's own defect record proves it: five regressions were
introduced *while building the hardening* and every one was intercepted
pre-merge by the machinery (suite, judge, differential oracle, adversarial
review). The same class of bug that once shipped to the previewer now dies in
CI. The lattice, not any single design choice, is the asset to protect.

One genuinely open architectural question remains, and it has a named,
empirical trigger rather than an opinion attached: whether real IMEs tolerate
authoritative parse adoption mid-composition (§4-W1). Everything else on the
risk register is either staged work with a defined gate or permanent,
budgeted tax.

## 2. Evidence base

- Pre-program assessment and correctness batch: PRs #27 (platform CI,
  FFI panic guard, packaging hardening), #29 (eight user-facing correctness
  bugs, each regression-tested).
- Program: #30 (RFC 022 contract + boundary pin + adoption assert),
  #31 (parser-as-judge, adaptive sync ceiling, loud contract violations),
  #32 (bridge protocol v2, links-first; first scanner deletions),
  #33 (bridge-derived reference definitions + protocol handshake),
  #34 (one shared inline segmentation for all three render walks +
  differential parity oracle).
- Reversals recorded with counterexamples in RFC 022 §5: keystroke
  sync-primary (echo-recognizer timing; device-gated) and the line-heuristic
  geometry assert (fence-topology counterexample).
- Steady-state gates: 1,200+ Flutter tests incl. goldens and fuzz lanes,
  26 cargo tests, 661/662 + 643/644 conformance with a one-entry deviation
  register, 5-job CI (macOS/iOS/Android/Linux + release gate) on every PR,
  WASM source-freshness manifest, payload `protocolVersion` handshake.

## 3. Current strengths

1. **A proven core.** The projection offset mapper survived 6,000-config
   fuzzing with zero violations; transactions/undo/invert and the
   revision-plus-length-gated adoption pipeline are race-free against stale
   parses. These components absorbed every phase unchanged.
2. **Grammar has one home, enforced.** Link/image/autolink/reference/
   definition structure crosses the boundary AST-derived (children
   sourcepos) or parser-validated (ref-def classifier refuted by comrak's
   own block output). Five Dart scanner functions are deleted; the
   remaining sanctioned scanner surface is pinned by
   `test/v2/markdown/flark_grammar_scanner_boundary_test.dart` with a
   disposition table in RFC 022 §3.
3. **Invariants are checked, not promised.** The always-valid-inline
   invariant is enforced at three layers: the judge rejects refuted authored
   claims before commit (coverage-based, so legitimate delimiter fusion
   commits and visible leaks veto); the Phase 0 assert re-verifies at
   adoption (including judge-skipped commits, e.g. web); the differential
   oracle renders 15 inline-heavy documents through both surfaces and
   compares character multisets. Contract violations are typed and rethrown
   loudly — never routed to the parse-error callback.
4. **Render surfaces cannot drift silently.** All three inline render walks
   (preview spans, per-block editor, whole-document host) consume one
   headless segmentation module; reintroducing drift is a red test.
5. **Honest-fallback rendering rule.** Wherever derivation cannot represent
   a form faithfully (wrapped link labels, trailing-softbreak labels), the
   construct renders fully source-visible — never half-hidden. Fidelity
   degrades to honesty, not to corruption.
6. **Liveliness posture is strong on the command path.** Judged commands
   adopt authoritatively in the same event turn; the sync-parse ceiling is
   latency-learned (floor = the old fixed 4 KiB cutoff, observed range up to
   1 MiB), so most documents get authoritative first paint with no flash.
   Per-keystroke rebuild cost is flat (~1.5 ms at 80 blocks,
   profile-validated) via stable block identity + instance reuse.
7. **The IME echo layer is institutional knowledge, pinned.** The recognizer
   matrix documents every asymmetry with a structural reason; pins force
   deliberate change; the device protocol defines exactly what real-keyboard
   evidence is required before convergence work lands. This is the state of
   the art for Flutter editors.
8. **Process as infrastructure.** Phase-per-PR through the full gate,
   adversarial review with empirical probes on every substantive diff, and
   an RFC culture that records reversals *with their counterexamples* so
   nothing is relitigated from scratch.

## 4. Weaknesses and concerns (ranked)

- **W1 — Typing liveliness is unfinished, and its gate is the one real
  architectural unknown.** Keystrokes still ride the 80 ms debounce; the
  endgame (block-local reparse + sync-primary scheduling, RFC 022 §6/Phase 4)
  is designed and staged but gated on the manual IME device pass, because
  suite evidence showed mid-typing adoption changes echo-recognizer behavior.
  **Trigger worth writing in ink: a failed S11 across device-matrix rows 1–8
  reopens the *input* architecture** (per-block input-connection isolation,
  scoped desktop-first sync-primary) — the only currently-known outcome that
  would justify structural change. Nothing else on this list does.
- **W2 — The keystroke path carries permanent heuristic tax.** Echo
  classification against platform text-input round-trips is Flutter's terms
  of service; it cannot be parsed away. Mitigation is containment (purity,
  pins, device evidence), and Phase 4 shrinks the pre-parse windows the
  heuristics defend — but this surface will always demand maintenance and
  reviewer skepticism.
- **W3 — Dual editing surface, on watch.** The host/per-block split switches
  code paths as a document gains structure. RFC 021's empirical re-litigation
  kept it (the drift premise was refuted; one 15-line fix), and Phase 3
  removed a drift axis, but it remains the most likely home of the *next*
  behavioral divergence. Watch signal: any new bug whose repro differs by
  surface.
- **W4 — Scale ceilings, known and triggered.** Eager block build (no
  viewport virtualization; Stage 5 deferred by measurement), whole-document
  reparse per debounce, and payload marshal cost that plausibly rivals parse
  at ~1 MB. Remedies are staged, not speculative: block-local reparse
  (Phase 4), delta payloads over protocol v2, virtualization behind a
  first-paint trigger. Profile marshal-vs-parse before optimizing either.
- **W5 — comrak dependency edges.** Multi-line inline sourcepos truncates at
  the first line (pinned, honest-fallback applies); reference definitions are
  invisible in the AST (worked around with validated classification); no
  incremental API (block-local reparse is the workaround). These are
  upstream-shaped constraints — acceptable, but each is a small standing
  divergence between "what comrak knows" and "what it will tell us."
- **W6 — Open defect/debt register.** The heading-trimmer/classifier validity
  disagreement can hide invalid-def-shaped setext heading lines (pre-existing,
  content-hiding, tracked); per-list-item toggle wrapping (current veto is
  correct but unhelpful); preview performance items (lazy list, highlight
  cache — the preview still rebuilds O(document) per notification);
  benchmark budgets are debug-VM-loose and hardware-pinned.
- **W7 — The lattice itself is now surface area.** Judge, asserts, oracle,
  boundary pins, freshness/protocol guards: each is a concept a contributor
  must learn and a mechanism that can rot if bypassed. RFC 022 §5's
  discipline — every phase updates the pins and the table in the same
  commit — is the mitigation and must remain a review requirement.

## 5. Proposal from here

**Step 1 — Run the IME device matrix now (the protocol in
`../testing/ime_device_protocol.md`), and treat it as two gates in one.**
S1–S10 validate *today's* editor on real keyboards (launch evidence); S11
across rows 1–8 is the *convergence* gate (Phase 4 evidence). Roughly a day
of manual work across the device rows; record outcomes per the protocol's
bookkeeping section. This is the highest-information action available and
every remaining decision keys off it.

**Step 2 — Launch on S1–S10 green, without waiting for Phase 4.** The stated
launch condition (Phases 0–2) is exceeded — Phase 3 landed too. Phase 4 is
additive liveliness, properly gated, and should not hold the release.
Mechanics: re-run `./scripts/verify_release.sh` from a clean checkout, bump
to 0.2.0 with a CHANGELOG entry covering #27–#34 (note the two intentional
rendering-fidelity changes: shortcut-reference/autolink brackets now hide;
unrepresentable link forms render fully source-visible), `dart pub publish`
after the dry run, tag, and update the pages site.

**Step 3 — Phase 4, shaped by S11's outcome:**
- *S11 clean (expected):* land in dependency order, one PR each through the
  existing loop — (a) parser-derived edit-invalidation ranges (fence topology
  from the parse; enables promoting the Phase 0 geometry counter to an
  assert); (b) block-local sync reparse machinery, flag-gated, with the
  conservative structural classifier; (c) keystroke sync-primary scheduling
  flipped on behind the flag, goldens/matrix re-validated, the Phase 1
  debounce pin deliberately retired; (d) scanner deletion driven by the
  telemetry counter + allowlist shrink; (e) per-list-item toggle wrapping
  from parsed block boundaries (multi-segment dispatch, one undo group).
- *S11 dirty:* do NOT land (b)/(c). Run the pre-declared contingency spike
  instead: per-block input-connection isolation (the rebuild-isolation work
  already guarantees untouched blocks keep their input connections — extend
  that to make adoption invisible to the composing block), and consider
  desktop-first sync-primary where IMEs are tamer. Mobile keeps the debounce
  until the spike proves out. File findings against the recognizer matrix as
  the protocol prescribes.

**Step 4 — Standing engineering policy (independent of S11):** keep
phase-per-PR with adversarial review for anything touching the parse
boundary, placement, or render surfaces; grow the parity-oracle corpus with
every inline feature; profile marshal-vs-parse at 100 KB/1 MB before any
performance work and prefer delta payloads if marshal dominates; revisit the
deviation register at each comrak upgrade (pin the comrak version bump as its
own PR through conformance).

**Deferred, with reopen-triggers recorded:** viewport virtualization (trigger:
measured first-paint budget breach on target documents); tree-sitter or a
bespoke incremental parser (trigger: multi-language embedded editing as a
product goal, or documents outgrowing block-local reparse); dual-surface
convergence (trigger: a recurrence of surface-divergence bugs); model-truth
document core (trigger: none foreseeable while byte-exact fidelity is the
differentiator).

## 6. Alternatives considered

1. **Full rebuild / clean-room v3.** Rejected on evidence, twice. The
   inventory of what a rebuild would keep (proven core, conformance corpus,
   IME knowledge, goldens) is exactly the codebase; the from-scratch design
   exercise converged on the same parser and same architecture with only an
   earlier contract; and the retrofit then demonstrated the architecture
   absorbs deep surgery with conformance unchanged. A rebuild would re-roll
   risk on the zero-defect components to reach a destination already reached,
   while forfeiting the verification state that makes changes safe.
2. **Model-truth editor core (ProseMirror/super_editor shape).** Rejected:
   byte-exact source fidelity is the product differentiator and
   serialization drift is a worse bug class than anything observed here; the
   performance rationale evaporated when rebuild isolation reached flat
   per-keystroke cost on the string model; and none of the recorded defects
   originated in string-truth itself — they lived at derivation seams, which
   the contract now closes.
3. **Pure-Dart parser (delete the FFI/WASM boundary).** Rejected: raw parse
   speed worsens and only the boundary tax improves — better attacked with
   delta payloads — while the cmark-gfm conformance lineage (the "renders
   like GitHub" guarantee) would become a hand-maintained liability. It is
   the two-parsers problem promoted to the whole system.
4. **Adopt an incremental parser now (tree-sitter / bespoke Lezer-style).**
   Deferred with triggers, not adopted: markdown's block/inline phase split
   makes block-granular reuse ~equivalent to true incrementality for editing
   workloads; markdown has no parse errors, removing the error-recovery
   rationale; and tree-sitter-markdown trades conformance — the moat — for
   robustness properties flark doesn't need. Block-local reparse over comrak
   reaches the same interactive contract without owning a grammar.
5. **Patch the bugs, skip the program (status quo ante).** Rejected by the
   program's own defect record: the bug class regenerated five times during
   the hardening itself and was caught only by the new machinery. Without
   the lattice, those five would have been the next launch blockers — the
   clearest possible demonstration that the class, not the instances, was
   the problem.
6. **Land Phase 4 now, skip the device gate.** Rejected: Phase 1 empirically
   demonstrated that mid-typing adoption changes typed-closing-fence echo
   behavior — precisely the class the device protocol exists to vouch for.
   Simulated input cannot clear it; the gate stands.

## 7. Bottom line

Flark does not need a different architecture to be a robust live markdown
editor; it needed — and now has — an architecture whose rules are enforced by
machinery with a demonstrated kill count. The remaining path is short and
fully specified: run the device matrix, launch on its S1–S10 evidence, land
Phase 4 in the shape S11 dictates, and keep the lattice green. The rethink
triggers are written down precisely so that no one — including a future
reviewer with a strong opinion — has to argue this from scratch again.
