//! Journal-owned current-prefix plus retained-old-suffix green composition.
//!
//! This module deliberately proves only the storage operation. Admission
//! consumes the exact current builder cut and the source-bound old-green tail,
//! revalidates the latter against either the selected parent's retained child
//! or (in tests) one immutable typed document, and never accepts a caller-
//! supplied event or leaf coordinate. When current-prefix output folds differ,
//! the shared suffix-adoption planner repairs exactly the old spanning Exits
//! inside-out before publication. Planning is O(open depth), and only packed
//! leaves containing changed Exits are rewritten.
//!
//! The completed value has no production manifest escape hatch. Publication
//! must remain impossible until the outer actor consumes the independently
//! matched donor-C authority together with this green result and the sibling
//! checkpoint result.

#[allow(clippy::wildcard_imports)]
use super::*;
use crate::persistent_sequence::{ResumableSequenceRetainedRange, ResumableSequenceSplitProgress};

/// Lexical friend token for moving the already source-bound old-green storage
/// authority into this descendant without exposing its packed cut scalars.
pub(super) struct GreenJournalSuffixMint(());

/// Lexical friend token proving host-splice coordinates were minted by this
/// storage join rather than reconstructed by an exporter or caller.
pub(crate) struct HostGreenPrefixSpliceMint(());

/// Private, bounded old-suffix description extracted by `suffix_adoption`.
///
/// The raw leaf cut never crosses the serialized-green module boundary. The
/// committed manifest binding and immutable frame vector are revalidated by
/// admission before any arena mutation begins.
#[derive(Debug)]
pub(super) struct GreenJournalSuffixParts {
    pub(super) binding: SerializedGreenManifestDescriptor,
    pub(super) old_total: SerializedMetric,
    pub(super) old_prefix: SerializedMetric,
    pub(super) suffix: SerializedMetric,
    pub(super) total_coverage_runs: u64,
    pub(super) prefix_coverage_runs: u64,
    pub(super) suffix_coverage_runs: u64,
    pub(super) prefix_leaves: u64,
    pub(super) event_cut: u64,
    pub(super) block_enters_before: u64,
    pub(super) frames: Vec<GreenRestartOutputFrame>,
    pub(super) source_receipt: GreenSourceTailAdoptionReceipt,
}

/// Borrowed sibling used for the one-way-adoption preflight. It exposes the
/// exact same private storage facts as the consuming parts without cloning the
/// O(open-depth) frame vector or surrendering source/composer authority.
pub(super) struct GreenJournalSuffixView<'tail> {
    pub(super) binding: SerializedGreenManifestDescriptor,
    pub(super) old_total: SerializedMetric,
    pub(super) old_prefix: SerializedMetric,
    pub(super) suffix: SerializedMetric,
    pub(super) total_coverage_runs: u64,
    pub(super) prefix_coverage_runs: u64,
    pub(super) suffix_coverage_runs: u64,
    pub(super) prefix_leaves: u64,
    pub(super) event_cut: u64,
    pub(super) block_enters_before: u64,
    pub(super) frames: &'tail [GreenRestartOutputFrame],
    pub(super) source_receipt: GreenSourceTailAdoptionReceipt,
}

impl GreenJournalSuffixParts {
    fn view(&self) -> GreenJournalSuffixView<'_> {
        GreenJournalSuffixView {
            binding: self.binding,
            old_total: self.old_total,
            old_prefix: self.old_prefix,
            suffix: self.suffix,
            total_coverage_runs: self.total_coverage_runs,
            prefix_coverage_runs: self.prefix_coverage_runs,
            suffix_coverage_runs: self.suffix_coverage_runs,
            prefix_leaves: self.prefix_leaves,
            event_cut: self.event_cut,
            block_enters_before: self.block_enters_before,
            frames: &self.frames,
            source_receipt: self.source_receipt,
        }
    }
}

/// Recoverable direct-lane rejection. The exact builder is returned so the
/// parser can continue farther or fall back to a full suffix parse; no guessed
/// green state is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GreenJournalSuffixIneligibleReason {
    GrammarChanged,
    SourceChanged,
    OpenPathChanged,
    SpanningExitRepairRequired,
}

#[must_use = "an ineligible green suffix must resume exact parsing or be cancelled"]
#[derive(Debug)]
pub(crate) struct GreenJournalSuffixFallback {
    builder: ResumableSerializedGreenBuild,
    reason: GreenJournalSuffixIneligibleReason,
}

impl GreenJournalSuffixFallback {
    pub(crate) const fn reason(&self) -> GreenJournalSuffixIneligibleReason {
        self.reason
    }

    pub(crate) fn into_builder(self) -> ResumableSerializedGreenBuild {
        self.builder
    }
}

#[must_use = "green suffix admission must be polled or exact parsing resumed"]
#[derive(Debug)]
pub(crate) enum GreenJournalSuffixAdmission {
    Ready(ResumableGreenJournalSuffixSplice),
    Ineligible(GreenJournalSuffixFallback),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GreenJournalSuffixSpliceProgress {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GreenJournalSuffixPreflight {
    Eligible,
    Ineligible(GreenJournalSuffixIneligibleReason),
}

/// Auditable locality and ownership receipt for the first direct splice lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GreenJournalSuffixSpliceReceipt {
    pub(crate) current_prefix_leaves: u64,
    pub(crate) old_suffix_leaves: u64,
    pub(crate) described_old_suffix_bytes: u64,
    pub(crate) open_frames_compared: usize,
    pub(crate) old_leaf_identity_sentinels_verified: usize,
    pub(crate) spanning_exit_repair: GreenSuffixAdoptionReceipt,
    pub(crate) normalization_polls: usize,
    pub(crate) retained_range_polls: usize,
    pub(crate) splice_polls: usize,
    pub(crate) manifest_allocations: usize,
    pub(crate) retained_source_bytes: usize,
    pub(crate) document_sized_event_vectors: usize,
    /// Source/storage continuity is present, but matched donor-C has not been
    /// consumed by this mechanism and publication therefore remains illegal.
    pub(crate) mechanism_only_unpublishable: bool,
    pub(crate) source_tail: GreenSourceTailAdoptionReceipt,
    pub(crate) build: SerializedGreenBuildReceipt,
}

#[derive(Debug)]
enum GreenJournalSuffixPhase {
    PollRepairPlan,
    BeginNormalize,
    PollNormalize,
    PollRetainedSuffix,
    BeginSplice,
    PollSplice,
    BeginRepairLeaf,
    PollRepairLeaf,
    AllocateManifest,
    Complete,
    Taken,
    Failed,
}

/// One suspended-session-safe green composition job.
///
/// Every poll performs at most one persistent-sequence step or one manifest
/// allocation. The old suffix range and the final splice both use journal-
/// owned roots, so dropping this state before bounded arena abort cannot
/// mutate the committed old document.
#[must_use = "the green suffix splice must be polled, taken, or cancelled with its build"]
#[derive(Debug)]
pub(crate) struct ResumableGreenJournalSuffixSplice {
    build: ArenaBuildId,
    builder: Option<ResumableSerializedGreenBuild>,
    cut: BuilderGreenPrefixSnapshot,
    suffix: GreenJournalSuffixParts,
    old_root: ArenaId,
    retained_suffix: ResumableSequenceRetainedRange<SerializedGreenSpec>,
    old_suffix_leaves: u64,
    first_old_suffix_leaf: ArenaId,
    last_old_suffix_leaf: ArenaId,
    repair_planner: Option<GreenSpanningExitRepairPlanner>,
    repair_rewrites: Vec<PlannedExitRewrite>,
    next_repair_rewrite: usize,
    repaired_root_child: Option<ClosedChildAggregate>,
    expected_summary: Option<GreenSummary>,
    result: Option<MechanismOnlyGreenJournalSuffix>,
    phase: GreenJournalSuffixPhase,
    receipt: GreenJournalSuffixSpliceReceipt,
}

/// Completed green storage result. There is intentionally no non-test method
/// that returns its `SerializedGreenBuildManifest`.
#[must_use = "mechanism-only green output must enter the matched-C rendezvous or be cancelled"]
#[derive(Debug)]
pub(crate) struct MechanismOnlyGreenJournalSuffix {
    manifest: SerializedGreenBuildManifest,
    receipt: GreenJournalSuffixSpliceReceipt,
    #[cfg(feature = "host-mirror-probe")]
    host_splice: crate::host_mirror::GreenSuffixLeafSpliceDraft,
    #[cfg(feature = "host-mirror-probe")]
    host_prefix: Option<crate::host_mirror::CanonicalRetainedGreenPrefixProof>,
}

impl MechanismOnlyGreenJournalSuffix {
    pub(crate) const fn receipt(&self) -> GreenJournalSuffixSpliceReceipt {
        self.receipt
    }

    /// Lexically restricted handoff into the actor's matched-C rendezvous.
    /// No general caller can extract the build manifest from this
    /// mechanism-only result or publish it independently.
    pub(crate) fn into_parent_selected_adoption_manifest(
        self,
        _mint: crate::candidate_writer::ParentSelectedAdoptionSpliceMint,
    ) -> SerializedGreenBuildManifest {
        self.manifest
    }

    /// Host-probe variant of the same lexical handoff. The draft and optional
    /// post-normalization prefix proof remain non-exportable until
    /// CandidateWriter consumes them together at the final matched-C seam.
    #[cfg(feature = "host-mirror-probe")]
    pub(crate) fn into_parent_selected_adoption_parts(
        self,
        _mint: crate::candidate_writer::ParentSelectedAdoptionSpliceMint,
    ) -> (
        SerializedGreenBuildManifest,
        crate::host_mirror::GreenSuffixLeafSpliceDraft,
        Option<crate::host_mirror::CanonicalRetainedGreenPrefixProof>,
    ) {
        (self.manifest, self.host_splice, self.host_prefix)
    }

    #[cfg(all(test, feature = "host-mirror-probe"))]
    pub(crate) fn into_host_mirror_fixture_parts(
        self,
    ) -> (
        SerializedGreenBuildManifest,
        crate::host_mirror::TypedGreenLeafSplice,
    ) {
        (
            self.manifest,
            self.host_splice.into_zero_prefix_fixture_proof(),
        )
    }

    #[cfg(test)]
    fn descriptor_for_test(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SerializedGreenCompositeDescriptor, SerializedGreenError> {
        self.manifest.composite_descriptor(session)
    }

    #[cfg(test)]
    fn leaf_at_for_test(
        &self,
        session: &ArenaBuildSession<'_>,
        leaf_index: u64,
    ) -> Result<Option<ArenaId>, SerializedGreenError> {
        let manifest = session.owner_id(&self.manifest.owner)?;
        let (_, root) = decode_document(session.arena(), manifest)?;
        locate_leaf_in_arena(session.arena(), root, leaf_index)
    }
}

struct ValidatedDirectGreenSuffix {
    old_root: ArenaId,
    old_suffix_leaves: u64,
    first_old_suffix_leaf: ArenaId,
    last_old_suffix_leaf: ArenaId,
    repair_required: bool,
}

fn validate_direct_green_suffix(
    ticket: &ArenaBuildTicket,
    arena: &PageArena,
    builder: &ResumableSerializedGreenBuild,
    cut: &BuilderGreenPrefixSnapshot,
    suffix: &GreenJournalSuffixView<'_>,
    old_manifest: ArenaId,
    old_descriptor: SerializedGreenCompositeDescriptor,
) -> Result<
    Result<ValidatedDirectGreenSuffix, GreenJournalSuffixIneligibleReason>,
    SerializedGreenError,
> {
    if ticket.id() != builder.build_id()
        || ticket.id() != cut.build_id()
        || !builder.builder_green_prefix_snapshot_is_current(cut)
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    let manifest_capability = SerializedGreenManifestId::new(arena.scoped_query_id(old_manifest)?);
    let (manifest, old_root) = decode_document(arena, old_manifest)?;
    if !suffix.binding.matches(manifest_capability, &manifest)
        || old_descriptor.manifest != old_manifest
        || old_descriptor.sequence_root != old_root
        || old_descriptor.summary != manifest.summary
        || suffix.old_total != manifest.summary.metric
        || suffix.old_prefix.checked_add(suffix.suffix)? != suffix.old_total
        || suffix.total_coverage_runs != old_descriptor.coverage_count()
        || suffix
            .prefix_coverage_runs
            .checked_add(suffix.suffix_coverage_runs)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix coverage total",
            ))?
            != suffix.total_coverage_runs
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    if cut.syntax_profile() != old_descriptor.syntax_profile()
        || cut.grammar_revision() != old_descriptor.grammar_revision()
    {
        return Ok(Err(GreenJournalSuffixIneligibleReason::GrammarChanged));
    }
    if cut.source_before().checked_add(suffix.suffix)? != cut.source_total() {
        return Ok(Err(GreenJournalSuffixIneligibleReason::SourceChanged));
    }
    if cut.open_frames().len() != suffix.frames.len() || cut.open_frames().is_empty() {
        return Ok(Err(GreenJournalSuffixIneligibleReason::OpenPathChanged));
    }
    let mut repair_required = false;
    for (current, old) in cut.open_frames().iter().zip(suffix.frames) {
        // Coverage ownership is persisted as relative open depth and Exit has
        // no block ID. A fresh candidate may therefore assign a different
        // identity to the structurally equivalent boundary frame without
        // changing the retained suffix bytes. Source-ledger adoption performs
        // the sibling typed depth rebind before publication.
        if current.kind() != old.kind() || current.facts() != old.facts() {
            return Ok(Err(GreenJournalSuffixIneligibleReason::OpenPathChanged));
        }
        if current.kind() == GreenKind::FENCED_CODE {
            return Ok(Err(
                GreenJournalSuffixIneligibleReason::SpanningExitRepairRequired,
            ));
        }
        repair_required |= current.closed_children() != old.closed_children();
    }

    let open_depth = u64::try_from(suffix.frames.len())
        .map_err(|_| SerializedGreenError::Overflow("green suffix open depth"))?;
    let exits_before =
        suffix
            .block_enters_before
            .checked_sub(open_depth)
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix open depth exceeds prior Enter count",
            ))?;
    let expected_event_cut = suffix
        .prefix_coverage_runs
        .checked_add(suffix.block_enters_before)
        .and_then(|events| events.checked_add(exits_before))
        .ok_or(SerializedGreenError::Overflow(
            "green suffix event-cut decomposition",
        ))?;
    if suffix.prefix_leaves == 0
        || suffix.prefix_leaves >= old_descriptor.leaf_pages()
        || suffix.event_cut != expected_event_cut
    {
        return Err(SerializedGreenError::Corrupt(
            "green suffix authority has an invalid packed boundary",
        ));
    }
    let old_suffix_leaves = old_descriptor
        .leaf_pages()
        .checked_sub(suffix.prefix_leaves)
        .ok_or(SerializedGreenError::Corrupt(
            "green suffix leaf boundary exceeds old root",
        ))?;
    let first_old_suffix_leaf = locate_leaf_in_arena(arena, old_root, suffix.prefix_leaves)?
        .ok_or(SerializedGreenError::Corrupt(
            "green suffix first retained leaf disappeared",
        ))?;
    let last_old_suffix_leaf = locate_leaf_in_arena(
        arena,
        old_root,
        old_descriptor
            .leaf_pages()
            .checked_sub(1)
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix old root is empty",
            ))?,
    )?
    .ok_or(SerializedGreenError::Corrupt(
        "green suffix last retained leaf disappeared",
    ))?;
    Ok(Ok(ValidatedDirectGreenSuffix {
        old_root,
        old_suffix_leaves,
        first_old_suffix_leaf,
        last_old_suffix_leaf,
        repair_required,
    }))
}

/// Re-encodes one old packed leaf after changing only authenticated spanning
/// Exit output folds. The decoded/event scratch is bounded by one arena page;
/// retained projection programs remain shared arena children.
fn allocate_repaired_exit_leaf(
    session: &mut ArenaBuildSession<'_>,
    expected_leaf: ArenaId,
    rewrites: &[PlannedExitRewrite],
    repair_receipt: &mut GreenSuffixAdoptionReceipt,
    build_receipt: &mut SerializedGreenBuildReceipt,
) -> Result<ArenaBuildOwner, SerializedGreenError> {
    if rewrites.is_empty()
        || rewrites
            .iter()
            .any(|rewrite| rewrite.location.leaf != expected_leaf)
    {
        return Err(SerializedGreenError::Corrupt(
            "green suffix repair leaf has inconsistent targets",
        ));
    }
    let payload_bytes = session.arena().payload(expected_leaf)?.len();
    let (old_summary, decoded) = decode_leaf(session.arena(), expected_leaf)?;
    repair_receipt.leaf_pages_decoded =
        repair_receipt
            .leaf_pages_decoded
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix repair decoded leaf receipt",
            ))?;
    repair_receipt.events_decoded = repair_receipt
        .events_decoded
        .checked_add(decoded.len())
        .ok_or(SerializedGreenError::Overflow(
            "green suffix repair decoded event receipt",
        ))?;
    let decoded_bytes = decoded
        .capacity()
        .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
        .and_then(|bytes| bytes.checked_add(payload_bytes))
        .ok_or(SerializedGreenError::Overflow(
            "green suffix repair decoded page receipt",
        ))?;
    repair_receipt.maximum_decoded_page_bytes =
        repair_receipt.maximum_decoded_page_bytes.max(decoded_bytes);
    build_receipt.maximum_decoded_page_buffer_bytes = build_receipt
        .maximum_decoded_page_buffer_bytes
        .max(decoded_bytes);

    let mut events = decoded
        .into_iter()
        .map(|decoded| (decoded.byte_offset, decoded.event))
        .collect::<Vec<_>>();
    repair_receipt.maximum_rewrite_scratch_bytes = repair_receipt
        .maximum_rewrite_scratch_bytes
        .max(events.capacity() * std::mem::size_of::<(u16, DecodedGreenEventKind)>());
    for rewrite in rewrites {
        let (_, event) = events
            .iter_mut()
            .find(|(offset, _)| *offset == rewrite.location.byte_offset)
            .ok_or(SerializedGreenError::StaleCursor)?;
        let DecodedGreenEventKind::Exit {
            closed,
            last_line_blank,
            facts,
        } = event
        else {
            return Err(SerializedGreenError::StaleCursor);
        };
        if *closed != rewrite.location.closed
            || *last_line_blank != rewrite.location.last_line_blank
            || *facts != rewrite.location.facts
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        *closed = rewrite.replacement_closed;
        *facts = rewrite.replacement_facts;
    }

    let mut page = LeafEncoder::default();
    for (_, event) in &events {
        let encoded = encode_decoded_event(session.arena(), event, page.programs.len())?;
        if !page.can_fit(&encoded) {
            return Err(SerializedGreenError::Corrupt(
                "fixed-width Exit repair changed packed leaf count",
            ));
        }
        page.push_decoded(event, encoded)?;
    }
    let (payload, replacement_summary, programs) = page.seal()?;
    if replacement_summary.tokens != old_summary.tokens
        || replacement_summary.blocks != old_summary.blocks
        || replacement_summary.metric != old_summary.metric
        || replacement_summary.logical_metric != old_summary.logical_metric
        || replacement_summary.balance != old_summary.balance
        || replacement_summary.minimum_prefix != old_summary.minimum_prefix
        || replacement_summary.minimum_closed_depth != old_summary.minimum_closed_depth
        || replacement_summary.leaves != old_summary.leaves
    {
        return Err(SerializedGreenError::Corrupt(
            "green suffix Exit repair changed non-output leaf semantics",
        ));
    }

    let mut program_owners = Vec::new();
    program_owners
        .try_reserve_exact(programs.len())
        .map_err(|_| {
            SerializedGreenError::Invalid("green suffix repair program reservation failed")
        })?;
    for program in programs {
        let PendingProjectionProgram::Retained(program) = program else {
            return Err(SerializedGreenError::Corrupt(
                "green suffix repair invented a projection program",
            ));
        };
        program_owners.push(session.retain(program)?);
    }
    let mut program_ids = Vec::new();
    program_ids
        .try_reserve_exact(program_owners.len())
        .map_err(|_| {
            SerializedGreenError::Invalid("green suffix repair program-ID reservation failed")
        })?;
    for owner in &program_owners {
        program_ids.push(session.owner_id(owner)?);
    }
    let encoded_scratch = payload
        .capacity()
        .checked_add(
            program_ids
                .capacity()
                .checked_mul(std::mem::size_of::<ArenaId>())
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix repair encoded scratch",
                ))?,
        )
        .ok_or(SerializedGreenError::Overflow(
            "green suffix repair encoded scratch",
        ))?;
    build_receipt.maximum_encoded_page_buffer_bytes = build_receipt
        .maximum_encoded_page_buffer_bytes
        .max(encoded_scratch);
    repair_receipt.maximum_rewrite_scratch_bytes = repair_receipt
        .maximum_rewrite_scratch_bytes
        .max(encoded_scratch);
    let (leaf, allocation) = session.allocate_packed(&payload, &program_ids)?;
    for owner in program_owners {
        session.release(owner)?;
    }
    build_receipt.leaf_pages_allocated =
        build_receipt
            .leaf_pages_allocated
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix repair leaf allocation count",
            ))?;
    build_receipt.resumable_arena_allocations = build_receipt
        .resumable_arena_allocations
        .checked_add(1)
        .ok_or(SerializedGreenError::Overflow(
            "green suffix repair resumable allocation count",
        ))?;
    build_receipt.payload_bytes_copied = build_receipt
        .payload_bytes_copied
        .checked_add(allocation.payload_bytes_copied)
        .ok_or(SerializedGreenError::Overflow(
            "green suffix repair payload receipt",
        ))?;
    build_receipt.edge_bytes_copied = build_receipt
        .edge_bytes_copied
        .checked_add(allocation.edge_bytes_copied)
        .ok_or(SerializedGreenError::Overflow(
            "green suffix repair edge receipt",
        ))?;
    Ok(leaf)
}

impl ResumableGreenJournalSuffixSplice {
    #[must_use]
    pub(crate) const fn receipt(&self) -> GreenJournalSuffixSpliceReceipt {
        self.receipt
    }

    /// Read-only direct-lane eligibility check run before the source/composer
    /// tail transition becomes one-way. It performs the same full
    /// O(open-depth) admission validation as `begin_from_parent`, allocates no
    /// arena owner, and consumes no authority.
    pub(crate) fn preflight_from_parent(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        builder: &ResumableSerializedGreenBuild,
        cut: &BuilderGreenPrefixSnapshot,
        old_tail: &SourceBoundGreenTailAdoption,
        old_green: &ParentRetainedGreenLease<'_>,
    ) -> Result<GreenJournalSuffixPreflight, SerializedGreenError> {
        let manifest = old_green.validated_suspended_manifest(ticket, arena)?;
        let descriptor = validate_serialized_green_composite_child(arena, manifest)?;
        let suffix = old_tail.green_journal_suffix_view(GreenJournalSuffixMint(()));
        match validate_direct_green_suffix(
            ticket, arena, builder, cut, &suffix, manifest, descriptor,
        )? {
            Ok(_) => Ok(GreenJournalSuffixPreflight::Eligible),
            Err(reason) => Ok(GreenJournalSuffixPreflight::Ineligible(reason)),
        }
    }

    /// Production-shaped admission from the exact parent-retained green child.
    /// The old suffix coordinate comes only from `old_tail`; the caller cannot
    /// provide or forge a leaf range.
    pub(crate) fn begin_from_parent(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        builder: ResumableSerializedGreenBuild,
        cut: BuilderGreenPrefixSnapshot,
        old_tail: GreenSourceTailAdoptionCapability,
        old_green: &ParentRetainedGreenLease<'_>,
    ) -> Result<GreenJournalSuffixAdmission, SerializedGreenError> {
        let manifest = old_green.validated_suspended_manifest(ticket, arena)?;
        let descriptor = validate_serialized_green_composite_child(arena, manifest)?;
        Self::admit(ticket, arena, builder, cut, old_tail, manifest, descriptor)
    }

    /// Unit-only immutable-document adapter. It exercises the identical typed
    /// storage path but cannot be used by the product actor.
    #[cfg(test)]
    pub(crate) fn begin_from_document_for_test(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        builder: ResumableSerializedGreenBuild,
        cut: BuilderGreenPrefixSnapshot,
        old_tail: GreenSourceTailAdoptionCapability,
        old_green: &SerializedGreenDocument,
    ) -> Result<GreenJournalSuffixAdmission, SerializedGreenError> {
        let manifest = old_green.local_manifest_id(arena)?;
        let descriptor = validate_serialized_green_composite_child(arena, manifest)?;
        Self::admit(ticket, arena, builder, cut, old_tail, manifest, descriptor)
    }

    #[allow(clippy::too_many_arguments)]
    fn admit(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        mut builder: ResumableSerializedGreenBuild,
        cut: BuilderGreenPrefixSnapshot,
        old_tail: GreenSourceTailAdoptionCapability,
        old_manifest: ArenaId,
        old_descriptor: SerializedGreenCompositeDescriptor,
    ) -> Result<GreenJournalSuffixAdmission, SerializedGreenError> {
        let suffix = old_tail.into_green_journal_suffix_parts(GreenJournalSuffixMint(()));
        let validation = match validate_direct_green_suffix(
            ticket,
            arena,
            &builder,
            &cut,
            &suffix.view(),
            old_manifest,
            old_descriptor,
        )? {
            Ok(validation) => validation,
            Err(reason) => {
                return Ok(GreenJournalSuffixAdmission::Ineligible(
                    GreenJournalSuffixFallback { builder, reason },
                ));
            }
        };
        let ValidatedDirectGreenSuffix {
            old_root,
            old_suffix_leaves,
            first_old_suffix_leaf,
            last_old_suffix_leaf,
            repair_required,
        } = validation;
        let retained_suffix = ResumableSequenceRetainedRange::<SerializedGreenSpec>::try_new(
            ticket,
            arena,
            old_root,
            suffix.prefix_leaves..old_descriptor.leaf_pages(),
            &mut builder.sequence_receipt,
        )?;
        let source_receipt = suffix.source_receipt;
        let receipt = GreenJournalSuffixSpliceReceipt {
            old_suffix_leaves,
            described_old_suffix_bytes: suffix.suffix.bytes,
            open_frames_compared: suffix.frames.len(),
            retained_source_bytes: source_receipt.retained_source_bytes,
            document_sized_event_vectors: source_receipt.document_sized_event_vectors,
            mechanism_only_unpublishable: true,
            source_tail: source_receipt,
            ..GreenJournalSuffixSpliceReceipt::default()
        };
        if receipt.retained_source_bytes != 0 || receipt.document_sized_event_vectors != 0 {
            return Err(SerializedGreenError::Corrupt(
                "green suffix authority retained forbidden document storage",
            ));
        }
        let repair_planner = if repair_required {
            Some(GreenSpanningExitRepairPlanner::new(
                suffix.frames.len(),
                suffix.event_cut,
                GreenSuffixAdoptionReceipt::default(),
            )?)
        } else {
            None
        };
        Ok(GreenJournalSuffixAdmission::Ready(Self {
            build: ticket.id(),
            builder: Some(builder),
            cut,
            suffix,
            old_root,
            retained_suffix,
            old_suffix_leaves,
            first_old_suffix_leaf,
            last_old_suffix_leaf,
            repair_planner,
            repair_rewrites: Vec::new(),
            next_repair_rewrite: 0,
            repaired_root_child: None,
            expected_summary: None,
            result: None,
            phase: if repair_required {
                GreenJournalSuffixPhase::PollRepairPlan
            } else {
                GreenJournalSuffixPhase::BeginNormalize
            },
            receipt,
        }))
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<GreenJournalSuffixSpliceProgress, SerializedGreenError> {
        if session.id() != self.build {
            return Err(SerializedGreenError::Invalid(
                "green suffix splice and arena session differ",
            ));
        }
        let phase = std::mem::replace(&mut self.phase, GreenJournalSuffixPhase::Failed);
        let result = self.poll_phase(session, phase);
        match result {
            Ok((phase, progress)) => {
                self.phase = phase;
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    fn poll_phase(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        phase: GreenJournalSuffixPhase,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        match phase {
            GreenJournalSuffixPhase::PollRepairPlan => self.poll_repair_plan(session),
            GreenJournalSuffixPhase::BeginNormalize => {
                self.builder_mut()?
                    .begin_working_prefix_reduction(session)?;
                Ok((
                    GreenJournalSuffixPhase::PollNormalize,
                    GreenJournalSuffixSpliceProgress::Pending,
                ))
            }
            GreenJournalSuffixPhase::PollNormalize => self.poll_normalize(session),
            GreenJournalSuffixPhase::PollRetainedSuffix => {
                self.receipt.retained_range_polls =
                    self.receipt.retained_range_polls.checked_add(1).ok_or(
                        SerializedGreenError::Overflow("green retained suffix poll count"),
                    )?;
                let builder = self.builder.as_mut().ok_or(SerializedGreenError::Corrupt(
                    "green suffix splice lost its builder",
                ))?;
                let progress = self
                    .retained_suffix
                    .poll(session, &mut builder.sequence_receipt)?;
                Ok((
                    if progress == ResumableSequenceSplitProgress::Complete {
                        GreenJournalSuffixPhase::BeginSplice
                    } else {
                        GreenJournalSuffixPhase::PollRetainedSuffix
                    },
                    GreenJournalSuffixSpliceProgress::Pending,
                ))
            }
            GreenJournalSuffixPhase::BeginSplice => self.begin_splice(session),
            GreenJournalSuffixPhase::PollSplice => self.poll_splice(session),
            GreenJournalSuffixPhase::BeginRepairLeaf => self.begin_repair_leaf(session),
            GreenJournalSuffixPhase::PollRepairLeaf => self.poll_repair_leaf(session),
            GreenJournalSuffixPhase::AllocateManifest => self.allocate_manifest(session),
            GreenJournalSuffixPhase::Complete => Ok((
                GreenJournalSuffixPhase::Complete,
                GreenJournalSuffixSpliceProgress::Complete,
            )),
            GreenJournalSuffixPhase::Taken => Err(SerializedGreenError::Invalid(
                "green suffix splice output was already taken",
            )),
            GreenJournalSuffixPhase::Failed => Err(SerializedGreenError::Invalid(
                "green suffix splice is poisoned",
            )),
        }
    }

    fn poll_repair_plan(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        let planner = self
            .repair_planner
            .as_mut()
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix repair phase lost its planner",
            ))?;
        let progress = planner.poll(
            session.arena(),
            self.old_root,
            self.suffix.prefix_leaves,
            self.cut.open_frames(),
            &self.suffix.frames,
        )?;
        self.receipt.spanning_exit_repair = *planner.receipt();
        if progress == GreenSuffixAdoptionPlanProgress::Pending {
            return Ok((
                GreenJournalSuffixPhase::PollRepairPlan,
                GreenJournalSuffixSpliceProgress::Pending,
            ));
        }
        let planner = self
            .repair_planner
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix repair planner disappeared at completion",
            ))?;
        let (mut rewrites, repaired_root_child, repair_receipt) = planner.into_parts()?;
        rewrites.sort_by_key(|rewrite| (rewrite.location.leaf_index, rewrite.location.byte_offset));
        if rewrites.windows(2).any(|pair| {
            pair[0].location.leaf_index == pair[1].location.leaf_index
                && pair[0].location.byte_offset == pair[1].location.byte_offset
        }) {
            return Err(SerializedGreenError::Corrupt(
                "green suffix repair planned the same Exit twice",
            ));
        }
        self.receipt.spanning_exit_repair = repair_receipt;
        self.repair_rewrites = rewrites;
        self.repaired_root_child = Some(repaired_root_child);
        Ok((
            GreenJournalSuffixPhase::BeginNormalize,
            GreenJournalSuffixSpliceProgress::Pending,
        ))
    }

    fn poll_normalize(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        self.receipt.normalization_polls = self.receipt.normalization_polls.checked_add(1).ok_or(
            SerializedGreenError::Overflow("green suffix normalization poll count"),
        )?;
        let cut_build = self.cut.build_id();
        let cut_leaves = self.cut.leaves_before();
        let cut_events = self.cut.events_before();
        let cut_source = self.cut.source_before();
        let builder = self.builder_mut()?;
        if builder.phase != SerializedGreenStreamPhase::Accepting {
            if builder.poll(session)? == SerializedGreenStreamProgress::Pending {
                return Ok((
                    GreenJournalSuffixPhase::PollNormalize,
                    GreenJournalSuffixSpliceProgress::Pending,
                ));
            }
        }
        let working = builder.take_working_prefix_cut(session)?;
        if working.build != cut_build
            || working.installed_leaves_before() != cut_leaves
            || working.events_before() != cut_events
            || working.source_before() != cut_source
        {
            return Err(SerializedGreenError::Corrupt(
                "normalized green prefix changed its convergence cut",
            ));
        }
        Ok((
            GreenJournalSuffixPhase::PollRetainedSuffix,
            GreenJournalSuffixSpliceProgress::Pending,
        ))
    }

    fn begin_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        let suffix_owner =
            self.retained_suffix
                .take_root()?
                .ok_or(SerializedGreenError::Corrupt(
                    "green retained suffix produced no root",
                ))?;
        let suffix_summary = sequence_node::<SerializedGreenSpec>(
            session.arena(),
            session.owner_id(&suffix_owner)?,
        )?
        .0;
        if suffix_summary.leaves != self.old_suffix_leaves
            || suffix_summary.metric != self.suffix.suffix
        {
            return Err(SerializedGreenError::Corrupt(
                "retained green suffix summary changed",
            ));
        }
        let build = self.build;
        let cut_leaves = self.cut.leaves_before();
        let cut_events = self.cut.events_before();
        let cut_source = self.cut.source_before();
        let cut_total = self.cut.source_total();
        let cut_enters = self.cut.block_enters_before();
        let cut_coverage = self.cut.coverage_runs_before();
        let cut_open_depth = self.cut.open_frames().len();
        let suffix_coverage = self.suffix.suffix_coverage_runs;
        let repaired_root_child = self.repaired_root_child;
        let builder = self.builder_mut()?;
        let prefix = builder
            .working_prefix
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "normalized green prefix snapshot has no working prefix",
            ))?;
        if prefix.build != build
            || prefix.summary.leaves != cut_leaves
            || prefix.summary.tokens != cut_events
            || prefix.summary.metric != cut_source
            || prefix.summary.blocks != cut_enters
            || prefix.summary.coverage_runs_for_valid_prefix()? != cut_coverage
            || u64::try_from(prefix.summary.balance)
                .map_err(|_| SerializedGreenError::Corrupt("green prefix balance is negative"))?
                != u64::try_from(cut_open_depth)
                    .map_err(|_| SerializedGreenError::Overflow("green prefix open depth"))?
        {
            return Err(SerializedGreenError::Corrupt(
                "normalized green prefix disagrees with its builder cut",
            ));
        }
        let mut expected = prefix.summary.followed_by(suffix_summary)?;
        if let Some(repaired_root_child) = repaired_root_child {
            // A balanced document exposes exactly its closed root child as the
            // outermost aggregate. All other summary fields are structural or
            // source folds and remain invariant under fixed-width Exit repair.
            expected.outermost = ChildSequenceAggregate::singleton(repaired_root_child);
        }
        let expected_coverage =
            cut_coverage
                .checked_add(suffix_coverage)
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix final coverage count",
                ))?;
        let structural_tokens =
            expected
                .blocks
                .checked_mul(2)
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix final structural tokens",
                ))?;
        let actual_coverage =
            expected
                .tokens
                .checked_sub(structural_tokens)
                .ok_or(SerializedGreenError::Corrupt(
                    "green suffix final event count is below its block envelope",
                ))?;
        if expected.metric != cut_total
            || expected.balance != 0
            || expected.minimum_prefix < 0
            || expected.blocks == 0
            || actual_coverage != expected_coverage
        {
            return Err(SerializedGreenError::Invalid(
                "direct green suffix does not produce the bound complete document",
            ));
        }
        let insertion = prefix.summary.leaves;
        builder.begin_canonical_leaf_insertion(session, prefix.owner, insertion, suffix_owner)?;
        self.receipt.current_prefix_leaves = insertion;
        self.expected_summary = Some(expected);
        Ok((
            GreenJournalSuffixPhase::PollSplice,
            GreenJournalSuffixSpliceProgress::Pending,
        ))
    }

    fn poll_splice(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        self.receipt.splice_polls =
            self.receipt
                .splice_polls
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix splice poll count",
                ))?;
        let builder = self.builder_mut()?;
        let progress = builder
            .splice
            .poll(session, &mut builder.sequence_receipt)?;
        Ok((
            if progress == ResumableSequenceSplitProgress::Complete {
                if self.repair_rewrites.is_empty() {
                    GreenJournalSuffixPhase::AllocateManifest
                } else {
                    GreenJournalSuffixPhase::BeginRepairLeaf
                }
            } else {
                GreenJournalSuffixPhase::PollSplice
            },
            GreenJournalSuffixSpliceProgress::Pending,
        ))
    }

    fn begin_repair_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        let start = self.next_repair_rewrite;
        let first = self
            .repair_rewrites
            .get(start)
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix repair lost its next rewrite",
            ))?;
        let absolute_old_leaf = first.location.leaf_index;
        let expected_old_leaf = first.location.leaf;
        let end = start
            + self.repair_rewrites[start..]
                .iter()
                .take_while(|rewrite| rewrite.location.leaf_index == absolute_old_leaf)
                .count();
        if self.repair_rewrites[start..end]
            .iter()
            .any(|rewrite| rewrite.location.leaf != expected_old_leaf)
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let relative_suffix_leaf = absolute_old_leaf
            .checked_sub(self.suffix.prefix_leaves)
            .ok_or(SerializedGreenError::StaleCursor)?;
        if relative_suffix_leaf >= self.old_suffix_leaves {
            return Err(SerializedGreenError::StaleCursor);
        }
        let target_leaf = self
            .cut
            .leaves_before()
            .checked_add(relative_suffix_leaf)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix repaired target leaf",
            ))?;

        let builder = self.builder.as_mut().ok_or(SerializedGreenError::Corrupt(
            "green suffix repair lost its builder",
        ))?;
        let root = builder
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix repair lost its joined root",
            ))?;
        let root_id = session.owner_id(&root)?;
        if locate_leaf_in_arena(session.arena(), root_id, target_leaf)? != Some(expected_old_leaf) {
            return Err(SerializedGreenError::StaleCursor);
        }
        let replacement = allocate_repaired_exit_leaf(
            session,
            expected_old_leaf,
            &self.repair_rewrites[start..end],
            &mut self.receipt.spanning_exit_repair,
            &mut builder.receipt,
        )?;
        builder.begin_canonical_leaf_replacement(
            session,
            root,
            target_leaf..target_leaf + 1,
            replacement,
        )?;
        self.next_repair_rewrite = end;
        self.receipt
            .spanning_exit_repair
            .distinct_exit_leaves_rewritten = self
            .receipt
            .spanning_exit_repair
            .distinct_exit_leaves_rewritten
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix repaired leaf receipt",
            ))?;
        Ok((
            GreenJournalSuffixPhase::PollRepairLeaf,
            GreenJournalSuffixSpliceProgress::Pending,
        ))
    }

    fn poll_repair_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        self.receipt.splice_polls =
            self.receipt
                .splice_polls
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix repair splice poll count",
                ))?;
        let builder = self.builder.as_mut().ok_or(SerializedGreenError::Corrupt(
            "green suffix repair lost its builder",
        ))?;
        if builder
            .splice
            .poll(session, &mut builder.sequence_receipt)?
            == ResumableSequenceSplitProgress::Pending
        {
            return Ok((
                GreenJournalSuffixPhase::PollRepairLeaf,
                GreenJournalSuffixSpliceProgress::Pending,
            ));
        }
        let complete = self.next_repair_rewrite == self.repair_rewrites.len();
        if complete {
            let repair = &mut self.receipt.spanning_exit_repair;
            repair.current_prefix_leaves_retained = self.cut.leaves_before();
            repair.old_suffix_leaves_retained = self.old_suffix_leaves;
            repair.unchanged_old_suffix_leaves = self
                .old_suffix_leaves
                .checked_sub(
                    u64::try_from(repair.distinct_exit_leaves_rewritten).map_err(|_| {
                        SerializedGreenError::Overflow(
                            "green suffix repaired leaf count conversion",
                        )
                    })?,
                )
                .ok_or(SerializedGreenError::Corrupt(
                    "green suffix repaired more leaves than it retained",
                ))?;
        }
        Ok((
            if complete {
                GreenJournalSuffixPhase::AllocateManifest
            } else {
                GreenJournalSuffixPhase::BeginRepairLeaf
            },
            GreenJournalSuffixSpliceProgress::Pending,
        ))
    }

    fn allocate_manifest(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(GreenJournalSuffixPhase, GreenJournalSuffixSpliceProgress), SerializedGreenError>
    {
        let expected = self
            .expected_summary
            .take()
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix splice lost its expected summary",
            ))?;
        let mut builder = self.builder.take().ok_or(SerializedGreenError::Corrupt(
            "green suffix splice lost its builder",
        ))?;
        let root = builder
            .splice
            .take_root()?
            .ok_or(SerializedGreenError::Corrupt(
                "green suffix splice produced no root",
            ))?;
        let root_id = session.owner_id(&root)?;
        let summary = sequence_node::<SerializedGreenSpec>(session.arena(), root_id)?.0;
        if summary.leaves != expected.leaves || !summary.same_semantics(expected) {
            return Err(SerializedGreenError::Corrupt(
                "green suffix splice changed folded semantics",
            ));
        }
        let suffix_end = self
            .suffix
            .prefix_leaves
            .checked_add(self.old_suffix_leaves)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix old leaf range",
            ))?;
        let rewritten = |leaf_index: u64| {
            self.repair_rewrites
                .iter()
                .any(|rewrite| rewrite.location.leaf_index == leaf_index)
        };
        let mut identity_sentinels = Vec::new();
        let mut first_unchanged = self.suffix.prefix_leaves;
        while first_unchanged < suffix_end && rewritten(first_unchanged) {
            first_unchanged =
                first_unchanged
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow(
                        "green suffix first unchanged leaf",
                    ))?;
        }
        if first_unchanged < suffix_end {
            identity_sentinels.push(first_unchanged);
        }
        if let Some(mut last_unchanged) = suffix_end.checked_sub(1) {
            while last_unchanged >= self.suffix.prefix_leaves && rewritten(last_unchanged) {
                let Some(previous) = last_unchanged.checked_sub(1) else {
                    break;
                };
                last_unchanged = previous;
            }
            if last_unchanged >= self.suffix.prefix_leaves
                && !rewritten(last_unchanged)
                && identity_sentinels.first().copied() != Some(last_unchanged)
            {
                identity_sentinels.push(last_unchanged);
            }
        }
        for absolute_old_leaf in &identity_sentinels {
            let expected =
                locate_leaf_in_arena(session.arena(), self.old_root, *absolute_old_leaf)?
                    .ok_or(SerializedGreenError::StaleCursor)?;
            let target_leaf = self
                .receipt
                .current_prefix_leaves
                .checked_add(
                    absolute_old_leaf
                        .checked_sub(self.suffix.prefix_leaves)
                        .ok_or(SerializedGreenError::StaleCursor)?,
                )
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix identity sentinel target",
                ))?;
            if locate_leaf_in_arena(session.arena(), root_id, target_leaf)? != Some(expected) {
                return Err(SerializedGreenError::Corrupt(
                    "green suffix splice copied rather than retained an unchanged old leaf",
                ));
            }
        }
        self.receipt.old_leaf_identity_sentinels_verified = identity_sentinels.len();
        let spec = &builder.spec;
        if summary.metric
            != (SerializedMetric {
                bytes: spec.source_bytes,
                utf16: spec.source_utf16,
            })
            || spec.known_bytes.end > summary.metric.bytes
        {
            return Err(SerializedGreenError::Invalid(
                "green suffix manifest does not cover its bound source",
            ));
        }
        let manifest_value = Manifest {
            syntax_profile: spec.syntax_profile,
            source_revision: spec.source_revision,
            source_root: spec.source_root,
            source_bytes: spec.source_bytes,
            source_utf16: spec.source_utf16,
            grammar_revision: spec.grammar_revision,
            parse_generation: spec.parse_generation,
            semantic_epoch: spec.semantic_epoch,
            known_bytes: spec.known_bytes.clone(),
            summary,
        };
        let payload = encode_manifest(&manifest_value);
        let (owner, allocation) = session.allocate(&payload, &[root_id])?;
        session.release(root)?;
        builder.receipt.manifest_nodes_allocated = builder
            .receipt
            .manifest_nodes_allocated
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix manifest allocation count",
            ))?;
        builder.receipt.resumable_arena_allocations = builder
            .receipt
            .resumable_arena_allocations
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix resumable allocation count",
            ))?;
        builder.receipt.payload_bytes_copied = builder
            .receipt
            .payload_bytes_copied
            .checked_add(allocation.payload_bytes_copied)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix payload bytes copied",
            ))?;
        builder.receipt.edge_bytes_copied = builder
            .receipt
            .edge_bytes_copied
            .checked_add(allocation.edge_bytes_copied)
            .ok_or(SerializedGreenError::Overflow(
                "green suffix edge bytes copied",
            ))?;
        builder.receipt.final_sequence_height = summary.height;
        builder.sync_journal_receipt(session)?;
        let build_receipt = builder.receipt();
        self.receipt.manifest_allocations = 1;
        self.receipt.build = build_receipt;
        self.receipt.spanning_exit_repair.build = build_receipt;
        let manifest = SerializedGreenBuildManifest {
            build: self.build,
            owner,
            receipt: build_receipt,
        };
        #[cfg(feature = "host-mirror-probe")]
        let target_manifest = session
            .arena()
            .scoped_query_id(session.owner_id(&manifest.owner)?)?;
        #[cfg(feature = "host-mirror-probe")]
        let host_prefix = builder
            .retained_host_prefix
            .take()
            .map(|prefix| {
                prefix.seal_after_writer_normalization(
                    HostGreenPrefixSpliceMint(()),
                    session.arena(),
                    target_manifest,
                )
            })
            .transpose()?;
        #[cfg(feature = "host-mirror-probe")]
        let host_splice = {
            let repaired_suffix = !self.repair_rewrites.is_empty();
            let old_total_leaves = self
                .suffix
                .prefix_leaves
                .checked_add(self.old_suffix_leaves)
                .ok_or(SerializedGreenError::Overflow(
                    "green suffix host old leaf total",
                ))?;
            crate::host_mirror::GreenSuffixLeafSpliceDraft::from_green_suffix_join(
                HostGreenPrefixSpliceMint(()),
                self.suffix.binding.manifest.scoped(),
                target_manifest,
                self.suffix.binding.source_revision,
                self.suffix.binding.source_root,
                self.cut.source_revision(),
                self.cut.source_root(),
                self.cut.grammar_revision(),
                self.cut.parse_generation(),
                self.suffix.old_total,
                self.cut.source_total(),
                SerializedMetric::default(),
                if repaired_suffix {
                    self.suffix.old_total
                } else {
                    self.suffix.old_prefix
                },
                if repaired_suffix {
                    self.cut.source_total()
                } else {
                    self.cut.source_before()
                },
                if repaired_suffix {
                    SerializedMetric::default()
                } else {
                    self.suffix.suffix
                },
                0,
                if repaired_suffix {
                    old_total_leaves
                } else {
                    self.suffix.prefix_leaves
                },
                if repaired_suffix {
                    summary.leaves
                } else {
                    self.receipt.current_prefix_leaves
                },
                if repaired_suffix {
                    0
                } else {
                    self.old_suffix_leaves
                },
                if repaired_suffix {
                    ArenaId::default()
                } else {
                    self.first_old_suffix_leaf
                },
                if repaired_suffix {
                    ArenaId::default()
                } else {
                    self.last_old_suffix_leaf
                },
            )?
        };
        self.result = Some(MechanismOnlyGreenJournalSuffix {
            manifest,
            receipt: self.receipt,
            #[cfg(feature = "host-mirror-probe")]
            host_splice,
            #[cfg(feature = "host-mirror-probe")]
            host_prefix,
        });
        Ok((
            GreenJournalSuffixPhase::Complete,
            GreenJournalSuffixSpliceProgress::Complete,
        ))
    }

    fn builder_mut(&mut self) -> Result<&mut ResumableSerializedGreenBuild, SerializedGreenError> {
        self.builder.as_mut().ok_or(SerializedGreenError::Corrupt(
            "green suffix splice lost its builder",
        ))
    }

    pub(crate) fn take_result(
        &mut self,
    ) -> Result<MechanismOnlyGreenJournalSuffix, SerializedGreenError> {
        if matches!(self.phase, GreenJournalSuffixPhase::Taken) {
            return Err(SerializedGreenError::Invalid(
                "green suffix splice output was already taken",
            ));
        }
        if !matches!(self.phase, GreenJournalSuffixPhase::Complete) {
            return Err(SerializedGreenError::Invalid(
                "green suffix splice is not complete",
            ));
        }
        let result = self.result.take().ok_or(SerializedGreenError::Corrupt(
            "green suffix splice lost its completed result",
        ))?;
        self.phase = GreenJournalSuffixPhase::Taken;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD_PREFIX_BYTES: u64 = 1;
    const CURRENT_PREFIX_BYTES: u64 = 2;

    fn spec(
        source_bytes: u64,
        revision: u64,
        root: u64,
        parse_generation: u64,
        semantic_epoch: u64,
    ) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 7,
            source_revision: SourceRevision(revision),
            source_root: SourceRootId(root),
            source_bytes,
            source_utf16: source_bytes,
            grammar_revision: GrammarRevision(11),
            parse_generation: ParseGeneration(parse_generation),
            semantic_epoch,
            known_bytes: 0..source_bytes,
        }
    }

    fn poll_to_input(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        loop {
            match builder.poll(session).unwrap() {
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("green splice fixture unexpectedly finalized")
                }
            }
        }
    }

    fn offer(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        builder.offer_event(session, event).unwrap();
        poll_to_input(builder, session);
    }

    fn identity_coverage(id: u64, bytes: u64) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(id),
                bytes,
                bytes,
                0,
                CoveragePart::CONTENT,
                BlockId(2),
                LogicalContribution::Identity,
            )
            .unwrap(),
        )
    }

    fn tail_chunks(total: u64, chunk: u64) -> Vec<u64> {
        let mut chunks = Vec::new();
        let mut remaining = total;
        while remaining != 0 {
            let next = remaining.min(chunk);
            chunks.push(next);
            remaining -= next;
        }
        chunks
    }

    fn committed_old(
        arena: &mut PageArena,
        tail_bytes: u64,
        chunk_bytes: u64,
    ) -> (SerializedGreenDocument, GreenSourceTailAdoptionCapability) {
        committed_old_with_open(
            arena,
            tail_bytes,
            chunk_bytes,
            GreenKind::PARAGRAPH,
            FactsEnvelope::empty(),
        )
    }

    fn committed_old_with_open(
        arena: &mut PageArena,
        tail_bytes: u64,
        chunk_bytes: u64,
        open_kind: GreenKind,
        open_facts: FactsEnvelope,
    ) -> (SerializedGreenDocument, GreenSourceTailAdoptionCapability) {
        let total = OLD_PREFIX_BYTES.checked_add(tail_bytes).unwrap();
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, spec(total, 1, 101, 1, 1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(2), open_kind, open_facts),
        );
        offer(
            &mut builder,
            &mut session,
            identity_coverage(1, OLD_PREFIX_BYTES),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_input(&mut builder, &mut session);
        let boundary_event_cut = builder
            .take_leaf_barrier_cut(&session)
            .unwrap()
            .events_before();
        for (index, bytes) in tail_chunks(tail_bytes, chunk_bytes).into_iter().enumerate() {
            offer(
                &mut builder,
                &mut session,
                identity_coverage(u64::try_from(index).unwrap() + 2, bytes),
            );
        }
        let open_close_facts = if open_kind == GreenKind::FENCED_CODE {
            let empty = GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap();
            let literal = GreenRelativeLogicalSlice::new(0..total, 0..total).unwrap();
            GreenCloseFacts::FencedCode(
                GreenFencedCodeCloseFacts::new(false, empty, literal).unwrap(),
            )
        } else {
            GreenCloseFacts::None
        };
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit_with_state(ClosedChildAggregate::default(), false, open_close_facts),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        builder.finish_input(&mut session).unwrap();
        loop {
            match builder.poll(&mut session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("old green fixture returned to input after EOF")
                }
            }
        }
        let document = builder.take_manifest().unwrap().commit(session).unwrap().0;
        let boundary = document
            .suffix_adoption_boundary_at_event_cut(arena, boundary_event_cut)
            .unwrap();
        let tail = document
            .source_tail_adoption_capability(arena, boundary)
            .unwrap();
        (document, tail)
    }

    fn current_prefix(
        arena: &mut PageArena,
        tail_bytes: u64,
    ) -> (
        ArenaBuildTicket,
        ResumableSerializedGreenBuild,
        BuilderGreenPrefixSnapshot,
    ) {
        current_prefix_with_open(
            arena,
            tail_bytes,
            GreenKind::PARAGRAPH,
            FactsEnvelope::empty(),
        )
    }

    fn current_prefix_with_open(
        arena: &mut PageArena,
        tail_bytes: u64,
        open_kind: GreenKind,
        open_facts: FactsEnvelope,
    ) -> (
        ArenaBuildTicket,
        ResumableSerializedGreenBuild,
        BuilderGreenPrefixSnapshot,
    ) {
        current_prefix_with_spec(
            arena,
            tail_bytes,
            open_kind,
            open_facts,
            CURRENT_PREFIX_BYTES.checked_add(tail_bytes).unwrap(),
            7,
            GrammarRevision(11),
        )
    }

    fn current_prefix_with_spec(
        arena: &mut PageArena,
        _tail_bytes: u64,
        open_kind: GreenKind,
        open_facts: FactsEnvelope,
        source_total: u64,
        syntax_profile: u64,
        grammar_revision: GrammarRevision,
    ) -> (
        ArenaBuildTicket,
        ResumableSerializedGreenBuild,
        BuilderGreenPrefixSnapshot,
    ) {
        let ticket = arena.begin_build().unwrap();
        let mut root_spec = spec(source_total, 2, 202, 2, 2);
        root_spec.syntax_profile = syntax_profile;
        root_spec.grammar_revision = grammar_revision;
        let mut builder = ResumableSerializedGreenBuild::new(&ticket, root_spec).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(2), open_kind, open_facts),
        );
        offer(
            &mut builder,
            &mut session,
            identity_coverage(1, CURRENT_PREFIX_BYTES),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_input(&mut builder, &mut session);
        let leaf = builder.take_leaf_barrier_cut(&session).unwrap();
        let snapshot = builder
            .capture_builder_green_prefix_snapshot(&session, &leaf)
            .unwrap();
        let ticket = session.suspend().unwrap();
        (ticket, builder, snapshot)
    }

    fn poll_job_to_completion(
        arena: &mut PageArena,
        mut ticket: ArenaBuildTicket,
        job: &mut ResumableGreenJournalSuffixSplice,
    ) -> (ArenaBuildTicket, usize) {
        let mut polls = 0;
        loop {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = job.poll(&mut session).unwrap();
            polls += 1;
            ticket = session.suspend().unwrap();
            if progress == GreenJournalSuffixSpliceProgress::Complete {
                return (ticket, polls);
            }
            assert!(polls < 1024, "green suffix splice must converge");
        }
    }

    fn abort_and_settle(arena: &mut PageArena, ticket: ArenaBuildTicket) {
        let build = arena.begin_build_abort(ticket).unwrap();
        while !arena.poll_build_abort(build, 1).unwrap().complete {}
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(16).unwrap();
        }
    }

    #[derive(Clone, Copy)]
    struct NestedFixtureFrame {
        kind: GreenKind,
        children: ChildSequenceAggregate,
    }

    struct NestedFixtureBuilder {
        events: Vec<GreenEvent>,
        open: Vec<NestedFixtureFrame>,
    }

    impl NestedFixtureBuilder {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                open: Vec::new(),
            }
        }

        fn facts(kind: GreenKind) -> FactsEnvelope {
            match kind {
                GreenKind::LIST => {
                    GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope()
                }
                GreenKind::ITEM => GreenItemOpenFacts::new(0, 2).unwrap().into_envelope(),
                _ => FactsEnvelope::empty(),
            }
        }

        fn enter(&mut self, block: BlockId, kind: GreenKind) {
            self.events
                .push(GreenEvent::enter(block, kind, Self::facts(kind)));
            self.open.push(NestedFixtureFrame {
                kind,
                children: ChildSequenceAggregate::default(),
            });
        }

        fn coverage(&mut self, id: CoverageId, target: BlockId) {
            self.events.push(GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    id,
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    target,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ));
        }

        fn close(&mut self, last_line_blank: bool) {
            let frame = self.open.pop().unwrap();
            let semantics = ContainerFoldSemantics {
                descends_through_last_child: matches!(
                    frame.kind,
                    GreenKind::LIST | GreenKind::ITEM
                ),
                is_item: frame.kind == GreenKind::ITEM,
                last_line_blank,
            };
            let closed = semantics.closed_summary(frame.children);
            let facts = if frame.kind == GreenKind::LIST {
                GreenCloseFacts::List {
                    tight: frame.children.list_is_tight(),
                }
            } else {
                GreenCloseFacts::None
            };
            self.events
                .push(GreenEvent::exit_with_state(closed, last_line_blank, facts));
            if let Some(parent) = self.open.last_mut() {
                parent.children = parent
                    .children
                    .followed_by(ChildSequenceAggregate::singleton(closed));
            }
        }
    }

    struct NestedRepairFixture {
        events: Vec<GreenEvent>,
        cut: usize,
        spanning_end: usize,
        sibling_end: usize,
    }

    fn nested_repair_fixture(prefix_ends_blank: bool, fresh_prefix: bool) -> NestedRepairFixture {
        let base = if fresh_prefix { 100 } else { 10 };
        let mut fixture = NestedFixtureBuilder::new();
        fixture.enter(BlockId(base + 1), GreenKind::DOCUMENT);
        fixture.enter(BlockId(base + 2), GreenKind::BLOCK_QUOTE);
        fixture.enter(BlockId(base + 3), GreenKind::LIST);
        fixture.enter(BlockId(base + 4), GreenKind::ITEM);
        for offset in 0..2 {
            let paragraph = BlockId(base + 5 + offset);
            fixture.enter(paragraph, GreenKind::PARAGRAPH);
            fixture.coverage(CoverageId(base + 1 + offset), paragraph);
            fixture.close(prefix_ends_blank);
        }
        let cut = fixture.events.len();
        fixture.close(false);
        fixture.close(false);
        fixture.close(false);
        let spanning_end = fixture.events.len();
        // The unchanged sibling keeps its committed old identity. Fresh prefix
        // IDs are intentionally disjoint from this retained suffix block.
        let sibling = BlockId(7);
        assert!((base + 1..=base + 6).all(|block| block != sibling.0));
        fixture.enter(sibling, GreenKind::PARAGRAPH);
        fixture.coverage(CoverageId(3), sibling);
        fixture.close(false);
        let sibling_end = fixture.events.len();
        fixture.close(false);
        assert!(fixture.open.is_empty());
        NestedRepairFixture {
            events: fixture.events,
            cut,
            spanning_end,
            sibling_end,
        }
    }

    fn build_nested_document(
        arena: &mut PageArena,
        root_spec: SerializedGreenRootSpec,
        fixture: &NestedRepairFixture,
    ) -> (SerializedGreenDocument, u64) {
        let ticket = arena.begin_build().unwrap();
        let mut builder = ResumableSerializedGreenBuild::new(&ticket, root_spec).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut boundary_event_cut = None;
        for (index, event) in fixture.events.iter().cloned().enumerate() {
            offer(&mut builder, &mut session, event);
            let event_end = index + 1;
            if [fixture.cut, fixture.spanning_end, fixture.sibling_end].contains(&event_end) {
                builder.begin_leaf_barrier(&mut session).unwrap();
                poll_to_input(&mut builder, &mut session);
                let cut = builder.take_leaf_barrier_cut(&session).unwrap();
                if event_end == fixture.cut {
                    boundary_event_cut = Some(cut.events_before());
                }
            }
        }
        builder.finish_input(&mut session).unwrap();
        loop {
            match builder.poll(&mut session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("nested green fixture returned to input after EOF")
                }
            }
        }
        (
            builder.take_manifest().unwrap().commit(session).unwrap().0,
            boundary_event_cut.unwrap(),
        )
    }

    fn build_nested_old(
        arena: &mut PageArena,
    ) -> (SerializedGreenDocument, GreenSourceTailAdoptionCapability) {
        let fixture = nested_repair_fixture(false, false);
        let (document, event_cut) = build_nested_document(arena, spec(3, 1, 501, 1, 1), &fixture);
        let boundary = document
            .suffix_adoption_boundary_at_event_cut(arena, event_cut)
            .unwrap();
        let tail = document
            .source_tail_adoption_capability(arena, boundary)
            .unwrap();
        (document, tail)
    }

    fn build_nested_current_prefix(
        arena: &mut PageArena,
    ) -> (
        ArenaBuildTicket,
        ResumableSerializedGreenBuild,
        BuilderGreenPrefixSnapshot,
    ) {
        let fixture = nested_repair_fixture(true, true);
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, spec(3, 2, 502, 2, 2)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        for event in fixture.events[..fixture.cut].iter().cloned() {
            offer(&mut builder, &mut session, event);
        }
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_input(&mut builder, &mut session);
        let cut = builder.take_leaf_barrier_cut(&session).unwrap();
        let snapshot = builder
            .capture_builder_green_prefix_snapshot(&session, &cut)
            .unwrap();
        let ticket = session.suspend().unwrap();
        (ticket, builder, snapshot)
    }

    fn trace_from_root(arena: &PageArena, root: ArenaId) -> Vec<DecodedGreenEventKind> {
        let leaves = sequence_node::<SerializedGreenSpec>(arena, root)
            .unwrap()
            .0
            .leaves;
        let mut trace = Vec::new();
        for leaf_index in 0..leaves {
            let leaf = locate_leaf_in_arena(arena, root, leaf_index)
                .unwrap()
                .unwrap();
            let (_, events) = decode_leaf(arena, leaf).unwrap();
            trace.extend(events.into_iter().map(|event| event.event));
        }
        trace
    }

    fn trace_from_document(
        document: &SerializedGreenDocument,
        arena: &PageArena,
    ) -> Vec<DecodedGreenEventKind> {
        let manifest = document.local_manifest_id(arena).unwrap();
        let (_, root) = decode_document(arena, manifest).unwrap();
        trace_from_root(arena, root)
    }

    #[test]
    fn ten_mib_tail_is_retained_with_tree_local_work_and_no_current_oracle() {
        const TAIL_BYTES: u64 = 10 * 1024 * 1024;
        const CHUNK_BYTES: u64 = 4 * 1024;

        let mut arena = PageArena::new();
        let (old, old_tail) = committed_old(&mut arena, TAIL_BYTES, CHUNK_BYTES);
        let old_first_suffix_leaf = old.leaf_at(&arena, 1).unwrap().unwrap();
        let (ticket, builder, snapshot) = current_prefix(&mut arena, TAIL_BYTES);
        let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
            &ticket, &arena, builder, snapshot, old_tail, &old,
        )
        .unwrap();
        let GreenJournalSuffixAdmission::Ready(mut job) = admission else {
            panic!("matching direct suffix must be admitted")
        };
        let (ticket, total_polls) = poll_job_to_completion(&mut arena, ticket, &mut job);
        let result = job.take_result().unwrap();
        let session = arena.resume_build(ticket).unwrap();
        let descriptor = result.descriptor_for_test(&session).unwrap();
        assert_eq!(
            descriptor.physical_metric(),
            SerializedMetric {
                bytes: CURRENT_PREFIX_BYTES + TAIL_BYTES,
                utf16: CURRENT_PREFIX_BYTES + TAIL_BYTES,
            }
        );
        assert_eq!(descriptor.parse_generation(), ParseGeneration(2));
        assert_eq!(
            result
                .leaf_at_for_test(&session, result.receipt().current_prefix_leaves)
                .unwrap(),
            Some(old_first_suffix_leaf)
        );
        let receipt = result.receipt();
        assert_eq!(receipt.described_old_suffix_bytes, TAIL_BYTES);
        assert!(receipt.old_suffix_leaves > 1);
        assert_eq!(receipt.open_frames_compared, 2);
        assert!(receipt.old_leaf_identity_sentinels_verified >= 1);
        assert_eq!(receipt.manifest_allocations, 1);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(receipt.document_sized_event_vectors, 0);
        assert!(receipt.mechanism_only_unpublishable);
        assert!(receipt.retained_range_polls < 128);
        assert!(receipt.splice_polls < 128);
        assert!(receipt.build.sequence_nodes_visited < 256);
        assert!(total_polls < 256);
        assert_eq!(receipt.build.leaf_pages_allocated, 1);
        let ticket = session.suspend().unwrap();
        drop(result);
        drop(job);
        abort_and_settle(&mut arena, ticket);
        assert_eq!(
            old.metric(&arena).unwrap().bytes,
            OLD_PREFIX_BYTES + TAIL_BYTES
        );
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "host-mirror-probe")]
    #[test]
    fn nested_quote_list_exit_repair_matches_full_rebuild_and_invalidates_host_suffix() {
        let mut arena = PageArena::new();
        let (old, old_tail) = build_nested_old(&mut arena);
        let current_fixture = nested_repair_fixture(true, true);
        let (oracle, _) =
            build_nested_document(&mut arena, spec(3, 3, 503, 3, 3), &current_fixture);
        let oracle_trace = trace_from_document(&oracle, &arena);
        let (ticket, builder, snapshot) = build_nested_current_prefix(&mut arena);
        let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
            &ticket, &arena, builder, snapshot, old_tail, &old,
        )
        .unwrap();
        let GreenJournalSuffixAdmission::Ready(mut job) = admission else {
            panic!("nested output-fold change must enter generic Exit repair")
        };
        let (ticket, total_polls) = poll_job_to_completion(&mut arena, ticket, &mut job);
        let result = job.take_result().unwrap();
        assert_eq!(
            job.take_result().unwrap_err(),
            SerializedGreenError::Invalid("green suffix splice output was already taken")
        );
        let receipt = result.receipt();
        let session = arena.resume_build(ticket).unwrap();
        let manifest_id = session.owner_id(&result.manifest.owner).unwrap();
        let (_, root) = decode_document(session.arena(), manifest_id).unwrap();
        assert_eq!(trace_from_root(session.arena(), root), oracle_trace);
        assert_eq!(receipt.spanning_exit_repair.open_depth, 4);
        assert_eq!(receipt.spanning_exit_repair.frames_planned, 4);
        assert_eq!(receipt.spanning_exit_repair.planning_polls, 4);
        assert_eq!(receipt.spanning_exit_repair.spanning_exits_examined, 4);
        assert!(receipt.spanning_exit_repair.exit_events_changed >= 2);
        assert!(receipt.spanning_exit_repair.distinct_exit_leaves_rewritten >= 1);
        assert!(receipt.old_leaf_identity_sentinels_verified >= 1);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(receipt.document_sized_event_vectors, 0);
        assert!(total_polls < 128);
        let (manifest, host) = result.into_host_mirror_fixture_parts();
        let (_, old_changed, new_changed, common_suffix) = host.range_counts_for_test();
        assert!(old_changed > 0);
        assert!(new_changed > 0);
        assert_eq!(common_suffix, 0);
        let ticket = session.suspend().unwrap();
        drop(manifest);
        drop(job);
        abort_and_settle(&mut arena, ticket);
        assert_eq!(old.metric(&arena).unwrap().bytes, 3);
        old.release_later(&mut arena).unwrap();
        oracle.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn nested_exit_repair_is_fuel_one_stale_safe_and_cancellable_after_mutation() {
        for cancel_after_rewrite_allocation in [false, true] {
            let mut arena = PageArena::new();
            let (old, old_tail) = build_nested_old(&mut arena);
            let old_trace = trace_from_document(&old, &arena);
            let (mut ticket, builder, snapshot) = build_nested_current_prefix(&mut arena);
            let GreenJournalSuffixAdmission::Ready(mut job) =
                ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
                    &ticket, &arena, builder, snapshot, old_tail, &old,
                )
                .unwrap()
            else {
                panic!("nested cancellation fixture must admit repair")
            };

            let foreign_ticket = arena.begin_build().unwrap();
            let mut foreign_session = arena.resume_build(foreign_ticket).unwrap();
            assert_eq!(
                job.poll(&mut foreign_session).unwrap_err(),
                SerializedGreenError::Invalid("green suffix splice and arena session differ")
            );
            let foreign_ticket = foreign_session.suspend().unwrap();
            abort_and_settle(&mut arena, foreign_ticket);

            let before_plan = arena.metrics();
            let mut session = arena.resume_build(ticket).unwrap();
            assert_eq!(
                job.poll(&mut session).unwrap(),
                GreenJournalSuffixSpliceProgress::Pending
            );
            assert_eq!(job.receipt().spanning_exit_repair.frames_planned, 1);
            ticket = session.suspend().unwrap();
            assert_eq!(arena.metrics(), before_plan);

            if cancel_after_rewrite_allocation {
                for _ in 0..64 {
                    let mut session = arena.resume_build(ticket).unwrap();
                    let progress = job.poll(&mut session).unwrap();
                    ticket = session.suspend().unwrap();
                    assert_eq!(progress, GreenJournalSuffixSpliceProgress::Pending);
                    if job
                        .receipt()
                        .spanning_exit_repair
                        .distinct_exit_leaves_rewritten
                        > 0
                    {
                        break;
                    }
                }
                assert!(
                    job.receipt()
                        .spanning_exit_repair
                        .distinct_exit_leaves_rewritten
                        > 0,
                    "fixture must reach one journalled repaired leaf"
                );
            }
            drop(job);
            abort_and_settle(&mut arena, ticket);
            assert_eq!(trace_from_document(&old, &arena), old_trace);
            old.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[test]
    fn changed_open_enter_facts_fail_closed_before_any_splice_work() {
        let mut arena = PageArena::new();
        let old_facts = GreenHeadingOpenFacts::new(1, GreenHeadingStyle::Atx)
            .unwrap()
            .into_envelope();
        let current_facts = GreenHeadingOpenFacts::new(2, GreenHeadingStyle::Atx)
            .unwrap()
            .into_envelope();
        let (old, old_tail) =
            committed_old_with_open(&mut arena, 4 * 1024, 1024, GreenKind::HEADING, old_facts);
        let (ticket, builder, snapshot) =
            current_prefix_with_open(&mut arena, 4 * 1024, GreenKind::HEADING, current_facts);

        let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
            &ticket, &arena, builder, snapshot, old_tail, &old,
        )
        .unwrap();
        let GreenJournalSuffixAdmission::Ineligible(fallback) = admission else {
            panic!("changed open Enter facts must not admit direct suffix reuse")
        };
        assert_eq!(
            fallback.reason(),
            GreenJournalSuffixIneligibleReason::OpenPathChanged
        );
        drop(fallback.into_builder());
        abort_and_settle(&mut arena, ticket);
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn spanning_fenced_code_is_ineligible_before_tail_adoption() {
        let mut arena = PageArena::new();
        let facts = GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 3, 0)
            .unwrap()
            .into_envelope();
        let (old, old_tail) = committed_old_with_open(
            &mut arena,
            4 * 1024,
            1024,
            GreenKind::FENCED_CODE,
            facts.clone(),
        );
        let (ticket, builder, snapshot) =
            current_prefix_with_open(&mut arena, 4 * 1024, GreenKind::FENCED_CODE, facts);
        let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
            &ticket, &arena, builder, snapshot, old_tail, &old,
        )
        .unwrap();
        let GreenJournalSuffixAdmission::Ineligible(fallback) = admission else {
            panic!("spanning FencedCode must remain outside generic Exit repair")
        };
        assert_eq!(
            fallback.reason(),
            GreenJournalSuffixIneligibleReason::SpanningExitRepairRequired
        );
        drop(fallback.into_builder());
        abort_and_settle(&mut arena, ticket);
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn source_and_profile_mismatches_fail_closed_at_admission() {
        const TAIL_BYTES: u64 = 4 * 1024;

        {
            let mut arena = PageArena::new();
            let (old, old_tail) = committed_old(&mut arena, TAIL_BYTES, 1024);
            let (ticket, builder, snapshot) = current_prefix_with_spec(
                &mut arena,
                TAIL_BYTES,
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
                CURRENT_PREFIX_BYTES + TAIL_BYTES + 1,
                7,
                GrammarRevision(11),
            );
            let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
                &ticket, &arena, builder, snapshot, old_tail, &old,
            )
            .unwrap();
            let GreenJournalSuffixAdmission::Ineligible(fallback) = admission else {
                panic!("a source total that cannot contain the retained tail must fail closed")
            };
            assert_eq!(
                fallback.reason(),
                GreenJournalSuffixIneligibleReason::SourceChanged
            );
            drop(fallback.into_builder());
            abort_and_settle(&mut arena, ticket);
            old.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }

        {
            let mut arena = PageArena::new();
            let (old, old_tail) = committed_old(&mut arena, TAIL_BYTES, 1024);
            let (ticket, builder, snapshot) = current_prefix_with_spec(
                &mut arena,
                TAIL_BYTES,
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
                CURRENT_PREFIX_BYTES + TAIL_BYTES,
                8,
                GrammarRevision(11),
            );
            let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
                &ticket, &arena, builder, snapshot, old_tail, &old,
            )
            .unwrap();
            let GreenJournalSuffixAdmission::Ineligible(fallback) = admission else {
                panic!("a grammar-profile mismatch must not admit direct suffix reuse")
            };
            assert_eq!(
                fallback.reason(),
                GreenJournalSuffixIneligibleReason::GrammarChanged
            );
            drop(fallback.into_builder());
            abort_and_settle(&mut arena, ticket);
            old.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[test]
    fn cancellation_after_every_poll_preserves_the_committed_old_green() {
        const TAIL_BYTES: u64 = 32 * 1024;
        const CHUNK_BYTES: u64 = 1024;

        let completion_polls = {
            let mut arena = PageArena::new();
            let (old, old_tail) = committed_old(&mut arena, TAIL_BYTES, CHUNK_BYTES);
            let (ticket, builder, snapshot) = current_prefix(&mut arena, TAIL_BYTES);
            let GreenJournalSuffixAdmission::Ready(mut job) =
                ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
                    &ticket, &arena, builder, snapshot, old_tail, &old,
                )
                .unwrap()
            else {
                panic!("cancellation fixture must be eligible")
            };
            let (ticket, polls) = poll_job_to_completion(&mut arena, ticket, &mut job);
            drop(job);
            abort_and_settle(&mut arena, ticket);
            old.release_later(&mut arena).unwrap();
            settle(&mut arena);
            polls
        };

        for cancel_after in 0..=completion_polls {
            let mut arena = PageArena::new();
            let (old, old_tail) = committed_old(&mut arena, TAIL_BYTES, CHUNK_BYTES);
            let old_metric = old.metric(&arena).unwrap();
            let (mut ticket, builder, snapshot) = current_prefix(&mut arena, TAIL_BYTES);
            let GreenJournalSuffixAdmission::Ready(mut job) =
                ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
                    &ticket, &arena, builder, snapshot, old_tail, &old,
                )
                .unwrap()
            else {
                panic!("cancellation fixture must be eligible")
            };
            for _ in 0..cancel_after {
                let mut session = arena.resume_build(ticket).unwrap();
                let progress = job.poll(&mut session).unwrap();
                ticket = session.suspend().unwrap();
                if progress == GreenJournalSuffixSpliceProgress::Complete {
                    break;
                }
            }
            drop(job);
            abort_and_settle(&mut arena, ticket);
            assert_eq!(old.metric(&arena).unwrap(), old_metric);
            old.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0, "cancel poll {cancel_after}");
        }
    }
}
