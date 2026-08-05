//! Atomic terminal parent for one candidate Green tree and reference index.
//!
//! This is deliberately smaller than the restart composite. It proves the
//! initial-publication ownership shape only: two completed, same-build typed
//! children become two ordered parent edges, their journal owners retire, and
//! the arena commits exactly one parent owner. Restart adoption and inline
//! lookup remain consumers of the committed typed child views, not concerns
//! of this join.

use std::fmt;

use super::ReferenceCandidateIndexWriterMint;
use crate::SourceSnapshotDescriptor;
use crate::arena::{
    ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession, ArenaError, ArenaId,
    ArenaScopedId, PageArena,
};
use crate::reference_restart_index::{
    CommittedReferenceIndex, ReferenceCandidateIndexManifest, ReferenceCandidateIndexReceipt,
    RestartIndexError,
};
use crate::serialized_green::{
    SerializedGreenBuildManifest, SerializedGreenBuildReceipt, SerializedGreenCompositeDescriptor,
    SerializedGreenDocument, SerializedGreenError, validate_serialized_green_composite_child,
};

const MANIFEST_TAG: u8 = 0xe7;
const FORMAT_VERSION: u8 = 1;
const CHILDREN: usize = 2;
const ENCODED_CHILDREN: u8 = 2;
const GREEN_ROLE: u8 = 1;
const REFERENCE_ROLE: u8 = 2;
const MANIFEST_BYTES: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GreenReferenceCompositeChildIds {
    pub(super) green: ArenaScopedId,
    pub(super) reference: ArenaScopedId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GreenReferenceCompositeReceipt {
    pub(super) green: SerializedGreenBuildReceipt,
    pub(super) reference: ReferenceCandidateIndexReceipt,
    pub(super) manifest_nodes_allocated: usize,
    pub(super) payload_bytes_copied: usize,
    pub(super) edge_bytes_copied: usize,
    pub(super) child_references_added: usize,
    pub(super) live_owners_after_join: usize,
}

/// The sole build-journal owner after Green and reference children are joined.
#[derive(Debug)]
#[must_use = "the Green/reference parent must be committed or its build aborted"]
pub(super) struct GreenReferenceCompositeBuildManifest {
    build: ArenaBuildId,
    owner: ArenaBuildOwner,
    children: GreenReferenceCompositeChildIds,
    green: SerializedGreenCompositeDescriptor,
    reference: CommittedReferenceIndex,
    reference_checkpoint: ArenaScopedId,
    source: SourceSnapshotDescriptor,
    receipt: GreenReferenceCompositeReceipt,
}

impl GreenReferenceCompositeBuildManifest {
    pub(super) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(super) const fn child_ids(&self) -> GreenReferenceCompositeChildIds {
        self.children
    }

    pub(super) const fn receipt(&self) -> GreenReferenceCompositeReceipt {
        self.receipt
    }

    pub(super) fn commit(
        self,
        session: ArenaBuildSession<'_>,
        mint: &mut ReferenceCandidateIndexWriterMint,
    ) -> Result<GreenReferenceCompositeDocument, GreenReferenceCompositeError> {
        if session.id() != self.build {
            return Err(GreenReferenceCompositeError::Invalid(
                "Green/reference parent and arena build generations differ",
            ));
        }
        let owner = session.commit(self.owner)?;
        let parent_root = owner.scoped_id();
        let green_document = SerializedGreenDocument::from_candidate_writer_composite_parent(
            owner,
            self.children.green,
            mint,
        );
        Ok(GreenReferenceCompositeDocument {
            parent_root,
            children: self.children,
            green_document,
            green_descriptor: self.green,
            reference: self.reference,
            reference_checkpoint: self.reference_checkpoint,
            source: self.source,
            receipt: self.receipt,
        })
    }
}

/// The committed parent is the only owner. Its child capabilities are views
/// rederived from the still-live parent, never independently retained owners.
#[derive(Debug)]
pub(super) struct GreenReferenceCompositeDocument {
    parent_root: ArenaScopedId,
    children: GreenReferenceCompositeChildIds,
    green_document: SerializedGreenDocument,
    green_descriptor: SerializedGreenCompositeDescriptor,
    reference: CommittedReferenceIndex,
    reference_checkpoint: ArenaScopedId,
    source: SourceSnapshotDescriptor,
    receipt: GreenReferenceCompositeReceipt,
}

impl GreenReferenceCompositeDocument {
    pub(super) const fn receipt(&self) -> GreenReferenceCompositeReceipt {
        self.receipt
    }

    pub(super) const fn green_document(&self) -> &SerializedGreenDocument {
        &self.green_document
    }

    pub(super) fn view<'parent>(
        &'parent self,
        arena: &PageArena,
    ) -> Result<GreenReferenceCompositeView<'parent>, GreenReferenceCompositeError> {
        let root = arena.local_id(self.parent_root)?;
        let mut mint = ReferenceCandidateIndexWriterMint(());
        let validated = validate_manifest(arena, root, self.reference_checkpoint, &mut mint)?;
        if validated.children != self.children
            || validated.green != self.green_descriptor
            || validated.reference != self.reference
            || validated.source != self.source
        {
            return Err(GreenReferenceCompositeError::Corrupt(
                "committed Green/reference parent descriptor changed",
            ));
        }
        Ok(GreenReferenceCompositeView {
            parent: self,
            children: validated.children,
            green: validated.green,
            reference: validated.reference,
        })
    }

    pub(super) fn child_ids(
        &self,
        arena: &PageArena,
    ) -> Result<GreenReferenceCompositeChildIds, GreenReferenceCompositeError> {
        Ok(self.view(arena)?.children)
    }

    pub(super) fn release_later(
        self,
        arena: &mut PageArena,
    ) -> Result<(), Box<GreenReferenceCompositeReleaseFailure>> {
        let Self {
            parent_root,
            children,
            green_document,
            green_descriptor,
            reference,
            reference_checkpoint,
            source,
            receipt,
        } = self;
        match green_document.release_later(arena) {
            Ok(()) => Ok(()),
            Err(failure) => Err(Box::new(GreenReferenceCompositeReleaseFailure {
                error: failure.error.into(),
                document: Self {
                    parent_root,
                    children,
                    green_document: failure.document,
                    green_descriptor,
                    reference,
                    reference_checkpoint,
                    source,
                    receipt,
                },
            })),
        }
    }
}

pub(super) struct GreenReferenceCompositeView<'parent> {
    parent: &'parent GreenReferenceCompositeDocument,
    children: GreenReferenceCompositeChildIds,
    green: SerializedGreenCompositeDescriptor,
    reference: CommittedReferenceIndex,
}

impl GreenReferenceCompositeView<'_> {
    pub(super) const fn parent(&self) -> &GreenReferenceCompositeDocument {
        self.parent
    }

    pub(super) const fn child_ids(&self) -> GreenReferenceCompositeChildIds {
        self.children
    }

    pub(super) const fn green(&self) -> SerializedGreenCompositeDescriptor {
        self.green
    }

    pub(super) const fn reference(&self) -> CommittedReferenceIndex {
        self.reference
    }
}

#[derive(Debug)]
pub(super) struct GreenReferenceCompositeReleaseFailure {
    pub(super) error: GreenReferenceCompositeError,
    pub(super) document: GreenReferenceCompositeDocument,
}

/// Constant-size terminal join. Both manifests are consumed on every path;
/// after any failure the caller must abort the one build journal.
pub(super) fn join_green_reference_children(
    session: &mut ArenaBuildSession<'_>,
    green: SerializedGreenBuildManifest,
    reference: ReferenceCandidateIndexManifest,
    mint: &mut ReferenceCandidateIndexWriterMint,
) -> Result<GreenReferenceCompositeBuildManifest, GreenReferenceCompositeError> {
    let build = session.id();
    if green.build_id() != build {
        return Err(GreenReferenceCompositeError::Invalid(
            "Green child and arena build generations differ",
        ));
    }
    if session.live_owners()? != CHILDREN {
        return Err(GreenReferenceCompositeError::Invalid(
            "Green/reference join requires exactly its two typed child owners",
        ));
    }

    let green_id = green.validate_composite_child(session)?;
    let green_descriptor = green.composite_descriptor(session)?;
    let reference_checkpoint = reference.checkpoint();
    let reference_receipt = reference.receipt();
    let reference_owner = reference.consume_for_candidate_writer(mint)?;
    let reference_id = session.owner_id(&reference_owner)?;
    if green_id == reference_id {
        return Err(GreenReferenceCompositeError::Corrupt(
            "Green and reference child roles alias one arena node",
        ));
    }

    let reference_root = session.arena().scoped_query_id(reference_id)?;
    let reference_checkpoint = session.arena().scoped_query_id(reference_checkpoint)?;
    let reference_view = CommittedReferenceIndex::from_candidate_writer_join(
        session.arena(),
        reference_root,
        reference_checkpoint,
        mint,
    )?;
    let reference_source = reference_view.source_snapshot(session.arena())?;
    validate_shared_source(green_descriptor, reference_source)?;

    let payload = encode_manifest(green_descriptor);
    let (parent, allocation) = session.allocate_packed(&payload, &[green_id, reference_id])?;
    let parent_id = session.owner_id(&parent)?;

    let (green_owner, green_receipt) = green.into_composite_parts();
    session.release(green_owner)?;
    session.release(reference_owner)?;

    let validated = validate_manifest(session.arena(), parent_id, reference_checkpoint, mint)?;
    let children = GreenReferenceCompositeChildIds {
        green: session.arena().scoped_query_id(green_id)?,
        reference: reference_root,
    };
    if validated.children != children
        || validated.green != green_descriptor
        || validated.reference != reference_view
        || validated.source != reference_source
    {
        return Err(GreenReferenceCompositeError::Corrupt(
            "Green/reference child order or descriptor changed during adoption",
        ));
    }
    let live_owners_after_join = session.live_owners()?;
    if live_owners_after_join != 1 {
        return Err(GreenReferenceCompositeError::Corrupt(
            "Green/reference join did not reduce the journal to one parent",
        ));
    }

    Ok(GreenReferenceCompositeBuildManifest {
        build,
        owner: parent,
        children,
        green: green_descriptor,
        reference: reference_view,
        reference_checkpoint,
        source: reference_source,
        receipt: GreenReferenceCompositeReceipt {
            green: green_receipt,
            reference: reference_receipt,
            manifest_nodes_allocated: 1,
            payload_bytes_copied: allocation.payload_bytes_copied,
            edge_bytes_copied: allocation.edge_bytes_copied,
            child_references_added: allocation.child_references_added,
            live_owners_after_join,
        },
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ValidatedManifest {
    children: GreenReferenceCompositeChildIds,
    green: SerializedGreenCompositeDescriptor,
    reference: CommittedReferenceIndex,
    source: SourceSnapshotDescriptor,
}

fn validate_manifest(
    arena: &PageArena,
    root: ArenaId,
    reference_checkpoint: ArenaScopedId,
    mint: &mut ReferenceCandidateIndexWriterMint,
) -> Result<ValidatedManifest, GreenReferenceCompositeError> {
    let payload = arena.payload(root)?;
    if payload.len() != MANIFEST_BYTES
        || payload[0] != MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != ENCODED_CHILDREN
        || payload[3] != GREEN_ROLE
        || payload[4] != REFERENCE_ROLE
        || payload[5..8] != [0, 0, 0]
        || arena.packed_child_count(root)? != CHILDREN
    {
        return Err(GreenReferenceCompositeError::Corrupt(
            "invalid Green/reference parent manifest",
        ));
    }
    let green_id = arena.packed_child_at(root, 0)?;
    let reference_id = arena.packed_child_at(root, 1)?;
    if green_id == reference_id {
        return Err(GreenReferenceCompositeError::Corrupt(
            "Green/reference parent child roles alias",
        ));
    }
    let green = validate_serialized_green_composite_child(arena, green_id)?;
    validate_encoded_green(payload, green)?;
    let reference_root = arena.scoped_query_id(reference_id)?;
    let reference = CommittedReferenceIndex::from_candidate_writer_join(
        arena,
        reference_root,
        reference_checkpoint,
        mint,
    )?;
    let source = reference.source_snapshot(arena)?;
    validate_shared_source(green, source)?;
    Ok(ValidatedManifest {
        children: GreenReferenceCompositeChildIds {
            green: arena.scoped_query_id(green_id)?,
            reference: reference_root,
        },
        green,
        reference,
        source,
    })
}

fn encode_manifest(green: SerializedGreenCompositeDescriptor) -> [u8; MANIFEST_BYTES] {
    let mut payload = [0_u8; MANIFEST_BYTES];
    payload[0] = MANIFEST_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = ENCODED_CHILDREN;
    payload[3] = GREEN_ROLE;
    payload[4] = REFERENCE_ROLE;
    put_u64(&mut payload, 8, green.source_revision().0);
    put_u64(&mut payload, 16, green.source_root().0);
    put_u64(&mut payload, 24, green.source_metric().bytes);
    put_u64(&mut payload, 32, green.source_metric().utf16);
    put_u64(&mut payload, 40, green.grammar_revision().0);
    put_u64(&mut payload, 48, green.parse_generation().0);
    put_u64(&mut payload, 56, green.semantic_epoch());
    put_u64(&mut payload, 64, green.syntax_profile());
    payload
}

fn validate_encoded_green(
    payload: &[u8],
    green: SerializedGreenCompositeDescriptor,
) -> Result<(), GreenReferenceCompositeError> {
    if read_u64(payload, 8) != green.source_revision().0
        || read_u64(payload, 16) != green.source_root().0
        || read_u64(payload, 24) != green.source_metric().bytes
        || read_u64(payload, 32) != green.source_metric().utf16
        || read_u64(payload, 40) != green.grammar_revision().0
        || read_u64(payload, 48) != green.parse_generation().0
        || read_u64(payload, 56) != green.semantic_epoch()
        || read_u64(payload, 64) != green.syntax_profile()
    {
        return Err(GreenReferenceCompositeError::Corrupt(
            "Green/reference parent metadata crossed its Green child",
        ));
    }
    Ok(())
}

fn validate_shared_source(
    green: SerializedGreenCompositeDescriptor,
    reference: SourceSnapshotDescriptor,
) -> Result<(), GreenReferenceCompositeError> {
    let green_bytes = usize::try_from(green.source_metric().bytes).map_err(|_| {
        GreenReferenceCompositeError::Invalid("Green source byte extent exceeds usize")
    })?;
    if reference.revision != green.source_revision()
        || reference.root != green.source_root()
        || reference.bytes != green_bytes
    {
        return Err(GreenReferenceCompositeError::Invalid(
            "Green and reference children describe different source snapshots",
        ));
    }
    Ok(())
}

fn put_u64(payload: &mut [u8], offset: usize, value: u64) {
    payload[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u64(payload: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        payload[offset..offset + 8]
            .try_into()
            .expect("fixed Green/reference manifest field is eight bytes"),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GreenReferenceCompositeError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Green(SerializedGreenError),
    Reference(RestartIndexError),
    Invalid(&'static str),
    Corrupt(&'static str),
}

impl From<ArenaError> for GreenReferenceCompositeError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for GreenReferenceCompositeError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl From<SerializedGreenError> for GreenReferenceCompositeError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl From<RestartIndexError> for GreenReferenceCompositeError {
    fn from(value: RestartIndexError) -> Self {
        Self::Reference(value)
    }
}

impl fmt::Display for GreenReferenceCompositeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
            Self::Reference(error) => error.fmt(formatter),
            Self::Invalid(message) | Self::Corrupt(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for GreenReferenceCompositeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference_label_interner::{ReferenceLabelInterner, ReferenceLabelInternerProgress};
    use crate::reference_restart_index::{
        ReferenceCandidateIndexAuthority, ReferenceCandidateIndexBuilder,
        ReferenceCandidateIndexProgress,
    };
    use crate::{
        BlockId, FactsEnvelope, GrammarRevision, GreenEvent, GreenKind, ParseGeneration,
        ResumableSerializedGreenBuild, SerializedGreenRootSpec, SerializedGreenStreamProgress,
        SourceRevision, SourceRootId,
    };

    fn offer(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        build.offer_event(session, event).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("Green event unexpectedly finalized the manifest")
                }
            }
        }
    }

    #[test]
    fn terminal_parent_commits_exactly_two_children_as_one_owner() {
        let source = SourceSnapshotDescriptor {
            revision: SourceRevision(1),
            root: SourceRootId(101),
            bytes: 0,
        };
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut green = ResumableSerializedGreenBuild::new(
            &ticket,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: source.revision,
                source_root: source.root,
                source_bytes: 0,
                source_utf16: 0,
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(1),
                semantic_epoch: 1,
                known_bytes: 0..0,
            },
        )
        .unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut green,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer(
            &mut green,
            &mut session,
            GreenEvent::exit(crate::ClosedChildAggregate::default()),
        );
        green.finish_input(&mut session).unwrap();
        while green.poll(&mut session).unwrap() != SerializedGreenStreamProgress::ManifestReady {}
        let green = green.take_manifest().unwrap();

        let mut mint = ReferenceCandidateIndexWriterMint(());
        let authority = ReferenceCandidateIndexAuthority::from_writer_join(
            session.id(),
            source,
            1,
            1,
            1,
            &mut mint,
        )
        .unwrap();
        let mut reference = ReferenceCandidateIndexBuilder::new(authority).unwrap();
        let mut interner = ReferenceLabelInterner::new_initial(session.id(), 1).unwrap();
        reference.capture_checkpoint(&mut session).unwrap();
        interner.begin_finish().unwrap();
        while interner.poll(&mut session).unwrap() != ReferenceLabelInternerProgress::ManifestReady
        {
        }
        reference
            .begin_finish(&mut session, interner.take_manifest().unwrap())
            .unwrap();
        while reference.poll(&mut session).unwrap()
            != ReferenceCandidateIndexProgress::ManifestReady
        {}
        let reference = reference.take_manifest().unwrap();

        assert_eq!(session.live_owners().unwrap(), 2);
        let parent =
            join_green_reference_children(&mut session, green, reference, &mut mint).unwrap();
        assert_eq!(parent.receipt().child_references_added, 2);
        assert_eq!(parent.receipt().live_owners_after_join, 1);
        let parent = parent.commit(session, &mut mint).unwrap();
        assert_eq!(parent.green_document().block_count(&arena).unwrap(), 1);
        let children = parent.child_ids(&arena).unwrap();
        assert_ne!(children.green, children.reference);

        parent.release_later(&mut arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}
