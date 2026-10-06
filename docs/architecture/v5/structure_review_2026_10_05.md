# Structure review — 2026-10-05

Base: `c3ae214c`, the head of PR #67 before this review. Apple M1 Pro, macOS
26.2, Dart 3.12.2, Flutter 3.44.4, load average 8 to 22. All evidence is
local; CI ran on the pushed intermediate head `4e07db3c` (all seven jobs
green).

The review asked one question of every mechanism in the editor: **what
requires it?** A mechanism is justified when a direct scenario, a conformance
case, a rule of the [edit profile](edit_profile_v1.md) or a named consumer
fails without it. Complexity the problem forces stays, and is written down
below. Complexity that only patches another mechanism, decides something a
second time, or survives because nothing exercises it goes. The cleanup in
PR #67 applied that rule; the second half of this document is the structure
the code should grow into next.

## Method

Six audits each took one area: typing and respelling; deletion, joins and
Return; the edit pipeline; projection and document; the code, link and
style commands; the two hosts. Claims were verified by mutation, not by
reading: remove a clause, run every suite, see what fails. Of 101 deletion
and Return clauses mutated one at a time, 29 failed nothing; each was then
either pinned by a scenario that needs it or removed with the reason given.

Every refactor was held to a **behaviour fingerprint**: 5,000 seeded
sequences (200,000 commands, with compositions, source splices, source-mode
trips and a full undo/redo walk each) recording after every step what a host
can observe — applied or refused and why, source, selection, typing context,
composing and source mode, style states, resource availability, a digest of
every projected row and of the legal caret offsets. A refactor had to leave
the 398,115-line transcript byte-identical. An intended fix had to account
for each sequence it changed; a variant run at the source limit covered the
refusal policy, which ordinary sequences never reach.

## What is essential

Most of the kernel's size belongs to the problem:

- **Validation by re-parse, with respellings.** CommonMark reads blocks in
  context — lazy continuation, setext underlines, item content columns, HTML
  end conditions, tables — so the same keystroke means different blocks
  beside different neighbours, and only the parser can say what an edit did.
  An edit is tried as asked, then as faithful respellings (an escape, a
  separating blank line, a container prefix); the first whose re-parse keeps
  the document's structure commits, and where none does the edit refuses
  rather than restructure.
- **The live tier.** Past it no parse checks a respelling, so only the edit
  as asked may leave it (EP1-RESULT-PRESENTATION-001), with the documented
  typed-line exception.
- **Input methods.** The platform owns the preedit. Typing's
  transformations apply once, at commit, as one undo step, and a commit
  typing refuses is withdrawn: literal preedits, a sandboxed retype and a
  rollback follow from that.
- **Unwritten table cells**, materialized privately, with history recorded
  against the original state.
- **The display/source mapping**: hidden ranges, anchors as typing context,
  entities and escapes in non-exact segments, tab columns, atomic CRLF.
- **Measured performance machinery**: incremental projection reuse (−17% VM
  keystroke at 32 KiB; about half the browser projection), the linear
  legality index, the binary searches that replaced measured quadratic scans.
- **Host realities**: platform text buffers, IME and accessibility engine
  bugs (flutter/flutter#193846, #193847), terminal cell rules.

## What was accidental

The audits found the same five shapes in every area.

1. **One model, written many times.** "Try the edit as asked, then its
   respellings; re-parse; commit the first that keeps the structure" was
   hand-rolled in about ten loops with three edit encodings and four offset
   maps. Their policies differed by accident: on a respelling the parser
   refused, some stopped and some went on; which spelling could leave the
   live tier varied; every loop re-derived the outcome from `_lastRejection`,
   `_inert` and closure flags.
2. **One decision in two places, drifting.** Two structural checks mapped an
   offset at an edit's start differently, refusing typing into an empty
   item. Of two input routes only one had received the lone-checkbox fix.
   Typing and Return computed a lazy line's prefix differently (a
   documented residual), Return and Backspace a container exit (a copied
   item marker), deletion and Return "all of a heading's text" (a deleted
   image), and the session's Undo pre-check disagreed with the kernel.
3. **Implicit structure, re-derived.** The projection never labelled bare
   markers, delimiter rows shown as source or bodyless fences, so the editor
   decoded them at 16 sites, and it found display lines by scanning for `\n`
   although the projection marks line breaks (`&#10;` made a heading command
   refuse).
4. **Dead and unpinned code.** A respelling path no input reaches, guards no
   row reaches, `History` and `FlarkDocument` API nothing calls, and a
   quarter of the deletion and Return clauses pinned by nothing.
5. **Code in the wrong layer.** The recorder inside the editor library with
   two closures per public call; hosts re-deciding availability the kernel
   owns (a link dialog that could only fail), moving the caret vertically by
   pointer rules, copying and pasting by different rules.

## What PR #67's cleanup did

91 commits on top of `c3ae214c`; every suite green at each merge.

- **One candidate search.** `Edits` (`edits.dart`, unit-tested: sorted
  splices, `apply`, `forward` with a row or caret bias, `back`), `Spelling`,
  and `_commitSpellings`, which owns the live-tier rule, one refusal policy
  (the edit as asked's refusal refuses it; a respelling the parser cannot
  take is passed over), the rule that an unchanged candidate must still pass
  its check, and a typed `_Outcome`. Typing, deletion, joins, lifts, Return,
  heading levels, link and image removal and code fences all search through
  it. One `_EditState` record replaces four hand-picked rollback subsets.
- **One decision in one place.** One entry step for every public call; one
  input route; one set of container comparators instead of six; one offset
  convention; explicit row queries (`row_queries.dart`) and display lines
  from line-break segments; one owners-of-row query; one prefix rule for new
  code lines; one style-to-delimiter map; one admission pass.
- **Recorder behind one hook.** Calls are sealed `FlarkEditorCall` values
  emitted through `FlarkEditor.onCall`; `FlarkEditRecorder` is its own
  library reading only public state.
- **Dead code removed, unpinned code pinned.** Each clause the audits found
  unpinned now has a direct scenario that fails without it, or is gone with
  the reason in its commit. `quick` proved to be a typing rule, not a
  shortcut, and is named `ordinary` and pinned.
- **Hosts.** Both open the resource editor only where the kernel can set a
  resource; the Flutter toolbar reads one `FlarkState` through one command
  path; the surface asks its controller whether it composes.
- **Nineteen bugs fixed**, each with a direct test that failed before: the
  six drifts above; `&#10;` refusing a heading level and moving a word
  delete; a heading under a table becoming a table row; a stuck Backspace
  on an empty line after a table; RemoveLink escaping twice and adding
  needless backslashes; RemoveLink leaving the live tier unchecked; a `\r`
  pasted into code; Shift-Tab with a tab step; stale refusals after
  composition and source-mode calls; a refused span continuation reported on
  an edit that applied; an undo step for a splice that changed nothing; the
  caret left past a removed fence; the link dialog that could only fail; and
  Fleury's source fallback splitting a surrogate pair.

| Measure | `c3ae214c` | Cleanup head |
| --- | ---: | ---: |
| `packages/flark/lib` production lines | 10,452 | 10,410 |
| `editor.dart` / `typed_lines.dart` | 3,462 / 691 | 3,291 / 563 |
| `_lastRejection` references in the kernel | 57 | 29 |
| Candidate loops | ~10 | 1 |
| flark tests | 1,106 | 1,173 |
| Insert p50, two alternating rounds (`bench_editor.dart 16`) | 1.21, 1.17 ms | 1.20, 1.18 ms |
| Backspace p50, same rounds | 1.25, 1.21 ms | 1.25, 1.23 ms |

The line count barely moved, and that is the finding: what the cleanup
removed was duplication, drift and dead code; what remains is the essential
list above. Fewer concepts, not fewer lines, is what changed.

## The structure to grow into

The kernel is still one class, `FlarkEditor`, with `part` files of private
extensions over its private state. Every area can read and write every
field, so boundaries are conventions, and a special case can be added
anywhere — which is how one decision came to live in two places. The next
step is to make the layers explicit:

| Layer | Owns | Today |
| --- | --- | --- |
| Parse (Rust) | All Markdown recognition; every range Dart needs | `native/flark_parse` |
| Model views | Typed views of the flat records; named flag bits generated from the schema | `parse/` |
| Positions | Legality, anchors, owners, resources, as a function of source, model and projection | `document.dart` |
| Projection | Rows, segments, shells, caret spans; special rows labelled by their builders; reuse by origin | `projection.dart`, `row_queries.dart` |
| Edit core | `Edits`, spellings, the one search, one structural check with named exemptions, the sole publisher, `EditState`, the outcome | `edits.dart`, `_attempt`, `_commitSpellings` |
| Command areas | Typing and placement; deletion and joins; Return; block structure; formatting; resources; code (a `CodeBody` value); tables. Each builds spellings and names its check | the `part` files |
| Editor facade | The entry step, composition policy, history, listeners, the call hook | `editor.dart` |
| Session | Parser leases, readiness, published state; maps the outcome one to one; asks the kernel what is available | `session/` |
| Hosts | Input adapters, keyboard maps, geometry as pure queries, paint, semantics, toolbars reading one `FlarkState` | `flark_flutter`, `flark_fleury` |

The rule that makes it hold: **a command area depends only on the layers
below it.** It receives the state it needs and returns spellings with their
check; the edit core decides what commits. A special case then has one
home, and a second home shows up in review as a second import.

## Sequence

**Owner decision, 2026-10-06:** no large re-architecture. The layer
reorganization (steps 1 to 3, 6 and 7) is not planned; small cleanups that
need no restructuring go ahead instead: named flag bits, kernel availability
queries for the hosts, session fixes, refreshed documentation. The steps are
kept as a record of what the structure would take.

Each step keeps every suite green, is gated by the fingerprint, and lands
as its own PR.

0. **Tools.** Commit the behaviour fingerprint as a tool that compares the
   working tree with any ref, and a line-budget check that CI runs (see
   Budgets). Both are small; the fingerprint is what made this review's 91
   commits safe.
1. **Finish the edit core.** One structural check configured per command
   (`edited`, kind and container rules, shown text, hidden source,
   exemptions), replacing `_keepsStructure`, `_keepsTyped`, `_showsRows` and
   `showsBreak` and mirroring the exemptions `structure_oracles.dart` already
   names. Typed fence and setext completion move out of `_attempt` into
   typing's spellings. A public `FlarkOutcome` (applied, unchanged,
   refused with a reason, plus a withdrawn composition) replaces
   `lastRejection` as the channel hosts and the session read; `History`
   becomes private behind `canUndo`/`canRedo`. About −100 to −150 lines;
   medium risk.
2. **Command areas as libraries.** Each part becomes
   `kernel/commands/<area>.dart`, receiving an edit-core interface instead of
   extending `FlarkEditor`: resources, formatting, tables and code first,
   then Return, deletion and joins, typing. Mostly moves; the gain is that
   the boundaries are enforced by the compiler. Medium risk, fingerprint
   gated.
3. **Projection.** Builders set the special-row labels instead of
   `row_queries.dart` decoding them; one builder for rows shown as source
   (definitions, bare markers, delimiter rows; −60 to −80); one caret-span
   representation.
4. **Parse-crate gaps.** A content record for a table delimiter line (so
   `_delimiterRow` stops reading source through `continuationPrefix`),
   escaped pipes as replacement pieces (about −70 Dart lines in two
   mechanisms), the closing fence's range, and named flag constants generated
   from `render_model_v4.json`. Needs a Wasm rebuild and the transport check.
5. **Kernel queries for hosts.** `canSetHeading`, `codeInfo(row)`, a copy
   rule (`selectedText`), a read-only composition view, a keyboard placement
   command for Up/Down, `ProjectedRow.samePresentation`, and one resource
   request lifecycle; `FlarkState` as the only toolbar model; the session
   stops validating and counting UTF-8 on its own. About −100 to −200 host
   lines.
6. **Hosts.** Split flark_flutter's `editor.dart` (1,736) and `surface.dart`
   (1,592) by responsibility — input connection, keyboard map, touch
   selection, resource UI, toolbar; row layout, geometry as pure queries,
   paint, semantics — move Fleury's preedit adapter and keymap out of
   `editor_view.dart`, give Fleury one command path, and retire the legacy
   controller API so each host has one public API.
7. **Code delegate.** The delegate proposes typing and Return only; the
   kernel owns Tab and Shift-Tab (today the delegate's Tab indents blank
   lines the kernel's leaves); highlighting is a separate language service.
8. **Recorder placement.** If it should not ship in the published package,
   move it to a development library or package: −345 lines from `flark`.
9. **Repository.** `legacy/` holds 1,192 files and about 600,000 lines of
   superseded code that git history already keeps; remove it. Fold the 50
   documents under `docs/architecture/v5`, most of them dated reviews, into
   one living architecture document (this structure, the edit profile, a
   decisions log) with the reviews archived, and refresh `build_plan.md`,
   which still schedules `flark_tree_sitter`.

## Budgets

The build plan's line budgets are gates on paper only: nothing checks them,
the kernel passed 8,000 on main before this PR, and flark_fleury stands at
3,696 against 3,000. The 8,000 figure for `packages/flark/lib` predates the
session API (779 lines), the web transport and model views (958) and
validated respelling. Budgets should follow the essential list, per
directory, and run in CI so that raising one is a reviewed change with its
reason:

| Directory | Today | Proposed budget |
| --- | ---: | ---: |
| `packages/flark/lib/src/parse` | 958 | 1,000 |
| `packages/flark/lib/src/session` | 779 | 800 |
| `packages/flark/lib/src/kernel` | 8,463 | 8,500 |
| `packages/flark/lib/*.dart` (public helper libraries) | 210 | 250 |
| `packages/flark_flutter/lib` | 5,111 | 6,000 |
| `packages/flark_fleury/lib` | 3,696 | 3,800 |
| `native/flark_parse/src` (excluding generated) | 2,918 | 3,000 |

Steps 1 to 3 and 8 should bring the kernel under 8,000 again without
touching the essential list; each budget would then come down with the step
that earns it.

## Decisions for the owner

1. **Budgets**: adopt per-directory budgets enforced in CI, starting from
   the table above.
2. **Refusals at the source limit**: with one refusal policy, a `\` typed at
   a line's end now becomes a hard break, and a `-` completing a delimiter
   row lets the table take the next line, where the respelling that keeps
   the line no longer fits; before, the keystroke was refused. Keep, or name
   an exception for `_unhideLine`.
3. **Formatting and links at the live tier**: they are refused where typing
   may cross the tier. Let them cross, or amend EP1-RESULT-PRESENTATION-001.
4. **The typed-underline exception at a limit** (a `-` under a paragraph
   makes a heading rather than entering source mode): keep or drop.
5. **Keyboard Up/Down** by the kernel's movement rules rather than pointer
   placement: changes where the caret lands at span edges.
6. **Copy and paste**: Flutter copies visible text and Fleury copies
   Markdown; only Fleury normalizes pasted line breaks.
7. **Recorder**: ship it in `package:flark` or keep it for development.
8. **Unconsumed public API**: `ProjectionOptions.softBreakAsNewline`,
   `Style.footnoteRef`, `Style.htmlInline`.
9. **Return at a quoted table's last row** is refused; exiting with the
   container's prefix would be better.
