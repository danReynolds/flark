# Render model schema v1

Generated from `schema/render_model_v1.json`; edit the JSON, then run `tool/gen_schema.py`.

Flat little-endian u32 render model written by flark_parse. Every range is [start, end) and is given in both UTF-8 bytes and UTF-16 code units. Hidden bytes of a run are exactly its source range minus its content range. A line of a leaf block that has no content record is hidden entirely.

Magic `FLK5` (u32 `0x354B4C46` little-endian). Sections follow the header in this order: lines, blocks, content, runs, definitions, strings. The string table is padded to a multiple of four bytes.

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
| 9 | `string_bytes` |
| 10 | `reserved` |

## Line record (2 words)

| Word | Field |
| --- | --- |
| 0 | `start_byte` |
| 1 | `start_utf16` |

## Block record (14 words)

| Word | Field |
| --- | --- |
| 0 | `kind` |
| 1 | `parent` |
| 2 | `start_byte` |
| 3 | `end_byte` |
| 4 | `start_utf16` |
| 5 | `end_utf16` |
| 6 | `first_line` |
| 7 | `line_count` |
| 8 | `content_offset` |
| 9 | `content_count` |
| 10 | `attr0` |
| 11 | `attr1` |
| 12 | `attr2` |
| 13 | `flags` |

## Content record (6 words)

| Word | Field |
| --- | --- |
| 0 | `line` |
| 1 | `start_byte` |
| 2 | `start_utf16` |
| 3 | `end_byte` |
| 4 | `end_utf16` |
| 5 | `virtual_leading_spaces` |

## Run record (16 words)

| Word | Field |
| --- | --- |
| 0 | `kind` |
| 1 | `block` |
| 2 | `parent` |
| 3 | `start_byte` |
| 4 | `end_byte` |
| 5 | `content_start_byte` |
| 6 | `content_end_byte` |
| 7 | `start_utf16` |
| 8 | `end_utf16` |
| 9 | `content_start_utf16` |
| 10 | `content_end_utf16` |
| 11 | `aux0` |
| 12 | `aux1` |
| 13 | `aux2` |
| 14 | `aux3` |
| 15 | `flags` |

## Definition record (8 words)

| Word | Field |
| --- | --- |
| 0 | `start_byte` |
| 1 | `end_byte` |
| 2 | `start_utf16` |
| 3 | `end_utf16` |
| 4 | `label_start_byte` |
| 5 | `label_end_byte` |
| 6 | `dest_start_byte` |
| 7 | `dest_end_byte` |

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
| `heading` | `attr0` | level 1-6 |
| `heading` | `flags` | bit0 setext |
| `code_block` | `attr0` | fence length (0 for indented) |
| `code_block` | `attr1` | info string start byte |
| `code_block` | `attr2` | info string end byte |
| `code_block` | `flags` | bit0 fenced, bit1 closed by a fence line |
| `list` | `attr0` | 1 if ordered |
| `list` | `attr1` | start number |
| `list` | `flags` | bit0 tight |
| `item` | `attr0` | content column offset (marker_offset + padding) |
| `item` | `attr1` | task symbol start byte (task items only) |
| `item` | `attr2` | task symbol end byte |
| `item` | `flags` | bit0 task item, bit1 checked |
| `table` | `attr0` | column count |
| `table` | `attr1` | column alignments packed two bits per column, column 0 in the low bits |
| `table_row` | `flags` | bit0 header row |
| `table_cell` | `attr0` | column index |
| `table_cell` | `attr1` | alignment |
| `footnote_definition` | `attr1` | label start byte |
| `footnote_definition` | `attr2` | label end byte |

## Run attributes

| Kind | Field | Meaning |
| --- | --- | --- |
| `link` | `aux0` | destination start byte |
| `link` | `aux1` | destination end byte |
| `link` | `aux2` | title start byte |
| `link` | `aux3` | title end byte |
| `link` | `flags` | bit0 reference style, bit1 has title |
| `image` | `aux0` | destination start byte |
| `image` | `aux1` | destination end byte |
| `image` | `aux2` | title start byte |
| `image` | `aux3` | title end byte |
| `image` | `flags` | bit0 reference style, bit1 has title |
| `autolink` | `aux0` | url start byte |
| `autolink` | `aux1` | url end byte |
| `replacement` | `aux0` | display text offset in the string table |
| `replacement` | `aux1` | display text byte length |
| `footnote_ref` | `aux0` | label start byte |
| `footnote_ref` | `aux1` | label end byte |
| `code` | `aux0` | backtick count |
| `code` | `aux2` | display text offset in the string table when flags bit1 is set |
| `code` | `aux3` | display text byte length when flags bit1 is set |
| `code` | `flags` | bit1 content displays from the string table instead of the source slice (escaped pipes inside table cells) |
| `any` | `flags` | bit8 run spans more than one line |

## Invariants

- Blocks are in document order; a block's parent index is smaller than its own index or 0xFFFFFFFF for the document.
- Runs are in document order and contiguous per block; a run's parent is an earlier run of the same block or 0xFFFFFFFF.
- content_start >= start and content_end <= end and content_start <= content_end for every run.
- Content records of a block are in line order and lie inside the block's source range.
- Every byte offset lies inside a UTF-8 scalar boundary; every UTF-16 offset equals the count of code units before its byte offset.
- Definition records never overlap a block's content record.
