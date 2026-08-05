# Comrak-derived block seam provenance

This research crate is a modified derivative of the block parser in Comrak
v0.54.0, git commit `172c2ee7d2c5c262a28be3e407aadf705daea2b7`.
It is not a clean-room Markdown implementation.

The state-machine ordering and conditions are adapted from these exact
upstream functions in `src/parser/mod.rs`:

| Upstream function | v0.54.0 lines | Adaptation in this probe |
| --- | ---: | --- |
| `Parser::process_line` | 260–301 | `DerivedBlockMachine::advance`, `LineWork::tick` |
| `check_open_blocks_inner` | 317–400 | `BuildPath`, `MatchFrame`, `MatchQuote`, `MatchItem`, `FenceClose*` stages |
| `find_first_nonspace` | 402–434 | `ScanIndent` and `IndentResult` |
| `parse_block_quote_prefix` | 436–454 | `MatchQuote` / `MatchQuoteSpace` |
| `parse_node_item_prefix` | 468–479 | `MatchItem` |
| `parse_code_block_prefix` | 499–545 | `FenceCloseRun` / `FenceCloseTail` |
| `open_new_blocks` | 752–791 | `OpenDispatch` and its quote/fence/list stages |
| `handle_blockquote` | 929–948 | `OpenDispatch` / `OpenQuoteSpace` |
| `handle_code_fence` | 1014–1042 | `FenceOpenRun` / `FenceOpenInfo` |
| `handle_list` / `detect_list` | 1635–1703 | `List*` stages and `open_list_item` |
| `advance_offset` | 1778–1807 | byte/column transitions and `virtual_prefix_spaces` |
| `add_text_to_container` | 1827–1998 | `Tail`, direct source-backed `LineChunk` emission |
| `parse_list_marker` | 2766–2881 | `ListBulletAfter`, `ListOrdered`, `ListAfterMarker` |
| `lists_match` | 2883–2887 | `lists_match` |

Representation changes are intentional:

- `Node<'a>`/arena ancestry becomes an immutable `Arc<FrameNode>` chain;
- mutable `Ast.content` becomes source ranges in `LineChunk`;
- line-local parser fields become explicit resumable stage values;
- finalization becomes direct `BlockEvent` and chunk emission;
- one transition tick performs at most one source-byte inspection;
- checkpoints clone the frame-chain head instead of the open AST spine.

This is only a seam falsification. It includes the retroactive setext promotion
shape, but omits ATX headings, thematic breaks, indented code, HTML, tables,
references, inline parsing, list tightness, and several exact
lazy-continuation/tab cases. Those are production gates, not features this
probe claims to implement.

See `COMRAK_LICENSE` for the upstream license notice.

## 2026-07-15 commitment-gate additions

`src/commitment_spine.rs` is Flark-owned orchestration, not a copied Comrak
module and not a function-correspondent complete port. It calls the pinned
internal module
`../comrak_inline_fragment_gate/vendor/comrak/src/parser/block_spine_facade.rs`
under an 8 KiB cap.

The facade delegates to these unchanged Comrak v0.54.0 authorities:

| Facade API | Donor authority |
| --- | --- |
| `html_block_start` | generated `html_block_start` and `html_block_start_7` scanners |
| `html_block_end` | generated `html_block_end_1` through `_5`; the caller owns blank termination for types 6/7 |
| `table_row` | `parser/table.rs::row`, including generated cell/end scanners and exact escaped-pipe handling |
| `table_delimiter_alignments` | donor row tokenization plus `try_opening_header`'s alignment rule |
| `reference_definitions` | `inlines::Scanner` label/space/line helpers, `manual_scan_link_url`, generated `link_title`, and `strings` normalization/cleaning |

The block-facade patch adds one 234-line module, one parser-module export, one
crate export, and `pub(super)` visibility on `table::row`, `Row`, `Cell`, and
their fields. It changes no donor scanner condition. First-definition wins,
persistent generations, source ranges, checkpoints, table/HTML state, list
aggregates, and opaque degradation remain Flark-owned.

The commitment list aggregate is informed by
`Parser::determine_list_tight` (v0.54.0 `parser/mod.rs` lines 2239--2257), but
the pinned differential records its remaining nested/container divergences.
This provenance relationship must not be presented as conformance.

`src/origin_runs.rs` is Flark-owned source/projection protocol work, not a
Comrak derivative. It represents identity, atomic transform, hidden-prefix,
and synthetic origin runs because the donor's owned-content model does not
preserve those editor semantics.

All facade additions remain subject to Comrak's BSD-2-Clause license. The
research crate must retain `COMRAK_LICENSE` while it vendors or depends on this
module.
