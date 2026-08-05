use std::cmp::min;

use comrak::block_spine_facade::{
    FacadeAlignment, FacadeTableCell, FacadeTableRow, table_delimiter_alignments,
    table_delimiter_candidate, table_row,
};

use crate::parser::{ParseError, ValueBlockParser, newlines_of};
use crate::source::{LeafContent, OriginTransform};
use crate::tree::{Alignment, BlockKind, NodeId, Position, TableData};

/// Once this many cells have been synthesized for short source rows, the next
/// physical line must no longer be accepted as a continuation of the table.
///
/// This is crate-visible because the physical-line convergence key stores the
/// exact future-observable equivalence class: every count above this ceiling is
/// identical, while every count at or below it can accept a different number
/// of later short rows.
pub(crate) const MAX_AUTOCOMPLETED_CELLS: usize = 500_000;

pub(crate) struct TableOpening {
    pub(crate) container: NodeId,
    pub(crate) replace: bool,
    pub(crate) mark_visited: bool,
}

pub(crate) fn try_opening_block(
    parser: &mut ValueBlockParser,
    container: NodeId,
    line: &str,
) -> Result<Option<TableOpening>, ParseError> {
    match parser.tree.node(container).kind.clone() {
        BlockKind::Paragraph => try_opening_header(parser, container, line),
        BlockKind::Table(table) => try_opening_row(parser, container, &table.alignments, line),
        _ => Ok(None),
    }
}

fn try_opening_header(
    parser: &mut ValueBlockParser,
    container: NodeId,
    line: &str,
) -> Result<Option<TableOpening>, ParseError> {
    if parser.tree.node(container).table_visited {
        return Ok(Some(TableOpening {
            container,
            replace: false,
            mark_visited: false,
        }));
    }

    let delimiter_input = &line[parser.first_nonspace..];
    if !table_delimiter_candidate(delimiter_input)? {
        return Ok(Some(TableOpening {
            container,
            replace: false,
            mark_visited: false,
        }));
    }

    let Some(facade_alignments) = table_delimiter_alignments(delimiter_input, false)? else {
        return Ok(Some(TableOpening {
            container,
            replace: false,
            mark_visited: true,
        }));
    };
    let container_content = parser.tree.node(container).content.clone();
    let Some(header_row) = row(&container_content.logical)? else {
        return Ok(Some(TableOpening {
            container,
            replace: false,
            mark_visited: true,
        }));
    };
    if header_row.cells.len() != facade_alignments.len() {
        return Ok(Some(TableOpening {
            container,
            replace: false,
            mark_visited: true,
        }));
    }

    if header_row.paragraph_offset > 0 {
        try_inserting_table_header_paragraph(
            parser,
            container,
            &container_content,
            header_row.paragraph_offset,
        )?;
    }

    let alignments = facade_alignments
        .into_iter()
        .map(|alignment| match alignment {
            FacadeAlignment::None => Alignment::None,
            FacadeAlignment::Left => Alignment::Left,
            FacadeAlignment::Center => Alignment::Center,
            FacadeAlignment::Right => Alignment::Right,
        })
        .collect::<Vec<_>>();
    let start = parser.tree.node(container).source_start;
    let table = parser.new_detached_node(
        BlockKind::Table(TableData {
            alignments,
            num_columns: header_row.cells.len(),
            num_rows: 0,
            num_nonempty_cells: 0,
        }),
        start,
    );
    let header = parser.add_child(table, BlockKind::TableRow { header: true }, start.column)?;
    parser.tree.node_mut(header).source_start.line = start.line;
    parser.tree.node_mut(header).source_end = Position::new(
        start.line,
        start.column
            + container_content
                .logical
                .len()
                .saturating_sub(newlines_of(&container_content.logical) + 1)
                .saturating_sub(header_row.paragraph_offset),
    );

    for cell in &header_row.cells {
        append_header_cell(
            parser,
            header,
            start,
            &container_content,
            header_row.paragraph_offset,
            cell,
        )?;
    }

    let offset = line.len() - newlines_of(line) - parser.offset;
    parser.advance_offset(line, offset, false);
    adjust_table_counters(
        parser,
        table,
        header_row.cells.len(),
        Position::new(parser.line_number, offset),
    );
    Ok(Some(TableOpening {
        container: table,
        replace: true,
        mark_visited: false,
    }))
}

fn append_header_cell(
    parser: &mut ValueBlockParser,
    header: NodeId,
    start: Position,
    container_content: &LeafContent,
    paragraph_offset: usize,
    cell: &FacadeTableCell,
) -> Result<(), ParseError> {
    let column = start.column + cell.source.start.saturating_sub(paragraph_offset);
    let cell_node = parser.add_child(header, BlockKind::TableCell, column)?;
    parser.tree.node_mut(cell_node).source_start.line = start.line;
    parser.tree.node_mut(cell_node).source_end = Position::new(
        start.line,
        start.column + cell.source.end.saturating_sub(1 + paragraph_offset),
    );
    let source_range = cell.source.start..cell.source.end;
    parser.tree.node_mut(cell_node).content = container_content.transformed_slice(
        source_range,
        cell.content.clone(),
        OriginTransform::TrimAndUnescapePipes,
    );
    parser.tree.node_mut(cell_node).content.line_offsets.push(
        start.column + cell.source.start.saturating_sub(1) + cell.internal_offset
            - paragraph_offset,
    );
    Ok(())
}

fn try_opening_row(
    parser: &mut ValueBlockParser,
    container: NodeId,
    alignments: &[Alignment],
    line: &str,
) -> Result<Option<TableOpening>, ParseError> {
    if parser.blank || get_num_autocompleted_cells(parser, container) > MAX_AUTOCOMPLETED_CELLS {
        return Ok(None);
    }
    let Some(this_row) = row(&line[parser.first_nonspace..])? else {
        return Ok(None);
    };
    let sourcepos = parser.tree.node(container).source_start;
    let new_row = parser.add_child(
        container,
        BlockKind::TableRow { header: false },
        sourcepos.column,
    )?;
    parser.tree.node_mut(new_row).source_end.column = parser.curline_end_col;

    let parsed_cells = min(alignments.len(), this_row.cells.len());
    let mut last_column = sourcepos.column;
    for cell in this_row.cells.iter().take(parsed_cells) {
        let cell_node = parser.add_child(
            new_row,
            BlockKind::TableCell,
            sourcepos.column + cell.source.start,
        )?;
        let end_column = sourcepos.column + cell.source.end.saturating_sub(1);
        parser.tree.node_mut(cell_node).source_end.column = end_column;
        parser.tree.node_mut(cell_node).content.push_source(
            parser.line_leaf_id,
            (parser.first_nonspace + cell.source.start)..(parser.first_nonspace + cell.source.end),
            &cell.content,
            OriginTransform::TrimAndUnescapePipes,
        );
        parser
            .tree
            .node_mut(cell_node)
            .content
            .line_offsets
            .push((sourcepos.column + cell.source.start + cell.internal_offset).saturating_sub(1));
        last_column = end_column;
    }
    for _ in parsed_cells..alignments.len() {
        let cell_node = parser.add_child(new_row, BlockKind::TableCell, last_column + 1)?;
        parser.tree.node_mut(cell_node).source_end.column = last_column + 1;
    }

    let offset = line.len() - parser.offset - newlines_of(line);
    parser.advance_offset(line, offset, false);
    // `parsed_cells` is deliberately captured before the output row is padded
    // to `alignments.len()`. The donor's public `TableData` counters count the
    // padded output width, so the exact hostile-short-row count lives in parser
    // continuation state instead of silently changing donor-visible metadata.
    adjust_table_counters(
        parser,
        container,
        parsed_cells,
        Position::new(parser.line_number, offset),
    );
    Ok(Some(TableOpening {
        container: new_row,
        replace: false,
        mark_visited: false,
    }))
}

fn row(input: &str) -> Result<Option<FacadeTableRow>, ParseError> {
    Ok(table_row(input, false)?)
}

fn try_inserting_table_header_paragraph(
    parser: &mut ValueBlockParser,
    container: NodeId,
    content: &LeafContent,
    paragraph_offset: usize,
) -> Result<(), ParseError> {
    let Some(parent) = parser.tree.parent(container) else {
        return Ok(());
    };
    if !parser
        .tree
        .node(parent)
        .kind
        .can_contain(&BlockKind::Paragraph)
    {
        return Ok(());
    }
    let mut logical = unescape_pipes(&content.logical[..paragraph_offset]);
    let newlines = logical.bytes().filter(|byte| *byte == b'\n').count();
    logical = logical.trim().to_owned();
    let start = parser.tree.node(container).source_start;
    let paragraph = parser.new_detached_node(BlockKind::Paragraph, start);
    parser.tree.node_mut(paragraph).content = content.transformed_slice(
        0..paragraph_offset,
        logical,
        OriginTransform::TrimAndUnescapePipes,
    );
    parser.tree.node_mut(paragraph).source_end = Position::new(
        start.line + newlines.saturating_sub(1),
        content
            .logical
            .get(..paragraph_offset)
            .and_then(|preface| preface.lines().last())
            .map_or(start.column, |last| start.column + last.len()),
    );
    parser.tree.insert_before(container, paragraph);
    Ok(())
}

fn unescape_pipes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            output.push(chars.next().expect("peeked pipe"));
        } else {
            output.push(ch);
        }
    }
    output
}

fn adjust_table_counters(
    parser: &mut ValueBlockParser,
    container: NodeId,
    nonempty: usize,
    end: Position,
) {
    let BlockKind::Table(table) = &mut parser.tree.node_mut(container).kind else {
        unreachable!("table counters target a table");
    };
    table.num_rows = table.num_rows.saturating_add(1);
    table.num_nonempty_cells = table.num_nonempty_cells.saturating_add(table.num_columns);
    let autocompleted = table.num_columns.saturating_sub(nonempty);
    parser.tree.node_mut(container).table_autocompleted_cells = parser
        .tree
        .node(container)
        .table_autocompleted_cells
        .saturating_add(autocompleted);
    parser.tree.node_mut(container).source_end = end;
}

fn get_num_autocompleted_cells(parser: &ValueBlockParser, container: NodeId) -> usize {
    let BlockKind::Table(_) = &parser.tree.node(container).kind else {
        return 0;
    };
    parser.tree.node(container).table_autocompleted_cells
}

/// Canonical physical-line transition state for the autocomplete guard.
///
/// `MAX + 1` means "already over the ceiling". Larger historical counts cannot
/// be distinguished by any future parser branch and must not prevent suffix
/// convergence after an otherwise irrelevant table-row edit.
#[must_use]
pub(crate) const fn capped_autocompleted_cells(count: usize) -> usize {
    if count > MAX_AUTOCOMPLETED_CELLS {
        MAX_AUTOCOMPLETED_CELLS + 1
    } else {
        count
    }
}

pub(crate) fn matches(line: &str) -> Result<bool, ParseError> {
    Ok(row(line)?.is_some())
}
