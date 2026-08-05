//! Bounded lexical facade for a caller-owned, source-backed block spine.
//!
//! This deliberately exposes Comrak's generated scanners and exact table and
//! reference helpers without exposing `Parser`, arena nodes, or finalization.
//! Calls are synchronous; the caller must enforce [`MAX_CLASSIFICATION_BYTES`]
//! and keep oversized regions source-visible rather than call this facade.

#![allow(missing_copy_implementations, missing_docs)]

use std::borrow::Cow;
use std::ops::Range;

use crate::Arena;
use crate::nodes::{Ast, NodeValue, TableAlignment};
use crate::parser::ResolvedReference;
use crate::parser::inlines::{self, Scanner};
use crate::scanners;
use crate::strings::{self, Case};

use super::{Options, Parser, table};

/// Hard ceiling that makes generated scanner calls atomically bounded.
pub const MAX_CLASSIFICATION_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FacadeError {
    OverCap { bytes: usize, cap: usize },
    UnsupportedHtmlBlockType(u8),
}

/// Oracle-only pre-inline block record used to falsify the value translation.
/// This is deliberately not used by the candidate parser path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacadeOracleBlock {
    pub parent: Option<usize>,
    pub kind: String,
    pub source: [usize; 4],
    pub logical: String,
    pub line_offsets: Vec<usize>,
}

/// Run pristine Comrak block finalization without inline parsing and expose a
/// stable value projection for differential tests. This is an oracle hook,
/// not a fallback or a production parser entry point.
pub fn oracle_block_projection(input: &str, gfm: bool) -> Vec<FacadeOracleBlock> {
    let arena = Arena::new();
    let root = arena.alloc(
        Ast {
            value: NodeValue::Document,
            content: String::new(),
            sourcepos: (1, 1, 1, 1).into(),
            #[cfg(feature = "attributes")]
            attrs: None,
            open: true,
            last_line_blank: false,
            table_visited: false,
            line_offsets: Vec::new(),
        }
        .into(),
    );
    let mut options = Options::default();
    if gfm {
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.extension.autolink = true;
        options.extension.tagfilter = true;
        options.extension.tasklist = true;
    }
    let mut parser = Parser::new(&arena, root, &options);
    let mut start = 0;
    while start < input.len() {
        let bytes = input.as_bytes();
        let mut end = start;
        while end < bytes.len() && !matches!(bytes[end], b'\r' | b'\n') {
            end += 1;
        }
        if end < bytes.len() {
            if bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n') {
                end += 2;
            } else {
                end += 1;
            }
        }
        parser.process_line(&input[start..end]);
        start = end;
    }
    while !parser.current.same_node(root) {
        parser.current = parser
            .finalize(parser.current)
            .expect("open node has parent");
    }
    let _ = parser.finalize(root);
    parser.propagate_list_sourcepos(root);

    let nodes = root.descendants().collect::<Vec<_>>();
    nodes
        .iter()
        .map(|node| {
            let ast = node.data();
            let parent = node.parent().and_then(|parent| {
                nodes
                    .iter()
                    .position(|candidate| candidate.same_node(parent))
            });
            FacadeOracleBlock {
                parent,
                kind: oracle_kind(&ast.value),
                source: [
                    ast.sourcepos.start.line,
                    ast.sourcepos.start.column,
                    ast.sourcepos.end.line,
                    ast.sourcepos.end.column,
                ],
                logical: ast.content.clone(),
                line_offsets: ast.line_offsets.clone(),
            }
        })
        .collect()
}

fn oracle_kind(value: &NodeValue) -> String {
    match value {
        NodeValue::Document => "document".to_owned(),
        NodeValue::BlockQuote => "block_quote".to_owned(),
        NodeValue::List(list) => format!(
            "list:{:?}:{}:{:?}:{}:{}:{}:{}",
            list.list_type,
            list.start,
            list.delimiter,
            list.bullet_char,
            list.marker_offset,
            list.padding,
            list.tight
        ),
        NodeValue::Item(list) => format!(
            "item:{:?}:{}:{:?}:{}:{}:{}",
            list.list_type,
            list.start,
            list.delimiter,
            list.bullet_char,
            list.marker_offset,
            list.padding
        ),
        NodeValue::CodeBlock(code) => format!(
            "code:{}:{}:{}:{}:{:?}:{:?}",
            code.fenced,
            code.fence_char,
            code.fence_length,
            code.fence_offset,
            code.info,
            code.literal
        ),
        NodeValue::HtmlBlock(html) => {
            format!("html:{}:{:?}", html.block_type, html.literal)
        }
        NodeValue::Paragraph => "paragraph".to_owned(),
        NodeValue::Heading(heading) => format!(
            "heading:{}:{}:{}",
            heading.level, heading.setext, heading.closed
        ),
        NodeValue::ThematicBreak => "thematic_break".to_owned(),
        NodeValue::Table(table) => format!(
            "table:{:?}:{}:{}:{}",
            table
                .alignments
                .iter()
                .map(|alignment| match alignment {
                    TableAlignment::None => "none",
                    TableAlignment::Left => "left",
                    TableAlignment::Center => "center",
                    TableAlignment::Right => "right",
                })
                .collect::<Vec<_>>(),
            table.num_columns,
            table.num_rows,
            table.num_nonempty_cells
        ),
        NodeValue::TableRow(header) => format!("table_row:{header}"),
        NodeValue::TableCell => "table_cell".to_owned(),
        other => format!("unsupported:{}", other.xml_node_name()),
    }
}

fn bounded(input: &str) -> Result<(), FacadeError> {
    if input.len() > MAX_CLASSIFICATION_BYTES {
        Err(FacadeError::OverCap {
            bytes: input.len(),
            cap: MAX_CLASSIFICATION_BYTES,
        })
    } else {
        Ok(())
    }
}

/// Exact CommonMark HTML block start classification. Type 7 can be disabled
/// while a paragraph is open, matching `Parser::detect_html_block`.
pub fn html_block_start(input: &str, allow_type_7: bool) -> Result<Option<u8>, FacadeError> {
    bounded(input)?;
    Ok(scanners::html_block_start(input)
        .or_else(|| {
            allow_type_7
                .then(|| scanners::html_block_start_7(input))
                .flatten()
        })
        .map(|kind| kind as u8))
}

/// Exact same-line terminator test for HTML block types 1 through 5. Types 6
/// and 7 are blank-line terminated by the caller-owned block state.
pub fn html_block_end(block_type: u8, input: &str) -> Result<bool, FacadeError> {
    bounded(input)?;
    match block_type {
        1 => Ok(scanners::html_block_end_1(input)),
        2 => Ok(scanners::html_block_end_2(input)),
        3 => Ok(scanners::html_block_end_3(input)),
        4 => Ok(scanners::html_block_end_4(input)),
        5 => Ok(scanners::html_block_end_5(input)),
        6 | 7 => Ok(false),
        other => Err(FacadeError::UnsupportedHtmlBlockType(other)),
    }
}

/// Exact generated ATX-heading opener length.
pub fn atx_heading_start(input: &str) -> Result<Option<usize>, FacadeError> {
    bounded(input)?;
    Ok(scanners::atx_heading_start(input))
}

/// Exact generated fenced-code opener run length.
pub fn open_code_fence(input: &str) -> Result<Option<usize>, FacadeError> {
    bounded(input)?;
    Ok(scanners::open_code_fence(input))
}

/// Exact generated fenced-code closer run length.
pub fn close_code_fence(input: &str) -> Result<Option<usize>, FacadeError> {
    bounded(input)?;
    Ok(scanners::close_code_fence(input))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacadeSetextChar {
    Equals,
    Hyphen,
}

/// Exact generated setext-underline classification.
pub fn setext_heading_line(input: &str) -> Result<Option<FacadeSetextChar>, FacadeError> {
    bounded(input)?;
    Ok(scanners::setext_heading_line(input).map(|kind| match kind {
        scanners::SetextChar::Equals => FacadeSetextChar::Equals,
        scanners::SetextChar::Hyphen => FacadeSetextChar::Hyphen,
    }))
}

/// Exact donor trailing-ATX-marker trimming and closed-marker fact.
pub fn chop_trailing_hashes(input: &str) -> Result<(&str, bool), FacadeError> {
    bounded(input)?;
    Ok(strings::chop_trailing_hashes(input))
}

/// Exact donor normalization of a fenced-code info string.
pub fn normalize_code_info(input: &str) -> Result<String, FacadeError> {
    bounded(input)?;
    let mut info = crate::entity::unescape_html(input);
    strings::trim_cow(&mut info);
    let mut info = info.into_owned();
    strings::unescape(&mut info);
    Ok(info)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacadeAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacadeTableCell {
    /// Raw cell span relative to the classified input. Consumers retain this
    /// source range; `content` exists only to differential the donor helper.
    pub source: Range<usize>,
    pub internal_offset: usize,
    pub had_escaped_pipe: bool,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacadeTableRow {
    pub paragraph_offset: usize,
    pub cells: Vec<FacadeTableCell>,
}

/// Exact Comrak table-row tokenization, including escaped pipes. This calls
/// the existing donor helper rather than maintaining a second row grammar.
pub fn table_row(input: &str, spoiler: bool) -> Result<Option<FacadeTableRow>, FacadeError> {
    bounded(input)?;
    Ok(table::row(input, spoiler).map(|row| FacadeTableRow {
        paragraph_offset: row.paragraph_offset,
        cells: row
            .cells
            .into_iter()
            .map(|cell| FacadeTableCell {
                source: cell.start_offset..cell.end_offset.saturating_add(1),
                internal_offset: cell.internal_offset,
                had_escaped_pipe: matches!(cell.content, Cow::Owned(_)),
                content: cell.content.into_owned(),
            })
            .collect(),
    }))
}

/// Validate a delimiter row and derive its exact alignment vector. Header
/// activation still belongs to the block spine because it depends on the
/// preceding paragraph and equal cell counts.
pub fn table_delimiter_alignments(
    input: &str,
    spoiler: bool,
) -> Result<Option<Vec<FacadeAlignment>>, FacadeError> {
    let Some(row) = table_row(input, spoiler)? else {
        return Ok(None);
    };
    let mut alignments = Vec::with_capacity(row.cells.len());
    for cell in row.cells {
        let content = cell.content.as_bytes();
        if content.is_empty()
            || !content.iter().all(|byte| matches!(byte, b'-' | b':'))
            || !content.contains(&b'-')
        {
            return Ok(None);
        }
        let left = content.first() == Some(&b':');
        let right = content.last() == Some(&b':');
        alignments.push(match (left, right) {
            (true, true) => FacadeAlignment::Center,
            (true, false) => FacadeAlignment::Left,
            (false, true) => FacadeAlignment::Right,
            (false, false) => FacadeAlignment::None,
        });
    }
    Ok(Some(alignments))
}

/// Exact donor prefilter for a potential GFM table delimiter line. Keeping
/// this separate from full delimiter validation preserves Comrak's
/// `table_visited` transition: non-candidates remain retryable, while a
/// malformed candidate permanently disqualifies that paragraph.
pub fn table_delimiter_candidate(input: &str) -> Result<bool, FacadeError> {
    bounded(input)?;
    Ok(scanners::table_start(input).is_some())
}

#[derive(Clone, Debug)]
pub struct FacadeReferenceDefinition {
    pub source: Range<usize>,
    pub label_source: Range<usize>,
    pub url_source: Range<usize>,
    pub title_source: Option<Range<usize>>,
    pub normalized_label: String,
    pub resolved: ResolvedReference,
}

/// Normalize one already-recognized reference label with the exact donor
/// transform used by [`reference_definitions`].  Incremental block consumers
/// retain only the spec-bounded label; destinations and titles remain
/// source-backed and must not be materialized merely to obtain this fact.
pub fn normalize_reference_label(label: &str) -> String {
    strings::normalize_label(label, Case::Fold)
}

/// Extract all leading reference definitions from one logical paragraph.
/// The caller inserts them into its persistent symbol map in source order;
/// first-definition-wins is therefore a state/output concern, not duplicated
/// lexical semantics.
pub fn reference_definitions(input: &str) -> Result<Vec<FacadeReferenceDefinition>, FacadeError> {
    bounded(input)?;
    let mut offset = 0;
    let mut definitions = Vec::new();
    while offset < input.len() && input.as_bytes()[offset] == b'[' {
        let Some(definition) = reference_definition(&input[offset..], offset) else {
            break;
        };
        offset = definition.source.end;
        definitions.push(definition);
    }
    Ok(definitions)
}

fn reference_definition(input: &str, base: usize) -> Option<FacadeReferenceDefinition> {
    let mut scanner = Scanner::new();
    let label = scanner.link_label(input)?;
    if label.is_empty() || scanner.peek_byte(input) != Some(b':') {
        return None;
    }
    let label_start = subslice_offset(input, label);
    scanner.pos += 1;
    scanner.spnl(input);

    let (url, matchlen) = inlines::manual_scan_link_url(&input[scanner.pos..])?;
    let url_start = scanner.pos + subslice_offset(&input[scanner.pos..], url);
    scanner.pos += matchlen;

    let before_title = scanner.pos;
    scanner.spnl(input);
    let title_match = if scanner.pos == before_title {
        None
    } else {
        scanners::link_title(&input[scanner.pos..])
    };
    let (title, title_source) = match title_match {
        Some(matchlen) => {
            let range = scanner.pos..scanner.pos + matchlen;
            scanner.pos += matchlen;
            (&input[range.clone()], Some(range))
        }
        None => {
            scanner.pos = before_title;
            ("", None)
        }
    };

    scanner.skip_spaces(input);
    if !scanner.skip_line_end(input) {
        if title.is_empty() {
            return None;
        }
        scanner.pos = before_title;
        scanner.skip_spaces(input);
        if !scanner.skip_line_end(input) {
            return None;
        }
    }

    let normalized_label = strings::normalize_label(label, Case::Fold);
    if normalized_label.is_empty() {
        return None;
    }
    Some(FacadeReferenceDefinition {
        source: base..base + scanner.pos,
        label_source: base + label_start..base + label_start + label.len(),
        url_source: base + url_start..base + url_start + url.len(),
        title_source: title_source.map(|range| base + range.start..base + range.end),
        normalized_label,
        resolved: ResolvedReference {
            url: strings::clean_url(url).into_owned(),
            title: strings::clean_title(title).into_owned(),
        },
    })
}

fn subslice_offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}
