//! Immutable green-suffix adoption with spanning-Exit repair.
//!
//! This is deliberately a storage-mechanism proof, not parser convergence
//! authority.  The caller supplies two manifest-bound, exact packed-leaf
//! boundaries after independently proving grammar convergence.  Planning
//! borrows both documents, recovers current output only from the current green
//! prefix, and performs one `O(tree height)` repair step per open frame.  The
//! commit retains the current prefix and old suffix under one arena transaction
//! and rewrites only leaves whose spanning Exit bytes actually change.
//!
//! A complete current manifest is used as the target/source authority in this
//! prototype.  The live candidate builder intentionally does not expose its
//! private working-prefix owner yet; wiring that linear owner into this job is
//! a separate integration gate.

#[allow(clippy::wildcard_imports)]
use super::*;
#[cfg(feature = "exact-parser")]
use crate::committed_checkpoint_index::ParentBoundSourceConvergence;
use crate::{
    BoundaryAffinity, LineageAdoptionBundleJob, LineageAdoptionBundleProof, LiveCandidateEpoch,
    ProvenSourceMapping, SourcePhysicalLineQueryReceipt, SourceSnapshotDescriptor, SourceStore,
    SourceStoreError,
};
use std::cmp::Ordering;

/// Private friend token allowing this storage module alone to read the old
/// event cut from a parent-bound convergence probe.
pub(crate) struct ParentBoundGreenConvergenceMint(());

/// Exact, manifest-bound event cut that also falls between packed leaves.
///
/// The capability owns only bounded open-path output and scalar coordinates;
/// it owns no arena root and retains no source bytes.
#[must_use = "a suffix-adoption boundary must be consumed or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct GreenSuffixAdoptionBoundary {
    binding: SerializedGreenManifestDescriptor,
    output: GreenRestartOutputAtEventCut,
    prefix_leaves: u64,
    boundary_sequence_nodes_visited: usize,
}

impl GreenSuffixAdoptionBoundary {
    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.binding.manifest
    }

    #[must_use]
    pub const fn event_cut(&self) -> u64 {
        self.output.event_cut()
    }

    #[must_use]
    pub const fn prefix_leaves(&self) -> u64 {
        self.prefix_leaves
    }

    #[must_use]
    pub const fn source_metric(&self) -> SerializedMetric {
        self.output.source_metric()
    }

    #[must_use]
    pub const fn open_depth(&self) -> u64 {
        self.output.open_depth()
    }

    #[must_use]
    pub const fn restart_output_receipt(&self) -> &GreenRestartOutputReceipt {
        self.output.receipt()
    }

    #[must_use]
    pub const fn boundary_sequence_nodes_visited(&self) -> usize {
        self.boundary_sequence_nodes_visited
    }
}

/// Storage-derived description of one immutable old-green suffix.
///
/// This is deliberately not grammar-convergence or attachment authority. It
/// owns the exact old packed-leaf boundary and only folded manifest totals;
/// no source root, arena owner, event vector, or caller-provided metric enters
/// the value. The source/lineage join below must consume it before a candidate
/// can stop replaying at the corresponding current-source boundary.
#[must_use = "the green tail capability must join source lineage or be discarded"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GreenSourceTailAdoptionCapability {
    old_source: SourceSnapshotDescriptor,
    old_total: SerializedMetric,
    old_prefix: SerializedMetric,
    suffix: SerializedMetric,
    total_coverage_runs: u64,
    prefix_coverage_runs: u64,
    suffix_coverage_runs: u64,
    first_suffix_coverage: FirstSuffixCoverageAuthority,
    boundary: GreenSuffixAdoptionBoundary,
    receipt: GreenSourceTailAdoptionReceipt,
}

/// Exact first old-green run after the accepted semantic cut. It is not, by
/// itself, grammar authority. Joined with unchanged-tail lineage and the
/// current source bytes it can authenticate the narrow staged-terminator
/// predecessor that sits between composer cut A and parser cut P.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FirstSuffixCoverageAuthority {
    metric: SerializedMetric,
    owner: BlockId,
    owner_kind: GreenKind,
    part: CoveragePart,
    logical: FirstSuffixLogical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FirstSuffixLogical {
    None,
    Identity,
    Atomic(AtomicProjectionKind),
    ProgramPrefix(FirstSuffixProgramPrefix),
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FirstSuffixProgramPrefix {
    Identity {
        metric: SerializedMetric,
    },
    Atomic {
        metric: SerializedMetric,
        kind: AtomicProjectionKind,
    },
    Unsupported,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FirstSuffixProgramPrefixReceipt {
    pages_validated: usize,
    bytes_validated: usize,
    pieces_validated: usize,
    prefix_pieces_decoded: usize,
}

fn first_suffix_program_prefix(
    arena: &PageArena,
    coverage: &GreenCoverageView,
    program: ProjectionProgramCapability,
) -> Result<(FirstSuffixProgramPrefix, FirstSuffixProgramPrefixReceipt), SerializedGreenError> {
    let metric = SerializedMetric {
        bytes: coverage
            .byte_range
            .end
            .checked_sub(coverage.byte_range.start)
            .ok_or(SerializedGreenError::Corrupt(
                "first suffix byte range is reversed",
            ))?,
        utf16: coverage
            .utf16_range
            .end
            .checked_sub(coverage.utf16_range.start)
            .ok_or(SerializedGreenError::Corrupt(
                "first suffix UTF-16 range is reversed",
            ))?,
    };
    if program.manifest != coverage.cursor.manifest
        || program.leaf != coverage.cursor.leaf
        || arena.packed_child_at(program.leaf, usize::from(program.edge_ordinal))? != program.page
        || program.piece_count == 0
        || program.physical_metric != metric
    {
        return Err(SerializedGreenError::Corrupt(
            "first suffix Program does not match its coverage edge",
        ));
    }
    let first_piece_offset = validate_projection_program_edge_payload(
        arena,
        program.page,
        usize::from(program.piece_count),
        program.physical_metric,
        program.logical_metric,
    )?;
    let payload = arena.payload(program.page)?;
    let mut decoder = Decoder::new(payload);
    decoder.cursor = first_piece_offset;
    let prefix = match decode_projection_piece(&mut decoder)? {
        ProjectionPiece::Identity { metric } => FirstSuffixProgramPrefix::Identity { metric },
        ProjectionPiece::Atomic {
            physical_metric,
            projection,
        } => FirstSuffixProgramPrefix::Atomic {
            metric: physical_metric,
            kind: projection.kind,
        },
        ProjectionPiece::Hidden { .. } | ProjectionPiece::Virtual { .. } => {
            FirstSuffixProgramPrefix::Unsupported
        }
    };
    Ok((
        prefix,
        FirstSuffixProgramPrefixReceipt {
            pages_validated: 1,
            bytes_validated: payload.len(),
            pieces_validated: usize::from(program.piece_count),
            prefix_pieces_decoded: 1,
        },
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GreenDeferredLineEnding {
    Lf,
    LoneCr,
    CrLf,
}

impl GreenDeferredLineEnding {
    const fn metric(self) -> SerializedMetric {
        match self {
            Self::Lf | Self::LoneCr => SerializedMetric { bytes: 1, utf16: 1 },
            Self::CrLf => SerializedMetric { bytes: 2, utf16: 2 },
        }
    }
}

/// Lineage- and source-bound description of the one deferred terminator. The
/// real ledger must still match its certified atom, owner, and accepted/
/// physical cuts before the predecessor can move into the retained suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GreenDeferredTerminatorAuthority {
    ending: GreenDeferredLineEnding,
    owner: BlockId,
    owner_kind: GreenKind,
    part: CoveragePart,
}

impl GreenDeferredTerminatorAuthority {
    pub(crate) const fn ending(self) -> GreenDeferredLineEnding {
        self.ending
    }

    pub(crate) const fn owner(self) -> BlockId {
        self.owner
    }

    pub(crate) const fn owner_kind(self) -> GreenKind {
        self.owner_kind
    }

    pub(crate) const fn part(self) -> CoveragePart {
        self.part
    }
}

impl FirstSuffixCoverageAuthority {
    fn bind_deferred_terminator(
        self,
        ending: GreenDeferredLineEnding,
    ) -> Result<GreenDeferredTerminatorAuthority, TailAdoptionJoinError> {
        let ending_metric = ending.metric();
        let metric_matches = match self.logical {
            // Identity coverage may legitimately coalesce the deferred line
            // ending with following unchanged text. The exact suffix leaf and
            // source lineage prove that the run starts at A; current source
            // bytes prove the ending is its physical prefix. No storage split
            // or new projection run is needed.
            FirstSuffixLogical::Identity => {
                self.metric.bytes >= ending_metric.bytes && self.metric.utf16 >= ending_metric.utf16
            }
            FirstSuffixLogical::ProgramPrefix(_) => {
                self.metric.bytes >= ending_metric.bytes && self.metric.utf16 >= ending_metric.utf16
            }
            FirstSuffixLogical::None
            | FirstSuffixLogical::Atomic(_)
            | FirstSuffixLogical::Unsupported => self.metric == ending_metric,
        };
        let logical_matches = match (ending, self.logical) {
            (_, FirstSuffixLogical::None) => self.part == CoveragePart::TERMINAL,
            (GreenDeferredLineEnding::Lf, FirstSuffixLogical::Identity) => {
                matches!(self.part, CoveragePart::CONTENT | CoveragePart::TERMINAL)
            }
            (
                GreenDeferredLineEnding::LoneCr,
                FirstSuffixLogical::Atomic(AtomicProjectionKind::LoneCrToLf),
            )
            | (
                GreenDeferredLineEnding::CrLf,
                FirstSuffixLogical::Atomic(AtomicProjectionKind::CrLfToLf),
            ) => matches!(self.part, CoveragePart::CONTENT | CoveragePart::TERMINAL),
            (
                GreenDeferredLineEnding::Lf,
                FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Identity { metric }),
            ) if metric.bytes >= ending_metric.bytes && metric.utf16 >= ending_metric.utf16 => {
                matches!(self.part, CoveragePart::CONTENT | CoveragePart::TERMINAL)
            }
            (
                GreenDeferredLineEnding::LoneCr,
                FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Atomic {
                    metric,
                    kind: AtomicProjectionKind::LoneCrToLf,
                }),
            )
            | (
                GreenDeferredLineEnding::CrLf,
                FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Atomic {
                    metric,
                    kind: AtomicProjectionKind::CrLfToLf,
                }),
            ) if metric == ending_metric => {
                matches!(self.part, CoveragePart::CONTENT | CoveragePart::TERMINAL)
            }
            _ => false,
        };
        if !metric_matches || !logical_matches {
            return Err(TailAdoptionJoinError::DeferredTerminatorMismatch);
        }
        Ok(GreenDeferredTerminatorAuthority {
            ending,
            owner: self.owner,
            owner_kind: self.owner_kind,
            part: self.part,
        })
    }
}

/// Work performed to obtain and source-bind a large immutable tail. Logical
/// suffix length is reported separately from work so tests can prove that a
/// 10 MiB tail does not cause a 10 MiB scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GreenSourceTailAdoptionReceipt {
    pub(crate) described_suffix_bytes: u64,
    pub(crate) described_suffix_utf16: u64,
    pub(crate) green_sequence_nodes_visited: usize,
    pub(crate) green_leaf_pages_decoded: usize,
    pub(crate) green_events_decoded: usize,
    pub(crate) green_projection_program_pages_validated: usize,
    pub(crate) green_projection_program_bytes_validated: usize,
    pub(crate) green_projection_program_pieces_validated: usize,
    pub(crate) green_projection_prefix_pieces_decoded: usize,
    pub(crate) green_maximum_route_depth: usize,
    pub(crate) green_maximum_open_depth: usize,
    pub(crate) source_queries: usize,
    pub(crate) source_index_nodes_visited: usize,
    pub(crate) source_summary_subtrees_reused: usize,
    pub(crate) source_boundary_bytes_scanned: usize,
    pub(crate) source_adjacent_bytes_read: usize,
    pub(crate) source_index_height: usize,
    pub(crate) retained_source_roots: usize,
    pub(crate) retained_source_bytes: usize,
    pub(crate) document_sized_event_vectors: usize,
}

impl GreenSourceTailAdoptionCapability {
    pub(crate) const fn old_source(&self) -> SourceSnapshotDescriptor {
        self.old_source
    }

    pub(crate) fn old_convergence_bytes(&self) -> Result<usize, TailAdoptionJoinError> {
        usize::try_from(self.old_prefix.bytes)
            .map_err(|_| TailAdoptionJoinError::Overflow("old convergence bytes"))
    }

    pub(crate) const fn arena_identity(&self) -> crate::ArenaIdentity {
        self.boundary.output.manifest().scoped().arena()
    }

    pub(crate) const fn old_prefix(&self) -> SerializedMetric {
        self.old_prefix
    }

    pub(crate) const fn suffix(&self) -> SerializedMetric {
        self.suffix
    }

    pub(crate) const fn suffix_coverage_runs(&self) -> u64 {
        self.suffix_coverage_runs
    }

    /// Complete current-prefix coverage count at semantic cut A, derived from
    /// the immutable old green root. A restarted composer uses this only to
    /// verify `checkpoint base + suffix-local replay`; it is never accepted as
    /// the checkpoint base by itself.
    pub(crate) const fn prefix_coverage_runs(&self) -> u64 {
        self.prefix_coverage_runs
    }

    pub(crate) fn open_depth(&self) -> usize {
        self.boundary.output.frames().len()
    }

    pub(crate) fn open_frames(&self) -> &[GreenRestartOutputFrame] {
        self.boundary.output.frames()
    }

    pub(crate) const fn receipt(&self) -> GreenSourceTailAdoptionReceipt {
        self.receipt
    }

    #[cfg(feature = "exact-parser")]
    pub(super) fn green_journal_suffix_view(
        &self,
        _mint: super::green_journal_suffix_splice::GreenJournalSuffixMint,
    ) -> super::green_journal_suffix_splice::GreenJournalSuffixView<'_> {
        super::green_journal_suffix_splice::GreenJournalSuffixView {
            binding: self.boundary.binding,
            old_total: self.old_total,
            old_prefix: self.old_prefix,
            suffix: self.suffix,
            total_coverage_runs: self.total_coverage_runs,
            prefix_coverage_runs: self.prefix_coverage_runs,
            suffix_coverage_runs: self.suffix_coverage_runs,
            prefix_leaves: self.boundary.prefix_leaves,
            event_cut: self.boundary.output.event_cut(),
            block_enters_before: self.boundary.output.blocks(),
            frames: self.boundary.output.frames(),
            source_receipt: self.receipt,
        }
    }

    /// Linear handoff into the journal-owned green splice. The friend mint is
    /// constructible only by that descendant module, so neither the parser nor
    /// the candidate actor can extract or pair the private packed boundary
    /// scalars independently.
    #[cfg(feature = "exact-parser")]
    pub(super) fn into_green_journal_suffix_parts(
        self,
        _mint: super::green_journal_suffix_splice::GreenJournalSuffixMint,
    ) -> super::green_journal_suffix_splice::GreenJournalSuffixParts {
        let Self {
            old_total,
            old_prefix,
            suffix,
            total_coverage_runs,
            prefix_coverage_runs,
            suffix_coverage_runs,
            boundary,
            receipt,
            ..
        } = self;
        let GreenSuffixAdoptionBoundary {
            binding,
            output,
            prefix_leaves,
            ..
        } = boundary;
        let output = output.into_parts();
        super::green_journal_suffix_splice::GreenJournalSuffixParts {
            binding,
            old_total,
            old_prefix,
            suffix,
            total_coverage_runs,
            prefix_coverage_runs,
            suffix_coverage_runs,
            prefix_leaves,
            event_cut: output.event_cut,
            block_enters_before: output.blocks,
            frames: output.frames,
            source_receipt: receipt,
        }
    }

    /// Starts the proof-harness zero-restart lineage pass without accepting a
    /// caller-authored convergence coordinate. The general product path will
    /// replace the fixed zero restart with the selected composite checkpoint,
    /// while retaining this exact storage-derived convergence cut.
    pub(crate) fn begin_zero_restart_lineage(
        &self,
        source: &SourceStore,
    ) -> Result<LineageAdoptionBundleJob, TailAdoptionJoinError> {
        let convergence = self.old_convergence_bytes()?;
        source
            .begin_lineage_adoption_bundle(self.old_source, 0, convergence, BoundaryAffinity::After)
            .map_err(|_| TailAdoptionJoinError::LineageMismatch)
    }

    /// Consumes storage and lineage into the only source-bound tail authority
    /// accepted by the candidate ledger. Current UTF-16 and physical-line
    /// totals come from persistent source summaries, never from the caller or
    /// from scanning the retained tail.
    pub(crate) fn join_current_source(
        mut self,
        source: &SourceStore,
        epoch: LiveCandidateEpoch,
        lineage: LineageAdoptionBundleProof,
    ) -> Result<SourceBoundGreenTailAdoption, TailAdoptionJoinError> {
        let actual = source.descriptor();
        if epoch.source() != actual
            || epoch.arena_identity() != self.arena_identity()
            || lineage.from() != self.old_source
            || lineage.to() != actual
        {
            return Err(TailAdoptionJoinError::WrongCandidate);
        }

        let (old_convergence, current_convergence, affinity) = match lineage.convergence() {
            ProvenSourceMapping::Boundary { from, to, affinity } => (*from, *to, *affinity),
            ProvenSourceMapping::Range { .. } => {
                return Err(TailAdoptionJoinError::LineageMismatch);
            }
        };
        let (old_tail, current_tail) = match lineage.tail() {
            ProvenSourceMapping::Range { from, to } => (from, to),
            ProvenSourceMapping::Boundary { .. } => {
                return Err(TailAdoptionJoinError::LineageMismatch);
            }
        };
        let old_prefix_bytes = usize::try_from(self.old_prefix.bytes)
            .map_err(|_| TailAdoptionJoinError::Overflow("old convergence bytes"))?;
        let suffix_bytes = usize::try_from(self.suffix.bytes)
            .map_err(|_| TailAdoptionJoinError::Overflow("suffix bytes"))?;
        if affinity != BoundaryAffinity::After
            || old_convergence != old_prefix_bytes
            || old_tail.start != old_convergence
            || old_tail.end != self.old_source.bytes
            || current_tail.start != current_convergence
            || current_tail.end != actual.bytes
            || old_tail.len() != suffix_bytes
            || current_tail.len() != suffix_bytes
        {
            return Err(TailAdoptionJoinError::LineageMismatch);
        }

        let prefix_metric = source
            .observe_prefix_metric_at(current_convergence)
            .map_err(SourceStoreError::from)?;
        self.receipt.source_queries = self
            .receipt
            .source_queries
            .checked_add(1)
            .ok_or(TailAdoptionJoinError::Overflow("source tail queries"))?;
        if prefix_metric.root != actual.root || prefix_metric.bytes != current_convergence {
            return Err(TailAdoptionJoinError::WrongCandidate);
        }
        let current_prefix = SerializedMetric {
            bytes: u64::try_from(prefix_metric.bytes)
                .map_err(|_| TailAdoptionJoinError::Overflow("current prefix bytes"))?,
            utf16: u64::try_from(prefix_metric.utf16)
                .map_err(|_| TailAdoptionJoinError::Overflow("current prefix UTF-16"))?,
        };
        let final_metric = current_prefix
            .checked_add(self.suffix)
            .map_err(TailAdoptionJoinError::Green)?;
        if final_metric.bytes
            != u64::try_from(actual.bytes)
                .map_err(|_| TailAdoptionJoinError::Overflow("current source bytes"))?
        {
            return Err(TailAdoptionJoinError::LineageMismatch);
        }

        // The joined parser checkpoint is physically after a staged line
        // ending (P), while green/composer coverage is still before it (A).
        // Read at most the two ending bytes at A; the first old suffix run and
        // the real source ledger must independently authenticate the same
        // atom. An identity run may continue beyond that atom; the unchanged tail
        // and exact run start make the terminator a proven prefix without scanning
        // or retaining tail bytes.
        let first = source
            .observe_byte_at(current_convergence)
            .map_err(SourceStoreError::from)?;
        let mut byte_queries = 1_usize;
        let mut adjacent_bytes_read = usize::from(first.is_some());
        let ending = match first {
            Some(byte) if byte.root != actual.root || byte.offset != current_convergence => {
                return Err(TailAdoptionJoinError::WrongCandidate);
            }
            Some(byte) if byte.byte == b'\n' => Some(GreenDeferredLineEnding::Lf),
            Some(byte) if byte.byte == b'\r' => {
                let next_offset = current_convergence
                    .checked_add(1)
                    .ok_or(TailAdoptionJoinError::Overflow("deferred terminator cut"))?;
                let next = source
                    .observe_byte_at(next_offset)
                    .map_err(SourceStoreError::from)?;
                byte_queries = byte_queries
                    .checked_add(1)
                    .ok_or(TailAdoptionJoinError::Overflow("source tail queries"))?;
                adjacent_bytes_read = adjacent_bytes_read
                    .checked_add(usize::from(next.is_some()))
                    .ok_or(TailAdoptionJoinError::Overflow(
                        "deferred terminator observation",
                    ))?;
                if next.is_some_and(|next| {
                    next.root == actual.root && next.offset == next_offset && next.byte == b'\n'
                }) {
                    Some(GreenDeferredLineEnding::CrLf)
                } else {
                    Some(GreenDeferredLineEnding::LoneCr)
                }
            }
            Some(_) | None => None,
        };
        self.receipt.source_queries = self
            .receipt
            .source_queries
            .checked_add(byte_queries)
            .ok_or(TailAdoptionJoinError::Overflow("source tail queries"))?;
        self.receipt.source_adjacent_bytes_read = self
            .receipt
            .source_adjacent_bytes_read
            .checked_add(adjacent_bytes_read)
            .ok_or(TailAdoptionJoinError::Overflow(
                "source adjacent bytes read",
            ))?;
        let deferred_terminator = ending
            .map(|ending| self.first_suffix_coverage.bind_deferred_terminator(ending))
            .transpose()?;
        let physical_convergence = match deferred_terminator {
            Some(deferred) => current_convergence
                .checked_add(
                    usize::try_from(deferred.ending().metric().bytes)
                        .map_err(|_| TailAdoptionJoinError::Overflow("terminator bytes"))?,
                )
                .ok_or(TailAdoptionJoinError::Overflow("physical convergence cut"))?,
            None => current_convergence,
        };
        let physical_metric = source
            .observe_prefix_metric_at(physical_convergence)
            .map_err(SourceStoreError::from)?;
        self.receipt.source_queries = self
            .receipt
            .source_queries
            .checked_add(1)
            .ok_or(TailAdoptionJoinError::Overflow("source tail queries"))?;
        if physical_metric.root != actual.root || physical_metric.bytes != physical_convergence {
            return Err(TailAdoptionJoinError::WrongCandidate);
        }
        let physical_prefix = SerializedMetric {
            bytes: u64::try_from(physical_metric.bytes)
                .map_err(|_| TailAdoptionJoinError::Overflow("physical prefix bytes"))?,
            utf16: u64::try_from(physical_metric.utf16)
                .map_err(|_| TailAdoptionJoinError::Overflow("physical prefix UTF-16"))?,
        };

        let convergence_cut = source
            .certify_current_byte_cut(actual, physical_convergence)
            .map_err(TailAdoptionJoinError::Source)?;
        let convergence_line = source
            .query_physical_line_at_cut(convergence_cut)
            .map_err(TailAdoptionJoinError::Source)?;
        if !convergence_line.is_physical_line_start() {
            return Err(TailAdoptionJoinError::NotPhysicalLineBoundary);
        }
        let eof_cut = source
            .certify_current_byte_cut(actual, actual.bytes)
            .map_err(TailAdoptionJoinError::Source)?;
        let eof_line = source
            .query_physical_line_at_cut(eof_cut)
            .map_err(TailAdoptionJoinError::Source)?;
        let total_line_count = if actual.bytes == 0 || eof_line.is_physical_line_start() {
            eof_line.line_ordinal()
        } else {
            eof_line
                .line_ordinal()
                .checked_add(1)
                .ok_or(TailAdoptionJoinError::Overflow("current physical lines"))?
        };
        merge_source_query_receipt(&mut self.receipt, convergence_line.receipt())?;
        merge_source_query_receipt(&mut self.receipt, eof_line.receipt())?;
        if self.receipt.retained_source_roots != 0 || self.receipt.retained_source_bytes != 0 {
            return Err(TailAdoptionJoinError::LineageMismatch);
        }
        Ok(SourceBoundGreenTailAdoption {
            epoch,
            old_source: self.old_source,
            current_prefix,
            physical_prefix,
            final_metric,
            current_line_ordinal: convergence_line.line_ordinal(),
            total_line_count,
            deferred_terminator,
            current_open_blocks: None,
            current_deferred_owner: None,
            storage: self,
        })
    }
}

/// Source- and build-bound storage tail accepted only by the actual candidate
/// ledger/composer line-boundary continuations.
#[must_use = "the source-bound green tail must enter candidate adoption or be discarded"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceBoundGreenTailAdoption {
    epoch: LiveCandidateEpoch,
    old_source: SourceSnapshotDescriptor,
    /// Semantic green/composer cut A. Retained coverage begins here.
    current_prefix: SerializedMetric,
    /// Physical parser/decoder cut P. For the narrow deferred-terminator form,
    /// P follows A by exactly one authenticated LF/lone-CR/CRLF atom.
    physical_prefix: SerializedMetric,
    final_metric: SerializedMetric,
    current_line_ordinal: u64,
    total_line_count: u64,
    deferred_terminator: Option<GreenDeferredTerminatorAuthority>,
    /// Candidate identities rebound by relative open depth. Packed Coverage
    /// stores that same relative depth rather than a block scalar, so this is
    /// the exact current-path interpretation of the immutable old suffix.
    current_open_blocks: Option<Vec<BlockId>>,
    current_deferred_owner: Option<BlockId>,
    storage: GreenSourceTailAdoptionCapability,
}

impl SourceBoundGreenTailAdoption {
    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    pub(crate) const fn current_prefix(&self) -> SerializedMetric {
        self.current_prefix
    }

    pub(crate) const fn physical_prefix(&self) -> SerializedMetric {
        self.physical_prefix
    }

    pub(crate) const fn final_metric(&self) -> SerializedMetric {
        self.final_metric
    }

    pub(crate) const fn current_line_ordinal(&self) -> u64 {
        self.current_line_ordinal
    }

    pub(crate) const fn total_line_count(&self) -> u64 {
        self.total_line_count
    }

    pub(crate) const fn deferred_terminator(&self) -> Option<GreenDeferredTerminatorAuthority> {
        self.deferred_terminator
    }

    pub(crate) fn current_open_block(&self, depth: usize) -> Option<BlockId> {
        self.current_open_blocks
            .as_ref()
            .and_then(|blocks| blocks.get(depth).copied())
    }

    pub(crate) const fn current_deferred_owner(&self) -> Option<BlockId> {
        self.current_deferred_owner
    }

    /// Reinterprets immutable relative-depth Coverage against the exact live
    /// candidate path. The friend mint is constructible only beside the
    /// source ledger that owns those current path stamps.
    pub(crate) fn rebind_current_open_path(
        &mut self,
        _mint: crate::source_bound_ledger::GreenTailOpenPathRebindMint,
        current: impl ExactSizeIterator<Item = (BlockId, GreenKind)>,
    ) -> Result<(), TailAdoptionJoinError> {
        if current.len() != self.storage.open_frames().len() {
            return Err(TailAdoptionJoinError::WrongCandidate);
        }
        let mut blocks = Vec::new();
        blocks
            .try_reserve_exact(current.len())
            .map_err(|_| TailAdoptionJoinError::Overflow("current green open path"))?;
        for ((block, kind), old) in current.zip(self.storage.open_frames()) {
            if block.0 == 0 || kind != old.kind() {
                return Err(TailAdoptionJoinError::WrongCandidate);
            }
            blocks.push(block);
        }
        let current_deferred_owner = if let Some(authority) = self.deferred_terminator {
            let depth = self
                .storage
                .open_frames()
                .iter()
                .position(|frame| {
                    frame.block() == authority.owner() && frame.kind() == authority.owner_kind()
                })
                .ok_or(TailAdoptionJoinError::WrongCandidate)?;
            Some(
                *blocks
                    .get(depth)
                    .ok_or(TailAdoptionJoinError::WrongCandidate)?,
            )
        } else {
            None
        };
        if let Some(bound) = self.current_open_blocks.as_ref() {
            if bound != &blocks || self.current_deferred_owner != current_deferred_owner {
                return Err(TailAdoptionJoinError::WrongCandidate);
            }
        } else {
            self.current_open_blocks = Some(blocks);
            self.current_deferred_owner = current_deferred_owner;
        }
        Ok(())
    }

    pub(crate) fn open_depth(&self) -> usize {
        self.storage.open_depth()
    }

    pub(crate) fn open_frames(&self) -> &[GreenRestartOutputFrame] {
        self.storage.open_frames()
    }

    pub(crate) const fn suffix_coverage_runs(&self) -> u64 {
        self.storage.suffix_coverage_runs()
    }

    pub(crate) const fn prefix_coverage_runs(&self) -> u64 {
        self.storage.prefix_coverage_runs()
    }

    pub(crate) const fn receipt(&self) -> GreenSourceTailAdoptionReceipt {
        self.storage.receipt()
    }

    #[cfg(feature = "exact-parser")]
    pub(super) fn green_journal_suffix_view(
        &self,
        mint: super::green_journal_suffix_splice::GreenJournalSuffixMint,
    ) -> super::green_journal_suffix_splice::GreenJournalSuffixView<'_> {
        self.storage.green_journal_suffix_view(mint)
    }

    pub(crate) fn into_composer_authority(self) -> GreenComposerTailAdoptionAuthority {
        GreenComposerTailAdoptionAuthority {
            epoch: self.epoch,
            current_prefix: self.current_prefix,
            final_metric: self.final_metric,
            suffix_coverage_runs: self.storage.suffix_coverage_runs,
            storage: self.storage,
        }
    }
}

/// Remainder of the linear authority after the source continuation has been
/// consumed. It must next consume the real composer continuation and keeps the
/// old packed boundary for the future green/index journal join.
#[must_use = "the composer tail authority must enter composer adoption or be discarded"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GreenComposerTailAdoptionAuthority {
    epoch: LiveCandidateEpoch,
    current_prefix: SerializedMetric,
    final_metric: SerializedMetric,
    suffix_coverage_runs: u64,
    storage: GreenSourceTailAdoptionCapability,
}

impl GreenComposerTailAdoptionAuthority {
    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    pub(crate) const fn current_prefix(&self) -> SerializedMetric {
        self.current_prefix
    }

    pub(crate) const fn final_metric(&self) -> SerializedMetric {
        self.final_metric
    }

    pub(crate) const fn suffix_coverage_runs(&self) -> u64 {
        self.suffix_coverage_runs
    }

    pub(crate) const fn prefix_coverage_runs(&self) -> u64 {
        self.storage.prefix_coverage_runs
    }

    pub(crate) const fn receipt(&self) -> GreenSourceTailAdoptionReceipt {
        self.storage.receipt()
    }

    pub(crate) fn into_storage(self) -> GreenSourceTailAdoptionCapability {
        self.storage
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TailAdoptionJoinError {
    Green(SerializedGreenError),
    Source(SourceStoreError),
    WrongCandidate,
    LineageMismatch,
    DeferredTerminatorMismatch,
    NotPhysicalLineBoundary,
    Overflow(&'static str),
}

impl From<SerializedGreenError> for TailAdoptionJoinError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

impl From<SourceStoreError> for TailAdoptionJoinError {
    fn from(error: SourceStoreError) -> Self {
        Self::Source(error)
    }
}

fn merge_source_query_receipt(
    target: &mut GreenSourceTailAdoptionReceipt,
    source: SourcePhysicalLineQueryReceipt,
) -> Result<(), TailAdoptionJoinError> {
    target.source_queries = target
        .source_queries
        .checked_add(1)
        .ok_or(TailAdoptionJoinError::Overflow("source tail queries"))?;
    target.source_index_nodes_visited = target
        .source_index_nodes_visited
        .checked_add(source.tree_nodes_visited)
        .ok_or(TailAdoptionJoinError::Overflow(
            "source index nodes visited",
        ))?;
    target.source_summary_subtrees_reused = target
        .source_summary_subtrees_reused
        .checked_add(source.summary_subtrees_reused)
        .ok_or(TailAdoptionJoinError::Overflow(
            "source summary subtrees reused",
        ))?;
    target.source_boundary_bytes_scanned = target
        .source_boundary_bytes_scanned
        .checked_add(source.boundary_bytes_scanned)
        .ok_or(TailAdoptionJoinError::Overflow(
            "source boundary bytes scanned",
        ))?;
    target.source_adjacent_bytes_read = target
        .source_adjacent_bytes_read
        .checked_add(source.adjacent_bytes_read)
        .ok_or(TailAdoptionJoinError::Overflow(
            "source adjacent bytes read",
        ))?;
    target.source_index_height = target.source_index_height.max(source.index_height);
    target.retained_source_roots = target
        .retained_source_roots
        .checked_add(source.retained_source_roots)
        .ok_or(TailAdoptionJoinError::Overflow("retained source roots"))?;
    target.retained_source_bytes = target
        .retained_source_bytes
        .checked_add(source.retained_source_bytes)
        .ok_or(TailAdoptionJoinError::Overflow("retained source bytes"))?;
    Ok(())
}

/// One-frame-at-a-time progress for immutable suffix-adoption planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenSuffixAdoptionPlanProgress {
    Pending,
    Ready,
}

/// Exact work and scratch receipt for one suffix adoption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenSuffixAdoptionReceipt {
    pub open_depth: usize,
    pub frames_planned: usize,
    pub planning_polls: usize,
    pub sequence_nodes_visited: usize,
    pub summary_nodes_reused: usize,
    pub leaf_pages_decoded: usize,
    pub events_decoded: usize,
    pub maximum_decoded_page_bytes: usize,
    pub maximum_route_depth: usize,
    pub spanning_exits_examined: usize,
    pub exit_events_changed: usize,
    pub distinct_exit_leaves_rewritten: usize,
    pub current_prefix_leaves_retained: u64,
    pub old_suffix_leaves_retained: u64,
    pub unchanged_old_suffix_leaves: u64,
    pub retained_source_bytes: usize,
    pub document_sized_event_vectors: usize,
    pub maximum_rewrite_scratch_bytes: usize,
    pub build: SerializedGreenBuildReceipt,
}

/// Committed immutable adopted root and its auditable work receipt.
#[must_use = "the adopted green document owns an arena reference"]
#[derive(Debug)]
pub struct GreenSuffixAdoptionCommit {
    pub document: SerializedGreenDocument,
    pub receipt: GreenSuffixAdoptionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpanningExitLocation {
    pub(super) leaf: ArenaId,
    pub(super) leaf_index: u64,
    pub(super) byte_offset: u16,
    pub(super) event_ordinal: u64,
    pub(super) closed: ClosedChildAggregate,
    pub(super) last_line_blank: bool,
    pub(super) facts: GreenCloseFacts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlannedExitRewrite {
    pub(super) location: SpanningExitLocation,
    pub(super) replacement_closed: ClosedChildAggregate,
    pub(super) replacement_facts: GreenCloseFacts,
}

/// The exact output fields needed to repair a spanning Exit.  The live
/// candidate's builder snapshot and an immutable document restart query both
/// implement this view; neither path needs to exchange block identities or
/// parser-construct flags with the storage planner.
pub(super) trait GreenSpanningFrame {
    fn spanning_kind(&self) -> GreenKind;
    fn spanning_facts(&self) -> &FactsEnvelope;
    fn spanning_closed_children(&self) -> ChildSequenceAggregate;
}

impl GreenSpanningFrame for GreenRestartOutputFrame {
    fn spanning_kind(&self) -> GreenKind {
        self.kind()
    }

    fn spanning_facts(&self) -> &FactsEnvelope {
        self.facts()
    }

    fn spanning_closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children()
    }
}

#[cfg(feature = "exact-parser")]
impl GreenSpanningFrame for BuilderGreenPrefixFrame {
    fn spanning_kind(&self) -> GreenKind {
        self.kind()
    }

    fn spanning_facts(&self) -> &FactsEnvelope {
        self.facts()
    }

    fn spanning_closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children()
    }
}

/// Shared inside-out repair state used by both the completed-document proof
/// and the journalled live-candidate splice.  It owns only O(open-depth)
/// metadata.  Every poll locates and folds one retained spanning Exit using
/// persistent summaries, so work never scales with suffix length.
#[derive(Debug)]
pub(super) struct GreenSpanningExitRepairPlanner {
    next_frame: Option<usize>,
    next_segment_start: u64,
    repaired_inner_child: Option<ClosedChildAggregate>,
    rewrites: Vec<PlannedExitRewrite>,
    ready: bool,
    receipt: GreenSuffixAdoptionReceipt,
}

impl GreenSpanningExitRepairPlanner {
    pub(super) fn new(
        open_depth: usize,
        event_cut: u64,
        mut receipt: GreenSuffixAdoptionReceipt,
    ) -> Result<Self, SerializedGreenError> {
        if open_depth == 0 {
            return Err(SerializedGreenError::Invalid(
                "spanning-Exit repair requires a nonempty open path",
            ));
        }
        let mut rewrites = Vec::new();
        rewrites.try_reserve_exact(open_depth).map_err(|_| {
            SerializedGreenError::Invalid("suffix-adoption rewrite reservation failed")
        })?;
        receipt.open_depth = open_depth;
        Ok(Self {
            next_frame: open_depth.checked_sub(1),
            next_segment_start: event_cut,
            repaired_inner_child: None,
            rewrites,
            ready: false,
            receipt,
        })
    }

    pub(super) const fn receipt(&self) -> &GreenSuffixAdoptionReceipt {
        &self.receipt
    }

    pub(super) const fn is_ready(&self) -> bool {
        self.ready
    }

    pub(super) const fn repaired_root_child(&self) -> Option<ClosedChildAggregate> {
        if self.ready {
            self.repaired_inner_child
        } else {
            None
        }
    }

    pub(super) fn into_parts(
        self,
    ) -> Result<
        (
            Vec<PlannedExitRewrite>,
            ClosedChildAggregate,
            GreenSuffixAdoptionReceipt,
        ),
        SerializedGreenError,
    > {
        if !self.ready || self.receipt.frames_planned != self.receipt.open_depth {
            return Err(SerializedGreenError::Invalid(
                "spanning-Exit repair plan is not complete",
            ));
        }
        let root = self
            .repaired_inner_child
            .ok_or(SerializedGreenError::Corrupt(
                "spanning-Exit repair lost its root aggregate",
            ))?;
        Ok((self.rewrites, root, self.receipt))
    }

    /// Performs exactly one spanning-frame repair. The caller supplies only
    /// storage-authenticated old-root coordinates and the two independently
    /// captured open output paths. Block identities are deliberately absent:
    /// retained suffix Exits are addressed by relative depth.
    #[allow(clippy::too_many_lines)]
    pub(super) fn poll<CurrentFrame, OldFrame>(
        &mut self,
        arena: &PageArena,
        old_root: ArenaId,
        old_prefix_leaves: u64,
        current_frames: &[CurrentFrame],
        old_frames: &[OldFrame],
    ) -> Result<GreenSuffixAdoptionPlanProgress, SerializedGreenError>
    where
        CurrentFrame: GreenSpanningFrame,
        OldFrame: GreenSpanningFrame,
    {
        if self.ready {
            return Ok(GreenSuffixAdoptionPlanProgress::Ready);
        }
        if current_frames.len() != self.receipt.open_depth
            || old_frames.len() != self.receipt.open_depth
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        let frame_index = self.next_frame.ok_or(SerializedGreenError::Corrupt(
            "suffix-adoption planner lost its next frame",
        ))?;
        self.receipt.planning_polls =
            self.receipt
                .planning_polls
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption planning poll receipt",
                ))?;

        let target_depth =
            old_frames
                .len()
                .checked_sub(frame_index)
                .ok_or(SerializedGreenError::Corrupt(
                    "suffix-adoption frame depth underflow",
                ))?;
        let relative_target = i64::try_from(target_depth)
            .map_err(|_| SerializedGreenError::Overflow("suffix-adoption frame depth"))?
            .checked_neg()
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption relative close depth",
            ))?;
        let exit = find_spanning_exit(
            arena,
            old_root,
            old_prefix_leaves,
            relative_target,
            &mut self.receipt,
        )?;
        if exit.event_ordinal < self.next_segment_start {
            return Err(SerializedGreenError::Corrupt(
                "spanning Exits are not in source order",
            ));
        }

        let old_frame = &old_frames[frame_index];
        let current_frame = &current_frames[frame_index];
        if old_frame.spanning_kind() != current_frame.spanning_kind()
            || old_frame.spanning_facts() != current_frame.spanning_facts()
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        if current_frame.spanning_kind() == GreenKind::FENCED_CODE
            || matches!(exit.facts, GreenCloseFacts::FencedCode(_))
        {
            return Err(SerializedGreenError::Invalid(
                "spanning FencedCode Exit repair requires typed boundary markers",
            ));
        }
        exit.facts
            .validate_for_kind(current_frame.spanning_kind())
            .map_err(|_| {
                SerializedGreenError::Corrupt("spanning Exit facts changed after validation")
            })?;

        let mut range_receipt = GreenRestartOutputReceipt::default();
        let root = load_query_node(arena, old_root, &mut range_receipt)?;
        let suffix_range = event_range_summary(
            arena,
            root,
            self.next_segment_start,
            exit.event_ordinal,
            &mut range_receipt,
        )?;
        merge_query_receipt(&mut self.receipt, range_receipt)?;
        let suffix_children = balanced_direct_children(suffix_range)?;
        let mut children = current_frame.spanning_closed_children();
        if let Some(inner) = self.repaired_inner_child {
            children = children.followed_by(ChildSequenceAggregate::singleton(inner));
        }
        children = children.followed_by(suffix_children);
        let semantics = ContainerFoldSemantics {
            descends_through_last_child: matches!(
                current_frame.spanning_kind(),
                GreenKind::LIST | GreenKind::ITEM
            ),
            is_item: current_frame.spanning_kind() == GreenKind::ITEM,
            // The live caller reaches this planner only after complete
            // line-local donor-C equality. Storage therefore preserves the
            // authenticated retained close's blank-line bit.
            last_line_blank: exit.last_line_blank,
        };
        let replacement_closed = semantics.closed_summary(children);
        let replacement_facts = match exit.facts {
            GreenCloseFacts::None => GreenCloseFacts::None,
            GreenCloseFacts::List { .. } => GreenCloseFacts::List {
                tight: children.list_is_tight(),
            },
            GreenCloseFacts::FencedCode(_) => {
                return Err(SerializedGreenError::Invalid(
                    "spanning FencedCode Exit repair requires typed boundary markers",
                ));
            }
        };
        replacement_facts
            .validate_for_kind(current_frame.spanning_kind())
            .map_err(|_| {
                SerializedGreenError::Corrupt("repaired Exit facts do not match their frame")
            })?;
        if replacement_closed != exit.closed || replacement_facts != exit.facts {
            self.rewrites.push(PlannedExitRewrite {
                location: exit.clone(),
                replacement_closed,
                replacement_facts,
            });
            self.receipt.exit_events_changed =
                self.receipt.exit_events_changed.checked_add(1).ok_or(
                    SerializedGreenError::Overflow("suffix-adoption changed Exit receipt"),
                )?;
        }
        self.receipt.spanning_exits_examined =
            self.receipt.spanning_exits_examined.checked_add(1).ok_or(
                SerializedGreenError::Overflow("suffix-adoption spanning Exit receipt"),
            )?;
        self.receipt.frames_planned =
            self.receipt
                .frames_planned
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption planned frame receipt",
                ))?;
        self.repaired_inner_child = Some(replacement_closed);
        self.next_segment_start =
            exit.event_ordinal
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption next segment event",
                ))?;
        self.next_frame = frame_index.checked_sub(1);
        if self.next_frame.is_none() {
            self.ready = true;
            Ok(GreenSuffixAdoptionPlanProgress::Ready)
        } else {
            Ok(GreenSuffixAdoptionPlanProgress::Pending)
        }
    }
}

/// Read-only, cancellable repair planner.  Dropping it releases only bounded
/// heap scratch; both immutable document owners remain with the caller.
#[must_use = "poll the suffix-adoption planner to Ready, commit it, or cancel by dropping it"]
#[derive(Debug)]
pub struct GreenSuffixAdoptionPlanner<'documents> {
    current: &'documents SerializedGreenDocument,
    old: &'documents SerializedGreenDocument,
    current_manifest: Manifest,
    old_manifest: Manifest,
    current_boundary: GreenSuffixAdoptionBoundary,
    old_boundary: GreenSuffixAdoptionBoundary,
    repair: GreenSpanningExitRepairPlanner,
}

impl GreenSuffixAdoptionPlanner<'_> {
    #[must_use]
    pub const fn receipt(&self) -> &GreenSuffixAdoptionReceipt {
        self.repair.receipt()
    }

    /// Performs exactly one spanning-frame repair.  Each poll uses persistent
    /// summaries plus bounded leaf decodes and is `O(tree height)`.
    #[allow(clippy::too_many_lines)] // One poll keeps one frame's locate/fold/rewrite derivation auditable.
    pub fn poll(
        &mut self,
        arena: &PageArena,
    ) -> Result<GreenSuffixAdoptionPlanProgress, SerializedGreenError> {
        if self.repair.is_ready() {
            return Ok(GreenSuffixAdoptionPlanProgress::Ready);
        }
        self.revalidate_documents(arena)?;
        let old_manifest_id = self.old.local_manifest_id(arena)?;
        let (_, old_root_id) = decode_document(arena, old_manifest_id)?;
        self.repair.poll(
            arena,
            old_root_id,
            self.old_boundary.prefix_leaves,
            self.current_boundary.output.frames(),
            self.old_boundary.output.frames(),
        )
    }

    /// Atomically retains the current prefix, repairs the old suffix, joins
    /// them, and publishes a manifest bound to the complete current target.
    ///
    /// This storage-algebra proof uses one bounded synchronous arena
    /// transaction after cancellable planning.  The production candidate seam
    /// must port that mutation phase to its journalled, fuelled transaction;
    /// this method does not claim that integration gate.
    #[allow(clippy::too_many_lines)]
    pub fn commit(
        mut self,
        arena: &mut PageArena,
        next_parse_generation: ParseGeneration,
        next_semantic_epoch: u64,
    ) -> Result<GreenSuffixAdoptionCommit, SerializedGreenError> {
        if !self.repair.ready
            || self.repair.receipt.frames_planned != self.repair.receipt.open_depth
        {
            return Err(SerializedGreenError::Invalid(
                "suffix-adoption plan is not complete",
            ));
        }
        self.revalidate_documents(arena)?;
        if next_parse_generation.0
            <= self
                .current_manifest
                .parse_generation
                .0
                .max(self.old_manifest.parse_generation.0)
            || next_semantic_epoch
                <= self
                    .current_manifest
                    .semantic_epoch
                    .max(self.old_manifest.semantic_epoch)
        {
            return Err(SerializedGreenError::Invalid(
                "suffix-adoption generation must advance both inputs",
            ));
        }

        let current_manifest_id = self.current.local_manifest_id(arena)?;
        let old_manifest_id = self.old.local_manifest_id(arena)?;
        let (current_manifest, current_root) = decode_document(arena, current_manifest_id)?;
        let (old_manifest, old_root) = decode_document(arena, old_manifest_id)?;
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let mut build_receipt = SerializedGreenBuildReceipt::default();

        self.repair
            .rewrites
            .sort_by_key(|rewrite| (rewrite.location.leaf_index, rewrite.location.byte_offset));
        if self.repair.rewrites.windows(2).any(|pair| {
            pair[0].location.leaf_index == pair[1].location.leaf_index
                && pair[0].location.byte_offset == pair[1].location.byte_offset
        }) {
            return Err(SerializedGreenError::Corrupt(
                "suffix-adoption planned the same Exit twice",
            ));
        }

        let old_suffix_leaves = old_manifest
            .summary
            .leaves
            .checked_sub(self.old_boundary.prefix_leaves)
            .ok_or(SerializedGreenError::StaleCursor)?;
        let prefix = retain_sequence_range_in_transaction::<SerializedGreenSpec>(
            &mut transaction,
            current_root,
            0..self.current_boundary.prefix_leaves,
            &mut sequence_receipt,
        )?
        .ok_or(SerializedGreenError::Corrupt(
            "suffix-adoption current prefix is empty",
        ))?;
        let old_suffix = retain_sequence_range_in_transaction::<SerializedGreenSpec>(
            &mut transaction,
            old_root,
            self.old_boundary.prefix_leaves..old_manifest.summary.leaves,
            &mut sequence_receipt,
        )?
        .ok_or(SerializedGreenError::Corrupt(
            "suffix-adoption old suffix is empty",
        ))?;

        let mut replacements = Vec::new();
        let mut cursor = 0_usize;
        while cursor < self.repair.rewrites.len() {
            let absolute_leaf_index = self.repair.rewrites[cursor].location.leaf_index;
            let expected_leaf = self.repair.rewrites[cursor].location.leaf;
            let end = cursor
                + self.repair.rewrites[cursor..]
                    .iter()
                    .take_while(|rewrite| rewrite.location.leaf_index == absolute_leaf_index)
                    .count();
            if self.repair.rewrites[cursor..end]
                .iter()
                .any(|rewrite| rewrite.location.leaf != expected_leaf)
            {
                return Err(SerializedGreenError::StaleCursor);
            }
            if locate_leaf_in_arena(transaction.arena(), old_root, absolute_leaf_index)?
                != Some(expected_leaf)
            {
                return Err(SerializedGreenError::StaleCursor);
            }
            let payload_bytes = transaction.arena().payload(expected_leaf)?.len();
            let (old_leaf_summary, decoded) = decode_leaf(transaction.arena(), expected_leaf)?;
            self.repair.receipt.leaf_pages_decoded = self
                .repair
                .receipt
                .leaf_pages_decoded
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption commit leaf receipt",
                ))?;
            self.repair.receipt.events_decoded = self
                .repair
                .receipt
                .events_decoded
                .checked_add(decoded.len())
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption commit event receipt",
                ))?;
            let decoded_bytes = decoded
                .capacity()
                .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                .and_then(|bytes| bytes.checked_add(payload_bytes))
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption decoded leaf receipt",
                ))?;
            self.repair.receipt.maximum_decoded_page_bytes = self
                .repair
                .receipt
                .maximum_decoded_page_bytes
                .max(decoded_bytes);
            let mut events = decoded
                .into_iter()
                .map(|decoded| (decoded.byte_offset, decoded.event))
                .collect::<Vec<_>>();
            self.repair.receipt.maximum_rewrite_scratch_bytes = self
                .repair
                .receipt
                .maximum_rewrite_scratch_bytes
                .max(events.capacity() * std::mem::size_of::<(u16, DecodedGreenEventKind)>());
            for rewrite in &self.repair.rewrites[cursor..end] {
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
            let handles = allocate_event_pages(
                &mut transaction,
                events.into_iter().map(|(_, event)| event),
                &mut build_receipt,
            )?;
            if handles.len() != 1 {
                return Err(SerializedGreenError::Corrupt(
                    "fixed-width Exit repair changed packed leaf count",
                ));
            }
            let replacement_summary = sequence_node::<SerializedGreenSpec>(
                transaction.arena(),
                transaction.id(&handles[0]),
            )?
            .0;
            if replacement_summary.tokens != old_leaf_summary.tokens
                || replacement_summary.blocks != old_leaf_summary.blocks
                || replacement_summary.metric != old_leaf_summary.metric
                || replacement_summary.logical_metric != old_leaf_summary.logical_metric
                || replacement_summary.balance != old_leaf_summary.balance
                || replacement_summary.minimum_prefix != old_leaf_summary.minimum_prefix
                || replacement_summary.minimum_closed_depth != old_leaf_summary.minimum_closed_depth
            {
                return Err(SerializedGreenError::Corrupt(
                    "Exit repair changed non-output leaf semantics",
                ));
            }
            let relative_leaf_index = absolute_leaf_index
                .checked_sub(self.old_boundary.prefix_leaves)
                .ok_or(SerializedGreenError::StaleCursor)?;
            replacements.push(BaseLeafReplacement {
                leaf_index: relative_leaf_index,
                expected_leaf,
                replacements: handles,
            });
            cursor = end;
        }
        self.repair.receipt.distinct_exit_leaves_rewritten = replacements.len();
        self.repair.receipt.current_prefix_leaves_retained = self.current_boundary.prefix_leaves;
        self.repair.receipt.old_suffix_leaves_retained = old_suffix_leaves;
        self.repair.receipt.unchanged_old_suffix_leaves = old_suffix_leaves
            .checked_sub(u64::try_from(replacements.len()).map_err(|_| {
                SerializedGreenError::Overflow("suffix-adoption replacement leaf count")
            })?)
            .ok_or(SerializedGreenError::Corrupt(
                "suffix-adoption rewrites exceed old suffix",
            ))?;

        let repaired_suffix = if replacements.is_empty() {
            Some(old_suffix)
        } else {
            let suffix_root = transaction.id(&old_suffix);
            let repaired = replace_leaf_batch_in_transaction::<SerializedGreenSpec>(
                &mut transaction,
                Some(suffix_root),
                replacements,
                &mut sequence_receipt,
            )?;
            transaction.release(old_suffix)?;
            repaired
        }
        .ok_or(SerializedGreenError::Corrupt(
            "suffix-adoption repair removed the old suffix",
        ))?;
        let prefix_end = self.current_boundary.prefix_leaves;
        let root = splice_owned_root_in_transaction::<SerializedGreenSpec>(
            &mut transaction,
            Some(prefix),
            prefix_end..prefix_end,
            Some(repaired_suffix),
            &mut sequence_receipt,
        )?
        .ok_or(SerializedGreenError::Corrupt(
            "suffix-adoption splice produced no root",
        ))?;
        let summary =
            sequence_node::<SerializedGreenSpec>(transaction.arena(), transaction.id(&root))?.0;
        if !summary.same_semantics(current_manifest.summary)
            || summary.balance != 0
            || summary.minimum_prefix < 0
        {
            return Err(SerializedGreenError::Invalid(
                "adopted suffix does not match the complete current green target",
            ));
        }
        let manifest = Manifest {
            parse_generation: next_parse_generation,
            semantic_epoch: next_semantic_epoch,
            summary,
            ..current_manifest
        };
        let payload = encode_manifest(&manifest);
        let (manifest_owner, allocation) =
            transaction.allocate(&payload, &[transaction.id(&root)])?;
        transaction.release(root)?;
        build_receipt.manifest_nodes_allocated = build_receipt
            .manifest_nodes_allocated
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption manifest receipt",
            ))?;
        build_receipt.payload_bytes_copied = build_receipt
            .payload_bytes_copied
            .checked_add(allocation.payload_bytes_copied)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption payload receipt",
            ))?;
        build_receipt.edge_bytes_copied = build_receipt
            .edge_bytes_copied
            .checked_add(allocation.edge_bytes_copied)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption edge receipt",
            ))?;
        build_receipt.final_sequence_height = summary.height;
        merge_sequence_receipt(&mut build_receipt, sequence_receipt);
        sync_transaction_receipt(&mut build_receipt, &transaction);
        self.repair.receipt.build = build_receipt;
        debug_assert_eq!(self.repair.receipt.retained_source_bytes, 0);
        debug_assert_eq!(self.repair.receipt.document_sized_event_vectors, 0);
        debug_assert_eq!(transaction.live_owners(), 1);
        let owner = transaction.take(manifest_owner);
        let manifest = SerializedGreenManifestId::new(owner.scoped_id());
        Ok(GreenSuffixAdoptionCommit {
            document: SerializedGreenDocument { owner, manifest },
            receipt: self.repair.receipt,
        })
    }

    fn revalidate_documents(&self, arena: &PageArena) -> Result<(), SerializedGreenError> {
        let current_manifest_id = self.current.local_manifest_id(arena)?;
        let old_manifest_id = self.old.local_manifest_id(arena)?;
        let (current, current_root) = decode_document(arena, current_manifest_id)?;
        let (old, old_root) = decode_document(arena, old_manifest_id)?;
        if current != self.current_manifest
            || old != self.old_manifest
            || !self
                .current_boundary
                .binding
                .matches(self.current.manifest_id(), &current)
            || !self
                .old_boundary
                .binding
                .matches(self.old.manifest_id(), &old)
            || exact_leaf_boundary(
                arena,
                current_root,
                self.current_boundary.event_cut(),
                &mut 0,
            )? != self.current_boundary.prefix_leaves
            || exact_leaf_boundary(arena, old_root, self.old_boundary.event_cut(), &mut 0)?
                != self.old_boundary.prefix_leaves
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        Ok(())
    }
}

impl SerializedGreenDocument {
    /// Mints one exact leaf-boundary capability and its current-root output.
    pub fn suffix_adoption_boundary_at_event_cut(
        &self,
        arena: &PageArena,
        event_cut: u64,
    ) -> Result<GreenSuffixAdoptionBoundary, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let (manifest, root) = decode_document(arena, manifest_id)?;
        let mut boundary_sequence_nodes_visited = 0_usize;
        let prefix_leaves =
            exact_leaf_boundary(arena, root, event_cut, &mut boundary_sequence_nodes_visited)?;
        let output = self.restart_output_at_event_cut(arena, event_cut)?;
        Ok(GreenSuffixAdoptionBoundary {
            binding: SerializedGreenManifestDescriptor::new(self.manifest_id(), &manifest),
            output,
            prefix_leaves,
            boundary_sequence_nodes_visited,
        })
    }

    /// Converts an exact old-green boundary into the storage half of suffix
    /// adoption authority.
    ///
    /// Every total is folded from the bound manifest or restart query. The
    /// caller supplies neither source coordinates nor coverage counts, and the
    /// returned value retains no arena owner or source root. Parser convergence
    /// is intentionally *not* proven here; [`GreenSourceTailAdoptionCapability::join_current_source`]
    /// must still consume a current lineage proof, and the eventual green
    /// journal must additionally consume grammar-convergence authority.
    pub(crate) fn source_tail_adoption_capability(
        &self,
        arena: &PageArena,
        boundary: GreenSuffixAdoptionBoundary,
    ) -> Result<GreenSourceTailAdoptionCapability, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let (manifest, root) = decode_document(arena, manifest_id)?;
        source_tail_adoption_capability_from_bound_root(
            arena,
            self.manifest_id(),
            &manifest,
            root,
            boundary,
        )
    }

    /// Begins the storage-only suffix repair after parser convergence has been
    /// proven elsewhere.  Neither raw root identity nor source text escapes.
    pub fn begin_suffix_adoption<'documents>(
        &'documents self,
        arena: &PageArena,
        current_boundary: GreenSuffixAdoptionBoundary,
        old: &'documents SerializedGreenDocument,
        old_boundary: GreenSuffixAdoptionBoundary,
    ) -> Result<GreenSuffixAdoptionPlanner<'documents>, SerializedGreenError> {
        let current_manifest_id = self.local_manifest_id(arena)?;
        let old_manifest_id = old.local_manifest_id(arena)?;
        let (current_manifest, current_root) = decode_document(arena, current_manifest_id)?;
        let (old_manifest, old_root) = decode_document(arena, old_manifest_id)?;
        if !current_boundary
            .binding
            .matches(self.manifest_id(), &current_manifest)
            || !old_boundary
                .binding
                .matches(old.manifest_id(), &old_manifest)
            || current_boundary.output.manifest() != self.manifest_id()
            || old_boundary.output.manifest() != old.manifest_id()
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        if exact_leaf_boundary(arena, current_root, current_boundary.event_cut(), &mut 0)?
            != current_boundary.prefix_leaves
            || exact_leaf_boundary(arena, old_root, old_boundary.event_cut(), &mut 0)?
                != old_boundary.prefix_leaves
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        if current_manifest.syntax_profile != old_manifest.syntax_profile
            || current_manifest.grammar_revision != old_manifest.grammar_revision
        {
            return Err(SerializedGreenError::Invalid(
                "suffix-adoption inputs use different grammars",
            ));
        }
        let current_frames = current_boundary.output.frames();
        let old_frames = old_boundary.output.frames();
        if current_frames.is_empty()
            || current_frames.len() != old_frames.len()
            || current_boundary.prefix_leaves == 0
            || old_boundary.prefix_leaves >= old_manifest.summary.leaves
        {
            return Err(SerializedGreenError::Invalid(
                "suffix-adoption cuts do not retain one matching open path and suffix",
            ));
        }
        for (current, old) in current_frames.iter().zip(old_frames) {
            if current.block() != old.block()
                || current.kind() != old.kind()
                || current.facts() != old.facts()
            {
                return Err(SerializedGreenError::Invalid(
                    "suffix-adoption open paths did not structurally converge",
                ));
            }
            if current.kind() == GreenKind::FENCED_CODE {
                return Err(SerializedGreenError::Invalid(
                    "spanning FencedCode Exit repair requires typed boundary markers",
                ));
            }
        }
        let open_depth = current_frames.len();
        let mut receipt = GreenSuffixAdoptionReceipt {
            open_depth,
            ..GreenSuffixAdoptionReceipt::default()
        };
        merge_query_receipt(&mut receipt, *current_boundary.output.receipt())?;
        merge_query_receipt(&mut receipt, *old_boundary.output.receipt())?;
        let repair =
            GreenSpanningExitRepairPlanner::new(open_depth, old_boundary.event_cut(), receipt)?;
        Ok(GreenSuffixAdoptionPlanner {
            current: self,
            old,
            current_manifest,
            old_manifest,
            current_boundary,
            old_boundary,
            repair,
        })
    }
}

fn source_tail_adoption_capability_from_bound_root(
    arena: &PageArena,
    manifest_capability: SerializedGreenManifestId,
    manifest: &Manifest,
    root: ArenaId,
    boundary: GreenSuffixAdoptionBoundary,
) -> Result<GreenSourceTailAdoptionCapability, SerializedGreenError> {
    if !boundary.binding.matches(manifest_capability, manifest)
        || boundary.output.manifest() != manifest_capability
        || exact_leaf_boundary(arena, root, boundary.event_cut(), &mut 0)? != boundary.prefix_leaves
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    if manifest.summary.balance != 0
        || manifest.summary.minimum_prefix < 0
        || manifest.summary.metric
            != (SerializedMetric {
                bytes: manifest.source_bytes,
                utf16: manifest.source_utf16,
            })
        || manifest.known_bytes != (0..manifest.source_bytes)
    {
        return Err(SerializedGreenError::Corrupt(
            "tail-adoption source manifest is not a complete balanced document",
        ));
    }
    if boundary.prefix_leaves == 0
        || boundary.prefix_leaves >= manifest.summary.leaves
        || boundary.open_depth() == 0
    {
        return Err(SerializedGreenError::Invalid(
            "tail-adoption boundary must retain a nonempty prefix, open path, and suffix",
        ));
    }

    let structural_events =
        manifest
            .summary
            .blocks
            .checked_mul(2)
            .ok_or(SerializedGreenError::Overflow(
                "tail-adoption structural event count",
            ))?;
    let total_coverage_runs = manifest
        .summary
        .tokens
        .checked_sub(structural_events)
        .ok_or(SerializedGreenError::Corrupt(
            "tail-adoption manifest has fewer events than its block envelope",
        ))?;
    let prefix_coverage_runs = boundary.output.coverage_count();
    let suffix_coverage_runs = total_coverage_runs
        .checked_sub(prefix_coverage_runs)
        .ok_or(SerializedGreenError::Corrupt(
            "tail-adoption prefix coverage exceeds the document total",
        ))?;
    if suffix_coverage_runs == 0 {
        return Err(SerializedGreenError::Invalid(
            "tail-adoption suffix has no source coverage",
        ));
    }

    let old_total = manifest.summary.metric;
    let old_prefix = boundary.source_metric();
    let suffix = old_total.checked_sub(old_prefix)?;
    if suffix.bytes == 0 {
        return Err(SerializedGreenError::Invalid(
            "tail-adoption suffix has no source bytes",
        ));
    }

    // The exact event cut may be followed by any number of zero-source
    // structural events before its first source-consuming run (Setext's
    // Heading Exit + next Paragraph Enter is one ordinary example). Seek by
    // source coordinate, not leaf position: downstream affinity crosses those
    // events, reverse summary-skipping reconstructs their resulting open path,
    // and `next_coverage` returns the correct post-transition owner.
    let manifest_id = arena.local_id(manifest_capability.scoped())?;
    let mut first_cursor = stream_at_bound_root(
        arena,
        manifest_id,
        manifest_capability,
        manifest,
        root,
        GreenCoordinate::Bytes,
        old_prefix.bytes,
        GreenAffinity::Downstream,
    )?;
    let first_run = first_cursor
        .next_coverage_at_bound_manifest(manifest_capability, arena)?
        .ok_or(SerializedGreenError::Corrupt(
            "tail-adoption suffix has no first source coverage",
        ))?;
    let first_receipt = first_cursor.receipt();
    if first_run.byte_range.start != old_prefix.bytes
        || first_run.utf16_range.start != old_prefix.utf16
    {
        return Err(SerializedGreenError::Corrupt(
            "tail-adoption first source coverage starts after its exact semantic cut",
        ));
    }
    let first_metric = SerializedMetric {
        bytes: first_run
            .byte_range
            .end
            .checked_sub(first_run.byte_range.start)
            .ok_or(SerializedGreenError::Corrupt(
                "tail-adoption first byte range is reversed",
            ))?,
        utf16: first_run
            .utf16_range
            .end
            .checked_sub(first_run.utf16_range.start)
            .ok_or(SerializedGreenError::Corrupt(
                "tail-adoption first UTF-16 range is reversed",
            ))?,
    };
    let (logical, program_receipt) = match first_run.logical_contribution {
        LogicalContributionView::None => (
            FirstSuffixLogical::None,
            FirstSuffixProgramPrefixReceipt::default(),
        ),
        LogicalContributionView::Identity { logical_metric } => {
            if logical_metric != first_metric {
                return Err(SerializedGreenError::Corrupt(
                    "first suffix Identity metric changed during source seek",
                ));
            }
            (
                FirstSuffixLogical::Identity,
                FirstSuffixProgramPrefixReceipt::default(),
            )
        }
        LogicalContributionView::Atomic { projection } => (
            FirstSuffixLogical::Atomic(projection.kind),
            FirstSuffixProgramPrefixReceipt::default(),
        ),
        LogicalContributionView::Hidden { .. } => (
            FirstSuffixLogical::Unsupported,
            FirstSuffixProgramPrefixReceipt::default(),
        ),
        LogicalContributionView::Program { program, .. } => {
            let (prefix, receipt) = first_suffix_program_prefix(arena, &first_run, program)?;
            (FirstSuffixLogical::ProgramPrefix(prefix), receipt)
        }
    };
    let first_suffix_coverage = FirstSuffixCoverageAuthority {
        metric: first_metric,
        owner: first_run.owner.block,
        owner_kind: first_run.owner.kind,
        part: first_run.part,
        logical,
    };
    let old_source = SourceSnapshotDescriptor {
        revision: manifest.source_revision,
        root: manifest.source_root,
        bytes: usize::try_from(manifest.source_bytes)
            .map_err(|_| SerializedGreenError::Overflow("old source bytes"))?,
    };
    let query = *boundary.output.receipt();
    let green_sequence_nodes_visited = boundary
        .boundary_sequence_nodes_visited
        .checked_add(query.sequence_nodes_visited)
        .and_then(|visited| visited.checked_add(first_receipt.sequence_nodes_visited))
        .ok_or(SerializedGreenError::Overflow(
            "tail-adoption green sequence work",
        ))?;
    let receipt = GreenSourceTailAdoptionReceipt {
        described_suffix_bytes: suffix.bytes,
        described_suffix_utf16: suffix.utf16,
        green_sequence_nodes_visited,
        green_leaf_pages_decoded: query
            .leaf_pages_decoded
            .checked_add(first_receipt.leaf_pages_decoded)
            .ok_or(SerializedGreenError::Overflow(
                "tail-adoption decoded leaf pages",
            ))?,
        green_events_decoded: query
            .events_decoded
            .checked_add(first_receipt.events_decoded)
            .ok_or(SerializedGreenError::Overflow(
                "tail-adoption decoded green events",
            ))?,
        green_projection_program_pages_validated: program_receipt.pages_validated,
        green_projection_program_bytes_validated: program_receipt.bytes_validated,
        green_projection_program_pieces_validated: program_receipt.pieces_validated,
        green_projection_prefix_pieces_decoded: program_receipt.prefix_pieces_decoded,
        green_maximum_route_depth: query
            .maximum_route_depth
            .max(first_receipt.maximum_route_depth),
        green_maximum_open_depth: query
            .maximum_open_depth
            .max(first_receipt.maximum_open_depth),
        retained_source_bytes: query.retained_source_bytes,
        document_sized_event_vectors: query.document_sized_event_vectors,
        ..GreenSourceTailAdoptionReceipt::default()
    };
    if receipt.retained_source_bytes != 0 || receipt.document_sized_event_vectors != 0 {
        return Err(SerializedGreenError::Corrupt(
            "tail-adoption boundary retained forbidden document storage",
        ));
    }
    Ok(GreenSourceTailAdoptionCapability {
        old_source,
        old_total,
        old_prefix,
        suffix,
        total_coverage_runs,
        prefix_coverage_runs,
        suffix_coverage_runs,
        first_suffix_coverage,
        boundary,
        receipt,
    })
}

#[cfg(feature = "exact-parser")]
impl ParentRetainedGreenLease<'_> {
    /// Production semantic-C resolver. The old event cut comes only from the
    /// parent-bound checkpoint probe; the retained manifest is authenticated
    /// through the exact suspended candidate ticket before any boundary is
    /// decoded.
    pub(crate) fn source_tail_adoption_capability_for_parent_convergence(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        convergence: &ParentBoundSourceConvergence,
    ) -> Result<GreenSourceTailAdoptionCapability, SerializedGreenError> {
        if convergence.parent_root(ParentBoundGreenConvergenceMint(())) != self.parent_activation {
            return Err(SerializedGreenError::Invalid(
                "parent-bound convergence and retained green lease differ",
            ));
        }
        let manifest_id = self.validated_suspended_manifest(ticket, arena)?;
        let manifest_capability =
            SerializedGreenManifestId::new(arena.scoped_query_id(manifest_id)?);
        let (manifest, root) = decode_document(arena, manifest_id)?;
        let event_cut = convergence.green_event_cut(ParentBoundGreenConvergenceMint(()));
        let mut boundary_sequence_nodes_visited = 0_usize;
        let prefix_leaves =
            exact_leaf_boundary(arena, root, event_cut, &mut boundary_sequence_nodes_visited)?;
        let output = super::restart_output_query::restart_output_at_bound_root(
            arena,
            manifest_capability,
            &manifest,
            root,
            event_cut,
        )?;
        let boundary = GreenSuffixAdoptionBoundary {
            binding: SerializedGreenManifestDescriptor::new(manifest_capability, &manifest),
            output,
            prefix_leaves,
            boundary_sequence_nodes_visited,
        };
        source_tail_adoption_capability_from_bound_root(
            arena,
            manifest_capability,
            &manifest,
            root,
            boundary,
        )
    }

    /// Mechanism-only scalar-cut adapter for the zero-restart feasibility
    /// proof. The production entry point must take the eventual consumed
    /// parser-convergence capability, which privately selects the event cut;
    /// it must not expose this scalar argument.
    ///
    /// The parent lease authenticates the hidden old manifest owner inside the
    /// live adoption session. This method derives the exact packed-leaf cut,
    /// restart output, source metrics, open frames, and coverage counts without
    /// constructing or exposing a standalone green document or either root.
    pub(crate) fn source_tail_adoption_capability_at_event_cut_mechanism_only(
        &self,
        session: &crate::ArenaBuildSession<'_>,
        event_cut: u64,
    ) -> Result<GreenSourceTailAdoptionCapability, SerializedGreenError> {
        let manifest_id = self.validated_manifest(session)?;
        let arena = session.arena();
        let manifest_capability =
            SerializedGreenManifestId::new(arena.scoped_query_id(manifest_id)?);
        let (manifest, root) = decode_document(arena, manifest_id)?;
        let mut boundary_sequence_nodes_visited = 0_usize;
        let prefix_leaves =
            exact_leaf_boundary(arena, root, event_cut, &mut boundary_sequence_nodes_visited)?;
        let output = super::restart_output_query::restart_output_at_bound_root(
            arena,
            manifest_capability,
            &manifest,
            root,
            event_cut,
        )?;
        let boundary = GreenSuffixAdoptionBoundary {
            binding: SerializedGreenManifestDescriptor::new(manifest_capability, &manifest),
            output,
            prefix_leaves,
            boundary_sequence_nodes_visited,
        };
        source_tail_adoption_capability_from_bound_root(
            arena,
            manifest_capability,
            &manifest,
            root,
            boundary,
        )
    }
}

fn exact_leaf_boundary(
    arena: &PageArena,
    root: ArenaId,
    event_cut: u64,
    nodes_visited: &mut usize,
) -> Result<u64, SerializedGreenError> {
    let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if event_cut > root_summary.tokens {
        return Err(SerializedGreenError::StaleCursor);
    }
    if event_cut == 0 {
        return Ok(0);
    }
    if event_cut == root_summary.tokens {
        return Ok(root_summary.leaves);
    }
    let mut node = root;
    let mut remaining = event_cut;
    let mut leaves_before = 0_u64;
    loop {
        *nodes_visited = nodes_visited
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption boundary descent receipt",
            ))?;
        let (summary, kind) = sequence_node::<SerializedGreenSpec>(arena, node)?;
        match kind {
            SequenceNodeKind::Leaf => {
                if remaining == 0 {
                    return Ok(leaves_before);
                }
                if remaining == summary.tokens {
                    return leaves_before
                        .checked_add(1)
                        .ok_or(SerializedGreenError::Overflow(
                            "suffix-adoption boundary leaf count",
                        ));
                }
                return Err(SerializedGreenError::StaleCursor);
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                match remaining.cmp(&left_summary.tokens) {
                    Ordering::Less => node = left,
                    Ordering::Equal => {
                        return leaves_before.checked_add(left_summary.leaves).ok_or(
                            SerializedGreenError::Overflow("suffix-adoption boundary leaf count"),
                        );
                    }
                    Ordering::Greater => {
                        remaining -= left_summary.tokens;
                        leaves_before = leaves_before.checked_add(left_summary.leaves).ok_or(
                            SerializedGreenError::Overflow("suffix-adoption boundary leaf count"),
                        )?;
                        node = right;
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] // Summary pruning and the one bounded leaf decode stay adjacent.
fn search_spanning_exit(
    arena: &PageArena,
    node: ArenaId,
    base_leaf_index: u64,
    base_event_ordinal: u64,
    suffix_start_leaf: u64,
    target_relative_balance: i64,
    running_relative_balance: &mut i64,
    route_depth: usize,
    receipt: &mut GreenSuffixAdoptionReceipt,
) -> Result<Option<SpanningExitLocation>, SerializedGreenError> {
    receipt.maximum_route_depth = receipt.maximum_route_depth.max(route_depth);
    receipt.sequence_nodes_visited =
        receipt
            .sequence_nodes_visited
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption sequence-node receipt",
            ))?;
    let (summary, kind) = sequence_node::<SerializedGreenSpec>(arena, node)?;
    let node_leaf_end =
        base_leaf_index
            .checked_add(summary.leaves)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption node leaf end",
            ))?;
    if node_leaf_end <= suffix_start_leaf {
        return Ok(None);
    }
    if base_leaf_index >= suffix_start_leaf {
        let minimum = running_relative_balance
            .checked_add(summary.minimum_prefix)
            .ok_or(SerializedGreenError::Overflow(
                "suffix-adoption relative minimum",
            ))?;
        if minimum > target_relative_balance {
            *running_relative_balance = running_relative_balance
                .checked_add(summary.balance)
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption relative balance",
                ))?;
            receipt.summary_nodes_reused = receipt.summary_nodes_reused.checked_add(1).ok_or(
                SerializedGreenError::Overflow("suffix-adoption summary reuse receipt"),
            )?;
            return Ok(None);
        }
    }
    match kind {
        SequenceNodeKind::Leaf => {
            if base_leaf_index < suffix_start_leaf {
                return Err(SerializedGreenError::Corrupt(
                    "exact suffix boundary split a packed leaf",
                ));
            }
            let payload_bytes = arena.payload(node)?.len();
            let (_, events) = decode_leaf(arena, node)?;
            receipt.leaf_pages_decoded =
                receipt
                    .leaf_pages_decoded
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow(
                        "suffix-adoption decoded leaf receipt",
                    ))?;
            receipt.events_decoded = receipt.events_decoded.checked_add(events.len()).ok_or(
                SerializedGreenError::Overflow("suffix-adoption decoded event receipt"),
            )?;
            let decoded_bytes = events
                .capacity()
                .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                .and_then(|bytes| bytes.checked_add(payload_bytes))
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption decoded leaf receipt",
                ))?;
            receipt.maximum_decoded_page_bytes =
                receipt.maximum_decoded_page_bytes.max(decoded_bytes);
            for (local_ordinal, decoded) in events.into_iter().enumerate() {
                match decoded.event {
                    DecodedGreenEventKind::Enter { .. } => {
                        *running_relative_balance = running_relative_balance.checked_add(1).ok_or(
                            SerializedGreenError::Overflow("suffix-adoption relative Enter"),
                        )?;
                    }
                    DecodedGreenEventKind::Coverage(_) => {}
                    DecodedGreenEventKind::Exit {
                        closed,
                        last_line_blank,
                        facts,
                    } => {
                        *running_relative_balance = running_relative_balance.checked_sub(1).ok_or(
                            SerializedGreenError::Overflow("suffix-adoption relative Exit"),
                        )?;
                        if *running_relative_balance == target_relative_balance {
                            let local_ordinal = u64::try_from(local_ordinal).map_err(|_| {
                                SerializedGreenError::Overflow("suffix-adoption leaf event ordinal")
                            })?;
                            return Ok(Some(SpanningExitLocation {
                                leaf: node,
                                leaf_index: base_leaf_index,
                                byte_offset: decoded.byte_offset,
                                event_ordinal: base_event_ordinal
                                    .checked_add(local_ordinal)
                                    .ok_or(SerializedGreenError::Overflow(
                                        "suffix-adoption event ordinal",
                                    ))?,
                                closed,
                                last_line_blank,
                                facts,
                            }));
                        }
                    }
                }
            }
            Ok(None)
        }
        SequenceNodeKind::Branch { left, right } => {
            let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
            let next_depth = route_depth
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "suffix-adoption route depth",
                ))?;
            if let Some(exit) = search_spanning_exit(
                arena,
                left,
                base_leaf_index,
                base_event_ordinal,
                suffix_start_leaf,
                target_relative_balance,
                running_relative_balance,
                next_depth,
                receipt,
            )? {
                return Ok(Some(exit));
            }
            search_spanning_exit(
                arena,
                right,
                base_leaf_index.checked_add(left_summary.leaves).ok_or(
                    SerializedGreenError::Overflow("suffix-adoption right leaf index"),
                )?,
                base_event_ordinal.checked_add(left_summary.tokens).ok_or(
                    SerializedGreenError::Overflow("suffix-adoption right event ordinal"),
                )?,
                suffix_start_leaf,
                target_relative_balance,
                running_relative_balance,
                next_depth,
                receipt,
            )
        }
    }
}

fn find_spanning_exit(
    arena: &PageArena,
    root: ArenaId,
    suffix_start_leaf: u64,
    target_relative_balance: i64,
    receipt: &mut GreenSuffixAdoptionReceipt,
) -> Result<SpanningExitLocation, SerializedGreenError> {
    let mut relative_balance = 0_i64;
    search_spanning_exit(
        arena,
        root,
        0,
        0,
        suffix_start_leaf,
        target_relative_balance,
        &mut relative_balance,
        0,
        receipt,
    )?
    .ok_or(SerializedGreenError::Corrupt(
        "old suffix has no matching spanning Exit",
    ))
}

fn balanced_direct_children(
    summary: GreenSummary,
) -> Result<ChildSequenceAggregate, SerializedGreenError> {
    if summary.balance != 0 || summary.minimum_prefix < 0 {
        return Err(SerializedGreenError::Corrupt(
            "spanning-Exit sibling range is not structurally balanced",
        ));
    }
    match summary.minimum_closed_depth {
        None => Ok(ChildSequenceAggregate::default()),
        Some(0) => Ok(summary.outermost),
        Some(_) => Err(SerializedGreenError::Corrupt(
            "balanced sibling range has an impossible close depth",
        )),
    }
}

fn merge_query_receipt(
    target: &mut GreenSuffixAdoptionReceipt,
    query: GreenRestartOutputReceipt,
) -> Result<(), SerializedGreenError> {
    target.sequence_nodes_visited = target
        .sequence_nodes_visited
        .checked_add(query.sequence_nodes_visited)
        .ok_or(SerializedGreenError::Overflow(
            "suffix-adoption sequence-node receipt",
        ))?;
    target.summary_nodes_reused = target
        .summary_nodes_reused
        .checked_add(query.summary_nodes_reused)
        .ok_or(SerializedGreenError::Overflow(
            "suffix-adoption summary reuse receipt",
        ))?;
    target.leaf_pages_decoded = target
        .leaf_pages_decoded
        .checked_add(query.leaf_pages_decoded)
        .ok_or(SerializedGreenError::Overflow(
            "suffix-adoption decoded leaf receipt",
        ))?;
    target.events_decoded = target
        .events_decoded
        .checked_add(query.events_decoded)
        .ok_or(SerializedGreenError::Overflow(
            "suffix-adoption decoded event receipt",
        ))?;
    target.maximum_decoded_page_bytes = target
        .maximum_decoded_page_bytes
        .max(query.maximum_decoded_page_bytes);
    target.maximum_route_depth = target.maximum_route_depth.max(query.maximum_route_depth);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "exact-parser")]
    use crate::SourceBoundLedgerError;
    #[cfg(feature = "exact-parser")]
    use crate::exact_block_job::{
        ExactBlockCheckpointAdmission, ExactBlockCheckpointCapturePoll, ExactBlockJob,
        ExactBlockJobProgress,
    };

    fn first_suffix_authority(
        metric: SerializedMetric,
        part: CoveragePart,
        logical: FirstSuffixLogical,
    ) -> FirstSuffixCoverageAuthority {
        FirstSuffixCoverageAuthority {
            metric,
            owner: BlockId(2),
            owner_kind: GreenKind::PARAGRAPH,
            part,
            logical,
        }
    }

    #[test]
    fn deferred_terminator_program_prefix_admission_is_typed_and_fail_closed() {
        let one = SerializedMetric { bytes: 1, utf16: 1 };
        let two = SerializedMetric { bytes: 2, utf16: 2 };
        let coalesced = SerializedMetric { bytes: 9, utf16: 9 };

        let lone_cr = first_suffix_authority(
            coalesced,
            CoveragePart::CONTENT,
            FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Atomic {
                metric: one,
                kind: AtomicProjectionKind::LoneCrToLf,
            }),
        );
        assert_eq!(
            lone_cr
                .bind_deferred_terminator(GreenDeferredLineEnding::LoneCr)
                .unwrap()
                .ending(),
            GreenDeferredLineEnding::LoneCr
        );

        let crlf = first_suffix_authority(
            coalesced,
            CoveragePart::TERMINAL,
            FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Atomic {
                metric: two,
                kind: AtomicProjectionKind::CrLfToLf,
            }),
        );
        assert_eq!(
            crlf.bind_deferred_terminator(GreenDeferredLineEnding::CrLf)
                .unwrap()
                .ending(),
            GreenDeferredLineEnding::CrLf
        );

        let rejected = [
            FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Atomic {
                metric: one,
                kind: AtomicProjectionKind::CrLfToLf,
            }),
            FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Atomic {
                metric: two,
                kind: AtomicProjectionKind::LoneCrToLf,
            }),
            FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Identity { metric: two }),
            FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Unsupported),
        ];
        for logical in rejected {
            assert_eq!(
                first_suffix_authority(coalesced, CoveragePart::CONTENT, logical)
                    .bind_deferred_terminator(GreenDeferredLineEnding::LoneCr),
                Err(TailAdoptionJoinError::DeferredTerminatorMismatch)
            );
        }
        assert_eq!(
            first_suffix_authority(
                coalesced,
                CoveragePart::GAP,
                FirstSuffixLogical::ProgramPrefix(FirstSuffixProgramPrefix::Identity {
                    metric: one,
                }),
            )
            .bind_deferred_terminator(GreenDeferredLineEnding::Lf),
            Err(TailAdoptionJoinError::DeferredTerminatorMismatch)
        );
    }

    #[derive(Clone, Copy)]
    struct FixtureFrame {
        kind: GreenKind,
        children: ChildSequenceAggregate,
        logical_metric: SerializedMetric,
    }

    struct Fixture {
        events: Vec<GreenEvent>,
        open: Vec<FixtureFrame>,
        next_block: u64,
        next_coverage: u64,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                open: Vec::new(),
                next_block: 1,
                next_coverage: 1,
            }
        }

        fn facts(kind: GreenKind) -> FactsEnvelope {
            match kind {
                GreenKind::LIST => {
                    GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope()
                }
                GreenKind::ITEM => GreenItemOpenFacts::new(0, 2)
                    .expect("valid fixture Item")
                    .into_envelope(),
                GreenKind::FENCED_CODE => {
                    GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 3, 0)
                        .expect("valid fixture fence")
                        .into_envelope()
                }
                _ => FactsEnvelope::empty(),
            }
        }

        fn enter(&mut self, kind: GreenKind) -> BlockId {
            let block = BlockId(self.next_block);
            self.next_block += 1;
            self.events
                .push(GreenEvent::enter(block, kind, Self::facts(kind)));
            self.open.push(FixtureFrame {
                kind,
                children: ChildSequenceAggregate::default(),
                logical_metric: SerializedMetric::default(),
            });
            block
        }

        fn identity_coverage(&mut self, target: BlockId) {
            let coverage = CoverageId(self.next_coverage);
            self.next_coverage += 1;
            let metric = SerializedMetric { bytes: 1, utf16: 1 };
            let target_frame = self
                .open
                .last_mut()
                .expect("coverage fixture has an open terminal");
            assert!(target_frame.kind.logical_channel().is_some());
            target_frame.logical_metric = target_frame
                .logical_metric
                .checked_add_logical(metric)
                .expect("fixture logical metric");
            self.events.push(GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    coverage,
                    metric.bytes,
                    metric.utf16,
                    0,
                    CoveragePart::CONTENT,
                    target,
                    LogicalContribution::Identity,
                )
                .expect("valid fixture coverage"),
            ));
        }

        fn close(&mut self, last_line_blank: bool) -> ClosedChildAggregate {
            let frame = self.open.pop().expect("fixture close has an open frame");
            let semantics = ContainerFoldSemantics {
                descends_through_last_child: matches!(
                    frame.kind,
                    GreenKind::LIST | GreenKind::ITEM
                ),
                is_item: frame.kind == GreenKind::ITEM,
                last_line_blank,
            };
            let closed = semantics.closed_summary(frame.children);
            let facts = match frame.kind {
                GreenKind::LIST => GreenCloseFacts::List {
                    tight: frame.children.list_is_tight(),
                },
                GreenKind::FENCED_CODE => {
                    let empty = GreenRelativeLogicalSlice::new(0..0, 0..0)
                        .expect("valid empty FencedCode slice");
                    let literal = GreenRelativeLogicalSlice::new(
                        0..frame.logical_metric.bytes,
                        0..frame.logical_metric.utf16,
                    )
                    .expect("valid FencedCode literal");
                    GreenCloseFacts::FencedCode(
                        GreenFencedCodeCloseFacts::new(false, empty, literal)
                            .expect("valid FencedCode close facts"),
                    )
                }
                _ => GreenCloseFacts::None,
            };
            self.events
                .push(GreenEvent::exit_with_state(closed, last_line_blank, facts));
            if let Some(parent) = self.open.last_mut() {
                parent.children = parent
                    .children
                    .followed_by(ChildSequenceAggregate::singleton(closed));
            }
            closed
        }
    }

    struct NestedFixture {
        events: Vec<GreenEvent>,
        cut: usize,
        spanning_end: usize,
        sibling_end: usize,
    }

    fn nested_fixture(prefix_children_end_blank: bool) -> NestedFixture {
        let mut fixture = Fixture::new();
        fixture.enter(GreenKind::DOCUMENT);
        fixture.enter(GreenKind::BLOCK_QUOTE);
        fixture.enter(GreenKind::LIST);
        fixture.enter(GreenKind::ITEM);
        for _ in 0..2 {
            let paragraph = fixture.enter(GreenKind::PARAGRAPH);
            fixture.identity_coverage(paragraph);
            fixture.close(prefix_children_end_blank);
        }
        let cut = fixture.events.len();
        fixture.close(false); // Item: prefix children change this aggregate.
        fixture.close(false); // List: repaired Item changes tightness.
        fixture.close(false); // Quote shares the same packed repair leaf.
        let spanning_end = fixture.events.len();
        let sibling = fixture.enter(GreenKind::PARAGRAPH);
        fixture.identity_coverage(sibling);
        fixture.close(false);
        let sibling_end = fixture.events.len();
        fixture.close(false); // Document Exit remains byte-identical.
        assert!(fixture.open.is_empty());
        NestedFixture {
            events: fixture.events,
            cut,
            spanning_end,
            sibling_end,
        }
    }

    fn spec(
        source_revision: u64,
        source_root: u64,
        parse_generation: u64,
        semantic_epoch: u64,
        metric: SerializedMetric,
    ) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(source_revision),
            source_root: SourceRootId(source_root),
            source_bytes: metric.bytes,
            source_utf16: metric.utf16,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(parse_generation),
            semantic_epoch,
            known_bytes: 0..metric.bytes,
        }
    }

    fn build_grouped(
        arena: &mut PageArena,
        root_spec: SerializedGreenRootSpec,
        events: &[GreenEvent],
        groups: &[Range<usize>],
    ) -> SerializedGreenDocument {
        validate_root_spec(&root_spec).expect("valid grouped test spec");
        assert!(!groups.is_empty());
        assert_eq!(groups.first().expect("first group").start, 0);
        assert_eq!(groups.last().expect("last group").end, events.len());
        assert!(groups.windows(2).all(|pair| pair[0].end == pair[1].start));
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut sequence = StreamingSequenceBuilder::<SerializedGreenSpec>::default();
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let mut build_receipt = SerializedGreenBuildReceipt::default();
        let mut validator = StructuralValidator::default();
        for group in groups {
            assert!(group.start < group.end);
            let mut page = LeafEncoder::default();
            for event in &events[group.clone()] {
                validator.push(event).expect("valid grouped fixture event");
                let encoded = encode_event(event, page.programs.len())
                    .expect("encodable grouped fixture event");
                assert!(page.can_fit(&encoded), "test group must fit one leaf");
                page.push(event, encoded)
                    .expect("grouped fixture page push");
            }
            let leaf = allocate_leaf_page(&mut transaction, page, &mut build_receipt)
                .expect("grouped fixture leaf allocation");
            sequence
                .push_handle(&mut transaction, leaf, &mut sequence_receipt)
                .expect("grouped fixture sequence push");
        }
        validator.finish().expect("complete grouped fixture");
        let root = sequence
            .finish(&mut transaction, &mut sequence_receipt)
            .expect("grouped fixture sequence finish")
            .expect("grouped fixture root");
        let summary =
            sequence_node::<SerializedGreenSpec>(transaction.arena(), transaction.id(&root))
                .expect("grouped fixture root summary")
                .0;
        assert_eq!(summary.metric.bytes, root_spec.source_bytes);
        assert_eq!(summary.metric.utf16, root_spec.source_utf16);
        let manifest = Manifest {
            syntax_profile: root_spec.syntax_profile,
            source_revision: root_spec.source_revision,
            source_root: root_spec.source_root,
            source_bytes: root_spec.source_bytes,
            source_utf16: root_spec.source_utf16,
            grammar_revision: root_spec.grammar_revision,
            parse_generation: root_spec.parse_generation,
            semantic_epoch: root_spec.semantic_epoch,
            known_bytes: root_spec.known_bytes,
            summary,
        };
        let (manifest_owner, _) = transaction
            .allocate(&encode_manifest(&manifest), &[transaction.id(&root)])
            .expect("grouped fixture manifest allocation");
        transaction
            .release(root)
            .expect("grouped fixture root release");
        let owner = transaction.take(manifest_owner);
        let manifest = SerializedGreenManifestId::new(owner.scoped_id());
        SerializedGreenDocument { owner, manifest }
    }

    #[cfg(feature = "exact-parser")]
    fn build_two_run_tail_capability(
        document: &mut crate::LiveDocumentStore,
        prefix_bytes: u64,
        suffix_bytes: u64,
    ) -> (SerializedGreenDocument, GreenSourceTailAdoptionCapability) {
        let source = document.source_descriptor();
        assert_eq!(
            u64::try_from(source.bytes).expect("source bytes fit u64"),
            prefix_bytes + suffix_bytes
        );
        let document_block = BlockId(1);
        let paragraph = BlockId(2);
        let prefix = SourceProjectionRun::with_logical(
            CoverageId(1),
            prefix_bytes,
            prefix_bytes,
            0,
            CoveragePart::CONTENT,
            paragraph,
            LogicalContribution::Identity,
        )
        .expect("valid prefix coverage");
        let suffix = SourceProjectionRun::with_logical(
            CoverageId(2),
            suffix_bytes,
            suffix_bytes,
            0,
            CoveragePart::CONTENT,
            paragraph,
            LogicalContribution::Identity,
        )
        .expect("valid suffix coverage");
        let events = vec![
            GreenEvent::enter(document_block, GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(paragraph, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(prefix),
            GreenEvent::Coverage(suffix),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let old = build_grouped(
            document.candidate_writer_test_arena_mut(),
            spec(
                source.revision.0,
                source.root.0,
                1,
                1,
                SerializedMetric {
                    bytes: prefix_bytes + suffix_bytes,
                    utf16: prefix_bytes + suffix_bytes,
                },
            ),
            &events,
            &[0..3, 3..6],
        );
        let boundary = old
            .suffix_adoption_boundary_at_event_cut(document.candidate_writer_test_arena(), 3)
            .expect("exact old tail boundary");
        let tail = old
            .source_tail_adoption_capability(document.candidate_writer_test_arena(), boundary)
            .expect("storage-derived old tail capability");
        (old, tail)
    }

    #[cfg(feature = "exact-parser")]
    fn build_deferred_lf_tail_capability(
        document: &mut crate::LiveDocumentStore,
        accepted_prefix_bytes: u64,
        tail_after_lf_bytes: u64,
    ) -> (SerializedGreenDocument, GreenSourceTailAdoptionCapability) {
        let source = document.source_descriptor();
        let total = accepted_prefix_bytes
            .checked_add(1)
            .and_then(|value| value.checked_add(tail_after_lf_bytes))
            .expect("fixture metric fits u64");
        assert_eq!(
            u64::try_from(source.bytes).expect("source bytes fit u64"),
            total
        );
        let document_block = BlockId(1);
        let paragraph = BlockId(2);
        let prefix = SourceProjectionRun::with_logical(
            CoverageId(1),
            accepted_prefix_bytes,
            accepted_prefix_bytes,
            0,
            CoveragePart::CONTENT,
            paragraph,
            LogicalContribution::Identity,
        )
        .expect("valid accepted prefix coverage");
        let deferred_lf = SourceProjectionRun::with_logical(
            CoverageId(2),
            1,
            1,
            0,
            CoveragePart::CONTENT,
            paragraph,
            LogicalContribution::Identity,
        )
        .expect("valid deferred LF coverage");
        let tail = SourceProjectionRun::with_logical(
            CoverageId(3),
            tail_after_lf_bytes,
            tail_after_lf_bytes,
            0,
            CoveragePart::CONTENT,
            paragraph,
            LogicalContribution::Identity,
        )
        .expect("valid retained tail coverage");
        let events = vec![
            GreenEvent::enter(document_block, GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(paragraph, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(prefix),
            GreenEvent::Coverage(deferred_lf),
            GreenEvent::Coverage(tail),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let old = build_grouped(
            document.candidate_writer_test_arena_mut(),
            spec(
                source.revision.0,
                source.root.0,
                1,
                1,
                SerializedMetric {
                    bytes: total,
                    utf16: total,
                },
            ),
            &events,
            &[0..3, 3..7],
        );
        let boundary = old
            .suffix_adoption_boundary_at_event_cut(document.candidate_writer_test_arena(), 3)
            .expect("exact accepted-prefix boundary");
        let tail = old
            .source_tail_adoption_capability(document.candidate_writer_test_arena(), boundary)
            .expect("storage-derived deferred-LF tail capability");
        (old, tail)
    }

    #[cfg(feature = "exact-parser")]
    fn build_blank_gap_tail_capability(
        document: &mut crate::LiveDocumentStore,
    ) -> (SerializedGreenDocument, GreenSourceTailAdoptionCapability) {
        let source = document.source_descriptor();
        assert_eq!(source.bytes, "old\n\nrest".len());
        let document_block = BlockId(1);
        let first_paragraph = BlockId(2);
        let second_paragraph = BlockId(3);
        let first_text = SourceProjectionRun::with_logical(
            CoverageId(1),
            3,
            3,
            0,
            CoveragePart::CONTENT,
            first_paragraph,
            LogicalContribution::Identity,
        )
        .unwrap();
        let first_close =
            SourceProjectionRun::new(CoverageId(2), 1, 1, 0, CoveragePart::TERMINAL).unwrap();
        let blank_gap =
            SourceProjectionRun::new(CoverageId(3), 1, 1, 0, CoveragePart::GAP).unwrap();
        let second_text = SourceProjectionRun::with_logical(
            CoverageId(4),
            4,
            4,
            0,
            CoveragePart::CONTENT,
            second_paragraph,
            LogicalContribution::Identity,
        )
        .unwrap();
        let events = vec![
            GreenEvent::enter(document_block, GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(
                first_paragraph,
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ),
            GreenEvent::Coverage(first_text),
            GreenEvent::Coverage(first_close),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::Coverage(blank_gap),
            GreenEvent::enter(
                second_paragraph,
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ),
            GreenEvent::Coverage(second_text),
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        let old = build_grouped(
            document.candidate_writer_test_arena_mut(),
            spec(
                source.revision.0,
                source.root.0,
                1,
                1,
                SerializedMetric { bytes: 9, utf16: 9 },
            ),
            &events,
            &[0..5, 5..10],
        );
        let boundary = old
            .suffix_adoption_boundary_at_event_cut(document.candidate_writer_test_arena(), 5)
            .expect("exact cut before blank gap");
        let tail = old
            .source_tail_adoption_capability(document.candidate_writer_test_arena(), boundary)
            .expect("storage-derived blank-gap tail");
        (old, tail)
    }

    #[cfg(feature = "exact-parser")]
    fn first_joined_checkpoint(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> crate::exact_block_job::ExactBlockCheckpoint {
        let mut job = ExactBlockJob::new(epoch).expect("exact block job");
        for _ in 0..100_000 {
            assert_eq!(
                job.poll(document).expect("exact prefix poll"),
                ExactBlockJobProgress::Pending
            );
            if job.is_line_boundary_checkpoint_seam() {
                break;
            }
        }
        assert!(job.is_line_boundary_checkpoint_seam());
        let mut capture = match job
            .start_line_boundary_checkpoint(document)
            .expect("checkpoint admission")
        {
            ExactBlockCheckpointAdmission::Started(capture) => *capture,
            ExactBlockCheckpointAdmission::Skipped { reason, .. } => {
                panic!("tail proof checkpoint skipped: {reason:?}")
            }
        };
        for _ in 0..100_000 {
            match capture.poll(document).expect("checkpoint capture") {
                ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => return checkpoint,
            }
        }
        panic!("tail proof checkpoint did not converge")
    }

    #[cfg(feature = "exact-parser")]
    fn prove_lineage(
        mut job: LineageAdoptionBundleJob,
    ) -> (
        LineageAdoptionBundleProof,
        crate::LineageAdoptionBundleMetrics,
    ) {
        loop {
            match job.poll(1) {
                crate::LineageAdoptionBundleStatus::Pending { .. } => {}
                crate::LineageAdoptionBundleStatus::Proven { .. } => break,
                status => panic!("expected unchanged lineage, got {status:?}"),
            }
        }
        let metrics = job.metrics();
        (job.into_proof().expect("completed lineage proof"), metrics)
    }

    #[cfg(feature = "exact-parser")]
    fn activate_current_checkpoint(
        document: &mut crate::LiveDocumentStore,
        semantic_epoch: u64,
    ) -> (
        LiveCandidateEpoch,
        crate::exact_block_job::ExactBlockCheckpoint,
    ) {
        let token = document
            .active_parse_plan()
            .expect("active parse plan")
            .token;
        let epoch = document.begin_candidate(token).expect("current candidate");
        document
            .activate_candidate_source_ledger(epoch)
            .expect("source ledger");
        document
            .activate_candidate_writer(
                epoch,
                crate::CandidateWriterConfig {
                    syntax_profile: 1,
                    grammar_revision: GrammarRevision(1),
                    semantic_epoch,
                },
            )
            .expect("candidate writer");
        let checkpoint = first_joined_checkpoint(document, epoch);
        (epoch, checkpoint)
    }

    #[cfg(feature = "exact-parser")]
    fn drain_abort(document: &mut crate::LiveDocumentStore, abort: crate::CandidateAbort) {
        for _ in 0..1_000 {
            if document
                .poll_candidate_abort(abort, 1)
                .expect("fuelled candidate abort")
                .complete
            {
                return;
            }
        }
        panic!("candidate abort did not converge")
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn ten_mib_unchanged_tail_seals_real_source_and_composer_without_replay() {
        const TEN_MIB: usize = 10 * 1024 * 1024;
        let old_source = format!("old\n{}", "x".repeat(TEN_MIB));
        let mut document = crate::LiveDocumentStore::new(&old_source, 8).expect("live document");
        let old_descriptor = document.source_descriptor();
        let (_old_green, tail) = build_deferred_lf_tail_capability(
            &mut document,
            3,
            u64::try_from(TEN_MIB).expect("10 MiB fits u64"),
        );

        document
            .accept_edit(old_descriptor, 0..3, "new")
            .expect("same-width prefix edit");
        document
            .promote_latest_parse()
            .expect("edited parse becomes active");
        let lineage = document
            .begin_zero_restart_tail_lineage_mechanism_only(&tail)
            .expect("storage-selected lineage job");
        let (lineage, lineage_metrics) = prove_lineage(lineage);
        let (epoch, checkpoint) = activate_current_checkpoint(&mut document, 2);
        let tail = document
            .join_zero_restart_tail_to_candidate_mechanism_only(epoch, tail, lineage)
            .expect("source-bound current tail");
        let receipt = document
            .adopt_candidate_writer_source_composer_tail(epoch, tail)
            .expect("real source/composer fast-forward");

        assert_eq!(receipt.storage.described_suffix_bytes, TEN_MIB as u64 + 1);
        assert_eq!(receipt.storage.described_suffix_utf16, TEN_MIB as u64 + 1);
        // The LF lane performs five exact, document-size-independent source
        // observations: logical-prefix, adjacent LF, physical-prefix,
        // convergence-line, and EOF-line. Keep this exact so a future query
        // accidentally added to the hot join is visible here.
        assert_eq!(receipt.storage.source_queries, 5);
        assert_eq!(receipt.storage.retained_source_roots, 0);
        assert_eq!(receipt.storage.retained_source_bytes, 0);
        assert_eq!(receipt.storage.document_sized_event_vectors, 0);
        assert!(receipt.storage.green_sequence_nodes_visited <= 32);
        // The restart-output query decodes at most three boundary leaves. The
        // downstream source-coordinate seek may decode both its boundary leaf
        // and the successor containing the first source-consuming run.
        assert!(
            receipt.storage.green_leaf_pages_decoded <= 5,
            "tail admission decoded {} green leaf pages",
            receipt.storage.green_leaf_pages_decoded,
        );
        assert!(receipt.storage.green_events_decoded <= 16);
        assert!(receipt.storage.source_index_nodes_visited <= 128);
        assert!(receipt.storage.source_boundary_bytes_scanned <= 16 * 1024);
        assert!(receipt.replayed_prefix_source_pieces < 16);
        assert!(receipt.replayed_prefix_projection_runs < 16);
        assert_eq!(receipt.checkpoint_prefix_projection_runs, 0);
        assert_eq!(
            receipt.cumulative_prefix_projection_runs,
            receipt.replayed_prefix_projection_runs
        );
        assert_eq!(receipt.adopted_suffix_projection_runs, 2);
        assert_eq!(
            receipt.final_projection_runs,
            receipt.cumulative_prefix_projection_runs + 2
        );
        assert_eq!(receipt.accepted_projection_prefix_metric.bytes(), 3);
        assert_eq!(receipt.accepted_projection_prefix_metric.utf16(), 3);
        assert_eq!(receipt.physical_parser_prefix_metric.bytes(), 4);
        assert_eq!(receipt.physical_parser_prefix_metric.utf16(), 4);
        assert_eq!(
            receipt.final_source_metric.bytes(),
            u64::try_from(4 + TEN_MIB).expect("source total fits u64")
        );
        assert_eq!(
            receipt.final_source_metric.utf16(),
            u64::try_from(4 + TEN_MIB).expect("source total fits u64")
        );
        assert_eq!(lineage_metrics.poll_records_examined, 1);
        assert!(lineage_metrics.maximum_tree_nodes_per_lookup <= 16);

        assert!(matches!(
            document.poll_candidate_writer(epoch),
            Err(crate::CandidateWriterError::Busy)
        ));
        let abort = checkpoint.cancel(&mut document).expect("tail-ready cancel");
        drain_abort(&mut document, abort);
        assert_eq!(document.candidate_epoch(), None);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn source_composer_tail_authority_negative_matrix_fails_closed() {
        // A changed retained tail never mints lineage proof.
        let mut changed = crate::LiveDocumentStore::new("old\nrest", 8).unwrap();
        let changed_source = changed.source_descriptor();
        let (_green, changed_tail) = build_two_run_tail_capability(&mut changed, 4, 4);
        changed
            .accept_edit(changed_source, 7..8, "X")
            .expect("tail edit");
        let mut changed_job = changed
            .begin_zero_restart_tail_lineage_mechanism_only(&changed_tail)
            .expect("changed-tail lineage job");
        let changed_status = changed_job.poll(1);
        assert!(matches!(
            changed_status,
            crate::LineageAdoptionBundleStatus::Changed {
                region: crate::LineageAdoptionRegion::RetainedTail,
                ..
            }
        ));
        assert!(matches!(
            changed_job.into_proof(),
            Err(crate::LineageAdoptionBundleStatus::Changed {
                region: crate::LineageAdoptionRegion::RetainedTail,
                ..
            })
        ));

        // A capability/proof from another document cannot bind even when the
        // current bytes happen to be identical.
        let mut origin = crate::LiveDocumentStore::new("old\nrest", 8).unwrap();
        let origin_source = origin.source_descriptor();
        let (_origin_green, foreign_tail) = build_two_run_tail_capability(&mut origin, 4, 4);
        origin
            .accept_edit(origin_source, 0..3, "new")
            .expect("origin prefix edit");
        let foreign_job = origin
            .begin_zero_restart_tail_lineage_mechanism_only(&foreign_tail)
            .unwrap();
        let (foreign_proof, _) = prove_lineage(foreign_job);
        let mut crossed = crate::LiveDocumentStore::new("new\nrest", 8).unwrap();
        let crossed_epoch = crossed
            .begin_candidate(crossed.active_parse_plan().unwrap().token)
            .unwrap();
        assert!(matches!(
            crossed.join_zero_restart_tail_to_candidate_mechanism_only(
                crossed_epoch,
                foreign_tail,
                foreign_proof,
            ),
            Err(TailAdoptionJoinError::WrongCandidate)
        ));
        let crossed_abort = crossed.cancel_candidate(crossed_epoch).unwrap();
        drain_abort(&mut crossed, crossed_abort);

        // Storage convergence inside a physical line is not a resumable source
        // boundary, even when the retained bytes themselves are unchanged.
        let mut unaligned = crate::LiveDocumentStore::new("oldrest", 8).unwrap();
        let unaligned_source = unaligned.source_descriptor();
        let (_unaligned_green, unaligned_tail) =
            build_two_run_tail_capability(&mut unaligned, 3, 4);
        unaligned
            .accept_edit(unaligned_source, 0..1, "n")
            .expect("unaligned prefix edit");
        unaligned.promote_latest_parse().unwrap();
        let unaligned_job = unaligned
            .begin_zero_restart_tail_lineage_mechanism_only(&unaligned_tail)
            .unwrap();
        let (unaligned_proof, _) = prove_lineage(unaligned_job);
        let unaligned_epoch = unaligned
            .begin_candidate(unaligned.active_parse_plan().unwrap().token)
            .unwrap();
        assert!(matches!(
            unaligned.join_zero_restart_tail_to_candidate_mechanism_only(
                unaligned_epoch,
                unaligned_tail,
                unaligned_proof,
            ),
            Err(TailAdoptionJoinError::NotPhysicalLineBoundary)
        ));
        let unaligned_abort = unaligned.cancel_candidate(unaligned_epoch).unwrap();
        drain_abort(&mut unaligned, unaligned_abort);

        // A blank-gap run is not the narrow staged-Terminator predecessor,
        // even when it begins with one LF at the same A/P distance.
        let mut blank = crate::LiveDocumentStore::new("old\n\nrest", 8).unwrap();
        let blank_source = blank.source_descriptor();
        let (_blank_green, blank_tail) = build_blank_gap_tail_capability(&mut blank);
        blank.accept_edit(blank_source, 0..3, "new").unwrap();
        blank.promote_latest_parse().unwrap();
        let blank_job = blank
            .begin_zero_restart_tail_lineage_mechanism_only(&blank_tail)
            .unwrap();
        let (blank_proof, _) = prove_lineage(blank_job);
        let blank_epoch = blank
            .begin_candidate(blank.active_parse_plan().unwrap().token)
            .unwrap();
        assert!(matches!(
            blank.join_zero_restart_tail_to_candidate_mechanism_only(
                blank_epoch,
                blank_tail,
                blank_proof,
            ),
            Err(TailAdoptionJoinError::DeferredTerminatorMismatch)
        ));
        let blank_abort = blank.cancel_candidate(blank_epoch).unwrap();
        drain_abort(&mut blank, blank_abort);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn source_bound_tail_cannot_cross_candidate_builds() {
        let mut document = crate::LiveDocumentStore::new("old\nrest", 8).unwrap();
        let old_source = document.source_descriptor();
        let (_old_green, tail) = build_two_run_tail_capability(&mut document, 4, 4);
        document.accept_edit(old_source, 0..3, "new").unwrap();
        document.promote_latest_parse().unwrap();
        let job = document
            .begin_zero_restart_tail_lineage_mechanism_only(&tail)
            .unwrap();
        let (proof, _) = prove_lineage(job);

        let (first_epoch, first_checkpoint) = activate_current_checkpoint(&mut document, 2);
        let tail = document
            .join_zero_restart_tail_to_candidate_mechanism_only(first_epoch, tail, proof)
            .expect("tail bound to first build");
        let first_abort = first_checkpoint.cancel(&mut document).unwrap();
        drain_abort(&mut document, first_abort);

        let (second_epoch, second_checkpoint) = activate_current_checkpoint(&mut document, 3);
        assert!(matches!(
            document.adopt_candidate_writer_source_composer_tail(second_epoch, tail),
            Err(crate::CandidateWriterError::SourceLedger(
                SourceBoundLedgerError::TailAdoptionMismatch
            ))
        ));
        let second_abort = second_checkpoint.cancel(&mut document).unwrap();
        drain_abort(&mut document, second_abort);
    }

    fn exact_trace(
        document: &SerializedGreenDocument,
        arena: &PageArena,
    ) -> Vec<DecodedGreenEventKind> {
        let mut trace = Vec::new();
        for leaf_index in 0..document.leaf_count(arena).expect("fixture leaf count") {
            let leaf = document
                .leaf_at(arena, leaf_index)
                .expect("fixture leaf lookup")
                .expect("fixture leaf exists");
            let (_, events) = decode_leaf(arena, leaf).expect("fixture leaf decode");
            trace.extend(events.into_iter().map(|event| event.event));
        }
        trace
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One end-to-end proof keeps identities, trace, and receipts together.
    fn nested_prefix_output_repairs_spanning_exits_and_retains_distant_suffix_pages() {
        let old_fixture = nested_fixture(false);
        let current_fixture = nested_fixture(true);
        assert_eq!(old_fixture.cut, current_fixture.cut);
        assert_eq!(old_fixture.events.len(), current_fixture.events.len());
        let ranges = [
            0..old_fixture.cut,
            old_fixture.cut..old_fixture.spanning_end,
            old_fixture.spanning_end..old_fixture.sibling_end,
            old_fixture.sibling_end..old_fixture.events.len(),
        ];
        let metric = SerializedMetric { bytes: 3, utf16: 3 };
        let mut arena = PageArena::new();
        let old = build_grouped(
            &mut arena,
            spec(1, 1, 1, 1, metric),
            &old_fixture.events,
            &ranges,
        );
        let current = build_grouped(
            &mut arena,
            spec(2, 2, 2, 2, metric),
            &current_fixture.events,
            &ranges,
        );
        let current_prefix_leaf = current
            .leaf_at(&arena, 0)
            .expect("current prefix lookup")
            .expect("current prefix leaf");
        let old_repair_leaf = old
            .leaf_at(&arena, 1)
            .expect("old repair lookup")
            .expect("old repair leaf");
        let old_distant_sibling = old
            .leaf_at(&arena, 2)
            .expect("old distant lookup")
            .expect("old distant leaf");
        let old_final_exit = old
            .leaf_at(&arena, 3)
            .expect("old final lookup")
            .expect("old final leaf");

        // A fuel-one read-only step can be cancelled without allocating or
        // changing either immutable owner.
        let current_boundary = current
            .suffix_adoption_boundary_at_event_cut(
                &arena,
                u64::try_from(current_fixture.cut).expect("cut fits u64"),
            )
            .expect("current exact boundary");
        let old_boundary = old
            .suffix_adoption_boundary_at_event_cut(
                &arena,
                u64::try_from(old_fixture.cut).expect("cut fits u64"),
            )
            .expect("old exact boundary");
        let before_cancel = arena.metrics();
        {
            let mut cancelled = current
                .begin_suffix_adoption(&arena, current_boundary, &old, old_boundary)
                .expect("cancellable plan");
            assert_eq!(
                cancelled.poll(&arena).expect("fuel-one repair"),
                GreenSuffixAdoptionPlanProgress::Pending
            );
            assert_eq!(cancelled.receipt().frames_planned, 1);
        }
        assert_eq!(arena.metrics(), before_cancel);

        let current_boundary = current
            .suffix_adoption_boundary_at_event_cut(
                &arena,
                u64::try_from(current_fixture.cut).expect("cut fits u64"),
            )
            .expect("current exact boundary");
        let old_boundary = old
            .suffix_adoption_boundary_at_event_cut(
                &arena,
                u64::try_from(old_fixture.cut).expect("cut fits u64"),
            )
            .expect("old exact boundary");
        let mut planner = current
            .begin_suffix_adoption(&arena, current_boundary, &old, old_boundary)
            .expect("suffix-adoption plan");
        loop {
            if planner.poll(&arena).expect("bounded repair poll")
                == GreenSuffixAdoptionPlanProgress::Ready
            {
                break;
            }
        }
        let commit = planner
            .commit(&mut arena, ParseGeneration(3), 3)
            .expect("atomic suffix adoption");
        let adopted = commit.document;

        assert_eq!(exact_trace(&adopted, &arena), exact_trace(&current, &arena));
        let (adopted_manifest, _) = decode_document(
            &arena,
            adopted
                .local_manifest_id(&arena)
                .expect("adopted manifest ID"),
        )
        .expect("adopted manifest");
        let (current_manifest, _) = decode_document(
            &arena,
            current
                .local_manifest_id(&arena)
                .expect("current manifest ID"),
        )
        .expect("current manifest");
        assert_eq!(adopted_manifest.summary, current_manifest.summary);
        assert_eq!(
            adopted_manifest.source_revision,
            current_manifest.source_revision
        );
        assert_eq!(adopted_manifest.source_root, current_manifest.source_root);
        assert_eq!(adopted_manifest.parse_generation, ParseGeneration(3));
        assert_eq!(adopted_manifest.semantic_epoch, 3);

        assert_eq!(
            adopted.leaf_at(&arena, 0).unwrap(),
            Some(current_prefix_leaf)
        );
        assert_ne!(adopted.leaf_at(&arena, 1).unwrap(), Some(old_repair_leaf));
        assert_eq!(
            adopted.leaf_at(&arena, 2).unwrap(),
            Some(old_distant_sibling)
        );
        assert_eq!(adopted.leaf_at(&arena, 3).unwrap(), Some(old_final_exit));
        assert_eq!(commit.receipt.open_depth, 4);
        assert_eq!(commit.receipt.frames_planned, 4);
        assert_eq!(commit.receipt.spanning_exits_examined, 4);
        assert_eq!(commit.receipt.exit_events_changed, 2);
        assert_eq!(commit.receipt.distinct_exit_leaves_rewritten, 1);
        assert_eq!(commit.receipt.current_prefix_leaves_retained, 1);
        assert_eq!(commit.receipt.old_suffix_leaves_retained, 3);
        assert_eq!(commit.receipt.unchanged_old_suffix_leaves, 2);
        assert_eq!(commit.receipt.retained_source_bytes, 0);
        assert_eq!(commit.receipt.document_sized_event_vectors, 0);
    }

    #[test]
    fn non_boundary_crossed_and_foreign_manifest_cuts_fail_closed() {
        let old_fixture = nested_fixture(false);
        let current_fixture = nested_fixture(true);
        let ranges = [
            0..old_fixture.cut,
            old_fixture.cut..old_fixture.spanning_end,
            old_fixture.spanning_end..old_fixture.sibling_end,
            old_fixture.sibling_end..old_fixture.events.len(),
        ];
        let metric = SerializedMetric { bytes: 3, utf16: 3 };
        let mut arena = PageArena::new();
        let old = build_grouped(
            &mut arena,
            spec(1, 11, 1, 1, metric),
            &old_fixture.events,
            &ranges,
        );
        let current = build_grouped(
            &mut arena,
            spec(2, 12, 2, 2, metric),
            &current_fixture.events,
            &ranges,
        );
        let foreign = build_grouped(
            &mut arena,
            spec(2, 13, 2, 2, metric),
            &current_fixture.events,
            &ranges,
        );
        assert_eq!(
            current
                .suffix_adoption_boundary_at_event_cut(&arena, 1)
                .expect_err("interior packed-leaf cut must fail"),
            SerializedGreenError::StaleCursor
        );

        let current_cut = current
            .suffix_adoption_boundary_at_event_cut(&arena, current_fixture.cut as u64)
            .expect("current cut");
        let old_cut = old
            .suffix_adoption_boundary_at_event_cut(&arena, old_fixture.cut as u64)
            .expect("old cut");
        assert_eq!(
            current
                .begin_suffix_adoption(&arena, old_cut, &old, current_cut)
                .expect_err("crossed cut capabilities must fail"),
            SerializedGreenError::StaleCursor
        );

        let foreign_cut = foreign
            .suffix_adoption_boundary_at_event_cut(&arena, current_fixture.cut as u64)
            .expect("foreign cut");
        let old_cut = old
            .suffix_adoption_boundary_at_event_cut(&arena, old_fixture.cut as u64)
            .expect("old cut");
        assert_eq!(
            current
                .begin_suffix_adoption(&arena, foreign_cut, &old, old_cut)
                .expect_err("foreign manifest cut must fail"),
            SerializedGreenError::StaleCursor
        );
    }

    #[test]
    fn spanning_fenced_code_fails_closed_until_boundary_markers_exist() {
        let mut fixture = Fixture::new();
        fixture.enter(GreenKind::DOCUMENT);
        fixture.enter(GreenKind::FENCED_CODE);
        let cut = fixture.events.len();
        fixture.close(false);
        fixture.close(false);
        let ranges = [0..cut, cut..fixture.events.len()];
        let mut arena = PageArena::new();
        let old = build_grouped(
            &mut arena,
            spec(1, 21, 1, 1, SerializedMetric::default()),
            &fixture.events,
            &ranges,
        );
        let current = build_grouped(
            &mut arena,
            spec(2, 22, 2, 2, SerializedMetric::default()),
            &fixture.events,
            &ranges,
        );
        let current_cut = current
            .suffix_adoption_boundary_at_event_cut(&arena, cut as u64)
            .expect("current fenced cut");
        let old_cut = old
            .suffix_adoption_boundary_at_event_cut(&arena, cut as u64)
            .expect("old fenced cut");
        assert_eq!(
            current
                .begin_suffix_adoption(&arena, current_cut, &old, old_cut)
                .expect_err("spanning FencedCode must fail closed"),
            SerializedGreenError::Invalid(
                "spanning FencedCode Exit repair requires typed boundary markers"
            )
        );
    }
}
