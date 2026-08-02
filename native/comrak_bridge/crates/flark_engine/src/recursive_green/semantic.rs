//! Canonical semantic equality for independently allocated Green roots.

use crate::document::DocumentRuntime;
use crate::measured_sequence::{
    SequenceInspectionReceipt, SequenceLeafVisitControl, SequenceSpecInspection,
};

use super::build::M11RecursiveGreenRoot;
use super::codec::{
    decode_leaf, decode_packed_event, LogicalAtom, M11RecursiveGreenError, M11RecursiveGreenEvent,
    M11RecursiveGreenFrameId, M11RecursiveGreenLogicalAction, PackedGreenEvent,
};

/// Opaque identity of one committed recursive-Green storage page.
///
/// Equality is exposed only so incremental gates can prove that a distant
/// source region retained the exact arena object across a path-copy splice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenStoragePageIdentity(crate::ArenaId);

impl M11RecursiveGreenRoot {
    /// Visits the authenticated semantic event stream in source order.
    ///
    /// This is a diagnostic conformance seam, not a viewport query. It walks
    /// the complete root and deliberately returns no storage identities or raw
    /// page bytes. Production consumers should use the bounded point/frame
    /// queries instead.
    #[doc(hidden)]
    pub fn visit_semantic_events_for_diagnostics(
        &self,
        runtime: &DocumentRuntime,
        mut visit: impl FnMut(M11RecursiveGreenEvent),
    ) -> Result<u64, M11RecursiveGreenError> {
        self.ensure_runtime(runtime)?;
        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut inspection = SequenceInspectionReceipt::default();
        let mut events = 0_u64;
        tree.as_ref().visit_leaves_from_metric(
            runtime.producer_arena(),
            0,
            |summary| summary.events,
            &mut inspection,
            |located| {
                let payload = runtime.producer_arena().payload(located.id)?;
                let mut local_inspection = SequenceSpecInspection::default();
                let leaf = decode_leaf(payload, &mut local_inspection)?.ok_or(
                    M11RecursiveGreenError::Corrupt("semantic visitor selected a branch payload"),
                )?;
                let mut cursor = 0_usize;
                for _ in 0..leaf.events {
                    let event = decode_packed_event(leaf.event_bytes, &mut cursor)?;
                    visit(unpack_diagnostic_event(event));
                    events = events
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
                if cursor != leaf.event_bytes.len() {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "semantic visitor did not consume one Green leaf",
                    ));
                }
                Ok(SequenceLeafVisitControl::Continue)
            },
        )?;
        if events != self.event_count() {
            return Err(M11RecursiveGreenError::Corrupt(
                "semantic visitor event count differs from the root summary",
            ));
        }
        Ok(events)
    }

    /// Returns the opaque committed leaf identity containing one source byte.
    #[doc(hidden)]
    pub fn storage_page_identity_at_source_byte(
        &self,
        runtime: &DocumentRuntime,
        byte_offset: usize,
    ) -> Result<M11RecursiveGreenStoragePageIdentity, M11RecursiveGreenError> {
        self.ensure_storage_live(runtime)?;
        if byte_offset >= self.source().byte_len() {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut inspection = SequenceInspectionReceipt::default();
        let leaf = tree
            .as_ref()
            .locate_leaf_containing_metric(
                runtime.producer_arena(),
                u64::try_from(byte_offset).map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
                |summary| summary.physical_bytes,
                &mut inspection,
            )?
            .ok_or(M11RecursiveGreenError::InvalidPoint)?;
        Ok(M11RecursiveGreenStoragePageIdentity(leaf.id))
    }

    /// Hashes the complete semantic event stream while alpha-renaming frame
    /// identities by Enter order.
    ///
    /// Frame allocation is deliberately not semantic: an incremental root
    /// mints replacement IDs above its retained base maximum, whereas a clean
    /// parse mints compact IDs. Every kind, property, coverage atom, owner
    /// depth, close fact, and stack pairing remains byte-for-byte significant.
    #[doc(hidden)]
    pub fn semantic_digest(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<[u8; 32], M11RecursiveGreenError> {
        self.ensure_runtime(runtime)?;
        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut inspection = SequenceInspectionReceipt::default();
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"flark.recursive-green.semantic.v1\0");
        let mut open: Vec<(M11RecursiveGreenFrameId, u64)> = Vec::new();
        let mut next_frame = 1_u64;
        let mut events = 0_u64;
        tree.as_ref().visit_leaves_from_metric(
            runtime.producer_arena(),
            0,
            |summary| summary.events,
            &mut inspection,
            |located| {
                let payload = runtime.producer_arena().payload(located.id)?;
                let mut local_inspection = SequenceSpecInspection::default();
                let leaf = decode_leaf(payload, &mut local_inspection)?.ok_or(
                    M11RecursiveGreenError::Corrupt("semantic digest selected a branch payload"),
                )?;
                let mut cursor = 0_usize;
                for _ in 0..leaf.events {
                    let event = decode_packed_event(leaf.event_bytes, &mut cursor)?;
                    hash_event(&mut hasher, &mut open, &mut next_frame, event)?;
                    events = events
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
                if cursor != leaf.event_bytes.len() {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "semantic digest did not consume one Green leaf",
                    ));
                }
                Ok(SequenceLeafVisitControl::Continue)
            },
        )?;
        if !open.is_empty() || events != self.event_count() {
            return Err(M11RecursiveGreenError::Corrupt(
                "semantic digest ended with unmatched Green structure",
            ));
        }
        Ok(*hasher.finalize().as_bytes())
    }
}

fn unpack_diagnostic_event(event: PackedGreenEvent) -> M11RecursiveGreenEvent {
    match event {
        PackedGreenEvent::Enter { frame, kind } => M11RecursiveGreenEvent::Enter { frame, kind },
        PackedGreenEvent::Property(property) => M11RecursiveGreenEvent::Property(property),
        PackedGreenEvent::Coverage {
            physical,
            owner_depth,
            part,
            atom,
        } => M11RecursiveGreenEvent::Coverage {
            physical,
            owner_depth,
            part,
            logical: match atom {
                LogicalAtom::None => M11RecursiveGreenLogicalAction::None,
                LogicalAtom::Identity => M11RecursiveGreenLogicalAction::Identity,
                LogicalAtom::TabToSpaces {
                    target_owner_depth,
                    spaces,
                } => M11RecursiveGreenLogicalAction::PartialTab {
                    target_owner_depth,
                    remaining_spaces: spaces,
                },
                LogicalAtom::HiddenUpstream => M11RecursiveGreenLogicalAction::HiddenUpstream,
                LogicalAtom::LfToLf | LogicalAtom::CrLfToLf | LogicalAtom::LoneCrToLf => {
                    M11RecursiveGreenLogicalAction::CanonicalNewline
                }
                LogicalAtom::NulToReplacement => M11RecursiveGreenLogicalAction::CanonicalText,
            },
        },
        PackedGreenEvent::RetypeOpen {
            frame,
            kind,
            property,
        } => M11RecursiveGreenEvent::RetypeOpen {
            frame,
            kind,
            property,
        },
        PackedGreenEvent::Exit {
            frame,
            final_kind,
            close,
            last_line_blank,
            child,
        } => M11RecursiveGreenEvent::Exit {
            frame,
            final_kind,
            close,
            last_line_blank,
            child,
        },
    }
}

fn hash_event(
    hasher: &mut blake3::Hasher,
    open: &mut Vec<(M11RecursiveGreenFrameId, u64)>,
    next_frame: &mut u64,
    event: PackedGreenEvent,
) -> Result<(), M11RecursiveGreenError> {
    match event {
        PackedGreenEvent::Enter { frame, kind } => {
            let canonical = *next_frame;
            *next_frame = next_frame
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            open.push((frame, canonical));
            hasher.update(&[1]);
            hasher.update(&canonical.to_le_bytes());
            hasher.update(&kind.get().to_le_bytes());
        }
        PackedGreenEvent::Property(property) => {
            if open.is_empty() {
                return Err(M11RecursiveGreenError::Corrupt(
                    "semantic property has no open frame",
                ));
            }
            hasher.update(&[2]);
            hasher.update(&property.tag().get().to_le_bytes());
            hash_bytes(hasher, property.as_bytes())?;
        }
        PackedGreenEvent::Coverage {
            physical,
            owner_depth,
            part,
            atom,
        } => {
            if usize::try_from(owner_depth)
                .ok()
                .is_none_or(|depth| depth >= open.len())
            {
                return Err(M11RecursiveGreenError::Corrupt(
                    "semantic coverage owner is outside its open path",
                ));
            }
            hasher.update(&[3]);
            hasher.update(&physical.bytes().to_le_bytes());
            hasher.update(&physical.utf16().to_le_bytes());
            hasher.update(&owner_depth.to_le_bytes());
            hasher.update(&[part as u8]);
            match atom {
                LogicalAtom::None => {
                    hasher.update(&[1]);
                }
                LogicalAtom::Identity => {
                    hasher.update(&[2]);
                }
                LogicalAtom::TabToSpaces {
                    target_owner_depth,
                    spaces,
                } => {
                    hasher.update(&[3]);
                    hasher.update(&target_owner_depth.to_le_bytes());
                    hasher.update(&[spaces]);
                }
                LogicalAtom::HiddenUpstream => {
                    hasher.update(&[4]);
                }
                LogicalAtom::LfToLf => {
                    hasher.update(&[5]);
                }
                LogicalAtom::CrLfToLf => {
                    hasher.update(&[6]);
                }
                LogicalAtom::LoneCrToLf => {
                    hasher.update(&[7]);
                }
                LogicalAtom::NulToReplacement => {
                    hasher.update(&[8]);
                }
            }
        }
        PackedGreenEvent::RetypeOpen {
            frame,
            kind,
            property,
        } => {
            let canonical = open
                .last()
                .filter(|value| value.0 == frame)
                .map(|value| value.1)
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "semantic retype crossed its open path",
                ))?;
            hasher.update(&[4]);
            hasher.update(&canonical.to_le_bytes());
            hasher.update(&kind.get().to_le_bytes());
            match property {
                Some(property) => {
                    hasher.update(&[1]);
                    hasher.update(&property.tag().get().to_le_bytes());
                    hash_bytes(hasher, property.as_bytes())?;
                }
                None => {
                    hasher.update(&[0]);
                }
            }
        }
        PackedGreenEvent::Exit {
            frame,
            final_kind,
            close,
            last_line_blank,
            child,
        } => {
            let canonical = open
                .pop()
                .filter(|value| value.0 == frame)
                .map(|value| value.1)
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "semantic exit crossed its open path",
                ))?;
            hasher.update(&[5]);
            hasher.update(&canonical.to_le_bytes());
            hasher.update(&final_kind.get().to_le_bytes());
            match close {
                Some(close) => {
                    hasher.update(&[1]);
                    hasher.update(&close.tag().get().to_le_bytes());
                    hash_bytes(hasher, close.as_bytes())?;
                }
                None => {
                    hasher.update(&[0]);
                }
            }
            hasher.update(&[
                u8::from(last_line_blank),
                u8::from(child.ends_blank()),
                u8::from(child.item_loose_if_nonlast()),
                u8::from(child.item_loose_if_last()),
            ]);
        }
    }
    Ok(())
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) -> Result<(), M11RecursiveGreenError> {
    let length = u64::try_from(bytes.len()).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
    Ok(())
}
