use crate::atx_cursor_generated::{
    AtxCursorSource, CursorAtxScanner, CursorScanError, CursorScanResult,
    CURSOR_ATX_MAX_LOOKAHEAD_SLACK, CURSOR_ATX_REJECTION_PREFIX_CAP,
};
use crate::atx_tail_cursor::{AtxLineCuts, AtxLineCutsError, AtxTailAccumulator, AtxTailScanError};

/// Hard cap on lexical source work in one fused poll, including generated
/// lookahead slack. Production adapters can use the same grant for their
/// actor-owned byte view without depending on physical-line length.
pub const FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL: usize = 4 * 1024;

const FUSED_ATX_MAX_GENERATED_FUEL: usize =
    FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL - CURSOR_ATX_MAX_LOOKAHEAD_SLACK;
const INNER_PROOF_SOURCE_KEY: u64 = 0x4655_5345_4441_5458;
const TAB_STOP: usize = 4;
const UTF8_BOM: [u8; 3] = [0xef, 0xbb, 0xbf];
pub const FUSED_ATX_REJECTION_PREFIX_CAP: usize =
    UTF8_BOM.len() + (TAB_STOP - 1) + CURSOR_ATX_REJECTION_PREFIX_CAP;

/// One actor-borrowed physical-line byte source.
///
/// `access_budget` is a lower bound on the number of next-sequential
/// `read_byte` calls this borrow promises to honor. Returning a smaller grant
/// is a resumable scheduling decision: the fused scanner yields before asking
/// for a byte it cannot safely obtain. Repeated generated peeks are served by
/// the fused scanner's one-byte scratch and never reach this interface.
pub trait FusedAtxLineSource {
    type Identity: Copy + Eq;
    type Error;

    fn identity(&self) -> Self::Identity;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn access_budget(&self) -> usize;
    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FusedAtxLineScanResult {
    NeedMore,
    Matched(AtxLineCuts),
    NoMatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FusedAtxLineScanReceipt {
    pub result: FusedAtxLineScanResult,
    /// Generated peeks plus forward tail-fold reads performed in this poll.
    pub lexical_work_units: usize,
    /// Unique sequential bytes requested from the caller in this poll.
    pub source_first_reads: usize,
    /// Generated requests served from the immediately preceding byte.
    pub repeated_generated_peeks: usize,
    /// Unique physical bytes exposed since admission.
    pub physical_high_water: usize,
    /// Accepted generated cursor. This can trail `physical_high_water` by one
    /// because the DFA may peek at the first content byte without accepting it.
    pub opener_logical_cut: Option<usize>,
    /// Retained source payload bytes, excluding fixed-size scalar summary
    /// fields such as the tail fold's last-byte values.
    pub retained_source_bytes: usize,
    pub rejection_prefix_bytes: usize,
    pub source_budget_exhausted: bool,
    pub maximum_source_request_rewind_bytes: usize,
}

/// Donor-owned block-prefix facts for one accepted ATX opener.
///
/// These facts remain inside the donor integration layer. They let a caller
/// construct exact source positions and coverage without independently
/// reclassifying a BOM, indentation, or the generated hash opener.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FusedAtxDonorMatch {
    claim_start: usize,
    opener_start: usize,
    opener_start_column: usize,
    opener_end_column: usize,
    indent_columns: usize,
    level: u8,
}

impl FusedAtxDonorMatch {
    #[must_use]
    pub const fn claim_start(self) -> usize {
        self.claim_start
    }

    #[must_use]
    pub const fn opener_start(self) -> usize {
        self.opener_start
    }

    #[must_use]
    pub const fn opener_start_column(self) -> usize {
        self.opener_start_column
    }

    #[must_use]
    pub const fn opener_end_column(self) -> usize {
        self.opener_end_column
    }

    #[must_use]
    pub const fn indent_columns(self) -> usize {
        self.indent_columns
    }

    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum FusedAtxLineScanError<SourceError> {
    ZeroFuel,
    WrongSource,
    Source(SourceError),
    SourceContainsSentinel {
        absolute_offset: usize,
    },
    SourceBudgetContractViolated,
    NonSequentialGeneratedRequest {
        requested: usize,
        physical_high_water: usize,
    },
    UnboundedRejectionPrefix {
        physical_high_water: usize,
        fixed_cap: usize,
    },
    PrefixInvariant,
    Generated(CursorScanError),
    Tail(AtxTailScanError),
    LineCuts(AtxLineCutsError),
    PollAfterComplete,
    PollAfterFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FusedAtxPhase {
    Prefix,
    Opener,
    Tail { opener_end: usize },
    Complete,
    Failed,
}

#[derive(Clone, Debug)]
struct FusedAtxObservation {
    initial_column: usize,
    column_before_last: usize,
    column_after_last: usize,
    opener_start: Option<usize>,
    opener_start_column: Option<usize>,
    leading_hash_bytes: usize,
    leading_hash_run: bool,
}

impl FusedAtxObservation {
    const fn new(initial_column: usize) -> Self {
        Self {
            initial_column,
            column_before_last: initial_column,
            column_after_last: initial_column,
            opener_start: None,
            opener_start_column: None,
            leading_hash_bytes: 0,
            leading_hash_run: false,
        }
    }

    fn start_opener(&mut self, absolute_offset: usize) -> Result<(), ()> {
        if self.opener_start.is_some() {
            return Err(());
        }
        self.opener_start = Some(absolute_offset);
        self.opener_start_column = Some(self.column_after_last);
        self.leading_hash_bytes = 0;
        self.leading_hash_run = true;
        Ok(())
    }

    fn observe_column_byte(&mut self, absolute_offset: usize, byte: u8) -> Result<(), ()> {
        if let Some(opener_start) = self.opener_start {
            if absolute_offset >= opener_start && self.leading_hash_run {
                if byte == b'#' {
                    self.leading_hash_bytes += 1;
                } else {
                    self.leading_hash_run = false;
                }
            }
        }
        self.column_before_last = self.column_after_last;
        self.column_after_last = if byte == b'\t' {
            self.column_after_last + (TAB_STOP - (self.column_after_last % TAB_STOP))
        } else {
            self.column_after_last + 1
        };
        Ok(())
    }

    fn column_at(&self, absolute_offset: usize, physical_high_water: usize) -> Option<usize> {
        if absolute_offset == physical_high_water {
            Some(self.column_after_last)
        } else if absolute_offset.checked_add(1) == Some(physical_high_water) {
            Some(self.column_before_last)
        } else {
            None
        }
    }
}

/// Source-generic one-pass ATX lexical continuation.
///
/// The generated opener remains the only opener recognizer. Every unique byte
/// it first-reads is also folded once through the donor-correspondent forward
/// `chop_trailing_hashes` summary. On an opener match, that summary continues
/// from the same physical high-water; neither scanner restarts from byte zero.
/// No grammar state or semantic command is represented by this type.
#[derive(Clone, Debug)]
pub struct FusedAtxLineScanner<Identity: Copy + Eq> {
    identity: Identity,
    source_len: usize,
    opener: Option<CursorAtxScanner>,
    tail: AtxTailAccumulator,
    phase: FusedAtxPhase,
    bom_probe_len: usize,
    bom_resolved: bool,
    claim_start: Option<usize>,
    observation: FusedAtxObservation,
    donor_match: Option<FusedAtxDonorMatch>,
    opener_logical_cut: Option<usize>,
    physical_high_water: usize,
    last_byte: Option<u8>,
    rejection_prefix: [u8; FUSED_ATX_REJECTION_PREFIX_CAP],
    rejection_prefix_len: usize,
    rejection_prefix_cap: usize,
}

impl<Identity: Copy + Eq> FusedAtxLineScanner<Identity> {
    #[must_use]
    pub fn new(identity: Identity, source_len: usize) -> Self {
        Self {
            identity,
            source_len,
            opener: Some(CursorAtxScanner::new(INNER_PROOF_SOURCE_KEY, source_len)),
            tail: AtxTailAccumulator::new(source_len),
            phase: FusedAtxPhase::Opener,
            bom_probe_len: 0,
            bom_resolved: true,
            claim_start: Some(0),
            observation: {
                let mut observation = FusedAtxObservation::new(0);
                observation
                    .start_opener(0)
                    .expect("fresh exact-opener observation");
                observation
            },
            donor_match: None,
            opener_logical_cut: None,
            physical_high_water: 0,
            last_byte: None,
            rejection_prefix: [0; FUSED_ATX_REJECTION_PREFIX_CAP],
            rejection_prefix_len: 0,
            rejection_prefix_cap: CURSOR_ATX_REJECTION_PREFIX_CAP,
        }
    }

    /// Construct the donor's root/container-relative prefix plus ATX scanner.
    ///
    /// The prefix recognizes an optional document-start UTF-8 BOM, then the
    /// exact space/tab column walk used by Comrak's `find_first_nonspace`. An
    /// indentation reaching four columns terminates as `NoMatch`; otherwise
    /// the first nonspace byte is handed to the generated ATX opener from the
    /// fixed replay cache without a source rewind.
    #[must_use]
    pub fn new_with_block_prefix(
        identity: Identity,
        source_len: usize,
        initial_column: usize,
        allow_initial_bom: bool,
    ) -> Self {
        Self {
            identity,
            source_len,
            opener: None,
            tail: AtxTailAccumulator::new(source_len),
            phase: FusedAtxPhase::Prefix,
            bom_probe_len: 0,
            bom_resolved: !allow_initial_bom,
            claim_start: (!allow_initial_bom).then_some(0),
            observation: FusedAtxObservation::new(initial_column),
            donor_match: None,
            opener_logical_cut: None,
            physical_high_water: 0,
            last_byte: None,
            rejection_prefix: [0; FUSED_ATX_REJECTION_PREFIX_CAP],
            rejection_prefix_len: 0,
            rejection_prefix_cap: FUSED_ATX_REJECTION_PREFIX_CAP,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> Identity {
        self.identity
    }

    #[must_use]
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    #[must_use]
    pub const fn physical_high_water(&self) -> usize {
        self.physical_high_water
    }

    #[must_use]
    pub const fn opener_logical_cut(&self) -> Option<usize> {
        self.opener_logical_cut
    }

    #[must_use]
    pub const fn donor_match(&self) -> Option<FusedAtxDonorMatch> {
        self.donor_match
    }

    /// Bounded first-read prefix retained only so a donor-owned next stage can
    /// continue after `NoMatch` without rewinding the physical source.
    #[must_use]
    pub fn rejection_prefix(&self) -> &[u8] {
        &self.rejection_prefix[..self.rejection_prefix_len]
    }

    #[must_use]
    pub fn retained_source_bytes(&self) -> usize {
        self.rejection_prefix_len + usize::from(self.last_byte.is_some())
    }

    pub fn poll<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<FusedAtxLineScanReceipt, FusedAtxLineScanError<S::Error>>
    where
        S: FusedAtxLineSource<Identity = Identity>,
    {
        if fuel == 0 {
            return Err(FusedAtxLineScanError::ZeroFuel);
        }
        match self.phase {
            FusedAtxPhase::Complete => {
                return Err(FusedAtxLineScanError::PollAfterComplete);
            }
            FusedAtxPhase::Failed => {
                return Err(FusedAtxLineScanError::PollAfterFailure);
            }
            FusedAtxPhase::Prefix | FusedAtxPhase::Opener | FusedAtxPhase::Tail { .. } => {}
        }
        if source.identity() != self.identity || source.len() != self.source_len {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::WrongSource);
        }

        match self.phase {
            FusedAtxPhase::Prefix => self.poll_prefix(source, fuel),
            FusedAtxPhase::Opener => self.poll_opener(source, fuel),
            FusedAtxPhase::Tail { opener_end } => self.poll_tail(source, fuel, opener_end),
            FusedAtxPhase::Complete | FusedAtxPhase::Failed => unreachable!(),
        }
    }

    fn poll_opener<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<FusedAtxLineScanReceipt, FusedAtxLineScanError<S::Error>>
    where
        S: FusedAtxLineSource<Identity = Identity>,
    {
        let Some(opener_start) = self.observation.opener_start else {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::PrefixInvariant);
        };
        let Some(opener) = self.opener.as_mut() else {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::PrefixInvariant);
        };
        let requested_generated_fuel = fuel.min(FUSED_ATX_MAX_GENERATED_FUEL);
        let source_budget = source
            .access_budget()
            .min(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let generated_fuel = requested_generated_fuel
            .min(source_budget.saturating_sub(CURSOR_ATX_MAX_LOOKAHEAD_SLACK));
        if generated_fuel == 0 {
            return Ok(self.receipt(FusedAtxLineScanResult::NeedMore, 0, 0, 0, true));
        }

        let source_budget_exhausted = generated_fuel < requested_generated_fuel;
        let mut adapter = FusedGeneratedSource {
            source,
            source_len: self.source_len - opener_start,
            physical_base: opener_start,
            tail: &mut self.tail,
            observation: &mut self.observation,
            physical_high_water: &mut self.physical_high_water,
            last_byte: &mut self.last_byte,
            rejection_prefix: &mut self.rejection_prefix,
            rejection_prefix_len: &mut self.rejection_prefix_len,
            rejection_prefix_cap: self.rejection_prefix_cap,
            first_read_limit: source_budget,
            lexical_work_units: 0,
            source_first_reads: 0,
            repeated_generated_peeks: 0,
            failure: None,
        };
        let generated = opener.poll(&mut adapter, generated_fuel);
        let lexical_work_units = adapter.lexical_work_units;
        let source_first_reads = adapter.source_first_reads;
        let repeated_generated_peeks = adapter.repeated_generated_peeks;
        let adapter_failure = adapter.failure.take();
        drop(adapter);

        if let Some(error) = adapter_failure {
            self.phase = FusedAtxPhase::Failed;
            return Err(match error {
                FusedGeneratedSourceFailure::Source(error) => FusedAtxLineScanError::Source(error),
                FusedGeneratedSourceFailure::SourceContainsSentinel { absolute_offset } => {
                    FusedAtxLineScanError::SourceContainsSentinel { absolute_offset }
                }
                FusedGeneratedSourceFailure::SourceBudgetContractViolated => {
                    FusedAtxLineScanError::SourceBudgetContractViolated
                }
                FusedGeneratedSourceFailure::NonSequentialGeneratedRequest {
                    requested,
                    physical_high_water,
                } => FusedAtxLineScanError::NonSequentialGeneratedRequest {
                    requested,
                    physical_high_water,
                },
                FusedGeneratedSourceFailure::Tail(error) => FusedAtxLineScanError::Tail(error),
            });
        }

        let generated = match generated {
            Ok(receipt) => receipt,
            Err(error) => {
                self.phase = FusedAtxPhase::Failed;
                return Err(FusedAtxLineScanError::Generated(error));
            }
        };
        debug_assert!(
            lexical_work_units <= FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL,
            "generated ATX work exceeds the fused actor cap"
        );
        let result = match generated
            .result
            .expect("generated ATX poll reports a result")
        {
            CursorScanResult::NeedMore => FusedAtxLineScanResult::NeedMore,
            CursorScanResult::Matched(relative_opener_end) => {
                let Some(opener_end) = opener_start.checked_add(relative_opener_end) else {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::PrefixInvariant);
                };
                let Some(opener_start_column) = self.observation.opener_start_column else {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::PrefixInvariant);
                };
                let Some(opener_end_column) = self
                    .observation
                    .column_at(opener_end, self.physical_high_water)
                else {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::PrefixInvariant);
                };
                let Ok(level) = u8::try_from(self.observation.leading_hash_bytes) else {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::PrefixInvariant);
                };
                if !(1..=6).contains(&level) {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::PrefixInvariant);
                }
                let Some(claim_start) = self.claim_start else {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::PrefixInvariant);
                };
                self.phase = FusedAtxPhase::Tail { opener_end };
                self.opener_logical_cut = Some(opener_end);
                self.donor_match = Some(FusedAtxDonorMatch {
                    claim_start,
                    opener_start,
                    opener_start_column,
                    opener_end_column,
                    indent_columns: opener_start_column
                        .checked_sub(self.observation.initial_column)
                        .ok_or(FusedAtxLineScanError::PrefixInvariant)?,
                    level,
                });
                self.rejection_prefix_len = 0;
                self.last_byte = None;
                FusedAtxLineScanResult::NeedMore
            }
            CursorScanResult::NoMatch => {
                if self.physical_high_water > self.rejection_prefix_cap
                    || self.rejection_prefix_len != self.physical_high_water
                {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::UnboundedRejectionPrefix {
                        physical_high_water: self.physical_high_water,
                        fixed_cap: self.rejection_prefix_cap,
                    });
                }
                self.phase = FusedAtxPhase::Complete;
                self.last_byte = None;
                FusedAtxLineScanResult::NoMatch
            }
        };
        Ok(self.receipt(
            result,
            lexical_work_units,
            source_first_reads,
            repeated_generated_peeks,
            source_budget_exhausted,
        ))
    }

    fn poll_prefix<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<FusedAtxLineScanReceipt, FusedAtxLineScanError<S::Error>>
    where
        S: FusedAtxLineSource<Identity = Identity>,
    {
        let requested_work = fuel.min(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let source_budget = source
            .access_budget()
            .min(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let work = requested_work.min(source_budget);
        if work == 0 {
            return Ok(self.receipt(FusedAtxLineScanResult::NeedMore, 0, 0, 0, true));
        }

        let mut source_first_reads = 0;
        while source_first_reads < work {
            if !self.bom_resolved {
                if self.physical_high_water == self.source_len {
                    self.resolve_non_bom_prefix()?;
                    break;
                }
                let byte = self.read_prefix_byte(source)?;
                source_first_reads += 1;
                if byte != UTF8_BOM[self.bom_probe_len] {
                    self.bom_probe_len += 1;
                    self.resolve_non_bom_prefix()?;
                    break;
                }
                self.bom_probe_len += 1;
                if self.bom_probe_len == UTF8_BOM.len() {
                    self.bom_resolved = true;
                    self.claim_start = Some(UTF8_BOM.len());
                }
                continue;
            }

            if self.physical_high_water == self.source_len {
                self.begin_generated_opener(self.source_len)?;
                break;
            }
            let offset = self.physical_high_water;
            let byte = self.read_prefix_byte(source)?;
            source_first_reads += 1;
            if matches!(byte, b' ' | b'\t') {
                self.observation
                    .observe_column_byte(offset, byte)
                    .map_err(|()| FusedAtxLineScanError::PrefixInvariant)?;
                let indent = self
                    .observation
                    .column_after_last
                    .checked_sub(self.observation.initial_column)
                    .ok_or(FusedAtxLineScanError::PrefixInvariant)?;
                if indent >= TAB_STOP {
                    self.phase = FusedAtxPhase::Complete;
                    self.last_byte = None;
                    return Ok(self.receipt(
                        FusedAtxLineScanResult::NoMatch,
                        source_first_reads,
                        source_first_reads,
                        0,
                        source_first_reads == source_budget,
                    ));
                }
            } else {
                self.begin_generated_opener(offset)?;
                self.observation
                    .observe_column_byte(offset, byte)
                    .map_err(|()| FusedAtxLineScanError::PrefixInvariant)?;
                break;
            }
        }

        if !self.bom_resolved && self.physical_high_water == self.source_len {
            self.resolve_non_bom_prefix()?;
        }
        if self.phase == FusedAtxPhase::Complete {
            self.last_byte = None;
            return Ok(self.receipt(
                FusedAtxLineScanResult::NoMatch,
                source_first_reads,
                source_first_reads,
                0,
                source_first_reads == source_budget,
            ));
        }
        if self.bom_resolved && self.opener.is_none() && self.physical_high_water == self.source_len
        {
            self.begin_generated_opener(self.source_len)?;
        }
        Ok(self.receipt(
            FusedAtxLineScanResult::NeedMore,
            source_first_reads,
            source_first_reads,
            0,
            source_first_reads == source_budget,
        ))
    }

    fn read_prefix_byte<S>(&mut self, source: &mut S) -> Result<u8, FusedAtxLineScanError<S::Error>>
    where
        S: FusedAtxLineSource<Identity = Identity>,
    {
        let offset = self.physical_high_water;
        let byte = match source.read_byte(offset) {
            Ok(byte) => byte,
            Err(error) => {
                self.phase = FusedAtxPhase::Failed;
                return Err(FusedAtxLineScanError::Source(error));
            }
        };
        if byte == 0xff {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::SourceContainsSentinel {
                absolute_offset: offset,
            });
        }
        if let Err(error) = self.tail.observe_first_read(offset, byte) {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::Tail(error));
        }
        if self.rejection_prefix_len == self.rejection_prefix_cap {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::UnboundedRejectionPrefix {
                physical_high_water: self.physical_high_water,
                fixed_cap: self.rejection_prefix_cap,
            });
        }
        self.rejection_prefix[self.rejection_prefix_len] = byte;
        self.rejection_prefix_len += 1;
        self.physical_high_water += 1;
        self.last_byte = Some(byte);
        Ok(byte)
    }

    fn resolve_non_bom_prefix<SourceError>(
        &mut self,
    ) -> Result<(), FusedAtxLineScanError<SourceError>> {
        self.bom_resolved = true;
        self.claim_start = Some(0);
        for offset in 0..self.bom_probe_len {
            let byte = self.rejection_prefix[offset];
            if self.observation.opener_start.is_none() {
                if matches!(byte, b' ' | b'\t') {
                    self.observation
                        .observe_column_byte(offset, byte)
                        .map_err(|()| FusedAtxLineScanError::PrefixInvariant)?;
                    let indent = self
                        .observation
                        .column_after_last
                        .checked_sub(self.observation.initial_column)
                        .ok_or(FusedAtxLineScanError::PrefixInvariant)?;
                    if indent >= TAB_STOP {
                        self.phase = FusedAtxPhase::Complete;
                        return Ok(());
                    }
                    continue;
                }
                self.begin_generated_opener(offset)?;
            }
            self.observation
                .observe_column_byte(offset, byte)
                .map_err(|()| FusedAtxLineScanError::PrefixInvariant)?;
        }
        Ok(())
    }

    fn begin_generated_opener<SourceError>(
        &mut self,
        opener_start: usize,
    ) -> Result<(), FusedAtxLineScanError<SourceError>> {
        if opener_start > self.source_len || self.opener.is_some() {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::PrefixInvariant);
        }
        self.observation
            .start_opener(opener_start)
            .map_err(|()| FusedAtxLineScanError::PrefixInvariant)?;
        self.opener = Some(CursorAtxScanner::new(
            INNER_PROOF_SOURCE_KEY,
            self.source_len - opener_start,
        ));
        self.phase = FusedAtxPhase::Opener;
        Ok(())
    }

    fn poll_tail<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
        opener_end: usize,
    ) -> Result<FusedAtxLineScanReceipt, FusedAtxLineScanError<S::Error>>
    where
        S: FusedAtxLineSource<Identity = Identity>,
    {
        if self.tail.cursor() != self.physical_high_water {
            self.phase = FusedAtxPhase::Failed;
            return Err(FusedAtxLineScanError::Tail(
                AtxTailScanError::OutOfOrderFirstRead {
                    requested: self.physical_high_water,
                    expected: self.tail.cursor(),
                },
            ));
        }
        let requested_work = fuel.min(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let source_budget = source
            .access_budget()
            .min(FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL);
        let work = requested_work.min(source_budget);
        if work == 0 && self.physical_high_water < self.source_len {
            return Ok(self.receipt(FusedAtxLineScanResult::NeedMore, 0, 0, 0, true));
        }

        let mut source_first_reads = 0;
        while self.physical_high_water < self.source_len && source_first_reads < work {
            let offset = self.physical_high_water;
            let byte = match source.read_byte(offset) {
                Ok(byte) => byte,
                Err(error) => {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::Source(error));
                }
            };
            if byte == 0xff {
                self.phase = FusedAtxPhase::Failed;
                return Err(FusedAtxLineScanError::SourceContainsSentinel {
                    absolute_offset: offset,
                });
            }
            if let Err(error) = self.tail.observe_first_read(offset, byte) {
                self.phase = FusedAtxPhase::Failed;
                return Err(FusedAtxLineScanError::Tail(error));
            }
            self.physical_high_water += 1;
            source_first_reads += 1;
        }

        let result = if self.physical_high_water == self.source_len {
            let cuts = match self.tail.finish() {
                Ok(cuts) => cuts,
                Err(error) => {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::Tail(error));
                }
            };
            let cuts = match cuts.with_opener_end(opener_end) {
                Ok(cuts) => cuts,
                Err(error) => {
                    self.phase = FusedAtxPhase::Failed;
                    return Err(FusedAtxLineScanError::LineCuts(error));
                }
            };
            self.phase = FusedAtxPhase::Complete;
            FusedAtxLineScanResult::Matched(cuts)
        } else {
            FusedAtxLineScanResult::NeedMore
        };
        Ok(self.receipt(
            result,
            source_first_reads,
            source_first_reads,
            0,
            source_first_reads == source_budget && self.physical_high_water < self.source_len,
        ))
    }

    fn receipt(
        &self,
        result: FusedAtxLineScanResult,
        lexical_work_units: usize,
        source_first_reads: usize,
        repeated_generated_peeks: usize,
        source_budget_exhausted: bool,
    ) -> FusedAtxLineScanReceipt {
        FusedAtxLineScanReceipt {
            result,
            lexical_work_units,
            source_first_reads,
            repeated_generated_peeks,
            physical_high_water: self.physical_high_water,
            opener_logical_cut: self.opener_logical_cut(),
            retained_source_bytes: self.retained_source_bytes(),
            rejection_prefix_bytes: self.rejection_prefix_len,
            source_budget_exhausted,
            maximum_source_request_rewind_bytes: 0,
        }
    }
}

enum FusedGeneratedSourceFailure<SourceError> {
    Source(SourceError),
    SourceContainsSentinel {
        absolute_offset: usize,
    },
    SourceBudgetContractViolated,
    NonSequentialGeneratedRequest {
        requested: usize,
        physical_high_water: usize,
    },
    Tail(AtxTailScanError),
}

struct FusedGeneratedSource<'a, S: FusedAtxLineSource> {
    source: &'a mut S,
    source_len: usize,
    physical_base: usize,
    tail: &'a mut AtxTailAccumulator,
    observation: &'a mut FusedAtxObservation,
    physical_high_water: &'a mut usize,
    last_byte: &'a mut Option<u8>,
    rejection_prefix: &'a mut [u8; FUSED_ATX_REJECTION_PREFIX_CAP],
    rejection_prefix_len: &'a mut usize,
    rejection_prefix_cap: usize,
    first_read_limit: usize,
    lexical_work_units: usize,
    source_first_reads: usize,
    repeated_generated_peeks: usize,
    failure: Option<FusedGeneratedSourceFailure<S::Error>>,
}

impl<S: FusedAtxLineSource> AtxCursorSource for FusedGeneratedSource<'_, S> {
    fn source_key(&self) -> u64 {
        INNER_PROOF_SOURCE_KEY
    }

    fn len(&self) -> usize {
        self.source_len
    }

    fn byte_at(&mut self, absolute_offset: usize) -> u8 {
        self.lexical_work_units += 1;
        if self.failure.is_some() {
            return 0xff;
        }
        let Some(physical_offset) = self.physical_base.checked_add(absolute_offset) else {
            self.failure = Some(FusedGeneratedSourceFailure::NonSequentialGeneratedRequest {
                requested: absolute_offset,
                physical_high_water: *self.physical_high_water,
            });
            return 0xff;
        };
        if physical_offset.checked_add(1) == Some(*self.physical_high_water) {
            self.repeated_generated_peeks += 1;
            return self
                .last_byte
                .expect("a repeated peek follows one first-read");
        }
        if physical_offset < *self.physical_high_water
            && physical_offset < *self.rejection_prefix_len
        {
            self.repeated_generated_peeks += 1;
            return self.rejection_prefix[physical_offset];
        }
        if physical_offset != *self.physical_high_water {
            self.failure = Some(FusedGeneratedSourceFailure::NonSequentialGeneratedRequest {
                requested: physical_offset,
                physical_high_water: *self.physical_high_water,
            });
            return 0xff;
        }
        if self.source_first_reads == self.first_read_limit {
            self.failure = Some(FusedGeneratedSourceFailure::SourceBudgetContractViolated);
            return 0xff;
        }
        let byte = match self.source.read_byte(physical_offset) {
            Ok(byte) => byte,
            Err(error) => {
                self.failure = Some(FusedGeneratedSourceFailure::Source(error));
                return 0xff;
            }
        };
        if byte == 0xff {
            self.failure = Some(FusedGeneratedSourceFailure::SourceContainsSentinel {
                absolute_offset: physical_offset,
            });
            return 0xff;
        }
        if let Err(error) = self.tail.observe_first_read(physical_offset, byte) {
            self.failure = Some(FusedGeneratedSourceFailure::Tail(error));
            return 0xff;
        }
        if self
            .observation
            .observe_column_byte(physical_offset, byte)
            .is_err()
        {
            self.failure = Some(FusedGeneratedSourceFailure::NonSequentialGeneratedRequest {
                requested: physical_offset,
                physical_high_water: *self.physical_high_water,
            });
            return 0xff;
        }
        if *self.rejection_prefix_len < self.rejection_prefix_cap {
            self.rejection_prefix[*self.rejection_prefix_len] = byte;
            *self.rejection_prefix_len += 1;
        }
        *self.physical_high_water += 1;
        *self.last_byte = Some(byte);
        self.source_first_reads += 1;
        byte
    }
}
