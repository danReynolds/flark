//! Bounded lexical facade for a caller-owned, source-backed block spine.
//!
//! This deliberately exposes Comrak's generated scanners and exact table and
//! reference helpers without exposing `Parser`, arena nodes, or finalization.
//! Calls are synchronous; the caller must enforce [`MAX_CLASSIFICATION_BYTES`]
//! and keep oversized regions source-visible rather than call this facade.

#![allow(missing_copy_implementations, missing_docs)]

use std::ops::Range;

use crate::parser::inlines::{self, Scanner};
use crate::scanners;
use crate::strings::{self, Case};

/// Temporary atomic ceiling for the lexical donor calls.
///
/// This is not a Markdown or M1.1 document-size limit. The caller must retain
/// resumable source ownership and either prove an oversized line cannot start
/// a competing opener from its bounded prefix, or fail that line closed.
pub const MAX_CLASSIFICATION_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FacadeError {
    OverCap { bytes: usize, cap: usize },
    UnsupportedHtmlBlockType(u8),
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

/// Exact CommonMark block-quote opener at an already-established first
/// nonspace byte. The optional greentext extension is deliberately disabled
/// by the M1.1 grammar profile.
pub fn block_quote_start(input: &str) -> Result<bool, FacadeError> {
    bounded(input)?;
    Ok(input.as_bytes().first() == Some(&b'>'))
}

/// Exact CommonMark thematic-break recognition at first nonspace.
///
/// Provenance: Comrak 0.54.0 `Parser::scan_thematic_break_inner`,
/// `src/parser/mod.rs` lines 1442-1477, SHA-256
/// `b39e1d65357f6cbfe953baf9cb769ebff2158f08fe1eaa60bc2fdcc4cfe4a675`.
pub fn thematic_break(input: &str) -> Result<bool, FacadeError> {
    bounded(input)?;
    let bytes = input.as_bytes();
    let Some(marker @ (b'*' | b'_' | b'-')) = bytes.first().copied() else {
        return Ok(false);
    };
    let mut count = 0;
    for byte in bytes {
        match *byte {
            byte if byte == marker => count += 1,
            b' ' | b'\t' => {}
            b'\r' | b'\n' => break,
            _ => return Ok(false),
        }
    }
    Ok(count >= 3)
}

/// Exact CommonMark list-marker recognition at first nonspace.
///
/// The result carries no list facts because the M1.1 caller only needs the
/// donor's competing-opener verdict. Paragraph interruption rules remain in
/// this donor function.
///
/// Provenance: Comrak 0.54.0 `parse_list_marker`, `src/parser/mod.rs` lines
/// 2767-2882, SHA-256
/// `25ea7a12c9bb73b50116acd4437c50e5dff78a8a21d2fafaf0075a95bc9e852b`.
pub fn list_marker_start(input: &str, interrupts_paragraph: bool) -> Result<bool, FacadeError> {
    bounded(input)?;
    let bytes = input.as_bytes();
    let Some(mut marker) = bytes.first().copied() else {
        return Ok(false);
    };
    let mut position = 0;

    if matches!(marker, b'*' | b'-' | b'+') {
        position += 1;
        if !bytes
            .get(position)
            .is_none_or(|byte| crate::ctype::isspace(*byte))
        {
            return Ok(false);
        }
        return Ok(!interrupts_paragraph || has_list_item_content(bytes, position));
    }

    if !crate::ctype::isdigit(marker) {
        return Ok(false);
    }
    let mut start = 0_usize;
    let mut digits = 0;
    loop {
        start = start * 10 + usize::from(bytes[position] - b'0');
        position += 1;
        digits += 1;
        if position == bytes.len() {
            return Ok(false);
        }
        if !(digits < 9 && crate::ctype::isdigit(bytes[position])) {
            break;
        }
    }
    if interrupts_paragraph && start != 1 {
        return Ok(false);
    }
    marker = bytes[position];
    if !matches!(marker, b'.' | b')') {
        return Ok(false);
    }
    position += 1;
    if position == bytes.len() || !crate::ctype::isspace(bytes[position]) {
        return Ok(false);
    }
    Ok(!interrupts_paragraph || has_list_item_content(bytes, position))
}

fn has_list_item_content(bytes: &[u8], mut position: usize) -> bool {
    if position == bytes.len() {
        return false;
    }
    while crate::strings::is_space_or_tab(bytes[position]) {
        position += 1;
        if position == bytes.len() {
            return false;
        }
    }
    !crate::strings::is_line_end_char(bytes[position])
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

/// Exact source cuts for one inline link/image tail beginning immediately
/// after the closing label bracket.
///
/// `source` includes the opening and closing parentheses. `url_source`
/// excludes optional angle brackets, matching the slice passed to Comrak's
/// `clean_url`. `title_source` includes its quote/parenthesis delimiters,
/// matching the slice passed to `clean_title`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacadeDirectLinkTail {
    pub source: Range<usize>,
    pub url_source: Range<usize>,
    pub title_source: Option<Range<usize>>,
}

/// Recognize one direct inline link/image tail with the exact donor scanner
/// sequence used by `Subject::handle_close_bracket`.
///
/// The caller owns bracket precedence and supplies a bounded candidate
/// beginning with `(`. This facade deliberately returns source cuts rather
/// than cooked strings so the production parser can stream the selected
/// values through its source-backed cleaner.
pub fn direct_link_tail(input: &str) -> Result<Option<FacadeDirectLinkTail>, FacadeError> {
    bounded(input)?;
    if input.as_bytes().first() != Some(&b'(') {
        return Ok(None);
    }

    let leading = scanners::spacechars(&input[1..]).unwrap_or(0);
    let destination_scan_start = 1 + leading;
    if destination_scan_start >= input.len() {
        return Ok(None);
    }
    let Some((url, matchlen)) = inlines::manual_scan_link_url(&input[destination_scan_start..])
    else {
        return Ok(None);
    };
    let url_start = destination_scan_start + subslice_offset(&input[destination_scan_start..], url);
    let destination_scan_end = destination_scan_start + matchlen;
    let title_start =
        destination_scan_end + scanners::spacechars(&input[destination_scan_end..]).unwrap_or(0);
    let title_match = (title_start != destination_scan_end)
        .then(|| scanners::link_title(&input[title_start..]))
        .flatten();
    let title_end = title_start + title_match.unwrap_or(0);
    let end = title_end + scanners::spacechars(&input[title_end..]).unwrap_or(0);
    if input.as_bytes().get(end) != Some(&b')') {
        return Ok(None);
    }

    Ok(Some(FacadeDirectLinkTail {
        source: 0..end + 1,
        url_source: url_start..url_start + url.len(),
        title_source: title_match.map(|_| title_start..title_end),
    }))
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
    })
}

fn subslice_offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

/// Decodes one complete, parser-bounded entity candidate using Comrak's
/// pinned entity table. `candidate` excludes the leading `&` and includes the
/// terminal `;`; partial matches are rejected so a streaming caller can replay
/// the spelling literally.
pub fn decode_reference_entity(candidate: &str, output: &mut [u8]) -> Option<usize> {
    let (decoded, consumed) = crate::entity::unescape(candidate)?;
    if consumed != candidate.len() || decoded.len() > output.len() {
        return None;
    }
    output[..decoded.len()].copy_from_slice(decoded.as_bytes());
    Some(decoded.len())
}

/// Atomic differential oracle for the Flark-owned streaming destination
/// cleaner. Production value paths call only [`decode_reference_entity`].
pub fn clean_reference_destination(input: &str) -> Result<String, FacadeError> {
    bounded(input)?;
    Ok(strings::clean_url(input).into_owned())
}

/// Atomic differential oracle for the Flark-owned streaming title cleaner.
/// Production value paths call only [`decode_reference_entity`].
pub fn clean_reference_title(input: &str) -> Result<String, FacadeError> {
    bounded(input)?;
    Ok(strings::clean_title(input).into_owned())
}
