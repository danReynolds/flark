//! Capture-only inline projection facts, cooked values, and validation.

use std::fmt;
use std::ops::Range;

use crate::document::DocumentRuntime;
use crate::parser_range::{M11ParserRangeError, M11ParserSourceRangeAuthority};
use crate::{ParserProfileId, SourceVersion};

const INLINE_LINK_VALUE_ENTRY_BYTES: usize = 32;

/// Selected inline semantics carried by an authoritative capture.
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
}

#[derive(Debug)]
pub enum M11InlineProjectionError {
    InvalidFact(&'static str),
    InvalidLinkValue(&'static str),
    FactsOutOfOrder,
    FactOutsideSourceRange,
    CoordinateOverflow,
    SourceAuthorityMismatch,
    ParserProfileMismatch,
    Pages(M11ParserRangeError),
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
            Self::Pages(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11InlineProjectionError {}

impl From<M11ParserRangeError> for M11InlineProjectionError {
    fn from(value: M11ParserRangeError) -> Self {
        Self::Pages(value)
    }
}

/// Allocation-free validation for capture-only inline projection.
///
/// This stamps a borrowed exact source authority while validating typed facts
/// and cooked link values without creating arena pages. Each offer is
/// failure-atomic: counters and ordering state advance only after the complete
/// fact/value batch passes.
#[must_use = "capture validators must be finalized against the returned authority"]
pub struct M11InlineProjectionCaptureValidator {
    source: SourceVersion,
    source_range: Range<u32>,
    parser_profile: ParserProfileId,
    fact_count: u64,
    last_fact_start: Option<u32>,
    link_value_entry_count: u32,
    link_value_payload_bytes: usize,
}

impl M11InlineProjectionCaptureValidator {
    pub fn new(
        runtime: &DocumentRuntime,
        authority: &M11ParserSourceRangeAuthority,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11InlineProjectionError> {
        authority.validate(runtime)?;
        let source = authority.source();
        let range = authority.source_range();
        let source_range = u32::try_from(range.start)
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
            ..u32::try_from(range.end).map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        Ok(Self {
            source,
            source_range,
            parser_profile,
            fact_count: 0,
            last_fact_start: None,
            link_value_entry_count: 0,
            link_value_payload_bytes: 0,
        })
    }

    pub fn offer(
        &mut self,
        facts: &[M11InlineProjectionFact],
        link_values: &[M11InlineLinkValue],
    ) -> Result<(), M11InlineProjectionError> {
        let source_len = self
            .source_range
            .end
            .checked_sub(self.source_range.start)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let mut previous_start = self.last_fact_start;
        let mut next_value = 0_usize;
        let mut added_value_bytes = 0_usize;

        for (local_ordinal, fact) in facts.iter().copied().enumerate() {
            validate_fact(fact)?;
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
            if fact.kind.has_link_value() {
                let value = link_values.get(next_value).ok_or(
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
                    .and_then(|ordinal| u32::try_from(ordinal).ok())
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                if value.parent_fact_ordinal != expected_ordinal {
                    return Err(M11InlineProjectionError::InvalidLinkValue(
                        "inline link values are not keyed by strict fact ordinal",
                    ));
                }
                value.validate_against_fact(fact, self.source.byte_len())?;
                added_value_bytes = added_value_bytes
                    .checked_add(value.encoded_len()?)
                    .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
                next_value += 1;
            }
            previous_start = Some(fact.relative_start);
        }
        if next_value != link_values.len() {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "orphan inline link value has no link/image fact",
            ));
        }

        let next_fact_count = self
            .fact_count
            .checked_add(
                u64::try_from(facts.len())
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let next_entry_count = self
            .link_value_entry_count
            .checked_add(
                u32::try_from(link_values.len())
                    .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let next_payload_bytes = self
            .link_value_payload_bytes
            .checked_add(added_value_bytes)
            .ok_or(M11InlineProjectionError::CoordinateOverflow)?;
        let next_encoded_bytes = if next_entry_count == 0 {
            0
        } else {
            16_usize
                .checked_add(next_payload_bytes)
                .ok_or(M11InlineProjectionError::CoordinateOverflow)?
        };
        if next_entry_count > M11_INLINE_LINK_VALUES_MAX_ENTRIES
            || next_encoded_bytes > M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES
        {
            return Err(M11InlineProjectionError::InvalidLinkValue(
                "encoded inline link values exceed the bounded query envelope",
            ));
        }

        self.fact_count = next_fact_count;
        self.last_fact_start = previous_start;
        self.link_value_entry_count = next_entry_count;
        self.link_value_payload_bytes = next_payload_bytes;
        Ok(())
    }

    pub fn finish(
        self,
        authority: M11ParserSourceRangeAuthority,
        source: SourceVersion,
        source_range: Range<u32>,
        parser_profile: ParserProfileId,
    ) -> Result<M11ParserSourceRangeAuthority, M11InlineProjectionError> {
        let authority_range = authority.source_range();
        let authority_range = u32::try_from(authority_range.start)
            .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?
            ..u32::try_from(authority_range.end)
                .map_err(|_| M11InlineProjectionError::CoordinateOverflow)?;
        if source != self.source
            || source_range != self.source_range
            || authority.source() != self.source
            || authority_range != self.source_range
        {
            return Err(M11InlineProjectionError::SourceAuthorityMismatch);
        }
        if parser_profile != self.parser_profile {
            return Err(M11InlineProjectionError::ParserProfileMismatch);
        }
        Ok(authority)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentRuntimeConfig;

    fn profile() -> ParserProfileId {
        ParserProfileId::new(1).expect("parser profile")
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    fn capture_validator(
        runtime: &DocumentRuntime,
        range: Range<usize>,
    ) -> M11InlineProjectionCaptureValidator {
        let authority = capture_authority(runtime, range);
        M11InlineProjectionCaptureValidator::new(runtime, &authority, profile())
            .expect("capture validator")
    }

    fn capture_authority(
        runtime: &DocumentRuntime,
        range: Range<usize>,
    ) -> M11ParserSourceRangeAuthority {
        M11ParserSourceRangeAuthority::new(
            runtime,
            runtime.snapshot_current_source().expect("source lease"),
            range,
        )
        .expect("exact authority")
    }

    fn direct_link_fact() -> M11InlineProjectionFact {
        M11InlineProjectionFact::new(M11InlineProjectionKind::DirectLink, 0, 0..12, 1..5)
            .expect("direct link fact")
    }

    fn direct_link_value(parent: u32) -> M11InlineLinkValue {
        M11InlineLinkValue::new(parent, 6..9, Some(9..12), "/x", Some("t".into()))
            .expect("direct link value")
    }

    #[test]
    fn capture_validator_accepts_exact_capture_without_arena_pages() {
        let runtime = DocumentRuntime::new(
            "[abc](/x t)....................",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let before = runtime.arena_metrics();
        let source = runtime.current_source_version().expect("source");
        let mut validator = capture_validator(&runtime, 0..12);
        validator
            .offer(&[direct_link_fact()], &[direct_link_value(0)])
            .expect("valid capture");
        assert_eq!(
            runtime.arena_metrics().resident_nodes,
            before.resident_nodes
        );
        let returned_authority = capture_authority(&runtime, 0..12);
        let authority = validator
            .finish(returned_authority, source, 0..12, profile())
            .expect("matching final stamp");
        authority
            .validate(&runtime)
            .expect("finished validator returns exact authority");
        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn capture_validator_rejects_malformed_order_and_extent_failure_atomically() {
        let runtime = DocumentRuntime::new("0123456789abcdef", DocumentRuntimeConfig::default())
            .expect("runtime");
        let mut validator = capture_validator(&runtime, 0..16);
        let malformed = M11InlineProjectionFact {
            kind: M11InlineProjectionKind::Strong,
            flags: 1,
            relative_start: 0,
            relative_len: 3,
            payload: M11InlineProjectionFactPayload::Marked {
                content_offset: 1,
                content_len: 1,
            },
        };
        assert!(matches!(
            validator.offer(&[malformed], &[]),
            Err(M11InlineProjectionError::InvalidFact(_))
        ));
        let later = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 8..13, 10..11)
            .expect("later");
        validator.offer(&[later], &[]).expect("first valid fact");
        let before = (validator.fact_count, validator.last_fact_start);
        let earlier =
            M11InlineProjectionFact::new(M11InlineProjectionKind::Emphasis, 0, 2..7, 4..5)
                .expect("earlier");
        assert!(matches!(
            validator.offer(&[earlier], &[]),
            Err(M11InlineProjectionError::FactsOutOfOrder)
        ));
        let outside =
            M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 14..19, 16..17)
                .expect("outside");
        assert!(matches!(
            validator.offer(&[outside], &[]),
            Err(M11InlineProjectionError::FactOutsideSourceRange)
        ));
        assert_eq!((validator.fact_count, validator.last_fact_start), before);
        drop(validator);
        close_runtime(runtime);
    }

    #[test]
    fn capture_validator_requires_one_strictly_keyed_value_per_link_fact() {
        let runtime = DocumentRuntime::new(
            "[abc](/x t)....................",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let fact = direct_link_fact();
        let mut missing = capture_validator(&runtime, 0..12);
        assert!(matches!(
            missing.offer(&[fact], &[]),
            Err(M11InlineProjectionError::InvalidLinkValue(_))
        ));
        let mut mis_keyed = capture_validator(&runtime, 0..12);
        assert!(matches!(
            mis_keyed.offer(&[fact], &[direct_link_value(1)]),
            Err(M11InlineProjectionError::InvalidLinkValue(_))
        ));
        let non_link = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 0..5, 2..3)
            .expect("strong");
        let mut orphan = capture_validator(&runtime, 0..12);
        assert!(matches!(
            orphan.offer(&[non_link], &[direct_link_value(0)]),
            Err(M11InlineProjectionError::InvalidLinkValue(_))
        ));
        drop((missing, mis_keyed, orphan));
        close_runtime(runtime);
    }

    #[test]
    fn capture_validator_checks_direct_and_reference_value_coordinate_bases() {
        let runtime = DocumentRuntime::new(
            "[abc](/x t)....................................................",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut direct = capture_validator(&runtime, 0..12);
        let invalid_direct =
            M11InlineLinkValue::new(0, 2..4, None, "/x", None).expect("shaped value");
        assert!(matches!(
            direct.offer(&[direct_link_fact()], &[invalid_direct]),
            Err(M11InlineProjectionError::InvalidLinkValue(_))
        ));

        let reference_fact =
            M11InlineProjectionFact::new(M11InlineProjectionKind::ReferenceLink, 0, 0..10, 1..4)
                .expect("reference fact");
        let mut reference = capture_validator(&runtime, 0..12);
        let valid_reference =
            M11InlineLinkValue::new(0, 40..43, Some(44..47), "/r", Some("r".into()))
                .expect("reference value");
        reference
            .offer(&[reference_fact], &[valid_reference])
            .expect("document-absolute reference cuts");
        let mut invalid_reference = capture_validator(&runtime, 0..12);
        let outside =
            M11InlineLinkValue::new(0, 60..80, None, "/r", None).expect("shaped outside value");
        assert!(matches!(
            invalid_reference.offer(&[reference_fact], &[outside]),
            Err(M11InlineProjectionError::InvalidLinkValue(_))
        ));
        drop((direct, reference, invalid_reference));
        close_runtime(runtime);
    }

    #[test]
    fn capture_validator_enforces_capacity_count_and_final_stamp() {
        let runtime = DocumentRuntime::new(
            "[abc](/x t)....................",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut capacity = capture_validator(&runtime, 0..12);
        let oversized = "x".repeat(M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES);
        let value = M11InlineLinkValue::new(0, 6..9, None, oversized, None)
            .expect("oversized shaped value");
        assert!(matches!(
            capacity.offer(&[direct_link_fact()], &[value]),
            Err(M11InlineProjectionError::InvalidLinkValue(_))
        ));
        assert_eq!(capacity.fact_count, 0);

        let mut overflow = capture_validator(&runtime, 0..12);
        overflow.fact_count = u64::MAX;
        let plain = M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, 0..5, 2..3)
            .expect("plain fact");
        assert!(matches!(
            overflow.offer(&[plain], &[]),
            Err(M11InlineProjectionError::CoordinateOverflow)
        ));

        let source = runtime.current_source_version().expect("source");
        let wrong_range = capture_validator(&runtime, 0..12);
        let authority = capture_authority(&runtime, 0..12);
        assert!(matches!(
            wrong_range.finish(authority, source, 0..11, profile()),
            Err(M11InlineProjectionError::SourceAuthorityMismatch)
        ));
        let wrong_returned_authority = capture_validator(&runtime, 0..12);
        let authority = capture_authority(&runtime, 0..11);
        assert!(matches!(
            wrong_returned_authority.finish(authority, source, 0..12, profile()),
            Err(M11InlineProjectionError::SourceAuthorityMismatch)
        ));
        let wrong_profile = capture_validator(&runtime, 0..12);
        let authority = capture_authority(&runtime, 0..12);
        assert!(matches!(
            wrong_profile.finish(
                authority,
                source,
                0..12,
                ParserProfileId::new(2).expect("other profile")
            ),
            Err(M11InlineProjectionError::ParserProfileMismatch)
        ));
        drop((capacity, overflow));
        close_runtime(runtime);
    }
}
