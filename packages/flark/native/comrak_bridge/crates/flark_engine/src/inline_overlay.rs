//! Revision-bound one-slot inline refinement sidecar.
//!
//! This module deliberately does not place inline facts back into the
//! five-role canonical candidate. A hot Paragraph refinement is a sibling
//! publication bound to one exact installed candidate and one exact block
//! fence. The envelope below is the typed payload for that future sidecar
//! root; the generic slot models atomic latest-wins replacement while the
//! previous imported root retires under fuel.
//!
//! Closure transport is intentionally not implemented here. The existing
//! snapshot producer traversal is almost generic, but host admission and
//! sealing currently require a five-role manifest. The production follow-up
//! is to factor that closure receiver, then attach this envelope plus the
//! existing `IPR3` descriptor and inline tree as a sibling root.

use std::fmt;
use std::ops::Range;

use crate::block_quote_projection::{
    decode_persistent_block_quote_projection_descriptor,
    validate_persistent_block_quote_projection_root, BlockQuoteLineV1,
    M11BlockQuoteProjectionError, M11BlockQuoteProjectionRoot, M11MarkedLineProjectionKind,
    PersistentM11BlockQuoteProjectionDescriptor, PersistentM11BlockQuoteProjectionHostCursor,
    PersistentM11BlockQuoteProjectionHostCursorPoll,
    PersistentM11BlockQuoteProjectionHostValidationPoll,
    PersistentM11BlockQuoteProjectionHostValidator,
    PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES,
};
use crate::candidate_manifest::CandidateAuthority;
use crate::host_store::{
    ArenaClosureCheckPoll, ArenaClosureSnapshotEncodePoll, ArenaClosureSnapshotEncoder,
    ArenaClosureSnapshotReceiver, CandidateHostError, CandidateHostLimits,
};
use crate::identity::RuntimeIdentity;
use crate::indented_code_projection::{
    decode_persistent_indented_code_projection_descriptor,
    validate_persistent_indented_code_projection_root, M11IndentedCodeProjectionError,
    M11IndentedCodeProjectionRoot, PersistentM11IndentedCodeProjectionDescriptor,
    PersistentM11IndentedCodeProjectionHostCursor,
    PersistentM11IndentedCodeProjectionHostValidationPoll,
    PersistentM11IndentedCodeProjectionHostValidator,
    PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES,
};
use crate::inline_projection::{
    decode_persistent_inline_projection_descriptor,
    decode_persistent_projected_inline_projection_descriptor,
    validate_persistent_inline_projection_role, M11InlineProjectionDescriptor,
    M11InlineProjectionError, M11InlineProjectionRoot, M11ProjectedInlineProjectionRoot,
    PersistentM11InlineProjectionDescriptor, PersistentM11InlineProjectionHostCursor,
    PersistentM11InlineProjectionHostValidationPoll, PersistentM11InlineProjectionHostValidator,
    PersistentM11ProjectedInlineProjectionDescriptor, M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES,
    M11_INLINE_LINK_VALUES_MAX_ENTRIES, PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES,
    PERSISTENT_PROJECTED_INLINE_PROJECTION_DESCRIPTOR_BYTES,
};
use crate::parser_pages::{
    is_m11_parser_page_node_payload, validate_imported_m11_parser_page_node,
};
use crate::storage::{CommittedArenaRoot, PageArena};
use crate::{DocumentRuntime, ParserProfileId, SourceVersion};

const INLINE_OVERLAY_MAGIC: [u8; 4] = *b"HIO1";
const INLINE_OVERLAY_SCHEMA_INLINE: u32 = 1;
const INLINE_OVERLAY_SCHEMA_TYPED: u32 = 2;
const INLINE_OVERLAY_SCHEMA_ORDERED_ITEM: u32 = 3;
const INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_INLINE: u32 = 4;
const INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_TYPED: u32 = 5;
const INLINE_OVERLAY_OWNER_ID_LIMIT: u64 = 1_u64 << 63;
const INLINE_OVERLAY_COMMITMENT_DOMAIN: &[u8] = b"flark.inline-overlay-envelope.v1\0";
const INLINE_OVERLAY_BODY_BYTES: usize = 224;
pub(crate) const M11_INLINE_OVERLAY_ENVELOPE_BYTES: usize = 256;
const INLINE_OVERLAY_BEGIN_TAG: u8 = 0xe6;
const INLINE_OVERLAY_TRANSPORT_VERSION: u8 = 1;
const INLINE_OVERLAY_BEGIN_HEADER_BYTES: usize = 16;
const INLINE_PROJECTION_BUNDLE_MAGIC: [u8; 4] = *b"ILB1";
const INLINE_PROJECTION_BUNDLE_SCHEMA: u32 = 1;
const INLINE_PROJECTION_BUNDLE_BYTES: usize = 16;
const INLINE_PROJECTION_BUNDLE_FACT_ROOT: u32 = 1;
const INLINE_PROJECTION_BUNDLE_VALUE_ROOT: u32 = 2;

fn encode_inline_projection_bundle(
    fact_root: Option<crate::ArenaId>,
    link_value_root: Option<crate::ArenaId>,
) -> Box<[u8]> {
    let mut bytes = [0_u8; INLINE_PROJECTION_BUNDLE_BYTES];
    bytes[..4].copy_from_slice(&INLINE_PROJECTION_BUNDLE_MAGIC);
    bytes[4..8].copy_from_slice(&INLINE_PROJECTION_BUNDLE_SCHEMA.to_le_bytes());
    let flags = (u32::from(fact_root.is_some()) * INLINE_PROJECTION_BUNDLE_FACT_ROOT)
        | (u32::from(link_value_root.is_some()) * INLINE_PROJECTION_BUNDLE_VALUE_ROOT);
    bytes[8..12].copy_from_slice(&flags.to_le_bytes());
    bytes.into()
}

fn decode_inline_projection_bundle(
    arena: &PageArena,
    root: Option<crate::ArenaId>,
) -> Result<(Option<crate::ArenaId>, Option<crate::ArenaId>), M11InlineOverlayTransportError> {
    let root = root.ok_or(M11InlineOverlayTransportError::InvalidProgram(
        "inline projection bundle root is absent",
    ))?;
    let payload = arena.payload(root)?;
    if payload.len() != INLINE_PROJECTION_BUNDLE_BYTES
        || payload[..4] != INLINE_PROJECTION_BUNDLE_MAGIC
        || u32::from_le_bytes(payload[4..8].try_into().expect("bundle schema")) != 1
        || payload[12..16] != [0; 4]
    {
        return Err(M11InlineOverlayTransportError::InvalidProgram(
            "inline projection bundle header is invalid",
        ));
    }
    let flags = u32::from_le_bytes(payload[8..12].try_into().expect("bundle flags"));
    if flags & !(INLINE_PROJECTION_BUNDLE_FACT_ROOT | INLINE_PROJECTION_BUNDLE_VALUE_ROOT) != 0
        || arena.child_count(root)? != flags.count_ones() as usize
    {
        return Err(M11InlineOverlayTransportError::InvalidProgram(
            "inline projection bundle children differ from its flags",
        ));
    }
    let fact_root = (flags & INLINE_PROJECTION_BUNDLE_FACT_ROOT != 0)
        .then(|| arena.child_at(root, 0))
        .transpose()?;
    let value_index = usize::from(fact_root.is_some());
    let link_value_root = (flags & INLINE_PROJECTION_BUNDLE_VALUE_ROOT != 0)
        .then(|| arena.child_at(root, value_index))
        .transpose()?;
    Ok((fact_root, link_value_root))
}

/// Exact installed-candidate authority to which one sidecar may attach.
///
/// Publication identity and parse generation are included here. The installed
/// manifest digest remains part of the top-level exact-base ACK and will be
/// checked by the transport adapter; duplicating it in every sidecar payload
/// would create a second candidate-authority schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayBase {
    candidate: CandidateAuthority,
    source: SourceVersion,
    parser_profile: ParserProfileId,
}

impl M11InlineOverlayBase {
    pub(crate) fn new(
        candidate: CandidateAuthority,
        source: SourceVersion,
        parser_profile: ParserProfileId,
    ) -> Result<Self, M11InlineOverlayError> {
        let syntax_profile =
            u32::try_from(parser_profile.get()).map_err(|_| M11InlineOverlayError::InvalidBase)?;
        if candidate.source_root != source.root()
            || candidate.source_revision != source.revision()
            || candidate.source_bytes
                != u64::try_from(source.byte_len())
                    .map_err(|_| M11InlineOverlayError::CoordinateOverflow)?
            || candidate.source_utf16
                != u64::try_from(source.utf16_len())
                    .map_err(|_| M11InlineOverlayError::CoordinateOverflow)?
            || candidate.syntax_profile != syntax_profile
        {
            return Err(M11InlineOverlayError::InvalidBase);
        }
        Ok(Self {
            candidate,
            source,
            parser_profile,
        })
    }

    pub(crate) const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }
}

/// Exact block fence and monotonic refinement attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOverlayOwner {
    BlockOrdinal(u64),
    RecursiveGreenFrame(u64),
}

impl M11InlineOverlayOwner {
    pub(crate) const fn id(self) -> u64 {
        match self {
            Self::BlockOrdinal(id) | Self::RecursiveGreenFrame(id) => id,
        }
    }
}

/// Exact structural owner fence and monotonic refinement attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayBinding {
    base: M11InlineOverlayBase,
    generation: u64,
    owner: M11InlineOverlayOwner,
    physical_range: Range<u32>,
    visible_range: Range<u32>,
    physical_range_utf16: Range<u32>,
    visible_range_utf16: Range<u32>,
}

impl M11InlineOverlayBinding {
    pub(crate) fn new(
        base: M11InlineOverlayBase,
        generation: u64,
        owner: M11InlineOverlayOwner,
        physical_range: Range<u32>,
        visible_range: Range<u32>,
        physical_range_utf16: Range<u32>,
        visible_range_utf16: Range<u32>,
    ) -> Result<Self, M11InlineOverlayError> {
        let source_bytes = u32::try_from(base.source.byte_len())
            .map_err(|_| M11InlineOverlayError::CoordinateOverflow)?;
        let source_utf16 = u32::try_from(base.source.utf16_len())
            .map_err(|_| M11InlineOverlayError::CoordinateOverflow)?;
        if generation == 0
            || owner.id() >= INLINE_OVERLAY_OWNER_ID_LIMIT
            || matches!(owner, M11InlineOverlayOwner::RecursiveGreenFrame(0))
            || physical_range.start >= physical_range.end
            || physical_range.end > source_bytes
            || visible_range.start >= visible_range.end
            || visible_range.start < physical_range.start
            || visible_range.end > physical_range.end
            || physical_range_utf16.start >= physical_range_utf16.end
            || physical_range_utf16.end > source_utf16
            || visible_range_utf16.start >= visible_range_utf16.end
            || visible_range_utf16.start < physical_range_utf16.start
            || visible_range_utf16.end > physical_range_utf16.end
        {
            return Err(M11InlineOverlayError::InvalidBinding);
        }
        Ok(Self {
            base,
            generation,
            owner,
            physical_range,
            visible_range,
            physical_range_utf16,
            visible_range_utf16,
        })
    }

    pub(crate) const fn base(&self) -> &M11InlineOverlayBase {
        &self.base
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn owner(&self) -> M11InlineOverlayOwner {
        self.owner
    }

    pub(crate) const fn physical_range(&self) -> &Range<u32> {
        &self.physical_range
    }

    pub(crate) const fn visible_range(&self) -> &Range<u32> {
        &self.visible_range
    }

    pub(crate) const fn physical_range_utf16(&self) -> &Range<u32> {
        &self.physical_range_utf16
    }

    pub(crate) const fn visible_range_utf16(&self) -> &Range<u32> {
        &self.visible_range_utf16
    }

    pub(crate) fn query(&self) -> M11InlineOverlayQuery {
        M11InlineOverlayQuery {
            base: self.base.clone(),
            owner: self.owner,
            physical_range: self.physical_range.clone(),
            visible_range: self.visible_range.clone(),
            physical_range_utf16: self.physical_range_utf16.clone(),
            visible_range_utf16: self.visible_range_utf16.clone(),
        }
    }
}

/// Query identity deliberately excludes refinement generation: the slot
/// returns whichever authenticated generation is latest for this exact fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayQuery {
    base: M11InlineOverlayBase,
    owner: M11InlineOverlayOwner,
    physical_range: Range<u32>,
    visible_range: Range<u32>,
    physical_range_utf16: Range<u32>,
    visible_range_utf16: Range<u32>,
}

/// Authenticated sidecar payload beside the existing persistent inline root.
///
/// This repeats the semantic `IFO2` summary so the future importer can compare
/// it with the independently validated `IPR3` descriptor before installation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayEnvelope {
    binding: M11InlineOverlayBinding,
    disposition: M11InlineOverlayDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOverlayDisposition {
    Authoritative {
        projection_kind: M11InlineOverlayProjectionKind,
        /// Present only when a BulletList sidecar carries one selected item
        /// rather than the complete list window.
        selected_item_ordinal: Option<u32>,
        selected_item_line_ending: Option<M11InlineOverlayCanonicalLineEnding>,
        /// Present only for one compact OrderedList item sidecar.
        ordered_item: Option<M11InlineOverlayOrderedItem>,
        logical_page_count: u64,
        fact_count: u64,
        storage_page_count: u64,
        ordered_commitment256: [u8; 32],
        link_value_entry_count: u32,
        link_value_encoded_bytes: u32,
        link_value_storage_page_count: u64,
    },
    Unsupported {
        reason: u32,
        metadata_commitment256: [u8; 32],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOverlayProjectionKind {
    Inline,
    ProjectedInline,
    IndentedCode,
    BlockQuote,
    /// A tight bullet list reuses the proven persistent line-prefix record
    /// substrate, but remains a distinct authenticated semantic kind.
    BulletList,
    /// One selected tight ordered-list item plus its literal source marker.
    OrderedList,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOverlayCanonicalLineEnding {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayOrderedItem {
    pub(crate) selected_item_ordinal: u32,
    pub(crate) selected_item_line_ending: M11InlineOverlayCanonicalLineEnding,
    pub(crate) opening_marker_start: u32,
    pub(crate) opening_marker_end: u32,
    pub(crate) marker_value: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistentM11LeafProjectionDescriptor {
    Inline(PersistentM11InlineProjectionDescriptor),
    ProjectedInline(PersistentM11ProjectedInlineProjectionDescriptor),
    IndentedCode(PersistentM11IndentedCodeProjectionDescriptor),
    BlockQuote(PersistentM11BlockQuoteProjectionDescriptor),
    BulletList(PersistentM11BlockQuoteProjectionDescriptor),
    OrderedList(PersistentM11BlockQuoteProjectionDescriptor),
}

impl PersistentM11LeafProjectionDescriptor {
    const fn logical_page_count(self) -> u64 {
        match self {
            Self::Inline(descriptor) => descriptor.logical_page_count(),
            Self::ProjectedInline(descriptor) => descriptor.inner().logical_page_count(),
            Self::IndentedCode(descriptor) => descriptor.logical_page_count(),
            Self::BlockQuote(descriptor) => descriptor.logical_page_count(),
            Self::BulletList(descriptor) => descriptor.logical_page_count(),
            Self::OrderedList(descriptor) => descriptor.logical_page_count(),
        }
    }
}

impl M11InlineOverlayEnvelope {
    pub(crate) fn from_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11InlineProjectionRoot,
    ) -> Result<Self, M11InlineOverlayError> {
        Self::from_projection_descriptor(binding, projection.descriptor())
    }

    pub(crate) fn from_projection_descriptor(
        binding: M11InlineOverlayBinding,
        projection: &M11InlineProjectionDescriptor,
    ) -> Result<Self, M11InlineOverlayError> {
        if projection.source() != binding.base.source
            || projection.parser_profile() != binding.base.parser_profile
            || projection.source_range() != &binding.visible_range
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::Inline,
                selected_item_ordinal: None,
                selected_item_line_ending: None,
                ordered_item: None,
                logical_page_count: projection.logical_page_count(),
                fact_count: projection.fact_count(),
                storage_page_count: projection.storage_page_count(),
                ordered_commitment256: projection.ordered_commitment256(),
                link_value_entry_count: projection.link_value_entry_count(),
                link_value_encoded_bytes: projection.link_value_encoded_bytes(),
                link_value_storage_page_count: projection.link_value_storage_page_count(),
            },
        })
    }

    pub(crate) fn from_projected_inline_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11ProjectedInlineProjectionRoot,
    ) -> Result<Self, M11InlineOverlayError> {
        let descriptor = projection.descriptor();
        if !matches!(binding.owner, M11InlineOverlayOwner::RecursiveGreenFrame(_))
            || descriptor.source() != binding.base.source
            || descriptor.parser_profile() != binding.base.parser_profile
            || descriptor.source_range() != &binding.physical_range
            || descriptor.source_range() != &binding.visible_range
            || projection.projected_utf8_length()
                > binding
                    .visible_range
                    .end
                    .saturating_sub(binding.visible_range.start)
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::ProjectedInline,
                selected_item_ordinal: None,
                selected_item_line_ending: None,
                ordered_item: None,
                logical_page_count: descriptor.logical_page_count(),
                fact_count: descriptor.fact_count(),
                storage_page_count: descriptor.storage_page_count(),
                ordered_commitment256: projection.ordered_commitment256(),
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        })
    }

    pub(crate) fn from_indented_code_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11IndentedCodeProjectionRoot,
    ) -> Result<Self, M11InlineOverlayError> {
        let descriptor = projection.descriptor();
        if descriptor.source() != binding.base.source
            || descriptor.parser_profile() != binding.base.parser_profile
            || descriptor.physical_block_range() != &binding.physical_range
            || descriptor.requested_window() != &binding.visible_range
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::IndentedCode,
                selected_item_ordinal: None,
                selected_item_line_ending: None,
                ordered_item: None,
                logical_page_count: descriptor.logical_page_count(),
                fact_count: descriptor.line_count(),
                storage_page_count: descriptor.storage_page_count(),
                ordered_commitment256: descriptor.ordered_commitment256(),
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        })
    }

    pub(crate) fn from_block_quote_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
    ) -> Result<Self, M11InlineOverlayError> {
        let descriptor = projection.descriptor();
        if descriptor.source() != binding.base.source
            || descriptor.parser_profile() != binding.base.parser_profile
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::BlockQuote
            || descriptor.physical_block_range() != &binding.physical_range
            || descriptor.requested_window() != &binding.visible_range
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::BlockQuote,
                selected_item_ordinal: None,
                selected_item_line_ending: None,
                ordered_item: None,
                logical_page_count: descriptor.logical_page_count(),
                fact_count: descriptor.line_count(),
                storage_page_count: descriptor.storage_page_count(),
                ordered_commitment256: descriptor.ordered_commitment256(),
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        })
    }

    /// Binds a parser-certified tight bullet list to the same persistent
    /// physical-line projection substrate used by block quotes.
    ///
    /// The HIO1 kind remains distinct, so a list payload can never satisfy a
    /// block-quote query (or vice versa) even though their canonical line
    /// records share storage machinery.
    pub(crate) fn from_bullet_list_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
    ) -> Result<Self, M11InlineOverlayError> {
        let descriptor = projection.descriptor();
        if descriptor.source() != binding.base.source
            || descriptor.parser_profile() != binding.base.parser_profile
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::BulletList
            || descriptor.physical_block_range() != &binding.physical_range
            || descriptor.requested_window() != &binding.visible_range
            || descriptor.requested_window() != descriptor.physical_block_range()
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::BulletList,
                selected_item_ordinal: None,
                selected_item_line_ending: None,
                ordered_item: None,
                logical_page_count: descriptor.logical_page_count(),
                fact_count: descriptor.line_count(),
                storage_page_count: descriptor.storage_page_count(),
                ordered_commitment256: descriptor.ordered_commitment256(),
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        })
    }

    /// Binds one parser-selected tight-list item while retaining the complete
    /// list as the structural block fence.
    ///
    /// BQP2 already authenticates a requested physical-line window inside a
    /// larger block. The HIO1 reserved authoritative word carries the exact
    /// parser-authored item ordinal, so the independent host never has to scan
    /// preceding list items or infer Markdown structure.
    pub(crate) fn from_bullet_list_item_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        selected_item_line_ending: M11InlineOverlayCanonicalLineEnding,
    ) -> Result<Self, M11InlineOverlayError> {
        let descriptor = projection.descriptor();
        if selected_item_ordinal == u32::MAX
            || descriptor.source() != binding.base.source
            || descriptor.parser_profile() != binding.base.parser_profile
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::BulletList
            || descriptor.physical_block_range() != &binding.physical_range
            || descriptor.requested_window() != &binding.visible_range
            || descriptor.requested_window() == descriptor.physical_block_range()
            || descriptor.line_count() != 1
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::BulletList,
                selected_item_ordinal: Some(selected_item_ordinal),
                selected_item_line_ending: Some(selected_item_line_ending),
                ordered_item: None,
                logical_page_count: descriptor.logical_page_count(),
                fact_count: descriptor.line_count(),
                storage_page_count: descriptor.storage_page_count(),
                ordered_commitment256: descriptor.ordered_commitment256(),
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        })
    }

    /// Binds one parser-selected tight ordered-list item and its literal
    /// opening marker while retaining the complete list as the block fence.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_ordered_list_item_projection(
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        selected_item_line_ending: M11InlineOverlayCanonicalLineEnding,
        opening_marker_start: u32,
        opening_marker_end: u32,
        marker_value: u32,
    ) -> Result<Self, M11InlineOverlayError> {
        let descriptor = projection.descriptor();
        let ordered_item = M11InlineOverlayOrderedItem {
            selected_item_ordinal,
            selected_item_line_ending,
            opening_marker_start,
            opening_marker_end,
            marker_value,
        };
        let item_physical_bytes = descriptor
            .requested_window()
            .end
            .checked_sub(descriptor.requested_window().start)
            .ok_or(M11InlineOverlayError::ProjectionMismatch)?;
        if !valid_ordered_item(ordered_item, item_physical_bytes)
            || descriptor.source() != binding.base.source
            || descriptor.parser_profile() != binding.base.parser_profile
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::OrderedList
            || descriptor.physical_block_range() != &binding.physical_range
            || descriptor.requested_window() != &binding.visible_range
            || descriptor.logical_page_count() != 1
            || descriptor.line_count() != 1
            || descriptor.storage_page_count() != 1
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::OrderedList,
                selected_item_ordinal: None,
                selected_item_line_ending: None,
                ordered_item: Some(ordered_item),
                logical_page_count: 1,
                fact_count: 1,
                storage_page_count: 1,
                ordered_commitment256: descriptor.ordered_commitment256(),
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        })
    }

    pub(crate) fn unsupported(
        binding: M11InlineOverlayBinding,
        reason: u32,
        metadata: &[u8],
    ) -> Result<Self, M11InlineOverlayError> {
        if reason == 0 {
            return Err(M11InlineOverlayError::InvalidUnsupported);
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.inline-overlay-unsupported.v1\0");
        hasher.update(&reason.to_le_bytes());
        hasher.update(
            &u64::try_from(metadata.len())
                .map_err(|_| M11InlineOverlayError::CoordinateOverflow)?
                .to_le_bytes(),
        );
        hasher.update(metadata);
        Ok(Self {
            binding,
            disposition: M11InlineOverlayDisposition::Unsupported {
                reason,
                metadata_commitment256: *hasher.finalize().as_bytes(),
            },
        })
    }

    pub(crate) const fn binding(&self) -> &M11InlineOverlayBinding {
        &self.binding
    }

    pub(crate) const fn disposition(&self) -> &M11InlineOverlayDisposition {
        &self.disposition
    }

    pub(crate) fn matches_query(&self, query: &M11InlineOverlayQuery) -> bool {
        self.binding.base == query.base
            && self.binding.owner == query.owner
            && self.binding.physical_range == query.physical_range
            && self.binding.visible_range == query.visible_range
            && self.binding.physical_range_utf16 == query.physical_range_utf16
            && self.binding.visible_range_utf16 == query.visible_range_utf16
    }

    pub(crate) fn validate_persistent_projection(
        &self,
        descriptor: PersistentM11InlineProjectionDescriptor,
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Authoritative {
            projection_kind,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        };
        if projection_kind != M11InlineOverlayProjectionKind::Inline
            || selected_item_ordinal.is_some()
            || selected_item_line_ending.is_some()
            || ordered_item.is_some()
            || descriptor.source() != self.binding.base.source
            || descriptor.parser_profile() != self.binding.base.parser_profile
            || descriptor.source_range() != self.binding.visible_range
            || descriptor.logical_page_count() != logical_page_count
            || descriptor.fact_count() != fact_count
            || descriptor.storage_page_count() != storage_page_count
            || descriptor.ordered_commitment256() != ordered_commitment256
            || descriptor.link_value_entry_count() != link_value_entry_count
            || descriptor.link_value_encoded_bytes() != link_value_encoded_bytes
            || descriptor.link_value_storage_page_count() != link_value_storage_page_count
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(())
    }

    pub(crate) fn validate_persistent_projected_inline_projection(
        &self,
        descriptor: PersistentM11ProjectedInlineProjectionDescriptor,
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Authoritative {
            projection_kind,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        };
        let inner = descriptor.inner();
        if projection_kind != M11InlineOverlayProjectionKind::ProjectedInline
            || selected_item_ordinal.is_some()
            || selected_item_line_ending.is_some()
            || ordered_item.is_some()
            || !matches!(
                self.binding.owner,
                M11InlineOverlayOwner::RecursiveGreenFrame(_)
            )
            || inner.source() != self.binding.base.source
            || inner.parser_profile() != self.binding.base.parser_profile
            || inner.source_range() != self.binding.physical_range
            || inner.source_range() != self.binding.visible_range
            || inner.logical_page_count() != logical_page_count
            || inner.fact_count() != fact_count
            || inner.storage_page_count() != storage_page_count
            || descriptor.ordered_commitment256() != ordered_commitment256
            || link_value_entry_count != 0
            || link_value_encoded_bytes != 0
            || link_value_storage_page_count != 0
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(())
    }

    pub(crate) fn validate_persistent_indented_code_projection(
        &self,
        descriptor: PersistentM11IndentedCodeProjectionDescriptor,
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Authoritative {
            projection_kind,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        };
        if projection_kind != M11InlineOverlayProjectionKind::IndentedCode
            || selected_item_ordinal.is_some()
            || selected_item_line_ending.is_some()
            || ordered_item.is_some()
            || descriptor.source() != self.binding.base.source
            || descriptor.parser_profile() != self.binding.base.parser_profile
            || descriptor.physical_block_range() != self.binding.physical_range
            || descriptor.requested_window() != self.binding.visible_range
            || descriptor.logical_page_count() != logical_page_count
            || descriptor.line_count() != fact_count
            || descriptor.storage_page_count() != storage_page_count
            || descriptor.ordered_commitment256() != ordered_commitment256
            || link_value_entry_count != 0
            || link_value_encoded_bytes != 0
            || link_value_storage_page_count != 0
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(())
    }

    pub(crate) fn validate_persistent_block_quote_projection(
        &self,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Authoritative {
            projection_kind,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        };
        if projection_kind != M11InlineOverlayProjectionKind::BlockQuote
            || selected_item_ordinal.is_some()
            || selected_item_line_ending.is_some()
            || ordered_item.is_some()
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::BlockQuote
            || descriptor.source() != self.binding.base.source
            || descriptor.parser_profile() != self.binding.base.parser_profile
            || descriptor.physical_block_range() != self.binding.physical_range
            || descriptor.requested_window() != self.binding.visible_range
            || descriptor.logical_page_count() != logical_page_count
            || descriptor.line_count() != fact_count
            || descriptor.storage_page_count() != storage_page_count
            || descriptor.ordered_commitment256() != ordered_commitment256
            || link_value_entry_count != 0
            || link_value_encoded_bytes != 0
            || link_value_storage_page_count != 0
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(())
    }

    pub(crate) fn validate_persistent_bullet_list_projection(
        &self,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Authoritative {
            projection_kind,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        };
        let compact_item = selected_item_ordinal.is_some();
        if projection_kind != M11InlineOverlayProjectionKind::BulletList
            || compact_item != selected_item_line_ending.is_some()
            || ordered_item.is_some()
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::BulletList
            || descriptor.source() != self.binding.base.source
            || descriptor.parser_profile() != self.binding.base.parser_profile
            || descriptor.physical_block_range() != self.binding.physical_range
            || descriptor.requested_window() != self.binding.visible_range
            || compact_item != (descriptor.requested_window() != descriptor.physical_block_range())
            || compact_item && descriptor.line_count() != 1
            || descriptor.logical_page_count() != logical_page_count
            || descriptor.line_count() != fact_count
            || descriptor.storage_page_count() != storage_page_count
            || descriptor.ordered_commitment256() != ordered_commitment256
            || link_value_entry_count != 0
            || link_value_encoded_bytes != 0
            || link_value_storage_page_count != 0
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(())
    }

    pub(crate) fn validate_persistent_ordered_list_projection(
        &self,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Authoritative {
            projection_kind,
            selected_item_ordinal,
            selected_item_line_ending,
            ordered_item,
            logical_page_count,
            fact_count,
            storage_page_count,
            ordered_commitment256,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        };
        let item_physical_bytes = descriptor
            .requested_window()
            .end
            .checked_sub(descriptor.requested_window().start)
            .ok_or(M11InlineOverlayError::ProjectionMismatch)?;
        if projection_kind != M11InlineOverlayProjectionKind::OrderedList
            || selected_item_ordinal.is_some()
            || selected_item_line_ending.is_some()
            || ordered_item.is_none_or(|item| !valid_ordered_item(item, item_physical_bytes))
            || descriptor.projection_kind() != M11MarkedLineProjectionKind::OrderedList
            || descriptor.source() != self.binding.base.source
            || descriptor.parser_profile() != self.binding.base.parser_profile
            || descriptor.physical_block_range() != self.binding.physical_range
            || descriptor.requested_window() != self.binding.visible_range
            || logical_page_count != 1
            || fact_count != 1
            || storage_page_count != 1
            || descriptor.logical_page_count() != 1
            || descriptor.line_count() != 1
            || descriptor.storage_page_count() != 1
            || descriptor.ordered_commitment256() != ordered_commitment256
            || link_value_entry_count != 0
            || link_value_encoded_bytes != 0
            || link_value_storage_page_count != 0
        {
            return Err(M11InlineOverlayError::ProjectionMismatch);
        }
        Ok(())
    }

    pub(crate) fn validate_unsupported_metadata(
        &self,
        metadata: &[u8],
    ) -> Result<(), M11InlineOverlayError> {
        let M11InlineOverlayDisposition::Unsupported {
            reason,
            metadata_commitment256,
        } = self.disposition
        else {
            return Err(M11InlineOverlayError::InvalidUnsupported);
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.inline-overlay-unsupported.v1\0");
        hasher.update(&reason.to_le_bytes());
        hasher.update(
            &u64::try_from(metadata.len())
                .map_err(|_| M11InlineOverlayError::CoordinateOverflow)?
                .to_le_bytes(),
        );
        hasher.update(metadata);
        if *hasher.finalize().as_bytes() != metadata_commitment256 {
            return Err(M11InlineOverlayError::InvalidUnsupported);
        }
        Ok(())
    }

    pub(crate) fn encode(&self) -> [u8; M11_INLINE_OVERLAY_ENVELOPE_BYTES] {
        let mut output = [0_u8; M11_INLINE_OVERLAY_ENVELOPE_BYTES];
        output[0..4].copy_from_slice(&INLINE_OVERLAY_MAGIC);
        let schema = if matches!(
            self.binding.owner,
            M11InlineOverlayOwner::RecursiveGreenFrame(_)
        ) {
            match self.disposition {
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind:
                        M11InlineOverlayProjectionKind::BlockQuote
                        | M11InlineOverlayProjectionKind::ProjectedInline,
                    ..
                } => INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_TYPED,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::Inline,
                    ..
                }
                | M11InlineOverlayDisposition::Unsupported { .. } => {
                    INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_INLINE
                }
                _ => unreachable!(
                    "recursive Green owner requires inline, projected-inline, or block-quote payload"
                ),
            }
        } else {
            match self.disposition {
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind:
                        M11InlineOverlayProjectionKind::IndentedCode
                        | M11InlineOverlayProjectionKind::BlockQuote
                        | M11InlineOverlayProjectionKind::BulletList,
                    selected_item_ordinal: _,
                    ..
                } => INLINE_OVERLAY_SCHEMA_TYPED,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::OrderedList,
                    ..
                } => INLINE_OVERLAY_SCHEMA_ORDERED_ITEM,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::Inline,
                    selected_item_ordinal: _,
                    ..
                }
                | M11InlineOverlayDisposition::Unsupported { .. } => INLINE_OVERLAY_SCHEMA_INLINE,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::ProjectedInline,
                    ..
                } => unreachable!("projected inline requires a recursive Green owner"),
            }
        };
        output[4..8].copy_from_slice(&schema.to_le_bytes());
        let candidate = self.binding.base.candidate;
        output[8..24].copy_from_slice(&candidate.document.0);
        output[24..40].copy_from_slice(&candidate.publication.0);
        output[40..48].copy_from_slice(&candidate.source_root.get().to_le_bytes());
        output[48..56].copy_from_slice(&candidate.source_revision.get().to_le_bytes());
        output[56..64].copy_from_slice(&candidate.parse_generation.get().to_le_bytes());
        output[64..68].copy_from_slice(&candidate.syntax_profile.to_le_bytes());
        if matches!(
            schema,
            INLINE_OVERLAY_SCHEMA_TYPED
                | INLINE_OVERLAY_SCHEMA_ORDERED_ITEM
                | INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_TYPED
        ) {
            let discriminator = match self.disposition {
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::IndentedCode,
                    selected_item_ordinal: _,
                    ..
                } => 2_u32,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::BlockQuote,
                    selected_item_ordinal: _,
                    ..
                } => 3_u32,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::ProjectedInline,
                    ..
                } => 11_u32,
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::BulletList,
                    selected_item_line_ending,
                    ..
                } => match selected_item_line_ending {
                    None => 4_u32,
                    Some(M11InlineOverlayCanonicalLineEnding::Lf) => 5_u32,
                    Some(M11InlineOverlayCanonicalLineEnding::CrLf) => 6_u32,
                    Some(M11InlineOverlayCanonicalLineEnding::Cr) => 7_u32,
                },
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind: M11InlineOverlayProjectionKind::OrderedList,
                    ordered_item: Some(item),
                    ..
                } => match item.selected_item_line_ending {
                    M11InlineOverlayCanonicalLineEnding::Lf => 8_u32,
                    M11InlineOverlayCanonicalLineEnding::CrLf => 9_u32,
                    M11InlineOverlayCanonicalLineEnding::Cr => 10_u32,
                },
                _ => unreachable!("typed HIO1 schema requires a typed projection"),
            };
            output[68..72].copy_from_slice(&discriminator.to_le_bytes());
        }
        output[72..80].copy_from_slice(&candidate.source_bytes.to_le_bytes());
        output[80..88].copy_from_slice(&candidate.source_utf16.to_le_bytes());
        output[88..96].copy_from_slice(&self.binding.base.parser_profile.get().to_le_bytes());
        output[96..104].copy_from_slice(&self.binding.generation.to_le_bytes());
        output[104..112].copy_from_slice(&self.binding.owner.id().to_le_bytes());
        output[112..116].copy_from_slice(&self.binding.physical_range.start.to_le_bytes());
        output[116..120].copy_from_slice(&self.binding.physical_range.end.to_le_bytes());
        output[120..124].copy_from_slice(&self.binding.visible_range.start.to_le_bytes());
        output[124..128].copy_from_slice(&self.binding.visible_range.end.to_le_bytes());
        output[128..132].copy_from_slice(&self.binding.physical_range_utf16.start.to_le_bytes());
        output[132..136].copy_from_slice(&self.binding.physical_range_utf16.end.to_le_bytes());
        output[136..140].copy_from_slice(&self.binding.visible_range_utf16.start.to_le_bytes());
        output[140..144].copy_from_slice(&self.binding.visible_range_utf16.end.to_le_bytes());
        match self.disposition {
            M11InlineOverlayDisposition::Authoritative {
                projection_kind: _,
                selected_item_ordinal,
                selected_item_line_ending: _,
                ordered_item,
                logical_page_count,
                fact_count,
                storage_page_count,
                ordered_commitment256,
                link_value_entry_count,
                link_value_encoded_bytes,
                link_value_storage_page_count,
            } => {
                output[144..148].copy_from_slice(&1_u32.to_le_bytes());
                output[148..152].copy_from_slice(
                    &ordered_item
                        .map(|item| item.selected_item_ordinal)
                        .or(selected_item_ordinal)
                        .map_or(0, |ordinal| {
                            ordinal
                                .checked_add(1)
                                .expect("selected item ordinal was validated")
                        })
                        .to_le_bytes(),
                );
                if let Some(item) = ordered_item {
                    debug_assert_eq!(
                        (logical_page_count, fact_count, storage_page_count),
                        (1, 1, 1)
                    );
                    output[152..156].copy_from_slice(&item.opening_marker_start.to_le_bytes());
                    output[156..160].copy_from_slice(&item.opening_marker_end.to_le_bytes());
                    output[160..164].copy_from_slice(&item.marker_value.to_le_bytes());
                } else {
                    output[152..160].copy_from_slice(&logical_page_count.to_le_bytes());
                    output[160..168].copy_from_slice(&fact_count.to_le_bytes());
                    output[168..176].copy_from_slice(&storage_page_count.to_le_bytes());
                }
                output[176..208].copy_from_slice(&ordered_commitment256);
                output[208..212].copy_from_slice(&link_value_entry_count.to_le_bytes());
                output[212..216].copy_from_slice(&link_value_encoded_bytes.to_le_bytes());
                output[216..224].copy_from_slice(&link_value_storage_page_count.to_le_bytes());
            }
            M11InlineOverlayDisposition::Unsupported {
                reason,
                metadata_commitment256,
            } => {
                output[144..148].copy_from_slice(&2_u32.to_le_bytes());
                output[148..152].copy_from_slice(&reason.to_le_bytes());
                output[176..208].copy_from_slice(&metadata_commitment256);
            }
        }
        let digest = envelope_digest(&output[..INLINE_OVERLAY_BODY_BYTES]);
        output[INLINE_OVERLAY_BODY_BYTES..].copy_from_slice(&digest);
        output
    }

    pub(crate) fn decode_exact(
        bytes: &[u8],
        expected: &M11InlineOverlayBinding,
    ) -> Result<Self, M11InlineOverlayError> {
        if bytes.len() != M11_INLINE_OVERLAY_ENVELOPE_BYTES
            || bytes[0..4] != INLINE_OVERLAY_MAGIC
            || bytes[INLINE_OVERLAY_BODY_BYTES..]
                != envelope_digest(&bytes[..INLINE_OVERLAY_BODY_BYTES])
        {
            return Err(M11InlineOverlayError::MalformedEnvelope);
        }
        let schema = read_u32(bytes, 4)?;
        let expected_green_owner = matches!(
            expected.owner,
            M11InlineOverlayOwner::RecursiveGreenFrame(_)
        );
        if matches!(
            schema,
            INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_INLINE
                | INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_TYPED
        ) != expected_green_owner
        {
            return Err(M11InlineOverlayError::BindingMismatch);
        }
        let (projection_kind, selected_item_line_ending) = match (schema, read_u32(bytes, 68)?) {
            (INLINE_OVERLAY_SCHEMA_INLINE, 0) => (M11InlineOverlayProjectionKind::Inline, None),
            (INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_INLINE, 0) => {
                (M11InlineOverlayProjectionKind::Inline, None)
            }
            (INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_TYPED, 3) => {
                (M11InlineOverlayProjectionKind::BlockQuote, None)
            }
            (INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_TYPED, 11) => {
                (M11InlineOverlayProjectionKind::ProjectedInline, None)
            }
            (INLINE_OVERLAY_SCHEMA_TYPED, 2) => {
                (M11InlineOverlayProjectionKind::IndentedCode, None)
            }
            (INLINE_OVERLAY_SCHEMA_TYPED, 3) => (M11InlineOverlayProjectionKind::BlockQuote, None),
            (INLINE_OVERLAY_SCHEMA_TYPED, 4) => (M11InlineOverlayProjectionKind::BulletList, None),
            (INLINE_OVERLAY_SCHEMA_TYPED, 5) => (
                M11InlineOverlayProjectionKind::BulletList,
                Some(M11InlineOverlayCanonicalLineEnding::Lf),
            ),
            (INLINE_OVERLAY_SCHEMA_TYPED, 6) => (
                M11InlineOverlayProjectionKind::BulletList,
                Some(M11InlineOverlayCanonicalLineEnding::CrLf),
            ),
            (INLINE_OVERLAY_SCHEMA_TYPED, 7) => (
                M11InlineOverlayProjectionKind::BulletList,
                Some(M11InlineOverlayCanonicalLineEnding::Cr),
            ),
            (INLINE_OVERLAY_SCHEMA_ORDERED_ITEM, 8) => (
                M11InlineOverlayProjectionKind::OrderedList,
                Some(M11InlineOverlayCanonicalLineEnding::Lf),
            ),
            (INLINE_OVERLAY_SCHEMA_ORDERED_ITEM, 9) => (
                M11InlineOverlayProjectionKind::OrderedList,
                Some(M11InlineOverlayCanonicalLineEnding::CrLf),
            ),
            (INLINE_OVERLAY_SCHEMA_ORDERED_ITEM, 10) => (
                M11InlineOverlayProjectionKind::OrderedList,
                Some(M11InlineOverlayCanonicalLineEnding::Cr),
            ),
            _ => return Err(M11InlineOverlayError::MalformedEnvelope),
        };
        let candidate = expected.base.candidate;
        if bytes[8..24] != candidate.document.0
            || bytes[24..40] != candidate.publication.0
            || read_u64(bytes, 40)? != candidate.source_root.get()
            || read_u64(bytes, 48)? != candidate.source_revision.get()
            || read_u64(bytes, 56)? != candidate.parse_generation.get()
            || read_u32(bytes, 64)? != candidate.syntax_profile
            || read_u64(bytes, 72)? != candidate.source_bytes
            || read_u64(bytes, 80)? != candidate.source_utf16
            || read_u64(bytes, 88)? != expected.base.parser_profile.get()
            || read_u64(bytes, 96)? != expected.generation
            || read_u64(bytes, 104)? != expected.owner.id()
            || read_u32(bytes, 112)? != expected.physical_range.start
            || read_u32(bytes, 116)? != expected.physical_range.end
            || read_u32(bytes, 120)? != expected.visible_range.start
            || read_u32(bytes, 124)? != expected.visible_range.end
            || read_u32(bytes, 128)? != expected.physical_range_utf16.start
            || read_u32(bytes, 132)? != expected.physical_range_utf16.end
            || read_u32(bytes, 136)? != expected.visible_range_utf16.start
            || read_u32(bytes, 140)? != expected.visible_range_utf16.end
        {
            return Err(M11InlineOverlayError::BindingMismatch);
        }
        let disposition = match read_u32(bytes, 144)? {
            1 => {
                let selected_item_wire = read_u32(bytes, 148)?;
                if schema == INLINE_OVERLAY_SCHEMA_ORDERED_ITEM {
                    let selected_item_ordinal = selected_item_wire
                        .checked_sub(1)
                        .ok_or(M11InlineOverlayError::MalformedEnvelope)?;
                    let selected_item_line_ending = selected_item_line_ending
                        .ok_or(M11InlineOverlayError::MalformedEnvelope)?;
                    let ordered_item = M11InlineOverlayOrderedItem {
                        selected_item_ordinal,
                        selected_item_line_ending,
                        opening_marker_start: read_u32(bytes, 152)?,
                        opening_marker_end: read_u32(bytes, 156)?,
                        marker_value: read_u32(bytes, 160)?,
                    };
                    let item_physical_bytes = expected
                        .visible_range
                        .end
                        .checked_sub(expected.visible_range.start)
                        .ok_or(M11InlineOverlayError::MalformedEnvelope)?;
                    if projection_kind != M11InlineOverlayProjectionKind::OrderedList
                        || !valid_ordered_item(ordered_item, item_physical_bytes)
                        || read_u32(bytes, 164)? != 0
                        || read_u32(bytes, 168)? != 0
                        || read_u32(bytes, 172)? != 0
                        || bytes[208..224] != [0; 16]
                    {
                        return Err(M11InlineOverlayError::MalformedEnvelope);
                    }
                    return Ok(Self {
                        binding: expected.clone(),
                        disposition: M11InlineOverlayDisposition::Authoritative {
                            projection_kind,
                            selected_item_ordinal: None,
                            selected_item_line_ending: None,
                            ordered_item: Some(ordered_item),
                            logical_page_count: 1,
                            fact_count: 1,
                            storage_page_count: 1,
                            ordered_commitment256: bytes[176..208]
                                .try_into()
                                .expect("fixed ordered-item commitment"),
                            link_value_entry_count: 0,
                            link_value_encoded_bytes: 0,
                            link_value_storage_page_count: 0,
                        },
                    });
                }
                if selected_item_wire != 0
                    && projection_kind != M11InlineOverlayProjectionKind::BulletList
                    || (selected_item_wire != 0) != selected_item_line_ending.is_some()
                {
                    return Err(M11InlineOverlayError::MalformedEnvelope);
                }
                let selected_item_ordinal = selected_item_wire.checked_sub(1);
                let logical_page_count = read_u64(bytes, 152)?;
                let fact_count = read_u64(bytes, 160)?;
                let storage_page_count = read_u64(bytes, 168)?;
                let link_value_entry_count = read_u32(bytes, 208)?;
                let link_value_encoded_bytes = read_u32(bytes, 212)?;
                let link_value_storage_page_count = read_u64(bytes, 216)?;
                if (logical_page_count == 0) != (fact_count == 0 && storage_page_count == 0)
                    || logical_page_count > 0
                        && (fact_count < logical_page_count || storage_page_count == 0)
                    || !matches!(
                        projection_kind,
                        M11InlineOverlayProjectionKind::Inline
                            | M11InlineOverlayProjectionKind::ProjectedInline
                    ) && (link_value_entry_count != 0
                        || link_value_encoded_bytes != 0
                        || link_value_storage_page_count != 0)
                    || matches!(
                        projection_kind,
                        M11InlineOverlayProjectionKind::Inline
                            | M11InlineOverlayProjectionKind::ProjectedInline
                    ) && !valid_inline_link_value_summary(
                        link_value_entry_count,
                        link_value_encoded_bytes,
                        link_value_storage_page_count,
                        fact_count,
                    )
                {
                    return Err(M11InlineOverlayError::MalformedEnvelope);
                }
                M11InlineOverlayDisposition::Authoritative {
                    projection_kind,
                    selected_item_ordinal,
                    selected_item_line_ending,
                    ordered_item: None,
                    logical_page_count,
                    fact_count,
                    storage_page_count,
                    ordered_commitment256: bytes[176..208]
                        .try_into()
                        .expect("fixed overlay commitment"),
                    link_value_entry_count,
                    link_value_encoded_bytes,
                    link_value_storage_page_count,
                }
            }
            2 => {
                if !matches!(
                    schema,
                    INLINE_OVERLAY_SCHEMA_INLINE | INLINE_OVERLAY_SCHEMA_RECURSIVE_GREEN_INLINE
                ) {
                    return Err(M11InlineOverlayError::MalformedEnvelope);
                }
                let reason = read_u32(bytes, 148)?;
                if reason == 0
                    || read_u64(bytes, 152)? != 0
                    || read_u64(bytes, 160)? != 0
                    || read_u64(bytes, 168)? != 0
                    || bytes[208..224] != [0; 16]
                {
                    return Err(M11InlineOverlayError::MalformedEnvelope);
                }
                M11InlineOverlayDisposition::Unsupported {
                    reason,
                    metadata_commitment256: bytes[176..208]
                        .try_into()
                        .expect("fixed unsupported commitment"),
                }
            }
            _ => return Err(M11InlineOverlayError::MalformedEnvelope),
        };
        Ok(Self {
            binding: expected.clone(),
            disposition,
        })
    }
}

const fn valid_ordered_item(item: M11InlineOverlayOrderedItem, item_physical_bytes: u32) -> bool {
    let marker_bytes = item
        .opening_marker_end
        .saturating_sub(item.opening_marker_start);
    item.selected_item_ordinal != u32::MAX
        && item.opening_marker_start < item.opening_marker_end
        && (marker_bytes >= 2 && marker_bytes <= 10)
        && item.opening_marker_end <= item_physical_bytes
        && item.marker_value <= 999_999_999
}

fn valid_inline_link_value_summary(
    entry_count: u32,
    encoded_bytes: u32,
    storage_page_count: u64,
    fact_count: u64,
) -> bool {
    if entry_count == 0 {
        return encoded_bytes == 0 && storage_page_count == 0;
    }
    let minimum_encoded_bytes = entry_count
        .checked_mul(32)
        .and_then(|entries| entries.checked_add(16));
    entry_count <= M11_INLINE_LINK_VALUES_MAX_ENTRIES
        && u64::from(entry_count) <= fact_count
        && storage_page_count != 0
        && minimum_encoded_bytes.is_some_and(|minimum| encoded_bytes >= minimum)
        && usize::try_from(encoded_bytes)
            .is_ok_and(|encoded| encoded <= M11_INLINE_LINK_VALUES_MAX_ENCODED_BYTES)
}

fn validate_ordered_item_line(
    envelope: &M11InlineOverlayEnvelope,
    line: BlockQuoteLineV1,
) -> Result<(), M11InlineOverlayError> {
    let M11InlineOverlayDisposition::Authoritative {
        projection_kind: M11InlineOverlayProjectionKind::OrderedList,
        selected_item_ordinal: None,
        selected_item_line_ending: None,
        ordered_item: Some(item),
        ..
    } = envelope.disposition
    else {
        return Err(M11InlineOverlayError::ProjectionMismatch);
    };
    if line.projection_kind() != M11MarkedLineProjectionKind::OrderedList
        || item.opening_marker_end > line.hidden_prefix_length()
    {
        return Err(M11InlineOverlayError::ProjectionMismatch);
    }
    Ok(())
}

fn envelope_digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(INLINE_OVERLAY_COMMITMENT_DOMAIN);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, M11InlineOverlayError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(M11InlineOverlayError::MalformedEnvelope)?
            .try_into()
            .map_err(|_| M11InlineOverlayError::MalformedEnvelope)?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, M11InlineOverlayError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(M11InlineOverlayError::MalformedEnvelope)?
            .try_into()
            .map_err(|_| M11InlineOverlayError::MalformedEnvelope)?,
    ))
}

fn encode_overlay_begin(
    envelope: &M11InlineOverlayEnvelope,
    descriptor: Option<&[u8]>,
) -> Box<[u8]> {
    let descriptor_len = descriptor.map_or(0, |bytes| bytes.len());
    let mut output = Vec::with_capacity(
        INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES + descriptor_len,
    );
    output.extend_from_slice(&[
        INLINE_OVERLAY_BEGIN_TAG,
        INLINE_OVERLAY_TRANSPORT_VERSION,
        0,
        0,
    ]);
    output.extend_from_slice(
        &u32::try_from(M11_INLINE_OVERLAY_ENVELOPE_BYTES)
            .expect("HIO1 envelope fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(
        &u32::try_from(descriptor_len)
            .expect("IPR3 descriptor fits u32")
            .to_le_bytes(),
    );
    output.extend_from_slice(&0_u32.to_le_bytes());
    output.extend_from_slice(&envelope.encode());
    if let Some(descriptor) = descriptor {
        output.extend_from_slice(descriptor);
    }
    output.into_boxed_slice()
}

struct DecodedOverlayBegin {
    envelope: M11InlineOverlayEnvelope,
    descriptor: Option<PersistentM11LeafProjectionDescriptor>,
    descriptor_bytes: Option<Box<[u8]>>,
}

fn decode_overlay_begin(
    frame: &[u8],
    expected: &M11InlineOverlayBinding,
) -> Result<DecodedOverlayBegin, M11InlineOverlayTransportError> {
    if frame.len() < INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES
        || frame[..4]
            != [
                INLINE_OVERLAY_BEGIN_TAG,
                INLINE_OVERLAY_TRANSPORT_VERSION,
                0,
                0,
            ]
        || read_u32(frame, 4)? as usize != M11_INLINE_OVERLAY_ENVELOPE_BYTES
        || read_u32(frame, 12)? != 0
    {
        return Err(M11InlineOverlayTransportError::InvalidProgram(
            "invalid hot-inline Begin envelope",
        ));
    }
    let descriptor_len = usize::try_from(read_u32(frame, 8)?)
        .map_err(|_| M11InlineOverlayTransportError::InvalidProgram("descriptor overflow"))?;
    if INLINE_OVERLAY_BEGIN_HEADER_BYTES
        .checked_add(M11_INLINE_OVERLAY_ENVELOPE_BYTES)
        .and_then(|length| length.checked_add(descriptor_len))
        != Some(frame.len())
    {
        return Err(M11InlineOverlayTransportError::InvalidProgram(
            "hot-inline Begin length changed",
        ));
    }
    let envelope_start = INLINE_OVERLAY_BEGIN_HEADER_BYTES;
    let envelope_end = envelope_start + M11_INLINE_OVERLAY_ENVELOPE_BYTES;
    let envelope =
        M11InlineOverlayEnvelope::decode_exact(&frame[envelope_start..envelope_end], expected)?;
    let (descriptor, descriptor_bytes) = match envelope.disposition() {
        M11InlineOverlayDisposition::Authoritative {
            projection_kind, ..
        } => {
            let expected_descriptor_len = match projection_kind {
                M11InlineOverlayProjectionKind::Inline => {
                    PERSISTENT_INLINE_PROJECTION_ROLE_DESCRIPTOR_BYTES
                }
                M11InlineOverlayProjectionKind::ProjectedInline => {
                    PERSISTENT_PROJECTED_INLINE_PROJECTION_DESCRIPTOR_BYTES
                }
                M11InlineOverlayProjectionKind::IndentedCode => {
                    PERSISTENT_INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES
                }
                M11InlineOverlayProjectionKind::BlockQuote => {
                    PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
                }
                M11InlineOverlayProjectionKind::BulletList => {
                    PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
                }
                M11InlineOverlayProjectionKind::OrderedList => {
                    PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
                }
            };
            if descriptor_len != expected_descriptor_len {
                return Err(M11InlineOverlayTransportError::InvalidProgram(
                    "authoritative leaf-projection Begin lost its descriptor",
                ));
            }
            let descriptor_bytes = frame[envelope_end..].to_vec().into_boxed_slice();
            let descriptor = match projection_kind {
                M11InlineOverlayProjectionKind::Inline => {
                    let descriptor = decode_persistent_inline_projection_descriptor(
                        &descriptor_bytes,
                        expected.base.source,
                        expected.base.parser_profile,
                    )?;
                    envelope.validate_persistent_projection(descriptor)?;
                    PersistentM11LeafProjectionDescriptor::Inline(descriptor)
                }
                M11InlineOverlayProjectionKind::ProjectedInline => {
                    let descriptor = decode_persistent_projected_inline_projection_descriptor(
                        &descriptor_bytes,
                        expected.base.source,
                        expected.base.parser_profile,
                    )?;
                    envelope.validate_persistent_projected_inline_projection(descriptor)?;
                    PersistentM11LeafProjectionDescriptor::ProjectedInline(descriptor)
                }
                M11InlineOverlayProjectionKind::IndentedCode => {
                    let descriptor = decode_persistent_indented_code_projection_descriptor(
                        &descriptor_bytes,
                        expected.base.source,
                        expected.base.parser_profile,
                    )?;
                    envelope.validate_persistent_indented_code_projection(descriptor)?;
                    PersistentM11LeafProjectionDescriptor::IndentedCode(descriptor)
                }
                M11InlineOverlayProjectionKind::BlockQuote => {
                    let descriptor = decode_persistent_block_quote_projection_descriptor(
                        &descriptor_bytes,
                        expected.base.source,
                        expected.base.parser_profile,
                    )?;
                    envelope.validate_persistent_block_quote_projection(descriptor)?;
                    PersistentM11LeafProjectionDescriptor::BlockQuote(descriptor)
                }
                M11InlineOverlayProjectionKind::BulletList => {
                    let descriptor = decode_persistent_block_quote_projection_descriptor(
                        &descriptor_bytes,
                        expected.base.source,
                        expected.base.parser_profile,
                    )?;
                    envelope.validate_persistent_bullet_list_projection(descriptor)?;
                    PersistentM11LeafProjectionDescriptor::BulletList(descriptor)
                }
                M11InlineOverlayProjectionKind::OrderedList => {
                    let descriptor = decode_persistent_block_quote_projection_descriptor(
                        &descriptor_bytes,
                        expected.base.source,
                        expected.base.parser_profile,
                    )?;
                    envelope.validate_persistent_ordered_list_projection(descriptor)?;
                    PersistentM11LeafProjectionDescriptor::OrderedList(descriptor)
                }
            };
            (Some(descriptor), Some(descriptor_bytes))
        }
        M11InlineOverlayDisposition::Unsupported { .. } => {
            if descriptor_len != 0 {
                return Err(M11InlineOverlayTransportError::InvalidProgram(
                    "unsupported hot-inline Begin carries IPR3",
                ));
            }
            (None, None)
        }
    };
    Ok(DecodedOverlayBegin {
        envelope,
        descriptor,
        descriptor_bytes,
    })
}

pub(crate) enum M11InlineOverlaySnapshotEncodePoll {
    Pending {
        transitions: usize,
    },
    Frame {
        transitions: usize,
        bytes: Box<[u8]>,
    },
    Complete {
        transitions: usize,
        bytes: Box<[u8]>,
    },
}

/// Typed producer adapter over the shared arena-closure Node/End program.
///
/// Authoritative publications traverse the exact existing parser-page root.
/// Unsupported publications emit their already-encoded bounded metadata as
/// one literal leaf through the same Node frame, so the independent host owns
/// and authenticates the raw terminal record rather than only its digest.
pub(crate) struct M11InlineOverlaySnapshotEncoder {
    runtime_identity: RuntimeIdentity,
    source: SourceVersion,
    closure: ArenaClosureSnapshotEncoder,
    begin: Box<[u8]>,
}

impl M11InlineOverlaySnapshotEncoder {
    pub(crate) fn authoritative(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11InlineProjectionRoot,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope = M11InlineOverlayEnvelope::from_projection(binding.clone(), projection)?;
        let (fact_root, link_value_root, descriptor) = projection.transport_bundle_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        let persistent = decode_persistent_inline_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_projection(persistent)?;
        let arena = runtime.producer_arena();
        let roots: Vec<_> = [fact_root, link_value_root].into_iter().flatten().collect();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new_bundle(
                arena,
                &roots,
                encode_inline_projection_bundle(fact_root, link_value_root),
            )?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    pub(crate) fn authoritative_projected_inline(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11ProjectedInlineProjectionRoot,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope = M11InlineOverlayEnvelope::from_projected_inline_projection(
            binding.clone(),
            projection,
        )?;
        let (fact_root, link_value_root, descriptor) = projection.transport_bundle_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        if link_value_root.is_some() {
            return Err(M11InlineOverlayTransportError::InvalidProgram(
                "projected-inline producer carried forbidden link values",
            ));
        }
        let persistent = decode_persistent_projected_inline_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_projected_inline_projection(persistent)?;
        let arena = runtime.producer_arena();
        let roots: Vec<_> = [fact_root, link_value_root].into_iter().flatten().collect();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new_bundle(
                arena,
                &roots,
                encode_inline_projection_bundle(fact_root, link_value_root),
            )?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    pub(crate) fn authoritative_indented_code(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11IndentedCodeProjectionRoot,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope =
            M11InlineOverlayEnvelope::from_indented_code_projection(binding.clone(), projection)?;
        let (root, descriptor) = projection.transport_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        let persistent = decode_persistent_indented_code_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_indented_code_projection(persistent)?;
        let arena = runtime.producer_arena();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new(arena, root, &[])?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    pub(crate) fn authoritative_block_quote(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope =
            M11InlineOverlayEnvelope::from_block_quote_projection(binding.clone(), projection)?;
        let (root, descriptor) = projection.transport_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        let persistent = decode_persistent_block_quote_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_block_quote_projection(persistent)?;
        let arena = runtime.producer_arena();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new(arena, root, &[])?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    pub(crate) fn authoritative_bullet_list(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope =
            M11InlineOverlayEnvelope::from_bullet_list_projection(binding.clone(), projection)?;
        let (root, descriptor) = projection.transport_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        let persistent = decode_persistent_block_quote_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_bullet_list_projection(persistent)?;
        let arena = runtime.producer_arena();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new(arena, root, &[])?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    pub(crate) fn authoritative_bullet_list_item(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        selected_item_line_ending: M11InlineOverlayCanonicalLineEnding,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope = M11InlineOverlayEnvelope::from_bullet_list_item_projection(
            binding.clone(),
            projection,
            selected_item_ordinal,
            selected_item_line_ending,
        )?;
        let (root, descriptor) = projection.transport_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        let persistent = decode_persistent_block_quote_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_bullet_list_projection(persistent)?;
        let arena = runtime.producer_arena();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new(arena, root, &[])?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn authoritative_ordered_list_item(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        projection: &M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        selected_item_line_ending: M11InlineOverlayCanonicalLineEnding,
        opening_marker_start: u32,
        opening_marker_end: u32,
        marker_value: u32,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        let envelope = M11InlineOverlayEnvelope::from_ordered_list_item_projection(
            binding.clone(),
            projection,
            selected_item_ordinal,
            selected_item_line_ending,
            opening_marker_start,
            opening_marker_end,
            marker_value,
        )?;
        let (root, descriptor) = projection.transport_parts(
            runtime,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        let persistent = decode_persistent_block_quote_projection_descriptor(
            &descriptor,
            binding.base.source,
            binding.base.parser_profile,
        )?;
        envelope.validate_persistent_ordered_list_projection(persistent)?;
        let arena = runtime.producer_arena();
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new(arena, root, &[])?,
            begin: encode_overlay_begin(&envelope, Some(&descriptor)),
        })
    }

    pub(crate) fn unsupported(
        runtime: &DocumentRuntime,
        binding: M11InlineOverlayBinding,
        reason: u32,
        metadata: Box<[u8]>,
    ) -> Result<Self, M11InlineOverlayTransportError> {
        if runtime.current_source_version() != Some(binding.base.source) {
            return Err(M11InlineOverlayTransportError::Overlay(
                M11InlineOverlayError::InvalidBase,
            ));
        }
        let envelope = M11InlineOverlayEnvelope::unsupported(binding, reason, &metadata)?;
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            source: envelope.binding.base.source,
            closure: ArenaClosureSnapshotEncoder::new_literal(metadata)?,
            begin: encode_overlay_begin(&envelope, None),
        })
    }

    pub(crate) fn begin_frame(&mut self) -> Result<Box<[u8]>, M11InlineOverlayTransportError> {
        self.closure.begin(&self.begin)?;
        Ok(self.begin.clone())
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineOverlaySnapshotEncodePoll, M11InlineOverlayTransportError> {
        if runtime.producer_identity() != self.runtime_identity
            || runtime.current_source_version() != Some(self.source)
        {
            return Err(M11InlineOverlayTransportError::Overlay(
                M11InlineOverlayError::InvalidBase,
            ));
        }
        let poll = self.closure.poll(runtime.producer_arena(), fuel)?;
        Ok(match poll {
            ArenaClosureSnapshotEncodePoll::Pending { transitions } => {
                M11InlineOverlaySnapshotEncodePoll::Pending { transitions }
            }
            ArenaClosureSnapshotEncodePoll::Frame { transitions, bytes } => {
                M11InlineOverlaySnapshotEncodePoll::Frame { transitions, bytes }
            }
            ArenaClosureSnapshotEncodePoll::Complete { transitions, bytes } => {
                M11InlineOverlaySnapshotEncodePoll::Complete { transitions, bytes }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayInstallPoll {
    pub(crate) transitions: usize,
    pub(crate) installed: bool,
}

enum ActiveOverlayPhase {
    Receiving,
    Checking,
    Validating(PersistentM11LeafProjectionHostValidator),
    Sealing,
}

enum PersistentM11LeafProjectionHostValidator {
    Inline(Box<PersistentM11InlineProjectionHostValidator>),
    ProjectedInline(Box<PersistentM11InlineProjectionHostValidator>),
    IndentedCode(Box<PersistentM11IndentedCodeProjectionHostValidator>),
    BlockQuote(Box<PersistentM11BlockQuoteProjectionHostValidator>),
    BulletList(Box<PersistentM11BlockQuoteProjectionHostValidator>),
    OrderedList(Box<PersistentM11BlockQuoteProjectionHostValidator>),
}

struct ActiveOverlayOffer {
    arena: PageArena,
    receiver: ArenaClosureSnapshotReceiver,
    envelope: M11InlineOverlayEnvelope,
    descriptor: Option<PersistentM11LeafProjectionDescriptor>,
    descriptor_bytes: Option<Box<[u8]>>,
    phase: ActiveOverlayPhase,
}

struct ImportedInlineOverlay {
    arena: PageArena,
    root: Option<CommittedArenaRoot>,
    descriptor: Option<PersistentM11LeafProjectionDescriptor>,
    retiring: bool,
}

impl ImportedInlineOverlay {
    fn cursor(
        &self,
    ) -> Result<Option<PersistentM11LeafProjectionHostCursor<'_>>, M11InlineOverlayTransportError>
    {
        if self.retiring {
            return Err(M11InlineOverlayTransportError::InvalidProgram(
                "retiring hot-inline root is not queryable",
            ));
        }
        let root = self.root.as_ref().map(CommittedArenaRoot::id);
        Ok(self.descriptor.map(|descriptor| match descriptor {
            PersistentM11LeafProjectionDescriptor::Inline(descriptor) => {
                let (fact_root, _) = decode_inline_projection_bundle(&self.arena, root)
                    .expect("installed inline bundle was validated before sealing");
                PersistentM11LeafProjectionHostCursor::Inline(
                    PersistentM11InlineProjectionHostCursor::new(
                        &self.arena,
                        fact_root,
                        descriptor,
                    ),
                )
            }
            PersistentM11LeafProjectionDescriptor::ProjectedInline(descriptor) => {
                let (fact_root, _) = decode_inline_projection_bundle(&self.arena, root)
                    .expect("installed projected-inline bundle was validated before sealing");
                PersistentM11LeafProjectionHostCursor::ProjectedInline(
                    PersistentM11InlineProjectionHostCursor::new_projected(
                        &self.arena,
                        fact_root,
                        descriptor.inner(),
                        descriptor.projected_utf8_length(),
                    ),
                )
            }
            PersistentM11LeafProjectionDescriptor::IndentedCode(descriptor) => {
                PersistentM11LeafProjectionHostCursor::IndentedCode(
                    PersistentM11IndentedCodeProjectionHostCursor::new(
                        &self.arena,
                        root,
                        descriptor,
                    ),
                )
            }
            PersistentM11LeafProjectionDescriptor::BlockQuote(descriptor) => {
                PersistentM11LeafProjectionHostCursor::BlockQuote(
                    PersistentM11BlockQuoteProjectionHostCursor::new(&self.arena, root, descriptor),
                )
            }
            PersistentM11LeafProjectionDescriptor::BulletList(descriptor) => {
                PersistentM11LeafProjectionHostCursor::BulletList(
                    PersistentM11BlockQuoteProjectionHostCursor::new(&self.arena, root, descriptor),
                )
            }
            PersistentM11LeafProjectionDescriptor::OrderedList(descriptor) => {
                PersistentM11LeafProjectionHostCursor::OrderedList(
                    PersistentM11BlockQuoteProjectionHostCursor::new(&self.arena, root, descriptor),
                )
            }
        }))
    }

    fn unsupported_metadata(&self) -> Result<Option<&[u8]>, M11InlineOverlayTransportError> {
        if self.retiring {
            return Err(M11InlineOverlayTransportError::InvalidProgram(
                "retiring hot-inline root is not queryable",
            ));
        }
        match (&self.descriptor, &self.root) {
            (None, Some(root)) => Ok(Some(self.arena.payload(root.id())?)),
            (Some(_), _) | (None, None) => Ok(None),
        }
    }
}

// Both variants stay inline so a successful live-render query remains
// allocation-free; their bounded fixed records deliberately differ in size.
#[allow(clippy::large_enum_variant)]
enum PersistentM11LeafProjectionHostCursor<'arena> {
    Inline(PersistentM11InlineProjectionHostCursor<'arena>),
    ProjectedInline(PersistentM11InlineProjectionHostCursor<'arena>),
    IndentedCode(PersistentM11IndentedCodeProjectionHostCursor<'arena>),
    BlockQuote(PersistentM11BlockQuoteProjectionHostCursor<'arena>),
    BulletList(PersistentM11BlockQuoteProjectionHostCursor<'arena>),
    OrderedList(PersistentM11BlockQuoteProjectionHostCursor<'arena>),
}

impl M11InlineOverlayRetirable for ImportedInlineOverlay {
    type Error = CandidateHostError;

    fn begin_retire(&mut self) -> Result<(), Self::Error> {
        if self.retiring {
            return Ok(());
        }
        if let Some(root) = self.root.take() {
            self.arena
                .release_committed_root(root)
                .map_err(|failure| CandidateHostError::Arena(failure.error))?;
        }
        self.retiring = true;
        Ok(())
    }

    fn poll_retire(&mut self, fuel: usize) -> Result<M11InlineOverlayRetirePoll, Self::Error> {
        if !self.retiring {
            return Err(CandidateHostError::Busy);
        }
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel);
        }
        let receipt = self.arena.poll_reclaim(fuel);
        let metrics = self.arena.metrics();
        Ok(M11InlineOverlayRetirePoll {
            transitions: receipt.transitions,
            complete: receipt.complete && metrics.resident_nodes == 0 && metrics.live_builds == 0,
        })
    }
}

impl Drop for ImportedInlineOverlay {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            let metrics = self.arena.metrics();
            debug_assert!(
                self.root.is_none()
                    && self.retiring
                    && metrics.resident_nodes == 0
                    && metrics.live_builds == 0,
                "imported hot-inline roots require explicit fuelled retirement"
            );
        }
    }
}

// The authoritative match intentionally carries its cursor inline. Queries are
// on the live-render path, and boxing the cursor would add one allocation to
// every query merely to shrink the uncommon unsupported variant.
#[allow(clippy::large_enum_variant)]
pub(crate) enum M11InlineOverlayHostMatch<'host> {
    InlineAuthoritative {
        #[cfg(test)]
        envelope: &'host M11InlineOverlayEnvelope,
        descriptor: PersistentM11InlineProjectionDescriptor,
        cursor: PersistentM11InlineProjectionHostCursor<'host>,
        link_value_arena: &'host PageArena,
        link_value_root: Option<crate::ArenaId>,
    },
    ProjectedInlineAuthoritative {
        descriptor: PersistentM11ProjectedInlineProjectionDescriptor,
        cursor: PersistentM11InlineProjectionHostCursor<'host>,
    },
    IndentedCodeAuthoritative {
        descriptor: PersistentM11IndentedCodeProjectionDescriptor,
        cursor: PersistentM11IndentedCodeProjectionHostCursor<'host>,
    },
    BlockQuoteAuthoritative {
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
        cursor: PersistentM11BlockQuoteProjectionHostCursor<'host>,
    },
    BulletListAuthoritative {
        envelope: &'host M11InlineOverlayEnvelope,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
        cursor: PersistentM11BlockQuoteProjectionHostCursor<'host>,
    },
    OrderedListAuthoritative {
        envelope: &'host M11InlineOverlayEnvelope,
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
        cursor: PersistentM11BlockQuoteProjectionHostCursor<'host>,
    },
    Unsupported {
        #[cfg(test)]
        envelope: &'host M11InlineOverlayEnvelope,
        metadata: &'host [u8],
    },
}

/// Independent one-slot host for sibling hot-inline publications.
pub(crate) struct M11InlineOverlayHostStore {
    limits: CandidateHostLimits,
    slot: M11InlineOverlaySlot<ImportedInlineOverlay>,
    active: Option<ActiveOverlayOffer>,
    discarding: Option<PageArena>,
    closing: bool,
}

impl M11InlineOverlayHostStore {
    pub(crate) fn new(base: M11InlineOverlayBase, limits: CandidateHostLimits) -> Self {
        Self {
            limits,
            slot: M11InlineOverlaySlot::new(base),
            active: None,
            discarding: None,
            closing: false,
        }
    }

    pub(crate) fn begin_snapshot(
        &mut self,
        expected: M11InlineOverlayBinding,
        frame: &[u8],
    ) -> Result<(), M11InlineOverlayTransportError> {
        if self.closing || self.active.is_some() || self.discarding.is_some() {
            return Err(M11InlineOverlayTransportError::Host(
                CandidateHostError::Busy,
            ));
        }
        if expected.base != self.slot.base {
            return Err(M11InlineOverlayTransportError::SlotBaseMismatch);
        }
        if self.slot.retiring.is_some() {
            return Err(M11InlineOverlayTransportError::Install(
                "retirement pending",
            ));
        }
        if self
            .slot
            .active
            .as_ref()
            .is_some_and(|active| active.envelope.binding.generation >= expected.generation)
        {
            return Err(M11InlineOverlayTransportError::StaleGeneration);
        }
        let decoded = decode_overlay_begin(frame, &expected)?;
        let arena = PageArena::new(self.limits.arena)?;
        let receiver = ArenaClosureSnapshotReceiver::new(frame)?;
        self.active = Some(ActiveOverlayOffer {
            arena,
            receiver,
            envelope: decoded.envelope,
            descriptor: decoded.descriptor,
            descriptor_bytes: decoded.descriptor_bytes,
            phase: ActiveOverlayPhase::Receiving,
        });
        Ok(())
    }

    pub(crate) fn offer_node(
        &mut self,
        frame: &[u8],
    ) -> Result<(), M11InlineOverlayTransportError> {
        let mut active = self
            .active
            .take()
            .ok_or(M11InlineOverlayTransportError::NoOffer)?;
        let authoritative = active.descriptor.is_some();
        let inline_authoritative = matches!(
            active.descriptor,
            Some(
                PersistentM11LeafProjectionDescriptor::Inline(_)
                    | PersistentM11LeafProjectionDescriptor::ProjectedInline(_)
            )
        );
        let envelope = active.envelope.clone();
        let result = active.receiver.offer_node(
            &mut active.arena,
            self.limits,
            frame,
            move |arena, root, payload| {
                if authoritative {
                    if inline_authoritative
                        && payload.get(..4) == Some(&INLINE_PROJECTION_BUNDLE_MAGIC)
                    {
                        decode_inline_projection_bundle(arena, Some(root))
                            .map(|_| ())
                            .map_err(|_| {
                                CandidateHostError::InvalidFrame(
                                    "hot-inline dual-root bundle is invalid",
                                )
                            })
                    } else if !is_m11_parser_page_node_payload(payload) {
                        Err(CandidateHostError::InvalidFrame(
                            "hot-inline closure contains a non-IPR3 node",
                        ))
                    } else {
                        validate_imported_m11_parser_page_node(arena, root).map_err(|_| {
                            CandidateHostError::InvalidFrame(
                                "hot-inline parser-page node is invalid",
                            )
                        })
                    }
                } else {
                    if arena.child_count(root)? != 0 {
                        return Err(CandidateHostError::InvalidFrame(
                            "unsupported hot-inline metadata is not one leaf",
                        ));
                    }
                    envelope
                        .validate_unsupported_metadata(payload)
                        .map_err(|_| {
                            CandidateHostError::InvalidFrame(
                                "unsupported hot-inline metadata commitment changed",
                            )
                        })
                }
            },
        );
        self.finish_active_operation(active, result)
    }

    pub(crate) fn finish_snapshot(
        &mut self,
        frame: &[u8],
    ) -> Result<(), M11InlineOverlayTransportError> {
        let mut active = self
            .active
            .take()
            .ok_or(M11InlineOverlayTransportError::NoOffer)?;
        let allow_empty = active
            .descriptor
            .is_some_and(|descriptor| descriptor.logical_page_count() == 0);
        let result = active.receiver.finish(frame, allow_empty);
        if result.is_ok() {
            active.phase = ActiveOverlayPhase::Checking;
        }
        self.finish_active_operation(active, result)
    }

    fn finish_active_operation(
        &mut self,
        active: ActiveOverlayOffer,
        result: Result<(), CandidateHostError>,
    ) -> Result<(), M11InlineOverlayTransportError> {
        match result {
            Ok(()) => {
                self.active = Some(active);
                Ok(())
            }
            Err(error) => {
                self.discard_active(active)?;
                Err(error.into())
            }
        }
    }

    pub(crate) fn poll_install(
        &mut self,
        fuel: usize,
    ) -> Result<M11InlineOverlayInstallPoll, M11InlineOverlayTransportError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel.into());
        }
        let mut active = self
            .active
            .take()
            .ok_or(M11InlineOverlayTransportError::NoOffer)?;
        let poll = self.poll_active_install(&mut active, fuel);
        match poll {
            Ok((transitions, None)) => {
                self.active = Some(active);
                Ok(M11InlineOverlayInstallPoll {
                    transitions,
                    installed: false,
                })
            }
            Ok((transitions, Some(root))) => {
                let owner = ImportedInlineOverlay {
                    arena: active.arena,
                    root,
                    descriptor: active.descriptor,
                    retiring: false,
                };
                let install = match active.envelope.disposition() {
                    M11InlineOverlayDisposition::Authoritative { .. } => {
                        self.slot.install_authoritative(active.envelope, owner)
                    }
                    M11InlineOverlayDisposition::Unsupported { .. } => {
                        self.slot.install_unsupported(active.envelope, owner)
                    }
                };
                install.map_err(|failure| {
                    let M11InlineOverlayInstallFailure { error, owner } = failure;
                    drop(owner);
                    M11InlineOverlayTransportError::Install(match error {
                        M11InlineOverlaySlotError::BaseMismatch => "base mismatch",
                        M11InlineOverlaySlotError::StaleGeneration => "stale generation",
                        M11InlineOverlaySlotError::RetirementPending => "retirement pending",
                        M11InlineOverlaySlotError::Closing => "slot closing",
                        M11InlineOverlaySlotError::ZeroFuel => "zero fuel",
                        M11InlineOverlaySlotError::InvalidRetirementReceipt => {
                            "invalid retirement receipt"
                        }
                        M11InlineOverlaySlotError::DispositionMismatch => "disposition mismatch",
                        M11InlineOverlaySlotError::Retirement(_) => "retirement failed",
                    })
                })?;
                Ok(M11InlineOverlayInstallPoll {
                    transitions,
                    installed: true,
                })
            }
            Err(error) => {
                self.discard_active(active)?;
                Err(error)
            }
        }
    }

    fn poll_active_install(
        &mut self,
        active: &mut ActiveOverlayOffer,
        fuel: usize,
    ) -> Result<(usize, Option<Option<CommittedArenaRoot>>), M11InlineOverlayTransportError> {
        match &mut active.phase {
            ActiveOverlayPhase::Receiving => Err(CandidateHostError::Busy.into()),
            ActiveOverlayPhase::Checking => {
                let ArenaClosureCheckPoll {
                    transitions,
                    complete,
                } = active.receiver.poll_check(fuel, 0)?;
                if !complete {
                    return Ok((transitions, None));
                }
                let root = active.receiver.root_id()?;
                if let Some(descriptor) = active.descriptor {
                    let descriptor_bytes = active.descriptor_bytes.as_ref().ok_or(
                        M11InlineOverlayTransportError::InvalidProgram(
                            "authoritative leaf-projection offer lost descriptor bytes",
                        ),
                    )?;
                    active.phase = ActiveOverlayPhase::Validating(match descriptor {
                        PersistentM11LeafProjectionDescriptor::Inline(descriptor) => {
                            let (fact_root, link_value_root) =
                                decode_inline_projection_bundle(&active.arena, root)?;
                            validate_persistent_inline_projection_role(
                                &active.arena,
                                fact_root,
                                link_value_root,
                                descriptor_bytes,
                                active.envelope.binding.base.source,
                                active.envelope.binding.base.parser_profile,
                            )?;
                            PersistentM11LeafProjectionHostValidator::Inline(Box::new(
                                PersistentM11InlineProjectionHostValidator::new(
                                    &active.arena,
                                    fact_root,
                                    link_value_root,
                                    descriptor,
                                )?,
                            ))
                        }
                        PersistentM11LeafProjectionDescriptor::ProjectedInline(descriptor) => {
                            let (fact_root, link_value_root) =
                                decode_inline_projection_bundle(&active.arena, root)?;
                            let decoded = decode_persistent_projected_inline_projection_descriptor(
                                descriptor_bytes,
                                active.envelope.binding.base.source,
                                active.envelope.binding.base.parser_profile,
                            )?;
                            if decoded != descriptor {
                                return Err(M11InlineOverlayTransportError::InvalidProgram(
                                    "projected-inline descriptor changed before validation",
                                ));
                            }
                            PersistentM11LeafProjectionHostValidator::ProjectedInline(Box::new(
                                PersistentM11InlineProjectionHostValidator::new_projected(
                                    &active.arena,
                                    fact_root,
                                    link_value_root,
                                    descriptor.inner(),
                                    descriptor.projected_utf8_length(),
                                )?,
                            ))
                        }
                        PersistentM11LeafProjectionDescriptor::IndentedCode(descriptor) => {
                            validate_persistent_indented_code_projection_root(
                                &active.arena,
                                root,
                                descriptor_bytes,
                                active.envelope.binding.base.source,
                                active.envelope.binding.base.parser_profile,
                            )?;
                            PersistentM11LeafProjectionHostValidator::IndentedCode(Box::new(
                                PersistentM11IndentedCodeProjectionHostValidator::new(
                                    &active.arena,
                                    root,
                                    descriptor,
                                )?,
                            ))
                        }
                        PersistentM11LeafProjectionDescriptor::BlockQuote(descriptor) => {
                            validate_persistent_block_quote_projection_root(
                                &active.arena,
                                root,
                                descriptor_bytes,
                                active.envelope.binding.base.source,
                                active.envelope.binding.base.parser_profile,
                            )?;
                            PersistentM11LeafProjectionHostValidator::BlockQuote(Box::new(
                                PersistentM11BlockQuoteProjectionHostValidator::new(
                                    &active.arena,
                                    root,
                                    descriptor,
                                )?,
                            ))
                        }
                        PersistentM11LeafProjectionDescriptor::BulletList(descriptor) => {
                            validate_persistent_block_quote_projection_root(
                                &active.arena,
                                root,
                                descriptor_bytes,
                                active.envelope.binding.base.source,
                                active.envelope.binding.base.parser_profile,
                            )?;
                            PersistentM11LeafProjectionHostValidator::BulletList(Box::new(
                                PersistentM11BlockQuoteProjectionHostValidator::new(
                                    &active.arena,
                                    root,
                                    descriptor,
                                )?,
                            ))
                        }
                        PersistentM11LeafProjectionDescriptor::OrderedList(descriptor) => {
                            validate_persistent_block_quote_projection_root(
                                &active.arena,
                                root,
                                descriptor_bytes,
                                active.envelope.binding.base.source,
                                active.envelope.binding.base.parser_profile,
                            )?;
                            PersistentM11LeafProjectionHostValidator::OrderedList(Box::new(
                                PersistentM11BlockQuoteProjectionHostValidator::new(
                                    &active.arena,
                                    root,
                                    descriptor,
                                )?,
                            ))
                        }
                    });
                } else {
                    let root = root.ok_or(M11InlineOverlayTransportError::InvalidProgram(
                        "unsupported hot-inline snapshot has no metadata leaf",
                    ))?;
                    if active.arena.child_count(root)? != 0 {
                        return Err(M11InlineOverlayTransportError::InvalidProgram(
                            "unsupported hot-inline snapshot is not one leaf",
                        ));
                    }
                    active
                        .envelope
                        .validate_unsupported_metadata(active.arena.payload(root)?)?;
                    active.receiver.begin_seal(&mut active.arena)?;
                    active.phase = ActiveOverlayPhase::Sealing;
                }
                Ok((transitions, None))
            }
            ActiveOverlayPhase::Validating(validator) => {
                let (transitions, complete) = match validator {
                    PersistentM11LeafProjectionHostValidator::Inline(validator) => {
                        let PersistentM11InlineProjectionHostValidationPoll {
                            transitions,
                            complete,
                        } = validator.poll(&active.arena, fuel)?;
                        (transitions, complete)
                    }
                    PersistentM11LeafProjectionHostValidator::ProjectedInline(validator) => {
                        let PersistentM11InlineProjectionHostValidationPoll {
                            transitions,
                            complete,
                        } = validator.poll(&active.arena, fuel)?;
                        (transitions, complete)
                    }
                    PersistentM11LeafProjectionHostValidator::IndentedCode(validator) => {
                        let PersistentM11IndentedCodeProjectionHostValidationPoll {
                            transitions,
                            complete,
                        } = validator.poll(&active.arena, fuel)?;
                        (transitions, complete)
                    }
                    PersistentM11LeafProjectionHostValidator::BlockQuote(validator) => {
                        let PersistentM11BlockQuoteProjectionHostValidationPoll {
                            transitions,
                            complete,
                        } = validator.poll(&active.arena, fuel)?;
                        (transitions, complete)
                    }
                    PersistentM11LeafProjectionHostValidator::BulletList(validator) => {
                        let PersistentM11BlockQuoteProjectionHostValidationPoll {
                            transitions,
                            complete,
                        } = validator.poll(&active.arena, fuel)?;
                        (transitions, complete)
                    }
                    PersistentM11LeafProjectionHostValidator::OrderedList(validator) => {
                        let PersistentM11BlockQuoteProjectionHostValidationPoll {
                            transitions,
                            complete,
                        } = validator.poll(&active.arena, fuel)?;
                        if complete {
                            let descriptor = match active.descriptor {
                                Some(PersistentM11LeafProjectionDescriptor::OrderedList(
                                    descriptor,
                                )) => descriptor,
                                _ => {
                                    return Err(M11InlineOverlayTransportError::InvalidProgram(
                                        "ordered-list validator lost its typed descriptor",
                                    ));
                                }
                            };
                            let mut cursor = PersistentM11BlockQuoteProjectionHostCursor::new(
                                &active.arena,
                                active.receiver.root_id()?,
                                descriptor,
                            );
                            let line = match cursor.poll()? {
                                PersistentM11BlockQuoteProjectionHostCursorPoll::Line { line } => {
                                    line
                                }
                                PersistentM11BlockQuoteProjectionHostCursorPoll::Complete => {
                                    return Err(M11InlineOverlayTransportError::InvalidProgram(
                                        "ordered-list compact projection lost its item",
                                    ));
                                }
                            };
                            validate_ordered_item_line(&active.envelope, line)?;
                            if !matches!(
                                cursor.poll()?,
                                PersistentM11BlockQuoteProjectionHostCursorPoll::Complete
                            ) {
                                return Err(M11InlineOverlayTransportError::InvalidProgram(
                                    "ordered-list compact projection contains multiple items",
                                ));
                            }
                        }
                        (transitions, complete)
                    }
                };
                if complete {
                    let empty = active.receiver.begin_seal(&mut active.arena)?;
                    if empty {
                        return Ok((transitions, Some(None)));
                    }
                    active.phase = ActiveOverlayPhase::Sealing;
                }
                Ok((transitions, None))
            }
            ActiveOverlayPhase::Sealing => {
                let root = active.receiver.poll_seal(&mut active.arena, fuel)?;
                Ok((fuel.min(1), root.map(Some)))
            }
        }
    }

    pub(crate) fn query(
        &self,
        query: &M11InlineOverlayQuery,
    ) -> Result<Option<M11InlineOverlayHostMatch<'_>>, M11InlineOverlayTransportError> {
        Ok(match self.slot.query(query) {
            Some(M11InlineOverlayMatch::Authoritative { envelope, owner }) => {
                let descriptor =
                    owner
                        .descriptor
                        .ok_or(M11InlineOverlayTransportError::InvalidProgram(
                            "authoritative leaf-projection root lost its descriptor",
                        ))?;
                let cursor =
                    owner
                        .cursor()?
                        .ok_or(M11InlineOverlayTransportError::InvalidProgram(
                            "authoritative leaf-projection root lost its cursor",
                        ))?;
                Some(match (descriptor, cursor) {
                    (
                        PersistentM11LeafProjectionDescriptor::Inline(descriptor),
                        PersistentM11LeafProjectionHostCursor::Inline(cursor),
                    ) => {
                        let (_, link_value_root) = decode_inline_projection_bundle(
                            &owner.arena,
                            owner.root.as_ref().map(CommittedArenaRoot::id),
                        )?;
                        M11InlineOverlayHostMatch::InlineAuthoritative {
                            #[cfg(test)]
                            envelope,
                            descriptor,
                            cursor,
                            link_value_arena: &owner.arena,
                            link_value_root,
                        }
                    }
                    (
                        PersistentM11LeafProjectionDescriptor::ProjectedInline(descriptor),
                        PersistentM11LeafProjectionHostCursor::ProjectedInline(cursor),
                    ) => {
                        let (_, link_value_root) = decode_inline_projection_bundle(
                            &owner.arena,
                            owner.root.as_ref().map(CommittedArenaRoot::id),
                        )?;
                        if link_value_root.is_some() {
                            return Err(M11InlineOverlayTransportError::InvalidProgram(
                                "projected-inline payload carries forbidden link values",
                            ));
                        }
                        M11InlineOverlayHostMatch::ProjectedInlineAuthoritative {
                            descriptor,
                            cursor,
                        }
                    }
                    (
                        PersistentM11LeafProjectionDescriptor::IndentedCode(descriptor),
                        PersistentM11LeafProjectionHostCursor::IndentedCode(cursor),
                    ) => {
                        M11InlineOverlayHostMatch::IndentedCodeAuthoritative { descriptor, cursor }
                    }
                    (
                        PersistentM11LeafProjectionDescriptor::BlockQuote(descriptor),
                        PersistentM11LeafProjectionHostCursor::BlockQuote(cursor),
                    ) => M11InlineOverlayHostMatch::BlockQuoteAuthoritative { descriptor, cursor },
                    (
                        PersistentM11LeafProjectionDescriptor::BulletList(descriptor),
                        PersistentM11LeafProjectionHostCursor::BulletList(cursor),
                    ) => M11InlineOverlayHostMatch::BulletListAuthoritative {
                        envelope,
                        descriptor,
                        cursor,
                    },
                    (
                        PersistentM11LeafProjectionDescriptor::OrderedList(descriptor),
                        PersistentM11LeafProjectionHostCursor::OrderedList(cursor),
                    ) => M11InlineOverlayHostMatch::OrderedListAuthoritative {
                        envelope,
                        descriptor,
                        cursor,
                    },
                    _ => {
                        return Err(M11InlineOverlayTransportError::InvalidProgram(
                            "leaf-projection descriptor and cursor kinds differ",
                        ));
                    }
                })
            }
            Some(M11InlineOverlayMatch::Unsupported {
                envelope: _unsupported_envelope,
                owner,
            }) => Some(M11InlineOverlayHostMatch::Unsupported {
                #[cfg(test)]
                envelope: _unsupported_envelope,
                metadata: owner.unsupported_metadata()?.ok_or(
                    M11InlineOverlayTransportError::InvalidProgram(
                        "unsupported hot-inline root lost metadata",
                    ),
                )?,
            }),
            None => None,
        })
    }

    pub(crate) fn observe_base(
        &mut self,
        base: M11InlineOverlayBase,
    ) -> Result<bool, M11InlineOverlayTransportError> {
        if self.active.is_some() || self.discarding.is_some() {
            return Err(CandidateHostError::Busy.into());
        }
        self.slot
            .observe_base(base)
            .map_err(|_| M11InlineOverlayTransportError::Install("base invalidation failed"))
    }

    pub(crate) fn abort_snapshot(&mut self) -> Result<bool, M11InlineOverlayTransportError> {
        let Some(active) = self.active.take() else {
            return Ok(false);
        };
        self.discard_active(active)?;
        Ok(true)
    }

    fn discard_active(
        &mut self,
        active: ActiveOverlayOffer,
    ) -> Result<(), M11InlineOverlayTransportError> {
        if self.discarding.is_some() {
            return Err(CandidateHostError::Busy.into());
        }
        let ActiveOverlayOffer {
            mut arena,
            receiver,
            ..
        } = active;
        receiver.abort(&mut arena)?;
        self.discarding = Some(arena);
        Ok(())
    }

    pub(crate) fn poll_retire(
        &mut self,
        fuel: usize,
    ) -> Result<M11InlineOverlayRetirePoll, M11InlineOverlayTransportError> {
        if fuel == 0 {
            return Err(CandidateHostError::ZeroFuel.into());
        }
        let mut transitions = 0;
        if let Some(arena) = self.discarding.as_mut() {
            let receipt = arena.poll_reclaim(fuel);
            transitions += receipt.transitions;
            let metrics = arena.metrics();
            if receipt.complete && metrics.resident_nodes == 0 && metrics.live_builds == 0 {
                self.discarding.take();
            }
        }
        if transitions < fuel {
            let poll = self
                .slot
                .poll_retire(fuel - transitions)
                .map_err(|_| M11InlineOverlayTransportError::Install("retirement failed"))?;
            transitions += poll.transitions;
        }
        Ok(M11InlineOverlayRetirePoll {
            transitions,
            complete: self.discarding.is_none() && !self.slot.has_retiring(),
        })
    }

    pub(crate) fn begin_close(&mut self) -> Result<(), M11InlineOverlayTransportError> {
        if self.closing {
            return Ok(());
        }
        if let Some(active) = self.active.take() {
            self.discard_active(active)?;
        }
        self.slot
            .begin_close()
            .map_err(|_| M11InlineOverlayTransportError::Install("close failed"))?;
        self.closing = true;
        Ok(())
    }

    pub(crate) fn poll_close(
        &mut self,
        fuel: usize,
    ) -> Result<bool, M11InlineOverlayTransportError> {
        if !self.closing {
            return Err(CandidateHostError::Busy.into());
        }
        Ok(self.poll_retire(fuel)?.complete && self.slot.is_empty())
    }
}

impl Drop for M11InlineOverlayHostStore {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(
                self.closing
                    && self.active.is_none()
                    && self.discarding.is_none()
                    && self.slot.is_empty(),
                "hot-inline host must be explicitly closed and fuel-drained"
            );
        }
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineOverlayTransportError {
    Overlay(M11InlineOverlayError),
    Projection(M11InlineProjectionError),
    IndentedCodeProjection(M11IndentedCodeProjectionError),
    BlockQuoteProjection(M11BlockQuoteProjectionError),
    Host(CandidateHostError),
    NoOffer,
    SlotBaseMismatch,
    StaleGeneration,
    InvalidProgram(&'static str),
    Install(&'static str),
}

impl From<M11InlineOverlayError> for M11InlineOverlayTransportError {
    fn from(value: M11InlineOverlayError) -> Self {
        Self::Overlay(value)
    }
}

impl From<M11InlineProjectionError> for M11InlineOverlayTransportError {
    fn from(value: M11InlineProjectionError) -> Self {
        Self::Projection(value)
    }
}

impl From<M11IndentedCodeProjectionError> for M11InlineOverlayTransportError {
    fn from(value: M11IndentedCodeProjectionError) -> Self {
        Self::IndentedCodeProjection(value)
    }
}

impl From<M11BlockQuoteProjectionError> for M11InlineOverlayTransportError {
    fn from(value: M11BlockQuoteProjectionError) -> Self {
        Self::BlockQuoteProjection(value)
    }
}

impl From<CandidateHostError> for M11InlineOverlayTransportError {
    fn from(value: CandidateHostError) -> Self {
        Self::Host(value)
    }
}

impl From<crate::storage::ArenaError> for M11InlineOverlayTransportError {
    fn from(value: crate::storage::ArenaError) -> Self {
        Self::Host(CandidateHostError::Arena(value))
    }
}

impl fmt::Display for M11InlineOverlayTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overlay(error) => write!(formatter, "{error}"),
            Self::Projection(error) => write!(formatter, "{error}"),
            Self::IndentedCodeProjection(error) => write!(formatter, "{error}"),
            Self::BlockQuoteProjection(error) => write!(formatter, "{error}"),
            Self::Host(error) => write!(formatter, "{error}"),
            Self::NoOffer => formatter.write_str("hot-inline host has no active offer"),
            Self::SlotBaseMismatch => formatter.write_str("hot-inline base is not installed"),
            Self::StaleGeneration => formatter.write_str("hot-inline generation is stale"),
            Self::InvalidProgram(message) => {
                write!(formatter, "invalid hot-inline program: {message}")
            }
            Self::Install(message) => write!(formatter, "hot-inline install failed: {message}"),
        }
    }
}

impl std::error::Error for M11InlineOverlayTransportError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOverlayRetirePoll {
    pub(crate) transitions: usize,
    pub(crate) complete: bool,
}

/// Lifecycle adapter for a future independently imported sidecar root.
///
/// `begin_retire` must be atomic: on error the owner remains queryable and a
/// later call may retry. Once it succeeds, the owner may be dropped only after
/// a poll reports `complete`.
pub(crate) trait M11InlineOverlayRetirable {
    type Error;

    fn begin_retire(&mut self) -> Result<(), Self::Error>;
    fn poll_retire(&mut self, fuel: usize) -> Result<M11InlineOverlayRetirePoll, Self::Error>;
}

struct InstalledOverlay<T> {
    envelope: M11InlineOverlayEnvelope,
    owner: Option<T>,
}

pub(crate) enum M11InlineOverlayMatch<'slot, T> {
    Authoritative {
        envelope: &'slot M11InlineOverlayEnvelope,
        owner: &'slot T,
    },
    Unsupported {
        envelope: &'slot M11InlineOverlayEnvelope,
        owner: &'slot T,
    },
}

/// One active refinement plus at most one fuel-retiring predecessor.
pub(crate) struct M11InlineOverlaySlot<T> {
    base: M11InlineOverlayBase,
    active: Option<InstalledOverlay<T>>,
    retiring: Option<T>,
    closing: bool,
}

impl<T> M11InlineOverlaySlot<T> {
    pub(crate) fn new(base: M11InlineOverlayBase) -> Self {
        Self {
            base,
            active: None,
            retiring: None,
            closing: false,
        }
    }

    pub(crate) fn query(
        &self,
        query: &M11InlineOverlayQuery,
    ) -> Option<M11InlineOverlayMatch<'_, T>> {
        if self.closing {
            return None;
        }
        let active = self.active.as_ref()?;
        if !active.envelope.matches_query(query) {
            return None;
        }
        match (&active.envelope.disposition, active.owner.as_ref()) {
            (M11InlineOverlayDisposition::Authoritative { .. }, Some(owner)) => {
                Some(M11InlineOverlayMatch::Authoritative {
                    envelope: &active.envelope,
                    owner,
                })
            }
            (M11InlineOverlayDisposition::Unsupported { .. }, Some(owner)) => {
                Some(M11InlineOverlayMatch::Unsupported {
                    envelope: &active.envelope,
                    owner,
                })
            }
            _ => None,
        }
    }

    pub(crate) const fn has_retiring(&self) -> bool {
        self.retiring.is_some()
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.active.is_none() && self.retiring.is_none()
    }
}

impl<T: M11InlineOverlayRetirable> M11InlineOverlaySlot<T> {
    pub(crate) fn install_authoritative(
        &mut self,
        envelope: M11InlineOverlayEnvelope,
        owner: T,
    ) -> Result<(), M11InlineOverlayInstallFailure<T, T::Error>> {
        self.install_inner(envelope, Some(owner))
    }

    pub(crate) fn install_unsupported(
        &mut self,
        envelope: M11InlineOverlayEnvelope,
        owner: T,
    ) -> Result<(), M11InlineOverlayInstallFailure<T, T::Error>> {
        self.install_inner(envelope, Some(owner))
    }

    fn install_inner(
        &mut self,
        envelope: M11InlineOverlayEnvelope,
        owner: Option<T>,
    ) -> Result<(), M11InlineOverlayInstallFailure<T, T::Error>> {
        let reject = |error, owner| Err(M11InlineOverlayInstallFailure { error, owner });
        if matches!(
            (&envelope.disposition, &owner),
            (M11InlineOverlayDisposition::Authoritative { .. }, None)
                | (M11InlineOverlayDisposition::Unsupported { .. }, None)
        ) {
            return reject(M11InlineOverlaySlotError::DispositionMismatch, owner);
        }
        if self.closing {
            return reject(M11InlineOverlaySlotError::Closing, owner);
        }
        if envelope.binding.base != self.base {
            return reject(M11InlineOverlaySlotError::BaseMismatch, owner);
        }
        if self.retiring.is_some() {
            return reject(M11InlineOverlaySlotError::RetirementPending, owner);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.envelope.binding.generation >= envelope.binding.generation)
        {
            return reject(M11InlineOverlaySlotError::StaleGeneration, owner);
        }
        if let Some(active) = self.active.as_mut() {
            if let Some(active_owner) = active.owner.as_mut() {
                if let Err(error) = active_owner.begin_retire() {
                    return reject(M11InlineOverlaySlotError::Retirement(error), owner);
                }
            }
        }
        if let Some(previous) = self.active.take() {
            self.retiring = previous.owner;
        }
        self.active = Some(InstalledOverlay { envelope, owner });
        Ok(())
    }

    pub(crate) fn observe_base(
        &mut self,
        base: M11InlineOverlayBase,
    ) -> Result<bool, M11InlineOverlaySlotError<T::Error>> {
        if self.closing {
            return Err(M11InlineOverlaySlotError::Closing);
        }
        if base == self.base {
            return Ok(false);
        }
        if self.active.is_some() && self.retiring.is_some() {
            return Err(M11InlineOverlaySlotError::RetirementPending);
        }
        if let Some(active) = self.active.as_mut() {
            if let Some(owner) = active.owner.as_mut() {
                owner
                    .begin_retire()
                    .map_err(M11InlineOverlaySlotError::Retirement)?;
            }
        }
        if let Some(active) = self.active.take() {
            self.retiring = active.owner;
        }
        self.base = base;
        Ok(true)
    }

    pub(crate) fn begin_close(&mut self) -> Result<(), M11InlineOverlaySlotError<T::Error>> {
        if self.closing {
            return Ok(());
        }
        if self.retiring.is_none() {
            self.schedule_active_retirement()?;
        }
        self.closing = true;
        Ok(())
    }

    pub(crate) fn poll_retire(
        &mut self,
        fuel: usize,
    ) -> Result<M11InlineOverlayRetirePoll, M11InlineOverlaySlotError<T::Error>> {
        if fuel == 0 {
            return Err(M11InlineOverlaySlotError::ZeroFuel);
        }
        if self.retiring.is_none() && self.closing {
            self.schedule_active_retirement()?;
        }
        let Some(retiring) = self.retiring.as_mut() else {
            return Ok(M11InlineOverlayRetirePoll {
                transitions: 0,
                complete: true,
            });
        };
        let poll = retiring
            .poll_retire(fuel)
            .map_err(M11InlineOverlaySlotError::Retirement)?;
        if poll.transitions > fuel {
            return Err(M11InlineOverlaySlotError::InvalidRetirementReceipt);
        }
        if poll.complete {
            self.retiring.take();
            if self.closing {
                self.schedule_active_retirement()?;
            }
        }
        Ok(M11InlineOverlayRetirePoll {
            transitions: poll.transitions,
            complete: self.retiring.is_none() && (!self.closing || self.active.is_none()),
        })
    }

    fn schedule_active_retirement(&mut self) -> Result<(), M11InlineOverlaySlotError<T::Error>> {
        if self.retiring.is_some() {
            return Ok(());
        }
        if let Some(active) = self.active.as_mut() {
            if let Some(owner) = active.owner.as_mut() {
                owner
                    .begin_retire()
                    .map_err(M11InlineOverlaySlotError::Retirement)?;
            }
        }
        if let Some(active) = self.active.take() {
            self.retiring = active.owner;
        }
        Ok(())
    }
}

pub(crate) struct M11InlineOverlayInstallFailure<T, E> {
    pub(crate) error: M11InlineOverlaySlotError<E>,
    pub(crate) owner: Option<T>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOverlaySlotError<E> {
    BaseMismatch,
    StaleGeneration,
    RetirementPending,
    Closing,
    ZeroFuel,
    InvalidRetirementReceipt,
    DispositionMismatch,
    Retirement(E),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOverlayError {
    InvalidBase,
    InvalidBinding,
    ProjectionMismatch,
    BindingMismatch,
    MalformedEnvelope,
    CoordinateOverflow,
    InvalidUnsupported,
}

impl fmt::Display for M11InlineOverlayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidBase => "inline overlay base authority is inconsistent",
            Self::InvalidBinding => "inline overlay block binding is invalid",
            Self::ProjectionMismatch => {
                "inline Projection does not match the exact overlay binding"
            }
            Self::BindingMismatch => "inline overlay envelope names another exact binding",
            Self::MalformedEnvelope => "inline overlay envelope is malformed",
            Self::CoordinateOverflow => "inline overlay coordinate overflow",
            Self::InvalidUnsupported => "inline overlay unsupported certificate is invalid",
        })
    }
}

impl std::error::Error for M11InlineOverlayError {}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::convert::Infallible;
    use std::rc::Rc;

    use super::*;
    use crate::block_quote_projection::{
        BlockQuoteLineV1, M11BlockQuoteProjectionBuild, M11BlockQuoteProjectionBuildStatus,
        PersistentM11BlockQuoteProjectionHostCursorPoll,
    };
    use crate::identity::CandidateGeneration;
    use crate::identity::RuntimeIdentity;
    use crate::indented_code_projection::{
        IndentedCodeLineV1, M11IndentedCodeProjectionBuild, M11IndentedCodeProjectionBuildStatus,
        PersistentM11IndentedCodeProjectionHostCursorPoll,
    };
    use crate::inline_projection::{
        encode_persistent_inline_link_values, M11InlineLinkValue, M11InlineProjectionBuild,
        M11InlineProjectionBuildStatus, M11InlineProjectionFact, M11InlineProjectionKind,
        PersistentM11InlineProjectionHostCursorPoll,
    };
    use crate::m11_host::{
        M11HostBlockQuoteCursorPoll, M11HostCanonicalLineEnding, M11HostInlineSidecar,
        M11HostInlineSidecarBinding, M11HostInlineSidecarQuery,
    };
    use crate::{DocumentRuntime, DocumentRuntimeConfig};

    struct TestOwner {
        id: u64,
        retirement_left: usize,
        retiring: bool,
        residents: Rc<Cell<usize>>,
    }

    impl TestOwner {
        fn new(id: u64, retirement_left: usize, residents: &Rc<Cell<usize>>) -> Self {
            residents.set(residents.get() + 1);
            Self {
                id,
                retirement_left,
                retiring: false,
                residents: Rc::clone(residents),
            }
        }
    }

    impl Drop for TestOwner {
        fn drop(&mut self) {
            self.residents.set(self.residents.get() - 1);
        }
    }

    impl M11InlineOverlayRetirable for TestOwner {
        type Error = Infallible;

        fn begin_retire(&mut self) -> Result<(), Self::Error> {
            self.retiring = true;
            Ok(())
        }

        fn poll_retire(&mut self, fuel: usize) -> Result<M11InlineOverlayRetirePoll, Self::Error> {
            assert!(self.retiring);
            let transitions = fuel.min(self.retirement_left);
            self.retirement_left -= transitions;
            Ok(M11InlineOverlayRetirePoll {
                transitions,
                complete: self.retirement_left == 0,
            })
        }
    }

    fn build_middle_projection(
        runtime: &mut DocumentRuntime,
        profile: ParserProfileId,
    ) -> M11InlineProjectionRoot {
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build =
            M11InlineProjectionBuild::new(runtime, lease, 6..16, profile).expect("build");
        build
            .offer_page(&[M11InlineProjectionFact::new(
                M11InlineProjectionKind::Strong,
                0,
                0..10,
                2..8,
            )
            .expect("strong fact")])
            .expect("page");
        loop {
            match build.poll(runtime, 32).expect("accept page").status() {
                M11InlineProjectionBuildStatus::NeedsPage => break,
                M11InlineProjectionBuildStatus::Pending => {}
                status => panic!("unexpected accepting status {status:?}"),
            }
        }
        build.finish_input().expect("finish");
        loop {
            match build.poll(runtime, 32).expect("poll").status() {
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete => break,
                status => panic!("unexpected build status {status:?}"),
            }
        }
        build.take_root().expect("root")
    }

    fn build_direct_link_projection(
        runtime: &mut DocumentRuntime,
        profile: ParserProfileId,
    ) -> M11InlineProjectionRoot {
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build =
            M11InlineProjectionBuild::new(runtime, lease, 6..16, profile).expect("build");
        let fact =
            M11InlineProjectionFact::new(M11InlineProjectionKind::DirectLink, 0, 0..10, 1..2)
                .expect("direct link fact");
        let value = M11InlineLinkValue::new(
            0,
            4..5,
            Some(6..9),
            "d",
            Some("t".to_owned().into_boxed_str()),
        )
        .expect("direct link value");
        build
            .offer_page_with_link_values(&[fact], &[value])
            .expect("paired page");
        loop {
            match build.poll(runtime, 32).expect("accept page").status() {
                M11InlineProjectionBuildStatus::NeedsPage => break,
                M11InlineProjectionBuildStatus::Pending => {}
                status => panic!("unexpected accepting status {status:?}"),
            }
        }
        build.finish_input().expect("finish");
        loop {
            match build.poll(runtime, 32).expect("poll").status() {
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete => break,
                status => panic!("unexpected build status {status:?}"),
            }
        }
        build.take_root().expect("root")
    }

    fn build_indented_projection(
        runtime: &mut DocumentRuntime,
        profile: ParserProfileId,
    ) -> M11IndentedCodeProjectionRoot {
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11IndentedCodeProjectionBuild::new(runtime, lease, 6..24, 6..24, profile)
            .expect("indented build");
        build
            .offer_page(&[
                IndentedCodeLineV1::code(0, 11, 4, 5).expect("first line"),
                IndentedCodeLineV1::internal_blank(11, 1, 0).expect("internal blank"),
                IndentedCodeLineV1::code(12, 6, 1, 4).expect("last line"),
            ])
            .expect("indented page");
        loop {
            match build.poll(runtime, 32).expect("accept page").status() {
                M11IndentedCodeProjectionBuildStatus::NeedsPage => break,
                M11IndentedCodeProjectionBuildStatus::Pending => {}
                status => panic!("unexpected indented accepting status {status:?}"),
            }
        }
        build.finish_input().expect("finish indented");
        loop {
            match build.poll(runtime, 32).expect("poll indented").status() {
                M11IndentedCodeProjectionBuildStatus::Pending => {}
                M11IndentedCodeProjectionBuildStatus::Complete => break,
                status => panic!("unexpected indented build status {status:?}"),
            }
        }
        build.take_root().expect("indented root")
    }

    fn build_block_quote_projection(
        runtime: &mut DocumentRuntime,
        profile: ParserProfileId,
    ) -> M11BlockQuoteProjectionRoot {
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build =
            M11BlockQuoteProjectionBuild::new(runtime, lease, 6..26, 6..26, 16, 16, profile)
                .expect("block-quote build");
        build
            .offer_page(&[
                BlockQuoteLineV1::marked(0, 9, 2, 5).expect("marked alpha"),
                BlockQuoteLineV1::lazy(9, 5, 4).expect("lazy continuation"),
                BlockQuoteLineV1::marked(14, 6, 2, 4).expect("marked beta"),
            ])
            .expect("block-quote page");
        loop {
            match build.poll(runtime, 32).expect("accept page").status() {
                M11BlockQuoteProjectionBuildStatus::NeedsPage => break,
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                status => panic!("unexpected block-quote accepting status {status:?}"),
            }
        }
        build.finish_input().expect("finish block quote");
        loop {
            match build.poll(runtime, 32).expect("poll block quote").status() {
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                M11BlockQuoteProjectionBuildStatus::Complete => break,
                status => panic!("unexpected block-quote build status {status:?}"),
            }
        }
        build.take_root().expect("block-quote root")
    }

    fn build_ordered_item_projection(
        runtime: &mut DocumentRuntime,
        profile: ParserProfileId,
    ) -> M11BlockQuoteProjectionRoot {
        let lease = runtime.snapshot_current_source().expect("source");
        let mut build = M11BlockQuoteProjectionBuild::new_ordered_list(
            runtime,
            lease,
            6..37,
            17..28,
            8,
            5,
            profile,
        )
        .expect("ordered-item build");
        build
            .offer_page(&[BlockQuoteLineV1::ordered_item(11, 11, 3, 0, 3, 6, 3)
                .expect("ordered Unicode item")])
            .expect("ordered-item page");
        loop {
            match build.poll(runtime, 32).expect("accept page").status() {
                M11BlockQuoteProjectionBuildStatus::NeedsPage => break,
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                status => panic!("unexpected ordered-item accepting status {status:?}"),
            }
        }
        build.finish_input().expect("finish ordered item");
        loop {
            match build.poll(runtime, 32).expect("poll ordered item").status() {
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                M11BlockQuoteProjectionBuildStatus::Complete => break,
                status => panic!("unexpected ordered-item build status {status:?}"),
            }
        }
        build.take_root().expect("ordered-item root")
    }

    fn base(
        runtime: &DocumentRuntime,
        profile: ParserProfileId,
        publication_seed: u8,
        generation: u64,
    ) -> M11InlineOverlayBase {
        let source = runtime.current_source_version().expect("source");
        let authority = CandidateAuthority::new(
            runtime.producer_identity(),
            RuntimeIdentity::new([publication_seed; 16]).expect("publication"),
            source,
            CandidateGeneration::from_wire(generation).expect("generation"),
            u32::try_from(profile.get()).expect("profile"),
        )
        .expect("authority");
        M11InlineOverlayBase::new(authority, source, profile).expect("base")
    }

    fn binding(
        base: M11InlineOverlayBase,
        generation: u64,
        ordinal: u64,
    ) -> M11InlineOverlayBinding {
        M11InlineOverlayBinding::new(
            base,
            generation,
            M11InlineOverlayOwner::BlockOrdinal(ordinal),
            6..17,
            6..16,
            6..17,
            6..16,
        )
        .expect("binding")
    }

    fn indented_binding(
        base: M11InlineOverlayBase,
        generation: u64,
        ordinal: u64,
    ) -> M11InlineOverlayBinding {
        M11InlineOverlayBinding::new(
            base,
            generation,
            M11InlineOverlayOwner::BlockOrdinal(ordinal),
            6..24,
            6..24,
            6..24,
            6..24,
        )
        .expect("indented binding")
    }

    fn block_quote_binding(
        base: M11InlineOverlayBase,
        generation: u64,
        ordinal: u64,
    ) -> M11InlineOverlayBinding {
        M11InlineOverlayBinding::new(
            base,
            generation,
            M11InlineOverlayOwner::BlockOrdinal(ordinal),
            6..26,
            6..26,
            6..26,
            6..26,
        )
        .expect("block-quote binding")
    }

    fn ordered_item_binding(
        base: M11InlineOverlayBase,
        generation: u64,
        ordinal: u64,
    ) -> M11InlineOverlayBinding {
        M11InlineOverlayBinding::new(
            base,
            generation,
            M11InlineOverlayOwner::BlockOrdinal(ordinal),
            6..37,
            17..28,
            6..34,
            17..25,
        )
        .expect("ordered-item binding")
    }

    fn close_projection(runtime: &mut DocumentRuntime, root: &mut M11InlineProjectionRoot) {
        root.begin_release(runtime).expect("begin root release");
        while !root
            .poll_release(runtime, 32)
            .expect("poll root release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    fn close_indented_projection(
        runtime: &mut DocumentRuntime,
        root: &mut M11IndentedCodeProjectionRoot,
    ) {
        root.begin_release(runtime).expect("begin root release");
        while !root
            .poll_release(runtime, 32)
            .expect("poll root release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    fn close_block_quote_projection(
        runtime: &mut DocumentRuntime,
        root: &mut M11BlockQuoteProjectionRoot,
    ) {
        root.begin_release(runtime).expect("begin root release");
        while !root
            .poll_release(runtime, 32)
            .expect("poll root release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    fn feed_snapshot(
        runtime: &DocumentRuntime,
        host: &mut M11InlineOverlayHostStore,
        encoder: &mut M11InlineOverlaySnapshotEncoder,
    ) {
        loop {
            match encoder.poll(runtime, 1).expect("encode") {
                M11InlineOverlaySnapshotEncodePoll::Pending { .. } => {}
                M11InlineOverlaySnapshotEncodePoll::Frame { bytes, .. } => {
                    host.offer_node(&bytes).expect("offer node");
                }
                M11InlineOverlaySnapshotEncodePoll::Complete { bytes, .. } => {
                    host.finish_snapshot(&bytes).expect("finish");
                    break;
                }
            }
        }
    }

    fn install_snapshot(host: &mut M11InlineOverlayHostStore) {
        loop {
            let poll = host.poll_install(1).expect("install poll");
            if poll.installed {
                break;
            }
        }
    }

    fn drain_host(host: &mut M11InlineOverlayHostStore) {
        host.begin_close().expect("begin host close");
        while !host.poll_close(1).expect("poll host close") {}
    }

    #[test]
    fn independent_arena_middle_paragraph_transport_installs_and_queries() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let exact = binding(base(&runtime, profile, 71, 13), 1, 2);
        let mut encoder =
            M11InlineOverlaySnapshotEncoder::authoritative(&runtime, exact.clone(), &projection)
                .expect("encoder");
        let begin = encoder.begin_frame().expect("begin");
        let legacy_envelope = &begin[INLINE_OVERLAY_BEGIN_HEADER_BYTES
            ..INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES];
        assert_eq!(
            read_u32(legacy_envelope, 4).expect("legacy schema"),
            INLINE_OVERLAY_SCHEMA_INLINE
        );
        assert_eq!(read_u32(legacy_envelope, 68).expect("legacy reserved"), 0);
        let mut host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());
        host.begin_snapshot(exact.clone(), &begin)
            .expect("begin host");
        feed_snapshot(&runtime, &mut host, &mut encoder);
        install_snapshot(&mut host);

        let M11InlineOverlayHostMatch::InlineAuthoritative {
            envelope,
            mut cursor,
            ..
        } = host.query(&exact.query()).expect("query").expect("match")
        else {
            panic!("expected authoritative sidecar");
        };
        assert_eq!(envelope.binding(), &exact);
        let PersistentM11InlineProjectionHostCursorPoll::Fact { fact } =
            cursor.poll().expect("first fact")
        else {
            panic!("expected middle Strong fact");
        };
        assert_eq!(fact.kind(), M11InlineProjectionKind::Strong);
        assert_eq!(fact.relative_range(), 0..10);
        assert_eq!(fact.relative_content_range(), 2..8);
        assert!(matches!(
            cursor.poll().expect("complete"),
            PersistentM11InlineProjectionHostCursorPoll::Complete
        ));

        drain_host(&mut host);
        drop(encoder);
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn direct_link_companion_values_cross_the_hot_overlay_and_query_together() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n[x](d \"t\")\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_direct_link_projection(&mut runtime, profile);
        let exact = binding(base(&runtime, profile, 72, 14), 1, 2);
        let mut encoder =
            M11InlineOverlaySnapshotEncoder::authoritative(&runtime, exact.clone(), &projection)
                .expect("encoder");
        let begin = encoder.begin_frame().expect("begin");
        let envelope = &begin[INLINE_OVERLAY_BEGIN_HEADER_BYTES
            ..INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES];
        assert_eq!(read_u32(envelope, 208).expect("link value count"), 1);
        assert_eq!(read_u32(envelope, 212).expect("FLKIV bytes"), 50);
        assert!(read_u64(envelope, 216).expect("value storage pages") > 0);

        let mut host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());
        host.begin_snapshot(exact.clone(), &begin)
            .expect("begin host");
        feed_snapshot(&runtime, &mut host, &mut encoder);
        install_snapshot(&mut host);

        let M11InlineOverlayHostMatch::InlineAuthoritative {
            descriptor,
            mut cursor,
            link_value_arena,
            link_value_root,
            ..
        } = host.query(&exact.query()).expect("query").expect("match")
        else {
            panic!("expected authoritative inline sidecar");
        };
        let PersistentM11InlineProjectionHostCursorPoll::Fact { fact } =
            cursor.poll().expect("first fact")
        else {
            panic!("expected direct link fact");
        };
        assert_eq!(fact.kind(), M11InlineProjectionKind::DirectLink);
        assert_eq!(descriptor.link_value_entry_count(), 1);
        assert_eq!(descriptor.link_value_encoded_bytes(), 50);
        let mut encoded = vec![0_u8; 50];
        let receipt = encode_persistent_inline_link_values(
            link_value_arena,
            link_value_root,
            descriptor,
            &mut encoded,
        )
        .expect("encode installed FLKIV");
        assert_eq!(receipt.entry_count, 1);
        assert!(receipt.tree_nodes_visited > 0);
        assert_eq!(&encoded[..16], b"FLKIV001\x01\0\0\0\x01\0\0\0");
        assert_eq!(
            &encoded[16..48],
            &[
                0, 0, 0, 0, // parent fact ordinal
                1, 0, 0, 0, // title present
                4, 0, 0, 0, // destination source start
                1, 0, 0, 0, // destination source length
                6, 0, 0, 0, // title source start
                3, 0, 0, 0, // title source length
                1, 0, 0, 0, // cooked destination length
                1, 0, 0, 0, // cooked title length
            ]
        );
        assert_eq!(&encoded[48..], b"dt");

        drain_host(&mut host);
        drop(encoder);
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn indented_code_schema2_roundtrip_corruption_stale_and_reclaim_are_exact() {
        let profile = ParserProfileId::new(2).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n    alpha\r\n\n\tbeta\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_indented_projection(&mut runtime, profile);
        let exact = indented_binding(base(&runtime, profile, 81, 17), 1, 2);
        let mut encoder = M11InlineOverlaySnapshotEncoder::authoritative_indented_code(
            &runtime,
            exact.clone(),
            &projection,
        )
        .expect("indented encoder");
        let begin = encoder.begin_frame().expect("begin");
        assert_eq!(
            read_u32(
                &begin[INLINE_OVERLAY_BEGIN_HEADER_BYTES
                    ..INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES],
                4,
            )
            .expect("schema"),
            INLINE_OVERLAY_SCHEMA_TYPED
        );
        assert_eq!(
            read_u32(
                &begin[INLINE_OVERLAY_BEGIN_HEADER_BYTES
                    ..INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES],
                68,
            )
            .expect("kind"),
            2
        );

        let mut host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());
        let mut corrupt = begin.to_vec();
        let descriptor_offset =
            INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES;
        corrupt[descriptor_offset + 128] ^= 1;
        assert!(host.begin_snapshot(exact.clone(), &corrupt).is_err());

        host.begin_snapshot(exact.clone(), &begin)
            .expect("begin host");
        feed_snapshot(&runtime, &mut host, &mut encoder);
        install_snapshot(&mut host);
        let M11InlineOverlayHostMatch::IndentedCodeAuthoritative {
            descriptor,
            mut cursor,
            ..
        } = host.query(&exact.query()).expect("query").expect("match")
        else {
            panic!("expected indented-code sidecar");
        };
        assert_eq!(descriptor.physical_block_range(), 6..24);
        assert_eq!(descriptor.requested_window(), 6..24);
        assert_eq!(descriptor.line_count(), 3);
        let mut lines = Vec::new();
        while let PersistentM11IndentedCodeProjectionHostCursorPoll::Line { line } =
            cursor.poll().expect("indented cursor")
        {
            lines.push(line);
        }
        assert_eq!(lines.len(), 3);
        assert!(lines[1].is_internal_blank());
        drop(cursor);

        assert!(matches!(
            host.begin_snapshot(exact.clone(), &begin),
            Err(M11InlineOverlayTransportError::StaleGeneration)
        ));
        drain_host(&mut host);
        drop(encoder);
        close_indented_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn block_quote_schema2_kind3_roundtrip_and_reclaim_are_exact() {
        let profile = ParserProfileId::new(3).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n> alpha\r\nlazy\n> beta",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_block_quote_projection(&mut runtime, profile);
        let exact = block_quote_binding(base(&runtime, profile, 91, 19), 1, 2);
        let mut encoder = M11InlineOverlaySnapshotEncoder::authoritative_block_quote(
            &runtime,
            exact.clone(),
            &projection,
        )
        .expect("block-quote encoder");
        let begin = encoder.begin_frame().expect("begin");
        let envelope = &begin[INLINE_OVERLAY_BEGIN_HEADER_BYTES
            ..INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES];
        assert_eq!(
            read_u32(envelope, 4).expect("schema"),
            INLINE_OVERLAY_SCHEMA_TYPED
        );
        assert_eq!(read_u32(envelope, 68).expect("kind"), 3);
        assert_eq!(
            read_u32(&begin, 8).expect("descriptor length") as usize,
            PERSISTENT_BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
        );

        let mut host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());
        let mut corrupt = begin.to_vec();
        let descriptor_offset =
            INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES;
        corrupt[descriptor_offset + 136] ^= 1;
        assert!(host.begin_snapshot(exact.clone(), &corrupt).is_err());

        host.begin_snapshot(exact.clone(), &begin)
            .expect("begin host");
        feed_snapshot(&runtime, &mut host, &mut encoder);
        install_snapshot(&mut host);
        let M11InlineOverlayHostMatch::BlockQuoteAuthoritative {
            descriptor,
            mut cursor,
            ..
        } = host.query(&exact.query()).expect("query").expect("match")
        else {
            panic!("expected block-quote sidecar");
        };
        assert_eq!(descriptor.physical_block_range(), 6..26);
        assert_eq!(descriptor.requested_window(), 6..26);
        assert_eq!(descriptor.projected_utf8_length(), 16);
        assert_eq!(descriptor.projected_utf16_length(), 16);
        assert_eq!(descriptor.line_count(), 3);
        let mut lines = Vec::new();
        while let PersistentM11BlockQuoteProjectionHostCursorPoll::Line { line } =
            cursor.poll().expect("block-quote cursor")
        {
            lines.push(line);
        }
        assert_eq!(lines.len(), 3);
        assert!(lines[0].is_marked());
        assert!(lines[1].is_lazy());
        assert!(lines[2].is_marked());
        drop(cursor);

        assert!(matches!(
            host.begin_snapshot(exact.clone(), &begin),
            Err(M11InlineOverlayTransportError::StaleGeneration)
        ));
        drain_host(&mut host);
        drop(encoder);
        close_block_quote_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn ordered_item_schema3_authenticates_marker_facts_and_replays_one_line() {
        let profile = ParserProfileId::new(4).expect("profile");
        let source = "left\n\n007) first\n9) α😀\r\n42) last\n\nright";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut projection = build_ordered_item_projection(&mut runtime, profile);
        let exact = ordered_item_binding(base(&runtime, profile, 101, 23), 1, 2);
        let mut encoder = M11InlineOverlaySnapshotEncoder::authoritative_ordered_list_item(
            &runtime,
            exact.clone(),
            &projection,
            1,
            M11InlineOverlayCanonicalLineEnding::CrLf,
            0,
            2,
            9,
        )
        .expect("ordered-item encoder");
        let begin = encoder.begin_frame().expect("begin");
        let envelope_start = INLINE_OVERLAY_BEGIN_HEADER_BYTES;
        let envelope_end = envelope_start + M11_INLINE_OVERLAY_ENVELOPE_BYTES;
        let envelope = &begin[envelope_start..envelope_end];
        assert_eq!(envelope.len(), M11_INLINE_OVERLAY_ENVELOPE_BYTES);
        assert_eq!(
            read_u32(envelope, 4).expect("schema"),
            INLINE_OVERLAY_SCHEMA_ORDERED_ITEM
        );
        assert_eq!(read_u32(envelope, 68).expect("CRLF kind"), 9);
        assert_eq!(read_u32(envelope, 144).expect("disposition"), 1);
        assert_eq!(read_u32(envelope, 148).expect("ordinal wire"), 2);
        assert_eq!(read_u32(envelope, 152).expect("marker start"), 0);
        assert_eq!(read_u32(envelope, 156).expect("marker end"), 2);
        assert_eq!(read_u32(envelope, 160).expect("marker value"), 9);
        assert_eq!(&envelope[164..176], &[0; 12]);

        let bullet_layout = M11InlineOverlayEnvelope {
            binding: exact.clone(),
            disposition: M11InlineOverlayDisposition::Authoritative {
                projection_kind: M11InlineOverlayProjectionKind::BulletList,
                selected_item_ordinal: Some(1),
                selected_item_line_ending: Some(M11InlineOverlayCanonicalLineEnding::CrLf),
                ordered_item: None,
                logical_page_count: 1,
                fact_count: 1,
                storage_page_count: 1,
                ordered_commitment256: [0xa5; 32],
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
            },
        }
        .encode();
        assert_eq!(
            read_u32(&bullet_layout, 4).expect("bullet schema"),
            INLINE_OVERLAY_SCHEMA_TYPED
        );
        assert_eq!(read_u32(&bullet_layout, 68).expect("bullet CRLF kind"), 6);
        assert_eq!(read_u64(&bullet_layout, 152).expect("bullet pages"), 1);
        assert_eq!(read_u64(&bullet_layout, 160).expect("bullet facts"), 1);
        assert_eq!(read_u64(&bullet_layout, 168).expect("bullet storage"), 1);
        assert_eq!(&bullet_layout[176..208], &[0xa5; 32]);

        let rewrite_envelope_digest = |frame: &mut [u8]| {
            let body_start = INLINE_OVERLAY_BEGIN_HEADER_BYTES;
            let body_end = body_start + INLINE_OVERLAY_BODY_BYTES;
            let digest = envelope_digest(&frame[body_start..body_end]);
            frame[body_end..body_end + 32].copy_from_slice(&digest);
        };
        let mut host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());

        let mut malformed_marker = begin.to_vec();
        malformed_marker[envelope_start + 156..envelope_start + 160]
            .copy_from_slice(&1_u32.to_le_bytes());
        rewrite_envelope_digest(&mut malformed_marker);
        assert!(host
            .begin_snapshot(exact.clone(), &malformed_marker)
            .is_err());

        let mut malformed_reserved = begin.to_vec();
        malformed_reserved[envelope_start + 164] = 1;
        rewrite_envelope_digest(&mut malformed_reserved);
        assert!(host
            .begin_snapshot(exact.clone(), &malformed_reserved)
            .is_err());

        let mut cross_kind_descriptor = begin.to_vec();
        let descriptor_start = envelope_end;
        cross_kind_descriptor[descriptor_start + 68..descriptor_start + 72]
            .copy_from_slice(&(M11MarkedLineProjectionKind::BulletList as u32).to_le_bytes());
        assert!(host
            .begin_snapshot(exact.clone(), &cross_kind_descriptor)
            .is_err());

        let mut misplaced_encoder =
            M11InlineOverlaySnapshotEncoder::authoritative_ordered_list_item(
                &runtime,
                exact.clone(),
                &projection,
                1,
                M11InlineOverlayCanonicalLineEnding::CrLf,
                0,
                4,
                9,
            )
            .expect("shape-valid misplaced marker encoder");
        let misplaced_begin = misplaced_encoder.begin_frame().expect("misplaced begin");
        let mut misplaced_host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());
        misplaced_host
            .begin_snapshot(exact.clone(), &misplaced_begin)
            .expect("shape-valid metadata reaches typed validation");
        feed_snapshot(&runtime, &mut misplaced_host, &mut misplaced_encoder);
        loop {
            match misplaced_host.poll_install(1) {
                Ok(poll) if poll.installed => {
                    panic!("marker outside the authenticated hidden prefix was installed")
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        misplaced_host.begin_close().expect("close rejected host");
        while !misplaced_host.poll_close(1).expect("drain rejected host") {}
        drop(misplaced_encoder);

        host.begin_snapshot(exact.clone(), &begin)
            .expect("begin ordered host");
        feed_snapshot(&runtime, &mut host, &mut encoder);
        install_snapshot(&mut host);
        let M11InlineOverlayHostMatch::OrderedListAuthoritative {
            envelope,
            descriptor,
            mut cursor,
        } = host.query(&exact.query()).expect("query").expect("match")
        else {
            panic!("expected ordered-list sidecar");
        };
        let M11InlineOverlayDisposition::Authoritative {
            ordered_item: Some(item),
            ..
        } = envelope.disposition()
        else {
            panic!("ordered metadata missing");
        };
        assert_eq!(
            *item,
            M11InlineOverlayOrderedItem {
                selected_item_ordinal: 1,
                selected_item_line_ending: M11InlineOverlayCanonicalLineEnding::CrLf,
                opening_marker_start: 0,
                opening_marker_end: 2,
                marker_value: 9,
            }
        );
        assert_eq!(
            descriptor.projection_kind(),
            M11MarkedLineProjectionKind::OrderedList
        );
        assert_eq!(
            cursor.poll().expect("ordered line"),
            PersistentM11BlockQuoteProjectionHostCursorPoll::Line {
                line: BlockQuoteLineV1::ordered_item(11, 11, 3, 0, 3, 6, 3)
                    .expect("expected ordered line"),
            }
        );
        assert!(matches!(
            cursor.poll().expect("ordered complete"),
            PersistentM11BlockQuoteProjectionHostCursorPoll::Complete
        ));
        drop(cursor);

        let public_binding = M11HostInlineSidecarBinding::from_engine_test(exact);
        let mut public_host = M11HostInlineSidecar::from_engine_test(host);
        let M11HostInlineSidecarQuery::OrderedList {
            selected_item_ordinal,
            selected_item_line_ending,
            opening_marker_start,
            opening_marker_end,
            marker_value,
            descriptor,
            mut cursor,
        } = public_host
            .query(&public_binding)
            .expect("public query")
            .expect("public match")
        else {
            panic!("expected public ordered-list sidecar");
        };
        assert_eq!(selected_item_ordinal, 1);
        assert_eq!(selected_item_line_ending, M11HostCanonicalLineEnding::CrLf);
        assert_eq!((opening_marker_start, opening_marker_end), (0, 2));
        assert_eq!(marker_value, 9);
        assert_eq!(
            (descriptor.logical_page_count(), descriptor.line_count()),
            (1, 1)
        );
        let M11HostBlockQuoteCursorPoll::Line(line) = cursor.poll().expect("public ordered line")
        else {
            panic!("expected public ordered line");
        };
        assert_eq!(line.ordered_content_utf16_length(), 3);
        assert!(matches!(
            cursor.poll().expect("public ordered complete"),
            M11HostBlockQuoteCursorPoll::Complete
        ));
        drop(cursor);

        public_host.begin_close().expect("begin public host close");
        while !public_host.poll_close(1).expect("poll public host close") {}
        drop(encoder);
        close_block_quote_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn sidecar_begin_and_closure_tamper_wrong_fence_and_stale_generation_fail_closed() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let exact = binding(base(&runtime, profile, 81, 14), 2, 2);
        let mut encoder =
            M11InlineOverlaySnapshotEncoder::authoritative(&runtime, exact.clone(), &projection)
                .expect("encoder");
        let begin = encoder.begin_frame().expect("begin");
        let mut host =
            M11InlineOverlayHostStore::new(exact.base.clone(), CandidateHostLimits::default());

        let mut tampered_hio = begin.clone();
        tampered_hio[INLINE_OVERLAY_BEGIN_HEADER_BYTES + 176] ^= 0x80;
        assert!(host.begin_snapshot(exact.clone(), &tampered_hio).is_err());

        let wrong_ordinal = binding(exact.base.clone(), 2, 3);
        assert!(host.begin_snapshot(wrong_ordinal, &begin).is_err());
        let wrong_range = M11InlineOverlayBinding::new(
            exact.base.clone(),
            2,
            M11InlineOverlayOwner::BlockOrdinal(2),
            6..17,
            7..16,
            6..17,
            7..16,
        )
        .expect("wrong range");
        assert!(host.begin_snapshot(wrong_range, &begin).is_err());

        let mut tampered_ipr2 = begin.clone();
        let descriptor_offset =
            INLINE_OVERLAY_BEGIN_HEADER_BYTES + M11_INLINE_OVERLAY_ENVELOPE_BYTES;
        tampered_ipr2[descriptor_offset + 128] ^= 0x20;
        assert!(host.begin_snapshot(exact.clone(), &tampered_ipr2).is_err());
        let mut wrong_profile_ipr2 = begin.clone();
        wrong_profile_ipr2[descriptor_offset + 40..descriptor_offset + 48]
            .copy_from_slice(&2_u64.to_le_bytes());
        assert!(host
            .begin_snapshot(exact.clone(), &wrong_profile_ipr2)
            .is_err());

        host.begin_snapshot(exact.clone(), &begin)
            .expect("exact begin");
        let first_node = loop {
            match encoder.poll(&runtime, 64).expect("encode node") {
                M11InlineOverlaySnapshotEncodePoll::Pending { .. } => {}
                M11InlineOverlaySnapshotEncodePoll::Frame { bytes, .. } => break bytes,
                M11InlineOverlaySnapshotEncodePoll::Complete { .. } => {
                    panic!("projection closure unexpectedly empty")
                }
            }
        };
        let mut tampered_node = first_node.clone();
        *tampered_node.last_mut().expect("payload") ^= 0x40;
        host.offer_node(&tampered_node)
            .expect("shape-valid tampered node enters staging only");
        loop {
            match encoder
                .poll(&runtime, 64)
                .expect("continue tampered stream")
            {
                M11InlineOverlaySnapshotEncodePoll::Pending { .. } => {}
                M11InlineOverlaySnapshotEncodePoll::Frame { bytes, .. } => {
                    host.offer_node(&bytes).expect("remaining node");
                }
                M11InlineOverlaySnapshotEncodePoll::Complete { bytes, .. } => {
                    assert!(host.finish_snapshot(&bytes).is_err());
                    break;
                }
            }
        }
        while !host.poll_retire(1).expect("discard").complete {}

        let mut clean_encoder =
            M11InlineOverlaySnapshotEncoder::authoritative(&runtime, exact.clone(), &projection)
                .expect("clean encoder");
        let clean_begin = clean_encoder.begin_frame().expect("clean begin");
        host.begin_snapshot(exact.clone(), &clean_begin)
            .expect("begin clean");
        feed_snapshot(&runtime, &mut host, &mut clean_encoder);
        install_snapshot(&mut host);

        let stale = binding(exact.base.clone(), 1, 2);
        let mut stale_encoder =
            M11InlineOverlaySnapshotEncoder::authoritative(&runtime, stale.clone(), &projection)
                .expect("stale encoder");
        let stale_begin = stale_encoder.begin_frame().expect("stale begin");
        assert!(matches!(
            host.begin_snapshot(stale, &stale_begin),
            Err(M11InlineOverlayTransportError::StaleGeneration)
        ));

        drain_host(&mut host);
        drop(stale_encoder);
        drop(clean_encoder);
        drop(encoder);
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn unsupported_metadata_replaces_authoritative_and_close_reclaims_both_arenas() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let exact_base = base(&runtime, profile, 91, 15);
        let first = binding(exact_base.clone(), 1, 2);
        let mut authoritative =
            M11InlineOverlaySnapshotEncoder::authoritative(&runtime, first.clone(), &projection)
                .expect("authoritative");
        let first_begin = authoritative.begin_frame().expect("first begin");
        let mut host =
            M11InlineOverlayHostStore::new(exact_base.clone(), CandidateHostLimits::default());
        host.begin_snapshot(first, &first_begin)
            .expect("first host");
        feed_snapshot(&runtime, &mut host, &mut authoritative);
        install_snapshot(&mut host);

        let unsupported_binding = binding(exact_base, 2, 2);
        let metadata = b"schema-2 unsupported inline facts"
            .to_vec()
            .into_boxed_slice();
        let mut unsupported = M11InlineOverlaySnapshotEncoder::unsupported(
            &runtime,
            unsupported_binding.clone(),
            7,
            metadata.clone(),
        )
        .expect("unsupported");
        let unsupported_begin = unsupported.begin_frame().expect("unsupported begin");
        host.begin_snapshot(unsupported_binding.clone(), &unsupported_begin)
            .expect("unsupported host");
        feed_snapshot(&runtime, &mut host, &mut unsupported);
        install_snapshot(&mut host);
        let M11InlineOverlayHostMatch::Unsupported {
            envelope,
            metadata: installed_metadata,
        } = host
            .query(&unsupported_binding.query())
            .expect("unsupported query")
            .expect("unsupported match")
        else {
            panic!("expected unsupported terminal");
        };
        assert_eq!(installed_metadata, metadata.as_ref());
        envelope
            .validate_unsupported_metadata(installed_metadata)
            .expect("metadata commitment");
        while !host.poll_retire(1).expect("old root retire").complete {}

        drain_host(&mut host);
        drop(unsupported);
        drop(authoritative);
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn exact_middle_paragraph_envelope_installs_and_queries() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let binding = binding(base(&runtime, profile, 41, 7), 1, 2);
        let envelope =
            M11InlineOverlayEnvelope::from_projection(binding.clone(), &projection).expect("env");
        let encoded = envelope.encode();
        let decoded = M11InlineOverlayEnvelope::decode_exact(&encoded, &binding).expect("decode");
        assert_eq!(decoded, envelope);

        let residents = Rc::new(Cell::new(0));
        let mut slot = M11InlineOverlaySlot::new(binding.base.clone());
        slot.install_authoritative(decoded, TestOwner::new(11, 2, &residents))
            .unwrap_or_else(|_| panic!("install"));
        let M11InlineOverlayMatch::Authoritative { owner, .. } =
            slot.query(&binding.query()).expect("matching overlay")
        else {
            panic!("expected authoritative overlay");
        };
        assert_eq!(owner.id, 11);

        slot.begin_close().expect("begin close");
        while !slot.poll_retire(1).expect("retire").complete {}
        assert!(slot.is_empty());
        assert_eq!(residents.get(), 0);
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn stale_wrong_authority_range_profile_ordinal_and_tamper_fail_closed() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let exact = binding(base(&runtime, profile, 51, 8), 2, 2);
        let envelope =
            M11InlineOverlayEnvelope::from_projection(exact.clone(), &projection).expect("env");
        let residents = Rc::new(Cell::new(0));
        let mut slot = M11InlineOverlaySlot::new(exact.base.clone());
        slot.install_authoritative(envelope.clone(), TestOwner::new(20, 1, &residents))
            .unwrap_or_else(|_| panic!("install"));

        let stale = M11InlineOverlayEnvelope::from_projection(
            binding(exact.base.clone(), 1, 2),
            &projection,
        )
        .expect("stale envelope");
        let failure = slot
            .install_authoritative(stale, TestOwner::new(21, 1, &residents))
            .expect_err("stale");
        assert!(matches!(
            failure.error,
            M11InlineOverlaySlotError::StaleGeneration
        ));
        drop(failure.owner.expect("returned stale owner"));

        let other_base = base(&runtime, profile, 52, 9);
        let foreign =
            M11InlineOverlayEnvelope::from_projection(binding(other_base, 3, 2), &projection)
                .expect("foreign envelope");
        let failure = slot
            .install_authoritative(foreign, TestOwner::new(22, 1, &residents))
            .expect_err("foreign base");
        assert!(matches!(
            failure.error,
            M11InlineOverlaySlotError::BaseMismatch
        ));
        drop(failure.owner.expect("returned foreign owner"));

        let wrong_range = M11InlineOverlayBinding::new(
            exact.base.clone(),
            3,
            M11InlineOverlayOwner::BlockOrdinal(2),
            6..17,
            7..16,
            6..17,
            7..16,
        )
        .expect("shape-valid wrong range");
        assert!(matches!(
            M11InlineOverlayEnvelope::from_projection(wrong_range, &projection),
            Err(M11InlineOverlayError::ProjectionMismatch)
        ));

        let wrong_profile = ParserProfileId::new(2).expect("profile");
        let wrong_profile_base = base(&runtime, wrong_profile, 53, 10);
        assert!(matches!(
            M11InlineOverlayEnvelope::from_projection(
                binding(wrong_profile_base, 3, 2),
                &projection
            ),
            Err(M11InlineOverlayError::ProjectionMismatch)
        ));

        let wrong_ordinal = binding(exact.base.clone(), 2, 3).query();
        assert!(slot.query(&wrong_ordinal).is_none());
        let M11InlineOverlayMatch::Authoritative { owner, .. } =
            slot.query(&exact.query()).expect("active exact")
        else {
            panic!("expected authoritative overlay");
        };
        assert_eq!(owner.id, 20);

        let mut tampered = envelope.encode();
        tampered[152] ^= 0x80;
        assert!(matches!(
            M11InlineOverlayEnvelope::decode_exact(&tampered, &exact),
            Err(M11InlineOverlayError::MalformedEnvelope)
        ));
        let M11InlineOverlayMatch::Authoritative { owner, .. } =
            slot.query(&exact.query()).expect("still active")
        else {
            panic!("expected authoritative overlay");
        };
        assert_eq!(owner.id, 20);

        let next_base = base(&runtime, profile, 54, 11);
        assert!(slot.observe_base(next_base).expect("invalidate"));
        assert!(slot.query(&exact.query()).is_none());
        while !slot.poll_retire(1).expect("retire").complete {}
        assert_eq!(residents.get(), 0);
        slot.begin_close().expect("close empty");
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn latest_replacement_is_active_while_predecessor_retires_then_closes_to_zero() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let base = base(&runtime, profile, 61, 12);
        let first_binding = binding(base.clone(), 1, 2);
        let second_binding = binding(base.clone(), 2, 2);
        let first = M11InlineOverlayEnvelope::from_projection(first_binding.clone(), &projection)
            .expect("first");
        let second = M11InlineOverlayEnvelope::from_projection(second_binding.clone(), &projection)
            .expect("second");
        let residents = Rc::new(Cell::new(0));
        let mut slot = M11InlineOverlaySlot::new(base);
        slot.install_authoritative(first, TestOwner::new(31, 3, &residents))
            .unwrap_or_else(|_| panic!("first install"));
        slot.install_authoritative(second, TestOwner::new(32, 2, &residents))
            .unwrap_or_else(|_| panic!("replacement"));
        assert!(slot.has_retiring());
        let M11InlineOverlayMatch::Authoritative { owner, .. } =
            slot.query(&second_binding.query()).expect("latest")
        else {
            panic!("expected authoritative overlay");
        };
        assert_eq!(owner.id, 32);
        assert_eq!(residents.get(), 2);

        while !slot.poll_retire(1).expect("old retire").complete {}
        assert_eq!(residents.get(), 1);
        let unsupported_binding = binding(second_binding.base.clone(), 3, 2);
        let unsupported =
            M11InlineOverlayEnvelope::unsupported(unsupported_binding.clone(), 9, b"raw html")
                .expect("unsupported certificate");
        let encoded = unsupported.encode();
        assert_eq!(
            M11InlineOverlayEnvelope::decode_exact(&encoded, &unsupported_binding)
                .expect("unsupported decode"),
            unsupported
        );
        slot.install_unsupported(unsupported, TestOwner::new(33, 1, &residents))
            .unwrap_or_else(|_| panic!("unsupported replacement"));
        assert!(matches!(
            slot.query(&unsupported_binding.query()),
            Some(M11InlineOverlayMatch::Unsupported { .. })
        ));
        while !slot.poll_retire(1).expect("authoritative retire").complete {}
        assert_eq!(residents.get(), 1);
        slot.begin_close().expect("begin close");
        while !slot.poll_retire(1).expect("unsupported retire").complete {}
        assert!(slot.is_empty());
        assert_eq!(residents.get(), 0);
        close_projection(&mut runtime, &mut projection);
    }

    #[test]
    fn close_serializes_active_overlay_behind_retiring_predecessor() {
        let profile = ParserProfileId::new(1).expect("profile");
        let mut runtime = DocumentRuntime::new(
            "left\n\n**middle**\n\nright",
            DocumentRuntimeConfig::default(),
        )
        .expect("runtime");
        let mut projection = build_middle_projection(&mut runtime, profile);
        let base = base(&runtime, profile, 62, 13);
        let first_binding = binding(base.clone(), 1, 2);
        let second_binding = binding(base.clone(), 2, 2);
        let first =
            M11InlineOverlayEnvelope::from_projection(first_binding, &projection).expect("first");
        let second =
            M11InlineOverlayEnvelope::from_projection(second_binding, &projection).expect("second");
        let residents = Rc::new(Cell::new(0));
        let mut slot = M11InlineOverlaySlot::new(base);
        slot.install_authoritative(first, TestOwner::new(41, 3, &residents))
            .unwrap_or_else(|_| panic!("first install"));
        slot.install_authoritative(second, TestOwner::new(42, 2, &residents))
            .unwrap_or_else(|_| panic!("replacement"));

        assert!(slot.has_retiring());
        assert_eq!(residents.get(), 2);
        slot.begin_close()
            .expect("close must retain both owners for serial retirement");
        while !slot.poll_retire(1).expect("close retirement").complete {}

        assert!(slot.is_empty());
        assert_eq!(residents.get(), 0);
        close_projection(&mut runtime, &mut projection);
    }
}
