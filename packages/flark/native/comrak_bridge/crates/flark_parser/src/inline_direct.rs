//! Fuelled direct-link/image ownership over one exact inline source range.
//!
//! Bracket precedence remains Flark-owned and source-backed. The pinned
//! Comrak facade supplies an atomic differential oracle for direct-link tail
//! cuts, while this production machine mirrors those cuts incrementally so a
//! large Paragraph is never materialized merely to recognize one link.

use std::cmp::Reverse;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::ops::Range;

use comrak::block_spine_facade;
use flark_engine::parser_internal::{
    M11ParserRangeCursor, M11ParserRangeError, M11ParserRangeStatus, M11ReferenceResolution,
    M11ReferenceResolver, M11ReferenceResolverError, M11_PARSER_RANGE_MAX_POLL_BYTES,
};
use flark_engine::{DocumentRuntime, SourceVersion};

#[cfg(any(test, feature = "m11-compact-probe"))]
use crate::block_core::M11CompactReferenceResolver;

use crate::inline_autolink::{
    M11InlineAutolinkError, M11InlineOpaqueCandidate, M11InlineOpaqueCandidates,
    M11InlineOpaqueKind,
};
use crate::inline_projection::M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES;
use crate::reference_label::{ReferenceLabelAccumulator, ReferenceLabelAccumulatorError};
use crate::reference_value::{
    clean_title_body_range, ReferenceValueBodyCleaner, ReferenceValueCleanerError,
    ReferenceValueCleanerStatus,
};

pub(crate) const M11_INLINE_DIRECT_MAX_POLL_TRANSITIONS: usize = M11_PARSER_RANGE_MAX_POLL_BYTES;
const DIRECT_TAIL_MAX_BYTES: usize = block_spine_facade::MAX_CLASSIFICATION_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineDirectKind {
    Link,
    Image,
    ReferenceLink,
    ReferenceImage,
}

/// One parser-authenticated direct link or image.
///
/// Source and label ranges are relative to the exact inline leaf. Direct-link
/// destination/title cuts use that same basis, widened to `u64`; reference
/// destination/title cuts are document-absolute ranges from the winning
/// definition. The kind is therefore the authority for interpreting those
/// value coordinates.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct M11InlineDirectFact {
    kind: M11InlineDirectKind,
    source: Range<u32>,
    label_source: Range<u32>,
    destination_source: Range<u64>,
    title_source: Option<Range<u64>>,
    cooked_destination: Box<str>,
    cooked_title: Option<Box<str>>,
}

impl M11InlineDirectFact {
    pub(crate) const fn kind(&self) -> M11InlineDirectKind {
        self.kind
    }

    pub(crate) fn source(&self) -> Range<u32> {
        self.source.clone()
    }

    pub(crate) fn label_source(&self) -> Range<u32> {
        self.label_source.clone()
    }

    pub(crate) fn destination_source(&self) -> Range<u64> {
        self.destination_source.clone()
    }

    pub(crate) fn title_source(&self) -> Option<Range<u64>> {
        self.title_source.clone()
    }

    pub(crate) fn cooked_destination(&self) -> &str {
        &self.cooked_destination
    }

    pub(crate) fn cooked_title(&self) -> Option<&str> {
        self.cooked_title.as_deref()
    }
}

/// Source-ordered direct facts and disjoint syntax ranges proven by the
/// bracket resolver.
///
/// Syntax ranges include only markers and tails owned by emitted facts.
/// Undefined, malformed, unmatched, and nested-link-invalidated brackets stay
/// literal and never enter this suppression map.
pub(crate) struct M11InlineDirectCandidates {
    source: SourceVersion,
    source_range: Range<u32>,
    facts: Vec<M11InlineDirectFact>,
    syntax: Vec<Range<u32>>,
    exhaustive_bracket_classification: bool,
}

impl fmt::Debug for M11InlineDirectCandidates {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineDirectCandidates")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("facts", &self.facts.len())
            .field("syntax", &self.syntax.len())
            .field(
                "exhaustive_bracket_classification",
                &self.exhaustive_bracket_classification,
            )
            .finish()
    }
}

impl M11InlineDirectCandidates {
    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    pub(crate) fn validate_source(
        &self,
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
    ) -> Result<(), M11InlineDirectError> {
        opaque.validate_source(runtime)?;
        if opaque.source() != self.source || opaque.source_range() != self.source_range {
            return Err(M11InlineDirectError::InvalidState);
        }
        Ok(())
    }

    pub(crate) fn len(&self) -> u32 {
        u32::try_from(self.facts.len()).expect("direct fact count was checked while building")
    }

    pub(crate) fn fact(&self, index: u32) -> Option<&M11InlineDirectFact> {
        self.facts.get(usize::try_from(index).ok()?)
    }

    pub(crate) fn syntax_range(&self, index: u32) -> Option<Range<u32>> {
        self.syntax.get(usize::try_from(index).ok()?).cloned()
    }

    pub(crate) fn syntax_ranges(&self) -> impl ExactSizeIterator<Item = Range<u32>> + '_ {
        self.syntax.iter().cloned()
    }

    pub(crate) fn fact_ranges(&self) -> impl ExactSizeIterator<Item = Range<u32>> + '_ {
        self.facts.iter().map(M11InlineDirectFact::source)
    }

    /// True only when every bracket candidate was classified against one
    /// definitive reference map without hitting the bounded tail-abandon
    /// fence. Undefined and malformed spellings are then proven literal.
    pub(crate) const fn exhaustive_bracket_classification(&self) -> bool {
        self.exhaustive_bracket_classification
    }

    /// Whether an already-resolved opaque candidate belongs to direct-link
    /// syntax rather than label content.
    pub(crate) fn suppresses_opaque(&self, candidate: M11InlineOpaqueCandidate) -> bool {
        self.intersects_syntax(candidate.relative_range())
    }

    pub(crate) fn intersects_syntax(&self, range: Range<u32>) -> bool {
        self.syntax
            .iter()
            .any(|syntax| syntax.start < range.end && range.start < syntax.end)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineDirectPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineDirectPoll {
    status: M11InlineDirectPollStatus,
    transitions: usize,
}

impl M11InlineDirectPoll {
    pub(crate) const fn status(self) -> M11InlineDirectPollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineDirectError {
    Opaque(M11InlineAutolinkError),
    Source(M11ParserRangeError),
    Cleaner(ReferenceValueCleanerError),
    Reference(M11ReferenceResolverError),
    ZeroFuel,
    PollLimitExceeded,
    CoordinateOverflow,
    InvalidUtf8,
    InvalidState,
}

impl fmt::Display for M11InlineDirectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opaque(error) => write!(formatter, "direct-link opaque map failed: {error}"),
            Self::Source(error) => write!(formatter, "direct-link source scan failed: {error}"),
            Self::Cleaner(error) => write!(formatter, "direct-link value cooking failed: {error}"),
            Self::Reference(error) => write!(formatter, "reference-link lookup failed: {error}"),
            Self::ZeroFuel => formatter.write_str("direct-link poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("direct-link poll exceeds its transition limit")
            }
            Self::CoordinateOverflow => {
                formatter.write_str("direct-link coordinate or counter overflow")
            }
            Self::InvalidUtf8 => formatter.write_str("direct-link cooked value is invalid UTF-8"),
            Self::InvalidState => formatter.write_str("direct-link job is in an invalid state"),
        }
    }
}

impl std::error::Error for M11InlineDirectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Opaque(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Cleaner(error) => Some(error),
            Self::Reference(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11InlineAutolinkError> for M11InlineDirectError {
    fn from(value: M11InlineAutolinkError) -> Self {
        Self::Opaque(value)
    }
}

impl From<M11ParserRangeError> for M11InlineDirectError {
    fn from(value: M11ParserRangeError) -> Self {
        Self::Source(value)
    }
}

impl From<ReferenceValueCleanerError> for M11InlineDirectError {
    fn from(value: ReferenceValueCleanerError) -> Self {
        Self::Cleaner(value)
    }
}

impl From<M11ReferenceResolverError> for M11InlineDirectError {
    fn from(value: M11ReferenceResolverError) -> Self {
        Self::Reference(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BracketKind {
    Link,
    Image,
}

#[derive(Debug)]
struct BracketOpener {
    kind: BracketKind,
    source_start: u32,
    label_start: u32,
    link_generation: u64,
    bracket_after: bool,
    literal_caret_start: bool,
    label: Option<StreamingReferenceLabel>,
}

impl BracketOpener {
    fn push_label_byte(&mut self, offset: u32, byte: u8) -> Result<(), M11InlineDirectError> {
        if offset == self.label_start {
            self.literal_caret_start = byte == b'^';
        }
        let Some(label) = self.label.as_mut() else {
            return Ok(());
        };
        if !label.push_byte(byte)? {
            self.label = None;
        }
        Ok(())
    }

    fn note_bracket_after(&mut self) {
        self.bracket_after = true;
        // The donor never falls back to the primary label after a nested
        // bracket, so stop retaining normalization state for that path.
        self.label = None;
    }

    fn normalized_primary_label(&mut self) -> Result<Option<String>, M11InlineDirectError> {
        let Some(label) = self.label.take() else {
            return Ok(None);
        };
        Ok(Some(label.finish()?))
    }
}

/// Constant-space UTF-8 framing around the normative label accumulator.
/// Every input byte is consumed by the surrounding scan transition; a
/// completed scalar performs only the pinned bounded case-fold expansion.
#[derive(Debug)]
struct StreamingReferenceLabel {
    accumulator: ReferenceLabelAccumulator,
    scalar: [u8; 4],
    scalar_len: u8,
    scalar_expected: u8,
}

impl StreamingReferenceLabel {
    fn new() -> Self {
        Self {
            accumulator: ReferenceLabelAccumulator::with_source_byte_hint(64),
            scalar: [0; 4],
            scalar_len: 0,
            scalar_expected: 0,
        }
    }

    /// Returns false only when the CommonMark 999-scalar envelope is
    /// exceeded. Invalid UTF-8 remains a hard source-authority failure.
    fn push_byte(&mut self, byte: u8) -> Result<bool, M11InlineDirectError> {
        if self.scalar_len == 0 {
            if byte.is_ascii() {
                return self.push_char(char::from(byte));
            }
            self.scalar_expected = match byte {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(M11InlineDirectError::InvalidUtf8),
            };
        } else if !matches!(byte, 0x80..=0xbf) {
            return Err(M11InlineDirectError::InvalidUtf8);
        }
        let index = usize::from(self.scalar_len);
        self.scalar[index] = byte;
        self.scalar_len += 1;
        if self.scalar_len != self.scalar_expected {
            return Ok(true);
        }
        let scalar = std::str::from_utf8(&self.scalar[..usize::from(self.scalar_len)])
            .map_err(|_| M11InlineDirectError::InvalidUtf8)?;
        let ch = scalar
            .chars()
            .next()
            .ok_or(M11InlineDirectError::InvalidUtf8)?;
        self.scalar_len = 0;
        self.scalar_expected = 0;
        self.push_char(ch)
    }

    fn push_char(&mut self, ch: char) -> Result<bool, M11InlineDirectError> {
        match self.accumulator.push(ch, 1) {
            Ok(()) => Ok(true),
            Err(ReferenceLabelAccumulatorError::TooLong) => Ok(false),
            Err(ReferenceLabelAccumulatorError::InvalidRawCodepointContribution(_)) => {
                Err(M11InlineDirectError::InvalidState)
            }
        }
    }

    fn finish(self) -> Result<String, M11InlineDirectError> {
        if self.scalar_len != 0 {
            return Err(M11InlineDirectError::InvalidUtf8);
        }
        Ok(self.accumulator.into_normalized())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TailPhase {
    ExpectOpen,
    LeadingSpace,
    AngleDestination,
    BareDestination,
    AfterDestination,
    Title,
    AfterTitle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TailStep {
    Pending,
    Matched(TailCuts),
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TailCuts {
    source_end: usize,
    destination_start: usize,
    destination_end: usize,
    title_start: Option<usize>,
    title_end: Option<usize>,
}

#[derive(Debug)]
struct TailScanner {
    phase: TailPhase,
    position: usize,
    destination_start: usize,
    destination_end: usize,
    title_start: Option<usize>,
    title_end: Option<usize>,
    bare_parentheses: u8,
    escaped: bool,
    title_delimiter: u8,
}

impl TailScanner {
    const fn new() -> Self {
        Self {
            phase: TailPhase::ExpectOpen,
            position: 0,
            destination_start: 0,
            destination_end: 0,
            title_start: None,
            title_end: None,
            bare_parentheses: 0,
            escaped: false,
            title_delimiter: 0,
        }
    }

    fn push(&mut self, byte: u8) -> TailStep {
        loop {
            let position = self.position;
            match self.phase {
                TailPhase::ExpectOpen => {
                    if byte != b'(' {
                        return TailStep::Failed;
                    }
                    self.position += 1;
                    self.phase = TailPhase::LeadingSpace;
                    return TailStep::Pending;
                }
                TailPhase::LeadingSpace => {
                    if is_spacechar(byte) {
                        self.position += 1;
                        return TailStep::Pending;
                    }
                    if byte == b'<' {
                        self.destination_start = position + 1;
                        self.position += 1;
                        self.phase = TailPhase::AngleDestination;
                        return TailStep::Pending;
                    }
                    self.destination_start = position;
                    self.phase = TailPhase::BareDestination;
                }
                TailPhase::AngleDestination => {
                    if self.escaped {
                        self.escaped = false;
                        self.position += 1;
                        return TailStep::Pending;
                    }
                    match byte {
                        b'\\' => {
                            self.escaped = true;
                            self.position += 1;
                            return TailStep::Pending;
                        }
                        b'>' => {
                            self.destination_end = position;
                            self.position += 1;
                            self.phase = TailPhase::AfterDestination;
                            return TailStep::Pending;
                        }
                        b'<' | b'\r' | b'\n' => return TailStep::Failed,
                        _ => {
                            self.position += 1;
                            return TailStep::Pending;
                        }
                    }
                }
                TailPhase::BareDestination => {
                    if self.escaped {
                        self.escaped = false;
                        if byte.is_ascii_punctuation() {
                            self.position += 1;
                            return TailStep::Pending;
                        }
                        // A backslash before non-punctuation is literal; the
                        // current byte still participates in destination
                        // grammar.
                    }
                    match byte {
                        b'\\' => {
                            self.escaped = true;
                            self.position += 1;
                            return TailStep::Pending;
                        }
                        b'(' => {
                            if self.bare_parentheses == 32 {
                                return TailStep::Failed;
                            }
                            self.bare_parentheses += 1;
                            self.position += 1;
                            return TailStep::Pending;
                        }
                        b')' if self.bare_parentheses != 0 => {
                            self.bare_parentheses -= 1;
                            self.position += 1;
                            return TailStep::Pending;
                        }
                        b')' => {
                            self.destination_end = position;
                            self.position += 1;
                            return TailStep::Matched(self.cuts());
                        }
                        byte if is_spacechar(byte) => {
                            if position == self.destination_start {
                                return TailStep::Failed;
                            }
                            self.destination_end = position;
                            self.position += 1;
                            self.phase = TailPhase::AfterDestination;
                            return TailStep::Pending;
                        }
                        byte if byte.is_ascii_control() && byte != 0 => {
                            return TailStep::Failed;
                        }
                        _ => {
                            self.position += 1;
                            return TailStep::Pending;
                        }
                    }
                }
                TailPhase::AfterDestination => {
                    if is_spacechar(byte) {
                        self.position += 1;
                        return TailStep::Pending;
                    }
                    if byte == b')' {
                        self.position += 1;
                        return TailStep::Matched(self.cuts());
                    }
                    if matches!(byte, b'\'' | b'"' | b'(') {
                        self.title_start = Some(position);
                        self.title_delimiter = if byte == b'(' { b')' } else { byte };
                        self.position += 1;
                        self.phase = TailPhase::Title;
                        return TailStep::Pending;
                    }
                    return TailStep::Failed;
                }
                TailPhase::Title => {
                    if self.escaped {
                        self.escaped = false;
                        self.position += 1;
                        return TailStep::Pending;
                    }
                    if byte == b'\\' {
                        self.escaped = true;
                        self.position += 1;
                        return TailStep::Pending;
                    }
                    if byte == self.title_delimiter {
                        self.position += 1;
                        self.title_end = Some(self.position);
                        self.phase = TailPhase::AfterTitle;
                        return TailStep::Pending;
                    }
                    if self.title_delimiter == b')' && byte == b'(' {
                        return TailStep::Failed;
                    }
                    self.position += 1;
                    return TailStep::Pending;
                }
                TailPhase::AfterTitle => {
                    if is_spacechar(byte) {
                        self.position += 1;
                        return TailStep::Pending;
                    }
                    if byte == b')' {
                        self.position += 1;
                        return TailStep::Matched(self.cuts());
                    }
                    return TailStep::Failed;
                }
            }
        }
    }

    const fn cuts(&self) -> TailCuts {
        TailCuts {
            source_end: self.position,
            destination_start: self.destination_start,
            destination_end: self.destination_end,
            title_start: self.title_start,
            title_end: self.title_end,
        }
    }
}

#[derive(Debug)]
struct TailAttempt {
    opener: BracketOpener,
    closer_start: u32,
    closer_end: u32,
    mode: TailAttemptMode,
    bytes: Vec<u8>,
}

impl TailAttempt {
    fn new(opener: BracketOpener, closer_start: u32, closer_end: u32) -> Self {
        Self {
            opener,
            closer_start,
            closer_end,
            mode: TailAttemptMode::Direct(TailScanner::new()),
            bytes: Vec::with_capacity(64),
        }
    }

    fn finish_explicit_label(&mut self) -> Result<String, M11InlineDirectError> {
        let mode = std::mem::replace(&mut self.mode, TailAttemptMode::Direct(TailScanner::new()));
        let TailAttemptMode::ExplicitReference(label) = mode else {
            return Err(M11InlineDirectError::InvalidState);
        };
        label.finish()
    }
}

#[derive(Debug)]
enum TailAttemptMode {
    Direct(TailScanner),
    ExplicitReference(ExplicitReferenceLabel),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExplicitReferenceStep {
    Pending,
    Complete,
    Failed,
}

#[derive(Debug)]
struct ExplicitReferenceLabel {
    label: Option<StreamingReferenceLabel>,
    pending_backslash: bool,
}

impl ExplicitReferenceLabel {
    fn new() -> Self {
        Self {
            label: Some(StreamingReferenceLabel::new()),
            pending_backslash: false,
        }
    }

    fn push(&mut self, byte: u8) -> Result<ExplicitReferenceStep, M11InlineDirectError> {
        if self.pending_backslash {
            self.pending_backslash = false;
            if !self.push_label_byte(byte)? {
                return Ok(ExplicitReferenceStep::Failed);
            }
            // ASCII punctuation is escaped by the preceding backslash. A
            // non-punctuation byte cannot be bracket syntax and needs no
            // further interpretation in this transition.
            return Ok(ExplicitReferenceStep::Pending);
        }
        match byte {
            b']' => Ok(ExplicitReferenceStep::Complete),
            b'[' => Ok(ExplicitReferenceStep::Failed),
            b'\\' => {
                if !self.push_label_byte(byte)? {
                    return Ok(ExplicitReferenceStep::Failed);
                }
                self.pending_backslash = true;
                Ok(ExplicitReferenceStep::Pending)
            }
            _ => {
                if self.push_label_byte(byte)? {
                    Ok(ExplicitReferenceStep::Pending)
                } else {
                    Ok(ExplicitReferenceStep::Failed)
                }
            }
        }
    }

    fn push_label_byte(&mut self, byte: u8) -> Result<bool, M11InlineDirectError> {
        let Some(label) = self.label.as_mut() else {
            return Ok(false);
        };
        label.push_byte(byte)
    }

    fn finish(mut self) -> Result<String, M11InlineDirectError> {
        self.label
            .take()
            .ok_or(M11InlineDirectError::InvalidState)?
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceForm {
    Full,
    Collapsed,
    Shortcut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CookValueKind {
    Destination,
    Title,
}

#[derive(Debug)]
struct PendingFact {
    opener: BracketOpener,
    closer_start: u32,
    closer_end: u32,
    tail: Vec<u8>,
    cuts: TailCuts,
    cook_kind: CookValueKind,
    cook_range: Range<usize>,
    cook_position: usize,
    cleaner: ReferenceValueBodyCleaner,
    cleaner_needs_input: bool,
    cleaner_finished: bool,
    cooked_destination: Vec<u8>,
    cooked_title: Vec<u8>,
}

impl PendingFact {
    fn new(attempt: TailAttempt, cuts: TailCuts) -> Result<Self, M11InlineDirectError> {
        let cook_range = cuts.destination_start..cuts.destination_end;
        Ok(Self {
            opener: attempt.opener,
            closer_start: attempt.closer_start,
            closer_end: attempt.closer_end,
            tail: attempt.bytes,
            cuts,
            cook_kind: CookValueKind::Destination,
            cook_position: cook_range.start,
            cook_range,
            cleaner: ReferenceValueBodyCleaner::new(),
            cleaner_needs_input: true,
            cleaner_finished: false,
            cooked_destination: Vec::new(),
            cooked_title: Vec::new(),
        })
    }

    fn begin_title(&mut self) -> Result<bool, M11InlineDirectError> {
        let (Some(start), Some(end)) = (self.cuts.title_start, self.cuts.title_end) else {
            return Ok(false);
        };
        let raw = self
            .tail
            .get(start..end)
            .ok_or(M11InlineDirectError::InvalidState)?;
        let body = clean_title_body_range(raw.len(), raw.first().copied(), raw.last().copied());
        self.cook_kind = CookValueKind::Title;
        self.cook_range = start + body.start..start + body.end;
        self.cook_position = self.cook_range.start;
        self.cleaner = ReferenceValueBodyCleaner::new();
        self.cleaner_needs_input = true;
        self.cleaner_finished = false;
        Ok(true)
    }

    fn cooked_mut(&mut self) -> &mut Vec<u8> {
        match self.cook_kind {
            CookValueKind::Destination => &mut self.cooked_destination,
            CookValueKind::Title => &mut self.cooked_title,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DirectPhase {
    Scanning,
    Cooking,
    OrderingFacts,
    OrderingSyntax,
    Complete,
    Faulted,
    Cancelled,
    Transferred,
}

enum M11InlineReferenceResolver {
    Persistent(M11ReferenceResolver),
    #[cfg(any(test, feature = "m11-compact-probe"))]
    Compact(M11CompactReferenceResolver),
}

impl M11InlineReferenceResolver {
    fn resolve(
        &self,
        runtime: &DocumentRuntime,
        normalized_label: &str,
        maximum_cooked_bytes: usize,
    ) -> Result<M11ReferenceResolution, M11InlineDirectError> {
        match self {
            Self::Persistent(resolver) => {
                Ok(resolver.resolve(runtime, normalized_label, maximum_cooked_bytes)?)
            }
            #[cfg(any(test, feature = "m11-compact-probe"))]
            Self::Compact(resolver) => resolver
                .resolve(runtime, normalized_label, maximum_cooked_bytes)
                .map_err(|_| M11InlineDirectError::InvalidState),
        }
    }
}

/// Resumable direct-link/image derivation over one parser-authenticated leaf.
pub(crate) struct M11InlineDirectJob {
    source: SourceVersion,
    source_range: Range<u32>,
    cursor: M11ParserRangeCursor,
    window: [u8; M11_PARSER_RANGE_MAX_POLL_BYTES],
    window_position: usize,
    window_len: usize,
    source_eof: bool,
    source_offset: u32,
    replay: VecDeque<(u32, u8)>,
    opaque_index: u32,
    openers: Vec<BracketOpener>,
    link_generation: u64,
    pending_backslash: bool,
    pending_image_bang: Option<u32>,
    attempt: Option<TailAttempt>,
    pending_fact: Option<PendingFact>,
    reference_resolver: Option<M11InlineReferenceResolver>,
    exhaustive_bracket_classification: bool,
    facts_by_order: BTreeMap<(u32, Reverse<u32>), M11InlineDirectFact>,
    syntax_by_start: BTreeMap<u32, u32>,
    ordered_facts: Vec<M11InlineDirectFact>,
    ordered_syntax: Vec<Range<u32>>,
    phase: DirectPhase,
}

impl M11InlineDirectJob {
    pub(crate) fn new(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
    ) -> Result<Self, M11InlineDirectError> {
        Self::new_inner(runtime, opaque, None)
    }

    pub(crate) fn new_with_reference_resolver(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        reference_resolver: M11ReferenceResolver,
    ) -> Result<Self, M11InlineDirectError> {
        // Empty labels can never win, but this bounded probe authenticates the
        // move-only resolver against the same live document actor now rather
        // than deferring a wrong-runtime error until the first reference.
        let _ = reference_resolver.resolve(runtime, "", 0)?;
        Self::new_inner(
            runtime,
            opaque,
            Some(M11InlineReferenceResolver::Persistent(reference_resolver)),
        )
    }

    #[cfg(any(test, feature = "m11-compact-probe"))]
    pub(crate) fn new_with_compact_reference_resolver(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        reference_resolver: M11CompactReferenceResolver,
    ) -> Result<Self, M11InlineDirectError> {
        let _ = reference_resolver
            .resolve(runtime, "", 0)
            .map_err(|_| M11InlineDirectError::InvalidState)?;
        Self::new_inner(
            runtime,
            opaque,
            Some(M11InlineReferenceResolver::Compact(reference_resolver)),
        )
    }

    fn new_inner(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        reference_resolver: Option<M11InlineReferenceResolver>,
    ) -> Result<Self, M11InlineDirectError> {
        opaque.validate_source(runtime)?;
        // Start optimistic and revoke only when this leaf actually contains a
        // valid reference-shaped use that needs unavailable global winners,
        // exceeds the bounded value lane, or crosses the tail cap. This keeps
        // direct-only leaves definitive without pretending `[label]` is
        // literal merely because no resolver was supplied.
        let exhaustive_bracket_classification = true;
        Ok(Self {
            source: opaque.source(),
            source_range: opaque.source_range(),
            cursor: opaque.source_cursor(runtime)?,
            window: [0; M11_PARSER_RANGE_MAX_POLL_BYTES],
            window_position: 0,
            window_len: 0,
            source_eof: false,
            source_offset: 0,
            replay: VecDeque::new(),
            opaque_index: 0,
            openers: Vec::new(),
            link_generation: 0,
            pending_backslash: false,
            pending_image_bang: None,
            attempt: None,
            pending_fact: None,
            reference_resolver,
            exhaustive_bracket_classification,
            facts_by_order: BTreeMap::new(),
            syntax_by_start: BTreeMap::new(),
            ordered_facts: Vec::new(),
            ordered_syntax: Vec::new(),
            phase: DirectPhase::Scanning,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        fuel: usize,
    ) -> Result<M11InlineDirectPoll, M11InlineDirectError> {
        validate_fuel(fuel)?;
        if self.phase == DirectPhase::Complete {
            return Ok(M11InlineDirectPoll {
                status: M11InlineDirectPollStatus::Complete,
                transitions: 0,
            });
        }
        if !matches!(
            self.phase,
            DirectPhase::Scanning
                | DirectPhase::Cooking
                | DirectPhase::OrderingFacts
                | DirectPhase::OrderingSyntax
        ) {
            return Err(M11InlineDirectError::InvalidState);
        }
        opaque.validate_source(runtime)?;
        if opaque.source() != self.source || opaque.source_range() != self.source_range {
            return Err(M11InlineDirectError::InvalidState);
        }

        let mut transitions = 0;
        while transitions < fuel {
            let result = match self.phase {
                DirectPhase::Scanning => {
                    self.poll_scanning(runtime, opaque, fuel, &mut transitions)
                }
                DirectPhase::Cooking => self.poll_cooking(&mut transitions),
                DirectPhase::OrderingFacts => self.poll_order_facts(&mut transitions),
                DirectPhase::OrderingSyntax => self.poll_order_syntax(&mut transitions),
                DirectPhase::Complete => break,
                _ => Err(M11InlineDirectError::InvalidState),
            };
            if let Err(error) = result {
                self.cursor.cancel();
                self.phase = DirectPhase::Faulted;
                return Err(error);
            }
            if self.phase == DirectPhase::Complete {
                return Ok(M11InlineDirectPoll {
                    status: M11InlineDirectPollStatus::Complete,
                    transitions,
                });
            }
        }

        Ok(M11InlineDirectPoll {
            status: M11InlineDirectPollStatus::Pending,
            transitions,
        })
    }

    fn poll_scanning(
        &mut self,
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineDirectError> {
        if let Some((offset, byte)) = self.replay.pop_front() {
            self.process_stream_byte(runtime, opaque, offset, byte)?;
            *transitions += 1;
            return Ok(());
        }
        if self.window_position < self.window_len {
            let byte = self.window[self.window_position];
            self.window_position += 1;
            let offset = self.source_offset;
            self.source_offset = self
                .source_offset
                .checked_add(1)
                .ok_or(M11InlineDirectError::CoordinateOverflow)?;
            self.process_stream_byte(runtime, opaque, offset, byte)?;
            *transitions += 1;
            return Ok(());
        }
        if self.source_eof {
            if let Some(attempt) = self.attempt.take() {
                self.finish_incomplete_attempt(runtime, attempt)?;
                *transitions += 1;
                return Ok(());
            }
            self.pending_backslash = false;
            self.pending_image_bang = None;
            self.phase = DirectPhase::OrderingFacts;
            *transitions += 1;
            return Ok(());
        }

        let poll = self.cursor.poll(fuel - *transitions, &mut self.window)?;
        self.window_position = 0;
        self.window_len = poll.bytes_read();
        self.source_eof = poll.status() == M11ParserRangeStatus::Complete;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        if self.window_len == 0 && !self.source_eof {
            return Err(M11InlineDirectError::InvalidState);
        }
        Ok(())
    }

    fn process_stream_byte(
        &mut self,
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        offset: u32,
        byte: u8,
    ) -> Result<(), M11InlineDirectError> {
        if self.attempt.is_some() {
            return self.process_attempt_byte(runtime, offset, byte);
        }

        while opaque
            .candidate(self.opaque_index)?
            .is_some_and(|candidate| candidate.relative_range().end <= offset)
        {
            self.opaque_index = self
                .opaque_index
                .checked_add(1)
                .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        }
        if let Some(candidate) = opaque.candidate(self.opaque_index)? {
            if candidate.relative_range().contains(&offset) {
                self.push_primary_label_byte(offset, byte)?;
                if candidate.relative_range().start == offset
                    && matches!(
                        candidate.kind(),
                        M11InlineOpaqueKind::AutolinkUri | M11InlineOpaqueKind::AutolinkEmail
                    )
                {
                    self.link_generation = self
                        .link_generation
                        .checked_add(1)
                        .ok_or(M11InlineDirectError::CoordinateOverflow)?;
                }
                self.pending_backslash = false;
                self.pending_image_bang = None;
                return Ok(());
            }
        }

        if self.pending_backslash {
            self.pending_backslash = false;
            self.pending_image_bang = None;
            if byte.is_ascii_punctuation() {
                self.push_primary_label_byte(offset, byte)?;
                return Ok(());
            }
        }
        if byte == b'\\' {
            self.push_primary_label_byte(offset, byte)?;
            self.pending_backslash = true;
            self.pending_image_bang = None;
            return Ok(());
        }

        match byte {
            b'[' => {
                self.push_primary_label_byte(offset, byte)?;
                if let Some(opener) = self.openers.last_mut() {
                    opener.note_bracket_after();
                }
                let (kind, source_start) = self
                    .pending_image_bang
                    .filter(|bang| bang.checked_add(1) == Some(offset))
                    .map_or((BracketKind::Link, offset), |bang| {
                        (BracketKind::Image, bang)
                    });
                self.openers.push(BracketOpener {
                    kind,
                    source_start,
                    label_start: offset
                        .checked_add(1)
                        .ok_or(M11InlineDirectError::CoordinateOverflow)?,
                    link_generation: self.link_generation,
                    bracket_after: false,
                    literal_caret_start: false,
                    label: Some(StreamingReferenceLabel::new()),
                });
                self.pending_image_bang = None;
            }
            b']' => {
                self.pending_image_bang = None;
                let Some(opener) = self.openers.pop() else {
                    return Ok(());
                };
                let closer_end = offset
                    .checked_add(1)
                    .ok_or(M11InlineDirectError::CoordinateOverflow)?;
                if opener.kind == BracketKind::Link
                    && opener.link_generation != self.link_generation
                {
                    // The nested link wins. The rejected outer brackets are
                    // literal source, not syntax owned by any emitted fact.
                    return Ok(());
                }
                self.attempt = Some(TailAttempt::new(opener, offset, closer_end));
            }
            b'!' => {
                self.push_primary_label_byte(offset, byte)?;
                self.pending_image_bang = Some(offset);
            }
            _ => {
                self.push_primary_label_byte(offset, byte)?;
                self.pending_image_bang = None;
            }
        }
        Ok(())
    }

    fn push_primary_label_byte(
        &mut self,
        offset: u32,
        byte: u8,
    ) -> Result<(), M11InlineDirectError> {
        if let Some(opener) = self.openers.last_mut() {
            opener.push_label_byte(offset, byte)?;
        }
        Ok(())
    }

    fn process_attempt_byte(
        &mut self,
        runtime: &DocumentRuntime,
        offset: u32,
        byte: u8,
    ) -> Result<(), M11InlineDirectError> {
        let attempt = self
            .attempt
            .as_mut()
            .ok_or(M11InlineDirectError::InvalidState)?;
        let expected = attempt
            .closer_end
            .checked_add(
                u32::try_from(attempt.bytes.len())
                    .map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        if offset != expected {
            return Err(M11InlineDirectError::InvalidState);
        }
        if attempt.bytes.len() == DIRECT_TAIL_MAX_BYTES {
            // The donor facade is intentionally bounded. Preserve correctness
            // by leaving this opener unresolved; the subsequent hazard gate
            // will fail the leaf closed.
            self.cursor.cancel();
            self.window_position = self.window_len;
            self.source_eof = true;
            self.replay.clear();
            self.exhaustive_bracket_classification = false;
            self.attempt = None;
            return Ok(());
        }
        attempt.bytes.push(byte);
        match &mut attempt.mode {
            TailAttemptMode::Direct(scanner) => match scanner.push(byte) {
                TailStep::Pending => Ok(()),
                TailStep::Failed if attempt.bytes.first() == Some(&b'[') => {
                    attempt.mode =
                        TailAttemptMode::ExplicitReference(ExplicitReferenceLabel::new());
                    Ok(())
                }
                TailStep::Failed => {
                    let attempt = self
                        .attempt
                        .take()
                        .ok_or(M11InlineDirectError::InvalidState)?;
                    self.resolve_shortcut_or_replay(runtime, attempt)
                }
                TailStep::Matched(cuts) => {
                    let attempt = self
                        .attempt
                        .take()
                        .ok_or(M11InlineDirectError::InvalidState)?;
                    self.pending_fact = Some(PendingFact::new(attempt, cuts)?);
                    self.phase = DirectPhase::Cooking;
                    Ok(())
                }
            },
            TailAttemptMode::ExplicitReference(label) => match label.push(byte)? {
                ExplicitReferenceStep::Pending => Ok(()),
                ExplicitReferenceStep::Failed => {
                    let attempt = self
                        .attempt
                        .take()
                        .ok_or(M11InlineDirectError::InvalidState)?;
                    self.resolve_shortcut_or_replay(runtime, attempt)
                }
                ExplicitReferenceStep::Complete => {
                    let mut attempt = self
                        .attempt
                        .take()
                        .ok_or(M11InlineDirectError::InvalidState)?;
                    let explicit_label = attempt.finish_explicit_label()?;
                    let source_end = offset
                        .checked_add(1)
                        .ok_or(M11InlineDirectError::CoordinateOverflow)?;
                    if explicit_label.is_empty() {
                        self.resolve_collapsed_or_replay(runtime, attempt, source_end)
                    } else {
                        self.resolve_reference_or_replay(
                            runtime,
                            attempt,
                            ReferenceForm::Full,
                            Some(explicit_label),
                            source_end,
                        )
                    }
                }
            },
        }
    }

    fn finish_incomplete_attempt(
        &mut self,
        runtime: &DocumentRuntime,
        attempt: TailAttempt,
    ) -> Result<(), M11InlineDirectError> {
        self.resolve_shortcut_or_replay(runtime, attempt)
    }

    fn resolve_shortcut_or_replay(
        &mut self,
        runtime: &DocumentRuntime,
        mut attempt: TailAttempt,
    ) -> Result<(), M11InlineDirectError> {
        let normalized_label = if !attempt.opener.bracket_after {
            attempt.opener.normalized_primary_label()?
        } else {
            None
        };
        let source_end = attempt.closer_end;
        self.resolve_reference_or_replay(
            runtime,
            attempt,
            ReferenceForm::Shortcut,
            normalized_label,
            source_end,
        )
    }

    fn resolve_collapsed_or_replay(
        &mut self,
        runtime: &DocumentRuntime,
        mut attempt: TailAttempt,
        source_end: u32,
    ) -> Result<(), M11InlineDirectError> {
        let normalized_label = if !attempt.opener.bracket_after {
            attempt.opener.normalized_primary_label()?
        } else {
            None
        };
        self.resolve_reference_or_replay(
            runtime,
            attempt,
            ReferenceForm::Collapsed,
            normalized_label,
            source_end,
        )
    }

    fn resolve_reference_or_replay(
        &mut self,
        runtime: &DocumentRuntime,
        attempt: TailAttempt,
        form: ReferenceForm,
        normalized_label: Option<String>,
        source_end: u32,
    ) -> Result<(), M11InlineDirectError> {
        let footnote_shortcut = form == ReferenceForm::Shortcut
            && attempt.opener.kind == BracketKind::Link
            && attempt.opener.literal_caret_start;
        let resolution = if !footnote_shortcut {
            match (
                normalized_label.filter(|label| !label.is_empty()),
                self.reference_resolver.as_ref(),
            ) {
                (Some(label), Some(resolver)) => resolver.resolve(
                    runtime,
                    &label,
                    M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES.saturating_sub(32),
                )?,
                (Some(_), None) => {
                    self.exhaustive_bracket_classification = false;
                    M11ReferenceResolution::Missing
                }
                (None, _) => M11ReferenceResolution::Missing,
            }
        } else {
            M11ReferenceResolution::Missing
        };
        let resolved = match resolution {
            M11ReferenceResolution::Missing => {
                // A complete but undefined explicit label must not fall back
                // to the primary label. Its tail still remains ordinary
                // Markdown, though: replaying it lets a following
                // label/direct tail bind to that later opener (CommonMark
                // examples 569-571).
                return self.replay_attempt(attempt);
            }
            M11ReferenceResolution::Unknown => {
                // Committed-prefix authority cannot prove this label absent:
                // a later definition may still bind it, so literal-text facts
                // here would be falsifiable by the suffix. Revoke the whole-
                // leaf bracket certificate and fail the leaf closed.
                self.exhaustive_bracket_classification = false;
                return self.replay_attempt(attempt);
            }
            M11ReferenceResolution::ValueTooLarge => {
                // This is a real reference, not literal text. The bounded
                // companion lane cannot represent it, so revoke the whole-
                // leaf bracket certificate and let the hazard gate fail
                // closed after preserving source-scanner progress.
                self.exhaustive_bracket_classification = false;
                return self.replay_attempt(attempt);
            }
            M11ReferenceResolution::Resolved(resolved) => resolved,
        };
        self.commit_reference_fact(&attempt, source_end, resolved)?;
        if form == ReferenceForm::Shortcut {
            self.replay_attempt(attempt)?;
        }
        Ok(())
    }

    fn commit_reference_fact(
        &mut self,
        attempt: &TailAttempt,
        source_end: u32,
        resolved: flark_engine::parser_internal::M11ResolvedReference,
    ) -> Result<(), M11InlineDirectError> {
        let source = attempt.opener.source_start..source_end;
        let fact = M11InlineDirectFact {
            kind: match attempt.opener.kind {
                BracketKind::Link => M11InlineDirectKind::ReferenceLink,
                BracketKind::Image => M11InlineDirectKind::ReferenceImage,
            },
            source: source.clone(),
            label_source: attempt.opener.label_start..attempt.closer_start,
            destination_source: resolved.destination_source().clone(),
            title_source: resolved.title_source().cloned(),
            cooked_destination: resolved.cooked_destination().to_owned().into_boxed_str(),
            cooked_title: resolved
                .cooked_title()
                .map(|title| title.to_owned().into_boxed_str()),
        };
        let key = (source.start, Reverse(source.end));
        if self.facts_by_order.insert(key, fact).is_some() {
            return Err(M11InlineDirectError::InvalidState);
        }
        self.insert_syntax(attempt.opener.source_start..attempt.opener.label_start)?;
        self.insert_syntax(attempt.closer_start..attempt.closer_end)?;
        if source_end > attempt.closer_end {
            self.insert_syntax(attempt.closer_end..source_end)?;
        }
        if attempt.opener.kind == BracketKind::Link {
            self.link_generation = self
                .link_generation
                .checked_add(1)
                .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        }
        Ok(())
    }

    fn replay_attempt(&mut self, attempt: TailAttempt) -> Result<(), M11InlineDirectError> {
        for (index, byte) in attempt.bytes.into_iter().enumerate().rev() {
            let offset = attempt
                .closer_end
                .checked_add(
                    u32::try_from(index).map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
                )
                .ok_or(M11InlineDirectError::CoordinateOverflow)?;
            self.replay.push_front((offset, byte));
        }
        Ok(())
    }

    fn poll_cooking(&mut self, transitions: &mut usize) -> Result<(), M11InlineDirectError> {
        let pending = self
            .pending_fact
            .as_mut()
            .ok_or(M11InlineDirectError::InvalidState)?;
        if pending.cleaner_needs_input && pending.cook_position < pending.cook_range.end {
            let byte = *pending
                .tail
                .get(pending.cook_position)
                .ok_or(M11InlineDirectError::InvalidState)?;
            pending.cleaner.offer_byte(byte)?;
            pending.cook_position += 1;
            pending.cleaner_needs_input = false;
            *transitions += 1;
            return Ok(());
        }
        if pending.cleaner_needs_input && !pending.cleaner_finished {
            pending.cleaner.finish_input()?;
            pending.cleaner_needs_input = false;
            pending.cleaner_finished = true;
            *transitions += 1;
            return Ok(());
        }

        match pending.cleaner.poll()? {
            ReferenceValueCleanerStatus::Progress => {}
            ReferenceValueCleanerStatus::NeedInput => pending.cleaner_needs_input = true,
            ReferenceValueCleanerStatus::OutputReady => {
                let chunk = pending.cleaner.take_output()?;
                pending.cooked_mut().extend_from_slice(chunk.bytes());
            }
            ReferenceValueCleanerStatus::Complete => {
                if pending.cook_kind == CookValueKind::Destination && pending.begin_title()? {
                    *transitions += 1;
                    return Ok(());
                }
                self.commit_pending_fact()?;
                self.phase = DirectPhase::Scanning;
            }
        }
        *transitions += 1;
        Ok(())
    }

    fn commit_pending_fact(&mut self) -> Result<(), M11InlineDirectError> {
        let pending = self
            .pending_fact
            .take()
            .ok_or(M11InlineDirectError::InvalidState)?;
        let tail_start = pending.closer_end;
        let tail_end = tail_start
            .checked_add(
                u32::try_from(pending.cuts.source_end)
                    .map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        let destination_start = tail_start
            .checked_add(
                u32::try_from(pending.cuts.destination_start)
                    .map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        let destination_end = tail_start
            .checked_add(
                u32::try_from(pending.cuts.destination_end)
                    .map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        let title_source = match (pending.cuts.title_start, pending.cuts.title_end) {
            (Some(start), Some(end)) => Some(
                tail_start
                    .checked_add(
                        u32::try_from(start)
                            .map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
                    )
                    .ok_or(M11InlineDirectError::CoordinateOverflow)?
                    ..tail_start
                        .checked_add(
                            u32::try_from(end)
                                .map_err(|_| M11InlineDirectError::CoordinateOverflow)?,
                        )
                        .ok_or(M11InlineDirectError::CoordinateOverflow)?,
            ),
            (None, None) => None,
            _ => return Err(M11InlineDirectError::InvalidState),
        };
        let cooked_destination = String::from_utf8(pending.cooked_destination)
            .map_err(|_| M11InlineDirectError::InvalidUtf8)?
            .into_boxed_str();
        let cooked_title = title_source
            .is_some()
            .then(|| {
                String::from_utf8(pending.cooked_title)
                    .map(String::into_boxed_str)
                    .map_err(|_| M11InlineDirectError::InvalidUtf8)
            })
            .transpose()?;
        let source = pending.opener.source_start..tail_end;
        let fact = M11InlineDirectFact {
            kind: match pending.opener.kind {
                BracketKind::Link => M11InlineDirectKind::Link,
                BracketKind::Image => M11InlineDirectKind::Image,
            },
            source: source.clone(),
            label_source: pending.opener.label_start..pending.closer_start,
            destination_source: u64::from(destination_start)..u64::from(destination_end),
            title_source: title_source.map(|range| u64::from(range.start)..u64::from(range.end)),
            cooked_destination,
            cooked_title,
        };
        let key = (source.start, Reverse(source.end));
        if self.facts_by_order.insert(key, fact).is_some() {
            return Err(M11InlineDirectError::InvalidState);
        }
        self.insert_syntax(pending.opener.source_start..pending.opener.label_start)?;
        self.insert_syntax(pending.closer_start..pending.closer_end)?;
        self.insert_syntax(pending.closer_end..tail_end)?;
        if pending.opener.kind == BracketKind::Link {
            self.link_generation = self
                .link_generation
                .checked_add(1)
                .ok_or(M11InlineDirectError::CoordinateOverflow)?;
        }
        Ok(())
    }

    fn insert_syntax(&mut self, range: Range<u32>) -> Result<(), M11InlineDirectError> {
        if range.start >= range.end {
            return Err(M11InlineDirectError::InvalidState);
        }
        if self
            .syntax_by_start
            .range(..=range.start)
            .next_back()
            .is_some_and(|(_, end)| *end > range.start)
            || self
                .syntax_by_start
                .range(range.start..)
                .next()
                .is_some_and(|(start, _)| *start < range.end)
        {
            return Err(M11InlineDirectError::InvalidState);
        }
        self.syntax_by_start.insert(range.start, range.end);
        Ok(())
    }

    fn poll_order_facts(&mut self, transitions: &mut usize) -> Result<(), M11InlineDirectError> {
        if let Some((_, fact)) = self.facts_by_order.pop_first() {
            self.ordered_facts.push(fact);
            *transitions += 1;
        } else {
            self.phase = DirectPhase::OrderingSyntax;
            *transitions += 1;
        }
        Ok(())
    }

    fn poll_order_syntax(&mut self, transitions: &mut usize) -> Result<(), M11InlineDirectError> {
        if let Some((start, end)) = self.syntax_by_start.pop_first() {
            self.ordered_syntax.push(start..end);
            *transitions += 1;
        } else {
            self.phase = DirectPhase::Complete;
            *transitions += 1;
        }
        Ok(())
    }

    pub(crate) fn take_output(&mut self) -> Option<M11InlineDirectCandidates> {
        if self.phase != DirectPhase::Complete {
            return None;
        }
        self.phase = DirectPhase::Transferred;
        Some(M11InlineDirectCandidates {
            source: self.source,
            source_range: self.source_range.clone(),
            facts: std::mem::take(&mut self.ordered_facts),
            syntax: std::mem::take(&mut self.ordered_syntax),
            exhaustive_bracket_classification: self.exhaustive_bracket_classification,
        })
    }

    pub(crate) fn cancel(&mut self) {
        if matches!(
            self.phase,
            DirectPhase::Scanning
                | DirectPhase::Cooking
                | DirectPhase::OrderingFacts
                | DirectPhase::OrderingSyntax
                | DirectPhase::Faulted
        ) {
            self.cursor.cancel();
            self.phase = DirectPhase::Cancelled;
        }
    }
}

impl Drop for M11InlineDirectJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    DirectPhase::Cancelled | DirectPhase::Transferred
                ),
                "direct-link jobs require output transfer or explicit cancellation"
            );
        }
    }
}

const fn is_spacechar(byte: u8) -> bool {
    matches!(byte, b'\t'..=b'\r' | b' ')
}

fn validate_fuel(fuel: usize) -> Result<(), M11InlineDirectError> {
    if fuel == 0 {
        return Err(M11InlineDirectError::ZeroFuel);
    }
    if fuel > M11_INLINE_DIRECT_MAX_POLL_TRANSITIONS {
        return Err(M11InlineDirectError::PollLimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_autolink::{
        M11InlineAutolinkJob, M11InlineAutolinkPollStatus, M11InlineOpaquePollStatus,
        M11InlineOpaqueResolveJob,
    };
    use crate::inline_code::{M11InlineCodeJob, M11InlineCodePollStatus};
    use flark_engine::parser_internal::{
        M11ParserSourceRangeAuthority, M11ReferenceJournal, M11ReferenceJournalOccurrence,
        M11ReferenceJournalRange, M11ReferenceJournalRoot, M11ReferenceJournalStatus,
    };
    use flark_engine::DocumentRuntimeConfig;

    struct Fixture {
        runtime: DocumentRuntime,
        code_job: M11InlineCodeJob,
        autolink_job: M11InlineAutolinkJob,
        opaque: M11InlineOpaqueCandidates,
        reference_resolver: Option<M11ReferenceResolver>,
        reference_root: Option<M11ReferenceJournalRoot>,
    }

    impl Fixture {
        fn new(source: &str) -> Self {
            let mut runtime =
                DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
            let authority = M11ParserSourceRangeAuthority::new(
                &runtime,
                runtime.snapshot_current_source().expect("source lease"),
                0..source.len(),
            )
            .expect("source authority");
            let mut code_job = M11InlineCodeJob::new(&runtime, authority).expect("code job");
            loop {
                if code_job
                    .poll(&mut runtime, M11_INLINE_DIRECT_MAX_POLL_TRANSITIONS)
                    .expect("code poll")
                    .status()
                    == M11InlineCodePollStatus::Complete
                {
                    break;
                }
            }
            let code = code_job.take_output().expect("code output");
            let mut autolink_job =
                M11InlineAutolinkJob::new(&runtime, &code).expect("autolink job");
            loop {
                if autolink_job
                    .poll(&mut runtime, M11_INLINE_DIRECT_MAX_POLL_TRANSITIONS)
                    .expect("autolink poll")
                    .status()
                    == M11InlineAutolinkPollStatus::Complete
                {
                    break;
                }
            }
            let mut code = Some(code);
            let mut opaque_job =
                M11InlineOpaqueResolveJob::take_new(&runtime, &mut code, &mut autolink_job)
                    .expect("opaque resolver");
            loop {
                if opaque_job
                    .poll(&mut runtime, M11_INLINE_DIRECT_MAX_POLL_TRANSITIONS)
                    .expect("opaque poll")
                    .status()
                    == M11InlineOpaquePollStatus::Complete
                {
                    break;
                }
            }
            let opaque = opaque_job.take_output().expect("opaque output");
            drop(opaque_job);
            Self {
                runtime,
                code_job,
                autolink_job,
                opaque,
                reference_resolver: None,
                reference_root: None,
            }
        }

        fn with_reference_records(
            source: &str,
            records: Vec<M11ReferenceJournalOccurrence>,
        ) -> Self {
            let mut fixture = Self::new(source);
            let source_version = fixture
                .runtime
                .current_source_version()
                .expect("current source");
            let mut journal = M11ReferenceJournal::new(&mut fixture.runtime, source_version, 1)
                .expect("reference journal");
            for record in records {
                journal
                    .offer_occurrence(&fixture.runtime, record)
                    .expect("reference occurrence");
                loop {
                    let poll = journal
                        .poll(&mut fixture.runtime, 64)
                        .expect("drain reference occurrence");
                    assert!(poll.transitions() <= 64);
                    if poll.status() == M11ReferenceJournalStatus::NeedsInput {
                        break;
                    }
                    assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
                }
            }
            journal
                .finish_input(&fixture.runtime)
                .expect("finish reference journal");
            loop {
                let poll = journal
                    .poll(&mut fixture.runtime, 64)
                    .expect("seal reference journal");
                assert!(poll.transitions() <= 64);
                if poll.status() == M11ReferenceJournalStatus::Complete {
                    break;
                }
                assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
            }
            let root = journal.take_root().expect("reference journal root");
            fixture.reference_resolver = Some(
                M11ReferenceResolver::from_live_reference_journal(&fixture.runtime, &root)
                    .expect("live reference resolver"),
            );
            fixture.reference_root = Some(root);
            fixture
        }

        fn resolve(&mut self, fuel: usize) -> M11InlineDirectCandidates {
            let mut job = if let Some(resolver) = self.reference_resolver.as_ref() {
                M11InlineDirectJob::new_with_reference_resolver(
                    &self.runtime,
                    &self.opaque,
                    resolver.clone(),
                )
                .expect("reference-aware direct job")
            } else {
                M11InlineDirectJob::new(&self.runtime, &self.opaque).expect("direct job")
            };
            loop {
                let poll = job
                    .poll(&self.runtime, &self.opaque, fuel)
                    .expect("direct poll");
                assert!(poll.transitions() <= fuel);
                if poll.status() == M11InlineDirectPollStatus::Complete {
                    break;
                }
                assert_ne!(poll.transitions(), 0);
            }
            let output = job.take_output().expect("direct output");
            drop(job);
            output
        }

        fn close(mut self) {
            self.opaque.begin_release().expect("begin opaque release");
            loop {
                if self
                    .opaque
                    .poll_release(&mut self.runtime, 1)
                    .expect("opaque release")
                    .complete()
                {
                    break;
                }
            }
            drop(self.opaque);
            drop(self.autolink_job);
            drop(self.code_job);
            drop(self.reference_resolver.take());
            if let Some(mut root) = self.reference_root.take() {
                root.begin_release(&mut self.runtime)
                    .expect("begin reference root release");
                while !root
                    .poll_release(&mut self.runtime, 1)
                    .expect("poll reference root release")
                    .complete()
                {}
            }
            self.runtime.begin_close().expect("begin runtime close");
            while !self.runtime.poll_close(64).expect("runtime close").complete {}
            assert_eq!(
                self.runtime.arena_metrics().reserved_external_payload_bytes,
                0
            );
        }
    }

    fn resolve(source: &str, fuel: usize) -> M11InlineDirectCandidates {
        let mut fixture = Fixture::new(source);
        let output = fixture.resolve(fuel);
        fixture.close();
        output
    }

    fn source_range(range: Range<u64>) -> M11ReferenceJournalRange {
        M11ReferenceJournalRange::new(range.clone(), range)
    }

    fn reference_record(
        slot: u64,
        normalized_label: &str,
        cooked_destination: &str,
        cooked_title: Option<&str>,
    ) -> M11ReferenceJournalOccurrence {
        let start = slot * 12;
        let source = start..start + 12;
        let label = start..start + 3;
        let destination = start + 3..if cooked_title.is_some() {
            start + 7
        } else {
            start + 12
        };
        let title = cooked_title.map(|_| start + 7..start + 12);
        M11ReferenceJournalOccurrence::new(
            source_range(source),
            source_range(label),
            source_range(destination),
            title.map(source_range),
            normalized_label.as_bytes(),
            cooked_destination.as_bytes(),
            cooked_title.map(|title| Box::<[u8]>::from(title.as_bytes())),
        )
    }

    fn reference_records() -> Vec<M11ReferenceJournalOccurrence> {
        vec![
            reference_record(0, "foo", "/foo", Some("title")),
            reference_record(1, "bar", "/bar", None),
            reference_record(2, "^foo", "/footnote", None),
        ]
    }

    fn resolve_references(
        source: &str,
        records: Vec<M11ReferenceJournalOccurrence>,
        fuel: usize,
    ) -> M11InlineDirectCandidates {
        let mut fixture = Fixture::with_reference_records(source, records);
        let output = fixture.resolve(fuel);
        fixture.close();
        output
    }

    #[test]
    fn direct_link_carries_exact_donor_cuts_and_cooked_values() {
        let source = "[link](/uri \"title\")";
        let output = resolve(source, 1);
        assert_eq!(output.len(), 1);
        let fact = output.fact(0).expect("direct fact");
        assert_eq!(fact.kind(), M11InlineDirectKind::Link);
        assert_eq!(fact.source(), 0..source.len() as u32);
        assert_eq!(fact.label_source(), 1..5);
        assert_eq!(fact.destination_source(), 7..11);
        assert_eq!(fact.title_source(), Some(12..19));
        assert_eq!(fact.cooked_destination(), "/uri");
        assert_eq!(fact.cooked_title(), Some("title"));
        assert_eq!(
            output.syntax_ranges().collect::<Vec<_>>(),
            vec![0..1, 5..6, 6..source.len() as u32]
        );
    }

    #[test]
    fn entity_output_feeds_the_same_backslash_removal_stage() {
        let source = "[x](&bsol;* \"a&bsol;*\")";
        let output = resolve(source, 1);
        let fact = output.fact(0).expect("direct fact");
        assert_eq!(fact.cooked_destination(), "*");
        assert_eq!(fact.cooked_title(), Some("a*"));
        assert_eq!(
            fact.cooked_destination(),
            block_spine_facade::clean_reference_destination("&bsol;*").expect("donor destination")
        );
        assert_eq!(
            fact.cooked_title(),
            Some(
                block_spine_facade::clean_reference_title("\"a&bsol;*\"")
                    .expect("donor title")
                    .as_str()
            )
        );
    }

    #[test]
    fn streaming_tail_cuts_match_the_pinned_donor_facade() {
        for tail in [
            "()",
            "(/uri \"title\")",
            "(</my uri>)",
            "(foo(and(bar)))",
            "(   /uri\n  \"title\"  )",
        ] {
            let source = format!("[x]{tail}");
            let output = resolve(&source, 1);
            let fact = output.fact(0).expect("direct fact");
            let donor = block_spine_facade::direct_link_tail(tail)
                .expect("donor facade")
                .expect("donor match");
            let tail_start = 3_u32;
            assert_eq!(
                fact.source().end - tail_start,
                donor.source.end as u32,
                "{tail:?}"
            );
            assert_eq!(
                fact.destination_source().start - u64::from(tail_start)
                    ..fact.destination_source().end - u64::from(tail_start),
                donor.url_source.start as u64..donor.url_source.end as u64,
                "{tail:?}"
            );
            assert_eq!(
                fact.title_source().map(|range| {
                    range.start - u64::from(tail_start)..range.end - u64::from(tail_start)
                }),
                donor
                    .title_source
                    .map(|range| range.start as u64..range.end as u64),
                "{tail:?}"
            );
        }
    }

    #[test]
    fn direct_resolution_is_fuel_partition_invariant() {
        let source = "before [a *b*](&bsol;* \"t&amp;x\") [foo [bar](/inner)](/outer) after";
        let baseline = resolve(source, 1);
        let baseline_facts = baseline
            .facts
            .iter()
            .map(|fact| {
                (
                    fact.kind(),
                    fact.source(),
                    fact.label_source(),
                    fact.destination_source(),
                    fact.title_source(),
                    fact.cooked_destination().to_owned(),
                    fact.cooked_title().map(str::to_owned),
                )
            })
            .collect::<Vec<_>>();
        let baseline_syntax = baseline.syntax_ranges().collect::<Vec<_>>();
        for fuel in [2, 7, 31, 257] {
            let output = resolve(source, fuel);
            let facts = output
                .facts
                .iter()
                .map(|fact| {
                    (
                        fact.kind(),
                        fact.source(),
                        fact.label_source(),
                        fact.destination_source(),
                        fact.title_source(),
                        fact.cooked_destination().to_owned(),
                        fact.cooked_title().map(str::to_owned),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(facts, baseline_facts, "fuel {fuel}");
            assert_eq!(
                output.syntax_ranges().collect::<Vec<_>>(),
                baseline_syntax,
                "fuel {fuel}"
            );
        }
    }

    #[test]
    fn direct_only_certificate_is_revoked_only_by_a_real_global_reference_question() {
        assert!(resolve("[foo](/uri)", 1).exhaustive_bracket_classification());
        assert!(
            resolve("[outer [bar](/uri)](/outer)", 1).exhaustive_bracket_classification(),
            "locally invalidated outer brackets need no global winner lookup"
        );
        assert!(
            !resolve("[foo]", 1).exhaustive_bracket_classification(),
            "a valid shortcut label is ambiguous without document winners"
        );
        assert!(
            !resolve("[foo][bar]", 1).exhaustive_bracket_classification(),
            "a valid explicit label is ambiguous without document winners"
        );
    }

    #[test]
    fn definitive_resolver_recognizes_all_reference_forms_and_footnote_stance() {
        let source = "[foo] [text][BAR] [foo][] ![foo] [missing] [^foo]";
        let output = resolve_references(source, reference_records(), 1);
        assert!(output.exhaustive_bracket_classification());
        let facts = output
            .facts
            .iter()
            .map(|fact| {
                (
                    fact.kind(),
                    fact.source(),
                    fact.label_source(),
                    fact.destination_source(),
                    fact.title_source(),
                    fact.cooked_destination().to_owned(),
                    fact.cooked_title().map(str::to_owned),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            facts,
            vec![
                (
                    M11InlineDirectKind::ReferenceLink,
                    0..5,
                    1..4,
                    3..7,
                    Some(7..12),
                    "/foo".to_owned(),
                    Some("title".to_owned()),
                ),
                (
                    M11InlineDirectKind::ReferenceLink,
                    6..17,
                    7..11,
                    15..24,
                    None,
                    "/bar".to_owned(),
                    None,
                ),
                (
                    M11InlineDirectKind::ReferenceLink,
                    18..25,
                    19..22,
                    3..7,
                    Some(7..12),
                    "/foo".to_owned(),
                    Some("title".to_owned()),
                ),
                (
                    M11InlineDirectKind::ReferenceImage,
                    26..32,
                    28..31,
                    3..7,
                    Some(7..12),
                    "/foo".to_owned(),
                    Some("title".to_owned()),
                ),
            ]
        );
        let missing_start = source.find("[missing]").expect("missing spelling") as u32;
        assert!(!output.intersects_syntax(missing_start..missing_start + 9));
        let footnote_start = source.find("[^foo]").expect("footnote spelling") as u32;
        assert!(!output.intersects_syntax(footnote_start..footnote_start + 6));
    }

    #[test]
    fn explicit_reference_miss_replays_its_tail_without_falling_back_to_primary_label() {
        let source = "[foo][missing](/direct)";
        let output = resolve_references(
            source,
            vec![reference_record(0, "foo", "/foo", Some("title"))],
            1,
        );
        assert!(output.exhaustive_bracket_classification());
        let fact = output.fact(0).expect("replayed direct tail");
        assert_eq!(fact.kind(), M11InlineDirectKind::Link);
        assert_eq!(fact.source(), 5..23);
        assert_eq!(fact.label_source(), 6..13);
        assert_eq!(fact.cooked_destination(), "/direct");
        assert!(!output.intersects_syntax(0..1));
        assert!(!output.intersects_syntax(4..5));

        let collapsed = resolve_references("[missing][](/direct)", Vec::new(), 1);
        assert!(collapsed.exhaustive_bracket_classification());
        let fact = collapsed.fact(0).expect("replayed empty direct link");
        assert_eq!(fact.kind(), M11InlineDirectKind::Link);
        assert_eq!(fact.source(), 9..20);
        assert_eq!(fact.label_source(), 10..10);
        assert_eq!(fact.cooked_destination(), "/direct");
        assert!(!collapsed.intersects_syntax(0..1));
        assert!(!collapsed.intersects_syntax(8..9));
    }

    #[test]
    fn commonmark_569_through_571_reconsider_undefined_explicit_tails() {
        let source = "[foo][bar][baz] padding long enough";

        let only_baz =
            resolve_references(source, vec![reference_record(0, "baz", "/baz", None)], 1);
        assert_eq!(
            only_baz
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.source(), fact.cooked_destination()))
                .collect::<Vec<_>>(),
            vec![(M11InlineDirectKind::ReferenceLink, 5..15, "/baz")]
        );

        let bar_and_baz = resolve_references(
            source,
            vec![
                reference_record(0, "baz", "/baz", None),
                reference_record(1, "bar", "/bar", None),
            ],
            1,
        );
        assert_eq!(
            bar_and_baz
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.source(), fact.cooked_destination()))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineDirectKind::ReferenceLink, 0..10, "/bar"),
                (M11InlineDirectKind::ReferenceLink, 10..15, "/baz"),
            ]
        );

        let foo_and_baz = resolve_references(
            source,
            vec![
                reference_record(0, "baz", "/baz", None),
                reference_record(1, "foo", "/foo", None),
            ],
            1,
        );
        assert_eq!(
            foo_and_baz
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.source(), fact.cooked_destination()))
                .collect::<Vec<_>>(),
            vec![(M11InlineDirectKind::ReferenceLink, 5..15, "/baz")]
        );
    }

    #[test]
    fn direct_tail_keeps_precedence_over_reference_lookup() {
        let source = "[foo](/direct)";
        let output = resolve_references(
            source,
            vec![reference_record(0, "foo", "/reference", None)],
            1,
        );
        let fact = output.fact(0).expect("direct fact");
        assert_eq!(fact.kind(), M11InlineDirectKind::Link);
        assert_eq!(fact.cooked_destination(), "/direct");
    }

    #[test]
    fn failed_direct_tail_falls_back_to_shortcut_and_replays_tail() {
        let source = "[foo](unterminated";
        let output = resolve_references(
            source,
            vec![reference_record(0, "foo", "/reference", None)],
            1,
        );
        let fact = output.fact(0).expect("shortcut fallback");
        assert_eq!(fact.kind(), M11InlineDirectKind::ReferenceLink);
        assert_eq!(fact.source(), 0..5);
        assert_eq!(fact.cooked_destination(), "/reference");
        assert_eq!(output.syntax_ranges().collect::<Vec<_>>(), vec![0..1, 4..5]);
    }

    #[test]
    fn nested_reference_link_wins_without_claiming_outer_literal_markers() {
        let source = "[outer [foo]][bar] padding long enough";
        let output = resolve_references(source, reference_records(), 1);
        let facts = output
            .facts
            .iter()
            .map(|fact| (fact.kind(), fact.source()))
            .collect::<Vec<_>>();
        assert_eq!(
            facts,
            vec![
                (M11InlineDirectKind::ReferenceLink, 7..12),
                (M11InlineDirectKind::ReferenceLink, 13..18),
            ]
        );
        assert!(!output.intersects_syntax(0..1));
        assert!(!output.intersects_syntax(12..13));
    }

    #[test]
    fn reference_resolution_uses_normative_unicode_case_fold() {
        let source = "[Straẞe] padding long enough";
        let output = resolve_references(
            source,
            vec![reference_record(0, "strasse", "/street", None)],
            1,
        );
        let fact = output.fact(0).expect("case-folded reference");
        assert_eq!(fact.kind(), M11InlineDirectKind::ReferenceLink);
        assert_eq!(fact.cooked_destination(), "/street");
    }

    #[test]
    fn existing_reference_with_unrepresentable_value_revokes_bracket_certificate() {
        let source = "[foo] padding long enough";
        let oversized = "x".repeat(M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES);
        let output = resolve_references(
            source,
            vec![reference_record(0, "foo", &oversized, None)],
            1,
        );
        assert_eq!(output.len(), 0);
        assert!(output.syntax_ranges().next().is_none());
        assert!(
            !output.exhaustive_bracket_classification(),
            "a defined-but-unrepresentable link must fail the enclosing leaf closed"
        );
    }

    #[test]
    fn reference_resolution_is_fuel_partition_invariant() {
        let source = "[foo] [text][BAR] [foo][] ![foo] padding long enough";
        let snapshot = |fuel| {
            let output = resolve_references(source, reference_records(), fuel);
            (
                output
                    .facts
                    .iter()
                    .map(|fact| {
                        (
                            fact.kind(),
                            fact.source(),
                            fact.destination_source(),
                            fact.title_source(),
                            fact.cooked_destination().to_owned(),
                            fact.cooked_title().map(str::to_owned),
                        )
                    })
                    .collect::<Vec<_>>(),
                output.syntax_ranges().collect::<Vec<_>>(),
                output.exhaustive_bracket_classification(),
            )
        };
        assert_eq!(snapshot(1), snapshot(31));
    }
}
