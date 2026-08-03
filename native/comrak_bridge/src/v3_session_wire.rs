//! Version-3 parser-session payloads for the native and Wasm worker endpoints.
//!
//! The endpoint-critical direction is deliberately small: Dart commands are
//! decoded into borrowed, bounded values and parser events are encoded into
//! caller-owned storage. Publication payloads remain owned by their separate
//! codec and are rejected before this module reads a session payload header.

use std::{fmt, str};

use flark_engine::SOURCE_FACT_ROOT_DEFAULT_MAX_PAGES;

use crate::v3_publication_wire::{structural_ack_is_valid, SourceVersion, StructuralAck};
use crate::v3_wire::{self, DecodeLimits, FrameKind, Header, Opcode, Status};

pub const PAYLOAD_SCHEMA: u16 = 3;
pub const MAXIMUM_SNAPSHOT_UTF16: u32 = 8_192;
pub const MAXIMUM_INTENT_COUNT: u32 = 64;
pub const MAXIMUM_OPERATION_COUNT: u32 = 1_024;
pub const MAXIMUM_INTENT_PAYLOAD_UTF16: u32 = 8_192;
pub const MAXIMUM_DRAIN_TRANSITIONS: u32 = 256;
pub const MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS: u32 = 64;
pub const PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM: u32 = 2;
pub const MAXIMUM_VIEWPORT_STRUCTURAL_ENTRIES: u32 = 256;
pub const MAXIMUM_VIEWPORT_STORAGE_PAGES: u32 = 257;
pub const MAXIMUM_VIEWPORT_INLINE_LEAVES: u32 = 128;
pub const MAXIMUM_VIEWPORT_INLINE_LEAF_SOURCE_BYTES: u32 = 8 * 1024;
pub const MAXIMUM_VIEWPORT_INLINE_SOURCE_BYTES: u32 = 1024 * 1024;
pub const MAXIMUM_VIEWPORT_FACT_RECORDS: u32 = 2_048;
pub const MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES: u32 = 4 * 1024 * 1024;
pub const MAXIMUM_VIEWPORT_PARSER_TRANSITIONS: u32 = 1_000_000;

const COMMON_BYTES: usize = 28;
const SOURCE_STAMP_BYTES: usize = 32;
#[cfg(test)]
const OBSERVED_REPLICA_BYTES: usize = 20;
const SOURCE_FACT_CHECKPOINT_BYTES: usize = 28;
const MAXIMUM_EVENT_PAYLOAD_BYTES: usize = COMMON_BYTES
    + 40
    + MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS as usize * SOURCE_FACT_CHECKPOINT_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionBinding {
    pub document_session: [u32; 4],
    pub source_session_identity: u32,
    pub worker_generation: u32,
}

impl SessionBinding {
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.source_session_identity != 0 && self.worker_generation != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    Fresh,
    Recovery,
}

/// Dart-declared identity of one exact source target.
///
/// A known stamp carries the Dart source authority's digest. Decoding it does
/// not certify that the worker installed matching bytes; worker observation is
/// reported separately through [`ObservedSourceReplicaVersion`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStamp {
    Provisional {
        revision: u32,
        utf16_length: u32,
    },
    Known {
        revision: u32,
        utf16_length: u32,
        utf8_length: u32,
        content_hash128: [u32; 4],
    },
}

impl SourceStamp {
    #[must_use]
    pub const fn revision(self) -> u32 {
        match self {
            Self::Provisional { revision, .. } | Self::Known { revision, .. } => revision,
        }
    }

    #[must_use]
    pub const fn utf16_length(self) -> u32 {
        match self {
            Self::Provisional { utf16_length, .. } | Self::Known { utf16_length, .. } => {
                utf16_length
            }
        }
    }
}

/// Worker-observed dimensions of one fully installed replica.
///
/// This intentionally contains no source hash and is not source-fingerprint
/// certification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservedSourceReplicaVersion {
    pub revision: u32,
    pub utf16_length: u32,
    pub utf8_length: u32,
    pub intent_high_water: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotCommand<'payload> {
    pub binding: SessionBinding,
    pub lease_id: u32,
    pub base_ui_revision: u32,
    pub start_utf16: u32,
    pub end_utf16: u32,
    pub total_utf16_length: u32,
    pub through_intent_sequence: u32,
    pub target_stamp: SourceStamp,
    pub source: &'payload str,
}

impl SnapshotCommand<'_> {
    #[must_use]
    pub const fn is_seed(self) -> bool {
        self.start_utf16 == 0
    }

    /// Produces the only acknowledgement that can return this lease credit.
    #[must_use]
    pub const fn acknowledgement(
        self,
        observed_replica: Option<ObservedSourceReplicaVersion>,
    ) -> SourceAcknowledgement {
        SourceAcknowledgement::Snapshot {
            source_session_identity: self.binding.source_session_identity,
            lease_id: self.lease_id,
            worker_generation: self.binding.worker_generation,
            base_ui_revision: self.base_ui_revision,
            start_utf16: self.start_utf16,
            end_utf16: self.end_utf16,
            through_intent_sequence: self.through_intent_sequence,
            observed_replica,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditCommand<'payload> {
    pub binding: SessionBinding,
    pub lease_id: u32,
    pub first_sequence: u32,
    pub last_sequence: u32,
    pub intent_count: u32,
    pub operation_count: u32,
    pub payload_utf16: u32,
    pub payload_utf8_bytes: u32,
    body: &'payload [u8],
}

impl<'payload> EditCommand<'payload> {
    /// Iterates the already-validated borrowed edit body without allocating.
    #[must_use]
    pub fn intents(self) -> EditIntentIterator<'payload> {
        EditIntentIterator {
            bytes: self.body,
            offset: 0,
            remaining: self.intent_count,
        }
    }

    /// Returns the exact declared base stamp of the first intent.
    #[must_use]
    pub fn base_stamp(self) -> SourceStamp {
        self.intents()
            .next()
            .expect("validated edit command always has an intent")
            .base_stamp
    }

    /// Returns the exact declared target stamp of the last intent.
    #[must_use]
    pub fn target_stamp(self) -> SourceStamp {
        self.intents()
            .last()
            .expect("validated edit command always has an intent")
            .target_stamp
    }

    /// Produces the only acknowledgement that can return this lease credit.
    #[must_use]
    pub const fn acknowledgement(
        self,
        observed_replica: ObservedSourceReplicaVersion,
    ) -> SourceAcknowledgement {
        SourceAcknowledgement::Edit {
            source_session_identity: self.binding.source_session_identity,
            lease_id: self.lease_id,
            worker_generation: self.binding.worker_generation,
            first_sequence: self.first_sequence,
            last_sequence: self.last_sequence,
            entry_count: self.intent_count,
            payload_utf16: self.payload_utf16,
            observed_replica,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditIntent<'payload> {
    pub sequence: u32,
    pub base_ui_revision: u32,
    pub ui_revision: u32,
    pub base_stamp: SourceStamp,
    pub target_stamp: SourceStamp,
    pub operations: EditOperationIterator<'payload>,
}

#[derive(Clone, Debug)]
pub struct EditIntentIterator<'payload> {
    bytes: &'payload [u8],
    offset: usize,
    remaining: u32,
}

impl<'payload> Iterator for EditIntentIterator<'payload> {
    type Item = EditIntent<'payload>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let sequence = read_u32_at(self.bytes, self.offset);
        let base_ui_revision = read_u32_at(self.bytes, self.offset + 4);
        let ui_revision = read_u32_at(self.bytes, self.offset + 8);
        let operation_count = read_u32_at(self.bytes, self.offset + 12);
        let base_stamp = read_validated_source_stamp_at(self.bytes, self.offset + 16);
        let target_stamp =
            read_validated_source_stamp_at(self.bytes, self.offset + 16 + SOURCE_STAMP_BYTES);
        let operation_start = self.offset + 16 + SOURCE_STAMP_BYTES * 2;
        let mut operation_end = operation_start;
        for _ in 0..operation_count {
            let replacement_bytes = read_u32_at(self.bytes, operation_end + 12) as usize;
            operation_end += 16 + replacement_bytes;
        }
        self.offset = operation_end;
        self.remaining -= 1;
        Some(EditIntent {
            sequence,
            base_ui_revision,
            ui_revision,
            base_stamp,
            target_stamp,
            operations: EditOperationIterator {
                bytes: &self.bytes[operation_start..operation_end],
                offset: 0,
                remaining: operation_count,
            },
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for EditIntentIterator<'_> {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditOperation<'payload> {
    pub start_utf16: u32,
    pub end_utf16: u32,
    pub replacement_utf16: u32,
    pub replacement: &'payload str,
}

#[derive(Clone, Debug)]
pub struct EditOperationIterator<'payload> {
    bytes: &'payload [u8],
    offset: usize,
    remaining: u32,
}

impl<'payload> Iterator for EditOperationIterator<'payload> {
    type Item = EditOperation<'payload>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let start_utf16 = read_u32_at(self.bytes, self.offset);
        let end_utf16 = read_u32_at(self.bytes, self.offset + 4);
        let replacement_utf16 = read_u32_at(self.bytes, self.offset + 8);
        let replacement_bytes = read_u32_at(self.bytes, self.offset + 12) as usize;
        let start = self.offset + 16;
        let end = start + replacement_bytes;
        // The only constructor validates this exact byte range as strict UTF-8.
        let replacement = str::from_utf8(&self.bytes[start..end])
            .expect("validated session edit replacement must remain UTF-8");
        self.offset = end;
        self.remaining -= 1;
        Some(EditOperation {
            start_utf16,
            end_utf16,
            replacement_utf16,
            replacement,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for EditOperationIterator<'_> {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum EventDisposition {
    Accepted = 0,
    Stale = 1,
    Rejected = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SourceReceiptDisposition {
    Acknowledged = 0,
    Stale = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceReceipt {
    pub disposition: SourceReceiptDisposition,
    pub dropped_intent_entries: u32,
    pub dropped_payload_utf16: u32,
    pub dropped_deleted_utf16: u32,
    pub dropped_operation_count: u32,
    pub worker_revision: u32,
}

/// Exact Dart-side proof that one canonical fact root was promoted and adopted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCertificationReceipt {
    pub certification_id: u32,
    pub worker_replica_revision: u32,
    pub ui_revision: u32,
    pub utf16_length: u32,
    pub intent_high_water: u32,
    pub fingerprint_algorithm: u32,
    pub utf8_length: u32,
    pub logical_line_breaks: u32,
    pub checkpoint_spacing_utf16: u32,
    pub checkpoint_count: u32,
    pub page_count: u32,
    pub content_hash128: [u32; 4],
    pub checkpoint_hash128: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventReceiptCommand {
    pub binding: SessionBinding,
    pub event_id: u32,
    pub disposition: EventDisposition,
    pub source: Option<SourceReceipt>,
    pub certification: Option<SourceCertificationReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrainGrant {
    pub binding: SessionBinding,
    pub drain_id: u32,
    pub maximum_transitions: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupersedeCommand {
    pub binding: SessionBinding,
    pub target_ui_revision: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlinePointAffinity {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineRefinementTarget {
    Automatic,
    BulletListItemInline,
    BulletListItemProjection,
    OrderedListItemInline,
    OrderedListItemProjection,
    /// Experimental bounded bridge through the recursive-Green Paragraph
    /// owner query. The established sidecar schema remains unchanged.
    RecursiveGreenParagraph,
    BlockQuoteProjection,
}

/// One late inline demand fenced to an exact installed structural base.
///
/// The source version is repeated outside `base_ack` intentionally: it is the
/// caller's current source claim, while `base_ack` is the independently
/// adopted structural authority against which the producer must validate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineRefinementCommand {
    pub binding: SessionBinding,
    pub refinement_generation: u32,
    pub source_version: SourceVersion,
    pub base_ack: StructuralAck,
    pub byte_offset: u32,
    pub utf16_offset: u32,
    pub affinity: InlinePointAffinity,
    pub target: InlineRefinementTarget,
}

/// Caller-owned hard bounds for one passive viewport-presentation batch.
///
/// These are lifetime bounds for the whole batch. The producer still applies
/// the existing per-poll transition ceiling while deriving each leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationLimits {
    pub maximum_structural_entries: u32,
    pub maximum_storage_pages: u32,
    pub maximum_inline_leaves: u32,
    pub maximum_inline_leaf_source_bytes: u32,
    pub maximum_inline_source_bytes: u32,
    pub maximum_fact_records: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_parser_transitions: u32,
}

/// One passive-presentation demand fenced to an exact installed structural
/// base and one authenticated measured-sequence start.
///
/// The requested range is the exact structural-page coverage that Dart has
/// already materialized. The producer must finish at `requested_end`; it may
/// not silently skip an admitted leaf or reinterpret this as a point query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationCommand {
    pub binding: SessionBinding,
    pub viewport_generation: u32,
    pub source_version: SourceVersion,
    pub base_ack: StructuralAck,
    pub requested_start_utf8: u32,
    pub requested_start_utf16: u32,
    pub requested_end_utf8: u32,
    pub requested_end_utf16: u32,
    pub start_block_ordinal: u64,
    pub start_utf8: u32,
    pub start_utf16: u32,
    pub limits: ViewportPresentationLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command<'payload> {
    Open {
        binding: SessionBinding,
        mode: OpenMode,
    },
    Snapshot(SnapshotCommand<'payload>),
    Edit(EditCommand<'payload>),
    RefineInline(InlineRefinementCommand),
    PresentViewport(ViewportPresentationCommand),
    Supersede(SupersedeCommand),
    EventReceipt(EventReceiptCommand),
    BeginClose {
        binding: SessionBinding,
        active_generation: u32,
    },
    Drain(DrainGrant),
}

impl Command<'_> {
    #[must_use]
    pub const fn binding(self) -> SessionBinding {
        match self {
            Self::Open { binding, .. } | Self::BeginClose { binding, .. } => binding,
            Self::Snapshot(command) => command.binding,
            Self::Edit(command) => command.binding,
            Self::RefineInline(command) => command.binding,
            Self::PresentViewport(command) => command.binding,
            Self::Supersede(command) => command.binding,
            Self::EventReceipt(command) => command.binding,
            Self::Drain(command) => command.binding,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedCommand<'payload> {
    pub correlation_id: u32,
    pub command: Command<'payload>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeFailure {
    Envelope(v3_wire::DecodeFailure),
    UnsupportedSchema,
    UnexpectedOpcode,
    UnknownVariant,
    TruncatedPayload,
    TrailingPayload,
    InvalidValue,
    OversizedValue,
    IdentityMismatch,
    InvalidUtf8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub failure: DecodeFailure,
    pub byte_offset: usize,
    pub expected: Option<usize>,
    pub actual: Option<usize>,
}

impl DecodeError {
    const fn session(
        failure: DecodeFailure,
        byte_offset: usize,
        expected: Option<usize>,
        actual: Option<usize>,
    ) -> Self {
        Self {
            failure,
            byte_offset,
            expected,
            actual,
        }
    }

    const fn envelope(error: v3_wire::DecodeError) -> Self {
        Self {
            failure: DecodeFailure::Envelope(error.failure),
            byte_offset: error.byte_offset,
            expected: error.expected,
            actual: error.actual,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid Flark v3 parser session: {:?}",
            self.failure
        )
    }
}

impl std::error::Error for DecodeError {}

/// Decodes one Dart-to-worker command while borrowing all source text.
///
/// Validation order is contractual: envelope, positive correlation,
/// direction-owned opcode, common header, bounded body, exact exhaustion,
/// binding transition, then correlation identity.
pub fn decode_command(
    bytes: &[u8],
    established_binding: Option<SessionBinding>,
) -> Result<DecodedCommand<'_>, DecodeError> {
    let frame = v3_wire::decode(bytes, FrameKind::Request, DecodeLimits::default())
        .map_err(DecodeError::envelope)?;
    if frame.header.correlation_id == 0 {
        return Err(DecodeError::session(
            DecodeFailure::InvalidValue,
            16,
            Some(1),
            Some(0),
        ));
    }
    require_command_opcode(frame.header.opcode)?;

    let mut reader = PayloadReader::new(frame.payload);
    let payload_header = read_payload_header(&mut reader)?;
    let command = match frame.header.opcode {
        Opcode::ParserOpen => match payload_header.variant {
            0 => Command::Open {
                binding: payload_header.binding,
                mode: OpenMode::Fresh,
            },
            1 => Command::Open {
                binding: payload_header.binding,
                mode: OpenMode::Recovery,
            },
            variant => return Err(unknown_variant(variant)),
        },
        Opcode::SnapshotPage => {
            let seed = match payload_header.variant {
                0 => true,
                1 => false,
                variant => return Err(unknown_variant(variant)),
            };
            Command::Snapshot(read_snapshot(&mut reader, payload_header.binding, seed)?)
        }
        Opcode::Edit => {
            if payload_header.variant != 0 {
                return Err(unknown_variant(payload_header.variant));
            }
            Command::Edit(read_edit(&mut reader, payload_header.binding)?)
        }
        Opcode::ParserRefineInline => {
            if payload_header.variant != 0 {
                return Err(unknown_variant(payload_header.variant));
            }
            Command::RefineInline(read_inline_refinement(&mut reader, payload_header.binding)?)
        }
        Opcode::ParserPresentViewport => {
            if payload_header.variant != 0 {
                return Err(unknown_variant(payload_header.variant));
            }
            Command::PresentViewport(read_viewport_presentation(
                &mut reader,
                payload_header.binding,
            )?)
        }
        Opcode::Supersede => {
            if payload_header.variant != 0 {
                return Err(unknown_variant(payload_header.variant));
            }
            Command::Supersede(SupersedeCommand {
                binding: payload_header.binding,
                target_ui_revision: reader.u32()?,
            })
        }
        Opcode::ParserAcknowledge => Command::EventReceipt(read_receipt(
            &mut reader,
            payload_header,
            frame.header.correlation_id,
        )?),
        Opcode::Close => {
            if payload_header.variant != 0 {
                return Err(unknown_variant(payload_header.variant));
            }
            let active_generation = reader.u32()?;
            if active_generation != 0
                && active_generation != payload_header.binding.worker_generation
            {
                return Err(identity(
                    reader.offset,
                    active_generation,
                    payload_header.binding.worker_generation,
                ));
            }
            Command::BeginClose {
                binding: payload_header.binding,
                active_generation,
            }
        }
        Opcode::Drain => {
            if payload_header.variant != 0 {
                return Err(unknown_variant(payload_header.variant));
            }
            let drain_id = reader.u32()?;
            let maximum_transitions = reader.u32()?;
            if drain_id == 0
                || maximum_transitions == 0
                || maximum_transitions > MAXIMUM_DRAIN_TRANSITIONS
            {
                return Err(invalid(reader.offset, None, None));
            }
            Command::Drain(DrainGrant {
                binding: payload_header.binding,
                drain_id,
                maximum_transitions,
            })
        }
        _ => unreachable!("direction-owned opcode guard must be exhaustive"),
    };
    reader.finish()?;
    validate_transition(command, established_binding, reader.offset)?;
    match command {
        Command::Snapshot(snapshot) if snapshot.lease_id != frame.header.correlation_id => {
            return Err(identity(
                reader.offset,
                frame.header.correlation_id,
                snapshot.lease_id,
            ));
        }
        Command::Edit(edit) if edit.lease_id != frame.header.correlation_id => {
            return Err(identity(
                reader.offset,
                frame.header.correlation_id,
                edit.lease_id,
            ));
        }
        Command::Drain(grant) if grant.drain_id != frame.header.correlation_id => {
            return Err(identity(
                reader.offset,
                frame.header.correlation_id,
                grant.drain_id,
            ));
        }
        _ => {}
    }
    Ok(DecodedCommand {
        correlation_id: frame.header.correlation_id,
        command,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceAcknowledgement {
    Snapshot {
        source_session_identity: u32,
        lease_id: u32,
        worker_generation: u32,
        base_ui_revision: u32,
        start_utf16: u32,
        end_utf16: u32,
        through_intent_sequence: u32,
        observed_replica: Option<ObservedSourceReplicaVersion>,
    },
    Edit {
        source_session_identity: u32,
        lease_id: u32,
        worker_generation: u32,
        first_sequence: u32,
        last_sequence: u32,
        entry_count: u32,
        payload_utf16: u32,
        observed_replica: ObservedSourceReplicaVersion,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrainProgress {
    pub drain_id: u32,
    pub released_source_leases: u32,
    pub released_source_bytes: u32,
    pub arena_transitions: u32,
    pub arena_nodes_reclaimed: u32,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceFactCheckpointWire {
    pub byte_offset: u32,
    pub utf16_offset: u32,
    pub logical_line_breaks: u32,
    pub rolling_hash128: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFactsPageEvent {
    pub certification_id: u32,
    pub worker_replica_revision: u32,
    pub ui_revision: u32,
    pub utf16_length: u32,
    pub intent_high_water: u32,
    pub checkpoint_spacing_utf16: u32,
    pub page_ordinal: u32,
    pub page_count: u32,
    pub checkpoint_count: u32,
    pub page_checkpoint_count: u32,
    pub checkpoints: [SourceFactCheckpointWire; MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS as usize],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFactsCompletionEvent {
    pub certification_id: u32,
    pub worker_replica_revision: u32,
    pub ui_revision: u32,
    pub utf16_length: u32,
    pub intent_high_water: u32,
    pub fingerprint_algorithm: u32,
    pub utf8_length: u32,
    pub logical_line_breaks: u32,
    pub checkpoint_spacing_utf16: u32,
    pub checkpoint_count: u32,
    pub page_count: u32,
    pub content_hash128: [u32; 4],
    pub checkpoint_hash128: [u32; 4],
}

/// Authenticated header for one exact-base canonical SourceFacts splice.
///
/// The base page range and target page range are half-open canonical page
/// ordinals. Replacement pages themselves use relative ordinals on the wire
/// so Dart can stage a compact zero-based stream without losing the absolute
/// target range authenticated here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFactsDeltaBeginEvent {
    pub certification_id: u32,
    pub worker_replica_revision: u32,
    pub ui_revision: u32,
    pub utf16_length: u32,
    pub intent_high_water: u32,
    pub base_ui_revision: u32,
    pub base_utf16_length: u32,
    pub base_utf8_length: u32,
    pub base_content_hash128: [u32; 4],
    pub base_checkpoint_hash128: [u32; 4],
    pub base_checkpoint_count: u32,
    pub base_page_count: u32,
    pub base_checkpoint_spacing_utf16: u32,
    pub base_page_start: u32,
    pub base_page_end: u32,
    pub target_page_start: u32,
    pub target_page_end: u32,
    pub target_checkpoint_count: u32,
    pub target_page_count: u32,
    pub target_checkpoint_root_guard_algorithm: u32,
    pub target_checkpoint_root_guard128: [u32; 4],
    pub replacement_checkpoint_count: u32,
}

/// One bounded page of absolute target-prefix facts in a delta splice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFactsDeltaPageEvent {
    pub certification_id: u32,
    pub worker_replica_revision: u32,
    pub ui_revision: u32,
    pub utf16_length: u32,
    pub intent_high_water: u32,
    pub replacement_page_ordinal: u32,
    pub page_checkpoint_count: u32,
    pub checkpoints: [SourceFactCheckpointWire; MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS as usize],
}

/// Terminal target proof for one exact-base canonical SourceFacts splice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFactsDeltaCompletionEvent {
    pub completion: SourceFactsCompletionEvent,
    pub checkpoint_root_guard_algorithm: u32,
    pub replacement_checkpoint_hash128: [u32; 4],
}

impl From<SourceFactsCompletionEvent> for SourceCertificationReceipt {
    fn from(completion: SourceFactsCompletionEvent) -> Self {
        Self {
            certification_id: completion.certification_id,
            worker_replica_revision: completion.worker_replica_revision,
            ui_revision: completion.ui_revision,
            utf16_length: completion.utf16_length,
            intent_high_water: completion.intent_high_water,
            fingerprint_algorithm: completion.fingerprint_algorithm,
            utf8_length: completion.utf8_length,
            logical_line_breaks: completion.logical_line_breaks,
            checkpoint_spacing_utf16: completion.checkpoint_spacing_utf16,
            checkpoint_count: completion.checkpoint_count,
            page_count: completion.page_count,
            content_hash128: completion.content_hash128,
            checkpoint_hash128: completion.checkpoint_hash128,
        }
    }
}

/// Terminal response to one accepted late-inline request that could not mint
/// exact block authority. This is attempt-local: it neither faults the parser
/// session nor fabricates a sidecar binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineRefinementUnavailableEvent {
    pub refinement_generation: u32,
    pub reason_code: u32,
}

/// Attempt-local terminal response for one passive viewport request that
/// cannot produce an atomic page under its exact base and admitted limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportPresentationUnavailableEvent {
    pub viewport_generation: u32,
    pub reason_code: u32,
}

// The largest variant is one protocol-capped 64-checkpoint page. Keeping that
// fixed-width value inline preserves allocation-free event replay and `Copy`;
// boxing it would add a fallible heap allocation to the credited wire path.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventBody {
    Opened(OpenMode),
    SourceSynchronized(SourceAcknowledgement),
    SourceFactsPage(SourceFactsPageEvent),
    SourceFactsCompleted(SourceFactsCompletionEvent),
    SourceFactsDeltaBegin(SourceFactsDeltaBeginEvent),
    SourceFactsDeltaPage(SourceFactsDeltaPageEvent),
    SourceFactsDeltaCompleted(SourceFactsDeltaCompletionEvent),
    InlineRefinementUnavailable(InlineRefinementUnavailableEvent),
    ViewportPresentationUnavailable(ViewportPresentationUnavailableEvent),
    DrainProgress(DrainProgress),
    Failed { failure_code: u32 },
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub binding: SessionBinding,
    pub event_id: u32,
    pub body: EventBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    InvalidValue,
    IdentityMismatch,
    DrainGrantRequired,
    DrainBudgetExceeded,
    Envelope(v3_wire::EncodeError),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot encode Flark v3 parser event: {self:?}")
    }
}

impl std::error::Error for EncodeError {}

/// Encodes one worker-to-Dart event into caller-owned transport storage.
pub fn encode_event_into(
    event: Event,
    expected_binding: SessionBinding,
    expected_drain_grant: Option<DrainGrant>,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    if event.event_id == 0 || !event.binding.is_valid() {
        return Err(EncodeError::InvalidValue);
    }
    if event.binding != expected_binding {
        return Err(EncodeError::IdentityMismatch);
    }

    let mut payload = [0_u8; MAXIMUM_EVENT_PAYLOAD_BYTES];
    let (opcode, payload_length) = {
        let mut writer = PayloadWriter::new(&mut payload);
        let opcode = match event.body {
            EventBody::Opened(mode) => {
                write_payload_header(
                    &mut writer,
                    match mode {
                        OpenMode::Fresh => 2,
                        OpenMode::Recovery => 3,
                    },
                    event.binding,
                );
                Opcode::ParserOpen
            }
            EventBody::SourceSynchronized(acknowledgement) => {
                encode_source_acknowledgement(&mut writer, event.binding, acknowledgement)?
            }
            EventBody::SourceFactsPage(page) => {
                encode_source_facts_page(&mut writer, event.binding, page)?;
                Opcode::ParserPoll
            }
            EventBody::SourceFactsCompleted(completion) => {
                encode_source_facts_completion(&mut writer, event.binding, completion)?;
                Opcode::ParserPoll
            }
            EventBody::SourceFactsDeltaBegin(begin) => {
                encode_source_facts_delta_begin(&mut writer, event.binding, begin)?;
                Opcode::ParserPoll
            }
            EventBody::SourceFactsDeltaPage(page) => {
                encode_source_facts_delta_page(&mut writer, event.binding, page)?;
                Opcode::ParserPoll
            }
            EventBody::SourceFactsDeltaCompleted(completion) => {
                encode_source_facts_delta_completion(&mut writer, event.binding, completion)?;
                Opcode::ParserPoll
            }
            EventBody::InlineRefinementUnavailable(unavailable) => {
                if unavailable.refinement_generation == 0 || unavailable.reason_code == 0 {
                    return Err(EncodeError::InvalidValue);
                }
                write_payload_header(&mut writer, 7, event.binding);
                writer.u32(unavailable.refinement_generation);
                writer.u32(unavailable.reason_code);
                Opcode::ParserPoll
            }
            EventBody::ViewportPresentationUnavailable(unavailable) => {
                if unavailable.viewport_generation == 0 || unavailable.reason_code == 0 {
                    return Err(EncodeError::InvalidValue);
                }
                write_payload_header(&mut writer, 8, event.binding);
                writer.u32(unavailable.viewport_generation);
                writer.u32(unavailable.reason_code);
                Opcode::ParserPoll
            }
            EventBody::DrainProgress(progress) => {
                let grant = expected_drain_grant.ok_or(EncodeError::DrainGrantRequired)?;
                if !grant.binding.is_valid()
                    || grant.drain_id == 0
                    || grant.maximum_transitions == 0
                    || grant.maximum_transitions > MAXIMUM_DRAIN_TRANSITIONS
                {
                    return Err(EncodeError::InvalidValue);
                }
                if grant.binding != event.binding || grant.drain_id != progress.drain_id {
                    return Err(EncodeError::IdentityMismatch);
                }
                if progress
                    .released_source_leases
                    .checked_add(progress.arena_transitions)
                    .is_none_or(|work| work > grant.maximum_transitions)
                {
                    return Err(EncodeError::DrainBudgetExceeded);
                }
                write_payload_header(&mut writer, 1, event.binding);
                writer.u32(progress.drain_id);
                writer.u32(progress.released_source_leases);
                writer.u32(progress.released_source_bytes);
                writer.u32(progress.arena_transitions);
                writer.u32(progress.arena_nodes_reclaimed);
                writer.u32(u32::from(progress.complete));
                Opcode::Drain
            }
            EventBody::Failed { failure_code } => {
                write_payload_header(&mut writer, 1, event.binding);
                writer.u32(failure_code);
                Opcode::ParserPoll
            }
            EventBody::Closed => {
                write_payload_header(&mut writer, 1, event.binding);
                Opcode::Close
            }
        };
        (opcode, writer.len())
    };
    v3_wire::encode_into(
        FrameKind::Request,
        Header {
            opcode,
            status: Status::Ok,
            flags: 0,
            correlation_id: event.event_id,
        },
        &payload[..payload_length],
        output,
    )
    .map_err(EncodeError::Envelope)
}

fn encode_source_acknowledgement(
    writer: &mut PayloadWriter<'_>,
    binding: SessionBinding,
    acknowledgement: SourceAcknowledgement,
) -> Result<Opcode, EncodeError> {
    match acknowledgement {
        SourceAcknowledgement::Snapshot {
            source_session_identity,
            lease_id,
            worker_generation,
            base_ui_revision,
            start_utf16,
            end_utf16,
            through_intent_sequence,
            observed_replica,
        } => {
            if source_session_identity == 0 || lease_id == 0 || worker_generation == 0 {
                return Err(EncodeError::InvalidValue);
            }
            if source_session_identity != binding.source_session_identity
                || worker_generation != binding.worker_generation
            {
                return Err(EncodeError::IdentityMismatch);
            }
            if end_utf16 < start_utf16 || end_utf16 - start_utf16 > MAXIMUM_SNAPSHOT_UTF16 {
                return Err(EncodeError::InvalidValue);
            }
            write_payload_header(writer, if start_utf16 == 0 { 2 } else { 3 }, binding);
            writer.u32(lease_id);
            writer.u32(base_ui_revision);
            writer.u32(start_utf16);
            writer.u32(end_utf16);
            writer.u32(through_intent_sequence);
            write_observed_replica(writer, observed_replica);
            Ok(Opcode::SnapshotPage)
        }
        SourceAcknowledgement::Edit {
            source_session_identity,
            lease_id,
            worker_generation,
            first_sequence,
            last_sequence,
            entry_count,
            payload_utf16,
            observed_replica,
        } => {
            if source_session_identity == 0 || lease_id == 0 || worker_generation == 0 {
                return Err(EncodeError::InvalidValue);
            }
            if source_session_identity != binding.source_session_identity
                || worker_generation != binding.worker_generation
            {
                return Err(EncodeError::IdentityMismatch);
            }
            if first_sequence == 0
                || last_sequence < first_sequence
                || entry_count == 0
                || entry_count > MAXIMUM_INTENT_COUNT
                || payload_utf16 > MAXIMUM_INTENT_PAYLOAD_UTF16
            {
                return Err(EncodeError::InvalidValue);
            }
            write_payload_header(writer, 1, binding);
            writer.u32(lease_id);
            writer.u32(first_sequence);
            writer.u32(last_sequence);
            writer.u32(entry_count);
            writer.u32(payload_utf16);
            write_observed_replica(writer, Some(observed_replica));
            Ok(Opcode::Edit)
        }
    }
}

fn encode_source_facts_page(
    writer: &mut PayloadWriter<'_>,
    binding: SessionBinding,
    page: SourceFactsPageEvent,
) -> Result<(), EncodeError> {
    validate_source_facts_page(binding, page)?;
    write_payload_header(writer, 2, binding);
    writer.u32(page.certification_id);
    writer.u32(page.worker_replica_revision);
    writer.u32(page.ui_revision);
    writer.u32(page.utf16_length);
    writer.u32(page.intent_high_water);
    writer.u32(page.checkpoint_spacing_utf16);
    writer.u32(page.page_ordinal);
    writer.u32(page.page_count);
    writer.u32(page.checkpoint_count);
    writer.u32(page.page_checkpoint_count);
    for checkpoint in page
        .checkpoints
        .iter()
        .take(page.page_checkpoint_count as usize)
    {
        writer.u32(checkpoint.byte_offset);
        writer.u32(checkpoint.utf16_offset);
        writer.u32(checkpoint.logical_line_breaks);
        write_hash128(writer, checkpoint.rolling_hash128);
    }
    Ok(())
}

fn encode_source_facts_completion(
    writer: &mut PayloadWriter<'_>,
    binding: SessionBinding,
    completion: SourceFactsCompletionEvent,
) -> Result<(), EncodeError> {
    validate_source_certification(binding, completion.into())?;
    if completion.page_count
        != completion
            .checkpoint_count
            .div_ceil(MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS)
    {
        return Err(EncodeError::InvalidValue);
    }
    write_payload_header(writer, 3, binding);
    write_source_certification(writer, completion.into());
    Ok(())
}

fn encode_source_facts_delta_begin(
    writer: &mut PayloadWriter<'_>,
    binding: SessionBinding,
    begin: SourceFactsDeltaBeginEvent,
) -> Result<(), EncodeError> {
    validate_source_facts_delta_begin(binding, begin)?;
    write_payload_header(writer, 4, binding);
    writer.u32(begin.certification_id);
    writer.u32(begin.worker_replica_revision);
    writer.u32(begin.ui_revision);
    writer.u32(begin.utf16_length);
    writer.u32(begin.intent_high_water);
    writer.u32(begin.base_ui_revision);
    writer.u32(begin.base_utf16_length);
    writer.u32(begin.base_utf8_length);
    write_hash128(writer, begin.base_content_hash128);
    write_hash128(writer, begin.base_checkpoint_hash128);
    writer.u32(begin.base_checkpoint_count);
    writer.u32(begin.base_page_count);
    writer.u32(begin.base_checkpoint_spacing_utf16);
    writer.u32(begin.base_page_start);
    writer.u32(begin.base_page_end);
    writer.u32(begin.target_page_start);
    writer.u32(begin.target_page_end);
    writer.u32(begin.target_checkpoint_count);
    writer.u32(begin.target_page_count);
    writer.u32(begin.target_checkpoint_root_guard_algorithm);
    write_hash128(writer, begin.target_checkpoint_root_guard128);
    writer.u32(begin.replacement_checkpoint_count);
    Ok(())
}

fn encode_source_facts_delta_page(
    writer: &mut PayloadWriter<'_>,
    binding: SessionBinding,
    page: SourceFactsDeltaPageEvent,
) -> Result<(), EncodeError> {
    validate_source_facts_delta_page(binding, page)?;
    write_payload_header(writer, 5, binding);
    writer.u32(page.certification_id);
    writer.u32(page.worker_replica_revision);
    writer.u32(page.ui_revision);
    writer.u32(page.utf16_length);
    writer.u32(page.intent_high_water);
    writer.u32(page.replacement_page_ordinal);
    writer.u32(page.page_checkpoint_count);
    for checkpoint in page
        .checkpoints
        .iter()
        .take(page.page_checkpoint_count as usize)
    {
        writer.u32(checkpoint.byte_offset);
        writer.u32(checkpoint.utf16_offset);
        writer.u32(checkpoint.logical_line_breaks);
        write_hash128(writer, checkpoint.rolling_hash128);
    }
    Ok(())
}

fn encode_source_facts_delta_completion(
    writer: &mut PayloadWriter<'_>,
    binding: SessionBinding,
    completion: SourceFactsDeltaCompletionEvent,
) -> Result<(), EncodeError> {
    if completion.checkpoint_root_guard_algorithm != PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM {
        return Err(EncodeError::InvalidValue);
    }
    validate_source_certification(binding, completion.completion.into())?;
    write_payload_header(writer, 6, binding);
    write_source_certification(writer, completion.completion.into());
    writer.u32(completion.checkpoint_root_guard_algorithm);
    write_hash128(writer, completion.replacement_checkpoint_hash128);
    Ok(())
}

fn validate_source_facts_delta_begin(
    binding: SessionBinding,
    begin: SourceFactsDeltaBeginEvent,
) -> Result<(), EncodeError> {
    let removed_pages = begin
        .base_page_end
        .checked_sub(begin.base_page_start)
        .ok_or(EncodeError::InvalidValue)?;
    let replacement_pages = begin
        .target_page_end
        .checked_sub(begin.target_page_start)
        .ok_or(EncodeError::InvalidValue)?;
    let expected_target_pages = begin
        .base_page_count
        .checked_sub(removed_pages)
        .and_then(|pages| pages.checked_add(replacement_pages))
        .ok_or(EncodeError::InvalidValue)?;
    if !binding.is_valid()
        || begin.certification_id == 0
        || begin.worker_replica_revision != begin.ui_revision
        || begin.base_ui_revision >= begin.ui_revision
        || !valid_persistent_page_topology(begin.base_checkpoint_count, begin.base_page_count)
        || !(2..=MAXIMUM_SNAPSHOT_UTF16).contains(&begin.base_checkpoint_spacing_utf16)
        || (begin.base_utf16_length == 0)
            != (begin.base_utf8_length == 0
                && begin.base_content_hash128 == [0; 4]
                && begin.base_checkpoint_count == 0
                && begin.base_page_count == 0)
        || begin.base_page_end > begin.base_page_count
        || begin.target_page_start != begin.base_page_start
        || begin.target_page_count != expected_target_pages
        || !valid_persistent_page_topology(begin.target_checkpoint_count, begin.target_page_count)
        || begin.target_checkpoint_root_guard_algorithm
            != PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM
        || (begin.utf16_length == 0)
            != (begin.target_checkpoint_count == 0 && begin.target_page_count == 0)
        || (begin.utf16_length != 0
            && (begin.target_checkpoint_count == 0 || begin.target_page_count == 0))
        || !valid_persistent_page_topology(begin.replacement_checkpoint_count, replacement_pages)
    {
        return Err(EncodeError::InvalidValue);
    }
    Ok(())
}

fn validate_source_facts_delta_page(
    binding: SessionBinding,
    page: SourceFactsDeltaPageEvent,
) -> Result<(), EncodeError> {
    if !binding.is_valid()
        || page.certification_id == 0
        || page.worker_replica_revision != page.ui_revision
        || page.page_checkpoint_count == 0
        || page.page_checkpoint_count > MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS
    {
        return Err(EncodeError::InvalidValue);
    }
    let mut prior = None;
    for checkpoint in page
        .checkpoints
        .iter()
        .copied()
        .take(page.page_checkpoint_count as usize)
    {
        if checkpoint.byte_offset == 0
            || checkpoint.utf16_offset == 0
            || checkpoint.utf16_offset > page.utf16_length
            || checkpoint.logical_line_breaks > checkpoint.utf16_offset
            || prior.is_some_and(|prior: SourceFactCheckpointWire| {
                checkpoint.byte_offset <= prior.byte_offset
                    || checkpoint.utf16_offset <= prior.utf16_offset
                    || checkpoint.logical_line_breaks < prior.logical_line_breaks
            })
        {
            return Err(EncodeError::InvalidValue);
        }
        prior = Some(checkpoint);
    }
    Ok(())
}

fn validate_source_facts_page(
    binding: SessionBinding,
    page: SourceFactsPageEvent,
) -> Result<(), EncodeError> {
    let expected_page_count = page
        .checkpoint_count
        .div_ceil(MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS);
    let expected_page_checkpoints = page
        .checkpoint_count
        .checked_sub(
            page.page_ordinal
                .checked_mul(MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS)
                .ok_or(EncodeError::InvalidValue)?,
        )
        .map(|remaining| remaining.min(MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS))
        .ok_or(EncodeError::InvalidValue)?;
    if !binding.is_valid()
        || page.certification_id == 0
        || page.worker_replica_revision != page.ui_revision
        || !(2..=MAXIMUM_SNAPSHOT_UTF16).contains(&page.checkpoint_spacing_utf16)
        || page.page_count == 0
        || page.page_count != expected_page_count
        || page.page_ordinal >= page.page_count
        || page.page_checkpoint_count == 0
        || page.page_checkpoint_count != expected_page_checkpoints
    {
        return Err(EncodeError::InvalidValue);
    }
    let mut prior = None;
    for checkpoint in page
        .checkpoints
        .iter()
        .copied()
        .take(page.page_checkpoint_count as usize)
    {
        if checkpoint.byte_offset == 0
            || checkpoint.utf16_offset == 0
            || checkpoint.utf16_offset > page.utf16_length
            || checkpoint.logical_line_breaks > checkpoint.utf16_offset
            || prior.is_some_and(|prior: SourceFactCheckpointWire| {
                checkpoint.byte_offset <= prior.byte_offset
                    || checkpoint.utf16_offset <= prior.utf16_offset
                    || checkpoint.utf16_offset - prior.utf16_offset
                        > page.checkpoint_spacing_utf16 + 1
                    || checkpoint.logical_line_breaks < prior.logical_line_breaks
            })
        {
            return Err(EncodeError::InvalidValue);
        }
        prior = Some(checkpoint);
    }
    let terminal = prior.is_some_and(|checkpoint| checkpoint.utf16_offset == page.utf16_length);
    if terminal != (page.page_ordinal + 1 == page.page_count) {
        return Err(EncodeError::InvalidValue);
    }
    Ok(())
}

fn validate_source_certification(
    binding: SessionBinding,
    certification: SourceCertificationReceipt,
) -> Result<(), EncodeError> {
    if !binding.is_valid()
        || certification.certification_id == 0
        || certification.worker_replica_revision != certification.ui_revision
        || certification.fingerprint_algorithm != 1
        || certification.logical_line_breaks > certification.utf16_length
        || !(2..=MAXIMUM_SNAPSHOT_UTF16).contains(&certification.checkpoint_spacing_utf16)
        || !valid_persistent_page_topology(certification.checkpoint_count, certification.page_count)
        || (certification.utf16_length == 0) != (certification.checkpoint_count == 0)
        || (certification.utf16_length == 0
            && (certification.utf8_length != 0
                || certification.logical_line_breaks != 0
                || certification.content_hash128 != [0; 4]
                || certification.checkpoint_hash128 != [0; 4]))
    {
        return Err(EncodeError::InvalidValue);
    }
    Ok(())
}

fn valid_persistent_page_topology(checkpoint_count: u32, page_count: u32) -> bool {
    if checkpoint_count == 0 {
        return page_count == 0;
    }
    let minimum_pages = checkpoint_count.div_ceil(MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS);
    minimum_pages <= page_count
        && page_count <= checkpoint_count
        && usize::try_from(page_count)
            .is_ok_and(|pages| pages <= SOURCE_FACT_ROOT_DEFAULT_MAX_PAGES)
}

fn write_source_certification(
    writer: &mut PayloadWriter<'_>,
    certification: SourceCertificationReceipt,
) {
    writer.u32(certification.certification_id);
    writer.u32(certification.worker_replica_revision);
    writer.u32(certification.ui_revision);
    writer.u32(certification.utf16_length);
    writer.u32(certification.intent_high_water);
    writer.u32(certification.fingerprint_algorithm);
    // The fingerprint repeats source revision and UTF-16 length deliberately;
    // crossed terminal receipts must fail before authority can advance.
    writer.u32(certification.ui_revision);
    writer.u32(certification.utf16_length);
    writer.u32(certification.utf8_length);
    writer.u32(certification.logical_line_breaks);
    writer.u32(certification.checkpoint_spacing_utf16);
    writer.u32(certification.checkpoint_count);
    writer.u32(certification.page_count);
    write_hash128(writer, certification.content_hash128);
    write_hash128(writer, certification.checkpoint_hash128);
}

fn write_hash128(writer: &mut PayloadWriter<'_>, hash: [u32; 4]) {
    for word in hash {
        writer.u32(word);
    }
}

#[derive(Clone, Copy)]
struct PayloadHeader {
    variant: u16,
    binding: SessionBinding,
}

fn read_payload_header(reader: &mut PayloadReader<'_>) -> Result<PayloadHeader, DecodeError> {
    let schema = reader.u16()?;
    if schema != PAYLOAD_SCHEMA {
        return Err(DecodeError::session(
            DecodeFailure::UnsupportedSchema,
            0,
            Some(PAYLOAD_SCHEMA as usize),
            Some(schema as usize),
        ));
    }
    let variant = reader.u16()?;
    let worker_generation = reader.u32()?;
    if worker_generation == 0 {
        return Err(invalid(4, Some(1), Some(0)));
    }
    let document_session = [reader.u32()?, reader.u32()?, reader.u32()?, reader.u32()?];
    let source_session_identity = reader.u32()?;
    if source_session_identity == 0 {
        return Err(invalid(24, Some(1), Some(0)));
    }
    Ok(PayloadHeader {
        variant,
        binding: SessionBinding {
            document_session,
            source_session_identity,
            worker_generation,
        },
    })
}

fn read_snapshot<'payload>(
    reader: &mut PayloadReader<'payload>,
    binding: SessionBinding,
    seed: bool,
) -> Result<SnapshotCommand<'payload>, DecodeError> {
    let lease_id = reader.u32()?;
    let base_ui_revision = reader.u32()?;
    let start_utf16 = reader.u32()?;
    let end_utf16 = reader.u32()?;
    let total_utf16_length = reader.u32()?;
    let through_intent_sequence = reader.u32()?;
    let target_stamp = read_source_stamp(reader)?;
    let source_utf16_length = reader.u32()?;
    let source_utf8_bytes = reader.u32()?;
    if source_utf16_length > MAXIMUM_SNAPSHOT_UTF16 {
        return Err(oversized(
            reader.offset,
            source_utf16_length,
            MAXIMUM_SNAPSHOT_UTF16,
        ));
    }
    let source = reader.strict_string(source_utf8_bytes as usize)?;
    let decoded_utf16 = utf16_length(source);
    if decoded_utf16 != source_utf16_length {
        return Err(invalid(
            reader.offset,
            Some(source_utf16_length as usize),
            Some(decoded_utf16 as usize),
        ));
    }
    if lease_id == 0
        || end_utf16 < start_utf16
        || end_utf16 > total_utf16_length
        || end_utf16 - start_utf16 != source_utf16_length
        || (start_utf16 > 0 && source.is_empty())
        || (start_utf16 == 0) != seed
        || target_stamp.revision() != base_ui_revision
        || target_stamp.utf16_length() != total_utf16_length
    {
        return Err(invalid(reader.offset, None, None));
    }
    Ok(SnapshotCommand {
        binding,
        lease_id,
        base_ui_revision,
        start_utf16,
        end_utf16,
        total_utf16_length,
        through_intent_sequence,
        target_stamp,
        source,
    })
}

fn read_edit<'payload>(
    reader: &mut PayloadReader<'payload>,
    binding: SessionBinding,
) -> Result<EditCommand<'payload>, DecodeError> {
    let lease_id = reader.u32()?;
    let first_sequence = reader.u32()?;
    let last_sequence = reader.u32()?;
    let intent_count = reader.u32()?;
    let operation_count = reader.u32()?;
    let payload_utf16 = reader.u32()?;
    let payload_utf8_bytes = reader.u32()?;
    if intent_count == 0 || intent_count > MAXIMUM_INTENT_COUNT {
        return Err(oversized(reader.offset, intent_count, MAXIMUM_INTENT_COUNT));
    }
    if operation_count == 0 || operation_count > MAXIMUM_OPERATION_COUNT {
        return Err(oversized(
            reader.offset,
            operation_count,
            MAXIMUM_OPERATION_COUNT,
        ));
    }
    if payload_utf16 > MAXIMUM_INTENT_PAYLOAD_UTF16 {
        return Err(oversized(
            reader.offset,
            payload_utf16,
            MAXIMUM_INTENT_PAYLOAD_UTF16,
        ));
    }
    if payload_utf8_bytes as usize > reader.remaining() {
        return Err(DecodeError::session(
            DecodeFailure::TruncatedPayload,
            reader.offset,
            Some(payload_utf8_bytes as usize),
            Some(reader.remaining()),
        ));
    }

    let body_start = reader.offset;
    let mut decoded_operations = 0_u32;
    let mut decoded_utf16 = 0_u64;
    let mut decoded_utf8 = 0_u64;
    let mut decoded_last_sequence = 0;
    for _ in 0..intent_count {
        let sequence = reader.u32()?;
        let _base_ui_revision = reader.u32()?;
        let _ui_revision = reader.u32()?;
        let intent_operation_count = reader.u32()?;
        let _base_stamp = read_source_stamp(reader)?;
        let _target_stamp = read_source_stamp(reader)?;
        if intent_operation_count == 0
            || decoded_operations
                .checked_add(intent_operation_count)
                .is_none_or(|count| count > operation_count)
        {
            return Err(invalid(reader.offset, None, None));
        }
        decoded_last_sequence = sequence;
        for _ in 0..intent_operation_count {
            let start_utf16 = reader.u32()?;
            let end_utf16 = reader.u32()?;
            let replacement_utf16 = reader.u32()?;
            let replacement_utf8 = reader.u32()?;
            if replacement_utf16 > MAXIMUM_INTENT_PAYLOAD_UTF16 {
                return Err(oversized(
                    reader.offset,
                    replacement_utf16,
                    MAXIMUM_INTENT_PAYLOAD_UTF16,
                ));
            }
            let replacement = reader.strict_string(replacement_utf8 as usize)?;
            let actual_utf16 = utf16_length(replacement);
            if actual_utf16 != replacement_utf16 || end_utf16 < start_utf16 {
                return Err(invalid(reader.offset, None, None));
            }
            decoded_utf16 += u64::from(replacement_utf16);
            decoded_utf8 += u64::from(replacement_utf8);
        }
        decoded_operations += intent_operation_count;
    }
    let body_end = reader.offset;
    let command = EditCommand {
        binding,
        lease_id,
        first_sequence,
        last_sequence,
        intent_count,
        operation_count,
        payload_utf16,
        payload_utf8_bytes,
        body: &reader.bytes[body_start..body_end],
    };
    validate_edit_shape(command, reader.offset)?;
    if first_sequence != read_u32_at(reader.bytes, body_start)
        || last_sequence != decoded_last_sequence
        || decoded_operations != operation_count
        || decoded_utf16 != u64::from(payload_utf16)
        || decoded_utf8 != u64::from(payload_utf8_bytes)
    {
        return Err(invalid(reader.offset, None, None));
    }
    Ok(command)
}

fn validate_edit_shape(command: EditCommand<'_>, offset: usize) -> Result<(), DecodeError> {
    if command.lease_id == 0 {
        return Err(invalid(offset, None, None));
    }
    let mut prior_sequence = None;
    let mut expected_base_revision = None;
    let mut expected_base_stamp = None;
    let mut operation_count = 0_u32;
    let mut payload_utf16 = 0_u64;
    for intent in command.intents() {
        if intent.sequence == 0
            || intent.ui_revision == 0
            || intent.base_ui_revision.checked_add(1) != Some(intent.ui_revision)
            || intent.base_stamp.revision() != intent.base_ui_revision
            || intent.target_stamp.revision() != intent.ui_revision
            || prior_sequence.is_some_and(|prior| intent.sequence <= prior)
            || expected_base_revision.is_some_and(|expected| intent.base_ui_revision != expected)
            || expected_base_stamp.is_some_and(|expected| intent.base_stamp != expected)
            || intent.operations.len() == 0
        {
            return Err(invalid(offset, None, None));
        }
        prior_sequence = Some(intent.sequence);
        expected_base_revision = Some(intent.ui_revision);
        expected_base_stamp = Some(intent.target_stamp);
        let mut prior_operation = None;
        let mut intent_deleted_utf16 = 0_u64;
        let mut intent_payload_utf16 = 0_u64;
        for operation in intent.operations {
            if operation.end_utf16 < operation.start_utf16
                || prior_operation.is_some_and(|(start, end)| {
                    operation.start_utf16 < start
                        || (operation.start_utf16 == start && operation.end_utf16 < end)
                        || operation.start_utf16 < end
                })
            {
                return Err(invalid(offset, None, None));
            }
            prior_operation = Some((operation.start_utf16, operation.end_utf16));
            operation_count += 1;
            intent_deleted_utf16 += u64::from(operation.end_utf16 - operation.start_utf16);
            intent_payload_utf16 += u64::from(operation.replacement_utf16);
            payload_utf16 += u64::from(operation.replacement_utf16);
        }
        let expected_target_utf16 = u64::from(intent.base_stamp.utf16_length())
            .checked_sub(intent_deleted_utf16)
            .and_then(|length| length.checked_add(intent_payload_utf16));
        if expected_target_utf16 != Some(u64::from(intent.target_stamp.utf16_length())) {
            return Err(invalid(offset, None, None));
        }
    }
    if operation_count > MAXIMUM_OPERATION_COUNT
        || operation_count != command.operation_count
        || payload_utf16 != u64::from(command.payload_utf16)
        || payload_utf16 > u64::from(MAXIMUM_INTENT_PAYLOAD_UTF16)
    {
        return Err(invalid(offset, None, None));
    }
    Ok(())
}

fn read_inline_refinement(
    reader: &mut PayloadReader<'_>,
    binding: SessionBinding,
) -> Result<InlineRefinementCommand, DecodeError> {
    let refinement_generation = reader.u32()?;
    let source_version = read_publication_source_version(reader)?;
    let base_ack = read_structural_ack(reader)?;
    let byte_offset = reader.u32()?;
    let utf16_offset = reader.u32()?;
    let affinity = match reader.u32()? {
        0 => InlinePointAffinity::Before,
        1 => InlinePointAffinity::After,
        value => return Err(unknown_variant_u32(value)),
    };
    let target = match reader.u32()? {
        0 => InlineRefinementTarget::Automatic,
        1 => InlineRefinementTarget::BulletListItemInline,
        2 => InlineRefinementTarget::BulletListItemProjection,
        3 => InlineRefinementTarget::OrderedListItemInline,
        4 => InlineRefinementTarget::OrderedListItemProjection,
        5 => InlineRefinementTarget::RecursiveGreenParagraph,
        6 => InlineRefinementTarget::BlockQuoteProjection,
        value => return Err(unknown_variant_u32(value)),
    };
    if refinement_generation == 0
        || source_version != base_ack.source_version
        || byte_offset > source_version.utf8_length
        || utf16_offset > source_version.utf16_length
    {
        return Err(invalid(reader.offset, None, None));
    }
    if source_version.document_session != binding.document_session {
        return Err(DecodeError::session(
            DecodeFailure::IdentityMismatch,
            reader.offset,
            None,
            None,
        ));
    }
    Ok(InlineRefinementCommand {
        binding,
        refinement_generation,
        source_version,
        base_ack,
        byte_offset,
        utf16_offset,
        affinity,
        target,
    })
}

fn read_viewport_presentation(
    reader: &mut PayloadReader<'_>,
    binding: SessionBinding,
) -> Result<ViewportPresentationCommand, DecodeError> {
    let viewport_generation = reader.u32()?;
    let source_version = read_publication_source_version(reader)?;
    let base_ack = read_structural_ack(reader)?;
    let requested_start_utf8 = reader.u32()?;
    let requested_start_utf16 = reader.u32()?;
    let requested_end_utf8 = reader.u32()?;
    let requested_end_utf16 = reader.u32()?;
    let start_block_ordinal = u64::from(reader.u32()?) | (u64::from(reader.u32()?) << 32);
    let start_utf8 = reader.u32()?;
    let start_utf16 = reader.u32()?;
    let limits = ViewportPresentationLimits {
        maximum_structural_entries: reader.u32()?,
        maximum_storage_pages: reader.u32()?,
        maximum_inline_leaves: reader.u32()?,
        maximum_inline_leaf_source_bytes: reader.u32()?,
        maximum_inline_source_bytes: reader.u32()?,
        maximum_fact_records: reader.u32()?,
        maximum_encoded_frame_bytes: reader.u32()?,
        maximum_parser_transitions: reader.u32()?,
    };
    let valid_range = requested_start_utf8 < requested_end_utf8
        && requested_start_utf16 < requested_end_utf16
        && requested_end_utf8 <= source_version.utf8_length
        && requested_end_utf16 <= source_version.utf16_length;
    let valid_start = start_utf8 == requested_start_utf8 && start_utf16 == requested_start_utf16;
    let valid_limits = limits.maximum_structural_entries != 0
        && limits.maximum_structural_entries <= MAXIMUM_VIEWPORT_STRUCTURAL_ENTRIES
        && limits.maximum_storage_pages != 0
        && limits.maximum_storage_pages <= MAXIMUM_VIEWPORT_STORAGE_PAGES
        && limits.maximum_inline_leaves != 0
        && limits.maximum_inline_leaves <= MAXIMUM_VIEWPORT_INLINE_LEAVES
        && limits.maximum_inline_leaves <= limits.maximum_structural_entries
        && limits.maximum_inline_leaf_source_bytes != 0
        && limits.maximum_inline_leaf_source_bytes <= MAXIMUM_VIEWPORT_INLINE_LEAF_SOURCE_BYTES
        && limits.maximum_inline_source_bytes != 0
        && limits.maximum_inline_source_bytes <= MAXIMUM_VIEWPORT_INLINE_SOURCE_BYTES
        && limits.maximum_inline_leaf_source_bytes <= limits.maximum_inline_source_bytes
        && limits.maximum_fact_records != 0
        && limits.maximum_fact_records <= MAXIMUM_VIEWPORT_FACT_RECORDS
        && limits.maximum_encoded_frame_bytes != 0
        && limits.maximum_encoded_frame_bytes <= MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES
        && limits.maximum_parser_transitions != 0
        && limits.maximum_parser_transitions <= MAXIMUM_VIEWPORT_PARSER_TRANSITIONS;
    if viewport_generation == 0
        || source_version != base_ack.source_version
        || !valid_range
        || !valid_start
        || !valid_limits
    {
        return Err(invalid(reader.offset, None, None));
    }
    if source_version.document_session != binding.document_session {
        return Err(DecodeError::session(
            DecodeFailure::IdentityMismatch,
            reader.offset,
            None,
            None,
        ));
    }
    Ok(ViewportPresentationCommand {
        binding,
        viewport_generation,
        source_version,
        base_ack,
        requested_start_utf8,
        requested_start_utf16,
        requested_end_utf8,
        requested_end_utf16,
        start_block_ordinal,
        start_utf8,
        start_utf16,
        limits,
    })
}

fn read_publication_source_version(
    reader: &mut PayloadReader<'_>,
) -> Result<SourceVersion, DecodeError> {
    Ok(SourceVersion {
        document_session: read_hash128(reader)?,
        revision: reader.u32()?,
        utf8_length: reader.u32()?,
        utf16_length: reader.u32()?,
        content_hash128: read_hash128(reader)?,
    })
}

fn read_structural_ack(reader: &mut PayloadReader<'_>) -> Result<StructuralAck, DecodeError> {
    let ack = StructuralAck {
        publication_session: read_hash128(reader)?,
        host_revision: reader.u32()?,
        source_version: read_publication_source_version(reader)?,
        source_root: [reader.u32()?, reader.u32()?],
        parse_generation: reader.u32()?,
        grammar_revision: reader.u32()?,
        syntax_profile: reader.u32()?,
        authority_mask: reader.u32()?,
        record_count: reader.u32()?,
        sequence_digest: read_hash128(reader)?,
        manifest_digest: read_hash128(reader)?,
    };
    if !structural_ack_is_valid(ack) {
        return Err(invalid(reader.offset, None, None));
    }
    Ok(ack)
}

fn read_receipt(
    reader: &mut PayloadReader<'_>,
    header: PayloadHeader,
    event_id: u32,
) -> Result<EventReceiptCommand, DecodeError> {
    let disposition = match header.variant {
        0 => EventDisposition::Accepted,
        1 => EventDisposition::Stale,
        2 => EventDisposition::Rejected,
        variant => return Err(unknown_variant(variant)),
    };
    let has_source = reader.u32()?;
    if has_source > 1 {
        return Err(invalid(reader.offset, Some(1), Some(has_source as usize)));
    }
    let has_certification = reader.u32()?;
    if has_certification > 1 {
        return Err(invalid(
            reader.offset,
            Some(1),
            Some(has_certification as usize),
        ));
    }
    if has_source == 1 && has_certification == 1 {
        return Err(invalid(reader.offset, None, None));
    }
    let source = if has_source == 0 {
        None
    } else {
        let source_disposition_code = reader.u32()?;
        let source_disposition = match source_disposition_code {
            0 => SourceReceiptDisposition::Acknowledged,
            1 => SourceReceiptDisposition::Stale,
            value => return Err(unknown_variant_u32(value)),
        };
        let source = SourceReceipt {
            disposition: source_disposition,
            dropped_intent_entries: reader.u32()?,
            dropped_payload_utf16: reader.u32()?,
            dropped_deleted_utf16: reader.u32()?,
            dropped_operation_count: reader.u32()?,
            worker_revision: reader.u32()?,
        };
        if source.dropped_intent_entries > MAXIMUM_INTENT_COUNT
            || source.dropped_payload_utf16 > MAXIMUM_INTENT_PAYLOAD_UTF16
            || source.dropped_operation_count > MAXIMUM_OPERATION_COUNT
            || (source.disposition == SourceReceiptDisposition::Stale
                && (source.dropped_intent_entries != 0
                    || source.dropped_payload_utf16 != 0
                    || source.dropped_deleted_utf16 != 0
                    || source.dropped_operation_count != 0))
        {
            return Err(invalid(reader.offset, None, None));
        }
        Some(source)
    };
    let certification = if has_certification == 0 {
        None
    } else {
        let certification = read_source_certification(reader)?;
        validate_source_certification(header.binding, certification)
            .map_err(|_| invalid(reader.offset, None, None))?;
        if disposition != EventDisposition::Accepted {
            return Err(invalid(reader.offset, None, None));
        }
        Some(certification)
    };
    Ok(EventReceiptCommand {
        binding: header.binding,
        event_id,
        disposition,
        source,
        certification,
    })
}

fn read_source_certification(
    reader: &mut PayloadReader<'_>,
) -> Result<SourceCertificationReceipt, DecodeError> {
    let certification_id = reader.u32()?;
    let worker_replica_revision = reader.u32()?;
    let ui_revision = reader.u32()?;
    let utf16_length = reader.u32()?;
    let intent_high_water = reader.u32()?;
    let fingerprint_algorithm = reader.u32()?;
    let fingerprint_revision = reader.u32()?;
    let fingerprint_utf16_length = reader.u32()?;
    let utf8_length = reader.u32()?;
    let logical_line_breaks = reader.u32()?;
    let checkpoint_spacing_utf16 = reader.u32()?;
    let checkpoint_count = reader.u32()?;
    let page_count = reader.u32()?;
    let content_hash128 = read_hash128(reader)?;
    let checkpoint_hash128 = read_hash128(reader)?;
    if fingerprint_revision != ui_revision || fingerprint_utf16_length != utf16_length {
        return Err(invalid(reader.offset, None, None));
    }
    Ok(SourceCertificationReceipt {
        certification_id,
        worker_replica_revision,
        ui_revision,
        utf16_length,
        intent_high_water,
        fingerprint_algorithm,
        utf8_length,
        logical_line_breaks,
        checkpoint_spacing_utf16,
        checkpoint_count,
        page_count,
        content_hash128,
        checkpoint_hash128,
    })
}

fn read_hash128(reader: &mut PayloadReader<'_>) -> Result<[u32; 4], DecodeError> {
    Ok([reader.u32()?, reader.u32()?, reader.u32()?, reader.u32()?])
}

fn validate_transition(
    command: Command<'_>,
    established: Option<SessionBinding>,
    offset: usize,
) -> Result<(), DecodeError> {
    if let Command::Open { binding, mode } = command {
        return match mode {
            OpenMode::Fresh if established.is_none() => Ok(()),
            OpenMode::Fresh => Err(identity(
                offset,
                binding.worker_generation,
                established
                    .expect("fresh-open mismatch requires an established binding")
                    .worker_generation,
            )),
            OpenMode::Recovery => {
                let Some(previous) = established else {
                    return Err(DecodeError::session(
                        DecodeFailure::IdentityMismatch,
                        offset,
                        None,
                        None,
                    ));
                };
                if binding.document_session != previous.document_session
                    || binding.source_session_identity != previous.source_session_identity
                    || previous.worker_generation == u32::MAX
                    || binding.worker_generation != previous.worker_generation + 1
                {
                    Err(identity(
                        offset,
                        binding.worker_generation,
                        previous.worker_generation.saturating_add(1),
                    ))
                } else {
                    Ok(())
                }
            }
        };
    }
    match established {
        Some(expected) if command.binding() == expected => Ok(()),
        _ => Err(DecodeError::session(
            DecodeFailure::IdentityMismatch,
            offset,
            None,
            None,
        )),
    }
}

fn require_command_opcode(opcode: Opcode) -> Result<(), DecodeError> {
    match opcode {
        Opcode::ParserOpen
        | Opcode::SnapshotPage
        | Opcode::Edit
        | Opcode::ParserRefineInline
        | Opcode::ParserPresentViewport
        | Opcode::Supersede
        | Opcode::ParserAcknowledge
        | Opcode::Close
        | Opcode::Drain => Ok(()),
        _ => Err(DecodeError::session(
            DecodeFailure::UnexpectedOpcode,
            8,
            None,
            Some(opcode.code() as usize),
        )),
    }
}

fn read_source_stamp(reader: &mut PayloadReader<'_>) -> Result<SourceStamp, DecodeError> {
    let tag = reader.u32()?;
    let revision = reader.u32()?;
    let utf16_length = reader.u32()?;
    let utf8_length = reader.u32()?;
    let content_hash128 = [reader.u32()?, reader.u32()?, reader.u32()?, reader.u32()?];
    match tag {
        0 if utf8_length == 0 && content_hash128 == [0; 4] => Ok(SourceStamp::Provisional {
            revision,
            utf16_length,
        }),
        0 => Err(invalid(reader.offset, Some(0), Some(1))),
        1 => Ok(SourceStamp::Known {
            revision,
            utf16_length,
            utf8_length,
            content_hash128,
        }),
        value => Err(unknown_variant_u32(value)),
    }
}

fn read_validated_source_stamp_at(bytes: &[u8], offset: usize) -> SourceStamp {
    let tag = read_u32_at(bytes, offset);
    let revision = read_u32_at(bytes, offset + 4);
    let utf16_length = read_u32_at(bytes, offset + 8);
    match tag {
        0 => SourceStamp::Provisional {
            revision,
            utf16_length,
        },
        1 => SourceStamp::Known {
            revision,
            utf16_length,
            utf8_length: read_u32_at(bytes, offset + 12),
            content_hash128: [
                read_u32_at(bytes, offset + 16),
                read_u32_at(bytes, offset + 20),
                read_u32_at(bytes, offset + 24),
                read_u32_at(bytes, offset + 28),
            ],
        },
        _ => unreachable!("validated source stamp tag must remain canonical"),
    }
}

fn write_observed_replica(
    writer: &mut PayloadWriter<'_>,
    observed: Option<ObservedSourceReplicaVersion>,
) {
    if let Some(observed) = observed {
        writer.u32(1);
        writer.u32(observed.revision);
        writer.u32(observed.utf16_length);
        writer.u32(observed.utf8_length);
        writer.u32(observed.intent_high_water);
    } else {
        for _ in 0..5 {
            writer.u32(0);
        }
    }
}

fn write_payload_header(writer: &mut PayloadWriter<'_>, variant: u16, binding: SessionBinding) {
    writer.u16(PAYLOAD_SCHEMA);
    writer.u16(variant);
    writer.u32(binding.worker_generation);
    for word in binding.document_session {
        writer.u32(word);
    }
    writer.u32(binding.source_session_identity);
}

fn utf16_length(value: &str) -> u32 {
    u32::try_from(value.encode_utf16().count()).expect("bounded payload UTF-16 must fit u32")
}

fn read_u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated edit field must remain in range"),
    )
}

fn unknown_variant(actual: u16) -> DecodeError {
    DecodeError::session(
        DecodeFailure::UnknownVariant,
        2,
        None,
        Some(actual as usize),
    )
}

fn unknown_variant_u32(actual: u32) -> DecodeError {
    DecodeError::session(
        DecodeFailure::UnknownVariant,
        2,
        None,
        Some(actual as usize),
    )
}

fn invalid(offset: usize, expected: Option<usize>, actual: Option<usize>) -> DecodeError {
    DecodeError::session(DecodeFailure::InvalidValue, offset, expected, actual)
}

fn oversized(offset: usize, actual: u32, expected: u32) -> DecodeError {
    DecodeError::session(
        DecodeFailure::OversizedValue,
        offset,
        Some(expected as usize),
        Some(actual as usize),
    )
}

fn identity(offset: usize, actual: u32, expected: u32) -> DecodeError {
    DecodeError::session(
        DecodeFailure::IdentityMismatch,
        offset,
        Some(expected as usize),
        Some(actual as usize),
    )
}

struct PayloadReader<'payload> {
    bytes: &'payload [u8],
    offset: usize,
}

impl<'payload> PayloadReader<'payload> {
    const fn new(bytes: &'payload [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        self.require(2)?;
        let value = u16::from_le_bytes([self.bytes[self.offset], self.bytes[self.offset + 1]]);
        self.offset += 2;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        self.require(4)?;
        let value = read_u32_at(self.bytes, self.offset);
        self.offset += 4;
        Ok(value)
    }

    fn strict_string(&mut self, length: usize) -> Result<&'payload str, DecodeError> {
        self.require(length)?;
        let start = self.offset;
        let end = start + length;
        self.offset = end;
        str::from_utf8(&self.bytes[start..end])
            .map_err(|_| DecodeError::session(DecodeFailure::InvalidUtf8, start, None, None))
    }

    fn finish(&self) -> Result<(), DecodeError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeError::session(
                DecodeFailure::TrailingPayload,
                self.offset,
                Some(self.offset),
                Some(self.bytes.len()),
            ))
        }
    }

    fn require(&self, count: usize) -> Result<(), DecodeError> {
        let expected = self.offset.checked_add(count).ok_or_else(|| {
            DecodeError::session(
                DecodeFailure::TruncatedPayload,
                self.offset,
                None,
                Some(self.bytes.len()),
            )
        })?;
        if expected > self.bytes.len() {
            Err(DecodeError::session(
                DecodeFailure::TruncatedPayload,
                self.offset,
                Some(expected),
                Some(self.bytes.len()),
            ))
        } else {
            Ok(())
        }
    }
}

struct PayloadWriter<'buffer> {
    bytes: &'buffer mut [u8],
    offset: usize,
}

impl<'buffer> PayloadWriter<'buffer> {
    fn new(bytes: &'buffer mut [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    const fn len(&self) -> usize {
        self.offset
    }

    fn u16(&mut self, value: u16) {
        let end = self.offset + 2;
        self.bytes[self.offset..end].copy_from_slice(&value.to_le_bytes());
        self.offset = end;
    }

    fn u32(&mut self, value: u32) {
        let end = self.offset + 4;
        self.bytes[self.offset..end].copy_from_slice(&value.to_le_bytes());
        self.offset = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> SessionBinding {
        SessionBinding {
            document_session: [1, 2, 3, 4],
            source_session_identity: 9,
            worker_generation: 3,
        }
    }

    fn with_generation(worker_generation: u32) -> SessionBinding {
        SessionBinding {
            worker_generation,
            ..binding()
        }
    }

    fn common(binding: SessionBinding, variant: u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(COMMON_BYTES);
        push_u16(&mut bytes, PAYLOAD_SCHEMA);
        push_u16(&mut bytes, variant);
        push_u32(&mut bytes, binding.worker_generation);
        for word in binding.document_session {
            push_u32(&mut bytes, word);
        }
        push_u32(&mut bytes, binding.source_session_identity);
        assert_eq!(bytes.len(), COMMON_BYTES);
        bytes
    }

    fn frame(opcode: Opcode, correlation_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + payload.len()];
        let written = v3_wire::encode_into(
            FrameKind::Request,
            Header {
                opcode,
                status: Status::Ok,
                flags: 0,
                correlation_id,
            },
            payload,
            &mut output,
        )
        .unwrap();
        assert_eq!(written, output.len());
        output
    }

    fn encoded_event(event: Event, expected_drain_grant: Option<DrainGrant>) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + MAXIMUM_EVENT_PAYLOAD_BYTES];
        let written =
            encode_event_into(event, binding(), expected_drain_grant, &mut output).unwrap();
        output.truncate(written);
        output
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_id128(bytes: &mut Vec<u8>, value: [u32; 4]) {
        for word in value {
            push_u32(bytes, word);
        }
    }

    fn refinement_source() -> SourceVersion {
        SourceVersion {
            document_session: binding().document_session,
            revision: 11,
            utf8_length: 17,
            utf16_length: 15,
            content_hash128: [21, 22, 23, 24],
        }
    }

    fn refinement_ack() -> StructuralAck {
        StructuralAck {
            publication_session: [31, 32, 33, 34],
            host_revision: 7,
            source_version: refinement_source(),
            source_root: [41, 42],
            parse_generation: 13,
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: 0x1f,
            record_count: 9,
            sequence_digest: [51, 52, 53, 54],
            manifest_digest: [61, 62, 63, 64],
        }
    }

    fn push_publication_source(bytes: &mut Vec<u8>, source: SourceVersion) {
        push_id128(bytes, source.document_session);
        push_u32(bytes, source.revision);
        push_u32(bytes, source.utf8_length);
        push_u32(bytes, source.utf16_length);
        push_id128(bytes, source.content_hash128);
    }

    fn push_structural_ack(bytes: &mut Vec<u8>, ack: StructuralAck) {
        push_id128(bytes, ack.publication_session);
        push_u32(bytes, ack.host_revision);
        push_publication_source(bytes, ack.source_version);
        push_u32(bytes, ack.source_root[0]);
        push_u32(bytes, ack.source_root[1]);
        push_u32(bytes, ack.parse_generation);
        push_u32(bytes, ack.grammar_revision);
        push_u32(bytes, ack.syntax_profile);
        push_u32(bytes, ack.authority_mask);
        push_u32(bytes, ack.record_count);
        push_id128(bytes, ack.sequence_digest);
        push_id128(bytes, ack.manifest_digest);
    }

    fn inline_refinement_payload(
        generation: u32,
        source: SourceVersion,
        ack: StructuralAck,
        byte_offset: u32,
        utf16_offset: u32,
        affinity: u32,
    ) -> Vec<u8> {
        let mut payload = common(binding(), 0);
        push_u32(&mut payload, generation);
        push_publication_source(&mut payload, source);
        push_structural_ack(&mut payload, ack);
        push_u32(&mut payload, byte_offset);
        push_u32(&mut payload, utf16_offset);
        push_u32(&mut payload, affinity);
        push_u32(&mut payload, 0);
        payload
    }

    fn viewport_presentation_payload(
        generation: u32,
        source: SourceVersion,
        ack: StructuralAck,
        requested_start_utf8: u32,
        requested_start_utf16: u32,
        requested_end_utf8: u32,
        requested_end_utf16: u32,
        start_block_ordinal: u64,
    ) -> Vec<u8> {
        let mut payload = common(binding(), 0);
        push_u32(&mut payload, generation);
        push_publication_source(&mut payload, source);
        push_structural_ack(&mut payload, ack);
        for value in [
            requested_start_utf8,
            requested_start_utf16,
            requested_end_utf8,
            requested_end_utf16,
            start_block_ordinal as u32,
            (start_block_ordinal >> 32) as u32,
            requested_start_utf8,
            requested_start_utf16,
            47,
            2,
            24,
            8 * 1024,
            64 * 1024,
            2_048,
            64 * 1024,
            250_000,
        ] {
            push_u32(&mut payload, value);
        }
        payload
    }

    fn provisional_stamp(revision: u32, utf16_length: u32) -> SourceStamp {
        SourceStamp::Provisional {
            revision,
            utf16_length,
        }
    }

    fn known_stamp(
        revision: u32,
        utf16_length: u32,
        utf8_length: u32,
        content_hash128: [u32; 4],
    ) -> SourceStamp {
        SourceStamp::Known {
            revision,
            utf16_length,
            utf8_length,
            content_hash128,
        }
    }

    fn push_source_stamp(bytes: &mut Vec<u8>, stamp: SourceStamp) {
        match stamp {
            SourceStamp::Provisional {
                revision,
                utf16_length,
            } => {
                for value in [0, revision, utf16_length, 0, 0, 0, 0, 0] {
                    push_u32(bytes, value);
                }
            }
            SourceStamp::Known {
                revision,
                utf16_length,
                utf8_length,
                content_hash128,
            } => {
                for value in [1, revision, utf16_length, utf8_length] {
                    push_u32(bytes, value);
                }
                for word in content_hash128 {
                    push_u32(bytes, word);
                }
            }
        }
    }

    fn observed(
        revision: u32,
        utf16_length: u32,
        utf8_length: u32,
        intent_high_water: u32,
    ) -> ObservedSourceReplicaVersion {
        ObservedSourceReplicaVersion {
            revision,
            utf16_length,
            utf8_length,
            intent_high_water,
        }
    }

    fn canonical_page() -> SourceFactsPageEvent {
        let mut checkpoints =
            [SourceFactCheckpointWire::default(); MAXIMUM_SOURCE_FACT_PAGE_CHECKPOINTS as usize];
        checkpoints[0] = SourceFactCheckpointWire {
            byte_offset: 5,
            utf16_offset: 4,
            logical_line_breaks: 1,
            rolling_hash128: [11, 12, 13, 14],
        };
        checkpoints[1] = SourceFactCheckpointWire {
            byte_offset: 12,
            utf16_offset: 10,
            logical_line_breaks: 2,
            rolling_hash128: [21, 22, 23, 24],
        };
        SourceFactsPageEvent {
            certification_id: 31,
            worker_replica_revision: 7,
            ui_revision: 7,
            utf16_length: 10,
            intent_high_water: 4,
            checkpoint_spacing_utf16: 8,
            page_ordinal: 0,
            page_count: 1,
            checkpoint_count: 2,
            page_checkpoint_count: 2,
            checkpoints,
        }
    }

    fn canonical_completion() -> SourceFactsCompletionEvent {
        SourceFactsCompletionEvent {
            certification_id: 31,
            worker_replica_revision: 7,
            ui_revision: 7,
            utf16_length: 10,
            intent_high_water: 4,
            fingerprint_algorithm: 1,
            utf8_length: 12,
            logical_line_breaks: 2,
            checkpoint_spacing_utf16: 8,
            checkpoint_count: 2,
            page_count: 1,
            content_hash128: [41, 42, 43, 44],
            checkpoint_hash128: [51, 52, 53, 54],
        }
    }

    fn push_observed(bytes: &mut Vec<u8>, observed: Option<ObservedSourceReplicaVersion>) {
        match observed {
            Some(observed) => {
                for value in [
                    1,
                    observed.revision,
                    observed.utf16_length,
                    observed.utf8_length,
                    observed.intent_high_water,
                ] {
                    push_u32(bytes, value);
                }
            }
            None => {
                for _ in 0..5 {
                    push_u32(bytes, 0);
                }
            }
        }
    }

    fn payload(bytes: &[u8]) -> &[u8] {
        v3_wire::decode(bytes, FrameKind::Request, DecodeLimits::default())
            .unwrap()
            .payload
    }

    #[test]
    fn fresh_open_matches_the_dart_golden_and_recovery_is_exact() {
        let fresh = frame(Opcode::ParserOpen, 7, &common(binding(), 0));
        assert_eq!(
            fresh,
            vec![
                0x46, 0x4c, 0x4b, 0x33, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
                0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00,
                0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00,
            ]
        );
        assert_eq!(
            decode_command(&fresh, None).unwrap().command,
            Command::Open {
                binding: binding(),
                mode: OpenMode::Fresh,
            }
        );

        let recovery = frame(Opcode::ParserOpen, 8, &common(with_generation(4), 1));
        assert_eq!(
            decode_command(&recovery, Some(binding())).unwrap().command,
            Command::Open {
                binding: with_generation(4),
                mode: OpenMode::Recovery,
            }
        );
        assert_eq!(
            decode_command(&recovery, Some(with_generation(2)))
                .unwrap_err()
                .failure,
            DecodeFailure::IdentityMismatch
        );
        let crossed_source = SessionBinding {
            source_session_identity: 10,
            ..binding()
        };
        assert_eq!(
            decode_command(&recovery, Some(crossed_source))
                .unwrap_err()
                .failure,
            DecodeFailure::IdentityMismatch
        );
    }

    #[test]
    fn snapshot_command_borrows_utf8_and_snapshot_ack_is_byte_exact() {
        let mut command_payload = common(binding(), 0);
        push_u32(&mut command_payload, 5);
        push_u32(&mut command_payload, 1);
        push_u32(&mut command_payload, 0);
        push_u32(&mut command_payload, 4);
        push_u32(&mut command_payload, 4);
        push_u32(&mut command_payload, 0);
        let target_stamp = known_stamp(1, 4, 6, [0x11, 0x22, 0x33, 0x44]);
        push_source_stamp(&mut command_payload, target_stamp);
        push_u32(&mut command_payload, 4);
        push_u32(&mut command_payload, 6);
        command_payload.extend_from_slice("hi🌍".as_bytes());
        let bytes = frame(Opcode::SnapshotPage, 5, &command_payload);
        // Emitted byte-for-byte by the Dart schema-v3 codec for the same value.
        assert_eq!(
            bytes,
            vec![
                0x46, 0x4c, 0x4b, 0x33, 0x01, 0x00, 0x01, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x62, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
                0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00,
                0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
                0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
                0x04, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0x22, 0x00,
                0x00, 0x00, 0x33, 0x00, 0x00, 0x00, 0x44, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x06, 0x00, 0x00, 0x00, 0x68, 0x69, 0xf0, 0x9f, 0x8c, 0x8d,
            ]
        );
        let decoded = decode_command(&bytes, Some(binding())).unwrap();
        let Command::Snapshot(snapshot) = decoded.command else {
            panic!("expected snapshot command");
        };
        assert!(snapshot.is_seed());
        assert_eq!(snapshot.source, "hi🌍");
        assert_eq!(snapshot.target_stamp, target_stamp);
        assert_eq!(snapshot.source.as_ptr(), bytes[bytes.len() - 6..].as_ptr());

        let event = Event {
            binding: binding(),
            event_id: 11,
            body: EventBody::SourceSynchronized(SourceAcknowledgement::Snapshot {
                source_session_identity: 9,
                lease_id: 5,
                worker_generation: 3,
                base_ui_revision: 1,
                start_utf16: 0,
                end_utf16: 4,
                through_intent_sequence: 0,
                observed_replica: Some(observed(1, 4, 6, 0)),
            }),
        };
        let encoded = encoded_event(event, None);
        // Emitted byte-for-byte by the Dart schema-v3 codec for the same event.
        assert_eq!(
            encoded,
            vec![
                0x46, 0x4c, 0x4b, 0x33, 0x01, 0x00, 0x01, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x44, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02, 0x00,
                0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00,
                0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
                0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
                0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]
        );
        let decoded_frame =
            v3_wire::decode(&encoded, FrameKind::Request, DecodeLimits::default()).unwrap();
        assert_eq!(decoded_frame.header.opcode, Opcode::SnapshotPage);
        assert_eq!(decoded_frame.header.correlation_id, 11);
        let mut expected = common(binding(), 2);
        for value in [5, 1, 0, 4, 0] {
            push_u32(&mut expected, value);
        }
        push_observed(&mut expected, Some(observed(1, 4, 6, 0)));
        assert_eq!(decoded_frame.payload, expected);

        let absent = encoded_event(
            Event {
                binding: binding(),
                event_id: 12,
                body: EventBody::SourceSynchronized(snapshot.acknowledgement(None)),
            },
            None,
        );
        let absent_payload = payload(&absent);
        assert_eq!(
            &absent_payload[absent_payload.len() - OBSERVED_REPLICA_BYTES..],
            &[0; OBSERVED_REPLICA_BYTES]
        );
    }

    #[test]
    fn edit_command_iterates_borrowed_operations_and_ack_is_exact() {
        let mut command_payload = common(binding(), 0);
        for value in [6, 10, 10, 1, 1, 2, 2] {
            push_u32(&mut command_payload, value);
        }
        for value in [10, 0, 1, 1] {
            push_u32(&mut command_payload, value);
        }
        let base_stamp = known_stamp(0, 0, 0, [0; 4]);
        let target_stamp = provisional_stamp(1, 2);
        push_source_stamp(&mut command_payload, base_stamp);
        push_source_stamp(&mut command_payload, target_stamp);
        for value in [0, 0, 2, 2] {
            push_u32(&mut command_payload, value);
        }
        command_payload.extend_from_slice(b"**");
        let bytes = frame(Opcode::Edit, 6, &command_payload);
        let decoded = decode_command(&bytes, Some(binding())).unwrap();
        let Command::Edit(edit) = decoded.command else {
            panic!("expected edit command");
        };
        assert_eq!(edit.intent_count, 1);
        assert_eq!(edit.operation_count, 1);
        assert_eq!(edit.base_stamp(), base_stamp);
        assert_eq!(edit.target_stamp(), target_stamp);
        let intent = edit.intents().next().unwrap();
        assert_eq!(intent.sequence, 10);
        assert_eq!(intent.base_ui_revision, 0);
        assert_eq!(intent.ui_revision, 1);
        assert_eq!(intent.base_stamp, base_stamp);
        assert_eq!(intent.target_stamp, target_stamp);
        let mut operations = intent.operations;
        let operation = operations.next().unwrap();
        assert_eq!(operation.start_utf16, 0);
        assert_eq!(operation.end_utf16, 0);
        assert_eq!(operation.replacement, "**");
        assert_eq!(
            operation.replacement.as_ptr(),
            bytes[bytes.len() - 2..].as_ptr()
        );

        let encoded = encoded_event(
            Event {
                binding: binding(),
                event_id: 12,
                body: EventBody::SourceSynchronized(SourceAcknowledgement::Edit {
                    source_session_identity: 9,
                    lease_id: 6,
                    worker_generation: 3,
                    first_sequence: 10,
                    last_sequence: 10,
                    entry_count: 1,
                    payload_utf16: 2,
                    observed_replica: observed(1, 2, 2, 10),
                }),
            },
            None,
        );
        let decoded_frame =
            v3_wire::decode(&encoded, FrameKind::Request, DecodeLimits::default()).unwrap();
        assert_eq!(decoded_frame.header.opcode, Opcode::Edit);
        let mut expected = common(binding(), 1);
        for value in [6, 10, 10, 1, 2] {
            push_u32(&mut expected, value);
        }
        push_observed(&mut expected, Some(observed(1, 2, 2, 10)));
        assert_eq!(decoded_frame.payload, expected);
    }

    #[test]
    fn edit_command_rejects_revision_gaps_and_overlapping_operations() {
        let mut valid_payload = common(binding(), 0);
        for value in [6, 10, 10, 1, 2, 1, 1] {
            push_u32(&mut valid_payload, value);
        }
        for value in [10, 0, 1, 2] {
            push_u32(&mut valid_payload, value);
        }
        push_source_stamp(&mut valid_payload, provisional_stamp(0, 2));
        push_source_stamp(&mut valid_payload, provisional_stamp(1, 1));
        for value in [0, 2, 0, 0, 2, 2, 1, 1] {
            push_u32(&mut valid_payload, value);
        }
        valid_payload.push(b'x');
        assert!(decode_command(&frame(Opcode::Edit, 6, &valid_payload), Some(binding())).is_ok());

        let mut revision_gap = valid_payload.clone();
        let ui_revision_offset = COMMON_BYTES + 28 + 8;
        revision_gap[ui_revision_offset..ui_revision_offset + 4]
            .copy_from_slice(&2_u32.to_le_bytes());
        assert_eq!(
            decode_command(&frame(Opcode::Edit, 6, &revision_gap), Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::InvalidValue
        );

        let mut overlap = valid_payload;
        let second_operation_offset = COMMON_BYTES + 28 + 16 + SOURCE_STAMP_BYTES * 2 + 16;
        overlap[second_operation_offset..second_operation_offset + 4]
            .copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode_command(&frame(Opcode::Edit, 6, &overlap), Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::InvalidValue
        );
    }

    #[test]
    fn source_stamps_reject_unknown_tags_reserved_words_and_wrong_root_binding() {
        let mut snapshot_payload = common(binding(), 0);
        for value in [5, 1, 0, 1, 1, 0] {
            push_u32(&mut snapshot_payload, value);
        }
        push_source_stamp(&mut snapshot_payload, provisional_stamp(1, 1));
        for value in [1, 1] {
            push_u32(&mut snapshot_payload, value);
        }
        snapshot_payload.push(b'x');
        let stamp_offset = COMMON_BYTES + 24;

        let mut unknown_tag = snapshot_payload.clone();
        unknown_tag[stamp_offset..stamp_offset + 4].copy_from_slice(&7_u32.to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::SnapshotPage, 5, &unknown_tag),
                Some(binding())
            )
            .unwrap_err()
            .failure,
            DecodeFailure::UnknownVariant
        );

        let mut noncanonical_provisional = snapshot_payload.clone();
        noncanonical_provisional[stamp_offset + 12..stamp_offset + 16]
            .copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::SnapshotPage, 5, &noncanonical_provisional),
                Some(binding())
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        for field_offset in [4, 8] {
            let mut wrong_binding = snapshot_payload.clone();
            wrong_binding[stamp_offset + field_offset..stamp_offset + field_offset + 4]
                .copy_from_slice(&2_u32.to_le_bytes());
            assert_eq!(
                decode_command(
                    &frame(Opcode::SnapshotPage, 5, &wrong_binding),
                    Some(binding())
                )
                .unwrap_err()
                .failure,
                DecodeFailure::InvalidValue
            );
        }
    }

    #[test]
    fn edit_stamp_chain_is_exact_across_mixed_known_and_provisional_targets() {
        let mut edit_payload = common(binding(), 0);
        for value in [6, 10, 11, 2, 2, 2, 2] {
            push_u32(&mut edit_payload, value);
        }

        for value in [10, 0, 1, 1] {
            push_u32(&mut edit_payload, value);
        }
        push_source_stamp(&mut edit_payload, known_stamp(0, 0, 0, [0; 4]));
        push_source_stamp(&mut edit_payload, provisional_stamp(1, 1));
        for value in [0, 0, 1, 1] {
            push_u32(&mut edit_payload, value);
        }
        edit_payload.push(b'a');

        for value in [11, 1, 2, 1] {
            push_u32(&mut edit_payload, value);
        }
        push_source_stamp(&mut edit_payload, provisional_stamp(1, 1));
        push_source_stamp(&mut edit_payload, known_stamp(2, 2, 2, [1, 2, 3, 4]));
        for value in [1, 1, 1, 1] {
            push_u32(&mut edit_payload, value);
        }
        edit_payload.push(b'b');

        let frame_bytes = frame(Opcode::Edit, 6, &edit_payload);
        let Command::Edit(command) = decode_command(&frame_bytes, Some(binding()))
            .unwrap()
            .command
        else {
            panic!("expected edit command");
        };
        let intents = command.intents().collect::<Vec<_>>();
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].target_stamp, provisional_stamp(1, 1));
        assert_eq!(intents[1].base_stamp, intents[0].target_stamp);
        assert_eq!(intents[1].target_stamp, known_stamp(2, 2, 2, [1, 2, 3, 4]));

        let first_intent_offset = COMMON_BYTES + 28;
        let first_intent_bytes = 16 + SOURCE_STAMP_BYTES * 2 + 16 + 1;
        let second_base_stamp_offset = first_intent_offset + first_intent_bytes + 16;
        let mut crossed_stamp_kind = edit_payload.clone();
        crossed_stamp_kind[second_base_stamp_offset..second_base_stamp_offset + 4]
            .copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::Edit, 6, &crossed_stamp_kind),
                Some(binding())
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let first_target_revision_offset = first_intent_offset + 16 + SOURCE_STAMP_BYTES + 4;
        let mut wrong_target_revision = edit_payload;
        wrong_target_revision[first_target_revision_offset..first_target_revision_offset + 4]
            .copy_from_slice(&2_u32.to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::Edit, 6, &wrong_target_revision),
                Some(binding())
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );
    }

    #[test]
    fn inline_refinement_is_exact_base_bound_and_coordinate_bounded() {
        let source = refinement_source();
        let ack = refinement_ack();
        let payload = inline_refinement_payload(5, source, ack, 9, 8, 1);
        let frame_bytes = frame(Opcode::ParserRefineInline, 71, &payload);
        let decoded =
            decode_command(&frame_bytes, Some(binding())).expect("valid inline refinement");
        assert_eq!(
            decoded.command,
            Command::RefineInline(InlineRefinementCommand {
                binding: binding(),
                refinement_generation: 5,
                source_version: source,
                base_ack: ack,
                byte_offset: 9,
                utf16_offset: 8,
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            })
        );

        let mut selected_item = payload.clone();
        let target_start = selected_item.len() - 4;
        selected_item[target_start..].copy_from_slice(&1_u32.to_le_bytes());
        let selected_item_frame = frame(Opcode::ParserRefineInline, 72, &selected_item);
        let selected_item = decode_command(&selected_item_frame, Some(binding()))
            .expect("valid selected list-item refinement");
        assert!(matches!(
            selected_item.command,
            Command::RefineInline(InlineRefinementCommand {
                target: InlineRefinementTarget::BulletListItemInline,
                ..
            })
        ));

        let mut selected_item_projection = payload.clone();
        selected_item_projection[target_start..].copy_from_slice(&2_u32.to_le_bytes());
        let selected_item_projection_frame =
            frame(Opcode::ParserRefineInline, 73, &selected_item_projection);
        let selected_item_projection =
            decode_command(&selected_item_projection_frame, Some(binding()))
                .expect("valid selected list-item projection");
        assert!(matches!(
            selected_item_projection.command,
            Command::RefineInline(InlineRefinementCommand {
                target: InlineRefinementTarget::BulletListItemProjection,
                ..
            })
        ));

        let mut selected_ordered_item = payload.clone();
        selected_ordered_item[target_start..].copy_from_slice(&3_u32.to_le_bytes());
        let selected_ordered_item_frame =
            frame(Opcode::ParserRefineInline, 74, &selected_ordered_item);
        let selected_ordered_item = decode_command(&selected_ordered_item_frame, Some(binding()))
            .expect("valid selected ordered-list item refinement");
        assert!(matches!(
            selected_ordered_item.command,
            Command::RefineInline(InlineRefinementCommand {
                target: InlineRefinementTarget::OrderedListItemInline,
                ..
            })
        ));

        let mut selected_ordered_item_projection = payload.clone();
        selected_ordered_item_projection[target_start..].copy_from_slice(&4_u32.to_le_bytes());
        let selected_ordered_item_projection_frame = frame(
            Opcode::ParserRefineInline,
            75,
            &selected_ordered_item_projection,
        );
        let selected_ordered_item_projection =
            decode_command(&selected_ordered_item_projection_frame, Some(binding()))
                .expect("valid selected ordered-list item projection");
        assert!(matches!(
            selected_ordered_item_projection.command,
            Command::RefineInline(InlineRefinementCommand {
                target: InlineRefinementTarget::OrderedListItemProjection,
                ..
            })
        ));

        let mut recursive_green_paragraph = payload.clone();
        recursive_green_paragraph[target_start..].copy_from_slice(&5_u32.to_le_bytes());
        let recursive_green_paragraph_frame =
            frame(Opcode::ParserRefineInline, 76, &recursive_green_paragraph);
        let recursive_green_paragraph =
            decode_command(&recursive_green_paragraph_frame, Some(binding()))
                .expect("valid recursive-Green paragraph refinement");
        assert!(matches!(
            recursive_green_paragraph.command,
            Command::RefineInline(InlineRefinementCommand {
                target: InlineRefinementTarget::RecursiveGreenParagraph,
                ..
            })
        ));

        let mut block_quote_projection = payload.clone();
        block_quote_projection[target_start..].copy_from_slice(&6_u32.to_le_bytes());
        let block_quote_projection_frame =
            frame(Opcode::ParserRefineInline, 77, &block_quote_projection);
        let block_quote_projection = decode_command(&block_quote_projection_frame, Some(binding()))
            .expect("valid block-quote projection");
        assert!(matches!(
            block_quote_projection.command,
            Command::RefineInline(InlineRefinementCommand {
                target: InlineRefinementTarget::BlockQuoteProjection,
                ..
            })
        ));

        let mut invalid_target = payload.clone();
        invalid_target[target_start..].copy_from_slice(&7_u32.to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserRefineInline, 72, &invalid_target),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::UnknownVariant
        );

        let zero_generation = inline_refinement_payload(0, source, ack, 9, 8, 1);
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserRefineInline, 72, &zero_generation),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let mut crossed_source = source;
        crossed_source.revision += 1;
        let mismatch = inline_refinement_payload(6, crossed_source, ack, 9, 8, 1);
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserRefineInline, 73, &mismatch),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let out_of_bounds = inline_refinement_payload(7, source, ack, source.utf8_length + 1, 8, 0);
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserRefineInline, 74, &out_of_bounds),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );
    }

    #[test]
    fn viewport_presentation_is_exact_base_range_and_budget_bound() {
        let source = refinement_source();
        let ack = refinement_ack();
        let payload = viewport_presentation_payload(
            17,
            source,
            ack,
            0,
            0,
            source.utf8_length,
            source.utf16_length,
            u64::from(u32::MAX) + 9,
        );
        let frame_bytes = frame(Opcode::ParserPresentViewport, 81, &payload);
        let decoded =
            decode_command(&frame_bytes, Some(binding())).expect("valid viewport presentation");
        assert_eq!(
            decoded.command,
            Command::PresentViewport(ViewportPresentationCommand {
                binding: binding(),
                viewport_generation: 17,
                source_version: source,
                base_ack: ack,
                requested_start_utf8: 0,
                requested_start_utf16: 0,
                requested_end_utf8: source.utf8_length,
                requested_end_utf16: source.utf16_length,
                start_block_ordinal: u64::from(u32::MAX) + 9,
                start_utf8: 0,
                start_utf16: 0,
                limits: ViewportPresentationLimits {
                    maximum_structural_entries: 47,
                    maximum_storage_pages: 2,
                    maximum_inline_leaves: 24,
                    maximum_inline_leaf_source_bytes: 8 * 1024,
                    maximum_inline_source_bytes: 64 * 1024,
                    maximum_fact_records: 2_048,
                    maximum_encoded_frame_bytes: 64 * 1024,
                    maximum_parser_transitions: 250_000,
                },
            })
        );

        let body_start = COMMON_BYTES + 4 + 44 + 124;
        let start_utf8_offset = body_start + 4 * 6;
        let maximum_inline_leaves_offset = body_start + 4 * 10;
        let maximum_inline_source_bytes_offset = body_start + 4 * 12;

        let mut skipped_prefix = payload.clone();
        skipped_prefix[start_utf8_offset..start_utf8_offset + 4]
            .copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserPresentViewport, 82, &skipped_prefix),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let mut too_many_leaves = payload.clone();
        too_many_leaves[maximum_inline_leaves_offset..maximum_inline_leaves_offset + 4]
            .copy_from_slice(&(MAXIMUM_VIEWPORT_INLINE_LEAVES + 1).to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserPresentViewport, 83, &too_many_leaves),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let mut aggregate_smaller_than_leaf = payload.clone();
        aggregate_smaller_than_leaf
            [maximum_inline_source_bytes_offset..maximum_inline_source_bytes_offset + 4]
            .copy_from_slice(&(4 * 1024_u32).to_le_bytes());
        assert_eq!(
            decode_command(
                &frame(
                    Opcode::ParserPresentViewport,
                    84,
                    &aggregate_smaller_than_leaf,
                ),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let empty = viewport_presentation_payload(18, source, ack, 3, 2, 3, 2, 0);
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserPresentViewport, 85, &empty),
                Some(binding()),
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );
    }

    #[test]
    fn source_receipt_close_and_drain_commands_round_trip() {
        let mut supersede_payload = common(binding(), 0);
        push_u32(&mut supersede_payload, 18);
        let supersede = frame(Opcode::Supersede, 17, &supersede_payload);
        assert_eq!(
            decode_command(&supersede, Some(binding())).unwrap().command,
            Command::Supersede(SupersedeCommand {
                binding: binding(),
                target_ui_revision: 18,
            })
        );

        let mut receipt_payload = common(binding(), EventDisposition::Accepted as u16);
        for value in [
            1,
            0,
            SourceReceiptDisposition::Acknowledged as u32,
            2,
            5,
            3,
            2,
            7,
        ] {
            push_u32(&mut receipt_payload, value);
        }
        let receipt = frame(Opcode::ParserAcknowledge, 19, &receipt_payload);
        let Command::EventReceipt(receipt) =
            decode_command(&receipt, Some(binding())).unwrap().command
        else {
            panic!("expected receipt command");
        };
        assert_eq!(receipt.event_id, 19);
        assert_eq!(receipt.disposition, EventDisposition::Accepted);
        assert_eq!(receipt.source.unwrap().worker_revision, 7);
        assert_eq!(receipt.certification, None);

        let mut close_payload = common(binding(), 0);
        push_u32(&mut close_payload, 3);
        let close = frame(Opcode::Close, 20, &close_payload);
        assert_eq!(
            decode_command(&close, Some(binding())).unwrap().command,
            Command::BeginClose {
                binding: binding(),
                active_generation: 3,
            }
        );

        let mut drain_payload = common(binding(), 0);
        push_u32(&mut drain_payload, 21);
        push_u32(&mut drain_payload, 3);
        let drain = frame(Opcode::Drain, 21, &drain_payload);
        let Command::Drain(grant) = decode_command(&drain, Some(binding())).unwrap().command else {
            panic!("expected drain grant");
        };
        assert_eq!(grant.maximum_transitions, 3);

        let encoded = encoded_event(
            Event {
                binding: binding(),
                event_id: 22,
                body: EventBody::DrainProgress(DrainProgress {
                    drain_id: 21,
                    released_source_leases: 1,
                    released_source_bytes: 4_096,
                    arena_transitions: 2,
                    arena_nodes_reclaimed: 17,
                    complete: true,
                }),
            },
            Some(grant),
        );
        let decoded_frame =
            v3_wire::decode(&encoded, FrameKind::Request, DecodeLimits::default()).unwrap();
        let mut expected = common(binding(), 1);
        for value in [21, 1, 4_096, 2, 17, 1] {
            push_u32(&mut expected, value);
        }
        assert_eq!(decoded_frame.header.opcode, Opcode::Drain);
        assert_eq!(decoded_frame.payload, expected);

        let too_small = DrainGrant {
            maximum_transitions: 2,
            ..grant
        };
        let mut output = vec![0; v3_wire::HEADER_BYTES + MAXIMUM_EVENT_PAYLOAD_BYTES];
        assert_eq!(
            encode_event_into(
                Event {
                    binding: binding(),
                    event_id: 22,
                    body: EventBody::DrainProgress(DrainProgress {
                        drain_id: 21,
                        released_source_leases: 1,
                        released_source_bytes: 4_096,
                        arena_transitions: 2,
                        arena_nodes_reclaimed: 17,
                        complete: true,
                    }),
                },
                binding(),
                Some(too_small),
                &mut output,
            ),
            Err(EncodeError::DrainBudgetExceeded)
        );
    }

    #[test]
    fn canonical_fact_events_and_promotion_receipt_have_exact_schema_three_layouts() {
        let page = canonical_page();
        let encoded_page = encoded_event(
            Event {
                binding: binding(),
                event_id: 30,
                body: EventBody::SourceFactsPage(page),
            },
            None,
        );
        let decoded_page =
            v3_wire::decode(&encoded_page, FrameKind::Request, DecodeLimits::default()).unwrap();
        let mut expected_page = common(binding(), 2);
        for value in [31, 7, 7, 10, 4, 8, 0, 1, 2, 2] {
            push_u32(&mut expected_page, value);
        }
        for checkpoint in page.checkpoints.iter().take(2) {
            for value in [
                checkpoint.byte_offset,
                checkpoint.utf16_offset,
                checkpoint.logical_line_breaks,
            ] {
                push_u32(&mut expected_page, value);
            }
            for word in checkpoint.rolling_hash128 {
                push_u32(&mut expected_page, word);
            }
        }
        assert_eq!(decoded_page.header.opcode, Opcode::ParserPoll);
        assert_eq!(decoded_page.payload, expected_page);

        let completion = canonical_completion();
        let encoded_completion = encoded_event(
            Event {
                binding: binding(),
                event_id: 31,
                body: EventBody::SourceFactsCompleted(completion),
            },
            None,
        );
        let decoded_completion = v3_wire::decode(
            &encoded_completion,
            FrameKind::Request,
            DecodeLimits::default(),
        )
        .unwrap();
        let mut expected_completion = common(binding(), 3);
        for value in [31, 7, 7, 10, 4, 1, 7, 10, 12, 2, 8, 2, 1] {
            push_u32(&mut expected_completion, value);
        }
        for word in [41, 42, 43, 44, 51, 52, 53, 54] {
            push_u32(&mut expected_completion, word);
        }
        assert_eq!(decoded_completion.header.opcode, Opcode::ParserPoll);
        assert_eq!(decoded_completion.payload, expected_completion);

        let delta_begin = SourceFactsDeltaBeginEvent {
            certification_id: 31,
            worker_replica_revision: 7,
            ui_revision: 7,
            utf16_length: 10,
            intent_high_water: 4,
            base_ui_revision: 6,
            base_utf16_length: 9,
            base_utf8_length: 11,
            base_content_hash128: [1, 2, 3, 4],
            base_checkpoint_hash128: [5, 6, 7, 8],
            base_checkpoint_count: 2,
            base_page_count: 1,
            base_checkpoint_spacing_utf16: 8,
            base_page_start: 0,
            base_page_end: 1,
            target_page_start: 0,
            target_page_end: 1,
            target_checkpoint_count: 2,
            target_page_count: 1,
            target_checkpoint_root_guard_algorithm: PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
            target_checkpoint_root_guard128: [51, 52, 53, 54],
            replacement_checkpoint_count: 2,
        };
        let encoded_delta_begin = encoded_event(
            Event {
                binding: binding(),
                event_id: 32,
                body: EventBody::SourceFactsDeltaBegin(delta_begin),
            },
            None,
        );
        let decoded_delta_begin = v3_wire::decode(
            &encoded_delta_begin,
            FrameKind::Request,
            DecodeLimits::default(),
        )
        .unwrap();
        let mut expected_delta_begin = common(binding(), 4);
        for value in [
            31,
            7,
            7,
            10,
            4,
            6,
            9,
            11,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            2,
            1,
            8,
            0,
            1,
            0,
            1,
            2,
            1,
            PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
            51,
            52,
            53,
            54,
            2,
        ] {
            push_u32(&mut expected_delta_begin, value);
        }
        assert_eq!(decoded_delta_begin.header.opcode, Opcode::ParserPoll);
        assert_eq!(decoded_delta_begin.payload, expected_delta_begin);

        let delta_page = SourceFactsDeltaPageEvent {
            certification_id: 31,
            worker_replica_revision: 7,
            ui_revision: 7,
            utf16_length: 10,
            intent_high_water: 4,
            replacement_page_ordinal: 0,
            page_checkpoint_count: 2,
            checkpoints: page.checkpoints,
        };
        let encoded_delta_page = encoded_event(
            Event {
                binding: binding(),
                event_id: 33,
                body: EventBody::SourceFactsDeltaPage(delta_page),
            },
            None,
        );
        let decoded_delta_page = v3_wire::decode(
            &encoded_delta_page,
            FrameKind::Request,
            DecodeLimits::default(),
        )
        .unwrap();
        let mut expected_delta_page = common(binding(), 5);
        for value in [31, 7, 7, 10, 4, 0, 2] {
            push_u32(&mut expected_delta_page, value);
        }
        for checkpoint in delta_page.checkpoints.iter().take(2) {
            for value in [
                checkpoint.byte_offset,
                checkpoint.utf16_offset,
                checkpoint.logical_line_breaks,
            ]
            .into_iter()
            .chain(checkpoint.rolling_hash128)
            {
                push_u32(&mut expected_delta_page, value);
            }
        }
        assert_eq!(decoded_delta_page.header.opcode, Opcode::ParserPoll);
        assert_eq!(decoded_delta_page.payload, expected_delta_page);

        let delta_completion = SourceFactsDeltaCompletionEvent {
            completion,
            checkpoint_root_guard_algorithm: PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
            replacement_checkpoint_hash128: [61, 62, 63, 64],
        };
        let encoded_delta_completion = encoded_event(
            Event {
                binding: binding(),
                event_id: 34,
                body: EventBody::SourceFactsDeltaCompleted(delta_completion),
            },
            None,
        );
        let decoded_delta_completion = v3_wire::decode(
            &encoded_delta_completion,
            FrameKind::Request,
            DecodeLimits::default(),
        )
        .unwrap();
        let mut expected_delta_completion = common(binding(), 6);
        expected_delta_completion.extend_from_slice(&expected_completion[COMMON_BYTES..]);
        for value in [PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM, 61, 62, 63, 64] {
            push_u32(&mut expected_delta_completion, value);
        }
        assert_eq!(decoded_delta_completion.header.opcode, Opcode::ParserPoll);
        assert_eq!(decoded_delta_completion.payload, expected_delta_completion);

        let mut receipt_payload = common(binding(), EventDisposition::Accepted as u16);
        push_u32(&mut receipt_payload, 0);
        push_u32(&mut receipt_payload, 1);
        receipt_payload.extend_from_slice(&expected_completion[COMMON_BYTES..]);
        let receipt = frame(Opcode::ParserAcknowledge, 31, &receipt_payload);
        assert_eq!(
            decode_command(&receipt, Some(binding())).unwrap().command,
            Command::EventReceipt(EventReceiptCommand {
                binding: binding(),
                event_id: 31,
                disposition: EventDisposition::Accepted,
                source: None,
                certification: Some(completion.into()),
            })
        );
    }

    fn persistent_completion(checkpoint_count: u32, page_count: u32) -> SourceFactsCompletionEvent {
        SourceFactsCompletionEvent {
            utf16_length: 440_062,
            utf8_length: 440_062,
            logical_line_breaks: 20_004,
            checkpoint_spacing_utf16: 4_096,
            checkpoint_count,
            page_count,
            ..canonical_completion()
        }
    }

    fn certification_receipt_with(
        completion: SourceFactsCompletionEvent,
    ) -> Result<EventReceiptCommand, DecodeError> {
        let mut proof = vec![0; 84];
        let mut writer = PayloadWriter::new(&mut proof);
        write_source_certification(&mut writer, completion.into());
        assert_eq!(writer.len(), proof.len());
        let mut receipt_payload = common(binding(), EventDisposition::Accepted as u16);
        push_u32(&mut receipt_payload, 0);
        push_u32(&mut receipt_payload, 1);
        receipt_payload.extend_from_slice(&proof);
        let receipt = frame(Opcode::ParserAcknowledge, 31, &receipt_payload);
        let decoded = decode_command(&receipt, Some(binding()))?;
        let Command::EventReceipt(receipt) = decoded.command else {
            unreachable!("ParserAcknowledge decodes only as an event receipt")
        };
        Ok(receipt)
    }

    #[test]
    fn persistent_certification_accepts_bounded_underfilled_page_topology() {
        let underfilled = persistent_completion(109, 3);
        assert!(valid_persistent_page_topology(109, 3));
        assert!(validate_source_certification(binding(), underfilled.into()).is_ok());
        assert!(matches!(
            certification_receipt_with(underfilled).expect("109 checkpoints in 3 pages"),
            EventReceiptCommand {
                certification: Some(SourceCertificationReceipt {
                    checkpoint_count: 109,
                    page_count: 3,
                    ..
                }),
                ..
            }
        ));

        let exact_packed = persistent_completion(109, 2);
        assert!(valid_persistent_page_topology(109, 2));
        assert!(validate_source_certification(binding(), exact_packed.into()).is_ok());
        assert!(certification_receipt_with(exact_packed).is_ok());

        for invalid_pages in [1, 110] {
            let invalid = persistent_completion(109, invalid_pages);
            assert!(!valid_persistent_page_topology(109, invalid_pages));
            assert!(matches!(
                validate_source_certification(binding(), invalid.into()),
                Err(EncodeError::InvalidValue)
            ));
            assert!(matches!(
                certification_receipt_with(invalid),
                Err(DecodeError {
                    failure: DecodeFailure::InvalidValue,
                    ..
                })
            ));
        }
        assert!(valid_persistent_page_topology(0, 0));
        assert!(!valid_persistent_page_topology(0, 1));
    }

    #[test]
    fn clean_completion_stays_packed_but_delta_allows_persistent_topology() {
        let underfilled = persistent_completion(109, 3);
        let mut output = vec![0; v3_wire::HEADER_BYTES + MAXIMUM_EVENT_PAYLOAD_BYTES];
        assert!(matches!(
            encode_event_into(
                Event {
                    binding: binding(),
                    event_id: 41,
                    body: EventBody::SourceFactsCompleted(underfilled),
                },
                binding(),
                None,
                &mut output,
            ),
            Err(EncodeError::InvalidValue)
        ));
        assert!(encode_event_into(
            Event {
                binding: binding(),
                event_id: 42,
                body: EventBody::SourceFactsDeltaCompleted(SourceFactsDeltaCompletionEvent {
                    completion: underfilled,
                    checkpoint_root_guard_algorithm: PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
                    replacement_checkpoint_hash128: [61, 62, 63, 64],
                }),
            },
            binding(),
            None,
            &mut output,
        )
        .is_ok());

        let begin = SourceFactsDeltaBeginEvent {
            certification_id: 31,
            worker_replica_revision: 7,
            ui_revision: 7,
            utf16_length: 440_062,
            intent_high_water: 4,
            base_ui_revision: 6,
            base_utf16_length: 440_057,
            base_utf8_length: 440_057,
            base_content_hash128: [1, 2, 3, 4],
            base_checkpoint_hash128: [5, 6, 7, 8],
            base_checkpoint_count: 109,
            base_page_count: 3,
            base_checkpoint_spacing_utf16: 4_096,
            base_page_start: 1,
            base_page_end: 2,
            target_page_start: 1,
            target_page_end: 2,
            target_checkpoint_count: 109,
            target_page_count: 3,
            target_checkpoint_root_guard_algorithm: PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
            target_checkpoint_root_guard128: [51, 52, 53, 54],
            replacement_checkpoint_count: 1,
        };
        assert!(
            validate_source_facts_delta_begin(binding(), begin).is_ok(),
            "the underfilled target must remain eligible as the next edit base"
        );
        for invalid_base_pages in [1, 110] {
            assert!(matches!(
                validate_source_facts_delta_begin(
                    binding(),
                    SourceFactsDeltaBeginEvent {
                        base_page_count: invalid_base_pages,
                        ..begin
                    },
                ),
                Err(EncodeError::InvalidValue)
            ));
        }
        assert!(matches!(
            validate_source_facts_delta_begin(
                binding(),
                SourceFactsDeltaBeginEvent {
                    replacement_checkpoint_count: 65,
                    ..begin
                },
            ),
            Err(EncodeError::InvalidValue)
        ));
    }

    #[test]
    fn certification_receipts_reject_crossed_fingerprints_and_non_acceptance() {
        let completion = canonical_completion();
        let mut proof = vec![0; 84];
        let mut writer = PayloadWriter::new(&mut proof);
        write_source_certification(&mut writer, completion.into());
        assert_eq!(writer.len(), proof.len());

        let mut crossed = proof.clone();
        crossed[24..28].copy_from_slice(&8_u32.to_le_bytes());
        let mut crossed_payload = common(binding(), EventDisposition::Accepted as u16);
        for flag in [0, 1] {
            push_u32(&mut crossed_payload, flag);
        }
        crossed_payload.extend_from_slice(&crossed);
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserAcknowledge, 31, &crossed_payload),
                Some(binding())
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );

        let mut stale_payload = common(binding(), EventDisposition::Stale as u16);
        for flag in [0, 1] {
            push_u32(&mut stale_payload, flag);
        }
        stale_payload.extend_from_slice(&proof);
        assert_eq!(
            decode_command(
                &frame(Opcode::ParserAcknowledge, 31, &stale_payload),
                Some(binding())
            )
            .unwrap_err()
            .failure,
            DecodeFailure::InvalidValue
        );
    }

    #[test]
    fn opened_failure_and_closed_events_use_the_exact_variants() {
        for (event_id, body, opcode, variant, tail) in [
            (
                1,
                EventBody::Opened(OpenMode::Fresh),
                Opcode::ParserOpen,
                2,
                None,
            ),
            (
                2,
                EventBody::Failed {
                    failure_code: 0x1020,
                },
                Opcode::ParserPoll,
                1,
                Some(0x1020),
            ),
            (3, EventBody::Closed, Opcode::Close, 1, None),
        ] {
            let encoded = encoded_event(
                Event {
                    binding: binding(),
                    event_id,
                    body,
                },
                None,
            );
            let decoded =
                v3_wire::decode(&encoded, FrameKind::Request, DecodeLimits::default()).unwrap();
            assert_eq!(decoded.header.opcode, opcode);
            let mut expected = common(binding(), variant);
            if let Some(value) = tail {
                push_u32(&mut expected, value);
            }
            assert_eq!(decoded.payload, expected);
        }

        let encoded = encoded_event(
            Event {
                binding: binding(),
                event_id: 4,
                body: EventBody::InlineRefinementUnavailable(InlineRefinementUnavailableEvent {
                    refinement_generation: 9,
                    reason_code: 2,
                }),
            },
            None,
        );
        let decoded =
            v3_wire::decode(&encoded, FrameKind::Request, DecodeLimits::default()).unwrap();
        assert_eq!(decoded.header.opcode, Opcode::ParserPoll);
        let mut expected = common(binding(), 7);
        push_u32(&mut expected, 9);
        push_u32(&mut expected, 2);
        assert_eq!(decoded.payload, expected);

        let encoded = encoded_event(
            Event {
                binding: binding(),
                event_id: 5,
                body: EventBody::ViewportPresentationUnavailable(
                    ViewportPresentationUnavailableEvent {
                        viewport_generation: 11,
                        reason_code: 2,
                    },
                ),
            },
            None,
        );
        let decoded =
            v3_wire::decode(&encoded, FrameKind::Request, DecodeLimits::default()).unwrap();
        assert_eq!(decoded.header.opcode, Opcode::ParserPoll);
        let mut expected = common(binding(), 8);
        push_u32(&mut expected, 11);
        push_u32(&mut expected, 2);
        assert_eq!(decoded.payload, expected);
    }

    #[test]
    fn decoder_rejects_wrong_direction_before_short_body() {
        let wrong = frame(Opcode::PublishAbort, 1, &[]);
        let error = decode_command(&wrong, Some(binding())).unwrap_err();
        assert_eq!(error.failure, DecodeFailure::UnexpectedOpcode);
        assert_eq!(error.byte_offset, 8);
    }

    #[test]
    fn decoder_fails_closed_on_schema_variant_identity_utf8_and_bounds() {
        let fresh = frame(Opcode::ParserOpen, 1, &common(binding(), 0));

        let mut schema = fresh.clone();
        schema[v3_wire::HEADER_BYTES] = 1;
        assert_eq!(
            decode_command(&schema, None).unwrap_err().failure,
            DecodeFailure::UnsupportedSchema
        );

        let mut variant = fresh.clone();
        variant[v3_wire::HEADER_BYTES + 2] = 7;
        assert_eq!(
            decode_command(&variant, None).unwrap_err().failure,
            DecodeFailure::UnknownVariant
        );

        for offset in [4, 24] {
            let mut invalid_identity = fresh.clone();
            let start = v3_wire::HEADER_BYTES + offset;
            invalid_identity[start..start + 4].copy_from_slice(&0_u32.to_le_bytes());
            assert_eq!(
                decode_command(&invalid_identity, None).unwrap_err().failure,
                DecodeFailure::InvalidValue
            );
        }

        let mut snapshot_payload = common(binding(), 0);
        for value in [5, 1, 0, 1, 1, 0] {
            push_u32(&mut snapshot_payload, value);
        }
        push_source_stamp(&mut snapshot_payload, provisional_stamp(1, 1));
        for value in [1, 1] {
            push_u32(&mut snapshot_payload, value);
        }
        snapshot_payload.push(b'x');
        let snapshot = frame(Opcode::SnapshotPage, 5, &snapshot_payload);

        let mut invalid_utf8 = snapshot.clone();
        *invalid_utf8.last_mut().unwrap() = 0xff;
        assert_eq!(
            decode_command(&invalid_utf8, Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::InvalidUtf8
        );

        let mut oversized = snapshot.clone();
        let source_utf16_offset = v3_wire::HEADER_BYTES + COMMON_BYTES + 24 + SOURCE_STAMP_BYTES;
        oversized[source_utf16_offset..source_utf16_offset + 4]
            .copy_from_slice(&(MAXIMUM_SNAPSHOT_UTF16 + 1).to_le_bytes());
        assert_eq!(
            decode_command(&oversized, Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::OversizedValue
        );

        let mut wrong_correlation = snapshot.clone();
        wrong_correlation[16..20].copy_from_slice(&6_u32.to_le_bytes());
        assert_eq!(
            decode_command(&wrong_correlation, Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::IdentityMismatch
        );

        let mut trailing_payload = snapshot_payload;
        trailing_payload.push(0);
        let trailing = frame(Opcode::SnapshotPage, 5, &trailing_payload);
        assert_eq!(
            decode_command(&trailing, Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::TrailingPayload
        );

        let mut stale_close_payload = common(binding(), 0);
        push_u32(&mut stale_close_payload, 2);
        let stale_close = frame(Opcode::Close, 2, &stale_close_payload);
        assert_eq!(
            decode_command(&stale_close, Some(binding()))
                .unwrap_err()
                .failure,
            DecodeFailure::IdentityMismatch
        );
    }

    #[test]
    fn event_encoder_rejects_cross_session_and_short_output() {
        let event = Event {
            binding: binding(),
            event_id: 1,
            body: EventBody::Closed,
        };
        let crossed = SessionBinding {
            source_session_identity: 10,
            ..binding()
        };
        let mut output = vec![0; v3_wire::HEADER_BYTES + COMMON_BYTES];
        assert_eq!(
            encode_event_into(event, crossed, None, &mut output),
            Err(EncodeError::IdentityMismatch)
        );
        let short_length = output.len() - 1;
        assert_eq!(
            encode_event_into(event, binding(), None, &mut output[..short_length]),
            Err(EncodeError::Envelope(
                v3_wire::EncodeError::BufferTooSmall {
                    required: v3_wire::HEADER_BYTES + COMMON_BYTES,
                    available: v3_wire::HEADER_BYTES + COMMON_BYTES - 1,
                }
            ))
        );
    }

    #[test]
    fn malformed_edit_totals_and_stale_receipt_drops_are_rejected() {
        let mut edit_payload = common(binding(), 0);
        for value in [6, 10, 10, 1, 1, 3, 2] {
            push_u32(&mut edit_payload, value);
        }
        for value in [10, 0, 1, 1] {
            push_u32(&mut edit_payload, value);
        }
        push_source_stamp(&mut edit_payload, known_stamp(0, 0, 0, [0; 4]));
        push_source_stamp(&mut edit_payload, provisional_stamp(1, 2));
        for value in [0, 0, 2, 2] {
            push_u32(&mut edit_payload, value);
        }
        edit_payload.extend_from_slice(b"**");
        let edit = frame(Opcode::Edit, 6, &edit_payload);
        assert_eq!(
            decode_command(&edit, Some(binding())).unwrap_err().failure,
            DecodeFailure::InvalidValue
        );

        let mut stale_receipt = common(binding(), EventDisposition::Stale as u16);
        for value in [1, 0, SourceReceiptDisposition::Stale as u32, 1, 0, 0, 0, 7] {
            push_u32(&mut stale_receipt, value);
        }
        let stale = frame(Opcode::ParserAcknowledge, 19, &stale_receipt);
        assert_eq!(
            decode_command(&stale, Some(binding())).unwrap_err().failure,
            DecodeFailure::InvalidValue
        );
    }

    #[test]
    fn payload_helper_borrows_the_envelope_for_test_parity() {
        let bytes = frame(Opcode::Close, 1, &common(binding(), 1));
        assert_eq!(payload(&bytes), common(binding(), 1));
    }
}
