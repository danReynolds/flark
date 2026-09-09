# Sourcepos and derivation register

Every known comrak 0.54 behavior the extraction has to work around, with its
correction. Each correction is validated in report mode against comrak's own
output, and `cargo test` asserts zero deviations across the 652 CommonMark and
670 GFM upstream cases plus the fuzz and regression suites. Anything new fails
the extraction test.

| comrak behavior | Correction | Validated by |
| --- | --- | --- |
| No node or sourcepos for link reference definitions | comrak's `resolve_reference_link_definitions` mirrored over each paragraph-like leaf's content buffer, and over every run of lines no leaf block covers (a paragraph that became blank); comrak's newline-terminated buffers mirrored exactly | Text literals equal their corrected slices; uncovered lines must parse fully as definitions |
| Inline positions after stripped definitions map through the original per-line offsets | positions on original line `n` moved to line `n + stripped lines` with `n`'s prefix width | Text literal check |
| Multiline inline links can shift following punctuation onto the link's own closing syntax, even when the literal bytes match | text relocation starts after the preceding sibling's complete range; the existing literal match authenticates the new position and carries its delta to following runs | link/image punctuation and following-break regressions with LF, CRLF and quote prefixes; Dart projection and editing invariants |
| GFM bare URLs and angle-bracket autolinks share a Link node kind; a bare URL may end in `>` | hide a closing angle only when the same run starts with an opening angle | bare-URL regression plus the upstream autolink corpus |
| Inline positions inside table cells computed on the unescaped text | each unescaped `\|` before a position shifts it right; the backslash-toggle rule of `unescape_pipes` mirrored | delimiter checks; code spans display the unescaped literal via the string table |
| Per-line content starts after container prefixes not exposed | container prefixes re-scanned per line with comrak's column rules, partial tabs carried as `virtual_leading_spaces` | inline starts never precede the derived start; code content equals the literal |
| One-byte sourcepos for indented code inside containers | the code block's range follows its validated content | code literal |
| Container ranges stop short of lazily continued children; rows short of a trailing cell | parents widened to cover children after extraction | schema containment invariant |
| Code span end column short by up to the backtick count when the span crosses a bare `\r` | end extended to the closing backtick run | literal check |
| Task items report no list padding | content column taken from the checkbox position on the marker line; the marker line keeps one space after the checkbox, as comrak does | text literal check |
| Footnote definitions relocated to the document end and unreferenced ones dropped | `parse.leave_footnote_definitions` set for the render-model parse (not for the spec HTML test) | document-order and coverage invariants |
| Column alignments stored two bits per column | tables with more than 16 aligned columns report `table-alignment-cap` | report mode |
| Code span end column short of the closing run when the span crosses a line ending | the closing run is located from the source as the next run of exactly the opener's length | multi-line literal check against the block's content records, whitespace collapsed |
| A repaired multiline code end must replace the full owner end as well as its content end | use the same corrected end when publishing both ranges; do not retain the original end in a shadowed variable | direct owner-range regression and generated editing seed 2028 |
| Unclosed fenced code inside a container ends the block early | the literal's line count extends the block's lines | code literal |
| Table body cell and inline columns use the header's origin although the row was parsed from its own first nonspace byte | translate the body row's origin after consuming parser-owned container prefixes, before slicing literals or shifting escaped pipes; replace the old content-from-literal heuristic | 256 piped/pipeless, indentation, quote/list, LF/CRLF and Unicode combinations; repeated literal tabs retain distinct source positions; prior strong-range rejection now parses exactly |
| Task checkbox followed by a tab, or sitting on the item's second line | one space or tab is dropped after the checkbox wherever comrak found it | text literal check |
| A paragraph split to make a table header keeps its leading definitions as text, and has its pipes unescaped like a cell | no stripping and the escaped-pipe shift applied when the next block is a table starting on the following line | text literal check |
| Task checkbox symbol position subject to the stripped-definition drift | the position is trusted only when the bytes there are the checkbox; otherwise the checkbox is found in the item's source | text literal check |
| A removed definition-only paragraph continues through lazy lines | uncovered runs extend through lazy lines owned by the scope or an ancestor | full-consumption check |
| Cells filling a row short of the header's columns carry the row's closing `|`, or its line break when the row has none, as a one-character sourcepos that several missing cells repeat | a cell range is clamped to its own line, and a childless cell starting at an unescaped pipe becomes empty there: content can begin at neither | short-row regression across containers, line endings, escaped pipes and genuinely empty cells; projection contract |
| Two adjacent entities decode into one text node, and the resync heuristic treats the next source entity as an immediate agreement | an entity's piece must display at least one character, and only a leading piece may display text the source does not hold | `&amp;&amp;`, `&lt;&gt;`, `&#10;&#10;` unit cases; projection segment coverage |
| A thematic break ending a document carries a sourcepos through the blank lines that follow | the block ends at its own line, which is all a thematic break can be | rule-then-blank regressions; every blank line keeps its own row and caret |
| A link title whose only reading closes on a literal backslash (`"a\"`) is accepted as a definition, though a greedy escape scan finds no close | the mirror takes the longest match over the backslash-as-escape / backslash-as-character alternation, as the generated scanner does | title scan unit cases; uncovered-line check over the corpus |
| Code block end line runs short or long of the literal (unclosed fences, trailing blank lines of a container) | the literal's line count is exact for unclosed and indented code | code literal |
| Lazy continuation lines keep partial indentation and partially consumed tabs | cmark's buffer mirrored: whitespace kept on lazy lines, virtual spaces subtracted from positions | text literal check |
| A text node's literal is one decoded string (entities, the backslash of an escaped pipe, a stray CR, virtual spaces of a partial tab) | the node is split into exact text runs and replacement runs by a lockstep walk that treats an entity reference as one unit; the pieces must rebuild the literal or the node stays one replacement run | literal rebuild in `text_pieces`, extraction test |

## Outside the contract

Bare `\r` line endings (classic Mac OS files). comrak splits blocks on them
but its inline line counter does not advance across them, so inline
positions after a bare CR drift; the extraction repairs text runs by
literal search and re-derives their containers, but text carrying entities
keeps the drifted range. The kernel normalizes bare CR to LF when a
document is loaded, so the parse crate never sees one in practice. CRLF is
fully supported and covered by the fuzz alphabet and regressions.

Registered spec deviations (comrak HTML versus the fixture HTML) live in
`test/fixtures/commonmark/deviation_register.json`; today that is CommonMark
example 354 only.
