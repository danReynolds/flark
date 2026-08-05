# Product-contract audit of partial parser output

Status: independent test-suite audit, 2026-07-15.

Scope: v2 projection, live/read-only Flutter surfaces, platform input, selection,
commands, history, and compound editors. This evaluates the proposed parser
shape:

```text
{ finalized prefix, O(depth) open overlay, UnknownRange,
  suffix only after exact convergence }
```

It does not evaluate the current `record_forest` implementation and does not
change parser code.

## Verdict

The shape is a good **parser-publication spine**, but it is not yet a complete
product-output model.

Two refinements are required:

1. The parser's `O(depth)` continuation/frontier state and the product's active
   presentation facts must be separate concepts. The latter is
   `O(requested facts)`, not `O(depth)`: an active leaf can need inline marker,
   replacement, ambiguity, run-edge, table-cell, task, fence, semantic-target,
   and capability records.
2. “Suffix attaches only after convergence” is correct for authoritative
   parser-tree reuse. It must not mean “Flutter unmounts the old host or throws
   away reusable layout.” Mounted shard identity/layout cache is independent of
   semantic authority. Old semantic facts may remain only when the parser has
   certified them exact for the target revision; otherwise the mounted host
   source-paints.

With those changes, `UnknownRange` is a clean, honest degradation state. Without
them, the model either recreates a prediction parser in Flutter or misses
existing live-feel, input, selection, and compound-editor contracts.

## Authority vocabulary

- **Sealed record:** complete target-revision facts for a closed block or
  projection leaf. “Block sealed” does not imply that reference-dependent
  inline meaning is also final.
- **Frontier state:** the exact resumable parser stack and bounded dependency
  state needed to continue parsing. Its frame count can be `O(depth)`.
- **Active presentation overlay:** exact target-revision facts requested for an
  active/visible range before the whole job converges. Its size is proportional
  to emitted facts, not stack depth.
- **Unknown range:** total source coverage with exact source/coordinate metrics
  but no Markdown presentation claim. Its only honest rendering is source/plain
  presentation.
- **Mounted host/layout lease:** stable Flutter element, input/shard identity,
  and cached geometry. It carries no Markdown semantic authority and may
  source-paint.
- **Certified reused facts:** old payload records explicitly proved exact for a
  target source range/revision. Source identity alone is insufficient when
  ancestor or reference state can change meaning. These are logically current
  facts after certification, not permission to paint unproved stale semantics.

## Acceptance matrix

| Product contract and concrete evidence | Minimum authority | Is source-visible `UnknownRange` acceptable? | Required gate |
| --- | --- | --- | --- |
| Exact source, grapheme-safe edits, source-transaction undo/redo. See `test/v2/core/flark_history_stack_test.dart:6`, `test/v2/flutter/flark_forward_delete_test.dart:55`, and `test/v2/flutter/flark_inline_run_backspace_test.dart:203`. | Persistent source/coordinate index and input/history state; no Markdown block record is required for raw text mechanics. | **Yes.** This is the correctness baseline. | Unknown leaves retain exact UTF-8/UTF-16/line/grapheme coverage. Parser adoption never creates a history entry or changes committed source. |
| IME text and composing range survive every stage, including CJK conversion, autocorrect, styled runs, quotes, lists, code, and tables. See the invariant in `test/v2/flutter/flark_ime_input_test.dart:886`, strong-run conversion at `:425`, block surfaces at `:707` and `:797`, and table/code grouped undo at `flark_live_rendered_editable_text_test.dart:3878` and `:5408`. | Stable input-island source slice, input lease, canonical source selection, and any presentation facts frozen/adopted under that lease. | **Yes, and it is preferable to a representation flip.** The platform value must remain exact and stable. | While composing, known/unknown adoption cannot rewrite `TextEditingValue.text`, move composing, replace the input host, or change local/source mapping. Unsafe structure queues until commit. |
| Incomplete or hand-authored syntax remains literal: partial/malformed strong/link/image/code, bare quote/list markers, and a typed fence opener with no committed body. See `flark_live_rendered_editable_text_test.dart:75`, `:121`, `:341`, `:779`, and `:1973`. | Exact source only, or an exact parser result saying the syntax is literal. | **Yes, indefinitely while it remains incomplete/literal.** | No partial marker hiding, styling, or chrome may be emitted from a frontier guess. |
| Locally committed but still-open structure becomes live immediately: `> ` and list marker + space, empty heading, and a fence after Enter. See `flark_live_rendered_editable_text_test.dart:377`, `:805`, `:2030`, `:2078`, and `:6507`. Parser-blocked fence cases at `:2811`, `:3043`, and `:3117` explicitly expect a code body before an authoritative full plan exists. | Exact active presentation overlay: block kind, complete ancestry, marker/content ranges, stable active target, and relevant continuation/Backspace capabilities. For a fence this includes opener/info/body/closer state. These blocks may be open at EOF and therefore have no ordinary finalized terminal record yet. | **Only before the syntax trigger is committed, or as an explicitly accepted missed-deadline frame.** Persistent raw source after the marker-space/Enter boundary violates the current live contract. | Urgent parser query or parser-issued authored-intent proof must produce the overlay by the tested frame. If it cannot, choose and document a product downgrade; do not replace the old prediction layer with a new Dart grammar guess. |
| Inline rendering/editing maps hidden markers, replacements/entities, ambiguity zones, and inside/outside run edges exactly. See `test/v2/projection/flark_projection_test.dart:646`, `:831`, `:1050`, `:1114`; `flark_projected_text_edit_adapter_test.dart:53`, `:182`, `:283`; and run-edge widgets at `flark_live_rendered_editable_text_test.dart:5949`-`:6507`. The live test at `:15` also pins continued strong styling/hidden projection immediately after an edit, before a new parse is applied. | Exact projection-leaf facts: hidden/replacement ranges, opening/closing-run flags, ambiguity/affinity, style runs, and local aggregate metrics. This can be `O(number of facts in the requested leaf)`, never merely `O(depth)`. | **Yes only in an active-source representation.** In the current v2 hidden-projection representation, switching to raw markers changes the platform text and display coordinate system, so it is unsafe during IME/gesture/edit leases. The `:15` assertion is a prediction-era stale-semantics requirement unless an urgent parser query certifies the updated run. | Make the RFC 023 active-source island an explicit product decision and port these tests to pin semantic styling, affinity, and no input rewrite. If launch requires v2 hidden syntax while focused, exact active projection facts become an urgent, lease-stable requirement; the old predictive carry-forward is not acceptable merely because its widget test passes. |
| Cross-block gestures, select-all, replacement, and deletion remain document-global and never bisect hidden markers. See `test/v2/flutter/flark_selection_gesture_test.dart:30`-`:93` and `flark_cross_block_selection_test.dart:48`-`:190`. | Total coverage forest plus exact local maps for every known or unknown leaf; canonical source anchors own selection. Finalized block records are insufficient by themselves. | **Yes**, if an unknown leaf has a 1:1 source map and selection is stored in source coordinates. Mixed known/unknown display is valid only when aggregate maps are exact and adoption remaps from source atomically. | Exercise forward/reversed gesture selection, partial cross-block selection, select-all, autoscroll, and edit-through-selection across known/unknown boundaries and across mounted/unmounted leaves. No display offset may survive as canonical state. |
| Blank parser-omitted gaps stay editable and can reclassify into quote/list/task/table while focus survives. See gap hosts at `flark_live_rendered_editable_text_test.dart:1141`-`:1276` and the source-host transition matrix at `:5818` with cases at `:7149`. | Exact total source coverage and stable edit-session/source anchor. A target-revision `UnknownRange` is enough while the new block is pending; a sealed/active presentation record is needed to adopt the block later. | **Yes. This is the suite's strongest direct support for `UnknownRange`.** | The same input session remains usable while pending; parsed structure adopts without source change, lost focus, selection drift, or lost following blocks. |
| Existing quote/list/task/code/table surfaces remain mounted through ordinary content edits and authoritative adoption. See the stability matrix at `flark_live_rendered_editable_text_test.dart:5754` with cases at `:7218`. | A mounted host/layout lease is enough for what this matrix actually asserts: the same keyed element remains mounted. Semantic contents may come from sealed/exact overlay facts, certified reused facts, or exact source fallback. | **Yes.** The host may source-paint without remounting. These tests do not require stale styling, marker hiding, task state, or table meaning to remain authoritative. | Keep host/shard identity independent from record authority. Port keys if necessary so fail-closed source painting does not look like a semantic record. |
| Prediction-era pending semantics appear in a few stronger tests: parser-blocked fence cases create code chrome with no authoritative full plan (`flark_live_rendered_editable_text_test.dart:2811`, `:3043`, `:3117`), and batched fence insertion before a following list expects both old list markers plus the new fence at `:5899`. | Urgent exact active/suffix facts or a parser-issued proof attached to the authored transaction. A host lease alone does not authorize code/list semantics. | **Not under the current assertions.** Exact fail-closed would source-paint, so these tests conflict unless the new parser can answer the urgent query independently of the deliberately blocked full parse. | Classify each as either a launch liveness invariant backed by an exact urgent path, or an intentional v2-prediction test to replace. Never preserve it through unproved stale facts. |
| Task, table, code, link, and image UX depends on semantic subtargets, not just block nesting. Tables require parser column count, row/cell source ranges, escaping maps, selection locality, and grouped IME; tasks require checked state and marker action; fences require body/info/closer ranges. See `flark_live_rendered_editable_text_test.dart:4969`-`:5690`, `flark_read_only_preview_test.dart:700`-`:1146`, and render descriptors in `test/v2/render_plan/flark_render_plan_test.dart:232`-`:330`. | Complete semantic-target records for the requested block, including children/cells and revision-scoped capabilities. A finalized inactive record may supply them; an edited/open compound block needs an exact overlay or certified reused facts. | **Yes for text preservation, no for interactive chrome.** Unknown/stale targets must not expose checkbox, link, table, language, copy, or mutation actions as current. | Record forest query returns semantic targets atomically with projection and capabilities. Flutter hides/disables stale actions immediately, but ordinary in-target edits retain focus and surface identity when the target is certified stable. |
| Enter/Backspace/format/table commands are context sensitive and sometimes must veto a plausible source rewrite. See `flark_markdown_input_commands_test.dart:14`-`:398`, cross-list style veto at `flark_cross_block_selection_test.dart:110`, table-inside-fence rejection at `flark_markdown_table_commands_test.dart:152`, and parser-judge tests in `test/v2/flutter/flark_parser_judge_test.dart:32`-`:155`. | Current `EditContext`: ancestry, marker/target ranges, capabilities, and, for grammar-sensitive proposals, an exact parser judgement. It may come from a sealed record or exact active overlay. | **Literal text mechanics are allowed. Grammar-sensitive commands must defer/no-op while unknown.** | Every command declares whether it is source mechanics, parser-context policy, or parser-judged. Unknown cannot silently fall through to a regex/source grammar scanner that claims Markdown meaning. |
| Read-only preview renders exact source while parse output is stale, then uses current facts for styling, semantics, links, code, quote, task, and table UI. See `flark_read_only_preview_test.dart:10`, `:26`, `:700`, `:921`, `:1017`, and `:1060`. | Exact source is sufficient for stale fallback; current sealed/overlay facts are required for rich rendering and actions. | **Yes.** The existing test explicitly pins source-visible stale preview. | Recovery from unknown to rich preview is revision-atomic. Accessibility/action nodes exist only for current semantic targets. |
| A later reference definition can alter inline recognition in an already closed prefix (`flark_projection_test.dart:999` exercises reference projection; the parser feature/conformance suites cover recognition). | Block finality and inline semantic finality are distinct. Prefix inline records need a matching reference-symbol generation/dependency proof, or must remain unknown/use certified reused facts. | **Yes for the affected inline leaf.** A false “finalized prefix” is not acceptable. | Edit/add/remove/winner-change reference definitions before and after consumers; no early prefix publication may mis-style or activate a consumer under the wrong generation. |
| Stale/superseded/failing parses never revive old facts. See `test/v2/flutter/flark_parse_scheduler_test.dart:10`-`:175` and stale delta rejection at `flark_flutter_controller_test.dart:73` and `:501`. | Revision/hash/provenance-scoped forest root and atomic multi-index adoption. | **Yes**, while the current revision remains pending. | Syntax, coverage, projection, selection reconciliation, semantic targets, and capabilities adopt as one revision. Stale partial pages and continuation tokens are rejected. |

## What “O(depth) open overlay” may safely mean

It may mean the **retained parser continuation state** is proportional to open
container depth (plus explicitly bounded reference/extension state). It may not
mean all publishable facts for an open region fit in `O(depth)`.

Examples:

- A 4 KiB active paragraph can have hundreds of exact emphasis/link/entity
  facts. The stack may be shallow while the projection output is dense.
- A table is one block at shallow depth but has `O(rows * columns)` semantic
  cell targets.
- A multi-line fence can be represented compactly by opener/info/body/closer
  ranges, but syntax highlighting and line-local layout facts are requested
  separately for visible body shards.
- An open blockquote/list needs ancestry plus per-line marker/content mapping;
  ancestry alone cannot drive one continuous rail or correct Backspace/Enter.

The clean split is:

```text
ParserJob
  continuation/frontier state        O(depth + bounded dependencies)
  sealed target-revision prefix      persistent record forest
  dirty/unknown total coverage       exact source metrics, no syntax claim
  active/visible fact query output   O(facts requested/emitted)
  reusable authoritative suffix      attach only after exact convergence

FlutterSnapshot
  current authoritative/certified-reused records
  exact unknown source leaves
  mounted host/layout leases (no semantic authority)
  canonical source selection + stable input island
```

## Record-forest gates to add

1. **Total coverage at every poll.** Known, unknown, and certified-reused fact
   ranges are ordered, non-overlapping, and cover the exact target source.
   Aggregate UTF-8, UTF-16, line, and display metrics remain queryable. Mounted
   host leases are not members of this semantic coverage partition.
2. **Separate finality dimensions.** Block structure, inline projection,
   reference resolution, semantic targets, and edit capabilities each carry
   explicit authority/generation; a closed block is not automatically final in
   every dimension.
3. **Frontier/open-block receipts.** At each fuel boundary, query empty quote,
   list, heading, open fence, nested quoted list, table candidate, and a dense
   inline leaf. Return exact overlay or one whole `UnknownRange`, never a
   plausible subset of facts.
4. **Active fact pagination/bounds.** Dense inline/table output is fuelled and
   bounded separately from `O(depth)` continuation storage. Partial pages are
   not renderable until their leaf-level completeness bit is true.
5. **Semantic target identity.** Table cells, task markers, code body/info,
   link/image actions, and gap hosts have stable source-anchored IDs across
   local edits and exact suffix shifts. Host/shard IDs remain a separate UI
   concern and do not imply the target record is current.
6. **Certified fact-reuse proof.** Reusing old fact payload requires unchanged
   source lineage plus equal relevant ancestor, reference, profile, and
   semantic-target state. Test local content edits, fence/list openers above the
   suffix, and reference definition edits. The latter two must revoke unsafe
   reuse. Without proof the host stays mounted but source-paints.
7. **Convergence remains authoritative.** No suffix record root is attached to
   the target forest before exact source alignment and complete continuation /
   dependency state equality, even if Flutter still has the suffix hosts
   mounted or is painting individually certified target-revision facts.
8. **Revision-atomic adoption and cancellation.** A poll cannot expose syntax
   without matching coverage/projection/capability generations. Superseded
   overlays and fact-reuse proofs cannot be promoted into a newer root.
9. **Fresh-parse equivalence.** Every fully converged root and every promoted
   active overlay must equal clean-parser facts for its covered range under the
   same symbol generation/profile.

## Flutter gates to add or port

1. **Known -> unknown -> known under active IME.** Text, composing range,
   selection, focus node, and input connection identity remain unchanged. This
   is the decisive gate for the active-source island.
2. **Explicit v2-compatibility decision.** Port the hidden-marker/run-edge tests
   to the active-source representation and state which visual assertions are
   intentionally changed. If hidden syntax remains a launch requirement, do
   not allow known/unknown representation switching inside an input lease.
3. **One-frame structural triggers with real partial output.** Re-run bare
   marker -> marker-space quote/list/heading, fence opener -> Enter, and the
   parser-blocked fence scenarios against the actual record-forest bridge. No
   Dart prediction grammar may fill the gap.
4. **Compound editor lease matrix.** Edit every table cell/task/fence target
   while the parser is pending, then test a preserving parse, a destructive
   reclassification, undo/redo, and IME commit. Safe cases keep surface/input
   identity; unsafe actions disappear before adoption.
5. **Mixed known/unknown selection.** Gesture, reverse, extend, select-all,
   copy, replace, and delete across projected inactive blocks, raw active/gap
   leaves, unknown dirty ranges, and virtualized unmounted ranges.
6. **Mounted suffix without premature fact reuse.** Edit before a visible
   quote/list/task/code/table suffix while convergence is artificially blocked.
   Suffix hosts remain mounted. Certified exact facts may keep rich rendering;
   otherwise the same hosts source-paint. An edit that opens a fence or changes
   ancestry/reference state must revoke semantics without forcing a remount.
7. **Command capability staleness.** While a range is unknown, literal typing
   works; context-sensitive Enter/Backspace/style/table/link commands defer or
   no-op, and no stale toolbar/checkbox/link action fires.
8. **Read-only degradation/recovery.** Unknown visible ranges show exact source;
   known surrounding blocks remain rich; recovery is atomic and semantics do
   not briefly expose stale actions.
9. **Deadline/device receipt.** The active-overlay query, bridge decode, forest
   adoption, coordinate reconciliation, and Flutter rebuild are measured
   independently. The one-frame cases above—not a synthetic plain edit—are the
   urgent latency corpus.

## Architectural consequence

No full redesign follows from this audit. The persistent coverage/record forest,
exact resumable parser, `UnknownRange`, source-first input island, and
convergence-gated suffix reuse still fit together cleanly.

The correction is to avoid making one data structure do four jobs. Parser
continuation state, authoritative/queried presentation facts, mounted
host/layout identity, and active input state have different size and validity
rules. Keeping them explicit removes the need for a second predictive grammar
while preserving the behaviors the suite says make Flark feel live.
