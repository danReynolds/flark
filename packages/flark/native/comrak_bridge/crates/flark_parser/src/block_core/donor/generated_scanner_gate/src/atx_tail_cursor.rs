/// Immutable UTF-8 physical-line identity plus byte access. The scanner owns
/// only scalar summary state and never retains source bytes.
pub trait AtxTailCursorSource {
    fn source_key(&self) -> u64;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn byte_at(&mut self, absolute_offset: usize) -> u8;
}

// These are the exact byte classes derived and checked by generate.py from
// pinned Comrak 0.54's rtrim_slice/ctype and is_space_or_tab dependencies.
const PINNED_DONOR_TRIM_BYTES: [u8; 4] = [9, 10, 13, 32];
const PINNED_DONOR_CLOSE_SEPARATOR_BYTES: [u8; 2] = [9, 32];

fn is_donor_trim_byte(byte: u8) -> bool {
    PINNED_DONOR_TRIM_BYTES.contains(&byte)
}

fn is_donor_close_separator(byte: u8) -> bool {
    PINNED_DONOR_CLOSE_SEPARATOR_BYTES.contains(&byte)
}

/// Exact cuts produced while corresponding to Comrak's
/// `strings::chop_trailing_hashes` over one complete physical line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtxTailCuts {
    chopped_end: usize,
    content_end: usize,
    line_end: usize,
    closed: bool,
}

impl AtxTailCuts {
    /// Donor-equivalent length of the returned chopped `&str`.
    #[must_use]
    pub const fn chopped_end(self) -> usize {
        self.chopped_end
    }

    /// Physical end excluding one final LF/CR or final CRLF.
    #[must_use]
    pub const fn content_end(self) -> usize {
        self.content_end
    }

    #[must_use]
    pub const fn line_end(self) -> usize {
        self.line_end
    }

    #[must_use]
    pub const fn closed(self) -> bool {
        self.closed
    }

    /// Combines this donor-owned tail result with the absolute end returned by
    /// the generated ATX opener. This only derives ordered source partitions;
    /// it does not reimplement trailing-hash classification.
    pub fn with_opener_end(self, opener_end: usize) -> Result<AtxLineCuts, AtxLineCutsError> {
        if opener_end > self.line_end {
            return Err(AtxLineCutsError::OpenerBeyondLine {
                opener_end,
                line_end: self.line_end,
            });
        }
        let marker_end = opener_end.min(self.content_end);
        let visible_end = self.chopped_end.max(marker_end);
        Ok(AtxLineCuts {
            opener_end,
            marker_end,
            donor_chopped_end: self.chopped_end,
            visible_end,
            content_end: self.content_end,
            line_end: self.line_end,
            closed: self.closed,
        })
    }
}

/// Ordered whole-line cuts for the direct ATX source partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtxLineCuts {
    opener_end: usize,
    marker_end: usize,
    donor_chopped_end: usize,
    visible_end: usize,
    content_end: usize,
    line_end: usize,
    closed: bool,
}

impl AtxLineCuts {
    /// Raw absolute opener end. Comrak's generated scanner may include a final
    /// CR/LF, so `marker_end` is the partition-safe cut.
    #[must_use]
    pub const fn opener_end(self) -> usize {
        self.opener_end
    }

    #[must_use]
    pub const fn marker_end(self) -> usize {
        self.marker_end
    }

    #[must_use]
    pub const fn donor_chopped_end(self) -> usize {
        self.donor_chopped_end
    }

    /// End of visible inline source after accounting for empty headings whose
    /// donor chop lands inside the already-consumed opener.
    #[must_use]
    pub const fn visible_end(self) -> usize {
        self.visible_end
    }

    #[must_use]
    pub const fn content_end(self) -> usize {
        self.content_end
    }

    #[must_use]
    pub const fn line_end(self) -> usize {
        self.line_end
    }

    #[must_use]
    pub const fn closed(self) -> bool {
        self.closed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtxLineCutsError {
    OpenerBeyondLine { opener_end: usize, line_end: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtxTailScanResult {
    NeedMore,
    Complete(AtxTailCuts),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtxTailScanError {
    ZeroFuel,
    WrongSource,
    OutOfOrderFirstRead {
        requested: usize,
        expected: usize,
    },
    FirstReadPastEnd {
        requested: usize,
        len: usize,
    },
    Incomplete {
        cursor: usize,
        len: usize,
    },
    /// The pinned donor assumes its ATX caller leaves at least one byte after
    /// rtrim. Rejecting the impossible caller state avoids reproducing its
    /// indexing panic.
    EmptyAfterTrim,
    PollAfterComplete,
    PollAfterFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtxTailScanReceipt {
    pub result: AtxTailScanResult,
    pub source_byte_requests: usize,
    pub source_high_water: usize,
    pub maximum_source_request_rewind_bytes: usize,
    pub retained_source_bytes: usize,
}

/// Flark-owned forward summary correspondent for pinned Comrak 0.54's
/// handwritten backwards `strings::chop_trailing_hashes` function.
///
/// Unlike the re2rust scanner artifacts, this state machine is not generated
/// from a regex. `generate.py` extracts and hashes the exact donor function,
/// verifies its helper-derived byte classes, and hashes this correspondent so
/// donor or local changes require an explicit provenance update.
#[derive(Clone, Debug)]
pub struct AtxTailCursorScanner {
    source_key: u64,
    accumulator: AtxTailAccumulator,
    terminal: Option<AtxTailCuts>,
    failed: bool,
}

/// Source-identity-free forward fold shared by the standalone proof cursor
/// and the fused ATX opener/tail continuation. Only this module owns the
/// handwritten donor correspondent; fused callers may feed each physical
/// first-read byte exactly once without reimplementing trailing-hash grammar.
#[derive(Clone, Debug)]
pub(crate) struct AtxTailAccumulator {
    source_len: usize,
    cursor: usize,
    source_high_water: usize,
    last_nontrim_end: usize,
    hash_run_start: Option<usize>,
    before_hash_nontrim_end: usize,
    previous_was_hash: bool,
    hash_run_preceded_by_separator: bool,
    second_to_last_byte: Option<u8>,
    last_byte: Option<u8>,
}

impl AtxTailCursorScanner {
    #[must_use]
    pub const fn new(source_key: u64, source_len: usize) -> Self {
        Self {
            source_key,
            accumulator: AtxTailAccumulator::new(source_len),
            terminal: None,
            failed: false,
        }
    }

    pub fn poll<S: AtxTailCursorSource>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<AtxTailScanReceipt, AtxTailScanError> {
        if fuel == 0 {
            return Err(AtxTailScanError::ZeroFuel);
        }
        if self.failed {
            return Err(AtxTailScanError::PollAfterFailure);
        }
        if self.terminal.is_some() {
            return Err(AtxTailScanError::PollAfterComplete);
        }
        if source.source_key() != self.source_key || source.len() != self.accumulator.source_len() {
            self.failed = true;
            return Err(AtxTailScanError::WrongSource);
        }

        let mut source_byte_requests = 0;
        while self.accumulator.cursor() < self.accumulator.source_len()
            && source_byte_requests < fuel
        {
            let offset = self.accumulator.cursor();
            let byte = source.byte_at(offset);
            if let Err(error) = self.accumulator.observe_first_read(offset, byte) {
                self.failed = true;
                return Err(error);
            }
            source_byte_requests += 1;
        }

        let result = if self.accumulator.cursor() == self.accumulator.source_len() {
            let cuts = match self.accumulator.finish() {
                Ok(cuts) => cuts,
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            };
            self.terminal = Some(cuts);
            AtxTailScanResult::Complete(cuts)
        } else {
            AtxTailScanResult::NeedMore
        };

        Ok(AtxTailScanReceipt {
            result,
            source_byte_requests,
            source_high_water: self.accumulator.source_high_water(),
            maximum_source_request_rewind_bytes: 0,
            retained_source_bytes: 0,
        })
    }

    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.accumulator.cursor()
    }

    #[must_use]
    pub const fn source_high_water(&self) -> usize {
        self.accumulator.source_high_water()
    }

    #[must_use]
    pub const fn retained_source_bytes(&self) -> usize {
        0
    }
}

impl AtxTailAccumulator {
    pub(crate) const fn new(source_len: usize) -> Self {
        Self {
            source_len,
            cursor: 0,
            source_high_water: 0,
            last_nontrim_end: 0,
            hash_run_start: None,
            before_hash_nontrim_end: 0,
            previous_was_hash: false,
            hash_run_preceded_by_separator: false,
            second_to_last_byte: None,
            last_byte: None,
        }
    }

    pub(crate) const fn source_len(&self) -> usize {
        self.source_len
    }

    pub(crate) const fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) const fn source_high_water(&self) -> usize {
        self.source_high_water
    }

    /// Advances the donor correspondent for one new physical byte. Repeated
    /// generated peeks must be served by the fused layer and never enter this
    /// fold a second time.
    pub(crate) fn observe_first_read(
        &mut self,
        offset: usize,
        byte: u8,
    ) -> Result<(), AtxTailScanError> {
        if offset != self.cursor {
            return Err(AtxTailScanError::OutOfOrderFirstRead {
                requested: offset,
                expected: self.cursor,
            });
        }
        if offset == self.source_len {
            return Err(AtxTailScanError::FirstReadPastEnd {
                requested: offset,
                len: self.source_len,
            });
        }
        self.observe(offset, byte);
        self.cursor += 1;
        self.source_high_water = self.cursor;
        Ok(())
    }

    fn observe(&mut self, offset: usize, byte: u8) {
        let preceding_byte = self.last_byte;
        if is_donor_trim_byte(byte) {
            self.previous_was_hash = false;
        } else {
            if byte == b'#' {
                if !self.previous_was_hash {
                    self.hash_run_start = Some(offset);
                    self.before_hash_nontrim_end = self.last_nontrim_end;
                    self.hash_run_preceded_by_separator =
                        preceding_byte.is_some_and(is_donor_close_separator);
                }
                self.previous_was_hash = true;
            } else {
                self.hash_run_start = None;
                self.previous_was_hash = false;
                self.hash_run_preceded_by_separator = false;
            }
            self.last_nontrim_end = offset + 1;
        }

        self.second_to_last_byte = self.last_byte;
        self.last_byte = Some(byte);
    }

    pub(crate) fn finish(&self) -> Result<AtxTailCuts, AtxTailScanError> {
        if self.cursor != self.source_len {
            return Err(AtxTailScanError::Incomplete {
                cursor: self.cursor,
                len: self.source_len,
            });
        }
        if self.last_nontrim_end == 0 {
            return Err(AtxTailScanError::EmptyAfterTrim);
        }
        let closed = self
            .hash_run_start
            .is_some_and(|start| start > 0 && self.hash_run_preceded_by_separator);
        let chopped_end = if closed {
            self.before_hash_nontrim_end
        } else {
            self.last_nontrim_end
        };
        let content_end = match (self.second_to_last_byte, self.last_byte) {
            (Some(b'\r'), Some(b'\n')) => self.source_len - 2,
            (_, Some(b'\r' | b'\n')) => self.source_len - 1,
            _ => self.source_len,
        };
        Ok(AtxTailCuts {
            chopped_end,
            content_end,
            line_end: self.source_len,
            closed,
        })
    }
}
