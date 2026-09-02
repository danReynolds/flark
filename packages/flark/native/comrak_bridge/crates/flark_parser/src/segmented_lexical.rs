//! Flark-owned, source-segmented correspondents of Comrak's block lexers.
//!
//! This module is deliberately lexical. It cannot admit a Markdown block and
//! it does not choose rule precedence. The direct block controller owns those
//! grammar transitions; these states only summarize one forward-only physical
//! line. The same states are used for every input size.

use comrak::block_spine_facade::{self, FacadeError, FacadeSetextChar};

/// Maximum raw physical-line prefix retained for finite-prefix donor rules.
///
/// The longest pinned Comrak HTML start family needs fewer than 16 bytes. The
/// larger bound leaves explicit upgrade headroom without making line length an
/// admission limit. Whole-line rules below retain scalar state, not this
/// prefix.
pub(crate) const SEGMENTED_LINE_PREFIX_BYTES: usize = 128;

// These are independent donor-rule results, not interchangeable configuration
// switches; named facts keep controller precedence explicit.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedLineFacts {
    pub(crate) blank: bool,
    pub(crate) had_ending: bool,
    pub(crate) has_bof_bom: bool,
    pub(crate) first_significant_byte: Option<u8>,
    pub(crate) first_nonspace: usize,
    pub(crate) indent: usize,
    pub(crate) block_quote: bool,
    pub(crate) block_quote_source: Option<SegmentedBlockQuoteFacts>,
    pub(crate) atx_heading: Option<SegmentedAtxHeadingFacts>,
    pub(crate) fence: SegmentedFenceFacts,
    pub(crate) html_block_1_to_6: Option<u8>,
    pub(crate) html_block_7: bool,
    pub(crate) setext: Option<SegmentedSetextHeadingFacts>,
    pub(crate) thematic_break: Option<SegmentedThematicBreakFacts>,
    pub(crate) indented_code: Option<SegmentedIndentedCodeLineFacts>,
    pub(crate) list_item: Option<SegmentedListItemFacts>,
    pub(crate) list: bool,
    pub(crate) interrupting_list: bool,
    pub(crate) table_delimiter_candidate: bool,
}

/// Exact source cuts and residual opener facts for one depth-1 block-quote
/// marker.
///
/// `hidden_prefix` covers the optional root indentation, `>`, and an optional
/// following space when that byte is consumed in full. A partially consumed
/// tab remains at `content.start`; `residual_tab_columns` records the exact
/// logical spaces contributed by the remainder of that source byte. The
/// controller therefore never has to rediscover quote-marker semantics from
/// retained text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedBlockQuoteFacts {
    pub(crate) hidden_prefix: SegmentedLineSpan,
    pub(crate) opening_marker: SegmentedLineSpan,
    pub(crate) content: SegmentedLineSpan,
    pub(crate) line_ending: SegmentedLineSpan,
    pub(crate) residual_tab_columns: u8,
    pub(crate) residual: SegmentedQuoteResidualFacts,
}

/// Narrow child-opener summary over the exact source remaining after one
/// depth-1 quote marker.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedQuoteResidualFacts {
    pub(crate) blank: bool,
    pub(crate) indent: usize,
    pub(crate) block_quote: bool,
    pub(crate) atx_heading: bool,
    pub(crate) fence: bool,
    pub(crate) html_block_1_to_6: bool,
    pub(crate) html_block_7: bool,
    pub(crate) setext: bool,
    pub(crate) thematic_break: bool,
    pub(crate) indented_code: bool,
    pub(crate) list: bool,
    pub(crate) interrupting_list: bool,
    pub(crate) table_delimiter_candidate: bool,
    pub(crate) potential_reference_definition: bool,
}

/// Exact byte range within one physical source line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedLineSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Source-backed ATX facts produced by the same forward line scan as opener
/// precedence. Gaps between these spans remain exact hidden source: indentation
/// and opener whitespace precede `content`, while trailing whitespace surrounds
/// an optional closing marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedAtxHeadingFacts {
    pub(crate) level: u8,
    pub(crate) opening_marker: SegmentedLineSpan,
    pub(crate) content: SegmentedLineSpan,
    pub(crate) closing_marker: Option<SegmentedLineSpan>,
    pub(crate) line_ending: SegmentedLineSpan,
}

/// Source-backed Setext underline facts from the shared forward line scan.
///
/// The gap from `underline_marker.end` to `line_ending.start` is the exact
/// trailing horizontal whitespace. The controller owns the decision that this
/// line promotes an already-open visible Paragraph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedSetextHeadingFacts {
    pub(crate) level: u8,
    pub(crate) underline_marker: SegmentedLineSpan,
    pub(crate) line_ending: SegmentedLineSpan,
}

/// Source-backed thematic-break facts from the shared forward line scan.
///
/// Markers may be separated by horizontal whitespace, so
/// [`marker_envelope`](Self::marker_envelope) deliberately spans from the
/// first marker through the last marker rather than pretending the markers
/// form one contiguous source range. [`marker_count`](Self::marker_count)
/// records the exact number of matching marker bytes within that envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedThematicBreakFacts {
    pub(crate) marker: u8,
    pub(crate) marker_count: usize,
    pub(crate) marker_envelope: SegmentedLineSpan,
    pub(crate) line_ending: SegmentedLineSpan,
}

/// Source-backed facts for one physical line as it would contribute to a
/// top-level indented-code block.
///
/// [`hidden_prefix`](Self::hidden_prefix) consumes exactly four columns on an
/// indented line (and includes a stripped BOF BOM when present). On a blank
/// line with fewer than four columns it consumes all horizontal whitespace.
/// The controller decides whether a blank line is internal to an already-open
/// code block or remains a separate `Blank` leaf.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedIndentedCodeLineFacts {
    pub(crate) hidden_prefix: SegmentedLineSpan,
    pub(crate) content: SegmentedLineSpan,
    pub(crate) line_ending: SegmentedLineSpan,
}

/// Parser-donor facts for one top-level list marker and its same-line child.
///
/// This is still a lexical result: the exact controller decides whether the
/// marker starts, continues, nests within, or fails closed from a list
/// container. `hidden_prefix` follows Comrak's item-padding decision and is
/// therefore source-authoritative; no downstream consumer needs to rediscover
/// marker or tab-stop semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentedListItemFacts {
    pub(crate) marker: SegmentedListMarker,
    pub(crate) hidden_prefix: SegmentedLineSpan,
    pub(crate) continuation_prefix: SegmentedLineSpan,
    pub(crate) opening_marker: SegmentedLineSpan,
    pub(crate) content: SegmentedLineSpan,
    pub(crate) line_ending: SegmentedLineSpan,
    pub(crate) opening_indent: usize,
    pub(crate) padding_columns: usize,
    pub(crate) tab_padded: bool,
    pub(crate) empty: bool,
    pub(crate) child: SegmentedListChildFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SegmentedListMarker {
    Bullet(u8),
    Ordered { start: usize, delimiter: u8 },
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SegmentedListChildFacts {
    pub(crate) task: bool,
    pub(crate) block_quote: bool,
    pub(crate) atx_heading: bool,
    pub(crate) fence: bool,
    pub(crate) html_block_1_to_6: bool,
    pub(crate) html_block_7: bool,
    pub(crate) setext: bool,
    pub(crate) thematic_break: bool,
    pub(crate) list: bool,
    pub(crate) table_delimiter_candidate: bool,
    pub(crate) potential_reference_definition: bool,
}

/// One forward-only physical-line lexical fold.
///
/// BOF BOM handling and first-nonspace columns correspond to Comrak's block
/// parser. Once the first significant byte is known, all suffix-sensitive
/// donor rules advance together with constant retained state. The controller
/// later consumes the facts in normative opener order.
pub(crate) struct SegmentedLineScanner {
    bom_pending: [u8; 3],
    bom_pending_len: usize,
    bom_resolved: bool,
    has_bof_bom: bool,
    raw_len: usize,
    blank: bool,
    first_nonspace: Option<usize>,
    indent: usize,
    indented_code_prefix_end: Option<usize>,
    ended: bool,
    line_ending_start: Option<usize>,
    significant: Option<SignificantLineScanner>,
    scan_block_quote_source: bool,
    block_quote_source: Option<SegmentedBlockQuoteScanner>,
}

impl SegmentedLineScanner {
    pub(crate) fn new(strip_bom: bool) -> Self {
        Self::new_with_block_quote_source(strip_bom, true)
    }

    fn new_with_block_quote_source(strip_bom: bool, scan_block_quote_source: bool) -> Self {
        Self {
            bom_pending: [0; 3],
            bom_pending_len: 0,
            bom_resolved: !strip_bom,
            has_bof_bom: false,
            raw_len: 0,
            blank: true,
            first_nonspace: None,
            indent: 0,
            indented_code_prefix_end: None,
            ended: false,
            line_ending_start: None,
            significant: None,
            scan_block_quote_source,
            block_quote_source: None,
        }
    }

    /// Feeds one unique physical source byte.
    pub(crate) fn push(&mut self, byte: u8) {
        let index = self.raw_len;
        self.raw_len += 1;
        if self.ended {
            // The only valid post-ending byte is LF after CR. Source facts are
            // cross-checked independently at commit.
            if byte == b'\n' {
                if let Some(scanner) = &mut self.block_quote_source {
                    scanner.push(index, byte);
                }
            }
            return;
        }
        if !self.bom_resolved {
            self.bom_pending[self.bom_pending_len] = byte;
            self.bom_pending_len += 1;
            if self.bom_pending_len == self.bom_pending.len() {
                self.resolve_bom();
            }
            return;
        }
        self.consume_content_byte(index, byte);
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn retained_source_bytes(&self) -> usize {
        self.bom_pending_len
            + self
                .significant
                .as_ref()
                .map_or(0, SignificantLineScanner::retained_source_bytes)
            + self
                .block_quote_source
                .as_ref()
                .map_or(0, SegmentedBlockQuoteScanner::retained_source_bytes)
    }

    pub(crate) fn finish(mut self) -> Result<SegmentedLineFacts, FacadeError> {
        if !self.bom_resolved {
            self.resolve_bom();
        }
        let first_nonspace = self.first_nonspace.unwrap_or(self.raw_len);
        let line_ending_start = self.line_ending_start.unwrap_or(self.raw_len);
        let significant = match self.significant {
            Some(scanner) => {
                scanner.finish(self.ended, first_nonspace, line_ending_start, self.raw_len)?
            }
            None => SignificantLineFacts::default(),
        };
        let block_quote_source = self
            .block_quote_source
            .map(|scanner| scanner.finish(line_ending_start, self.raw_len))
            .transpose()?;
        let indented_code_prefix_end = self
            .indented_code_prefix_end
            .or_else(|| self.blank.then_some(line_ending_start));
        let indented_code =
            indented_code_prefix_end.map(|prefix_end| SegmentedIndentedCodeLineFacts {
                hidden_prefix: SegmentedLineSpan {
                    start: 0,
                    end: prefix_end,
                },
                content: SegmentedLineSpan {
                    start: prefix_end,
                    end: line_ending_start,
                },
                line_ending: SegmentedLineSpan {
                    start: line_ending_start,
                    end: self.raw_len,
                },
            });
        Ok(SegmentedLineFacts {
            blank: self.blank,
            had_ending: self.ended,
            has_bof_bom: self.has_bof_bom,
            first_significant_byte: significant.first_significant_byte,
            first_nonspace,
            indent: self.indent,
            block_quote: significant.block_quote,
            block_quote_source,
            atx_heading: significant.atx_heading,
            fence: significant.fence,
            html_block_1_to_6: significant.html_block_1_to_6,
            html_block_7: significant.html_block_7,
            setext: significant.setext,
            thematic_break: significant.thematic_break,
            indented_code,
            list_item: significant.list_item,
            list: significant.list,
            interrupting_list: significant.interrupting_list,
            table_delimiter_candidate: significant.table_delimiter_candidate,
        })
    }

    fn resolve_bom(&mut self) {
        self.bom_resolved = true;
        let pending = self.bom_pending_len;
        let is_bom = pending == 3 && self.bom_pending == [0xef, 0xbb, 0xbf];
        self.has_bof_bom = is_bom;
        if !is_bom {
            for offset in 0..pending {
                self.consume_content_byte(offset, self.bom_pending[offset]);
            }
        }
        self.bom_pending_len = 0;
    }

    fn consume_content_byte(&mut self, index: usize, byte: u8) {
        if let Some(scanner) = &mut self.block_quote_source {
            scanner.push(index, byte);
        }
        if matches!(byte, b'\r' | b'\n') {
            self.ended = true;
            self.line_ending_start = Some(index);
            return;
        }
        self.blank &= matches!(byte, b' ' | b'\t');
        if let Some(scanner) = &mut self.significant {
            scanner.push(byte);
            return;
        }
        match byte {
            b' ' => {
                self.indent += 1;
                if self.indent == 4 {
                    self.indented_code_prefix_end = Some(index + 1);
                }
            }
            b'\t' => {
                self.indent += 4 - (self.indent % 4);
                if self.indent >= 4 && self.indented_code_prefix_end.is_none() {
                    self.indented_code_prefix_end = Some(index + 1);
                }
            }
            _ => {
                self.first_nonspace = Some(index);
                if self.scan_block_quote_source && byte == b'>' && self.indent <= 3 {
                    self.block_quote_source = Some(SegmentedBlockQuoteScanner::new(
                        index,
                        self.indent,
                        self.has_bof_bom,
                    ));
                }
                let mut scanner = SignificantLineScanner::new(self.indent);
                scanner.push(byte);
                self.significant = Some(scanner);
            }
        }
    }
}

struct SegmentedBlockQuoteScanner {
    hidden_start: usize,
    marker_start: usize,
    opening_indent: usize,
    prefix_resolved: bool,
    content_start: usize,
    residual_tab_columns: u8,
    residual: QuoteResidualScanner,
}

impl SegmentedBlockQuoteScanner {
    fn new(marker_start: usize, opening_indent: usize, has_bof_bom: bool) -> Self {
        let marker_end_column = opening_indent + 1;
        Self {
            hidden_start: if has_bof_bom {
                0
            } else {
                marker_start - opening_indent
            },
            marker_start,
            opening_indent,
            prefix_resolved: false,
            content_start: marker_start + 1,
            residual_tab_columns: 0,
            residual: QuoteResidualScanner::new(marker_end_column),
        }
    }

    fn push(&mut self, index: usize, byte: u8) {
        if matches!(byte, b'\r' | b'\n') {
            return;
        }
        if !self.prefix_resolved {
            self.prefix_resolved = true;
            match byte {
                b' ' => {
                    self.residual.set_container_column(self.opening_indent + 2);
                    self.content_start = index + 1;
                    return;
                }
                b'\t' => {
                    let marker_end_column = self.opening_indent + 1;
                    let columns_to_tab = 4 - (marker_end_column % 4);
                    self.residual.set_container_column(marker_end_column + 1);
                    if columns_to_tab == 1 {
                        self.content_start = index + 1;
                    } else {
                        self.content_start = index;
                        self.residual_tab_columns =
                            u8::try_from(columns_to_tab - 1).expect("tab stop is at most four");
                        self.residual
                            .push_virtual_spaces(columns_to_tab.saturating_sub(1));
                    }
                    return;
                }
                _ => {}
            }
        }
        self.residual.push(byte);
    }

    fn finish(
        mut self,
        line_ending_start: usize,
        line_end: usize,
    ) -> Result<SegmentedBlockQuoteFacts, FacadeError> {
        if !self.prefix_resolved {
            self.prefix_resolved = true;
        }
        let marker_end = self.marker_start + 1;
        debug_assert!(self.hidden_start <= self.marker_start);
        debug_assert!(marker_end <= self.content_start);
        debug_assert!(self.content_start <= line_ending_start);
        debug_assert!(line_ending_start <= line_end);
        let had_ending = line_ending_start != line_end;
        Ok(SegmentedBlockQuoteFacts {
            hidden_prefix: SegmentedLineSpan {
                start: self.hidden_start,
                end: self.content_start,
            },
            opening_marker: SegmentedLineSpan {
                start: self.marker_start,
                end: marker_end,
            },
            content: SegmentedLineSpan {
                start: self.content_start,
                end: line_ending_start,
            },
            line_ending: SegmentedLineSpan {
                start: line_ending_start,
                end: line_end,
            },
            residual_tab_columns: self.residual_tab_columns,
            residual: self.residual.finish(had_ending)?,
        })
    }

    #[cfg(test)]
    fn retained_source_bytes(&self) -> usize {
        self.residual.retained_source_bytes()
    }
}

struct QuoteResidualScanner {
    blank: bool,
    indent: usize,
    column: usize,
    significant_len: usize,
    significant: Option<SignificantLineScanner>,
}

impl QuoteResidualScanner {
    fn new(container_column: usize) -> Self {
        Self {
            blank: true,
            indent: 0,
            column: container_column,
            significant_len: 0,
            significant: None,
        }
    }

    fn set_container_column(&mut self, container_column: usize) {
        debug_assert_eq!(self.indent, 0);
        debug_assert!(self.significant.is_none());
        self.column = container_column;
    }

    fn push_virtual_spaces(&mut self, count: usize) {
        if self.significant.is_none() {
            self.indent += count;
            self.column += count;
        }
    }

    fn push(&mut self, byte: u8) {
        if let Some(significant) = &mut self.significant {
            significant.push(byte);
            self.significant_len += 1;
            return;
        }
        match byte {
            b' ' => {
                self.indent += 1;
                self.column += 1;
            }
            b'\t' => {
                let columns_to_tab = 4 - (self.column % 4);
                self.indent += columns_to_tab;
                self.column += columns_to_tab;
            }
            _ => {
                self.blank = false;
                self.significant_len = 1;
                let mut significant = SignificantLineScanner::new(self.indent);
                significant.push(byte);
                self.significant = Some(significant);
            }
        }
    }

    fn finish(self, had_ending: bool) -> Result<SegmentedQuoteResidualFacts, FacadeError> {
        let significant = match self.significant {
            Some(scanner) => {
                scanner.finish(had_ending, 0, self.significant_len, self.significant_len)?
            }
            None => SignificantLineFacts::default(),
        };
        Ok(SegmentedQuoteResidualFacts {
            blank: self.blank,
            indent: self.indent,
            block_quote: significant.block_quote,
            atx_heading: significant.atx_heading.is_some(),
            fence: significant.fence.opener_valid,
            html_block_1_to_6: significant.html_block_1_to_6.is_some(),
            html_block_7: significant.html_block_7,
            setext: significant.setext.is_some(),
            thematic_break: significant.thematic_break.is_some(),
            indented_code: !self.blank && self.indent >= 4,
            list: significant.list,
            interrupting_list: significant.interrupting_list,
            table_delimiter_candidate: significant.table_delimiter_candidate,
            potential_reference_definition: significant.first_significant_byte == Some(b'['),
        })
    }

    #[cfg(test)]
    fn retained_source_bytes(&self) -> usize {
        self.significant
            .as_ref()
            .map_or(0, SignificantLineScanner::retained_source_bytes)
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SignificantLineFacts {
    first_significant_byte: Option<u8>,
    block_quote: bool,
    atx_heading: Option<SegmentedAtxHeadingFacts>,
    fence: SegmentedFenceFacts,
    html_block_1_to_6: Option<u8>,
    html_block_7: bool,
    setext: Option<SegmentedSetextHeadingFacts>,
    thematic_break: Option<SegmentedThematicBreakFacts>,
    list_item: Option<SegmentedListItemFacts>,
    list: bool,
    interrupting_list: bool,
    table_delimiter_candidate: bool,
}

struct SignificantLineScanner {
    prefix: [u8; SEGMENTED_LINE_PREFIX_BYTES],
    prefix_len: usize,
    block_quote: bool,
    first_significant_byte: Option<u8>,
    atx: AtxFold,
    fence: FenceFold,
    marker: MarkerFold,
    html_7: HtmlType7Fold,
    list: ListFold,
    table: TableDelimiterFold,
}

impl SignificantLineScanner {
    fn new(opening_indent: usize) -> Self {
        Self {
            prefix: [0; SEGMENTED_LINE_PREFIX_BYTES],
            prefix_len: 0,
            block_quote: false,
            first_significant_byte: None,
            atx: AtxFold::default(),
            fence: FenceFold::default(),
            marker: MarkerFold::default(),
            html_7: HtmlType7Fold::default(),
            list: ListFold::new(opening_indent),
            table: TableDelimiterFold::default(),
        }
    }

    fn push(&mut self, byte: u8) {
        if self.first_significant_byte.is_none() {
            self.first_significant_byte = Some(byte);
            if byte == b'>' {
                self.block_quote = true;
                return;
            }
        } else if self.block_quote {
            // Root block-quote precedence makes every other root donor
            // irrelevant. The residual scanner owns the bounded child facts,
            // so retaining this suffix would duplicate the line prefix.
            return;
        }
        if self.prefix_len < self.prefix.len() {
            self.prefix[self.prefix_len] = byte;
            self.prefix_len += 1;
        }
        self.atx.push(byte);
        self.fence.push(byte);
        self.marker.push(byte);
        self.html_7.push(byte);
        self.list.push(byte);
        self.table.push(byte);
    }

    #[cfg(test)]
    const fn retained_source_bytes(&self) -> usize {
        self.prefix_len
    }

    fn finish(
        self,
        had_ending: bool,
        first_nonspace: usize,
        line_ending_start: usize,
        line_end: usize,
    ) -> Result<SignificantLineFacts, FacadeError> {
        // Type 1-6 HTML starts are a finite-prefix donor rule. Trim only an
        // incomplete final UTF-8 scalar; a valid source cannot contain an
        // earlier invalid byte. Every matching token lies before that cut.
        let prefix = &self.prefix[..self.prefix_len];
        let valid = std::str::from_utf8(prefix).map_or_else(
            |error| &prefix[..error.valid_up_to()],
            |text| text.as_bytes(),
        );
        let mut donor_prefix = [0_u8; SEGMENTED_LINE_PREFIX_BYTES + 1];
        donor_prefix[..valid.len()].copy_from_slice(valid);
        let donor_len = valid.len() + usize::from(had_ending);
        if had_ending {
            donor_prefix[valid.len()] = b'\n';
        }
        let html_block_1_to_6 = block_spine_facade::html_block_start(
            // SAFETY is not needed: `valid` was either the successful UTF-8
            // slice or the valid prefix reported by `from_utf8`.
            std::str::from_utf8(&donor_prefix[..donor_len]).expect("validated prefix"),
            false,
        )?;
        let list_item =
            self.list
                .finish(had_ending, first_nonspace, line_ending_start, line_end)?;
        let list = list_item.is_some();
        let interrupting_list = list_item.is_some_and(|facts| match facts.marker {
            SegmentedListMarker::Bullet(_) => !facts.empty,
            SegmentedListMarker::Ordered { start, .. } => start == 1 && !facts.empty,
        });
        Ok(SignificantLineFacts {
            first_significant_byte: self.first_significant_byte,
            block_quote: self.block_quote,
            atx_heading: self
                .atx
                .finish()
                .map(|facts| facts.into_physical(first_nonspace, line_ending_start, line_end)),
            fence: self.fence.finish(),
            html_block_1_to_6,
            html_block_7: self.html_7.finish(),
            setext: self.marker.setext().map(|(marker, marker_count)| {
                SegmentedSetextHeadingFacts {
                    level: match marker {
                        FacadeSetextChar::Equals => 1,
                        FacadeSetextChar::Hyphen => 2,
                    },
                    underline_marker: SegmentedLineSpan {
                        start: first_nonspace,
                        end: first_nonspace + marker_count,
                    },
                    line_ending: SegmentedLineSpan {
                        start: line_ending_start,
                        end: line_end,
                    },
                }
            }),
            thematic_break: self.marker.thematic_break().map(
                |(marker, marker_count, marker_envelope_end)| SegmentedThematicBreakFacts {
                    marker,
                    marker_count,
                    marker_envelope: SegmentedLineSpan {
                        start: first_nonspace,
                        end: first_nonspace + marker_envelope_end,
                    },
                    line_ending: SegmentedLineSpan {
                        start: line_ending_start,
                        end: line_end,
                    },
                },
            ),
            list_item,
            list,
            interrupting_list,
            table_delimiter_candidate: self.table.finish(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RelativeAtxHeadingFacts {
    level: u8,
    opening_marker: SegmentedLineSpan,
    content: SegmentedLineSpan,
    closing_marker: Option<SegmentedLineSpan>,
}

impl RelativeAtxHeadingFacts {
    fn into_physical(
        self,
        first_nonspace: usize,
        line_ending_start: usize,
        line_end: usize,
    ) -> SegmentedAtxHeadingFacts {
        let shift = |span: SegmentedLineSpan| SegmentedLineSpan {
            start: first_nonspace + span.start,
            end: first_nonspace + span.end,
        };
        SegmentedAtxHeadingFacts {
            level: self.level,
            opening_marker: shift(self.opening_marker),
            content: shift(self.content),
            closing_marker: self.closing_marker.map(shift),
            line_ending: SegmentedLineSpan {
                start: line_ending_start,
                end: line_end,
            },
        }
    }
}

#[derive(Default)]
enum AtxPhase {
    #[default]
    Hashes,
    Separator,
    Body,
    Rejected,
}

#[derive(Default)]
struct AtxFold {
    position: usize,
    hashes: u8,
    opener_end: usize,
    phase: AtxPhase,
    tail: AtxTailFold,
}

impl AtxFold {
    fn push(&mut self, byte: u8) {
        let offset = self.position;
        self.position += 1;
        self.tail.push(offset, byte);
        self.phase = match self.phase {
            AtxPhase::Hashes if byte == b'#' && self.hashes < 6 => {
                self.hashes += 1;
                AtxPhase::Hashes
            }
            AtxPhase::Hashes if byte == b'#' => {
                self.hashes += 1;
                AtxPhase::Rejected
            }
            AtxPhase::Hashes if self.hashes > 0 && matches!(byte, b' ' | b'\t') => {
                self.opener_end = self.position;
                AtxPhase::Separator
            }
            AtxPhase::Hashes => AtxPhase::Rejected,
            AtxPhase::Separator if matches!(byte, b' ' | b'\t') => {
                self.opener_end = self.position;
                AtxPhase::Separator
            }
            AtxPhase::Separator => AtxPhase::Body,
            AtxPhase::Body => AtxPhase::Body,
            AtxPhase::Rejected => AtxPhase::Rejected,
        };
    }

    fn finish(self) -> Option<RelativeAtxHeadingFacts> {
        if matches!(self.phase, AtxPhase::Rejected) || !(1..=6).contains(&self.hashes) {
            return None;
        }
        let marker_end = usize::from(self.hashes);
        let content_start = match self.phase {
            AtxPhase::Hashes => marker_end,
            AtxPhase::Separator | AtxPhase::Body => self.opener_end,
            AtxPhase::Rejected => unreachable!("rejected above"),
        };
        let (chopped_end, closing_marker) = self.tail.finish();
        Some(RelativeAtxHeadingFacts {
            level: self.hashes,
            opening_marker: SegmentedLineSpan {
                start: 0,
                end: marker_end,
            },
            content: SegmentedLineSpan {
                start: content_start,
                end: chopped_end.max(content_start),
            },
            closing_marker,
        })
    }
}

/// Constant-state forward correspondent of Comrak's trailing-ATX-marker chop.
///
/// Comrak performs this operation backwards after retaining the line. Tracking
/// only the final non-trimmed hash run gives the same cuts without retaining or
/// revisiting source.
#[derive(Default)]
struct AtxTailFold {
    last_byte: Option<u8>,
    last_nontrim_end: usize,
    hash_run_start: Option<usize>,
    before_hash_nontrim_end: usize,
    previous_was_hash: bool,
    hash_run_preceded_by_separator: bool,
}

impl AtxTailFold {
    fn push(&mut self, offset: usize, byte: u8) {
        let preceding_byte = self.last_byte;
        if matches!(byte, b'\t' | b'\r' | b'\n' | b' ') {
            self.previous_was_hash = false;
        } else {
            if byte == b'#' {
                if !self.previous_was_hash {
                    self.hash_run_start = Some(offset);
                    self.before_hash_nontrim_end = self.last_nontrim_end;
                    self.hash_run_preceded_by_separator =
                        preceding_byte.is_some_and(|byte| matches!(byte, b'\t' | b' '));
                }
                self.previous_was_hash = true;
            } else {
                self.hash_run_start = None;
                self.previous_was_hash = false;
                self.hash_run_preceded_by_separator = false;
            }
            self.last_nontrim_end = offset + 1;
        }
        self.last_byte = Some(byte);
    }

    fn finish(self) -> (usize, Option<SegmentedLineSpan>) {
        let closing_marker = self
            .hash_run_start
            .filter(|start| *start > 0 && self.hash_run_preceded_by_separator)
            .map(|start| SegmentedLineSpan {
                start,
                end: self.last_nontrim_end,
            });
        let chopped_end =
            closing_marker.map_or(self.last_nontrim_end, |_| self.before_hash_nontrim_end);
        (chopped_end, closing_marker)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SegmentedFenceFacts {
    pub(crate) marker: Option<u8>,
    pub(crate) opening_run_length: usize,
    pub(crate) opener_valid: bool,
    pub(crate) tail_horizontal_whitespace_only: bool,
}

#[derive(Default)]
enum FencePhase {
    #[default]
    Start,
    MarkerRun,
    Tail,
    Rejected,
}

#[derive(Default)]
struct FenceFold {
    marker: u8,
    run: usize,
    tail_horizontal_whitespace_only: bool,
    backtick_in_tail: bool,
    phase: FencePhase,
}

impl FenceFold {
    fn push(&mut self, byte: u8) {
        self.phase = match self.phase {
            FencePhase::Start if matches!(byte, b'`' | b'~') => {
                self.marker = byte;
                self.run = 1;
                self.tail_horizontal_whitespace_only = true;
                FencePhase::MarkerRun
            }
            FencePhase::Start | FencePhase::Rejected => FencePhase::Rejected,
            FencePhase::MarkerRun if byte == self.marker => {
                self.run += 1;
                FencePhase::MarkerRun
            }
            FencePhase::MarkerRun | FencePhase::Tail => {
                self.tail_horizontal_whitespace_only &= matches!(byte, b' ' | b'\t');
                self.backtick_in_tail |= self.marker == b'`' && byte == b'`';
                FencePhase::Tail
            }
        };
    }

    fn finish(self) -> SegmentedFenceFacts {
        let marker =
            matches!(self.phase, FencePhase::MarkerRun | FencePhase::Tail).then_some(self.marker);
        SegmentedFenceFacts {
            marker,
            opening_run_length: marker.map_or(0, |_| self.run),
            opener_valid: marker.is_some() && self.run >= 3 && !self.backtick_in_tail,
            tail_horizontal_whitespace_only: marker.is_some()
                && self.tail_horizontal_whitespace_only,
        }
    }
}

#[derive(Default)]
struct MarkerFold {
    first: Option<u8>,
    marker_count: usize,
    position: usize,
    last_thematic_marker_end: usize,
    setext_valid: bool,
    setext_tail: bool,
    thematic_valid: bool,
}

impl MarkerFold {
    fn push(&mut self, byte: u8) {
        let position = self.position;
        self.position += 1;
        let Some(marker) = self.first else {
            self.first = Some(byte);
            self.marker_count = 1;
            self.setext_valid = matches!(byte, b'=' | b'-');
            self.thematic_valid = matches!(byte, b'*' | b'_' | b'-');
            self.last_thematic_marker_end = usize::from(self.thematic_valid);
            return;
        };
        if byte == marker {
            self.marker_count += 1;
            self.last_thematic_marker_end = position + 1;
            if self.setext_tail {
                self.setext_valid = false;
            }
        } else if matches!(byte, b' ' | b'\t') {
            self.setext_tail = true;
        } else {
            self.setext_valid = false;
            self.thematic_valid = false;
        }
        if byte != marker && !matches!(byte, b' ' | b'\t') {
            self.thematic_valid = false;
        }
    }

    fn setext(&self) -> Option<(FacadeSetextChar, usize)> {
        if !self.setext_valid || self.marker_count == 0 {
            return None;
        }
        match self.first {
            Some(b'=') => Some((FacadeSetextChar::Equals, self.marker_count)),
            Some(b'-') => Some((FacadeSetextChar::Hyphen, self.marker_count)),
            _ => None,
        }
    }

    fn thematic_break(&self) -> Option<(u8, usize, usize)> {
        if self.thematic_valid && self.marker_count >= 3 {
            Some((
                self.first.expect("valid thematic fold has a marker"),
                self.marker_count,
                self.last_thematic_marker_end,
            ))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Default)]
enum HtmlType7Phase {
    #[default]
    LessThan,
    AfterLessThan,
    OpenTagName,
    CloseTagNameStart,
    CloseTagName,
    OpenSpace,
    CloseSpace,
    AttributeName,
    AfterAttributeSpace,
    BeforeValue,
    UnquotedValue,
    SingleQuotedValue,
    DoubleQuotedValue,
    AfterValue,
    ExpectGreaterThan,
    Trailing,
    Rejected,
}

#[derive(Default)]
struct HtmlType7Fold {
    phase: HtmlType7Phase,
}

impl HtmlType7Fold {
    // The one-to-one phase/guard spelling is intentionally kept aligned with
    // the pinned donor regex, even where several transitions share a target.
    #[allow(clippy::match_same_arms)]
    fn push(&mut self, byte: u8) {
        use HtmlType7Phase as Phase;
        self.phase = match self.phase {
            Phase::LessThan if byte == b'<' => Phase::AfterLessThan,
            Phase::AfterLessThan if byte == b'/' => Phase::CloseTagNameStart,
            Phase::AfterLessThan if byte.is_ascii_alphabetic() => Phase::OpenTagName,
            Phase::OpenTagName if is_tag_name_continue(byte) => Phase::OpenTagName,
            Phase::OpenTagName => after_open_tag_name(byte),
            Phase::CloseTagNameStart if byte.is_ascii_alphabetic() => Phase::CloseTagName,
            Phase::CloseTagName if is_tag_name_continue(byte) => Phase::CloseTagName,
            Phase::CloseTagName if is_html_space(byte) => Phase::CloseSpace,
            Phase::CloseTagName if byte == b'>' => Phase::Trailing,
            Phase::OpenSpace if is_html_space(byte) => Phase::OpenSpace,
            Phase::OpenSpace if byte == b'>' => Phase::Trailing,
            Phase::OpenSpace if byte == b'/' => Phase::ExpectGreaterThan,
            Phase::OpenSpace if is_attribute_name_start(byte) => Phase::AttributeName,
            Phase::CloseSpace if is_html_space(byte) => Phase::CloseSpace,
            Phase::CloseSpace if byte == b'>' => Phase::Trailing,
            Phase::AttributeName if is_attribute_name_continue(byte) => Phase::AttributeName,
            Phase::AttributeName if byte == b'=' => Phase::BeforeValue,
            Phase::AttributeName if is_html_space(byte) => Phase::AfterAttributeSpace,
            Phase::AttributeName if byte == b'>' => Phase::Trailing,
            Phase::AttributeName if byte == b'/' => Phase::ExpectGreaterThan,
            Phase::AfterAttributeSpace if is_html_space(byte) => Phase::AfterAttributeSpace,
            Phase::AfterAttributeSpace if byte == b'=' => Phase::BeforeValue,
            Phase::AfterAttributeSpace if byte == b'>' => Phase::Trailing,
            Phase::AfterAttributeSpace if byte == b'/' => Phase::ExpectGreaterThan,
            Phase::AfterAttributeSpace if is_attribute_name_start(byte) => Phase::AttributeName,
            Phase::BeforeValue if is_html_space(byte) => Phase::BeforeValue,
            Phase::BeforeValue if byte == b'\'' => Phase::SingleQuotedValue,
            Phase::BeforeValue if byte == b'"' => Phase::DoubleQuotedValue,
            Phase::BeforeValue if is_unquoted_value(byte) => Phase::UnquotedValue,
            Phase::UnquotedValue if is_unquoted_value(byte) => Phase::UnquotedValue,
            Phase::UnquotedValue if is_html_space(byte) => Phase::OpenSpace,
            Phase::UnquotedValue if byte == b'>' => Phase::Trailing,
            Phase::SingleQuotedValue if byte == b'\'' => Phase::AfterValue,
            Phase::SingleQuotedValue => Phase::SingleQuotedValue,
            Phase::DoubleQuotedValue if byte == b'"' => Phase::AfterValue,
            Phase::DoubleQuotedValue => Phase::DoubleQuotedValue,
            Phase::AfterValue if is_html_space(byte) => Phase::OpenSpace,
            Phase::AfterValue if byte == b'>' => Phase::Trailing,
            Phase::AfterValue if byte == b'/' => Phase::ExpectGreaterThan,
            Phase::ExpectGreaterThan if byte == b'>' => Phase::Trailing,
            Phase::Trailing if is_type_7_trailing_space(byte) => Phase::Trailing,
            _ => Phase::Rejected,
        };
    }

    fn finish(self) -> bool {
        matches!(self.phase, HtmlType7Phase::Trailing)
    }
}

fn after_open_tag_name(byte: u8) -> HtmlType7Phase {
    if is_html_space(byte) {
        HtmlType7Phase::OpenSpace
    } else if byte == b'>' {
        HtmlType7Phase::Trailing
    } else if byte == b'/' {
        HtmlType7Phase::ExpectGreaterThan
    } else {
        HtmlType7Phase::Rejected
    }
}

const fn is_tag_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

const fn is_attribute_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')
}

const fn is_attribute_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'_' | b'-')
}

const fn is_html_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

const fn is_type_7_trailing_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0c)
}

const fn is_unquoted_value(byte: u8) -> bool {
    !is_html_space(byte) && !matches!(byte, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
}

#[derive(Default)]
enum ListMarkerPhase {
    #[default]
    Start,
    BulletNeedsSeparator {
        marker: u8,
    },
    OrderedDigits {
        value: usize,
        digits: usize,
    },
    OrderedNeedsSeparator {
        value: usize,
        delimiter: u8,
    },
    Padding,
    Content,
    Rejected,
}

/// One streaming counterpart of Comrak's `parse_list_marker` plus the
/// padding transition in `handle_list`.
///
/// It does not scan the line again. `push` reports the first content byte so
/// the parent fold can advance the child-opener folds in the same pass.
struct ListMarkerFold {
    phase: ListMarkerPhase,
    opening_indent: usize,
    position: usize,
    marker: Option<SegmentedListMarker>,
    marker_end: usize,
    first_padding_end: Option<usize>,
    padding_columns: usize,
    padding_has_tab: bool,
    content_start: Option<usize>,
}

impl ListMarkerFold {
    fn new(opening_indent: usize) -> Self {
        Self {
            phase: ListMarkerPhase::Start,
            opening_indent,
            position: 0,
            marker: None,
            marker_end: 0,
            first_padding_end: None,
            padding_columns: 0,
            padding_has_tab: false,
            content_start: None,
        }
    }

    /// Returns `true` when `byte` belongs to the item child content.
    fn push(&mut self, byte: u8) -> bool {
        use ListMarkerPhase as Phase;
        let offset = self.position;
        self.position += 1;
        let phase = std::mem::take(&mut self.phase);
        self.phase = match phase {
            Phase::Start if matches!(byte, b'*' | b'-' | b'+') => {
                self.marker = Some(SegmentedListMarker::Bullet(byte));
                self.marker_end = self.position;
                Phase::BulletNeedsSeparator { marker: byte }
            }
            Phase::Start if byte.is_ascii_digit() => Phase::OrderedDigits {
                value: usize::from(byte - b'0'),
                digits: 1,
            },
            Phase::OrderedDigits { value, digits } if byte.is_ascii_digit() && digits < 9 => {
                Phase::OrderedDigits {
                    value: value * 10 + usize::from(byte - b'0'),
                    digits: digits + 1,
                }
            }
            Phase::OrderedDigits { value, .. } if matches!(byte, b'.' | b')') => {
                self.marker = Some(SegmentedListMarker::Ordered {
                    start: value,
                    delimiter: byte,
                });
                self.marker_end = self.position;
                Phase::OrderedNeedsSeparator {
                    value,
                    delimiter: byte,
                }
            }
            Phase::BulletNeedsSeparator { marker } if is_cmark_space(byte) => {
                debug_assert_eq!(self.marker, Some(SegmentedListMarker::Bullet(marker)));
                self.push_padding(offset, byte);
                Phase::Padding
            }
            Phase::OrderedNeedsSeparator { value, delimiter } if is_cmark_space(byte) => {
                debug_assert_eq!(
                    self.marker,
                    Some(SegmentedListMarker::Ordered {
                        start: value,
                        delimiter,
                    })
                );
                self.push_padding(offset, byte);
                Phase::Padding
            }
            Phase::Padding if matches!(byte, b' ' | b'\t') => {
                self.push_padding(offset, byte);
                Phase::Padding
            }
            Phase::Padding => {
                self.content_start = Some(offset);
                Phase::Content
            }
            Phase::Content => Phase::Content,
            _ => Phase::Rejected,
        };
        matches!(self.phase, Phase::Content)
    }

    fn push_padding(&mut self, offset: usize, byte: u8) {
        self.first_padding_end.get_or_insert(offset + 1);
        let marker_column = self.opening_indent + self.marker_end;
        let column = marker_column + self.padding_columns;
        match byte {
            b' ' => self.padding_columns += 1,
            b'\t' => {
                self.padding_has_tab = true;
                self.padding_columns += 4 - (column % 4);
            }
            _ => {}
        }
    }

    fn finish(self, had_ending: bool) -> Option<RelativeListMarkerFacts> {
        let empty = matches!(
            self.phase,
            ListMarkerPhase::BulletNeedsSeparator { .. }
                | ListMarkerPhase::OrderedNeedsSeparator { .. }
                | ListMarkerPhase::Padding
        );
        let valid = match self.phase {
            ListMarkerPhase::BulletNeedsSeparator { .. } => true,
            ListMarkerPhase::OrderedNeedsSeparator { .. } => had_ending,
            ListMarkerPhase::Padding | ListMarkerPhase::Content => true,
            ListMarkerPhase::Start
            | ListMarkerPhase::OrderedDigits { .. }
            | ListMarkerPhase::Rejected => false,
        };
        if !valid {
            return None;
        }
        let marker = self.marker?;
        let donor_content_start = if empty || !(1..=4).contains(&self.padding_columns) {
            self.first_padding_end.unwrap_or(self.marker_end)
        } else {
            self.content_start.unwrap_or(self.position)
        };
        Some(RelativeListMarkerFacts {
            marker,
            marker_span: SegmentedLineSpan {
                start: 0,
                end: self.marker_end,
            },
            donor_content_start,
            padding_columns: self.padding_columns,
            tab_padded: self.padding_has_tab,
            empty,
        })
    }
}

#[derive(Clone, Copy)]
struct RelativeListMarkerFacts {
    marker: SegmentedListMarker,
    marker_span: SegmentedLineSpan,
    donor_content_start: usize,
    padding_columns: usize,
    tab_padded: bool,
    empty: bool,
}

struct ListFold {
    marker: ListMarkerFold,
    child: ListChildFold,
}

impl ListFold {
    fn new(opening_indent: usize) -> Self {
        Self {
            marker: ListMarkerFold::new(opening_indent),
            child: ListChildFold::new(),
        }
    }

    fn push(&mut self, byte: u8) {
        if self.marker.push(byte) {
            self.child.push(byte);
        }
    }

    fn finish(
        self,
        had_ending: bool,
        first_nonspace: usize,
        line_ending_start: usize,
        line_end: usize,
    ) -> Result<Option<SegmentedListItemFacts>, FacadeError> {
        let opening_indent = self.marker.opening_indent;
        let Some(marker) = self.marker.finish(had_ending) else {
            return Ok(None);
        };
        let shift = |span: SegmentedLineSpan| SegmentedLineSpan {
            start: first_nonspace + span.start,
            end: first_nonspace + span.end,
        };
        let continuation_end = first_nonspace + marker.donor_content_start;
        let content_start = if marker.empty {
            line_ending_start
        } else {
            continuation_end
        };
        Ok(Some(SegmentedListItemFacts {
            marker: marker.marker,
            hidden_prefix: SegmentedLineSpan {
                start: 0,
                end: content_start,
            },
            continuation_prefix: SegmentedLineSpan {
                start: 0,
                end: continuation_end,
            },
            opening_marker: shift(marker.marker_span),
            content: SegmentedLineSpan {
                start: content_start,
                end: line_ending_start,
            },
            line_ending: SegmentedLineSpan {
                start: line_ending_start,
                end: line_end,
            },
            opening_indent,
            padding_columns: marker.padding_columns,
            tab_padded: marker.tab_padded,
            empty: marker.empty,
            child: if marker.empty {
                SegmentedListChildFacts::default()
            } else {
                self.child.finish(had_ending)?
            },
        }))
    }
}

struct ListChildFold {
    prefix: [u8; SEGMENTED_LINE_PREFIX_BYTES],
    prefix_len: usize,
    first: Option<u8>,
    block_quote: bool,
    atx: AtxFold,
    fence: FenceFold,
    marker: MarkerFold,
    html_7: HtmlType7Fold,
    list: ListMarkerFold,
    table: TableDelimiterFold,
    task: TaskPrefixFold,
}

impl ListChildFold {
    fn new() -> Self {
        Self {
            prefix: [0; SEGMENTED_LINE_PREFIX_BYTES],
            prefix_len: 0,
            first: None,
            block_quote: false,
            atx: AtxFold::default(),
            fence: FenceFold::default(),
            marker: MarkerFold::default(),
            html_7: HtmlType7Fold::default(),
            list: ListMarkerFold::new(0),
            table: TableDelimiterFold::default(),
            task: TaskPrefixFold::default(),
        }
    }

    fn push(&mut self, byte: u8) {
        if self.first.is_none() {
            self.first = Some(byte);
            self.block_quote = byte == b'>';
        }
        if self.prefix_len < self.prefix.len() {
            self.prefix[self.prefix_len] = byte;
            self.prefix_len += 1;
        }
        self.atx.push(byte);
        self.fence.push(byte);
        self.marker.push(byte);
        self.html_7.push(byte);
        self.list.push(byte);
        self.table.push(byte);
        self.task.push(byte);
    }

    fn finish(self, had_ending: bool) -> Result<SegmentedListChildFacts, FacadeError> {
        let prefix = &self.prefix[..self.prefix_len];
        let valid = std::str::from_utf8(prefix).map_or_else(
            |error| &prefix[..error.valid_up_to()],
            |text| text.as_bytes(),
        );
        let mut donor_prefix = [0_u8; SEGMENTED_LINE_PREFIX_BYTES + 1];
        donor_prefix[..valid.len()].copy_from_slice(valid);
        let donor_len = valid.len() + usize::from(had_ending);
        if had_ending {
            donor_prefix[valid.len()] = b'\n';
        }
        let html_block_1_to_6 = block_spine_facade::html_block_start(
            std::str::from_utf8(&donor_prefix[..donor_len]).expect("validated prefix"),
            false,
        )?
        .is_some();
        Ok(SegmentedListChildFacts {
            task: self.task.finish(had_ending),
            block_quote: self.block_quote,
            atx_heading: self.atx.finish().is_some(),
            fence: self.fence.finish().opener_valid,
            html_block_1_to_6,
            html_block_7: self.html_7.finish(),
            setext: self.marker.setext().is_some(),
            thematic_break: self.marker.thematic_break().is_some(),
            list: self.list.finish(had_ending).is_some(),
            table_delimiter_candidate: self.table.finish(),
            potential_reference_definition: self.first == Some(b'['),
        })
    }
}

#[derive(Default)]
struct TaskPrefixFold {
    prefix: [u8; 4],
    length: usize,
    overflow: bool,
}

impl TaskPrefixFold {
    fn push(&mut self, byte: u8) {
        if self.length < self.prefix.len() {
            self.prefix[self.length] = byte;
            self.length += 1;
        } else {
            self.overflow = true;
        }
    }

    fn finish(self, _had_ending: bool) -> bool {
        self.length >= 3
            && self.prefix[0] == b'['
            && matches!(self.prefix[1], b' ' | b'x' | b'X')
            && self.prefix[2] == b']'
            && (self.length == 3 && !self.overflow
                || self.length >= 4 && is_cmark_space(self.prefix[3]))
    }
}

#[derive(Default)]
enum TablePhase {
    #[default]
    Start,
    CellLeading {
        trailing_allowed: bool,
    },
    NeedHyphen,
    Hyphens,
    AfterRightColon,
    TrailingSpace,
    Rejected,
}

#[derive(Default)]
struct TableDelimiterFold {
    phase: TablePhase,
    cells: usize,
}

impl TableDelimiterFold {
    fn push(&mut self, byte: u8) {
        use TablePhase as Phase;
        let phase = std::mem::take(&mut self.phase);
        self.phase = match phase {
            Phase::Start if byte == b'|' => Phase::CellLeading {
                trailing_allowed: false,
            },
            Phase::Start => Self::start_cell(byte, false),
            Phase::CellLeading { trailing_allowed } if is_table_space(byte) => {
                Phase::CellLeading { trailing_allowed }
            }
            Phase::CellLeading { .. } if byte == b':' => Phase::NeedHyphen,
            Phase::CellLeading { .. } | Phase::NeedHyphen | Phase::Hyphens if byte == b'-' => {
                Phase::Hyphens
            }
            Phase::Hyphens if byte == b':' => Phase::AfterRightColon,
            Phase::Hyphens | Phase::AfterRightColon | Phase::TrailingSpace
                if is_table_space(byte) =>
            {
                Phase::TrailingSpace
            }
            Phase::Hyphens | Phase::AfterRightColon | Phase::TrailingSpace if byte == b'|' => {
                self.finish_cell()
            }
            _ => Phase::Rejected,
        };
    }

    fn start_cell(byte: u8, trailing_allowed: bool) -> TablePhase {
        if is_table_space(byte) {
            TablePhase::CellLeading { trailing_allowed }
        } else if byte == b':' {
            TablePhase::NeedHyphen
        } else if byte == b'-' {
            TablePhase::Hyphens
        } else {
            TablePhase::Rejected
        }
    }

    fn finish_cell(&mut self) -> TablePhase {
        self.cells += 1;
        TablePhase::CellLeading {
            trailing_allowed: true,
        }
    }

    fn finish(mut self) -> bool {
        match self.phase {
            TablePhase::Hyphens | TablePhase::AfterRightColon | TablePhase::TrailingSpace => {
                self.cells += 1;
                self.cells > 0
            }
            TablePhase::CellLeading {
                trailing_allowed: true,
            } => self.cells > 0,
            _ => false,
        }
    }
}

const fn is_table_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0b | 0x0c)
}

const fn is_cmark_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_line(input: &str) -> SegmentedLineFacts {
        scan_line_with_bom_policy(input, false)
    }

    fn scan_line_with_bom_policy(input: &str, strip_bom: bool) -> SegmentedLineFacts {
        let mut scanner = SegmentedLineScanner::new(strip_bom);
        for byte in input.bytes() {
            scanner.push(byte);
        }
        scanner.finish().expect("bounded donor prefix")
    }

    fn first_nonspace(input: &str) -> usize {
        let mut offset = 0;
        let mut column = 0;
        for byte in input.bytes() {
            match byte {
                b' ' => {
                    offset += 1;
                    column += 1;
                }
                b'\t' => {
                    offset += 1;
                    column += 4 - (column % 4);
                }
                _ => break,
            }
        }
        offset
    }

    fn physical_content_end(input: &str) -> usize {
        if input.ends_with("\r\n") {
            input.len() - 2
        } else if input
            .as_bytes()
            .last()
            .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
        {
            input.len() - 1
        } else {
            input.len()
        }
    }

    #[test]
    fn indented_code_facts_preserve_exact_deindent_and_line_geometry() {
        let cases = [
            (
                "    code\r\n",
                false,
                Some((
                    SegmentedLineSpan { start: 0, end: 4 },
                    SegmentedLineSpan { start: 4, end: 8 },
                    SegmentedLineSpan { start: 8, end: 10 },
                )),
            ),
            (
                "\tcode\n",
                false,
                Some((
                    SegmentedLineSpan { start: 0, end: 1 },
                    SegmentedLineSpan { start: 1, end: 5 },
                    SegmentedLineSpan { start: 5, end: 6 },
                )),
            ),
            (
                "      \n",
                false,
                Some((
                    SegmentedLineSpan { start: 0, end: 4 },
                    SegmentedLineSpan { start: 4, end: 6 },
                    SegmentedLineSpan { start: 6, end: 7 },
                )),
            ),
            (
                "  \n",
                false,
                Some((
                    SegmentedLineSpan { start: 0, end: 2 },
                    SegmentedLineSpan { start: 2, end: 2 },
                    SegmentedLineSpan { start: 2, end: 3 },
                )),
            ),
            (
                "\u{feff}\tα\r\n",
                true,
                Some((
                    SegmentedLineSpan { start: 0, end: 4 },
                    SegmentedLineSpan { start: 4, end: 6 },
                    SegmentedLineSpan { start: 6, end: 8 },
                )),
            ),
            ("   code", false, None),
        ];

        for (source, strip_bom, expected) in cases {
            let actual = scan_line_with_bom_policy(source, strip_bom)
                .indented_code
                .map(|facts| (facts.hidden_prefix, facts.content, facts.line_ending));
            assert_eq!(actual, expected, "{source:?}");
        }
    }

    #[test]
    fn block_quote_facts_preserve_prefix_tabs_residual_openers_and_endings() {
        let plain = scan_line("> alpha\r\n")
            .block_quote_source
            .expect("depth-one quote");
        assert_eq!(
            (
                plain.hidden_prefix,
                plain.opening_marker,
                plain.content,
                plain.line_ending,
                plain.residual_tab_columns,
            ),
            (
                SegmentedLineSpan { start: 0, end: 2 },
                SegmentedLineSpan { start: 0, end: 1 },
                SegmentedLineSpan { start: 2, end: 7 },
                SegmentedLineSpan { start: 7, end: 9 },
                0,
            ),
        );
        assert!(!plain.residual.blank);
        assert_eq!(plain.residual.indent, 0);

        let fully_consumed_tab = scan_line("  >\talpha\n")
            .block_quote_source
            .expect("quote tab at a tab stop");
        assert_eq!(
            fully_consumed_tab.hidden_prefix,
            SegmentedLineSpan { start: 0, end: 4 },
        );
        assert_eq!(
            fully_consumed_tab.content,
            SegmentedLineSpan { start: 4, end: 9 },
        );
        assert_eq!(fully_consumed_tab.residual_tab_columns, 0);

        let partial_tab = scan_line(">\talpha\n")
            .block_quote_source
            .expect("partially consumed quote tab");
        assert_eq!(
            partial_tab.hidden_prefix,
            SegmentedLineSpan { start: 0, end: 1 },
        );
        assert_eq!(partial_tab.content, SegmentedLineSpan { start: 1, end: 7 },);
        assert_eq!(partial_tab.residual_tab_columns, 2);
        assert_eq!(partial_tab.residual.indent, 2);

        let bom = scan_line_with_bom_policy("\u{feff}> q", true)
            .block_quote_source
            .expect("BOF quote");
        assert_eq!(bom.hidden_prefix, SegmentedLineSpan { start: 0, end: 5 },);
        assert_eq!(bom.opening_marker, SegmentedLineSpan { start: 3, end: 4 });
        assert_eq!(bom.content, SegmentedLineSpan { start: 5, end: 6 });

        let marker_only = scan_line(">\r\n")
            .block_quote_source
            .expect("marker-only quote");
        assert!(marker_only.residual.blank);
        assert_eq!(marker_only.content, SegmentedLineSpan { start: 1, end: 1 },);

        let nested = scan_line("> > nested\n")
            .block_quote_source
            .expect("outer quote");
        assert!(nested.residual.block_quote);
        let list = scan_line("> - item\n")
            .block_quote_source
            .expect("quoted list");
        assert!(list.residual.list);
        assert!(list.residual.interrupting_list);
        assert!(
            scan_line("> ``` dart\n")
                .block_quote_source
                .expect("quoted fence")
                .residual
                .fence,
        );
        assert!(
            scan_line("> # heading\n")
                .block_quote_source
                .expect("quoted ATX")
                .residual
                .atx_heading,
        );
        assert!(
            scan_line(">     code\n")
                .block_quote_source
                .expect("quoted indented code")
                .residual
                .indented_code,
        );
        assert!(
            scan_line("> [label]: /url\n")
                .block_quote_source
                .expect("quoted reference definition")
                .residual
                .potential_reference_definition,
        );
    }

    #[test]
    fn block_quote_residual_tabs_keep_container_relative_columns() {
        let cases = [
            ("> \tfoo\n", 2, false, 0),
            (">  \tfoo\n", 2, false, 0),
            (">   \tfoo\n", 6, true, 0),
            (" > \tfoo\n", 1, false, 0),
            ("  > \tfoo\n", 4, true, 0),
            ("   > \tfoo\n", 3, false, 0),
            (">\tfoo\n", 2, false, 2),
            (">\t \tfoo\n", 6, true, 2),
            (" >\tfoo\n", 1, false, 1),
            ("  >\tfoo\n", 0, false, 0),
            ("   >\tfoo\n", 3, false, 3),
        ];

        for (source, expected_indent, expected_indented_code, expected_residual_tab) in cases {
            let quote = scan_line(source)
                .block_quote_source
                .expect("depth-one quote");
            assert_eq!(quote.residual.indent, expected_indent, "{source:?}");
            assert_eq!(
                quote.residual.indented_code, expected_indented_code,
                "{source:?}",
            );
            assert_eq!(
                quote.residual_tab_columns, expected_residual_tab,
                "{source:?}",
            );
        }
    }

    #[test]
    fn list_item_facts_preserve_marker_padding_child_and_terminal_empty_cuts() {
        let plain = scan_line("- alpha\r\n").list_item.expect("bullet item");
        assert_eq!(plain.marker, SegmentedListMarker::Bullet(b'-'));
        assert_eq!(plain.hidden_prefix, SegmentedLineSpan { start: 0, end: 2 });
        assert_eq!(
            plain.continuation_prefix,
            SegmentedLineSpan { start: 0, end: 2 }
        );
        assert_eq!(plain.opening_marker, SegmentedLineSpan { start: 0, end: 1 });
        assert_eq!(plain.content, SegmentedLineSpan { start: 2, end: 7 });
        assert_eq!(plain.line_ending, SegmentedLineSpan { start: 7, end: 9 });
        assert_eq!(plain.opening_indent, 0);
        assert_eq!(plain.padding_columns, 1);
        assert!(!plain.tab_padded);
        assert!(!plain.empty);
        assert_eq!(plain.child, SegmentedListChildFacts::default());

        let unicode = scan_line("  +   β\n")
            .list_item
            .expect("indented bullet item");
        assert_eq!(unicode.marker, SegmentedListMarker::Bullet(b'+'));
        assert_eq!(
            unicode.hidden_prefix,
            SegmentedLineSpan { start: 0, end: 6 }
        );
        assert_eq!(
            unicode.opening_marker,
            SegmentedLineSpan { start: 2, end: 3 }
        );
        assert_eq!(unicode.content, SegmentedLineSpan { start: 6, end: 8 });
        assert_eq!(unicode.opening_indent, 2);
        assert_eq!(unicode.padding_columns, 3);

        let empty = scan_line("-   \n").list_item.expect("terminal empty item");
        assert!(empty.empty);
        assert_eq!(empty.hidden_prefix, SegmentedLineSpan { start: 0, end: 4 });
        assert_eq!(
            empty.continuation_prefix,
            SegmentedLineSpan { start: 0, end: 2 }
        );
        assert_eq!(empty.content, SegmentedLineSpan { start: 4, end: 4 });
        assert_eq!(empty.line_ending, SegmentedLineSpan { start: 4, end: 5 });

        let tab = scan_line("-\tfoo\n").list_item.expect("tab-padded item");
        assert!(tab.tab_padded);
        assert_eq!(tab.padding_columns, 3);

        let ordered = scan_line("12) value\n").list_item.expect("ordered item");
        assert_eq!(
            ordered.marker,
            SegmentedListMarker::Ordered {
                start: 12,
                delimiter: b')'
            }
        );
        assert_eq!(
            ordered.opening_marker,
            SegmentedLineSpan { start: 0, end: 3 }
        );

        assert!(
            scan_line("- [x] task\n")
                .list_item
                .expect("task item")
                .child
                .task
        );
        assert!(
            scan_line("- - nested\n")
                .list_item
                .expect("nested item")
                .child
                .list
        );
        assert!(
            scan_line("- # heading\n")
                .list_item
                .expect("heading child")
                .child
                .atx_heading
        );
    }

    fn donor_closing_marker(input: &str, closed: bool) -> Option<SegmentedLineSpan> {
        if !closed {
            return None;
        }
        let bytes = input.as_bytes();
        let mut end = bytes.len();
        while end > 0 && matches!(bytes[end - 1], b'\t' | b'\r' | b'\n' | b' ') {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && bytes[start - 1] == b'#' {
            start -= 1;
        }
        Some(SegmentedLineSpan { start, end })
    }

    fn compare_line(input: &str) {
        let facts = scan_line(input);
        let start = first_nonspace(input);
        let significant = &input[start..];
        assert_eq!(
            facts.block_quote,
            block_spine_facade::block_quote_start(significant).unwrap(),
            "block quote {input:?}"
        );
        let donor_atx = block_spine_facade::atx_heading_start(significant).unwrap();
        match (facts.atx_heading, donor_atx) {
            (None, None) => {}
            (Some(actual), Some(opener_end)) => {
                let level = significant.bytes().take_while(|byte| *byte == b'#').count();
                let content_end = physical_content_end(input);
                let significant_content_end = content_end - start;
                let (chopped, closed) =
                    block_spine_facade::chop_trailing_hashes(significant).unwrap();
                assert_eq!(
                    actual.level,
                    u8::try_from(level).unwrap(),
                    "ATX level {input:?}"
                );
                assert_eq!(
                    actual.opening_marker,
                    SegmentedLineSpan {
                        start,
                        end: start + level,
                    },
                    "ATX opening marker {input:?}"
                );
                let content_start = start + opener_end.min(significant_content_end);
                assert_eq!(
                    actual.content,
                    SegmentedLineSpan {
                        start: content_start,
                        end: start + chopped.len().max(opener_end.min(significant_content_end)),
                    },
                    "ATX content {input:?}"
                );
                assert_eq!(
                    actual.closing_marker,
                    donor_closing_marker(significant, closed).map(|span| SegmentedLineSpan {
                        start: start + span.start,
                        end: start + span.end,
                    }),
                    "ATX closing marker {input:?}"
                );
                assert_eq!(
                    actual.line_ending,
                    SegmentedLineSpan {
                        start: content_end,
                        end: input.len(),
                    },
                    "ATX line ending {input:?}"
                );
            }
            (actual, expected) => {
                panic!("ATX donor mismatch for {input:?}: {actual:?} != {expected:?}")
            }
        }
        let donor_open = block_spine_facade::open_code_fence(significant).unwrap();
        let donor_close = block_spine_facade::close_code_fence(significant).unwrap();
        assert_eq!(
            facts.fence.opener_valid,
            donor_open.is_some(),
            "fence opener {input:?}"
        );
        assert_eq!(
            facts.fence.marker,
            significant
                .as_bytes()
                .first()
                .copied()
                .filter(|byte| matches!(byte, b'`' | b'~')),
            "fence marker {input:?}"
        );
        if let Some(run) = donor_open {
            assert_eq!(
                facts.fence.opening_run_length, run,
                "fence opener run {input:?}"
            );
        }
        assert_eq!(
            facts.fence.marker.is_some()
                && facts.fence.opening_run_length >= 3
                && facts.fence.tail_horizontal_whitespace_only,
            donor_close.is_some(),
            "fence closer {input:?}"
        );
        if let Some(run) = donor_close {
            assert_eq!(
                facts.fence.opening_run_length, run,
                "fence closer run {input:?}"
            );
        }
        assert_eq!(
            facts.html_block_1_to_6,
            block_spine_facade::html_block_start(significant, false).unwrap(),
            "HTML 1-6 {input:?}"
        );
        assert_eq!(
            facts.html_block_1_to_6.or(facts.html_block_7.then_some(7)),
            block_spine_facade::html_block_start(significant, true).unwrap(),
            "HTML all {input:?}"
        );
        let donor_setext = block_spine_facade::setext_heading_line(significant).unwrap();
        assert_eq!(
            facts.setext.map(|setext| match setext.level {
                1 => FacadeSetextChar::Equals,
                2 => FacadeSetextChar::Hyphen,
                _ => unreachable!("Setext level is lexical"),
            }),
            donor_setext,
            "Setext {input:?}"
        );
        if let Some(setext) = facts.setext {
            let marker_count = significant
                .bytes()
                .take_while(|byte| {
                    *byte
                        == match setext.level {
                            1 => b'=',
                            2 => b'-',
                            _ => unreachable!("Setext level is lexical"),
                        }
                })
                .count();
            assert_eq!(
                setext.underline_marker,
                SegmentedLineSpan {
                    start,
                    end: start + marker_count,
                },
                "Setext marker {input:?}"
            );
            assert_eq!(
                setext.line_ending,
                SegmentedLineSpan {
                    start: physical_content_end(input),
                    end: input.len(),
                },
                "Setext line ending {input:?}"
            );
        }
        assert_eq!(
            facts.thematic_break.is_some(),
            block_spine_facade::thematic_break(significant).unwrap(),
            "thematic {input:?}"
        );
        if let Some(thematic) = facts.thematic_break {
            let content = &input.as_bytes()[start..physical_content_end(input)];
            let marker = content[0];
            let marker_count = content.iter().filter(|byte| **byte == marker).count();
            let last_marker_end = content
                .iter()
                .rposition(|byte| *byte == marker)
                .expect("thematic marker")
                + 1;
            assert_eq!(thematic.marker, marker, "thematic marker {input:?}");
            assert_eq!(
                thematic.marker_count, marker_count,
                "thematic marker count {input:?}"
            );
            assert_eq!(
                thematic.marker_envelope,
                SegmentedLineSpan {
                    start,
                    end: start + last_marker_end,
                },
                "thematic marker envelope {input:?}"
            );
            assert_eq!(
                thematic.line_ending,
                SegmentedLineSpan {
                    start: physical_content_end(input),
                    end: input.len(),
                },
                "thematic line ending {input:?}"
            );
        }
        assert_eq!(
            facts.list,
            block_spine_facade::list_marker_start(significant, false).unwrap(),
            "list {input:?}"
        );
        assert_eq!(
            facts.interrupting_list,
            block_spine_facade::list_marker_start(significant, true).unwrap(),
            "interrupting list {input:?}"
        );
        assert_eq!(
            facts.table_delimiter_candidate,
            block_spine_facade::table_delimiter_candidate(significant).unwrap(),
            "table {input:?}"
        );
    }

    #[test]
    fn atx_facts_preserve_commonmark_marker_content_close_and_eol_cuts() {
        let cases = [
            (
                "# foo\n",
                SegmentedAtxHeadingFacts {
                    level: 1,
                    opening_marker: SegmentedLineSpan { start: 0, end: 1 },
                    content: SegmentedLineSpan { start: 2, end: 5 },
                    closing_marker: None,
                    line_ending: SegmentedLineSpan { start: 5, end: 6 },
                },
            ),
            (
                "  ###  foo ###  \r\n",
                SegmentedAtxHeadingFacts {
                    level: 3,
                    opening_marker: SegmentedLineSpan { start: 2, end: 5 },
                    content: SegmentedLineSpan { start: 7, end: 10 },
                    closing_marker: Some(SegmentedLineSpan { start: 11, end: 14 }),
                    line_ending: SegmentedLineSpan { start: 16, end: 18 },
                },
            ),
            (
                "###     ###\n",
                SegmentedAtxHeadingFacts {
                    level: 3,
                    opening_marker: SegmentedLineSpan { start: 0, end: 3 },
                    content: SegmentedLineSpan { start: 8, end: 8 },
                    closing_marker: Some(SegmentedLineSpan { start: 8, end: 11 }),
                    line_ending: SegmentedLineSpan { start: 11, end: 12 },
                },
            ),
            (
                "######\tβ#   \r",
                SegmentedAtxHeadingFacts {
                    level: 6,
                    opening_marker: SegmentedLineSpan { start: 0, end: 6 },
                    content: SegmentedLineSpan { start: 7, end: 10 },
                    closing_marker: None,
                    line_ending: SegmentedLineSpan { start: 13, end: 14 },
                },
            ),
            (
                "#\n",
                SegmentedAtxHeadingFacts {
                    level: 1,
                    opening_marker: SegmentedLineSpan { start: 0, end: 1 },
                    content: SegmentedLineSpan { start: 1, end: 1 },
                    closing_marker: None,
                    line_ending: SegmentedLineSpan { start: 1, end: 2 },
                },
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(scan_line(input).atx_heading, Some(expected), "{input:?}");
            compare_line(input);
        }

        for rejected in ["####### foo\n", "#5 bolt\n", "#no-space", "\\## foo\n"] {
            assert_eq!(scan_line(rejected).atx_heading, None, "{rejected:?}");
            compare_line(rejected);
        }
    }

    #[test]
    fn atx_facts_keep_bof_bom_and_physical_offsets_exact() {
        let input = "\u{feff} # x ###\r\n";
        assert_eq!(
            scan_line_with_bom_policy(input, true).atx_heading,
            Some(SegmentedAtxHeadingFacts {
                level: 1,
                opening_marker: SegmentedLineSpan { start: 4, end: 5 },
                content: SegmentedLineSpan { start: 6, end: 7 },
                closing_marker: Some(SegmentedLineSpan { start: 8, end: 11 }),
                line_ending: SegmentedLineSpan { start: 11, end: 13 },
            })
        );
    }

    #[test]
    fn thematic_facts_keep_spaced_markers_trailing_space_and_endings_exact() {
        let cases = [
            (
                "***\n",
                SegmentedThematicBreakFacts {
                    marker: b'*',
                    marker_count: 3,
                    marker_envelope: SegmentedLineSpan { start: 0, end: 3 },
                    line_ending: SegmentedLineSpan { start: 3, end: 4 },
                },
            ),
            (
                " - - -  \r\n",
                SegmentedThematicBreakFacts {
                    marker: b'-',
                    marker_count: 3,
                    marker_envelope: SegmentedLineSpan { start: 1, end: 6 },
                    line_ending: SegmentedLineSpan { start: 8, end: 10 },
                },
            ),
            (
                "  _\t_\t_ \r",
                SegmentedThematicBreakFacts {
                    marker: b'_',
                    marker_count: 3,
                    marker_envelope: SegmentedLineSpan { start: 2, end: 7 },
                    line_ending: SegmentedLineSpan { start: 8, end: 9 },
                },
            ),
            (
                "   ****",
                SegmentedThematicBreakFacts {
                    marker: b'*',
                    marker_count: 4,
                    marker_envelope: SegmentedLineSpan { start: 3, end: 7 },
                    line_ending: SegmentedLineSpan { start: 7, end: 7 },
                },
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(scan_line(input).thematic_break, Some(expected), "{input:?}");
            compare_line(input);
        }

        for rejected in ["**\n", "*-*\n", "_ _ a _\n", "----x\n"] {
            assert_eq!(scan_line(rejected).thematic_break, None, "{rejected:?}");
            compare_line(rejected);
        }
    }

    #[test]
    fn ten_mib_atx_line_is_exact_across_tiny_feed_quanta_and_source_bounded() {
        const BODY_BYTES: usize = 10 * 1024 * 1024;
        let mut input = String::with_capacity(BODY_BYTES + 16);
        input.push_str("# ");
        input.extend(std::iter::repeat_n('a', BODY_BYTES));
        input.push_str(" ###   \r\n");

        // Production spends one lexical work unit per `push`. Varying this
        // feed quantum exercises the exact state carried across fuel yields.
        let mut scanner = SegmentedLineScanner::new(false);
        let quanta = [1, 2, 7, 31, 4_090];
        let mut cursor = 0;
        let mut polls = 0;
        let mut maximum_retained = 0;
        while cursor < input.len() {
            let grant = quanta[polls % quanta.len()];
            let end = (cursor + grant).min(input.len());
            for byte in input.as_bytes()[cursor..end].iter().copied() {
                scanner.push(byte);
            }
            assert!(end - cursor <= grant);
            cursor = end;
            polls += 1;
            maximum_retained = maximum_retained.max(scanner.retained_source_bytes());
        }
        let facts = scanner.finish().expect("bounded donor prefix");
        let body_end = 2 + BODY_BYTES;
        assert_eq!(
            facts.atx_heading,
            Some(SegmentedAtxHeadingFacts {
                level: 1,
                opening_marker: SegmentedLineSpan { start: 0, end: 1 },
                content: SegmentedLineSpan {
                    start: 2,
                    end: body_end,
                },
                closing_marker: Some(SegmentedLineSpan {
                    start: body_end + 1,
                    end: body_end + 4,
                }),
                line_ending: SegmentedLineSpan {
                    start: input.len() - 2,
                    end: input.len(),
                },
            })
        );
        assert!(polls > 2_500);
        assert!(maximum_retained <= SEGMENTED_LINE_PREFIX_BYTES);
    }

    #[test]
    fn segmented_line_folds_match_the_pinned_facade() {
        for fixed in [
            "plain",
            "# heading\n",
            "####### no\r\n",
            "``` a`b\n",
            "~~~~ anything\n",
            "<x a='b'> \n",
            "</x>\n",
            "---\n",
            "- - -\n",
            "-\n",
            "-   \n",
            "-\titem\n",
            "  +    item\r\n",
            "2. item\n",
            "1. item\n",
            "123456789. item\n",
            "| :--- | ---: |\n",
            "\tplain\n",
        ] {
            compare_line(fixed);
        }

        let alphabet = b"`~#*-_=<>/|'\".: abcXYZ09\t\x0b\x0c";
        let mut state = 0x51ce_5ca1_u64;
        for case in 0..20_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let len = 1 + usize::try_from(state % 160).unwrap();
            let mut input = String::new();
            for _ in 0..len {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                input.push(char::from(
                    alphabet[usize::try_from(state).unwrap() % alphabet.len()],
                ));
            }
            match case % 3 {
                0 => input.push('\n'),
                1 => input.push_str("\r\n"),
                _ => {}
            }
            compare_line(&input);
        }
    }
}
