//! Bounded parser-owned GFM table projection over one recursive-Green
//! Paragraph leaf.
//!
//! The direct block tree deliberately retains stable Paragraph identity. This
//! projector supplies exact table rows and cells to semantic and live-view
//! consumers without asking Dart or Flutter to infer Markdown structure.

use std::ops::Range;

use comrak::block_spine_facade::{self, FacadeError, FacadeTableRow};

pub const M11_GFM_TABLE_MAX_BYTES: usize = block_spine_facade::MAX_CLASSIFICATION_BYTES;
pub const M11_GFM_TABLE_MAX_COLUMNS: usize = 256;
pub const M11_GFM_TABLE_MAX_ROWS: usize = 512;
pub const M11_GFM_TABLE_MAX_CELLS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11GfmTableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11GfmTableCell {
    pub source_range: Range<u32>,
    pub content_range: Range<u32>,
    pub cooked_content: String,
    pub autocompleted: bool,
    pub pipe_escape_ranges: Vec<Range<u32>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11GfmTableRow {
    pub source_range: Range<u32>,
    pub cells: Vec<M11GfmTableCell>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11GfmTableProjection {
    pub preface_range: Option<Range<u32>>,
    pub delimiter_range: Range<u32>,
    pub alignments: Vec<M11GfmTableAlignment>,
    pub header: M11GfmTableRow,
    pub body: Vec<M11GfmTableRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11GfmTableProjectionError {
    OverCap { bytes: usize, cap: usize },
    CoordinateOverflow,
}

pub fn project_m11_gfm_table(
    input: &str,
) -> Result<Option<M11GfmTableProjection>, M11GfmTableProjectionError> {
    if input.len() > M11_GFM_TABLE_MAX_BYTES {
        return Err(M11GfmTableProjectionError::OverCap {
            bytes: input.len(),
            cap: M11_GFM_TABLE_MAX_BYTES,
        });
    }
    let lines = logical_lines(input);
    if lines.len() < 2 {
        return Ok(None);
    }

    for delimiter_index in 1..lines.len() {
        let delimiter_line = &lines[delimiter_index];
        let delimiter_input = &input[delimiter_line.clone()];
        if !map_facade(block_spine_facade::table_delimiter_candidate(
            delimiter_input,
        ))? {
            continue;
        }
        let Some(delimiter) = map_facade(block_spine_facade::table_row(delimiter_input))? else {
            return Ok(None);
        };
        let header_line = &lines[delimiter_index - 1];
        let Some(header) = map_facade(block_spine_facade::table_row(&input[header_line.clone()]))?
        else {
            return Ok(None);
        };
        if header.cells.len() != delimiter.cells.len()
            || header.cells.len() > M11_GFM_TABLE_MAX_COLUMNS
        {
            return Ok(None);
        }

        let columns = header.cells.len();
        let alignments = delimiter
            .cells
            .iter()
            .map(|cell| alignment(&cell.content))
            .collect::<Vec<_>>();
        let header = map_row(input, header_line, header, columns)?;
        let mut body = Vec::new();
        let mut total_cells = header.cells.len();
        for line in &lines[delimiter_index + 1..] {
            if body.len() >= M11_GFM_TABLE_MAX_ROWS.saturating_sub(1) {
                return Ok(None);
            }
            let Some(scanned) = map_facade(block_spine_facade::table_row(&input[line.clone()]))?
            else {
                break;
            };
            total_cells = total_cells
                .checked_add(columns)
                .ok_or(M11GfmTableProjectionError::CoordinateOverflow)?;
            if total_cells > M11_GFM_TABLE_MAX_CELLS {
                return Ok(None);
            }
            body.push(map_row(input, line, scanned, columns)?);
        }
        let preface_range = (delimiter_index > 1)
            .then(|| to_u32_range(lines[0].start..lines[delimiter_index - 1].start))
            .transpose()?;
        return Ok(Some(M11GfmTableProjection {
            preface_range,
            delimiter_range: to_u32_range(delimiter_line.clone())?,
            alignments,
            header,
            body,
        }));
    }
    Ok(None)
}

fn map_row(
    input: &str,
    line: &Range<usize>,
    scanned: FacadeTableRow,
    columns: usize,
) -> Result<M11GfmTableRow, M11GfmTableProjectionError> {
    let mut cells = scanned
        .cells
        .into_iter()
        .take(columns)
        .map(|cell| {
            Ok(M11GfmTableCell {
                source_range: to_u32_range(
                    line.start + cell.source.start..line.start + cell.source.end,
                )?,
                content_range: to_u32_range(
                    line.start + cell.content_source.start..line.start + cell.content_source.end,
                )?,
                cooked_content: cell.content,
                autocompleted: false,
                pipe_escape_ranges: cell
                    .pipe_escape_sources
                    .into_iter()
                    .map(|range| to_u32_range(line.start + range.start..line.start + range.end))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        })
        .collect::<Result<Vec<_>, M11GfmTableProjectionError>>()?;
    let ending_bytes = match input.as_bytes().get(line.end.saturating_sub(2)..line.end) {
        Some([b'\r', b'\n']) => 2,
        _ if input
            .as_bytes()
            .get(line.end.saturating_sub(1))
            .is_some_and(|byte| matches!(byte, b'\r' | b'\n')) =>
        {
            1
        }
        _ => 0,
    };
    let point = line.end.saturating_sub(ending_bytes);
    while cells.len() < columns {
        let point =
            u32::try_from(point).map_err(|_| M11GfmTableProjectionError::CoordinateOverflow)?;
        cells.push(M11GfmTableCell {
            source_range: point..point,
            content_range: point..point,
            cooked_content: String::new(),
            autocompleted: true,
            pipe_escape_ranges: Vec::new(),
        });
    }
    Ok(M11GfmTableRow {
        source_range: to_u32_range(line.clone())?,
        cells,
    })
}

fn alignment(content: &str) -> M11GfmTableAlignment {
    let left = content.starts_with(':');
    let right = content.ends_with(':');
    match (left, right) {
        (true, true) => M11GfmTableAlignment::Center,
        (true, false) => M11GfmTableAlignment::Left,
        (false, true) => M11GfmTableAlignment::Right,
        (false, false) => M11GfmTableAlignment::None,
    }
}

fn logical_lines(input: &str) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in input.bytes().enumerate() {
        if byte == b'\n' {
            lines.push(start..index + 1);
            start = index + 1;
        }
    }
    if start < input.len() {
        lines.push(start..input.len());
    }
    lines
}

fn to_u32_range(range: Range<usize>) -> Result<Range<u32>, M11GfmTableProjectionError> {
    Ok(
        u32::try_from(range.start).map_err(|_| M11GfmTableProjectionError::CoordinateOverflow)?
            ..u32::try_from(range.end)
                .map_err(|_| M11GfmTableProjectionError::CoordinateOverflow)?,
    )
}

fn map_facade<T>(result: Result<T, FacadeError>) -> Result<T, M11GfmTableProjectionError> {
    match result {
        Ok(value) => Ok(value),
        Err(FacadeError::OverCap { bytes, cap }) => {
            Err(M11GfmTableProjectionError::OverCap { bytes, cap })
        }
        Err(FacadeError::UnsupportedHtmlBlockType(_)) => {
            Err(M11GfmTableProjectionError::CoordinateOverflow)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_alignment_body_padding_and_preface() {
        let input = "preface\nfoo | bar\n:--- | ---:\nbaz\nqux | quux | ignored\n";
        let table = project_m11_gfm_table(input).unwrap().unwrap();
        assert_eq!(table.preface_range, Some(0..8));
        assert_eq!(
            table.alignments,
            vec![M11GfmTableAlignment::Left, M11GfmTableAlignment::Right]
        );
        assert_eq!(table.header.cells[0].cooked_content, "foo");
        assert_eq!(table.header.cells[1].cooked_content, "bar");
        assert_eq!(table.body.len(), 2);
        assert_eq!(table.body[0].cells[0].cooked_content, "baz");
        assert!(table.body[0].cells[1].autocompleted);
        assert_eq!(table.body[1].cells[1].cooked_content, "quux");
    }

    #[test]
    fn escaped_pipes_do_not_split_cells_and_are_cooked() {
        let input = "f\\|oo | b\n--- | ---\n`x\\|y` | z\n";
        let table = project_m11_gfm_table(input).unwrap().unwrap();
        assert_eq!(table.header.cells[0].cooked_content, "f|oo");
        assert_eq!(table.body[0].cells[0].cooked_content, "`x|y`");
    }

    #[test]
    fn mismatched_header_and_delimiter_fail_closed() {
        assert_eq!(
            project_m11_gfm_table("foo | bar\n---\nbaz\n").unwrap(),
            None
        );
    }
}
