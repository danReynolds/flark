//! Authority-bound publication and independent-host access for recursive Green.

use crate::candidate_manifest::StrongIdentity;
use crate::identity::ArenaId;
use crate::measured_sequence::{
    retain_committed_measured_sequence_root_with_measure, validate_measured_sequence_node,
    MeasuredSequenceRef, SequenceInspectionReceipt, SequenceMutationReceipt,
};
use crate::source::SourceVersion;
use crate::storage::{ArenaBuildOwner, ArenaBuildSession, PageArena};

use super::build::M11RecursiveGreenRoot;
use super::codec::{
    M11RecursiveGreenError, RecursiveGreenCommitment, RecursiveGreenSpec, RecursiveGreenSummary,
};
use super::query::{
    locate_point_in_arena_bounded, locate_renderable_row_ordinal_window_in_arena,
    locate_renderable_rows_in_arena, M11RecursiveGreenPoint, M11RecursiveGreenPointQueryOutcome,
    M11RecursiveGreenRowOrdinalWindow, M11RecursiveGreenRowQueryLimits,
    M11RecursiveGreenRowQueryOutcome,
};

const RECURSIVE_GREEN_ROLE_DESCRIPTOR_MAGIC: [u8; 4] = *b"RGR1";
const RECURSIVE_GREEN_ROLE_DESCRIPTOR_SCHEMA: u32 = 2;

/// Encoded descriptor appended to the authority-bound Green role wrapper.
pub(crate) const PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES: usize = 128;

/// Compact authenticated description of one persistent recursive Green root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11RecursiveGreenRoleDescriptor {
    source_bytes: u64,
    source_utf16: u64,
    logical_bytes: u64,
    logical_utf16: u64,
    events: u64,
    enters: u64,
    renderable_row_exits: u64,
    maximum_frame_id: u64,
    storage_page_count: u64,
    tree_height: u16,
    canonical_event_bytes: u64,
    commitment256: [u8; 32],
}

impl PersistentM11RecursiveGreenRoleDescriptor {
    pub(crate) const fn event_count(self) -> u64 {
        self.events
    }

    pub(crate) const fn renderable_row_count(self) -> u64 {
        self.renderable_row_exits
    }

    pub(crate) const fn record_count(self) -> u64 {
        self.events
    }

    pub(crate) const fn canonical_bytes(self) -> u64 {
        self.canonical_event_bytes
    }

    pub(crate) const fn canonical_event_bytes(self) -> u64 {
        self.canonical_event_bytes
    }

    pub(crate) const fn source_bytes(self) -> u64 {
        self.source_bytes
    }

    pub(crate) const fn source_utf16(self) -> u64 {
        self.source_utf16
    }

    pub(crate) const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    pub(crate) const fn tree_height(self) -> u16 {
        self.tree_height
    }

    pub(crate) const fn canonical_commitment256(self) -> [u8; 32] {
        self.commitment256
    }
}

/// Fully validated host claim for one imported recursive Green root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistentM11RecursiveGreenRootClaim {
    summary: RecursiveGreenSummary,
    storage_page_count: u64,
    tree_height: u16,
}

impl PersistentM11RecursiveGreenRootClaim {
    pub(super) const fn summary(self) -> RecursiveGreenSummary {
        self.summary
    }

    pub(crate) const fn source_bytes(self) -> u64 {
        self.summary.physical_bytes
    }

    pub(crate) const fn source_utf16(self) -> u64 {
        self.summary.physical_utf16
    }

    pub(crate) const fn event_count(self) -> u64 {
        self.summary.events
    }

    pub(crate) const fn renderable_row_count(self) -> u64 {
        self.summary.renderable_row_exits
    }

    pub(crate) const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    pub(crate) const fn tree_height(self) -> u16 {
        self.tree_height
    }
}

pub(crate) struct M11RetainedRecursiveGreenRoot {
    owner: Option<ArenaBuildOwner>,
    summary: RecursiveGreenSummary,
    page_count: u64,
    tree_height: u16,
    inspection: SequenceInspectionReceipt,
}

impl M11RetainedRecursiveGreenRoot {
    pub(crate) fn take_owner(&mut self) -> Option<ArenaBuildOwner> {
        self.owner.take()
    }

    pub(crate) const fn event_count(&self) -> u64 {
        self.summary.events
    }

    pub(crate) const fn canonical_event_bytes(&self) -> u64 {
        self.summary.canonical_event_bytes
    }

    pub(crate) const fn page_count(&self) -> u64 {
        self.page_count
    }

    pub(crate) const fn tree_height(&self) -> u16 {
        self.tree_height
    }

    pub(crate) const fn inspection(&self) -> SequenceInspectionReceipt {
        self.inspection
    }

    pub(crate) fn descriptor(&self) -> PersistentM11RecursiveGreenRoleDescriptor {
        descriptor_for(self.summary, self.page_count, self.tree_height)
    }
}

impl M11RecursiveGreenRoot {
    pub(crate) fn retain_for_publication(
        &self,
        session: &mut ArenaBuildSession<'_>,
        expected_runtime_identity: StrongIdentity,
        expected_source: SourceVersion,
    ) -> Result<M11RetainedRecursiveGreenRoot, M11RecursiveGreenError> {
        if self.released
            || self.lease.is_none()
            || self.runtime_identity != expected_runtime_identity
            || self.source != expected_source
        {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        let mut mutation = SequenceMutationReceipt::default();
        let owner = match self.tree.as_ref() {
            Some(tree) => {
                let (retained, measure) = retain_committed_measured_sequence_root_with_measure(
                    session,
                    tree,
                    &mut mutation,
                )?;
                if measure.summary() != self.summary
                    || measure.leaves() != self.page_count
                    || measure.height() != self.tree_height
                {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "retained recursive Green root summary changed",
                    ));
                }
                Some(retained.into_owner())
            }
            None => {
                if self.summary != RecursiveGreenSummary::empty()
                    || self.page_count != 0
                    || self.tree_height != 0
                {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "empty recursive Green root changed shape",
                    ));
                }
                None
            }
        };
        Ok(M11RetainedRecursiveGreenRoot {
            owner,
            summary: self.summary,
            page_count: self.page_count,
            tree_height: self.tree_height,
            inspection: mutation.inspection,
        })
    }
}

pub(super) fn descriptor_for(
    summary: RecursiveGreenSummary,
    storage_page_count: u64,
    tree_height: u16,
) -> PersistentM11RecursiveGreenRoleDescriptor {
    PersistentM11RecursiveGreenRoleDescriptor {
        source_bytes: summary.physical_bytes,
        source_utf16: summary.physical_utf16,
        logical_bytes: summary.logical_bytes,
        logical_utf16: summary.logical_utf16,
        events: summary.events,
        enters: summary.enters,
        renderable_row_exits: summary.renderable_row_exits,
        maximum_frame_id: summary.max_frame_id,
        storage_page_count,
        tree_height,
        canonical_event_bytes: summary.canonical_event_bytes,
        commitment256: summary.canonical_commitment.checksum(),
    }
}

pub(crate) fn encode_persistent_m11_recursive_green_role_descriptor(
    descriptor: PersistentM11RecursiveGreenRoleDescriptor,
) -> Result<[u8; PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES], M11RecursiveGreenError> {
    validate_descriptor(descriptor)?;
    let mut output = [0_u8; PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES];
    let mut cursor = 0;
    write_bytes(
        &mut output,
        &mut cursor,
        &RECURSIVE_GREEN_ROLE_DESCRIPTOR_MAGIC,
    )?;
    write_u32(
        &mut output,
        &mut cursor,
        RECURSIVE_GREEN_ROLE_DESCRIPTOR_SCHEMA,
    )?;
    for value in [
        descriptor.source_bytes,
        descriptor.source_utf16,
        descriptor.logical_bytes,
        descriptor.logical_utf16,
        descriptor.events,
        descriptor.enters,
        descriptor.renderable_row_exits,
        descriptor.maximum_frame_id,
        descriptor.storage_page_count,
    ] {
        write_u64(&mut output, &mut cursor, value)?;
    }
    write_u16(&mut output, &mut cursor, descriptor.tree_height)?;
    write_bytes(&mut output, &mut cursor, &[0; 6])?;
    write_u64(&mut output, &mut cursor, descriptor.canonical_event_bytes)?;
    write_bytes(&mut output, &mut cursor, &descriptor.commitment256)?;
    if cursor != output.len() {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green role descriptor encoding length changed",
        ));
    }
    Ok(output)
}

pub(crate) fn decode_persistent_m11_recursive_green_role_descriptor(
    input: &[u8],
    source_bytes: u64,
    source_utf16: u64,
) -> Result<PersistentM11RecursiveGreenRoleDescriptor, M11RecursiveGreenError> {
    if input.len() != PERSISTENT_RECURSIVE_GREEN_ROLE_DESCRIPTOR_BYTES
        || input.get(..4) != Some(RECURSIVE_GREEN_ROLE_DESCRIPTOR_MAGIC.as_slice())
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green role descriptor has the wrong shape",
        ));
    }
    let mut cursor = 4;
    if read_u32(input, &mut cursor)? != RECURSIVE_GREEN_ROLE_DESCRIPTOR_SCHEMA {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green role descriptor schema is unsupported",
        ));
    }
    let descriptor = PersistentM11RecursiveGreenRoleDescriptor {
        source_bytes: read_u64(input, &mut cursor)?,
        source_utf16: read_u64(input, &mut cursor)?,
        logical_bytes: read_u64(input, &mut cursor)?,
        logical_utf16: read_u64(input, &mut cursor)?,
        events: read_u64(input, &mut cursor)?,
        enters: read_u64(input, &mut cursor)?,
        renderable_row_exits: read_u64(input, &mut cursor)?,
        maximum_frame_id: read_u64(input, &mut cursor)?,
        storage_page_count: read_u64(input, &mut cursor)?,
        tree_height: {
            let value = read_u16(input, &mut cursor)?;
            if read_bytes(input, &mut cursor, 6)? != [0; 6] {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive Green role descriptor reserved bytes changed",
                ));
            }
            value
        },
        canonical_event_bytes: read_u64(input, &mut cursor)?,
        commitment256: read_array_32(input, &mut cursor)?,
    };
    if cursor != input.len()
        || descriptor.source_bytes != source_bytes
        || descriptor.source_utf16 != source_utf16
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green role descriptor source changed",
        ));
    }
    validate_descriptor(descriptor)?;
    Ok(descriptor)
}

pub(super) fn validate_descriptor(
    descriptor: PersistentM11RecursiveGreenRoleDescriptor,
) -> Result<(), M11RecursiveGreenError> {
    let empty_commitment = RecursiveGreenCommitment::empty().checksum();
    if descriptor.source_bytes < descriptor.source_utf16
        || descriptor.logical_bytes < descriptor.logical_utf16
        || descriptor.enters > descriptor.events
        || descriptor.renderable_row_exits > descriptor.enters
        || (descriptor.enters == 0) != (descriptor.maximum_frame_id == 0)
        || (descriptor.events == 0) != (descriptor.canonical_event_bytes == 0)
        || (descriptor.events == 0) != (descriptor.commitment256 == empty_commitment)
        || (descriptor.events == 0)
            != (descriptor.storage_page_count == 0 && descriptor.tree_height == 0)
        || (descriptor.events != 0
            && (descriptor.storage_page_count == 0 || descriptor.tree_height == 0))
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green role descriptor metrics are invalid",
        ));
    }
    Ok(())
}

pub(crate) fn validate_persistent_m11_recursive_green_root(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11RecursiveGreenRoleDescriptor,
) -> Result<PersistentM11RecursiveGreenRootClaim, M11RecursiveGreenError> {
    validate_descriptor(descriptor)?;
    match root {
        None if descriptor.events == 0 => Ok(PersistentM11RecursiveGreenRootClaim {
            summary: RecursiveGreenSummary::empty(),
            storage_page_count: 0,
            tree_height: 0,
        }),
        None => Err(M11RecursiveGreenError::Corrupt(
            "nonempty recursive Green descriptor lost its root",
        )),
        Some(_) if descriptor.events == 0 => Err(M11RecursiveGreenError::Corrupt(
            "empty recursive Green descriptor owns a root",
        )),
        Some(root) => {
            let mut inspection = SequenceInspectionReceipt::default();
            let measure = validate_measured_sequence_node::<RecursiveGreenSpec>(
                arena,
                root,
                &mut inspection,
            )?;
            let summary = measure.summary();
            if descriptor_for(summary, measure.leaves(), measure.height()) != descriptor {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive Green root differs from its descriptor",
                ));
            }
            Ok(PersistentM11RecursiveGreenRootClaim {
                summary,
                storage_page_count: measure.leaves(),
                tree_height: measure.height(),
            })
        }
    }
}

pub(crate) fn is_m11_recursive_green_node_payload(payload: &[u8]) -> bool {
    matches!(payload.get(..4), Some(magic) if magic == b"RGL1" || magic == b"RGB1")
}

pub(crate) fn validate_imported_m11_recursive_green_node(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(), M11RecursiveGreenError> {
    if !is_m11_recursive_green_node_payload(arena.payload(id)?) {
        return Err(M11RecursiveGreenError::Corrupt(
            "imported recursive Green node has the wrong payload kind",
        ));
    }
    let mut inspection = SequenceInspectionReceipt::default();
    let _ = validate_measured_sequence_node::<RecursiveGreenSpec>(arena, id, &mut inspection)?;
    Ok(())
}

pub(crate) fn persistent_m11_recursive_green_locate_point(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11RecursiveGreenRootClaim,
    point: M11RecursiveGreenPoint,
    maximum_tree_nodes_visited: u64,
) -> Result<M11RecursiveGreenPointQueryOutcome, M11RecursiveGreenError> {
    if (claim.event_count() == 0) != root.is_none() {
        return Err(M11RecursiveGreenError::Corrupt(
            "installed recursive Green root changed shape",
        ));
    }
    locate_point_in_arena_bounded(
        arena,
        MeasuredSequenceRef::<RecursiveGreenSpec>::from_imported_root(root),
        claim.summary,
        point,
        maximum_tree_nodes_visited,
    )
}

pub(crate) fn persistent_m11_recursive_green_locate_rows(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11RecursiveGreenRootClaim,
    point: M11RecursiveGreenPoint,
    requested_end_byte: u64,
    limits: M11RecursiveGreenRowQueryLimits,
) -> Result<M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenError> {
    if (claim.event_count() == 0) != root.is_none() {
        return Err(M11RecursiveGreenError::Corrupt(
            "installed recursive Green root changed shape",
        ));
    }
    locate_renderable_rows_in_arena(
        arena,
        MeasuredSequenceRef::<RecursiveGreenSpec>::from_imported_root(root),
        claim.summary,
        point,
        requested_end_byte,
        limits,
    )
}

pub(crate) fn persistent_m11_recursive_green_locate_row_ordinal_window(
    arena: &PageArena,
    root: Option<ArenaId>,
    claim: PersistentM11RecursiveGreenRootClaim,
    start_ordinal: u64,
    maximum_rows: u32,
) -> Result<M11RecursiveGreenRowOrdinalWindow, M11RecursiveGreenError> {
    if (claim.event_count() == 0) != root.is_none() {
        return Err(M11RecursiveGreenError::Corrupt(
            "installed recursive Green root changed shape",
        ));
    }
    locate_renderable_row_ordinal_window_in_arena(
        arena,
        MeasuredSequenceRef::<RecursiveGreenSpec>::from_imported_root(root),
        claim.summary,
        start_ordinal,
        maximum_rows,
    )
}

fn write_bytes(
    output: &mut [u8],
    cursor: &mut usize,
    bytes: &[u8],
) -> Result<(), M11RecursiveGreenError> {
    let end = cursor
        .checked_add(bytes.len())
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    output
        .get_mut(*cursor..end)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "recursive Green descriptor write exceeded its envelope",
        ))?
        .copy_from_slice(bytes);
    *cursor = end;
    Ok(())
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
            "recursive Green descriptor read exceeded its envelope",
        ))?;
    *cursor = end;
    Ok(bytes)
}

fn read_u16(input: &[u8], cursor: &mut usize) -> Result<u16, M11RecursiveGreenError> {
    let bytes: [u8; 2] = read_bytes(input, cursor, 2)?
        .try_into()
        .map_err(|_| M11RecursiveGreenError::Corrupt("invalid recursive Green u16"))?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(input: &[u8], cursor: &mut usize) -> Result<u32, M11RecursiveGreenError> {
    let bytes: [u8; 4] = read_bytes(input, cursor, 4)?
        .try_into()
        .map_err(|_| M11RecursiveGreenError::Corrupt("invalid recursive Green u32"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64, M11RecursiveGreenError> {
    let bytes: [u8; 8] = read_bytes(input, cursor, 8)?
        .try_into()
        .map_err(|_| M11RecursiveGreenError::Corrupt("invalid recursive Green u64"))?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_array_32(input: &[u8], cursor: &mut usize) -> Result<[u8; 32], M11RecursiveGreenError> {
    read_bytes(input, cursor, 32)?
        .try_into()
        .map_err(|_| M11RecursiveGreenError::Corrupt("invalid recursive Green commitment"))
}
