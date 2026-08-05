//! Same-parser continuation over a write-only structural sink.
//!
//! [`ResumableValueBlockParser`] invokes the exact [`crate::ValueBlockParser`]
//! line machine.  At a pause boundary it serializes only the canonical open
//! semantic path. Closed output is folded out of parser scratch and can only
//! be observed by the event consumer; the parser has no sink read API.

use std::collections::{BTreeSet, HashMap, HashSet};

use comrak::block_spine_facade::{MAX_CLASSIFICATION_BYTES, reference_definitions, table_row};
use serde::{Deserialize, Serialize};

use crate::parser::{ParseError, ValueBlockParser};
use crate::source::{
    CoverageLeaf, LeafContent, LogicalProjection, SourceBackedContent, SourceDocument,
};
use crate::tree::{
    BlockDocument, BlockEvent, BlockKind, BlockTree, ChildSequenceFold, ListDelimiter, ListType,
    NodeId, Position, ReferenceOccurrence, SyntaxProfile,
};

const CHECKPOINT_SCHEMA: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Parse(ParseError),
    InvalidCheckpoint(&'static str),
    InvalidScratch(&'static str),
}

impl From<ParseError> for CheckpointError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// Canonical equality state at a physical-line boundary.
///
/// Deliberately absent: `NodeId`, `BlockTree`, absolute/revision-root source
/// positions, output handles, materialization cursors, and reference winners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCheckpoint {
    pub schema: u32,
    pub profile: SyntaxProfile,
    pub at_document_start: bool,
    pub current_frame: usize,
    pub frames: Vec<SemanticFrame>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticFrame {
    pub kind: BlockKind,
    pub last_line_blank: bool,
    pub table_visited: bool,
    pub table_autocompleted_cells: usize,
    pub pending: LeafContent,
    /// Closed output children preceding the retained open-path child.
    pub closed_children: ChildSequenceFold,
}

/// Exact, typed grammar equality state for a later composed suffix adoption.
///
/// The projection is variant-local against the current parser call sites.
/// Output accumulators remain in [`BlockCheckpoint`] and are deliberately not
/// compared here. Paragraph payload is summarized into the exact future block
/// control decisions; no owned paragraph bytes or source identity enter this
/// key. Equality is necessary but not sufficient for semantic suffix reuse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockTransitionCheckpoint {
    pub schema: u32,
    pub profile: SyntaxProfile,
    pub at_document_start: bool,
    pub current_frame: usize,
    pub frames: Vec<TransitionFrame>,
}

/// Product-facing name for the grammar half of a reusable pause.
pub type GrammarContinuation = BlockTransitionCheckpoint;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionFrame {
    pub kind: BlockTransitionKind,
}

/// Existing-list syntax that is consulted when a later marker decides whether
/// to extend the same list or open a sibling list.
///
/// The source `ListData` stores a rectangular bag of fields, but the reachable
/// grammar has two disjoint shapes. Encoding those shapes here prevents an
/// ordered list's permanently-zero bullet character or a bullet list's
/// permanently-`Period` delimiter from becoming accidental convergence state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListMatchKey {
    Bullet { marker: u8 },
    Ordered { delimiter: ListDelimiter },
}

/// Whether a later setext underline would retain visible paragraph content
/// after leading reference definitions are removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphSetextState {
    VisibleContent,
    DefinitionsOnlyOrBlank,
    /// The bounded reference-prefix recognizer cannot yet certify the answer.
    /// This value must never authorize convergence, even against itself.
    UnknownLeadingReferencePrefix,
}

/// Exact table-header fact consulted by a later GFM delimiter row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphTableState {
    /// `table_visited` permanently disables another header attempt.
    NotApplicable,
    Ineligible,
    Eligible {
        columns: u32,
    },
    /// The last physical line exceeded the ordinary generated-scanner grant.
    /// A resumable scanner may replace this with a certification.
    UnknownOversizedLine,
}

/// Scalar result accepted from the exact resumable table-row scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphTableCertification {
    Ineligible,
    Eligible { columns: u32 },
}

/// Exact future block decisions for an open paragraph.
///
/// Reference occurrences, consumed source ranges, paragraph payload, origins,
/// and the preface/header source split live in [`ParagraphOutputAccumulator`].
/// Unknown states are deliberately non-convergent; a hash is never accepted as
/// a substitute for exact recognition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParagraphTransitionState {
    pub table_visited: bool,
    pub setext: ParagraphSetextState,
    pub table_header: ParagraphTableState,
}

/// The exact portion of an open block frame that can affect a later physical
/// line transition or the logical content emitted for that later line.
///
/// Donor output metadata is intentionally absent. In particular, an ordered
/// list's displayed start number and eventual tightness do not affect list
/// matching, and retaining them here would prevent convergence for the entire
/// remainder of a document-spanning list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockTransitionKind {
    Document,
    BlockQuote,
    List {
        match_key: ListMatchKey,
    },
    Item {
        /// The parser only ever observes `marker_offset + padding`.
        effective_content_indent: usize,
        /// Historical child presence is grammar state only for an Item: it
        /// decides whether a blank physical line continues an empty item.
        has_any_child: bool,
    },
    IndentedCode,
    FencedCode {
        fence_char: u8,
        fence_length: usize,
        fence_offset: usize,
    },
    HtmlBlock {
        block_type: u8,
    },
    Paragraph(ParagraphTransitionState),
    Heading,
    ThematicBreak,
    Table {
        columns: usize,
        /// Exact future-observable equivalence class for the hostile-short-row
        /// guard. `MAX + 1` represents every already-over-cap count.
        capped_autocompleted_cells: usize,
    },
    TableRow,
    TableCell,
}

impl BlockTransitionKind {
    fn from_frame(
        frame: &SemanticFrame,
        has_retained_child: bool,
        paragraph: Option<ParagraphTransitionState>,
    ) -> Self {
        match &frame.kind {
            BlockKind::Document => Self::Document,
            BlockKind::BlockQuote => Self::BlockQuote,
            BlockKind::List(list) => Self::List {
                match_key: match list.list_type {
                    ListType::Bullet => ListMatchKey::Bullet {
                        marker: list.bullet_char,
                    },
                    ListType::Ordered => ListMatchKey::Ordered {
                        delimiter: list.delimiter,
                    },
                },
            },
            BlockKind::Item(item) => Self::Item {
                effective_content_indent: item.marker_offset.saturating_add(item.padding),
                has_any_child: frame.closed_children.had_child || has_retained_child,
            },
            BlockKind::CodeBlock {
                fenced,
                fence_char,
                fence_length,
                fence_offset,
                ..
            } => {
                if *fenced {
                    Self::FencedCode {
                        fence_char: *fence_char,
                        fence_length: *fence_length,
                        fence_offset: *fence_offset,
                    }
                } else {
                    Self::IndentedCode
                }
            }
            BlockKind::HtmlBlock { block_type, .. } => Self::HtmlBlock {
                block_type: *block_type,
            },
            BlockKind::Paragraph => {
                Self::Paragraph(paragraph.expect("paragraph frame has a continuation analysis"))
            }
            BlockKind::Heading { .. } => Self::Heading,
            BlockKind::ThematicBreak => Self::ThematicBreak,
            BlockKind::Table(table) => Self::Table {
                columns: table.num_columns,
                capped_autocompleted_cells: crate::table::capped_autocompleted_cells(
                    frame.table_autocompleted_cells,
                ),
            },
            BlockKind::TableRow { .. } => Self::TableRow,
            BlockKind::TableCell => Self::TableCell,
        }
    }
}

/// Proof receipt for projecting complete pause state onto grammar equality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransitionProjectionReceipt {
    /// Maximum bytes synchronously inspected for one paragraph. This is capped
    /// by two ordinary scanner grants plus a small leading-byte probe.
    pub maximum_paragraph_bytes_inspected: usize,
    /// Paragraph payload retained by the grammar key. This must remain zero.
    pub retained_paragraph_payload_bytes: usize,
    pub uncertified_paragraphs: usize,
}

/// Source-segmented output state for one open paragraph.
///
/// The current proof parser still owns [`LeafContent::logical`] in the complete
/// output accumulator. These projections demonstrate the production split:
/// the immutable output root owns payload and provenance, while grammar holds
/// only [`ParagraphTransitionState`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphOutputAccumulator {
    pub logical: LogicalProjection,
    pub preface: Option<LogicalProjection>,
    pub last_line: Option<LogicalProjection>,
    pub reference_prefix: ReferencePrefixOutputState,
}

/// Reference-definition finalization is output work, not block grammar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferencePrefixOutputState {
    Certified {
        consumed_prefix: u32,
        visible_remainder: bool,
    },
    /// The exact logical projection remains source-visible until a generated
    /// refillable scanner can finish it. Grammar convergence is disabled.
    Unknown { logical: LogicalProjection },
}

/// The output/property half of a reusable pause. Construction consumes the
/// complete checkpoint, so paragraph payload and origin vectors are moved, not
/// cloned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputAccumulatorCheckpoint {
    pub schema: u32,
    pub profile: SyntaxProfile,
    pub at_document_start: bool,
    pub current_frame: usize,
    pub frames: Vec<OutputFrameAccumulator>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputFrameAccumulator {
    pub semantic: SemanticFrame,
    pub paragraph: Option<ParagraphOutputAccumulator>,
}

#[derive(Clone, Debug)]
struct ParagraphAnalysis {
    transition: ParagraphTransitionState,
    output: ParagraphOutputAccumulator,
    inspected: usize,
}

fn analyze_paragraph(frame: &SemanticFrame, profile: SyntaxProfile) -> ParagraphAnalysis {
    let logical = &frame.pending.logical;
    let logical_projection = LogicalProjection::new(
        0,
        u32::try_from(logical.len()).expect("logical paragraph below u32"),
    );
    let (setext, reference_prefix, reference_inspected) = analyze_reference_prefix(logical);
    let (last_line, line_inspected) = bounded_last_physical_line(logical);
    let table_visited = profile == SyntaxProfile::Gfm && frame.table_visited;
    let table_header = if profile != SyntaxProfile::Gfm || table_visited {
        ParagraphTableState::NotApplicable
    } else if let Some(projection) = last_line {
        let input = logical
            .get(projection.start as usize..projection.end as usize)
            .expect("bounded last-line projection");
        match table_row(input, false) {
            Ok(Some(row)) => ParagraphTableState::Eligible {
                columns: u32::try_from(row.cells.len()).expect("donor table row below u32 cap"),
            },
            Ok(None) => ParagraphTableState::Ineligible,
            Err(_) => ParagraphTableState::UnknownOversizedLine,
        }
    } else {
        ParagraphTableState::UnknownOversizedLine
    };
    let preface = last_line.and_then(|projection| {
        (projection.start > 0).then(|| LogicalProjection::new(0, projection.start))
    });
    ParagraphAnalysis {
        transition: ParagraphTransitionState {
            table_visited,
            setext,
            table_header,
        },
        output: ParagraphOutputAccumulator {
            logical: logical_projection,
            preface,
            last_line,
            reference_prefix,
        },
        inspected: reference_inspected.saturating_add(line_inspected),
    }
}

fn analyze_reference_prefix(
    logical: &str,
) -> (ParagraphSetextState, ReferencePrefixOutputState, usize) {
    if logical.len() <= MAX_CLASSIFICATION_BYTES {
        if let Ok(definitions) = reference_definitions(logical) {
            let consumed = definitions
                .last()
                .map_or(0, |definition| definition.source.end);
            let visible_remainder = logical[consumed..]
                .bytes()
                .any(|byte| !byte.is_ascii_whitespace());
            return (
                if visible_remainder {
                    ParagraphSetextState::VisibleContent
                } else {
                    ParagraphSetextState::DefinitionsOnlyOrBlank
                },
                ReferencePrefixOutputState::Certified {
                    consumed_prefix: u32::try_from(consumed)
                        .expect("bounded reference prefix below u32"),
                    visible_remainder,
                },
                logical.len(),
            );
        }
    } else if logical.as_bytes().first() != Some(&b'[') {
        let probe = logical
            .bytes()
            .take(MAX_CLASSIFICATION_BYTES)
            .position(|byte| !byte.is_ascii_whitespace());
        if let Some(nonblank) = probe {
            return (
                ParagraphSetextState::VisibleContent,
                ReferencePrefixOutputState::Certified {
                    consumed_prefix: 0,
                    visible_remainder: true,
                },
                nonblank + 1,
            );
        }
    }

    (
        ParagraphSetextState::UnknownLeadingReferencePrefix,
        ReferencePrefixOutputState::Unknown {
            logical: LogicalProjection::new(
                0,
                u32::try_from(logical.len()).expect("logical paragraph below u32"),
            ),
        },
        logical.len().min(MAX_CLASSIFICATION_BYTES),
    )
}

/// Finds the last physical line while inspecting at most one ordinary scanner
/// grant. A longer line is handed to the refillable table-row scanner rather
/// than being synchronously scanned here.
fn bounded_last_physical_line(logical: &str) -> (Option<LogicalProjection>, usize) {
    let bytes = logical.as_bytes();
    let mut content_end = bytes.len();
    if bytes.get(content_end.wrapping_sub(1)) == Some(&b'\n') {
        content_end -= 1;
        if bytes.get(content_end.wrapping_sub(1)) == Some(&b'\r') {
            content_end -= 1;
        }
    } else if bytes.get(content_end.wrapping_sub(1)) == Some(&b'\r') {
        content_end -= 1;
    }

    let lower = content_end.saturating_sub(MAX_CLASSIFICATION_BYTES);
    let mut cursor = content_end;
    while cursor > lower {
        cursor -= 1;
        if matches!(bytes[cursor], b'\r' | b'\n') {
            let start = cursor + 1;
            let inspected = content_end - cursor;
            if bytes.len() - start <= MAX_CLASSIFICATION_BYTES {
                return (
                    Some(LogicalProjection::new(
                        u32::try_from(start).expect("paragraph offset below u32"),
                        u32::try_from(bytes.len()).expect("paragraph length below u32"),
                    )),
                    inspected,
                );
            }
            return (None, inspected);
        }
    }
    let inspected = content_end - lower;
    if lower == 0 && bytes.len() <= MAX_CLASSIFICATION_BYTES {
        (
            Some(LogicalProjection::new(
                0,
                u32::try_from(bytes.len()).expect("paragraph length below u32"),
            )),
            inspected,
        )
    } else {
        (None, inspected)
    }
}

/// Copy accounting for the correctness-only serialized checkpoint lane.
///
/// JSON necessarily copies pending leaf bytes. This is a hidden-state
/// falsifier, not the proposed production pause representation; production can
/// retain the same coverage-relative builder behind an immutable source lease.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckpointCopyReceipt {
    pub serialized_bytes: usize,
    pub pending_logical_bytes: usize,
    pub pending_origin_runs: usize,
}

impl BlockCheckpoint {
    #[must_use]
    pub fn copy_receipt(&self, serialized_bytes: usize) -> CheckpointCopyReceipt {
        let mut receipt = CheckpointCopyReceipt {
            serialized_bytes,
            ..CheckpointCopyReceipt::default()
        };
        for frame in &self.frames {
            receipt.pending_logical_bytes += frame.pending.logical.len();
            receipt.pending_origin_runs += frame.pending.origins.len();
        }
        receipt
    }

    /// Project the checkpoint onto state that is sufficient for future block
    /// transitions while excluding accumulated list-looseness output.
    #[must_use]
    pub fn transition_checkpoint(&self) -> BlockTransitionCheckpoint {
        self.transition_checkpoint_with_receipt().0
    }

    /// Same projection with an explicit bounded-work receipt.
    #[must_use]
    pub fn transition_checkpoint_with_receipt(
        &self,
    ) -> (BlockTransitionCheckpoint, TransitionProjectionReceipt) {
        let (grammar, _, receipt) = self.project_reuse_parts();
        (grammar, receipt)
    }

    /// Move complete pause state into grammar and output/property halves.
    ///
    /// No paragraph payload is cloned. The output half remains sufficient to
    /// rebuild the current proof parser until it consumes persistent source
    /// segments directly.
    #[must_use]
    pub fn into_reuse_parts(
        self,
    ) -> (
        GrammarContinuation,
        OutputAccumulatorCheckpoint,
        TransitionProjectionReceipt,
    ) {
        let (grammar, paragraph_outputs, receipt) = self.project_reuse_parts();
        let output = OutputAccumulatorCheckpoint {
            schema: self.schema,
            profile: self.profile,
            at_document_start: self.at_document_start,
            current_frame: self.current_frame,
            frames: self
                .frames
                .into_iter()
                .zip(paragraph_outputs)
                .map(|(semantic, paragraph)| OutputFrameAccumulator {
                    semantic,
                    paragraph,
                })
                .collect(),
        };
        (grammar, output, receipt)
    }

    fn project_reuse_parts(
        &self,
    ) -> (
        GrammarContinuation,
        Vec<Option<ParagraphOutputAccumulator>>,
        TransitionProjectionReceipt,
    ) {
        let mut receipt = TransitionProjectionReceipt::default();
        let mut frames = Vec::with_capacity(self.frames.len());
        let mut paragraph_outputs = Vec::with_capacity(self.frames.len());
        for (index, frame) in self.frames.iter().enumerate() {
            let paragraph = matches!(frame.kind, BlockKind::Paragraph).then(|| {
                let analysis = analyze_paragraph(frame, self.profile);
                receipt.maximum_paragraph_bytes_inspected = receipt
                    .maximum_paragraph_bytes_inspected
                    .max(analysis.inspected);
                if matches!(
                    analysis.transition.setext,
                    ParagraphSetextState::UnknownLeadingReferencePrefix
                ) || matches!(
                    analysis.transition.table_header,
                    ParagraphTableState::UnknownOversizedLine
                ) {
                    receipt.uncertified_paragraphs += 1;
                }
                analysis
            });
            frames.push(TransitionFrame {
                kind: BlockTransitionKind::from_frame(
                    frame,
                    index + 1 < self.frames.len(),
                    paragraph.as_ref().map(|analysis| analysis.transition),
                ),
            });
            paragraph_outputs.push(paragraph.map(|analysis| analysis.output));
        }
        (
            BlockTransitionCheckpoint {
                schema: self.schema,
                profile: self.profile,
                at_document_start: self.at_document_start,
                current_frame: self.current_frame,
                frames,
            },
            paragraph_outputs,
            receipt,
        )
    }
}

impl BlockTransitionCheckpoint {
    /// Necessary grammar compatibility for a later composed suffix adoption.
    ///
    /// This does not authorize reuse by itself: stable bindings, edit lineage,
    /// and every changed output-prefix adoption must also be certified by the
    /// composer. Unknown grammar state is deliberately not reflexively
    /// compatible.
    #[must_use]
    pub fn is_grammar_compatible_for_suffix_reuse(&self, other: &Self) -> bool {
        self == other && self.is_fully_certified() && other.is_fully_certified()
    }

    #[must_use]
    pub fn is_fully_certified(&self) -> bool {
        self.frames.iter().all(|frame| match frame.kind {
            BlockTransitionKind::Paragraph(paragraph) => {
                !matches!(
                    paragraph.setext,
                    ParagraphSetextState::UnknownLeadingReferencePrefix
                ) && !matches!(
                    paragraph.table_header,
                    ParagraphTableState::UnknownOversizedLine
                )
            }
            _ => true,
        })
    }

    /// Install a scalar result from the exact refillable table-row scanner.
    /// The scanner owns DFA/cursor state; only its semantic result enters
    /// grammar equality.
    pub fn certify_paragraph_table_header(
        &mut self,
        frame_index: usize,
        certification: ParagraphTableCertification,
    ) -> Result<(), CheckpointError> {
        let frame = self
            .frames
            .get_mut(frame_index)
            .ok_or(CheckpointError::InvalidCheckpoint(
                "paragraph certification frame is absent",
            ))?;
        let BlockTransitionKind::Paragraph(paragraph) = &mut frame.kind else {
            return Err(CheckpointError::InvalidCheckpoint(
                "table certification targets a non-paragraph frame",
            ));
        };
        if !matches!(
            paragraph.table_header,
            ParagraphTableState::UnknownOversizedLine
        ) {
            return Err(CheckpointError::InvalidCheckpoint(
                "table certification replaces only an unknown state",
            ));
        }
        paragraph.table_header = match certification {
            ParagraphTableCertification::Ineligible => ParagraphTableState::Ineligible,
            ParagraphTableCertification::Eligible { columns } => {
                ParagraphTableState::Eligible { columns }
            }
        };
        Ok(())
    }
}

/// Rebuild a complete proof-parser pause from independently selected grammar
/// and output/property roots.
///
/// A changed output root is authoritative for list start/tightness, paragraph
/// payload, origins, and child folds. The grammar root can validate it but can
/// never restore those facts from an older prefix.
pub fn reconstruct_checkpoint(
    grammar: &GrammarContinuation,
    output: OutputAccumulatorCheckpoint,
) -> Result<BlockCheckpoint, CheckpointError> {
    for frame in &output.frames {
        let expected = if matches!(frame.semantic.kind, BlockKind::Paragraph) {
            Some(analyze_paragraph(&frame.semantic, output.profile).output)
        } else {
            None
        };
        if frame.paragraph != expected {
            return Err(CheckpointError::InvalidCheckpoint(
                "paragraph output cursor does not match its accumulator",
            ));
        }
    }
    let checkpoint = BlockCheckpoint {
        schema: output.schema,
        profile: output.profile,
        at_document_start: output.at_document_start,
        current_frame: output.current_frame,
        frames: output
            .frames
            .into_iter()
            .map(|frame| frame.semantic)
            .collect(),
    };
    if checkpoint.transition_checkpoint() != *grammar {
        return Err(CheckpointError::InvalidCheckpoint(
            "grammar continuation does not match output accumulator",
        ));
    }
    Ok(checkpoint)
}

/// Runtime-only authorization to update already-written open output nodes.
/// It is intentionally neither serializable nor part of checkpoint equality.
#[derive(Clone, Debug)]
pub struct OpenOutputBindings {
    frames: Vec<FrameBinding>,
}

#[derive(Clone, Debug)]
struct FrameBinding {
    handle: u64,
    last_descendant_handle: u64,
    source_start: Position,
    source_end: Position,
}

impl OpenOutputBindings {
    #[must_use]
    pub fn receipt(&self) -> RuntimeBindingReceipt {
        RuntimeBindingReceipt {
            frame_count: self.frames.len(),
            handles: self.frames.iter().map(|frame| frame.handle).collect(),
            contains_revision_positions: !self.frames.is_empty(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeBindingReceipt {
    pub frame_count: usize,
    pub handles: Vec<u64>,
    pub contains_revision_positions: bool,
}

/// Revision-local scheduling/materialization state, outside semantic equality.
#[derive(Clone, Debug)]
pub struct MaterializationCursor {
    pub line_number: usize,
    pub last_line_length: usize,
    pub next_handle: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveContinuationReceipt {
    pub transient_nodes_before_compaction: usize,
    pub retained_open_frames: usize,
    pub repair_position_entries: usize,
    /// Persisted JSON pauses copy this state, but the per-line live rebuild
    /// must report zero here because it consumes/moves open frames.
    pub pending_logical_bytes_copied: usize,
    /// Logical content copied into append/replacement or one-time open events.
    pub materialized_logical_bytes_copied: usize,
    /// Bounded source-reference metadata copied into append events.
    pub materialized_origin_runs_copied: usize,
    pub materialized_line_offsets_copied: usize,
    pub max_delta_logical_bytes_copied: usize,
    pub max_delta_origin_runs_copied: usize,
    pub max_delta_line_offsets_copied: usize,
    pub materialized_kind_bytes_copied: usize,
    pub source_leaf_bytes_copied: usize,
    pub structural_events_emitted: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MaterializationReceipt {
    transient_nodes: usize,
    repair_position_entries: usize,
    materialized_logical_bytes_copied: usize,
    materialized_origin_runs_copied: usize,
    materialized_line_offsets_copied: usize,
    max_delta_logical_bytes_copied: usize,
    max_delta_origin_runs_copied: usize,
    max_delta_line_offsets_copied: usize,
    materialized_kind_bytes_copied: usize,
}

struct LiveFrame {
    kind: BlockKind,
    last_line_blank: bool,
    table_visited: bool,
    table_autocompleted_cells: usize,
    pending: LeafContent,
    closed_children: ChildSequenceFold,
    source_start: Position,
    source_end: Position,
}

#[derive(Clone, Debug)]
pub struct PhysicalLine<'a> {
    pub coverage_leaf_id: u64,
    pub absolute_start: usize,
    pub text: &'a str,
}

/// An output consumer with intentionally no read/query operation.
pub trait WriteOnlyBlockSink {
    fn emit(&mut self, event: StructuralEvent);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedNodeState {
    pub kind: BlockKind,
    pub last_line_blank: bool,
    pub table_visited: bool,
    pub source_start: Position,
    pub source_end: Position,
    /// Present only for one-time direct-content opens/replacements. Ordinary
    /// open leaves are maintained through [`StructuralEvent::AppendContent`].
    pub content: Option<LeafContent>,
}

impl MaterializedNodeState {
    fn from_tree(tree: &BlockTree, node: NodeId, include_content: bool) -> Self {
        let node = tree.node(node);
        Self {
            kind: node.kind.clone(),
            last_line_blank: node.last_line_blank,
            table_visited: node.table_visited,
            source_start: node.source_start,
            source_end: node.source_end,
            content: include_content.then(|| node.content.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafContentDelta {
    pub logical_start: usize,
    pub logical: String,
    pub origins: Vec<crate::source::OriginRun>,
    pub line_offsets: Vec<usize>,
    pub source_backed: Option<SourceBackedContent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralEvent {
    SourceLeaf(CoverageLeaf),
    Open {
        handle: u64,
        parent: u64,
        state: MaterializedNodeState,
    },
    Update {
        handle: u64,
        state: MaterializedNodeState,
        preserve_source_positions: bool,
    },
    Close {
        handle: u64,
    },
    Detach {
        handle: u64,
    },
    RepairListSourcePositions {
        handle: u64,
        last_descendant_handle: u64,
    },
    UpdateSourcePositions {
        handle: u64,
        source_start: Position,
        source_end: Position,
    },
    AppendContent {
        handle: u64,
        delta: LeafContentDelta,
    },
    DrainContentPrefix {
        handle: u64,
        bytes: usize,
    },
    Reference(ReferenceOccurrence),
}

struct MeteredSink<'a, Sink> {
    inner: &'a mut Sink,
    events: usize,
}

impl<Sink: WriteOnlyBlockSink> WriteOnlyBlockSink for MeteredSink<'_, Sink> {
    fn emit(&mut self, event: StructuralEvent) {
        self.events += 1;
        self.inner.emit(event);
    }
}

/// The exact parser wrapped in pause/resume and write-only materialization.
pub struct ResumableValueBlockParser {
    parser: ValueBlockParser,
    handles: HashMap<NodeId, u64>,
    subtree_ends: HashMap<NodeId, u64>,
    cursor: MaterializationCursor,
}

impl ResumableValueBlockParser {
    #[must_use]
    pub fn begin(profile: SyntaxProfile) -> Self {
        let mut parser = ValueBlockParser::new("", profile);
        parser.defer_output_repairs = true;
        let mut handles = HashMap::new();
        handles.insert(parser.tree.root, 0);
        let mut subtree_ends = HashMap::new();
        subtree_ends.insert(parser.tree.root, 0);
        Self {
            parser,
            handles,
            subtree_ends,
            cursor: MaterializationCursor {
                line_number: 0,
                last_line_length: 0,
                next_handle: 1,
            },
        }
    }

    pub fn resume(
        checkpoint: BlockCheckpoint,
        bindings: OpenOutputBindings,
        cursor: MaterializationCursor,
    ) -> Result<Self, CheckpointError> {
        validate_checkpoint(&checkpoint, &bindings, &cursor)?;
        let mut parser = ValueBlockParser::new("", checkpoint.profile);
        parser.defer_output_repairs = true;
        let root = parser.tree.root;
        let mut path = vec![root];

        apply_frame(
            &mut parser.tree,
            root,
            &checkpoint.frames[0],
            &bindings.frames[0],
        );
        for index in 1..checkpoint.frames.len() {
            let parent = *path.last().expect("root frame exists");
            let binding = &bindings.frames[index];
            let node = parser.tree.append(
                parent,
                checkpoint.frames[index].kind.clone(),
                binding.source_start,
            );
            apply_frame(&mut parser.tree, node, &checkpoint.frames[index], binding);
            path.push(node);
        }
        parser.tree.events.clear();
        parser.current = path[checkpoint.current_frame];
        parser.line_number = cursor.line_number;
        parser.last_line_length = cursor.last_line_length;

        let handles = path
            .iter()
            .copied()
            .zip(bindings.frames.iter().map(|frame| frame.handle))
            .collect();
        let subtree_ends = path
            .iter()
            .copied()
            .zip(
                bindings
                    .frames
                    .iter()
                    .map(|frame| frame.last_descendant_handle),
            )
            .collect();
        Ok(Self {
            parser,
            handles,
            subtree_ends,
            cursor,
        })
    }

    pub fn push_line(
        &mut self,
        physical: PhysicalLine<'_>,
        sink: &mut impl WriteOnlyBlockSink,
    ) -> Result<LiveContinuationReceipt, CheckpointError> {
        let source_leaf_bytes_copied = physical.text.len();
        let mut metered = MeteredSink {
            inner: sink,
            events: 0,
        };
        metered.emit(StructuralEvent::SourceLeaf(CoverageLeaf {
            id: physical.coverage_leaf_id,
            absolute_start: physical.absolute_start,
            text: physical.text.to_owned(),
        }));
        self.parser.line_leaf_id = physical.coverage_leaf_id;
        self.parser.process_line(physical.text)?;
        self.cursor.line_number = self.parser.line_number;
        self.cursor.last_line_length = self.parser.last_line_length;
        self.flush_references(&mut metered);
        let path = open_path(&self.parser.tree);
        let materialization = self.materialize(&path, false, &mut metered)?;
        let retained_open_frames = path.len();
        self.compact_live(&path)?;
        let receipt = LiveContinuationReceipt {
            transient_nodes_before_compaction: materialization.transient_nodes,
            retained_open_frames,
            repair_position_entries: materialization.repair_position_entries,
            pending_logical_bytes_copied: 0,
            materialized_logical_bytes_copied: materialization.materialized_logical_bytes_copied,
            materialized_origin_runs_copied: materialization.materialized_origin_runs_copied,
            materialized_line_offsets_copied: materialization.materialized_line_offsets_copied,
            max_delta_logical_bytes_copied: materialization.max_delta_logical_bytes_copied,
            max_delta_origin_runs_copied: materialization.max_delta_origin_runs_copied,
            max_delta_line_offsets_copied: materialization.max_delta_line_offsets_copied,
            materialized_kind_bytes_copied: materialization.materialized_kind_bytes_copied,
            source_leaf_bytes_copied,
            structural_events_emitted: metered.events,
        };
        Ok(receipt)
    }

    fn compact_live(&mut self, path: &[NodeId]) -> Result<(), CheckpointError> {
        let current_frame = path
            .iter()
            .position(|node| *node == self.parser.current)
            .ok_or(CheckpointError::InvalidScratch(
                "current parser node is outside canonical open path",
            ))?;
        for (index, node) in path.iter().copied().enumerate() {
            self.parser
                .tree
                .fold_children_before(node, path.get(index + 1).copied());
        }
        let mut closed_children = Vec::with_capacity(path.len());
        for (index, node) in path.iter().copied().enumerate() {
            let next = path.get(index + 1).copied();
            let scratch = self.parser.tree.node(node);
            let fold = scratch.historical_children;
            if let Some(next) = next
                && scratch.children.last().copied() != Some(next)
            {
                return Err(CheckpointError::InvalidScratch(
                    "retained child is not final scratch child",
                ));
            }
            if scratch.folded_children + usize::from(next.is_some()) != scratch.children.len() {
                return Err(CheckpointError::InvalidScratch(
                    "closed child is missing from the frame fold",
                ));
            }
            closed_children.push(fold);
        }

        let runtime = path
            .iter()
            .copied()
            .map(|node| {
                let scratch = self.parser.tree.node(node);
                Ok((
                    *self
                        .handles
                        .get(&node)
                        .ok_or(CheckpointError::InvalidScratch("open frame has no handle"))?,
                    *self
                        .subtree_ends
                        .get(&node)
                        .ok_or(CheckpointError::InvalidScratch(
                            "open frame has no subtree end",
                        ))?,
                    scratch.source_start,
                    scratch.source_end,
                ))
            })
            .collect::<Result<Vec<_>, CheckpointError>>()?;

        let mut frames = Vec::with_capacity(path.len());
        for (index, node) in path.iter().copied().enumerate() {
            let scratch = self.parser.tree.node_mut(node);
            frames.push(LiveFrame {
                kind: std::mem::replace(&mut scratch.kind, BlockKind::Document),
                last_line_blank: scratch.last_line_blank,
                table_visited: scratch.table_visited,
                table_autocompleted_cells: scratch.table_autocompleted_cells,
                pending: std::mem::take(&mut scratch.content),
                closed_children: closed_children[index],
                source_start: runtime[index].2,
                source_end: runtime[index].3,
            });
        }

        let profile = self.parser.profile;
        let line_number = self.parser.line_number;
        let last_line_length = self.parser.last_line_length;
        let mut parser = ValueBlockParser::new("", profile);
        parser.defer_output_repairs = true;
        let root = parser.tree.root;
        let mut rebuilt_path = vec![root];
        let mut frames = frames.into_iter();
        apply_live_frame(
            &mut parser.tree,
            root,
            frames.next().expect("document frame"),
        );
        for frame in frames {
            let parent = *rebuilt_path.last().expect("document frame");
            let LiveFrame {
                kind,
                last_line_blank,
                table_visited,
                table_autocompleted_cells,
                pending,
                closed_children,
                source_start,
                source_end,
            } = frame;
            let node = parser.tree.append(parent, kind, source_start);
            let scratch = parser.tree.node_mut(node);
            scratch.last_line_blank = last_line_blank;
            scratch.table_visited = table_visited;
            scratch.table_autocompleted_cells = table_autocompleted_cells;
            scratch.content = pending;
            scratch.historical_children = closed_children;
            scratch.source_end = source_end;
            rebuilt_path.push(node);
        }
        parser.tree.events.clear();
        parser.current = rebuilt_path[current_frame];
        parser.line_number = line_number;
        parser.last_line_length = last_line_length;
        self.handles = rebuilt_path
            .iter()
            .copied()
            .zip(runtime.iter().map(|binding| binding.0))
            .collect();
        self.subtree_ends = rebuilt_path
            .iter()
            .copied()
            .zip(runtime.iter().map(|binding| binding.1))
            .collect();
        self.parser = parser;
        Ok(())
    }

    pub fn pause(
        mut self,
        sink: &mut impl WriteOnlyBlockSink,
    ) -> Result<(BlockCheckpoint, OpenOutputBindings, MaterializationCursor), CheckpointError> {
        self.flush_references(sink);
        let (checkpoint, bindings) = self.continuation_state()?;
        Ok((checkpoint, bindings, self.cursor))
    }

    fn continuation_state(&self) -> Result<(BlockCheckpoint, OpenOutputBindings), CheckpointError> {
        let path = open_path(&self.parser.tree);
        let current_frame = path
            .iter()
            .position(|node| *node == self.parser.current)
            .ok_or(CheckpointError::InvalidScratch(
                "current parser node is outside canonical open path",
            ))?;

        let mut frames = Vec::with_capacity(path.len());
        let mut runtime_frames = Vec::with_capacity(path.len());
        for (index, node) in path.iter().copied().enumerate() {
            let next = path.get(index + 1).copied();
            let scratch = self.parser.tree.node(node);
            let closed_children = scratch.historical_children;
            if let Some(next) = next {
                if scratch.children.last().copied() != Some(next) {
                    return Err(CheckpointError::InvalidScratch(
                        "retained child is not final scratch child",
                    ));
                }
            }
            if scratch.folded_children + usize::from(next.is_some()) != scratch.children.len() {
                return Err(CheckpointError::InvalidScratch(
                    "closed child is missing from the checkpoint fold",
                ));
            }
            frames.push(SemanticFrame {
                kind: scratch.kind.clone(),
                last_line_blank: scratch.last_line_blank,
                table_visited: scratch.table_visited,
                table_autocompleted_cells: scratch.table_autocompleted_cells,
                pending: scratch.content.clone(),
                closed_children,
            });
            runtime_frames.push(FrameBinding {
                handle: *self
                    .handles
                    .get(&node)
                    .ok_or(CheckpointError::InvalidScratch("open frame has no handle"))?,
                last_descendant_handle: *self.subtree_ends.get(&node).ok_or(
                    CheckpointError::InvalidScratch("open frame has no subtree end"),
                )?,
                source_start: scratch.source_start,
                source_end: scratch.source_end,
            });
        }
        Ok((
            BlockCheckpoint {
                schema: CHECKPOINT_SCHEMA,
                profile: self.parser.profile,
                at_document_start: self.parser.line_number == 0,
                current_frame,
                frames,
            },
            OpenOutputBindings {
                frames: runtime_frames,
            },
        ))
    }

    pub fn finish(
        mut self,
        sink: &mut impl WriteOnlyBlockSink,
    ) -> Result<LiveContinuationReceipt, CheckpointError> {
        let mut metered = MeteredSink {
            inner: sink,
            events: 0,
        };
        self.parser.finalize_document()?;
        self.flush_references(&mut metered);
        let materialization = self.materialize(&[], true, &mut metered)?;
        Ok(LiveContinuationReceipt {
            transient_nodes_before_compaction: materialization.transient_nodes,
            retained_open_frames: 0,
            repair_position_entries: materialization.repair_position_entries,
            pending_logical_bytes_copied: 0,
            materialized_logical_bytes_copied: materialization.materialized_logical_bytes_copied,
            materialized_origin_runs_copied: materialization.materialized_origin_runs_copied,
            materialized_line_offsets_copied: materialization.materialized_line_offsets_copied,
            max_delta_logical_bytes_copied: materialization.max_delta_logical_bytes_copied,
            max_delta_origin_runs_copied: materialization.max_delta_origin_runs_copied,
            max_delta_line_offsets_copied: materialization.max_delta_line_offsets_copied,
            materialized_kind_bytes_copied: materialization.materialized_kind_bytes_copied,
            source_leaf_bytes_copied: 0,
            structural_events_emitted: metered.events,
        })
    }

    fn flush_references(&mut self, sink: &mut impl WriteOnlyBlockSink) {
        for reference in self.parser.references.drain(..) {
            sink.emit(StructuralEvent::Reference(reference));
        }
    }

    fn materialize(
        &mut self,
        retained_path: &[NodeId],
        finish: bool,
        sink: &mut impl WriteOnlyBlockSink,
    ) -> Result<MaterializationReceipt, CheckpointError> {
        let receipt = MaterializationReceipt {
            transient_nodes: self.parser.tree.nodes.len(),
            repair_position_entries: self
                .parser
                .tree
                .events
                .iter()
                .map(|event| match event {
                    BlockEvent::RepairListSourcePositions {
                        scratch_positions, ..
                    } => scratch_positions.len(),
                    _ => 0,
                })
                .sum(),
            materialized_logical_bytes_copied: 0,
            materialized_origin_runs_copied: 0,
            materialized_line_offsets_copied: 0,
            max_delta_logical_bytes_copied: 0,
            max_delta_origin_runs_copied: 0,
            max_delta_line_offsets_copied: 0,
            materialized_kind_bytes_copied: 0,
        };
        let reachable = reachable_preorder(&self.parser.tree);
        let reachable_set = reachable.iter().copied().collect::<HashSet<_>>();
        let retained = retained_path.iter().copied().collect::<HashSet<_>>();
        let existing = self.handles.keys().copied().collect::<HashSet<_>>();
        let mut receipt = receipt;
        let append_nodes = self
            .parser
            .tree
            .events
            .iter()
            .filter_map(|event| match event {
                BlockEvent::AppendContent { node, .. } => Some(*node),
                _ => None,
            })
            .collect::<HashSet<_>>();

        for node in reachable.iter().copied() {
            if !self.handles.contains_key(&node) {
                let handle = self.cursor.next_handle;
                self.cursor.next_handle += 1;
                self.handles.insert(node, handle);
            }
        }

        // Fold output preorder intervals bottom-up once. This survives live
        // scratch compaction and gives a list-repair event its historical
        // last descendant without querying or walking materialized output.
        let mut current_subtree_ends = HashMap::<NodeId, u64>::new();
        for node in reachable.iter().rev().copied() {
            let mut last = self
                .subtree_ends
                .get(&node)
                .copied()
                .unwrap_or(self.handles[&node]);
            for child in &self.parser.tree.node(node).children {
                last = last.max(current_subtree_ends[child]);
            }
            current_subtree_ends.insert(node, last);
        }

        for node in existing.iter().copied() {
            if node != self.parser.tree.root && !reachable_set.contains(&node) {
                let handle = self.handles[&node];
                sink.emit(StructuralEvent::Detach { handle });
            }
        }

        let root = self.parser.tree.root;
        for child in self.parser.tree.node(root).children.iter().copied() {
            self.open_new_subtree(child, &existing, &append_nodes, &mut receipt, sink)?;
        }

        // Replay each output-history fold against the exact scratch Positions
        // visible at its original finalization point. A final state equal to
        // this snapshot preserves the repair; a later mutation overwrites it.
        let mut repair_snapshots = HashMap::<NodeId, (Position, Position)>::new();
        for event in &self.parser.tree.events {
            if let BlockEvent::RepairListSourcePositions {
                node,
                scratch_positions,
            } = event
            {
                for (scratch, source_start, source_end) in scratch_positions {
                    let handle =
                        *self
                            .handles
                            .get(scratch)
                            .ok_or(CheckpointError::InvalidScratch(
                                "list repair snapshot node has no output handle",
                            ))?;
                    sink.emit(StructuralEvent::UpdateSourcePositions {
                        handle,
                        source_start: *source_start,
                        source_end: *source_end,
                    });
                    repair_snapshots.insert(*scratch, (*source_start, *source_end));
                }
                let handle = *self
                    .handles
                    .get(node)
                    .ok_or(CheckpointError::InvalidScratch(
                        "list repair target has no output handle",
                    ))?;
                let last_descendant_handle = current_subtree_ends[node];
                sink.emit(StructuralEvent::RepairListSourcePositions {
                    handle,
                    last_descendant_handle,
                });
            }
        }

        for event in &self.parser.tree.events {
            match event {
                BlockEvent::AppendContent {
                    node,
                    logical_start,
                    logical_end,
                    origin_start,
                    origin_end,
                    line_offsets_start,
                    line_offsets_end,
                    source_backed,
                } => {
                    let content = &self.parser.tree.node(*node).content;
                    let logical_start = *logical_start as usize;
                    let logical_end = *logical_end as usize;
                    let logical = if source_backed.is_some() {
                        String::new()
                    } else {
                        content.logical[logical_start..logical_end].to_owned()
                    };
                    let delta = LeafContentDelta {
                        logical_start,
                        logical,
                        origins: content.origins[*origin_start as usize..*origin_end as usize]
                            .to_vec(),
                        line_offsets: content.line_offsets
                            [*line_offsets_start as usize..*line_offsets_end as usize]
                            .to_vec(),
                        source_backed: *source_backed,
                    };
                    receipt.materialized_logical_bytes_copied += delta.logical.len();
                    receipt.materialized_origin_runs_copied += delta.origins.len();
                    receipt.materialized_line_offsets_copied += delta.line_offsets.len();
                    receipt.max_delta_logical_bytes_copied = receipt
                        .max_delta_logical_bytes_copied
                        .max(delta.logical.len());
                    receipt.max_delta_origin_runs_copied = receipt
                        .max_delta_origin_runs_copied
                        .max(delta.origins.len());
                    receipt.max_delta_line_offsets_copied = receipt
                        .max_delta_line_offsets_copied
                        .max(delta.line_offsets.len());
                    sink.emit(StructuralEvent::AppendContent {
                        handle: self.handles[node],
                        delta,
                    });
                }
                BlockEvent::DrainContentPrefix { node, bytes } => {
                    sink.emit(StructuralEvent::DrainContentPrefix {
                        handle: self.handles[node],
                        bytes: *bytes as usize,
                    });
                }
                _ => {}
            }
        }

        let root_state = MaterializedNodeState::from_tree(&self.parser.tree, root, false);
        receipt.materialized_kind_bytes_copied += kind_owned_bytes(&root_state.kind);
        let root_preserve = repair_snapshots.get(&root).is_some_and(|positions| {
            *positions == (root_state.source_start, root_state.source_end)
        });
        sink.emit(StructuralEvent::Update {
            handle: 0,
            state: root_state.clone(),
            preserve_source_positions: root_preserve,
        });
        for child in self.parser.tree.node(root).children.iter().copied() {
            self.materialize_subtree(child, &retained, &repair_snapshots, &mut receipt, sink)?;
        }
        if finish {
            sink.emit(StructuralEvent::Close { handle: 0 });
        }
        self.subtree_ends = retained_path
            .iter()
            .copied()
            .map(|node| (node, current_subtree_ends[&node]))
            .collect();
        self.parser.tree.events.clear();
        Ok(receipt)
    }

    fn open_new_subtree(
        &self,
        node: NodeId,
        existing: &HashSet<NodeId>,
        append_nodes: &HashSet<NodeId>,
        receipt: &mut MaterializationReceipt,
        sink: &mut impl WriteOnlyBlockSink,
    ) -> Result<(), CheckpointError> {
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            if !existing.contains(&current) {
                let parent =
                    self.parser
                        .tree
                        .parent(current)
                        .ok_or(CheckpointError::InvalidScratch(
                            "attached node has no parent",
                        ))?;
                let state = MaterializedNodeState::from_tree(
                    &self.parser.tree,
                    current,
                    !append_nodes.contains(&current),
                );
                receipt.materialized_kind_bytes_copied += kind_owned_bytes(&state.kind);
                sink.emit(StructuralEvent::Open {
                    handle: self.handles[&current],
                    parent: self.handles[&parent],
                    state,
                });
                if !append_nodes.contains(&current) {
                    receipt.materialized_logical_bytes_copied +=
                        self.parser.tree.node(current).content.logical.len();
                }
            }
            stack.extend(
                self.parser
                    .tree
                    .node(current)
                    .children
                    .iter()
                    .rev()
                    .copied(),
            );
        }
        Ok(())
    }

    fn materialize_subtree(
        &self,
        node: NodeId,
        retained: &HashSet<NodeId>,
        repair_snapshots: &HashMap<NodeId, (Position, Position)>,
        receipt: &mut MaterializationReceipt,
        sink: &mut impl WriteOnlyBlockSink,
    ) -> Result<(), CheckpointError> {
        let mut stack = vec![(node, false)];
        while let Some((current, visited)) = stack.pop() {
            let handle = self.handles[&current];
            if visited {
                if !retained.contains(&current) {
                    sink.emit(StructuralEvent::Close { handle });
                }
                continue;
            }
            let state = MaterializedNodeState::from_tree(&self.parser.tree, current, false);
            receipt.materialized_kind_bytes_copied += kind_owned_bytes(&state.kind);
            let preserve_source_positions = repair_snapshots
                .get(&current)
                .is_some_and(|positions| *positions == (state.source_start, state.source_end));
            sink.emit(StructuralEvent::Update {
                handle,
                state,
                preserve_source_positions,
            });
            stack.push((current, true));
            stack.extend(
                self.parser
                    .tree
                    .node(current)
                    .children
                    .iter()
                    .rev()
                    .copied()
                    .map(|child| (child, false)),
            );
        }
        Ok(())
    }
}

fn validate_checkpoint(
    checkpoint: &BlockCheckpoint,
    bindings: &OpenOutputBindings,
    cursor: &MaterializationCursor,
) -> Result<(), CheckpointError> {
    if checkpoint.schema != CHECKPOINT_SCHEMA
        || checkpoint.frames.is_empty()
        || checkpoint.frames.len() != bindings.frames.len()
        || checkpoint.current_frame >= checkpoint.frames.len()
        || !matches!(checkpoint.frames[0].kind, BlockKind::Document)
        || bindings.frames[0].handle != 0
        || checkpoint.at_document_start != (cursor.line_number == 0)
    {
        return Err(CheckpointError::InvalidCheckpoint("checkpoint shape"));
    }
    Ok(())
}

fn apply_frame(tree: &mut BlockTree, node: NodeId, frame: &SemanticFrame, binding: &FrameBinding) {
    let scratch = tree.node_mut(node);
    scratch.kind = frame.kind.clone();
    scratch.last_line_blank = frame.last_line_blank;
    scratch.table_visited = frame.table_visited;
    scratch.table_autocompleted_cells = frame.table_autocompleted_cells;
    scratch.content = frame.pending.clone();
    scratch.historical_children = frame.closed_children;
    scratch.source_start = binding.source_start;
    scratch.source_end = binding.source_end;
    scratch.open = true;
}

fn apply_live_frame(tree: &mut BlockTree, node: NodeId, frame: LiveFrame) {
    let scratch = tree.node_mut(node);
    scratch.kind = frame.kind;
    scratch.last_line_blank = frame.last_line_blank;
    scratch.table_visited = frame.table_visited;
    scratch.table_autocompleted_cells = frame.table_autocompleted_cells;
    scratch.content = frame.pending;
    scratch.historical_children = frame.closed_children;
    scratch.source_start = frame.source_start;
    scratch.source_end = frame.source_end;
    scratch.open = true;
}

fn open_path(tree: &BlockTree) -> Vec<NodeId> {
    let mut path = vec![tree.root];
    let mut current = tree.root;
    while let Some(child) = tree.last_child(current) {
        if !tree.node(child).open {
            break;
        }
        path.push(child);
        current = child;
    }
    path
}

fn reachable_preorder(tree: &BlockTree) -> Vec<NodeId> {
    let mut result = Vec::new();
    let mut stack = vec![tree.root];
    while let Some(node) = stack.pop() {
        result.push(node);
        stack.extend(tree.node(node).children.iter().rev().copied());
    }
    result
}

fn kind_owned_bytes(kind: &BlockKind) -> usize {
    match kind {
        // Code/HTML kinds retain constant-size logical projections only.
        BlockKind::CodeBlock { .. } | BlockKind::HtmlBlock { .. } => 0,
        BlockKind::Table(table) => {
            table.alignments.len() * std::mem::size_of::<crate::tree::Alignment>()
        }
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug)]
struct PositionWrite {
    seq: u64,
    value: Position,
}

#[derive(Clone, Copy, Debug)]
struct ChildWrite {
    seq: u64,
    value: Option<NodeId>,
}

/// Append-friendly range maximum used by the output-side position overlay.
///
/// The vector growth policy is a prototype convenience. A production output
/// page tree supplies the same point-update/range-query contract without a
/// resizing rebuild.
#[derive(Debug)]
struct PositionRangeMax {
    len: usize,
    capacity: usize,
    tree: Vec<Position>,
    steps: usize,
    resize_nodes_rebuilt: usize,
}

impl PositionRangeMax {
    fn new(first: Position) -> Self {
        Self {
            len: 1,
            capacity: 1,
            tree: vec![Position::default(), first],
            steps: 0,
            resize_nodes_rebuilt: 0,
        }
    }

    fn append(&mut self, value: Position) {
        if self.len == self.capacity {
            self.grow();
        }
        let index = self.len;
        self.len += 1;
        self.update(index, value);
    }

    fn update(&mut self, index: usize, value: Position) {
        assert!(index < self.len);
        let mut cursor = self.capacity + index;
        self.tree[cursor] = value;
        self.steps += 1;
        while cursor > 1 {
            cursor /= 2;
            self.tree[cursor] = self.tree[cursor * 2].max(self.tree[cursor * 2 + 1]);
            self.steps += 1;
        }
    }

    fn maximum(&mut self, start: usize, end: usize) -> Option<Position> {
        if start >= end || start >= self.len {
            return None;
        }
        let mut left = self.capacity + start;
        let mut right = self.capacity + end.min(self.len);
        let mut result = Position::default();
        while left < right {
            self.steps += 1;
            if left % 2 == 1 {
                result = result.max(self.tree[left]);
                left += 1;
            }
            if right % 2 == 1 {
                right -= 1;
                result = result.max(self.tree[right]);
            }
            left /= 2;
            right /= 2;
        }
        (result.column != 0).then_some(result)
    }

    fn grow(&mut self) {
        let old_capacity = self.capacity;
        let old_tree = std::mem::take(&mut self.tree);
        self.capacity *= 2;
        self.tree = vec![Position::default(); self.capacity * 2];
        for index in 0..self.len {
            self.tree[self.capacity + index] = old_tree[old_capacity + index];
            self.resize_nodes_rebuilt += 1;
        }
        for cursor in (1..self.capacity).rev() {
            self.tree[cursor] = self.tree[cursor * 2].max(self.tree[cursor * 2 + 1]);
            self.resize_nodes_rebuilt += 1;
        }
    }
}

#[derive(Debug)]
struct OverlayNode {
    parent: Option<NodeId>,
    nearest_list_ancestor: Option<NodeId>,
    active_children: BTreeSet<NodeId>,
    current_last_child: ChildWrite,
    prior_last_children_at_repairs: Vec<ChildWrite>,
    opened_seq: u64,
    detached_seq: Option<u64>,
    current_start: PositionWrite,
    current_end: PositionWrite,
    /// Raw writes are discarded on ordinary updates. A previous write is
    /// retained only when the update crosses a repair that may query the old
    /// value later. Thus this grows with relevant repair boundaries, not with
    /// physical lines or keystrokes.
    prior_starts_at_repairs: Vec<PositionWrite>,
    prior_ends_at_repairs: Vec<PositionWrite>,
    repair_seqs: Vec<u64>,
    is_list: bool,
    list_subtree_end: Option<usize>,
}

#[derive(Debug)]
struct PositionOverlay {
    seq: u64,
    nodes: Vec<OverlayNode>,
    position_index: PositionRangeMax,
    repair_scope_records: usize,
    repair_descendant_touches: usize,
    repair_open_depth_steps: usize,
    max_repair_open_depth_steps: usize,
    candidate_updates: usize,
    detach_nodes_touched: usize,
    position_resolution_steps: usize,
    max_position_resolution_steps: usize,
    final_list_aggregate_reads: usize,
    sparse_position_snapshots: usize,
    sparse_child_snapshots: usize,
    position_page_queries: usize,
    max_position_page_nodes: usize,
}

impl PositionOverlay {
    fn new(root: NodeId, start: Position, end: Position) -> Self {
        assert_eq!(root.index(), 0);
        Self {
            seq: 1,
            nodes: vec![OverlayNode {
                parent: None,
                nearest_list_ancestor: None,
                active_children: BTreeSet::new(),
                current_last_child: ChildWrite {
                    seq: 1,
                    value: None,
                },
                prior_last_children_at_repairs: Vec::new(),
                opened_seq: 1,
                detached_seq: None,
                current_start: PositionWrite {
                    seq: 1,
                    value: start,
                },
                current_end: PositionWrite { seq: 1, value: end },
                prior_starts_at_repairs: Vec::new(),
                prior_ends_at_repairs: Vec::new(),
                repair_seqs: Vec::new(),
                is_list: false,
                list_subtree_end: None,
            }],
            position_index: PositionRangeMax::new(start.max(end)),
            repair_scope_records: 0,
            repair_descendant_touches: 0,
            repair_open_depth_steps: 0,
            max_repair_open_depth_steps: 0,
            candidate_updates: 0,
            detach_nodes_touched: 0,
            position_resolution_steps: 0,
            max_position_resolution_steps: 0,
            final_list_aggregate_reads: 0,
            sparse_position_snapshots: 0,
            sparse_child_snapshots: 0,
            position_page_queries: 0,
            max_position_page_nodes: 0,
        }
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    fn open(
        &mut self,
        node: NodeId,
        parent: NodeId,
        start: Position,
        end: Position,
        is_list: bool,
    ) {
        let seq = self.next_seq();
        assert_eq!(node.index(), self.nodes.len());
        let nearest_list_ancestor = if self.nodes[parent.index()].is_list {
            Some(parent)
        } else {
            self.nodes[parent.index()].nearest_list_ancestor
        };
        self.preserve_last_child_across_repair(parent);
        self.nodes[parent.index()].active_children.insert(node);
        self.nodes[parent.index()].current_last_child = ChildWrite {
            seq,
            value: Some(node),
        };
        self.nodes.push(OverlayNode {
            parent: Some(parent),
            nearest_list_ancestor,
            active_children: BTreeSet::new(),
            current_last_child: ChildWrite { seq, value: None },
            prior_last_children_at_repairs: Vec::new(),
            opened_seq: seq,
            detached_seq: None,
            current_start: PositionWrite { seq, value: start },
            current_end: PositionWrite { seq, value: end },
            prior_starts_at_repairs: Vec::new(),
            prior_ends_at_repairs: Vec::new(),
            repair_seqs: Vec::new(),
            is_list,
            list_subtree_end: None,
        });
        self.position_index
            .append(Self::position_candidate(start, end));
        self.candidate_updates += 1;
    }

    fn write_positions(&mut self, node: NodeId, start: Position, end: Position) {
        // Ordinary successive writes supersede each other. Preserve the old
        // raw value only if a lazy repair lies between it and this write; that
        // old value is then observable when resolving the repair snapshot.
        let mut ignored_steps = 0;
        let current_write_seq = self.nodes[node.index()].current_end.seq;
        if self
            .latest_affecting_repair(node, self.seq, &mut ignored_steps)
            .is_some_and(|(repair_seq, _)| repair_seq > current_write_seq)
        {
            let value = &mut self.nodes[node.index()];
            value.prior_starts_at_repairs.push(value.current_start);
            value.prior_ends_at_repairs.push(value.current_end);
            self.sparse_position_snapshots += 1;
        }

        let seq = self.next_seq();
        self.nodes[node.index()].current_start = PositionWrite { seq, value: start };
        self.nodes[node.index()].current_end = PositionWrite { seq, value: end };
        if self.active_now(node) {
            self.position_index
                .update(node.index(), Self::position_candidate(start, end));
            self.candidate_updates += 1;
        }
    }

    fn detach(&mut self, node: NodeId) {
        let seq = self.next_seq();
        if !self.active_now(node) {
            return;
        }
        assert!(
            self.nodes[node.index()].active_children.is_empty(),
            "current grammar may detach only a leaf; subtree detach needs a page aggregate"
        );
        self.detach_nodes_touched += 1;
        self.position_index
            .update(node.index(), Position::default());
        self.candidate_updates += 1;
        let parent = self.nodes[node.index()]
            .parent
            .expect("detached output node has a parent");
        self.preserve_last_child_across_repair(parent);
        assert!(self.nodes[parent.index()].active_children.remove(&node));
        self.nodes[parent.index()].current_last_child = ChildWrite {
            seq,
            value: self.nodes[parent.index()].active_children.last().copied(),
        };
        self.nodes[node.index()].detached_seq = Some(seq);
    }

    fn repair_list(&mut self, list: NodeId, subtree_end: usize, open_depth_steps: usize) {
        let seq = self.next_seq();
        self.nodes[list.index()].repair_seqs.push(seq);
        self.nodes[list.index()].list_subtree_end = Some(subtree_end);
        self.repair_scope_records += 1;
        self.repair_open_depth_steps += open_depth_steps;
        self.max_repair_open_depth_steps = self.max_repair_open_depth_steps.max(open_depth_steps);
        // The overlay records one scope; it never enumerates descendants.
    }

    fn materialize(&mut self, tree: &mut BlockTree) {
        for index in 0..self.nodes.len() {
            let node = NodeId(index as u32);
            let (start, end) = self.resolve_position(node);
            tree.node_mut(node).source_start = start;
            tree.node_mut(node).source_end = end;
        }
    }

    fn resolve_page(
        &mut self,
        start_index: usize,
        length: usize,
    ) -> Vec<(NodeId, Position, Position)> {
        let end_index = start_index.saturating_add(length).min(self.nodes.len());
        self.position_page_queries += 1;
        self.max_position_page_nodes = self
            .max_position_page_nodes
            .max(end_index.saturating_sub(start_index));
        (start_index..end_index)
            .map(|index| {
                let node = NodeId(u32::try_from(index).expect("node index below u32"));
                let (start, end) = self.resolve_position(node);
                (node, start, end)
            })
            .collect()
    }

    fn resolve_position(&mut self, node: NodeId) -> (Position, Position) {
        let at = self.seq;
        let mut steps = 0;
        let start = self.start_at(node, at, &mut steps);
        let mut end = self.resolve_end(node, at, &mut steps);
        self.position_resolution_steps += steps;
        self.max_position_resolution_steps = self.max_position_resolution_steps.max(steps);
        if self.nodes[node.index()].is_list {
            self.final_list_aggregate_reads += 1;
            let range_end = self.nodes[node.index()]
                .list_subtree_end
                .unwrap_or(node.index() + 1);
            let maximum = self.position_index.maximum(node.index() + 1, range_end);
            if let Some(maximum) = maximum
                && maximum.column != 0
                && maximum > end
            {
                end = maximum;
            }
        }
        (start, end)
    }

    fn resolve_end(&self, node: NodeId, at: u64, steps: &mut usize) -> Position {
        *steps += 1;
        let raw = self.end_at(node, at);
        let Some((repair_seq, scope)) = self.latest_affecting_repair(node, at, steps) else {
            return raw.value;
        };
        if raw.seq >= repair_seq {
            return raw.value;
        }

        if node == scope {
            if let Some(child) = self.last_child_at(node, repair_seq, steps) {
                let candidate = self.resolve_end(child, repair_seq, steps);
                if candidate.column != 0 {
                    return candidate;
                }
            }
            return self.resolve_end(node, repair_seq.saturating_sub(1), steps);
        }

        let before = self.resolve_end(node, repair_seq.saturating_sub(1), steps);
        if before.column != 0 {
            return before;
        }
        let mut deepest = node;
        while let Some(child) = self.last_child_at(deepest, repair_seq, steps) {
            deepest = child;
        }
        if deepest != node {
            let candidate = self.resolve_end(deepest, repair_seq, steps);
            if candidate.column != 0 {
                return candidate;
            }
        }
        self.start_at(node, repair_seq, steps)
    }

    fn latest_affecting_repair(
        &self,
        node: NodeId,
        at: u64,
        steps: &mut usize,
    ) -> Option<(u64, NodeId)> {
        let mut best = None;
        let mut scope = if self.nodes[node.index()].is_list {
            Some(node)
        } else {
            self.nodes[node.index()].nearest_list_ancestor
        };
        while let Some(candidate) = scope {
            *steps += 1;
            for repair in self.nodes[candidate.index()].repair_seqs.iter().rev() {
                if *repair > at {
                    continue;
                }
                // `candidate` comes from the cached list-ancestor chain.
                // Subtree detaches are forbidden above, so lifecycle
                // membership is a constant-time interval test.
                if self.active_at(node, *repair)
                    && best.is_none_or(|(best_seq, _)| *repair > best_seq)
                {
                    best = Some((*repair, candidate));
                }
                break;
            }
            scope = self.nodes[candidate.index()].nearest_list_ancestor;
        }
        best
    }

    fn last_child_at(&self, parent: NodeId, at: u64, steps: &mut usize) -> Option<NodeId> {
        *steps += 1;
        let value = &self.nodes[parent.index()];
        if value.current_last_child.seq <= at {
            return value.current_last_child.value;
        }
        let index = value
            .prior_last_children_at_repairs
            .partition_point(|write| write.seq <= at);
        value
            .prior_last_children_at_repairs
            .get(index.saturating_sub(1))
            .expect("repair-crossing child mutation retained prior last child")
            .value
    }

    fn preserve_last_child_across_repair(&mut self, parent: NodeId) {
        let mut ignored_steps = 0;
        let current = self.nodes[parent.index()].current_last_child;
        if self
            .latest_affecting_repair(parent, self.seq, &mut ignored_steps)
            .is_some_and(|(repair_seq, _)| repair_seq > current.seq)
        {
            self.nodes[parent.index()]
                .prior_last_children_at_repairs
                .push(current);
            self.sparse_child_snapshots += 1;
        }
    }

    fn start_at(&self, node: NodeId, at: u64, steps: &mut usize) -> Position {
        *steps += 1;
        let value = &self.nodes[node.index()];
        self.write_at(value.current_start, &value.prior_starts_at_repairs, at)
            .value
    }

    fn end_at(&self, node: NodeId, at: u64) -> PositionWrite {
        let value = &self.nodes[node.index()];
        self.write_at(value.current_end, &value.prior_ends_at_repairs, at)
    }

    fn write_at(
        &self,
        current: PositionWrite,
        prior_at_repairs: &[PositionWrite],
        at: u64,
    ) -> PositionWrite {
        if current.seq <= at {
            return current;
        }
        let index = prior_at_repairs.partition_point(|write| write.seq <= at);
        *prior_at_repairs
            .get(index.saturating_sub(1))
            .expect("repair-crossing update retained prior raw position")
    }

    fn active_at(&self, node: NodeId, at: u64) -> bool {
        let value = &self.nodes[node.index()];
        value.opened_seq <= at && value.detached_seq.is_none_or(|detached| at < detached)
    }

    fn active_now(&self, node: NodeId) -> bool {
        self.active_at(node, self.seq)
    }

    fn position_candidate(start: Position, end: Position) -> Position {
        if end.column == 0 {
            start
        } else {
            start.max(end)
        }
    }
}

/// Tree materialization exists only as an event consumer for differential
/// tests. The parser cannot access it through [`WriteOnlyBlockSink`].
pub struct TreeMaterializer {
    profile: SyntaxProfile,
    source: Vec<CoverageLeaf>,
    tree: BlockTree,
    handles: HashMap<u64, NodeId>,
    references: Vec<ReferenceOccurrence>,
    repair_events: usize,
    repair_nodes_scanned: usize,
    max_repair_nodes_scanned: usize,
    position_overlay: Option<PositionOverlay>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterializerReceipt {
    pub repair_events: usize,
    pub repair_nodes_scanned: usize,
    pub max_repair_nodes_scanned: usize,
    pub final_list_nodes_scanned: usize,
    pub lazy_repair_scope_records: usize,
    pub lazy_repair_descendant_touches: usize,
    pub lazy_repair_open_depth_steps: usize,
    pub lazy_max_repair_open_depth_steps: usize,
    pub lazy_candidate_updates: usize,
    pub lazy_position_index_steps: usize,
    pub lazy_position_index_resize_nodes_rebuilt: usize,
    pub lazy_detach_nodes_touched: usize,
    pub lazy_position_resolution_steps: usize,
    pub lazy_max_position_resolution_steps: usize,
    pub lazy_final_list_aggregate_reads: usize,
    pub lazy_sparse_position_snapshots: usize,
    pub lazy_sparse_child_snapshots: usize,
    pub lazy_position_page_queries: usize,
    pub lazy_max_position_page_nodes: usize,
}

impl TreeMaterializer {
    #[must_use]
    pub fn new(profile: SyntaxProfile) -> Self {
        let tree = BlockTree::new();
        let mut handles = HashMap::new();
        handles.insert(0, tree.root);
        Self {
            profile,
            source: Vec::new(),
            tree,
            handles,
            references: Vec::new(),
            repair_events: 0,
            repair_nodes_scanned: 0,
            max_repair_nodes_scanned: 0,
            position_overlay: None,
        }
    }

    /// Build output through the production-shaped source-position overlay.
    ///
    /// The default constructor deliberately retains the eager tree rewrite as
    /// a differential oracle. This lane records list repairs as lazy scopes
    /// and maintains list maxima incrementally instead of scanning descendants.
    #[must_use]
    pub fn new_aggregate(profile: SyntaxProfile) -> Self {
        let mut result = Self::new(profile);
        let root = result.tree.root;
        result.position_overlay = Some(PositionOverlay::new(
            root,
            result.tree.node(root).source_start,
            result.tree.node(root).source_end,
        ));
        result
    }

    #[must_use]
    pub fn receipt(&self) -> MaterializerReceipt {
        let overlay = self.position_overlay.as_ref();
        MaterializerReceipt {
            repair_events: self.repair_events,
            repair_nodes_scanned: self.repair_nodes_scanned,
            max_repair_nodes_scanned: self.max_repair_nodes_scanned,
            final_list_nodes_scanned: 0,
            lazy_repair_scope_records: overlay.map_or(0, |value| value.repair_scope_records),
            lazy_repair_descendant_touches: overlay
                .map_or(0, |value| value.repair_descendant_touches),
            lazy_repair_open_depth_steps: overlay.map_or(0, |value| value.repair_open_depth_steps),
            lazy_max_repair_open_depth_steps: overlay
                .map_or(0, |value| value.max_repair_open_depth_steps),
            lazy_candidate_updates: overlay.map_or(0, |value| value.candidate_updates),
            lazy_position_index_steps: overlay.map_or(0, |value| value.position_index.steps),
            lazy_position_index_resize_nodes_rebuilt: overlay
                .map_or(0, |value| value.position_index.resize_nodes_rebuilt),
            lazy_detach_nodes_touched: overlay.map_or(0, |value| value.detach_nodes_touched),
            lazy_position_resolution_steps: overlay
                .map_or(0, |value| value.position_resolution_steps),
            lazy_max_position_resolution_steps: overlay
                .map_or(0, |value| value.max_position_resolution_steps),
            lazy_final_list_aggregate_reads: overlay
                .map_or(0, |value| value.final_list_aggregate_reads),
            lazy_sparse_position_snapshots: overlay
                .map_or(0, |value| value.sparse_position_snapshots),
            lazy_sparse_child_snapshots: overlay.map_or(0, |value| value.sparse_child_snapshots),
            lazy_position_page_queries: overlay.map_or(0, |value| value.position_page_queries),
            lazy_max_position_page_nodes: overlay.map_or(0, |value| value.max_position_page_nodes),
        }
    }

    /// Resolve one bounded output page without walking the repaired list's
    /// descendant set. Production pages would call the same point resolver
    /// from their persistent index; this vector materializer exposes it only
    /// as an executable complexity receipt.
    #[must_use]
    pub fn resolve_position_page(
        &mut self,
        start_index: usize,
        length: usize,
    ) -> Vec<(NodeId, Position, Position)> {
        self.position_overlay
            .as_mut()
            .expect("position pages require aggregate materializer")
            .resolve_page(start_index, length)
    }

    #[must_use]
    pub fn output_node_count(&self) -> usize {
        self.tree.nodes.len()
    }

    #[must_use]
    pub fn into_document(self) -> BlockDocument {
        self.into_document_with_receipt().0
    }

    #[must_use]
    pub fn into_document_with_receipt(mut self) -> (BlockDocument, MaterializerReceipt) {
        let final_list_nodes_scanned = if let Some(overlay) = &mut self.position_overlay {
            overlay.materialize(&mut self.tree);
            0
        } else {
            repair_output_source_positions(&mut self.tree)
        };
        let receipt = MaterializerReceipt {
            final_list_nodes_scanned,
            ..self.receipt()
        };
        let document = BlockDocument {
            profile: self.profile,
            source: SourceDocument::from_leaves(self.source),
            tree: self.tree,
            references: self.references,
        };
        (document, receipt)
    }

    fn apply_state(
        &mut self,
        handle: u64,
        state: MaterializedNodeState,
        preserve_source_positions: bool,
    ) {
        let node = self.handles[&handle];
        let target = self.tree.node_mut(node);
        target.kind = state.kind;
        target.last_line_blank = state.last_line_blank;
        target.table_visited = state.table_visited;
        if !preserve_source_positions {
            target.source_start = state.source_start;
            target.source_end = state.source_end;
        }
        if let Some(content) = state.content {
            target.content = content;
        }
    }
}

impl WriteOnlyBlockSink for TreeMaterializer {
    fn emit(&mut self, event: StructuralEvent) {
        match event {
            StructuralEvent::SourceLeaf(leaf) => self.source.push(leaf),
            StructuralEvent::Open {
                handle,
                parent,
                state,
            } => {
                let parent_node = self.handles[&parent];
                let node = self
                    .tree
                    .append(parent_node, state.kind.clone(), state.source_start);
                assert!(self.handles.insert(handle, node).is_none());
                if let Some(overlay) = &mut self.position_overlay {
                    overlay.open(
                        node,
                        parent_node,
                        state.source_start,
                        state.source_end,
                        matches!(state.kind, BlockKind::List(_)),
                    );
                }
                self.apply_state(handle, state, false);
            }
            StructuralEvent::Update {
                handle,
                state,
                preserve_source_positions,
            } => {
                if !preserve_source_positions && let Some(overlay) = &mut self.position_overlay {
                    overlay.write_positions(
                        self.handles[&handle],
                        state.source_start,
                        state.source_end,
                    );
                }
                self.apply_state(handle, state, preserve_source_positions);
            }
            StructuralEvent::Close { handle } => {
                let node = self.handles[&handle];
                if self.tree.node(node).open {
                    self.tree.close(node);
                }
            }
            StructuralEvent::Detach { handle } => {
                let node = self.handles[&handle];
                if let Some(overlay) = &mut self.position_overlay {
                    overlay.detach(node);
                }
                self.tree.detach(node);
            }
            StructuralEvent::RepairListSourcePositions {
                handle,
                last_descendant_handle,
            } => {
                self.repair_events += 1;
                if let Some(overlay) = &mut self.position_overlay {
                    let list = self.handles[&handle];
                    let last_descendant = self.handles[&last_descendant_handle];
                    overlay.repair_list(list, last_descendant.index() + 1, 0);
                } else {
                    let scanned =
                        repair_list_source_positions(&mut self.tree, self.handles[&handle]);
                    self.repair_nodes_scanned += scanned;
                    self.max_repair_nodes_scanned = self.max_repair_nodes_scanned.max(scanned);
                }
            }
            StructuralEvent::UpdateSourcePositions {
                handle,
                source_start,
                source_end,
            } => {
                let node = self.handles[&handle];
                if let Some(overlay) = &mut self.position_overlay {
                    overlay.write_positions(node, source_start, source_end);
                }
                self.tree.node_mut(node).source_start = source_start;
                self.tree.node_mut(node).source_end = source_end;
            }
            StructuralEvent::AppendContent { handle, delta } => {
                let node = self.handles[&handle];
                let content = &mut self.tree.node_mut(node).content;
                assert_eq!(
                    content.logical_len(),
                    delta.logical_start,
                    "append delta mismatch for output handle {handle}: existing={:?} delta={:?}",
                    content.logical,
                    delta.logical
                );
                if let Some(source_backed) = delta.source_backed {
                    assert!(
                        delta.logical.is_empty(),
                        "source-backed append copied logical payload"
                    );
                    content.source_backed = Some(source_backed);
                } else {
                    assert!(
                        content.source_backed.is_none(),
                        "owned append followed source-backed content"
                    );
                    content.logical.push_str(&delta.logical);
                }
                content.origins.extend(delta.origins);
                content.line_offsets.extend(delta.line_offsets);
            }
            StructuralEvent::DrainContentPrefix { handle, bytes } => {
                let node = self.handles[&handle];
                self.tree.node_mut(node).content.drain_prefix(bytes);
            }
            StructuralEvent::Reference(reference) => self.references.push(reference),
        }
    }
}

fn repair_list_source_positions(tree: &mut BlockTree, list: NodeId) -> usize {
    let mut scanned = 1;
    let mut stack = Vec::new();
    for child in tree.node(list).children.clone() {
        stack.push((child, false));
        while let Some((node, visited)) = stack.pop() {
            if !visited {
                stack.push((node, true));
                for descendant in tree.node(node).children.clone() {
                    stack.push((descendant, false));
                }
                continue;
            }
            scanned += 1;
            if tree.node(node).source_end.column == 0 {
                let mut last = tree.last_child(node);
                while let Some(next) = last.and_then(|candidate| tree.last_child(candidate)) {
                    last = Some(next);
                }
                if let Some(last) = last {
                    let position = tree.node(last).source_end;
                    if position.column != 0 {
                        tree.node_mut(node).source_end = position;
                        continue;
                    }
                }
                let start = tree.node(node).source_start;
                tree.node_mut(node).source_end = start;
            }
        }
    }
    if let Some(end) = tree
        .last_child(list)
        .map(|last| tree.node(last).source_end)
        .filter(|end| end.column != 0)
    {
        tree.node_mut(list).source_end = end;
    }
    scanned
}

fn repair_output_source_positions(tree: &mut BlockTree) -> usize {
    let preorder = reachable_preorder(tree);
    let mut scanned = 0;
    // Closed-node end positions arrive in their final event state. Only the
    // document-final list aggregate needs the complete historical output;
    // applying a blanket zero-column repair here would destroy meaningful EOF
    // sentinels such as an item ending at `[line, 0]`.
    for node in preorder.iter().rev().copied() {
        if matches!(tree.node(node).kind, BlockKind::List(_)) {
            let mut max_end = tree.node(node).source_end;
            let mut descendants = tree.node(node).children.clone();
            while let Some(descendant) = descendants.pop() {
                scanned += 1;
                let candidate = tree.node(descendant).source_end;
                if candidate.column != 0 && candidate > max_end {
                    max_end = candidate;
                }
                descendants.extend(tree.node(descendant).children.iter().copied());
            }
            if max_end.column != 0 {
                tree.node_mut(node).source_end = max_end;
            }
        }
    }
    scanned
}
