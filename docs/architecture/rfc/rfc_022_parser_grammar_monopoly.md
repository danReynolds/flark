# RFC 022: Parser grammar monopoly — the target contract

**Status:** ACCEPTED 2026-07-12. Launch is deferred until Phases 0–2 land (3–4
optional-but-planned). Phase 0 shipped with this RFC.
**Author:** architecture review follow-through (see PRs #27, #29)
**Related:** `docs/architecture/v2/parser_protocol_2026-05-02.md`,
`docs/architecture/v2/inline_delimiter_validity_2026-07-10.md`,
`doc/architecture/live_edit_intent_pipeline.md`, RFC 017, RFC 021.

## 1. Summary

The 2026-07 pre-launch review found that every correctness bug shipped to that
point clustered at one seam: places where Dart code **re-derived markdown
structure from source characters** instead of consuming what the comrak parser
already knew, or guessed structure the parser would later contradict
(`_findUnescaped` link scanning, single-line reference-definition regexes,
toggle flanking approximations, the preview's flat inline walk). The parts
with a single structural source of truth — projection offset math,
transactions, adoption gating — fuzzed clean.

This RFC fixes the class, not the instances. It declares one rule:

> **Grammar belongs to the parser. Everything else is geometry.**
>
> *Grammar* = deciding what text means (does this `*` close a run, is this
> line a fence, would this wrap parse as intended). *Geometry* = mapping
> already-parsed structure through edits (offsets, ranges, carets).
> Below the parse result, code may do geometry freely; it may not answer
> grammar questions from source characters. Grammar questions go to comrak —
> synchronously where latency allows, via validated proposals where it
> does not.

The decision context: a rebuild was considered and rejected — the proven core
(projection math, transaction model, IME recognizer matrix, conformance
corpus) is exactly what a rebuild would put back at risk, and the from-scratch
design converges on this same contract (see §8). We retrofit the contract and
clean-room only the seams.

## 2. The two illegitimate parser roles this removes

1. **Freelance derivation** — Dart re-parsing source text that comrak already
   parsed (link tails, ref-defs, fence shapes) because the bridge payload was
   too poor to carry the answer. Fixed by Phase 2: the bridge emits **every
   structural fact as ranges**; the Dart mapping layer shrinks to decode +
   UTF-8/16 offset mapping.
2. **Speculative grammar** — the prediction layer answering grammar questions
   with textual scanners so a keystroke can resolve synchronously. Fixed by
   Phases 1/4: grammar answers become cheap enough to be synchronous
   (adaptive sync threshold, block-local reparse), and until then scanner
   output is a *proposal* that the parser confirms (§4) or vetoes (Phase 1
   judge). Prediction itself stays **geometry-only**: mapping the last adopted
   structure through the transaction (`predictAfter`), which is fuzz-proven.

## 3. Sanctioned scanner surface (the allowlist)

The textual-analysis primitives and every module allowed to import them are
pinned by `test/v2/markdown/flark_grammar_scanner_boundary_test.dart`. Adding
an import site fails the test until it is deliberately added here; Phases 2/4
shrink this table and update the test in the same commit.

Scanner libraries (the pinned set):
`flark_inline_delimiter_placement.dart`, `flark_inline_flanking.dart`,
`flark_inline_run_scanner.dart`, `flark_markdown_fenced_code_scanner.dart`.

| Importer | Role | Disposition |
| --- | --- | --- |
| `markdown/parse/flark_native_comrak_parse_backend.dart` | mapping-layer synthesis | **Phase 2: delete** (bridge emits ranges) |
| `markdown/commands/flark_markdown_inline_commands.dart` | wrap/toggle proposals | Phase 1: parser-judged |
| `markdown/commands/flark_markdown_command_capabilities.dart` | capability probing | Phase 1: parser-judged |
| `markdown/source/flark_markdown_input_engine.dart` | line-local input conveniences | Phase 4: consult adopted plan |
| `markdown/source/flark_markdown_fenced_code_policy.dart` | fence edit policy | Phase 4: block-local reparse |
| `flutter/flark_flutter_controller.dart` | armed-wrap placement | Phase 0: adoption-asserted; Phase 1: judged |
| `flutter/flark_markdown_input_policy.dart` | Enter/Backspace structure | Phase 4: consult adopted plan |
| `flutter/flark_live_code_fence_input_policy.dart` | fence echo/structure | Phase 4: block-local reparse |
| `flutter/flark_live_edit_classifier.dart` | echo recognition | stays (echo ≠ grammar) |
| `flutter/flark_projected_editable_text.dart` | edge stepping | Phase 4: re-evaluate |
| `projection/flark_projection.dart` | run scans over parsed tokens | stays (reads parser pairing, not raw source) |
| `projection/flark_projected_text_edit_adapter.dart` | marker-aware deletion | stays (operates on parsed hidden ranges) |

## 4. Enforcement shipped with Phase 0

1. **Import boundary test** — pins §3 exactly, both directions (new importers
   fail; removals must update the pin, keeping the table honest).
2. **Adoption-time confirmation (debug builds)** — when a parse result is
   adopted over a live prediction:
   - **Authored-claim assert (strict).** Delimiter ranges the editor itself
     authored and pre-hid (`armed wrap`, placement relocations —
     `_pendingAuthoredMarkers`) must be re-derived by the parse with
     identical bounds, per the documented contract in
     `live_edit_intent_pipeline.md` ("the immediate parse then re-derives the
     identical hidden ranges authoritatively"). A violation means a scanner
     authored markdown comrak disagrees with (the `**foo***` class) and
     throws in debug builds — the entire existing suite becomes an invariant
     gate for free.
   - **Geometry confirmation (counter, not throw).** Predicted
     hidden/replacement ranges outside the transaction's invalidated range
     that the parse does not confirm increment
     `flarkDebugUnconfirmedPredictionRanges`. Markdown is non-local (a fence
     opener restructures everything downstream), and the raw typing path does
     not yet declare its blast radius, so this ships as telemetry: it
     produces the Phase 4 evidence for which scanners still lie or fire, and
     is promoted to an assert once the typing paths set honest
     `projectionInvalidationRange` metadata (Phase 1).

## 5. Phase plan

| Phase | Deliverable | Deletion target | Status |
| --- | --- | --- | --- |
| 0 | This RFC; boundary test; adoption assert + telemetry | — | shipped with this RFC |
| 1 | Parser-as-judge on command paths (sync-parse candidate transactions, reject on mismatch); adaptive sync-parse threshold (latency-learned, replaces the fixed size cutoff); honest invalidation metadata on typing paths | toggle/wrap flanking approximations demoted to proposals | pending |
| 2 | Bridge protocol v2: marker sub-ranges for links/images/autolinks, multi-line ref-def spans, every structural fact as ranges; new thin Dart decoder cut over under conformance/parity/goldens | `_nativeInlineHiddenRanges`, `_referenceDefinitionRanges`, the mapper's scanner imports (~1,400 of 1,853 lines) | pending |
| 3 | Preview consumes the editor's boundary-segmentation module; differential preview-vs-editor oracle over the conformance corpus | preview's bespoke span walk | pending |
| 4 | Block-local sync reparse for inline-safe edits at any doc size (conservative structural classifier gates; async whole-doc remains the reconciler); delete scanners the §4 telemetry proves dead | input-engine/fence-policy structural probing | pending |

Each phase lands as its own PR through the full release gate (conformance,
parity, goldens, platform smokes) so the system stays shippable at every
boundary.

## 6. Sync-authoritative contract (Phases 1/4 target state)

Keystroke → transaction → **authoritative parse** → adopt, same frame, for
any document whose recent parse+decode p95 fits the frame budget (adaptive
threshold, not a fixed byte cutoff). Above budget: geometry-only prediction
covers the debounce window — stale truth mapped forward, never guessed
structure — and Phase 4's block-local reparse shrinks "above budget" to
structural edits only. IME constraint unchanged: the editable is never
resynced mid-composition; predictions pre-hide authored markers exactly as
today.

## 7. Bridge v2 sketch (Phase 2)

The payload already preserves unknown fields and carries `abi_version`, so v2
fields ship additively: `marker_ranges` on link/image/autolink tokens,
`reference_definition` tokens with full (multi-line) spans, and any
structural fact the mapping layer currently re-derives. Derivation happens in
the bridge crate with the comrak AST in hand, tested in Rust against comrak
itself, and is identical for FFI and WASM by construction (one crate, WASM
freshness-guarded).

## 8. Rejected alternatives (recorded so they are not relitigated)

- **Full rebuild** — discards verification state (661 conformance cases,
  parity, goldens, fuzz) and re-rolls risk on zero-defect components to reach
  the same contract. Triggers that would reopen it: product shift to
  multi-language nested editing; Phase 2 empirically revealing coupling that
  resists migration; a decision to own the parser as product.
- **Pure-Dart parser** — deletes the boundary tax but forfeits cmark-gfm
  conformance lineage; promotes the second-parser problem to the whole
  system.
- **tree-sitter now** — pays conformance (approximate grammar) for error
  recovery markdown does not need; incrementality is reachable via block
  granularity on comrak (§6, Phase 4).
- **AST-truth document model** — forfeits byte-exact source fidelity (the
  product differentiator) and imports serialization-drift bugs; disproven
  need — flat per-keystroke rebuild cost was reached without it
  (`live_rendered_rebuild_isolation.md`).
