//! Authenticated provisional-Paragraph projection replay for GFM Table promotion.
//!
//! This is deliberately a mechanism gate, not the shipping writer hookup.  It
//! proves the narrow seam the packed-green builder still needs to mint:
//!
//! 1. a read-only Table pass scans immutable logical-row leases;
//! 2. success returns a non-`Clone` authority bound to the exact candidate,
//!    Paragraph, projection root/generations, row cuts, grammar and writer
//!    epoch;
//! 3. a private join replays the same retained projection root, including
//!    syntax gaps and Program provenance, into one speculative writer
//!    transaction; and
//! 4. callers never receive scalar cuts or author projection actions.
//!
//! `ResumableSerializedGreenBuild` does not yet mint this cursor.  Its eventual
//! actor-owned session must replace `ImmutableTableProjectionProvider` without
//! weakening these identities or exposing journal events/Program pages to
//! grammar code.  No cloneable arena snapshot is required: the unpublished
//! build already outlives the non-`Clone` session, and cancellation invalidates
//! both.  The `Arc<[u8]>` rows below exist only because the current isolated
//! scanner facade accepts a byte slice.  Production should instead feed that
//! scanner from the joined green/composer/Crop session one logical byte at a
//! time, avoiding a `String` or row copy.  Production `TableReady` should hold
//! only a non-`Clone` seal naming that actor-owned session; moving the Arc-backed
//! provider through `TableReady` below is solely a self-contained test model.

#![allow(
    clippy::match_same_arms,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use std::{fmt, ops::Range, sync::Arc};

use flark_oversized_block_line_gate::{
    CancellationToken, CellSummary, TableHeaderDisposition, TableHeaderPassOneJob,
    TableHeaderPassOnePoll, TableHeaderRejectReason, TableHeaderReplayJob, TableHeaderReplayPoll,
    TableReplayError, ValidatedTableHeader,
};

use crate::{CandidateWriterConfig, LiveCandidateEpoch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectedLineRole {
    Header,
    Delimiter,
}

impl ProjectedLineRole {
    const fn index(self) -> usize {
        match self {
            Self::Header => 0,
            Self::Delimiter => 1,
        }
    }
}

/// Builder-minted identity of the provisional Paragraph being replaced.
///
/// The production provider must derive this from
/// `CandidateWriterBindingIdentity`; this local representation exists only
/// because the Setext work owns that shared writer seam while this gate runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProvisionalParagraphIdentity {
    owner: u64,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectionFragmentIdentity(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectionCutIdentity {
    fragment: ProjectionFragmentIdentity,
    ordinal: u8,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectionProgramPageIdentity(u64);

/// Opaque actor join of the builder leaf barrier, composer projection
/// high-water, Crop source cursor, and Paragraph consumer ownership.  It is
/// intentionally not an event ordinal or source offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParagraphProjectionBarrierIdentity(u64);

/// Opaque ownership of the staged delimiter's terminator and containing block
/// path.  A raw source range cannot substitute for this actor-held witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DelimiterRecognitionOwnershipIdentity(u64);

/// Complete equality witness required by the private writer join.
///
/// `LiveCandidateEpoch` binds source root/revision and arena build.  The
/// writer config binds syntax profile, grammar revision and semantic epoch.
/// Cut identities are capabilities minted from exact journal positions, not
/// offsets echoed by the grammar adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TableProjectionBinding {
    epoch: LiveCandidateEpoch,
    writer_config: CandidateWriterConfig,
    paragraph: ProvisionalParagraphIdentity,
    paragraph_barrier: ParagraphProjectionBarrierIdentity,
    fragment: ProjectionFragmentIdentity,
    projection_generation: u64,
    program_generation: u64,
    header_cut: ProjectionCutIdentity,
    delimiter_cut: ProjectionCutIdentity,
    delimiter_owner: DelimiterRecognitionOwnershipIdentity,
    writer_epoch: u64,
    table_visited: bool,
}

impl TableProjectionBinding {
    fn first_mismatch(self, expected: Self) -> Option<TableJoinBindingMismatch> {
        if self.epoch.source() != expected.epoch.source() {
            return Some(TableJoinBindingMismatch::Source);
        }
        if self.epoch != expected.epoch {
            return Some(TableJoinBindingMismatch::CandidateEpoch);
        }
        if self.writer_config != expected.writer_config {
            return Some(TableJoinBindingMismatch::GrammarOrSemanticConfig);
        }
        if self.paragraph != expected.paragraph {
            return Some(TableJoinBindingMismatch::Paragraph);
        }
        if self.paragraph_barrier != expected.paragraph_barrier {
            return Some(TableJoinBindingMismatch::ParagraphBarrier);
        }
        if self.fragment != expected.fragment
            || self.projection_generation != expected.projection_generation
            || self.program_generation != expected.program_generation
        {
            return Some(TableJoinBindingMismatch::Projection);
        }
        if self.header_cut != expected.header_cut || self.delimiter_cut != expected.delimiter_cut {
            return Some(TableJoinBindingMismatch::Cuts);
        }
        if self.delimiter_owner != expected.delimiter_owner {
            return Some(TableJoinBindingMismatch::DelimiterOwnership);
        }
        if self.writer_epoch != expected.writer_epoch {
            return Some(TableJoinBindingMismatch::WriterEpoch);
        }
        if self.table_visited != expected.table_visited {
            return Some(TableJoinBindingMismatch::CandidateState);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableJoinBindingMismatch {
    Source,
    CandidateEpoch,
    GrammarOrSemanticConfig,
    Paragraph,
    ParagraphBarrier,
    Projection,
    Cuts,
    DelimiterOwnership,
    WriterEpoch,
    CandidateState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HiddenProjectionAffinity {
    Before,
    After,
}

/// Typed provenance retained by one provisional projection segment.
///
/// A Program carries only an observational page identity here.  Replay
/// authority comes from the non-`Clone` piece capability retaining the root;
/// a numeric page identity alone can perform no operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectionOrigin {
    Identity,
    Hidden {
        affinity: HiddenProjectionAffinity,
    },
    TabToSpaces {
        spaces: u8,
    },
    NulToReplacement,
    CanonicalCrLf,
    CanonicalLoneCr,
    Program {
        page: ProjectionProgramPageIdentity,
        generation: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionSegment {
    parser: Range<usize>,
    physical: Range<usize>,
    origin: ProjectionOrigin,
}

#[derive(Debug)]
struct ProjectedLineRoot {
    logical: Arc<[u8]>,
    segments: Arc<[ProjectionSegment]>,
    cut: ProjectionCutIdentity,
}

#[derive(Debug)]
struct ProjectionFragmentRoot {
    identity: ProjectionFragmentIdentity,
    projection_generation: u64,
    program_generation: u64,
    paragraph_barrier: ParagraphProjectionBarrierIdentity,
    delimiter_owner: DelimiterRecognitionOwnershipIdentity,
    lines: [ProjectedLineRoot; 2],
}

impl ProjectionFragmentRoot {
    fn line(&self, role: ProjectedLineRole) -> &ProjectedLineRoot {
        &self.lines[role.index()]
    }

    fn validate(&self) -> Result<(), TableProjectionProviderError> {
        for line in &self.lines {
            let mut parser_cut = 0;
            let mut physical_cut = 0;
            for segment in line.segments.iter() {
                if segment.parser.start != parser_cut || segment.physical.start != physical_cut {
                    return Err(TableProjectionProviderError::NonPartitioningProjection);
                }
                if segment.parser.start > segment.parser.end
                    || segment.physical.start > segment.physical.end
                {
                    return Err(TableProjectionProviderError::NonPartitioningProjection);
                }
                if segment.parser.is_empty()
                    && !matches!(segment.origin, ProjectionOrigin::Hidden { .. })
                {
                    return Err(TableProjectionProviderError::ZeroLogicalNonHiddenSegment);
                }
                if segment.physical.is_empty() {
                    return Err(TableProjectionProviderError::ZeroPhysicalSegment);
                }
                parser_cut = segment.parser.end;
                physical_cut = segment.physical.end;
            }
            if parser_cut != line.logical.len() {
                return Err(TableProjectionProviderError::NonPartitioningProjection);
            }
            if line.cut.fragment != self.identity {
                return Err(TableProjectionProviderError::CrossedCut);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableProjectionProviderError {
    AlreadyVisited,
    CrossedCut,
    CrossedProjectionRoot,
    NonPartitioningProjection,
    ZeroLogicalNonHiddenSegment,
    ZeroPhysicalSegment,
    InvalidCellRange,
    ReversedReplayCut,
    UnalignedAtomicCut,
}

/// Mechanism-provider contract.  The grammar adapter can start validation,
/// but only the join below calls the projection-cursor methods.
///
/// `logical_line` is explicitly the temporary adapter mismatch called out in
/// the module docs; it is not a requirement for the actor-held production
/// cursor.
trait AuthenticatedTableProjectionProvider: Sized {
    fn binding(&self) -> TableProjectionBinding;
    fn logical_line(&self, role: ProjectedLineRole) -> Arc<[u8]>;
    fn line_len(&self, role: ProjectedLineRole) -> usize;
    fn open_cell_cursor(
        &self,
        role: ProjectedLineRole,
        previous_cut: usize,
        cell: &CellSummary,
        content_is_logical: bool,
    ) -> Result<AuthenticatedProjectionCursor, TableProjectionProviderError>;
    fn open_tail_cursor(
        &self,
        role: ProjectedLineRole,
        previous_cut: usize,
    ) -> Result<AuthenticatedProjectionCursor, TableProjectionProviderError>;
}

/// In-memory stand-in for the builder-owned provisional-fragment session.
/// It retains immutable logical rows and typed projection descriptors.  It is
/// intentionally module-private and has no scalar replay method.  Its `Arc`
/// storage makes tests self-contained; production does not clone this root.
#[derive(Debug)]
struct ImmutableTableProjectionProvider {
    binding: TableProjectionBinding,
    root: Arc<ProjectionFragmentRoot>,
}

impl ImmutableTableProjectionProvider {
    fn try_new(
        binding: TableProjectionBinding,
        root: ProjectionFragmentRoot,
    ) -> Result<Self, TableProjectionProviderError> {
        if binding.table_visited {
            return Err(TableProjectionProviderError::AlreadyVisited);
        }
        if root.identity != binding.fragment
            || root.projection_generation != binding.projection_generation
            || root.program_generation != binding.program_generation
            || root.paragraph_barrier != binding.paragraph_barrier
            || root.delimiter_owner != binding.delimiter_owner
        {
            return Err(TableProjectionProviderError::CrossedProjectionRoot);
        }
        if root.line(ProjectedLineRole::Header).cut != binding.header_cut
            || root.line(ProjectedLineRole::Delimiter).cut != binding.delimiter_cut
        {
            return Err(TableProjectionProviderError::CrossedCut);
        }
        root.validate()?;
        Ok(Self {
            binding,
            root: Arc::new(root),
        })
    }

    fn open_cursor(
        &self,
        role: ProjectedLineRole,
        total: Range<usize>,
        content: Option<Range<usize>>,
    ) -> Result<AuthenticatedProjectionCursor, TableProjectionProviderError> {
        AuthenticatedProjectionCursor::new(Arc::clone(&self.root), role, total, content)
    }
}

impl AuthenticatedTableProjectionProvider for ImmutableTableProjectionProvider {
    fn binding(&self) -> TableProjectionBinding {
        self.binding
    }

    fn logical_line(&self, role: ProjectedLineRole) -> Arc<[u8]> {
        Arc::clone(&self.root.line(role).logical)
    }

    fn line_len(&self, role: ProjectedLineRole) -> usize {
        self.root.line(role).logical.len()
    }

    fn open_cell_cursor(
        &self,
        role: ProjectedLineRole,
        previous_cut: usize,
        cell: &CellSummary,
        content_is_logical: bool,
    ) -> Result<AuthenticatedProjectionCursor, TableProjectionProviderError> {
        let line_len = self.line_len(role);
        if previous_cut > cell.source.start
            || cell.source.start > cell.source.end
            || cell.source.end > line_len
            || cell.content.start < cell.source.start
            || cell.content.end > cell.source.end
            || cell.content.start > cell.content.end
        {
            return Err(TableProjectionProviderError::InvalidCellRange);
        }
        self.open_cursor(
            role,
            previous_cut..cell.source.end,
            content_is_logical.then(|| cell.content.clone()),
        )
    }

    fn open_tail_cursor(
        &self,
        role: ProjectedLineRole,
        previous_cut: usize,
    ) -> Result<AuthenticatedProjectionCursor, TableProjectionProviderError> {
        let line_len = self.line_len(role);
        if previous_cut > line_len {
            return Err(TableProjectionProviderError::ReversedReplayCut);
        }
        self.open_cursor(role, previous_cut..line_len, None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableProjectionPart {
    Syntax,
    CellContent,
}

/// Non-`Clone` root-retaining authority for exactly one projection slice.
struct AuthenticatedPhysicalCutCapability {
    window: Range<usize>,
}

impl AuthenticatedPhysicalCutCapability {
    fn bytes(&self) -> usize {
        self.window.len()
    }
}

/// Non-`Clone` root-retaining authority for exactly one logical projection
/// slice and its non-overlapping physical-source consumption.
#[must_use = "projection authority must be consumed by the private Table writer join"]
struct AuthenticatedProjectionPiece {
    root: Arc<ProjectionFragmentRoot>,
    role: ProjectedLineRole,
    segment: usize,
    parser_window: Range<usize>,
    physical_cut: AuthenticatedPhysicalCutCapability,
    part: TableProjectionPart,
}

impl AuthenticatedProjectionPiece {
    fn origin(&self) -> ProjectionOrigin {
        self.root.line(self.role).segments[self.segment].origin
    }

    const fn part(&self) -> TableProjectionPart {
        self.part
    }

    fn parser_bytes(&self) -> usize {
        self.parser_window.len()
    }

    fn physical_bytes(&self) -> usize {
        self.physical_cut.bytes()
    }
}

impl fmt::Debug for AuthenticatedProjectionPiece {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedProjectionPiece")
            .field("role", &self.role)
            .field("origin", &self.origin())
            .field("parser_bytes", &self.parser_bytes())
            .field("physical_bytes", &self.physical_bytes())
            .field("part", &self.part)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
enum ProjectionCursorPoll {
    Piece {
        value: AuthenticatedProjectionPiece,
        inspected: usize,
    },
    Complete {
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
}

/// Minimal reusable cursor behavior required from provisional packed green.
/// One poll crosses at most one retained projection-piece partition.  Program
/// decoding remains page-bounded behind the capability consumed by the writer.
/// In production this state remains inside `LiveDocumentStore` beside the
/// builder and Crop cursor; the `Arc` root is only the test backing store.
#[derive(Debug)]
struct AuthenticatedProjectionCursor {
    root: Arc<ProjectionFragmentRoot>,
    role: ProjectedLineRole,
    total: Range<usize>,
    content: Option<Range<usize>>,
    segment: usize,
    parser_cut: usize,
    complete: bool,
}

impl AuthenticatedProjectionCursor {
    fn new(
        root: Arc<ProjectionFragmentRoot>,
        role: ProjectedLineRole,
        total: Range<usize>,
        content: Option<Range<usize>>,
    ) -> Result<Self, TableProjectionProviderError> {
        let line = root.line(role);
        if total.start > total.end || total.end > line.logical.len() {
            return Err(TableProjectionProviderError::ReversedReplayCut);
        }
        if content.as_ref().is_some_and(|content| {
            content.start > content.end || content.start < total.start || content.end > total.end
        }) {
            return Err(TableProjectionProviderError::InvalidCellRange);
        }
        for boundary in [
            Some(total.start),
            Some(total.end),
            content.as_ref().map(|content| content.start),
            content.as_ref().map(|content| content.end),
        ]
        .into_iter()
        .flatten()
        {
            let Some(segment) = line
                .segments
                .iter()
                .find(|segment| segment.parser.start < boundary && boundary < segment.parser.end)
            else {
                continue;
            };
            let splittable_identity = matches!(segment.origin, ProjectionOrigin::Identity)
                && segment.parser.len() == segment.physical.len();
            if !splittable_identity {
                return Err(TableProjectionProviderError::UnalignedAtomicCut);
            }
        }
        Ok(Self {
            root,
            role,
            parser_cut: total.start,
            total,
            content,
            segment: 0,
            complete: false,
        })
    }

    fn poll(&mut self, cancellation: &CancellationToken) -> ProjectionCursorPoll {
        if cancellation.is_cancelled() {
            return ProjectionCursorPoll::Cancelled { inspected: 0 };
        }
        if self.complete {
            return ProjectionCursorPoll::Complete { inspected: 0 };
        }

        let line = self.root.line(self.role);
        while let Some(segment) = line.segments.get(self.segment) {
            if segment.parser.is_empty() {
                let point = segment.parser.start;
                let segment_index = self.segment;
                self.segment += 1;
                if point >= self.total.start && point < self.total.end {
                    return ProjectionCursorPoll::Piece {
                        value: AuthenticatedProjectionPiece {
                            root: Arc::clone(&self.root),
                            role: self.role,
                            segment: segment_index,
                            parser_window: point..point,
                            physical_cut: AuthenticatedPhysicalCutCapability {
                                window: segment.physical.clone(),
                            },
                            part: TableProjectionPart::Syntax,
                        },
                        inspected: 1,
                    };
                }
                continue;
            }

            if segment.parser.end <= self.total.start || segment.parser.start >= self.total.end {
                self.segment += 1;
                continue;
            }

            let start = self
                .parser_cut
                .max(segment.parser.start)
                .max(self.total.start);
            let mut end = segment.parser.end.min(self.total.end);
            let part = self
                .content
                .as_ref()
                .map_or(TableProjectionPart::Syntax, |content| {
                    if start < content.start {
                        end = end.min(content.start);
                        TableProjectionPart::Syntax
                    } else if start < content.end {
                        end = end.min(content.end);
                        TableProjectionPart::CellContent
                    } else {
                        TableProjectionPart::Syntax
                    }
                });
            debug_assert!(start < end, "cursor partition must make progress");
            let segment_index = self.segment;
            let physical_window = if start == segment.parser.start && end == segment.parser.end {
                segment.physical.clone()
            } else {
                debug_assert!(matches!(segment.origin, ProjectionOrigin::Identity));
                debug_assert_eq!(segment.parser.len(), segment.physical.len());
                let physical_start = segment.physical.start + (start - segment.parser.start);
                physical_start..physical_start + (end - start)
            };
            self.parser_cut = end;
            if end == segment.parser.end || end == self.total.end {
                self.segment += 1;
            }
            return ProjectionCursorPoll::Piece {
                value: AuthenticatedProjectionPiece {
                    root: Arc::clone(&self.root),
                    role: self.role,
                    segment: segment_index,
                    parser_window: start..end,
                    physical_cut: AuthenticatedPhysicalCutCapability {
                        window: physical_window,
                    },
                    part,
                },
                inspected: 1,
            };
        }
        self.complete = true;
        ProjectionCursorPoll::Complete { inspected: 0 }
    }
}

struct TableValidationAuthority<P> {
    binding: TableProjectionBinding,
    provider: P,
}

impl<P: fmt::Debug> fmt::Debug for TableValidationAuthority<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TableValidationAuthority")
            .field("binding", &self.binding)
            .field("provider", &self.provider)
            .finish()
    }
}

#[derive(Debug)]
enum AuthenticatedTableValidationPoll<P> {
    Pending {
        inspected: usize,
    },
    NotCandidate {
        provider: P,
        inspected: usize,
    },
    Rejected {
        provider: P,
        reason: TableHeaderRejectReason,
        inspected: usize,
    },
    Ready {
        value: AuthenticatedTableReady<P>,
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
}

/// Read-only validation wrapper.  Logical row leases and actor binding are
/// obtained once from the provider and moved into the scanner job.
#[derive(Debug)]
struct AuthenticatedTableValidationJob<P> {
    inner: TableHeaderPassOneJob<TableValidationAuthority<P>>,
}

impl<P: AuthenticatedTableProjectionProvider> AuthenticatedTableValidationJob<P> {
    fn new(provider: P) -> Result<Self, TableProjectionProviderError> {
        let binding = provider.binding();
        if binding.table_visited {
            return Err(TableProjectionProviderError::AlreadyVisited);
        }
        let header = provider.logical_line(ProjectedLineRole::Header);
        let delimiter = provider.logical_line(ProjectedLineRole::Delimiter);
        Ok(Self {
            inner: TableHeaderPassOneJob::new(
                TableValidationAuthority { binding, provider },
                header,
                delimiter,
            ),
        })
    }

    fn poll(
        &mut self,
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> AuthenticatedTableValidationPoll<P> {
        match self.inner.poll(fuel, cancellation) {
            TableHeaderPassOnePoll::Pending { inspected } => {
                AuthenticatedTableValidationPoll::Pending { inspected }
            }
            TableHeaderPassOnePoll::Cancelled { inspected } => {
                AuthenticatedTableValidationPoll::Cancelled { inspected }
            }
            TableHeaderPassOnePoll::Complete { value, inspected } => match value {
                TableHeaderDisposition::NotCandidate { binding } => {
                    AuthenticatedTableValidationPoll::NotCandidate {
                        provider: binding.provider,
                        inspected,
                    }
                }
                TableHeaderDisposition::Rejected { binding, reason } => {
                    AuthenticatedTableValidationPoll::Rejected {
                        provider: binding.provider,
                        reason,
                        inspected,
                    }
                }
                TableHeaderDisposition::Ready(value) => AuthenticatedTableValidationPoll::Ready {
                    value: AuthenticatedTableReady { inner: value },
                    inspected,
                },
            },
        }
    }
}

/// Non-`Clone` writer authority.  Its certified cell count lives in the
/// scanner-owned value and is checked again during replay.  The generic
/// provider is moved here only for this isolated mechanism; production carries
/// a session seal while `LiveDocumentStore` retains the actual cursor state.
#[must_use = "a certified Table must enter the private writer join or be discarded"]
struct AuthenticatedTableReady<P> {
    inner: ValidatedTableHeader<TableValidationAuthority<P>>,
}

impl<P> AuthenticatedTableReady<P> {
    fn columns(&self) -> u32 {
        self.inner.columns()
    }
}

impl<P: fmt::Debug> fmt::Debug for AuthenticatedTableReady<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedTableReady")
            .field("columns", &self.columns())
            .finish_non_exhaustive()
    }
}

/// Private sink implemented by the candidate writer transaction.  Projection
/// pieces cannot be constructed by its caller and remain bound to the retained
/// fragment root until this method consumes them.
trait AuthenticatedTableProjectionSink {
    fn expected_binding(&self) -> TableProjectionBinding;
    fn begin_table_transaction(&mut self, columns: u32);
    fn begin_cell(&mut self, column: u32, alignment: u8);
    fn push_projection(&mut self, piece: AuthenticatedProjectionPiece);
    fn end_cell(&mut self);
    fn commit_table_transaction(&mut self);
    fn abort_table_transaction(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableProjectionJoinError {
    Binding(TableJoinBindingMismatch),
    Provider(TableProjectionProviderError),
    Replay(TableReplayError),
}

#[derive(Debug)]
struct TableProjectionJoinStartError<W> {
    error: TableProjectionJoinError,
    writer: W,
}

#[derive(Debug)]
enum TableProjectionJoinPoll {
    Pending {
        inspected: usize,
    },
    Complete {
        inspected: usize,
    },
    Cancelled {
        inspected: usize,
    },
    Failed {
        error: TableProjectionJoinError,
        inspected: usize,
    },
}

#[derive(Debug)]
struct PendingCellProjection {
    header: AuthenticatedProjectionCursor,
    delimiter: AuthenticatedProjectionCursor,
    header_complete: bool,
}

#[derive(Debug)]
struct PendingTailProjection {
    header: AuthenticatedProjectionCursor,
    delimiter: AuthenticatedProjectionCursor,
    header_complete: bool,
}

/// Private validate-then-replay join.  Scanner ranges never cross this seam;
/// they immediately select authenticated cursors from the retained provider.
#[derive(Debug)]
struct AuthenticatedTableProjectionJoin<P, W> {
    replay: Option<TableHeaderReplayJob<TableValidationAuthority<P>>>,
    completed_authority: Option<TableValidationAuthority<P>>,
    writer: W,
    header_cut: usize,
    delimiter_cut: usize,
    pending_cell: Option<PendingCellProjection>,
    pending_tail: Option<PendingTailProjection>,
    complete: bool,
}

impl<P, W> AuthenticatedTableProjectionJoin<P, W>
where
    P: AuthenticatedTableProjectionProvider,
    W: AuthenticatedTableProjectionSink,
{
    fn new(
        ready: AuthenticatedTableReady<P>,
        mut writer: W,
    ) -> Result<Self, TableProjectionJoinStartError<W>> {
        let replay = ready.inner.into_replay();
        let actual = replay.binding().binding;
        if let Some(mismatch) = actual.first_mismatch(writer.expected_binding()) {
            return Err(TableProjectionJoinStartError {
                error: TableProjectionJoinError::Binding(mismatch),
                writer,
            });
        }
        writer.begin_table_transaction(replay.columns());
        Ok(Self {
            replay: Some(replay),
            completed_authority: None,
            writer,
            header_cut: 0,
            delimiter_cut: 0,
            pending_cell: None,
            pending_tail: None,
            complete: false,
        })
    }

    fn poll(&mut self, fuel: usize, cancellation: &CancellationToken) -> TableProjectionJoinPoll {
        assert!(fuel > 0);
        assert!(
            !self.complete,
            "Table projection join polled after completion"
        );
        if cancellation.is_cancelled() {
            return TableProjectionJoinPoll::Cancelled { inspected: 0 };
        }

        if self.pending_cell.is_some() {
            return self.poll_pending_cell(cancellation);
        }
        if self.pending_tail.is_some() {
            return self.poll_pending_tail(cancellation);
        }
        if self.completed_authority.is_some() {
            self.writer.commit_table_transaction();
            self.complete = true;
            return TableProjectionJoinPoll::Complete { inspected: 0 };
        }

        let replay = self
            .replay
            .as_mut()
            .expect("active join retains replay until scanner completion");
        match replay.poll(fuel, cancellation) {
            TableHeaderReplayPoll::Pending { inspected } => {
                TableProjectionJoinPoll::Pending { inspected }
            }
            TableHeaderReplayPoll::Cancelled { inspected } => {
                TableProjectionJoinPoll::Cancelled { inspected }
            }
            TableHeaderReplayPoll::Failed { error, inspected } => {
                self.writer.abort_table_transaction();
                TableProjectionJoinPoll::Failed {
                    error: TableProjectionJoinError::Replay(error),
                    inspected,
                }
            }
            TableHeaderReplayPoll::Cell { value, inspected } => {
                let authority = replay.binding();
                let header = authority.provider.open_cell_cursor(
                    ProjectedLineRole::Header,
                    self.header_cut,
                    value.header(),
                    true,
                );
                let delimiter = authority.provider.open_cell_cursor(
                    ProjectedLineRole::Delimiter,
                    self.delimiter_cut,
                    value.delimiter(),
                    false,
                );
                let (header, delimiter) = match (header, delimiter) {
                    (Ok(header), Ok(delimiter)) => (header, delimiter),
                    (Err(error), _) | (_, Err(error)) => {
                        self.writer.abort_table_transaction();
                        return TableProjectionJoinPoll::Failed {
                            error: TableProjectionJoinError::Provider(error),
                            inspected,
                        };
                    }
                };
                self.header_cut = value.header().source.end;
                self.delimiter_cut = value.delimiter().source.end;
                self.writer.begin_cell(value.column(), value.alignment());
                self.pending_cell = Some(PendingCellProjection {
                    header,
                    delimiter,
                    header_complete: false,
                });
                TableProjectionJoinPoll::Pending { inspected }
            }
            TableHeaderReplayPoll::Complete { binding, inspected } => {
                let header = binding
                    .provider
                    .open_tail_cursor(ProjectedLineRole::Header, self.header_cut);
                let delimiter = binding
                    .provider
                    .open_tail_cursor(ProjectedLineRole::Delimiter, self.delimiter_cut);
                let (header, delimiter) = match (header, delimiter) {
                    (Ok(header), Ok(delimiter)) => (header, delimiter),
                    (Err(error), _) | (_, Err(error)) => {
                        self.writer.abort_table_transaction();
                        return TableProjectionJoinPoll::Failed {
                            error: TableProjectionJoinError::Provider(error),
                            inspected,
                        };
                    }
                };
                self.pending_tail = Some(PendingTailProjection {
                    header,
                    delimiter,
                    header_complete: false,
                });
                self.completed_authority = Some(binding);
                self.replay = None;
                TableProjectionJoinPoll::Pending { inspected }
            }
        }
    }

    fn poll_pending_cell(&mut self, cancellation: &CancellationToken) -> TableProjectionJoinPoll {
        let pending = self
            .pending_cell
            .as_mut()
            .expect("pending cell was checked above");
        let progress = if pending.header_complete {
            pending.delimiter.poll(cancellation)
        } else {
            pending.header.poll(cancellation)
        };
        match progress {
            ProjectionCursorPoll::Piece { value, inspected } => {
                self.writer.push_projection(value);
                TableProjectionJoinPoll::Pending { inspected }
            }
            ProjectionCursorPoll::Cancelled { inspected } => {
                TableProjectionJoinPoll::Cancelled { inspected }
            }
            ProjectionCursorPoll::Complete { inspected } if !pending.header_complete => {
                pending.header_complete = true;
                TableProjectionJoinPoll::Pending { inspected }
            }
            ProjectionCursorPoll::Complete { inspected } => {
                self.writer.end_cell();
                self.pending_cell = None;
                TableProjectionJoinPoll::Pending { inspected }
            }
        }
    }

    fn poll_pending_tail(&mut self, cancellation: &CancellationToken) -> TableProjectionJoinPoll {
        let pending = self
            .pending_tail
            .as_mut()
            .expect("pending tail was checked above");
        let progress = if pending.header_complete {
            pending.delimiter.poll(cancellation)
        } else {
            pending.header.poll(cancellation)
        };
        match progress {
            ProjectionCursorPoll::Piece { value, inspected } => {
                self.writer.push_projection(value);
                TableProjectionJoinPoll::Pending { inspected }
            }
            ProjectionCursorPoll::Cancelled { inspected } => {
                TableProjectionJoinPoll::Cancelled { inspected }
            }
            ProjectionCursorPoll::Complete { inspected } if !pending.header_complete => {
                pending.header_complete = true;
                TableProjectionJoinPoll::Pending { inspected }
            }
            ProjectionCursorPoll::Complete { inspected } => {
                self.pending_tail = None;
                TableProjectionJoinPoll::Pending { inspected }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use flark_oversized_block_line_gate::MAX_TABLE_CELLS;

    use super::*;
    use crate::{GrammarRevision, LiveDocumentStore};

    const CONFIG: CandidateWriterConfig = CandidateWriterConfig {
        syntax_profile: 7,
        grammar_revision: GrammarRevision(23),
        semantic_epoch: 11,
    };

    #[derive(Clone, Copy)]
    struct FixtureIdentity {
        fragment: u64,
        paragraph: u64,
        paragraph_generation: u64,
        paragraph_barrier: u64,
        projection_generation: u64,
        program_generation: u64,
        delimiter_owner: u64,
        writer_epoch: u64,
    }

    impl Default for FixtureIdentity {
        fn default() -> Self {
            Self {
                fragment: 1,
                paragraph: 2,
                paragraph_generation: 3,
                paragraph_barrier: 4,
                projection_generation: 5,
                program_generation: 6,
                delimiter_owner: 7,
                writer_epoch: 8,
            }
        }
    }

    fn begin_epoch(text: &str) -> LiveCandidateEpoch {
        let mut document = LiveDocumentStore::new(text, 4).unwrap();
        document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap()
    }

    fn binding(epoch: LiveCandidateEpoch, identity: FixtureIdentity) -> TableProjectionBinding {
        let fragment = ProjectionFragmentIdentity(identity.fragment);
        TableProjectionBinding {
            epoch,
            writer_config: CONFIG,
            paragraph: ProvisionalParagraphIdentity {
                owner: identity.paragraph,
                generation: identity.paragraph_generation,
            },
            paragraph_barrier: ParagraphProjectionBarrierIdentity(identity.paragraph_barrier),
            fragment,
            projection_generation: identity.projection_generation,
            program_generation: identity.program_generation,
            header_cut: ProjectionCutIdentity {
                fragment,
                ordinal: 0,
                generation: identity.projection_generation,
            },
            delimiter_cut: ProjectionCutIdentity {
                fragment,
                ordinal: 1,
                generation: identity.projection_generation,
            },
            delimiter_owner: DelimiterRecognitionOwnershipIdentity(identity.delimiter_owner),
            writer_epoch: identity.writer_epoch,
            table_visited: false,
        }
    }

    fn identity_segments(bytes: &[u8]) -> Arc<[ProjectionSegment]> {
        Arc::from([ProjectionSegment {
            parser: 0..bytes.len(),
            physical: 0..bytes.len(),
            origin: ProjectionOrigin::Identity,
        }])
    }

    fn provider_with_segments(
        binding: TableProjectionBinding,
        header: Arc<[u8]>,
        delimiter: Arc<[u8]>,
        header_segments: Arc<[ProjectionSegment]>,
        delimiter_segments: Arc<[ProjectionSegment]>,
    ) -> ImmutableTableProjectionProvider {
        let root = ProjectionFragmentRoot {
            identity: binding.fragment,
            projection_generation: binding.projection_generation,
            program_generation: binding.program_generation,
            paragraph_barrier: binding.paragraph_barrier,
            delimiter_owner: binding.delimiter_owner,
            lines: [
                ProjectedLineRoot {
                    logical: header,
                    segments: header_segments,
                    cut: binding.header_cut,
                },
                ProjectedLineRoot {
                    logical: delimiter,
                    segments: delimiter_segments,
                    cut: binding.delimiter_cut,
                },
            ],
        };
        ImmutableTableProjectionProvider::try_new(binding, root).unwrap()
    }

    fn identity_provider(
        binding: TableProjectionBinding,
        header: Arc<[u8]>,
        delimiter: Arc<[u8]>,
    ) -> ImmutableTableProjectionProvider {
        let header_segments = identity_segments(&header);
        let delimiter_segments = identity_segments(&delimiter);
        provider_with_segments(
            binding,
            header,
            delimiter,
            header_segments,
            delimiter_segments,
        )
    }

    fn drive_ready<P: AuthenticatedTableProjectionProvider>(
        provider: P,
        fuel: usize,
    ) -> Result<AuthenticatedTableReady<P>, TableHeaderRejectReason> {
        let mut job = AuthenticatedTableValidationJob::new(provider).unwrap();
        let cancellation = CancellationToken::default();
        loop {
            match job.poll(fuel, &cancellation) {
                AuthenticatedTableValidationPoll::Pending { inspected } => {
                    assert!(inspected <= fuel);
                }
                AuthenticatedTableValidationPoll::Ready { value, inspected } => {
                    assert!(inspected <= fuel);
                    return Ok(value);
                }
                AuthenticatedTableValidationPoll::Rejected { reason, .. } => return Err(reason),
                AuthenticatedTableValidationPoll::NotCandidate { .. } => {
                    panic!("fixture must be a syntactically valid delimiter candidate")
                }
                AuthenticatedTableValidationPoll::Cancelled { .. } => {
                    panic!("uncancelled validation")
                }
            }
        }
    }

    #[derive(Debug)]
    struct ObservedProjection {
        origin: ProjectionOrigin,
        part: TableProjectionPart,
        parser_bytes: usize,
        physical_bytes: usize,
    }

    #[derive(Debug)]
    struct RecordingWriter {
        expected: TableProjectionBinding,
        begin_calls: usize,
        begun_cells: usize,
        ended_cells: usize,
        staged: Vec<ObservedProjection>,
        visible_commits: usize,
        aborts: usize,
    }

    impl RecordingWriter {
        fn new(expected: TableProjectionBinding) -> Self {
            Self {
                expected,
                begin_calls: 0,
                begun_cells: 0,
                ended_cells: 0,
                staged: Vec::new(),
                visible_commits: 0,
                aborts: 0,
            }
        }
    }

    impl AuthenticatedTableProjectionSink for RecordingWriter {
        fn expected_binding(&self) -> TableProjectionBinding {
            self.expected
        }

        fn begin_table_transaction(&mut self, _columns: u32) {
            self.begin_calls += 1;
        }

        fn begin_cell(&mut self, _column: u32, _alignment: u8) {
            self.begun_cells += 1;
        }

        fn push_projection(&mut self, piece: AuthenticatedProjectionPiece) {
            self.staged.push(ObservedProjection {
                origin: piece.origin(),
                part: piece.part(),
                parser_bytes: piece.parser_bytes(),
                physical_bytes: piece.physical_bytes(),
            });
        }

        fn end_cell(&mut self) {
            self.ended_cells += 1;
        }

        fn commit_table_transaction(&mut self) {
            self.visible_commits += 1;
        }

        fn abort_table_transaction(&mut self) {
            self.aborts += 1;
        }
    }

    #[test]
    fn private_join_replays_every_projection_kind_and_whole_row_with_fuel_one() {
        let epoch = begin_epoch("a | b\n--- | ---\n");
        let actor_binding = binding(epoch, FixtureIdentity::default());
        let header: Arc<[u8]> = Arc::from(&b"a | b\n"[..]);
        let delimiter: Arc<[u8]> = Arc::from(&b"--- | ---\n"[..]);
        let header_segments: Arc<[ProjectionSegment]> = Arc::from([
            ProjectionSegment {
                parser: 0..1,
                physical: 0..1,
                origin: ProjectionOrigin::Identity,
            },
            ProjectionSegment {
                parser: 1..1,
                physical: 1..2,
                origin: ProjectionOrigin::Hidden {
                    affinity: HiddenProjectionAffinity::After,
                },
            },
            ProjectionSegment {
                parser: 1..2,
                physical: 2..3,
                origin: ProjectionOrigin::TabToSpaces { spaces: 1 },
            },
            ProjectionSegment {
                parser: 2..3,
                physical: 3..4,
                origin: ProjectionOrigin::Program {
                    page: ProjectionProgramPageIdentity(41),
                    generation: actor_binding.program_generation,
                },
            },
            ProjectionSegment {
                parser: 3..4,
                physical: 4..5,
                origin: ProjectionOrigin::NulToReplacement,
            },
            ProjectionSegment {
                parser: 4..5,
                physical: 5..6,
                origin: ProjectionOrigin::CanonicalLoneCr,
            },
            ProjectionSegment {
                parser: 5..6,
                physical: 6..8,
                origin: ProjectionOrigin::CanonicalCrLf,
            },
        ]);
        let provider = provider_with_segments(
            actor_binding,
            Arc::clone(&header),
            Arc::clone(&delimiter),
            header_segments,
            identity_segments(&delimiter),
        );
        let ready = drive_ready(provider, 1).unwrap();
        assert_eq!(ready.columns(), 2);
        let mut join =
            AuthenticatedTableProjectionJoin::new(ready, RecordingWriter::new(actor_binding))
                .unwrap();
        let cancellation = CancellationToken::default();
        for _ in 0..10_000 {
            match join.poll(1, &cancellation) {
                TableProjectionJoinPoll::Pending { inspected } => assert!(inspected <= 1),
                TableProjectionJoinPoll::Complete { inspected } => {
                    assert_eq!(inspected, 0);
                    break;
                }
                other => panic!("successful join failed: {other:?}"),
            }
        }
        assert!(join.complete);
        assert_eq!(join.writer.begin_calls, 1);
        assert_eq!(join.writer.begun_cells, 2);
        assert_eq!(join.writer.ended_cells, 2);
        assert_eq!(join.writer.visible_commits, 1);
        assert_eq!(join.writer.aborts, 0);
        assert!(join.writer.staged.iter().any(|piece| {
            matches!(piece.origin, ProjectionOrigin::Program { page, generation }
                if page == ProjectionProgramPageIdentity(41)
                    && generation == actor_binding.program_generation)
        }));
        assert!(
            join.writer
                .staged
                .iter()
                .any(|piece| matches!(piece.origin, ProjectionOrigin::Hidden { .. }))
        );
        assert!(
            join.writer
                .staged
                .iter()
                .any(|piece| piece.part == TableProjectionPart::CellContent)
        );
        assert!(
            join.writer
                .staged
                .iter()
                .any(|piece| piece.part == TableProjectionPart::Syntax)
        );
        assert_eq!(
            join.writer
                .staged
                .iter()
                .map(|piece| piece.parser_bytes)
                .sum::<usize>(),
            header.len() + delimiter.len()
        );
        assert_eq!(
            join.writer
                .staged
                .iter()
                .map(|piece| piece.physical_bytes)
                .sum::<usize>(),
            8 + delimiter.len()
        );
    }

    #[test]
    fn validation_and_join_cancellation_are_zero_read_and_never_publish() {
        let epoch = begin_epoch("a | b\n--- | ---\n");
        let actor_binding = binding(epoch, FixtureIdentity::default());
        let provider = identity_provider(
            actor_binding,
            Arc::from(&b"a | b\n"[..]),
            Arc::from(&b"--- | ---\n"[..]),
        );
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let mut validation = AuthenticatedTableValidationJob::new(provider).unwrap();
        assert!(matches!(
            validation.poll(1, &cancellation),
            AuthenticatedTableValidationPoll::Cancelled { inspected: 0 }
        ));

        let provider = identity_provider(
            actor_binding,
            Arc::from(&b"a | b\n"[..]),
            Arc::from(&b"--- | ---\n"[..]),
        );
        let ready = drive_ready(provider, 1).unwrap();
        let mut join =
            AuthenticatedTableProjectionJoin::new(ready, RecordingWriter::new(actor_binding))
                .unwrap();
        assert!(matches!(
            join.poll(1, &cancellation),
            TableProjectionJoinPoll::Cancelled { inspected: 0 }
        ));
        assert_eq!(join.writer.visible_commits, 0);
        assert!(join.writer.staged.is_empty());
    }

    #[test]
    fn rejected_validation_and_crossed_join_bindings_cannot_mutate_writer() {
        let epoch = begin_epoch("a | b\n---\n");
        let base_identity = FixtureIdentity::default();
        let base_binding = binding(epoch, base_identity);
        let invalid = identity_provider(
            base_binding,
            Arc::from(&b"a | b\n"[..]),
            Arc::from(&b"---\n"[..]),
        );
        let writer = RecordingWriter::new(base_binding);
        assert_eq!(
            drive_ready(invalid, 1).unwrap_err(),
            TableHeaderRejectReason::ColumnCountMismatch
        );
        assert_eq!(writer.begin_calls, 0);
        assert_eq!(writer.visible_commits, 0);

        let header: Arc<[u8]> = Arc::from(&b"a | b\n"[..]);
        let delimiter: Arc<[u8]> = Arc::from(&b"--- | ---\n"[..]);
        for (identity, expected_mismatch) in [
            (
                FixtureIdentity {
                    fragment: 99,
                    ..base_identity
                },
                TableJoinBindingMismatch::Projection,
            ),
            (
                FixtureIdentity {
                    paragraph: 99,
                    ..base_identity
                },
                TableJoinBindingMismatch::Paragraph,
            ),
            (
                FixtureIdentity {
                    paragraph_barrier: 99,
                    ..base_identity
                },
                TableJoinBindingMismatch::ParagraphBarrier,
            ),
            (
                FixtureIdentity {
                    delimiter_owner: 99,
                    ..base_identity
                },
                TableJoinBindingMismatch::DelimiterOwnership,
            ),
            (
                FixtureIdentity {
                    writer_epoch: 99,
                    ..base_identity
                },
                TableJoinBindingMismatch::WriterEpoch,
            ),
        ] {
            let crossed_binding = binding(epoch, identity);
            let ready = drive_ready(
                identity_provider(crossed_binding, Arc::clone(&header), Arc::clone(&delimiter)),
                1,
            )
            .unwrap();
            let error =
                AuthenticatedTableProjectionJoin::new(ready, RecordingWriter::new(base_binding))
                    .unwrap_err();
            assert_eq!(
                error.error,
                TableProjectionJoinError::Binding(expected_mismatch)
            );
            assert_eq!(error.writer.begin_calls, 0);
            assert_eq!(error.writer.visible_commits, 0);
        }

        // A fresh build on the same source root/revision is a crossed
        // candidate epoch, independently of source identity.
        let mut same_source = LiveDocumentStore::new("a | b\n--- | ---\n", 4).unwrap();
        let first_epoch = same_source
            .begin_candidate(same_source.active_parse_plan().unwrap().token)
            .unwrap();
        let abort = same_source.cancel_candidate(first_epoch).unwrap();
        while !same_source.poll_candidate_abort(abort, 1).unwrap().complete {}
        let second_epoch = same_source
            .begin_candidate(same_source.active_parse_plan().unwrap().token)
            .unwrap();
        assert_eq!(first_epoch.source(), second_epoch.source());
        assert_ne!(first_epoch, second_epoch);
        let first_binding = binding(first_epoch, base_identity);
        let second_binding = binding(second_epoch, base_identity);
        let ready = drive_ready(
            identity_provider(second_binding, Arc::clone(&header), Arc::clone(&delimiter)),
            1,
        )
        .unwrap();
        let error =
            AuthenticatedTableProjectionJoin::new(ready, RecordingWriter::new(first_binding))
                .unwrap_err();
        assert_eq!(
            error.error,
            TableProjectionJoinError::Binding(TableJoinBindingMismatch::CandidateEpoch)
        );
        assert_eq!(error.writer.begin_calls, 0);

        // A different same-length source root/revision fails before the more
        // general epoch comparison and before a transaction begins.
        let changed_epoch = begin_epoch("z | b\n---\n");
        assert_eq!(
            changed_epoch.source().bytes,
            base_binding.epoch.source().bytes
        );
        assert_ne!(changed_epoch.source(), base_binding.epoch.source());
        let changed_binding = binding(changed_epoch, base_identity);
        let ready = drive_ready(identity_provider(changed_binding, header, delimiter), 1).unwrap();
        let error =
            AuthenticatedTableProjectionJoin::new(ready, RecordingWriter::new(base_binding))
                .unwrap_err();
        assert_eq!(
            error.error,
            TableProjectionJoinError::Binding(TableJoinBindingMismatch::Source)
        );
        assert_eq!(error.writer.begin_calls, 0);
    }

    #[test]
    fn exact_cell_cap_is_certified_before_any_writer_transaction() {
        fn row(cells: usize, byte: u8) -> Arc<[u8]> {
            let mut bytes = Vec::with_capacity(cells.saturating_mul(2));
            for column in 0..cells {
                if column > 0 {
                    bytes.push(b'|');
                }
                bytes.push(byte);
            }
            Arc::from(bytes)
        }

        let epoch = begin_epoch("");
        let accepted_binding = binding(epoch, FixtureIdentity::default());
        let accepted_header = row(MAX_TABLE_CELLS, b'a');
        let accepted_delimiter = row(MAX_TABLE_CELLS, b'-');
        let accepted = identity_provider(accepted_binding, accepted_header, accepted_delimiter);
        let ready = drive_ready(accepted, 4_096).unwrap();
        assert_eq!(usize::try_from(ready.columns()).unwrap(), MAX_TABLE_CELLS);

        let rejected_identity = FixtureIdentity {
            fragment: 77,
            ..FixtureIdentity::default()
        };
        let rejected_binding = binding(epoch, rejected_identity);
        let rejected_header = row(MAX_TABLE_CELLS + 1, b'a');
        let rejected_delimiter = row(MAX_TABLE_CELLS + 1, b'-');
        let rejected = identity_provider(rejected_binding, rejected_header, rejected_delimiter);
        assert_eq!(
            drive_ready(rejected, 4_096).unwrap_err(),
            TableHeaderRejectReason::TooManyColumns
        );
        let writer = RecordingWriter::new(rejected_binding);
        assert_eq!(writer.begin_calls, 0);
        assert_eq!(writer.visible_commits, 0);
    }
}
