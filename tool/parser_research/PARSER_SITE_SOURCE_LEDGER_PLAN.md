# Parser-site source-ledger plan

Status: **Stage 0 contract gate passed; source-bound Stage 1 spine in progress**, 2026-07-16.

Scope: the selected direct parser output port, specifically every source-consuming
decision in `comrak_value_block_core/src/parser.rs` and `src/table.rs`. This is
not a proposal to translate the donor's chronological events. It specifies how
the block parser can build one total physical source partition and its logical
projection directly, without copied aggregate leaf strings, historical source
position repair, or a second Markdown classifier.

## Conclusion

The direct output port remains feasible, but `advance_offset` and `add_line`
cannot be instrumented as if they were source-ledger operations:

- `advance_offset` moves a grammar cursor. In columns mode it can consume only
  part of a tab without advancing the physical byte offset. A grammar-column
  consumer is therefore not necessarily the physical owner of that tab.
- `add_line` combines at least four responsibilities: selecting physical source,
  expanding a partially consumed tab, extending a logical terminal, and copying
  an aggregate string. The selected architecture needs the first three as typed
  source actions and must remove the fourth.
- blank-line ownership and the final line terminator are not always knowable at
  the moment their bytes are read. Both need small, serializable pending states;
  neither justifies a repair log.
- Setext headings, reference-only paragraphs, and tables need typed retroactive
  range mutation. They cannot be represented correctly by translating the
  donor's event order after the fact.

The smallest clean seam is an exact per-line source ledger plus stable semantic
owner leases. Every committed line must finish as an ordered, disjoint, total
partition of its source lease. Claims name physical owner, source part, and
logical contribution independently.

## Stage 0 result

The standalone contract now exists in
`comrak_value_block_core/src/source_ledger.rs` with ten adversarial tests in
`tests/source_ledger_contract.rs`. It streams validated claims directly to its
caller and retains no claim `Vec`, logical `String`, owner map, or catch-all
"claim remaining" operation. Fixed state tracks only source/claim cursors,
byte/UTF-16 metrics, pending-tail authority, counts, and a deterministic
non-cryptographic debug digest.

The gate proves ordered/disjoint/total claims, poison-on-failure, exact
root/revision/snapshot/line/owner/target scoping, Unicode metrics and UTF-8
boundaries, exact tab/NUL/CR/LF/CRLF physical recipes, typed recoverable
gap/terminator tails, and executable indent/non-closing-ATX partitions. The
focused suite, complete all-target corpus (including the 1,322-document resume
lane), formatting, regular baseline Clippy, and a rustup-toolchain Wasm check
pass.

Stage 0 is deliberately not the production source adapter. Its borrowed `str`
derives metrics with a second line scan, and its private root/snapshot nonce is
a harness stand-in. Stage 1 must stream from the candidate-owned Crop cursor,
reuse the exact `LiveCandidateEpoch`/`ArenaBuildId`/source descriptor, mint
bindings from the real open path, certify surviving ancestry for gaps, and
coalesce arbitrarily many pending blank lines into one O(1) restart value.

## Terms and invariants

For one `SourceLineLease`:

- `N` is its byte length.
- `E = N - newlines_of(line)` is the end of non-line-ending bytes.
- `eol = [E, N)` is empty, `\n`, `\r`, or `\r\n`.
- `o` is the grammar byte offset before a decision.
- `F` is `first_nonspace` for that decision.
- all ranges below are half-open byte ranges relative to the line lease.
- byte and UTF-16 metrics come from the source capability. They are never
  reconstructed from parser columns or logical text.

The ledger must enforce:

1. Every physical byte is claimed exactly once, including BOMs, indentation,
   markers, ignored reference definitions, extra table cells, and line endings.
2. A claim's `physical_owner` is an already-created stable binding or the
   document binding. It is independent of its logical consumer.
3. A source part does not imply a logical transform. Each claim explicitly says
   `None`, `Identity`, `Atomic`, or `Program` and, when non-`None`, names the
   logical target.
4. Parser recognition may scan speculatively, but it emits no claim until the
   branch commits. Failed or rolled-back scans have no output effect.
5. A line cannot publish until its claims cover `[0, N)` or one explicitly typed
   pending range holds the uncovered suffix/gap.
6. Output work is fuelled. Opening a deeply nested container, scanning a giant
   marker candidate, and producing thousands of table-cell actions must all be
   resumable.
7. Every pending ledger state is revision-bound and is included in restart
   equality/adoption. A restart can occur after every physical line.

### Source parts

The existing names are retained with these meanings:

| Part | Meaning |
|---|---|
| `CONTENT` | Physical bytes participating in a terminal's source-backed logical stream. The logical action may still hide, replace, or normalize some bytes. |
| `CONTAINER_MARKER` | Quote/list prefixes, table row/cell delimiters, or code-body deindent serving an enclosing construct. |
| `BLOCK_MARKER` | ATX/fence/setext/thematic/table-delimiter syntax belonging to the block it establishes. |
| `GAP` | Exact editable source that is neither terminal content nor syntax: stripped indentation, blank gaps, BOM, and finalized reference definitions. |
| `TERMINAL` | A trailing physical line terminator held as the block's boundary rather than coalesced internal content. Its explicit logical action may be a canonical newline or `None`, depending on the block. This meaning is proposed here and requires a schema-versioned contract decision. |

`CoverageRun` currently carries only identity, metrics, relative owner depth,
and `CoveragePart`. That is insufficient. The production schema needs an
orthogonal logical action/target field. It must not infer the transform from
`part`.

### Physical owner, grammar-column use, and logical consumer

These are deliberately separate questions, but partial tabs narrow rather than
expand the retained schema. After a quote/list/code prefix consumes some of the
columns represented by a tab, it has not consumed the physical byte. The ledger
defers that indivisible byte to the terminal as `CONTENT`; an
`Atomic(TabExpansion { spaces })` contributes only the remaining logical
spaces. The prefix consumed grammar columns, not source ownership.

Parser actions should still name the intended logical terminal so the line
ledger can fail closed. Whether the packed record must retain an explicit
consumer depth is reopened: if every non-`None` contribution is structurally
inside and feeds the innermost compatible terminal, the green cursor can derive
that consumer from the open path and omit the redundant field/channel. An
explicit same-path consumer belongs in the codec only if a supported fixture
proves a non-`None` contribution to some other open block. Neither design may
introduce a BlockId directory or infer semantics from `CoveragePart`.

## Minimal parser/output API

Names are illustrative; the semantic separation is required.

```rust
begin_line(SourceLineLease) -> LineLedger

advance_columns(line, count) -> CursorAdvance {
  fully_consumed_source: SourceSpan,
  partial_tab: Option<PartialTab { source_byte, consumed_columns, remaining_spaces }>,
}

claim(SourceClaim {
  source: SourceCapability,
  physical_owner: OpenBindingRef,
  part: CoveragePart,
  logical: LogicalAction, // None | To(target, Identity | Atomic | Program)
  affinity: BoundaryAffinity,
})

stage_pending_gap(PendingGapRange)
stage_pending_terminator(PendingTerminator)
finish_line() -> LineLedgerReceipt
```

Required typed helpers are:

```text
advance_and_claim_committed_prefix
append_terminal_slice
resolve_pending_gap(surviving_open_path | eof)
resolve_pending_terminator(continue_same_terminal | close)
finalize_reference_prefix
promote_setext
promote_table_header
unwrap_semantic_wrapper_preserving_coverage
```

`LineLedgerReceipt` validates ordered, disjoint, exact coverage and records the
source-root generation used by every capability. There must be no generic
"claim whatever remains" fallback: that would recreate the missing Markdown
classifier at the sink.

Semantic bindings are opened before their first owned claim. An `Enter` may be
transaction-local until all mandatory facts are known, but the source ledger
must never refer to a numeric donor node handle or backpatch an already-published
event.

## Exhaustive parser-site matrix

“Lease timing” states when the physical owner must exist. “Mutation” describes
typed output mutation, not donor-tree mutation. Every fixture is an exact input
unless it contains the named generated repetition.

| Site | Committed physical interval | Lease timing and owner | Part and logical contribution | Retroactive mutation | Yield / fanout | Exact adversarial fixture |
|---|---|---|---|---|---|---|
| BOM, `parser.rs:313-315` | First line `[0,3)` when it starts with UTF-8 BOM | Document binding exists before `begin_line` | Document `GAP`, `None` | None | Constant | `\u{feff}>\tα\r\n` |
| Lazy paragraph continuation, `parser.rs:488-494`, `add_line` at `:493` | `[o,E)` plus staged `eol`; importantly starts at current `o`, not `F` | Existing Paragraph lease | Paragraph `CONTENT`, `Identity`/tab `Atomic`; terminator policy below | Resolve preceding pending terminator as internal content | Source-run append is bounded | `> a\nlazy *b*\r\n` |
| Existing indented/fenced code dispatch, `parser.rs:514` | From current `o` to `E`, with partial tab handled separately, plus staged `eol` | Existing Code lease | Code `CONTENT`; `Identity` or tab `Atomic`; `eol` uses the terminal policy | Close-time code projection facts only | Raw append bounded; close scanner fuelled | <code>```x\n\tα\r\n```\n</code> |
| Existing HTML dispatch, `parser.rs:515-519`, `add_line` at `:516` | `[o,E)` plus staged `eol`; do not strip the HTML block's own up-to-three leading spaces | Existing HTML lease | HTML `CONTENT`, source-backed `Identity`; final `eol` may be `TERMINAL` but still contributes the literal newline | Close attaches literal/end facts | End recognizer must be refillable | `   <!-- a\r\n b -->\r\n` |
| Generic accepted-line indentation, `parser.rs:522-538`, advance at `:536`, add at `:537` | Fully advanced bytes in `[o,F)`, then terminal `[F,E)`; partial tab byte is excluded from prefix | Existing accepts-lines owner and nearest surviving pre-existing nonterminal container | Prefix: container `GAP`, `None`; it has no terminal consumer. Remainder: terminal `CONTENT` | Pending terminator of same terminal becomes internal content | One claim per coalescible run | `alpha\n   beta\n` and `> alpha\n>   beta\n` |
| New paragraph indentation/content, `parser.rs:541-549`, advance at `:547`, add at `:548` | Fully advanced `[o,F)`, then `[F,E)` and staged `eol` | Create Paragraph before content claim; prefix belongs to pre-existing nearest container | Prefix `GAP`, `None`; Paragraph `CONTENT`, `Identity`/tab `Atomic` | None | Constant claims; reference scan is separately fuelled | ` \tα😀\r\n` |
| Quote continuation prefix, `parser.rs:606-621`, advances at `:616`, `:618` | Fully advanced indent plus `>` and fully consumed optional one space/tab | Existing BlockQuote lease | Quote `CONTAINER_MARKER`, `None`; partial optional tab remains for terminal | None | Constant | `>\tα\n> \tβ\n` |
| Item continuation, `parser.rs:623-641`, advance at `:636` | Fully advanced `marker_offset + padding` columns | Existing Item lease | Item `CONTAINER_MARKER`, `None`; partial tab remains terminal content | May resolve pending blank gap to Item when this line proves continuation | Constant | `- a\n\n  continuation\n` and `- a\n   \tb\n` |
| Blank inside item, `parser.rs:638-641`, advance at `:640` | Fully advanced whitespace `[o,F)`; any remaining byte and `eol` join pending blank range | Existing ancestry is candidate, not final owner | `GAP`, `None`, staged in `PendingGapRange` | Resolve/lift at next nonblank or EOF | Arbitrarily many compatible blanks coalesce O(1) | `- a\n \n\t\n  continuation\n` |
| Indented-code continuation, `parser.rs:654-672`, advances at `:664`, `:669` | Nonblank: fully advanced four columns. Blank: fully advanced whitespace, then line ending | Existing IndentedCode lease | Prefix Code `CONTAINER_MARKER`, `None`; partial tab becomes Code `CONTENT` `Atomic`. On a blank code line, stripped spaces are Code `CONTENT` with `None`, while its newline contributes to the raw stream; close-time projection may trim it | Close-time trim and virtual final newline are projection facts, not source deletion | Append bounded; final trim summary O(1) | `\tfoo\n   \tbar\n    \n` |
| Closing fence, `parser.rs:674-687`, advance at `:685` | Exact accepted close: leading indentation, complete fence run, allowed trailing whitespace, excluding `eol` | Existing FencedCode lease | Non-EOL Code `BLOCK_MARKER`, `None`; `eol` Code `TERMINAL`, `None` | Close attaches info/literal/source-end facts | Scanner must return exact span and yield on giant runs | <code>```x\na\n````  \r\n</code>; reject <code>```` x\n</code> |
| Fenced-code body deindent, `parser.rs:688-693`, advance at `:692` | Each fully advanced leading byte up to `fence_offset`; partial tab excluded | Existing FencedCode lease | Prefix Code `CONTAINER_MARKER`, `None`; rest Code `CONTENT`; partial tab `Atomic` | None | Constant | `  ```\n \talpha\n  ```\n` |
| New quote opener, `parser.rs:706-722`, advances at `:716`, `:718` | Fully consumed indent plus `>` and optional one whitespace byte/columns | Create BlockQuote binding before claim | Quote `CONTAINER_MARKER`, `None`; partial tab deferred to terminal | May resolve a preceding pending gap only to a surviving old ancestor, never this new quote | One nested open/action per poll | 20,000 repeated `>` followed by `\tα\n` |
| ATX opener, `parser.rs:724-747`, advance at `:738` | Leading indent, opener hashes, and syntactic separator before content | Create Heading with final level before claim | Heading `BLOCK_MARKER`, `None`; visible content is Heading `CONTENT`; an accepted closing suffix is `BLOCK_MARKER`, `None`; a donor-trimmed non-closing horizontal tail is Heading `CONTENT`, `Hidden { Upstream }`; trailing `eol` is Heading `TERMINAL`, `None` | Tail classification occurs before line publication | Reverse/forward scanners must be fuelled | `  ###\tα  ###\r\n`, `# alpha#   \n`, `# α   \n` |
| Fence opener, `parser.rs:749-812`, advance at `:794` | Leading indent plus complete opening fence; remainder through `eol` is info line source | Create FencedCode with char, length, offset before claim | Opener Code `BLOCK_MARKER`, `None`; info/remainder Code `CONTENT`, source-backed; `eol` is internal first-line content | Close later publishes info/literal projection facts | Candidate scan and info validation refillable | <code>```lang x\r\nbody</code> and a 10 MiB info candidate |
| HTML opener, `parser.rs:814-838` and subsequent `add_line` | From current `o` through `E` and staged `eol`; leading up-to-three spaces after outer prefixes remain raw HTML | Create HTML binding only after exact type is known | HTML `CONTENT`, `Identity` | Close/end facts later | Seven HTML classes and end scan refillable | `   <script>\r\nx\r\n</script>\r\n` |
| Setext underline, `parser.rs:840-862`, advance at `:853` | After reference-prefix finalization, consume the whole current non-EOL `[o,E)` in every accepted Setext branch | With visible content, promote the surviving Paragraph binding to Heading without changing stable BlockId; with definitions only, retain Paragraph | Visible branch: Heading `BLOCK_MARKER`, `None` plus `eol` Heading `TERMINAL`; definitions-only branch: underline and its ending remain Paragraph `CONTENT` | Atomic reference-prefix finalization plus the exhaustive visible-promotion or definitions-only continuation recipe; never retry the line as a thematic break | Reference scan and underline scan are fuelled; publish transaction atomically | `[foo]: /url\nvisible\n===\r\n`; definitions-only `[foo]: /url\n===\n[foo]\n` consumes the underline as Paragraph content |
| Thematic break, `parser.rs:864-890`, advance at `:886` | Whole accepted non-EOL `[o,E)`, including indentation/internal whitespace | Create ThematicBreak before claim | Block `BLOCK_MARKER`, `None`; `eol` Block `TERMINAL` | None | Giant candidate scanner fuelled | ` \t* * *\r\n`; near miss `* * x\n` emits no marker claim |
| List opener marker, `parser.rs:945-982`, advance at `:955` | Leading indent plus exact bullet/ordered marker | Create matching List if needed and Item before the single committed claim | Item `CONTAINER_MARKER`, `None` | None | Marker scanner bounded (9 digits) but nested opens yield | `123456789.\tα\n`; reject `1234567890. x\n` |
| Speculative list padding, `parser.rs:956-970`, advances at `:959`, `:966` | Emit nothing while scanning. After branch selection, claim only fully advanced chosen padding bytes/columns | Item lease already created transaction-locally, or buffer claim until it is | Item `CONTAINER_MARKER`, `None`; partial tab byte deferred to content | Cursor rollback is not output rollback because no speculative claims exist | Padding scan bounded; one committed action | `-     x\n`, `-\talpha\n`, `-   \talpha\n`, `-\n` |
| New indented code, `parser.rs:1006-1030`, advance at `:1016` | Fully advanced four indentation columns | Create IndentedCode before claim | Code `CONTAINER_MARKER`, `None`; partial tab becomes Code `CONTENT` `Atomic`; remainder content | None | Constant | `   \tfoo\n`, `\tfoo\n` |
| Header table delimiter, `table.rs:52-147`, prior-cell slicing at `:150-175`, advance at `:136` | Entire delimiter line non-EOL `[o,E)` plus exact certified cuts through the prior Paragraph range | Table binding and alignments, HeaderRow/Cell leases are transaction-local before claim | Table `BLOCK_MARKER`, `None`; prior cell spans use `CONTENT` programs; `eol` Table `TERMINAL` | Atomic header/preface range rewrite described below | Alignment/header scans and per-cell action generation yield | `before\n| a\\|b | c |\n| :- | -: |\r\n` |
| Table body row, `table.rs:178-243`, cell source append at `:198-225`, advance at `:228` | Entire row non-EOL `[o,E)` split at exact cell source spans and delimiter gaps | Create TableRow, then source-present and synthesized Cell bindings | Cell spans `CONTENT` `Program(TrimAndUnescapePipes)`; pipe/gap bytes Row `CONTAINER_MARKER`; `eol` Row `TERMINAL`; synthesized cells own no source | Atomic row attachment and counter fold | Row scan and action producer both resumable; no unbounded `Vec` | `| a\\|b | | extra |\r\n` against a two-column table; >500,000 padded cells |
| `add_line`, `parser.rs:1122-1205` (all five callsites / six semantic routes above) | Exact unclaimed terminal slice, never a copied `line[self.offset..]` aggregate | Terminal binding supplied by caller | Explicit `CONTENT` action(s); partial tab one-byte `Atomic`, remaining span `Identity`; terminator staged with an explicit block-specific logical action | Extends pending source-run/projection state only | O(1) append/coalesce | 10 MiB/100,000-line paragraph, fence, and HTML body under a tiny poll budget |
| Reference definitions, finalize dispatch at `parser.rs:1337-1342`, resolver at `:1459-1484` | Exact source ranges for the leading definition prefix, split only at scanner-certified source boundaries | Paragraph initially owns scans; final physical owner is nearest surviving parent when definitions are removed | Parent `GAP`, `None`; occurrences/winner facts are separate reference-root output, not terminal text | Atomic split/reclassify; reference-only case unwraps Paragraph while preserving all coverage | Scanner, occurrence emission, and duplicate resolution resumable | `[\nfoo\n]: /url\n`, `[r]: /a\n[r]: /b\n`, `- [r]: /u\n`, `> [r]: /u\n`, million-byte definition |

### Call-site coverage checksum

The matrix accounts for all source cursor/content sites in the audited files:

```text
parser.rs advance_offset:
  536 547 616 618 636 640 664 669 685 692 716 718
  738 794 853 886 955 959 966 1016
parser.rs add_line:
  493 514 516 537 548
  (the sixth source path is the first line of newly opened code/HTML through
   DispatchText/add_line; implementation should route it through the same helper)
table.rs advance_offset:
  136 228
raw offset mutation:
  parser.rs 313-315 (BOM), 1133-1187 (partial-tab/content append)
direct source slicing/transformation:
  table.rs 150-175 (retroactive header cells)
  table.rs 198-225 (body cells and synthesized padding)
  table.rs 250-286 (retroactive preface)
  parser.rs 1459-1484 (reference occurrence origins and prefix drain)
```

There are five syntactic `add_line(...)` call expressions in `parser.rs`; they
cover six semantic routes because the generic accepted-line call serves ordinary
accepts-lines content after opening as well as continuation. The implementation
gate should count semantic routes, not manufacture a nonexistent sixth callsite.

## Pending blank-gap ownership

Blank-line ownership is future-dependent. The same initially matched blank can
belong to an item if the item continues, to its list between sibling items, to an
outer quote when the list closes, or to the document when all containers close.
Committing it immediately would require historical repair; assigning it always
to the root would violate nested editable-gap behavior.

Use one O(1), coalesced `PendingGapRange` in semantic/output restart state:

```text
PendingGapRange {
  source_range_capability,
  exact byte/utf16/line aggregates,
  candidate_open_ancestry_at_first_blank,
  deepest_explicit_marker_floor,
  boundary_affinity,
  blank/list-fold summary,
}
```

Compatible consecutive plain blank lines coalesce by source capability and
aggregates. Lines containing real explicit markers may create separate typed
claims; the design promises O(1) state for arbitrarily many ordinary blanks,
not for arbitrarily many semantic structures. If a partial root must publish
before resolution, this exact range appears through the existing `UnknownRange`
authority. It never masquerades as a committed semantic owner, but total source
coverage and coordinate metrics remain available.

After `CheckOpen` processes the next nonblank line:

1. intersect the candidate ancestry with the line's surviving old open path;
2. choose the deepest surviving binding that structurally spans both sides;
3. never assign the gap to a block newly opened after it;
4. do not lift above an explicit marker floor established on a blank line;
5. if no candidate survives, assign it to Document;
6. commit the range as that owner's `GAP`, `None` before publishing the new
   line's first source action.

At EOF, determine which syntactic containers own any explicit blank-line
markers before closing their leases. An unprefixed trailing blank gap lifts to
that deepest explicit-marker floor or Document. A line such as `> \n` keeps its
quote marker under BlockQuote; trailing plain spaces after a closed list do not
remain Item-owned.

Required gap cases:

| Input | Expected owner of the blank source |
|---|---|
| `- a\n\n  continuation\n` | Item |
| `- a\n\n- b\n` | List, not either Item |
| `- one\n\n two\n` | Document after the list exits |
| `> - a\n>\n>   continuation\n` | Surviving quoted Item when indentation continues it |
| `> - a\n>\n> outside item\n` | BlockQuote after Item/List close |
| `> - a\n\nroot\n` | Document |
| `- a\n` followed by one million `\n`, then `  b\n` | Item, with bounded pending state |
| `- a\n` followed by one million `\n`, then EOF | Document unless an explicit outer marker floor survives |

The pending gap is semantic/output state, not scanner `ControlContinuation`.
It is serialized into the physical-line restart key, compared for suffix
convergence, and adopted only with the matching source generation.

## Pending line terminators and coalescing

Product-visible block ranges generally exclude a final line ending, while a
line ending between two lines of one paragraph/raw terminal contributes a
logical newline. The owner cannot always know which role applies until the next
line.

Keep one `PendingTerminator` for the current terminal:

- if the same terminal continues, commit the previous terminator as internal
  `CONTENT` with the newline transform and coalesce it with adjacent compatible
  content;
- if the terminal closes, publish it as that owner's `TERMINAL` with its exact
  block-specific logical action. Paragraph/heading inline input excludes its
  final terminator (`None`); raw HTML/code literal contracts may contribute a
  canonical newline; syntax-only lines such as a fence closer contribute
  `None`;
- a truly blank line has no terminal consumer and joins `PendingGapRange`;
- serialize the pending terminator in output/restart state.

This keeps a huge LF paragraph compact: all prior `\n` bytes become coalescible
identity content while only its last terminator remains pending. `\r\n` and lone
`\r` use an atomic canonical-newline transform if the product contract chooses
canonical logical `\n`.

The contract is now frozen as canonical logical `\n` with exact physical
origins. CommonMark defines LF, lone CR, and CRLF as forms of one line-ending
concept; Flark's source remains byte-exact, while the parser-certified logical
stream represents each as one LF. CRLF-to-LF is therefore an indivisible atomic
projection and lone-CR-to-LF needs its own typed atomic transform. Donor
differentials normalize at this boundary rather than silently preserving a
donor buffer representation. See the
[CommonMark 0.31.2 line-ending definition](https://spec.commonmark.org/0.31.2/#characters-and-lines).

The site policy after that normalization decision is:

| Line-ending site | Physical part | Logical action |
|---|---|---|
| Continued Paragraph line | `CONTENT` after the next line proves continuation | Canonical newline |
| Final Paragraph/Heading line | `TERMINAL` | `None`; terminal inline input ends before the physical line ending |
| ATX line | `TERMINAL` | `None`; the donor tail chop removes it |
| Fence opener/info and fenced/indented body line | internal `CONTENT`, or final `TERMINAL` at EOF | Canonical newline in the raw source-backed stream; info/literal slices decide visibility |
| HTML line, including the line that closes the block | internal `CONTENT`, or final `TERMINAL` | Canonical newline in the raw literal stream |
| Fence closer, Setext underline, thematic break, table delimiter/row | `TERMINAL` | `None` |
| Reference-definition or structural blank line | parent `GAP` | `None` |

This table is why `TERMINAL` cannot itself imply `None` and why logical action
must be encoded independently.

## Feature-specific range recipes

### Generic stripped indentation

Once an existing terminal has accepted a continuation line, fully consumed
indentation between the surviving syntactic-container prefixes and the first
logical content byte belongs to the nearest surviving pre-existing nonterminal
container as `GAP`, `None`. It does not become hidden terminal content: the
terminal has no logical consumer for those bytes, and a newly opened container
cannot own source to its left. A partial tab is excluded from this gap and
remains a terminal-targeted atomic tab expansion.

The executable golden distinguishes the two prefix layers. In
`> alpha\n>   beta\n`, the second line's `> ` is BlockQuote
`CONTAINER_MARKER`, the following two spaces are BlockQuote `GAP`, and the
Paragraph origin starts at `beta`. At the root, all three leading spaces in
`alpha\n   beta\n` are Document `GAP`. See
[`source_ledger_goldens.rs`](comrak_value_block_core/tests/source_ledger_goldens.rs).

### ATX headings

The ATX scanner must return exact source cuts, not only `matched` and a chopped
string:

```text
leading/opening marker | separator | visible content | optional accepted close | eol
```

The accepted closing hashes and the whitespace that makes them a closer are
Heading `BLOCK_MARKER`, `None`. The exact donor-trimmed non-line-ending tail of
a non-closing ATX heading is Heading `CONTENT`, `Hidden { Upstream }`: it belongs
to the terminal and maps caret affinity back toward its visible text, but it is
not closing syntax and contributes no inline bytes. The physical line ending is
a separate Heading `TERMINAL`, `None`. Thus `# alpha#   \n` keeps the final `#`
visible and hides only the following spaces, whereas `# alpha ###   \r\n`
classifies the whole accepted close suffix as marker source. The focused golden
checks the donor cut, `closed` fact, logical leaf, source origin, hidden/marker
tail, and separate LF/CRLF terminator. The reverse scan must be resumable for
oversized lines.

### Fenced and indented code

Opening and closing scanners return exact marker spans. In particular,
`advance_offset(line, matched, false)` at `parser.rs:685` is a grammar result,
not a sufficient physical close-marker contract.

Code info/literal/end facts are source-backed projection slices attached at
close. Indented code's trimmed trailing blank content and mandatory virtual
final newline are projection facts; physical source remains totally covered.
No aggregate code string is retained in a continuation.

### HTML

Once one of the seven HTML block classes opens, the raw bytes from the current
offset are HTML content, including the block's allowed leading indentation.
End detection and close-time trim/literal metadata must operate over source-run
summaries and refillable scanners. They do not justify copying the raw block.

### References and Setext

Reference definition scanning produces:

- exact source boundaries for the leading definition prefix;
- each occurrence origin and a duplicate-winner delta;
- a safe split boundary in the source projection; and
- visible remainder, if any.

If visible paragraph content remains, definition ranges reclassify to the nearest
surviving parent as `GAP` while visible runs retain the Paragraph binding. If no
visible content remains, the Paragraph wrapper is removed but its coverage is
not detached or deleted. A reference-only paragraph inside an Item or Quote is
therefore parent-owned, not automatically Document-owned.

Setext handling first runs that exact finalization. When visible content
remains, the Paragraph's stable BlockId promotes to Heading in the same
candidate transaction and the underline is Heading `BLOCK_MARKER`. When the
prefix is definitions-only, the handler still consumes the Setext branch: it
reclassifies the definition prefix to the surviving parent but retains the
same Paragraph wrapper and appends the underline as Paragraph `CONTENT`.
`===`/`---` must not be retried through normal block precedence; in particular,
the hyphen form must not become a thematic break. CommonMark example 216 pins
this distinction.

An atomic projection range may not be split in its interior. The reference
scanner must either return a certified source cut or produce typed ambiguity and
defer; the output composer must not guess a byte boundary from logical offsets.

### Tables

Table activation is a typed rewrite of an already-owned paragraph range, not a
sequence of historical events.

For a header with no preface:

1. retire the Paragraph wrapper;
2. create Table, HeaderRow, and TableCell bindings;
3. split prior paragraph source runs at scanner-certified cell boundaries;
4. assign each cell source span to its Cell as `CONTENT` with a
   `Program(TrimAndUnescapePipes)`;
5. assign leading/intercell/trailing pipes and gaps to HeaderRow as
   `CONTAINER_MARKER`;
6. assign the prior header line terminator to HeaderRow `TERMINAL`;
7. claim the current delimiter non-EOL range as Table `BLOCK_MARKER` and its
   terminator as Table `TERMINAL`;
8. publish the rewrite and alignment fact edge atomically.

For a split preface, the recommendation is that the old Paragraph BlockId
survives on the actual preface and a new Table identity is minted. Its visible
preface uses a projection program for trim/unescaped pipes, and the boundary
between preface and header is parent-owned `GAP`/terminal source. The donor's
`try_inserting_table_header_paragraph` contains a defensive early return when
the parent cannot contain a Paragraph, but that branch is unreachable in the
pinned CommonMark 0.31.2 plus five-GFM-extension profile. The source Paragraph
is opened through `add_child`, which first closes parents until
`can_contain(Paragraph)` is true; the only reachable parent classes here are
Document, BlockQuote, and Item, and all three continue to accept Paragraph when
the table delimiter arrives. Root, quote, and item split-preface goldens exercise
all three classes. The direct parser should treat a rejecting parent as a typed
profile/invariant failure rather than inventing a fourth identity policy. Any
future extension that adds another reachable parent must define and test its
policy before that extension profile can be enabled.

Table physical line endings follow the smallest structural boundary that the
line ends. The header source line ending belongs to HeaderRow `TERMINAL`, a body
line ending belongs to its body Row `TERMINAL`, and the delimiter line ending
belongs to Table `TERMINAL` because the delimiter is table syntax and has no Row
node. All contribute `None`; a CRLF remains one exact two-byte physical range.
This keeps cell inline projections free of row separators and avoids assigning
table-internal boundaries to an ambient parent. The focused golden pins LF,
CRLF, and lone-CR forms and verifies that no row spans the delimiter line.

A body row emits physical cell spans and delimiter gaps from the facade's exact
`FacadeTableCell.source` ranges. Escaped pipes require a program: the backslash
is hidden and the pipe contributes identity; treating the whole cell as one
opaque atomic replacement would make edits/origins too coarse. Extra physical
cells beyond the table width remain Row-owned marker/gap source; they are never
dropped. Padded synthesized cells have Enter/Exit records but zero source.
Source-present empty cells may own a nonempty physical span with zero logical
output.

Both the row scanner and action producer need continuations. A source line that
would synthesize more than the autocomplete ceiling still must be scanned and
rejected under fuel without first allocating that many actions.

## Yield, fanout, and restart requirements

The direct port is not bounded merely because the existing line transition is:

- nested quote/list opening emits one binding/claim at a time;
- ATX, fence, thematic, HTML-end, table, and reference scanners retain bounded
  refillable cursors;
- table header/body production retains `(row, next_cell, next_gap)` rather than
  a complete action `Vec`;
- duplicate reference occurrence/winner output is paged;
- range rewrites prepare typed slices incrementally but publish one atomic root;
- close-time code/HTML projection summaries are composable and source-backed;
- no continuation stores a document-sized logical string or leaf.

Every physical-line restart serializes or hashes:

```text
grammar/control state
stable open semantic bindings and mandatory facts
source-run/projection cursors
PendingGapRange
PendingTerminator
reference symbol generation and pending occurrence cursor
table autocomplete equivalence class and any row/action cursor
output composer base root/generation and unpublished transaction recipe
```

Restart adoption requires equality of all future-observable state. Scanner-only
progress can remain outside semantic equality only when it has emitted no claims
and cannot affect future output except by resuming the same deterministic scan.

## Frozen contract decisions

Frozen for the Stage 1 falsifier:

1. **`TERMINAL` meaning.** `TERMINAL` is the final physical line terminator
   held separately from coalesced internal content. Its logical action remains
   independent and block-specific; the source part never implies `None`.
2. **Logical newline normalization.** When a block's logical projection
   includes a physical line ending, LF, CRLF, and lone CR each contribute one
   canonical logical LF while preserving exact physical bytes/UTF-16 and an
   atomic ambiguity for multi-byte CRLF. A final Paragraph/Heading terminator
   contributes `None`; internal Paragraph/Heading endings contribute LF, while
   code and HTML follow their typed literal projection. `TERMINAL` never
   decides this by itself.
3. **Logical target authority.** Parser actions name the target terminal and
   the ledger validates it. Packed encoding is under a final simplification
   audit: derive the innermost compatible terminal unless an exact fixture
   requires retained divergent depth. No BlockId lookup is introduced.
4. **Reference split through non-identity projection.** A split requires a
   scanner-certified physical boundary. An atomic interior produces typed
   ambiguity and defers/fails closed; logical arithmetic never guesses.
5. **Generic stripped indentation.** The owner is the nearest
   surviving pre-existing nonterminal container as `GAP`, `None`; partial tabs
   remain terminal atomic transforms. Root and nested-quote source-range
   goldens pin the owner and exact donor content start.
6. **Non-closing ATX trailing whitespace.** The exact non-line-ending donor tail
   is Heading `CONTENT`, `Hidden { Upstream }`, not `BLOCK_MARKER`; the EOL is a
   separate Heading `TERMINAL`, `None`. Accepted closes remain marker source.
7. **Split table preface under a rejecting parent.** This state is unreachable
   in the pinned profile. Root, BlockQuote, and Item are the exhaustive actual
   parent classes and all accept Paragraph. A rejecting parent is a typed
   invariant/profile failure; it does not receive an invented BlockId policy.
8. **Table terminator owner.** HeaderRow/body Row own their respective physical
   row endings; Table owns the delimiter ending. LF, CRLF, and lone CR each have
   an executable exact-range golden and contribute `None`.

Decisions 5-8 are executable in
[`source_ledger_goldens.rs`](comrak_value_block_core/tests/source_ledger_goldens.rs).
These fixtures pin the product ownership/classification choice while also
checking the current parser/scanner boundaries from which the future direct
ledger actions must be emitted.

## Staged implementation and gates

### Stage 0: freeze the ledger contract

- **Passed:** version the logical-action/target schema and `TERMINAL` semantics.
- **Passed:** add a standalone streaming validator and golden debug form.
- **Passed for frozen policy fixtures:** exact byte/UTF-16/line assertions,
  authority isolation, atomic transforms, and pending-tail resolution.
- **Still expands with later stages:** compare complete semantics to clean
  Comrak while comparing source ownership/projection to the product contract.

### Stage 1: ledger spine and ordinary containers

- Introduce `SourceLineLease`, explicit claims, stable open bindings, and exact
  metric derivation.
- Implement BOM, Paragraph, Quote, List/Item, partial tabs, pending gaps, and
  pending terminators.
- Prove a restart after every line and arbitrarily many blank lines with bounded
  state.

### Stage 2: simple leaf blocks

- Port ATX, thematic breaks, fenced/indented code, and HTML using exact scanner
  cuts and source-backed close facts.
- Add oversized-line, cancellation, and tiny-fuel tests for every scanner.

### Stage 3: typed retroactive paragraph mutation

- Add exact reference-prefix finalization, occurrence/winner output, wrapper
  removal preserving coverage, and Setext promotion.
- Test definitions-only, definitions-plus-visible-content, nested-parent
  ownership, duplicates, multiline definitions, and million-byte definitions.

### Stage 4: table rewrite and dense output

- Add range cut capabilities, deterministic preface/header identity recipes,
  escaped-pipe projection programs, and resumable row/action producers.
- Gate no-preface and split-preface activation, empty/extra/padded cells,
  Unicode/CRLF, giant rows, and the autocomplete ceiling.

### Stage 5: production parity and scale

- Run the full Comrak conformance corpus and the 1,322-case product block suite.
- Restart after every physical line and compare clean/incremental roots.
- Exercise 10 MiB open paragraphs/code/HTML, 100,000 small blocks, 20,000 nested
  containers, dense tables, cancellation, stale-revision rejection, and Wasm.
- Require total source coverage, exact byte/UTF-16 aggregates, no aggregate
  logical string in continuation state, and no unbounded producer poll.

## Recommendation

Proceed with the narrow Stage 1 slice. Do not begin by patching every
`advance_offset` call or by teaching the sink to reinterpret copied lines. The
parser should retain Comrak-derived recognition, but source ownership becomes an
explicit parser decision at the moment each branch commits. If partial tabs,
pending blank gaps, and pending terminal resolution cannot pass the Stage 1
fixtures with bounded restart state, stop before implementing tables; those are
the smallest cases that falsify the selected architecture.
