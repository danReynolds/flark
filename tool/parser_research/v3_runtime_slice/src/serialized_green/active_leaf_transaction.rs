//! Typed normalization of one provisional Paragraph range.
//!
//! This is a selected-storage feasibility seam, not Markdown grammar.  The
//! parser supplies an opaque capability for a Paragraph it already opened and
//! chooses one exhaustive normalization operation.  Storage revalidates the
//! exact packed Enter/matching Exit range, applies the operation against one
//! immutable root, and publishes only a complete replacement manifest.
//!
//! No operation searches by `BlockId`, retains a mutation event, or rebuilds an
//! aggregate source string.  A current candidate can bind the same capability
//! path through [`CandidateActiveLeafStorage`]; origin affects observation only,
//! not the normalization kernel.

use super::{
    ArenaBuildTransaction, ArenaId, BaseLeafReplacement, BlockId, CoveragePart,
    DecodedGreenEventKind, DecodedLeafEvent, DecodedLogicalContribution, FactsEnvelope,
    GreenEnterCapability, GreenHeadingOpenFacts, GreenKind, Manifest, PageArena, ParseGeneration,
    SequenceMutationReceipt, SerializedGreenBuildReceipt, SerializedGreenDocument,
    SerializedGreenError, SerializedGreenManifestId, SerializedGreenSpec, SerializedMetric,
    allocate_event_pages, decode_document, decode_leaf, encode_manifest, locate_leaf_in_arena,
    merge_sequence_receipt, replace_leaf_batch_in_transaction, sequence_node,
    sync_transaction_receipt,
};
use std::fmt;

/// Where the active-range capability entered the common storage path.
///
/// This is receipt data, not dispatch authority. Both variants execute the
/// same capability validation and immutable-range replacement code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveLeafOrigin {
    RetainedBase,
    CurrentCandidate,
}

/// Opaque authority for one exact, balanced provisional Paragraph range.
///
/// Construction is storage-only. It records physical event coordinates rather
/// than a global semantic lookup key. The capability is deliberately neither
/// `Clone` nor `Copy`; one normalization transaction consumes it.
#[must_use = "an active leaf capability must be consumed by one normalization transaction"]
#[derive(Debug)]
pub struct ActiveLeafCapability {
    manifest: SerializedGreenManifestId,
    enter: GreenEnterCapability,
    exit_leaf: ArenaId,
    exit_leaf_index: u64,
    exit_byte_offset: u16,
    source_metric: SerializedMetric,
}

impl ActiveLeafCapability {
    #[must_use]
    pub const fn block(&self) -> BlockId {
        self.enter.block
    }

    #[must_use]
    pub const fn source_metric(&self) -> SerializedMetric {
        self.source_metric
    }
}

/// Candidate-owned adapter for an unpublished packed root.
///
/// The adapter does not manufacture a second candidate representation. It
/// owns the same serialized-green root type and contributes only the fact that
/// this root is still candidate-local. The exact Paragraph capability is
/// resolved once and then consumed by [`Self::begin_transaction`].
#[derive(Debug)]
pub struct CandidateActiveLeafStorage {
    document: SerializedGreenDocument,
    capability: Option<ActiveLeafCapability>,
}

impl CandidateActiveLeafStorage {
    pub fn bind_unpublished(
        document: SerializedGreenDocument,
        arena: &PageArena,
        enter: GreenEnterCapability,
    ) -> Result<Self, SerializedGreenError> {
        let capability = document.resolve_active_paragraph(arena, enter)?;
        Ok(Self {
            document,
            capability: Some(capability),
        })
    }

    /// Consumes the one candidate-local capability while borrowing its packed
    /// root. The returned transaction has exactly the same implementation as a
    /// retained-base transaction.
    pub fn begin_transaction(&mut self) -> Result<ActiveLeafTransaction<'_>, SerializedGreenError> {
        let capability = self.capability.take().ok_or(SerializedGreenError::Invalid(
            "candidate active Paragraph capability was already consumed",
        ))?;
        ActiveLeafTransaction::new(
            &self.document,
            capability,
            ActiveLeafOrigin::CurrentCandidate,
        )
    }

    #[must_use]
    pub const fn document(&self) -> &SerializedGreenDocument {
        &self.document
    }

    #[must_use = "the candidate document remains an arena owner and must be transferred or released"]
    pub fn into_document(self) -> SerializedGreenDocument {
        self.document
    }
}

/// Successful atomic normalization result.
#[must_use = "the new serialized-green document owns an arena root"]
#[derive(Debug)]
pub struct ActiveLeafCommit {
    pub document: SerializedGreenDocument,
    pub receipt: ActiveLeafTransactionReceipt,
}

/// Candidate-only work receipt. Failed transactions return the same receipt
/// shape while the arena transaction rolls every owner back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveLeafTransactionReceipt {
    pub origin: Option<ActiveLeafOrigin>,
    pub pages_scanned: usize,
    pub events_scanned: usize,
    pub pages_rewritten: usize,
    pub source_metric: SerializedMetric,
    pub build: SerializedGreenBuildReceipt,
}

/// Failure after zero or more candidate-only allocations. The receipt is
/// diagnostic; no manifest owner escapes on this path.
#[derive(Debug, PartialEq, Eq)]
pub struct ActiveLeafTransactionFailure {
    pub error: SerializedGreenError,
    pub receipt: Box<ActiveLeafTransactionReceipt>,
}

impl fmt::Display for ActiveLeafTransactionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for ActiveLeafTransactionFailure {}

/// One typed, one-use Paragraph normalization transaction.
#[must_use = "an active leaf transaction must be normalized or discarded"]
#[derive(Debug)]
pub struct ActiveLeafTransaction<'a> {
    document: &'a SerializedGreenDocument,
    capability: ActiveLeafCapability,
    origin: ActiveLeafOrigin,
}

impl SerializedGreenDocument {
    /// Resolves one exact Paragraph Enter into its balanced packed range.
    ///
    /// This feasibility implementation walks the active range to mint the
    /// matching-Exit capability. Production restart storage should persist the
    /// sealed normalization-group footprint so restoration can resolve it by
    /// summary/path descent rather than rescanning a giant open Paragraph.
    pub fn resolve_active_paragraph(
        &self,
        arena: &PageArena,
        enter: GreenEnterCapability,
    ) -> Result<ActiveLeafCapability, SerializedGreenError> {
        resolve_active_paragraph(self, arena, enter)
    }

    /// Consumes retained-base Paragraph authority into the common transaction.
    pub fn begin_retained_active_leaf_transaction(
        &self,
        capability: ActiveLeafCapability,
    ) -> Result<ActiveLeafTransaction<'_>, SerializedGreenError> {
        ActiveLeafTransaction::new(self, capability, ActiveLeafOrigin::RetainedBase)
    }
}

impl<'a> ActiveLeafTransaction<'a> {
    fn new(
        document: &'a SerializedGreenDocument,
        capability: ActiveLeafCapability,
        origin: ActiveLeafOrigin,
    ) -> Result<Self, SerializedGreenError> {
        if capability.manifest != document.manifest_id()
            || capability.enter.manifest != document.manifest_id()
            || capability.enter.kind != GreenKind::PARAGRAPH
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        Ok(Self {
            document,
            capability,
            origin,
        })
    }

    /// Preserves the Paragraph identity while normalizing its wrapper to a
    /// canonical Setext Heading. Source/projection records are already exact
    /// parser output and are retained unchanged.
    pub fn promote_setext(
        self,
        arena: &mut PageArena,
        level: u8,
        next_parse_generation: ParseGeneration,
        next_semantic_epoch: u64,
    ) -> Result<ActiveLeafCommit, ActiveLeafTransactionFailure> {
        let facts = GreenHeadingOpenFacts::setext(level)
            .map(GreenHeadingOpenFacts::into_envelope)
            .map_err(|error| ActiveLeafTransactionFailure {
                error,
                receipt: Box::new(ActiveLeafTransactionReceipt {
                    origin: Some(self.origin),
                    ..ActiveLeafTransactionReceipt::default()
                }),
            })?;
        self.commit(
            arena,
            &ActiveLeafNormalization::SetextHeading { facts },
            next_parse_generation,
            next_semantic_epoch,
        )
    }

    /// Removes a reference-only Paragraph wrapper without deleting source.
    /// Paragraph-owned runs become parent-owned Gap/None records; already
    /// ancestor-owned marker/gap records retain their semantic part and owner.
    pub fn remove_reference_only(
        self,
        arena: &mut PageArena,
        next_parse_generation: ParseGeneration,
        next_semantic_epoch: u64,
    ) -> Result<ActiveLeafCommit, ActiveLeafTransactionFailure> {
        self.commit(
            arena,
            &ActiveLeafNormalization::ReferenceOnly,
            next_parse_generation,
            next_semantic_epoch,
        )
    }

    fn commit(
        self,
        arena: &mut PageArena,
        normalization: &ActiveLeafNormalization,
        next_parse_generation: ParseGeneration,
        next_semantic_epoch: u64,
    ) -> Result<ActiveLeafCommit, ActiveLeafTransactionFailure> {
        let mut receipt = ActiveLeafTransactionReceipt {
            origin: Some(self.origin),
            ..ActiveLeafTransactionReceipt::default()
        };
        match normalize_active_leaf(
            self.document,
            arena,
            &self.capability,
            normalization,
            next_parse_generation,
            next_semantic_epoch,
            &mut receipt,
        ) {
            Ok(document) => Ok(ActiveLeafCommit { document, receipt }),
            Err(error) => Err(ActiveLeafTransactionFailure {
                error,
                receipt: Box::new(receipt),
            }),
        }
    }
}

#[derive(Debug)]
enum ActiveLeafNormalization {
    SetextHeading { facts: FactsEnvelope },
    ReferenceOnly,
}

impl ActiveLeafNormalization {
    const fn removed_blocks(&self) -> u64 {
        match self {
            Self::SetextHeading { .. } => 0,
            Self::ReferenceOnly => 1,
        }
    }

    const fn removed_tokens(&self) -> u64 {
        match self {
            Self::SetextHeading { .. } => 0,
            Self::ReferenceOnly => 2,
        }
    }
}

fn resolve_active_paragraph(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    enter: GreenEnterCapability,
) -> Result<ActiveLeafCapability, SerializedGreenError> {
    if enter.manifest != document.manifest_id() || enter.kind != GreenKind::PARAGRAPH {
        return Err(SerializedGreenError::StaleCursor);
    }
    let manifest_id = document.local_manifest_id(arena)?;
    let (_, root) = decode_document(arena, manifest_id)?;
    let leaf_count = sequence_node::<SerializedGreenSpec>(arena, root)?.0.leaves;
    if enter.base_leaf_index >= leaf_count
        || locate_leaf_in_arena(arena, root, enter.base_leaf_index)? != Some(enter.leaf)
    {
        return Err(SerializedGreenError::StaleCursor);
    }

    let mut started = false;
    let mut metric = SerializedMetric::default();
    for leaf_index in enter.base_leaf_index..leaf_count {
        let leaf = locate_leaf_in_arena(arena, root, leaf_index)?
            .ok_or(SerializedGreenError::StaleCursor)?;
        let (_, events) = decode_leaf(arena, leaf)?;
        for decoded in events {
            if !started {
                if leaf_index != enter.base_leaf_index || decoded.byte_offset < enter.byte_offset {
                    continue;
                }
                if decoded.byte_offset != enter.byte_offset
                    || leaf != enter.leaf
                    || !matches!(
                        decoded.event,
                        DecodedGreenEventKind::Enter { block, kind, .. }
                            if block == enter.block && kind == GreenKind::PARAGRAPH
                    )
                {
                    return Err(SerializedGreenError::StaleCursor);
                }
                started = true;
                continue;
            }
            match decoded.event {
                DecodedGreenEventKind::Enter { .. } => {
                    return Err(SerializedGreenError::Corrupt(
                        "active Paragraph contains a nested block",
                    ));
                }
                DecodedGreenEventKind::Coverage(run) => {
                    metric = metric.checked_add(run.metric)?;
                }
                DecodedGreenEventKind::Exit { facts, .. } => {
                    facts.validate_for_kind(GreenKind::PARAGRAPH)?;
                    if metric.is_zero() {
                        return Err(SerializedGreenError::Invalid(
                            "active Paragraph has no source coverage",
                        ));
                    }
                    return Ok(ActiveLeafCapability {
                        manifest: document.manifest_id(),
                        enter,
                        exit_leaf: leaf,
                        exit_leaf_index: leaf_index,
                        exit_byte_offset: decoded.byte_offset,
                        source_metric: metric,
                    });
                }
            }
        }
    }
    Err(SerializedGreenError::Corrupt(
        "active Paragraph has no matching Exit",
    ))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn normalize_active_leaf(
    document: &SerializedGreenDocument,
    arena: &mut PageArena,
    capability: &ActiveLeafCapability,
    normalization: &ActiveLeafNormalization,
    next_parse_generation: ParseGeneration,
    next_semantic_epoch: u64,
    receipt: &mut ActiveLeafTransactionReceipt,
) -> Result<SerializedGreenDocument, SerializedGreenError> {
    if capability.manifest != document.manifest_id()
        || capability.enter.manifest != document.manifest_id()
        || capability.enter.kind != GreenKind::PARAGRAPH
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    let manifest_id = document.local_manifest_id(arena)?;
    let (base_manifest, base_root) = decode_document(arena, manifest_id)?;
    if next_parse_generation.0 <= base_manifest.parse_generation.0
        || next_semantic_epoch <= base_manifest.semantic_epoch
    {
        return Err(SerializedGreenError::Invalid(
            "active leaf normalization generation must advance",
        ));
    }
    if capability.enter.base_leaf_index > capability.exit_leaf_index {
        return Err(SerializedGreenError::StaleCursor);
    }

    let expected_blocks = base_manifest
        .summary
        .blocks
        .checked_sub(normalization.removed_blocks())
        .ok_or(SerializedGreenError::Corrupt(
            "active leaf block delta underflows",
        ))?;
    let expected_tokens = base_manifest
        .summary
        .tokens
        .checked_sub(normalization.removed_tokens())
        .ok_or(SerializedGreenError::Corrupt(
            "active leaf token delta underflows",
        ))?;

    let mut transaction = ArenaBuildTransaction::new(arena);
    let mut replacements = Vec::new();
    let mut metric = SerializedMetric::default();
    let mut started = false;
    let mut finished = false;

    for leaf_index in capability.enter.base_leaf_index..=capability.exit_leaf_index {
        let leaf = locate_leaf_in_arena(transaction.arena(), base_root, leaf_index)?
            .ok_or(SerializedGreenError::StaleCursor)?;
        if leaf_index == capability.enter.base_leaf_index && leaf != capability.enter.leaf {
            return Err(SerializedGreenError::StaleCursor);
        }
        if leaf_index == capability.exit_leaf_index && leaf != capability.exit_leaf {
            return Err(SerializedGreenError::StaleCursor);
        }
        let payload_bytes = transaction.arena().payload(leaf)?.len();
        let (_, events) = decode_leaf(transaction.arena(), leaf)?;
        receipt.pages_scanned += 1;
        receipt.events_scanned += events.len();
        receipt.build.maximum_decoded_page_buffer_bytes = receipt
            .build
            .maximum_decoded_page_buffer_bytes
            .max(payload_bytes + events.capacity() * std::mem::size_of::<DecodedLeafEvent>());

        let mut changed = false;
        let mut output = Vec::with_capacity(events.len());
        for decoded in events {
            let at_enter = leaf_index == capability.enter.base_leaf_index
                && decoded.byte_offset == capability.enter.byte_offset;
            let at_exit = leaf_index == capability.exit_leaf_index
                && decoded.byte_offset == capability.exit_byte_offset;
            if !started {
                if at_enter {
                    let DecodedGreenEventKind::Enter { block, kind, .. } = decoded.event else {
                        return Err(SerializedGreenError::StaleCursor);
                    };
                    if block != capability.enter.block || kind != GreenKind::PARAGRAPH {
                        return Err(SerializedGreenError::StaleCursor);
                    }
                    started = true;
                    changed = true;
                    match normalization {
                        ActiveLeafNormalization::SetextHeading { facts } => {
                            output.push(DecodedGreenEventKind::Enter {
                                block,
                                kind: GreenKind::HEADING,
                                facts: facts.clone(),
                            });
                        }
                        ActiveLeafNormalization::ReferenceOnly => {}
                    }
                } else {
                    output.push(decoded.event);
                }
                continue;
            }

            if finished {
                output.push(decoded.event);
                continue;
            }

            match decoded.event {
                DecodedGreenEventKind::Enter { .. } => {
                    return Err(SerializedGreenError::Corrupt(
                        "active Paragraph contains a nested block",
                    ));
                }
                DecodedGreenEventKind::Coverage(mut run) => {
                    metric = metric.checked_add(run.metric)?;
                    if matches!(normalization, ActiveLeafNormalization::ReferenceOnly) {
                        changed = true;
                        if run.owner_relative_depth == 0 {
                            run.part = CoveragePart::GAP;
                            run.logical_contribution = DecodedLogicalContribution::None;
                        } else {
                            run.owner_relative_depth -= 1;
                            if !matches!(run.logical_contribution, DecodedLogicalContribution::None)
                            {
                                return Err(SerializedGreenError::Invalid(
                                    "ancestor-owned reference source contributes to Paragraph logical text",
                                ));
                            }
                        }
                    }
                    output.push(DecodedGreenEventKind::Coverage(run));
                }
                DecodedGreenEventKind::Exit {
                    closed,
                    last_line_blank,
                    facts,
                } => {
                    if !at_exit {
                        return Err(SerializedGreenError::StaleCursor);
                    }
                    facts.validate_for_kind(GreenKind::PARAGRAPH)?;
                    finished = true;
                    match normalization {
                        ActiveLeafNormalization::SetextHeading { .. } => {
                            output.push(DecodedGreenEventKind::Exit {
                                closed,
                                last_line_blank,
                                facts,
                            });
                        }
                        ActiveLeafNormalization::ReferenceOnly => {
                            changed = true;
                        }
                    }
                }
            }
        }

        if changed {
            let handles = if output.is_empty() {
                Vec::new()
            } else {
                allocate_event_pages(&mut transaction, output, &mut receipt.build)?
            };
            receipt.pages_rewritten += 1;
            replacements.push(BaseLeafReplacement {
                leaf_index,
                expected_leaf: leaf,
                replacements: handles,
            });
        }
    }

    if !started || !finished || metric != capability.source_metric {
        return Err(SerializedGreenError::StaleCursor);
    }
    receipt.source_metric = metric;

    let mut sequence_receipt = SequenceMutationReceipt::default();
    let next_root = replace_leaf_batch_in_transaction::<SerializedGreenSpec>(
        &mut transaction,
        Some(base_root),
        replacements,
        &mut sequence_receipt,
    )?
    .ok_or(SerializedGreenError::Corrupt(
        "active leaf normalization removed the document root",
    ))?;
    let next_summary =
        sequence_node::<SerializedGreenSpec>(transaction.arena(), transaction.id(&next_root))?.0;
    if next_summary.balance != 0
        || next_summary.minimum_prefix < 0
        || next_summary.blocks != expected_blocks
        || next_summary.tokens != expected_tokens
        || next_summary.metric != base_manifest.summary.metric
    {
        return Err(SerializedGreenError::Corrupt(
            "active leaf normalization changed an unapproved document summary",
        ));
    }

    let next_manifest = Manifest {
        parse_generation: next_parse_generation,
        semantic_epoch: next_semantic_epoch,
        summary: next_summary,
        ..base_manifest
    };
    receipt.build.final_sequence_height = next_summary.height;
    let payload = encode_manifest(&next_manifest);
    let (manifest_owner, allocation) =
        transaction.allocate(&payload, &[transaction.id(&next_root)])?;
    transaction.release(next_root)?;
    receipt.build.manifest_nodes_allocated += 1;
    receipt.build.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.build.edge_bytes_copied += allocation.edge_bytes_copied;
    merge_sequence_receipt(&mut receipt.build, sequence_receipt);
    sync_transaction_receipt(&mut receipt.build, &transaction);
    debug_assert_eq!(transaction.live_owners(), 1);
    let owner = transaction.take(manifest_owner);
    let manifest = SerializedGreenManifestId::new(owner.scoped_id());
    Ok(SerializedGreenDocument { owner, manifest })
}

#[cfg(test)]
#[allow(clippy::wildcard_imports)]
mod tests {
    use super::*;
    use crate::{
        ClosedChildAggregate, CoverageId, GrammarRevision, GreenAffinity, GreenCloseFacts,
        GreenCoordinate, GreenEvent, GreenItemOpenFacts, GreenListBullet, GreenListOpenFacts,
        LogicalContribution, LogicalContributionView, SerializedGreenRootSpec, SourceProjectionRun,
        SourceRevision, SourceRootId,
    };

    const DOCUMENT: BlockId = BlockId(1);
    const PARAGRAPH: BlockId = BlockId(2);
    const QUOTE: BlockId = BlockId(3);
    const LIST: BlockId = BlockId(4);
    const ITEM: BlockId = BlockId(5);

    fn root_spec(
        bytes: u64,
        parse_generation: u64,
        semantic_epoch: u64,
    ) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(1),
            source_root: SourceRootId(1),
            source_bytes: bytes,
            source_utf16: bytes,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(parse_generation),
            semantic_epoch,
            known_bytes: 0..bytes,
        }
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(256).unwrap();
        }
    }

    fn paragraph_enter(
        document: &SerializedGreenDocument,
        arena: &PageArena,
        offset: u64,
    ) -> GreenEnterCapability {
        let cursor = document
            .seek(
                arena,
                GreenCoordinate::Bytes,
                offset,
                GreenAffinity::Downstream,
            )
            .unwrap();
        cursor
            .open_path()
            .iter()
            .find(|frame| frame.block == PARAGRAPH)
            .unwrap()
            .enter
    }

    fn long_setext_base(arena: &mut PageArena) -> SerializedGreenDocument {
        const CONTENT_RUNS: u64 = 2_000;
        const SUFFIX_BLOCKS: u64 = 2_000;
        let mut events = vec![
            GreenEvent::enter(DOCUMENT, GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(PARAGRAPH, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        ];
        for index in 0..CONTENT_RUNS {
            events.push(GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1 + index),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    PARAGRAPH,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ));
        }
        events.push(GreenEvent::Coverage(
            SourceProjectionRun::new(CoverageId(10_000), 3, 3, 0, CoveragePart::BLOCK_MARKER)
                .unwrap(),
        ));
        events.push(GreenEvent::Coverage(
            SourceProjectionRun::new(CoverageId(10_001), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
        ));
        events.push(GreenEvent::exit(ClosedChildAggregate::default()));
        for index in 0..SUFFIX_BLOCKS {
            let block = BlockId(20_000 + index);
            events.push(GreenEvent::enter(
                block,
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ));
            events.push(GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(30_000 + index),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    block,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ));
            events.push(GreenEvent::exit(ClosedChildAggregate::default()));
        }
        events.push(GreenEvent::exit(ClosedChildAggregate::default()));
        let bytes = CONTENT_RUNS + 4 + SUFFIX_BLOCKS;
        SerializedGreenDocument::build(
            arena,
            root_spec(bytes, 1, 1),
            events,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap()
    }

    #[test]
    fn retained_setext_preserves_block_and_distant_suffix_leaf_identity() {
        let mut arena = PageArena::new();
        let base = long_setext_base(&mut arena);
        settle(&mut arena);
        let far = base
            .leaf_at(&arena, base.leaf_count(&arena).unwrap() - 1)
            .unwrap()
            .unwrap();
        let metric = base.metric(&arena).unwrap();
        let capability = base
            .resolve_active_paragraph(&arena, paragraph_enter(&base, &arena, 0))
            .unwrap();
        let commit = base
            .begin_retained_active_leaf_transaction(capability)
            .unwrap()
            .promote_setext(&mut arena, 1, ParseGeneration(2), 2)
            .unwrap();
        settle(&mut arena);

        assert_eq!(commit.receipt.origin, Some(ActiveLeafOrigin::RetainedBase));
        assert_eq!(
            commit.receipt.source_metric,
            SerializedMetric {
                bytes: 2_004,
                utf16: 2_004
            }
        );
        assert!(commit.receipt.pages_scanned > 1);
        assert!(commit.receipt.build.sequence_leaves_reused > 0);
        assert_eq!(commit.document.metric(&arena).unwrap(), metric);
        assert_eq!(
            commit
                .document
                .leaf_at(&arena, commit.document.leaf_count(&arena).unwrap() - 1)
                .unwrap()
                .unwrap(),
            far
        );

        let mut next = commit
            .document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        let owner = next
            .next_coverage(&commit.document, &arena)
            .unwrap()
            .unwrap()
            .owner;
        assert_eq!(owner.block, PARAGRAPH);
        assert_eq!(owner.kind, GreenKind::HEADING);
        assert_eq!(
            GreenHeadingOpenFacts::try_from_envelope(
                &next
                    .open_path()
                    .iter()
                    .find(|frame| frame.block == PARAGRAPH)
                    .unwrap()
                    .facts,
            )
            .unwrap(),
            GreenHeadingOpenFacts::setext(1).unwrap()
        );

        let mut old = base
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        assert_eq!(
            old.next_coverage(&base, &arena)
                .unwrap()
                .unwrap()
                .owner
                .kind,
            GreenKind::PARAGRAPH
        );

        base.release_later(&mut arena).unwrap();
        commit.document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_and_retained_roots_use_the_same_capability_kernel() {
        let mut arena = PageArena::new();
        let candidate_document = long_setext_base(&mut arena);
        assert!(
            candidate_document.leaf_count(&arena).unwrap()
                < candidate_document.block_count(&arena).unwrap() / 10,
            "candidate storage must pack many small Paragraphs per page"
        );
        let enter = paragraph_enter(&candidate_document, &arena, 0);
        let mut candidate =
            CandidateActiveLeafStorage::bind_unpublished(candidate_document, &arena, enter)
                .unwrap();
        let old_manifest = candidate.document().manifest_id();
        let commit = candidate
            .begin_transaction()
            .unwrap()
            .promote_setext(&mut arena, 2, ParseGeneration(2), 2)
            .unwrap();
        assert_eq!(
            commit.receipt.origin,
            Some(ActiveLeafOrigin::CurrentCandidate)
        );
        assert_ne!(commit.document.manifest_id(), old_manifest);

        let mut next = commit
            .document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        assert_eq!(
            next.next_coverage(&commit.document, &arena)
                .unwrap()
                .unwrap()
                .owner
                .kind,
            GreenKind::HEADING
        );
        let mut old = candidate
            .document()
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        assert_eq!(
            old.next_coverage(candidate.document(), &arena)
                .unwrap()
                .unwrap()
                .owner
                .kind,
            GreenKind::PARAGRAPH
        );

        candidate.into_document().release_later(&mut arena).unwrap();
        commit.document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn reference_only_removes_wrapper_but_preserves_total_nested_source() {
        let mut arena = PageArena::new();
        let events = vec![
            GreenEvent::enter(DOCUMENT, GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(QUOTE, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
            GreenEvent::enter(PARAGRAPH, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(1), 2, 2, 1, CoveragePart::CONTAINER_MARKER)
                    .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    9,
                    9,
                    0,
                    CoveragePart::CONTENT,
                    PARAGRAPH,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(3), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let base = SerializedGreenDocument::build(
            &mut arena,
            root_spec(12, 1, 1),
            events,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap();
        let capability = base
            .resolve_active_paragraph(&arena, paragraph_enter(&base, &arena, 0))
            .unwrap();
        let commit = base
            .begin_retained_active_leaf_transaction(capability)
            .unwrap()
            .remove_reference_only(&mut arena, ParseGeneration(2), 2)
            .unwrap();

        assert_eq!(
            base.metric(&arena).unwrap(),
            commit.document.metric(&arena).unwrap()
        );
        assert_eq!(base.block_count(&arena).unwrap(), 3);
        assert_eq!(commit.document.block_count(&arena).unwrap(), 2);
        let mut cursor = commit
            .document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        let marker = cursor
            .next_coverage(&commit.document, &arena)
            .unwrap()
            .unwrap();
        assert_eq!(marker.coverage, CoverageId(1));
        assert_eq!(marker.owner.block, QUOTE);
        assert_eq!(marker.part, CoveragePart::CONTAINER_MARKER);
        assert_eq!(marker.logical_contribution, LogicalContributionView::None);
        let definition = cursor
            .next_coverage(&commit.document, &arena)
            .unwrap()
            .unwrap();
        assert_eq!(definition.coverage, CoverageId(2));
        assert_eq!(definition.owner.block, QUOTE);
        assert_eq!(definition.part, CoveragePart::GAP);
        assert_eq!(
            definition.logical_contribution,
            LogicalContributionView::None
        );
        let ending = cursor
            .next_coverage(&commit.document, &arena)
            .unwrap()
            .unwrap();
        assert_eq!(ending.coverage, CoverageId(3));
        assert_eq!(ending.owner.block, QUOTE);
        assert_eq!(ending.part, CoveragePart::GAP);
        assert_eq!(ending.logical_contribution, LogicalContributionView::None);
        assert!(
            cursor
                .next_coverage(&commit.document, &arena)
                .unwrap()
                .is_none()
        );

        base.release_later(&mut arena).unwrap();
        commit.document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn reference_only_rebases_nested_quote_list_and_item_markers_exactly() {
        let mut arena = PageArena::new();
        let events = vec![
            GreenEvent::enter(DOCUMENT, GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(QUOTE, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
            GreenEvent::enter(
                LIST,
                GreenKind::LIST,
                GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope(),
            ),
            GreenEvent::enter(
                ITEM,
                GreenKind::ITEM,
                GreenItemOpenFacts::new(0, 2).unwrap().into_envelope(),
            ),
            GreenEvent::enter(PARAGRAPH, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(1), 2, 2, 3, CoveragePart::CONTAINER_MARKER)
                    .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(2), 2, 2, 2, CoveragePart::CONTAINER_MARKER)
                    .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(3), 2, 2, 1, CoveragePart::CONTAINER_MARKER)
                    .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(4),
                    9,
                    9,
                    0,
                    CoveragePart::CONTENT,
                    PARAGRAPH,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            GreenEvent::Coverage(
                SourceProjectionRun::new(CoverageId(5), 1, 1, 0, CoveragePart::TERMINAL).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit_with_facts(
                ClosedChildAggregate::default(),
                GreenCloseFacts::List { tight: true },
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let base = SerializedGreenDocument::build(
            &mut arena,
            root_spec(16, 1, 1),
            events,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap();
        let capability = base
            .resolve_active_paragraph(&arena, paragraph_enter(&base, &arena, 0))
            .unwrap();
        let commit = base
            .begin_retained_active_leaf_transaction(capability)
            .unwrap()
            .remove_reference_only(&mut arena, ParseGeneration(2), 2)
            .unwrap();

        let mut cursor = commit
            .document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        let expected = [
            (CoverageId(1), QUOTE, CoveragePart::CONTAINER_MARKER),
            (CoverageId(2), LIST, CoveragePart::CONTAINER_MARKER),
            (CoverageId(3), ITEM, CoveragePart::CONTAINER_MARKER),
            (CoverageId(4), ITEM, CoveragePart::GAP),
            (CoverageId(5), ITEM, CoveragePart::GAP),
        ];
        for (coverage, owner, part) in expected {
            let value = cursor
                .next_coverage(&commit.document, &arena)
                .unwrap()
                .unwrap();
            assert_eq!(value.coverage, coverage);
            assert_eq!(value.owner.block, owner);
            assert_eq!(value.part, part);
            assert_eq!(value.logical_contribution, LogicalContributionView::None);
        }
        assert!(
            cursor
                .next_coverage(&commit.document, &arena)
                .unwrap()
                .is_none()
        );
        assert_eq!(commit.document.metric(&arena).unwrap().bytes, 16);
        assert_eq!(commit.document.block_count(&arena).unwrap(), 4);

        base.release_later(&mut arena).unwrap();
        commit.document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn stale_capability_fails_closed_without_publishing() {
        let mut arena = PageArena::new();
        let base = long_setext_base(&mut arena);
        let first = base
            .resolve_active_paragraph(&arena, paragraph_enter(&base, &arena, 0))
            .unwrap();
        let stale = base
            .resolve_active_paragraph(&arena, paragraph_enter(&base, &arena, 0))
            .unwrap();
        let next = base
            .begin_retained_active_leaf_transaction(first)
            .unwrap()
            .promote_setext(&mut arena, 1, ParseGeneration(2), 2)
            .unwrap()
            .document;
        settle(&mut arena);
        let before = arena.metrics().live_nodes;
        assert!(matches!(
            next.begin_retained_active_leaf_transaction(stale),
            Err(SerializedGreenError::StaleCursor)
        ));
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, before);
        assert_eq!(next.metric(&arena).unwrap(), base.metric(&arena).unwrap());

        base.release_later(&mut arena).unwrap();
        next.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn late_close_failure_rolls_back_pages_and_keeps_old_root_queryable() {
        let mut arena = PageArena::new();
        let base = long_setext_base(&mut arena);
        settle(&mut arena);
        let before = arena.metrics().live_nodes;
        let enter = paragraph_enter(&base, &arena, 0);
        let mut capability = base.resolve_active_paragraph(&arena, enter).unwrap();
        assert!(capability.exit_leaf_index > capability.enter.base_leaf_index);
        capability.exit_byte_offset = capability.exit_byte_offset.saturating_add(1);
        let failure = base
            .begin_retained_active_leaf_transaction(capability)
            .unwrap()
            .promote_setext(&mut arena, 1, ParseGeneration(2), 2)
            .unwrap_err();
        assert_eq!(failure.error, SerializedGreenError::StaleCursor);
        assert!(failure.receipt.build.leaf_pages_allocated > 0);
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, before);
        let mut old = base
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        assert_eq!(
            old.next_coverage(&base, &arena)
                .unwrap()
                .unwrap()
                .owner
                .kind,
            GreenKind::PARAGRAPH
        );

        base.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}
