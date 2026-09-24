# Render model schema v5

Generated from `schema/render_model_v5.json`; edit the JSON, then run `tool/gen_schema.py`.

Flat little-endian u32 render model written by flark_parse. Every offset is a UTF-16 code unit, the unit of the host's source string; the header also records the source's UTF-8 length. Every range is [start, end). Hidden units of a run are exactly its source range minus its content range. A line of a leaf block that has no content record is hidden entirely. A content record's prefix_start is where the innermost container's prefix (a quote marker, a list marker with its padding and task checkbox, a footnote indent) begins on that line; it equals the content start when the line has no such prefix, so a host lifts a prefix by deleting [prefix_start, start) and never scans for a marker. Empty lines owned only by a container have a zero-width content record on the innermost container. Its prefix_start identifies only that container prefix; lifting it retains the outer containers. Records are fixed-width and some words pack several fields. Values only some kinds carry live in the extras section: a block reaches its record through its extra word, and a run through the run-extra index, which is sorted by run.

Magic `FLK5` (u32 `0x354B4C46` little-endian). Sections follow the header in this order: lines, blocks, content, runs, definitions, run_extras, extras, strings. The extras section is `extra_words` words long. The string table is padded to a multiple of four bytes.

## Header

| Word | Field |
| --- | --- |
| 0 | `magic` |
| 1 | `version` |
| 2 | `src_bytes` |
| 3 | `src_utf16` |
| 4 | `line_count` |
| 5 | `block_count` |
| 6 | `content_count` |
| 7 | `run_count` |
| 8 | `definition_count` |
| 9 | `run_extra_count` |
| 10 | `extra_words` |
| 11 | `string_bytes` |

## Line record (1 words)

| Word | Field |
| --- | --- |
| 0 | `start` |

## Block record (10 words)

| Word | Field |
| --- | --- |
| 0 | `kind_flags` |
| 1 | `parent` |
| 2 | `start` |
| 3 | `end` |
| 4 | `first_line` |
| 5 | `line_count` |
| 6 | `content_offset` |
| 7 | `first_run` |
| 8 | `attr` |
| 9 | `extra` |

## Content record (4 words)

| Word | Field |
| --- | --- |
| 0 | `start` |
| 1 | `end` |
| 2 | `prefix_start` |
| 3 | `line_virtual` |

## Run record (4 words)

| Word | Field |
| --- | --- |
| 0 | `start` |
| 1 | `end` |
| 2 | `hidden` |
| 3 | `kind_flags_parent` |

## Definition record (6 words)

| Word | Field |
| --- | --- |
| 0 | `start` |
| 1 | `end` |
| 2 | `label_start` |
| 3 | `label_end` |
| 4 | `dest_start` |
| 5 | `dest_end` |

## Run extra record (2 words)

| Word | Field |
| --- | --- |
| 0 | `run` |
| 1 | `offset` |

## Packed words

| Word | Field | Bits |
| --- | --- | --- |
| `block.kind_flags` | `kind` | 0-7 |
| `block.kind_flags` | `flags` | 8-31 |
| `content.line_virtual` | `line` | 0-29 |
| `content.line_virtual` | `virtual_leading_spaces` | 30-31 |
| `run.hidden` | `before` | 0-15 |
| `run.hidden` | `after` | 16-31 |
| `run.kind_flags_parent` | `kind` | 0-7 |
| `run.kind_flags_parent` | `flags` | 8-15 |
| `run.kind_flags_parent` | `parent_distance` | 16-31 |

## Extra records

| Record | Owner | Words |
| --- | --- | --- |
| `code_block` | fenced `code_block` blocks | `info_start`, `info_end` |
| `item` | `item` blocks | `marker_end`, `task_start`, `task_end` |
| `table` | `table` blocks | `alignments` |
| `footnote_definition` | `footnote_definition` blocks | `label_start`, `label_end` |
| `wide_run` | runs with flags bit3; the other kind's record follows | `content_start`, `content_end`, `parent` |
| `link` | `link`, `image` and `autolink` runs | `destination_start`, `destination_end`, `title_start`, `title_end`, `destination_offset`, `destination_length`, `title_offset`, `title_length` |
| `display_text` | `replacement` runs, and `code` runs with flags bit1 | `offset`, `length` |
| `footnote_ref` | `footnote_ref` runs | `label_start`, `label_end` |

## block_kind

| Name | Value |
| --- | --- |
| `document` | 0 |
| `paragraph` | 1 |
| `heading` | 2 |
| `code_block` | 3 |
| `html_block` | 4 |
| `block_quote` | 5 |
| `list` | 6 |
| `item` | 7 |
| `thematic_break` | 8 |
| `table` | 9 |
| `table_row` | 10 |
| `table_cell` | 11 |
| `footnote_definition` | 12 |
| `other` | 14 |

## run_kind

| Name | Value |
| --- | --- |
| `text` | 1 |
| `emph` | 2 |
| `strong` | 3 |
| `code` | 4 |
| `strike` | 5 |
| `link` | 6 |
| `image` | 7 |
| `autolink` | 8 |
| `escape` | 9 |
| `replacement` | 10 |
| `hard_break` | 11 |
| `soft_break` | 12 |
| `html_inline` | 13 |
| `footnote_ref` | 14 |
| `other` | 16 |

## table_alignment

| Name | Value |
| --- | --- |
| `none` | 0 |
| `left` | 1 |
| `center` | 2 |
| `right` | 3 |

## Block attributes

| Kind | Field | Meaning |
| --- | --- | --- |
| `heading` | `attr` | level 1-6 |
| `heading` | `flags` | bit0 setext |
| `code_block` | `attr` | fence length (0 for indented) |
| `code_block` | `flags` | bit0 fenced, bit1 closed by a fence line |
| `code_block` | `extra` | fenced blocks: info string start and end |
| `list` | `attr` | start number |
| `list` | `flags` | bit0 tight, bit1 ordered |
| `item` | `attr` | content column offset (marker_offset + padding) |
| `item` | `flags` | bit0 task item, bit1 checked |
| `item` | `extra` | marker end (exclusive end of the first-line list marker and padding, before any task checkbox), then the task symbol start and end (zero unless a task item) |
| `table` | `attr` | column count |
| `table` | `extra` | column alignments packed two bits per column, column 0 in the low bits |
| `table_row` | `flags` | bit0 header row |
| `footnote_definition` | `extra` | label start and end |
| `any` | `extra` | offset of the block's record in the extras section, or 0xFFFFFFFF when it has none |

## Run attributes

| Kind | Field | Meaning |
| --- | --- | --- |
| `link` | `flags` | bit0 reference style, bit1 has title |
| `link` | `extra` | destination and title source ranges, then comrak's resolved destination and title in the string table |
| `image` | `flags` | bit0 reference style, bit1 has title |
| `image` | `extra` | as for link |
| `autolink` | `extra` | as for link; the destination range is the URL and the title is empty |
| `replacement` | `extra` | display text in the string table |
| `code` | `flags` | bit1 content displays from the string table instead of the source slice (escaped pipes inside table cells) |
| `code` | `extra` | with flags bit1: display text in the string table |
| `footnote_ref` | `extra` | label start and end |
| `any` | `flags` | bit2 run spans more than one line; bit3 wide: the exact content range and parent are in the run's extra record because a hidden length or the parent distance exceeds 0xFFFE, and the packed fields read 0xFFFF |
| `any` | `hidden` | units hidden before and after the content; content_start = start + before, content_end = end - after |
| `any` | `parent_distance` | runs back to the parent run, 0 when the run has none |

## Invariants

- Leaf content retains editable trailing whitespace: paragraph and setext content reaches physical line ends, table cells retain trailing padding, and ATX closing markers keep one required separator outside content. Space-based hard-break runs expose their spaces as content; deleting across the break removes the full source range of its marker and newline. Backslash hard breaks and soft breaks have empty content ranges.
- Blocks are in document order; a block's parent index is smaller than its own index or 0xFFFFFFFF for the document.
- Runs are in document order and contiguous per block: block b owns runs [first_run(b), first_run(b + 1)), where first_run is non-decreasing and first_run(block_count) is run_count. A run's parent is an earlier run of the same block.
- content_start >= start and content_end <= end and content_start <= content_end for every run.
- Content records are in block order: block b owns records [content_offset(b), content_offset(b + 1)), where content_offset(block_count) is content_count. A block's records are in line order and lie inside its source range.
- Every offset is at most src_utf16 and never falls between the two code units of a surrogate pair.
- Definition records never overlap a block's content record.
- Item marker endpoints lie within the first source line, after the marker start and before task checkboxes; attr remains a display-column indentation offset, never a source length.
- The run-extra index lists, in run order, every run with an extra record: link, image, autolink, replacement and footnote_ref runs, code runs with flags bit1, and wide runs. String ranges are in bounds.
- A content line is below 2^30 and virtual leading spaces are at most 3; a source beyond that fails extraction.
