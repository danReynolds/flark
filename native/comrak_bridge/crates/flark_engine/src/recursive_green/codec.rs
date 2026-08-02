//! Canonical event and measured-page codec for recursive Green storage.

use std::fmt;

use crate::document::DocumentRuntimeError;
use crate::measured_sequence::{SequenceMeasure, SequenceSpec, SequenceSpecInspection};
use crate::source::SourceEditError;
use crate::storage::{ArenaError, ARENA_PAGE_BYTES};

const GREEN_LEAF_MAGIC: [u8; 4] = *b"RGL1";
const GREEN_BRANCH_MAGIC: [u8; 4] = *b"RGB1";
const GREEN_SCHEMA: u32 = 3;
const COMMITMENT_LANES: usize = 4;
const COMMITMENT_BYTES: usize = COMMITMENT_LANES * 2 * 8;
const COMMITMENT_MODULUS: u64 = (1_u64 << 61) - 1;
const COMMITMENT_BASES: [u64; COMMITMENT_LANES] = [
    0x0a09_e667_f3bc_c909,
    0x1b67_ae85_84ca_a73b,
    0x1c6e_f372_fe94_f82b,
    0x154f_f53a_5f1d_36f1,
];
const MAX_PACKED_EVENT_BYTES: usize = 14 + 3 + M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES;
pub(super) const GREEN_EVENTS_PER_PAGE_MAX: usize = 128;
pub const M11_RECURSIVE_GREEN_PROPERTY_CHUNK_MAX_BYTES: usize = 32;
pub const M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES: usize = 64;
const M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_MAGIC: [u8; 4] = *b"RGEO";
const M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_VERSION: u8 = 1;
const M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_BYTES: usize = 24;

#[derive(Debug)]
pub enum M11RecursiveGreenError {
    ZeroFuel,
    PollLimitExceeded,
    InvalidState,
    InputClosed,
    EventAlreadyPending,
    InvalidEvent,
    IncompleteCoverage,
    InvalidPoint,
    WrongRuntime,
    SourceAuthorityMismatch,
    CounterOverflow,
    Corrupt(&'static str),
    Arena(ArenaError),
    Document(DocumentRuntimeError),
    Source(SourceEditError),
}

impl fmt::Display for M11RecursiveGreenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFuel => formatter.write_str("recursive-green poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("recursive-green poll fuel exceeds its public bound")
            }
            Self::InvalidState => {
                formatter.write_str("recursive-green owner is in an invalid state")
            }
            Self::InputClosed => formatter.write_str("recursive-green input is closed"),
            Self::EventAlreadyPending => {
                formatter.write_str("a recursive-green event is already pending")
            }
            Self::InvalidEvent => formatter.write_str("recursive-green event is invalid"),
            Self::IncompleteCoverage => {
                formatter.write_str("recursive-green coverage is incomplete")
            }
            Self::InvalidPoint => formatter.write_str("recursive-green source point is invalid"),
            Self::WrongRuntime => {
                formatter.write_str("recursive-green owner belongs to another runtime")
            }
            Self::SourceAuthorityMismatch => {
                formatter.write_str("recursive-green source authority does not match")
            }
            Self::CounterOverflow => formatter.write_str("recursive-green counter overflow"),
            Self::Corrupt(message) => {
                write!(formatter, "corrupt recursive-green storage: {message}")
            }
            Self::Arena(error) => error.fmt(formatter),
            Self::Document(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11RecursiveGreenError {}

impl From<ArenaError> for M11RecursiveGreenError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<DocumentRuntimeError> for M11RecursiveGreenError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self::Document(error)
    }
}

impl From<SourceEditError> for M11RecursiveGreenError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct M11RecursiveGreenFrameId(u64);

impl M11RecursiveGreenFrameId {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct M11RecursiveGreenKind(u16);

impl M11RecursiveGreenKind {
    #[must_use]
    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct M11RecursiveGreenFactTag(u16);

impl M11RecursiveGreenFactTag {
    #[must_use]
    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M11RecursiveGreenSourceMetric {
    bytes: u64,
    utf16: u64,
}

impl M11RecursiveGreenSourceMetric {
    #[must_use]
    pub const fn new(bytes: u64, utf16: u64) -> Option<Self> {
        if bytes < utf16 || (bytes == 0) != (utf16 == 0) {
            None
        } else {
            Some(Self { bytes, utf16 })
        }
    }

    pub(super) const fn from_validated(bytes: u64, utf16: u64) -> Self {
        Self { bytes, utf16 }
    }

    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn utf16(self) -> u64 {
        self.utf16
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bytes == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenPropertyChunk {
    tag: M11RecursiveGreenFactTag,
    len: u8,
    bytes: [u8; M11_RECURSIVE_GREEN_PROPERTY_CHUNK_MAX_BYTES],
}

impl M11RecursiveGreenPropertyChunk {
    pub fn new(
        tag: M11RecursiveGreenFactTag,
        bytes: &[u8],
    ) -> Result<Self, M11RecursiveGreenError> {
        if bytes.is_empty() || bytes.len() > M11_RECURSIVE_GREEN_PROPERTY_CHUNK_MAX_BYTES {
            return Err(M11RecursiveGreenError::InvalidEvent);
        }
        let mut storage = [0; M11_RECURSIVE_GREEN_PROPERTY_CHUNK_MAX_BYTES];
        storage[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            tag,
            len: u8::try_from(bytes.len()).expect("property cap fits u8"),
            bytes: storage,
        })
    }

    #[must_use]
    pub const fn tag(self) -> M11RecursiveGreenFactTag {
        self.tag
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenCloseFacts {
    tag: M11RecursiveGreenFactTag,
    len: u8,
    bytes: [u8; M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES],
}

impl M11RecursiveGreenCloseFacts {
    pub fn new(
        tag: M11RecursiveGreenFactTag,
        bytes: &[u8],
    ) -> Result<Self, M11RecursiveGreenError> {
        if bytes.is_empty() || bytes.len() > M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES {
            return Err(M11RecursiveGreenError::InvalidEvent);
        }
        let mut storage = [0; M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES];
        storage[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            tag,
            len: u8::try_from(bytes.len()).expect("close-facts cap fits u8"),
            bytes: storage,
        })
    }

    #[must_use]
    pub const fn tag(self) -> M11RecursiveGreenFactTag {
        self.tag
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    /// Appends versioned, frame-relative editable geometry to grammar-owned
    /// close facts. The trailer is covered by the canonical Green commitment,
    /// while remaining relative to the frame Enter so unchanged suffix rows
    /// can move without rewriting their facts.
    pub fn new_with_cached_row_editable(
        tag: M11RecursiveGreenFactTag,
        semantic: &[u8],
        cached: M11RecursiveGreenCachedRowEditable,
    ) -> Result<Self, M11RecursiveGreenError> {
        let total = semantic
            .len()
            .checked_add(M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_BYTES)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if total > M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES {
            return Err(M11RecursiveGreenError::InvalidEvent);
        }
        let mut bytes = [0_u8; M11_RECURSIVE_GREEN_CLOSE_FACTS_MAX_BYTES];
        bytes[..semantic.len()].copy_from_slice(semantic);
        let mut cursor = semantic.len();
        bytes[cursor..cursor + 4].copy_from_slice(&M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_MAGIC);
        cursor += 4;
        bytes[cursor] = M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_VERSION;
        cursor += 1;
        bytes[cursor] = match cached.capability {
            M11RecursiveGreenCachedRowEditCapability::Contiguous => 1,
            M11RecursiveGreenCachedRowEditCapability::Unavailable => 2,
        };
        cursor += 3; // capability plus two reserved zero bytes
        for metric in [cached.start, cached.end] {
            let source_bytes =
                u32::try_from(metric.bytes()).map_err(|_| M11RecursiveGreenError::InvalidEvent)?;
            let source_utf16 =
                u32::try_from(metric.utf16()).map_err(|_| M11RecursiveGreenError::InvalidEvent)?;
            bytes[cursor..cursor + 4].copy_from_slice(&source_bytes.to_le_bytes());
            cursor += 4;
            bytes[cursor..cursor + 4].copy_from_slice(&source_utf16.to_le_bytes());
            cursor += 4;
        }
        debug_assert_eq!(cursor, total);
        Self::new(tag, &bytes[..total])
    }

    /// Splits the optional cached row trailer from the grammar-owned prefix.
    /// Older roots without the trailer remain valid and return `None`.
    pub fn cached_row_editable(
        &self,
        semantic_bytes: usize,
    ) -> Result<Option<(&[u8], M11RecursiveGreenCachedRowEditable)>, M11RecursiveGreenError> {
        let bytes = self.as_bytes();
        let expected = semantic_bytes
            .checked_add(M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_BYTES)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if bytes.len() != expected {
            return Ok(None);
        }
        let semantic_end = semantic_bytes;
        let trailer = &bytes[semantic_end..];
        if trailer[..4] != M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_MAGIC {
            return Ok(None);
        }
        if trailer[4] != M11_RECURSIVE_GREEN_ROW_EDITABLE_TRAILER_VERSION
            || trailer[6] != 0
            || trailer[7] != 0
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "invalid cached row-editable trailer header",
            ));
        }
        let capability = match trailer[5] {
            1 => M11RecursiveGreenCachedRowEditCapability::Contiguous,
            2 => M11RecursiveGreenCachedRowEditCapability::Unavailable,
            _ => {
                return Err(M11RecursiveGreenError::Corrupt(
                    "invalid cached row-editable capability",
                ));
            }
        };
        let read_metric = |offset: usize| {
            let bytes = u64::from(u32::from_le_bytes(
                trailer[offset..offset + 4]
                    .try_into()
                    .expect("validated cached row trailer width"),
            ));
            let utf16 = u64::from(u32::from_le_bytes(
                trailer[offset + 4..offset + 8]
                    .try_into()
                    .expect("validated cached row trailer width"),
            ));
            M11RecursiveGreenSourceMetric::new(bytes, utf16).ok_or(M11RecursiveGreenError::Corrupt(
                "invalid cached row-editable metric",
            ))
        };
        let start = read_metric(8)?;
        let end = read_metric(16)?;
        if start.bytes() > end.bytes() || start.utf16() > end.utf16() {
            return Err(M11RecursiveGreenError::Corrupt(
                "cached row-editable bounds are reversed",
            ));
        }
        Ok(Some((
            &bytes[..semantic_end],
            M11RecursiveGreenCachedRowEditable {
                capability,
                start,
                end,
            },
        )))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenCachedRowEditCapability {
    Contiguous,
    Unavailable,
}

/// Parser-certified physical geometry relative to one renderable frame Enter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenCachedRowEditable {
    capability: M11RecursiveGreenCachedRowEditCapability,
    start: M11RecursiveGreenSourceMetric,
    end: M11RecursiveGreenSourceMetric,
}

impl M11RecursiveGreenCachedRowEditable {
    #[must_use]
    pub const fn new(
        capability: M11RecursiveGreenCachedRowEditCapability,
        start: M11RecursiveGreenSourceMetric,
        end: M11RecursiveGreenSourceMetric,
    ) -> Option<Self> {
        if start.bytes() > end.bytes() || start.utf16() > end.utf16() {
            None
        } else {
            Some(Self {
                capability,
                start,
                end,
            })
        }
    }

    #[must_use]
    pub const fn capability(self) -> M11RecursiveGreenCachedRowEditCapability {
        self.capability
    }

    #[must_use]
    pub const fn start(self) -> M11RecursiveGreenSourceMetric {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> M11RecursiveGreenSourceMetric {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum M11RecursiveGreenCoveragePart {
    Content = 1,
    ContainerMarker = 2,
    BlockMarker = 3,
    Gap = 4,
    Terminal = 5,
}

impl M11RecursiveGreenCoveragePart {
    fn decode(value: u8) -> Result<Self, M11RecursiveGreenError> {
        match value {
            1 => Ok(Self::Content),
            2 => Ok(Self::ContainerMarker),
            3 => Ok(Self::BlockMarker),
            4 => Ok(Self::Gap),
            5 => Ok(Self::Terminal),
            _ => Err(M11RecursiveGreenError::Corrupt("unknown coverage part")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenLogicalAction {
    None,
    Identity,
    CanonicalText,
    PartialTab {
        target_owner_depth: u32,
        remaining_spaces: u8,
    },
    HiddenUpstream,
    CanonicalNewline,
}

impl M11RecursiveGreenLogicalAction {
    pub(super) fn validate(self, physical_owner_depth: u32) -> Result<(), M11RecursiveGreenError> {
        if let Self::PartialTab {
            target_owner_depth,
            remaining_spaces,
        } = self
        {
            if !(1..=3).contains(&remaining_spaces) || target_owner_depth > physical_owner_depth {
                return Err(M11RecursiveGreenError::InvalidEvent);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct M11RecursiveGreenClosedChild {
    ends_blank: bool,
    item_loose_if_nonlast: bool,
    item_loose_if_last: bool,
}

impl M11RecursiveGreenClosedChild {
    #[must_use]
    pub const fn new(
        ends_blank: bool,
        item_loose_if_nonlast: bool,
        item_loose_if_last: bool,
    ) -> Self {
        Self {
            ends_blank,
            item_loose_if_nonlast,
            item_loose_if_last,
        }
    }

    #[must_use]
    pub const fn ends_blank(self) -> bool {
        self.ends_blank
    }
    #[must_use]
    pub const fn item_loose_if_nonlast(self) -> bool {
        self.item_loose_if_nonlast
    }
    #[must_use]
    pub const fn item_loose_if_last(self) -> bool {
        self.item_loose_if_last
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct M11RecursiveGreenChildFold {
    had_child: bool,
    any_nonlast_child_ends_blank: bool,
    last_child_ends_blank: bool,
    list_loose_before_last: bool,
    last_item_loose_if_nonlast: bool,
    last_item_loose_if_last: bool,
}

impl M11RecursiveGreenChildFold {
    pub(super) fn push(&mut self, child: M11RecursiveGreenClosedChild) {
        if self.had_child {
            self.any_nonlast_child_ends_blank |= self.last_child_ends_blank;
            self.list_loose_before_last |= self.last_item_loose_if_nonlast;
        }
        self.had_child = true;
        self.last_child_ends_blank = child.ends_blank;
        self.last_item_loose_if_nonlast = child.item_loose_if_nonlast;
        self.last_item_loose_if_last = child.item_loose_if_last;
    }

    pub(super) const fn followed_by(self, suffix: Self) -> Self {
        if !self.had_child {
            return suffix;
        }
        if !suffix.had_child {
            return self;
        }
        Self {
            had_child: true,
            any_nonlast_child_ends_blank: self.any_nonlast_child_ends_blank
                || self.last_child_ends_blank
                || suffix.any_nonlast_child_ends_blank,
            last_child_ends_blank: suffix.last_child_ends_blank,
            list_loose_before_last: self.list_loose_before_last
                || self.last_item_loose_if_nonlast
                || suffix.list_loose_before_last,
            last_item_loose_if_nonlast: suffix.last_item_loose_if_nonlast,
            last_item_loose_if_last: suffix.last_item_loose_if_last,
        }
    }

    #[must_use]
    pub const fn had_child(self) -> bool {
        self.had_child
    }
    #[must_use]
    pub const fn any_nonlast_child_ends_blank(self) -> bool {
        self.any_nonlast_child_ends_blank
    }
    #[must_use]
    pub const fn last_child_ends_blank(self) -> bool {
        self.last_child_ends_blank
    }
    #[must_use]
    pub const fn list_loose_before_last(self) -> bool {
        self.list_loose_before_last
    }
    #[must_use]
    pub const fn last_item_loose_if_nonlast(self) -> bool {
        self.last_item_loose_if_nonlast
    }
    #[must_use]
    pub const fn last_item_loose_if_last(self) -> bool {
        self.last_item_loose_if_last
    }
    #[must_use]
    pub const fn list_is_tight(self) -> bool {
        !(self.list_loose_before_last || self.last_item_loose_if_last)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenEvent {
    Enter {
        frame: M11RecursiveGreenFrameId,
        kind: M11RecursiveGreenKind,
    },
    Property(M11RecursiveGreenPropertyChunk),
    Coverage {
        physical: M11RecursiveGreenSourceMetric,
        owner_depth: u32,
        part: M11RecursiveGreenCoveragePart,
        logical: M11RecursiveGreenLogicalAction,
    },
    RetypeOpen {
        frame: M11RecursiveGreenFrameId,
        kind: M11RecursiveGreenKind,
        property: Option<M11RecursiveGreenPropertyChunk>,
    },
    Exit {
        frame: M11RecursiveGreenFrameId,
        final_kind: M11RecursiveGreenKind,
        close: Option<M11RecursiveGreenCloseFacts>,
        last_line_blank: bool,
        child: M11RecursiveGreenClosedChild,
    },
}

/// Exact source-to-logical atom persisted after source-derived validation.
///
/// Parser callers can observe this value but cannot offer it directly; the
/// build accepts only [`M11RecursiveGreenLogicalAction`] recipes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenLogicalAtom {
    None,
    Identity,
    TabToSpaces { target_owner_depth: u32, spaces: u8 },
    HiddenUpstream,
    LfToLf,
    CrLfToLf,
    LoneCrToLf,
    NulToReplacement,
}

pub(super) type LogicalAtom = M11RecursiveGreenLogicalAtom;

impl M11RecursiveGreenLogicalAtom {
    pub(super) const fn logical_metric(
        self,
        physical: M11RecursiveGreenSourceMetric,
    ) -> M11RecursiveGreenSourceMetric {
        match self {
            Self::None | Self::HiddenUpstream => {
                M11RecursiveGreenSourceMetric::from_validated(0, 0)
            }
            Self::Identity => physical,
            Self::TabToSpaces { spaces, .. } => {
                let spaces = spaces as u64;
                M11RecursiveGreenSourceMetric::from_validated(spaces, spaces)
            }
            Self::LfToLf | Self::CrLfToLf | Self::LoneCrToLf => {
                M11RecursiveGreenSourceMetric::from_validated(1, 1)
            }
            Self::NulToReplacement => M11RecursiveGreenSourceMetric::from_validated(3, 1),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PackedGreenEvent {
    Enter {
        frame: M11RecursiveGreenFrameId,
        kind: M11RecursiveGreenKind,
    },
    Property(M11RecursiveGreenPropertyChunk),
    Coverage {
        physical: M11RecursiveGreenSourceMetric,
        owner_depth: u32,
        part: M11RecursiveGreenCoveragePart,
        atom: LogicalAtom,
    },
    RetypeOpen {
        frame: M11RecursiveGreenFrameId,
        kind: M11RecursiveGreenKind,
        property: Option<M11RecursiveGreenPropertyChunk>,
    },
    Exit {
        frame: M11RecursiveGreenFrameId,
        final_kind: M11RecursiveGreenKind,
        close: Option<M11RecursiveGreenCloseFacts>,
        last_line_blank: bool,
        child: M11RecursiveGreenClosedChild,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GreenOpenWitness {
    pub(super) frame: M11RecursiveGreenFrameId,
    pub(super) kind: M11RecursiveGreenKind,
    pub(super) event_ordinal: u64,
}

/// Shape-independent commitment to one ordered canonical event stream.
///
/// Each event contributes a domain-separated BLAKE3 coefficient. Combining
/// summaries is polynomial concatenation, so leaf packing and measured-tree
/// shape do not affect the resulting commitment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecursiveGreenCommitment {
    hash: [u64; COMMITMENT_LANES],
    factor: [u64; COMMITMENT_LANES],
}

impl RecursiveGreenCommitment {
    pub(super) const fn empty() -> Self {
        Self {
            hash: [0; COMMITMENT_LANES],
            factor: [1; COMMITMENT_LANES],
        }
    }

    fn for_event(event: PackedGreenEvent) -> Result<Self, M11RecursiveGreenError> {
        let expected_len = packed_event_len(event);
        let mut encoded = [0_u8; MAX_PACKED_EVENT_BYTES];
        let mut cursor = 0;
        encode_packed_event(event, &mut encoded, &mut cursor)?;
        if cursor != expected_len {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green canonical event length changed",
            ));
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.recursive-green.event.v1\0");
        hasher.update(&encoded[..cursor]);
        let digest = hasher.finalize();
        let mut hash = [0_u64; COMMITMENT_LANES];
        for (lane, value) in hash.iter_mut().enumerate() {
            let start = lane * 8;
            let coefficient = u64::from_le_bytes(
                digest.as_bytes()[start..start + 8]
                    .try_into()
                    .expect("BLAKE3 lane has eight bytes"),
            );
            *value = coefficient % (COMMITMENT_MODULUS - 1) + 1;
        }
        Ok(Self {
            hash,
            factor: COMMITMENT_BASES,
        })
    }

    pub(super) fn from_lanes(
        hash: [u64; COMMITMENT_LANES],
        factor: [u64; COMMITMENT_LANES],
    ) -> Result<Self, M11RecursiveGreenError> {
        let commitment = Self { hash, factor };
        if !commitment.is_valid() {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green commitment lane is invalid",
            ));
        }
        Ok(commitment)
    }

    const fn combine(self, right: Self) -> Self {
        let mut hash = [0; COMMITMENT_LANES];
        let mut factor = [0; COMMITMENT_LANES];
        let mut lane = 0;
        while lane < COMMITMENT_LANES {
            hash[lane] = add_mod(
                multiply_mod(self.hash[lane], right.factor[lane]),
                right.hash[lane],
            );
            factor[lane] = multiply_mod(self.factor[lane], right.factor[lane]);
            lane += 1;
        }
        Self { hash, factor }
    }

    pub(super) fn checksum(self) -> [u8; 32] {
        let mut checksum = [0_u8; 32];
        for (lane, value) in self.hash.into_iter().enumerate() {
            let start = lane * 8;
            checksum[start..start + 8].copy_from_slice(&value.to_le_bytes());
        }
        checksum
    }

    const fn is_valid(self) -> bool {
        let mut lane = 0;
        while lane < COMMITMENT_LANES {
            if self.hash[lane] >= COMMITMENT_MODULUS
                || self.factor[lane] == 0
                || self.factor[lane] >= COMMITMENT_MODULUS
            {
                return false;
            }
            lane += 1;
        }
        true
    }
}

impl Default for RecursiveGreenCommitment {
    fn default() -> Self {
        Self::empty()
    }
}

const fn add_mod(left: u64, right: u64) -> u64 {
    let sum = left + right;
    if sum >= COMMITMENT_MODULUS {
        sum - COMMITMENT_MODULUS
    } else {
        sum
    }
}

const fn multiply_mod(left: u64, right: u64) -> u64 {
    ((left as u128 * right as u128) % COMMITMENT_MODULUS as u128) as u64
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RecursiveGreenSummary {
    pub(super) physical_bytes: u64,
    pub(super) physical_utf16: u64,
    pub(super) logical_bytes: u64,
    pub(super) logical_utf16: u64,
    pub(super) events: u64,
    pub(super) enters: u64,
    pub(super) renderable_row_exits: u64,
    pub(super) canonical_event_bytes: u64,
    pub(super) canonical_commitment: RecursiveGreenCommitment,
    /// Greatest frame identity contained in this subtree.
    ///
    /// Frame identities are not document ordinals.  Keeping this maximum in
    /// the additive tree summary lets an incremental replacement mint fresh
    /// identities above every retained prefix/suffix identity without
    /// scanning or rewriting either side of the edit.
    pub(super) max_frame_id: u64,
    pub(super) balance: i64,
    pub(super) minimum_prefix: i64,
    pub(super) minimum_closed_depth: Option<i64>,
    pub(super) oldest_open: Option<GreenOpenWitness>,
    pub(super) outermost_children: M11RecursiveGreenChildFold,
}

impl RecursiveGreenSummary {
    pub(super) const fn empty() -> Self {
        Self {
            physical_bytes: 0,
            physical_utf16: 0,
            logical_bytes: 0,
            logical_utf16: 0,
            events: 0,
            enters: 0,
            renderable_row_exits: 0,
            canonical_event_bytes: 0,
            canonical_commitment: RecursiveGreenCommitment::empty(),
            max_frame_id: 0,
            balance: 0,
            minimum_prefix: 0,
            minimum_closed_depth: None,
            oldest_open: None,
            outermost_children: M11RecursiveGreenChildFold {
                had_child: false,
                any_nonlast_child_ends_blank: false,
                last_child_ends_blank: false,
                list_loose_before_last: false,
                last_item_loose_if_nonlast: false,
                last_item_loose_if_last: false,
            },
        }
    }

    pub(super) fn unmatched_closes(self) -> Result<u64, M11RecursiveGreenError> {
        if self.minimum_prefix > 0 {
            return Err(M11RecursiveGreenError::Corrupt(
                "positive minimum structural prefix",
            ));
        }
        self.minimum_prefix
            .checked_neg()
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(M11RecursiveGreenError::CounterOverflow)
    }

    pub(super) fn unmatched_opens(self) -> Result<u64, M11RecursiveGreenError> {
        let closes = i128::from(self.unmatched_closes()?);
        let opens = i128::from(self.balance) + closes;
        u64::try_from(opens)
            .map_err(|_| M11RecursiveGreenError::Corrupt("negative unmatched open count"))
    }

    pub(super) fn checked_followed_by(self, right: Self) -> Result<Self, M11RecursiveGreenError> {
        validate_summary(self)?;
        validate_summary(right)?;
        let left_opens = self.unmatched_opens()?;
        let right_closes = right.unmatched_closes()?;
        let cancelled = left_opens.min(right_closes);
        let left_survivors = left_opens - cancelled;
        let oldest_open = if left_survivors != 0 {
            self.oldest_open
        } else {
            match right.oldest_open {
                Some(mut witness) => {
                    witness.event_ordinal = witness
                        .event_ordinal
                        .checked_add(self.events)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    Some(witness)
                }
                None => None,
            }
        };
        let balance = self
            .balance
            .checked_add(right.balance)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let shifted_right_minimum = self
            .balance
            .checked_add(right.minimum_prefix)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let shifted_right_closed = right
            .minimum_closed_depth
            .map(|depth| {
                self.balance
                    .checked_add(depth)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)
            })
            .transpose()?;
        let minimum_closed_depth = match (self.minimum_closed_depth, shifted_right_closed) {
            (None, None) => None,
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (Some(left), Some(right)) => Some(left.min(right)),
        };
        let left_is_minimum = self.minimum_closed_depth == minimum_closed_depth;
        let right_is_minimum = shifted_right_closed == minimum_closed_depth;
        let outermost_children = match (left_is_minimum, right_is_minimum) {
            (true, true) => self
                .outermost_children
                .followed_by(right.outermost_children),
            (true, false) => self.outermost_children,
            (false, true) => right.outermost_children,
            (false, false) => M11RecursiveGreenChildFold::default(),
        };
        let summary = Self {
            physical_bytes: checked_add(self.physical_bytes, right.physical_bytes)?,
            physical_utf16: checked_add(self.physical_utf16, right.physical_utf16)?,
            logical_bytes: checked_add(self.logical_bytes, right.logical_bytes)?,
            logical_utf16: checked_add(self.logical_utf16, right.logical_utf16)?,
            events: checked_add(self.events, right.events)?,
            enters: checked_add(self.enters, right.enters)?,
            renderable_row_exits: checked_add(
                self.renderable_row_exits,
                right.renderable_row_exits,
            )?,
            canonical_event_bytes: checked_add(
                self.canonical_event_bytes,
                right.canonical_event_bytes,
            )?,
            canonical_commitment: self
                .canonical_commitment
                .combine(right.canonical_commitment),
            max_frame_id: self.max_frame_id.max(right.max_frame_id),
            balance,
            minimum_prefix: self.minimum_prefix.min(shifted_right_minimum),
            minimum_closed_depth,
            oldest_open,
            outermost_children,
        };
        validate_summary(summary)?;
        Ok(summary)
    }
}

fn checked_add(left: u64, right: u64) -> Result<u64, M11RecursiveGreenError> {
    left.checked_add(right)
        .ok_or(M11RecursiveGreenError::CounterOverflow)
}

fn validate_summary(summary: RecursiveGreenSummary) -> Result<(), M11RecursiveGreenError> {
    let commitment_valid = summary.canonical_commitment.is_valid();
    if summary.physical_bytes < summary.physical_utf16
        || summary.logical_bytes < summary.logical_utf16
        || summary.enters > summary.events
        || (summary.enters == 0) != (summary.max_frame_id == 0)
        || (summary.events == 0) != (summary.canonical_event_bytes == 0)
        || summary.canonical_event_bytes < summary.events
        || !commitment_valid
        || (summary.events == 0 && summary != RecursiveGreenSummary::empty())
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "invalid recursive-green metric summary",
        ));
    }
    let opens = summary.unmatched_opens()?;
    if (opens == 0) != summary.oldest_open.is_none() {
        return Err(M11RecursiveGreenError::Corrupt(
            "open witness differs from structural balance",
        ));
    }
    if let Some(witness) = summary.oldest_open {
        if witness.event_ordinal >= summary.events {
            return Err(M11RecursiveGreenError::Corrupt(
                "open witness is outside its event range",
            ));
        }
    }
    if summary.minimum_closed_depth.is_none()
        && summary.outermost_children != M11RecursiveGreenChildFold::default()
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "child fold exists without a closed structural depth",
        ));
    }
    Ok(())
}

pub(super) fn packed_event_summary(
    event: PackedGreenEvent,
) -> Result<RecursiveGreenSummary, M11RecursiveGreenError> {
    let mut summary = RecursiveGreenSummary::empty();
    summary.events = 1;
    summary.canonical_event_bytes = u64::try_from(packed_event_len(event))
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    summary.canonical_commitment = RecursiveGreenCommitment::for_event(event)?;
    match event {
        PackedGreenEvent::Enter { frame, kind } => {
            summary.enters = 1;
            summary.max_frame_id = frame.get();
            summary.balance = 1;
            summary.oldest_open = Some(GreenOpenWitness {
                frame,
                kind,
                event_ordinal: 0,
            });
        }
        PackedGreenEvent::Coverage { physical, atom, .. } => {
            if physical.is_empty() {
                return Err(M11RecursiveGreenError::Corrupt(
                    "empty packed coverage atom",
                ));
            }
            validate_atom(atom, physical)?;
            let logical = atom.logical_metric(physical);
            summary.physical_bytes = physical.bytes;
            summary.physical_utf16 = physical.utf16;
            summary.logical_bytes = logical.bytes;
            summary.logical_utf16 = logical.utf16;
        }
        PackedGreenEvent::Exit {
            final_kind, child, ..
        } => {
            summary.renderable_row_exits = u64::from(is_renderable_row_kind(final_kind));
            summary.balance = -1;
            summary.minimum_prefix = -1;
            summary.minimum_closed_depth = Some(-1);
            summary.outermost_children.push(child);
        }
        PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {}
    }
    validate_summary(summary)?;
    Ok(summary)
}

pub(super) const EMPTY_ITEM_ROW_KIND: u16 = 14;

pub(super) const fn is_renderable_row_kind(kind: M11RecursiveGreenKind) -> bool {
    matches!(kind.get(), 5 | 6 | 7 | 8 | 12 | 13 | EMPTY_ITEM_ROW_KIND)
}

fn validate_atom(
    atom: LogicalAtom,
    physical: M11RecursiveGreenSourceMetric,
) -> Result<(), M11RecursiveGreenError> {
    let valid = match atom {
        LogicalAtom::None | LogicalAtom::Identity | LogicalAtom::HiddenUpstream => true,
        LogicalAtom::TabToSpaces { spaces, .. } => {
            physical == M11RecursiveGreenSourceMetric::from_validated(1, 1)
                && (1..=3).contains(&spaces)
        }
        LogicalAtom::LfToLf | LogicalAtom::LoneCrToLf | LogicalAtom::NulToReplacement => {
            physical == M11RecursiveGreenSourceMetric::from_validated(1, 1)
        }
        LogicalAtom::CrLfToLf => physical == M11RecursiveGreenSourceMetric::from_validated(2, 2),
    };
    if valid {
        Ok(())
    } else {
        Err(M11RecursiveGreenError::Corrupt(
            "invalid logical atom geometry",
        ))
    }
}

const GREEN_SUMMARY_BYTES: usize = 100 + 16 + COMMITMENT_BYTES;
pub(super) const GREEN_LEAF_HEADER_BYTES: usize = 4 + 4 + 2 + 2 + GREEN_SUMMARY_BYTES;
const GREEN_BRANCH_BYTES: usize = 4 + 4 + 8 + 2 + GREEN_SUMMARY_BYTES;

pub(super) fn packed_event_len(event: PackedGreenEvent) -> usize {
    match event {
        PackedGreenEvent::Enter { .. } => 11,
        PackedGreenEvent::Property(property) => 4 + property.as_bytes().len(),
        PackedGreenEvent::Coverage { atom, .. } => 23 + atom_extra_len(atom),
        PackedGreenEvent::RetypeOpen { property, .. } => {
            12 + property.map_or(0, |property| 3 + property.as_bytes().len())
        }
        PackedGreenEvent::Exit { close, .. } => {
            14 + close.map_or(0, |facts| 3 + facts.as_bytes().len())
        }
    }
}

const fn atom_extra_len(atom: LogicalAtom) -> usize {
    match atom {
        LogicalAtom::TabToSpaces { .. } => 5,
        _ => 0,
    }
}

pub(super) fn encode_packed_event(
    event: PackedGreenEvent,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11RecursiveGreenError> {
    match event {
        PackedGreenEvent::Enter { frame, kind } => {
            write_u8(output, cursor, 1)?;
            write_u64(output, cursor, frame.get())?;
            write_u16(output, cursor, kind.get())?;
        }
        PackedGreenEvent::Property(property) => {
            write_u8(output, cursor, 2)?;
            encode_property(property, output, cursor)?;
        }
        PackedGreenEvent::Coverage {
            physical,
            owner_depth,
            part,
            atom,
        } => {
            validate_atom(atom, physical)?;
            write_u8(output, cursor, 3)?;
            write_u64(output, cursor, physical.bytes())?;
            write_u64(output, cursor, physical.utf16())?;
            write_u32(output, cursor, owner_depth)?;
            write_u8(output, cursor, part as u8)?;
            encode_atom(atom, output, cursor)?;
        }
        PackedGreenEvent::RetypeOpen {
            frame,
            kind,
            property,
        } => {
            write_u8(output, cursor, 4)?;
            write_u64(output, cursor, frame.get())?;
            write_u16(output, cursor, kind.get())?;
            write_u8(output, cursor, u8::from(property.is_some()))?;
            if let Some(property) = property {
                encode_property(property, output, cursor)?;
            }
        }
        PackedGreenEvent::Exit {
            frame,
            final_kind,
            close,
            last_line_blank,
            child,
        } => {
            write_u8(output, cursor, 5)?;
            write_u64(output, cursor, frame.get())?;
            write_u16(output, cursor, final_kind.get())?;
            write_u8(output, cursor, u8::from(close.is_some()))?;
            if let Some(close) = close {
                encode_close(close, output, cursor)?;
            }
            write_u8(output, cursor, u8::from(last_line_blank))?;
            let flags = u8::from(child.ends_blank)
                | (u8::from(child.item_loose_if_nonlast) << 1)
                | (u8::from(child.item_loose_if_last) << 2);
            write_u8(output, cursor, flags)?;
        }
    }
    Ok(())
}

pub(super) fn decode_packed_event(
    input: &[u8],
    cursor: &mut usize,
) -> Result<PackedGreenEvent, M11RecursiveGreenError> {
    match read_u8(input, cursor)? {
        1 => Ok(PackedGreenEvent::Enter {
            frame: decode_frame(read_u64(input, cursor)?)?,
            kind: decode_kind(read_u16(input, cursor)?)?,
        }),
        2 => Ok(PackedGreenEvent::Property(decode_property(input, cursor)?)),
        3 => {
            let physical = decode_metric(read_u64(input, cursor)?, read_u64(input, cursor)?)?;
            let owner_depth = read_u32(input, cursor)?;
            let part = M11RecursiveGreenCoveragePart::decode(read_u8(input, cursor)?)?;
            let atom = decode_atom(input, cursor)?;
            validate_atom(atom, physical)?;
            Ok(PackedGreenEvent::Coverage {
                physical,
                owner_depth,
                part,
                atom,
            })
        }
        4 => {
            let frame = decode_frame(read_u64(input, cursor)?)?;
            let kind = decode_kind(read_u16(input, cursor)?)?;
            let property = match read_u8(input, cursor)? {
                0 => None,
                1 => Some(decode_property(input, cursor)?),
                _ => {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "invalid retype property flag",
                    ));
                }
            };
            Ok(PackedGreenEvent::RetypeOpen {
                frame,
                kind,
                property,
            })
        }
        5 => {
            let frame = decode_frame(read_u64(input, cursor)?)?;
            let final_kind = decode_kind(read_u16(input, cursor)?)?;
            let close = match read_u8(input, cursor)? {
                0 => None,
                1 => Some(decode_close(input, cursor)?),
                _ => return Err(M11RecursiveGreenError::Corrupt("invalid close-facts flag")),
            };
            let last_line_blank = match read_u8(input, cursor)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "invalid last-line-blank flag",
                    ));
                }
            };
            let flags = read_u8(input, cursor)?;
            if flags & !0b111 != 0 {
                return Err(M11RecursiveGreenError::Corrupt(
                    "invalid closed-child flags",
                ));
            }
            Ok(PackedGreenEvent::Exit {
                frame,
                final_kind,
                close,
                last_line_blank,
                child: M11RecursiveGreenClosedChild::new(
                    flags & 1 != 0,
                    flags & 2 != 0,
                    flags & 4 != 0,
                ),
            })
        }
        _ => Err(M11RecursiveGreenError::Corrupt(
            "unknown recursive-green event tag",
        )),
    }
}

fn encode_property(
    property: M11RecursiveGreenPropertyChunk,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11RecursiveGreenError> {
    write_u16(output, cursor, property.tag.get())?;
    write_u8(output, cursor, property.len)?;
    write_bytes(output, cursor, property.as_bytes())
}

fn decode_property(
    input: &[u8],
    cursor: &mut usize,
) -> Result<M11RecursiveGreenPropertyChunk, M11RecursiveGreenError> {
    let tag = decode_tag(read_u16(input, cursor)?)?;
    let len = usize::from(read_u8(input, cursor)?);
    let bytes = read_bytes(input, cursor, len)?;
    M11RecursiveGreenPropertyChunk::new(tag, bytes)
        .map_err(|_| M11RecursiveGreenError::Corrupt("invalid property chunk"))
}

fn encode_close(
    facts: M11RecursiveGreenCloseFacts,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11RecursiveGreenError> {
    write_u16(output, cursor, facts.tag.get())?;
    write_u8(output, cursor, facts.len)?;
    write_bytes(output, cursor, facts.as_bytes())
}

fn decode_close(
    input: &[u8],
    cursor: &mut usize,
) -> Result<M11RecursiveGreenCloseFacts, M11RecursiveGreenError> {
    let tag = decode_tag(read_u16(input, cursor)?)?;
    let len = usize::from(read_u8(input, cursor)?);
    let bytes = read_bytes(input, cursor, len)?;
    M11RecursiveGreenCloseFacts::new(tag, bytes)
        .map_err(|_| M11RecursiveGreenError::Corrupt("invalid close facts"))
}

fn encode_atom(
    atom: LogicalAtom,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11RecursiveGreenError> {
    let tag = match atom {
        LogicalAtom::None => 0,
        LogicalAtom::Identity => 1,
        LogicalAtom::TabToSpaces { .. } => 2,
        LogicalAtom::HiddenUpstream => 3,
        LogicalAtom::LfToLf => 4,
        LogicalAtom::CrLfToLf => 5,
        LogicalAtom::LoneCrToLf => 6,
        LogicalAtom::NulToReplacement => 7,
    };
    write_u8(output, cursor, tag)?;
    if let LogicalAtom::TabToSpaces {
        target_owner_depth,
        spaces,
    } = atom
    {
        write_u32(output, cursor, target_owner_depth)?;
        write_u8(output, cursor, spaces)?;
    }
    Ok(())
}

fn decode_atom(input: &[u8], cursor: &mut usize) -> Result<LogicalAtom, M11RecursiveGreenError> {
    match read_u8(input, cursor)? {
        0 => Ok(LogicalAtom::None),
        1 => Ok(LogicalAtom::Identity),
        2 => Ok(LogicalAtom::TabToSpaces {
            target_owner_depth: read_u32(input, cursor)?,
            spaces: read_u8(input, cursor)?,
        }),
        3 => Ok(LogicalAtom::HiddenUpstream),
        4 => Ok(LogicalAtom::LfToLf),
        5 => Ok(LogicalAtom::CrLfToLf),
        6 => Ok(LogicalAtom::LoneCrToLf),
        7 => Ok(LogicalAtom::NulToReplacement),
        _ => Err(M11RecursiveGreenError::Corrupt("unknown logical atom")),
    }
}

pub(super) struct DecodedGreenLeaf<'payload> {
    pub(super) events: u16,
    pub(super) summary: RecursiveGreenSummary,
    pub(super) event_bytes: &'payload [u8],
}

pub(super) fn encode_leaf_header(
    page: &mut [u8; ARENA_PAGE_BYTES],
    events: u16,
    data_bytes: usize,
    summary: RecursiveGreenSummary,
) -> Result<(), M11RecursiveGreenError> {
    if events == 0 || data_bytes == 0 || usize::from(events) > GREEN_EVENTS_PER_PAGE_MAX {
        return Err(M11RecursiveGreenError::Corrupt(
            "invalid recursive-green leaf shape",
        ));
    }
    validate_summary(summary)?;
    let mut cursor = 0;
    write_bytes(page, &mut cursor, &GREEN_LEAF_MAGIC)?;
    write_u32(page, &mut cursor, GREEN_SCHEMA)?;
    write_u16(page, &mut cursor, events)?;
    write_u16(
        page,
        &mut cursor,
        u16::try_from(data_bytes).map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )?;
    encode_summary(summary, page, &mut cursor)?;
    if cursor != GREEN_LEAF_HEADER_BYTES {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green leaf header size changed",
        ));
    }
    Ok(())
}

pub(super) fn decode_leaf<'payload>(
    payload: &'payload [u8],
    inspection: &mut SequenceSpecInspection,
) -> Result<Option<DecodedGreenLeaf<'payload>>, M11RecursiveGreenError> {
    if payload.get(..4) != Some(GREEN_LEAF_MAGIC.as_slice()) {
        return Ok(None);
    }
    if payload.len() < GREEN_LEAF_HEADER_BYTES {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green leaf is truncated",
        ));
    }
    inspection
        .charge_payload_bytes(payload.len())
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let mut cursor = 4;
    if read_u32(payload, &mut cursor)? != GREEN_SCHEMA {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green leaf schema changed",
        ));
    }
    let events = read_u16(payload, &mut cursor)?;
    let data_bytes = usize::from(read_u16(payload, &mut cursor)?);
    let claimed = decode_summary(payload, &mut cursor)?;
    if cursor != GREEN_LEAF_HEADER_BYTES
        || events == 0
        || usize::from(events) > GREEN_EVENTS_PER_PAGE_MAX
        || payload.len() != GREEN_LEAF_HEADER_BYTES + data_bytes
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "invalid recursive-green leaf metadata",
        ));
    }
    let event_bytes = &payload[cursor..];
    let mut event_cursor = 0;
    let mut observed = RecursiveGreenSummary::empty();
    for _ in 0..events {
        let event = decode_packed_event(event_bytes, &mut event_cursor)?;
        observed = observed.checked_followed_by(packed_event_summary(event)?)?;
    }
    inspection
        .charge_hashed_items(usize::from(events))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if event_cursor != event_bytes.len() || observed != claimed {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green leaf summary mismatch",
        ));
    }
    Ok(Some(DecodedGreenLeaf {
        events,
        summary: observed,
        event_bytes,
    }))
}

/// Counts the canonical events packed into one recursive-Green leaf.
/// Branch and unrelated payloads contribute no records.
pub(crate) fn m11_recursive_green_canonical_record_count(payload: &[u8]) -> u32 {
    let mut inspection = SequenceSpecInspection::default();
    decode_leaf(payload, &mut inspection)
        .ok()
        .flatten()
        .map_or(0, |leaf| u32::from(leaf.events))
}

pub(super) struct RecursiveGreenSpec;

impl SequenceSpec for RecursiveGreenSpec {
    type Summary = RecursiveGreenSummary;
    type Error = M11RecursiveGreenError;

    fn leaf_summary(
        payload: &[u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<Self::Summary>, Self::Error> {
        decode_leaf(payload, inspection).map(|leaf| leaf.map(|leaf| leaf.summary))
    }

    fn branch_measure(
        payload: &[u8],
        inspection: &mut SequenceSpecInspection,
    ) -> Result<Option<SequenceMeasure<Self::Summary>>, Self::Error> {
        if payload.get(..4) != Some(GREEN_BRANCH_MAGIC.as_slice()) {
            return Ok(None);
        }
        if payload.len() != GREEN_BRANCH_BYTES {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green branch length changed",
            ));
        }
        inspection
            .charge_payload_bytes(payload.len())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let mut cursor = 4;
        if read_u32(payload, &mut cursor)? != GREEN_SCHEMA {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green branch schema changed",
            ));
        }
        let leaves = read_u64(payload, &mut cursor)?;
        let height = read_u16(payload, &mut cursor)?;
        let summary = decode_summary(payload, &mut cursor)?;
        if cursor != payload.len() || leaves < 2 || height < 2 {
            return Err(M11RecursiveGreenError::Corrupt(
                "invalid recursive-green branch metadata",
            ));
        }
        Ok(Some(SequenceMeasure::new(summary, leaves, height)))
    }

    fn encode_branch(
        measure: SequenceMeasure<Self::Summary>,
        output: &mut [u8; ARENA_PAGE_BYTES],
    ) -> Result<usize, Self::Error> {
        validate_summary(measure.summary())?;
        let mut cursor = 0;
        write_bytes(output, &mut cursor, &GREEN_BRANCH_MAGIC)?;
        write_u32(output, &mut cursor, GREEN_SCHEMA)?;
        write_u64(output, &mut cursor, measure.leaves())?;
        write_u16(output, &mut cursor, measure.height())?;
        encode_summary(measure.summary(), output, &mut cursor)?;
        if cursor != GREEN_BRANCH_BYTES {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green branch size changed",
            ));
        }
        Ok(cursor)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.checked_followed_by(right)
    }

    fn invalid(message: &'static str) -> Self::Error {
        M11RecursiveGreenError::Corrupt(message)
    }
}

fn encode_summary(
    summary: RecursiveGreenSummary,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11RecursiveGreenError> {
    validate_summary(summary)?;
    write_u64(output, cursor, summary.physical_bytes)?;
    write_u64(output, cursor, summary.physical_utf16)?;
    write_u64(output, cursor, summary.logical_bytes)?;
    write_u64(output, cursor, summary.logical_utf16)?;
    write_u64(output, cursor, summary.events)?;
    write_u64(output, cursor, summary.enters)?;
    write_u64(output, cursor, summary.renderable_row_exits)?;
    write_u64(output, cursor, summary.canonical_event_bytes)?;
    encode_event_commitment(summary.canonical_commitment, output, cursor)?;
    write_u64(output, cursor, summary.max_frame_id)?;
    write_i64(output, cursor, summary.balance)?;
    write_i64(output, cursor, summary.minimum_prefix)?;
    write_i64(
        output,
        cursor,
        summary.minimum_closed_depth.unwrap_or(i64::MIN),
    )?;
    write_u8(output, cursor, u8::from(summary.oldest_open.is_some()))?;
    let witness = summary.oldest_open.unwrap_or(GreenOpenWitness {
        frame: M11RecursiveGreenFrameId(1),
        kind: M11RecursiveGreenKind(1),
        event_ordinal: 0,
    });
    write_u64(output, cursor, witness.frame.get())?;
    write_u16(output, cursor, witness.kind.get())?;
    write_u64(output, cursor, witness.event_ordinal)?;
    write_u8(
        output,
        cursor,
        encode_child_fold(summary.outermost_children),
    )?;
    Ok(())
}

fn decode_summary(
    input: &[u8],
    cursor: &mut usize,
) -> Result<RecursiveGreenSummary, M11RecursiveGreenError> {
    let physical_bytes = read_u64(input, cursor)?;
    let physical_utf16 = read_u64(input, cursor)?;
    let logical_bytes = read_u64(input, cursor)?;
    let logical_utf16 = read_u64(input, cursor)?;
    let events = read_u64(input, cursor)?;
    let enters = read_u64(input, cursor)?;
    let renderable_row_exits = read_u64(input, cursor)?;
    let canonical_event_bytes = read_u64(input, cursor)?;
    let canonical_commitment = decode_event_commitment(input, cursor)?;
    let max_frame_id = read_u64(input, cursor)?;
    let balance = read_i64(input, cursor)?;
    let minimum_prefix = read_i64(input, cursor)?;
    let minimum_closed_depth = match read_i64(input, cursor)? {
        i64::MIN => None,
        value => Some(value),
    };
    let witness_present = read_u8(input, cursor)?;
    let witness_frame = read_u64(input, cursor)?;
    let witness_kind = read_u16(input, cursor)?;
    let witness_ordinal = read_u64(input, cursor)?;
    let outermost_children = decode_child_fold(read_u8(input, cursor)?)?;
    let oldest_open = match witness_present {
        0 => None,
        1 => Some(GreenOpenWitness {
            frame: decode_frame(witness_frame)?,
            kind: decode_kind(witness_kind)?,
            event_ordinal: witness_ordinal,
        }),
        _ => return Err(M11RecursiveGreenError::Corrupt("invalid open-witness flag")),
    };
    let summary = RecursiveGreenSummary {
        physical_bytes,
        physical_utf16,
        logical_bytes,
        logical_utf16,
        events,
        enters,
        renderable_row_exits,
        canonical_event_bytes,
        canonical_commitment,
        max_frame_id,
        balance,
        minimum_prefix,
        minimum_closed_depth,
        oldest_open,
        outermost_children,
    };
    validate_summary(summary)?;
    Ok(summary)
}

fn encode_event_commitment(
    commitment: RecursiveGreenCommitment,
    output: &mut [u8],
    cursor: &mut usize,
) -> Result<(), M11RecursiveGreenError> {
    for value in commitment.hash {
        write_u64(output, cursor, value)?;
    }
    for value in commitment.factor {
        write_u64(output, cursor, value)?;
    }
    Ok(())
}

fn decode_event_commitment(
    input: &[u8],
    cursor: &mut usize,
) -> Result<RecursiveGreenCommitment, M11RecursiveGreenError> {
    let mut hash = [0_u64; COMMITMENT_LANES];
    let mut factor = [0_u64; COMMITMENT_LANES];
    for value in &mut hash {
        *value = read_u64(input, cursor)?;
    }
    for value in &mut factor {
        *value = read_u64(input, cursor)?;
    }
    RecursiveGreenCommitment::from_lanes(hash, factor)
}

const fn encode_child_fold(fold: M11RecursiveGreenChildFold) -> u8 {
    fold.had_child as u8
        | ((fold.any_nonlast_child_ends_blank as u8) << 1)
        | ((fold.last_child_ends_blank as u8) << 2)
        | ((fold.list_loose_before_last as u8) << 3)
        | ((fold.last_item_loose_if_nonlast as u8) << 4)
        | ((fold.last_item_loose_if_last as u8) << 5)
}

fn decode_child_fold(flags: u8) -> Result<M11RecursiveGreenChildFold, M11RecursiveGreenError> {
    if flags & !0b11_1111 != 0 {
        return Err(M11RecursiveGreenError::Corrupt("invalid child-fold flags"));
    }
    Ok(M11RecursiveGreenChildFold {
        had_child: flags & 1 != 0,
        any_nonlast_child_ends_blank: flags & 2 != 0,
        last_child_ends_blank: flags & 4 != 0,
        list_loose_before_last: flags & 8 != 0,
        last_item_loose_if_nonlast: flags & 16 != 0,
        last_item_loose_if_last: flags & 32 != 0,
    })
}

fn decode_frame(value: u64) -> Result<M11RecursiveGreenFrameId, M11RecursiveGreenError> {
    M11RecursiveGreenFrameId::new(value).ok_or(M11RecursiveGreenError::Corrupt("zero frame id"))
}

fn decode_kind(value: u16) -> Result<M11RecursiveGreenKind, M11RecursiveGreenError> {
    M11RecursiveGreenKind::new(value).ok_or(M11RecursiveGreenError::Corrupt("zero kind"))
}

fn decode_tag(value: u16) -> Result<M11RecursiveGreenFactTag, M11RecursiveGreenError> {
    M11RecursiveGreenFactTag::new(value).ok_or(M11RecursiveGreenError::Corrupt("zero fact tag"))
}

fn decode_metric(
    bytes: u64,
    utf16: u64,
) -> Result<M11RecursiveGreenSourceMetric, M11RecursiveGreenError> {
    M11RecursiveGreenSourceMetric::new(bytes, utf16)
        .filter(|metric| !metric.is_empty())
        .ok_or(M11RecursiveGreenError::Corrupt("invalid source metric"))
}

fn write_bytes(
    output: &mut [u8],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), M11RecursiveGreenError> {
    let end = cursor
        .checked_add(bytes.len())
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let target = output
        .get_mut(*cursor..end)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "recursive-green encoding exceeds page",
        ))?;
    target.copy_from_slice(bytes);
    *cursor = end;
    Ok(())
}

fn read_bytes<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], M11RecursiveGreenError> {
    let end = cursor
        .checked_add(len)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let bytes = input
        .get(*cursor..end)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "recursive-green payload is truncated",
        ))?;
    *cursor = end;
    Ok(bytes)
}

fn write_u8(
    output: &mut [u8],
    cursor: &mut usize,
    value: u8,
) -> Result<(), M11RecursiveGreenError> {
    write_bytes(output, cursor, &[value])
}
fn write_u16(
    output: &mut [u8],
    cursor: &mut usize,
    value: u16,
) -> Result<(), M11RecursiveGreenError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}
fn write_u32(
    output: &mut [u8],
    cursor: &mut usize,
    value: u32,
) -> Result<(), M11RecursiveGreenError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}
fn write_u64(
    output: &mut [u8],
    cursor: &mut usize,
    value: u64,
) -> Result<(), M11RecursiveGreenError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}
fn write_i64(
    output: &mut [u8],
    cursor: &mut usize,
    value: i64,
) -> Result<(), M11RecursiveGreenError> {
    write_bytes(output, cursor, &value.to_le_bytes())
}
fn read_u8(input: &[u8], cursor: &mut usize) -> Result<u8, M11RecursiveGreenError> {
    Ok(read_bytes(input, cursor, 1)?[0])
}
fn read_u16(input: &[u8], cursor: &mut usize) -> Result<u16, M11RecursiveGreenError> {
    Ok(u16::from_le_bytes(
        read_bytes(input, cursor, 2)?.try_into().expect("two bytes"),
    ))
}
fn read_u32(input: &[u8], cursor: &mut usize) -> Result<u32, M11RecursiveGreenError> {
    Ok(u32::from_le_bytes(
        read_bytes(input, cursor, 4)?
            .try_into()
            .expect("four bytes"),
    ))
}
fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64, M11RecursiveGreenError> {
    Ok(u64::from_le_bytes(
        read_bytes(input, cursor, 8)?
            .try_into()
            .expect("eight bytes"),
    ))
}
fn read_i64(input: &[u8], cursor: &mut usize) -> Result<i64, M11RecursiveGreenError> {
    Ok(i64::from_le_bytes(
        read_bytes(input, cursor, 8)?
            .try_into()
            .expect("eight bytes"),
    ))
}

const _: () = assert!(GREEN_SUMMARY_BYTES == 180);
const _: () = assert!(GREEN_LEAF_HEADER_BYTES == 192);
const _: () = assert!(GREEN_BRANCH_BYTES == 198);
