//! Host-owned immutable green-page mirror feasibility gate.
//!
//! The worker exports only closed leaf envelopes (one green leaf plus its
//! ordered projection Program children). The host owns those bytes, applies
//! one measured leaf splice, and can answer queries after every worker root
//! has been retired. Splice coordinates come from the typed green suffix join;
//! neither exporter nor host compares complete old and new documents.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{
    ARENA_PAGE_BYTES, ArenaBuildId, ArenaId, ArenaScopedId, CopiedGreenLeafDecoded,
    CopiedGreenLeafSummary, CopiedGreenStructuralEvent, FactsEnvelope, GrammarRevision,
    GreenCloseFacts, GreenKind, MAX_PACKED_ARENA_CHILDREN, PageArena, ParseGeneration,
    SerializedGreenCompositeDescriptor, SerializedGreenDocument, SerializedGreenError,
    SerializedMetric, SourceRevision, SourceRootId, serialized_green_leaf_at_scoped_manifest,
    validate_copied_green_leaf_closure,
};

const HOST_BUNDLE_SCHEMA: u16 = 1;
const MAX_BUNDLE_OBJECTS: usize = 8_192;
const MAX_BUNDLE_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
const HOST_MEASURED_PAGE_LEAVES: usize = 64;
/// Synchronous host viewport reconstruction never materializes a deeper open
/// stack. Documents remain source-editable beyond this product budget, while
/// semantic rendering/actions fail closed until a bounded view is available.
const HOST_MAX_VIEWPORT_OPEN_DEPTH: u64 = 256;
const SEQUENCE_POLYNOMIAL_BASE: u128 = 0x0000_0000_0000_0000_0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PublicationSessionId(pub(crate) [u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DocumentSessionId(pub(crate) [u8; 16]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct HostRevisionId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProtocolDigest(pub(crate) u128);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceContentHash128 {
    pub(crate) word0: u32,
    pub(crate) word1: u32,
    pub(crate) word2: u32,
    pub(crate) word3: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct HostObjectId {
    pub(crate) session: PublicationSessionId,
    pub(crate) arena_slot: u32,
    pub(crate) arena_generation: u32,
}

impl HostObjectId {
    const fn from_arena(session: PublicationSessionId, id: ArenaId) -> Self {
        Self {
            session,
            arena_slot: id.index,
            arena_generation: id.generation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CopiedObject {
    ProjectionProgram {
        payload: Arc<[u8]>,
    },
    GreenLeaf {
        payload: Arc<[u8]>,
        children: Arc<[HostObjectId]>,
    },
}

impl CopiedObject {
    fn payload(&self) -> &[u8] {
        match self {
            Self::ProjectionProgram { payload } | Self::GreenLeaf { payload, .. } => payload,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CopiedObjectEnvelope {
    pub(crate) id: HostObjectId,
    pub(crate) object: CopiedObject,
    pub(crate) content_digest: ProtocolDigest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MetricRange {
    pub(crate) start: SerializedMetric,
    pub(crate) end: SerializedMetric,
}

impl MetricRange {
    fn validate(self, total: SerializedMetric) -> Result<(), HostMirrorError> {
        if self.start.bytes > self.end.bytes
            || self.start.utf16 > self.end.utf16
            || self.end.bytes > total.bytes
            || self.end.utf16 > total.utf16
        {
            return Err(HostMirrorError::Invalid("dirty range escapes exact source"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirtyOverlay {
    pub(crate) structural_base_revision: SourceRevision,
    pub(crate) source_target_revision: SourceRevision,
    /// Prefixes before this boundary are byte-for-byte and coordinate-for-
    /// coordinate identical to the installed structural revision. Everything
    /// at or after it is served from the exact source until an exact-current
    /// structural bundle lands atomically.
    pub(crate) damage_start: SerializedMetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceLineageEdit {
    pub(crate) base: MetricRange,
    pub(crate) target: MetricRange,
}

/// Hash/root/metric authority already maintained by the persistent source
/// tree. Production publication never hashes or materializes the full source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceVersion {
    pub(crate) document_session: DocumentSessionId,
    pub(crate) revision: SourceRevision,
    pub(crate) metric: SerializedMetric,
    pub(crate) hash: SourceContentHash128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeafSplice {
    pub(crate) old_start: u64,
    pub(crate) old_delete: u64,
    pub(crate) inserted: Vec<HostObjectId>,
    pub(crate) deleted_id_digest: ProtocolDigest,
    pub(crate) deleted_content_digest: ProtocolDigest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StructuralBundle {
    pub(crate) schema: u16,
    pub(crate) session: PublicationSessionId,
    pub(crate) base: Option<HostRevisionId>,
    pub(crate) target: HostRevisionId,
    pub(crate) source_document_session: DocumentSessionId,
    pub(crate) structural_source_revision: SourceRevision,
    pub(crate) parse_generation: ParseGeneration,
    pub(crate) grammar_revision: GrammarRevision,
    pub(crate) source_hash: SourceContentHash128,
    pub(crate) target_metric: SerializedMetric,
    pub(crate) target_leaf_count: u64,
    pub(crate) base_manifest_digest: ProtocolDigest,
    pub(crate) splice: LeafSplice,
    pub(crate) objects: Vec<CopiedObjectEnvelope>,
}

/// Opaque storage provenance minted by canonical retained-prefix admission.
///
/// This seed is deliberately grammar-neutral and incomplete publication
/// authority. It records only the maximum old leaf prefix whose identities
/// entered the fresh journal. Every retroactive sealed-leaf replacement in
/// the builder monotonically caps that count before mutation. The actual
/// metric and identity witness are derived only after writer normalization
/// and final manifest allocation.
#[derive(Debug)]
pub(crate) struct CanonicalRetainedGreenPrefixSeed {
    target_build: ArenaBuildId,
    base_manifest: ArenaScopedId,
    base_source_revision: SourceRevision,
    base_source_root: SourceRootId,
    target_source_revision: SourceRevision,
    target_source_root: SourceRootId,
    grammar_revision: GrammarRevision,
    target_parse_generation: ParseGeneration,
    retained_identity_prefix_leaves: u64,
}

impl CanonicalRetainedGreenPrefixSeed {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_retained_restart(
        _mint: crate::serialized_green::HostRetainedPrefixMint,
        target_build: ArenaBuildId,
        base_manifest: ArenaScopedId,
        base_source_revision: SourceRevision,
        base_source_root: SourceRootId,
        target_source_revision: SourceRevision,
        target_source_root: SourceRootId,
        grammar_revision: GrammarRevision,
        target_parse_generation: ParseGeneration,
        retained_identity_prefix_leaves: u64,
    ) -> Result<Self, SerializedGreenError> {
        if base_source_revision == target_source_revision
            || base_source_root.0 == 0
            || target_source_root.0 == 0
            || grammar_revision.0 == 0
            || target_parse_generation.0 == 0
        {
            return Err(SerializedGreenError::Corrupt(
                "retained host prefix has inconsistent bindings",
            ));
        }
        Ok(Self {
            target_build,
            base_manifest,
            base_source_revision,
            base_source_root,
            target_source_revision,
            target_source_root,
            grammar_revision,
            target_parse_generation,
            retained_identity_prefix_leaves,
        })
    }

    /// Records the first leaf touched by one canonical retroactive builder
    /// rewrite. The cap may only decrease; append-only work does not call this
    /// method and therefore preserves the retained identity prefix.
    pub(crate) fn cap_before_rewrite(
        &mut self,
        target_build: ArenaBuildId,
        first_rewritten_leaf: u64,
    ) -> Result<(), SerializedGreenError> {
        if target_build != self.target_build {
            return Err(SerializedGreenError::Corrupt(
                "retained host prefix rewrite belongs to another build",
            ));
        }
        self.retained_identity_prefix_leaves = self
            .retained_identity_prefix_leaves
            .min(first_rewritten_leaf);
        Ok(())
    }

    /// Seals the monotone provenance only after all writer normalization and
    /// the fresh manifest exist. Two logarithmic prefix observations validate
    /// the claimed old/fresh leaf identity and derive the exact shared metric;
    /// they never discover or widen the retained prefix.
    pub(crate) fn seal_after_writer_normalization(
        self,
        _mint: crate::serialized_green::HostGreenPrefixSpliceMint,
        arena: &PageArena,
        target_manifest: ArenaScopedId,
    ) -> Result<CanonicalRetainedGreenPrefixProof, SerializedGreenError> {
        if self.base_manifest.arena() != target_manifest.arena() {
            return Err(SerializedGreenError::Corrupt(
                "retained host prefix manifests belong to different arenas",
            ));
        }
        let (common_prefix_metric, last_common_leaf) = if self.retained_identity_prefix_leaves == 0
        {
            (SerializedMetric::default(), None)
        } else {
            let base = crate::serialized_green::serialized_green_prefix_metric_and_last_leaf_at_scoped_manifest(
                    arena,
                    self.base_manifest,
                    self.retained_identity_prefix_leaves,
                )?
                .ok_or(SerializedGreenError::Corrupt(
                    "retained base host prefix disappeared before sealing",
                ))?;
            let target = crate::serialized_green::serialized_green_prefix_metric_and_last_leaf_at_scoped_manifest(
                    arena,
                    target_manifest,
                    self.retained_identity_prefix_leaves,
                )?
                .ok_or(SerializedGreenError::Corrupt(
                    "retained target host prefix disappeared before sealing",
                ))?;
            if base != target {
                return Err(SerializedGreenError::Corrupt(
                    "retained host prefix identity or metric changed during normalization",
                ));
            }
            (base.0, Some(base.1))
        };
        Ok(CanonicalRetainedGreenPrefixProof {
            target_build: self.target_build,
            base_manifest: self.base_manifest,
            base_source_revision: self.base_source_revision,
            base_source_root: self.base_source_root,
            target_source_revision: self.target_source_revision,
            target_source_root: self.target_source_root,
            grammar_revision: self.grammar_revision,
            target_parse_generation: self.target_parse_generation,
            common_prefix_metric,
            common_prefix_leaves: self.retained_identity_prefix_leaves,
            last_common_leaf,
        })
    }
}

/// Post-normalization retained-prefix proof. It remains mechanism-only until
/// the actor's matched-C join consumes it together with the suffix draft.
#[derive(Debug)]
pub(crate) struct CanonicalRetainedGreenPrefixProof {
    target_build: ArenaBuildId,
    base_manifest: ArenaScopedId,
    base_source_revision: SourceRevision,
    base_source_root: SourceRootId,
    target_source_revision: SourceRevision,
    target_source_root: SourceRootId,
    grammar_revision: GrammarRevision,
    target_parse_generation: ParseGeneration,
    common_prefix_metric: SerializedMetric,
    common_prefix_leaves: u64,
    last_common_leaf: Option<ArenaId>,
}

/// Storage-authored suffix-splice draft. Although all fields are typed, this
/// value is not exporter authority: it initially classifies the entire fresh
/// prefix as changed and has not met matched-C or canonical prefix identity.
#[derive(Debug)]
pub(crate) struct GreenSuffixLeafSpliceDraft {
    base_manifest: ArenaScopedId,
    target_manifest: ArenaScopedId,
    base_source_revision: SourceRevision,
    base_source_root: SourceRootId,
    target_source_revision: SourceRevision,
    target_source_root: SourceRootId,
    grammar_revision: GrammarRevision,
    target_parse_generation: ParseGeneration,
    base_metric: SerializedMetric,
    target_metric: SerializedMetric,
    common_prefix_metric: SerializedMetric,
    old_changed_metric: SerializedMetric,
    new_changed_metric: SerializedMetric,
    common_suffix_metric: SerializedMetric,
    common_prefix_leaves: u64,
    old_changed_leaves: u64,
    new_changed_leaves: u64,
    common_suffix_leaves: u64,
    first_retained_leaf: ArenaId,
    last_retained_leaf: ArenaId,
}

impl GreenSuffixLeafSpliceDraft {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_green_suffix_join(
        _mint: crate::serialized_green::HostGreenPrefixSpliceMint,
        base_manifest: ArenaScopedId,
        target_manifest: ArenaScopedId,
        base_source_revision: SourceRevision,
        base_source_root: SourceRootId,
        target_source_revision: SourceRevision,
        target_source_root: SourceRootId,
        grammar_revision: GrammarRevision,
        target_parse_generation: ParseGeneration,
        base_metric: SerializedMetric,
        target_metric: SerializedMetric,
        common_prefix_metric: SerializedMetric,
        old_changed_metric: SerializedMetric,
        new_changed_metric: SerializedMetric,
        common_suffix_metric: SerializedMetric,
        common_prefix_leaves: u64,
        old_changed_leaves: u64,
        new_changed_leaves: u64,
        common_suffix_leaves: u64,
        first_retained_leaf: ArenaId,
        last_retained_leaf: ArenaId,
    ) -> Result<Self, SerializedGreenError> {
        if base_manifest.arena() != target_manifest.arena()
            || base_source_revision == target_source_revision
            || base_source_root.0 == 0
            || target_source_root.0 == 0
            || grammar_revision.0 == 0
            || target_parse_generation.0 == 0
            || old_changed_leaves
                .checked_add(common_prefix_leaves)
                .and_then(|count| count.checked_add(common_suffix_leaves))
                .is_none_or(|count| count == 0)
            || new_changed_leaves
                .checked_add(common_prefix_leaves)
                .and_then(|count| count.checked_add(common_suffix_leaves))
                .is_none_or(|count| count == 0)
            || common_suffix_leaves == 0
                && (first_retained_leaf.index != 0 || last_retained_leaf.index != 0)
            || common_suffix_leaves != 0
                && first_retained_leaf == last_retained_leaf
                && common_suffix_leaves != 1
            || metric_sum3(
                common_prefix_metric,
                old_changed_metric,
                common_suffix_metric,
            ) != Some(base_metric)
            || metric_sum3(
                common_prefix_metric,
                new_changed_metric,
                common_suffix_metric,
            ) != Some(target_metric)
        {
            return Err(SerializedGreenError::Corrupt(
                "typed host splice has inconsistent range bindings",
            ));
        }
        Ok(Self {
            base_manifest,
            target_manifest,
            base_source_revision,
            base_source_root,
            target_source_revision,
            target_source_root,
            grammar_revision,
            target_parse_generation,
            base_metric,
            target_metric,
            common_prefix_metric,
            old_changed_metric,
            new_changed_metric,
            common_suffix_metric,
            common_prefix_leaves,
            old_changed_leaves,
            new_changed_leaves,
            common_suffix_leaves,
            first_retained_leaf,
            last_retained_leaf,
        })
    }

    /// Reclassifies the canonical retained prefix only after the actor's final
    /// matched-C transaction has validated all sibling results. A nonzero
    /// prefix is rechecked by logarithmic identity lookups; zero is an honest
    /// proof outcome when normalization touched the first retained leaf. No
    /// manifest scan discovers or widens the prefix.
    pub(crate) fn finalize_matched_canonical(
        self,
        _mint: crate::candidate_writer::ParentSelectedAdoptionSpliceMint,
        arena: &PageArena,
        target_build: ArenaBuildId,
        proof: CanonicalRetainedGreenPrefixProof,
    ) -> Result<TypedGreenLeafSplice, SerializedGreenError> {
        if self.common_prefix_leaves != 0
            || self.common_prefix_metric != SerializedMetric::default()
            || target_build != proof.target_build
            || self.base_manifest != proof.base_manifest
            || self.base_source_revision != proof.base_source_revision
            || self.base_source_root != proof.base_source_root
            || self.target_source_revision != proof.target_source_revision
            || self.target_source_root != proof.target_source_root
            || self.grammar_revision != proof.grammar_revision
            || self.target_parse_generation != proof.target_parse_generation
            || proof.common_prefix_leaves > self.old_changed_leaves
            || proof.common_prefix_leaves > self.new_changed_leaves
        {
            return Err(SerializedGreenError::Corrupt(
                "matched-C canonical prefix disagrees with the green suffix draft",
            ));
        }
        if proof.common_prefix_leaves == 0 {
            if proof.common_prefix_metric != SerializedMetric::default()
                || proof.last_common_leaf.is_some()
            {
                return Err(SerializedGreenError::Corrupt(
                    "matched-C zero prefix retained a metric or leaf witness",
                ));
            }
        } else {
            let common_last_index = proof.common_prefix_leaves - 1;
            let base_last = serialized_green_leaf_at_scoped_manifest(
                arena,
                self.base_manifest,
                common_last_index,
            )?
            .ok_or(SerializedGreenError::Corrupt(
                "matched-C canonical base prefix disappeared",
            ))?;
            let target_last = serialized_green_leaf_at_scoped_manifest(
                arena,
                self.target_manifest,
                common_last_index,
            )?
            .ok_or(SerializedGreenError::Corrupt(
                "matched-C canonical target prefix disappeared",
            ))?;
            if Some(base_last) != proof.last_common_leaf
                || Some(target_last) != proof.last_common_leaf
            {
                return Err(SerializedGreenError::Corrupt(
                    "matched-C canonical prefix identity changed before final authority",
                ));
            }
        }

        let old_changed_metric = metric_checked_sub(
            self.old_changed_metric,
            proof.common_prefix_metric,
            "matched-C canonical old changed metric",
        )?;
        let new_changed_metric = metric_checked_sub(
            self.new_changed_metric,
            proof.common_prefix_metric,
            "matched-C canonical new changed metric",
        )?;
        let old_changed_leaves = self
            .old_changed_leaves
            .checked_sub(proof.common_prefix_leaves)
            .ok_or(SerializedGreenError::Corrupt(
                "matched-C canonical old changed range underflowed",
            ))?;
        let new_changed_leaves = self
            .new_changed_leaves
            .checked_sub(proof.common_prefix_leaves)
            .ok_or(SerializedGreenError::Corrupt(
                "matched-C canonical new changed range underflowed",
            ))?;
        let finalized = Self {
            common_prefix_metric: proof.common_prefix_metric,
            old_changed_metric,
            new_changed_metric,
            common_prefix_leaves: proof.common_prefix_leaves,
            old_changed_leaves,
            new_changed_leaves,
            ..self
        };
        if metric_sum3(
            finalized.common_prefix_metric,
            finalized.old_changed_metric,
            finalized.common_suffix_metric,
        ) != Some(finalized.base_metric)
            || metric_sum3(
                finalized.common_prefix_metric,
                finalized.new_changed_metric,
                finalized.common_suffix_metric,
            ) != Some(finalized.target_metric)
        {
            return Err(SerializedGreenError::Corrupt(
                "matched-C canonical range reclassification changed document totals",
            ));
        }
        Ok(TypedGreenLeafSplice(finalized))
    }

    #[cfg(test)]
    pub(crate) fn into_zero_prefix_fixture_proof(self) -> TypedGreenLeafSplice {
        debug_assert_eq!(self.common_prefix_leaves, 0);
        debug_assert_eq!(self.common_prefix_metric, SerializedMetric::default());
        TypedGreenLeafSplice(self)
    }
}

/// Final exporter authority. Construction requires either the production
/// matched-C canonical join or an explicitly test-only zero-prefix fixture
/// seam.
#[derive(Debug)]
pub(crate) struct TypedGreenLeafSplice(GreenSuffixLeafSpliceDraft);

impl TypedGreenLeafSplice {
    #[cfg(test)]
    pub(crate) const fn range_counts_for_test(&self) -> (u64, u64, u64, u64) {
        let proof = &self.0;
        (
            proof.common_prefix_leaves,
            proof.old_changed_leaves,
            proof.new_changed_leaves,
            proof.common_suffix_leaves,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StructuralAck {
    pub(crate) session: PublicationSessionId,
    pub(crate) target: HostRevisionId,
    pub(crate) source_document_session: DocumentSessionId,
    pub(crate) structural_source_revision: SourceRevision,
    pub(crate) metric: SerializedMetric,
    pub(crate) leaf_count: u64,
    pub(crate) sequence_digest: ProtocolDigest,
    pub(crate) manifest_digest: ProtocolDigest,
    pub(crate) splice_receipt: HostMeasuredSpliceReceipt,
}

#[derive(Clone, Debug)]
struct MeasuredLeaf {
    id: HostObjectId,
    metric: SerializedMetric,
    closure_digest: ProtocolDigest,
    object: Arc<CopiedObject>,
    programs: Arc<[Arc<CopiedObject>]>,
    structural: CopiedGreenLeafSummary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MeasuredSummary {
    leaves: u64,
    metric: SerializedMetric,
    identity_digest: ProtocolDigest,
    content_digest: ProtocolDigest,
    structural_balance: i64,
    structural_minimum_prefix: i64,
    height: u16,
}

#[derive(Debug)]
enum MeasuredNode {
    Page {
        leaves: Arc<[MeasuredLeaf]>,
        summary: MeasuredSummary,
    },
    Branch {
        left: Arc<MeasuredNode>,
        right: Arc<MeasuredNode>,
        summary: MeasuredSummary,
    },
}

impl MeasuredNode {
    const fn summary(&self) -> MeasuredSummary {
        match self {
            Self::Page { summary, .. } | Self::Branch { summary, .. } => *summary,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HostMeasuredSpliceReceipt {
    pub(crate) tree_nodes_visited: usize,
    pub(crate) tree_nodes_allocated: usize,
    pub(crate) boundary_leaf_entries_copied: usize,
    pub(crate) inserted_leaf_entries: usize,
}

impl HostMeasuredSpliceReceipt {
    fn merge(&mut self, other: Self) -> Result<(), HostMirrorError> {
        self.tree_nodes_visited = self
            .tree_nodes_visited
            .checked_add(other.tree_nodes_visited)
            .ok_or(HostMirrorError::Invalid("tree visit receipt overflow"))?;
        self.tree_nodes_allocated = self
            .tree_nodes_allocated
            .checked_add(other.tree_nodes_allocated)
            .ok_or(HostMirrorError::Invalid("tree allocation receipt overflow"))?;
        self.boundary_leaf_entries_copied = self
            .boundary_leaf_entries_copied
            .checked_add(other.boundary_leaf_entries_copied)
            .ok_or(HostMirrorError::Invalid("boundary copy receipt overflow"))?;
        self.inserted_leaf_entries = self
            .inserted_leaf_entries
            .checked_add(other.inserted_leaf_entries)
            .ok_or(HostMirrorError::Invalid("inserted leaf receipt overflow"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct MeasuredLeafSequence {
    root: Option<Arc<MeasuredNode>>,
    measured_splices: u64,
    last_splice: HostMeasuredSpliceReceipt,
}

impl MeasuredLeafSequence {
    fn summary(&self) -> MeasuredSummary {
        self.root
            .as_deref()
            .map_or(MeasuredSummary::default(), MeasuredNode::summary)
    }

    fn range_summary(
        &self,
        start: u64,
        count: u64,
        receipt: &mut HostMeasuredSpliceReceipt,
    ) -> Result<MeasuredSummary, HostMirrorError> {
        let end = start
            .checked_add(count)
            .ok_or(HostMirrorError::Invalid("measured range overflow"))?;
        if end > self.summary().leaves {
            return Err(HostMirrorError::Invalid(
                "measured range escapes leaf sequence",
            ));
        }
        range_summary_node(self.root.as_ref(), start, count, receipt)
    }

    fn splice(
        &self,
        old_start: u64,
        old_delete: u64,
        inserted: Vec<MeasuredLeaf>,
    ) -> Result<Self, HostMirrorError> {
        let mut receipt = HostMeasuredSpliceReceipt {
            inserted_leaf_entries: inserted.len(),
            ..HostMeasuredSpliceReceipt::default()
        };
        let (prefix, rest) = split_measured(self.root.clone(), old_start, &mut receipt)?;
        let (deleted, suffix) = split_measured(rest, old_delete, &mut receipt)?;
        if deleted.as_deref().map_or(0, |node| node.summary().leaves) != old_delete {
            return Err(HostMirrorError::Invalid("measured delete range mismatch"));
        }
        let inserted = build_measured_pages(inserted, &mut receipt)?;
        let target = concat_measured(
            concat_measured(prefix, inserted, &mut receipt)?,
            suffix,
            &mut receipt,
        )?;
        Ok(Self {
            root: target,
            measured_splices: self
                .measured_splices
                .checked_add(1)
                .ok_or(HostMirrorError::Invalid("measured splice count overflow"))?,
            last_splice: receipt,
        })
    }

    fn locate_byte(
        &self,
        byte: u64,
    ) -> Result<(MeasuredLeaf, SerializedMetric, u64), HostMirrorError> {
        let root = self
            .root
            .as_ref()
            .ok_or(HostMirrorError::Invalid("empty measured sequence"))?;
        if byte >= root.summary().metric.bytes {
            return Err(HostMirrorError::Invalid(
                "byte query escapes measured sequence",
            ));
        }
        locate_measured_byte(root, byte, SerializedMetric::default(), 0)
    }

    fn viewport_context_before(&self, leaf_index: u64) -> Result<ViewportContext, HostMirrorError> {
        let root = self
            .root
            .as_ref()
            .ok_or(HostMirrorError::Invalid("empty measured sequence"))?;
        if leaf_index >= root.summary().leaves {
            return Err(HostMirrorError::Invalid(
                "viewport leaf index escapes measured sequence",
            ));
        }
        // The measured prefix balance is the exact open depth at this leaf
        // boundary. Check it by summary before allocating or reverse-walking
        // one frame per open block.
        let mut prefix_receipt = HostMeasuredSpliceReceipt::default();
        let prefix = range_summary_node(Some(root), 0, leaf_index, &mut prefix_receipt)?;
        if prefix.structural_minimum_prefix < 0 || prefix.structural_balance < 0 {
            return Err(HostMirrorError::Invalid(
                "viewport prefix is structurally invalid",
            ));
        }
        let open_depth = u64::try_from(prefix.structural_balance)
            .map_err(|_| HostMirrorError::Invalid("viewport depth exceeds u64"))?;
        if open_depth > HOST_MAX_VIEWPORT_OPEN_DEPTH {
            return Err(HostMirrorError::ViewportOpenDepthExceeded {
                observed: open_depth,
                maximum: HOST_MAX_VIEWPORT_OPEN_DEPTH,
            });
        }
        let open_capacity = usize::try_from(open_depth)
            .map_err(|_| HostMirrorError::Invalid("viewport depth exceeds usize"))?;
        let mut receipt = HostViewportReceipt {
            tree_nodes_visited: prefix_receipt.tree_nodes_visited,
            ..HostViewportReceipt::default()
        };
        let mut unmatched_exits = 0_u64;
        let mut inner_first = Vec::new();
        inner_first
            .try_reserve_exact(open_capacity)
            .map_err(|_| HostMirrorError::Invalid("viewport frame reservation failed"))?;
        scan_measured_prefix_reverse(
            root,
            leaf_index,
            &mut unmatched_exits,
            &mut inner_first,
            &mut receipt,
        )?;
        if unmatched_exits != 0 {
            return Err(HostMirrorError::Invalid(
                "viewport position follows unmatched structural Exit",
            ));
        }
        if inner_first.len() != open_capacity {
            return Err(HostMirrorError::Invalid(
                "viewport decoded depth disagrees with measured prefix",
            ));
        }
        inner_first.reverse();
        receipt.maximum_open_depth = receipt.maximum_open_depth.max(inner_first.len());
        let mut depth = i64::try_from(inner_first.len())
            .map_err(|_| HostMirrorError::Invalid("viewport depth exceeds i64"))?;
        scan_measured_suffix_for_close_facts(
            root,
            leaf_index,
            &mut depth,
            &mut inner_first,
            &mut receipt,
        )?;
        if inner_first
            .iter()
            .any(|frame| close_facts_required(frame.kind) && frame.close_facts.is_none())
        {
            return Err(HostMirrorError::Invalid(
                "viewport close-time facts are missing",
            ));
        }
        Ok(ViewportContext {
            open: inner_first,
            receipt,
        })
    }
}

#[derive(Clone, Debug)]
struct StructuralState {
    session: PublicationSessionId,
    revision: HostRevisionId,
    source_revision: SourceRevision,
    parse_generation: ParseGeneration,
    grammar_revision: GrammarRevision,
    source_hash: SourceContentHash128,
    sequence: MeasuredLeafSequence,
    sequence_digest: ProtocolDigest,
    manifest_digest: ProtocolDigest,
}

#[derive(Clone, Debug)]
pub(crate) struct StructuralLeafQuery {
    pub(crate) id: HostObjectId,
    pub(crate) prefix: SerializedMetric,
    pub(crate) metric: SerializedMetric,
    pub(crate) object: Arc<CopiedObject>,
    /// Ordered closure for every Program child referenced by `object`. The
    /// query remains independently decodable after worker-root retirement.
    pub(crate) programs: Arc<[Arc<CopiedObject>]>,
    pub(crate) viewport_context: ViewportContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewportOpenFrame {
    pub(crate) block: crate::BlockId,
    pub(crate) kind: GreenKind,
    pub(crate) open_facts: FactsEnvelope,
    /// Close-time facts are populated for kinds whose rendering depends on
    /// their matching Exit (currently List and FencedCode).
    pub(crate) close_facts: Option<GreenCloseFacts>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HostViewportReceipt {
    pub(crate) tree_nodes_visited: usize,
    pub(crate) summary_nodes_skipped: usize,
    pub(crate) leaf_pages_decoded: usize,
    pub(crate) structural_events_decoded: usize,
    pub(crate) maximum_open_depth: usize,
    pub(crate) maximum_decoded_page_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewportContext {
    pub(crate) open: Vec<ViewportOpenFrame>,
    pub(crate) receipt: HostViewportReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceGapQuery {
    pub(crate) document_session: DocumentSessionId,
    pub(crate) source_revision: SourceRevision,
    pub(crate) source_hash: SourceContentHash128,
    pub(crate) range: MetricRange,
    pub(crate) reason: SourceGapReason,
    /// Caret movement, selection, and source editing remain live.
    pub(crate) source_editable: bool,
    /// Markdown-derived actions (links, task toggles, references) are disabled
    /// until an exact-current structural bundle replaces this gap.
    pub(crate) semantic_actions_valid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceGapReason {
    StructuralLag,
    ViewportOpenDepthExceeded { observed: u64, maximum: u64 },
}

#[derive(Clone, Debug)]
pub(crate) enum HostQuery {
    Structural(StructuralLeafQuery),
    SourceGap(SourceGapQuery),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostMetricAffinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Debug)]
pub(crate) struct HostMirror {
    current_source: SourceVersion,
    dirty: Option<DirtyOverlay>,
    structural: Option<StructuralState>,
    unacknowledged: Option<StructuralAck>,
}

struct StagedBundleObjects {
    inserted: Vec<MeasuredLeaf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostMirrorError {
    Invalid(&'static str),
    Backpressure,
    SessionSnapshotRequired,
    BaseMismatch,
    SourceAheadNeedsRetainedLineage {
        current: SourceRevision,
        offered: SourceRevision,
    },
    ObjectConflict(HostObjectId),
    MissingObject(HostObjectId),
    WrongObjectKind(HostObjectId),
    ViewportOpenDepthExceeded {
        observed: u64,
        maximum: u64,
    },
    CorruptGreen(SerializedGreenError),
}

impl From<SerializedGreenError> for HostMirrorError {
    fn from(error: SerializedGreenError) -> Self {
        Self::CorruptGreen(error)
    }
}

impl HostMirror {
    pub(crate) fn new(current_source: SourceVersion) -> Self {
        Self {
            current_source,
            dirty: None,
            structural: None,
            unacknowledged: None,
        }
    }

    /// Installs exact source immediately while leaving the structural leaf
    /// sequence untouched. The host retains only one conservative boundary:
    /// the prefix before it is coordinate-identical to the structural tree and
    /// the remainder is exact-source fallback. Exact-current-only publication
    /// means no transition-chain rebase is needed on the UI isolate.
    pub(crate) fn observe_source_edit(
        &mut self,
        target: SourceVersion,
        edits: Vec<SourceLineageEdit>,
    ) -> Result<(), HostMirrorError> {
        if target.revision <= self.current_source.revision
            || target.document_session != self.current_source.document_session
        {
            return Err(HostMirrorError::Invalid(
                "source edit does not extend this document lineage",
            ));
        }
        validate_source_lineage_edits(&edits, self.current_source.metric, target.metric)?;
        let first = edits
            .first()
            .ok_or(HostMirrorError::Invalid("source edit has no changed range"))?;
        let prior = self.dirty.take();
        let structural_base_revision = prior
            .as_ref()
            .map_or(self.current_source.revision, |dirty| {
                dirty.structural_base_revision
            });

        // If this edit begins inside the already-dirty suffix, the prior
        // boundary remains a valid prefix certificate. Otherwise the honest
        // first gate falls back to BOF: a nearby leaf boundary alone is not a
        // parser-safe invalidation boundary (Setext and list tightness can
        // affect earlier structure). A later production slice may replace BOF
        // with parser-authored sparse `earliest_may_change` metadata copied
        // into the structural snapshot. No dirty coordinate is ever rebased.
        let damage_start = match prior.as_ref() {
            Some(dirty) if metric_at_or_after(first.base.start, dirty.damage_start) => {
                dirty.damage_start
            }
            _ => SerializedMetric::default(),
        };
        self.current_source = target;
        self.dirty = Some(DirtyOverlay {
            structural_base_revision,
            source_target_revision: target.revision,
            damage_start,
        });
        Ok(())
    }

    pub(crate) fn apply_bundle(
        &mut self,
        bundle: StructuralBundle,
    ) -> Result<StructuralAck, HostMirrorError> {
        if let Some(pending) = self.unacknowledged {
            // A fresh publication session plus a full snapshot is the explicit
            // lost-ACK recovery protocol. Keep the old pending ACK and root in
            // place while the replacement validates; commit below replaces
            // both atomically. Same-session offers remain backpressured.
            if bundle.base.is_some() || bundle.session == pending.session {
                return Err(HostMirrorError::Backpressure);
            }
        }
        if bundle.schema != HOST_BUNDLE_SCHEMA || bundle.target.0 == 0 {
            return Err(HostMirrorError::Invalid(
                "unsupported structural bundle schema",
            ));
        }
        if bundle.structural_source_revision < self.current_source.revision {
            return Err(HostMirrorError::SourceAheadNeedsRetainedLineage {
                current: self.current_source.revision,
                offered: bundle.structural_source_revision,
            });
        }
        if bundle.structural_source_revision != self.current_source.revision
            || bundle.source_document_session != self.current_source.document_session
            || bundle.source_hash != self.current_source.hash
            || bundle.target_metric != self.current_source.metric
        {
            return Err(HostMirrorError::Invalid(
                "bundle does not bind the exact current source",
            ));
        }
        let snapshot = bundle.base.is_none();
        let base_sequence = match &self.structural {
            None => {
                if bundle.base.is_some()
                    || bundle.base_manifest_digest != ProtocolDigest::default()
                    || bundle.splice.old_start != 0
                    || bundle.splice.old_delete != 0
                {
                    return Err(HostMirrorError::BaseMismatch);
                }
                MeasuredLeafSequence::default()
            }
            Some(current) => {
                if snapshot {
                    if bundle.base_manifest_digest != ProtocolDigest::default()
                        || bundle.splice.old_start != 0
                        || bundle.splice.old_delete != 0
                        || (bundle.session == current.session && bundle.target <= current.revision)
                    {
                        return Err(HostMirrorError::BaseMismatch);
                    }
                    MeasuredLeafSequence::default()
                } else {
                    if current.session != bundle.session {
                        return Err(HostMirrorError::SessionSnapshotRequired);
                    }
                    if bundle.base != Some(current.revision)
                        || bundle.target <= current.revision
                        || bundle.base_manifest_digest != current.manifest_digest
                    {
                        return Err(HostMirrorError::BaseMismatch);
                    }
                    current.sequence.clone()
                }
            }
        };

        let mut range_receipt = HostMeasuredSpliceReceipt::default();
        let deleted = base_sequence.range_summary(
            bundle.splice.old_start,
            bundle.splice.old_delete,
            &mut range_receipt,
        )?;
        if deleted.identity_digest != bundle.splice.deleted_id_digest
            || deleted.content_digest != bundle.splice.deleted_content_digest
        {
            return Err(HostMirrorError::BaseMismatch);
        }

        let StagedBundleObjects { inserted } = self.stage_bundle_objects(&bundle)?;
        let mut sequence =
            base_sequence.splice(bundle.splice.old_start, bundle.splice.old_delete, inserted)?;
        sequence.last_splice.merge(range_receipt)?;
        let summary = sequence.summary();
        let leaf_count = summary.leaves;
        if summary.metric != bundle.target_metric || leaf_count != bundle.target_leaf_count {
            return Err(HostMirrorError::Invalid(
                "measured splice disagrees with target totals",
            ));
        }
        let sequence_digest = summary.content_digest;
        let manifest_digest = manifest_digest(
            bundle.session,
            bundle.source_document_session,
            bundle.structural_source_revision,
            bundle.parse_generation,
            bundle.grammar_revision,
            bundle.source_hash,
            bundle.target_metric,
            leaf_count,
            sequence_digest,
        );
        let splice_receipt = sequence.last_splice;
        self.structural = Some(StructuralState {
            session: bundle.session,
            revision: bundle.target,
            source_revision: bundle.structural_source_revision,
            parse_generation: bundle.parse_generation,
            grammar_revision: bundle.grammar_revision,
            source_hash: bundle.source_hash,
            sequence,
            sequence_digest,
            manifest_digest,
        });
        self.dirty = None;
        let ack = StructuralAck {
            session: bundle.session,
            target: bundle.target,
            source_document_session: bundle.source_document_session,
            structural_source_revision: bundle.structural_source_revision,
            metric: bundle.target_metric,
            leaf_count,
            sequence_digest,
            manifest_digest,
            splice_receipt,
        };
        self.unacknowledged = Some(ack);
        Ok(ack)
    }

    fn stage_bundle_objects(
        &self,
        bundle: &StructuralBundle,
    ) -> Result<StagedBundleObjects, HostMirrorError> {
        if bundle.objects.len() > MAX_BUNDLE_OBJECTS {
            return Err(HostMirrorError::Invalid("bundle object limit exceeded"));
        }
        let payload_bytes = bundle.objects.iter().try_fold(0_usize, |total, envelope| {
            total
                .checked_add(envelope.object.payload().len())
                .ok_or(HostMirrorError::Invalid(
                    "bundle payload byte count overflow",
                ))
        })?;
        if payload_bytes > MAX_BUNDLE_PAYLOAD_BYTES {
            return Err(HostMirrorError::Invalid(
                "bundle payload byte limit exceeded",
            ));
        }

        let mut objects: BTreeMap<HostObjectId, Arc<CopiedObject>> = BTreeMap::new();
        let mut offered = BTreeSet::new();
        for envelope in &bundle.objects {
            if envelope.id.session != bundle.session
                || envelope.object.payload().len() > ARENA_PAGE_BYTES
                || !offered.insert(envelope.id)
                || copied_object_digest(envelope.id, &envelope.object) != envelope.content_digest
            {
                return Err(HostMirrorError::Invalid(
                    "duplicate, oversized, or cross-session copied object",
                ));
            }
            if let Some(existing) = objects.get(&envelope.id) {
                debug_assert_eq!(existing.as_ref(), &envelope.object);
            } else {
                let object = Arc::new(envelope.object.clone());
                objects.insert(envelope.id, object);
            }
        }

        let mut reachable = BTreeSet::new();
        let mut inserted = Vec::new();
        inserted
            .try_reserve_exact(bundle.splice.inserted.len())
            .map_err(|_| HostMirrorError::Invalid("inserted leaf reservation failed"))?;
        for id in &bundle.splice.inserted {
            if id.session != bundle.session {
                return Err(HostMirrorError::Invalid("cross-session inserted leaf"));
            }
            if !offered.contains(id) {
                return Err(HostMirrorError::MissingObject(*id));
            }
            let object = objects.get(id).ok_or(HostMirrorError::MissingObject(*id))?;
            let CopiedObject::GreenLeaf { payload, children } = object.as_ref() else {
                return Err(HostMirrorError::WrongObjectKind(*id));
            };
            if children.len() > MAX_PACKED_ARENA_CHILDREN {
                return Err(HostMirrorError::Invalid("leaf child limit exceeded"));
            }
            reachable.insert(*id);
            let mut program_payloads = Vec::new();
            let mut program_objects = Vec::new();
            program_payloads
                .try_reserve_exact(children.len())
                .map_err(|_| HostMirrorError::Invalid("Program closure reservation failed"))?;
            for child in children.iter() {
                if !offered.contains(child) {
                    return Err(HostMirrorError::MissingObject(*child));
                }
                let program = objects
                    .get(child)
                    .ok_or(HostMirrorError::MissingObject(*child))?;
                let CopiedObject::ProjectionProgram { payload } = program.as_ref() else {
                    return Err(HostMirrorError::WrongObjectKind(*child));
                };
                reachable.insert(*child);
                program_payloads.push(payload.as_ref());
                program_objects.push(program.clone());
            }
            let decoded = validate_copied_green_leaf_closure(payload.as_ref(), &program_payloads)?;
            let closure_digest = leaf_closure_digest(*id, payload, children, &program_objects)?;
            inserted.push(MeasuredLeaf {
                id: *id,
                metric: decoded.summary.metric,
                closure_digest,
                object: object.clone(),
                programs: Arc::from(program_objects),
                structural: decoded.summary,
            });
        }
        if offered.iter().any(|id| !reachable.contains(id)) {
            return Err(HostMirrorError::Invalid(
                "bundle contains an object outside inserted leaf closure",
            ));
        }
        Ok(StagedBundleObjects { inserted })
    }

    pub(crate) fn acknowledge_delivery(
        &mut self,
        ack: StructuralAck,
    ) -> Result<(), HostMirrorError> {
        if self.unacknowledged != Some(ack) {
            return Err(HostMirrorError::Invalid(
                "ack does not match pending bundle",
            ));
        }
        self.unacknowledged = None;
        Ok(())
    }

    pub(crate) fn query_metric(
        &self,
        position: SerializedMetric,
    ) -> Result<HostQuery, HostMirrorError> {
        self.query_metric_with_affinity(position, HostMetricAffinity::Downstream)
    }

    pub(crate) fn query_metric_with_affinity(
        &self,
        position: SerializedMetric,
        affinity: HostMetricAffinity,
    ) -> Result<HostQuery, HostMirrorError> {
        if position.bytes > self.current_source.metric.bytes
            || position.utf16 > self.current_source.metric.utf16
        {
            return Err(HostMirrorError::Invalid("metric query escapes source"));
        }
        if let Some(dirty) = self
            .dirty
            .as_ref()
            .filter(|dirty| metric_at_or_after(position, dirty.damage_start))
        {
            return Ok(HostQuery::SourceGap(SourceGapQuery {
                document_session: self.current_source.document_session,
                source_revision: self.current_source.revision,
                source_hash: self.current_source.hash,
                range: MetricRange {
                    start: dirty.damage_start,
                    end: self.current_source.metric,
                },
                reason: SourceGapReason::StructuralLag,
                source_editable: true,
                semantic_actions_valid: false,
            }));
        }

        let probe_byte = if position == self.current_source.metric {
            match affinity {
                HostMetricAffinity::Upstream if self.current_source.metric.bytes != 0 => {
                    self.current_source.metric.bytes - 1
                }
                HostMetricAffinity::Upstream | HostMetricAffinity::Downstream => {
                    return Err(HostMirrorError::Invalid(
                        "clean EOF query needs a nonempty upstream leaf",
                    ));
                }
            }
        } else {
            if position.bytes >= self.current_source.metric.bytes
                || position.utf16 >= self.current_source.metric.utf16
            {
                return Err(HostMirrorError::Invalid(
                    "metric query is not a complete source coordinate",
                ));
            }
            position.bytes
        };

        let structural = self
            .structural
            .as_ref()
            .ok_or(HostMirrorError::Invalid("no structural snapshot installed"))?;
        if self
            .dirty
            .as_ref()
            .is_some_and(|dirty| structural.source_revision != dirty.structural_base_revision)
        {
            return Err(HostMirrorError::Invalid(
                "dirty prefix does not bind installed structural revision",
            ));
        }
        let (leaf, prefix, leaf_index) = structural.sequence.locate_byte(probe_byte)?;
        let viewport_context = match structural.sequence.viewport_context_before(leaf_index) {
            Ok(context) => context,
            Err(HostMirrorError::ViewportOpenDepthExceeded { observed, maximum }) => {
                return Ok(HostQuery::SourceGap(SourceGapQuery {
                    document_session: self.current_source.document_session,
                    source_revision: self.current_source.revision,
                    source_hash: self.current_source.hash,
                    range: MetricRange {
                        start: SerializedMetric::default(),
                        end: self.current_source.metric,
                    },
                    reason: SourceGapReason::ViewportOpenDepthExceeded { observed, maximum },
                    source_editable: true,
                    semantic_actions_valid: false,
                }));
            }
            Err(error) => return Err(error),
        };
        Ok(HostQuery::Structural(StructuralLeafQuery {
            id: leaf.id,
            prefix,
            metric: leaf.metric,
            object: leaf.object,
            programs: leaf.programs,
            viewport_context,
        }))
    }
}

pub(crate) fn prepare_full_snapshot_bundle(
    arena: &PageArena,
    document: &SerializedGreenDocument,
    session: PublicationSessionId,
    target: HostRevisionId,
    source: SourceVersion,
) -> Result<StructuralBundle, HostMirrorError> {
    let descriptor = document.manifest_descriptor(arena)?;
    if descriptor.source_revision != source.revision
        || descriptor.source_bytes != source.metric.bytes
        || descriptor.source_utf16 != source.metric.utf16
    {
        return Err(HostMirrorError::Invalid(
            "full snapshot source disagrees with green manifest",
        ));
    }
    let leaf_count = document.leaf_count(arena)?;
    let mut inserted = Vec::new();
    let mut inserted_content = Vec::new();
    let mut objects = BTreeMap::new();
    for index in 0..leaf_count {
        let leaf = document
            .leaf_at(arena, index)?
            .ok_or(HostMirrorError::Invalid("full snapshot leaf disappeared"))?;
        let id = HostObjectId::from_arena(session, leaf);
        inserted.push(id);
        let closure = copy_leaf_closure(arena, session, leaf, &mut objects)?;
        inserted_content.push((id, closure));
    }
    let _sequence_digest = sequence_content_digest(inserted_content.iter().copied());
    Ok(StructuralBundle {
        schema: HOST_BUNDLE_SCHEMA,
        session,
        base: None,
        target,
        source_document_session: source.document_session,
        structural_source_revision: descriptor.source_revision,
        parse_generation: descriptor.parse_generation,
        grammar_revision: descriptor.grammar_revision,
        source_hash: source.hash,
        target_metric: source.metric,
        target_leaf_count: leaf_count,
        base_manifest_digest: ProtocolDigest::default(),
        splice: LeafSplice {
            old_start: 0,
            old_delete: 0,
            inserted,
            deleted_id_digest: ProtocolDigest::default(),
            deleted_content_digest: ProtocolDigest::default(),
        },
        objects: objects.into_values().collect(),
    })
}

/// Full snapshot for a green child already owned by a published composite
/// parent. The composite view supplies the typed descriptor; no child owner or
/// caller-invented manifest/leaf coordinates escape the actor.
pub(crate) fn prepare_full_snapshot_bundle_from_composite(
    arena: &PageArena,
    descriptor: SerializedGreenCompositeDescriptor,
    session: PublicationSessionId,
    target: HostRevisionId,
    source: SourceVersion,
) -> Result<StructuralBundle, HostMirrorError> {
    if descriptor.source_revision() != source.revision
        || descriptor.source_metric() != source.metric
    {
        return Err(HostMirrorError::Invalid(
            "composite full snapshot source disagrees with green manifest",
        ));
    }
    let manifest = descriptor.scoped_manifest_for_host_snapshot(arena)?;
    let leaf_count = descriptor.leaf_pages();
    let mut inserted = Vec::new();
    let mut inserted_content = Vec::new();
    let mut objects = BTreeMap::new();
    for index in 0..leaf_count {
        let leaf = serialized_green_leaf_at_scoped_manifest(arena, manifest, index)?.ok_or(
            HostMirrorError::Invalid("composite full snapshot leaf disappeared"),
        )?;
        let id = HostObjectId::from_arena(session, leaf);
        inserted.push(id);
        let closure = copy_leaf_closure(arena, session, leaf, &mut objects)?;
        inserted_content.push((id, closure));
    }
    let _sequence_digest = sequence_content_digest(inserted_content.iter().copied());
    Ok(StructuralBundle {
        schema: HOST_BUNDLE_SCHEMA,
        session,
        base: None,
        target,
        source_document_session: source.document_session,
        structural_source_revision: descriptor.source_revision(),
        parse_generation: descriptor.parse_generation(),
        grammar_revision: descriptor.grammar_revision(),
        source_hash: source.hash,
        target_metric: source.metric,
        target_leaf_count: leaf_count,
        base_manifest_digest: ProtocolDigest::default(),
        splice: LeafSplice {
            old_start: 0,
            old_delete: 0,
            inserted,
            deleted_id_digest: ProtocolDigest::default(),
            deleted_content_digest: ProtocolDigest::default(),
        },
        objects: objects.into_values().collect(),
    })
}

pub(crate) fn prepare_typed_leaf_delta_bundle(
    arena: &PageArena,
    proof: &TypedGreenLeafSplice,
    base: StructuralAck,
    target: HostRevisionId,
    session: PublicationSessionId,
    source: SourceVersion,
) -> Result<StructuralBundle, HostMirrorError> {
    let proof = &proof.0;
    if base.session != session
        || base.structural_source_revision != proof.base_source_revision
        || base.source_document_session != source.document_session
        || base.metric != proof.base_metric
        || base.leaf_count
            != proof
                .common_prefix_leaves
                .checked_add(proof.old_changed_leaves)
                .and_then(|count| count.checked_add(proof.common_suffix_leaves))
                .ok_or(HostMirrorError::Invalid("base proof leaf count overflow"))?
        || source.revision != proof.target_source_revision
        || source.metric != proof.target_metric
    {
        return Err(HostMirrorError::BaseMismatch);
    }

    if proof.common_suffix_leaves != 0 {
        let base_suffix_start = proof
            .common_prefix_leaves
            .checked_add(proof.old_changed_leaves)
            .ok_or(HostMirrorError::Invalid("base suffix start overflow"))?;
        let target_suffix_start = proof
            .common_prefix_leaves
            .checked_add(proof.new_changed_leaves)
            .ok_or(HostMirrorError::Invalid("target suffix start overflow"))?;
        let base_first_suffix = serialized_green_leaf_at_scoped_manifest(
            arena,
            proof.base_manifest,
            base_suffix_start,
        )?
        .ok_or(HostMirrorError::Invalid("base retained suffix disappeared"))?;
        let target_first_suffix = serialized_green_leaf_at_scoped_manifest(
            arena,
            proof.target_manifest,
            target_suffix_start,
        )?
        .ok_or(HostMirrorError::Invalid(
            "target retained suffix disappeared",
        ))?;
        let target_last = target_suffix_start
            .checked_add(proof.common_suffix_leaves)
            .and_then(|count| count.checked_sub(1))
            .ok_or(HostMirrorError::Invalid(
                "target retained suffix count overflow",
            ))?;
        let target_last_suffix =
            serialized_green_leaf_at_scoped_manifest(arena, proof.target_manifest, target_last)?
                .ok_or(HostMirrorError::Invalid(
                    "target last suffix leaf disappeared",
                ))?;
        if base_first_suffix != proof.first_retained_leaf
            || target_first_suffix != proof.first_retained_leaf
            || target_last_suffix != proof.last_retained_leaf
        {
            return Err(HostMirrorError::Invalid(
                "typed retained suffix sentinels changed before export",
            ));
        }
    }

    // These loops enumerate only the typed changed ranges. They do not compare
    // manifests and do not inspect the retained suffix to discover a splice.
    let mut deleted = Vec::new();
    let mut deleted_content = Vec::new();
    let old_changed_end = proof
        .common_prefix_leaves
        .checked_add(proof.old_changed_leaves)
        .ok_or(HostMirrorError::Invalid("old changed range overflow"))?;
    for index in proof.common_prefix_leaves..old_changed_end {
        let leaf = serialized_green_leaf_at_scoped_manifest(arena, proof.base_manifest, index)?
            .ok_or(HostMirrorError::Invalid("typed deleted leaf disappeared"))?;
        let id = HostObjectId::from_arena(session, leaf);
        deleted.push(id);
        deleted_content.push((id, digest_leaf_closure_in_arena(arena, session, leaf)?));
    }
    let mut inserted = Vec::new();
    let mut inserted_content = Vec::new();
    let mut objects = BTreeMap::new();
    let new_changed_end = proof
        .common_prefix_leaves
        .checked_add(proof.new_changed_leaves)
        .ok_or(HostMirrorError::Invalid("new changed range overflow"))?;
    for index in proof.common_prefix_leaves..new_changed_end {
        let leaf = serialized_green_leaf_at_scoped_manifest(arena, proof.target_manifest, index)?
            .ok_or(HostMirrorError::Invalid("typed inserted leaf disappeared"))?;
        let id = HostObjectId::from_arena(session, leaf);
        inserted.push(id);
        let closure = copy_leaf_closure(arena, session, leaf, &mut objects)?;
        inserted_content.push((id, closure));
    }
    let deleted_id_digest = sequence_identity_digest(deleted.iter().copied());
    let deleted_content_digest = sequence_content_digest(deleted_content.iter().copied());
    let _inserted_digest = sequence_content_digest(inserted_content.iter().copied());
    let target_leaf_count = proof
        .common_prefix_leaves
        .checked_add(proof.new_changed_leaves)
        .and_then(|count| count.checked_add(proof.common_suffix_leaves))
        .ok_or(HostMirrorError::Invalid("target proof leaf count overflow"))?;
    Ok(StructuralBundle {
        schema: HOST_BUNDLE_SCHEMA,
        session,
        base: Some(base.target),
        target,
        source_document_session: source.document_session,
        structural_source_revision: proof.target_source_revision,
        parse_generation: proof.target_parse_generation,
        grammar_revision: proof.grammar_revision,
        source_hash: source.hash,
        target_metric: proof.target_metric,
        target_leaf_count,
        base_manifest_digest: base.manifest_digest,
        splice: LeafSplice {
            old_start: proof.common_prefix_leaves,
            old_delete: proof.old_changed_leaves,
            inserted,
            deleted_id_digest,
            deleted_content_digest,
        },
        objects: objects.into_values().collect(),
    })
}

fn copy_leaf_closure(
    arena: &PageArena,
    session: PublicationSessionId,
    leaf: ArenaId,
    output: &mut BTreeMap<HostObjectId, CopiedObjectEnvelope>,
) -> Result<ProtocolDigest, HostMirrorError> {
    let child_count = arena
        .packed_child_count(leaf)
        .map_err(SerializedGreenError::from)?;
    if child_count > MAX_PACKED_ARENA_CHILDREN {
        return Err(HostMirrorError::Invalid("worker leaf child limit exceeded"));
    }
    let mut children = Vec::new();
    let mut program_digests = Vec::new();
    children
        .try_reserve_exact(child_count)
        .map_err(|_| HostMirrorError::Invalid("copied child reservation failed"))?;
    for index in 0..child_count {
        let child = arena
            .packed_child_at(leaf, index)
            .map_err(SerializedGreenError::from)?;
        if arena
            .packed_child_count(child)
            .map_err(SerializedGreenError::from)?
            != 0
        {
            return Err(HostMirrorError::Invalid(
                "projection Program child owns another page",
            ));
        }
        let id = HostObjectId::from_arena(session, child);
        let object = CopiedObject::ProjectionProgram {
            payload: Arc::from(arena.payload(child).map_err(SerializedGreenError::from)?),
        };
        let content_digest = copied_object_digest(id, &object);
        let envelope = CopiedObjectEnvelope {
            id,
            object,
            content_digest,
        };
        insert_worker_object(output, envelope)?;
        children.push(id);
        program_digests.push(content_digest);
    }
    let id = HostObjectId::from_arena(session, leaf);
    let object = CopiedObject::GreenLeaf {
        payload: Arc::from(arena.payload(leaf).map_err(SerializedGreenError::from)?),
        children: Arc::from(children),
    };
    let leaf_digest = copied_object_digest(id, &object);
    insert_worker_object(
        output,
        CopiedObjectEnvelope {
            id,
            object,
            content_digest: leaf_digest,
        },
    )?;
    Ok(leaf_closure_digest_from_object_digests(
        id,
        leaf_digest,
        &program_digests,
    ))
}

fn digest_leaf_closure_in_arena(
    arena: &PageArena,
    session: PublicationSessionId,
    leaf: ArenaId,
) -> Result<ProtocolDigest, HostMirrorError> {
    let mut closure = BTreeMap::new();
    copy_leaf_closure(arena, session, leaf, &mut closure)
}

fn insert_worker_object(
    output: &mut BTreeMap<HostObjectId, CopiedObjectEnvelope>,
    envelope: CopiedObjectEnvelope,
) -> Result<(), HostMirrorError> {
    if let Some(existing) = output.get(&envelope.id) {
        if existing != &envelope {
            return Err(HostMirrorError::ObjectConflict(envelope.id));
        }
    } else {
        output.insert(envelope.id, envelope);
    }
    Ok(())
}

fn measured_page_summary(leaves: &[MeasuredLeaf]) -> Result<MeasuredSummary, HostMirrorError> {
    let mut summary = MeasuredSummary {
        height: 1,
        ..MeasuredSummary::default()
    };
    let mut power = 1_u128;
    for leaf in leaves {
        summary.structural_minimum_prefix = summary.structural_minimum_prefix.min(
            summary
                .structural_balance
                .checked_add(leaf.structural.minimum_prefix)
                .ok_or(HostMirrorError::Invalid("structural minimum overflow"))?,
        );
        summary.structural_balance = summary
            .structural_balance
            .checked_add(leaf.structural.balance)
            .ok_or(HostMirrorError::Invalid("structural balance overflow"))?;
        summary.leaves = summary
            .leaves
            .checked_add(1)
            .ok_or(HostMirrorError::Invalid("measured leaf count overflow"))?;
        summary.metric = checked_metric_add(summary.metric, leaf.metric)?;
        summary.identity_digest.0 = summary
            .identity_digest
            .0
            .wrapping_add(object_atom_digest(leaf.id).wrapping_mul(power));
        summary.content_digest.0 = summary.content_digest.0.wrapping_add(
            leaf_sequence_atom_digest(leaf.id, leaf.closure_digest).wrapping_mul(power),
        );
        power = power.wrapping_mul(SEQUENCE_POLYNOMIAL_BASE);
    }
    Ok(summary)
}

fn followed_summary(
    left: MeasuredSummary,
    right: MeasuredSummary,
) -> Result<MeasuredSummary, HostMirrorError> {
    if left.leaves == 0 {
        return Ok(right);
    }
    if right.leaves == 0 {
        return Ok(left);
    }
    let left_count = usize::try_from(left.leaves)
        .map_err(|_| HostMirrorError::Invalid("measured count exceeds usize"))?;
    let shift = wrapping_pow(SEQUENCE_POLYNOMIAL_BASE, left_count);
    Ok(MeasuredSummary {
        leaves: left
            .leaves
            .checked_add(right.leaves)
            .ok_or(HostMirrorError::Invalid("measured count overflow"))?,
        metric: checked_metric_add(left.metric, right.metric)?,
        identity_digest: ProtocolDigest(
            left.identity_digest
                .0
                .wrapping_add(shift.wrapping_mul(right.identity_digest.0)),
        ),
        content_digest: ProtocolDigest(
            left.content_digest
                .0
                .wrapping_add(shift.wrapping_mul(right.content_digest.0)),
        ),
        structural_balance: left
            .structural_balance
            .checked_add(right.structural_balance)
            .ok_or(HostMirrorError::Invalid("structural balance overflow"))?,
        structural_minimum_prefix: left.structural_minimum_prefix.min(
            left.structural_balance
                .checked_add(right.structural_minimum_prefix)
                .ok_or(HostMirrorError::Invalid("structural minimum overflow"))?,
        ),
        height: left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or(HostMirrorError::Invalid("measured tree height overflow"))?,
    })
}

fn make_measured_page(
    leaves: Vec<MeasuredLeaf>,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<Option<Arc<MeasuredNode>>, HostMirrorError> {
    if leaves.is_empty() {
        return Ok(None);
    }
    if leaves.len() > HOST_MEASURED_PAGE_LEAVES {
        return Err(HostMirrorError::Invalid(
            "measured page leaf limit exceeded",
        ));
    }
    let summary = measured_page_summary(&leaves)?;
    receipt.tree_nodes_allocated = receipt
        .tree_nodes_allocated
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("tree allocation receipt overflow"))?;
    Ok(Some(Arc::new(MeasuredNode::Page {
        leaves: Arc::from(leaves),
        summary,
    })))
}

fn make_measured_branch(
    left: Arc<MeasuredNode>,
    right: Arc<MeasuredNode>,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<Arc<MeasuredNode>, HostMirrorError> {
    let summary = followed_summary(left.summary(), right.summary())?;
    receipt.tree_nodes_allocated = receipt
        .tree_nodes_allocated
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("tree allocation receipt overflow"))?;
    Ok(Arc::new(MeasuredNode::Branch {
        left,
        right,
        summary,
    }))
}

fn concat_measured(
    left: Option<Arc<MeasuredNode>>,
    right: Option<Arc<MeasuredNode>>,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<Option<Arc<MeasuredNode>>, HostMirrorError> {
    let (left, right) = match (left, right) {
        (None, right) => return Ok(right),
        (left, None) => return Ok(left),
        (Some(left), Some(right)) => (left, right),
    };
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("tree visit receipt overflow"))?;
    let left_height = left.summary().height;
    let right_height = right.summary().height;
    if left_height > right_height.saturating_add(1) {
        let MeasuredNode::Branch {
            left: outer,
            right: inner,
            ..
        } = left.as_ref()
        else {
            return Err(HostMirrorError::Invalid("unbalanced measured page height"));
        };
        let joined = concat_measured(Some(inner.clone()), Some(right), receipt)?
            .ok_or(HostMirrorError::Invalid("measured join lost right side"))?;
        return Ok(Some(balance_measured_branch(
            outer.clone(),
            joined,
            receipt,
        )?));
    }
    if right_height > left_height.saturating_add(1) {
        let MeasuredNode::Branch {
            left: inner,
            right: outer,
            ..
        } = right.as_ref()
        else {
            return Err(HostMirrorError::Invalid("unbalanced measured page height"));
        };
        let joined = concat_measured(Some(left), Some(inner.clone()), receipt)?
            .ok_or(HostMirrorError::Invalid("measured join lost left side"))?;
        return Ok(Some(balance_measured_branch(
            joined,
            outer.clone(),
            receipt,
        )?));
    }
    Ok(Some(make_measured_branch(left, right, receipt)?))
}

fn balance_measured_branch(
    left: Arc<MeasuredNode>,
    right: Arc<MeasuredNode>,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<Arc<MeasuredNode>, HostMirrorError> {
    let left_height = left.summary().height;
    let right_height = right.summary().height;
    if left_height <= right_height.saturating_add(1)
        && right_height <= left_height.saturating_add(1)
    {
        return make_measured_branch(left, right, receipt);
    }
    if left_height > right_height.saturating_add(1) {
        let MeasuredNode::Branch {
            left: far,
            right: near,
            ..
        } = left.as_ref()
        else {
            return Err(HostMirrorError::Invalid("left-heavy measured page"));
        };
        if far.summary().height >= near.summary().height {
            let new_right = make_measured_branch(near.clone(), right, receipt)?;
            return make_measured_branch(far.clone(), new_right, receipt);
        }
        let MeasuredNode::Branch {
            left: near_left,
            right: near_right,
            ..
        } = near.as_ref()
        else {
            return Err(HostMirrorError::Invalid(
                "left double rotation lacks branch",
            ));
        };
        let new_left = make_measured_branch(far.clone(), near_left.clone(), receipt)?;
        let new_right = make_measured_branch(near_right.clone(), right, receipt)?;
        return make_measured_branch(new_left, new_right, receipt);
    }
    let MeasuredNode::Branch {
        left: near,
        right: far,
        ..
    } = right.as_ref()
    else {
        return Err(HostMirrorError::Invalid("right-heavy measured page"));
    };
    if far.summary().height >= near.summary().height {
        let new_left = make_measured_branch(left, near.clone(), receipt)?;
        return make_measured_branch(new_left, far.clone(), receipt);
    }
    let MeasuredNode::Branch {
        left: near_left,
        right: near_right,
        ..
    } = near.as_ref()
    else {
        return Err(HostMirrorError::Invalid(
            "right double rotation lacks branch",
        ));
    };
    let new_left = make_measured_branch(left, near_left.clone(), receipt)?;
    let new_right = make_measured_branch(near_right.clone(), far.clone(), receipt)?;
    make_measured_branch(new_left, new_right, receipt)
}

fn build_measured_pages(
    leaves: Vec<MeasuredLeaf>,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<Option<Arc<MeasuredNode>>, HostMirrorError> {
    if leaves.is_empty() {
        return Ok(None);
    }
    let mut pages = Vec::new();
    let mut pending = Vec::new();
    for leaf in leaves {
        pending.push(leaf);
        if pending.len() == HOST_MEASURED_PAGE_LEAVES {
            pages.push(
                make_measured_page(std::mem::take(&mut pending), receipt)?
                    .expect("nonempty measured page"),
            );
        }
    }
    if !pending.is_empty() {
        pages.push(make_measured_page(pending, receipt)?.expect("nonempty measured page"));
    }
    build_balanced_measured_nodes(pages, receipt).map(Some)
}

fn build_balanced_measured_nodes(
    mut nodes: Vec<Arc<MeasuredNode>>,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<Arc<MeasuredNode>, HostMirrorError> {
    if nodes.len() == 1 {
        return nodes.pop().ok_or(HostMirrorError::Invalid(
            "balanced builder lost measured node",
        ));
    }
    let right = nodes.split_off(nodes.len() / 2);
    let left = build_balanced_measured_nodes(nodes, receipt)?;
    let right = build_balanced_measured_nodes(right, receipt)?;
    make_measured_branch(left, right, receipt)
}

fn split_measured(
    node: Option<Arc<MeasuredNode>>,
    index: u64,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<(Option<Arc<MeasuredNode>>, Option<Arc<MeasuredNode>>), HostMirrorError> {
    let Some(node) = node else {
        if index == 0 {
            return Ok((None, None));
        }
        return Err(HostMirrorError::Invalid(
            "split escapes empty measured tree",
        ));
    };
    let count = node.summary().leaves;
    if index > count {
        return Err(HostMirrorError::Invalid("split escapes measured tree"));
    }
    if index == 0 {
        return Ok((None, Some(node)));
    }
    if index == count {
        return Ok((Some(node), None));
    }
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("tree visit receipt overflow"))?;
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            let index = usize::try_from(index)
                .map_err(|_| HostMirrorError::Invalid("page split exceeds usize"))?;
            receipt.boundary_leaf_entries_copied = receipt
                .boundary_leaf_entries_copied
                .checked_add(leaves.len())
                .ok_or(HostMirrorError::Invalid("boundary copy receipt overflow"))?;
            Ok((
                make_measured_page(leaves[..index].to_vec(), receipt)?,
                make_measured_page(leaves[index..].to_vec(), receipt)?,
            ))
        }
        MeasuredNode::Branch { left, right, .. } => {
            let left_count = left.summary().leaves;
            if index < left_count {
                let (prefix, inner) = split_measured(Some(left.clone()), index, receipt)?;
                Ok((
                    prefix,
                    concat_measured(inner, Some(right.clone()), receipt)?,
                ))
            } else if index == left_count {
                Ok((Some(left.clone()), Some(right.clone())))
            } else {
                let (inner, suffix) =
                    split_measured(Some(right.clone()), index - left_count, receipt)?;
                Ok((concat_measured(Some(left.clone()), inner, receipt)?, suffix))
            }
        }
    }
}

fn range_summary_node(
    node: Option<&Arc<MeasuredNode>>,
    start: u64,
    count: u64,
    receipt: &mut HostMeasuredSpliceReceipt,
) -> Result<MeasuredSummary, HostMirrorError> {
    if count == 0 {
        return Ok(MeasuredSummary::default());
    }
    let node = node.ok_or(HostMirrorError::Invalid("range escapes measured tree"))?;
    let summary = node.summary();
    if start == 0 && count == summary.leaves {
        return Ok(summary);
    }
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("tree visit receipt overflow"))?;
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            let start = usize::try_from(start)
                .map_err(|_| HostMirrorError::Invalid("range start exceeds usize"))?;
            let count = usize::try_from(count)
                .map_err(|_| HostMirrorError::Invalid("range count exceeds usize"))?;
            let end = start
                .checked_add(count)
                .ok_or(HostMirrorError::Invalid("range end overflow"))?;
            measured_page_summary(
                leaves
                    .get(start..end)
                    .ok_or(HostMirrorError::Invalid("range escapes measured page"))?,
            )
        }
        MeasuredNode::Branch { left, right, .. } => {
            let left_count = left.summary().leaves;
            if start >= left_count {
                range_summary_node(Some(right), start - left_count, count, receipt)
            } else {
                let in_left = count.min(left_count - start);
                let left_summary = range_summary_node(Some(left), start, in_left, receipt)?;
                let remaining = count - in_left;
                let right_summary = range_summary_node(Some(right), 0, remaining, receipt)?;
                followed_summary(left_summary, right_summary)
            }
        }
    }
}

fn locate_measured_byte(
    node: &Arc<MeasuredNode>,
    byte: u64,
    prefix: SerializedMetric,
    base_leaf_index: u64,
) -> Result<(MeasuredLeaf, SerializedMetric, u64), HostMirrorError> {
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            let mut prefix = prefix;
            for (offset, leaf) in leaves.iter().enumerate() {
                let end = prefix
                    .bytes
                    .checked_add(leaf.metric.bytes)
                    .ok_or(HostMirrorError::Invalid("query byte overflow"))?;
                if byte < end {
                    return Ok((
                        leaf.clone(),
                        prefix,
                        base_leaf_index
                            .checked_add(u64::try_from(offset).map_err(|_| {
                                HostMirrorError::Invalid("page leaf offset exceeds u64")
                            })?)
                            .ok_or(HostMirrorError::Invalid("leaf index overflow"))?,
                    ));
                }
                prefix = checked_metric_add(prefix, leaf.metric)?;
            }
            Err(HostMirrorError::Invalid("query escaped measured page"))
        }
        MeasuredNode::Branch { left, right, .. } => {
            let left_summary = left.summary();
            let left_end = prefix
                .bytes
                .checked_add(left_summary.metric.bytes)
                .ok_or(HostMirrorError::Invalid("query byte overflow"))?;
            if byte < left_end {
                locate_measured_byte(left, byte, prefix, base_leaf_index)
            } else {
                locate_measured_byte(
                    right,
                    byte,
                    checked_metric_add(prefix, left_summary.metric)?,
                    base_leaf_index
                        .checked_add(left_summary.leaves)
                        .ok_or(HostMirrorError::Invalid("leaf index overflow"))?,
                )
            }
        }
    }
}

fn structural_unmatched(balance: i64, minimum_prefix: i64) -> Result<(u64, u64), HostMirrorError> {
    if minimum_prefix > 0 {
        return Err(HostMirrorError::Invalid(
            "positive structural minimum prefix",
        ));
    }
    let closes = u64::try_from(minimum_prefix.saturating_neg())
        .map_err(|_| HostMirrorError::Invalid("negative structural close count"))?;
    let opens = balance
        .checked_add(
            i64::try_from(closes)
                .map_err(|_| HostMirrorError::Invalid("structural close count exceeds i64"))?,
        )
        .ok_or(HostMirrorError::Invalid("structural unmatched overflow"))?;
    Ok((
        u64::try_from(opens)
            .map_err(|_| HostMirrorError::Invalid("negative structural open count"))?,
        closes,
    ))
}

fn decode_measured_leaf_structure(
    leaf: &MeasuredLeaf,
    receipt: &mut HostViewportReceipt,
) -> Result<CopiedGreenLeafDecoded, HostMirrorError> {
    let CopiedObject::GreenLeaf { payload, children } = leaf.object.as_ref() else {
        return Err(HostMirrorError::WrongObjectKind(leaf.id));
    };
    if children.len() != leaf.programs.len() {
        return Err(HostMirrorError::Invalid(
            "retained leaf Program closure changed",
        ));
    }
    let mut program_payloads = Vec::new();
    program_payloads
        .try_reserve_exact(leaf.programs.len())
        .map_err(|_| HostMirrorError::Invalid("viewport Program reservation failed"))?;
    for program in leaf.programs.iter() {
        let CopiedObject::ProjectionProgram { payload } = program.as_ref() else {
            return Err(HostMirrorError::Invalid(
                "viewport Program closure has wrong object kind",
            ));
        };
        program_payloads.push(payload.as_ref());
    }
    let decoded = validate_copied_green_leaf_closure(payload, &program_payloads)?;
    if decoded.summary != leaf.structural {
        return Err(HostMirrorError::Invalid(
            "retained leaf structural summary changed",
        ));
    }
    receipt.leaf_pages_decoded = receipt
        .leaf_pages_decoded
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("viewport leaf receipt overflow"))?;
    receipt.structural_events_decoded = receipt
        .structural_events_decoded
        .checked_add(decoded.structural_events.len())
        .ok_or(HostMirrorError::Invalid("viewport event receipt overflow"))?;
    receipt.maximum_decoded_page_bytes = receipt.maximum_decoded_page_bytes.max(
        payload.len().saturating_add(
            decoded
                .structural_events
                .len()
                .saturating_mul(std::mem::size_of::<CopiedGreenStructuralEvent>()),
        ),
    );
    Ok(decoded)
}

fn scan_measured_prefix_reverse(
    node: &Arc<MeasuredNode>,
    take_leaves: u64,
    unmatched_exits: &mut u64,
    output: &mut Vec<ViewportOpenFrame>,
    receipt: &mut HostViewportReceipt,
) -> Result<(), HostMirrorError> {
    if take_leaves == 0 {
        return Ok(());
    }
    if take_leaves > node.summary().leaves {
        return Err(HostMirrorError::Invalid(
            "viewport prefix escapes measured tree",
        ));
    }
    if take_leaves == node.summary().leaves {
        return scan_measured_node_reverse(node, unmatched_exits, output, receipt);
    }
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("viewport node receipt overflow"))?;
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            let take = usize::try_from(take_leaves)
                .map_err(|_| HostMirrorError::Invalid("viewport prefix exceeds usize"))?;
            for leaf in leaves[..take].iter().rev() {
                scan_measured_leaf_reverse(leaf, unmatched_exits, output, receipt)?;
            }
            Ok(())
        }
        MeasuredNode::Branch { left, right, .. } => {
            let left_count = left.summary().leaves;
            if take_leaves <= left_count {
                scan_measured_prefix_reverse(left, take_leaves, unmatched_exits, output, receipt)
            } else {
                scan_measured_prefix_reverse(
                    right,
                    take_leaves - left_count,
                    unmatched_exits,
                    output,
                    receipt,
                )?;
                scan_measured_node_reverse(left, unmatched_exits, output, receipt)
            }
        }
    }
}

fn scan_measured_node_reverse(
    node: &Arc<MeasuredNode>,
    unmatched_exits: &mut u64,
    output: &mut Vec<ViewportOpenFrame>,
    receipt: &mut HostViewportReceipt,
) -> Result<(), HostMirrorError> {
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("viewport node receipt overflow"))?;
    let summary = node.summary();
    let (opens, closes) = structural_unmatched(
        summary.structural_balance,
        summary.structural_minimum_prefix,
    )?;
    if opens <= *unmatched_exits {
        *unmatched_exits = unmatched_exits
            .checked_sub(opens)
            .and_then(|remaining| remaining.checked_add(closes))
            .ok_or(HostMirrorError::Invalid(
                "reverse structural count overflow",
            ))?;
        receipt.summary_nodes_skipped = receipt
            .summary_nodes_skipped
            .checked_add(1)
            .ok_or(HostMirrorError::Invalid("viewport skip receipt overflow"))?;
        return Ok(());
    }
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            for leaf in leaves.iter().rev() {
                scan_measured_leaf_reverse(leaf, unmatched_exits, output, receipt)?;
            }
            Ok(())
        }
        MeasuredNode::Branch { left, right, .. } => {
            scan_measured_node_reverse(right, unmatched_exits, output, receipt)?;
            scan_measured_node_reverse(left, unmatched_exits, output, receipt)
        }
    }
}

fn scan_measured_leaf_reverse(
    leaf: &MeasuredLeaf,
    unmatched_exits: &mut u64,
    output: &mut Vec<ViewportOpenFrame>,
    receipt: &mut HostViewportReceipt,
) -> Result<(), HostMirrorError> {
    let (opens, closes) =
        structural_unmatched(leaf.structural.balance, leaf.structural.minimum_prefix)?;
    if opens <= *unmatched_exits {
        *unmatched_exits = unmatched_exits
            .checked_sub(opens)
            .and_then(|remaining| remaining.checked_add(closes))
            .ok_or(HostMirrorError::Invalid(
                "reverse structural count overflow",
            ))?;
        receipt.summary_nodes_skipped = receipt
            .summary_nodes_skipped
            .checked_add(1)
            .ok_or(HostMirrorError::Invalid("viewport skip receipt overflow"))?;
        return Ok(());
    }
    let decoded = decode_measured_leaf_structure(leaf, receipt)?;
    for event in decoded.structural_events.iter().rev() {
        match event {
            CopiedGreenStructuralEvent::Exit { .. } => {
                *unmatched_exits = unmatched_exits
                    .checked_add(1)
                    .ok_or(HostMirrorError::Invalid("reverse Exit count overflow"))?;
            }
            CopiedGreenStructuralEvent::Enter { block, kind, facts } => {
                if *unmatched_exits != 0 {
                    *unmatched_exits -= 1;
                } else {
                    let next_depth = u64::try_from(output.len())
                        .ok()
                        .and_then(|depth| depth.checked_add(1))
                        .ok_or(HostMirrorError::Invalid("viewport decoded depth overflow"))?;
                    if next_depth > HOST_MAX_VIEWPORT_OPEN_DEPTH {
                        return Err(HostMirrorError::ViewportOpenDepthExceeded {
                            observed: next_depth,
                            maximum: HOST_MAX_VIEWPORT_OPEN_DEPTH,
                        });
                    }
                    output.push(ViewportOpenFrame {
                        block: *block,
                        kind: *kind,
                        open_facts: facts.clone(),
                        close_facts: None,
                    });
                    receipt.maximum_open_depth = receipt.maximum_open_depth.max(output.len());
                }
            }
        }
    }
    Ok(())
}

const fn close_facts_required(kind: GreenKind) -> bool {
    matches!(kind, GreenKind::LIST | GreenKind::FENCED_CODE)
}

fn unresolved_close_may_occur(
    frames: &[ViewportOpenFrame],
    depth: i64,
    minimum_depth: i64,
) -> bool {
    frames.iter().enumerate().any(|(index, frame)| {
        if !close_facts_required(frame.kind) || frame.close_facts.is_some() {
            return false;
        }
        let Ok(frame_depth) = i64::try_from(index + 1) else {
            return true;
        };
        minimum_depth < frame_depth && frame_depth <= depth
    })
}

fn scan_measured_suffix_for_close_facts(
    node: &Arc<MeasuredNode>,
    skip_leaves: u64,
    depth: &mut i64,
    frames: &mut [ViewportOpenFrame],
    receipt: &mut HostViewportReceipt,
) -> Result<(), HostMirrorError> {
    if frames
        .iter()
        .all(|frame| !close_facts_required(frame.kind) || frame.close_facts.is_some())
        || skip_leaves == node.summary().leaves
    {
        return Ok(());
    }
    if skip_leaves > node.summary().leaves {
        return Err(HostMirrorError::Invalid(
            "viewport suffix escapes measured tree",
        ));
    }
    if skip_leaves == 0 {
        return scan_measured_node_for_close_facts(node, depth, frames, receipt);
    }
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("viewport node receipt overflow"))?;
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            let skip = usize::try_from(skip_leaves)
                .map_err(|_| HostMirrorError::Invalid("viewport suffix exceeds usize"))?;
            for leaf in &leaves[skip..] {
                scan_measured_leaf_for_close_facts(leaf, depth, frames, receipt)?;
            }
            Ok(())
        }
        MeasuredNode::Branch { left, right, .. } => {
            let left_count = left.summary().leaves;
            if skip_leaves < left_count {
                scan_measured_suffix_for_close_facts(left, skip_leaves, depth, frames, receipt)?;
                scan_measured_node_for_close_facts(right, depth, frames, receipt)
            } else {
                scan_measured_suffix_for_close_facts(
                    right,
                    skip_leaves - left_count,
                    depth,
                    frames,
                    receipt,
                )
            }
        }
    }
}

fn scan_measured_node_for_close_facts(
    node: &Arc<MeasuredNode>,
    depth: &mut i64,
    frames: &mut [ViewportOpenFrame],
    receipt: &mut HostViewportReceipt,
) -> Result<(), HostMirrorError> {
    if frames
        .iter()
        .all(|frame| !close_facts_required(frame.kind) || frame.close_facts.is_some())
    {
        return Ok(());
    }
    receipt.tree_nodes_visited = receipt
        .tree_nodes_visited
        .checked_add(1)
        .ok_or(HostMirrorError::Invalid("viewport node receipt overflow"))?;
    let summary = node.summary();
    let minimum_depth = depth
        .checked_add(summary.structural_minimum_prefix)
        .ok_or(HostMirrorError::Invalid("viewport depth overflow"))?;
    if !unresolved_close_may_occur(frames, *depth, minimum_depth) {
        *depth = depth
            .checked_add(summary.structural_balance)
            .ok_or(HostMirrorError::Invalid("viewport depth overflow"))?;
        if *depth < 0 {
            return Err(HostMirrorError::Invalid(
                "viewport suffix crosses structural root",
            ));
        }
        receipt.summary_nodes_skipped = receipt
            .summary_nodes_skipped
            .checked_add(1)
            .ok_or(HostMirrorError::Invalid("viewport skip receipt overflow"))?;
        return Ok(());
    }
    match node.as_ref() {
        MeasuredNode::Page { leaves, .. } => {
            for leaf in leaves.iter() {
                scan_measured_leaf_for_close_facts(leaf, depth, frames, receipt)?;
            }
            Ok(())
        }
        MeasuredNode::Branch { left, right, .. } => {
            scan_measured_node_for_close_facts(left, depth, frames, receipt)?;
            scan_measured_node_for_close_facts(right, depth, frames, receipt)
        }
    }
}

fn scan_measured_leaf_for_close_facts(
    leaf: &MeasuredLeaf,
    depth: &mut i64,
    frames: &mut [ViewportOpenFrame],
    receipt: &mut HostViewportReceipt,
) -> Result<(), HostMirrorError> {
    if frames
        .iter()
        .all(|frame| !close_facts_required(frame.kind) || frame.close_facts.is_some())
    {
        return Ok(());
    }
    let minimum_depth = depth
        .checked_add(leaf.structural.minimum_prefix)
        .ok_or(HostMirrorError::Invalid("viewport depth overflow"))?;
    if !unresolved_close_may_occur(frames, *depth, minimum_depth) {
        *depth = depth
            .checked_add(leaf.structural.balance)
            .ok_or(HostMirrorError::Invalid("viewport depth overflow"))?;
        receipt.summary_nodes_skipped = receipt
            .summary_nodes_skipped
            .checked_add(1)
            .ok_or(HostMirrorError::Invalid("viewport skip receipt overflow"))?;
        return Ok(());
    }
    let decoded = decode_measured_leaf_structure(leaf, receipt)?;
    for event in decoded.structural_events {
        match event {
            CopiedGreenStructuralEvent::Enter { .. } => {
                *depth = depth
                    .checked_add(1)
                    .ok_or(HostMirrorError::Invalid("viewport depth overflow"))?;
            }
            CopiedGreenStructuralEvent::Exit { facts } => {
                if *depth <= 0 {
                    return Err(HostMirrorError::Invalid(
                        "viewport suffix has unmatched Exit",
                    ));
                }
                if let Ok(index) = usize::try_from(*depth - 1)
                    && let Some(frame) = frames.get_mut(index)
                    && close_facts_required(frame.kind)
                    && frame.close_facts.is_none()
                {
                    facts.validate_for_kind(frame.kind)?;
                    frame.close_facts = Some(facts);
                }
                *depth -= 1;
            }
        }
    }
    Ok(())
}

fn checked_metric_add(
    left: SerializedMetric,
    right: SerializedMetric,
) -> Result<SerializedMetric, HostMirrorError> {
    Ok(SerializedMetric {
        bytes: left
            .bytes
            .checked_add(right.bytes)
            .ok_or(HostMirrorError::Invalid("byte metric overflow"))?,
        utf16: left
            .utf16
            .checked_add(right.utf16)
            .ok_or(HostMirrorError::Invalid("UTF-16 metric overflow"))?,
    })
}

fn checked_metric_sub(
    left: SerializedMetric,
    right: SerializedMetric,
) -> Result<SerializedMetric, HostMirrorError> {
    Ok(SerializedMetric {
        bytes: left
            .bytes
            .checked_sub(right.bytes)
            .ok_or(HostMirrorError::Invalid("byte metric order"))?,
        utf16: left
            .utf16
            .checked_sub(right.utf16)
            .ok_or(HostMirrorError::Invalid("UTF-16 metric order"))?,
    })
}

fn metric_sum3(
    first: SerializedMetric,
    second: SerializedMetric,
    third: SerializedMetric,
) -> Option<SerializedMetric> {
    Some(SerializedMetric {
        bytes: first
            .bytes
            .checked_add(second.bytes)?
            .checked_add(third.bytes)?,
        utf16: first
            .utf16
            .checked_add(second.utf16)?
            .checked_add(third.utf16)?,
    })
}

fn metric_checked_sub(
    left: SerializedMetric,
    right: SerializedMetric,
    context: &'static str,
) -> Result<SerializedMetric, SerializedGreenError> {
    Ok(SerializedMetric {
        bytes: left
            .bytes
            .checked_sub(right.bytes)
            .ok_or(SerializedGreenError::Corrupt(context))?,
        utf16: left
            .utf16
            .checked_sub(right.utf16)
            .ok_or(SerializedGreenError::Corrupt(context))?,
    })
}

const fn metric_at_or_before(left: SerializedMetric, right: SerializedMetric) -> bool {
    left.bytes <= right.bytes && left.utf16 <= right.utf16
}

const fn metric_at_or_after(left: SerializedMetric, right: SerializedMetric) -> bool {
    left.bytes >= right.bytes && left.utf16 >= right.utf16
}

fn validate_source_lineage_edits(
    edits: &[SourceLineageEdit],
    base_total: SerializedMetric,
    target_total: SerializedMetric,
) -> Result<(), HostMirrorError> {
    if edits.is_empty() {
        return Err(HostMirrorError::Invalid("source lineage has no edits"));
    }
    let mut base_cursor = SerializedMetric::default();
    let mut target_cursor = SerializedMetric::default();
    for edit in edits {
        edit.base.validate(base_total)?;
        edit.target.validate(target_total)?;
        if edit.base.start.bytes < base_cursor.bytes
            || edit.base.start.utf16 < base_cursor.utf16
            || edit.target.start.bytes < target_cursor.bytes
            || edit.target.start.utf16 < target_cursor.utf16
            || checked_metric_sub(edit.base.start, base_cursor)?
                != checked_metric_sub(edit.target.start, target_cursor)?
        {
            return Err(HostMirrorError::Invalid(
                "source lineage edits do not preserve unchanged gaps",
            ));
        }
        base_cursor = edit.base.end;
        target_cursor = edit.target.end;
    }
    if checked_metric_sub(base_total, base_cursor)?
        != checked_metric_sub(target_total, target_cursor)?
    {
        return Err(HostMirrorError::Invalid(
            "source lineage trailing suffix metric differs",
        ));
    }
    Ok(())
}

fn text_metric(text: &str) -> SerializedMetric {
    SerializedMetric {
        bytes: u64::try_from(text.len()).expect("Rust string byte length fits u64"),
        utf16: u64::try_from(text.encode_utf16().count())
            .expect("Rust string UTF-16 length fits u64"),
    }
}

fn source_digest(source: &str) -> SourceContentHash128 {
    const BASES: [u32; 4] = [0x0010_0193, 0x9e37_79b1, 0x85eb_ca77, 0xc2b2_ae3d];
    let mut words = [0_u32; 4];
    for byte in source.as_bytes() {
        let value = u32::from(*byte).wrapping_add(1);
        for (word, base) in words.iter_mut().zip(BASES) {
            *word = word.wrapping_mul(base).wrapping_add(value);
        }
    }
    SourceContentHash128 {
        word0: words[0],
        word1: words[1],
        word2: words[2],
        word3: words[3],
    }
}

#[cfg(test)]
pub(crate) fn source_digest_for_test(source: &str) -> SourceContentHash128 {
    source_digest(source)
}

fn object_atom_digest(id: HostObjectId) -> u128 {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(&id.session.0);
    bytes.extend_from_slice(&id.arena_slot.to_le_bytes());
    bytes.extend_from_slice(&id.arena_generation.to_le_bytes());
    digest_tagged_bytes(0x4f, &bytes).0
}

fn copied_object_digest(id: HostObjectId, object: &CopiedObject) -> ProtocolDigest {
    let mut bytes = Vec::with_capacity(object.payload().len().saturating_add(128));
    bytes.extend_from_slice(&id.session.0);
    bytes.extend_from_slice(&id.arena_slot.to_le_bytes());
    bytes.extend_from_slice(&id.arena_generation.to_le_bytes());
    match object {
        CopiedObject::ProjectionProgram { payload } => {
            bytes.push(1);
            bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes.extend_from_slice(payload);
        }
        CopiedObject::GreenLeaf { payload, children } => {
            bytes.push(2);
            bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
            bytes.extend_from_slice(payload);
            bytes.extend_from_slice(&(children.len() as u64).to_le_bytes());
            for child in children.iter() {
                bytes.extend_from_slice(&child.session.0);
                bytes.extend_from_slice(&child.arena_slot.to_le_bytes());
                bytes.extend_from_slice(&child.arena_generation.to_le_bytes());
            }
        }
    }
    digest_tagged_bytes(0x43, &bytes)
}

fn leaf_closure_digest_from_object_digests(
    id: HostObjectId,
    leaf: ProtocolDigest,
    programs: &[ProtocolDigest],
) -> ProtocolDigest {
    let mut bytes = Vec::with_capacity(48 + programs.len() * 16);
    bytes.extend_from_slice(&id.session.0);
    bytes.extend_from_slice(&id.arena_slot.to_le_bytes());
    bytes.extend_from_slice(&id.arena_generation.to_le_bytes());
    bytes.extend_from_slice(&leaf.0.to_le_bytes());
    for program in programs {
        bytes.extend_from_slice(&program.0.to_le_bytes());
    }
    digest_tagged_bytes(0x4c, &bytes)
}

fn leaf_closure_digest(
    id: HostObjectId,
    leaf_payload: &[u8],
    children: &[HostObjectId],
    programs: &[Arc<CopiedObject>],
) -> Result<ProtocolDigest, HostMirrorError> {
    if children.len() != programs.len() {
        return Err(HostMirrorError::Invalid(
            "leaf closure child count mismatch",
        ));
    }
    let leaf = CopiedObject::GreenLeaf {
        payload: Arc::from(leaf_payload),
        children: Arc::from(children),
    };
    let leaf_digest = copied_object_digest(id, &leaf);
    let program_digests = children
        .iter()
        .zip(programs)
        .map(|(child, program)| copied_object_digest(*child, program))
        .collect::<Vec<_>>();
    Ok(leaf_closure_digest_from_object_digests(
        id,
        leaf_digest,
        &program_digests,
    ))
}

fn leaf_sequence_atom_digest(id: HostObjectId, closure: ProtocolDigest) -> u128 {
    let mut bytes = Vec::with_capacity(48);
    bytes.extend_from_slice(&id.session.0);
    bytes.extend_from_slice(&id.arena_slot.to_le_bytes());
    bytes.extend_from_slice(&id.arena_generation.to_le_bytes());
    bytes.extend_from_slice(&closure.0.to_le_bytes());
    digest_tagged_bytes(0x41, &bytes).0
}

fn sequence_identity_digest(ids: impl IntoIterator<Item = HostObjectId>) -> ProtocolDigest {
    let mut value = 0_u128;
    let mut power = 1_u128;
    for id in ids {
        value = value.wrapping_add(object_atom_digest(id).wrapping_mul(power));
        power = power.wrapping_mul(SEQUENCE_POLYNOMIAL_BASE);
    }
    ProtocolDigest(value)
}

fn sequence_content_digest(
    leaves: impl IntoIterator<Item = (HostObjectId, ProtocolDigest)>,
) -> ProtocolDigest {
    let mut value = 0_u128;
    let mut power = 1_u128;
    for (id, closure) in leaves {
        value = value.wrapping_add(leaf_sequence_atom_digest(id, closure).wrapping_mul(power));
        power = power.wrapping_mul(SEQUENCE_POLYNOMIAL_BASE);
    }
    ProtocolDigest(value)
}

fn wrapping_pow(mut base: u128, mut exponent: usize) -> u128 {
    let mut output = 1_u128;
    while exponent != 0 {
        if exponent & 1 != 0 {
            output = output.wrapping_mul(base);
        }
        base = base.wrapping_mul(base);
        exponent >>= 1;
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn manifest_digest(
    session: PublicationSessionId,
    document_session: DocumentSessionId,
    source_revision: SourceRevision,
    parse_generation: ParseGeneration,
    grammar_revision: GrammarRevision,
    source_hash: SourceContentHash128,
    metric: SerializedMetric,
    leaf_count: u64,
    sequence_digest: ProtocolDigest,
) -> ProtocolDigest {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(&HOST_BUNDLE_SCHEMA.to_le_bytes());
    bytes.extend_from_slice(&session.0);
    bytes.extend_from_slice(&document_session.0);
    bytes.extend_from_slice(&source_revision.0.to_le_bytes());
    bytes.extend_from_slice(&parse_generation.0.to_le_bytes());
    bytes.extend_from_slice(&grammar_revision.0.to_le_bytes());
    bytes.extend_from_slice(&source_hash.word0.to_le_bytes());
    bytes.extend_from_slice(&source_hash.word1.to_le_bytes());
    bytes.extend_from_slice(&source_hash.word2.to_le_bytes());
    bytes.extend_from_slice(&source_hash.word3.to_le_bytes());
    bytes.extend_from_slice(&metric.bytes.to_le_bytes());
    bytes.extend_from_slice(&metric.utf16.to_le_bytes());
    bytes.extend_from_slice(&leaf_count.to_le_bytes());
    bytes.extend_from_slice(&sequence_digest.0.to_le_bytes());
    digest_tagged_bytes(0x4d, &bytes)
}

fn digest_tagged_bytes(tag: u8, bytes: &[u8]) -> ProtocolDigest {
    // Deterministic feasibility checksum. The production transport should use
    // a cryptographic digest (for example BLAKE3) without changing protocol
    // ownership or splice semantics.
    let mut low = 0xcbf2_9ce4_8422_2325_u64 ^ u64::from(tag);
    let mut high = 0x8422_2325_cbf2_9ce4_u64 ^ (u64::from(tag) << 32);
    for byte in bytes {
        low ^= u64::from(*byte);
        low = low.wrapping_mul(0x0000_0100_0000_01b3);
        high ^= u64::from(*byte).rotate_left(1);
        high = high.wrapping_mul(0x9e37_79b1_85eb_ca87);
    }
    ProtocolDigest((u128::from(high) << 64) | u128::from(low))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArenaBuildSession, BlockId, BuilderGreenPrefixSnapshot, ClosedChildAggregate, CoverageId,
        CoveragePart, FactsEnvelope, GreenEvent, GreenFenceCharacter, GreenFencedCodeCloseFacts,
        GreenFencedCodeOpenFacts, GreenItemOpenFacts, GreenJournalSuffixAdmission,
        GreenJournalSuffixSpliceProgress, GreenKind, GreenListBullet, GreenListOpenFacts,
        GreenRelativeLogicalSlice, LogicalContribution, ProjectionPiece, ProjectionProgram,
        ResumableGreenJournalSuffixSplice, ResumableSerializedGreenBuild, SerializedGreenRootSpec,
        SerializedGreenStreamProgress, SourceProjectionRun,
    };

    const TEST_SESSION: PublicationSessionId = PublicationSessionId([0x51; 16]);

    fn synthetic_leaf(ordinal: u32, metric: SerializedMetric) -> MeasuredLeaf {
        synthetic_leaf_with_structure(ordinal, metric, 0, 0)
    }

    fn synthetic_leaf_with_structure(
        ordinal: u32,
        metric: SerializedMetric,
        balance: i64,
        minimum_prefix: i64,
    ) -> MeasuredLeaf {
        let id = HostObjectId {
            session: TEST_SESSION,
            arena_slot: ordinal,
            arena_generation: 1,
        };
        let object = Arc::new(CopiedObject::GreenLeaf {
            payload: Arc::from([]),
            children: Arc::from([]),
        });
        let closure_digest = leaf_closure_digest(id, &[], &[], &[]).unwrap();
        MeasuredLeaf {
            id,
            metric,
            closure_digest,
            object,
            programs: Arc::from([]),
            structural: CopiedGreenLeafSummary {
                metric,
                balance,
                minimum_prefix,
            },
        }
    }

    fn flatten(node: Option<&Arc<MeasuredNode>>, output: &mut Vec<MeasuredLeaf>) {
        let Some(node) = node else { return };
        match node.as_ref() {
            MeasuredNode::Page { leaves, .. } => output.extend(leaves.iter().cloned()),
            MeasuredNode::Branch { left, right, .. } => {
                flatten(Some(left), output);
                flatten(Some(right), output);
            }
        }
    }

    fn assert_tree_invariants(node: &Arc<MeasuredNode>) -> MeasuredSummary {
        match node.as_ref() {
            MeasuredNode::Page { leaves, summary } => {
                assert!(!leaves.is_empty());
                assert!(leaves.len() <= HOST_MEASURED_PAGE_LEAVES);
                let expected = measured_page_summary(leaves).unwrap();
                assert_eq!(*summary, expected);
                expected
            }
            MeasuredNode::Branch {
                left,
                right,
                summary,
            } => {
                let left_summary = assert_tree_invariants(left);
                let right_summary = assert_tree_invariants(right);
                assert!(
                    left_summary.height.abs_diff(right_summary.height) <= 1,
                    "AVL height mismatch: left={} right={}",
                    left_summary.height,
                    right_summary.height,
                );
                let expected = followed_summary(left_summary, right_summary).unwrap();
                assert_eq!(*summary, expected);
                expected
            }
        }
    }

    fn assert_sequence_matches(sequence: &MeasuredLeafSequence, flat: &[MeasuredLeaf]) {
        let mut actual = Vec::new();
        flatten(sequence.root.as_ref(), &mut actual);
        assert_eq!(
            actual.iter().map(|leaf| leaf.id).collect::<Vec<_>>(),
            flat.iter().map(|leaf| leaf.id).collect::<Vec<_>>()
        );
        let expected = measured_page_summary(flat).unwrap();
        let actual_summary = sequence.summary();
        assert_eq!(actual_summary.leaves, expected.leaves);
        assert_eq!(actual_summary.metric, expected.metric);
        assert_eq!(actual_summary.identity_digest, expected.identity_digest);
        assert_eq!(actual_summary.content_digest, expected.content_digest);
        assert_eq!(
            actual_summary.structural_balance,
            expected.structural_balance
        );
        assert_eq!(
            actual_summary.structural_minimum_prefix,
            expected.structural_minimum_prefix
        );
        if let Some(root) = &sequence.root {
            assert_eq!(assert_tree_invariants(root), actual_summary);
        } else {
            assert!(flat.is_empty());
        }

        let mut prefix = SerializedMetric::default();
        for expected_leaf in flat {
            let (actual_leaf, actual_prefix, _) = sequence.locate_byte(prefix.bytes).unwrap();
            assert_eq!(actual_leaf.id, expected_leaf.id);
            assert_eq!(actual_prefix, prefix);
            prefix = checked_metric_add(prefix, expected_leaf.metric).unwrap();
        }
    }

    #[test]
    fn source_hash_matches_dart_four_lane_utf8_contract() {
        assert_eq!(source_digest(""), SourceContentHash128::default());
        assert_eq!(
            source_digest("a😀 café β\n"),
            SourceContentHash128 {
                word0: 0xb991_edd9,
                word1: 0x5fb5_7c47,
                word2: 0x8873_2115,
                word3: 0x2292_a46b,
            }
        );
        assert_eq!(
            source_digest("a🌍b\n"),
            SourceContentHash128 {
                word0: 0xcc6c_28f6,
                word1: 0x0aa8_0a4c,
                word2: 0xdf5f_6342,
                word3: 0x250a_ffb0,
            }
        );
        assert_eq!(
            source_digest("aé🌍b\n"),
            SourceContentHash128 {
                word0: 0x9cfb_81cc,
                word1: 0x8cb1_defa,
                word2: 0x6f97_a348,
                word3: 0x6f98_e8ee,
            }
        );
        assert_eq!(
            source_digest("aéb\n"),
            SourceContentHash128 {
                word0: 0x9167_4f8c,
                word1: 0x5d5a_b6ce,
                word2: 0xb9ac_ab58,
                word3: 0x359e_37f2,
            }
        );
    }

    #[test]
    fn randomized_middle_splices_preserve_avl_summaries_and_flat_oracle() {
        let mut next_id = 1_u32;
        let mut flat = (0..641)
            .map(|index| {
                let metric = if index % 7 == 0 {
                    SerializedMetric { bytes: 4, utf16: 2 }
                } else {
                    SerializedMetric { bytes: 1, utf16: 1 }
                };
                let (balance, minimum_prefix) = match index % 5 {
                    0 => (2, 0),
                    1 => (-1, -1),
                    2 => (0, 0),
                    3 => (-1, -1),
                    _ => (0, -1),
                };
                let leaf = synthetic_leaf_with_structure(next_id, metric, balance, minimum_prefix);
                next_id += 1;
                leaf
            })
            .collect::<Vec<_>>();
        let mut receipt = HostMeasuredSpliceReceipt::default();
        let mut sequence = MeasuredLeafSequence {
            root: build_measured_pages(flat.clone(), &mut receipt).unwrap(),
            measured_splices: 0,
            last_splice: receipt,
        };
        assert!(
            sequence.summary().height >= 5,
            ">=4-leaf/page path required"
        );
        assert_sequence_matches(&sequence, &flat);

        let mut random = 0x4d59_5df4_d0f3_3173_u64;
        for _step in 0..300 {
            random = random
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let start = usize::try_from(random % u64::try_from(flat.len()).unwrap()).unwrap();
            random = random.rotate_left(17);
            let delete = usize::try_from(random % 4)
                .unwrap()
                .min(flat.len().saturating_sub(start));
            random = random.rotate_left(23);
            let insert_count = usize::try_from(random % 4).unwrap();
            let inserted = (0..insert_count)
                .map(|offset| {
                    let metric = if (next_id + u32::try_from(offset).unwrap()) % 5 == 0 {
                        SerializedMetric { bytes: 4, utf16: 2 }
                    } else {
                        SerializedMetric { bytes: 1, utf16: 1 }
                    };
                    let (balance, minimum_prefix) = match next_id % 4 {
                        0 => (1, 0),
                        1 => (-1, -1),
                        2 => (0, 0),
                        _ => (0, -1),
                    };
                    let leaf =
                        synthetic_leaf_with_structure(next_id, metric, balance, minimum_prefix);
                    next_id += 1;
                    leaf
                })
                .collect::<Vec<_>>();
            sequence = sequence
                .splice(
                    u64::try_from(start).unwrap(),
                    u64::try_from(delete).unwrap(),
                    inserted.clone(),
                )
                .unwrap();
            flat.splice(start..start + delete, inserted);
            assert_sequence_matches(&sequence, &flat);
        }
    }

    #[test]
    fn repeated_same_gap_splice_has_bounded_real_tree_work() {
        let mut next_id = 1_u32;
        let mut flat = (0..1025)
            .map(|_| {
                let leaf = synthetic_leaf(next_id, SerializedMetric { bytes: 1, utf16: 1 });
                next_id += 1;
                leaf
            })
            .collect::<Vec<_>>();
        let mut build_receipt = HostMeasuredSpliceReceipt::default();
        let mut sequence = MeasuredLeafSequence {
            root: build_measured_pages(flat.clone(), &mut build_receipt).unwrap(),
            measured_splices: 0,
            last_splice: build_receipt,
        };
        let gap = 513_usize;
        let retained_suffix_object = flat[gap + 1].object.clone();
        for _ in 0..256 {
            let inserted = synthetic_leaf(next_id, SerializedMetric { bytes: 1, utf16: 1 });
            next_id += 1;
            sequence = sequence
                .splice(u64::try_from(gap).unwrap(), 1, vec![inserted.clone()])
                .unwrap();
            flat.splice(gap..gap + 1, [inserted]);
            let height = usize::from(sequence.summary().height);
            let receipt = sequence.last_splice;
            assert!(
                receipt.tree_nodes_visited <= 16 * height + 16,
                "{receipt:?}"
            );
            assert!(
                receipt.tree_nodes_allocated <= 24 * height + 24,
                "{receipt:?}"
            );
            assert!(receipt.boundary_leaf_entries_copied <= 2 * HOST_MEASURED_PAGE_LEAVES);
            assert_eq!(receipt.inserted_leaf_entries, 1);
            assert_sequence_matches(&sequence, &flat);
            assert!(Arc::ptr_eq(&flat[gap + 1].object, &retained_suffix_object));
        }
    }

    fn green_spec(
        metric: SerializedMetric,
        revision: u64,
        root: u64,
        parse_generation: u64,
    ) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 7,
            source_revision: SourceRevision(revision),
            source_root: SourceRootId(root),
            source_bytes: metric.bytes,
            source_utf16: metric.utf16,
            grammar_revision: GrammarRevision(11),
            parse_generation: ParseGeneration(parse_generation),
            semantic_epoch: revision.max(1),
            known_bytes: 0..metric.bytes,
        }
    }

    fn poll_green_to_input(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        loop {
            match builder.poll(session).unwrap() {
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("green fixture finalized before EOF")
                }
            }
        }
    }

    fn offer_green(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        builder.offer_event(session, event).unwrap();
        poll_green_to_input(builder, session);
    }

    fn logical_coverage(
        id: u64,
        metric: SerializedMetric,
        contribution: LogicalContribution,
    ) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(id),
                metric.bytes,
                metric.utf16,
                0,
                CoveragePart::CONTENT,
                BlockId(2),
                contribution,
            )
            .unwrap(),
        )
    }

    fn identity_program(metric: SerializedMetric) -> LogicalContribution {
        LogicalContribution::Program(
            ProjectionProgram::new(vec![ProjectionPiece::Identity { metric }]).unwrap(),
        )
    }

    fn build_old_unicode_document(
        arena: &mut PageArena,
    ) -> (
        SerializedGreenDocument,
        crate::GreenSourceTailAdoptionCapability,
    ) {
        let metric = text_metric("a🌍b\n");
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, green_spec(metric, 1, 101, 1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer_green(
            &mut builder,
            &mut session,
            logical_coverage(
                1,
                SerializedMetric { bytes: 1, utf16: 1 },
                LogicalContribution::Identity,
            ),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let suffix_event_cut = builder
            .take_leaf_barrier_cut(&session)
            .unwrap()
            .events_before();
        offer_green(
            &mut builder,
            &mut session,
            logical_coverage(
                2,
                SerializedMetric { bytes: 4, utf16: 2 },
                identity_program(SerializedMetric { bytes: 4, utf16: 2 }),
            ),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let _ = builder.take_leaf_barrier_cut(&session).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            logical_coverage(
                3,
                SerializedMetric { bytes: 1, utf16: 1 },
                LogicalContribution::Identity,
            ),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let _ = builder.take_leaf_barrier_cut(&session).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            logical_coverage(
                4,
                SerializedMetric { bytes: 1, utf16: 1 },
                LogicalContribution::Identity,
            ),
        );
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_green(
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
                    panic!("old Unicode fixture requested input after EOF")
                }
            }
        }
        let document = builder.take_manifest().unwrap().commit(session).unwrap().0;
        assert_eq!(document.leaf_count(arena).unwrap(), 4);
        let boundary = document
            .suffix_adoption_boundary_at_event_cut(arena, suffix_event_cut)
            .unwrap();
        let tail = document
            .source_tail_adoption_capability(arena, boundary)
            .unwrap();
        (document, tail)
    }

    fn build_unicode_suffix_target(
        arena: &mut PageArena,
        old: &SerializedGreenDocument,
        tail: crate::GreenSourceTailAdoptionCapability,
    ) -> (SerializedGreenDocument, TypedGreenLeafSplice) {
        let metric = text_metric("aé🌍b\n");
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, green_spec(metric, 2, 202, 2)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer_green(
            &mut builder,
            &mut session,
            logical_coverage(
                10,
                SerializedMetric { bytes: 3, utf16: 2 },
                identity_program(SerializedMetric { bytes: 3, utf16: 2 }),
            ),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let cut = builder.take_leaf_barrier_cut(&session).unwrap();
        let snapshot: BuilderGreenPrefixSnapshot = builder
            .capture_builder_green_prefix_snapshot(&session, &cut)
            .unwrap();
        let mut ticket = session.suspend().unwrap();
        let admission = ResumableGreenJournalSuffixSplice::begin_from_document_for_test(
            &ticket, arena, builder, snapshot, tail, old,
        )
        .unwrap();
        let GreenJournalSuffixAdmission::Ready(mut job) = admission else {
            panic!("Unicode direct suffix fixture must be admitted")
        };
        for polls in 0..1024 {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = job.poll(&mut session).unwrap();
            ticket = session.suspend().unwrap();
            if progress == GreenJournalSuffixSpliceProgress::Complete {
                break;
            }
            assert!(polls < 1023, "Unicode suffix splice must converge");
        }
        let result = job.take_result().unwrap();
        let (manifest, proof) = result.into_host_mirror_fixture_parts();
        drop(job);
        let session = arena.resume_build(ticket).unwrap();
        let target = manifest.commit(session).unwrap().0;
        assert_eq!(target.leaf_count(arena).unwrap(), 4);
        (target, proof)
    }

    fn settle_arena(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(64).unwrap();
        }
    }

    fn source_version(
        document_session: DocumentSessionId,
        revision: u64,
        source: &str,
    ) -> SourceVersion {
        SourceVersion {
            document_session,
            revision: SourceRevision(revision),
            metric: text_metric(source),
            hash: source_digest(source),
        }
    }

    fn build_nested_close_facts_document(
        arena: &mut PageArena,
        revision: u64,
    ) -> SerializedGreenDocument {
        let metric = text_metric("code\n");
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, green_spec(metric, revision, 303, 1))
                .unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        for event in [
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(
                BlockId(2),
                GreenKind::LIST,
                GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope(),
            ),
            GreenEvent::enter(
                BlockId(3),
                GreenKind::ITEM,
                GreenItemOpenFacts::new(0, 2).unwrap().into_envelope(),
            ),
            GreenEvent::enter(
                BlockId(4),
                GreenKind::FENCED_CODE,
                GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 3, 0)
                    .unwrap()
                    .into_envelope(),
            ),
        ] {
            offer_green(&mut builder, &mut session, event);
        }
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let _ = builder.take_leaf_barrier_cut(&session).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(20),
                    metric.bytes,
                    metric.utf16,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(4),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let _ = builder.take_leaf_barrier_cut(&session).unwrap();
        let fence_facts = GreenFencedCodeCloseFacts::new(
            true,
            GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
            GreenRelativeLogicalSlice::new(0..5, 0..5).unwrap(),
        )
        .unwrap();
        for event in [
            GreenEvent::exit_with_facts(
                ClosedChildAggregate::default(),
                GreenCloseFacts::FencedCode(fence_facts),
            ),
            GreenEvent::exit_with_state(
                ClosedChildAggregate {
                    ends_blank: true,
                    item_loose_if_nonlast: true,
                    item_loose_if_last: false,
                },
                true,
                GreenCloseFacts::None,
            ),
            GreenEvent::enter(
                BlockId(5),
                GreenKind::ITEM,
                GreenItemOpenFacts::new(0, 2).unwrap().into_envelope(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit_with_facts(
                ClosedChildAggregate::default(),
                GreenCloseFacts::List { tight: false },
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ] {
            offer_green(&mut builder, &mut session, event);
        }
        builder.finish_input(&mut session).unwrap();
        loop {
            match builder.poll(&mut session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("nested close-facts fixture requested input after EOF")
                }
            }
        }
        let document = builder.take_manifest().unwrap().commit(session).unwrap().0;
        assert_eq!(document.leaf_count(arena).unwrap(), 3);
        document
    }

    fn build_over_depth_blockquote_document(
        arena: &mut PageArena,
        quote_depth: u64,
        source: &str,
    ) -> SerializedGreenDocument {
        let metric = text_metric(source);
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, green_spec(metric, 1, 404, 1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        for offset in 0..quote_depth {
            offer_green(
                &mut builder,
                &mut session,
                GreenEvent::enter(
                    BlockId(offset + 2),
                    GreenKind::BLOCK_QUOTE,
                    FactsEnvelope::empty(),
                ),
            );
        }
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::enter(
                BlockId(quote_depth + 2),
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ),
        );
        // Put the source-bearing run in a later leaf so its measured prefix
        // balance is the exact pathological open depth.
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_green_to_input(&mut builder, &mut session);
        let _ = builder.take_leaf_barrier_cut(&session).unwrap();
        offer_green(
            &mut builder,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(30),
                    metric.bytes,
                    metric.utf16,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(quote_depth + 2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        for _ in 0..quote_depth + 2 {
            offer_green(
                &mut builder,
                &mut session,
                GreenEvent::exit(ClosedChildAggregate::default()),
            );
        }
        builder.finish_input(&mut session).unwrap();
        loop {
            match builder.poll(&mut session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("deep blockquote fixture requested input after EOF")
                }
            }
        }
        builder.take_manifest().unwrap().commit(session).unwrap().0
    }

    #[test]
    fn full_snapshot_accepts_valid_source_revision_zero() {
        let mut arena = PageArena::new();
        let document = build_nested_close_facts_document(&mut arena, 0);
        let document_session = DocumentSessionId([0x65; 16]);
        let source = source_version(document_session, 0, "code\n");
        let bundle = prepare_full_snapshot_bundle(
            &arena,
            &document,
            PublicationSessionId([0x66; 16]),
            HostRevisionId(1),
            source,
        )
        .expect("source revision zero is a valid initial snapshot");
        document.release_later(&mut arena).unwrap();
        settle_arena(&mut arena);

        let mut host = HostMirror::new(source);
        let ack = host.apply_bundle(bundle).unwrap();
        assert_eq!(ack.structural_source_revision, SourceRevision(0));
        host.acknowledge_delivery(ack).unwrap();
        assert!(matches!(
            host.query_metric(SerializedMetric::default()).unwrap(),
            HostQuery::Structural(_)
        ));
    }

    #[test]
    fn viewport_context_resolves_nested_list_and_fence_after_worker_retirement() {
        let mut arena = PageArena::new();
        let document = build_nested_close_facts_document(&mut arena, 1);
        let document_session = DocumentSessionId([0x61; 16]);
        let source = source_version(document_session, 1, "code\n");
        let bundle = prepare_full_snapshot_bundle(
            &arena,
            &document,
            PublicationSessionId([0x62; 16]),
            HostRevisionId(1),
            source,
        )
        .unwrap();
        document.release_later(&mut arena).unwrap();
        settle_arena(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);

        let mut host = HostMirror::new(source);
        let ack = host.apply_bundle(bundle).unwrap();
        assert_eq!(
            host.structural
                .as_ref()
                .unwrap()
                .sequence
                .summary()
                .structural_balance,
            0
        );
        assert_eq!(
            host.structural
                .as_ref()
                .unwrap()
                .sequence
                .summary()
                .structural_minimum_prefix,
            0
        );
        host.acknowledge_delivery(ack).unwrap();
        let query = match host.query_metric(SerializedMetric::default()).unwrap() {
            HostQuery::Structural(query) => query,
            HostQuery::SourceGap(_) => panic!("clean viewport must be structural"),
        };
        assert_eq!(query.viewport_context.open.len(), 4);
        assert_eq!(
            query
                .viewport_context
                .open
                .iter()
                .map(|frame| frame.kind)
                .collect::<Vec<_>>(),
            vec![
                GreenKind::DOCUMENT,
                GreenKind::LIST,
                GreenKind::ITEM,
                GreenKind::FENCED_CODE,
            ]
        );
        assert_eq!(
            query.viewport_context.open[1].close_facts,
            Some(GreenCloseFacts::List { tight: false })
        );
        let Some(GreenCloseFacts::FencedCode(fence)) = query.viewport_context.open[3].close_facts
        else {
            panic!("fence close-time facts must be resolved")
        };
        assert!(fence.closed());
        assert_eq!(fence.literal().bytes(), 0..5);
        assert_eq!(query.viewport_context.receipt.leaf_pages_decoded, 2);
        assert!(query.viewport_context.receipt.summary_nodes_skipped >= 1);
        assert_eq!(query.viewport_context.receipt.maximum_open_depth, 4);
        assert!(query.viewport_context.receipt.maximum_decoded_page_bytes <= 2 * ARENA_PAGE_BYTES);

        let eof = match host
            .query_metric_with_affinity(source.metric, HostMetricAffinity::Upstream)
            .unwrap()
        {
            HostQuery::Structural(query) => query,
            HostQuery::SourceGap(_) => panic!("clean upstream EOF must resolve a leaf"),
        };
        assert_eq!(eof.id, query.id);
        assert!(
            host.query_metric_with_affinity(source.metric, HostMetricAffinity::Downstream)
                .is_err()
        );
    }

    #[test]
    fn viewport_depth_budget_falls_back_to_exact_editable_source_before_frame_walk() {
        let quote_depth = HOST_MAX_VIEWPORT_OPEN_DEPTH + 16;
        let source_text = format!("{}x\n", "> ".repeat(usize::try_from(quote_depth).unwrap()));
        let mut arena = PageArena::new();
        let document = build_over_depth_blockquote_document(&mut arena, quote_depth, &source_text);
        let document_session = DocumentSessionId([0x63; 16]);
        let source = source_version(document_session, 1, &source_text);
        let bundle = prepare_full_snapshot_bundle(
            &arena,
            &document,
            PublicationSessionId([0x64; 16]),
            HostRevisionId(1),
            source,
        )
        .unwrap();
        document.release_later(&mut arena).unwrap();
        settle_arena(&mut arena);

        let mut host = HostMirror::new(source);
        let ack = host.apply_bundle(bundle).unwrap();
        host.acknowledge_delivery(ack).unwrap();
        let HostQuery::SourceGap(fallback) =
            host.query_metric(SerializedMetric::default()).unwrap()
        else {
            panic!("pathological viewport depth must fail closed to exact source")
        };
        assert_eq!(
            fallback.reason,
            SourceGapReason::ViewportOpenDepthExceeded {
                observed: quote_depth + 2,
                maximum: HOST_MAX_VIEWPORT_OPEN_DEPTH,
            }
        );
        assert_eq!(
            fallback.range,
            MetricRange {
                start: SerializedMetric::default(),
                end: source.metric,
            }
        );
        assert!(fallback.source_editable);
        assert!(!fallback.semantic_actions_valid);
    }

    #[test]
    fn dirty_point_queries_cover_full_deletion_and_eof_insertion() {
        let document_session = DocumentSessionId([0x71; 16]);
        let old = source_version(document_session, 1, "x");
        let empty = source_version(document_session, 2, "");
        let mut deletion_host = HostMirror::new(old);
        deletion_host
            .observe_source_edit(
                empty,
                vec![SourceLineageEdit {
                    base: MetricRange {
                        start: SerializedMetric::default(),
                        end: SerializedMetric { bytes: 1, utf16: 1 },
                    },
                    target: MetricRange {
                        start: SerializedMetric::default(),
                        end: SerializedMetric::default(),
                    },
                }],
            )
            .unwrap();
        let HostQuery::SourceGap(deleted) = deletion_host
            .query_metric_with_affinity(SerializedMetric::default(), HostMetricAffinity::Downstream)
            .unwrap()
        else {
            panic!("full deletion must expose an empty exact-source gap")
        };
        assert_eq!(deleted.range.start, SerializedMetric::default());
        assert_eq!(deleted.range.end, SerializedMetric::default());
        assert!(deleted.source_editable);
        assert!(!deleted.semantic_actions_valid);

        let inserted = source_version(document_session, 3, "é");
        let mut insertion_host = HostMirror::new(empty);
        insertion_host
            .observe_source_edit(
                inserted,
                vec![SourceLineageEdit {
                    base: MetricRange {
                        start: SerializedMetric::default(),
                        end: SerializedMetric::default(),
                    },
                    target: MetricRange {
                        start: SerializedMetric::default(),
                        end: SerializedMetric { bytes: 2, utf16: 1 },
                    },
                }],
            )
            .unwrap();
        let HostQuery::SourceGap(eof) = insertion_host
            .query_metric_with_affinity(inserted.metric, HostMetricAffinity::Upstream)
            .unwrap()
        else {
            panic!("dirty EOF insertion must remain an exact-source point query")
        };
        assert_eq!(eof.range.start, SerializedMetric::default());
        assert_eq!(eof.range.end, inserted.metric);

        let unicode0 = source_version(document_session, 10, "a🌍b\n");
        let unicode1 = source_version(document_session, 11, "aé🌍b\n");
        let unicode2 = source_version(document_session, 12, "aéb\n");
        let mut rapid_host = HostMirror::new(unicode0);
        rapid_host
            .observe_source_edit(
                unicode1,
                vec![SourceLineageEdit {
                    base: MetricRange {
                        start: SerializedMetric { bytes: 1, utf16: 1 },
                        end: SerializedMetric { bytes: 1, utf16: 1 },
                    },
                    target: MetricRange {
                        start: SerializedMetric { bytes: 1, utf16: 1 },
                        end: SerializedMetric { bytes: 3, utf16: 2 },
                    },
                }],
            )
            .unwrap();
        rapid_host
            .observe_source_edit(
                unicode2,
                vec![SourceLineageEdit {
                    base: MetricRange {
                        start: SerializedMetric { bytes: 3, utf16: 2 },
                        end: SerializedMetric { bytes: 7, utf16: 4 },
                    },
                    target: MetricRange {
                        start: SerializedMetric { bytes: 3, utf16: 2 },
                        end: SerializedMetric { bytes: 3, utf16: 2 },
                    },
                }],
            )
            .unwrap();
        assert_eq!(
            rapid_host.dirty.as_ref().unwrap().damage_start,
            SerializedMetric::default(),
            "rapid edits retain one conservative structural boundary without affinity mapping",
        );
        let HostQuery::SourceGap(rapid_gap) = rapid_host
            .query_metric_with_affinity(
                SerializedMetric { bytes: 3, utf16: 2 },
                HostMetricAffinity::Downstream,
            )
            .unwrap()
        else {
            panic!("second Unicode revision must remain exact-source fallback")
        };
        assert_eq!(rapid_gap.source_revision, SourceRevision(12));
        assert_eq!(rapid_gap.source_hash, source_digest("aéb\n"));
    }

    #[test]
    fn exact_current_unicode_delta_survives_worker_retirement_and_recovers_lost_ack() {
        let mut arena = PageArena::new();
        let document_session = DocumentSessionId([0x31; 16]);
        let publication = PublicationSessionId([0x41; 16]);
        let recovery_publication = PublicationSessionId([0x42; 16]);
        let old_source = source_version(document_session, 1, "a🌍b\n");
        let target_source = source_version(document_session, 2, "aé🌍b\n");
        let (old, tail) = build_old_unicode_document(&mut arena);

        let base_bundle =
            prepare_full_snapshot_bundle(&arena, &old, publication, HostRevisionId(1), old_source)
                .unwrap();
        let stale_bundle = base_bundle.clone();
        let mut host = HostMirror::new(old_source);
        let base_ack = host.apply_bundle(base_bundle).unwrap();
        assert_eq!(base_ack.leaf_count, 4);
        assert_eq!(base_ack.splice_receipt.inserted_leaf_entries, 4);
        let old_globe = match host
            .query_metric(SerializedMetric { bytes: 1, utf16: 1 })
            .unwrap()
        {
            HostQuery::Structural(query) => query,
            HostQuery::SourceGap(_) => panic!("clean base must be structural"),
        };
        assert_eq!(old_globe.programs.len(), 1);
        let old_globe_object = old_globe.object.clone();
        let old_globe_program = old_globe.programs[0].clone();
        host.acknowledge_delivery(base_ack).unwrap();

        host.observe_source_edit(
            target_source,
            vec![SourceLineageEdit {
                base: MetricRange {
                    start: SerializedMetric { bytes: 1, utf16: 1 },
                    end: SerializedMetric { bytes: 1, utf16: 1 },
                },
                target: MetricRange {
                    start: SerializedMetric { bytes: 1, utf16: 1 },
                    end: SerializedMetric { bytes: 3, utf16: 2 },
                },
            }],
        )
        .unwrap();
        let HostQuery::SourceGap(gap) = host
            .query_metric(SerializedMetric { bytes: 3, utf16: 2 })
            .unwrap()
        else {
            panic!("dirty suffix is exact-source fallback")
        };
        assert_eq!(gap.range.start, SerializedMetric::default());
        assert_eq!(gap.range.end, target_source.metric);
        assert!(gap.source_editable);
        assert!(!gap.semantic_actions_valid);
        let dirty_before_stale = host.dirty.clone();
        assert_eq!(
            host.apply_bundle(stale_bundle),
            Err(HostMirrorError::SourceAheadNeedsRetainedLineage {
                current: SourceRevision(2),
                offered: SourceRevision(1),
            })
        );
        assert_eq!(host.dirty, dirty_before_stale);

        let (target, proof) = build_unicode_suffix_target(&mut arena, &old, tail);
        let delta = prepare_typed_leaf_delta_bundle(
            &arena,
            &proof,
            base_ack,
            HostRevisionId(2),
            publication,
            target_source,
        )
        .unwrap();
        assert_eq!(delta.splice.old_start, 0);
        assert_eq!(delta.splice.old_delete, 1);
        assert_eq!(delta.splice.inserted.len(), 1);
        assert_eq!(delta.objects.len(), 2, "changed leaf plus its Program only");
        assert!(delta.objects.iter().all(|object| object.id != old_globe.id));
        let old_program_id = match old_globe.object.as_ref() {
            CopiedObject::GreenLeaf { children, .. } => children[0],
            CopiedObject::ProjectionProgram { .. } => panic!("query must return a green leaf"),
        };
        assert!(
            delta
                .objects
                .iter()
                .all(|object| object.id != old_program_id)
        );

        let recovery_snapshot = prepare_full_snapshot_bundle(
            &arena,
            &target,
            recovery_publication,
            HostRevisionId(1),
            target_source,
        )
        .unwrap();
        old.release_later(&mut arena).unwrap();
        target.release_later(&mut arena).unwrap();
        settle_arena(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);

        let retry_delta = delta.clone();
        let delta_ack = host.apply_bundle(delta).unwrap();
        assert_eq!(delta_ack.leaf_count, 4);
        assert_eq!(delta_ack.splice_receipt.inserted_leaf_entries, 1);
        assert!(delta_ack.splice_receipt.tree_nodes_visited > 0);
        assert!(host.dirty.is_none());
        let shifted_globe = match host
            .query_metric(SerializedMetric { bytes: 3, utf16: 2 })
            .unwrap()
        {
            HostQuery::Structural(query) => query,
            HostQuery::SourceGap(_) => panic!("exact-current delta must clear fallback"),
        };
        assert_eq!(
            shifted_globe.prefix,
            SerializedMetric { bytes: 3, utf16: 2 }
        );
        assert!(Arc::ptr_eq(&shifted_globe.object, &old_globe_object));
        assert_eq!(shifted_globe.programs.len(), 1);
        assert!(Arc::ptr_eq(&shifted_globe.programs[0], &old_globe_program));
        assert_eq!(
            host.apply_bundle(retry_delta),
            Err(HostMirrorError::Backpressure)
        );

        let old_root = host
            .structural
            .as_ref()
            .unwrap()
            .sequence
            .root
            .clone()
            .unwrap();
        let mut corrupt_recovery = recovery_snapshot.clone();
        corrupt_recovery.objects[0].content_digest.0 ^= 1;
        assert!(matches!(
            host.apply_bundle(corrupt_recovery),
            Err(HostMirrorError::Invalid(_))
        ));
        assert_eq!(host.unacknowledged, Some(delta_ack));
        assert!(Arc::ptr_eq(
            &host
                .structural
                .as_ref()
                .unwrap()
                .sequence
                .root
                .clone()
                .unwrap(),
            &old_root,
        ));

        let recovery_ack = host.apply_bundle(recovery_snapshot).unwrap();
        assert_eq!(recovery_ack.session, recovery_publication);
        assert_eq!(host.unacknowledged, Some(recovery_ack));
        assert!(host.acknowledge_delivery(delta_ack).is_err());
        host.acknowledge_delivery(recovery_ack).unwrap();
        let recovered = match host
            .query_metric(SerializedMetric { bytes: 3, utf16: 2 })
            .unwrap()
        {
            HostQuery::Structural(query) => query,
            HostQuery::SourceGap(_) => panic!("recovery snapshot must be atomic and clean"),
        };
        assert_eq!(recovered.programs.len(), 1);
    }
}
