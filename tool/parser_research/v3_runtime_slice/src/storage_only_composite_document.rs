//! Atomic parents for the serialized green tree and sparse checkpoint index.
//!
//! Version 1 remains the original topology-only storage proof. Version 2 is a
//! separate non-reloadable, actor-owned restart parent: it binds complete
//! child descriptors and is the sole gateway to donor lookup and to retaining
//! both old children for a future same-journal adoption splice. Source lineage
//! and convergence still remain later gates.

use std::fmt;

#[cfg(feature = "exact-parser")]
use crate::SourceSnapshotDescriptor;
use crate::arena::{
    ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession, ArenaBuildTicket,
    ArenaError, ArenaId, ArenaScopedId, OwnedArenaRef, PageArena,
};
#[cfg(feature = "exact-parser")]
use crate::candidate_writer::{
    ParentSelectedRestartCompositeReplacement, RestartCompositeChildren,
};
use crate::committed_checkpoint_index::{
    CommittedCheckpointIndexBuildReceipt, CommittedCheckpointIndexError,
    StorageOnlyCheckpointIndexBuildManifest,
};
#[cfg(feature = "exact-parser")]
use crate::committed_checkpoint_index::{
    CommittedCheckpointIndexCompositeDescriptor, DonorCheckpointLookupReceipt,
    LocatedDonorCheckpointRecipe, ParentBoundCurrentRestartRole,
    ParentBoundNormalizationCheckpoint, ParentRetainedCheckpointIndexLease,
    ParentSelectedRestartAnchor, RelativeCheckpointMeasure,
    authenticate_parent_bound_current_restart_role,
    authenticate_parent_bound_normalization_checkpoint, bind_parent_selected_restart_anchor,
    locate_parent_bound_donor_checkpoint_at_or_before_cut,
    validate_committed_checkpoint_index_composite_child,
};
#[cfg(feature = "exact-parser")]
use crate::coordinator::{
    CandidateOutput, Coordinator, CoordinatorError, OutputRootLease, ParseToken, PublicationDelta,
};
#[cfg(all(feature = "exact-parser", test))]
use crate::serialized_green::ParentSelectedGreenRestartAuthority;
#[cfg(feature = "exact-parser")]
use crate::serialized_green::{
    CurrentRestartPath, CurrentRestartPathError, ParentRetainedGreenLease,
    SerializedGreenCompositeDescriptor, restart_output_at_parent_bound_event_cut,
    validate_serialized_green_composite_child,
};
use crate::serialized_green::{
    SerializedGreenBuildManifest, SerializedGreenBuildReceipt, SerializedGreenError,
};
#[cfg(feature = "exact-parser")]
use flark_comrak_value_block_core::ParseError;

const COMPOSITE_MANIFEST_TAG: u8 = 0xd1;
const FORMAT_VERSION: u8 = 1;
const STORAGE_ONLY_ROLE_TAG: u8 = 1;
const GREEN_CHILD_ROLE_TAG: u8 = 1;
const CHECKPOINT_INDEX_CHILD_ROLE_TAG: u8 = 2;
const COMPOSITE_MANIFEST_BYTES: usize = 64;
const COMPOSITE_CHILDREN: usize = 2;
const ENCODED_COMPOSITE_CHILDREN: u8 = 2;

#[cfg(feature = "exact-parser")]
const RESTART_COMPOSITE_MANIFEST_TAG: u8 = 0xd2;
#[cfg(feature = "exact-parser")]
const RESTART_COMPOSITE_FORMAT_VERSION: u8 = 2;
#[cfg(feature = "exact-parser")]
const RESTART_AUTHORITATIVE_ROLE_TAG: u8 = 2;
#[cfg(feature = "exact-parser")]
const NONRELOADABLE_ACTOR_OWNED_ALLOCATOR_FLAG: u8 = 1 << 0;
#[cfg(feature = "exact-parser")]
const TERMINAL_TAIL_FLAG: u8 = 1 << 1;
#[cfg(feature = "exact-parser")]
const RESTART_COMPOSITE_MANIFEST_BYTES: usize = 320;
#[cfg(feature = "exact-parser")]
const RESTART_REPLACEMENT_CHILD_OWNERS: usize = COMPOSITE_CHILDREN * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StorageOnlyCompositeDocumentBuildReceipt {
    pub(crate) green: SerializedGreenBuildReceipt,
    pub(crate) checkpoint_index: CommittedCheckpointIndexBuildReceipt,
    pub(crate) manifest_nodes_allocated: usize,
    pub(crate) payload_bytes_copied: usize,
    pub(crate) edge_bytes_copied: usize,
    pub(crate) child_references_added: usize,
    pub(crate) maximum_page_payload_bytes: usize,
}

/// The sole build-journal owner after both typed children have been adopted.
#[derive(Debug)]
pub(crate) struct StorageOnlyCompositeDocumentBuildManifest {
    build: ArenaBuildId,
    owner: ArenaBuildOwner,
    receipt: StorageOnlyCompositeDocumentBuildReceipt,
}

impl StorageOnlyCompositeDocumentBuildManifest {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn receipt(&self) -> StorageOnlyCompositeDocumentBuildReceipt {
        self.receipt
    }

    pub(crate) fn commit(
        self,
        session: ArenaBuildSession<'_>,
    ) -> Result<
        (
            StorageOnlyCompositeDocument,
            StorageOnlyCompositeDocumentBuildReceipt,
        ),
        StorageOnlyCompositeDocumentError,
    > {
        if session.id() != self.build {
            return Err(StorageOnlyCompositeDocumentError::Invalid(
                "composite manifest and arena session build generations differ",
            ));
        }
        let owner = session.commit(self.owner)?;
        Ok((
            StorageOnlyCompositeDocument { owner: Some(owner) },
            self.receipt,
        ))
    }
}

/// The committed owner is storage topology only; it has no parser-resume API.
#[derive(Debug)]
pub(crate) struct StorageOnlyCompositeDocument {
    owner: Option<OwnedArenaRef>,
}

impl StorageOnlyCompositeDocument {
    fn root_id(&self) -> ArenaId {
        self.owner
            .as_ref()
            .expect("live storage-only composite owns its root")
            .id()
    }

    fn child_ids(
        &self,
        arena: &PageArena,
    ) -> Result<(ArenaId, ArenaId), StorageOnlyCompositeDocumentError> {
        decode_composite_manifest(arena, self.root_id())
    }

    fn release_later(
        mut self,
        arena: &mut PageArena,
    ) -> Result<(), StorageOnlyCompositeDocumentError> {
        let owner = self
            .owner
            .take()
            .expect("live storage-only composite owns its root");
        arena
            .release_later(owner)
            .map_err(|failure| failure.error.into())
    }
}

#[derive(Debug)]
pub(crate) struct StorageOnlyCompositeDocumentBuilder;

impl StorageOnlyCompositeDocumentBuilder {
    /// Adopts exactly one typed green manifest and one typed checkpoint-index
    /// root into one fixed-page parent. Success leaves exactly one owner in
    /// the build journal, making the following `ArenaBuildSession::commit`
    /// atomic over the pair.
    ///
    /// The two child capabilities are consumed on every path. If an invariant
    /// fails after the parent allocation, the parent and any unreleased child
    /// owners remain in the journal while released child owners are only
    /// queued. The caller must abort the whole build; the parent edges keep
    /// both graphs live until fuelled abort/reclaim releases every reference.
    pub(crate) fn join(
        session: &mut ArenaBuildSession<'_>,
        green: SerializedGreenBuildManifest,
        checkpoint_index: StorageOnlyCheckpointIndexBuildManifest,
    ) -> Result<StorageOnlyCompositeDocumentBuildManifest, StorageOnlyCompositeDocumentError> {
        let build = session.id();
        if green.build_id() != build || checkpoint_index.build_id() != build {
            return Err(StorageOnlyCompositeDocumentError::Invalid(
                "composite children and arena session build generations differ",
            ));
        }

        let green_id = green.validate_composite_child(session)?;
        let checkpoint_index_id = checkpoint_index.validate_composite_child(session)?;
        if green_id == checkpoint_index_id {
            return Err(StorageOnlyCompositeDocumentError::Corrupt(
                "composite child roles alias one arena node",
            ));
        }
        if session.live_owners()? != COMPOSITE_CHILDREN {
            return Err(StorageOnlyCompositeDocumentError::Invalid(
                "composite join requires exactly its two typed child owners",
            ));
        }

        let payload = encode_composite_manifest();
        let (root, allocation) =
            session.allocate_packed(&payload, &[green_id, checkpoint_index_id])?;
        let root_id = session.owner_id(&root)?;

        let (green_owner, green_receipt) = green.into_composite_parts();
        let (checkpoint_index_owner, checkpoint_index_receipt) =
            checkpoint_index.into_composite_parts();
        session.release(green_owner)?;
        session.release(checkpoint_index_owner)?;

        let decoded_children = decode_composite_manifest(session.arena(), root_id)?;
        if decoded_children != (green_id, checkpoint_index_id) {
            return Err(StorageOnlyCompositeDocumentError::Corrupt(
                "composite child order changed during adoption",
            ));
        }
        if session.live_owners()? != 1 {
            return Err(StorageOnlyCompositeDocumentError::Corrupt(
                "composite join did not reduce the journal to one root",
            ));
        }

        Ok(StorageOnlyCompositeDocumentBuildManifest {
            build,
            owner: root,
            receipt: StorageOnlyCompositeDocumentBuildReceipt {
                green: green_receipt,
                checkpoint_index: checkpoint_index_receipt,
                manifest_nodes_allocated: 1,
                payload_bytes_copied: allocation.payload_bytes_copied,
                edge_bytes_copied: allocation.edge_bytes_copied,
                child_references_added: allocation.child_references_added,
                maximum_page_payload_bytes: allocation.payload_bytes_copied,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageOnlyCompositeDocumentError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Green(SerializedGreenError),
    CheckpointIndex(CommittedCheckpointIndexError),
    Invalid(&'static str),
    Corrupt(&'static str),
}

impl From<ArenaError> for StorageOnlyCompositeDocumentError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for StorageOnlyCompositeDocumentError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl From<SerializedGreenError> for StorageOnlyCompositeDocumentError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl From<CommittedCheckpointIndexError> for StorageOnlyCompositeDocumentError {
    fn from(value: CommittedCheckpointIndexError) -> Self {
        Self::CheckpointIndex(value)
    }
}

impl fmt::Display for StorageOnlyCompositeDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
            Self::CheckpointIndex(error) => error.fmt(formatter),
            Self::Invalid(message) => {
                write!(formatter, "invalid storage-only composite: {message}")
            }
            Self::Corrupt(message) => {
                write!(formatter, "corrupt storage-only composite: {message}")
            }
        }
    }
}

impl std::error::Error for StorageOnlyCompositeDocumentError {}

fn encode_composite_manifest() -> [u8; COMPOSITE_MANIFEST_BYTES] {
    let mut payload = [0_u8; COMPOSITE_MANIFEST_BYTES];
    payload[0] = COMPOSITE_MANIFEST_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = STORAGE_ONLY_ROLE_TAG;
    payload[3] = ENCODED_COMPOSITE_CHILDREN;
    payload[4] = GREEN_CHILD_ROLE_TAG;
    payload[5] = CHECKPOINT_INDEX_CHILD_ROLE_TAG;
    payload
}

fn decode_composite_manifest(
    arena: &PageArena,
    root: ArenaId,
) -> Result<(ArenaId, ArenaId), StorageOnlyCompositeDocumentError> {
    let payload = arena.payload(root)?;
    if payload.len() != COMPOSITE_MANIFEST_BYTES
        || payload[0] != COMPOSITE_MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != STORAGE_ONLY_ROLE_TAG
        || payload[3] != ENCODED_COMPOSITE_CHILDREN
        || payload[4] != GREEN_CHILD_ROLE_TAG
        || payload[5] != CHECKPOINT_INDEX_CHILD_ROLE_TAG
        || payload[6..] != [0; COMPOSITE_MANIFEST_BYTES - 6]
    {
        return Err(StorageOnlyCompositeDocumentError::Corrupt(
            "invalid composite manifest payload",
        ));
    }
    if arena.packed_child_count(root)? != COMPOSITE_CHILDREN {
        return Err(StorageOnlyCompositeDocumentError::Corrupt(
            "composite manifest must own exactly two children",
        ));
    }
    let green = arena.packed_child_at(root, 0)?;
    let checkpoint_index = arena.packed_child_at(root, 1)?;
    if green == checkpoint_index {
        return Err(StorageOnlyCompositeDocumentError::Corrupt(
            "composite child roles alias one arena node",
        ));
    }
    Ok((green, checkpoint_index))
}

/// Restart-authoritative v2 parent receipt. This role is intentionally
/// separate from the unchanged topology-only v1 parent above.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RestartCompositeDocumentBuildReceipt {
    pub(crate) green: SerializedGreenBuildReceipt,
    pub(crate) checkpoint_index: CommittedCheckpointIndexBuildReceipt,
    pub(crate) manifest_nodes_allocated: usize,
    pub(crate) payload_bytes_copied: usize,
    pub(crate) edge_bytes_copied: usize,
    pub(crate) child_references_added: usize,
    pub(crate) maximum_page_payload_bytes: usize,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartCompositeDocumentBuildManifest {
    build: ArenaBuildId,
    owner: ArenaBuildOwner,
    receipt: RestartCompositeDocumentBuildReceipt,
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeDocumentBuildManifest {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn receipt(&self) -> RestartCompositeDocumentBuildReceipt {
        self.receipt
    }

    pub(crate) fn commit(
        self,
        session: ArenaBuildSession<'_>,
    ) -> Result<
        (
            RestartCompositeDocument,
            RestartCompositeDocumentBuildReceipt,
        ),
        RestartCompositeDocumentError,
    > {
        if session.id() != self.build {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart parent and arena session build generations differ",
            ));
        }
        let owner = session.commit(self.owner)?;
        Ok((
            RestartCompositeDocument { owner: Some(owner) },
            self.receipt,
        ))
    }
}

/// The committed v2 owner. It is intentionally non-reloadable: no API can
/// construct this handle from a persisted root after loss of the actor-owned
/// `DocumentIdentityAllocator`.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartCompositeDocument {
    owner: Option<OwnedArenaRef>,
}

/// Private one-shot carrier from a freshly revalidated composite manifest to
/// the otherwise unconstructible coordinator publication bundle. Both fields
/// are extracted from one committed parent in one method; no caller can mint
/// this type from a raw owner or a free grammar scalar.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartCompositeCandidateOutputMint {
    owner: OwnedArenaRef,
    grammar_revision: crate::GrammarRevision,
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeCandidateOutputMint {
    pub(crate) fn into_candidate_output_parts(self) -> (OwnedArenaRef, crate::GrammarRevision) {
        (self.owner, self.grammar_revision)
    }
}

/// Opaque, non-owning description of the exact committed parent validated at
/// the ownership-transfer boundary. Child arena identities stay private; the
/// scoped parent identity and complete child descriptors are compared again
/// whenever a published binding is used.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartCompositeRootDescriptor {
    root: ArenaScopedId,
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeRootDescriptor {
    fn from_validated(root: ArenaScopedId, validated: &ValidatedRestartComposite) -> Self {
        Self {
            root,
            green: validated.green,
            checkpoint_index: validated.checkpoint_index,
        }
    }

    const fn arena_root(self) -> ArenaScopedId {
        self.root
    }

    const fn grammar_revision(self) -> crate::GrammarRevision {
        self.green.grammar_revision()
    }

    fn revalidate(
        self,
        arena: &PageArena,
    ) -> Result<ValidatedRestartComposite, RestartCompositeDocumentError> {
        let root = arena.local_id(self.root)?;
        let validated = validate_restart_composite_manifest(arena, root)?;
        if validated.green != self.green || validated.checkpoint_index != self.checkpoint_index {
            return Err(RestartCompositeDocumentError::Corrupt(
                "published restart root descriptor changed",
            ));
        }
        Ok(validated)
    }
}

/// Fully recoverable owner-plus-descriptor transaction awaiting coordinator
/// publication. It deliberately exposes no operation which splits the owner
/// from its root descriptor.
#[cfg(feature = "exact-parser")]
#[must_use = "the prepared restart output must be published, retried, or deliberately released"]
#[derive(Debug)]
pub(crate) struct PreparedRestartCompositePublication {
    candidate: CandidateOutput,
    descriptor: RestartCompositeRootDescriptor,
}

#[cfg(feature = "exact-parser")]
impl PreparedRestartCompositePublication {
    /// Atomically hands the exact owner-plus-grammar bundle to the
    /// coordinator. Every storage and token check runs before that transfer;
    /// a coordinator rejection returns the same candidate bundle intact.
    pub(crate) fn publish(
        self,
        coordinator: &mut Coordinator,
        token: ParseToken,
        arena: &mut PageArena,
    ) -> Result<RestartCompositePublicationReceipt, Box<RestartCompositePublicationFailure>> {
        if let Err(error) = self.validate_for_token(token, arena) {
            return Err(Box::new(RestartCompositePublicationFailure {
                error,
                publication: self,
            }));
        }
        let Self {
            candidate,
            descriptor,
        } = self;
        match coordinator.publish_candidate(token, candidate, arena) {
            Ok(delta) => {
                // CandidateOutput was minted from this descriptor, and the
                // coordinator constructs the lease from CandidateOutput. No
                // fallible work remains after ownership transfer.
                debug_assert_eq!(delta.offered_output.arena_root, descriptor.arena_root());
                debug_assert_eq!(
                    delta.offered_output.grammar_revision,
                    descriptor.grammar_revision()
                );
                Ok(RestartCompositePublicationReceipt {
                    delta,
                    binding: PublishedRestartCompositeHandle {
                        lease: delta.offered_output,
                        descriptor,
                    },
                })
            }
            Err(failure) => Err(Box::new(RestartCompositePublicationFailure {
                error: failure.error.into(),
                publication: Self {
                    candidate: failure.candidate,
                    descriptor,
                },
            })),
        }
    }

    fn validate_for_token(
        &self,
        token: ParseToken,
        arena: &PageArena,
    ) -> Result<(), RestartCompositeDocumentError> {
        if self.candidate.arena_root() != self.descriptor.arena_root()
            || self.candidate.grammar_revision() != self.descriptor.grammar_revision()
        {
            return Err(RestartCompositeDocumentError::Corrupt(
                "restart publication candidate and root descriptor differ",
            ));
        }
        let validated = self.descriptor.revalidate(arena)?;
        if validated.green.source_revision() != token.source_revision
            || validated.green.source_root() != token.source_root
            || validated.green.parse_generation() != token.generation
        {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart publication manifest and parse token differ",
            ));
        }
        Ok(())
    }

    /// Retires an arena-committed candidate which became stale before
    /// publication. A failed release returns this exact opaque bundle, so the
    /// actor can retry disposal without recreating or splitting authority.
    pub(crate) fn release_later(
        self,
        arena: &mut PageArena,
    ) -> Result<(), Box<RestartCompositePublicationReleaseFailure>> {
        let Self {
            candidate,
            descriptor,
        } = self;
        match candidate.release_later(arena) {
            Ok(()) => Ok(()),
            Err(failure) => Err(Box::new(RestartCompositePublicationReleaseFailure {
                error: failure.error.into(),
                publication: Self {
                    candidate: failure.candidate,
                    descriptor,
                },
            })),
        }
    }
}

/// Successful atomic publication. The delta remains available for UI
/// offering while the private binding is the only restart authority.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartCompositePublicationReceipt {
    delta: PublicationDelta,
    binding: PublishedRestartCompositeHandle,
}

#[cfg(feature = "exact-parser")]
impl RestartCompositePublicationReceipt {
    pub(crate) const fn delta(&self) -> PublicationDelta {
        self.delta
    }

    pub(crate) fn into_binding(self) -> PublishedRestartCompositeHandle {
        self.binding
    }
}

/// Recoverable validation failure before the committed owner has crossed into
/// `CandidateOutput`. The original owning document remains intact.
#[cfg(feature = "exact-parser")]
#[must_use = "the rejected owning document must be recovered or deliberately released"]
#[derive(Debug)]
pub(crate) struct RestartCompositePublicationPreparationFailure {
    pub(crate) error: RestartCompositeDocumentError,
    pub(crate) document: RestartCompositeDocument,
}

/// Recoverable publication failure after preparation. The exact
/// owner-plus-descriptor transaction can be retried without reconstructing or
/// re-pairing any scalar authority.
#[cfg(feature = "exact-parser")]
#[must_use = "the rejected publication bundle must be recovered or deliberately released"]
#[derive(Debug)]
pub(crate) struct RestartCompositePublicationFailure {
    pub(crate) error: RestartCompositeDocumentError,
    pub(crate) publication: PreparedRestartCompositePublication,
}

/// Recoverable retirement failure for an arena-committed candidate which was
/// invalidated before coordinator publication.
#[cfg(feature = "exact-parser")]
#[must_use = "the rejected publication bundle must be recovered or deliberately released"]
#[derive(Debug)]
pub(crate) struct RestartCompositePublicationReleaseFailure {
    pub(crate) error: RestartCompositeDocumentError,
    pub(crate) publication: PreparedRestartCompositePublication,
}

/// Non-owning actor binding to one exact coordinator-published restart root.
/// It is harmless to retain after retirement: every semantic operation first
/// asks the coordinator to re-resolve this exact lease as worker-current.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublishedRestartCompositeHandle {
    lease: OutputRootLease,
    descriptor: RestartCompositeRootDescriptor,
}

#[cfg(feature = "exact-parser")]
impl PublishedRestartCompositeHandle {
    pub(crate) const fn output_lease(self) -> OutputRootLease {
        self.lease
    }

    /// Test-only corruption seam for proving that live-actor restart
    /// activation re-resolves this exact lease instead of trusting a cached
    /// root descriptor.
    #[cfg(test)]
    pub(crate) fn replace_output_lease_for_test(&mut self, lease: OutputRootLease) {
        self.lease = lease;
    }

    fn revalidate_worker_current(
        self,
        coordinator: &Coordinator,
        arena: &PageArena,
    ) -> Result<ValidatedRestartComposite, RestartCompositeDocumentError> {
        let resolved = coordinator.resolve_worker_current(self.lease, arena)?;
        if resolved != self.descriptor.arena_root()
            || self.lease.grammar_revision != self.descriptor.grammar_revision()
        {
            return Err(RestartCompositeDocumentError::Invalid(
                "published restart binding and coordinator root differ",
            ));
        }
        self.descriptor.revalidate(arena)
    }

    /// Read-only child views are minted only after the coordinator resolves
    /// this exact lease as worker-current and the complete parent descriptor
    /// is revalidated at that root.
    pub(crate) fn view(
        &self,
        coordinator: &Coordinator,
        arena: &PageArena,
    ) -> Result<PublishedRestartCompositeDocumentView<'_>, RestartCompositeDocumentError> {
        let validated = (*self).revalidate_worker_current(coordinator, arena)?;
        Ok(PublishedRestartCompositeDocumentView {
            parent: self,
            green: validated.green,
            checkpoint_index: validated.checkpoint_index,
        })
    }

    /// Published-parent donor selection has the same semantic query as the
    /// owning parent, but the selected recipe remains bound to this handle,
    /// coordinator, and arena until it is consumed.
    pub(crate) fn locate_donor_checkpoint_at_or_before_cut<'binding, 'coordinator, 'arena>(
        &'binding self,
        coordinator: &'coordinator Coordinator,
        arena: &'arena PageArena,
        source_cut: u64,
    ) -> Result<
        Option<PublishedRestartParentDonorCheckpoint<'binding, 'coordinator, 'arena>>,
        RestartCompositeDocumentError,
    > {
        let validated = (*self).revalidate_worker_current(coordinator, arena)?;
        let recipe = locate_parent_bound_donor_checkpoint_at_or_before_cut(
            RestartCheckpointQueryMint {
                arena,
                descriptor: validated.checkpoint_index,
            },
            source_cut,
        )?;
        Ok(recipe.map(|recipe| PublishedRestartParentDonorCheckpoint {
            parent: self,
            coordinator,
            arena,
            recipe,
        }))
    }

    /// Retains both exact published children into one fresh journal. Binding
    /// validation happens only after certifying the journal empty, preserving
    /// the existing pristine-vs-mutated recovery contract.
    pub(crate) fn retain_children_for_adoption(
        &self,
        coordinator: &Coordinator,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<RestartCompositeAdoptionLease, RestartCompositeAdoptionRetentionFailure> {
        match session.live_owners() {
            Ok(0) => {}
            Ok(_) => {
                return Err(RestartCompositeAdoptionRetentionFailure::Mutated {
                    error: RestartCompositeDocumentError::Invalid(
                        "restart adoption requires a fresh empty build journal",
                    ),
                    cleanup_error: None,
                });
            }
            Err(error) => {
                return Err(RestartCompositeAdoptionRetentionFailure::Mutated {
                    error: error.into(),
                    cleanup_error: None,
                });
            }
        }
        (*self)
            .revalidate_worker_current(coordinator, session.arena())
            .map_err(RestartCompositeAdoptionRetentionFailure::Pristine)?;
        retain_restart_composite_children_from_root(
            self.descriptor.arena_root(),
            session,
            RestartCompositeAdoptionRetentionControl::NORMAL,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeDocument {
    fn checked_root_id(&self, arena: &PageArena) -> Result<ArenaId, RestartCompositeDocumentError> {
        let scoped = self
            .owner
            .as_ref()
            .expect("live restart composite owns its root")
            .scoped_id();
        arena.local_id(scoped).map_err(Into::into)
    }

    /// Revalidates the complete committed parent and moves its sole owner into
    /// one root-bound publication transaction. A validation failure returns
    /// this exact owning document; success leaves no second owning parent
    /// handle behind.
    pub(crate) fn prepare_publication(
        mut self,
        arena: &PageArena,
    ) -> Result<PreparedRestartCompositePublication, RestartCompositePublicationPreparationFailure>
    {
        let descriptor = (|| {
            let root = self.checked_root_id(arena)?;
            let validated = validate_restart_composite_manifest(arena, root)?;
            let scoped = self
                .owner
                .as_ref()
                .expect("live restart composite owns its root")
                .scoped_id();
            Ok::<_, RestartCompositeDocumentError>(RestartCompositeRootDescriptor::from_validated(
                scoped, &validated,
            ))
        })();
        let descriptor = match descriptor {
            Ok(descriptor) => descriptor,
            Err(error) => {
                return Err(RestartCompositePublicationPreparationFailure {
                    error,
                    document: self,
                });
            }
        };
        let owner = self
            .owner
            .take()
            .expect("validated restart composite still owns its root");
        let candidate =
            CandidateOutput::from_restart_composite_mint(RestartCompositeCandidateOutputMint {
                owner,
                grammar_revision: descriptor.grammar_revision(),
            });
        Ok(PreparedRestartCompositePublication {
            candidate,
            descriptor,
        })
    }

    /// Revalidates the parent payload, both exact child roots, and every
    /// cross-child total before minting parent-borrowed views.
    pub(crate) fn view<'parent>(
        &'parent self,
        arena: &PageArena,
    ) -> Result<RestartCompositeDocumentView<'parent>, RestartCompositeDocumentError> {
        let validated = validate_restart_composite_manifest(arena, self.checked_root_id(arena)?)?;
        Ok(RestartCompositeDocumentView {
            parent: self,
            green: validated.green,
            checkpoint_index: validated.checkpoint_index,
        })
    }

    /// Selects a donor only through the committed v2 parent. The returned
    /// value owns bounded parser scratch, but remains borrowed from both this
    /// parent and the arena; it contains no independently queryable index root
    /// or ownership token.
    pub(crate) fn locate_donor_checkpoint_at_or_before_cut<'parent, 'arena>(
        &'parent self,
        arena: &'arena PageArena,
        source_cut: u64,
    ) -> Result<Option<RestartParentDonorCheckpoint<'parent, 'arena>>, RestartCompositeDocumentError>
    {
        let validated = validate_restart_composite_manifest(arena, self.checked_root_id(arena)?)?;
        let recipe = locate_parent_bound_donor_checkpoint_at_or_before_cut(
            RestartCheckpointQueryMint {
                arena,
                descriptor: validated.checkpoint_index,
            },
            source_cut,
        )?;
        Ok(recipe.map(|recipe| RestartParentDonorCheckpoint {
            parent: self,
            arena,
            recipe,
        }))
    }

    /// Test-only stand-in for the production stamp carried by the selected
    /// source/donor activation. It still fully validates the exact actor root;
    /// no constructor from a cut or child descriptor is available.
    #[cfg(test)]
    pub(crate) fn parent_selection_stamp_for_test(
        &self,
        arena: &PageArena,
    ) -> Result<RestartParentSelectionStamp, RestartCompositeDocumentError> {
        let root = self.checked_root_id(arena)?;
        validate_restart_composite_manifest(arena, root)?;
        Ok(RestartParentSelectionStamp {
            root: self
                .owner
                .as_ref()
                .expect("live restart composite owns its root")
                .scoped_id(),
        })
    }

    /// Retains both exact child roots into one fresh same-arena journal. The
    /// caller supplies no roots, descriptors, totals, or cuts; all authority
    /// is re-derived from the still-live parent. A successful call leaves
    /// exactly two journal owners hidden behind one non-cloneable lease.
    pub(crate) fn retain_children_for_adoption(
        &self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<RestartCompositeAdoptionLease, RestartCompositeAdoptionRetentionFailure> {
        self.retain_children_for_adoption_with_control(
            session,
            RestartCompositeAdoptionRetentionControl::NORMAL,
        )
    }

    fn retain_children_for_adoption_with_control(
        &self,
        session: &mut ArenaBuildSession<'_>,
        control: RestartCompositeAdoptionRetentionControl,
    ) -> Result<RestartCompositeAdoptionLease, RestartCompositeAdoptionRetentionFailure> {
        let parent_root = self
            .owner
            .as_ref()
            .expect("live restart composite owns its root")
            .scoped_id();
        retain_restart_composite_children_from_root(parent_root, session, control)
    }

    #[cfg(test)]
    fn retain_children_for_adoption_with_test_fault(
        &self,
        session: &mut ArenaBuildSession<'_>,
        fault: RestartCompositeAdoptionRetentionTestFault,
    ) -> Result<RestartCompositeAdoptionLease, RestartCompositeAdoptionRetentionFailure> {
        self.retain_children_for_adoption_with_control(session, fault.into())
    }

    pub(crate) fn release_later(
        mut self,
        arena: &mut PageArena,
    ) -> Result<(), Box<RestartCompositeDocumentReleaseFailure>> {
        let owner = self
            .owner
            .take()
            .expect("live restart composite owns its root");
        match arena.release_later(owner) {
            Ok(()) => Ok(()),
            Err(failure) => {
                self.owner = Some(failure.owner);
                Err(Box::new(RestartCompositeDocumentReleaseFailure {
                    error: failure.error.into(),
                    document: self,
                }))
            }
        }
    }
}

#[cfg(feature = "exact-parser")]
fn retain_restart_composite_children_from_root(
    parent_root: ArenaScopedId,
    session: &mut ArenaBuildSession<'_>,
    control: RestartCompositeAdoptionRetentionControl,
) -> Result<RestartCompositeAdoptionLease, RestartCompositeAdoptionRetentionFailure> {
    let live_owners = match session.live_owners() {
        Ok(live_owners) => live_owners,
        Err(error) => {
            return Err(RestartCompositeAdoptionRetentionFailure::Mutated {
                error: error.into(),
                cleanup_error: None,
            });
        }
    };
    if live_owners != 0 {
        return Err(RestartCompositeAdoptionRetentionFailure::Mutated {
            error: RestartCompositeDocumentError::Invalid(
                "restart adoption requires a fresh empty build journal",
            ),
            cleanup_error: None,
        });
    }
    let parent_root_id = session
        .arena()
        .local_id(parent_root)
        .map_err(|error| RestartCompositeAdoptionRetentionFailure::Pristine(error.into()))?;
    let validated = validate_restart_composite_manifest(session.arena(), parent_root_id)
        .map_err(RestartCompositeAdoptionRetentionFailure::Pristine)?;
    let green_retain = if control.force_first_retain_failure {
        Err(ArenaBuildError::Invariant(
            "injected first restart-adoption retain failure",
        ))
    } else {
        session.retain(validated.green_root)
    };
    let green_owner = match green_retain {
        Ok(owner) => owner,
        Err(error) => {
            // `retain` preflights the journal before the arena reference
            // change, but retryability still requires a fresh explicit
            // empty-journal receipt at this transaction boundary.
            return Err(cleanup_restart_adoption_retention(
                session,
                error.into(),
                None,
                None,
                true,
                control,
            ));
        }
    };
    let checkpoint_retain = if control.force_second_retain_failure {
        Err(ArenaBuildError::Invariant(
            "injected second restart-adoption retain failure",
        ))
    } else {
        session.retain(validated.checkpoint_index_root)
    };
    let checkpoint_owner = match checkpoint_retain {
        Ok(owner) => owner,
        Err(error) => {
            // A failed second retain is retryable only after independently
            // releasing the first owner and certifying the journal empty.
            // Any cleanup uncertainty is instead an abort-only failure.
            return Err(cleanup_restart_adoption_retention(
                session,
                error.into(),
                Some(green_owner),
                None,
                true,
                control,
            ));
        }
    };

    let validation = if control.force_post_retain_validation_failure {
        Err(RestartCompositeDocumentError::Corrupt(
            "injected post-retain restart-adoption validation failure",
        ))
    } else {
        (|| -> Result<(), RestartCompositeDocumentError> {
            if session.owner_id(&green_owner)? != validated.green_root
                || session.owner_id(&checkpoint_owner)? != validated.checkpoint_index_root
                || session.live_owners()? != COMPOSITE_CHILDREN
            {
                return Err(RestartCompositeDocumentError::Corrupt(
                    "restart adoption journal does not own the exact parent children",
                ));
            }
            Ok(())
        })()
    };
    if let Err(error) = validation {
        // Once both owners existed, even successful compensating cleanup
        // does not make a failed integrity check retryable. The actor must
        // detach the attempt and drive the build through bounded abort.
        return Err(cleanup_restart_adoption_retention(
            session,
            error,
            Some(green_owner),
            Some(checkpoint_owner),
            false,
            control,
        ));
    }
    Ok(RestartCompositeAdoptionLease {
        parent_activation: RestartParentActivationStamp { root: parent_root },
        build: session.id(),
        green_owner,
        checkpoint_owner,
        green: validated.green,
        checkpoint_index: validated.checkpoint_index,
    })
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartCompositeAdoptionRetentionControl {
    force_first_retain_failure: bool,
    force_second_retain_failure: bool,
    force_post_retain_validation_failure: bool,
    force_cleanup_failure: bool,
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeAdoptionRetentionControl {
    const NORMAL: Self = Self {
        force_first_retain_failure: false,
        force_second_retain_failure: false,
        force_post_retain_validation_failure: false,
        force_cleanup_failure: false,
    };
}

#[cfg(all(feature = "exact-parser", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartCompositeAdoptionRetentionTestFault {
    FirstRetain,
    SecondRetain,
    PostRetainValidation,
    Cleanup,
}

#[cfg(all(feature = "exact-parser", test))]
impl From<RestartCompositeAdoptionRetentionTestFault> for RestartCompositeAdoptionRetentionControl {
    fn from(fault: RestartCompositeAdoptionRetentionTestFault) -> Self {
        match fault {
            RestartCompositeAdoptionRetentionTestFault::FirstRetain => Self {
                force_first_retain_failure: true,
                ..Self::NORMAL
            },
            RestartCompositeAdoptionRetentionTestFault::SecondRetain => Self {
                force_second_retain_failure: true,
                ..Self::NORMAL
            },
            RestartCompositeAdoptionRetentionTestFault::PostRetainValidation => Self {
                force_post_retain_validation_failure: true,
                ..Self::NORMAL
            },
            RestartCompositeAdoptionRetentionTestFault::Cleanup => Self {
                force_post_retain_validation_failure: true,
                force_cleanup_failure: true,
                ..Self::NORMAL
            },
        }
    }
}

#[cfg(feature = "exact-parser")]
fn cleanup_restart_adoption_retention(
    session: &mut ArenaBuildSession<'_>,
    error: RestartCompositeDocumentError,
    green_owner: Option<ArenaBuildOwner>,
    checkpoint_owner: Option<ArenaBuildOwner>,
    pristine_if_certified_empty: bool,
    control: RestartCompositeAdoptionRetentionControl,
) -> RestartCompositeAdoptionRetentionFailure {
    let mut cleanup_error = None;

    for (index, owner) in [green_owner, checkpoint_owner].into_iter().enumerate() {
        let Some(owner) = owner else {
            continue;
        };
        if control.force_cleanup_failure && index == 0 {
            // Test-only fault policy: leaving this owner in the journal models
            // a failed transfer while preserving the real abort recovery path.
            cleanup_error = Some(RestartCompositeDocumentError::ArenaBuild(
                ArenaBuildError::Invariant("injected restart-adoption cleanup failure"),
            ));
            drop(owner);
            continue;
        }
        if let Err(release_error) = session.release(owner) {
            cleanup_error.get_or_insert_with(|| release_error.into());
        }
    }

    let certified_empty = match session.live_owners() {
        Ok(0) => cleanup_error.is_none(),
        Ok(_) => {
            cleanup_error.get_or_insert(RestartCompositeDocumentError::Corrupt(
                "restart adoption cleanup left a nonempty journal",
            ));
            false
        }
        Err(live_error) => {
            cleanup_error.get_or_insert_with(|| live_error.into());
            false
        }
    };

    if pristine_if_certified_empty && certified_empty {
        RestartCompositeAdoptionRetentionFailure::Pristine(error)
    } else {
        RestartCompositeAdoptionRetentionFailure::Mutated {
            error,
            cleanup_error,
        }
    }
}

/// One parent-selected donor checkpoint. Lifetimes prevent either the parent
/// or its arena from changing while this value is live; resume still repeats
/// complete parent and selected-sample validation before using parser state.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartParentDonorCheckpoint<'parent, 'arena> {
    parent: &'parent RestartCompositeDocument,
    arena: &'arena PageArena,
    recipe: LocatedDonorCheckpointRecipe,
}

/// Donor selected from one exact coordinator-published parent. The recipe
/// remains borrowed from the binding, coordinator, and arena so consumption
/// can re-resolve the same worker-current root before minting restart state.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct PublishedRestartParentDonorCheckpoint<'binding, 'coordinator, 'arena> {
    parent: &'binding PublishedRestartCompositeHandle,
    coordinator: &'coordinator Coordinator,
    arena: &'arena PageArena,
    recipe: LocatedDonorCheckpointRecipe,
}

/// Linear parent-minted source-ledger restart input. No constructor accepts
/// the three component values: they are joined only after the same parent has
/// revalidated the selected checkpoint and queried its exact green child at
/// that checkpoint's event cut.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartSourceLedgerCheckpointMint {
    parent_selection: RestartParentSelectionStamp,
    source: SourceSnapshotDescriptor,
    checkpoint_cut: RelativeCheckpointMeasure,
    path: CurrentRestartPath,
    kind: RestartSourceLedgerCheckpointKind,
    restart_anchor: ParentSelectedRestartAnchor,
}

/// Physical/source relationship authenticated while the selected parent and
/// current-green child are still joined. Deferred-LF and source-complete line
/// restarts deliberately remain different variants downstream.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartSourceLedgerCheckpointKind {
    DeferredLf,
    SourceCompleteLineBoundary,
}

#[cfg(feature = "exact-parser")]
impl RestartSourceLedgerCheckpointMint {
    pub(crate) fn into_source_ledger_parts(
        self,
    ) -> (
        SourceSnapshotDescriptor,
        RelativeCheckpointMeasure,
        CurrentRestartPath,
        RestartSourceLedgerCheckpointKind,
        ParentSelectedRestartAnchor,
        RestartParentSelectionStamp,
    ) {
        (
            self.source,
            self.checkpoint_cut,
            self.path,
            self.kind,
            self.restart_anchor,
            self.parent_selection,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl<'parent, 'arena> RestartParentDonorCheckpoint<'parent, 'arena> {
    pub(crate) const fn ordinal(&self) -> u64 {
        self.recipe.ordinal()
    }

    pub(crate) const fn prefix(&self) -> RelativeCheckpointMeasure {
        self.recipe.prefix()
    }

    pub(crate) const fn interval(&self) -> RelativeCheckpointMeasure {
        self.recipe.interval()
    }

    pub(crate) const fn checkpoint_cut(&self) -> RelativeCheckpointMeasure {
        self.recipe.checkpoint_cut()
    }

    pub(crate) const fn receipt(&self) -> DonorCheckpointLookupReceipt {
        self.recipe.receipt()
    }

    /// Atomically joins this exact selected donor recipe to the current-green
    /// path queried from the same fully revalidated parent at the recipe's
    /// event cut. The opaque donor recipe remains in the linear mint so later
    /// source reconstruction cannot cross it with another equal-cut sample.
    pub(crate) fn into_source_ledger_restart_mint(
        self,
    ) -> Result<RestartSourceLedgerCheckpointMint, RestartParentDonorResumeError> {
        let parent_root = self
            .parent
            .owner
            .as_ref()
            .expect("live restart composite owns its root")
            .scoped_id();
        let validated = validate_restart_composite_manifest(
            self.arena,
            self.parent.checked_root_id(self.arena)?,
        )?;
        mint_source_ledger_restart_from_validated_parent(
            parent_root,
            self.arena,
            &validated,
            self.recipe,
        )
    }

    /// Consumes this parent-selected recipe into opaque normalization-role
    /// authority. Complete parent and index/sample bindings are revalidated;
    /// a direct-run donor cannot mint the capability.
    pub(crate) fn into_normalization_splice_checkpoint(
        self,
    ) -> Result<ParentBoundNormalizationCheckpoint, RestartParentDonorResumeError> {
        let validated = validate_restart_composite_manifest(
            self.arena,
            self.parent.checked_root_id(self.arena)?,
        )?;
        authenticate_normalization_checkpoint_from_validated_parent(
            self.arena,
            &validated,
            &self.recipe,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl PublishedRestartParentDonorCheckpoint<'_, '_, '_> {
    pub(crate) const fn ordinal(&self) -> u64 {
        self.recipe.ordinal()
    }

    pub(crate) const fn prefix(&self) -> RelativeCheckpointMeasure {
        self.recipe.prefix()
    }

    pub(crate) const fn interval(&self) -> RelativeCheckpointMeasure {
        self.recipe.interval()
    }

    pub(crate) const fn checkpoint_cut(&self) -> RelativeCheckpointMeasure {
        self.recipe.checkpoint_cut()
    }

    pub(crate) const fn receipt(&self) -> DonorCheckpointLookupReceipt {
        self.recipe.receipt()
    }

    pub(crate) fn into_source_ledger_restart_mint(
        self,
    ) -> Result<RestartSourceLedgerCheckpointMint, RestartParentDonorResumeError> {
        let validated = (*self.parent)
            .revalidate_worker_current(self.coordinator, self.arena)
            .map_err(RestartParentDonorResumeError::from)?;
        mint_source_ledger_restart_from_validated_parent(
            self.parent.descriptor.arena_root(),
            self.arena,
            &validated,
            self.recipe,
        )
    }

    pub(crate) fn into_normalization_splice_checkpoint(
        self,
    ) -> Result<ParentBoundNormalizationCheckpoint, RestartParentDonorResumeError> {
        let validated = (*self.parent)
            .revalidate_worker_current(self.coordinator, self.arena)
            .map_err(RestartParentDonorResumeError::from)?;
        authenticate_normalization_checkpoint_from_validated_parent(
            self.arena,
            &validated,
            &self.recipe,
        )
    }
}

#[cfg(feature = "exact-parser")]
fn mint_source_ledger_restart_from_validated_parent(
    parent_root: ArenaScopedId,
    arena: &PageArena,
    validated: &ValidatedRestartComposite,
    recipe: LocatedDonorCheckpointRecipe,
) -> Result<RestartSourceLedgerCheckpointMint, RestartParentDonorResumeError> {
    let checkpoint_cut = recipe.checkpoint_cut();
    let role = authenticate_parent_bound_current_restart_role(
        RestartCheckpointQueryMint {
            arena,
            descriptor: validated.checkpoint_index,
        },
        &recipe,
    )?;
    let current = restart_output_at_parent_bound_event_cut(
        RestartGreenQueryMint {
            arena,
            descriptor: validated.green,
        },
        checkpoint_cut.green_events(),
    )
    .map_err(RestartCompositeDocumentError::from)?;
    let mut path = current.into_current_restart_path()?;
    let queried_path_is_source_complete = path.source_metric().bytes
        == checkpoint_cut.source_bytes()
        && path.source_metric().utf16 == checkpoint_cut.source_utf16();
    path = match role {
        ParentBoundCurrentRestartRole::Direct => path,
        ParentBoundCurrentRestartRole::Normalization(_authority)
            if queried_path_is_source_complete =>
        {
            path
        }
        ParentBoundCurrentRestartRole::Normalization(authority) => {
            path.apply_parent_bound_normalization(authority)?
        }
    };
    if path.event_cut() != checkpoint_cut.green_events() {
        return Err(RestartCompositeDocumentError::Corrupt(
            "parent green restart path and selected checkpoint event cut disagree",
        )
        .into());
    }
    let path_source = path.source_metric();
    let kind = if path_source.bytes == checkpoint_cut.source_bytes()
        && path_source.utf16 == checkpoint_cut.source_utf16()
    {
        RestartSourceLedgerCheckpointKind::SourceCompleteLineBoundary
    } else if path_source.bytes.checked_add(1) == Some(checkpoint_cut.source_bytes())
        && path_source.utf16.checked_add(1) == Some(checkpoint_cut.source_utf16())
    {
        RestartSourceLedgerCheckpointKind::DeferredLf
    } else {
        return Err(RestartCompositeDocumentError::Corrupt(
            "parent green/source restart coordinates are neither deferred LF nor source-complete line boundary",
        )
        .into());
    };
    let source_metric = validated.green.source_metric();
    let source = SourceSnapshotDescriptor {
        revision: validated.green.source_revision(),
        root: validated.green.source_root(),
        bytes: usize::try_from(source_metric.bytes).map_err(|_| {
            RestartCompositeDocumentError::Corrupt(
                "parent source byte length does not fit the runtime",
            )
        })?,
    };
    let restart_anchor = bind_parent_selected_restart_anchor(
        RestartAnchorMint {
            arena,
            descriptor: validated.checkpoint_index,
            parent_root,
        },
        recipe,
    )?;
    Ok(RestartSourceLedgerCheckpointMint {
        parent_selection: RestartParentSelectionStamp { root: parent_root },
        source,
        checkpoint_cut,
        path,
        kind,
        restart_anchor,
    })
}

#[cfg(feature = "exact-parser")]
fn authenticate_normalization_checkpoint_from_validated_parent(
    arena: &PageArena,
    validated: &ValidatedRestartComposite,
    recipe: &LocatedDonorCheckpointRecipe,
) -> Result<ParentBoundNormalizationCheckpoint, RestartParentDonorResumeError> {
    authenticate_parent_bound_normalization_checkpoint(
        RestartCheckpointQueryMint {
            arena,
            descriptor: validated.checkpoint_index,
        },
        recipe,
    )
    .map_err(RestartParentDonorResumeError::from)
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum RestartParentDonorResumeError {
    Parent(RestartCompositeDocumentError),
    CurrentPath(CurrentRestartPathError),
    Parser(ParseError),
}

#[cfg(feature = "exact-parser")]
impl From<RestartCompositeDocumentError> for RestartParentDonorResumeError {
    fn from(error: RestartCompositeDocumentError) -> Self {
        Self::Parent(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<CommittedCheckpointIndexError> for RestartParentDonorResumeError {
    fn from(error: CommittedCheckpointIndexError) -> Self {
        Self::Parent(error.into())
    }
}

#[cfg(feature = "exact-parser")]
impl From<CurrentRestartPathError> for RestartParentDonorResumeError {
    fn from(error: CurrentRestartPathError) -> Self {
        Self::CurrentPath(error)
    }
}

#[cfg(feature = "exact-parser")]
impl fmt::Display for RestartParentDonorResumeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parent(error) => error.fmt(formatter),
            Self::CurrentPath(error) => error.fmt(formatter),
            Self::Parser(error) => write!(formatter, "parser resume failed: {error:?}"),
        }
    }
}

#[cfg(feature = "exact-parser")]
impl std::error::Error for RestartParentDonorResumeError {}

/// Linear old-child retention authority for a future adoption build. The two
/// `ArenaBuildOwner`s remain private and are never split or converted into
/// query IDs. The lease owns no borrow of the old actor document, so it may
/// move into a candidate; every splice/commit still revalidates the private
/// parent activation stamp and both retained children.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartCompositeAdoptionRetentionFailure {
    /// No adoption owner remains in the fresh journal, so the actor may keep
    /// its active parent and retry without first aborting this build.
    Pristine(RestartCompositeDocumentError),
    /// Retention crossed a mutation boundary or cleanup could not be proven.
    /// The actor must detach this attempt and drive the build through abort.
    Mutated {
        error: RestartCompositeDocumentError,
        cleanup_error: Option<RestartCompositeDocumentError>,
    },
}

#[cfg(feature = "exact-parser")]
impl fmt::Display for RestartCompositeAdoptionRetentionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pristine(error) => {
                write!(formatter, "pristine restart retention failure: {error}")
            }
            Self::Mutated {
                error,
                cleanup_error: None,
            } => write!(formatter, "mutated restart retention failure: {error}"),
            Self::Mutated {
                error,
                cleanup_error: Some(cleanup_error),
            } => write!(
                formatter,
                "mutated restart retention failure: {error}; cleanup failed: {cleanup_error}"
            ),
        }
    }
}

#[cfg(feature = "exact-parser")]
impl std::error::Error for RestartCompositeAdoptionRetentionFailure {}

#[cfg(feature = "exact-parser")]
#[must_use = "the retained old children must enter a same-journal splice or the build must abort"]
#[derive(Debug)]
pub(crate) struct RestartCompositeAdoptionLease {
    parent_activation: RestartParentActivationStamp,
    build: ArenaBuildId,
    green_owner: ArenaBuildOwner,
    checkpoint_owner: ArenaBuildOwner,
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
}

/// Non-owning, ABA-safe identity of the still-active old actor parent. The
/// retained child owners keep storage alive; this stamp separately requires
/// the actor root itself to remain live and semantically unchanged until an
/// adoption operation completes.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestartParentActivationStamp {
    root: ArenaScopedId,
}

/// Non-cloneable identity of the exact actor parent which selected one
/// restart. This value travels linearly through source lineage and donor
/// reconstruction, then brands a separately retained adoption lease. Its
/// private scoped ID cannot be reconstructed from equal cuts or child
/// descriptors.
#[cfg(feature = "exact-parser")]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RestartParentSelectionStamp {
    root: ArenaScopedId,
}

/// Adoption lease after consuming the exact selection stamp carried through
/// restart reconstruction. Only this branded form exposes retained green and
/// checkpoint-index children.
#[cfg(feature = "exact-parser")]
#[must_use = "the parent-matched retained children must be adopted or cancelled"]
#[derive(Debug)]
pub(crate) struct ParentSelectedRestartCompositeAdoptionLease {
    lease: RestartCompositeAdoptionLease,
}

/// Recoverable cross-parent branding failure. The unbranded lease remains
/// available so its journal can be cancelled without leaking linear cleanup
/// authority.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartCompositeAdoptionSelectionFailure {
    pub(crate) error: RestartCompositeDocumentError,
    pub(crate) lease: RestartCompositeAdoptionLease,
}

/// Private one-shot mint passed directly to the committed-index module. No
/// API returns this value, so a caller cannot substitute an owner or echoed
/// descriptor when deriving the typed checkpoint-splice borrow.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartCheckpointLeaseMint<'lease> {
    build: ArenaBuildId,
    parent_activation: ArenaScopedId,
    owner: &'lease ArenaBuildOwner,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
}

/// Private parent-to-index query mint. Requiring this carrier prevents any
/// crate-internal holder of an echoed descriptor from invoking donor lookup;
/// only a fresh v2 parent validation constructs it.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartCheckpointQueryMint<'arena> {
    arena: &'arena PageArena,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
}

/// Private composite-to-index mint for the one transition that binds a
/// selected donor recipe to its validated parent root. Its fields are private
/// to this module, so a standalone index lookup cannot create a restart
/// anchor.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartAnchorMint<'arena> {
    arena: &'arena PageArena,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
    parent_root: ArenaScopedId,
}

#[cfg(feature = "exact-parser")]
impl<'arena> RestartCheckpointQueryMint<'arena> {
    pub(crate) fn into_query_parts(
        self,
    ) -> (
        &'arena PageArena,
        CommittedCheckpointIndexCompositeDescriptor,
    ) {
        (self.arena, self.descriptor)
    }
}

#[cfg(feature = "exact-parser")]
impl<'arena> RestartAnchorMint<'arena> {
    pub(crate) fn into_anchor_parts(
        self,
    ) -> (
        &'arena PageArena,
        CommittedCheckpointIndexCompositeDescriptor,
        ArenaScopedId,
    ) {
        (self.arena, self.descriptor, self.parent_root)
    }
}

/// Private parent-to-green query mint. The serialized-green module consumes
/// this carrier, revalidates the complete descriptor, and performs a bounded
/// event-cut query without exposing either child root ID.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartGreenQueryMint<'arena> {
    arena: &'arena PageArena,
    descriptor: SerializedGreenCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl<'arena> RestartGreenQueryMint<'arena> {
    pub(crate) fn into_query_parts(
        self,
    ) -> (&'arena PageArena, SerializedGreenCompositeDescriptor) {
        (self.arena, self.descriptor)
    }
}

/// Symmetric private mint for the retained green child. As with the
/// checkpoint mint, no API returns this carrier or accepts caller-authored
/// descriptor fields.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartGreenLeaseMint<'lease> {
    build: ArenaBuildId,
    parent_activation: ArenaScopedId,
    owner: &'lease ArenaBuildOwner,
    descriptor: SerializedGreenCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl<'lease> RestartGreenLeaseMint<'lease> {
    pub(crate) fn into_green_lease_parts(
        self,
    ) -> (
        ArenaBuildId,
        ArenaScopedId,
        &'lease ArenaBuildOwner,
        SerializedGreenCompositeDescriptor,
    ) {
        (
            self.build,
            self.parent_activation,
            self.owner,
            self.descriptor,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl<'lease> RestartCheckpointLeaseMint<'lease> {
    pub(crate) fn into_checkpoint_lease_parts(
        self,
    ) -> (
        ArenaBuildId,
        ArenaScopedId,
        &'lease ArenaBuildOwner,
        CommittedCheckpointIndexCompositeDescriptor,
    ) {
        (
            self.build,
            self.parent_activation,
            self.owner,
            self.descriptor,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeAdoptionLease {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn source_root(&self) -> crate::SourceRootId {
        self.green.source_root()
    }

    pub(crate) const fn source_revision(&self) -> crate::SourceRevision {
        self.green.source_revision()
    }

    pub(crate) const fn source_metric(&self) -> crate::SerializedMetric {
        self.green.source_metric()
    }

    pub(crate) const fn final_checkpoint_measure(&self) -> RelativeCheckpointMeasure {
        self.checkpoint_index.final_measure()
    }

    /// Derives the sole typed checkpoint-splice seam from this still-joined
    /// two-child lease. It validates the complete parent and both journal
    /// owners before minting the borrow; the splice job revalidates the child
    /// again at admission.
    fn checkpoint_index_for_splice_after_selection<'lease>(
        &'lease self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ParentRetainedCheckpointIndexLease<'lease>, RestartCompositeDocumentError> {
        // The integrated adoption transaction admits the green suffix first,
        // which legitimately adds working owners to this same build journal
        // before the checkpoint splice starts. Revalidate the exact parent and
        // both originally retained children here without requiring the journal
        // to remain otherwise pristine.
        self.validate_retained_children_in_session(session)?;
        let retained =
            ParentRetainedCheckpointIndexLease::from_parent_mint(RestartCheckpointLeaseMint {
                build: self.build,
                parent_activation: self.parent_activation.root,
                owner: &self.checkpoint_owner,
                descriptor: self.checkpoint_index,
            });
        retained.validate_session(session)?;
        Ok(retained)
    }

    /// Symmetric green-child bridge for retained-prefix restart and
    /// builder-owned suffix splice. The returned borrow remains joined to
    /// this two-child lease and exposes no document, manifest, or root ID.
    fn green_for_adoption_after_selection<'lease>(
        &'lease self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ParentRetainedGreenLease<'lease>, RestartCompositeDocumentError> {
        self.validate_session(session)?;
        let retained = ParentRetainedGreenLease::from_parent_mint(RestartGreenLeaseMint {
            build: self.build,
            parent_activation: self.parent_activation.root,
            owner: &self.green_owner,
            descriptor: self.green,
        });
        retained.validate_session(session)?;
        Ok(retained)
    }

    /// Consumes the non-cloneable parent-selection stamp before either child
    /// can be exposed. Equal checkpoint cuts and equal child descriptors from
    /// another actor parent cannot brand this lease.
    pub(crate) fn join_parent_selection(
        self,
        selection: RestartParentSelectionStamp,
    ) -> Result<ParentSelectedRestartCompositeAdoptionLease, RestartCompositeAdoptionSelectionFailure>
    {
        if self.parent_activation.root != selection.root {
            return Err(RestartCompositeAdoptionSelectionFailure {
                error: RestartCompositeDocumentError::Invalid(
                    "restart selection and retained parent activation differ",
                ),
                lease: self,
            });
        }
        Ok(ParentSelectedRestartCompositeAdoptionLease { lease: self })
    }

    /// Production restart join: the same non-cloneable selection stamp must
    /// identify both the retained two-child lease and the composite root that
    /// minted the donor anchor. The anchor is returned only after branding, so
    /// source/green state from one composite cannot be paired with an index
    /// sample from another composite that happens to share a child root.
    pub(crate) fn join_parent_selection_and_restart_anchor(
        self,
        selection: RestartParentSelectionStamp,
        anchor: ParentSelectedRestartAnchor,
    ) -> Result<
        (
            ParentSelectedRestartCompositeAdoptionLease,
            ParentSelectedRestartAnchor,
        ),
        RestartCompositeAdoptionSelectionFailure,
    > {
        if self.parent_activation.root != selection.root
            || !anchor.matches_parent_root(selection.root)
        {
            return Err(RestartCompositeAdoptionSelectionFailure {
                error: RestartCompositeDocumentError::Invalid(
                    "restart selection, donor anchor, and retained parent activation differ",
                ),
                lease: self,
            });
        }
        Ok((
            ParentSelectedRestartCompositeAdoptionLease { lease: self },
            anchor,
        ))
    }

    /// Cancels the fresh adoption build as one linear operation. Dropping the
    /// two hidden handles does not release them independently; the arena-owned
    /// journal retains both until bounded abort polling schedules them.
    pub(crate) fn cancel(
        self,
        session: ArenaBuildSession<'_>,
    ) -> Result<ArenaBuildId, RestartCompositeDocumentError> {
        self.validate_session(&session)?;
        drop(self);
        session.begin_abort().map_err(Into::into)
    }

    /// Revalidates parent, arena, build generation, both hidden owner handles,
    /// and both complete child descriptors before a later splice can consume
    /// the lease.
    pub(crate) fn validate_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), RestartCompositeDocumentError> {
        if session.id() != self.build {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart adoption lease and build generations differ",
            ));
        }
        let parent_root = session.arena().local_id(self.parent_activation.root)?;
        let validated = validate_restart_composite_manifest(session.arena(), parent_root)?;
        if validated.green != self.green
            || validated.checkpoint_index != self.checkpoint_index
            || session.owner_id(&self.green_owner)? != validated.green_root
            || session.owner_id(&self.checkpoint_owner)? != validated.checkpoint_index_root
            || session.live_owners()? != COMPOSITE_CHILDREN
        {
            return Err(RestartCompositeDocumentError::Corrupt(
                "restart adoption lease no longer matches its parent or journal",
            ));
        }
        Ok(())
    }

    /// Revalidates the actor parent and the two retained child owners without
    /// requiring the adoption journal to remain otherwise empty. A retained
    /// green rewrite necessarily allocates additional owners after pristine
    /// admission; those new owners must not make the parent capability stale.
    fn validate_retained_children_in_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenCompositeDescriptor, RestartCompositeDocumentError> {
        if session.id() != self.build {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart adoption lease and build generations differ",
            ));
        }
        let parent_root = session.arena().local_id(self.parent_activation.root)?;
        let validated = validate_restart_composite_manifest(session.arena(), parent_root)?;
        if validated.green != self.green
            || validated.checkpoint_index != self.checkpoint_index
            || session.owner_id(&self.green_owner)? != validated.green_root
            || session.owner_id(&self.checkpoint_owner)? != validated.checkpoint_index_root
        {
            return Err(RestartCompositeDocumentError::Corrupt(
                "restart adoption lease no longer matches its parent or retained children",
            ));
        }
        Ok(validated.green)
    }

    /// Read-only suspended sibling for convergence selection after suffix
    /// replay has allocated working owners. Unlike pristine restart admission,
    /// it authenticates the two retained parent children without requiring the
    /// journal to contain only those children.
    fn validate_retained_children_suspended(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<SerializedGreenCompositeDescriptor, RestartCompositeDocumentError> {
        if ticket.id() != self.build {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart adoption lease and suspended build generations differ",
            ));
        }
        let parent_root = arena.local_id(self.parent_activation.root)?;
        let validated = validate_restart_composite_manifest(arena, parent_root)?;
        if validated.green != self.green
            || validated.checkpoint_index != self.checkpoint_index
            || arena.suspended_owner_id(ticket, &self.green_owner)? != validated.green_root
            || arena.suspended_owner_id(ticket, &self.checkpoint_owner)?
                != validated.checkpoint_index_root
        {
            return Err(RestartCompositeDocumentError::Corrupt(
                "suspended restart adoption lease no longer matches retained children",
            ));
        }
        Ok(validated.green)
    }

    /// Suspended sibling of `validate_retained_children_in_session`. The
    /// exact ticket proves the journal cannot mutate while a resumable green
    /// job is being constructed. Admission remains intentionally stricter:
    /// the journal must still contain only the two parent-retained children.
    fn validate_pristine_suspended_children(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<SerializedGreenCompositeDescriptor, RestartCompositeDocumentError> {
        if ticket.id() != self.build {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart adoption lease and suspended build generations differ",
            ));
        }
        let parent_root = arena.local_id(self.parent_activation.root)?;
        let validated = validate_restart_composite_manifest(arena, parent_root)?;
        if validated.green != self.green
            || validated.checkpoint_index != self.checkpoint_index
            || arena.suspended_owner_id(ticket, &self.green_owner)? != validated.green_root
            || arena.suspended_owner_id(ticket, &self.checkpoint_owner)?
                != validated.checkpoint_index_root
            || arena.build_journal_metrics(self.build)?.live_owners != COMPOSITE_CHILDREN
        {
            return Err(RestartCompositeDocumentError::Corrupt(
                "suspended restart adoption lease is not its pristine parent retention",
            ));
        }
        Ok(validated.green)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedRestartCompositeAdoptionLease {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.lease.build_id()
    }

    pub(crate) const fn source_root(&self) -> crate::SourceRootId {
        self.lease.source_root()
    }

    pub(crate) const fn source_revision(&self) -> crate::SourceRevision {
        self.lease.source_revision()
    }

    pub(crate) const fn source_metric(&self) -> crate::SerializedMetric {
        self.lease.source_metric()
    }

    pub(crate) const fn final_checkpoint_measure(&self) -> RelativeCheckpointMeasure {
        self.lease.final_checkpoint_measure()
    }

    pub(crate) fn checkpoint_index_for_splice<'lease>(
        &'lease self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ParentRetainedCheckpointIndexLease<'lease>, RestartCompositeDocumentError> {
        self.lease
            .checkpoint_index_for_splice_after_selection(session)
    }

    /// Mints a read-only retained-index borrow while the candidate writer is
    /// paused. Working green owners are allowed; both original child edges and
    /// the complete parent manifest are still revalidated first.
    pub(crate) fn checkpoint_index_for_convergence<'lease>(
        &'lease self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<ParentRetainedCheckpointIndexLease<'lease>, RestartCompositeDocumentError> {
        self.lease
            .validate_retained_children_suspended(ticket, arena)?;
        Ok(ParentRetainedCheckpointIndexLease::from_parent_mint(
            RestartCheckpointLeaseMint {
                build: self.lease.build,
                parent_activation: self.lease.parent_activation.root,
                owner: &self.lease.checkpoint_owner,
                descriptor: self.lease.checkpoint_index,
            },
        ))
    }

    /// Suspended green-child sibling for resolving the semantic adoption cut
    /// corresponding to an authenticated old parser checkpoint. Working
    /// journal owners are allowed; the retained parent and both original child
    /// edges are revalidated first.
    pub(crate) fn green_for_convergence<'lease>(
        &'lease self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<ParentRetainedGreenLease<'lease>, RestartCompositeDocumentError> {
        self.lease
            .validate_retained_children_suspended(ticket, arena)?;
        Ok(ParentRetainedGreenLease::from_parent_mint(
            RestartGreenLeaseMint {
                build: self.lease.build,
                parent_activation: self.lease.parent_activation.root,
                owner: &self.lease.green_owner,
                descriptor: self.lease.green,
            },
        ))
    }

    pub(crate) fn green_for_adoption<'lease>(
        &'lease self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ParentRetainedGreenLease<'lease>, RestartCompositeDocumentError> {
        self.lease.green_for_adoption_after_selection(session)
    }

    pub(crate) fn validate_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), RestartCompositeDocumentError> {
        self.lease.validate_session(session)
    }

    /// Production retained-restart admission seam. It returns only the typed green
    /// descriptor; its manifest and sequence IDs remain private to the
    /// serialized-green module which owns the bounded inverse algorithm.
    pub(crate) fn validated_suspended_green_for_restart(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<SerializedGreenCompositeDescriptor, RestartCompositeDocumentError> {
        self.lease
            .validate_pristine_suspended_children(ticket, arena)
    }

    /// Poll-time retained-restart seam. Unlike pristine admission, this permits the
    /// resumable green job's additional journal owners while continuing to
    /// authenticate the exact actor root and both retained parent children.
    pub(crate) fn revalidate_green_for_restart(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenCompositeDescriptor, RestartCompositeDocumentError> {
        self.lease.validate_retained_children_in_session(session)
    }

    pub(crate) fn cancel(
        self,
        session: ArenaBuildSession<'_>,
    ) -> Result<ArenaBuildId, RestartCompositeDocumentError> {
        self.lease.cancel(session)
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartCompositeDocumentReleaseFailure {
    pub(crate) error: RestartCompositeDocumentError,
    pub(crate) document: RestartCompositeDocument,
}

/// A v2 document view cannot outlive its owning parent. Child identities stay
/// private even inside this value; callers receive only typed semantic views.
#[cfg(feature = "exact-parser")]
pub(crate) struct RestartCompositeDocumentView<'parent> {
    parent: &'parent RestartCompositeDocument,
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl<'parent> RestartCompositeDocumentView<'parent> {
    pub(crate) fn green(&self) -> RestartGreenChildView<'parent> {
        RestartGreenChildView {
            parent: self.parent,
            descriptor: self.green,
        }
    }

    pub(crate) fn checkpoint_index(&self) -> RestartCheckpointIndexChildView<'parent> {
        RestartCheckpointIndexChildView {
            parent: self.parent,
            descriptor: self.checkpoint_index,
        }
    }
}

#[cfg(feature = "exact-parser")]
pub(crate) struct RestartGreenChildView<'parent> {
    parent: &'parent RestartCompositeDocument,
    descriptor: SerializedGreenCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl RestartGreenChildView<'_> {
    pub(crate) const fn source_root(&self) -> crate::SourceRootId {
        self.descriptor.source_root()
    }

    pub(crate) const fn source_revision(&self) -> crate::SourceRevision {
        self.descriptor.source_revision()
    }

    pub(crate) const fn source_metric(&self) -> crate::SerializedMetric {
        self.descriptor.source_metric()
    }

    pub(crate) const fn syntax_profile(&self) -> u64 {
        self.descriptor.syntax_profile()
    }

    pub(crate) const fn grammar_revision(&self) -> crate::GrammarRevision {
        self.descriptor.grammar_revision()
    }

    pub(crate) const fn parse_generation(&self) -> crate::ParseGeneration {
        self.descriptor.parse_generation()
    }

    pub(crate) const fn semantic_epoch(&self) -> u64 {
        self.descriptor.semantic_epoch()
    }

    pub(crate) const fn tokens(&self) -> u64 {
        self.descriptor.tokens()
    }

    pub(crate) const fn coverage_count(&self) -> u64 {
        self.descriptor.coverage_count()
    }

    pub(crate) const fn logical_metric(&self) -> crate::SerializedMetric {
        self.descriptor.logical_metric()
    }

    pub(crate) const fn parent(&self) -> &RestartCompositeDocument {
        self.parent
    }
}

#[cfg(feature = "exact-parser")]
pub(crate) struct RestartCheckpointIndexChildView<'parent> {
    parent: &'parent RestartCompositeDocument,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl RestartCheckpointIndexChildView<'_> {
    pub(crate) const fn final_measure(&self) -> RelativeCheckpointMeasure {
        self.descriptor.final_measure()
    }

    pub(crate) const fn physical_lines(&self) -> u64 {
        self.descriptor.final_measure().physical_lines()
    }

    pub(crate) const fn has_terminal_tail(&self) -> bool {
        self.descriptor.has_terminal_tail()
    }

    pub(crate) const fn parent(&self) -> &RestartCompositeDocument {
        self.parent
    }
}

/// Published counterpart of `RestartCompositeDocumentView`. It owns no arena
/// reference or storage authority and cannot be constructed without a fresh
/// exact coordinator/root validation.
#[cfg(feature = "exact-parser")]
pub(crate) struct PublishedRestartCompositeDocumentView<'binding> {
    parent: &'binding PublishedRestartCompositeHandle,
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl<'binding> PublishedRestartCompositeDocumentView<'binding> {
    pub(crate) fn green(&self) -> PublishedRestartGreenChildView<'binding> {
        PublishedRestartGreenChildView {
            parent: self.parent,
            descriptor: self.green,
        }
    }

    pub(crate) fn checkpoint_index(&self) -> PublishedRestartCheckpointIndexChildView<'binding> {
        PublishedRestartCheckpointIndexChildView {
            parent: self.parent,
            descriptor: self.checkpoint_index,
        }
    }
}

#[cfg(feature = "exact-parser")]
pub(crate) struct PublishedRestartGreenChildView<'binding> {
    parent: &'binding PublishedRestartCompositeHandle,
    descriptor: SerializedGreenCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl PublishedRestartGreenChildView<'_> {
    pub(crate) const fn source_root(&self) -> crate::SourceRootId {
        self.descriptor.source_root()
    }

    pub(crate) const fn source_revision(&self) -> crate::SourceRevision {
        self.descriptor.source_revision()
    }

    pub(crate) const fn source_metric(&self) -> crate::SerializedMetric {
        self.descriptor.source_metric()
    }

    #[cfg(feature = "host-mirror-probe")]
    pub(crate) const fn descriptor_for_host_snapshot(&self) -> SerializedGreenCompositeDescriptor {
        self.descriptor
    }

    #[cfg(test)]
    pub(crate) const fn descriptor_for_test(&self) -> SerializedGreenCompositeDescriptor {
        self.descriptor
    }

    pub(crate) const fn syntax_profile(&self) -> u64 {
        self.descriptor.syntax_profile()
    }

    pub(crate) const fn grammar_revision(&self) -> crate::GrammarRevision {
        self.descriptor.grammar_revision()
    }

    pub(crate) const fn parse_generation(&self) -> crate::ParseGeneration {
        self.descriptor.parse_generation()
    }

    pub(crate) const fn semantic_epoch(&self) -> u64 {
        self.descriptor.semantic_epoch()
    }

    pub(crate) const fn tokens(&self) -> u64 {
        self.descriptor.tokens()
    }

    pub(crate) const fn coverage_count(&self) -> u64 {
        self.descriptor.coverage_count()
    }

    pub(crate) const fn logical_metric(&self) -> crate::SerializedMetric {
        self.descriptor.logical_metric()
    }

    pub(crate) const fn parent(&self) -> &PublishedRestartCompositeHandle {
        self.parent
    }
}

#[cfg(feature = "exact-parser")]
pub(crate) struct PublishedRestartCheckpointIndexChildView<'binding> {
    parent: &'binding PublishedRestartCompositeHandle,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl PublishedRestartCheckpointIndexChildView<'_> {
    pub(crate) const fn final_measure(&self) -> RelativeCheckpointMeasure {
        self.descriptor.final_measure()
    }

    pub(crate) const fn physical_lines(&self) -> u64 {
        self.descriptor.final_measure().physical_lines()
    }

    pub(crate) const fn has_terminal_tail(&self) -> bool {
        self.descriptor.has_terminal_tail()
    }

    pub(crate) const fn parent(&self) -> &PublishedRestartCompositeHandle {
        self.parent
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct RestartCompositeDocumentBuilder;

/// A pre-allocation rejection returns the complete linear replacement so the
/// actor may drain unrelated scratch owners and retry. Once the new parent has
/// been allocated, rollback is intentionally forbidden: the build journal is
/// the sole cleanup authority and the actor must drive bounded abort.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum RestartCompositeReplacementJoinFailure {
    Retryable {
        error: RestartCompositeDocumentError,
        replacement: ParentSelectedRestartCompositeReplacement,
    },
    AbortRequired {
        error: RestartCompositeDocumentError,
        build: ArenaBuildId,
    },
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeDocumentBuilder {
    /// Sole production v2 builder. It consumes one private writer-minted seal,
    /// never independently supplied green/index manifests.
    pub(crate) fn join(
        session: &mut ArenaBuildSession<'_>,
        children: RestartCompositeChildren,
    ) -> Result<RestartCompositeDocumentBuildManifest, RestartCompositeDocumentError> {
        let build = session.id();
        let (green, checkpoint_index) = children.into_parent_parts();
        if green.build_id() != build || checkpoint_index.build_id() != build {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart children and arena session build generations differ",
            ));
        }

        let green_descriptor = green.composite_descriptor(session)?;
        let checkpoint_descriptor = checkpoint_index.composite_descriptor(session)?;
        validate_restart_semantics(green_descriptor, checkpoint_descriptor)?;
        let green_id = green.validate_composite_child(session)?;
        let checkpoint_id = checkpoint_index.validate_composite_child(session)?;
        if green_id == checkpoint_id {
            return Err(RestartCompositeDocumentError::Corrupt(
                "restart child roles alias one arena node",
            ));
        }
        if session.live_owners()? != COMPOSITE_CHILDREN {
            return Err(RestartCompositeDocumentError::Invalid(
                "restart join requires exactly its two writer-minted child owners",
            ));
        }

        let payload = encode_restart_composite_manifest(
            green_id,
            checkpoint_id,
            green_descriptor,
            checkpoint_descriptor,
        );
        let (root, allocation) = session.allocate_packed(&payload, &[green_id, checkpoint_id])?;
        let root_id = session.owner_id(&root)?;
        let (green_owner, green_receipt) = green.into_composite_parts();
        let (checkpoint_owner, checkpoint_receipt) = checkpoint_index.into_composite_parts();
        session.release(green_owner)?;
        session.release(checkpoint_owner)?;

        let validated = validate_restart_composite_manifest(session.arena(), root_id)?;
        if validated.green != green_descriptor
            || validated.checkpoint_index != checkpoint_descriptor
        {
            return Err(RestartCompositeDocumentError::Corrupt(
                "restart child identity changed during adoption",
            ));
        }
        if session.live_owners()? != 1 {
            return Err(RestartCompositeDocumentError::Corrupt(
                "restart join did not reduce the journal to one parent root",
            ));
        }

        Ok(RestartCompositeDocumentBuildManifest {
            build,
            owner: root,
            receipt: RestartCompositeDocumentBuildReceipt {
                green: green_receipt,
                checkpoint_index: checkpoint_receipt,
                manifest_nodes_allocated: 1,
                payload_bytes_copied: allocation.payload_bytes_copied,
                edge_bytes_copied: allocation.edge_bytes_copied,
                child_references_added: allocation.child_references_added,
                maximum_page_payload_bytes: allocation.payload_bytes_copied,
            },
        })
    }

    /// Atomically replaces the two children retained from the selected actor
    /// parent with the two completed current-candidate children.
    ///
    /// This is a persistent-graph adoption, not an in-place mutation. The old
    /// published parent remains live throughout; the candidate journal owns
    /// two extra references to its children plus the two new child roots. A
    /// successful join creates a new parent, releases all four direct child
    /// owners, and leaves exactly that parent in the journal for coordinator
    /// publication.
    #[allow(dead_code)] // Production mint arrives with the green/index rendezvous.
    pub(crate) fn join_adopted_candidate(
        session: &mut ArenaBuildSession<'_>,
        replacement: ParentSelectedRestartCompositeReplacement,
    ) -> Result<RestartCompositeDocumentBuildManifest, RestartCompositeReplacementJoinFailure> {
        let build = session.id();
        let preflight = (|| -> Result<
            (
                SerializedGreenCompositeDescriptor,
                CommittedCheckpointIndexCompositeDescriptor,
                ArenaId,
                ArenaId,
            ),
            RestartCompositeDocumentError,
        > {
            let (adoption, children) = replacement.parent_parts();
            if adoption.build_id() != build {
                return Err(RestartCompositeDocumentError::Invalid(
                    "restart replacement and arena session build generations differ",
                ));
            }
            // Unlike pristine restart admission, completed child jobs have
            // added owners. This still revalidates the exact actor parent and
            // both retained old-child capabilities.
            adoption.revalidate_green_for_restart(session)?;

            let (green, checkpoint_index) = children.parent_parts();
            if green.build_id() != build || checkpoint_index.build_id() != build {
                return Err(RestartCompositeDocumentError::Invalid(
                    "restart replacement children and arena session build generations differ",
                ));
            }
            let green_descriptor = green.composite_descriptor(session)?;
            let checkpoint_descriptor = checkpoint_index.composite_descriptor(session)?;
            validate_restart_semantics(green_descriptor, checkpoint_descriptor)?;
            let green_id = green.validate_composite_child(session)?;
            let checkpoint_id = checkpoint_index.validate_composite_child(session)?;
            if green_id == checkpoint_id {
                return Err(RestartCompositeDocumentError::Corrupt(
                    "restart replacement child roles alias one arena node",
                ));
            }
            if session.live_owners()? != RESTART_REPLACEMENT_CHILD_OWNERS {
                return Err(RestartCompositeDocumentError::Invalid(
                    "restart replacement requires exactly two retained and two new child owners",
                ));
            }
            Ok((
                green_descriptor,
                checkpoint_descriptor,
                green_id,
                checkpoint_id,
            ))
        })();
        let (green_descriptor, checkpoint_descriptor, green_id, checkpoint_id) = match preflight {
            Ok(preflight) => preflight,
            Err(error) => {
                return Err(RestartCompositeReplacementJoinFailure::Retryable {
                    error,
                    replacement,
                });
            }
        };

        let payload = encode_restart_composite_manifest(
            green_id,
            checkpoint_id,
            green_descriptor,
            checkpoint_descriptor,
        );
        let (root, allocation) = match session.allocate_packed(&payload, &[green_id, checkpoint_id])
        {
            Ok(allocation) => allocation,
            Err(error) => {
                return Err(RestartCompositeReplacementJoinFailure::Retryable {
                    error: error.into(),
                    replacement,
                });
            }
        };

        // From this point onward the newly allocated parent is the journal's
        // rollback authority. Any unexpected transfer or validation failure
        // must abort the whole candidate; reconstructing a retry bundle would
        // create two competing cleanup authorities.
        let root_id = match session.owner_id(&root) {
            Ok(root_id) => root_id,
            Err(error) => {
                return Err(RestartCompositeReplacementJoinFailure::AbortRequired {
                    error: error.into(),
                    build,
                });
            }
        };
        let (adoption, children) = replacement.into_parent_parts();
        let ParentSelectedRestartCompositeAdoptionLease {
            lease:
                RestartCompositeAdoptionLease {
                    green_owner: old_green_owner,
                    checkpoint_owner: old_checkpoint_owner,
                    ..
                },
        } = adoption;
        let (green, checkpoint_index) = children.into_parent_parts();
        let (new_green_owner, green_receipt) = green.into_composite_parts();
        let (new_checkpoint_owner, checkpoint_receipt) = checkpoint_index.into_composite_parts();

        for owner in [
            old_green_owner,
            old_checkpoint_owner,
            new_green_owner,
            new_checkpoint_owner,
        ] {
            if let Err(error) = session.release(owner) {
                return Err(RestartCompositeReplacementJoinFailure::AbortRequired {
                    error: error.into(),
                    build,
                });
            }
        }

        let validated = match validate_restart_composite_manifest(session.arena(), root_id) {
            Ok(validated) => validated,
            Err(error) => {
                return Err(RestartCompositeReplacementJoinFailure::AbortRequired { error, build });
            }
        };
        if validated.green != green_descriptor
            || validated.checkpoint_index != checkpoint_descriptor
        {
            return Err(RestartCompositeReplacementJoinFailure::AbortRequired {
                error: RestartCompositeDocumentError::Corrupt(
                    "restart replacement child identity changed during adoption",
                ),
                build,
            });
        }
        match session.live_owners() {
            Ok(1) => {}
            Ok(_) => {
                return Err(RestartCompositeReplacementJoinFailure::AbortRequired {
                    error: RestartCompositeDocumentError::Corrupt(
                        "restart replacement did not reduce the journal to one parent root",
                    ),
                    build,
                });
            }
            Err(error) => {
                return Err(RestartCompositeReplacementJoinFailure::AbortRequired {
                    error: error.into(),
                    build,
                });
            }
        }

        Ok(RestartCompositeDocumentBuildManifest {
            build,
            owner: root,
            receipt: RestartCompositeDocumentBuildReceipt {
                green: green_receipt,
                checkpoint_index: checkpoint_receipt,
                manifest_nodes_allocated: 1,
                payload_bytes_copied: allocation.payload_bytes_copied,
                edge_bytes_copied: allocation.edge_bytes_copied,
                child_references_added: allocation.child_references_added,
                maximum_page_payload_bytes: allocation.payload_bytes_copied,
            },
        })
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartCompositeDocumentError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Coordinator(CoordinatorError),
    Green(SerializedGreenError),
    CheckpointIndex(CommittedCheckpointIndexError),
    Invalid(&'static str),
    Corrupt(&'static str),
}

#[cfg(feature = "exact-parser")]
impl From<ArenaError> for RestartCompositeDocumentError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<ArenaBuildError> for RestartCompositeDocumentError {
    fn from(error: ArenaBuildError) -> Self {
        Self::ArenaBuild(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<CoordinatorError> for RestartCompositeDocumentError {
    fn from(error: CoordinatorError) -> Self {
        Self::Coordinator(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<SerializedGreenError> for RestartCompositeDocumentError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<CommittedCheckpointIndexError> for RestartCompositeDocumentError {
    fn from(error: CommittedCheckpointIndexError) -> Self {
        Self::CheckpointIndex(error)
    }
}

#[cfg(feature = "exact-parser")]
impl fmt::Display for RestartCompositeDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Coordinator(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
            Self::CheckpointIndex(error) => error.fmt(formatter),
            Self::Invalid(message) => write!(formatter, "invalid restart composite: {message}"),
            Self::Corrupt(message) => write!(formatter, "corrupt restart composite: {message}"),
        }
    }
}

#[cfg(feature = "exact-parser")]
impl std::error::Error for RestartCompositeDocumentError {}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy)]
struct ValidatedRestartComposite {
    green_root: ArenaId,
    checkpoint_index_root: ArenaId,
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
fn validate_restart_semantics(
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
) -> Result<(), RestartCompositeDocumentError> {
    let source = green.source_metric();
    let output = checkpoint_index.final_measure();
    if source != green.physical_metric()
        || output.source_bytes() != source.bytes
        || output.source_utf16() != source.utf16
        || output.green_events() != green.tokens()
        || output.projection_runs() != green.coverage_count()
        || green.coverage_count() > green.tokens()
        || green.balance() != 0
        || green.known_bytes_start() > green.known_bytes_end()
        || green.known_bytes_end() > source.bytes
    {
        return Err(RestartCompositeDocumentError::Corrupt(
            "restart source, generation, green, and checkpoint totals disagree",
        ));
    }
    Ok(())
}

#[cfg(feature = "exact-parser")]
fn validate_restart_composite_manifest(
    arena: &PageArena,
    root: ArenaId,
) -> Result<ValidatedRestartComposite, RestartCompositeDocumentError> {
    let payload = arena.payload(root)?;
    if payload.len() != RESTART_COMPOSITE_MANIFEST_BYTES
        || payload[0] != RESTART_COMPOSITE_MANIFEST_TAG
        || payload[1] != RESTART_COMPOSITE_FORMAT_VERSION
        || payload[2] != RESTART_AUTHORITATIVE_ROLE_TAG
        || payload[3] != ENCODED_COMPOSITE_CHILDREN
        || payload[4] != GREEN_CHILD_ROLE_TAG
        || payload[5] != CHECKPOINT_INDEX_CHILD_ROLE_TAG
        || payload[6] & NONRELOADABLE_ACTOR_OWNED_ALLOCATOR_FLAG == 0
        || payload[6] & !(NONRELOADABLE_ACTOR_OWNED_ALLOCATOR_FLAG | TERMINAL_TAIL_FLAG) != 0
        || payload[7] != 0
    {
        return Err(RestartCompositeDocumentError::Corrupt(
            "invalid restart composite header",
        ));
    }
    if arena.packed_child_count(root)? != COMPOSITE_CHILDREN {
        return Err(RestartCompositeDocumentError::Corrupt(
            "restart composite must own exactly two role-ordered children",
        ));
    }
    let green_id = arena.packed_child_at(root, 0)?;
    let checkpoint_id = arena.packed_child_at(root, 1)?;
    if green_id == checkpoint_id
        || decode_arena_id(&payload[8..16]) != green_id
        || decode_arena_id(&payload[16..24]) != checkpoint_id
    {
        return Err(RestartCompositeDocumentError::Corrupt(
            "restart encoded identities and child roles disagree",
        ));
    }
    let green = validate_serialized_green_composite_child(arena, green_id)?;
    let checkpoint_index =
        validate_committed_checkpoint_index_composite_child(arena, checkpoint_id)?;
    validate_restart_semantics(green, checkpoint_index)?;
    let expected =
        encode_restart_composite_manifest(green_id, checkpoint_id, green, checkpoint_index);
    if payload != expected {
        return Err(RestartCompositeDocumentError::Corrupt(
            "restart payload descriptors or summary totals are forged",
        ));
    }
    Ok(ValidatedRestartComposite {
        green_root: green_id,
        checkpoint_index_root: checkpoint_id,
        green,
        checkpoint_index,
    })
}

#[cfg(feature = "exact-parser")]
fn encode_restart_composite_manifest(
    green_id: ArenaId,
    checkpoint_id: ArenaId,
    green: SerializedGreenCompositeDescriptor,
    checkpoint_index: CommittedCheckpointIndexCompositeDescriptor,
) -> [u8; RESTART_COMPOSITE_MANIFEST_BYTES] {
    let mut payload = [0_u8; RESTART_COMPOSITE_MANIFEST_BYTES];
    payload[0] = RESTART_COMPOSITE_MANIFEST_TAG;
    payload[1] = RESTART_COMPOSITE_FORMAT_VERSION;
    payload[2] = RESTART_AUTHORITATIVE_ROLE_TAG;
    payload[3] = ENCODED_COMPOSITE_CHILDREN;
    payload[4] = GREEN_CHILD_ROLE_TAG;
    payload[5] = CHECKPOINT_INDEX_CHILD_ROLE_TAG;
    payload[6] = NONRELOADABLE_ACTOR_OWNED_ALLOCATOR_FLAG
        | if checkpoint_index.has_terminal_tail() {
            TERMINAL_TAIL_FLAG
        } else {
            0
        };
    encode_arena_id(&mut payload[8..16], green_id);
    encode_arena_id(&mut payload[16..24], checkpoint_id);
    put_u64(&mut payload, 24, green.source_root().0);
    put_u64(&mut payload, 32, green.source_revision().0);
    put_u64(&mut payload, 40, green.source_metric().bytes);
    put_u64(&mut payload, 48, green.source_metric().utf16);
    put_u64(&mut payload, 56, green.syntax_profile());
    put_u64(&mut payload, 64, green.grammar_revision().0);
    put_u64(&mut payload, 72, green.parse_generation().0);
    put_u64(&mut payload, 80, green.semantic_epoch());
    put_u64(&mut payload, 88, green.known_bytes_start());
    put_u64(&mut payload, 96, green.known_bytes_end());
    let measure = checkpoint_index.final_measure();
    put_u64(&mut payload, 104, measure.source_bytes());
    put_u64(&mut payload, 112, measure.source_utf16());
    put_u64(&mut payload, 120, measure.physical_lines());
    put_u64(&mut payload, 128, measure.green_events());
    put_u64(&mut payload, 136, measure.projection_runs());
    put_u64(&mut payload, 144, checkpoint_index.leaf_pages());
    put_u64(&mut payload, 152, checkpoint_index.partitions());
    put_u64(&mut payload, 160, checkpoint_index.samples());
    put_u64(&mut payload, 168, u64::from(checkpoint_index.height()));
    put_u64(&mut payload, 176, green.leaf_pages());
    put_u64(&mut payload, 184, green.tokens());
    put_u64(&mut payload, 192, green.blocks());
    put_u64(&mut payload, 200, u64::from(green.height()));
    put_u64(&mut payload, 208, green.physical_metric().bytes);
    put_u64(&mut payload, 216, green.physical_metric().utf16);
    put_u64(&mut payload, 224, green.logical_metric().bytes);
    put_u64(&mut payload, 232, green.logical_metric().utf16);
    put_u64(&mut payload, 240, green.coverage_count());
    put_i64(&mut payload, 248, green.balance());
    put_i64(&mut payload, 256, green.minimum_prefix());
    put_i64(
        &mut payload,
        264,
        green.minimum_closed_depth().unwrap_or(i64::MIN),
    );
    payload
}

#[cfg(feature = "exact-parser")]
fn encode_arena_id(output: &mut [u8], id: ArenaId) {
    output[..4].copy_from_slice(&id.index.to_le_bytes());
    output[4..8].copy_from_slice(&id.generation.to_le_bytes());
}

#[cfg(feature = "exact-parser")]
fn decode_arena_id(input: &[u8]) -> ArenaId {
    ArenaId {
        index: u32::from_le_bytes(input[..4].try_into().expect("arena index field")),
        generation: u32::from_le_bytes(input[4..8].try_into().expect("arena generation field")),
    }
}

#[cfg(feature = "exact-parser")]
fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(feature = "exact-parser")]
fn put_i64(output: &mut [u8], offset: usize, value: i64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[cfg(feature = "exact-parser")]
    use crate::committed_checkpoint_index::DonorCheckpointSampleDraft;
    use crate::committed_checkpoint_index::{
        RelativeCheckpointMeasure, StorageOnlyCheckpointIndexBuilder,
        StorageOnlyCheckpointPartition, StorageOnlyNormalizationOutcome,
    };
    #[cfg(feature = "exact-parser")]
    use crate::serialized_green::CurrentRestartNormalizationRole;
    #[cfg(feature = "exact-parser")]
    use crate::serialized_green::setext_retained_restart::{
        ParentSelectedSetextRetainedGreenRestart, ParentSelectedSetextRetainedGreenRestartError,
        SetextRetainedGreenRestartProgress,
    };
    use crate::serialized_green::{
        CoveragePart, FactsEnvelope, GreenEvent, GreenHeadingOpenFacts, GreenKind,
        LogicalContribution, ResumableSerializedGreenBuild, SerializedGreenRootSpec,
        SerializedGreenStreamProgress, SourceProjectionRun,
    };
    use crate::{
        ARENA_PAGE_BYTES, ArenaLimits, BlockId, ClosedChildAggregate, CoverageId, GrammarRevision,
        ParseGeneration, SourceRevision, SourceRootId, SourceTransition,
    };
    #[cfg(feature = "exact-parser")]
    use flark_comrak_value_block_core::{
        DirectBlockKind, DirectDurableGrammarCapture, DirectPollStatus, DirectValueBlockParser,
        SyntaxProfile,
    };

    fn green_builder(ticket: &crate::ArenaBuildTicket) -> ResumableSerializedGreenBuild {
        green_builder_with_source(ticket, 1, 1)
    }

    fn green_builder_with_source(
        ticket: &crate::ArenaBuildTicket,
        source_bytes: u64,
        source_utf16: u64,
    ) -> ResumableSerializedGreenBuild {
        green_builder_with_epoch(
            ticket,
            source_bytes,
            source_utf16,
            SourceRevision(1),
            SourceRootId(1),
            GrammarRevision(1),
            ParseGeneration(1),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn green_builder_with_epoch(
        ticket: &crate::ArenaBuildTicket,
        source_bytes: u64,
        source_utf16: u64,
        source_revision: SourceRevision,
        source_root: SourceRootId,
        grammar_revision: GrammarRevision,
        parse_generation: ParseGeneration,
    ) -> ResumableSerializedGreenBuild {
        ResumableSerializedGreenBuild::new(
            ticket,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision,
                source_root,
                source_bytes,
                source_utf16,
                grammar_revision,
                parse_generation,
                semantic_epoch: 1,
                known_bytes: 0..source_bytes,
            },
        )
        .unwrap()
    }

    fn offer(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        build.offer_event(session, event).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => break,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("event polling finalized the green manifest")
                }
            }
        }
    }

    fn finish_green(
        mut build: ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) -> SerializedGreenBuildManifest {
        offer(
            &mut build,
            session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer(
            &mut build,
            session,
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer(
            &mut build,
            session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        offer(
            &mut build,
            session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut build,
            session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        build.finish_input(session).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("green finalization returned to input")
                }
            }
        }
        build.take_manifest().unwrap()
    }

    #[cfg(feature = "exact-parser")]
    fn finish_lf_restart_green(
        mut build: ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        heading: Option<GreenHeadingOpenFacts>,
    ) -> SerializedGreenBuildManifest {
        offer(
            &mut build,
            session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let (kind, facts) = heading.map_or_else(
            || (GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            |facts| (GreenKind::HEADING, facts.into_envelope()),
        );
        offer(
            &mut build,
            session,
            GreenEvent::enter(BlockId(2), kind, facts),
        );
        offer(
            &mut build,
            session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        // The persisted A/P checkpoint is a physical leaf boundary. The
        // retained Setext inverse can therefore retain an exact prefix and
        // rewrite only the bounded leaf containing the finalized Enter.
        build.begin_leaf_barrier(session).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => break,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("LF checkpoint barrier finalized the green manifest")
                }
            }
        }
        let checkpoint_cut = build.take_leaf_barrier_cut(session).unwrap();
        assert_eq!(
            checkpoint_cut.source_before(),
            crate::SerializedMetric { bytes: 1, utf16: 1 }
        );
        offer(
            &mut build,
            session,
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(2), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
            ),
        );
        offer(
            &mut build,
            session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut build,
            session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        build.finish_input(session).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("LF fixture finalization returned to input")
                }
            }
        }
        build.take_manifest().unwrap()
    }

    fn build_index(session: &mut ArenaBuildSession<'_>) -> StorageOnlyCheckpointIndexBuildManifest {
        build_index_with_measure(session, RelativeCheckpointMeasure::new(1, 1, 1, 5, 1))
    }

    fn build_index_with_measure(
        session: &mut ArenaBuildSession<'_>,
        measure: RelativeCheckpointMeasure,
    ) -> StorageOnlyCheckpointIndexBuildManifest {
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::direct(measure))
            .unwrap();
        builder.build_in_session(session).unwrap()
    }

    #[cfg(feature = "exact-parser")]
    fn durable_capture_after(lines: &[&str]) -> DirectDurableGrammarCapture {
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        assert!(parser.pending_command().is_some());
        parser.acknowledge_command().unwrap();
        for line in lines {
            parser.begin_line((*line).to_owned()).unwrap();
            let limit = line.len().saturating_mul(8).saturating_add(256);
            let mut complete = false;
            for _ in 0..limit {
                match parser.poll_line(1).unwrap().status {
                    DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                    DirectPollStatus::Pending => {}
                    DirectPollStatus::ExternalWorkReady => {
                        panic!("non-reference donor fixture unexpectedly requested external work")
                    }
                    DirectPollStatus::Complete => {
                        complete = true;
                        break;
                    }
                }
            }
            assert!(complete, "test line converges within its fuel bound");
        }
        parser
            .capture_durable_grammar_line_boundary_checkpoint()
            .unwrap()
    }

    #[cfg(feature = "exact-parser")]
    fn build_donor_index_from_capture(
        session: &mut ArenaBuildSession<'_>,
        capture: DirectDurableGrammarCapture,
    ) -> StorageOnlyCheckpointIndexBuildManifest {
        let sample = DonorCheckpointSampleDraft::try_new(
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 1),
            capture,
        )
        .unwrap();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(sample))
            .unwrap();
        builder.build_in_session(session).unwrap()
    }

    #[cfg(feature = "exact-parser")]
    fn build_donor_index(
        session: &mut ArenaBuildSession<'_>,
    ) -> StorageOnlyCheckpointIndexBuildManifest {
        build_donor_index_from_capture(session, durable_capture_after(&["a"]))
    }

    #[cfg(feature = "exact-parser")]
    fn build_lf_restart_index(
        session: &mut ArenaBuildSession<'_>,
        normalization: Option<(BlockId, u8)>,
    ) -> StorageOnlyCheckpointIndexBuildManifest {
        let sample = DonorCheckpointSampleDraft::try_new(
            RelativeCheckpointMeasure::new(2, 2, 1, 3, 1),
            durable_capture_after(&["a\n"]),
        )
        .unwrap();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        let partition = match normalization {
            None => StorageOnlyCheckpointPartition::donor_direct(sample),
            Some((block, level)) => StorageOnlyCheckpointPartition::donor_normalization_group(
                block.0,
                StorageOnlyNormalizationOutcome::SetextHeading { level },
                vec![sample],
            ),
        };
        builder.push(partition).unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::terminal_tail(
                RelativeCheckpointMeasure::new(0, 0, 0, 3, 1),
            ))
            .unwrap();
        builder.build_in_session(session).unwrap()
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn commit_lf_restart_parent(
        arena: &mut PageArena,
        green_heading_level: Option<u8>,
        normalization: Option<(BlockId, u8)>,
    ) -> RestartCompositeDocument {
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder_with_source(&ticket, 2, 2);
        let mut session = arena.resume_build(ticket).unwrap();
        let checkpoint_index = build_lf_restart_index(&mut session, normalization);
        let heading =
            green_heading_level.map(|level| GreenHeadingOpenFacts::setext(level).unwrap());
        let green = finish_lf_restart_green(green_builder, &mut session, heading);
        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        RestartCompositeDocumentBuilder::join(&mut session, children)
            .unwrap()
            .commit(session)
            .unwrap()
            .0
    }

    #[cfg(feature = "exact-parser")]
    fn commit_restart_v2_document(
        arena: &mut PageArena,
        donor_index: bool,
    ) -> RestartCompositeDocument {
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let checkpoint_index = if donor_index {
            build_donor_index(&mut session)
        } else {
            build_index(&mut session)
        };
        let green = finish_green(green_builder, &mut session);
        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        RestartCompositeDocumentBuilder::join(&mut session, children)
            .unwrap()
            .commit(session)
            .unwrap()
            .0
    }

    #[cfg(feature = "exact-parser")]
    fn commit_restart_v2_document_with_epoch(
        arena: &mut PageArena,
        source_revision: SourceRevision,
        source_root: SourceRootId,
        grammar_revision: GrammarRevision,
        parse_generation: ParseGeneration,
        donor_index: bool,
    ) -> RestartCompositeDocument {
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder_with_epoch(
            &ticket,
            1,
            1,
            source_revision,
            source_root,
            grammar_revision,
            parse_generation,
        );
        let mut session = arena.resume_build(ticket).unwrap();
        let checkpoint_index = if donor_index {
            build_donor_index(&mut session)
        } else {
            build_index(&mut session)
        };
        let green = finish_green(green_builder, &mut session);
        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        RestartCompositeDocumentBuilder::join(&mut session, children)
            .unwrap()
            .commit(session)
            .unwrap()
            .0
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
    }

    fn abort_and_settle(arena: &mut PageArena, build: ArenaBuildId) {
        loop {
            if arena.poll_build_abort(build, 1).unwrap().complete {
                break;
            }
        }
        settle(arena);
    }

    #[test]
    fn two_typed_children_commit_as_one_atomic_fixed_page_root() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();

        // Build the sibling first to prove green finalization no longer
        // depends on being the journal's only owner.
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        assert_eq!(session.live_owners().unwrap(), 2);

        let composite =
            StorageOnlyCompositeDocumentBuilder::join(&mut session, green, checkpoint_index)
                .unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        assert_eq!(composite.build_id(), session.id());
        assert_eq!(composite.receipt().manifest_nodes_allocated, 1);
        let (document, receipt) = composite.commit(session).unwrap();
        let root = document.root_id();
        let (green_child, checkpoint_child) = document.child_ids(&arena).unwrap();

        assert_ne!(green_child, checkpoint_child);
        assert!(arena.contains(green_child));
        assert!(arena.contains(checkpoint_child));
        assert_eq!(arena.payload(root).unwrap().len(), COMPOSITE_MANIFEST_BYTES);
        assert_eq!(arena.packed_child_count(root).unwrap(), COMPOSITE_CHILDREN);
        assert_eq!(receipt.payload_bytes_copied, COMPOSITE_MANIFEST_BYTES);
        assert_eq!(receipt.child_references_added, COMPOSITE_CHILDREN);
        assert_eq!(receipt.edge_bytes_copied, COMPOSITE_CHILDREN * 8);
        assert!(receipt.maximum_page_payload_bytes + receipt.edge_bytes_copied <= ARENA_PAGE_BYTES);

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[test]
    fn cancelling_the_composite_root_reclaims_both_child_graphs() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        let _composite =
            StorageOnlyCompositeDocumentBuilder::join(&mut session, green, checkpoint_index)
                .unwrap();
        let build = session.begin_abort().unwrap();

        abort_and_settle(&mut arena, build);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[test]
    fn forged_checkpoint_child_is_rejected_before_parent_allocation() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let green = finish_green(green_builder, &mut session);
        let build = session.id();
        let (wrong_owner, _) = session.allocate(b"not a checkpoint index", &[]).unwrap();
        let wrong_index =
            StorageOnlyCheckpointIndexBuildManifest::from_unchecked_test_owner(build, wrong_owner);
        let nodes_before = session.arena().metrics().live_nodes;

        assert!(matches!(
            StorageOnlyCompositeDocumentBuilder::join(&mut session, green, wrong_index),
            Err(StorageOnlyCompositeDocumentError::CheckpointIndex(_))
        ));
        assert_eq!(session.arena().metrics().live_nodes, nodes_before);
        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn cross_build_children_are_rejected_and_both_journals_remain_reclaimable() {
        let mut arena = PageArena::new();

        let green_ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&green_ticket);
        let mut green_session = arena.resume_build(green_ticket).unwrap();
        let green = finish_green(green_builder, &mut green_session);
        let green_ticket = green_session.suspend().unwrap();

        let index_ticket = arena.begin_build().unwrap();
        let mut index_session = arena.resume_build(index_ticket).unwrap();
        let checkpoint_index = build_index(&mut index_session);
        let index_ticket = index_session.suspend().unwrap();

        let mut green_session = arena.resume_build(green_ticket).unwrap();
        assert!(matches!(
            StorageOnlyCompositeDocumentBuilder::join(&mut green_session, green, checkpoint_index),
            Err(StorageOnlyCompositeDocumentError::Invalid(
                "composite children and arena session build generations differ"
            ))
        ));
        let green_build = green_session.begin_abort().unwrap();
        let index_build = arena
            .resume_build(index_ticket)
            .unwrap()
            .begin_abort()
            .unwrap();

        abort_and_settle(&mut arena, green_build);
        abort_and_settle(&mut arena, index_build);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_commits_one_parent_and_exposes_only_parent_borrowed_views() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);

        let parent = RestartCompositeDocumentBuilder::join(&mut session, children).unwrap();
        assert_eq!(parent.build_id(), session.id());
        assert_eq!(session.live_owners().unwrap(), 1);
        assert_eq!(parent.receipt().manifest_nodes_allocated, 1);
        let (document, receipt) = parent.commit(session).unwrap();
        assert_eq!(
            receipt.payload_bytes_copied,
            RESTART_COMPOSITE_MANIFEST_BYTES
        );
        assert_eq!(receipt.child_references_added, COMPOSITE_CHILDREN);

        let view = document.view(&arena).unwrap();
        let green = view.green();
        let index = view.checkpoint_index();
        assert_eq!(green.source_root(), SourceRootId(1));
        assert_eq!(green.source_revision(), SourceRevision(1));
        assert_eq!(
            green.source_metric(),
            crate::SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(green.syntax_profile(), 1);
        assert_eq!(green.grammar_revision(), GrammarRevision(1));
        assert_eq!(green.parse_generation(), ParseGeneration(1));
        assert_eq!(green.semantic_epoch(), 1);
        assert_eq!(green.tokens(), 5);
        assert_eq!(green.coverage_count(), 1);
        assert_eq!(
            green.logical_metric(),
            crate::SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(
            index.final_measure(),
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 1)
        );
        assert_eq!(index.physical_lines(), 1);
        assert!(!index.has_terminal_tail());
        assert!(std::ptr::eq(
            std::ptr::from_ref(green.parent()),
            std::ptr::from_ref(&document)
        ));
        assert!(std::ptr::eq(
            std::ptr::from_ref(index.parent()),
            std::ptr::from_ref(&document)
        ));

        let wrong_arena = PageArena::new();
        assert!(matches!(
            document.view(&wrong_arena),
            Err(RestartCompositeDocumentError::Arena(
                ArenaError::WrongArena { .. }
            ))
        ));
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_parent_borrow_is_the_donor_lookup_and_resume_gateway() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, true);

        assert!(
            document
                .locate_donor_checkpoint_at_or_before_cut(&arena, 0)
                .unwrap()
                .is_none()
        );
        let checkpoint = document
            .locate_donor_checkpoint_at_or_before_cut(&arena, 1)
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint.ordinal(), 0);
        assert_eq!(checkpoint.prefix(), RelativeCheckpointMeasure::default());
        assert_eq!(
            checkpoint.interval(),
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 1)
        );
        assert_eq!(checkpoint.checkpoint_cut(), checkpoint.interval());
        assert_eq!(checkpoint.receipt().retained_source_bytes, 0);
        drop(checkpoint);

        let wrong_arena = PageArena::new();
        assert!(matches!(
            document.locate_donor_checkpoint_at_or_before_cut(&wrong_arena, 1),
            Err(RestartCompositeDocumentError::Arena(
                ArenaError::WrongArena { .. }
            ))
        ));

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn publication_preflight_returns_the_exact_owning_document_on_wrong_arena() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, false);
        let mut wrong_arena = PageArena::new();

        let failure = document
            .prepare_publication(&wrong_arena)
            .expect_err("foreign arena must not consume the committed owner");
        assert!(matches!(
            failure.error,
            RestartCompositeDocumentError::Arena(ArenaError::WrongArena { .. })
        ));
        assert_eq!(failure.document.view(&arena).unwrap().green().tokens(), 5);

        let prepared = failure.document.prepare_publication(&arena).unwrap();
        let candidate_root = prepared.descriptor.arena_root();
        let release_failure = prepared
            .release_later(&mut wrong_arena)
            .expect_err("wrong-arena retirement must return the exact prepared bundle");
        assert!(matches!(
            release_failure.error,
            RestartCompositeDocumentError::Arena(ArenaError::WrongArena { .. })
        ));
        assert_eq!(
            release_failure.publication.descriptor.arena_root(),
            candidate_root
        );
        release_failure
            .publication
            .release_later(&mut arena)
            .unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn atomic_publication_recovers_the_bundle_and_binds_manifest_grammar_and_root() {
        let mut arena = PageArena::new();
        let initial = arena.allocate(b"bootstrap", &[]).unwrap().owner;
        let initial_root = initial.scoped_id();
        let document = commit_restart_v2_document_with_epoch(
            &mut arena,
            SourceRevision(1),
            SourceRootId(9),
            GrammarRevision(77),
            ParseGeneration(1),
            true,
        );
        let mut coordinator = Coordinator::new(SourceRootId(8), initial);
        let token = coordinator
            .accept_source_transition(SourceTransition {
                base_revision: SourceRevision(0),
                target_revision: SourceRevision(1),
                base_root: SourceRootId(8),
                result_root: SourceRootId(9),
            })
            .unwrap()
            .active
            .token;
        let prepared = document.prepare_publication(&arena).unwrap();
        let candidate_root = prepared.descriptor.arena_root();
        assert_eq!(prepared.descriptor.grammar_revision(), GrammarRevision(77));
        let wrong_token = ParseToken {
            source_root: SourceRootId(10),
            ..token
        };

        let failure = prepared
            .publish(&mut coordinator, wrong_token, &mut arena)
            .expect_err("manifest/token mismatch must preserve publication authority");
        assert!(matches!(
            failure.error,
            RestartCompositeDocumentError::Invalid(
                "restart publication manifest and parse token differ"
            )
        ));
        assert_eq!(failure.publication.descriptor.arena_root(), candidate_root);
        assert_eq!(
            failure.publication.descriptor.grammar_revision(),
            GrammarRevision(77)
        );
        assert_eq!(coordinator.current_output().arena_root, initial_root);

        // A coordinator-level rejection also returns the same opaque bundle;
        // storage preflight has passed, but ownership cannot cross into a
        // coordinator bound to another arena namespace.
        let mut foreign_arena = PageArena::new();
        let foreign_initial = foreign_arena.allocate(b"foreign", &[]).unwrap().owner;
        let mut foreign_coordinator = Coordinator::new(SourceRootId(8), foreign_initial);
        let foreign_token = foreign_coordinator
            .accept_source_transition(SourceTransition {
                base_revision: SourceRevision(0),
                target_revision: SourceRevision(1),
                base_root: SourceRootId(8),
                result_root: SourceRootId(9),
            })
            .unwrap()
            .active
            .token;
        assert_eq!(foreign_token, token);
        let failure = failure
            .publication
            .publish(&mut foreign_coordinator, token, &mut arena)
            .expect_err("wrong coordinator arena must return the intact bundle");
        assert!(matches!(
            failure.error,
            RestartCompositeDocumentError::Coordinator(CoordinatorError::Arena(
                ArenaError::WrongArena { .. }
            ))
        ));
        assert_eq!(failure.publication.descriptor.arena_root(), candidate_root);

        let receipt = failure
            .publication
            .publish(&mut coordinator, token, &mut arena)
            .expect("the exact recovered publication bundle remains usable");
        let delta = receipt.delta();
        assert_eq!(delta.offered_output.arena_root, candidate_root);
        assert_eq!(delta.offered_output.grammar_revision, GrammarRevision(77));
        assert_ne!(
            delta.offered_output.grammar_revision,
            GrammarRevision(token.source_revision.0)
        );
        let binding = receipt.into_binding();
        let view = binding.view(&coordinator, &arena).unwrap();
        assert_eq!(view.green().grammar_revision(), GrammarRevision(77));
        assert_eq!(view.green().source_root(), SourceRootId(9));
        assert_eq!(
            view.checkpoint_index().final_measure(),
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 1)
        );

        // Even with an otherwise exact lease, a free grammar guess is not a
        // valid binding to the coordinator-owned root.
        let wrong_grammar = PublishedRestartCompositeHandle {
            lease: OutputRootLease {
                grammar_revision: GrammarRevision(1),
                ..binding.output_lease()
            },
            descriptor: binding.descriptor,
        };
        assert!(matches!(
            wrong_grammar.view(&coordinator, &arena),
            Err(RestartCompositeDocumentError::Coordinator(
                CoordinatorError::LeaseMismatch(_)
            ))
        ));

        // Conversely, the exact coordinator lease cannot authorize a
        // descriptor copied onto another live arena root.
        let mut wrong_descriptor = binding.descriptor;
        wrong_descriptor.root = initial_root;
        let wrong_root = PublishedRestartCompositeHandle {
            lease: binding.output_lease(),
            descriptor: wrong_descriptor,
        };
        assert!(matches!(
            wrong_root.view(&coordinator, &arena),
            Err(RestartCompositeDocumentError::Invalid(
                "published restart binding and coordinator root differ"
            ))
        ));

        // The next exact generation retires this unacknowledged worker root.
        // A copied non-owning binding is inert even if arena reclamation is
        // still waiting in the bounded release queue.
        let next_document = commit_restart_v2_document_with_epoch(
            &mut arena,
            SourceRevision(2),
            SourceRootId(10),
            GrammarRevision(78),
            ParseGeneration(2),
            false,
        );
        let next_token = coordinator
            .accept_source_transition(SourceTransition {
                base_revision: SourceRevision(1),
                target_revision: SourceRevision(2),
                base_root: SourceRootId(9),
                result_root: SourceRootId(10),
            })
            .unwrap()
            .active
            .token;
        next_document
            .prepare_publication(&arena)
            .unwrap()
            .publish(&mut coordinator, next_token, &mut arena)
            .unwrap();
        assert!(matches!(
            binding.view(&coordinator, &arena),
            Err(RestartCompositeDocumentError::Coordinator(
                CoordinatorError::UnknownRoot(_) | CoordinatorError::RootNotWorkerCurrent(_)
            ))
        ));
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn published_parent_drives_donor_selection_and_branded_child_retention() {
        let mut arena = PageArena::new();
        let initial = arena.allocate(b"bootstrap", &[]).unwrap().owner;
        let document = commit_lf_restart_parent(&mut arena, None, None);
        let mut coordinator = Coordinator::new(SourceRootId(8), initial);
        let token = coordinator
            .accept_source_transition(SourceTransition {
                base_revision: SourceRevision(0),
                target_revision: SourceRevision(1),
                base_root: SourceRootId(8),
                result_root: SourceRootId(1),
            })
            .unwrap()
            .active
            .token;
        let binding = document
            .prepare_publication(&arena)
            .unwrap()
            .publish(&mut coordinator, token, &mut arena)
            .unwrap()
            .into_binding();

        let selected = binding
            .locate_donor_checkpoint_at_or_before_cut(&coordinator, &arena, 2)
            .unwrap()
            .unwrap();
        assert_eq!(selected.ordinal(), 0);
        assert_eq!(
            selected.checkpoint_cut(),
            RelativeCheckpointMeasure::new(2, 2, 1, 3, 1)
        );
        assert_eq!(selected.receipt().retained_source_bytes, 0);
        let mint = selected.into_source_ledger_restart_mint().unwrap();
        let (source, cut, path, _kind, donor, selection) = mint.into_source_ledger_parts();
        assert_eq!(source.root, SourceRootId(1));
        assert_eq!(cut, RelativeCheckpointMeasure::new(2, 2, 1, 3, 1));
        assert_eq!(path.event_cut(), cut.green_events());
        assert_eq!(donor.checkpoint_cut_for_test(), cut);

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let retained = binding
            .retain_children_for_adoption(&coordinator, &mut session)
            .unwrap();
        assert_eq!(session.live_owners().unwrap(), COMPOSITE_CHILDREN);
        let branded = retained.join_parent_selection(selection).unwrap();
        branded.validate_session(&session).unwrap();
        let build = branded.cancel(session).unwrap();
        abort_and_settle(&mut arena, build);

        // Cancelling the candidate journal does not disturb the published
        // coordinator parent or its read-only descriptor.
        assert_eq!(
            binding.view(&coordinator, &arena).unwrap().green().tokens(),
            6
        );
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn source_ledger_mint_keeps_direct_path_and_exact_selected_donor_joined() {
        let mut arena = PageArena::new();
        let document = commit_lf_restart_parent(&mut arena, None, None);
        let selected = document
            .locate_donor_checkpoint_at_or_before_cut(&arena, 2)
            .unwrap()
            .unwrap();
        let mint = selected.into_source_ledger_restart_mint().unwrap();
        let (source, cut, path, _kind, donor, _selection) = mint.into_source_ledger_parts();

        assert_eq!(source.bytes, 2);
        assert_eq!(cut, RelativeCheckpointMeasure::new(2, 2, 1, 3, 1));
        assert_eq!(path.event_cut(), 3);
        assert_eq!(
            path.source_metric(),
            crate::SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(path.frames().len(), 2);
        let terminal = path.frames().last().unwrap();
        assert_eq!(terminal.block(), BlockId(2));
        assert_eq!(terminal.green_kind(), GreenKind::PARAGRAPH);
        assert!(matches!(terminal.donor().kind, DirectBlockKind::Paragraph));
        assert!(terminal.normalization().is_none());
        assert_eq!(donor.checkpoint_cut_for_test(), cut);
        donor.decode_grammar_parts().unwrap();

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn source_ledger_mint_applies_only_exact_parent_bound_setext_inverse() {
        let mut arena = PageArena::new();
        let document = commit_lf_restart_parent(&mut arena, Some(1), Some((BlockId(2), 1)));
        let selected = document
            .locate_donor_checkpoint_at_or_before_cut(&arena, 2)
            .unwrap()
            .unwrap();
        let (source, cut, path, _kind, donor, _selection) = selected
            .into_source_ledger_restart_mint()
            .unwrap()
            .into_source_ledger_parts();

        assert_eq!(source.bytes, 2);
        assert_eq!(path.event_cut(), cut.green_events());
        let terminal = path.frames().last().unwrap();
        assert_eq!(terminal.block(), BlockId(2));
        assert_eq!(terminal.green_kind(), GreenKind::HEADING);
        assert!(matches!(terminal.donor().kind, DirectBlockKind::Paragraph));
        assert_eq!(
            terminal.normalization().unwrap().role(),
            CurrentRestartNormalizationRole::SetextHeadingToProvisionalParagraph
        );
        assert_eq!(donor.checkpoint_cut_for_test(), cut);
        donor.decode_grammar_parts().unwrap();

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn parent_selected_setext_green_inverse_owns_and_returns_the_branded_parent_lease() {
        let mut arena = PageArena::new();
        let document = commit_lf_restart_parent(&mut arena, Some(1), Some((BlockId(2), 1)));
        let selected = document
            .locate_donor_checkpoint_at_or_before_cut(&arena, 2)
            .unwrap()
            .unwrap();
        let (_source, cut, path, _kind, donor, selection) = selected
            .into_source_ledger_restart_mint()
            .unwrap()
            .into_source_ledger_parts();
        assert_eq!(cut, RelativeCheckpointMeasure::new(2, 2, 1, 3, 1));
        drop(donor);
        let (accepted_source, event_cut, projection_runs, green) =
            path.into_parent_selected_activation_parts().unwrap();
        assert_eq!(
            accepted_source,
            crate::SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(event_cut, 3);
        assert_eq!(projection_runs, 1);
        let inverse = match green {
            ParentSelectedGreenRestartAuthority::Setext(inverse) => inverse,
            ParentSelectedGreenRestartAuthority::Direct(_) => {
                panic!("normalized parent path lost its Setext inverse")
            }
        };

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let lease = document
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        let ticket = session.suspend().unwrap();
        let new_spec = SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(2),
            source_root: SourceRootId(2),
            source_bytes: 1,
            source_utf16: 1,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(2),
            semantic_epoch: 2,
            known_bytes: 0..1,
        };
        let mut restart = ParentSelectedSetextRetainedGreenRestart::try_new(
            &ticket, &arena, lease, inverse, new_spec,
        )
        .unwrap();
        assert_eq!(restart.receipt().canonical_resolution_passes, 1);

        let mut session = arena.resume_build(ticket).unwrap();
        for _ in 0..256 {
            if restart.poll(&mut session).unwrap() == SetextRetainedGreenRestartProgress::Ready {
                break;
            }
        }
        let output = restart.take_output(&session).unwrap();
        assert_eq!(output.build_id_for_test(), session.id());
        assert_eq!(output.source_before_for_test(), accepted_source);
        let receipt = output.receipt_for_test();
        assert_eq!(receipt.canonical_resolution_passes, 1);
        assert!(receipt.retained_leaves > 0);
        assert!(receipt.inverse_leaf_pages_allocated > 0);
        assert!(session.live_owners().unwrap() > COMPOSITE_CHILDREN);
        output.revalidate_parent_for_test(&session).unwrap();
        assert!(matches!(
            output.validate_pristine_parent_for_test(&session),
            Err(RestartCompositeDocumentError::Corrupt(_))
        ));
        drop(output);
        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);

        assert!(document.view(&arena).is_ok());
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn parent_selected_setext_green_inverse_fails_closed_when_actor_parent_is_released() {
        for release_before_admission in [true, false] {
            let mut arena = PageArena::new();
            let document = commit_lf_restart_parent(&mut arena, Some(1), Some((BlockId(2), 1)));
            let selected = document
                .locate_donor_checkpoint_at_or_before_cut(&arena, 2)
                .unwrap()
                .unwrap();
            let (_source, _cut, path, _kind, donor, selection) = selected
                .into_source_ledger_restart_mint()
                .unwrap()
                .into_source_ledger_parts();
            drop(donor);
            let (_accepted_source, _event_cut, _projection_runs, green) =
                path.into_parent_selected_activation_parts().unwrap();
            let inverse = match green {
                ParentSelectedGreenRestartAuthority::Setext(inverse) => inverse,
                ParentSelectedGreenRestartAuthority::Direct(_) => {
                    panic!("normalized parent path lost its Setext inverse")
                }
            };

            let ticket = arena.begin_build().unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            let lease = document
                .retain_children_for_adoption(&mut session)
                .unwrap()
                .join_parent_selection(selection)
                .unwrap();
            let ticket = session.suspend().unwrap();
            let new_spec = SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: SourceRevision(2),
                source_root: SourceRootId(2),
                source_bytes: 1,
                source_utf16: 1,
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(2),
                semantic_epoch: 2,
                known_bytes: 0..1,
            };

            if release_before_admission {
                document.release_later(&mut arena).unwrap();
                settle(&mut arena);
                assert!(matches!(
                    ParentSelectedSetextRetainedGreenRestart::try_new(
                        &ticket, &arena, lease, inverse, new_spec,
                    ),
                    Err(ParentSelectedSetextRetainedGreenRestartError::Parent(
                        RestartCompositeDocumentError::Arena(ArenaError::StaleId(_))
                    ))
                ));
                let build = arena.resume_build(ticket).unwrap().begin_abort().unwrap();
                abort_and_settle(&mut arena, build);
            } else {
                let mut restart = ParentSelectedSetextRetainedGreenRestart::try_new(
                    &ticket, &arena, lease, inverse, new_spec,
                )
                .unwrap();
                document.release_later(&mut arena).unwrap();
                settle(&mut arena);
                let mut session = arena.resume_build(ticket).unwrap();
                assert!(matches!(
                    restart.poll(&mut session),
                    Err(ParentSelectedSetextRetainedGreenRestartError::Parent(
                        RestartCompositeDocumentError::Arena(ArenaError::StaleId(_))
                    ))
                ));
                drop(restart);
                let build = session.begin_abort().unwrap();
                abort_and_settle(&mut arena, build);
            }
            assert_eq!(arena.metrics().live_nodes, 0);
            assert_eq!(arena.metrics().live_storage_bytes, 0);
        }
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn source_ledger_mint_rejects_crossed_setext_block_and_level() {
        for normalization in [(BlockId(3), 1), (BlockId(2), 2)] {
            let mut arena = PageArena::new();
            let document = commit_lf_restart_parent(&mut arena, Some(1), Some(normalization));
            let selected = document
                .locate_donor_checkpoint_at_or_before_cut(&arena, 2)
                .unwrap()
                .unwrap();
            assert!(matches!(
                selected.into_source_ledger_restart_mint(),
                Err(RestartParentDonorResumeError::CurrentPath(
                    CurrentRestartPathError::NormalizationMismatch(_)
                ))
            ));
            document.release_later(&mut arena).unwrap();
            settle(&mut arena);
        }
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_adoption_lease_retains_exactly_two_children_and_parent_survives_abort() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, true);
        let selection = document.parent_selection_stamp_for_test(&arena).unwrap();
        assert!(
            document
                .locate_donor_checkpoint_at_or_before_cut(&arena, 1)
                .unwrap()
                .is_some()
        );

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let lease = document
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        assert_eq!(lease.build_id(), session.id());
        assert_eq!(session.live_owners().unwrap(), 2);
        assert_eq!(
            session
                .arena()
                .build_journal_metrics(session.id())
                .unwrap()
                .maximum_live_owners,
            2
        );
        assert_eq!(lease.source_root(), SourceRootId(1));
        assert_eq!(lease.source_revision(), SourceRevision(1));
        assert_eq!(
            lease.source_metric(),
            crate::SerializedMetric { bytes: 1, utf16: 1 }
        );
        assert_eq!(
            lease.final_checkpoint_measure(),
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 1)
        );
        assert_eq!(
            lease.lease.parent_activation.root,
            document
                .owner
                .as_ref()
                .expect("live test parent owns its root")
                .scoped_id()
        );
        lease.validate_session(&session).unwrap();
        let retained_index = lease.checkpoint_index_for_splice(&session).unwrap();
        assert_eq!(retained_index.build_id(), session.id());
        retained_index.validate_session(&session).unwrap();
        drop(retained_index);
        let retained_green = lease.green_for_adoption(&session).unwrap();
        assert_eq!(retained_green.build_id(), session.id());
        retained_green.validate_session(&session).unwrap();
        drop(retained_green);

        // Suspending the fresh build proves the old parent remains a fully
        // queryable owner while both additional child references are retained.
        let ticket = session.suspend().unwrap();
        assert_eq!(document.view(&arena).unwrap().green().tokens(), 5);
        assert!(
            document
                .locate_donor_checkpoint_at_or_before_cut(&arena, 1)
                .unwrap()
                .is_some()
        );
        let session = arena.resume_build(ticket).unwrap();
        lease.validate_session(&session).unwrap();
        let build = lease.cancel(session).unwrap();
        abort_and_settle(&mut arena, build);

        // Cancelling the adoption journal releases only its two retained
        // references. The old parent and both of its child edges remain live.
        assert_eq!(
            document
                .view(&arena)
                .unwrap()
                .checkpoint_index()
                .final_measure(),
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 1)
        );
        assert!(
            document
                .locate_donor_checkpoint_at_or_before_cut(&arena, 1)
                .unwrap()
                .is_some()
        );
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_adoption_rejects_wrong_arena_build_and_nonempty_journal() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, false);
        let selection = document.parent_selection_stamp_for_test(&arena).unwrap();

        let mut wrong_arena = PageArena::new();
        let wrong_ticket = wrong_arena.begin_build().unwrap();
        let mut wrong_session = wrong_arena.resume_build(wrong_ticket).unwrap();
        assert!(matches!(
            document.retain_children_for_adoption(&mut wrong_session),
            Err(RestartCompositeAdoptionRetentionFailure::Pristine(
                RestartCompositeDocumentError::Arena(ArenaError::WrongArena { .. })
            ))
        ));
        assert_eq!(wrong_session.live_owners().unwrap(), 0);
        let wrong_build = wrong_session.begin_abort().unwrap();
        abort_and_settle(&mut wrong_arena, wrong_build);

        let occupied_ticket = arena.begin_build().unwrap();
        let mut occupied = arena.resume_build(occupied_ticket).unwrap();
        let (_filler, _) = occupied.allocate(b"unrelated owner", &[]).unwrap();
        assert!(matches!(
            document.retain_children_for_adoption(&mut occupied),
            Err(RestartCompositeAdoptionRetentionFailure::Mutated {
                error: RestartCompositeDocumentError::Invalid(
                    "restart adoption requires a fresh empty build journal"
                ),
                cleanup_error: None,
            })
        ));
        assert_eq!(occupied.live_owners().unwrap(), 1);
        let occupied_build = occupied.begin_abort().unwrap();
        abort_and_settle(&mut arena, occupied_build);

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let lease = document
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        let retained_index = lease.checkpoint_index_for_splice(&session).unwrap();
        let retained_green = lease.green_for_adoption(&session).unwrap();
        let ticket = session.suspend().unwrap();

        let foreign_ticket = arena.begin_build().unwrap();
        let foreign = arena.resume_build(foreign_ticket).unwrap();
        assert!(matches!(
            lease.validate_session(&foreign),
            Err(RestartCompositeDocumentError::Invalid(
                "restart adoption lease and build generations differ"
            ))
        ));
        assert!(matches!(
            retained_index.validate_session(&foreign),
            Err(CommittedCheckpointIndexError::Invalid(
                "parent-retained checkpoint lease and arena build differ"
            ))
        ));
        assert!(matches!(
            retained_green.validate_session(&foreign),
            Err(SerializedGreenError::Invalid(
                "parent-retained green lease and arena build differ"
            ))
        ));
        let foreign_build = foreign.begin_abort().unwrap();
        abort_and_settle(&mut arena, foreign_build);

        let session = arena.resume_build(ticket).unwrap();
        lease.validate_session(&session).unwrap();
        retained_index.validate_session(&session).unwrap();
        retained_green.validate_session(&session).unwrap();
        drop(retained_index);
        drop(retained_green);
        let build = lease.cancel(session).unwrap();
        abort_and_settle(&mut arena, build);

        assert!(document.view(&arena).is_ok());
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn first_adoption_retain_failure_is_pristine_only_after_empty_journal_recertification() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, false);
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();

        let failure = document
            .retain_children_for_adoption_with_test_fault(
                &mut session,
                RestartCompositeAdoptionRetentionTestFault::FirstRetain,
            )
            .unwrap_err();
        assert!(matches!(
            failure,
            RestartCompositeAdoptionRetentionFailure::Pristine(
                RestartCompositeDocumentError::ArenaBuild(ArenaBuildError::Invariant(
                    "injected first restart-adoption retain failure"
                ))
            )
        ));
        assert_eq!(session.live_owners().unwrap(), 0);

        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn second_adoption_retain_failure_is_pristine_only_after_empty_journal_certification() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, false);
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();

        let failure = document
            .retain_children_for_adoption_with_test_fault(
                &mut session,
                RestartCompositeAdoptionRetentionTestFault::SecondRetain,
            )
            .unwrap_err();
        assert!(matches!(
            failure,
            RestartCompositeAdoptionRetentionFailure::Pristine(
                RestartCompositeDocumentError::ArenaBuild(ArenaBuildError::Invariant(
                    "injected second restart-adoption retain failure"
                ))
            )
        ));
        assert_eq!(session.live_owners().unwrap(), 0);

        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn post_retain_validation_failure_is_mutated_even_after_successful_cleanup() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, false);
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();

        let failure = document
            .retain_children_for_adoption_with_test_fault(
                &mut session,
                RestartCompositeAdoptionRetentionTestFault::PostRetainValidation,
            )
            .unwrap_err();
        assert!(matches!(
            failure,
            RestartCompositeAdoptionRetentionFailure::Mutated {
                error: RestartCompositeDocumentError::Corrupt(
                    "injected post-retain restart-adoption validation failure"
                ),
                cleanup_error: None,
            }
        ));
        assert_eq!(session.live_owners().unwrap(), 0);

        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn adoption_cleanup_failure_is_mutated_and_remains_abort_recoverable() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, false);
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();

        let failure = document
            .retain_children_for_adoption_with_test_fault(
                &mut session,
                RestartCompositeAdoptionRetentionTestFault::Cleanup,
            )
            .unwrap_err();
        assert!(matches!(
            failure,
            RestartCompositeAdoptionRetentionFailure::Mutated {
                error: RestartCompositeDocumentError::Corrupt(
                    "injected post-retain restart-adoption validation failure"
                ),
                cleanup_error: Some(RestartCompositeDocumentError::ArenaBuild(
                    ArenaBuildError::Invariant("injected restart-adoption cleanup failure")
                )),
            }
        ));
        assert_eq!(session.live_owners().unwrap(), 1);

        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn owned_adoption_lease_fails_closed_after_parent_actor_root_is_released() {
        let mut arena = PageArena::new();
        let document = commit_restart_v2_document(&mut arena, true);
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let lease = document.retain_children_for_adoption(&mut session).unwrap();

        // No Rust borrow ties the movable lease to the actor document. Its
        // private scoped stamp instead requires that exact parent root to stay
        // live at every operation boundary.
        let ticket = session.suspend().unwrap();
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        let session = arena.resume_build(ticket).unwrap();
        assert!(matches!(
            lease.validate_session(&session),
            Err(RestartCompositeDocumentError::Arena(ArenaError::StaleId(_)))
        ));

        drop(lease);
        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn equal_cut_selection_from_another_parent_cannot_brand_adoption() {
        let mut arena = PageArena::new();
        let retained_parent = commit_restart_v2_document(&mut arena, true);
        let crossed_parent = commit_restart_v2_document(&mut arena, true);
        assert_eq!(
            retained_parent
                .view(&arena)
                .unwrap()
                .checkpoint_index()
                .final_measure(),
            crossed_parent
                .view(&arena)
                .unwrap()
                .checkpoint_index()
                .final_measure()
        );
        let crossed_selection = crossed_parent
            .parent_selection_stamp_for_test(&arena)
            .unwrap();

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let lease = retained_parent
            .retain_children_for_adoption(&mut session)
            .unwrap();
        let failure = lease.join_parent_selection(crossed_selection).unwrap_err();
        assert!(matches!(
            failure.error,
            RestartCompositeDocumentError::Invalid(
                "restart selection and retained parent activation differ"
            )
        ));
        let build = failure.lease.cancel(session).unwrap();
        abort_and_settle(&mut arena, build);

        retained_parent.release_later(&mut arena).unwrap();
        crossed_parent.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_rejects_a_pre_final_or_forged_index_before_parent_allocation() {
        for forged in [
            RelativeCheckpointMeasure::new(1, 1, 1, 4, 1),
            RelativeCheckpointMeasure::new(1, 1, 1, 5, 2),
            RelativeCheckpointMeasure::new(2, 2, 1, 5, 1),
        ] {
            let mut arena = PageArena::new();
            let ticket = arena.begin_build().unwrap();
            let green_builder = green_builder(&ticket);
            let mut session = arena.resume_build(ticket).unwrap();
            let checkpoint_index = build_index_with_measure(&mut session, forged);
            let green = finish_green(green_builder, &mut session);
            let children =
                RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
            let nodes_before = session.arena().metrics().live_nodes;
            assert!(matches!(
                RestartCompositeDocumentBuilder::join(&mut session, children),
                Err(RestartCompositeDocumentError::Corrupt(
                    "restart source, generation, green, and checkpoint totals disagree"
                ))
            ));
            assert_eq!(session.arena().metrics().live_nodes, nodes_before);
            let build = session.begin_abort().unwrap();
            abort_and_settle(&mut arena, build);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_read_rejects_forged_totals_swapped_roles_and_stale_child_generation() {
        for forge_case in 0..3 {
            let mut arena = PageArena::new();
            let ticket = arena.begin_build().unwrap();
            let green_builder = green_builder(&ticket);
            let mut session = arena.resume_build(ticket).unwrap();
            let checkpoint_index = build_index(&mut session);
            let green = finish_green(green_builder, &mut session);
            let green_descriptor = green.composite_descriptor(&session).unwrap();
            let checkpoint_descriptor = checkpoint_index.composite_descriptor(&session).unwrap();
            let green_id = green.validate_composite_child(&session).unwrap();
            let checkpoint_id = checkpoint_index.validate_composite_child(&session).unwrap();
            let mut payload = encode_restart_composite_manifest(
                green_id,
                checkpoint_id,
                green_descriptor,
                checkpoint_descriptor,
            );
            match forge_case {
                0 => put_u64(&mut payload, 184, green_descriptor.tokens() + 1),
                1 => {}
                2 => {
                    let stale_generation = ArenaId {
                        generation: green_id.generation.checked_add(1).unwrap(),
                        ..green_id
                    };
                    encode_arena_id(&mut payload[8..16], stale_generation);
                }
                _ => unreachable!(),
            }
            let children = if forge_case == 1 {
                [checkpoint_id, green_id]
            } else {
                [green_id, checkpoint_id]
            };
            let (root, _) = session.allocate_packed(&payload, &children).unwrap();
            let (green_owner, _) = green.into_composite_parts();
            let (checkpoint_owner, _) = checkpoint_index.into_composite_parts();
            session.release(green_owner).unwrap();
            session.release(checkpoint_owner).unwrap();
            let owner = session.commit(root).unwrap();
            let document = RestartCompositeDocument { owner: Some(owner) };

            assert!(matches!(
                document.view(&arena),
                Err(RestartCompositeDocumentError::Corrupt(_))
            ));
            let adoption_ticket = arena.begin_build().unwrap();
            let mut adoption = arena.resume_build(adoption_ticket).unwrap();
            assert!(matches!(
                document.retain_children_for_adoption(&mut adoption),
                Err(RestartCompositeAdoptionRetentionFailure::Pristine(
                    RestartCompositeDocumentError::Corrupt(_)
                ))
            ));
            assert_eq!(adoption.live_owners().unwrap(), 0);
            let adoption_build = adoption.begin_abort().unwrap();
            abort_and_settle(&mut arena, adoption_build);
            document.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_replacement_adopts_four_child_owners_as_one_new_parent() {
        let mut arena = PageArena::new();
        let old_parent = commit_restart_v2_document(&mut arena, false);
        let selection = old_parent.parent_selection_stamp_for_test(&arena).unwrap();

        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let adoption = old_parent
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        assert_eq!(
            session.live_owners().unwrap(),
            RESTART_REPLACEMENT_CHILD_OWNERS
        );

        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        let replacement = ParentSelectedRestartCompositeReplacement::from_independent_test_parts(
            adoption, children,
        );
        let parent =
            RestartCompositeDocumentBuilder::join_adopted_candidate(&mut session, replacement)
                .unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        let (new_parent, receipt) = parent.commit(session).unwrap();
        assert_eq!(receipt.child_references_added, COMPOSITE_CHILDREN);

        // The candidate transaction created a sibling persistent parent. It
        // neither mutated nor invalidated the actor's still-published parent.
        assert_eq!(old_parent.view(&arena).unwrap().green().tokens(), 5);
        assert_eq!(new_parent.view(&arena).unwrap().green().tokens(), 5);
        old_parent.release_later(&mut arena).unwrap();
        new_parent.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_replacement_returns_intact_bundle_before_parent_allocation() {
        let mut arena = PageArena::new();
        let old_parent = commit_restart_v2_document(&mut arena, false);
        let selection = old_parent.parent_selection_stamp_for_test(&arena).unwrap();

        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let adoption = old_parent
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        let (unrelated, _) = session.allocate(b"unrelated scratch owner", &[]).unwrap();
        assert_eq!(
            session.live_owners().unwrap(),
            RESTART_REPLACEMENT_CHILD_OWNERS + 1
        );
        let nodes_before = session.arena().metrics().live_nodes;

        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        let replacement = ParentSelectedRestartCompositeReplacement::from_independent_test_parts(
            adoption, children,
        );
        let failure =
            RestartCompositeDocumentBuilder::join_adopted_candidate(&mut session, replacement)
                .unwrap_err();
        let replacement = match failure {
            RestartCompositeReplacementJoinFailure::Retryable { error, replacement } => {
                assert!(matches!(
                    error,
                    RestartCompositeDocumentError::Invalid(
                        "restart replacement requires exactly two retained and two new child owners"
                    )
                ));
                replacement
            }
            RestartCompositeReplacementJoinFailure::AbortRequired { error, build } => {
                panic!(
                    "pre-allocation rejection unexpectedly required abort for {build:?}: {error:?}"
                )
            }
        };
        assert_eq!(session.arena().metrics().live_nodes, nodes_before);

        session.release(unrelated).unwrap();
        let parent =
            RestartCompositeDocumentBuilder::join_adopted_candidate(&mut session, replacement)
                .unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        let (new_parent, _) = parent.commit(session).unwrap();
        assert!(old_parent.view(&arena).is_ok());
        assert!(new_parent.view(&arena).is_ok());

        old_parent.release_later(&mut arena).unwrap();
        new_parent.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_replacement_allocation_failure_is_pristine_and_retryable() {
        let limits = ArenaLimits::new(128, 4 * 1024 * 1024, 1, 64);
        let mut arena = PageArena::try_with_limits(limits).unwrap();
        let old_parent = commit_restart_v2_document(&mut arena, false);
        let selection = old_parent.parent_selection_stamp_for_test(&arena).unwrap();

        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let adoption = old_parent
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        assert_eq!(
            session.live_owners().unwrap(),
            RESTART_REPLACEMENT_CHILD_OWNERS
        );

        // Occupy every reusable or not-yet-created arena slot with a queued
        // release. Those nodes are outside the build journal, so the exact
        // four replacement capabilities remain the only live build owners.
        let metrics = session.arena().metrics();
        let max_slots = usize::try_from(session.arena().limits().max_slots()).unwrap();
        let saturation_nodes = metrics
            .reusable_slots
            .checked_add(max_slots - metrics.slots)
            .unwrap();
        assert!(saturation_nodes > 0);
        for _ in 0..saturation_nodes {
            let (filler, _) = session
                .allocate(b"replacement-allocation-filler", &[])
                .unwrap();
            session.release(filler).unwrap();
        }
        assert_eq!(
            session.live_owners().unwrap(),
            RESTART_REPLACEMENT_CHILD_OWNERS
        );

        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        let replacement = ParentSelectedRestartCompositeReplacement::from_independent_test_parts(
            adoption, children,
        );
        let nodes_before = session.arena().metrics().live_nodes;
        let failure =
            RestartCompositeDocumentBuilder::join_adopted_candidate(&mut session, replacement)
                .unwrap_err();
        let replacement = match failure {
            RestartCompositeReplacementJoinFailure::Retryable { error, replacement } => {
                assert!(matches!(
                    error,
                    RestartCompositeDocumentError::ArenaBuild(ArenaBuildError::Arena(
                        ArenaError::SlotLimitReached { .. }
                    ))
                ));
                replacement
            }
            RestartCompositeReplacementJoinFailure::AbortRequired { error, build } => {
                panic!("allocation preflight unexpectedly required abort for {build:?}: {error:?}")
            }
        };
        assert_eq!(session.live_owners().unwrap(), 4);
        assert_eq!(session.arena().metrics().live_nodes, nodes_before);

        let ticket = session.suspend().unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(64).unwrap();
        }
        let mut session = arena.resume_build(ticket).unwrap();
        let parent =
            RestartCompositeDocumentBuilder::join_adopted_candidate(&mut session, replacement)
                .unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        let (new_parent, _) = parent.commit(session).unwrap();
        assert!(old_parent.view(&arena).is_ok());
        assert!(new_parent.view(&arena).is_ok());

        old_parent.release_later(&mut arena).unwrap();
        new_parent.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_v2_parent_allocation_failure_leaves_both_children_under_one_abort_journal() {
        let child_slot_count = {
            let mut probe = PageArena::new();
            let ticket = probe.begin_build().unwrap();
            let green_builder = green_builder(&ticket);
            let mut session = probe.resume_build(ticket).unwrap();
            let _checkpoint_index = build_index(&mut session);
            let _green = finish_green(green_builder, &mut session);
            let slots = session.arena().metrics().slots;
            let build = session.begin_abort().unwrap();
            abort_and_settle(&mut probe, build);
            slots
        };
        let limits = ArenaLimits::new(
            u32::try_from(child_slot_count.checked_add(1).unwrap()).unwrap(),
            1024 * 1024,
            1,
            64,
        );
        let mut arena = PageArena::try_with_limits(limits).unwrap();
        let ticket = arena.begin_build().unwrap();
        let green_builder = green_builder(&ticket);
        let mut session = arena.resume_build(ticket).unwrap();
        let checkpoint_index = build_index(&mut session);
        let green = finish_green(green_builder, &mut session);
        let metrics = session.arena().metrics();
        let max_slots = usize::try_from(session.arena().limits().max_slots()).unwrap();
        let saturation_nodes = metrics
            .reusable_slots
            .checked_add(max_slots - metrics.slots)
            .unwrap();
        for _ in 0..saturation_nodes {
            let (filler, _) = session
                .allocate(b"v2-parent-allocation-filler", &[])
                .unwrap();
            session.release(filler).unwrap();
        }
        assert_eq!(session.live_owners().unwrap(), 2);
        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        let nodes_before = session.arena().metrics().live_nodes;
        let failure = RestartCompositeDocumentBuilder::join(&mut session, children)
            .expect_err("saturated arena must reject the v2 parent allocation");
        assert!(
            matches!(
                failure,
                RestartCompositeDocumentError::ArenaBuild(ArenaBuildError::Arena(
                    ArenaError::SlotLimitReached { .. }
                ))
            ),
            "unexpected saturated-parent failure: {failure:?}"
        );
        assert_eq!(session.live_owners().unwrap(), 2);
        assert_eq!(session.arena().metrics().live_nodes, nodes_before);

        let build = session.begin_abort().unwrap();
        abort_and_settle(&mut arena, build);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }
}
