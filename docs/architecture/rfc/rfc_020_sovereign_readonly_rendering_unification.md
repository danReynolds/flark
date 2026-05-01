# RFC 020: Sovereign Read-Only Rendering Unification

**Status**: IN PROGRESS (Waves 1-4 detail-surface scope delivered)  
**Author**: Codex (draft for Dune engineering review)  
**Date**: 2026-03-16  
**Related docs**:
- `docs/architecture/sovereign_markdown_profile_v1.md`
- `docs/architecture/sovereign_editor_markdown_support_matrix.md`
- `docs/architecture/rfc/rfc_009_commonmark_native_execution_plan.md`
- `docs/architecture/rfc/rfc_016_sovereign_markdown_command_layer.md`

Implementation update (2026-03-16):

- `SovereignMarkdownView` now exists in package API and uses sovereign
  parse/render primitives.
- Focused post detail screens are cut over to `SovereignMarkdownView`.
- Read-only inline action overlay now supports open/copy/edit actions for
  link and image markdown targets.
- Dedicated read-only parity regression suite added for
  heading/quote/list/task/fence/link/image/thematic-break semantics.
- Enforced benchmark lane currently misses strict scanner/render budgets in
  local runs; detail surfaces remain the rollout boundary while we tune budgets
  and/or optimize hot paths before feed/card adoption.
- Feed excerpt previews now render markdown via `PostReadMarkdown` for both
  non-clamped and clamped paths (clamped path uses sovereign clip + read-more overlay).

Rollout decision status (current):

- **Detail screens**: use sovereign read-only renderer now.
- **Feed/excerpt screens**: cut over for markdown content to sovereign read rendering.
  Benchmark lane remains the gate for any further broad list-surface expansion.

## 1. Context and Problem

Dune currently uses two different markdown rendering stacks:

1. **Edit mode**: Sovereign editor path (`SovereignEditor` + native `comrak` parse + sovereign projection/rendering).
2. **Read-only mode**: `flutter_markdown`/`package:markdown` path (`PostMarkdownBody`).

Even with theme harmonization, discrepancies persist because the systems differ in:

- parser implementation and extension handling,
- line/layout/bullet/blockquote/code block rendering behavior,
- interaction model (caret/marker projection vs static markdown widgets),
- feature-specific behavior (links/images/task checkboxes/code treatment).

Result: edit/read visual drift, ongoing parity patches, and recurring regression risk.

## 2. Goals

1. Eliminate edit/read visual drift for markdown content.
2. Keep one authoritative markdown rendering contract for native targets.
3. Reduce long-term maintenance cost from dual renderer logic.
4. Preserve strong runtime performance and stable UX in read-heavy surfaces.

## 3. Non-Goals

1. Replacing sovereign editing internals.
2. Rewriting markdown semantics contract (Profile v1 remains authoritative).
3. Adding web runtime support in this RFC.
4. Solving all feed virtualization issues in the first iteration.
5. Introducing a persistent parse worker-isolate architecture in the initial
   detail-surface rollout.

## 4. Options Considered

### Option A: Keep dual stacks, keep tuning style parity

**Pros**
- Lowest short-term implementation cost.
- No architectural changes to post rendering path.

**Cons**
- Does not solve parser/renderer behavior divergence.
- Ongoing maintenance tax.
- Regressions likely reappear with new markdown features.

### Option B: Add `readOnly` flag directly to `SovereignEditor`

**Pros**
- Reuses existing sovereign render path quickly.
- Improves parity immediately for many cases.

**Cons**
- `SovereignEditor` is editing-first (`TextField`, caret/focus/shortcuts, edit overlays).
- Risk of carrying unnecessary edit machinery into read-only surfaces.
- Potential perf and semantics overhead in feed contexts.
- Tends toward mode-branch complexity inside one widget.

### Option C (Recommended): Introduce dedicated `SovereignMarkdownView`

**Pros**
- True parser/render parity with Sovereign while keeping a read-only-optimized widget.
- Cleaner architecture: edit and read share core renderer/snapshot contracts, not UI plumbing.
- Better long-term surface for performance tuning and feature parity tests.

**Cons**
- Higher initial implementation effort than Option A/B.
- Requires migration of some post-view affordances (excerpt/fullscreen code/link/image interactions).

## 5. Decision Summary

Proceed with **Option C**: build a dedicated read-only sovereign renderer surface
(`SovereignMarkdownView`) instead of retrofitting `SovereignEditor` with a
simple `readOnly` toggle.

Rationale: parity needs parser+renderer unification, but read-only surfaces
need a different interaction/performance profile than edit surfaces.

Rollout constraint:

- Initial implementation prioritizes correctness/parity on focused/detail
  surfaces without adding a worker-isolate parse queue.
- Worker-isolate parsing is a conditional follow-up only if measured
  performance data justifies it for broader feed/card rollout.

## 6. Proposed Architecture

### 6.1 Public API (draft)

```dart
class SovereignMarkdownView extends StatelessWidget {
  final String markdown;
  final MarkdownSyntaxProfile profile;
  final SovereignEditorThemeData? theme;
  final bool selectable;
  final bool showLinkActionsOverlay;
  final Future<void> Function(String url)? onOpenLink;
}
```

### 6.2 Internal model

- Reuse sovereign syntax pipeline and snapshot normalization.
- Reuse sovereign block/background/inline rendering primitives.
- Remove edit-specific pieces from view path:
  - no text mutation policies,
  - no command/undo machinery,
  - no caret-dependent editing intents.
- Keep read-only interactions that matter:
  - link/image actions (open/copy/edit where applicable),
  - optional selection/copy behavior,
  - optional code-block expand/fullscreen hooks.

### 6.3 Parser contract

- Use the same native commonmark backend/profile contract as edit mode on native targets.
- Preserve UTF-16 offset contract in all externally visible ranges.

## 7. Migration Plan

### Phase 1: Build and validate `SovereignMarkdownView`

1. Add new view widget in `sovereign_editor` package.
2. Wire rendering from authoritative syntax snapshot + existing renderer.
3. Add focused tests:
   - visual parity with editor theme contracts,
   - links/images/task-list read interactions,
   - code-block presentation behavior.

### Phase 2: Introduce in focused post surfaces

1. Replace `PostMarkdownBody` in focused post/detail view first.
2. Keep existing `flutter_markdown` path as fallback during bake-in.
3. Run UX parity review (desktop/mobile).

### Phase 3: Evaluate feed/card surfaces

1. Benchmark memory/layout/scroll performance with large lists.
2. Decide final rollout scope:
   - sovereign view everywhere, or
   - sovereign view for detail + existing path for large feed cards.

### Phase 4: Cleanup

1. Remove duplicated style-mapping glue that only existed for cross-renderer parity.
2. Update docs/support matrix to make read rendering contract explicit.

### Phase 5: Feed/card rollout decision

1. Benchmark memory/layout/scroll performance on candidate feed/card surfaces.
2. If budgets are met, proceed without parser concurrency architecture changes.
3. If budgets are not met, open a follow-up RFC/work item for worker-isolate
   parse queue + caching as a targeted optimization.

## 8. Testing and Acceptance Gates

1. **Parity contract tests**
   - same markdown + same theme -> no visual class drift between edit and read surfaces.
2. **Feature regressions**
   - headings, blockquotes, lists/task lists, fenced code, links/images, thematic breaks.
3. **Interaction tests**
   - read-only link/image open/copy/edit behavior.
4. **Performance gates**
   - frame budget in long post/detail view,
   - memory budget in list scenarios before feed/card rollout.

## 9. Risks and Mitigations

1. **Risk**: read view still too heavy for large feed lists.  
   **Mitigation**: keep phased rollout; benchmark before feed cutover; add
   worker-isolate queue only if data shows need.

2. **Risk**: duplicated logic accidentally remains in app layer.  
   **Mitigation**: keep markdown read rendering inside package; app composes only.

3. **Risk**: mode-specific branching regresses maintainability.  
   **Mitigation**: separate read widget from edit widget instead of deep read-only branches.

## 10. Open Questions for Review

1. Should task checkboxes in read view be non-interactive or toggleable? -> non-interactive
2. Should read view show inline link/image action overlays by default? -> no
3. For large feeds, do we prefer: -> feeds will be small
   - sovereign view only in detail surfaces, or
   - sovereign view everywhere with aggressive caching/virtualization?
4. How much post-specific behavior (excerpting/fullscreen code browser hooks) should live in package vs app? -> im thinking this should be app, since it's pretty app-specific? Thoughts?

## 11. Go/No-Go Criteria

Go to rollout when:

1. parity regressions are materially reduced versus current dual-stack baseline,
2. focused post surfaces meet UX/performance expectations,
3. no blocking accessibility regressions are introduced.
