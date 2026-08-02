//! Typed persistent inline Projection pages.
//!
//! This is intentionally a narrow facade over [`crate::parser_pages`]. The
//! generic page tree owns persistence and lifecycle; this module owns the
//! canonical inline schema, exact source/profile authority, cryptographic
//! ordered commitment, and typed replay validation.
//!
//! Logical Projection pages are explicit parser inputs. Each page encodes an
//! anchor delta from the previous logical page, while facts are relative to
//! that page anchor. A uniform source shift therefore changes at most the
//! boundary page rather than rewriting every suffix fact. The generic arena
//! may pack several logical pages into one physical leaf without changing
//! their canonical bytes.
//!
//! The checkpoint cursor starts at page zero and reconstructs anchors
//! linearly. A future typed measured root can seek directly without changing
//! this encoding by summarizing, for every prefix, (a) the sum of page-anchor
//! deltas and (b) the terminal fact-start delta used for cross-page ordering.
//! Those two prefix measures are the exact seed needed to validate an
//! arbitrary located page; neither belongs in the suffix page payload.
//!
//! The linear BLAKE3 commitment is sufficient to authenticate this full-build
//! checkpoint, but it is not the final exact-base splice commitment: replaying
//! it after a splice is O(all pages). Production typed measured summaries must
//! add a strong composable/Merkle commitment (or an equivalent canonical root
//! digest) beside the prefix-anchor measures. `IFO2` proves the stable logical
//! page payload and ownership facade; it does not claim the final indexed
//! splice-root schema.

use std::fmt;
use std::ops::Range;

use crate::candidate_manifest::StrongIdentity;
use crate::document::DocumentRuntime;
use crate::identity::ArenaId;
use crate::measured_sequence::{maximum_avl_height, SequenceInspectionReceipt};
use crate::parser_pages::{
    imported_m11_parser_page_record_at, imported_m11_parser_page_record_at_inspected,
    validate_imported_m11_parser_page_root, M11ImportedParserPageRootClaim, M11ParserPageBuild,
    M11ParserPageBuildReceipt, M11ParserPageBuildStatus, M11ParserPageCursor,
    M11ParserPageCursorPoll, M11ParserPageError, M11ParserPageReclaimPoll, M11ParserPageRecord,
    M11ParserPageRoot, M11ParserSourceRangeAuthority, M11_PARSER_PAGE_MAX_RECORD_BYTES,
};
use crate::storage::{ArenaBuildOwner, ArenaBuildSession, PageArena};
use crate::{ParserProfileId, SourceSnapshotLease, SourceVersion};

const INLINE_PROJECTION_STREAM_TAG: u32 = u32::from_le_bytes(*b"IFO2");
const INLINE_PROJECTION_SCHEMA: u32 = 2;
const INLINE_PROJECTION_PAGE_MAGIC: [u8; 4] = *b"IFP2";
const INLINE_PROJECTION_PAGE_HEADER_BYTES: usize = 20;
const INLINE_PROJECTION_FACT_BYTES: usize = 20;
const INLINE_PROJECTION_COMMITMENT_DOMAIN: &[u8] = b"flark.inline-projection.v2\0";
const INLINE_PROJECTION_COMMITMENT_TRAILER: &[u8] = b"flark.inline-projection.end.v2\0";
const INLINE_LINK_VALUE_STREAM_TAG: u32 = u32::from_le_bytes(*b"ILV1");
const INLINE_LINK_VALUE_ENTRY_BYTES: usize = 32;
const INLINE_LINK_VALUE_CHUNK_BYTES: usize = M11_PARSER_PAGE_MAX_RECORD_BYTES;
const INLINE_LINK_VALUE_COMMITMENT_DOMAIN: &[u8] = b"flark.inline-link-values.v1\0";
const INLINE_LINK_VALUE_COMMITMENT_TRAILER: &[u8] = b"flark.inline-link-values.end.v1\0";
const INLINE_LINK_VALUE_TITLE_PRESENT: u32 = 1;

// Schema 3 is the persistent block-Projection lane and schema 4 was the
// fact-only inline lane. Schema 5 is an intentionally incompatible two-root
// bundle: child zero is the fixed-width fact tree and child one, when present,
// is the variable-width link-value tree.
pub(crate) const PERSISTENT_INLINE_PROJECTION_ROLE_SCHEMA: u32 = 5;
const PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_MAGIC: [u8; 4] = *b"IPB5";
const PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_VERSION: u32 = 1;
pub(crate) const PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES: usize = 280;
type M11InlineProjectionTransportBundleParts = (
    Option<ArenaId>,
    Option<ArenaId>,
    [u8; PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES],
);

/// Maximum facts in one parser-defined logical Projection page.
pub const M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX: usize = (M11_PARSER_PAGE_MAX_RECORD_BYTES
    - INLINE_PROJECTION_PAGE_HEADER_BYTES)
    / INLINE_PROJECTION_FACT_BYTES;

/// Selected inline semantics carried by the first persistent Projection root.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum M11InlineProjectionKind {
    Emphasis = 1,
    Strong = 2,
    Code = 3,
    Strikethrough = 4,
    AutolinkUri = 5,
    AutolinkEmail = 6,
    BackslashEscape = 7,
    HardLineBreak = 8,
    CharacterReference = 9,
    DirectLink = 10,
    DirectImage = 11,
    ReferenceLink = 12,
    ReferenceImage = 13,
}

impl M11InlineProjectionKind {
    fn decode(value: u8) -> Result<Self, M11InlineProjectionError> {
        match value {
            1 => Ok(Self::Emphasis),
            2 => Ok(Self::Strong),
            3 => Ok(Self::Code),
            4 => Ok(Self::Strikethrough),
            5 => Ok(Self::AutolinkUri),
            6 => Ok(Self::AutolinkEmail),
            7 => Ok(Self::BackslashEscape),
            8 => Ok(Self::HardLineBreak),
            9 => Ok(Self::CharacterReference),
            10 => Ok(Self::DirectLink),
            11 => Ok(Self::DirectImage),
            12 => Ok(Self::ReferenceLink),
            13 => Ok(Self::ReferenceImage),
            _ => Err(M11InlineProjectionError::Malformed(
                "inline Projection fact kind is unsupported",
            )),
        }
    }

    const fn is_autolink(self) -> bool {
        matches!(self, Self::AutolinkUri | Self::AutolinkEmail)
    }

    const fn is_direct_target(self) -> bool {
        matches!(self, Self::DirectLink | Self::DirectImage)
    }

    const fn is_reference_target(self) -> bool {
        matches!(self, Self::ReferenceLink | Self::ReferenceImage)
    }

    const fn has_link_value(self) -> bool {
        self.is_direct_target() || self.is_reference_target()
    }

    const fn has_collapsed_closer(self) -> bool {
        matches!(self, Self::BackslashEscape | Self::HardLineBreak)
    }
}

/// Code content replaces physical line endings with spaces.
pub const M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS: u8 = 1;
/// Code content removes one edge space under the CommonMark code-span rule.
pub const M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE: u8 = 2;
const M11_INLINE_PROJECTION_CODE_FLAGS: u8 = M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS
    | M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE;
/// A markerless URI autolink whose exact source starts with `www.`.
///
/// Consumers prepend `http://` only when constructing the link destination;
/// the visible content and canonical source geometry remain unchanged.
pub const M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW: u8 = 1;
const M11_INLINE_PROJECTION_AUTOLINK_URI_FLAGS: u8 = M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW;

/// Largest exact `&...;` source spelling admitted by Comrak's pinned
/// character-reference decoder: one leading ampersand plus at most 32 bytes
/// examined by the decoder.
pub const M11_INLINE_CHARACTER_REFERENCE_SOURCE_MAX_BYTES: u32 = 33;
/// Maximum public `FLKIV001` payload returned for one certified leaf.
pub const M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES: usize = 64 * 1024;
/// The 16-byte header plus at least one 32-byte entry bounds entry density.
pub const M11_INLINE_LINK_VALUES_MAX_ENTRIES: u32 =
    ((M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES - 16) / INLINE_LINK_VALUE_ENTRY_BYTES) as u32;

/// Parser-authored cooked target data for one link or image fact.
///
/// Direct-link geometry is relative to the same leaf as the parent fact.
/// Reference-link geometry is document-absolute and names the winning
/// definition's source cuts. The destination range excludes optional angle
/// delimiters. A title range, when present, includes its complete source
/// delimiters; an empty cooked title therefore remains distinguishable from
/// an absent title. The parent fact kind is the coordinate-basis authority;
/// the companion stream does not carry a second, forgeable mode bit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11InlineLinkValue {
    parent_fact_ordinal: u32,
    destination_source_range: Range<u32>,
    title_source_range: Option<Range<u32>>,
    cooked_destination: Box<str>,
    cooked_title: Option<Box<str>>,
}

impl M11InlineLinkValue {
    pub fn new(
        parent_fact_ordinal: u32,
        destination_source_range: Range<u32>,
        title_source_range: Option<Range<u32>>,
        cooked_destination: impl Into<Box<str>>,
        cooked_title: Option<Box<str>>,
    ) -> Result<Self, M11InlineProjectionError> {
        if destination_source_range.start > destination_source_range.end
            || title_source_range
                .as_ref()
                .is_some_and(|range| range.start >= range.end)
            || title_source_range.is_some() != cooked_title.is_some()
        {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "inline link value title presence or source geometry is invalid",
            ));
        }
        Ok(Self {
            parent_fact_ordinal,
            destination_source_range,
            title_source_range,
            cooked_destination: cooked_destination.into(),
            cooked_title,
        })
    }

    #[must_use]
    pub const fn parent_fact_ordinal(&self) -> u32 {
        self.parent_fact_ordinal
    }

    #[must_use]
    pub const fn destination_source_range(&self) -> &Range<u32> {
        &self.destination_source_range
    }

    #[must_use]
    pub const fn title_source_range(&self) -> Option<&Range<u32>> {
        self.title_source_range.as_ref()
    }

    #[must_use]
    pub const fn cooked_destination(&self) -> &str {
        &self.cooked_destination
    }

    #[must_use]
    pub fn cooked_title(&self) -> Option<&str> {
        self.cooked_title.as_deref()
    }

    fn encoded_len(&self) -> Result<usize, M11InlineProjectionError> {
        INLINE_LINK_VALUE_ENTRY_BYTES
            .checked_add(self.cooked_destination.len())
            .and_then(|value| {
                value.checked_add(self.cooked_title.as_ref().map_or(0, |title| title.len()))
            })
            .ok_or(M11InlineProjectionError::CoordinateOverflow)
    }

    fn validate_against_fact(
        &self,
        fact: M11InlineProjectionFact,
        document_source_len: usize,
    ) -> Result<(), M11InlineProjectionError> {
        if !fact.kind.has_link_value() {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "inline link value targets a non-link fact",
            ));
        }
        if fact.kind.is_direct_target() {
            let fact_range = fact.relative_range();
            let content_range = fact.relative_content_range();
            if self.destination_source_range.start < content_range.end
                || self.destination_source_range.end > fact_range.end
                || self.title_source_range.as_ref().is_some_and(|title| {
                    title.start < self.destination_source_range.end || title.end > fact_range.end
                })
            {
                return Err(M11InlineProjectionError::InvalidLinkValue(
                    "direct inline link value cuts are outside the parent closer",
                ));
            }
            return Ok(());
        }

        let destination_end = usize::try_from(self.destination_source_range.end)
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        let invalid_title = self.title_source_range.as_ref().is_some_and(|title| {
            title.start < self.destination_source_range.end
                || usize::try_from(title.end)
                    .ok()
                    .is_none_or(|end| end > document_source_len)
        });
        if destination_end > document_source_len || invalid_title {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "reference inline link value cuts are outside document source or out of order",
            ));
        }
        Ok(())
    }

    fn encode_into(&self, output: &mut Vec<u8>) -> Result<(), M11InlineProjectionError> {
        let cooked_destination_len = u32::try_from(self.cooked_destination.len())
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        let cooked_title_len =
            u32::try_from(self.cooked_title.as_ref().map_or(0, |title| title.len()))
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        let title_present = self.title_source_range.is_some();
        let title_range = self.title_source_range.clone().unwrap_or(0..0);
        let destination_len = self
            .destination_source_range
            .end
            .checked_sub(self.destination_source_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let title_len = title_range
            .end
            .checked_sub(title_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        output.extend_from_slice(&self.parent_fact_ordinal.to_le_bytes());
        output.extend_from_slice(
            &(if title_present {
                INLINE_LINK_VALUE_TITLE_PRESENT
            } else {
                0
            })
            .to_le_bytes(),
        );
        output.extend_from_slice(&self.destination_source_range.start.to_le_bytes());
        output.extend_from_slice(&destination_len.to_le_bytes());
        output.extend_from_slice(&title_range.start.to_le_bytes());
        output.extend_from_slice(&title_len.to_le_bytes());
        output.extend_from_slice(&cooked_destination_len.to_le_bytes());
        output.extend_from_slice(&cooked_title_len.to_le_bytes());
        output.extend_from_slice(self.cooked_destination.as_bytes());
        if let Some(title) = self.cooked_title.as_ref() {
            output.extend_from_slice(title.as_bytes());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11InlineProjectionFactPayload {
    Marked {
        content_offset: u32,
        content_len: u32,
    },
    CharacterReference {
        first: char,
        second: Option<char>,
    },
}

/// One typed fact in root-relative UTF-8 byte coordinates.
///
/// Canonical pages do not store `relative_start` directly. They store a
/// logical-page anchor delta and then this fact's start relative to that
/// anchor, keeping suffix page bytes independent of absolute document offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InlineProjectionFact {
    kind: M11InlineProjectionKind,
    flags: u8,
    relative_start: u32,
    relative_len: u32,
    payload: M11InlineProjectionFactPayload,
}

impl M11InlineProjectionFact {
    pub fn new(
        kind: M11InlineProjectionKind,
        flags: u8,
        relative_range: Range<u32>,
        relative_content_range: Range<u32>,
    ) -> Result<Self, M11InlineProjectionError> {
        if kind == M11InlineProjectionKind::CharacterReference {
            return Err(M11InlineProjectionError::InvalidFact(
                "character references require their typed scalar constructor",
            ));
        }
        if relative_range.start >= relative_range.end
            || relative_content_range.start <= relative_range.start
            || relative_content_range.end > relative_range.end
            || relative_content_range.start > relative_content_range.end
            || (!kind.has_collapsed_closer() && relative_content_range.end == relative_range.end)
        {
            return Err(M11InlineProjectionError::InvalidFact(
                "inline Projection content must be enclosed by nonempty markers",
            ));
        }
        if kind == M11InlineProjectionKind::Code {
            if flags & !M11_INLINE_PROJECTION_CODE_FLAGS != 0 {
                return Err(M11InlineProjectionError::InvalidFact(
                    "inline code fact uses unknown flags",
                ));
            }
        } else if kind.is_autolink() {
            if flags != 0 {
                return Err(M11InlineProjectionError::InvalidFact(
                    "angle autolink fact cannot carry bare-autolink flags",
                ));
            }
        } else if flags != 0 {
            return Err(M11InlineProjectionError::InvalidFact(
                "non-code inline fact carries code flags",
            ));
        }
        let relative_len = relative_range
            .end
            .checked_sub(relative_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let content_offset = relative_content_range
            .start
            .checked_sub(relative_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let content_len = relative_content_range
            .end
            .checked_sub(relative_content_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let content_end_offset = content_offset
            .checked_add(content_len)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let closer_len = relative_len
            .checked_sub(content_end_offset)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        if kind == M11InlineProjectionKind::BackslashEscape
            && (relative_len != 2 || content_offset != 1 || content_len != 1 || closer_len != 0)
        {
            return Err(M11InlineProjectionError::InvalidFact(
                "backslash escape fact must preserve one-byte opener and escaped content",
            ));
        }
        if kind == M11InlineProjectionKind::HardLineBreak
            && (!(1..=2).contains(&content_len) || closer_len != 0)
        {
            return Err(M11InlineProjectionError::InvalidFact(
                "hard line break fact must preserve a nonempty marker and exact physical EOL",
            ));
        }
        if kind.is_autolink() && (content_offset != 1 || closer_len != 1) {
            return Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact must preserve one-byte angle markers",
            ));
        }
        Ok(Self {
            kind,
            flags,
            relative_start: relative_range.start,
            relative_len,
            payload: M11InlineProjectionFactPayload::Marked {
                content_offset,
                content_len,
            },
        })
    }

    /// Creates one parser-authenticated GFM bare URI or email autolink.
    ///
    /// Bare autolinks have no hidden delimiters: their source and content
    /// ranges are identical. A URI beginning with `www.` carries
    /// [`M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW`]; scheme URIs and email
    /// autolinks carry no flags and need no variable-width value companion.
    pub fn new_bare_autolink(
        kind: M11InlineProjectionKind,
        flags: u8,
        relative_range: Range<u32>,
    ) -> Result<Self, M11InlineProjectionError> {
        if !kind.is_autolink() {
            return Err(M11InlineProjectionError::InvalidFact(
                "bare autolink fact requires a URI or email kind",
            ));
        }
        let relative_len = relative_range
            .end
            .checked_sub(relative_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        if relative_len == 0 {
            return Err(M11InlineProjectionError::InvalidFact(
                "bare autolink source extent must be nonempty",
            ));
        }
        validate_bare_autolink_flags(kind, flags)?;
        Ok(Self {
            kind,
            flags,
            relative_start: relative_range.start,
            relative_len,
            payload: M11InlineProjectionFactPayload::Marked {
                content_offset: 0,
                content_len: relative_len,
            },
        })
    }

    /// Creates one parser-authenticated CommonMark character reference.
    ///
    /// CommonMark's pinned HTML entity table expands to one or two Unicode
    /// scalar values. Carrying those scalars directly keeps the canonical
    /// 20-byte fact fixed-width and gives every decoder constant work without
    /// asking another runtime to interpret the source spelling.
    pub fn new_character_reference(
        relative_range: Range<u32>,
        first: char,
        second: Option<char>,
    ) -> Result<Self, M11InlineProjectionError> {
        let relative_len = relative_range
            .end
            .checked_sub(relative_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        if !(4..=M11_INLINE_CHARACTER_REFERENCE_SOURCE_MAX_BYTES).contains(&relative_len) {
            return Err(M11InlineProjectionError::InvalidFact(
                "character reference source extent is outside the bounded grammar",
            ));
        }
        if second == Some('\0') {
            return Err(M11InlineProjectionError::InvalidFact(
                "character reference second scalar must be nonzero",
            ));
        }
        Ok(Self {
            kind: M11InlineProjectionKind::CharacterReference,
            flags: 0,
            relative_start: relative_range.start,
            relative_len,
            payload: M11InlineProjectionFactPayload::CharacterReference { first, second },
        })
    }

    #[must_use]
    pub const fn kind(self) -> M11InlineProjectionKind {
        self.kind
    }

    #[must_use]
    pub const fn flags(self) -> u8 {
        self.flags
    }

    #[must_use]
    pub fn relative_range(self) -> Range<u32> {
        self.relative_start..self.relative_start + self.relative_len
    }

    #[must_use]
    pub fn relative_content_range(self) -> Range<u32> {
        match self.payload {
            M11InlineProjectionFactPayload::Marked {
                content_offset,
                content_len,
            } => {
                let start = self.relative_start + content_offset;
                start..start + content_len
            }
            M11InlineProjectionFactPayload::CharacterReference { .. } => self.relative_range(),
        }
    }

    /// Cooked Unicode scalar value(s) for a character reference.
    ///
    /// Non-character-reference facts return `None`; their final two wire words
    /// retain the existing content-geometry meaning.
    #[must_use]
    pub const fn character_reference(self) -> Option<(char, Option<char>)> {
        match self.payload {
            M11InlineProjectionFactPayload::CharacterReference { first, second } => {
                Some((first, second))
            }
            M11InlineProjectionFactPayload::Marked { .. } => None,
        }
    }

    #[must_use]
    pub fn absolute_range(self, descriptor: &M11InlineProjectionDescriptor) -> Range<u32> {
        let relative = self.relative_range();
        (descriptor.source_range.start + relative.start)
            ..(descriptor.source_range.start + relative.end)
    }

    #[must_use]
    pub fn absolute_content_range(self, descriptor: &M11InlineProjectionDescriptor) -> Range<u32> {
        let relative = self.relative_content_range();
        (descriptor.source_range.start + relative.start)
            ..(descriptor.source_range.start + relative.end)
    }
}

/// Exact authority and authenticated canonical-stream summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11InlineProjectionDescriptor {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    source_range: Range<u32>,
    logical_page_count: u64,
    fact_count: u64,
    storage_page_count: u64,
    ordered_commitment256: [u8; 32],
    link_value_entry_count: u32,
    link_value_record_count: u64,
    link_value_storage_page_count: u64,
    link_value_encoded_bytes: u32,
    link_value_ordered_commitment256: [u8; 32],
}

impl M11InlineProjectionDescriptor {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn source_range(&self) -> &Range<u32> {
        &self.source_range
    }

    #[must_use]
    pub const fn logical_page_count(&self) -> u64 {
        self.logical_page_count
    }

    #[must_use]
    pub const fn fact_count(&self) -> u64 {
        self.fact_count
    }

    #[must_use]
    pub const fn storage_page_count(&self) -> u64 {
        self.storage_page_count
    }

    /// Domain-separated BLAKE3 commitment to authority and ordered pages.
    ///
    /// This, not the generic page tree's non-authoritative checksum, is the
    /// canonical full-build checkpoint commitment. It is deliberately not
    /// advertised as the future O(changed-pages) splice commitment.
    #[must_use]
    pub const fn ordered_commitment256(&self) -> [u8; 32] {
        self.ordered_commitment256
    }

    #[must_use]
    pub const fn link_value_entry_count(&self) -> u32 {
        self.link_value_entry_count
    }

    #[must_use]
    pub const fn link_value_record_count(&self) -> u64 {
        self.link_value_record_count
    }

    #[must_use]
    pub const fn link_value_storage_page_count(&self) -> u64 {
        self.link_value_storage_page_count
    }

    /// Exact public `FLKIV001` byte length, or zero for the canonical absent
    /// value lane.
    #[must_use]
    pub const fn link_value_encoded_bytes(&self) -> u32 {
        self.link_value_encoded_bytes
    }

    #[must_use]
    pub const fn link_value_ordered_commitment256(&self) -> [u8; 32] {
        self.link_value_ordered_commitment256
    }
}

/// One journalled retain of typed inline Projection content.
pub(crate) struct RetainedM11InlineProjectionRole {
    fact_owner: Option<ArenaBuildOwner>,
    value_owner: Option<ArenaBuildOwner>,
    descriptor: [u8; PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES],
    canonical_record_count: u64,
    canonical_bytes: u64,
}

impl RetainedM11InlineProjectionRole {
    pub(crate) fn take_fact_owner(&mut self) -> Option<ArenaBuildOwner> {
        self.fact_owner.take()
    }

    pub(crate) fn take_value_owner(&mut self) -> Option<ArenaBuildOwner> {
        self.value_owner.take()
    }

    pub(crate) const fn descriptor(
        &self,
    ) -> &[u8; PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES] {
        &self.descriptor
    }

    pub(crate) const fn canonical_record_count(&self) -> u64 {
        self.canonical_record_count
    }

    pub(crate) const fn canonical_bytes(&self) -> u64 {
        self.canonical_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11InlineProjectionDescriptor {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    source_start: u32,
    source_end: u32,
    logical_page_count: u64,
    fact_count: u64,
    storage_page_count: u64,
    payload_bytes: u64,
    encoded_bytes: u64,
    checksum: [u8; 32],
    ordered_commitment256: [u8; 32],
    link_value_entry_count: u32,
    link_value_record_count: u64,
    link_value_storage_page_count: u64,
    link_value_payload_bytes: u64,
    link_value_tree_encoded_bytes: u64,
    link_value_encoded_bytes: u32,
    link_value_checksum: [u8; 32],
    link_value_ordered_commitment256: [u8; 32],
}

impl PersistentM11InlineProjectionDescriptor {
    pub(crate) const fn source(self) -> SourceVersion {
        self.source
    }

    pub(crate) const fn parser_profile(self) -> ParserProfileId {
        self.parser_profile
    }

    pub(crate) fn source_range(self) -> Range<u32> {
        self.source_start..self.source_end
    }

    pub(crate) const fn logical_page_count(self) -> u64 {
        self.logical_page_count
    }

    pub(crate) const fn fact_count(self) -> u64 {
        self.fact_count
    }

    pub(crate) const fn canonical_bytes(self) -> u64 {
        self.payload_bytes + self.link_value_payload_bytes
    }

    pub(crate) const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    pub(crate) const fn ordered_commitment256(self) -> [u8; 32] {
        self.ordered_commitment256
    }

    pub(crate) const fn link_value_entry_count(self) -> u32 {
        self.link_value_entry_count
    }

    pub(crate) const fn link_value_record_count(self) -> u64 {
        self.link_value_record_count
    }

    pub(crate) const fn link_value_storage_page_count(self) -> u64 {
        self.link_value_storage_page_count
    }

    pub(crate) const fn link_value_encoded_bytes(self) -> u32 {
        self.link_value_encoded_bytes
    }

    /// Conservative source-tree depth needed by the current imported logical
    /// page cursor. The extra level is the structural viewport root above the
    /// measured inline sequence.
    pub(crate) fn maximum_query_open_depth(self) -> u32 {
        u32::from(
            maximum_avl_height(self.storage_page_count)
                .max(maximum_avl_height(self.link_value_storage_page_count)),
        )
        .saturating_add(1)
    }

    /// Conservative header-inspection bound for replaying every logical page
    /// through the current checked ordinal lookup.
    ///
    /// Each lookup validates the imported root and descends at most one AVL
    /// height. A branch decode inspects its own header and both child headers,
    /// so `3 * height + 6` is deliberately above the exact per-page maximum.
    pub(crate) fn maximum_query_tree_nodes_visited(self) -> Option<u64> {
        let fact_height = u64::from(maximum_avl_height(self.storage_page_count));
        let value_height = u64::from(maximum_avl_height(self.link_value_storage_page_count));
        let fact_work = self
            .logical_page_count
            .checked_mul(fact_height.checked_mul(3)?.checked_add(6)?)?;
        let value_work = self
            .link_value_record_count
            .checked_mul(value_height.checked_mul(3)?.checked_add(6)?)?;
        fact_work.checked_add(value_work)?.checked_add(1)
    }

    fn page_claim(self) -> M11ImportedParserPageRootClaim {
        M11ImportedParserPageRootClaim {
            stream_tag: INLINE_PROJECTION_STREAM_TAG,
            storage_page_count: self.storage_page_count,
            record_count: self.logical_page_count,
            payload_bytes: self.payload_bytes,
            encoded_bytes: self.encoded_bytes,
            checksum: self.checksum,
        }
    }

    fn link_value_page_claim(self) -> M11ImportedParserPageRootClaim {
        M11ImportedParserPageRootClaim {
            stream_tag: INLINE_LINK_VALUE_STREAM_TAG,
            storage_page_count: self.link_value_storage_page_count,
            record_count: self.link_value_record_count,
            payload_bytes: self.link_value_payload_bytes,
            encoded_bytes: self.link_value_tree_encoded_bytes,
            checksum: self.link_value_checksum,
        }
    }
}

/// Authority, canonical-schema, lifecycle, or query-budget failure.
#[derive(Debug)]
pub enum M11InlineProjectionError {
    InvalidFact(&'static str),
    InvalidLinkValue(&'static str),
    EmptyLogicalPage,
    TooManyFacts { facts: usize, cap: usize },
    FactsOutOfOrder,
    FactOutsideSourceRange,
    CoordinateOverflow,
    SourceAuthorityMismatch,
    ParserProfileMismatch,
    QueryRangeInvalid,
    QueryBudgetInvalid,
    QueryBudgetExceeded,
    InvalidState,
    CommitmentMismatch,
    Malformed(&'static str),
    Pages(M11ParserPageError),
}

impl fmt::Display for M11InlineProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFact(message) => {
                write!(formatter, "invalid inline Projection fact: {message}")
            }
            Self::InvalidLinkValue(message) => {
                write!(formatter, "invalid inline link value: {message}")
            }
            Self::EmptyLogicalPage => {
                formatter.write_str("inline Projection logical pages must not be empty")
            }
            Self::TooManyFacts { facts, cap } => {
                write!(
                    formatter,
                    "inline Projection page has {facts} facts above the {cap}-fact cap"
                )
            }
            Self::FactsOutOfOrder => {
                formatter.write_str("inline Projection facts are not in source order")
            }
            Self::FactOutsideSourceRange => {
                formatter.write_str("inline Projection fact exceeds its exact source range")
            }
            Self::CoordinateOverflow => {
                formatter.write_str("inline Projection coordinate overflow")
            }
            Self::SourceAuthorityMismatch => {
                formatter.write_str("inline Projection source authority mismatch")
            }
            Self::ParserProfileMismatch => {
                formatter.write_str("inline Projection parser profile mismatch")
            }
            Self::QueryRangeInvalid => {
                formatter.write_str("inline Projection query range is invalid")
            }
            Self::QueryBudgetInvalid => {
                formatter.write_str("inline Projection query budget must be nonzero")
            }
            Self::QueryBudgetExceeded => {
                formatter.write_str("inline Projection checkpoint query exhausted its work budget")
            }
            Self::InvalidState => {
                formatter.write_str("inline Projection owner is in the wrong state")
            }
            Self::CommitmentMismatch => {
                formatter.write_str("inline Projection ordered commitment mismatch")
            }
            Self::Malformed(message) => {
                write!(formatter, "malformed inline Projection page: {message}")
            }
            Self::Pages(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11InlineProjectionError {}

impl From<M11ParserPageError> for M11InlineProjectionError {
    fn from(value: M11ParserPageError) -> Self {
        Self::Pages(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineProjectionBuildStatus {
    NeedsPage,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InlineProjectionBuildPoll {
    status: M11InlineProjectionBuildStatus,
    transitions: usize,
}

impl M11InlineProjectionBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11InlineProjectionBuildStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InlineProjectionBuildPhase {
    Accepting,
    Finishing,
    Complete,
    Cancelled,
    Failed,
}

/// Move-only builder for one exact-source/profile persistent inline root.
#[must_use = "inline Projection builds require root transfer or explicit cancellation"]
pub struct M11InlineProjectionBuild {
    inner: M11ParserPageBuild,
    link_value_inner: M11ParserPageBuild,
    source: SourceVersion,
    parser_profile: ParserProfileId,
    source_range: Range<u32>,
    phase: InlineProjectionBuildPhase,
    previous_page_anchor: u32,
    last_fact_start: Option<u32>,
    logical_page_count: u64,
    fact_count: u64,
    stream_hasher: blake3::Hasher,
    ordered_commitment256: Option<[u8; 32]>,
    link_value_bytes: Vec<u8>,
    link_value_flush_offset: usize,
    link_value_entry_count: u32,
    link_value_input_closed: bool,
    link_value_ordered_commitment256: Option<[u8; 32]>,
    output: Option<M11InlineProjectionRoot>,
    failed_root: Option<M11ParserPageRoot>,
    failed_link_value_root: Option<M11ParserPageRoot>,
}

impl fmt::Debug for M11InlineProjectionBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineProjectionBuild")
            .field("source", &self.source)
            .field("parser_profile", &self.parser_profile)
            .field("source_range", &self.source_range)
            .field("phase", &self.phase)
            .field("logical_page_count", &self.logical_page_count)
            .field("fact_count", &self.fact_count)
            .field("link_value_entry_count", &self.link_value_entry_count)
            .finish_non_exhaustive()
    }
}

impl M11InlineProjectionBuild {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
        source_range: Range<usize>,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11InlineProjectionError> {
        let source = lease.version();
        let exact_range = source_range.clone();
        let source_range_u32 = projection_source_range(&exact_range)?;
        let link_value_inner = M11ParserPageBuild::new(
            runtime,
            lease.duplicate(),
            source_range.clone(),
            INLINE_LINK_VALUE_STREAM_TAG,
        )?;
        let inner =
            M11ParserPageBuild::new(runtime, lease, source_range, INLINE_PROJECTION_STREAM_TAG)?;
        Ok(Self::from_page_build(
            inner,
            link_value_inner,
            source,
            source_range_u32,
            parser_profile,
        ))
    }

    /// Starts a typed build from the parser's existing exact-range authority.
    ///
    /// The authority remains move-only and borrowed: this constructor neither
    /// consumes it nor exposes its private source lease. The generic page
    /// owner receives an engine-private duplicate only after revalidating the
    /// exact document runtime and current source.
    pub fn new_from_source_authority(
        runtime: &DocumentRuntime,
        authority: &M11ParserSourceRangeAuthority,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11InlineProjectionError> {
        let source = authority.source();
        let source_range = authority.source_range();
        let source_range_u32 = projection_source_range(&source_range)?;
        let inner = M11ParserPageBuild::new_from_source_authority(
            runtime,
            authority,
            INLINE_PROJECTION_STREAM_TAG,
        )?;
        let link_value_inner = M11ParserPageBuild::new_from_source_authority(
            runtime,
            authority,
            INLINE_LINK_VALUE_STREAM_TAG,
        )?;
        Ok(Self::from_page_build(
            inner,
            link_value_inner,
            source,
            source_range_u32,
            parser_profile,
        ))
    }

    fn from_page_build(
        inner: M11ParserPageBuild,
        link_value_inner: M11ParserPageBuild,
        source: SourceVersion,
        source_range: Range<u32>,
        parser_profile: ParserProfileId,
    ) -> Self {
        Self {
            inner,
            link_value_inner,
            source,
            parser_profile,
            source_range: source_range.clone(),
            phase: InlineProjectionBuildPhase::Accepting,
            previous_page_anchor: 0,
            last_fact_start: None,
            logical_page_count: 0,
            fact_count: 0,
            stream_hasher: begin_commitment(source, parser_profile, &source_range),
            ordered_commitment256: None,
            link_value_bytes: Vec::new(),
            link_value_flush_offset: 0,
            link_value_entry_count: 0,
            link_value_input_closed: false,
            link_value_ordered_commitment256: None,
            output: None,
            failed_root: None,
            failed_link_value_root: None,
        }
    }

    /// Offers one parser-defined logical page.
    ///
    /// Page membership is intentionally explicit. The parser can align pages
    /// to stable Green/block boundaries; this storage facade never regroups
    /// facts by a global ordinal that would cascade after an early insertion.
    pub fn offer_page(
        &mut self,
        facts: &[M11InlineProjectionFact],
    ) -> Result<(), M11InlineProjectionError> {
        self.offer_page_with_link_values(facts, &[])
    }

    /// Offers one logical fact page and the exact cooked values for every
    /// link/image fact in that page.
    ///
    /// Values are keyed by global fact ordinal, not by a second independently
    /// inferred syntax order. This method validates the complete page/value
    /// relation before either persistent stream is mutated.
    pub fn offer_page_with_link_values(
        &mut self,
        facts: &[M11InlineProjectionFact],
        link_values: &[M11InlineLinkValue],
    ) -> Result<(), M11InlineProjectionError> {
        if self.phase != InlineProjectionBuildPhase::Accepting {
            return Err(M11InlineProjectionError::InvalidState);
        }
        if facts.is_empty() {
            return Err(M11InlineProjectionError::EmptyLogicalPage);
        }
        if facts.len() > M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX {
            return Err(M11InlineProjectionError::TooManyFacts {
                facts: facts.len(),
                cap: M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX,
            });
        }
        let source_len = self
            .source_range
            .end
            .checked_sub(self.source_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let page_anchor = facts[0].relative_start;
        if page_anchor < self.previous_page_anchor
            || self.last_fact_start.is_some_and(|last| page_anchor < last)
        {
            return Err(M11InlineProjectionError::FactsOutOfOrder);
        }
        let anchor_delta = page_anchor
            .checked_sub(self.previous_page_anchor)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let mut previous_start = self.last_fact_start;
        let mut maximum_local_end = 0_u32;
        let mut encoded_link_values = Vec::new();
        let mut next_link_value = 0_usize;
        for (local_ordinal, fact) in facts.iter().enumerate() {
            validate_fact(*fact)?;
            if previous_start.is_some_and(|start| fact.relative_start < start) {
                return Err(M11InlineProjectionError::FactsOutOfOrder);
            }
            let end = fact
                .relative_start
                .checked_add(fact.relative_len)
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
            if end > source_len {
                return Err(M11InlineProjectionError::FactOutsideSourceRange);
            }
            let local_start = fact
                .relative_start
                .checked_sub(page_anchor)
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
            maximum_local_end = maximum_local_end.max(
                local_start
                    .checked_add(fact.relative_len)
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
            );
            if fact.kind.has_link_value() {
                let value = link_values.get(next_link_value).ok_or(
                    M11InlineProjectionError::InvalidLinkValue(
                        "link/image fact has no companion value",
                    ),
                )?;
                let expected_ordinal = self
                    .fact_count
                    .checked_add(
                        u64::try_from(local_ordinal)
                            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
                    )
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                if value.parent_fact_ordinal != expected_ordinal {
                    return Err(M11InlineProjectionError::InvalidLinkValue(
                        "inline link values are not keyed by strict fact ordinal",
                    ));
                }
                value.validate_against_fact(*fact, self.source.byte_len())?;
                let value_encoded_len = value.encoded_len()?;
                let bounded_page_bytes = self
                    .link_value_bytes
                    .len()
                    .checked_add(encoded_link_values.len())
                    .and_then(|bytes| bytes.checked_add(value_encoded_len))
                    .and_then(|bytes| bytes.checked_add(16))
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                let bounded_entry_count = self
                    .link_value_entry_count
                    .checked_add(
                        u32::try_from(next_link_value)
                            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
                    )
                    .and_then(|count| count.checked_add(1))
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                if bounded_page_bytes > M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES
                    || bounded_entry_count > M11_INLINE_LINK_VALUES_MAX_ENTRIES
                {
                    return Err(M11InlineProjectionError::InvalidLinkValue(
                        "encoded inline link values exceed the bounded query envelope",
                    ));
                }
                value.encode_into(&mut encoded_link_values)?;
                next_link_value += 1;
            }
            previous_start = Some(fact.relative_start);
        }
        if next_link_value != link_values.len() {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "orphan inline link value has no link/image fact",
            ));
        }
        let next_link_value_entry_count = self
            .link_value_entry_count
            .checked_add(
                u32::try_from(link_values.len())
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let next_link_value_bytes = self
            .link_value_bytes
            .len()
            .checked_add(encoded_link_values.len())
            .and_then(|bytes| bytes.checked_add(16))
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        if next_link_value_entry_count > M11_INLINE_LINK_VALUES_MAX_ENTRIES
            || next_link_value_bytes > M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES
        {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "encoded inline link values exceed the bounded query envelope",
            ));
        }
        let encoded = encode_logical_page(anchor_delta, maximum_local_end, page_anchor, facts)?;
        let record = M11ParserPageRecord::new(encoded.as_bytes())?;
        let next_logical_page_count = self
            .logical_page_count
            .checked_add(1)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let next_fact_count = self
            .fact_count
            .checked_add(
                u64::try_from(facts.len())
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        self.inner.offer_record(record)?;
        self.link_value_bytes
            .extend_from_slice(&encoded_link_values);
        self.link_value_entry_count = next_link_value_entry_count;
        append_page_to_commitment(&mut self.stream_hasher, encoded.as_bytes());
        self.previous_page_anchor = page_anchor;
        self.last_fact_start = previous_start;
        self.logical_page_count = next_logical_page_count;
        self.fact_count = next_fact_count;
        Ok(())
    }

    pub fn finish_input(&mut self) -> Result<(), M11InlineProjectionError> {
        if self.phase != InlineProjectionBuildPhase::Accepting {
            return Err(M11InlineProjectionError::InvalidState);
        }
        self.inner.finish_input()?;
        self.ordered_commitment256 = Some(finish_commitment(
            &self.stream_hasher,
            self.logical_page_count,
            self.fact_count,
        ));
        self.link_value_ordered_commitment256 = Some(if self.link_value_entry_count == 0 {
            [0; 32]
        } else {
            finish_link_value_commitment(
                self.source,
                self.parser_profile,
                &self.source_range,
                &self.link_value_bytes,
                self.link_value_entry_count,
            )
        });
        self.phase = InlineProjectionBuildPhase::Finishing;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineProjectionBuildPoll, M11InlineProjectionError> {
        if self.phase == InlineProjectionBuildPhase::Failed {
            return Err(M11InlineProjectionError::InvalidState);
        }
        let inner_poll = self.inner.poll(runtime, fuel)?;
        let mut transitions = inner_poll.transitions();
        let status = match inner_poll.status() {
            M11ParserPageBuildStatus::NeedsInput => {
                if self.phase != InlineProjectionBuildPhase::Accepting {
                    self.phase = InlineProjectionBuildPhase::Failed;
                    return Err(M11InlineProjectionError::InvalidState);
                }
                M11InlineProjectionBuildStatus::NeedsPage
            }
            M11ParserPageBuildStatus::Pending => M11InlineProjectionBuildStatus::Pending,
            M11ParserPageBuildStatus::Cancelled => {
                self.phase = InlineProjectionBuildPhase::Cancelled;
                M11InlineProjectionBuildStatus::Cancelled
            }
            M11ParserPageBuildStatus::Complete => {
                if self.phase == InlineProjectionBuildPhase::Accepting {
                    self.phase = InlineProjectionBuildPhase::Failed;
                    return Err(M11InlineProjectionError::InvalidState);
                }
                if transitions == fuel {
                    M11InlineProjectionBuildStatus::Pending
                } else {
                    let link_poll = self.poll_link_value_build(runtime, fuel - transitions)?;
                    transitions = transitions
                        .checked_add(link_poll.1)
                        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                    match link_poll.0 {
                        M11ParserPageBuildStatus::Complete => {
                            if self.output.is_none() {
                                self.complete_root()?;
                            }
                            self.phase = InlineProjectionBuildPhase::Complete;
                            M11InlineProjectionBuildStatus::Complete
                        }
                        M11ParserPageBuildStatus::Cancelled => {
                            self.phase = InlineProjectionBuildPhase::Cancelled;
                            M11InlineProjectionBuildStatus::Cancelled
                        }
                        M11ParserPageBuildStatus::NeedsInput
                        | M11ParserPageBuildStatus::Pending => {
                            M11InlineProjectionBuildStatus::Pending
                        }
                    }
                }
            }
        };
        Ok(M11InlineProjectionBuildPoll {
            status,
            transitions,
        })
    }

    fn poll_link_value_build(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<(M11ParserPageBuildStatus, usize), M11InlineProjectionError> {
        let poll = self.link_value_inner.poll(runtime, fuel)?;
        if poll.status() != M11ParserPageBuildStatus::NeedsInput || poll.transitions() == fuel {
            return Ok((poll.status(), poll.transitions()));
        }
        let mut transitions = poll.transitions();
        if self.link_value_flush_offset < self.link_value_bytes.len() {
            let end = self
                .link_value_flush_offset
                .checked_add(INLINE_LINK_VALUE_CHUNK_BYTES)
                .map_or(self.link_value_bytes.len(), |end| {
                    end.min(self.link_value_bytes.len())
                });
            let record = M11ParserPageRecord::new(
                &self.link_value_bytes[self.link_value_flush_offset..end],
            )?;
            self.link_value_inner.offer_record(record)?;
            self.link_value_flush_offset = end;
            transitions += 1;
        } else if !self.link_value_input_closed {
            self.link_value_inner.finish_input()?;
            self.link_value_input_closed = true;
            transitions += 1;
        }
        Ok((M11ParserPageBuildStatus::Pending, transitions))
    }

    fn complete_root(&mut self) -> Result<(), M11InlineProjectionError> {
        let ordered_commitment256 = self
            .ordered_commitment256
            .ok_or(M11InlineProjectionError::InvalidState)?;
        let root = self
            .inner
            .take_root()
            .ok_or(M11InlineProjectionError::InvalidState)?;
        let link_value_root = self
            .link_value_inner
            .take_root()
            .ok_or(M11InlineProjectionError::InvalidState)?;
        let range = root.source_range();
        let valid = root.source() == self.source
            && range.start == self.source_range.start as usize
            && range.end == self.source_range.end as usize
            && root.stream_tag() == INLINE_PROJECTION_STREAM_TAG
            && root.record_count() == self.logical_page_count;
        let expected_link_value_records = u64::try_from(
            self.link_value_bytes
                .len()
                .div_ceil(INLINE_LINK_VALUE_CHUNK_BYTES),
        )
        .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        let link_value_range = link_value_root.source_range();
        let link_value_valid = link_value_root.source() == self.source
            && link_value_range.start == self.source_range.start as usize
            && link_value_range.end == self.source_range.end as usize
            && link_value_root.stream_tag() == INLINE_LINK_VALUE_STREAM_TAG
            && link_value_root.record_count() == expected_link_value_records
            && link_value_root.payload_bytes()
                == u64::try_from(self.link_value_bytes.len())
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        if !valid || !link_value_valid {
            self.failed_root = Some(root);
            self.failed_link_value_root = Some(link_value_root);
            self.phase = InlineProjectionBuildPhase::Failed;
            return Err(M11InlineProjectionError::Malformed(
                "generic page root changed typed Projection authority",
            ));
        }
        let descriptor = M11InlineProjectionDescriptor {
            source: self.source,
            parser_profile: self.parser_profile,
            source_range: self.source_range.clone(),
            logical_page_count: self.logical_page_count,
            fact_count: self.fact_count,
            storage_page_count: root.page_count(),
            ordered_commitment256,
            link_value_entry_count: self.link_value_entry_count,
            link_value_record_count: link_value_root.record_count(),
            link_value_storage_page_count: link_value_root.page_count(),
            link_value_encoded_bytes: if self.link_value_entry_count == 0 {
                0
            } else {
                u32::try_from(
                    16_usize
                        .checked_add(self.link_value_bytes.len())
                        .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
                )
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
            },
            link_value_ordered_commitment256: self
                .link_value_ordered_commitment256
                .ok_or(M11InlineProjectionError::InvalidState)?,
        };
        self.output = Some(M11InlineProjectionRoot {
            inner: root,
            link_values: link_value_root,
            descriptor,
        });
        Ok(())
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11InlineProjectionError> {
        if let Some(output) = self.output.as_mut() {
            output.begin_release(runtime)?;
        }
        if let Some(root) = self.failed_root.as_mut() {
            root.begin_release(runtime)?;
        }
        if let Some(root) = self.failed_link_value_root.as_mut() {
            root.begin_release(runtime)?;
        }
        self.inner.begin_cancel(runtime)?;
        self.link_value_inner.begin_cancel(runtime)?;
        self.phase = InlineProjectionBuildPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageReclaimPoll, M11InlineProjectionError> {
        if self.phase != InlineProjectionBuildPhase::Cancelled {
            return Err(M11InlineProjectionError::InvalidState);
        }
        let poll = self.inner.poll_cancel(runtime, fuel)?;
        let link_value_poll = self.link_value_inner.poll_cancel(runtime, fuel)?;
        if poll.complete() {
            self.output.take();
            self.failed_root.take();
            self.failed_link_value_root.take();
        }
        Ok(if link_value_poll.complete() {
            poll
        } else {
            link_value_poll
        })
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11InlineProjectionRoot> {
        if self.phase != InlineProjectionBuildPhase::Complete {
            return None;
        }
        self.output.take()
    }

    #[must_use]
    pub fn build_receipt(&self) -> M11ParserPageBuildReceipt {
        self.inner.receipt()
    }
}

/// Move-only exact-authority persistent inline Projection root.
#[must_use = "inline Projection roots require transfer or explicit fuelled release"]
pub struct M11InlineProjectionRoot {
    inner: M11ParserPageRoot,
    link_values: M11ParserPageRoot,
    descriptor: M11InlineProjectionDescriptor,
}

impl fmt::Debug for M11InlineProjectionRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineProjectionRoot")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl M11InlineProjectionRoot {
    #[must_use]
    pub const fn descriptor(&self) -> &M11InlineProjectionDescriptor {
        &self.descriptor
    }

    pub fn cursor<'root>(
        &'root self,
        runtime: &DocumentRuntime,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
    ) -> Result<M11InlineProjectionCursor<'root>, M11InlineProjectionError> {
        self.validate_expected_authority(expected_source, expected_profile)?;
        Ok(M11InlineProjectionCursor {
            inner: self.inner.cursor(runtime)?,
            descriptor: &self.descriptor,
            hasher: begin_commitment(
                self.descriptor.source,
                self.descriptor.parser_profile,
                &self.descriptor.source_range,
            ),
            previous_page_anchor: 0,
            last_fact_start: None,
            observed_pages: 0,
            observed_facts: 0,
            current_page: None,
            complete: false,
        })
    }

    /// Starts the bounded linear proof query used only while the indexed
    /// viewport structure is not yet joined.
    ///
    /// `maximum_polls` is a hard total budget. Exhaustion fails before another
    /// page/fact transition is attempted.
    pub fn begin_checkpoint_query<'root>(
        &'root self,
        runtime: &DocumentRuntime,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
        absolute_range: Range<u32>,
        maximum_polls: usize,
    ) -> Result<M11InlineProjectionCheckpointQuery<'root>, M11InlineProjectionError> {
        if maximum_polls == 0 {
            return Err(M11InlineProjectionError::QueryBudgetInvalid);
        }
        if absolute_range.start >= absolute_range.end
            || absolute_range.start < self.descriptor.source_range.start
            || absolute_range.end > self.descriptor.source_range.end
        {
            return Err(M11InlineProjectionError::QueryRangeInvalid);
        }
        Ok(M11InlineProjectionCheckpointQuery {
            cursor: self.cursor(runtime, expected_source, expected_profile)?,
            absolute_range,
            maximum_polls,
            polls: 0,
        })
    }

    fn validate_expected_authority(
        &self,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
    ) -> Result<(), M11InlineProjectionError> {
        if expected_source != self.descriptor.source {
            return Err(M11InlineProjectionError::SourceAuthorityMismatch);
        }
        if expected_profile != self.descriptor.parser_profile {
            return Err(M11InlineProjectionError::ParserProfileMismatch);
        }
        Ok(())
    }

    pub(crate) fn retain_for_publication(
        &self,
        session: &mut ArenaBuildSession<'_>,
        expected_runtime_identity: StrongIdentity,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
    ) -> Result<RetainedM11InlineProjectionRole, M11InlineProjectionError> {
        self.validate_expected_authority(expected_source, expected_profile)?;
        let retained = self.inner.retain_for_publication(
            session,
            expected_runtime_identity,
            expected_source,
            usize::try_from(self.descriptor.source_range.start)
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
                ..usize::try_from(self.descriptor.source_range.end)
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            INLINE_PROJECTION_STREAM_TAG,
        )?;
        let retained_link_values = self.link_values.retain_for_publication(
            session,
            expected_runtime_identity,
            expected_source,
            usize::try_from(self.descriptor.source_range.start)
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
                ..usize::try_from(self.descriptor.source_range.end)
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            INLINE_LINK_VALUE_STREAM_TAG,
        )?;
        let descriptor = encode_persistent_inline_projection_descriptor(
            &self.descriptor,
            self.inner.payload_bytes(),
            self.inner.encoded_bytes(),
            self.inner.checksum(),
            self.link_values.payload_bytes(),
            self.link_values.encoded_bytes(),
            self.link_values.checksum(),
        );
        let mut retained = retained;
        let mut retained_link_values = retained_link_values;
        let canonical_record_count = self
            .descriptor
            .logical_page_count
            .checked_add(self.descriptor.link_value_record_count)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        Ok(RetainedM11InlineProjectionRole {
            fact_owner: retained.take_owner(),
            value_owner: retained_link_values.take_owner(),
            descriptor,
            canonical_record_count,
            canonical_bytes: self
                .inner
                .payload_bytes()
                .checked_add(self.link_values.payload_bytes())
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
        })
    }

    /// Returns both authority-free closure roots and their persistent
    /// descriptor for independently transported hot-inline sidecars.
    pub(crate) fn transport_bundle_parts(
        &self,
        runtime: &DocumentRuntime,
        expected_source: SourceVersion,
        expected_profile: ParserProfileId,
    ) -> Result<M11InlineProjectionTransportBundleParts, M11InlineProjectionError> {
        self.validate_expected_authority(expected_source, expected_profile)?;
        let _ = self.inner.cursor(runtime)?;
        let _ = self.link_values.cursor(runtime)?;
        Ok((
            self.inner.transport_root_id()?,
            self.link_values.transport_root_id()?,
            encode_persistent_inline_projection_descriptor(
                &self.descriptor,
                self.inner.payload_bytes(),
                self.inner.encoded_bytes(),
                self.inner.checksum(),
                self.link_values.payload_bytes(),
                self.link_values.encoded_bytes(),
                self.link_values.checksum(),
            ),
        ))
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11InlineProjectionError> {
        self.inner.begin_release(runtime)?;
        self.link_values.begin_release(runtime)?;
        Ok(())
    }

    pub fn poll_release(
        &self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ParserPageReclaimPoll, M11InlineProjectionError> {
        Ok(self.inner.poll_release(runtime, fuel)?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineProjectionCursorPoll {
    Pending {
        transitions: usize,
    },
    Fact {
        transitions: usize,
        fact: M11InlineProjectionFact,
    },
    Complete {
        transitions: usize,
    },
}

struct LoadedLogicalPage {
    record: M11ParserPageRecord,
    anchor: u32,
    next_fact: usize,
    fact_count: usize,
}

/// Typed, validating, source-order cursor over one persistent root.
pub struct M11InlineProjectionCursor<'root> {
    inner: M11ParserPageCursor<'root>,
    descriptor: &'root M11InlineProjectionDescriptor,
    hasher: blake3::Hasher,
    previous_page_anchor: u32,
    last_fact_start: Option<u32>,
    observed_pages: u64,
    observed_facts: u64,
    current_page: Option<LoadedLogicalPage>,
    complete: bool,
}

impl M11InlineProjectionCursor<'_> {
    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11InlineProjectionCursorPoll, M11InlineProjectionError> {
        if self.complete {
            return Ok(M11InlineProjectionCursorPoll::Complete { transitions: 0 });
        }
        if let Some(page) = self.current_page.as_mut() {
            if page.next_fact < page.fact_count {
                let fact = decode_fact(page.record.as_bytes(), page.next_fact, page.anchor)?;
                page.next_fact += 1;
                return Ok(M11InlineProjectionCursorPoll::Fact {
                    transitions: 1,
                    fact,
                });
            }
            self.current_page = None;
        }

        match self.inner.poll(runtime)? {
            M11ParserPageCursorPoll::Pending { transitions } => {
                Ok(M11InlineProjectionCursorPoll::Pending { transitions })
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                let decoded = validate_logical_page(
                    record.as_bytes(),
                    self.previous_page_anchor,
                    self.last_fact_start,
                    self.descriptor
                        .source_range
                        .end
                        .checked_sub(self.descriptor.source_range.start)
                        .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
                )?;
                append_page_to_commitment(&mut self.hasher, record.as_bytes());
                self.previous_page_anchor = decoded.anchor;
                self.last_fact_start = Some(decoded.last_fact_start);
                self.observed_pages = self
                    .observed_pages
                    .checked_add(1)
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                self.observed_facts = self
                    .observed_facts
                    .checked_add(
                        u64::try_from(decoded.fact_count)
                            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                let fact = decode_fact(record.as_bytes(), 0, decoded.anchor)?;
                self.current_page = Some(LoadedLogicalPage {
                    record,
                    anchor: decoded.anchor,
                    next_fact: 1,
                    fact_count: decoded.fact_count,
                });
                Ok(M11InlineProjectionCursorPoll::Fact { transitions, fact })
            }
            M11ParserPageCursorPoll::Complete { transitions } => {
                if self.observed_pages != self.descriptor.logical_page_count
                    || self.observed_facts != self.descriptor.fact_count
                {
                    return Err(M11InlineProjectionError::Malformed(
                        "inline Projection descriptor counts differ from replay",
                    ));
                }
                let actual =
                    finish_commitment(&self.hasher, self.observed_pages, self.observed_facts);
                if actual != self.descriptor.ordered_commitment256 {
                    return Err(M11InlineProjectionError::CommitmentMismatch);
                }
                self.complete = true;
                Ok(M11InlineProjectionCursorPoll::Complete { transitions })
            }
        }
    }

    #[must_use]
    pub const fn descriptor(&self) -> &M11InlineProjectionDescriptor {
        self.descriptor
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineProjectionCheckpointQueryPoll {
    Pending {
        transitions: usize,
    },
    Fact {
        transitions: usize,
        fact: M11InlineProjectionFact,
    },
    Complete {
        transitions: usize,
    },
}

/// Explicitly temporary bounded linear query over the typed root.
pub struct M11InlineProjectionCheckpointQuery<'root> {
    cursor: M11InlineProjectionCursor<'root>,
    absolute_range: Range<u32>,
    maximum_polls: usize,
    polls: usize,
}

impl M11InlineProjectionCheckpointQuery<'_> {
    pub fn poll(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<M11InlineProjectionCheckpointQueryPoll, M11InlineProjectionError> {
        if self.polls == self.maximum_polls {
            return Err(M11InlineProjectionError::QueryBudgetExceeded);
        }
        self.polls += 1;
        match self.cursor.poll(runtime)? {
            M11InlineProjectionCursorPoll::Pending { transitions } => {
                Ok(M11InlineProjectionCheckpointQueryPoll::Pending { transitions })
            }
            M11InlineProjectionCursorPoll::Fact { transitions, fact } => {
                let range = fact.absolute_range(self.cursor.descriptor());
                if range.start < self.absolute_range.end && range.end > self.absolute_range.start {
                    Ok(M11InlineProjectionCheckpointQueryPoll::Fact { transitions, fact })
                } else {
                    Ok(M11InlineProjectionCheckpointQueryPoll::Pending { transitions })
                }
            }
            M11InlineProjectionCursorPoll::Complete { transitions } => {
                Ok(M11InlineProjectionCheckpointQueryPoll::Complete { transitions })
            }
        }
    }

    #[must_use]
    pub const fn polls(&self) -> usize {
        self.polls
    }
}

struct DecodedLogicalPage {
    anchor: u32,
    fact_count: usize,
    last_fact_start: u32,
}

#[derive(Clone)]
struct EncodedLogicalPage {
    bytes: [u8; M11_PARSER_PAGE_MAX_RECORD_BYTES],
    len: usize,
}

impl EncodedLogicalPage {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

fn encode_logical_page(
    anchor_delta: u32,
    maximum_local_end: u32,
    page_anchor: u32,
    facts: &[M11InlineProjectionFact],
) -> Result<EncodedLogicalPage, M11InlineProjectionError> {
    let encoded_len = INLINE_PROJECTION_PAGE_HEADER_BYTES
        .checked_add(
            facts
                .len()
                .checked_mul(INLINE_PROJECTION_FACT_BYTES)
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
        )
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    if encoded_len > M11_PARSER_PAGE_MAX_RECORD_BYTES {
        return Err(M11InlineProjectionError::TooManyFacts {
            facts: facts.len(),
            cap: M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX,
        });
    }
    let mut output = [0_u8; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    output[..4].copy_from_slice(&INLINE_PROJECTION_PAGE_MAGIC);
    output[4..8].copy_from_slice(&INLINE_PROJECTION_SCHEMA.to_le_bytes());
    output[8..12].copy_from_slice(&anchor_delta.to_le_bytes());
    output[12..16].copy_from_slice(&maximum_local_end.to_le_bytes());
    output[16..18].copy_from_slice(
        &u16::try_from(facts.len())
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
            .to_le_bytes(),
    );
    let mut cursor = INLINE_PROJECTION_PAGE_HEADER_BYTES;
    for fact in facts {
        let local_start = fact
            .relative_start
            .checked_sub(page_anchor)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        output[cursor] = fact.kind as u8;
        output[cursor + 4..cursor + 8].copy_from_slice(&local_start.to_le_bytes());
        output[cursor + 8..cursor + 12].copy_from_slice(&fact.relative_len.to_le_bytes());
        match fact.payload {
            M11InlineProjectionFactPayload::Marked {
                content_offset,
                content_len,
            } => {
                output[cursor + 1] = fact.flags;
                output[cursor + 12..cursor + 16].copy_from_slice(&content_offset.to_le_bytes());
                output[cursor + 16..cursor + 20].copy_from_slice(&content_len.to_le_bytes());
            }
            M11InlineProjectionFactPayload::CharacterReference { first, second } => {
                output[cursor + 1] = if second.is_some() { 2 } else { 1 };
                output[cursor + 12..cursor + 16].copy_from_slice(&(first as u32).to_le_bytes());
                output[cursor + 16..cursor + 20]
                    .copy_from_slice(&second.map_or(0, |scalar| scalar as u32).to_le_bytes());
            }
        }
        cursor += INLINE_PROJECTION_FACT_BYTES;
    }
    debug_assert_eq!(cursor, encoded_len);
    Ok(EncodedLogicalPage {
        bytes: output,
        len: encoded_len,
    })
}

fn validate_logical_page(
    bytes: &[u8],
    previous_page_anchor: u32,
    previous_fact_start: Option<u32>,
    source_len: u32,
) -> Result<DecodedLogicalPage, M11InlineProjectionError> {
    if bytes.len() < INLINE_PROJECTION_PAGE_HEADER_BYTES
        || bytes.get(..4) != Some(INLINE_PROJECTION_PAGE_MAGIC.as_slice())
    {
        return Err(M11InlineProjectionError::Malformed(
            "inline Projection page header is absent",
        ));
    }
    let schema = read_u32(bytes, 4)?;
    if schema != INLINE_PROJECTION_SCHEMA {
        return Err(M11InlineProjectionError::Malformed(
            "inline Projection page schema is unsupported",
        ));
    }
    let anchor_delta = read_u32(bytes, 8)?;
    let expected_maximum_local_end = read_u32(bytes, 12)?;
    let fact_count = usize::from(read_u16(bytes, 16)?);
    if fact_count == 0
        || fact_count > M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX
        || read_u16(bytes, 18)? != 0
        || bytes.len()
            != INLINE_PROJECTION_PAGE_HEADER_BYTES + fact_count * INLINE_PROJECTION_FACT_BYTES
    {
        return Err(M11InlineProjectionError::Malformed(
            "inline Projection page dimensions are invalid",
        ));
    }
    let anchor = previous_page_anchor
        .checked_add(anchor_delta)
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    if anchor > source_len {
        return Err(M11InlineProjectionError::FactOutsideSourceRange);
    }
    let mut last_fact_start = previous_fact_start;
    let mut maximum_local_end = 0_u32;
    for ordinal in 0..fact_count {
        let fact = decode_fact(bytes, ordinal, anchor)?;
        validate_fact(fact)?;
        if ordinal == 0 && fact.relative_start != anchor {
            return Err(M11InlineProjectionError::Malformed(
                "inline Projection page first fact is not at its anchor",
            ));
        }
        if last_fact_start.is_some_and(|start| fact.relative_start < start) {
            return Err(M11InlineProjectionError::FactsOutOfOrder);
        }
        let end = fact
            .relative_start
            .checked_add(fact.relative_len)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        if end > source_len {
            return Err(M11InlineProjectionError::FactOutsideSourceRange);
        }
        maximum_local_end = maximum_local_end.max(
            end.checked_sub(anchor)
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
        );
        last_fact_start = Some(fact.relative_start);
    }
    if maximum_local_end != expected_maximum_local_end {
        return Err(M11InlineProjectionError::Malformed(
            "inline Projection page maximum extent changed",
        ));
    }
    Ok(DecodedLogicalPage {
        anchor,
        fact_count,
        last_fact_start: last_fact_start.expect("nonempty page"),
    })
}

fn decode_fact(
    bytes: &[u8],
    ordinal: usize,
    anchor: u32,
) -> Result<M11InlineProjectionFact, M11InlineProjectionError> {
    let offset = INLINE_PROJECTION_PAGE_HEADER_BYTES
        .checked_add(
            ordinal
                .checked_mul(INLINE_PROJECTION_FACT_BYTES)
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
        )
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    let record = bytes
        .get(offset..offset + INLINE_PROJECTION_FACT_BYTES)
        .ok_or(M11InlineProjectionError::Malformed(
            "inline Projection fact is truncated",
        ))?;
    if read_u16(record, 2)? != 0 {
        return Err(M11InlineProjectionError::Malformed(
            "inline Projection fact reserved bytes are nonzero",
        ));
    }
    let local_start = read_u32(record, 4)?;
    let kind = M11InlineProjectionKind::decode(record[0])?;
    let payload = if kind == M11InlineProjectionKind::CharacterReference {
        let first =
            char::from_u32(read_u32(record, 12)?).ok_or(M11InlineProjectionError::Malformed(
                "character reference first value is not a Unicode scalar",
            ))?;
        let second_word = read_u32(record, 16)?;
        let second = match record[1] {
            1 if second_word == 0 => None,
            2 if second_word != 0 => Some(char::from_u32(second_word).ok_or(
                M11InlineProjectionError::Malformed(
                    "character reference second value is not a Unicode scalar",
                ),
            )?),
            _ => {
                return Err(M11InlineProjectionError::Malformed(
                    "character reference scalar count or sentinel is invalid",
                ))
            }
        };
        M11InlineProjectionFactPayload::CharacterReference { first, second }
    } else {
        M11InlineProjectionFactPayload::Marked {
            content_offset: read_u32(record, 12)?,
            content_len: read_u32(record, 16)?,
        }
    };
    Ok(M11InlineProjectionFact {
        kind,
        flags: if kind == M11InlineProjectionKind::CharacterReference {
            0
        } else {
            record[1]
        },
        relative_start: anchor
            .checked_add(local_start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?,
        relative_len: read_u32(record, 8)?,
        payload,
    })
}

fn projection_source_range(
    source_range: &Range<usize>,
) -> Result<Range<u32>, M11InlineProjectionError> {
    let start = u32::try_from(source_range.start)
        .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
    let end = u32::try_from(source_range.end)
        .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
    Ok(start..end)
}

fn validate_bare_autolink_flags(
    kind: M11InlineProjectionKind,
    flags: u8,
) -> Result<(), M11InlineProjectionError> {
    match kind {
        M11InlineProjectionKind::AutolinkUri
            if flags & !M11_INLINE_PROJECTION_AUTOLINK_URI_FLAGS == 0 =>
        {
            Ok(())
        }
        M11InlineProjectionKind::AutolinkUri => Err(M11InlineProjectionError::InvalidFact(
            "bare URI autolink fact uses unknown flags",
        )),
        M11InlineProjectionKind::AutolinkEmail if flags == 0 => Ok(()),
        M11InlineProjectionKind::AutolinkEmail => Err(M11InlineProjectionError::InvalidFact(
            "bare email autolink fact cannot carry URI flags",
        )),
        _ => Err(M11InlineProjectionError::InvalidFact(
            "bare autolink fact requires a URI or email kind",
        )),
    }
}

fn validate_fact(fact: M11InlineProjectionFact) -> Result<(), M11InlineProjectionError> {
    let relative_end = fact
        .relative_start
        .checked_add(fact.relative_len)
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    if fact.kind == M11InlineProjectionKind::CharacterReference {
        if !matches!(
            fact.payload,
            M11InlineProjectionFactPayload::CharacterReference { .. }
        ) || fact.flags != 0
            || !(4..=M11_INLINE_CHARACTER_REFERENCE_SOURCE_MAX_BYTES).contains(&fact.relative_len)
        {
            return Err(M11InlineProjectionError::InvalidFact(
                "character reference payload or source extent is invalid",
            ));
        }
        return Ok(());
    }
    let M11InlineProjectionFactPayload::Marked {
        content_offset,
        content_len,
    } = fact.payload
    else {
        return Err(M11InlineProjectionError::InvalidFact(
            "non-character-reference fact carries cooked scalar payload",
        ));
    };
    let content_start = fact
        .relative_start
        .checked_add(content_offset)
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    let content_end = content_start
        .checked_add(content_len)
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    if fact.relative_len == 0 || content_start > content_end || content_end > relative_end {
        return Err(M11InlineProjectionError::InvalidFact(
            "inline Projection content is outside its marked extent",
        ));
    }
    let content_end_offset = content_offset
        .checked_add(content_len)
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    let closer_len = fact
        .relative_len
        .checked_sub(content_end_offset)
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    if fact.kind.is_autolink() {
        if content_offset == 0 && content_len == fact.relative_len {
            return validate_bare_autolink_flags(fact.kind, fact.flags);
        }
        if content_offset != 1 || closer_len != 1 {
            return Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact must preserve one-byte angle markers",
            ));
        }
        if fact.flags != 0 {
            return Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact cannot carry bare-autolink flags",
            ));
        }
        return Ok(());
    }
    if content_offset == 0 || (!fact.kind.has_collapsed_closer() && content_end == relative_end) {
        return Err(M11InlineProjectionError::InvalidFact(
            "inline Projection content is outside its marked extent",
        ));
    }
    if fact.kind == M11InlineProjectionKind::BackslashEscape
        && (fact.relative_len != 2 || content_offset != 1 || content_len != 1 || closer_len != 0)
    {
        return Err(M11InlineProjectionError::InvalidFact(
            "backslash escape fact must preserve one-byte opener and escaped content",
        ));
    }
    if fact.kind == M11InlineProjectionKind::HardLineBreak
        && (!(1..=2).contains(&content_len) || closer_len != 0)
    {
        return Err(M11InlineProjectionError::InvalidFact(
            "hard line break fact must preserve a nonempty marker and exact physical EOL",
        ));
    }
    if fact.kind == M11InlineProjectionKind::Code {
        if fact.flags & !M11_INLINE_PROJECTION_CODE_FLAGS != 0 {
            return Err(M11InlineProjectionError::InvalidFact(
                "inline code fact uses unknown flags",
            ));
        }
    } else if fact.flags != 0 {
        return Err(M11InlineProjectionError::InvalidFact(
            "non-code inline fact carries code flags",
        ));
    }
    Ok(())
}

fn encode_persistent_inline_projection_descriptor(
    descriptor: &M11InlineProjectionDescriptor,
    payload_bytes: u64,
    encoded_bytes: u64,
    checksum: [u8; 32],
    link_value_payload_bytes: u64,
    link_value_tree_encoded_bytes: u64,
    link_value_checksum: [u8; 32],
) -> [u8; PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES] {
    let mut output = [0_u8; PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES];
    let mut cursor = 0;
    let mut write = |bytes: &[u8]| {
        let end = cursor + bytes.len();
        output[cursor..end].copy_from_slice(bytes);
        cursor = end;
    };
    write(&PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_MAGIC);
    write(&PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_VERSION.to_le_bytes());
    write(&descriptor.source.root().get().to_le_bytes());
    write(&descriptor.source.revision().get().to_le_bytes());
    write(&(descriptor.source.byte_len() as u64).to_le_bytes());
    write(&(descriptor.source.utf16_len() as u64).to_le_bytes());
    write(&descriptor.parser_profile.get().to_le_bytes());
    write(&descriptor.source_range.start.to_le_bytes());
    write(&descriptor.source_range.end.to_le_bytes());
    write(&descriptor.logical_page_count.to_le_bytes());
    write(&descriptor.fact_count.to_le_bytes());
    write(&descriptor.storage_page_count.to_le_bytes());
    write(&payload_bytes.to_le_bytes());
    write(&encoded_bytes.to_le_bytes());
    write(&checksum);
    write(&descriptor.ordered_commitment256);
    write(&u64::from(descriptor.link_value_entry_count).to_le_bytes());
    write(&descriptor.link_value_record_count.to_le_bytes());
    write(&descriptor.link_value_storage_page_count.to_le_bytes());
    write(&link_value_payload_bytes.to_le_bytes());
    write(&link_value_tree_encoded_bytes.to_le_bytes());
    write(&u64::from(descriptor.link_value_encoded_bytes).to_le_bytes());
    write(&link_value_checksum);
    write(&descriptor.link_value_ordered_commitment256);
    write(
        &(if descriptor.link_value_entry_count == 0 {
            0_u32
        } else {
            1_u32
        })
        .to_le_bytes(),
    );
    write(&0_u32.to_le_bytes());
    debug_assert_eq!(cursor, PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES);
    output
}

pub(crate) fn decode_persistent_inline_projection_descriptor(
    bytes: &[u8],
    expected_source: SourceVersion,
    expected_profile: ParserProfileId,
) -> Result<PersistentM11InlineProjectionDescriptor, M11InlineProjectionError> {
    if bytes.len() != PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES
        || bytes[..4] != PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_MAGIC
        || read_u32(bytes, 4)? != PERSISTENT_INLINE_PROJECTION_DESCRIPTOR_VERSION
        || read_u64(bytes, 8)? != expected_source.root().get()
        || read_u64(bytes, 16)? != expected_source.revision().get()
        || read_u64(bytes, 24)?
            != u64::try_from(expected_source.byte_len())
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
        || read_u64(bytes, 32)?
            != u64::try_from(expected_source.utf16_len())
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
    {
        return Err(M11InlineProjectionError::SourceAuthorityMismatch);
    }
    if read_u64(bytes, 40)? != expected_profile.get() {
        return Err(M11InlineProjectionError::ParserProfileMismatch);
    }
    let source_start = read_u32(bytes, 48)?;
    let source_end = read_u32(bytes, 52)?;
    let logical_page_count = read_u64(bytes, 56)?;
    let fact_count = read_u64(bytes, 64)?;
    let storage_page_count = read_u64(bytes, 72)?;
    let payload_bytes = read_u64(bytes, 80)?;
    let encoded_bytes = read_u64(bytes, 88)?;
    let checksum: [u8; 32] = bytes[96..128]
        .try_into()
        .expect("fixed inline Projection checksum");
    let ordered_commitment256: [u8; 32] = bytes[128..160]
        .try_into()
        .expect("fixed inline Projection commitment");
    let link_value_entry_count = u32::try_from(read_u64(bytes, 160)?).map_err(|_| {
        M11InlineProjectionError::Malformed("inline link value entry count exceeds u32")
    })?;
    let link_value_record_count = read_u64(bytes, 168)?;
    let link_value_storage_page_count = read_u64(bytes, 176)?;
    let link_value_payload_bytes = read_u64(bytes, 184)?;
    let link_value_tree_encoded_bytes = read_u64(bytes, 192)?;
    let link_value_encoded_bytes = u32::try_from(read_u64(bytes, 200)?).map_err(|_| {
        M11InlineProjectionError::Malformed("inline link value public bytes exceed u32")
    })?;
    let link_value_checksum: [u8; 32] = bytes[208..240]
        .try_into()
        .expect("fixed inline link-value checksum");
    let link_value_ordered_commitment256: [u8; 32] = bytes[240..272]
        .try_into()
        .expect("fixed inline link-value commitment");
    let link_value_flags = read_u32(bytes, 272)?;
    let link_values_absent = link_value_entry_count == 0;
    let expected_link_value_records = u64::from(link_value_payload_bytes != 0)
        .checked_add(
            link_value_payload_bytes.saturating_sub(1)
                / u64::try_from(INLINE_LINK_VALUE_CHUNK_BYTES)
                    .expect("link-value chunk width fits u64"),
        )
        .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
    if source_start > source_end
        || usize::try_from(source_end)
            .ok()
            .is_none_or(|end| end > expected_source.byte_len())
        || (logical_page_count == 0)
            != (fact_count == 0
                && storage_page_count == 0
                && payload_bytes == 0
                && encoded_bytes == 0
                && checksum == [0; 32])
        || logical_page_count > 0
            && (fact_count < logical_page_count
                || storage_page_count == 0
                || payload_bytes == 0
                || encoded_bytes == 0)
        || read_u32(bytes, 276)? != 0
        || link_values_absent
            && (link_value_flags != 0
                || link_value_record_count != 0
                || link_value_storage_page_count != 0
                || link_value_payload_bytes != 0
                || link_value_tree_encoded_bytes != 0
                || link_value_encoded_bytes != 0
                || link_value_checksum != [0; 32]
                || link_value_ordered_commitment256 != [0; 32])
        || !link_values_absent
            && (link_value_flags != 1
                || link_value_entry_count > M11_INLINE_LINK_VALUES_MAX_ENTRIES
                || link_value_record_count != expected_link_value_records
                || link_value_storage_page_count == 0
                || link_value_payload_bytes
                    < u64::from(link_value_entry_count)
                        * u64::try_from(INLINE_LINK_VALUE_ENTRY_BYTES)
                            .expect("entry header fits u64")
                || link_value_tree_encoded_bytes == 0
                || !(16..=M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES)
                    .contains(&usize::try_from(link_value_encoded_bytes).unwrap_or(usize::MAX))
                || u64::from(link_value_encoded_bytes)
                    != link_value_payload_bytes.saturating_add(16))
        || payload_bytes
            .checked_add(link_value_payload_bytes)
            .is_none()
    {
        return Err(M11InlineProjectionError::Malformed(
            "persistent inline Projection descriptor dimensions are invalid",
        ));
    }
    Ok(PersistentM11InlineProjectionDescriptor {
        source: expected_source,
        parser_profile: expected_profile,
        source_start,
        source_end,
        logical_page_count,
        fact_count,
        storage_page_count,
        payload_bytes,
        encoded_bytes,
        checksum,
        ordered_commitment256,
        link_value_entry_count,
        link_value_record_count,
        link_value_storage_page_count,
        link_value_payload_bytes,
        link_value_tree_encoded_bytes,
        link_value_encoded_bytes,
        link_value_checksum,
        link_value_ordered_commitment256,
    })
}

pub(crate) fn validate_persistent_inline_projection_role(
    arena: &PageArena,
    fact_root: Option<ArenaId>,
    link_value_root: Option<ArenaId>,
    descriptor_bytes: &[u8],
    expected_source: SourceVersion,
    expected_profile: ParserProfileId,
) -> Result<PersistentM11InlineProjectionDescriptor, M11InlineProjectionError> {
    let descriptor = decode_persistent_inline_projection_descriptor(
        descriptor_bytes,
        expected_source,
        expected_profile,
    )?;
    validate_imported_m11_parser_page_root(arena, fact_root, descriptor.page_claim())?;
    validate_imported_m11_parser_page_root(
        arena,
        link_value_root,
        descriptor.link_value_page_claim(),
    )?;
    Ok(descriptor)
}

pub(crate) fn persistent_inline_projection_record_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11InlineProjectionDescriptor,
    ordinal: u64,
) -> Result<M11ParserPageRecord, M11InlineProjectionError> {
    Ok(imported_m11_parser_page_record_at(
        arena,
        root,
        descriptor.page_claim(),
        ordinal,
    )?)
}

pub(crate) fn persistent_inline_link_value_record_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11InlineProjectionDescriptor,
    ordinal: u64,
) -> Result<M11ParserPageRecord, M11InlineProjectionError> {
    Ok(imported_m11_parser_page_record_at(
        arena,
        root,
        descriptor.link_value_page_claim(),
        ordinal,
    )?)
}

fn decode_link_value_entries(
    bytes: &[u8],
    entry_count: u32,
) -> Result<Vec<M11InlineLinkValue>, M11InlineProjectionError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(
            usize::try_from(entry_count)
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
        )
        .map_err(|_| {
            M11InlineProjectionError::InvalidLinkValue("inline link value decode allocation failed")
        })?;
    let mut cursor = 0_usize;
    let mut previous_ordinal = None;
    for _ in 0..entry_count {
        let header = bytes
            .get(cursor..cursor + INLINE_LINK_VALUE_ENTRY_BYTES)
            .ok_or(M11InlineProjectionError::InvalidLinkValue(
                "inline link value entry header is truncated",
            ))?;
        let parent_fact_ordinal = read_u32(header, 0)?;
        let flags = read_u32(header, 4)?;
        let destination_start = read_u32(header, 8)?;
        let destination_len = read_u32(header, 12)?;
        let title_start = read_u32(header, 16)?;
        let title_len = read_u32(header, 20)?;
        let cooked_destination_len = usize::try_from(read_u32(header, 24)?)
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        let cooked_title_len = usize::try_from(read_u32(header, 28)?)
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        if flags & !INLINE_LINK_VALUE_TITLE_PRESENT != 0
            || previous_ordinal.is_some_and(|previous| parent_fact_ordinal <= previous)
        {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "inline link values are not canonical or strictly ordered",
            ));
        }
        let title_present = flags == INLINE_LINK_VALUE_TITLE_PRESENT;
        if (!title_present && (title_start != 0 || title_len != 0 || cooked_title_len != 0))
            || (title_present && title_len == 0)
        {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "inline link value title presence encoding is invalid",
            ));
        }
        let destination_end = destination_start
            .checked_add(destination_len)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let title_end = title_start
            .checked_add(title_len)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        cursor = cursor
            .checked_add(INLINE_LINK_VALUE_ENTRY_BYTES)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let destination_end_bytes = cursor
            .checked_add(cooked_destination_len)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let cooked_destination =
            std::str::from_utf8(bytes.get(cursor..destination_end_bytes).ok_or(
                M11InlineProjectionError::InvalidLinkValue(
                    "cooked inline destination is truncated",
                ),
            )?)
            .map_err(|_| {
                M11InlineProjectionError::InvalidLinkValue(
                    "cooked inline destination is not valid UTF-8",
                )
            })?
            .to_owned()
            .into_boxed_str();
        let title_end_bytes = destination_end_bytes
            .checked_add(cooked_title_len)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let cooked_title = if title_present {
            Some(
                std::str::from_utf8(bytes.get(destination_end_bytes..title_end_bytes).ok_or(
                    M11InlineProjectionError::InvalidLinkValue("cooked inline title is truncated"),
                )?)
                .map_err(|_| {
                    M11InlineProjectionError::InvalidLinkValue(
                        "cooked inline title is not valid UTF-8",
                    )
                })?
                .to_owned()
                .into_boxed_str(),
            )
        } else {
            None
        };
        values.push(M11InlineLinkValue {
            parent_fact_ordinal,
            destination_source_range: destination_start..destination_end,
            title_source_range: title_present.then_some(title_start..title_end),
            cooked_destination,
            cooked_title,
        });
        cursor = title_end_bytes;
        previous_ordinal = Some(parent_fact_ordinal);
    }
    if cursor != bytes.len() {
        return Err(M11InlineProjectionError::InvalidLinkValue(
            "inline link value stream has orphan trailing bytes",
        ));
    }
    Ok(values)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11InlineLinkValueEncodeReceipt {
    pub(crate) entry_count: u32,
    pub(crate) tree_nodes_visited: u64,
}

pub(crate) fn encode_persistent_inline_link_values(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11InlineProjectionDescriptor,
    output: &mut [u8],
) -> Result<PersistentM11InlineLinkValueEncodeReceipt, M11InlineProjectionError> {
    if descriptor.link_value_entry_count == 0 {
        if !output.is_empty() {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "absent inline link values require an empty output",
            ));
        }
        return Ok(PersistentM11InlineLinkValueEncodeReceipt {
            entry_count: 0,
            tree_nodes_visited: 0,
        });
    }
    let expected = usize::try_from(descriptor.link_value_encoded_bytes)
        .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
    if output.len() != expected {
        return Err(M11InlineProjectionError::InvalidLinkValue(
            "inline link value output length changed after preflight",
        ));
    }
    output[..8].copy_from_slice(b"FLKIV001");
    output[8..12].copy_from_slice(&1_u32.to_le_bytes());
    output[12..16].copy_from_slice(&descriptor.link_value_entry_count.to_le_bytes());
    let mut offset = 16_usize;
    let mut tree_nodes_visited = 0_u64;
    for ordinal in 0..descriptor.link_value_record_count {
        let mut inspection = SequenceInspectionReceipt::default();
        let record = imported_m11_parser_page_record_at_inspected(
            arena,
            root,
            descriptor.link_value_page_claim(),
            ordinal,
            &mut inspection,
        )?;
        tree_nodes_visited = tree_nodes_visited
            .checked_add(inspection.node_headers_decoded)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let expected_chunk = usize::try_from(descriptor.link_value_payload_bytes)
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
            .saturating_sub(offset - 16)
            .min(INLINE_LINK_VALUE_CHUNK_BYTES);
        if record.as_bytes().len() != expected_chunk {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "inline link value chunks are not canonical",
            ));
        }
        let end = offset
            .checked_add(record.as_bytes().len())
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        output[offset..end].copy_from_slice(record.as_bytes());
        offset = end;
    }
    if offset != expected {
        return Err(M11InlineProjectionError::InvalidLinkValue(
            "inline link value query length disagrees with its descriptor",
        ));
    }
    Ok(PersistentM11InlineLinkValueEncodeReceipt {
        entry_count: descriptor.link_value_entry_count,
        tree_nodes_visited,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PersistentM11InlineProjectionHostCursorPoll {
    Fact { fact: M11InlineProjectionFact },
    Complete,
}

/// Installed-only typed replay over an independently validated inline root.
///
/// The cursor never exposes arena identities or raw `IFP2` bytes. It reuses
/// the schema decoder while reconstructing page anchors, and reports the
/// actual measured-tree headers inspected so a host receipt can remain below
/// the descriptor's conservative pre-admission bound.
pub(crate) struct PersistentM11InlineProjectionHostCursor<'arena> {
    arena: &'arena PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11InlineProjectionDescriptor,
    previous_page_anchor: u32,
    last_fact_start: Option<u32>,
    next_page: u64,
    observed_facts: u64,
    current_page: Option<LoadedLogicalPage>,
    tree_nodes_visited: u64,
    complete: bool,
}

impl<'arena> PersistentM11InlineProjectionHostCursor<'arena> {
    pub(crate) fn new(
        arena: &'arena PageArena,
        root: Option<ArenaId>,
        descriptor: PersistentM11InlineProjectionDescriptor,
    ) -> Self {
        Self {
            arena,
            root,
            descriptor,
            previous_page_anchor: 0,
            last_fact_start: None,
            next_page: 0,
            observed_facts: 0,
            current_page: None,
            tree_nodes_visited: 0,
            complete: false,
        }
    }

    pub(crate) fn poll(
        &mut self,
    ) -> Result<PersistentM11InlineProjectionHostCursorPoll, M11InlineProjectionError> {
        if self.complete {
            return Ok(PersistentM11InlineProjectionHostCursorPoll::Complete);
        }
        if let Some(page) = self.current_page.as_mut() {
            if page.next_fact < page.fact_count {
                let fact = decode_fact(page.record.as_bytes(), page.next_fact, page.anchor)?;
                page.next_fact += 1;
                self.observed_facts = self
                    .observed_facts
                    .checked_add(1)
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                return Ok(PersistentM11InlineProjectionHostCursorPoll::Fact { fact });
            }
            self.current_page = None;
        }
        if self.next_page == self.descriptor.logical_page_count {
            if self.observed_facts != self.descriptor.fact_count {
                return Err(M11InlineProjectionError::Malformed(
                    "installed inline Projection cursor count changed",
                ));
            }
            self.complete = true;
            return Ok(PersistentM11InlineProjectionHostCursorPoll::Complete);
        }

        let mut inspection = SequenceInspectionReceipt::default();
        let record = imported_m11_parser_page_record_at_inspected(
            self.arena,
            self.root,
            self.descriptor.page_claim(),
            self.next_page,
            &mut inspection,
        )?;
        self.tree_nodes_visited = self
            .tree_nodes_visited
            .checked_add(inspection.node_headers_decoded)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let source_len = self
            .descriptor
            .source_end
            .checked_sub(self.descriptor.source_start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let decoded = validate_logical_page(
            record.as_bytes(),
            self.previous_page_anchor,
            self.last_fact_start,
            source_len,
        )?;
        self.previous_page_anchor = decoded.anchor;
        self.last_fact_start = Some(decoded.last_fact_start);
        self.next_page = self
            .next_page
            .checked_add(1)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let fact = decode_fact(record.as_bytes(), 0, decoded.anchor)?;
        self.current_page = Some(LoadedLogicalPage {
            record,
            anchor: decoded.anchor,
            next_fact: 1,
            fact_count: decoded.fact_count,
        });
        self.observed_facts = self
            .observed_facts
            .checked_add(1)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        Ok(PersistentM11InlineProjectionHostCursorPoll::Fact { fact })
    }

    pub(crate) const fn tree_nodes_visited(&self) -> u64 {
        self.tree_nodes_visited
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11InlineProjectionHostValidationPoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

/// Fuelled typed validation of one imported persistent inline Projection.
///
/// Generic parser-page admission proves the packed tree shape and its opaque
/// record bytes. This validator supplies the missing schema proof before a
/// host installs that tree: every logical `IFP2` page is decoded in source
/// order, all fact dimensions and kinds are checked, descriptor counts are
/// derived from replay, and the strong ordered commitment is recomputed from
/// the imported bytes. The terminal transition also authenticates the
/// deterministic commitment of an empty stream.
pub(crate) struct PersistentM11InlineProjectionHostValidator {
    fact_root: Option<ArenaId>,
    link_value_root: Option<ArenaId>,
    descriptor: PersistentM11InlineProjectionDescriptor,
    hasher: blake3::Hasher,
    previous_page_anchor: u32,
    last_fact_start: Option<u32>,
    observed_pages: u64,
    observed_facts: u64,
    link_value_facts: Vec<(u32, M11InlineProjectionFact)>,
    observed_link_value_records: u64,
    link_value_bytes: Vec<u8>,
    complete: bool,
}

impl PersistentM11InlineProjectionHostValidator {
    pub(crate) fn new(
        arena: &PageArena,
        fact_root: Option<ArenaId>,
        link_value_root: Option<ArenaId>,
        descriptor: PersistentM11InlineProjectionDescriptor,
    ) -> Result<Self, M11InlineProjectionError> {
        validate_imported_m11_parser_page_root(arena, fact_root, descriptor.page_claim())?;
        validate_imported_m11_parser_page_root(
            arena,
            link_value_root,
            descriptor.link_value_page_claim(),
        )?;
        let mut link_value_facts = Vec::new();
        link_value_facts
            .try_reserve_exact(
                usize::try_from(descriptor.link_value_entry_count)
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            )
            .map_err(|_| {
                M11InlineProjectionError::InvalidLinkValue(
                    "inline link value validation allocation failed",
                )
            })?;
        let mut link_value_bytes = Vec::new();
        link_value_bytes
            .try_reserve_exact(
                usize::try_from(descriptor.link_value_payload_bytes)
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            )
            .map_err(|_| {
                M11InlineProjectionError::InvalidLinkValue(
                    "inline link value validation allocation failed",
                )
            })?;
        Ok(Self {
            fact_root,
            link_value_root,
            descriptor,
            hasher: begin_commitment(
                descriptor.source,
                descriptor.parser_profile,
                &(descriptor.source_start..descriptor.source_end),
            ),
            previous_page_anchor: 0,
            last_fact_start: None,
            observed_pages: 0,
            observed_facts: 0,
            link_value_facts,
            observed_link_value_records: 0,
            link_value_bytes,
            complete: false,
        })
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<PersistentM11InlineProjectionHostValidationPoll, M11InlineProjectionError> {
        if fuel == 0 {
            return Err(M11InlineProjectionError::QueryBudgetInvalid);
        }
        if self.complete {
            return Ok(PersistentM11InlineProjectionHostValidationPoll {
                transitions: 0,
                complete: true,
            });
        }

        let source_len = self
            .descriptor
            .source_end
            .checked_sub(self.descriptor.source_start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let mut transitions = 0;
        while transitions < fuel {
            if self.observed_pages < self.descriptor.logical_page_count {
                let record = persistent_inline_projection_record_at(
                    arena,
                    self.fact_root,
                    self.descriptor,
                    self.observed_pages,
                )?;
                let decoded = validate_logical_page(
                    record.as_bytes(),
                    self.previous_page_anchor,
                    self.last_fact_start,
                    source_len,
                )?;
                for fact_index in 0..decoded.fact_count {
                    let fact = decode_fact(record.as_bytes(), fact_index, decoded.anchor)?;
                    if fact.kind.has_link_value() {
                        let ordinal = self
                            .observed_facts
                            .checked_add(
                                u64::try_from(fact_index)
                                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
                            )
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                        if self.link_value_facts.len()
                            >= usize::try_from(M11_INLINE_LINK_VALUES_MAX_ENTRIES)
                                .expect("entry cap fits usize")
                        {
                            return Err(M11InlineProjectionError::InvalidLinkValue(
                                "link/image fact density exceeds the bounded value lane",
                            ));
                        }
                        self.link_value_facts.push((ordinal, fact));
                    }
                }
                append_page_to_commitment(&mut self.hasher, record.as_bytes());
                self.previous_page_anchor = decoded.anchor;
                self.last_fact_start = Some(decoded.last_fact_start);
                self.observed_pages = self
                    .observed_pages
                    .checked_add(1)
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                self.observed_facts = self
                    .observed_facts
                    .checked_add(
                        u64::try_from(decoded.fact_count)
                            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
                    )
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                transitions += 1;
                continue;
            }

            if self.observed_pages != self.descriptor.logical_page_count
                || self.observed_facts != self.descriptor.fact_count
            {
                return Err(M11InlineProjectionError::Malformed(
                    "persistent inline Projection descriptor counts differ from host replay",
                ));
            }
            let commitment =
                finish_commitment(&self.hasher, self.observed_pages, self.observed_facts);
            if commitment != self.descriptor.ordered_commitment256 {
                return Err(M11InlineProjectionError::CommitmentMismatch);
            }
            if self.observed_link_value_records < self.descriptor.link_value_record_count {
                let record = persistent_inline_link_value_record_at(
                    arena,
                    self.link_value_root,
                    self.descriptor,
                    self.observed_link_value_records,
                )?;
                let remaining = usize::try_from(self.descriptor.link_value_payload_bytes)
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
                    .checked_sub(self.link_value_bytes.len())
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                let expected_chunk = remaining.min(INLINE_LINK_VALUE_CHUNK_BYTES);
                if record.as_bytes().len() != expected_chunk {
                    return Err(M11InlineProjectionError::InvalidLinkValue(
                        "inline link value chunks are not canonical",
                    ));
                }
                self.link_value_bytes.extend_from_slice(record.as_bytes());
                self.observed_link_value_records = self
                    .observed_link_value_records
                    .checked_add(1)
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                transitions += 1;
                continue;
            }
            if self.link_value_bytes.len()
                != usize::try_from(self.descriptor.link_value_payload_bytes)
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
                || self.link_value_facts.len()
                    != usize::try_from(self.descriptor.link_value_entry_count)
                        .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
            {
                return Err(M11InlineProjectionError::InvalidLinkValue(
                    "link/image fact and inline link value counts differ",
                ));
            }
            let expected_link_value_commitment = if self.link_value_facts.is_empty() {
                [0; 32]
            } else {
                finish_link_value_commitment(
                    self.descriptor.source,
                    self.descriptor.parser_profile,
                    &(self.descriptor.source_start..self.descriptor.source_end),
                    &self.link_value_bytes,
                    self.descriptor.link_value_entry_count,
                )
            };
            if expected_link_value_commitment != self.descriptor.link_value_ordered_commitment256 {
                return Err(M11InlineProjectionError::CommitmentMismatch);
            }
            let values = decode_link_value_entries(
                &self.link_value_bytes,
                self.descriptor.link_value_entry_count,
            )?;
            for ((expected_ordinal, fact), value) in self.link_value_facts.iter().zip(&values) {
                if value.parent_fact_ordinal != *expected_ordinal {
                    return Err(M11InlineProjectionError::InvalidLinkValue(
                        "inline link value parent ordinal differs from its direct fact",
                    ));
                }
                value.validate_against_fact(*fact, self.descriptor.source.byte_len())?;
            }
            self.complete = true;
            transitions += 1;
            break;
        }
        Ok(PersistentM11InlineProjectionHostValidationPoll {
            transitions,
            complete: self.complete,
        })
    }
}

fn begin_commitment(
    source: SourceVersion,
    parser_profile: ParserProfileId,
    source_range: &Range<u32>,
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(INLINE_PROJECTION_COMMITMENT_DOMAIN);
    hasher.update(&INLINE_PROJECTION_SCHEMA.to_le_bytes());
    hasher.update(&parser_profile.get().to_le_bytes());
    hasher.update(&source.root().get().to_le_bytes());
    hasher.update(&source.revision().get().to_le_bytes());
    hasher.update(&(source.byte_len() as u64).to_le_bytes());
    hasher.update(&(source.utf16_len() as u64).to_le_bytes());
    hasher.update(&source_range.start.to_le_bytes());
    hasher.update(&source_range.end.to_le_bytes());
    hasher
}

fn append_page_to_commitment(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn finish_commitment(hasher: &blake3::Hasher, pages: u64, facts: u64) -> [u8; 32] {
    let mut hasher = hasher.clone();
    hasher.update(INLINE_PROJECTION_COMMITMENT_TRAILER);
    hasher.update(&pages.to_le_bytes());
    hasher.update(&facts.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn finish_link_value_commitment(
    source: SourceVersion,
    parser_profile: ParserProfileId,
    source_range: &Range<u32>,
    entry_bytes: &[u8],
    entry_count: u32,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(INLINE_LINK_VALUE_COMMITMENT_DOMAIN);
    hasher.update(&parser_profile.get().to_le_bytes());
    hasher.update(&source.root().get().to_le_bytes());
    hasher.update(&source.revision().get().to_le_bytes());
    hasher.update(&(source.byte_len() as u64).to_le_bytes());
    hasher.update(&(source.utf16_len() as u64).to_le_bytes());
    hasher.update(&source_range.start.to_le_bytes());
    hasher.update(&source_range.end.to_le_bytes());
    hasher.update(&(entry_bytes.len() as u64).to_le_bytes());
    hasher.update(entry_bytes);
    hasher.update(INLINE_LINK_VALUE_COMMITMENT_TRAILER);
    hasher.update(&entry_count.to_le_bytes());
    hasher.update(
        &u64::try_from(16_usize + entry_bytes.len())
            .expect("bounded link-value payload fits u64")
            .to_le_bytes(),
    );
    *hasher.finalize().as_bytes()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, M11InlineProjectionError> {
    let bytes = bytes
        .get(offset..offset + 2)
        .ok_or(M11InlineProjectionError::Malformed(
            "inline Projection u16 is truncated",
        ))?;
    Ok(u16::from_le_bytes(
        bytes.try_into().expect("checked u16 width"),
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, M11InlineProjectionError> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or(M11InlineProjectionError::Malformed(
            "inline Projection u32 is truncated",
        ))?;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("checked u32 width"),
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, M11InlineProjectionError> {
    let bytes = bytes
        .get(offset..offset + 8)
        .ok_or(M11InlineProjectionError::Malformed(
            "inline Projection u64 is truncated",
        ))?;
    Ok(u64::from_le_bytes(
        bytes.try_into().expect("checked u64 width"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentRuntimeConfig;

    fn profile() -> ParserProfileId {
        ParserProfileId::new(1).expect("parser profile")
    }

    fn accept_inline_page(
        build: &mut M11InlineProjectionBuild,
        runtime: &mut DocumentRuntime,
        facts: &[M11InlineProjectionFact],
    ) {
        build.offer_page(facts).expect("offer inline page");
        loop {
            match build.poll(runtime, 32).expect("poll inline page").status() {
                M11InlineProjectionBuildStatus::NeedsPage => break,
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete
                | M11InlineProjectionBuildStatus::Cancelled => {
                    panic!("inline build ended before input")
                }
            }
        }
    }

    fn finish_inline_build(
        build: &mut M11InlineProjectionBuild,
        runtime: &mut DocumentRuntime,
    ) -> M11InlineProjectionRoot {
        build.finish_input().expect("finish inline input");
        loop {
            match build
                .poll(runtime, 32)
                .expect("poll inline finish")
                .status()
            {
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete => {
                    return build.take_root().expect("inline root")
                }
                M11InlineProjectionBuildStatus::NeedsPage
                | M11InlineProjectionBuildStatus::Cancelled => {
                    panic!("finished inline build returned the wrong state")
                }
            }
        }
    }

    fn persistent_descriptor(
        root: &M11InlineProjectionRoot,
    ) -> PersistentM11InlineProjectionDescriptor {
        PersistentM11InlineProjectionDescriptor {
            source: root.descriptor.source,
            parser_profile: root.descriptor.parser_profile,
            source_start: root.descriptor.source_range.start,
            source_end: root.descriptor.source_range.end,
            logical_page_count: root.descriptor.logical_page_count,
            fact_count: root.descriptor.fact_count,
            storage_page_count: root.inner.page_count(),
            payload_bytes: root.inner.payload_bytes(),
            encoded_bytes: root.inner.encoded_bytes(),
            checksum: root.inner.checksum(),
            ordered_commitment256: root.descriptor.ordered_commitment256,
            link_value_entry_count: root.descriptor.link_value_entry_count,
            link_value_record_count: root.link_values.record_count(),
            link_value_storage_page_count: root.link_values.page_count(),
            link_value_payload_bytes: root.link_values.payload_bytes(),
            link_value_tree_encoded_bytes: root.link_values.encoded_bytes(),
            link_value_encoded_bytes: root.descriptor.link_value_encoded_bytes,
            link_value_checksum: root.link_values.checksum(),
            link_value_ordered_commitment256: root.descriptor.link_value_ordered_commitment256,
        }
    }

    fn release_inline_root(root: &mut M11InlineProjectionRoot, runtime: &mut DocumentRuntime) {
        root.begin_release(runtime).expect("begin inline release");
        while !root
            .poll_release(runtime, 32)
            .expect("poll inline release")
            .complete()
        {}
    }

    fn generic_root_with_record(runtime: &mut DocumentRuntime, bytes: &[u8]) -> M11ParserPageRoot {
        let source = runtime.current_source_version().expect("source");
        let mut build = M11ParserPageBuild::new(
            runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..source.byte_len(),
            INLINE_PROJECTION_STREAM_TAG,
        )
        .expect("generic inline page build");
        build
            .offer_record(M11ParserPageRecord::new(bytes).expect("generic record"))
            .expect("offer generic record");
        loop {
            match build
                .poll(runtime, 32)
                .expect("poll generic record")
                .status()
            {
                M11ParserPageBuildStatus::NeedsInput => break,
                M11ParserPageBuildStatus::Pending => {}
                M11ParserPageBuildStatus::Complete | M11ParserPageBuildStatus::Cancelled => {
                    panic!("generic page build ended before input")
                }
            }
        }
        build.finish_input().expect("finish generic input");
        loop {
            match build
                .poll(runtime, 32)
                .expect("finish generic root")
                .status()
            {
                M11ParserPageBuildStatus::Pending => {}
                M11ParserPageBuildStatus::Complete => {
                    return build.take_root().expect("generic root")
                }
                M11ParserPageBuildStatus::NeedsInput | M11ParserPageBuildStatus::Cancelled => {
                    panic!("finished generic build returned the wrong state")
                }
            }
        }
    }

    fn release_generic_root(root: &mut M11ParserPageRoot, runtime: &mut DocumentRuntime) {
        root.begin_release(runtime).expect("begin generic release");
        while !root
            .poll_release(runtime, 32)
            .expect("poll generic release")
            .complete()
        {}
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn malformed_page_schema_reserved_bytes_and_maximum_extent_fail_closed() {
        let fact = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 7..13, 9..11)
            .expect("fact");
        let valid = encode_logical_page(7, 6, 7, &[fact]).expect("page");
        validate_logical_page(valid.as_bytes(), 0, None, 32).expect("valid page");

        let mut wrong_schema = valid.as_bytes().to_vec();
        wrong_schema[4] ^= 1;
        assert!(matches!(
            validate_logical_page(&wrong_schema, 0, None, 32),
            Err(M11InlineProjectionError::Malformed(
                "inline Projection page schema is unsupported"
            ))
        ));

        let mut reserved = valid.as_bytes().to_vec();
        reserved[18] = 1;
        assert!(matches!(
            validate_logical_page(&reserved, 0, None, 32),
            Err(M11InlineProjectionError::Malformed(
                "inline Projection page dimensions are invalid"
            ))
        ));

        let mut wrong_extent = valid.as_bytes().to_vec();
        wrong_extent[12..16].copy_from_slice(&5_u32.to_le_bytes());
        assert!(matches!(
            validate_logical_page(&wrong_extent, 0, None, 32),
            Err(M11InlineProjectionError::Malformed(
                "inline Projection page maximum extent changed"
            ))
        ));
    }

    #[test]
    fn angle_autolink_fact_codec_requires_exact_one_byte_markers() {
        let valid =
            M11InlineProjectionFact::new(M11InlineProjectionKind::AutolinkUri, 0, 7..15, 8..14)
                .expect("one-byte angle markers");
        assert_eq!(valid.relative_range(), 7..15);
        assert_eq!(valid.relative_content_range(), 8..14);
        assert!(matches!(
            M11InlineProjectionFact::new(M11InlineProjectionKind::AutolinkEmail, 0, 7..16, 9..15,),
            Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact must preserve one-byte angle markers"
            ))
        ));
        assert!(matches!(
            M11InlineProjectionFact::new(
                M11InlineProjectionKind::AutolinkUri,
                M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
                7..15,
                8..14,
            ),
            Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact cannot carry bare-autolink flags"
            ))
        ));

        let strong = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 7..13, 9..11)
            .expect("strong fact");
        let page = encode_logical_page(7, 6, 7, &[strong]).expect("page");
        let mut corrupted = page.as_bytes().to_vec();
        corrupted[INLINE_PROJECTION_PAGE_HEADER_BYTES] = M11InlineProjectionKind::AutolinkUri as u8;
        assert!(matches!(
            validate_logical_page(&corrupted, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact must preserve one-byte angle markers"
            ))
        ));

        let mut flagged_angle = encode_logical_page(7, 8, 7, &[valid])
            .expect("angle page")
            .as_bytes()
            .to_vec();
        flagged_angle[INLINE_PROJECTION_PAGE_HEADER_BYTES + 1] =
            M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW;
        assert!(matches!(
            validate_logical_page(&flagged_angle, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact cannot carry bare-autolink flags"
            ))
        ));
    }

    #[test]
    fn bare_autolink_fact_codec_uses_exact_markerless_geometry_and_kind_specific_flags() {
        let scheme_uri = M11InlineProjectionFact::new_bare_autolink(
            M11InlineProjectionKind::AutolinkUri,
            0,
            7..27,
        )
        .expect("bare scheme URI");
        let www_uri = M11InlineProjectionFact::new_bare_autolink(
            M11InlineProjectionKind::AutolinkUri,
            M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
            30..45,
        )
        .expect("bare www URI");
        let email = M11InlineProjectionFact::new_bare_autolink(
            M11InlineProjectionKind::AutolinkEmail,
            0,
            48..64,
        )
        .expect("bare email");

        for fact in [scheme_uri, www_uri, email] {
            assert_eq!(fact.relative_content_range(), fact.relative_range());
            let range = fact.relative_range();
            let page =
                encode_logical_page(range.start, range.end - range.start, range.start, &[fact])
                    .expect("bare autolink page");
            validate_logical_page(page.as_bytes(), 0, None, 80)
                .expect("canonical bare autolink page");
            assert_eq!(
                decode_fact(page.as_bytes(), 0, range.start).expect("decoded bare autolink"),
                fact
            );
        }
        assert_eq!(scheme_uri.flags(), 0);
        assert_eq!(www_uri.flags(), M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW);
        assert_eq!(email.flags(), 0);

        assert!(matches!(
            M11InlineProjectionFact::new_bare_autolink(M11InlineProjectionKind::Strong, 0, 7..15,),
            Err(M11InlineProjectionError::InvalidFact(
                "bare autolink fact requires a URI or email kind"
            ))
        ));
        assert!(matches!(
            M11InlineProjectionFact::new_bare_autolink(
                M11InlineProjectionKind::AutolinkUri,
                2,
                7..15,
            ),
            Err(M11InlineProjectionError::InvalidFact(
                "bare URI autolink fact uses unknown flags"
            ))
        ));
        assert!(matches!(
            M11InlineProjectionFact::new_bare_autolink(
                M11InlineProjectionKind::AutolinkEmail,
                M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
                7..15,
            ),
            Err(M11InlineProjectionError::InvalidFact(
                "bare email autolink fact cannot carry URI flags"
            ))
        ));
        assert!(matches!(
            M11InlineProjectionFact::new_bare_autolink(
                M11InlineProjectionKind::AutolinkUri,
                0,
                7..7,
            ),
            Err(M11InlineProjectionError::InvalidFact(
                "bare autolink source extent must be nonempty"
            ))
        ));
    }

    #[test]
    fn bare_autolink_fact_decoder_rejects_illegal_flag_and_geometry_combinations() {
        let email = M11InlineProjectionFact::new_bare_autolink(
            M11InlineProjectionKind::AutolinkEmail,
            0,
            7..15,
        )
        .expect("bare email");
        let page = encode_logical_page(7, 8, 7, &[email]).expect("bare email page");

        let mut email_with_uri_flag = page.as_bytes().to_vec();
        email_with_uri_flag[INLINE_PROJECTION_PAGE_HEADER_BYTES + 1] =
            M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW;
        assert!(matches!(
            validate_logical_page(&email_with_uri_flag, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "bare email autolink fact cannot carry URI flags"
            ))
        ));

        let mut unknown_uri_flag = page.as_bytes().to_vec();
        unknown_uri_flag[INLINE_PROJECTION_PAGE_HEADER_BYTES] =
            M11InlineProjectionKind::AutolinkUri as u8;
        unknown_uri_flag[INLINE_PROJECTION_PAGE_HEADER_BYTES + 1] = 2;
        assert!(matches!(
            validate_logical_page(&unknown_uri_flag, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "bare URI autolink fact uses unknown flags"
            ))
        ));

        let mut partial_markerless_content = page.as_bytes().to_vec();
        partial_markerless_content
            [INLINE_PROJECTION_PAGE_HEADER_BYTES + 16..INLINE_PROJECTION_PAGE_HEADER_BYTES + 20]
            .copy_from_slice(&7_u32.to_le_bytes());
        assert!(matches!(
            validate_logical_page(&partial_markerless_content, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "angle autolink fact must preserve one-byte angle markers"
            ))
        ));
    }

    #[test]
    fn backslash_escape_fact_codec_requires_one_hidden_byte_and_no_closer() {
        let valid =
            M11InlineProjectionFact::new(M11InlineProjectionKind::BackslashEscape, 0, 7..9, 8..9)
                .expect("one-byte backslash opener");
        assert_eq!(valid.relative_range(), 7..9);
        assert_eq!(valid.relative_content_range(), 8..9);
        let page = encode_logical_page(7, 2, 7, &[valid]).expect("escape page");
        validate_logical_page(page.as_bytes(), 0, None, 32).expect("valid escape page");
        assert_eq!(
            decode_fact(page.as_bytes(), 0, 7).expect("decoded escape"),
            valid
        );

        assert!(matches!(
            M11InlineProjectionFact::new(M11InlineProjectionKind::BackslashEscape, 0, 7..10, 8..10,),
            Err(M11InlineProjectionError::InvalidFact(
                "backslash escape fact must preserve one-byte opener and escaped content"
            ))
        ));

        let strong = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 7..13, 9..11)
            .expect("strong fact");
        let page = encode_logical_page(7, 6, 7, &[strong]).expect("page");
        let mut corrupted = page.as_bytes().to_vec();
        corrupted[INLINE_PROJECTION_PAGE_HEADER_BYTES] =
            M11InlineProjectionKind::BackslashEscape as u8;
        assert!(matches!(
            validate_logical_page(&corrupted, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "backslash escape fact must preserve one-byte opener and escaped content"
            ))
        ));
    }

    #[test]
    fn hard_line_break_fact_codec_requires_marker_physical_eol_and_no_closer() {
        for valid in [
            M11InlineProjectionFact::new(M11InlineProjectionKind::HardLineBreak, 0, 7..9, 8..9)
                .expect("one-byte marker and LF/CR"),
            M11InlineProjectionFact::new(M11InlineProjectionKind::HardLineBreak, 0, 7..11, 9..11)
                .expect("two-byte marker and CRLF"),
            M11InlineProjectionFact::new(M11InlineProjectionKind::HardLineBreak, 0, 7..12, 10..12)
                .expect("three-byte marker and CRLF"),
        ] {
            let range = valid.relative_range();
            let page =
                encode_logical_page(range.start, range.end - range.start, range.start, &[valid])
                    .expect("hard-break page");
            validate_logical_page(page.as_bytes(), 0, None, 32).expect("valid hard-break page");
            assert_eq!(
                decode_fact(page.as_bytes(), 0, range.start).expect("decoded hard break"),
                valid
            );
        }

        for malformed in [
            M11InlineProjectionFact::new(M11InlineProjectionKind::HardLineBreak, 0, 7..11, 8..11),
            M11InlineProjectionFact::new(M11InlineProjectionKind::HardLineBreak, 0, 7..11, 9..10),
        ] {
            assert!(matches!(
                malformed,
                Err(M11InlineProjectionError::InvalidFact(
                    "hard line break fact must preserve a nonempty marker and exact physical EOL"
                ))
            ));
        }

        let strong = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 7..13, 9..11)
            .expect("strong fact");
        let page = encode_logical_page(7, 6, 7, &[strong]).expect("page");
        let mut corrupted = page.as_bytes().to_vec();
        corrupted[INLINE_PROJECTION_PAGE_HEADER_BYTES] =
            M11InlineProjectionKind::HardLineBreak as u8;
        assert!(matches!(
            validate_logical_page(&corrupted, 0, None, 32),
            Err(M11InlineProjectionError::InvalidFact(
                "hard line break fact must preserve a nonempty marker and exact physical EOL"
            ))
        ));
    }

    #[test]
    fn character_reference_fact_codec_carries_one_or_two_typed_scalars() {
        let single = M11InlineProjectionFact::new_character_reference(7..13, '©', None)
            .expect("single-scalar character reference");
        assert_eq!(single.kind(), M11InlineProjectionKind::CharacterReference);
        assert_eq!(single.flags(), 0);
        assert_eq!(single.relative_range(), 7..13);
        assert_eq!(single.relative_content_range(), 7..13);
        assert_eq!(single.character_reference(), Some(('©', None)));
        let single_page = encode_logical_page(7, 6, 7, &[single]).expect("single-scalar page");
        let single_record = INLINE_PROJECTION_PAGE_HEADER_BYTES;
        assert_eq!(single_page.as_bytes()[single_record + 1], 1);
        assert_eq!(
            read_u32(single_page.as_bytes(), single_record + 12).expect("first scalar"),
            '©' as u32
        );
        assert_eq!(
            read_u32(single_page.as_bytes(), single_record + 16).expect("second sentinel"),
            0
        );
        assert_eq!(
            decode_fact(single_page.as_bytes(), 0, 7).expect("decode single scalar"),
            single
        );

        let double =
            M11InlineProjectionFact::new_character_reference(20..35, '\u{2242}', Some('\u{0338}'))
                .expect("double-scalar character reference");
        let double_page = encode_logical_page(13, 15, 20, &[double]).expect("double-scalar page");
        let double_record = INLINE_PROJECTION_PAGE_HEADER_BYTES;
        assert_eq!(double_page.as_bytes()[double_record + 1], 2);
        assert_eq!(
            decode_fact(double_page.as_bytes(), 0, 20).expect("decode double scalar"),
            double
        );

        assert!(matches!(
            M11InlineProjectionFact::new(
                M11InlineProjectionKind::CharacterReference,
                0,
                7..13,
                8..12,
            ),
            Err(M11InlineProjectionError::InvalidFact(
                "character references require their typed scalar constructor"
            ))
        ));
        for range in [0..3, 0..34] {
            assert!(matches!(
                M11InlineProjectionFact::new_character_reference(range, '&', None),
                Err(M11InlineProjectionError::InvalidFact(
                    "character reference source extent is outside the bounded grammar"
                ))
            ));
        }
        assert!(matches!(
            M11InlineProjectionFact::new_character_reference(0..4, '&', Some('\0')),
            Err(M11InlineProjectionError::InvalidFact(
                "character reference second scalar must be nonzero"
            ))
        ));
    }

    #[test]
    fn character_reference_fact_decoder_rejects_malformed_scalar_records() {
        let fact =
            M11InlineProjectionFact::new_character_reference(7..22, '\u{2242}', Some('\u{0338}'))
                .expect("double-scalar character reference");
        let page = encode_logical_page(7, 15, 7, &[fact]).expect("character-reference page");
        let record = INLINE_PROJECTION_PAGE_HEADER_BYTES;

        for count in [0, 3] {
            let mut malformed = page.as_bytes().to_vec();
            malformed[record + 1] = count;
            assert!(matches!(
                validate_logical_page(&malformed, 0, None, 64),
                Err(M11InlineProjectionError::Malformed(
                    "character reference scalar count or sentinel is invalid"
                ))
            ));
        }

        let mut nonzero_single_sentinel = page.as_bytes().to_vec();
        nonzero_single_sentinel[record + 1] = 1;
        assert!(matches!(
            validate_logical_page(&nonzero_single_sentinel, 0, None, 64),
            Err(M11InlineProjectionError::Malformed(
                "character reference scalar count or sentinel is invalid"
            ))
        ));

        let mut zero_second_for_count_two = page.as_bytes().to_vec();
        zero_second_for_count_two[record + 16..record + 20].copy_from_slice(&0_u32.to_le_bytes());
        assert!(matches!(
            validate_logical_page(&zero_second_for_count_two, 0, None, 64),
            Err(M11InlineProjectionError::Malformed(
                "character reference scalar count or sentinel is invalid"
            ))
        ));

        let mut invalid_first = page.as_bytes().to_vec();
        invalid_first[record + 12..record + 16].copy_from_slice(&0xd800_u32.to_le_bytes());
        assert!(matches!(
            validate_logical_page(&invalid_first, 0, None, 64),
            Err(M11InlineProjectionError::Malformed(
                "character reference first value is not a Unicode scalar"
            ))
        ));

        let mut invalid_second = page.as_bytes().to_vec();
        invalid_second[record + 16..record + 20].copy_from_slice(&0x11_0000_u32.to_le_bytes());
        assert!(matches!(
            validate_logical_page(&invalid_second, 0, None, 64),
            Err(M11InlineProjectionError::Malformed(
                "character reference second value is not a Unicode scalar"
            ))
        ));
    }

    #[test]
    fn direct_link_values_round_trip_through_the_dual_persistent_roots() {
        let text = "[x](dest \"t\") ![i](img)";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let mut build = M11InlineProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..text.len(),
            profile(),
        )
        .expect("inline build");
        let facts = [
            M11InlineProjectionFact::new(M11InlineProjectionKind::DirectLink, 0, 0..13, 1..2)
                .expect("direct link"),
            M11InlineProjectionFact::new(M11InlineProjectionKind::DirectImage, 0, 14..23, 16..17)
                .expect("direct image"),
        ];
        let values = [
            M11InlineLinkValue::new(
                0,
                4..8,
                Some(9..12),
                "dest",
                Some("t".to_owned().into_boxed_str()),
            )
            .expect("link value"),
            M11InlineLinkValue::new(1, 19..22, None, "img", None).expect("image value"),
        ];
        build
            .offer_page_with_link_values(&facts, &values)
            .expect("paired page");
        loop {
            if build
                .poll(&mut runtime, 32)
                .expect("poll paired page")
                .status()
                == M11InlineProjectionBuildStatus::NeedsPage
            {
                break;
            }
        }
        let mut root = {
            build.finish_input().expect("finish input");
            loop {
                if build
                    .poll(&mut runtime, 32)
                    .expect("finish dual root")
                    .status()
                    == M11InlineProjectionBuildStatus::Complete
                {
                    break build.take_root().expect("dual root");
                }
            }
        };
        assert_eq!(root.descriptor().fact_count(), 2);
        assert_eq!(root.descriptor().link_value_entry_count(), 2);
        assert_eq!(root.descriptor().link_value_encoded_bytes(), 88);
        let descriptor = persistent_descriptor(&root);
        let mut validator = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root.inner.tree_root_id_for_test(),
            root.link_values.tree_root_id_for_test(),
            descriptor,
        )
        .expect("dual-root validator");
        while !validator
            .poll(runtime.producer_arena(), 1)
            .expect("validate dual roots")
            .complete
        {}
        let mut encoded =
            vec![0_u8; usize::try_from(descriptor.link_value_encoded_bytes()).expect("query size")];
        let receipt = encode_persistent_inline_link_values(
            runtime.producer_arena(),
            root.link_values.tree_root_id_for_test(),
            descriptor,
            &mut encoded,
        )
        .expect("encode FLKIV001");
        assert_eq!(receipt.entry_count, 2);
        assert!(receipt.tree_nodes_visited > 0);
        assert_eq!(&encoded[..8], b"FLKIV001");
        assert_eq!(read_u32(&encoded, 8).expect("schema"), 1);
        assert_eq!(read_u32(&encoded, 12).expect("count"), 2);
        assert_eq!(
            decode_link_value_entries(&encoded[16..], 2).expect("decode query values"),
            values
        );

        drop(validator);
        release_inline_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn reference_link_values_use_document_absolute_definition_cuts() {
        let text = "[x] ![i]\n\n[x]: destination \"title\"\n[i]: image";
        let inline_end = text.find('\n').expect("inline leaf end");
        let destination_start = u32::try_from(text.find("destination").expect("destination"))
            .expect("destination coordinate");
        let title_start =
            u32::try_from(text.find("\"title\"").expect("title")).expect("title coordinate");
        let image_start = u32::try_from(text.rfind("image").expect("image destination"))
            .expect("image coordinate");
        assert!(usize::try_from(destination_start).expect("coordinate") > inline_end);

        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let mut build = M11InlineProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..inline_end,
            profile(),
        )
        .expect("inline build");
        let facts = [
            M11InlineProjectionFact::new(M11InlineProjectionKind::ReferenceLink, 0, 0..3, 1..2)
                .expect("reference link"),
            M11InlineProjectionFact::new(M11InlineProjectionKind::ReferenceImage, 0, 4..8, 6..7)
                .expect("reference image"),
        ];
        let values = [
            M11InlineLinkValue::new(
                0,
                destination_start..destination_start + 11,
                Some(title_start..title_start + 7),
                "destination",
                Some("title".to_owned().into_boxed_str()),
            )
            .expect("reference link value"),
            M11InlineLinkValue::new(1, image_start..image_start + 5, None, "image", None)
                .expect("reference image value"),
        ];
        build
            .offer_page_with_link_values(&facts, &values)
            .expect("reference values may point beyond their inline leaf");
        loop {
            if build
                .poll(&mut runtime, 32)
                .expect("poll reference page")
                .status()
                == M11InlineProjectionBuildStatus::NeedsPage
            {
                break;
            }
        }
        let mut root = {
            build.finish_input().expect("finish input");
            loop {
                if build
                    .poll(&mut runtime, 32)
                    .expect("finish reference roots")
                    .status()
                    == M11InlineProjectionBuildStatus::Complete
                {
                    break build.take_root().expect("reference roots");
                }
            }
        };
        let descriptor = persistent_descriptor(&root);
        let mut validator = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root.inner.tree_root_id_for_test(),
            root.link_values.tree_root_id_for_test(),
            descriptor,
        )
        .expect("reference-root validator");
        while !validator
            .poll(runtime.producer_arena(), 1)
            .expect("validate reference roots")
            .complete
        {}
        let mut encoded =
            vec![0_u8; usize::try_from(descriptor.link_value_encoded_bytes()).expect("query size")];
        encode_persistent_inline_link_values(
            runtime.producer_arena(),
            root.link_values.tree_root_id_for_test(),
            descriptor,
            &mut encoded,
        )
        .expect("encode reference values");
        assert_eq!(
            decode_link_value_entries(&encoded[16..], 2).expect("decode reference values"),
            values
        );

        drop(validator);
        release_inline_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn link_value_geometry_keeps_direct_and_reference_coordinate_bases_distinct() {
        let direct =
            M11InlineProjectionFact::new(M11InlineProjectionKind::DirectLink, 0, 0..6, 1..2)
                .expect("direct link");
        let reference =
            M11InlineProjectionFact::new(M11InlineProjectionKind::ReferenceLink, 0, 0..3, 1..2)
                .expect("reference link");
        let absolute = M11InlineLinkValue::new(
            0,
            80..84,
            Some(90..97),
            "dest",
            Some("title".to_owned().into_boxed_str()),
        )
        .expect("absolute definition value");

        assert!(matches!(
            absolute.validate_against_fact(direct, 128),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "direct inline link value cuts are outside the parent closer"
            ))
        ));
        absolute
            .validate_against_fact(reference, 128)
            .expect("document-absolute reference cuts");
        assert!(matches!(
            absolute.validate_against_fact(reference, 96),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "reference inline link value cuts are outside document source or out of order"
            ))
        ));

        let reversed_title = M11InlineLinkValue::new(
            0,
            80..84,
            Some(70..77),
            "dest",
            Some("title".to_owned().into_boxed_str()),
        )
        .expect("syntactically shaped value");
        assert!(matches!(
            reversed_title.validate_against_fact(reference, 128),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "reference inline link value cuts are outside document source or out of order"
            ))
        ));
    }

    #[test]
    fn link_facts_reject_missing_or_orphan_values_atomically() {
        let text = "[x](d)";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let fact = M11InlineProjectionFact::new(M11InlineProjectionKind::DirectLink, 0, 0..6, 1..2)
            .expect("direct link");
        let mut build = M11InlineProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..text.len(),
            profile(),
        )
        .expect("inline build");
        assert!(matches!(
            build.offer_page(&[fact]),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "link/image fact has no companion value"
            ))
        ));
        let reference =
            M11InlineProjectionFact::new(M11InlineProjectionKind::ReferenceLink, 0, 0..3, 1..2)
                .expect("reference link");
        assert!(matches!(
            build.offer_page(&[reference]),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "link/image fact has no companion value"
            ))
        ));
        let oversized = M11InlineLinkValue::new(
            0,
            0..0,
            None,
            "x".repeat(M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES),
            None,
        )
        .expect("oversized cooked value shape");
        assert!(matches!(
            build.offer_page_with_link_values(&[reference], &[oversized]),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "encoded inline link values exceed the bounded query envelope"
            ))
        ));
        let orphan = M11InlineLinkValue::new(0, 4..5, None, "d", None).expect("orphan");
        let emphasis =
            M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 0..3, 1..2)
                .expect("emphasis");
        assert!(matches!(
            build.offer_page_with_link_values(&[emphasis], &[orphan]),
            Err(M11InlineProjectionError::InvalidLinkValue(
                "orphan inline link value has no link/image fact"
            ))
        ));
        build.begin_cancel(&mut runtime).expect("cancel build");
        while !build
            .poll_cancel(&mut runtime, 32)
            .expect("poll cancel")
            .complete()
        {}
        close_runtime(runtime);
    }

    #[test]
    fn logical_page_bytes_are_stable_under_uniform_suffix_shift() {
        let original =
            M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 20..25, 21..24)
                .expect("original");
        let shifted =
            M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 120..125, 121..124)
                .expect("shifted");
        let original_page = encode_logical_page(10, 5, 20, &[original]).expect("original page");
        let shifted_page = encode_logical_page(10, 5, 120, &[shifted]).expect("shifted page");
        assert_eq!(original_page.as_bytes(), shifted_page.as_bytes());
    }

    #[test]
    fn host_validator_derives_fact_count_and_ordered_commitment() {
        let text = "**x** and _y_";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let mut build = M11InlineProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..text.len(),
            profile(),
        )
        .expect("inline build");
        accept_inline_page(
            &mut build,
            &mut runtime,
            &[
                M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 0..5, 2..3)
                    .expect("strong fact"),
            ],
        );
        accept_inline_page(
            &mut build,
            &mut runtime,
            &[
                M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 10..13, 11..12)
                    .expect("emphasis fact"),
            ],
        );
        let mut root = finish_inline_build(&mut build, &mut runtime);
        let root_id = root.inner.tree_root_id_for_test();
        let descriptor = persistent_descriptor(&root);

        let mut valid = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root_id,
            root.link_values.tree_root_id_for_test(),
            descriptor,
        )
        .expect("valid imported root");
        let mut transitions = 0;
        loop {
            let poll = valid
                .poll(runtime.producer_arena(), 1)
                .expect("bounded typed validation");
            assert!(poll.transitions <= 1);
            transitions += poll.transitions;
            if poll.complete {
                break;
            }
        }
        assert_eq!(transitions, descriptor.logical_page_count as usize + 1);

        let mut wrong_count = descriptor;
        wrong_count.fact_count += 1;
        let mut wrong_count_validator = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root_id,
            root.link_values.tree_root_id_for_test(),
            wrong_count,
        )
        .expect("generic root still matches");
        assert!(matches!(
            wrong_count_validator.poll(runtime.producer_arena(), 3),
            Err(M11InlineProjectionError::Malformed(
                "persistent inline Projection descriptor counts differ from host replay"
            ))
        ));

        let mut wrong_commitment = descriptor;
        wrong_commitment.ordered_commitment256[0] ^= 1;
        let mut wrong_commitment_validator = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root_id,
            root.link_values.tree_root_id_for_test(),
            wrong_commitment,
        )
        .expect("generic root still matches");
        assert!(matches!(
            wrong_commitment_validator.poll(runtime.producer_arena(), 3),
            Err(M11InlineProjectionError::CommitmentMismatch)
        ));

        drop(valid);
        drop(wrong_count_validator);
        drop(wrong_commitment_validator);
        release_inline_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn host_validator_authenticates_deterministic_empty_commitment() {
        let text = "plain text";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let mut build = M11InlineProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..text.len(),
            profile(),
        )
        .expect("inline build");
        let mut root = finish_inline_build(&mut build, &mut runtime);
        assert_eq!(root.descriptor.logical_page_count, 0);
        assert_eq!(root.inner.tree_root_id_for_test(), None);
        let descriptor = persistent_descriptor(&root);

        let mut valid = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            None,
            None,
            descriptor,
        )
        .expect("empty imported root");
        assert_eq!(
            valid
                .poll(runtime.producer_arena(), 1)
                .expect("validate empty root"),
            PersistentM11InlineProjectionHostValidationPoll {
                transitions: 1,
                complete: true,
            }
        );

        let mut wrong = descriptor;
        wrong.ordered_commitment256[0] ^= 1;
        let mut invalid = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            None,
            None,
            wrong,
        )
        .expect("empty dimensions still match");
        assert!(matches!(
            invalid.poll(runtime.producer_arena(), 1),
            Err(M11InlineProjectionError::CommitmentMismatch)
        ));

        drop(valid);
        drop(invalid);
        release_inline_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn host_validator_rejects_self_consistent_generic_page_with_invalid_inline_kind() {
        let text = "0123456789abcdef";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let parser_profile = profile();
        let fact = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 2..7, 4..5)
            .expect("fact");
        let encoded = encode_logical_page(2, 5, 2, &[fact]).expect("logical page");
        let mut malformed = encoded.as_bytes().to_vec();
        malformed[INLINE_PROJECTION_PAGE_HEADER_BYTES] = u8::MAX;
        let mut root = generic_root_with_record(&mut runtime, &malformed);
        let root_id = root.tree_root_id_for_test();
        let source_range = 0..u32::try_from(text.len()).expect("source length");
        let mut hasher = begin_commitment(source, parser_profile, &source_range);
        append_page_to_commitment(&mut hasher, &malformed);
        let descriptor = PersistentM11InlineProjectionDescriptor {
            source,
            parser_profile,
            source_start: source_range.start,
            source_end: source_range.end,
            logical_page_count: 1,
            fact_count: 1,
            storage_page_count: root.page_count(),
            payload_bytes: root.payload_bytes(),
            encoded_bytes: root.encoded_bytes(),
            checksum: root.checksum(),
            ordered_commitment256: finish_commitment(&hasher, 1, 1),
            link_value_entry_count: 0,
            link_value_record_count: 0,
            link_value_storage_page_count: 0,
            link_value_payload_bytes: 0,
            link_value_tree_encoded_bytes: 0,
            link_value_encoded_bytes: 0,
            link_value_checksum: [0; 32],
            link_value_ordered_commitment256: [0; 32],
        };
        let mut validator = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root_id,
            None,
            descriptor,
        )
        .expect("generic root and strong descriptor are self-consistent");
        assert!(matches!(
            validator.poll(runtime.producer_arena(), 1),
            Err(M11InlineProjectionError::Malformed(
                "inline Projection fact kind is unsupported"
            ))
        ));

        drop(validator);
        release_generic_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }

    #[test]
    fn host_validator_is_fuelled_beyond_128_physical_pages() {
        const LOGICAL_PAGES: usize = 2_100;

        let text = "*x*";
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let repeated =
            M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 0..3, 1..2)
                .expect("fact");
        let page = [repeated; M11_INLINE_PROJECTION_FACTS_PER_PAGE_MAX];
        let mut build = M11InlineProjectionBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source lease"),
            0..text.len(),
            profile(),
        )
        .expect("inline build");
        for _ in 0..LOGICAL_PAGES {
            accept_inline_page(&mut build, &mut runtime, &page);
        }
        let mut root = finish_inline_build(&mut build, &mut runtime);
        assert!(root.inner.page_count() > 128);
        let descriptor = persistent_descriptor(&root);
        let mut validator = PersistentM11InlineProjectionHostValidator::new(
            runtime.producer_arena(),
            root.inner.tree_root_id_for_test(),
            root.link_values.tree_root_id_for_test(),
            descriptor,
        )
        .expect("large imported root");
        let mut transitions = 0;
        loop {
            let poll = validator
                .poll(runtime.producer_arena(), 17)
                .expect("fuelled large-root validation");
            assert!(poll.transitions <= 17);
            transitions += poll.transitions;
            if poll.complete {
                break;
            }
        }
        assert_eq!(transitions, LOGICAL_PAGES + 1);

        drop(validator);
        release_inline_root(&mut root, &mut runtime);
        drop(root);
        close_runtime(runtime);
    }
}
