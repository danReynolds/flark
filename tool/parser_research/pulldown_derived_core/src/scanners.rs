//! Bounded block scanners derived from Pulldown-Cmark 0.13.4.
//!
//! Donor correspondence is recorded in the crate README. The important
//! adaptation is that scanners operate over a capped syntactic prefix plus
//! constant-size summaries of the rest of a physical line. No scanner can
//! turn a 10 MB line into one uninterruptible call or one node per byte.

use crate::model::{HeadingLevel, ParseError};

pub(crate) const PREFIX_LIMIT: usize = 512;
pub(crate) const MAX_CONTAINER_DEPTH: usize = 64;

const PATTERN_BYTES: [u8; 6] = [b'=', b'-', b'`', b'~', b'*', b'_'];

fn is_horizontal_whitespace(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn pattern_index(byte: u8) -> Option<usize> {
    PATTERN_BYTES
        .iter()
        .position(|candidate| *candidate == byte)
}

#[derive(Clone, Copy, Debug, Default)]
struct TailPattern {
    len: usize,
    leading: usize,
    total: usize,
    rest_all_whitespace: bool,
    rest_all_spaces: bool,
    only_char_or_whitespace: bool,
}

impl TailPattern {
    fn empty() -> Self {
        Self {
            len: 0,
            leading: 0,
            total: 0,
            rest_all_whitespace: true,
            rest_all_spaces: true,
            only_char_or_whitespace: true,
        }
    }

    fn observe(&mut self, byte: u8, candidate: u8) {
        if self.len == self.leading && byte == candidate {
            self.leading += 1;
        } else {
            self.rest_all_whitespace &= is_horizontal_whitespace(byte);
            self.rest_all_spaces &= byte == b' ';
        }
        self.total += usize::from(byte == candidate);
        self.only_char_or_whitespace &= byte == candidate || is_horizontal_whitespace(byte);
        self.len += 1;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LineWork {
    pub(crate) start: usize,
    prefix: Vec<u8>,
    tail_len: usize,
    tail_all_whitespace: bool,
    tail_first_non_whitespace: Option<usize>,
    patterns: [TailPattern; PATTERN_BYTES.len()],
    pub(crate) digest: u64,
    pub(crate) last_non_whitespace: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PatternMatch {
    pub(crate) leading: usize,
    pub(crate) total: usize,
    pub(crate) valid_trailing_whitespace: bool,
    pub(crate) valid_trailing_spaces: bool,
}

impl LineWork {
    pub(crate) fn new(start: usize) -> Self {
        Self {
            start,
            prefix: Vec::with_capacity(PREFIX_LIMIT),
            tail_len: 0,
            tail_all_whitespace: true,
            tail_first_non_whitespace: None,
            patterns: [TailPattern::empty(); PATTERN_BYTES.len()],
            // FNV-1a offset basis. This digest participates in convergence for
            // a paragraph that a following setext line may still promote.
            digest: 0xcbf2_9ce4_8422_2325,
            last_non_whitespace: None,
        }
    }

    pub(crate) fn observe(&mut self, absolute: usize, byte: u8) {
        self.digest ^= u64::from(byte);
        self.digest = self.digest.wrapping_mul(0x0000_0100_0000_01b3);
        if !is_horizontal_whitespace(byte) {
            self.last_non_whitespace = Some(absolute + 1);
        }
        if self.prefix.len() < PREFIX_LIMIT {
            self.prefix.push(byte);
            return;
        }

        self.tail_all_whitespace &= is_horizontal_whitespace(byte);
        if self.tail_first_non_whitespace.is_none() && !is_horizontal_whitespace(byte) {
            self.tail_first_non_whitespace = Some(absolute);
        }
        for (candidate, pattern) in PATTERN_BYTES.iter().zip(self.patterns.iter_mut()) {
            pattern.observe(byte, *candidate);
        }
        self.tail_len += 1;
    }

    pub(crate) fn len(&self) -> usize {
        self.prefix.len() + self.tail_len
    }

    pub(crate) fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    pub(crate) fn is_blank_from(&self, relative: usize) -> Result<bool, ParseError> {
        if relative > self.prefix.len() {
            return Err(ParseError::PrefixLimit {
                line_start: self.start,
            });
        }
        Ok(self.prefix[relative..]
            .iter()
            .all(|byte| is_horizontal_whitespace(*byte))
            && self.tail_all_whitespace)
    }

    pub(crate) fn first_non_whitespace_from(&self, relative: usize) -> Result<usize, ParseError> {
        if relative > self.prefix.len() {
            return Err(ParseError::PrefixLimit {
                line_start: self.start,
            });
        }
        if let Some(found) = self.prefix[relative..]
            .iter()
            .position(|byte| !is_horizontal_whitespace(*byte))
        {
            return Ok(relative + found);
        }
        if let Some(absolute) = self.tail_first_non_whitespace {
            return Ok(absolute - self.start);
        }
        Ok(self.len())
    }

    pub(crate) fn pattern_from(
        &self,
        relative: usize,
        candidate: u8,
    ) -> Result<PatternMatch, ParseError> {
        if relative > self.prefix.len() {
            return Err(ParseError::PrefixLimit {
                line_start: self.start,
            });
        }
        let tail = self.patterns[pattern_index(candidate).expect("registered pattern byte")];
        let prefix = &self.prefix[relative..];
        let prefix_leading = prefix.iter().take_while(|byte| **byte == candidate).count();
        let prefix_rest = &prefix[prefix_leading..];
        let prefix_in_trailing = !prefix_rest.is_empty();
        let prefix_rest_whitespace = prefix_rest
            .iter()
            .all(|byte| is_horizontal_whitespace(*byte));
        let prefix_rest_spaces = prefix_rest.iter().all(|byte| *byte == b' ');
        let prefix_total = prefix.iter().filter(|byte| **byte == candidate).count();

        let (leading, valid_whitespace, valid_spaces) = if prefix_in_trailing {
            (
                prefix_leading,
                prefix_rest_whitespace && tail_all_whitespace(tail),
                prefix_rest_spaces && tail_all_spaces(tail),
            )
        } else {
            (
                prefix_leading + tail.leading,
                tail.rest_all_whitespace,
                tail.rest_all_spaces,
            )
        };

        Ok(PatternMatch {
            leading,
            total: prefix_total + tail.total,
            valid_trailing_whitespace: valid_whitespace,
            valid_trailing_spaces: valid_spaces,
        })
    }

    pub(crate) fn is_thematic_break_from(
        &self,
        relative: usize,
        candidate: u8,
    ) -> Result<bool, ParseError> {
        if relative > self.prefix.len() {
            return Err(ParseError::PrefixLimit {
                line_start: self.start,
            });
        }
        let prefix = &self.prefix[relative..];
        let prefix_valid = prefix
            .iter()
            .all(|byte| *byte == candidate || is_horizontal_whitespace(*byte));
        let prefix_count = prefix.iter().filter(|byte| **byte == candidate).count();
        let tail = self.patterns[pattern_index(candidate).expect("registered pattern byte")];
        Ok(prefix_valid && tail.only_char_or_whitespace && prefix_count + tail.total >= 3)
    }

    pub(crate) fn transient_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.prefix.capacity()
    }
}

fn tail_all_whitespace(pattern: TailPattern) -> bool {
    pattern.leading == 0 && pattern.rest_all_whitespace
}

fn tail_all_spaces(pattern: TailPattern) -> bool {
    pattern.leading == 0 && pattern.rest_all_spaces
}

#[derive(Clone, Debug)]
pub(crate) struct LineCursor<'a> {
    line: &'a LineWork,
    ix: usize,
    tab_start: usize,
    spaces_remaining: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ListScan {
    pub(crate) marker: u8,
    pub(crate) start: u32,
    pub(crate) indent: usize,
    pub(crate) marker_start: usize,
    pub(crate) marker_end: usize,
}

impl<'a> LineCursor<'a> {
    pub(crate) fn new(line: &'a LineWork) -> Self {
        Self {
            line,
            ix: 0,
            tab_start: 0,
            spaces_remaining: 0,
        }
    }

    pub(crate) fn position(&self) -> usize {
        self.ix
    }

    fn byte(&self) -> Result<Option<u8>, ParseError> {
        if let Some(byte) = self.line.prefix().get(self.ix) {
            Ok(Some(*byte))
        } else if self.ix == self.line.len() {
            Ok(None)
        } else {
            Err(ParseError::PrefixLimit {
                line_start: self.line.start,
            })
        }
    }

    pub(crate) fn is_at_eol(&self) -> Result<bool, ParseError> {
        Ok(self.ix == self.line.len())
    }

    fn scan_ch(&mut self, byte: u8) -> Result<bool, ParseError> {
        if self.byte()? == Some(byte) {
            self.ix += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn scan_space(&mut self, spaces: usize) -> Result<bool, ParseError> {
        Ok(self.scan_space_inner(spaces)? == 0)
    }

    pub(crate) fn scan_space_upto(&mut self, spaces: usize) -> Result<usize, ParseError> {
        Ok(spaces - self.scan_space_inner(spaces)?)
    }

    fn scan_space_inner(&mut self, mut spaces: usize) -> Result<usize, ParseError> {
        let from_remaining = self.spaces_remaining.min(spaces);
        self.spaces_remaining -= from_remaining;
        spaces -= from_remaining;

        while spaces > 0 {
            match self.byte()? {
                Some(b' ') => {
                    self.ix += 1;
                    spaces -= 1;
                }
                Some(b'\t') => {
                    let width = 4 - (self.ix - self.tab_start) % 4;
                    self.ix += 1;
                    self.tab_start = self.ix;
                    let consumed = width.min(spaces);
                    spaces -= consumed;
                    self.spaces_remaining = width - consumed;
                }
                _ => break,
            }
        }
        Ok(spaces)
    }

    pub(crate) fn scan_all_space(&mut self) -> Result<(), ParseError> {
        self.spaces_remaining = 0;
        loop {
            match self.byte()? {
                Some(b' ' | b'\t') => self.ix += 1,
                _ => return Ok(()),
            }
        }
    }

    /// Pulldown 0.13.4 `LineStart::scan_blockquote_marker`, adapted to return
    /// the exact marker range needed by Flark.
    pub(crate) fn scan_blockquote_marker(&mut self) -> Result<Option<RangeMarker>, ParseError> {
        let start = self.ix;
        if self.scan_ch(b'>')? {
            let end = self.ix;
            let _ = self.scan_space(1)?;
            Ok(Some(RangeMarker { start, end }))
        } else {
            Ok(None)
        }
    }

    /// Pulldown 0.13.4 `scan_list_marker_with_indent` and
    /// `finish_list_marker`, with the thematic-break query supplied by the
    /// bounded whole-line summary.
    pub(crate) fn scan_list_marker_with_indent(
        &mut self,
        outer_indent: usize,
    ) -> Result<Option<ListScan>, ParseError> {
        let saved = self.clone();
        let marker_start = self.ix;
        let Some(byte) = self.byte()? else {
            return Ok(None);
        };
        if matches!(byte, b'-' | b'+' | b'*') {
            if byte != b'+' && self.line.is_thematic_break_from(marker_start, byte)? {
                return Ok(None);
            }
            self.ix += 1;
            let marker_end = self.ix;
            if self.scan_space(1)? || self.is_at_eol()? {
                return self.finish_list_marker(
                    byte,
                    0,
                    outer_indent + 2,
                    marker_start,
                    marker_end,
                );
            }
        } else if byte.is_ascii_digit() {
            let digit_start = self.ix;
            let mut value = u32::from(byte - b'0');
            self.ix += 1;
            while self.ix - digit_start < 10 {
                match self.byte()? {
                    Some(next) if next.is_ascii_digit() => {
                        value = value
                            .checked_mul(10)
                            .and_then(|value| value.checked_add(u32::from(next - b'0')))
                            .unwrap_or(u32::MAX);
                        self.ix += 1;
                    }
                    Some(delimiter @ (b'.' | b')')) => {
                        self.ix += 1;
                        let marker_end = self.ix;
                        if self.scan_space(1)? || self.is_at_eol()? {
                            return self.finish_list_marker(
                                delimiter,
                                value,
                                outer_indent + 1 + self.ix - digit_start,
                                marker_start,
                                marker_end,
                            );
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
        *self = saved;
        Ok(None)
    }

    fn finish_list_marker(
        &mut self,
        marker: u8,
        start: u32,
        mut indent: usize,
        marker_start: usize,
        marker_end: usize,
    ) -> Result<Option<ListScan>, ParseError> {
        let saved = self.clone();
        if self.line.is_blank_from(self.ix)? {
            return Ok(Some(ListScan {
                marker,
                start,
                indent,
                marker_start,
                marker_end,
            }));
        }
        let post_indent = self.scan_space_upto(4)?;
        if post_indent < 4 {
            indent += post_indent;
        } else {
            *self = saved;
        }
        Ok(Some(ListScan {
            marker,
            start,
            indent,
            marker_start,
            marker_end,
        }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RangeMarker {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

pub(crate) fn scan_setext(
    line: &LineWork,
    relative: usize,
) -> Result<Option<(HeadingLevel, usize)>, ParseError> {
    for (candidate, level) in [(b'=', HeadingLevel::H1), (b'-', HeadingLevel::H2)] {
        let pattern = line.pattern_from(relative, candidate)?;
        if pattern.leading > 0 && pattern.valid_trailing_whitespace {
            return Ok(Some((level, pattern.leading)));
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FenceScan {
    pub(crate) marker: u8,
    pub(crate) len: usize,
    pub(crate) info_start: usize,
}

pub(crate) fn scan_opening_fence(
    line: &LineWork,
    relative: usize,
) -> Result<Option<FenceScan>, ParseError> {
    for marker in [b'`', b'~'] {
        let pattern = line.pattern_from(relative, marker)?;
        if pattern.leading < 3 {
            continue;
        }
        // Pulldown rejects a backtick fence when its info string contains a
        // backtick. `total` lets the bounded tail summary make that decision
        // without rescanning the physical line.
        if marker == b'`' && pattern.total != pattern.leading {
            continue;
        }
        let marker_end = relative + pattern.leading;
        let info_start = line.first_non_whitespace_from(marker_end)?;
        return Ok(Some(FenceScan {
            marker,
            len: pattern.leading,
            info_start,
        }));
    }
    Ok(None)
}

pub(crate) fn scan_closing_fence(
    line: &LineWork,
    relative: usize,
    marker: u8,
    opening_len: usize,
) -> Result<Option<usize>, ParseError> {
    let pattern = line.pattern_from(relative, marker)?;
    Ok(
        (pattern.leading >= opening_len && pattern.valid_trailing_spaces)
            .then_some(pattern.leading),
    )
}
