use std::cmp::min;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use comrak::block_spine_facade::{
    FacadeError, FacadeSetextChar, atx_heading_start, chop_trailing_hashes, close_code_fence,
    html_block_end, html_block_start, open_code_fence, setext_heading_line, task_list_marker,
};
use generated_scanner_gate::{
    AtxLineCuts, CURSOR_ATX_MAX_LOOKAHEAD_SLACK, CursorScanError,
    FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL, FusedAtxDonorMatch, FusedAtxLineScanError,
    FusedAtxLineScanResult, FusedAtxLineScanner, FusedAtxLineSource,
};

use crate::reference_prefix::{
    DirectReferenceLogicalPosition, DirectReferencePrefixDisposition,
    DirectReferencePrefixTerminalAck, DirectReferencePrefixWork,
};
use crate::source::{LogicalProjection, OriginTransform};
use crate::table;
use crate::tree::{
    BlockEvent, BlockKind, BlockTree, ListData, ListDelimiter, ListType, NodeId, Position,
    ReferenceOccurrence, SyntaxProfile,
};

const TAB_STOP: usize = 4;
const CODE_INDENT: usize = 4;
const MAX_LIST_DEPTH: usize = 100;
const DIRECT_MAX_LINE_BYTES: usize = 8 * 1024;
/// Maximum physical prefix retained while the exact block controller consumes
/// a source-backed line. Source access is independently capped to the same
/// amount per poll; neither bound grows with the physical line.
pub const DIRECT_SEGMENTED_LINE_WINDOW_BYTES: usize = 4 * 1024;
const DIRECT_MAX_QUEUED_COMMANDS: usize = 1;
// Preserve the old proof allowance at the largest currently admitted direct
// line, but never derive scratch capacity or the admission limit from the
// physical line presented to `begin_recipe`. A future refillable input may be
// much larger while still producing only a handful of intents. Adversarial
// syntax that produces more line-local intents than this fixed proof envelope
// fails closed instead of turning physical length into an allocation request.
const DIRECT_LINE_LOCAL_INTENT_LIMIT: usize = DIRECT_MAX_LINE_BYTES * 3 + 64;
const DIRECT_OPEN_FRAME_INTENT_ALLOWANCE: usize = 2;
const DIRECT_INITIAL_PREVIOUS_INTENTS: usize = 2;
const DIRECT_INITIAL_BODY_INTENTS: usize = 8;
const DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA: u32 = 2;
const DIRECT_DURABLE_LINE_BOUNDARY_MAGIC: &[u8; 8] = b"FLRKDLBP";
const DIRECT_DURABLE_GRAMMAR_MAGIC: &[u8; 8] = b"FLRKDGCP";
const DIRECT_DURABLE_GRAMMAR_SCHEMA: u32 = 1;
const DIRECT_DURABLE_CHECKSUM_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const DIRECT_DURABLE_CHECKSUM_PRIME: u64 = 0x0000_0100_0000_01b3;
static DIRECT_PARSER_INSTANCE_IDS: AtomicU64 = AtomicU64::new(1);

/// Maximum physical source bytes retained by one source-backed line scanner.
///
/// The ATX stage retains less; its no-match continuation grows only to one
/// fixed controller window. The bound is independent of physical line size.
pub const DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES: usize = DIRECT_SEGMENTED_LINE_WINDOW_BYTES;

/// Fixed generated-DFA work beyond caller fuel (`YYMAXFILL - 1`).
pub const DIRECT_SOURCE_LINE_MAX_LEXICAL_SLACK: usize = CURSOR_ATX_MAX_LOOKAHEAD_SLACK;

/// Fixed bytes retained by one durable parser sample, excluding the root of
/// the consumer-owned persistent open-path sequence.
#[doc(hidden)]
pub const DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES: usize = 64;

/// Fixed bytes in one opaque donor frame suitable for persistent path sharing.
#[doc(hidden)]
pub const DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES: usize = 48;

/// Fixed bytes retained by one durable grammar-and-line-local sample,
/// excluding the consumer-owned persistent open-path root.
#[doc(hidden)]
pub const DIRECT_DURABLE_GRAMMAR_HEADER_BYTES: usize = 64;

/// Fixed bytes in one durable grammar-and-line-local open-path record.
#[doc(hidden)]
pub const DIRECT_DURABLE_GRAMMAR_FRAME_BYTES: usize = 48;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Facade(FacadeError),
    InvalidUtf8Boundary,
    Invariant(&'static str),
    DirectUnsupported(DirectUnsupported),
    /// Non-fatal parser control rendezvous.  The direct driver must poll the
    /// parser-minted external work and commit its terminal result before the
    /// interrupted grammar transition may resume.
    DirectExternalWork(DirectReferencePrefixRequest),
}

/// Deliberately explicit exits from the first direct parser slice.
///
/// Returning one of these errors poisons the speculative candidate. The
/// direct path never falls through to a lossy approximation when the donor
/// grammar asks for retroactive or aggregate-content behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectUnsupported {
    SyntaxProfile,
    LineTooLarge,
    EmbeddedLineEnding,
    TabOrNul,
    BlockKind,
    AggregateContent,
    /// The exact donor needed source beyond the certified controller window,
    /// or produced a block shape outside the clean M1.1 root-Paragraph slice.
    /// The speculative candidate must become `Unknown`; no alternate grammar
    /// classifier may reinterpret the line.
    SegmentedLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectReferencePrefixContext {
    /// The interrupted transition is closing the Paragraph. A
    /// `ReferenceOnly` terminal must remove the provisional wrapper.
    ParagraphFinalization,
    /// The parser has already recognized a Setext underline candidate. A
    /// `ReferenceOnly` terminal must retain the empty Paragraph shell so the
    /// underline is consumed as literal Paragraph content.
    SetextCandidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectReferencePrefixRequest {
    rendezvous_id: u64,
    logical_base: DirectReferenceLogicalPosition,
    include_pending_terminator: bool,
    context: DirectReferencePrefixContext,
}

impl DirectReferencePrefixRequest {
    #[must_use]
    pub const fn rendezvous_id(self) -> u64 {
        self.rendezvous_id
    }

    #[must_use]
    pub const fn logical_base(self) -> DirectReferenceLogicalPosition {
        self.logical_base
    }

    /// The writer's staged final line ending participates as one provisional
    /// canonical newline in the logical scan without being committed first.
    #[must_use]
    pub const fn include_pending_terminator(self) -> bool {
        self.include_pending_terminator
    }

    /// Parser-selected chronology for the terminal Paragraph mutation. The
    /// actor may execute this policy but may not infer or replace it.
    #[must_use]
    pub const fn context(self) -> DirectReferencePrefixContext {
        self.context
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectExternalWorkKind {
    ReferencePrefixFinalizer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectExternalWork {
    ReferencePrefixFinalizer {
        request: DirectReferencePrefixRequest,
    },
}

impl DirectExternalWork {
    #[must_use]
    pub const fn kind(&self) -> DirectExternalWorkKind {
        match self {
            Self::ReferencePrefixFinalizer { .. } => {
                DirectExternalWorkKind::ReferencePrefixFinalizer
            }
        }
    }

    #[must_use]
    pub const fn request(&self) -> DirectReferencePrefixRequest {
        match self {
            Self::ReferencePrefixFinalizer { request } => *request,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectReferencePrefixCommitStatus {
    ParagraphUnchangedArmed,
    VisibleRemainderArmed,
    ReferenceOnlyArmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectReferenceFinalizeResume {
    Continue,
    ReferenceOnly,
}

/// Explicit ownership transfer produced when the reference rendezvous proves
/// that the currently matched Paragraph contains definitions only. The
/// Paragraph is detached immediately, but the suspended `OpenNew` coroutine
/// still names it as its matched text owner until it consumes this receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectReferenceCurrentRebase {
    discarded: NodeId,
    current: NodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectBlockKind {
    Document,
    BlockQuote,
    List(DirectListFacts),
    Item(DirectItemFacts),
    Paragraph,
    Heading(DirectHeadingFacts),
    IndentedCode,
    FencedCode(DirectFencedCodeFacts),
    HtmlBlock(DirectHtmlBlockFacts),
    /// A parser-certified instantaneous leaf. It is emitted and closed within
    /// one line recipe, so it can never appear in a restart continuation.
    ThematicBreak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectHtmlBlockFacts {
    pub block_type: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectHeadingFacts {
    pub level: u8,
    pub setext: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectFenceCharacter {
    Backtick,
    Tilde,
}

impl DirectFenceCharacter {
    #[must_use]
    pub const fn marker(self) -> u8 {
        match self {
            Self::Backtick => b'`',
            Self::Tilde => b'~',
        }
    }
}

/// Definitive grammar facts for one fenced-code opener.
///
/// The closing run length is deliberately not display-sized. A valid long
/// run must reach the writer without truncation even though the current proof
/// driver still has a separate temporary whole-line ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectFencedCodeFacts {
    pub fence: DirectFenceCharacter,
    pub minimum_closing_length: u64,
    pub fence_offset_columns: u8,
}

/// Semantic facts known only when a fenced code block closes.
///
/// Exact info/literal byte and UTF-16 slices are intentionally writer-derived.
/// [`DirectCommand::MarkFencedCodeBoundary`] identifies the two exact grammar
/// cuts while the writer snapshots its own relative logical metric fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectFencedCodeCloseFacts {
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectFencedCodeBoundary {
    InfoEnd,
    LiteralStart,
}

/// Exhaustive semantic result for the active Paragraph transaction supported
/// by this direct slice. The command is transient writer authority: canonical
/// green storage contains only the resulting structure, never this history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectParagraphOutcome {
    SetextHeading { level: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectListFacts {
    pub list_type: ListType,
    pub start: u32,
    pub delimiter: ListDelimiter,
    pub bullet_char: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectItemFacts {
    pub marker_offset: u16,
    pub padding: u16,
    pub task_checked: Option<bool>,
}

/// Ephemeral selector into the consumer's currently open stack.
///
/// This is deliberately not a parser node or output handle. It is valid only
/// for the command being acknowledged and cannot be retained as identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectOwner {
    generations_from_top: u32,
}

impl DirectOwner {
    pub const TOP: Self = Self {
        generations_from_top: 0,
    };
    pub const PARENT_OF_TOP: Self = Self {
        generations_from_top: 1,
    };

    #[must_use]
    pub const fn ancestor(generations_from_top: u32) -> Self {
        Self {
            generations_from_top,
        }
    }

    #[must_use]
    pub const fn generations_from_top(self) -> u32 {
        self.generations_from_top
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectCoveragePart {
    Content,
    ContainerMarker,
    BlockMarker,
    Gap,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectLogicalAction {
    Identity,
    /// Preserve ordinary source text while applying the source-normalization
    /// rules whose physical atom kinds are certified by the consumer. This is
    /// one range recipe, not one parser command per scalar/tab/NUL.
    CanonicalText,
    /// The residual logical spaces from one physically indivisible tab whose
    /// leading columns were consumed by an enclosing grammar prefix.
    PartialTab(DirectPartialTab),
    /// Preserve a zero-width source-to-logical mapping whose interior caret
    /// positions resolve toward the preceding visible content.
    HiddenUpstream,
    CanonicalNewline,
    None,
}

/// Parser-owned logical half of one partially consumed tab.
///
/// Physical ownership remains on the enclosing [`DirectCommand::Consume`]; the
/// residual spaces may target a different, descendant terminal. Private
/// fields and the checked constructor prevent an invalid zero/full-width
/// "partial" expansion from crossing the command seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectPartialTab {
    logical_target: DirectOwner,
    remaining_spaces: u8,
}

impl DirectPartialTab {
    #[must_use]
    pub(crate) const fn new(logical_target: DirectOwner, remaining_spaces: u8) -> Option<Self> {
        if remaining_spaces >= 1 && remaining_spaces <= 3 {
            Some(Self {
                logical_target,
                remaining_spaces,
            })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn logical_target(self) -> DirectOwner {
        self.logical_target
    }

    #[must_use]
    pub const fn remaining_spaces(self) -> u8 {
        self.remaining_spaces
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectLineEnding {
    Lf,
    Cr,
    CrLf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectTerminatorResolution {
    ContinueCanonicalNewline,
    CloseNone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectClosedChild {
    pub ends_blank: bool,
    pub item_loose_if_nonlast: bool,
    pub item_loose_if_last: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirectFinalFacts {
    #[default]
    None,
    List {
        tight: bool,
    },
    FencedCode(DirectFencedCodeCloseFacts),
}

/// Stack-shaped commands emitted directly from the donor grammar decisions.
///
/// There are intentionally no output handles or parser `NodeId`s. A consumer
/// owns a single open stack and acknowledges exactly one command at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectCommand {
    Open {
        kind: DirectBlockKind,
    },
    Consume {
        owner: DirectOwner,
        part: DirectCoveragePart,
        range: std::ops::Range<u32>,
        logical: DirectLogicalAction,
    },
    StageTerminator {
        range: std::ops::Range<u32>,
        ending: DirectLineEnding,
    },
    ResolveTerminator {
        resolution: DirectTerminatorResolution,
    },
    StageBlankGap {
        range: std::ops::Range<u32>,
    },
    ResolveBlankGap {
        owner: DirectOwner,
    },
    FinalizeParagraph {
        outcome: DirectParagraphOutcome,
    },
    /// Snapshot one parser-certified logical cut in the writer's own metric
    /// space. The command is transient and is not retained in canonical green.
    MarkFencedCodeBoundary {
        boundary: DirectFencedCodeBoundary,
    },
    Close {
        kind: DirectBlockKind,
        final_facts: DirectFinalFacts,
        /// Exact parser state on the closing node before close/finalization
        /// mutates any output fields. This bit is intentionally separate from
        /// `child`: the derived closed-child summary is not invertible.
        last_line_blank: bool,
        child: DirectClosedChild,
    },
    FinishLine {
        physical_bytes: u32,
        physical_utf16: u32,
    },
    FinishDocument,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirectPollStatus {
    #[default]
    Pending,
    CommandReady,
    ExternalWorkReady,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectPollReceipt {
    pub transitions: usize,
    pub status: DirectPollStatus,
}

/// Actor-owned sequential source contract for one direct physical line.
///
/// The donor borrows this source only while polling its opaque line work. A
/// successful byte request must be the next physical byte; repeated generated
/// peeks are absorbed inside the donor and never reach this interface.
pub trait DirectSourceLineSource {
    type Identity: Copy + Eq;
    type Error;

    fn identity(&self) -> Self::Identity;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn access_budget(&self) -> usize;
    /// Read the next unique physical byte.
    ///
    /// # Errors
    ///
    /// Returns a source-owned error when the active borrow cannot honor the
    /// promised sequential access.
    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectSourceLinePollStatus {
    NeedMore,
    Matched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectSourceLinePollReceipt {
    pub status: DirectSourceLinePollStatus,
    pub lexical_work_units: usize,
    pub source_first_reads: usize,
    pub physical_high_water: usize,
    /// Retained source payload only; fixed scalar summaries are excluded.
    pub retained_source_bytes: usize,
    pub source_budget_exhausted: bool,
    pub maximum_source_request_rewind_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DirectSourceLinePollError<SourceError> {
    ZeroFuel,
    WrongSource,
    Source(SourceError),
    InvalidSourceByte { absolute_offset: usize },
    InvalidUtf8 { absolute_offset: usize },
    EmbeddedLineEnding { absolute_offset: usize },
    SourceBudgetContractViolated,
    ScannerInvariant,
    PollAfterComplete,
    PollAfterFailure,
}

#[derive(Debug)]
struct DirectHooks {
    commands: VecDeque<DirectCommand>,
    pending_stack_effect: Option<DirectStackEffect>,
    emission_stack: Vec<NodeId>,
    previous: VecDeque<DirectIntent>,
    /// Old frames retired by the current line, keyed by their stable node.
    ///
    /// Donor mutation may discover these closes in either ancestor-first or
    /// descendant-first order. Emission order must instead be the exact
    /// reverse of `emission_stack`. Keeping the intents keyed and letting that
    /// stack drive removal avoids insertion-sorting every close by depth.
    retired: HashMap<NodeId, DirectIntent>,
    old_source: VecDeque<DirectIntent>,
    body: VecDeque<DirectIntent>,
    emission_phase: DirectEmissionPhase,
    old_source_index: usize,
    old_depth_by_node: HashMap<NodeId, usize>,
    old_last_use: Vec<Option<usize>>,
    recipe_sealed: bool,
    recipe_line_bytes: usize,
    intent_limit: usize,
    pending_gap_at_line_start: bool,
    pending_gap_floor_at_line_start: Option<NodeId>,
    claimed_offset: usize,
    line_marker_floor: Option<NodeId>,
    pending_terminator: bool,
    pending_blank_gap: bool,
    pending_blank_gap_floor: Option<NodeId>,
    paragraph_has_content: bool,
    paragraph_may_have_reference_prefix: bool,
    pending_external_work: Option<DirectExternalWork>,
    reference_work_id: Option<u64>,
    reference_finalize_resume_once: Option<DirectReferencePrefixDisposition>,
    reference_current_rebase: Option<DirectReferenceCurrentRebase>,
    next_reference_rendezvous: u64,
    #[cfg(test)]
    retired_insertions: usize,
    #[cfg(test)]
    retired_stack_probes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectStackEffect {
    Push(NodeId),
    Pop(NodeId),
}

#[derive(Clone, Debug)]
enum DirectIntent {
    Open {
        node: NodeId,
        kind: DirectBlockKind,
    },
    Consume {
        owner: NodeId,
        part: DirectCoveragePart,
        range: std::ops::Range<u32>,
        logical: DirectLogicalAction,
    },
    ConsumePartialTab {
        owner: NodeId,
        logical_target: NodeId,
        part: DirectCoveragePart,
        range: std::ops::Range<u32>,
        remaining_spaces: u8,
    },
    StageTerminator {
        range: std::ops::Range<u32>,
        ending: DirectLineEnding,
    },
    ResolveTerminator {
        resolution: DirectTerminatorResolution,
    },
    StageBlankGap {
        range: std::ops::Range<u32>,
    },
    ResolveBlankGap {
        owner: NodeId,
    },
    FinalizeParagraph {
        node: NodeId,
        outcome: DirectParagraphOutcome,
    },
    MarkFencedCodeBoundary {
        node: NodeId,
        boundary: DirectFencedCodeBoundary,
    },
    Close {
        node: NodeId,
        kind: DirectBlockKind,
        final_facts: DirectFinalFacts,
        last_line_blank: bool,
        child: DirectClosedChild,
    },
}

impl DirectIntent {
    fn required_old_owner(&self) -> Option<NodeId> {
        match self {
            Self::Consume { owner, .. }
            | Self::ConsumePartialTab { owner, .. }
            | Self::ResolveBlankGap { owner } => Some(*owner),
            Self::Open { .. }
            | Self::StageTerminator { .. }
            | Self::ResolveTerminator { .. }
            | Self::StageBlankGap { .. }
            | Self::FinalizeParagraph { .. }
            | Self::MarkFencedCodeBoundary { .. }
            | Self::Close { .. } => None,
        }
    }

    fn separate_logical_target(&self) -> Option<NodeId> {
        match self {
            Self::ConsumePartialTab { logical_target, .. } => Some(*logical_target),
            Self::Open { .. }
            | Self::Consume { .. }
            | Self::StageTerminator { .. }
            | Self::ResolveTerminator { .. }
            | Self::StageBlankGap { .. }
            | Self::ResolveBlankGap { .. }
            | Self::FinalizeParagraph { .. }
            | Self::MarkFencedCodeBoundary { .. }
            | Self::Close { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DirectEmissionPhase {
    #[default]
    Previous,
    OldBoundary,
    Body,
    Complete,
}

impl DirectHooks {
    fn new() -> Self {
        Self {
            commands: VecDeque::with_capacity(DIRECT_MAX_QUEUED_COMMANDS),
            pending_stack_effect: None,
            emission_stack: Vec::new(),
            previous: VecDeque::new(),
            retired: HashMap::new(),
            old_source: VecDeque::new(),
            body: VecDeque::new(),
            emission_phase: DirectEmissionPhase::Complete,
            old_source_index: 0,
            old_depth_by_node: HashMap::new(),
            old_last_use: Vec::new(),
            recipe_sealed: false,
            recipe_line_bytes: 0,
            intent_limit: 0,
            pending_gap_at_line_start: false,
            pending_gap_floor_at_line_start: None,
            claimed_offset: 0,
            line_marker_floor: None,
            pending_terminator: false,
            pending_blank_gap: false,
            pending_blank_gap_floor: None,
            paragraph_has_content: false,
            paragraph_may_have_reference_prefix: false,
            pending_external_work: None,
            reference_work_id: None,
            reference_finalize_resume_once: None,
            reference_current_rebase: None,
            next_reference_rendezvous: 1,
            #[cfg(test)]
            retired_insertions: 0,
            #[cfg(test)]
            retired_stack_probes: 0,
        }
    }

    fn request_reference_prefix(
        &mut self,
        context: DirectReferencePrefixContext,
    ) -> Result<DirectReferencePrefixRequest, ParseError> {
        if self.pending_external_work.is_some() || self.reference_work_id.is_some() {
            return Err(ParseError::Invariant(
                "one reference-prefix rendezvous is active",
            ));
        }
        let request = DirectReferencePrefixRequest {
            rendezvous_id: self.next_reference_rendezvous,
            logical_base: DirectReferenceLogicalPosition::default(),
            include_pending_terminator: self.pending_terminator,
            context,
        };
        self.next_reference_rendezvous =
            self.next_reference_rendezvous
                .checked_add(1)
                .ok_or(ParseError::Invariant(
                    "reference rendezvous identity exhausted",
                ))?;
        self.pending_external_work = Some(DirectExternalWork::ReferencePrefixFinalizer { request });
        Ok(request)
    }

    fn push_command(
        &mut self,
        command: DirectCommand,
        effect: Option<DirectStackEffect>,
    ) -> Result<(), ParseError> {
        if self.commands.len() == DIRECT_MAX_QUEUED_COMMANDS {
            return Err(ParseError::Invariant(
                "direct command scratch exceeded bound",
            ));
        }
        if self.pending_stack_effect.is_some() {
            return Err(ParseError::Invariant(
                "direct stack effect awaits acknowledgement",
            ));
        }
        self.commands.push_back(command);
        self.pending_stack_effect = effect;
        Ok(())
    }

    fn begin_recipe(&mut self, line_bytes: usize) -> Result<(), ParseError> {
        if !self.commands.is_empty()
            || self.pending_stack_effect.is_some()
            || !self.recipe_is_empty()
        {
            return Err(ParseError::Invariant(
                "direct recipe begins without outstanding work",
            ));
        }
        let open_depth = self.emission_stack.len();
        let depth_allowance = open_depth
            .checked_mul(DIRECT_OPEN_FRAME_INTENT_ALLOWANCE)
            .ok_or(ParseError::Invariant("direct recipe depth overflow"))?;
        self.intent_limit = DIRECT_LINE_LOCAL_INTENT_LIMIT
            .checked_add(depth_allowance)
            .ok_or(ParseError::Invariant("direct recipe bound overflow"))?;
        self.previous
            .try_reserve(DIRECT_INITIAL_PREVIOUS_INTENTS)
            .map_err(|_| ParseError::Invariant("direct recipe allocation failed"))?;
        self.retired
            .try_reserve(open_depth)
            .map_err(|_| ParseError::Invariant("direct recipe allocation failed"))?;
        self.old_source
            .try_reserve(open_depth)
            .map_err(|_| ParseError::Invariant("direct recipe allocation failed"))?;
        self.body
            .try_reserve(DIRECT_INITIAL_BODY_INTENTS)
            .map_err(|_| ParseError::Invariant("direct recipe allocation failed"))?;
        self.emission_phase = DirectEmissionPhase::Previous;
        self.old_source_index = 0;
        self.old_depth_by_node.clear();
        self.old_depth_by_node
            .try_reserve(open_depth)
            .map_err(|_| ParseError::Invariant("direct recipe allocation failed"))?;
        self.old_last_use.clear();
        self.old_last_use
            .try_reserve(open_depth)
            .map_err(|_| ParseError::Invariant("direct recipe allocation failed"))?;
        self.recipe_sealed = false;
        self.recipe_line_bytes = line_bytes;
        self.pending_gap_at_line_start = self.pending_blank_gap;
        self.pending_gap_floor_at_line_start = self.pending_blank_gap_floor;
        self.claimed_offset = 0;
        self.line_marker_floor = None;
        #[cfg(test)]
        {
            self.retired_insertions = 0;
            self.retired_stack_probes = 0;
        }
        Ok(())
    }

    fn recipe_is_empty(&self) -> bool {
        self.previous.is_empty()
            && self.retired.is_empty()
            && self.old_source.is_empty()
            && self.body.is_empty()
    }

    fn intent_count(&self) -> usize {
        self.previous.len() + self.retired.len() + self.old_source.len() + self.body.len()
    }

    fn ensure_intent_slot(&self) -> Result<(), ParseError> {
        if self.intent_count() >= self.intent_limit {
            Err(ParseError::Invariant("direct line intent bound exceeded"))
        } else {
            Ok(())
        }
    }

    fn push_previous(&mut self, intent: DirectIntent) -> Result<(), ParseError> {
        self.ensure_intent_slot()?;
        self.previous.push_back(intent);
        Ok(())
    }

    fn push_retired(&mut self, intent: DirectIntent) -> Result<(), ParseError> {
        self.ensure_intent_slot()?;
        let DirectIntent::Close { node, .. } = intent else {
            return Err(ParseError::Invariant(
                "retired recipe contains only old-frame closes",
            ));
        };
        if self.retired.insert(node, intent).is_some() {
            return Err(ParseError::Invariant(
                "retired frame closes exactly once per recipe",
            ));
        }
        #[cfg(test)]
        {
            self.retired_insertions += 1;
        }
        Ok(())
    }

    fn push_old_source_front(&mut self, intent: DirectIntent) -> Result<(), ParseError> {
        self.ensure_intent_slot()?;
        self.old_source.push_front(intent);
        Ok(())
    }

    fn push_old_source(&mut self, intent: DirectIntent) -> Result<(), ParseError> {
        self.ensure_intent_slot()?;
        self.old_source.push_back(intent);
        Ok(())
    }

    fn push_body(&mut self, intent: DirectIntent) -> Result<(), ParseError> {
        self.ensure_intent_slot()?;
        self.body.push_back(intent);
        Ok(())
    }

    fn push_body_before_trailing_open(
        &mut self,
        node: NodeId,
        intent: DirectIntent,
    ) -> Result<(), ParseError> {
        self.ensure_intent_slot()?;
        let open = self
            .body
            .pop_back()
            .ok_or(ParseError::Invariant("new block has a body open intent"))?;
        if !matches!(open, DirectIntent::Open { node: open_node, .. } if open_node == node) {
            return Err(ParseError::Invariant(
                "parent-owned source immediately precedes the new block open",
            ));
        }
        self.body.push_back(intent);
        self.body.push_back(open);
        Ok(())
    }

    fn validate_source_partition(&self) -> Result<(), ParseError> {
        let mut covered = 0_u32;
        for intent in self.old_source.iter().chain(self.body.iter()) {
            let range = match intent {
                DirectIntent::Consume { range, .. }
                | DirectIntent::ConsumePartialTab { range, .. }
                | DirectIntent::StageTerminator { range, .. }
                | DirectIntent::StageBlankGap { range } => range,
                DirectIntent::Open { .. }
                | DirectIntent::ResolveTerminator { .. }
                | DirectIntent::ResolveBlankGap { .. }
                | DirectIntent::FinalizeParagraph { .. }
                | DirectIntent::MarkFencedCodeBoundary { .. }
                | DirectIntent::Close { .. } => continue,
            };
            if range.start != covered || range.end < range.start {
                return Err(ParseError::Invariant(
                    "direct recipe source ranges form one ordered partition",
                ));
            }
            covered = range.end;
        }
        let expected = u32::try_from(self.recipe_line_bytes)
            .map_err(|_| ParseError::Invariant("direct line below u32"))?;
        if covered != expected {
            return Err(ParseError::Invariant(
                "direct recipe source ranges cover the physical line",
            ));
        }
        Ok(())
    }

    fn seal_recipe(&mut self) -> Result<(), ParseError> {
        if self.recipe_sealed {
            return Ok(());
        }
        self.validate_source_partition()?;
        if self.body.iter().any(|intent| {
            intent
                .required_old_owner()
                .is_some_and(|owner| self.retired.contains_key(&owner))
                || intent
                    .separate_logical_target()
                    .is_some_and(|target| self.retired.contains_key(&target))
        }) {
            return Err(ParseError::Invariant(
                "replacement source cannot target a retired frame",
            ));
        }
        // Retired frames must be one suffix of the pre-line writer stack.
        // Validate that invariant with one top-down stack walk. The same
        // stack then remains the sole ordering authority while commands are
        // emitted, so close discovery chronology never needs normalization.
        let mut retired_suffix_len = 0_usize;
        for node in self.emission_stack.iter().rev() {
            #[cfg(test)]
            {
                self.retired_stack_probes += 1;
            }
            if self.retired.contains_key(node) {
                retired_suffix_len += 1;
            } else {
                break;
            }
        }
        if retired_suffix_len != self.retired.len() {
            return Err(ParseError::Invariant(
                "retired closes are one suffix of the pre-line emitted stack",
            ));
        }
        if !self.old_source.is_empty() {
            for (depth, node) in self.emission_stack.iter().copied().enumerate() {
                #[cfg(test)]
                {
                    self.retired_stack_probes += 1;
                }
                if self.old_depth_by_node.insert(node, depth).is_some() {
                    return Err(ParseError::Invariant(
                        "pre-line emitted stack contains unique frames",
                    ));
                }
            }
        }
        self.old_last_use.resize(self.emission_stack.len(), None);
        for (ordinal, intent) in self.old_source.iter().enumerate() {
            let Some(owner) = intent.required_old_owner() else {
                return Err(ParseError::Invariant(
                    "old-source intent names an old owner",
                ));
            };
            let depth =
                self.old_depth_by_node
                    .get(&owner)
                    .copied()
                    .ok_or(ParseError::Invariant(
                        "old-source owner is on the pre-line stack",
                    ))?;
            self.old_last_use[depth] = Some(ordinal);
        }
        self.recipe_sealed = true;
        Ok(())
    }

    fn retired_top_last_use(&self) -> Result<Option<usize>, ParseError> {
        let Some(node) = self.emission_stack.last() else {
            return if self.retired.is_empty() {
                Ok(None)
            } else {
                Err(ParseError::Invariant("retired stack is nonempty"))
            };
        };
        let Some(DirectIntent::Close { .. }) = self.retired.get(node) else {
            return if self.retired.is_empty() {
                Ok(None)
            } else {
                Err(ParseError::Invariant(
                    "retired recipe follows the emitted stack suffix",
                ))
            };
        };
        let depth = self
            .emission_stack
            .len()
            .checked_sub(1)
            .ok_or(ParseError::Invariant("retired stack is nonempty"))?;
        Ok(self.old_last_use.get(depth).copied().flatten())
    }

    fn pop_retired_top(&mut self) -> Result<Option<DirectIntent>, ParseError> {
        if self.retired.is_empty() {
            return Ok(None);
        }
        let node = self
            .emission_stack
            .last()
            .copied()
            .ok_or(ParseError::Invariant("retired stack is nonempty"))?;
        self.retired
            .remove(&node)
            .map(Some)
            .ok_or(ParseError::Invariant(
                "retired recipe follows the emitted stack suffix",
            ))
    }

    fn pop_old_boundary_intent(&mut self) -> Result<Option<DirectIntent>, ParseError> {
        if let Some(last_use) = self.retired_top_last_use()? {
            if last_use < self.old_source_index {
                return self.pop_retired_top();
            }
        } else if !self.retired.is_empty() {
            return self.pop_retired_top();
        }

        if let Some(intent) = self.old_source.pop_front() {
            self.old_source_index = self
                .old_source_index
                .checked_add(1)
                .ok_or(ParseError::Invariant("old-source ordinal overflow"))?;
            return Ok(Some(intent));
        }

        if !self.retired.is_empty() {
            return self.pop_retired_top();
        }
        Ok(None)
    }

    fn pop_next_intent(&mut self) -> Result<Option<DirectIntent>, ParseError> {
        loop {
            let next = match self.emission_phase {
                DirectEmissionPhase::Previous => self.previous.pop_front(),
                DirectEmissionPhase::OldBoundary => self.pop_old_boundary_intent()?,
                DirectEmissionPhase::Body => self.body.pop_front(),
                DirectEmissionPhase::Complete => return Ok(None),
            };
            if next.is_some() {
                return Ok(next);
            }
            self.emission_phase = match self.emission_phase {
                DirectEmissionPhase::Previous => DirectEmissionPhase::OldBoundary,
                DirectEmissionPhase::OldBoundary => DirectEmissionPhase::Body,
                DirectEmissionPhase::Body | DirectEmissionPhase::Complete => {
                    DirectEmissionPhase::Complete
                }
            };
        }
    }

    fn owner(&self, node: NodeId) -> Result<DirectOwner, ParseError> {
        let index = self
            .emission_stack
            .iter()
            .rposition(|candidate| *candidate == node)
            .ok_or(ParseError::Invariant(
                "direct source owner is on the emitted open path",
            ))?;
        let generations = self
            .emission_stack
            .len()
            .checked_sub(index + 1)
            .ok_or(ParseError::Invariant("direct owner depth underflow"))?;
        Ok(DirectOwner::ancestor(u32::try_from(generations).map_err(
            |_| ParseError::Invariant("direct owner depth below u32"),
        )?))
    }

    fn queue_next_intent(&mut self) -> Result<bool, ParseError> {
        if !self.commands.is_empty() {
            return Ok(true);
        }
        self.seal_recipe()?;
        let Some(intent) = self.pop_next_intent()? else {
            return Ok(false);
        };
        let (command, effect) = match intent {
            DirectIntent::Open { node, kind } => (
                DirectCommand::Open { kind },
                Some(DirectStackEffect::Push(node)),
            ),
            DirectIntent::Consume {
                owner,
                part,
                range,
                logical,
            } => (
                DirectCommand::Consume {
                    owner: self.owner(owner)?,
                    part,
                    range,
                    logical,
                },
                None,
            ),
            DirectIntent::ConsumePartialTab {
                owner,
                logical_target,
                part,
                range,
                remaining_spaces,
            } => {
                let owner = self.owner(owner)?;
                let logical_target = self.owner(logical_target)?;
                if logical_target.generations_from_top() > owner.generations_from_top() {
                    return Err(ParseError::Invariant(
                        "direct partial-tab target is the owner or its descendant",
                    ));
                }
                let partial = DirectPartialTab::new(logical_target, remaining_spaces).ok_or(
                    ParseError::Invariant("direct partial tab retains one through three spaces"),
                )?;
                (
                    DirectCommand::Consume {
                        owner,
                        part,
                        range,
                        logical: DirectLogicalAction::PartialTab(partial),
                    },
                    None,
                )
            }
            DirectIntent::StageTerminator { range, ending } => {
                (DirectCommand::StageTerminator { range, ending }, None)
            }
            DirectIntent::ResolveTerminator { resolution } => {
                (DirectCommand::ResolveTerminator { resolution }, None)
            }
            DirectIntent::StageBlankGap { range } => (DirectCommand::StageBlankGap { range }, None),
            DirectIntent::ResolveBlankGap { owner } => (
                DirectCommand::ResolveBlankGap {
                    owner: self.owner(owner)?,
                },
                None,
            ),
            DirectIntent::FinalizeParagraph { node, outcome } => {
                if self.emission_stack.last() != Some(&node) {
                    return Err(ParseError::Invariant(
                        "direct Paragraph finalization targets the emitted stack top",
                    ));
                }
                (DirectCommand::FinalizeParagraph { outcome }, None)
            }
            DirectIntent::MarkFencedCodeBoundary { node, boundary } => {
                if self.emission_stack.last() != Some(&node) {
                    return Err(ParseError::Invariant(
                        "direct FencedCode boundary targets the emitted stack top",
                    ));
                }
                (DirectCommand::MarkFencedCodeBoundary { boundary }, None)
            }
            DirectIntent::Close {
                node,
                kind,
                final_facts,
                last_line_blank,
                child,
            } => {
                if self.emission_stack.last() != Some(&node) {
                    return Err(ParseError::Invariant(
                        "direct close is the emitted stack top",
                    ));
                }
                (
                    DirectCommand::Close {
                        kind,
                        final_facts,
                        last_line_blank,
                        child,
                    },
                    Some(DirectStackEffect::Pop(node)),
                )
            }
        };
        self.push_command(command, effect)?;
        Ok(true)
    }

    fn acknowledge_stack_effect(&mut self) -> Result<(), ParseError> {
        match self.pending_stack_effect.take() {
            None => Ok(()),
            Some(DirectStackEffect::Push(node)) => {
                self.emission_stack.push(node);
                Ok(())
            }
            Some(DirectStackEffect::Pop(node)) => {
                if self.emission_stack.pop() == Some(node) {
                    Ok(())
                } else {
                    Err(ParseError::Invariant(
                        "direct acknowledged close matches emitted stack",
                    ))
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WorkPollReceipt {
    transitions: usize,
    index_operations: usize,
}

#[derive(Debug)]
enum LinePhase {
    CheckOpen {
        container: NodeId,
    },
    OpenNew(OpenNewTransition),
    PrepareText {
        container: NodeId,
        last_matched_container: NodeId,
    },
    ClearAncestors {
        container: NodeId,
        last_matched_container: NodeId,
        ancestor: NodeId,
    },
    CloseUnmatched {
        container: NodeId,
        last_matched_container: NodeId,
    },
    DispatchText {
        container: NodeId,
    },
}

/// Donor-owned coroutine cursor for the `CommonMark` block-opener precedence
/// chain. Every non-`Start` stage invokes exactly one existing handler family;
/// false results preserve the handler's parser/container mutations for the
/// next stage instead of replaying earlier recognizers.
#[derive(Clone, Copy, Debug)]
struct OpenNewTransition {
    container: NodeId,
    last_matched_container: NodeId,
    all_matched: bool,
    maybe_lazy: bool,
    depth: usize,
    indented: bool,
    stage: OpenNewStage,
}

impl OpenNewTransition {
    fn apply_reference_current_rebase(
        &mut self,
        rebase: DirectReferenceCurrentRebase,
    ) -> Result<(), ParseError> {
        if self.last_matched_container != rebase.discarded {
            return Err(ParseError::Invariant(
                "reference-only rebase owns the matched Paragraph",
            ));
        }
        self.last_matched_container = rebase.current;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenNewStage {
    Start,
    BlockQuote,
    AtxHeading,
    CodeFence,
    HtmlBlock,
    SetextHeading,
    ThematicBreak,
    List,
    CodeBlock,
    Table,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenNewScheduler {
    Resumable,
    LegacyAtomic,
}

/// Parser-owned physical-line control state.  Scheduling wrappers may retain
/// this value between polls, but they do not own any grammar phase ordering.
#[derive(Debug)]
struct LineTransition {
    phase: LinePhase,
}

#[derive(Clone, Copy, Debug)]
struct TreeCursorFrame {
    node: NodeId,
    next_child: usize,
}

#[derive(Debug)]
struct ListPositionScan {
    list: NodeId,
    max_end: Position,
    stack: Vec<TreeCursorFrame>,
}

#[derive(Debug)]
enum FinishPhase {
    CloseCurrent,
    CloseRoot,
    Propagate {
        postorder: Vec<TreeCursorFrame>,
        active_list: Option<ListPositionScan>,
    },
}

/// Parser-owned EOF control state, shared by unlimited and fuelled drivers.
#[derive(Debug)]
struct FinishTransition {
    phase: FinishPhase,
}

#[derive(Debug)]
enum DirectLineInput {
    Buffered(String),
    Segmented {
        controller_window: String,
        physical_bytes: u32,
        physical_utf16: u32,
    },
    SourceMetrics {
        physical_bytes: u32,
        physical_utf16: u32,
    },
}

#[derive(Debug)]
struct DirectLineWork {
    input: DirectLineInput,
    transition: Option<LineTransition>,
    semantic_complete: bool,
    output_prepared: bool,
    finish_queued: bool,
}

#[derive(Clone, Copy, Debug)]
struct DirectAtxMatch {
    level: u8,
    claim_start: usize,
    opener_start: usize,
    opener_start_column: usize,
    indent_columns: usize,
    opener_end: usize,
    opener_column: usize,
    marker_end: usize,
    donor_chopped_end: usize,
    visible_end: usize,
    content_end: usize,
    line_end: usize,
    closed: bool,
    ending: Option<DirectLineEnding>,
}

#[derive(Clone, Copy, Debug, Default)]
struct DirectAtxObservation {
    observed_bytes: usize,
    second_to_last_byte: Option<u8>,
    last_byte: Option<u8>,
}

#[derive(Clone, Copy, Debug)]
struct DirectSegmentedLineFacts {
    physical_bytes: u32,
    content_end: usize,
    ending: Option<DirectLineEnding>,
    controller_window_complete: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct DirectSegmentedUtf8Fold {
    remaining: u8,
    code_point: u32,
    minimum: u32,
    utf16: u64,
}

#[derive(Debug)]
struct DirectSegmentedPhysicalScan<I: Copy + Eq> {
    identity: I,
    physical_bytes: usize,
    next_offset: usize,
    controller_window: Vec<u8>,
    ending_tail: [u8; 2],
    ending_tail_len: usize,
    utf8: DirectSegmentedUtf8Fold,
}

#[derive(Debug)]
struct DirectSegmentedPhysicalLine {
    controller_window: String,
    physical_bytes: u32,
    physical_utf16: u32,
    content_end: usize,
    ending: Option<DirectLineEnding>,
    controller_window_complete: bool,
}

enum DirectSourceLineStage<I: Copy + Eq> {
    Atx {
        scanner: FusedAtxLineScanner<I>,
        observation: DirectAtxObservation,
    },
    MatchedAtx {
        scanner: FusedAtxLineScanner<I>,
        matched: DirectAtxMatch,
    },
    Segmented {
        scan: DirectSegmentedPhysicalScan<I>,
    },
    MatchedSegmented {
        line: DirectSegmentedPhysicalLine,
    },
    Failed,
}

/// Opaque donor-owned continuation for one source-backed physical line.
///
/// This type is deliberately non-`Clone`: a terminal match can enter the
/// parser only by being consumed by [`DirectValueBlockParser::commit_source_line`].
/// Generated cuts and heading facts remain private donor state.
pub struct DirectSourceLineWork<I: Copy + Eq> {
    parser_instance_id: u64,
    admission_id: u64,
    source_identity: I,
    boundary_line_number: usize,
    physical_bytes: usize,
    stage: DirectSourceLineStage<I>,
}

impl DirectAtxObservation {
    const fn new() -> Self {
        Self {
            observed_bytes: 0,
            second_to_last_byte: None,
            last_byte: None,
        }
    }

    fn observe(&mut self, absolute_offset: usize, byte: u8) -> bool {
        if absolute_offset != self.observed_bytes {
            return false;
        }
        self.second_to_last_byte = self.last_byte;
        self.last_byte = Some(byte);
        self.observed_bytes += 1;
        true
    }

    fn finish_match(
        &self,
        cuts: AtxLineCuts,
        donor: FusedAtxDonorMatch,
        base: usize,
    ) -> Result<DirectAtxMatch, ()> {
        if self.observed_bytes != cuts.line_end() {
            return Err(());
        }
        if donor.claim_start() > donor.opener_start()
            || donor.opener_start() > cuts.opener_end()
            || donor.indent_columns() >= CODE_INDENT
            || !(1..=6).contains(&donor.level())
        {
            return Err(());
        }
        let ending = match cuts.line_end().checked_sub(cuts.content_end()).ok_or(())? {
            0 => None,
            1 => match self.last_byte {
                Some(b'\n') => Some(DirectLineEnding::Lf),
                Some(b'\r') => Some(DirectLineEnding::Cr),
                _ => return Err(()),
            },
            2 if self.second_to_last_byte == Some(b'\r') && self.last_byte == Some(b'\n') => {
                Some(DirectLineEnding::CrLf)
            }
            _ => return Err(()),
        };
        Ok(DirectAtxMatch {
            level: donor.level(),
            claim_start: base.checked_add(donor.claim_start()).ok_or(())?,
            opener_start: base.checked_add(donor.opener_start()).ok_or(())?,
            opener_start_column: donor.opener_start_column(),
            indent_columns: donor.indent_columns(),
            opener_end: base.checked_add(cuts.opener_end()).ok_or(())?,
            opener_column: donor.opener_end_column(),
            marker_end: base.checked_add(cuts.marker_end()).ok_or(())?,
            donor_chopped_end: base.checked_add(cuts.donor_chopped_end()).ok_or(())?,
            visible_end: base.checked_add(cuts.visible_end()).ok_or(())?,
            content_end: base.checked_add(cuts.content_end()).ok_or(())?,
            line_end: base.checked_add(cuts.line_end()).ok_or(())?,
            closed: cuts.closed(),
            ending,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectSegmentedScanError {
    InvalidUtf8 { absolute_offset: usize },
    EmbeddedLineEnding { absolute_offset: usize },
    MetricOverflow,
    Invariant,
}

impl DirectSegmentedUtf8Fold {
    fn push(&mut self, byte: u8, absolute_offset: usize) -> Result<(), DirectSegmentedScanError> {
        if self.remaining == 0 {
            match byte {
                0x00..=0x7f => self.add_utf16(1),
                0xc2..=0xdf => {
                    self.remaining = 1;
                    self.code_point = u32::from(byte & 0x1f);
                    self.minimum = 0x80;
                    Ok(())
                }
                0xe0..=0xef => {
                    self.remaining = 2;
                    self.code_point = u32::from(byte & 0x0f);
                    self.minimum = 0x800;
                    Ok(())
                }
                0xf0..=0xf4 => {
                    self.remaining = 3;
                    self.code_point = u32::from(byte & 0x07);
                    self.minimum = 0x1_0000;
                    Ok(())
                }
                _ => Err(DirectSegmentedScanError::InvalidUtf8 { absolute_offset }),
            }
        } else {
            if byte & 0xc0 != 0x80 {
                return Err(DirectSegmentedScanError::InvalidUtf8 { absolute_offset });
            }
            self.code_point = (self.code_point << 6) | u32::from(byte & 0x3f);
            self.remaining -= 1;
            if self.remaining != 0 {
                return Ok(());
            }
            if self.code_point < self.minimum
                || self.code_point > 0x10_ffff
                || (0xd800..=0xdfff).contains(&self.code_point)
            {
                return Err(DirectSegmentedScanError::InvalidUtf8 { absolute_offset });
            }
            self.add_utf16(if self.code_point <= 0xffff { 1 } else { 2 })
        }
    }

    fn add_utf16(&mut self, units: u64) -> Result<(), DirectSegmentedScanError> {
        self.utf16 = self
            .utf16
            .checked_add(units)
            .ok_or(DirectSegmentedScanError::MetricOverflow)?;
        Ok(())
    }

    fn finish(self, absolute_offset: usize) -> Result<u32, DirectSegmentedScanError> {
        if self.remaining != 0 {
            return Err(DirectSegmentedScanError::InvalidUtf8 { absolute_offset });
        }
        u32::try_from(self.utf16).map_err(|_| DirectSegmentedScanError::MetricOverflow)
    }
}

impl<I: Copy + Eq> DirectSegmentedPhysicalScan<I> {
    fn from_atx_rejection(
        identity: I,
        physical_bytes: usize,
        rejection_prefix: &[u8],
        physical_high_water: usize,
    ) -> Result<Self, DirectSegmentedScanError> {
        if rejection_prefix.len() != physical_high_water
            || physical_high_water > physical_bytes
            || rejection_prefix.len() > DIRECT_SEGMENTED_LINE_WINDOW_BYTES
        {
            return Err(DirectSegmentedScanError::Invariant);
        }
        let mut scan = Self {
            identity,
            physical_bytes,
            next_offset: 0,
            controller_window: Vec::with_capacity(DIRECT_SEGMENTED_LINE_WINDOW_BYTES),
            ending_tail: [0; 2],
            ending_tail_len: 0,
            utf8: DirectSegmentedUtf8Fold::default(),
        };
        for byte in rejection_prefix.iter().copied() {
            scan.push(byte)?;
        }
        Ok(scan)
    }

    fn push(&mut self, byte: u8) -> Result<(), DirectSegmentedScanError> {
        let offset = self.next_offset;
        self.utf8.push(byte, offset)?;
        if self.controller_window.len() < DIRECT_SEGMENTED_LINE_WINDOW_BYTES {
            self.controller_window.push(byte);
        }
        if self.ending_tail_len < self.ending_tail.len() {
            self.ending_tail[self.ending_tail_len] = byte;
            self.ending_tail_len += 1;
        } else {
            let content = self.ending_tail[0];
            self.ending_tail[0] = self.ending_tail[1];
            self.ending_tail[1] = byte;
            Self::validate_content_byte(content, offset - 2)?;
        }
        self.next_offset = self
            .next_offset
            .checked_add(1)
            .ok_or(DirectSegmentedScanError::MetricOverflow)?;
        Ok(())
    }

    fn validate_content_byte(
        byte: u8,
        absolute_offset: usize,
    ) -> Result<(), DirectSegmentedScanError> {
        if matches!(byte, b'\r' | b'\n') {
            Err(DirectSegmentedScanError::EmbeddedLineEnding { absolute_offset })
        } else {
            Ok(())
        }
    }

    fn complete(mut self) -> Result<DirectSegmentedPhysicalLine, DirectSegmentedScanError> {
        if self.next_offset != self.physical_bytes {
            return Err(DirectSegmentedScanError::Invariant);
        }
        let physical_utf16 = self.utf8.finish(self.next_offset)?;
        let tail = &self.ending_tail[..self.ending_tail_len];
        let (ending_bytes, ending) = if tail.ends_with(b"\r\n") {
            (2, Some(DirectLineEnding::CrLf))
        } else if tail.ends_with(b"\n") {
            (1, Some(DirectLineEnding::Lf))
        } else if tail.ends_with(b"\r") {
            (1, Some(DirectLineEnding::Cr))
        } else {
            (0, None)
        };
        let body_tail = self.ending_tail_len - ending_bytes;
        let tail_start = self.next_offset - self.ending_tail_len;
        for (index, byte) in tail[..body_tail].iter().copied().enumerate() {
            Self::validate_content_byte(byte, tail_start + index)?;
        }
        let content_end = self
            .next_offset
            .checked_sub(ending_bytes)
            .ok_or(DirectSegmentedScanError::Invariant)?;
        let complete = self.controller_window.len() == self.physical_bytes;
        if !complete {
            match std::str::from_utf8(&self.controller_window) {
                Ok(_) => {}
                Err(error) if error.error_len().is_none() => {
                    self.controller_window.truncate(error.valid_up_to());
                }
                Err(error) => {
                    return Err(DirectSegmentedScanError::InvalidUtf8 {
                        absolute_offset: error.valid_up_to(),
                    });
                }
            }
        }
        let controller_window = String::from_utf8(self.controller_window)
            .map_err(|_| DirectSegmentedScanError::Invariant)?;
        Ok(DirectSegmentedPhysicalLine {
            controller_window,
            physical_bytes: u32::try_from(self.physical_bytes)
                .map_err(|_| DirectSegmentedScanError::MetricOverflow)?,
            physical_utf16,
            content_end,
            ending,
            controller_window_complete: complete,
        })
    }

    fn retained_source_bytes(&self) -> usize {
        self.controller_window.len()
    }
}

impl DirectAtxMatch {
    fn offset_by(self, base: usize) -> Result<Self, ParseError> {
        Ok(Self {
            level: self.level,
            claim_start: base
                .checked_add(self.claim_start)
                .ok_or(ParseError::Invariant("direct ATX claim offset overflow"))?,
            opener_start: base
                .checked_add(self.opener_start)
                .ok_or(ParseError::Invariant("direct ATX start offset overflow"))?,
            opener_start_column: self.opener_start_column,
            indent_columns: self.indent_columns,
            opener_end: base
                .checked_add(self.opener_end)
                .ok_or(ParseError::Invariant("direct ATX opener offset overflow"))?,
            opener_column: self.opener_column,
            marker_end: base
                .checked_add(self.marker_end)
                .ok_or(ParseError::Invariant("direct ATX marker offset overflow"))?,
            donor_chopped_end: base
                .checked_add(self.donor_chopped_end)
                .ok_or(ParseError::Invariant("direct ATX chop offset overflow"))?,
            visible_end: base
                .checked_add(self.visible_end)
                .ok_or(ParseError::Invariant("direct ATX visible offset overflow"))?,
            content_end: base
                .checked_add(self.content_end)
                .ok_or(ParseError::Invariant("direct ATX content offset overflow"))?,
            line_end: base
                .checked_add(self.line_end)
                .ok_or(ParseError::Invariant("direct ATX line offset overflow"))?,
            closed: self.closed,
            ending: self.ending,
        })
    }
}

struct DirectSliceSource<'a> {
    bytes: &'a [u8],
    next: usize,
    remaining_budget: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectSliceSourceError {
    BudgetExhausted,
    NonSequential,
    PastEnd,
}

impl DirectSliceSource<'_> {
    fn replenish(&mut self) {
        self.remaining_budget = FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL;
    }
}

impl DirectSourceLineSource for DirectSliceSource<'_> {
    type Identity = ();
    type Error = DirectSliceSourceError;

    fn identity(&self) -> Self::Identity {}

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn access_budget(&self) -> usize {
        self.remaining_budget
    }

    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error> {
        if absolute_offset != self.next {
            return Err(DirectSliceSourceError::NonSequential);
        }
        if self.remaining_budget == 0 {
            return Err(DirectSliceSourceError::BudgetExhausted);
        }
        let byte = self
            .bytes
            .get(absolute_offset)
            .copied()
            .ok_or(DirectSliceSourceError::PastEnd)?;
        self.next += 1;
        self.remaining_budget -= 1;
        Ok(byte)
    }
}

fn direct_atx_match_from_slice(
    line_suffix: &str,
    base: usize,
    initial_column: usize,
    allow_initial_bom: bool,
) -> Result<Option<DirectAtxMatch>, ParseError> {
    let mut source = DirectSliceSource {
        bytes: line_suffix.as_bytes(),
        next: 0,
        remaining_budget: FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL,
    };
    let mut work = DirectSourceLineWork::new_with_scan_context(
        0,
        0,
        0,
        (),
        source.len(),
        initial_column,
        allow_initial_bom,
    );
    loop {
        source.replenish();
        let receipt = work
            .poll_source(&mut source, FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL)
            .map_err(|error| match error {
                DirectSourceLinePollError::ScannerInvariant => {
                    ParseError::Invariant("buffered fused ATX scanner invariant")
                }
                DirectSourceLinePollError::InvalidSourceByte { .. } => {
                    ParseError::Invariant("buffered UTF-8 contains scanner sentinel")
                }
                DirectSourceLinePollError::InvalidUtf8 { .. }
                | DirectSourceLinePollError::EmbeddedLineEnding { .. } => {
                    ParseError::Invariant("buffered line physical validation failed")
                }
                DirectSourceLinePollError::ZeroFuel
                | DirectSourceLinePollError::WrongSource
                | DirectSourceLinePollError::SourceBudgetContractViolated
                | DirectSourceLinePollError::PollAfterComplete
                | DirectSourceLinePollError::PollAfterFailure => {
                    ParseError::Invariant("buffered fused ATX scan is infallible")
                }
                DirectSourceLinePollError::Source(_) => {
                    ParseError::Invariant("buffered fused ATX source contract")
                }
            })?;
        if matches!(&work.stage, DirectSourceLineStage::Segmented { .. }) {
            return Ok(None);
        }
        match receipt.status {
            DirectSourceLinePollStatus::NeedMore => {}
            DirectSourceLinePollStatus::Matched => {
                return match work.stage {
                    DirectSourceLineStage::MatchedAtx { matched, .. } => {
                        Ok(Some(matched.offset_by(base)?))
                    }
                    DirectSourceLineStage::MatchedSegmented { .. } => Ok(None),
                    _ => Err(ParseError::Invariant(
                        "matched source status owns terminal donor facts",
                    )),
                };
            }
        }
    }
}

struct DirectFusedSource<'a, S: DirectSourceLineSource> {
    source: &'a mut S,
    observation: &'a mut DirectAtxObservation,
    observation_failed: bool,
}

impl<S: DirectSourceLineSource> FusedAtxLineSource for DirectFusedSource<'_, S> {
    type Identity = S::Identity;
    type Error = S::Error;

    fn identity(&self) -> Self::Identity {
        self.source.identity()
    }

    fn len(&self) -> usize {
        self.source.len()
    }

    fn access_budget(&self) -> usize {
        self.source.access_budget()
    }

    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error> {
        let byte = self.source.read_byte(absolute_offset)?;
        if !self.observation.observe(absolute_offset, byte) {
            self.observation_failed = true;
            return Ok(0xff);
        }
        Ok(byte)
    }
}

impl<I: Copy + Eq> DirectSourceLineWork<I> {
    fn new(
        parser_instance_id: u64,
        admission_id: u64,
        boundary_line_number: usize,
        identity: I,
        physical_bytes: usize,
    ) -> Self {
        Self::new_with_scan_context(
            parser_instance_id,
            admission_id,
            boundary_line_number,
            identity,
            physical_bytes,
            0,
            boundary_line_number == 0,
        )
    }

    fn new_with_scan_context(
        parser_instance_id: u64,
        admission_id: u64,
        boundary_line_number: usize,
        identity: I,
        physical_bytes: usize,
        initial_column: usize,
        allow_initial_bom: bool,
    ) -> Self {
        Self {
            parser_instance_id,
            admission_id,
            source_identity: identity,
            boundary_line_number,
            physical_bytes,
            stage: DirectSourceLineStage::Atx {
                scanner: FusedAtxLineScanner::new_with_block_prefix(
                    identity,
                    physical_bytes,
                    initial_column,
                    allow_initial_bom,
                ),
                observation: DirectAtxObservation::new(),
            },
        }
    }

    fn new_segmented(
        parser_instance_id: u64,
        admission_id: u64,
        boundary_line_number: usize,
        identity: I,
        physical_bytes: usize,
    ) -> Result<Self, ParseError> {
        let scan =
            DirectSegmentedPhysicalScan::from_atx_rejection(identity, physical_bytes, &[], 0)
                .map_err(|_| ParseError::Invariant("empty segmented source-line scan is valid"))?;
        Ok(Self {
            parser_instance_id,
            admission_id,
            source_identity: identity,
            boundary_line_number,
            physical_bytes,
            stage: DirectSourceLineStage::Segmented { scan },
        })
    }

    /// Advance the donor opener state within caller and source budgets.
    ///
    /// # Errors
    ///
    /// Returns a typed source or scanner error. Zero fuel is a resumable caller
    /// precondition, and a zero access grant yields `NeedMore`; identity,
    /// source, contract, sentinel, or scanner failures poison the work and can
    /// only report [`DirectSourceLinePollError::PollAfterFailure`] afterward.
    pub fn poll_source<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
    ) -> Result<DirectSourceLinePollReceipt, DirectSourceLinePollError<S::Error>>
    where
        S: DirectSourceLineSource<Identity = I>,
    {
        if fuel == 0 {
            return Err(DirectSourceLinePollError::ZeroFuel);
        }
        let stage = std::mem::replace(&mut self.stage, DirectSourceLineStage::Failed);
        match stage {
            DirectSourceLineStage::Atx {
                scanner,
                observation,
            } => self.poll_atx_source_stage(source, fuel, scanner, observation),
            DirectSourceLineStage::Segmented { scan } => {
                self.poll_segmented_source_stage(source, fuel, scan)
            }
            DirectSourceLineStage::MatchedAtx { scanner, matched } => {
                self.stage = DirectSourceLineStage::MatchedAtx { scanner, matched };
                Err(DirectSourceLinePollError::PollAfterComplete)
            }
            DirectSourceLineStage::MatchedSegmented { line } => {
                self.stage = DirectSourceLineStage::MatchedSegmented { line };
                Err(DirectSourceLinePollError::PollAfterComplete)
            }
            DirectSourceLineStage::Failed => Err(DirectSourceLinePollError::PollAfterFailure),
        }
    }

    fn poll_atx_source_stage<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
        mut scanner: FusedAtxLineScanner<I>,
        mut observation: DirectAtxObservation,
    ) -> Result<DirectSourceLinePollReceipt, DirectSourceLinePollError<S::Error>>
    where
        S: DirectSourceLineSource<Identity = I>,
    {
        let mut adapted = DirectFusedSource {
            source,
            observation: &mut observation,
            observation_failed: false,
        };
        let receipt = match scanner.poll(&mut adapted, fuel) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.stage = DirectSourceLineStage::Atx {
                    scanner,
                    observation,
                };
                return Err(map_direct_source_line_poll_error(error));
            }
        };
        if adapted.observation_failed {
            self.stage = DirectSourceLineStage::Failed;
            return Err(DirectSourceLinePollError::ScannerInvariant);
        }
        let status = match receipt.result {
            FusedAtxLineScanResult::NeedMore => {
                self.stage = DirectSourceLineStage::Atx {
                    scanner,
                    observation,
                };
                DirectSourceLinePollStatus::NeedMore
            }
            FusedAtxLineScanResult::Matched(cuts) => {
                let Some(donor) = scanner.donor_match() else {
                    self.stage = DirectSourceLineStage::Failed;
                    return Err(DirectSourceLinePollError::ScannerInvariant);
                };
                let Ok(matched) = observation.finish_match(cuts, donor, 0) else {
                    self.stage = DirectSourceLineStage::Failed;
                    return Err(DirectSourceLinePollError::ScannerInvariant);
                };
                self.stage = DirectSourceLineStage::MatchedAtx { scanner, matched };
                DirectSourceLinePollStatus::Matched
            }
            FusedAtxLineScanResult::NoMatch => {
                let scan = DirectSegmentedPhysicalScan::from_atx_rejection(
                    self.source_identity,
                    self.physical_bytes,
                    scanner.rejection_prefix(),
                    scanner.physical_high_water(),
                )
                .map_err(|_| {
                    self.stage = DirectSourceLineStage::Failed;
                    DirectSourceLinePollError::ScannerInvariant
                })?;
                self.stage = DirectSourceLineStage::Segmented { scan };
                DirectSourceLinePollStatus::NeedMore
            }
        };
        Ok(DirectSourceLinePollReceipt {
            status,
            lexical_work_units: receipt.lexical_work_units,
            source_first_reads: receipt.source_first_reads,
            physical_high_water: receipt.physical_high_water,
            retained_source_bytes: self.retained_source_bytes(),
            source_budget_exhausted: receipt.source_budget_exhausted,
            maximum_source_request_rewind_bytes: receipt.maximum_source_request_rewind_bytes,
        })
    }

    fn poll_segmented_source_stage<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
        mut scan: DirectSegmentedPhysicalScan<I>,
    ) -> Result<DirectSourceLinePollReceipt, DirectSourceLinePollError<S::Error>>
    where
        S: DirectSourceLineSource<Identity = I>,
    {
        if source.identity() != scan.identity || source.len() != scan.physical_bytes {
            self.stage = DirectSourceLineStage::Failed;
            return Err(DirectSourceLinePollError::WrongSource);
        }
        let remaining = scan.physical_bytes.saturating_sub(scan.next_offset);
        let access_grant = source.access_budget();
        let grant = fuel
            .min(access_grant)
            .min(DIRECT_SEGMENTED_LINE_WINDOW_BYTES)
            .min(remaining);
        let mut source_first_reads = 0;
        for _ in 0..grant {
            let offset = scan.next_offset;
            let byte = match source.read_byte(offset) {
                Ok(byte) => byte,
                Err(error) => {
                    self.stage = DirectSourceLineStage::Failed;
                    return Err(DirectSourceLinePollError::Source(error));
                }
            };
            if let Err(error) = scan.push(byte) {
                self.stage = DirectSourceLineStage::Failed;
                return Err(map_direct_segmented_scan_error(error));
            }
            source_first_reads += 1;
        }
        let physical_high_water = scan.next_offset;
        let source_budget_exhausted = grant == access_grant
            && access_grant <= fuel.min(DIRECT_SEGMENTED_LINE_WINDOW_BYTES)
            && scan.next_offset < scan.physical_bytes;
        if scan.next_offset == scan.physical_bytes {
            let line = match scan.complete() {
                Ok(line) => line,
                Err(error) => {
                    self.stage = DirectSourceLineStage::Failed;
                    return Err(map_direct_segmented_scan_error(error));
                }
            };
            let retained_source_bytes = line.controller_window.len();
            self.stage = DirectSourceLineStage::MatchedSegmented { line };
            return Ok(DirectSourceLinePollReceipt {
                status: DirectSourceLinePollStatus::Matched,
                lexical_work_units: source_first_reads,
                source_first_reads,
                physical_high_water,
                retained_source_bytes,
                source_budget_exhausted: false,
                maximum_source_request_rewind_bytes: 0,
            });
        }
        let retained_source_bytes = scan.retained_source_bytes();
        self.stage = DirectSourceLineStage::Segmented { scan };
        Ok(DirectSourceLinePollReceipt {
            status: DirectSourceLinePollStatus::NeedMore,
            lexical_work_units: source_first_reads,
            source_first_reads,
            physical_high_water,
            retained_source_bytes,
            source_budget_exhausted,
            maximum_source_request_rewind_bytes: 0,
        })
    }

    #[must_use]
    pub fn retained_source_bytes(&self) -> usize {
        match &self.stage {
            DirectSourceLineStage::Atx { scanner, .. }
            | DirectSourceLineStage::MatchedAtx { scanner, .. } => scanner.retained_source_bytes(),
            DirectSourceLineStage::Segmented { scan } => scan.retained_source_bytes(),
            DirectSourceLineStage::MatchedSegmented { line } => line.controller_window.len(),
            DirectSourceLineStage::Failed => 0,
        }
    }

    /// Logical generated-scanner lookahead already retained inside this work.
    /// Persistent source adapters may add it to their per-poll reported grant;
    /// it never authorizes an additional physical first read.
    #[must_use]
    pub fn logical_access_budget_slack(&self) -> usize {
        match &self.stage {
            DirectSourceLineStage::Atx { scanner, .. } => scanner.logical_access_budget_slack(),
            DirectSourceLineStage::Segmented { .. }
            | DirectSourceLineStage::MatchedAtx { .. }
            | DirectSourceLineStage::MatchedSegmented { .. }
            | DirectSourceLineStage::Failed => 0,
        }
    }
}

fn map_direct_segmented_scan_error<SourceError>(
    error: DirectSegmentedScanError,
) -> DirectSourceLinePollError<SourceError> {
    match error {
        DirectSegmentedScanError::InvalidUtf8 { absolute_offset } => {
            DirectSourceLinePollError::InvalidUtf8 { absolute_offset }
        }
        DirectSegmentedScanError::EmbeddedLineEnding { absolute_offset } => {
            DirectSourceLinePollError::EmbeddedLineEnding { absolute_offset }
        }
        DirectSegmentedScanError::MetricOverflow | DirectSegmentedScanError::Invariant => {
            DirectSourceLinePollError::ScannerInvariant
        }
    }
}

fn map_direct_source_line_poll_error<SourceError>(
    error: FusedAtxLineScanError<SourceError>,
) -> DirectSourceLinePollError<SourceError> {
    match error {
        FusedAtxLineScanError::ZeroFuel => DirectSourceLinePollError::ZeroFuel,
        FusedAtxLineScanError::WrongSource => DirectSourceLinePollError::WrongSource,
        FusedAtxLineScanError::Source(error) => DirectSourceLinePollError::Source(error),
        FusedAtxLineScanError::SourceContainsSentinel { absolute_offset } => {
            DirectSourceLinePollError::InvalidSourceByte { absolute_offset }
        }
        FusedAtxLineScanError::SourceBudgetContractViolated => {
            DirectSourceLinePollError::SourceBudgetContractViolated
        }
        FusedAtxLineScanError::PollAfterComplete => DirectSourceLinePollError::PollAfterComplete,
        FusedAtxLineScanError::PollAfterFailure => DirectSourceLinePollError::PollAfterFailure,
        FusedAtxLineScanError::Generated(CursorScanError::ZeroFuel) => {
            DirectSourceLinePollError::ZeroFuel
        }
        FusedAtxLineScanError::Generated(CursorScanError::WrongSource) => {
            DirectSourceLinePollError::WrongSource
        }
        FusedAtxLineScanError::Generated(CursorScanError::SourceContainsSentinel {
            absolute_offset,
        }) => DirectSourceLinePollError::InvalidSourceByte { absolute_offset },
        FusedAtxLineScanError::Generated(CursorScanError::PollAfterComplete) => {
            DirectSourceLinePollError::PollAfterComplete
        }
        FusedAtxLineScanError::Generated(CursorScanError::PollAfterFailure) => {
            DirectSourceLinePollError::PollAfterFailure
        }
        FusedAtxLineScanError::NonSequentialGeneratedRequest { .. }
        | FusedAtxLineScanError::UnboundedRejectionPrefix { .. }
        | FusedAtxLineScanError::PrefixInvariant
        | FusedAtxLineScanError::Tail(_)
        | FusedAtxLineScanError::LineCuts(_) => DirectSourceLinePollError::ScannerInvariant,
    }
}

#[derive(Debug)]
struct DirectFinishWork {
    transition: FinishTransition,
    semantic_complete: bool,
    finish_queued: bool,
}

/// First direct parser-to-writer proof slice.
///
/// This wrapper drives the same `LineTransition` and `FinishTransition`
/// functions as the unlimited parser. Output is intercepted at the actual
/// grammar mutation sites and exposed as a bounded stack protocol. The
/// supported slice currently covers Document, Paragraph, Setext Heading,
/// Quote, List, Item, and `FencedCode` blocks plus blank-line ownership. The
/// selected CommonMark/GFM profile is retained through restart; extensions
/// without a promoted direct protocol remain fail closed.
pub struct DirectValueBlockParser {
    parser: ValueBlockParser,
    line_work: Option<DirectLineWork>,
    finish_work: Option<DirectFinishWork>,
    line_complete: bool,
    finished: bool,
    source_line_instance_id: u64,
    next_source_line_admission: u64,
    active_source_line_admission: Option<u64>,
}

/// Parser-private proof representation for an acknowledged physical-line
/// boundary.
///
/// This is deliberately not a serialized product checkpoint. It contains only
/// the direct parser state needed to reproduce later commands; a real restart
/// additionally needs the writer's open bindings, deferred source ledger, and
/// generation capabilities.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectLineBoundaryPause {
    schema: u32,
    profile: SyntaxProfile,
    cursor: DirectPauseCursor,
    current_frame: usize,
    frames: Box<[DirectPauseFrame]>,
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
}

/// Result of asking the definitive parser for an optional restart sample.
///
/// [`Self::Unavailable`] is not a parse failure. It means the current valid
/// line boundary contains state that this restart codec cannot yet reproduce,
/// so a sparse-index consumer must simply omit that sample. Every malformed
/// parser state and codec invariant remains an ordinary [`ParseError`].
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use = "an optional restart sample must be retained or deliberately skipped"]
pub enum DirectLineBoundaryPauseCapture {
    Available(DirectLineBoundaryPause),
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectPauseCursor {
    line_number: usize,
    last_line_length: usize,
}

/// Current-source cursor scalars supplied when a durable semantic checkpoint
/// is installed into a fresh parser. They are independently measured from the
/// selected current-source prefix and deliberately do not enter the durable
/// donor bytes or checkpoint equality.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectLineBoundaryResumeCursor {
    line_number: usize,
    last_line_length: usize,
}

impl DirectLineBoundaryResumeCursor {
    /// Validate the current physical line ordinal and preceding logical line
    /// length supplied by the source-owning restart coordinator.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for an invalid/exhausted ordinal or a
    /// value that does not fit this target. Ordinal zero is reserved for the
    /// canonical Document-only restart at BOF and therefore requires a zero
    /// preceding-line length. This cursor stores only scalars, so its ordinary
    /// preceding-line length deliberately has no recognizer-window cap.
    pub fn new(line_ordinal: u64, last_line_length: u64) -> Result<Self, ParseError> {
        let line_number = usize::try_from(line_ordinal)
            .map_err(|_| ParseError::Invariant("direct rebound line ordinal fits usize"))?;
        let last_line_length = usize::try_from(last_line_length)
            .map_err(|_| ParseError::Invariant("direct rebound last-line length fits usize"))?;
        if line_number == usize::MAX || (line_number == 0 && last_line_length != 0) {
            return Err(ParseError::Invariant(
                "direct rebound line cursor is inside parser limits",
            ));
        }
        Ok(Self {
            line_number,
            last_line_length,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectPauseFrame {
    kind: DirectBlockKind,
    last_line_blank: bool,
    closed_children: crate::tree::ChildSequenceFold,
}

/// Current writer/green facts for one open frame at a restart cut.
///
/// The consumer supplies complete block display facts and the exact finalized
/// direct-child fold from its current output root. Per-line blankness is kept
/// out of this value so it can only enter through the deliberately narrow
/// [`DirectRestartLineLocalOutput`] seam.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectRestartFrameOutput {
    pub kind: DirectBlockKind,
    pub closed_children: crate::tree::ChildSequenceFold,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DirectDeferredState {
    terminator: bool,
    blank_gap: bool,
    /// Open-path depth, never a parser `NodeId`.
    blank_gap_floor: Option<usize>,
}

/// Donor-owned semantic state for the one provisional Paragraph transaction.
/// No writer group or block identity crosses this seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectPauseParagraphState {
    frame_depth: usize,
    has_visible_content: bool,
    may_have_reference_prefix: bool,
}

/// Exact direct-slice grammar/control equality at one physical-line boundary.
///
/// This is a necessary suffix-convergence key, not output or resume authority.
/// In particular, list display starts, item marker/padding decomposition,
/// heading display facts, blankness, and closed-child output folds are absent.
/// The composite source/writer/green coordinator remains responsible for
/// selecting the current [`DirectRestartOutput`].
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectGrammarContinuation {
    schema: u32,
    profile: SyntaxProfile,
    current_frame: usize,
    frames: Box<[DirectGrammarFrame]>,
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectGrammarFrame {
    kind: DirectGrammarKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectListMatchKey {
    Bullet { marker: u8 },
    Ordered { delimiter: ListDelimiter },
}

/// Variant-local future block-control projection for the supported direct
/// slice. Every omitted fact remains authoritative in `DirectRestartOutput`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectGrammarKind {
    Document,
    BlockQuote,
    List {
        match_key: DirectListMatchKey,
    },
    Item {
        effective_content_indent: u32,
        has_any_child: bool,
    },
    Paragraph,
    Heading,
    IndentedCode,
    FencedCode {
        fence: DirectFenceCharacter,
        minimum_closing_length: u64,
        fence_offset_columns: u8,
    },
    HtmlBlock {
        block_type: u8,
    },
}

/// Current-revision output/property half of an in-memory direct restart.
///
/// Each frame retains the complete donor recipe: [`DirectBlockKind`],
/// `last_line_blank`, and every [`crate::tree::ChildSequenceFold`] bit. Header,
/// deferred, and provisional-Paragraph fields are duplicated deliberately so
/// reconstruction can reject crossed joins before rebuilding parser scratch.
///
/// This opaque value carries no temporal authority by itself. The enclosing
/// source/writer/green checkpoint must select the current output root; donor
/// reconstruction consumes exactly the supplied output and never falls back
/// to an older accumulator merely because grammar is compatible.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectRestartOutput {
    schema: u32,
    profile: SyntaxProfile,
    current_frame: usize,
    frames: Box<[DirectPauseFrame]>,
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
}

/// Opaque donor continuation immediately after a leading reference-definition
/// prefix and before the surviving visible Paragraph remainder.
///
/// The reference rendezvous may mint this only from its committed
/// `VisibleRemainder` terminal. Source coordinates and writer/Green authority
/// deliberately remain consumer-owned and must be joined separately.
#[doc(hidden)]
#[must_use = "a leading-reference remainder continuation must be joined or discarded"]
pub struct DirectLeadingReferenceRemainderContinuation {
    grammar: DirectGrammarContinuation,
    output: DirectRestartOutput,
}

impl DirectLeadingReferenceRemainderContinuation {
    /// Consume the semantic continuation into the ordinary restart parts used
    /// by the direct parser's joined-resume path.
    #[doc(hidden)]
    #[must_use]
    pub fn into_restart_parts(self) -> (DirectGrammarContinuation, DirectRestartOutput) {
        (self.grammar, self.output)
    }
}

/// Opaque borrowed view of only the per-frame line-local blankness retained by
/// an existing donor restart sample.
///
/// This view is not proof that its bits belong to the current revision. A
/// composite coordinator may pass it to
/// [`DirectGrammarContinuation::bind_current_restart_output`] only after its
/// source lineage has established an identical stabilized physical line and
/// compatible open path. The donor can validate grammar and shape, but cannot
/// establish temporal currentness.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct DirectRestartLineLocalOutput<'a> {
    output: &'a DirectRestartOutput,
}

/// Opaque owned line-local half of a durable grammar checkpoint.
///
/// The bits are meaningful only inside the composite restart induction:
/// unchanged prefix lineage authorizes the selected restart sample through
/// `R`; the fresh parse supplies the transient convergence sample at `C`; and
/// grammar-compatible state at `C` plus an identical suffix authorizes old
/// samples strictly after `C`. Predecessor-byte identity alone is not general
/// authority because blankness can depend on opened-this-line and prior-child
/// control state. Cumulative child folds and display facts are deliberately
/// absent and must come from the current committed output root.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub struct DirectRestartLineLocalContinuation {
    schema: u32,
    profile: SyntaxProfile,
    current_frame: usize,
    last_line_blank: Box<[bool]>,
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
}

impl DirectRestartOutput {
    /// Heap payload retained by this opaque restart half. This excludes the
    /// inline `Self` bytes owned by the enclosing checkpoint allocation.
    #[doc(hidden)]
    #[must_use]
    pub fn allocated_bytes_for_diagnostics(&self) -> usize {
        self.frames
            .len()
            .saturating_mul(std::mem::size_of::<DirectPauseFrame>())
    }

    /// Borrow only the line-local output seam from this opaque restart sample.
    ///
    /// This does not authorize reuse. Identical-line stabilization and current
    /// source/output authority remain external composite obligations.
    #[doc(hidden)]
    #[must_use]
    pub const fn line_local_output(&self) -> DirectRestartLineLocalOutput<'_> {
        DirectRestartLineLocalOutput { output: self }
    }

    /// Necessary predecessor-line-local equality for suffix convergence.
    ///
    /// This deliberately ignores revision-cumulative child folds and display
    /// facts: those must be rebound from the current writer/Green path.  It is
    /// not temporal or source authority, and is useful only beside equal
    /// [`DirectGrammarContinuation`] values and an independently authenticated
    /// unchanged suffix.
    #[doc(hidden)]
    #[must_use]
    pub fn is_future_line_local_compatible(&self, other: &Self) -> bool {
        self.schema == other.schema
            && self.profile == other.profile
            && self.current_frame == other.current_frame
            && self.deferred == other.deferred
            && self.paragraph == other.paragraph
            && self.frames.len() == other.frames.len()
            && self
                .frames
                .iter()
                .zip(other.frames.iter())
                .all(|(left, right)| left.last_line_blank == right.last_line_blank)
    }
}

/// Opaque fixed-size sample state. The consumer may persist these bytes beside
/// one persistent-path root, but only this donor decodes their meaning.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectDurableLineBoundaryHeader {
    bytes: [u8; DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES],
}

impl DirectDurableLineBoundaryHeader {
    /// Reconstitute an opaque header read from storage while validating its
    /// version, reserved fields, and deterministic corruption checksum.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] unless `bytes` is exactly one
    /// canonical current-schema header.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let bytes: [u8; DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES] = bytes
            .try_into()
            .map_err(|_| ParseError::Invariant("direct durable header has fixed size"))?;
        let header = Self { bytes };
        let _ = decode_direct_durable_header(header)?;
        Ok(header)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES] {
        &self.bytes
    }
}

/// One opaque, fixed-size donor frame. Persistent storage may compare and
/// structurally share these bytes; it must not interpret their contents.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DirectDurableLineBoundaryFrameRecord {
    bytes: [u8; DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES],
}

impl DirectDurableLineBoundaryFrameRecord {
    /// Reconstitute one opaque frame read from persistent storage.
    /// Contextual path and aggregate-checksum validation occurs on resume.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] unless the record has the exact
    /// current schema and canonical reserved/fact encoding.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let bytes: [u8; DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES] = bytes
            .try_into()
            .map_err(|_| ParseError::Invariant("direct durable frame has fixed size"))?;
        let record = Self { bytes };
        let _ = decode_direct_durable_frame(record)?;
        Ok(record)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES] {
        &self.bytes
    }
}

/// Transient donor capture. Consumers insert the opaque records into one
/// persistent shared path and retain only `header` plus that path's root.
#[doc(hidden)]
#[derive(Debug)]
pub struct DirectDurableLineBoundaryCapture {
    header: DirectDurableLineBoundaryHeader,
    frames: Box<[DirectDurableLineBoundaryFrameRecord]>,
}

impl DirectDurableLineBoundaryCapture {
    #[must_use]
    pub const fn header(&self) -> DirectDurableLineBoundaryHeader {
        self.header
    }

    #[must_use]
    pub fn frame_records(
        &self,
    ) -> impl ExactSizeIterator<Item = DirectDurableLineBoundaryFrameRecord> + '_ {
        self.frames.iter().copied()
    }

    #[must_use]
    pub const fn receipt(&self) -> DirectDurableLineBoundaryCaptureReceipt {
        DirectDurableLineBoundaryCaptureReceipt {
            sample_header_bytes: DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES,
            materialized_path_records: self.frames.len(),
            materialized_path_bytes: self.frames.len() * DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES,
            retained_source_bytes: 0,
        }
    }

    /// Project a transient legacy full-output capture into the suffix-safe
    /// durable grammar-and-line-local contract. Revision-cumulative output is
    /// decoded and discarded inside the donor boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if the full capture is malformed or
    /// the grammar projection cannot be allocated or encoded.
    #[doc(hidden)]
    pub fn into_durable_grammar_capture(self) -> Result<DirectDurableGrammarCapture, ParseError> {
        let output =
            decode_direct_durable_restart_output(self.header, self.frames.iter().copied())?;
        let pause = DirectLineBoundaryPause {
            schema: output.schema,
            profile: output.profile,
            cursor: DirectPauseCursor {
                line_number: 1,
                last_line_length: 0,
            },
            current_frame: output.current_frame,
            frames: output.frames,
            deferred: output.deferred,
            paragraph: output.paragraph,
        };
        direct_pause_to_durable_grammar_capture(&pause)
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectDurableLineBoundaryCaptureReceipt {
    pub sample_header_bytes: usize,
    pub materialized_path_records: usize,
    pub materialized_path_bytes: usize,
    pub retained_source_bytes: usize,
}

/// Opaque fixed-size header for a durable grammar-and-line-local checkpoint.
/// Its wire magic/schema are disjoint from the legacy full-output recipe.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectDurableGrammarHeader {
    bytes: [u8; DIRECT_DURABLE_GRAMMAR_HEADER_BYTES],
}

impl DirectDurableGrammarHeader {
    /// Validate one persisted grammar header without exposing its fields.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for an unknown schema, noncanonical
    /// reserved field, invalid control scalar, or checksum mismatch.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let bytes: [u8; DIRECT_DURABLE_GRAMMAR_HEADER_BYTES] = bytes
            .try_into()
            .map_err(|_| ParseError::Invariant("direct durable grammar header has fixed size"))?;
        let header = Self { bytes };
        let _ = decode_direct_durable_grammar_header(header)?;
        Ok(header)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIRECT_DURABLE_GRAMMAR_HEADER_BYTES] {
        &self.bytes
    }
}

/// Opaque fixed-size open-path grammar record plus one line-local blank bit.
/// It contains no revision-cumulative child fold or display-only block fact.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DirectDurableGrammarFrameRecord {
    bytes: [u8; DIRECT_DURABLE_GRAMMAR_FRAME_BYTES],
}

impl DirectDurableGrammarFrameRecord {
    /// Validate one persisted grammar record's local encoding. Path order and
    /// the aggregate checksum are validated by the checkpoint decoder.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for an unknown kind/schema or a
    /// noncanonical flag, fact, or reserved field.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
        let bytes: [u8; DIRECT_DURABLE_GRAMMAR_FRAME_BYTES] = bytes
            .try_into()
            .map_err(|_| ParseError::Invariant("direct durable grammar frame has fixed size"))?;
        let record = Self { bytes };
        let _ = decode_direct_durable_grammar_frame(record)?;
        Ok(record)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIRECT_DURABLE_GRAMMAR_FRAME_BYTES] {
        &self.bytes
    }
}

/// Transient donor capture for suffix-persisted grammar and line-local output
/// only.
///
/// Authorization is deliberately external and role-specific: a selected
/// restart sample is covered by unchanged prefix lineage through `R`; the
/// convergence transaction uses its fresh capture at `C` and discards old-C
/// line-local state; old samples strictly after `C` are retainable only by the
/// deterministic grammar+line-local induction over the identical suffix.
#[doc(hidden)]
#[derive(Debug)]
pub struct DirectDurableGrammarCapture {
    header: DirectDurableGrammarHeader,
    frames: Box<[DirectDurableGrammarFrameRecord]>,
}

impl DirectDurableGrammarCapture {
    #[must_use]
    pub const fn header(&self) -> DirectDurableGrammarHeader {
        self.header
    }

    #[must_use]
    pub fn frame_records(
        &self,
    ) -> impl ExactSizeIterator<Item = DirectDurableGrammarFrameRecord> + '_ {
        self.frames.iter().copied()
    }

    #[must_use]
    pub const fn receipt(&self) -> DirectDurableGrammarCaptureReceipt {
        DirectDurableGrammarCaptureReceipt {
            sample_header_bytes: DIRECT_DURABLE_GRAMMAR_HEADER_BYTES,
            materialized_path_records: self.frames.len(),
            materialized_path_bytes: self.frames.len() * DIRECT_DURABLE_GRAMMAR_FRAME_BYTES,
            retained_source_bytes: 0,
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectDurableGrammarCaptureReceipt {
    pub sample_header_bytes: usize,
    pub materialized_path_records: usize,
    pub materialized_path_bytes: usize,
    pub retained_source_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
struct DirectDecodedDurableHeader {
    profile: SyntaxProfile,
    current_frame: usize,
    frame_count: usize,
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
    path_checksum: u64,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectLineBoundaryPauseReceipt {
    pub retained_open_frames: usize,
    pub estimated_owned_bytes: usize,
    pub retained_source_bytes: usize,
}

/// Read-only deferred-source shape used to pair the parser pause with the
/// writer/source-ledger half of one composite checkpoint.
///
/// This value is diagnostic validation input, never source or resume
/// authority. The enclosing pause remains the one consumed parser capability.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectLineBoundaryDeferredRole {
    None,
    Terminator,
    BlankGap { floor_depth: Option<usize> },
    Invalid,
}

const fn direct_line_boundary_deferred_role(
    deferred: DirectDeferredState,
) -> DirectLineBoundaryDeferredRole {
    match deferred {
        DirectDeferredState {
            terminator: true,
            blank_gap: false,
            blank_gap_floor: None,
        } => DirectLineBoundaryDeferredRole::Terminator,
        DirectDeferredState {
            terminator: false,
            blank_gap: true,
            blank_gap_floor,
        } => DirectLineBoundaryDeferredRole::BlankGap {
            floor_depth: blank_gap_floor,
        },
        DirectDeferredState {
            terminator: false,
            blank_gap: false,
            blank_gap_floor: None,
        } => DirectLineBoundaryDeferredRole::None,
        _ => DirectLineBoundaryDeferredRole::Invalid,
    }
}

/// Zero-copy observation of the fields that a storage-owned composite
/// checkpoint must cross-check against its writer continuation.
///
/// The view borrows an opaque pause and cannot construct or alter parser state.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct DirectLineBoundaryPairingView<'a> {
    pause: &'a DirectLineBoundaryPause,
}

impl<'a> DirectLineBoundaryPairingView<'a> {
    #[must_use]
    pub const fn profile(self) -> SyntaxProfile {
        self.pause.profile
    }

    #[must_use]
    pub const fn line_number(self) -> usize {
        self.pause.cursor.line_number
    }

    #[must_use]
    pub const fn last_line_length(self) -> usize {
        self.pause.cursor.last_line_length
    }

    #[must_use]
    pub const fn current_frame_depth(self) -> usize {
        self.pause.current_frame
    }

    #[must_use]
    pub fn open_frame_count(self) -> usize {
        self.pause.frames.len()
    }

    #[must_use]
    pub fn open_kinds(
        self,
    ) -> impl ExactSizeIterator<Item = DirectBlockKind> + DoubleEndedIterator + 'a {
        self.pause.frames.iter().map(|frame| frame.kind)
    }

    #[must_use]
    pub const fn deferred_role(self) -> DirectLineBoundaryDeferredRole {
        direct_line_boundary_deferred_role(self.pause.deferred)
    }
}

fn project_direct_grammar_continuation(
    schema: u32,
    profile: SyntaxProfile,
    current_frame: usize,
    frames: &[DirectPauseFrame],
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
) -> Result<DirectGrammarContinuation, ParseError> {
    let mut projected = Vec::new();
    projected
        .try_reserve_exact(frames.len())
        .map_err(|_| ParseError::Invariant("direct grammar path allocation failed"))?;
    for (index, frame) in frames.iter().enumerate() {
        let kind = match frame.kind {
            DirectBlockKind::Document => DirectGrammarKind::Document,
            DirectBlockKind::BlockQuote => DirectGrammarKind::BlockQuote,
            DirectBlockKind::List(facts) => DirectGrammarKind::List {
                match_key: match facts.list_type {
                    ListType::Bullet => DirectListMatchKey::Bullet {
                        marker: facts.bullet_char,
                    },
                    ListType::Ordered => DirectListMatchKey::Ordered {
                        delimiter: facts.delimiter,
                    },
                },
            },
            DirectBlockKind::Item(facts) => DirectGrammarKind::Item {
                effective_content_indent: u32::from(facts.marker_offset) + u32::from(facts.padding),
                has_any_child: frame.closed_children.had_child || index + 1 < frames.len(),
            },
            DirectBlockKind::Paragraph => DirectGrammarKind::Paragraph,
            DirectBlockKind::Heading(_) => DirectGrammarKind::Heading,
            DirectBlockKind::IndentedCode => DirectGrammarKind::IndentedCode,
            DirectBlockKind::FencedCode(facts) => DirectGrammarKind::FencedCode {
                fence: facts.fence,
                minimum_closing_length: facts.minimum_closing_length,
                fence_offset_columns: facts.fence_offset_columns,
            },
            DirectBlockKind::HtmlBlock(facts) => DirectGrammarKind::HtmlBlock {
                block_type: facts.block_type,
            },
            DirectBlockKind::ThematicBreak => {
                return Err(ParseError::Invariant(
                    "a thematic break never enters a grammar continuation",
                ));
            }
        };
        projected.push(DirectGrammarFrame { kind });
    }
    Ok(DirectGrammarContinuation {
        schema,
        profile,
        current_frame,
        frames: projected.into_boxed_slice(),
        deferred,
        paragraph,
    })
}

impl DirectGrammarContinuation {
    /// Heap payload retained by this opaque restart half. This excludes the
    /// inline `Self` bytes owned by the enclosing checkpoint allocation.
    #[doc(hidden)]
    #[must_use]
    pub fn allocated_bytes_for_diagnostics(&self) -> usize {
        self.frames
            .len()
            .saturating_mul(std::mem::size_of::<DirectGrammarFrame>())
    }

    /// Deferred source role encoded by this exact grammar continuation.
    ///
    /// A composite source ledger uses this read-only projection to reject a
    /// crossed restart recipe before binding current output. It is diagnostic
    /// validation input only: the enclosing grammar continuation remains the
    /// one consumed restart capability.
    #[doc(hidden)]
    #[must_use]
    pub const fn deferred_role(&self) -> DirectLineBoundaryDeferredRole {
        direct_line_boundary_deferred_role(self.deferred)
    }

    /// Necessary future grammar compatibility between two direct states.
    ///
    /// Equality does not establish suffix convergence, select an output
    /// revision, or authorize resume. Line-local blankness is deliberately
    /// outside this projection and can change commands when a later line
    /// closes unmatched containers. A convergence coordinator must compare
    /// the complete narrow durable codec (grammar plus line-local state) and
    /// independently provide current output.
    #[must_use]
    pub fn is_future_grammar_compatible(&self, other: &Self) -> bool {
        self == other
    }

    /// Bind current writer/green frame facts to line-local blankness from a
    /// separately stabilized donor sample.
    ///
    /// The returned output copies all donor-owned header, deferred-source, and
    /// provisional-Paragraph state from this grammar continuation. It copies
    /// only `last_line_blank` from `line_local`; ordered starts, heading levels,
    /// child folds, and every other output fact come exclusively from
    /// `current_frames`.
    ///
    /// This operation validates shape and grammar, not time. The enclosing
    /// composite coordinator must prove that `line_local` names an identical
    /// stabilized physical line in an authorized source lineage.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if the line-local sample is crossed or
    /// malformed, the current frame count is not exact, a supplied block kind
    /// is not donor-reachable, allocation fails, or the current output projects
    /// to different grammar.
    #[doc(hidden)]
    pub fn bind_current_restart_output<I>(
        &self,
        line_local: DirectRestartLineLocalOutput<'_>,
        current_frames: I,
    ) -> Result<DirectRestartOutput, ParseError>
    where
        I: IntoIterator<Item = DirectRestartFrameOutput>,
    {
        validate_direct_restart_output(self, line_local.output)?;
        bind_direct_current_restart_output(
            self,
            line_local
                .output
                .frames
                .iter()
                .map(|frame| frame.last_line_blank),
            current_frames,
        )
    }

    /// Bind current committed output to a lineage-authorized line-local half
    /// decoded from the durable grammar codec.
    ///
    /// This method validates grammar and shape only. The caller must consume
    /// the applicable composite induction proof (unchanged prefix through a
    /// selected restart, the fresh convergence transaction, or compatible
    /// convergence plus identical retained suffix) before passing
    /// `line_local`; the opaque value and one predecessor-line comparison are
    /// not temporal authority on their own.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for a crossed grammar/line-local pair,
    /// malformed blankness, a non-exact current frame path, allocation failure,
    /// or current output that projects to different grammar.
    #[doc(hidden)]
    pub fn bind_current_restart_output_from_stabilized_line<I>(
        &self,
        line_local: DirectRestartLineLocalContinuation,
        current_frames: I,
    ) -> Result<DirectRestartOutput, ParseError>
    where
        I: IntoIterator<Item = DirectRestartFrameOutput>,
    {
        validate_direct_restart_line_local(self, &line_local)?;
        bind_direct_current_restart_output(
            self,
            line_local.last_line_blank.iter().copied(),
            current_frames,
        )
    }
}

fn bind_direct_current_restart_output<I, L>(
    grammar: &DirectGrammarContinuation,
    line_local_blankness: L,
    current_frames: I,
) -> Result<DirectRestartOutput, ParseError>
where
    I: IntoIterator<Item = DirectRestartFrameOutput>,
    L: IntoIterator<Item = bool>,
{
    let mut current_frames = current_frames.into_iter();
    let mut line_local_blankness = line_local_blankness.into_iter();
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(grammar.frames.len())
        .map_err(|_| ParseError::Invariant("direct restart output path allocation failed"))?;
    for _ in &grammar.frames {
        let current = current_frames.next().ok_or(ParseError::Invariant(
            "direct current restart output has exact frame count",
        ))?;
        let last_line_blank = line_local_blankness.next().ok_or(ParseError::Invariant(
            "direct line-local restart output has exact frame count",
        ))?;
        frames.push(DirectPauseFrame {
            kind: current.kind,
            last_line_blank,
            closed_children: current.closed_children,
        });
    }
    if current_frames.next().is_some() || line_local_blankness.next().is_some() {
        return Err(ParseError::Invariant(
            "direct current and line-local restart paths have exact frame count",
        ));
    }

    let output = DirectRestartOutput {
        schema: grammar.schema,
        profile: grammar.profile,
        current_frame: grammar.current_frame,
        frames: frames.into_boxed_slice(),
        deferred: grammar.deferred,
        paragraph: grammar.paragraph,
    };
    validate_direct_restart_output(grammar, &output)?;
    Ok(output)
}

impl DirectLineBoundaryPause {
    #[must_use]
    pub fn receipt(&self) -> DirectLineBoundaryPauseReceipt {
        DirectLineBoundaryPauseReceipt {
            retained_open_frames: self.frames.len(),
            estimated_owned_bytes: std::mem::size_of::<Self>()
                + self.frames.len() * std::mem::size_of::<DirectPauseFrame>(),
            retained_source_bytes: 0,
        }
    }

    /// Borrows the minimum parser state needed to reject a mismatched writer
    /// continuation before either half is consumed for resume.
    #[doc(hidden)]
    #[must_use]
    pub const fn pairing_view(&self) -> DirectLineBoundaryPairingView<'_> {
        DirectLineBoundaryPairingView { pause: self }
    }

    /// Move one complete in-memory pause into grammar and current-output
    /// halves. The revision-local cursor is intentionally discarded and must
    /// be supplied independently when the parts are reconstructed.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if the projected grammar path cannot
    /// be allocated.
    #[doc(hidden)]
    pub fn into_restart_parts(
        self,
    ) -> Result<(DirectGrammarContinuation, DirectRestartOutput), ParseError> {
        let output = DirectRestartOutput {
            schema: self.schema,
            profile: self.profile,
            current_frame: self.current_frame,
            frames: self.frames,
            deferred: self.deferred,
            paragraph: self.paragraph,
        };
        direct_restart_output_into_parts(output)
    }

    /// Encode this already-authenticated transient pause into the donor-owned
    /// fixed durable representation. Cursor and writer authority remain
    /// external to the returned semantic payload.
    #[doc(hidden)]
    pub fn into_durable_line_boundary_capture(
        self,
    ) -> Result<DirectDurableLineBoundaryCapture, ParseError> {
        direct_pause_to_durable_capture(&self)
    }
}

fn direct_restart_output_into_parts(
    output: DirectRestartOutput,
) -> Result<(DirectGrammarContinuation, DirectRestartOutput), ParseError> {
    let grammar = project_direct_grammar_continuation(
        output.schema,
        output.profile,
        output.current_frame,
        &output.frames,
        output.deferred,
        output.paragraph,
    )?;
    validate_direct_restart_output(&grammar, &output)?;
    Ok((grammar, output))
}

fn validate_direct_pause_shape(
    schema: u32,
    _profile: SyntaxProfile,
    current_frame: usize,
    frames: &[DirectPauseFrame],
    deferred: DirectDeferredState,
    paragraph: Option<DirectPauseParagraphState>,
) -> Result<(), ParseError> {
    if schema != DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA
        || frames.is_empty()
        || u32::try_from(frames.len()).is_err()
        || current_frame.checked_add(1) != Some(frames.len())
        || frames[0].kind != DirectBlockKind::Document
    {
        return Err(ParseError::Invariant(
            "direct restart output header is valid",
        ));
    }
    if deferred
        .blank_gap_floor
        .is_some_and(|depth| depth >= frames.len())
        || (deferred.blank_gap_floor.is_some() && !deferred.blank_gap)
        || (deferred.terminator && deferred.blank_gap)
    {
        return Err(ParseError::Invariant(
            "direct restart output deferred state is valid",
        ));
    }

    let mut blank_frame = None;
    for (depth, frame) in frames.iter().enumerate() {
        if !frame.last_line_blank {
            continue;
        }
        if blank_frame.replace(depth).is_some()
            || depth != current_frame
            || !deferred.blank_gap
            || matches!(
                frame.kind,
                DirectBlockKind::BlockQuote
                    | DirectBlockKind::Paragraph
                    | DirectBlockKind::Heading(_)
                    | DirectBlockKind::FencedCode(_)
            )
        {
            return Err(ParseError::Invariant(
                "direct restart line-local blankness is donor-reachable",
            ));
        }
    }

    let mut parent_kind = None;
    for (depth, frame) in frames.iter().enumerate() {
        if depth > 0 && frame.kind == DirectBlockKind::Document {
            return Err(ParseError::Invariant(
                "direct restart output document is the root frame",
            ));
        }
        let kind = direct_pause_block_kind(frame.kind)?;
        if parent_kind
            .as_ref()
            .is_some_and(|parent: &BlockKind| !parent.can_contain(&kind))
        {
            return Err(ParseError::Invariant(
                "direct restart output frames form a valid open block path",
            ));
        }
        parent_kind = Some(kind);
    }

    let terminal_is_paragraph = frames
        .last()
        .is_some_and(|frame| frame.kind == DirectBlockKind::Paragraph);
    let terminal_is_indented_code = frames
        .last()
        .is_some_and(|frame| frame.kind == DirectBlockKind::IndentedCode);
    let paragraph_has_content = match (terminal_is_paragraph, paragraph) {
        (true, Some(paragraph))
            if paragraph.frame_depth == frames.len() - 1 && paragraph.has_visible_content =>
        {
            true
        }
        (false, None) => false,
        _ => {
            return Err(ParseError::Invariant(
                "direct restart output provisional Paragraph targets the terminal frame",
            ));
        }
    };
    if deferred.terminator && !(paragraph_has_content || terminal_is_indented_code) {
        return Err(ParseError::Invariant(
            "direct restart output terminator targets an open paragraph or indented code",
        ));
    }
    if let Some(depth) = deferred.blank_gap_floor
        && !matches!(
            frames[depth].kind,
            DirectBlockKind::BlockQuote | DirectBlockKind::Item(_)
        )
    {
        return Err(ParseError::Invariant(
            "direct restart output blank floor is a container marker owner",
        ));
    }
    Ok(())
}

fn validate_direct_grammar_shape(grammar: &DirectGrammarContinuation) -> Result<(), ParseError> {
    if grammar.schema != DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA
        || grammar.frames.is_empty()
        || u32::try_from(grammar.frames.len()).is_err()
        || grammar.current_frame.checked_add(1) != Some(grammar.frames.len())
        || grammar.frames[0].kind != DirectGrammarKind::Document
        || grammar
            .frames
            .iter()
            .skip(1)
            .any(|frame| frame.kind == DirectGrammarKind::Document)
    {
        return Err(ParseError::Invariant(
            "direct durable grammar header and root path are valid",
        ));
    }
    if grammar
        .deferred
        .blank_gap_floor
        .is_some_and(|depth| depth >= grammar.frames.len())
        || (grammar.deferred.blank_gap_floor.is_some() && !grammar.deferred.blank_gap)
        || (grammar.deferred.terminator && grammar.deferred.blank_gap)
    {
        return Err(ParseError::Invariant(
            "direct durable grammar deferred state is valid",
        ));
    }

    for (depth, frame) in grammar.frames.iter().enumerate() {
        match frame.kind {
            DirectGrammarKind::List {
                match_key: DirectListMatchKey::Bullet { marker },
            } if !matches!(marker, b'-' | b'+' | b'*') => {
                return Err(ParseError::Invariant(
                    "direct durable grammar bullet marker is donor-reachable",
                ));
            }
            DirectGrammarKind::Item {
                effective_content_indent,
                has_any_child,
            } if !(2..=17).contains(&effective_content_indent)
                || (depth + 1 < grammar.frames.len() && !has_any_child) =>
            {
                return Err(ParseError::Invariant(
                    "direct durable grammar item facts are donor-reachable",
                ));
            }
            DirectGrammarKind::FencedCode {
                minimum_closing_length,
                fence_offset_columns,
                ..
            } if minimum_closing_length < 3 || fence_offset_columns > 3 => {
                return Err(ParseError::Invariant(
                    "direct durable grammar fence facts are donor-reachable",
                ));
            }
            DirectGrammarKind::HtmlBlock { block_type } if !(1..=7).contains(&block_type) => {
                return Err(ParseError::Invariant(
                    "direct durable grammar HTML type is donor-reachable",
                ));
            }
            _ => {}
        }
        if depth != 0
            && !direct_grammar_kind_can_contain(grammar.frames[depth - 1].kind, frame.kind)
        {
            return Err(ParseError::Invariant(
                "direct durable grammar frames form a valid open block path",
            ));
        }
    }

    let terminal_is_paragraph = grammar
        .frames
        .last()
        .is_some_and(|frame| frame.kind == DirectGrammarKind::Paragraph);
    let terminal_is_indented_code = grammar
        .frames
        .last()
        .is_some_and(|frame| frame.kind == DirectGrammarKind::IndentedCode);
    let paragraph_has_content = match (terminal_is_paragraph, grammar.paragraph) {
        (true, Some(paragraph))
            if paragraph.frame_depth == grammar.frames.len() - 1
                && paragraph.has_visible_content =>
        {
            true
        }
        (false, None) => false,
        _ => {
            return Err(ParseError::Invariant(
                "direct durable grammar provisional Paragraph targets the terminal frame",
            ));
        }
    };
    if grammar.deferred.terminator && !(paragraph_has_content || terminal_is_indented_code) {
        return Err(ParseError::Invariant(
            "direct durable grammar terminator targets an open Paragraph or IndentedCode",
        ));
    }
    if let Some(depth) = grammar.deferred.blank_gap_floor
        && !matches!(
            grammar.frames[depth].kind,
            DirectGrammarKind::BlockQuote | DirectGrammarKind::Item { .. }
        )
    {
        return Err(ParseError::Invariant(
            "direct durable grammar blank floor is a container marker owner",
        ));
    }
    Ok(())
}

const fn direct_grammar_kind_can_contain(
    parent: DirectGrammarKind,
    child: DirectGrammarKind,
) -> bool {
    match parent {
        DirectGrammarKind::Document
        | DirectGrammarKind::BlockQuote
        | DirectGrammarKind::Item { .. } => !matches!(
            child,
            DirectGrammarKind::Document | DirectGrammarKind::Item { .. }
        ),
        DirectGrammarKind::List { .. } => matches!(child, DirectGrammarKind::Item { .. }),
        DirectGrammarKind::Paragraph
        | DirectGrammarKind::Heading
        | DirectGrammarKind::IndentedCode
        | DirectGrammarKind::FencedCode { .. }
        | DirectGrammarKind::HtmlBlock { .. } => false,
    }
}

fn validate_direct_restart_line_local(
    grammar: &DirectGrammarContinuation,
    line_local: &DirectRestartLineLocalContinuation,
) -> Result<(), ParseError> {
    validate_direct_grammar_shape(grammar)?;
    if line_local.schema != grammar.schema
        || line_local.profile != grammar.profile
        || line_local.current_frame != grammar.current_frame
        || line_local.last_line_blank.len() != grammar.frames.len()
        || line_local.deferred != grammar.deferred
        || line_local.paragraph != grammar.paragraph
    {
        return Err(ParseError::Invariant(
            "direct durable grammar and line-local headers match",
        ));
    }
    let mut blank_depth = None;
    for (depth, last_line_blank) in line_local.last_line_blank.iter().copied().enumerate() {
        if !last_line_blank {
            continue;
        }
        if blank_depth.replace(depth).is_some()
            || depth != grammar.current_frame
            || !grammar.deferred.blank_gap
            || matches!(
                grammar.frames[depth].kind,
                DirectGrammarKind::BlockQuote
                    | DirectGrammarKind::Paragraph
                    | DirectGrammarKind::Heading
                    | DirectGrammarKind::FencedCode { .. }
            )
        {
            return Err(ParseError::Invariant(
                "direct durable line-local blankness is donor-reachable",
            ));
        }
    }
    Ok(())
}

fn validate_direct_restart_output(
    grammar: &DirectGrammarContinuation,
    output: &DirectRestartOutput,
) -> Result<(), ParseError> {
    if grammar.schema != output.schema
        || grammar.profile != output.profile
        || grammar.current_frame != output.current_frame
        || grammar.frames.len() != output.frames.len()
        || grammar.deferred != output.deferred
        || grammar.paragraph != output.paragraph
    {
        return Err(ParseError::Invariant(
            "direct grammar and output restart headers match",
        ));
    }
    validate_direct_pause_shape(
        output.schema,
        output.profile,
        output.current_frame,
        &output.frames,
        output.deferred,
        output.paragraph,
    )?;
    let projected = project_direct_grammar_continuation(
        output.schema,
        output.profile,
        output.current_frame,
        &output.frames,
        output.deferred,
        output.paragraph,
    )?;
    if projected != *grammar {
        return Err(ParseError::Invariant(
            "direct current output projects to the supplied grammar continuation",
        ));
    }
    Ok(())
}

fn direct_restart_parts_into_pause(
    grammar: &DirectGrammarContinuation,
    output: DirectRestartOutput,
    cursor: DirectLineBoundaryResumeCursor,
) -> Result<DirectLineBoundaryPause, ParseError> {
    validate_direct_restart_output(grammar, &output)?;
    Ok(DirectLineBoundaryPause {
        schema: output.schema,
        profile: output.profile,
        cursor: DirectPauseCursor {
            line_number: cursor.line_number,
            last_line_length: cursor.last_line_length,
        },
        current_frame: output.current_frame,
        frames: output.frames,
        deferred: output.deferred,
        paragraph: output.paragraph,
    })
}

// Durable split-codec helpers live here so parser internals never escape to storage.
fn direct_durable_checksum(mut checksum: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        checksum ^= u64::from(*byte);
        checksum = checksum.wrapping_mul(DIRECT_DURABLE_CHECKSUM_PRIME);
    }
    checksum
}

fn direct_durable_read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed durable field is in bounds"),
    )
}

fn direct_durable_read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed durable field is in bounds"),
    )
}

fn direct_durable_write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn direct_durable_write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn encode_direct_durable_frame(frame: DirectPauseFrame) -> DirectDurableLineBoundaryFrameRecord {
    let (kind_tag, facts) = match frame.kind {
        DirectBlockKind::Document => (0, [0; 4]),
        DirectBlockKind::BlockQuote => (1, [0; 4]),
        DirectBlockKind::List(facts) => (
            2,
            [
                match facts.list_type {
                    ListType::Bullet => 0,
                    ListType::Ordered => 1,
                },
                u64::from(facts.start),
                match facts.delimiter {
                    ListDelimiter::Period => 0,
                    ListDelimiter::Paren => 1,
                },
                u64::from(facts.bullet_char),
            ],
        ),
        DirectBlockKind::Item(facts) => (
            3,
            [
                u64::from(facts.marker_offset),
                u64::from(facts.padding),
                match facts.task_checked {
                    None => 0,
                    Some(false) => 1,
                    Some(true) => 2,
                },
                0,
            ],
        ),
        DirectBlockKind::Paragraph => (4, [0; 4]),
        DirectBlockKind::Heading(facts) => {
            (5, [u64::from(facts.level), u64::from(facts.setext), 0, 0])
        }
        DirectBlockKind::FencedCode(facts) => (
            6,
            [
                match facts.fence {
                    DirectFenceCharacter::Backtick => 0,
                    DirectFenceCharacter::Tilde => 1,
                },
                facts.minimum_closing_length,
                u64::from(facts.fence_offset_columns),
                0,
            ],
        ),
        // The codec remains exhaustive even though pause validation rejects
        // this instantaneous kind before a durable record can be published.
        DirectBlockKind::ThematicBreak => (7, [0; 4]),
        DirectBlockKind::IndentedCode => (8, [0; 4]),
        DirectBlockKind::HtmlBlock(facts) => (9, [u64::from(facts.block_type), 0, 0, 0]),
    };
    let fold = frame.closed_children;
    let fold_flags = u8::from(fold.had_child)
        | (u8::from(fold.any_nonlast_child_ends_blank) << 1)
        | (u8::from(fold.last_child_ends_blank) << 2)
        | (u8::from(fold.list_loose_before_last) << 3)
        | (u8::from(fold.last_item_loose_if_nonlast) << 4)
        | (u8::from(fold.last_item_loose_if_last) << 5);
    let mut bytes = [0; DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES];
    direct_durable_write_u32(&mut bytes, 0, DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA);
    bytes[4] = kind_tag;
    bytes[5] = u8::from(frame.last_line_blank);
    bytes[6] = fold_flags;
    for (index, fact) in facts.into_iter().enumerate() {
        direct_durable_write_u64(&mut bytes, 8 + index * 8, fact);
    }
    DirectDurableLineBoundaryFrameRecord { bytes }
}

fn decode_direct_durable_frame(
    record: DirectDurableLineBoundaryFrameRecord,
) -> Result<DirectPauseFrame, ParseError> {
    let bytes = record.bytes;
    if direct_durable_read_u32(&bytes, 0) != DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA
        || bytes[5] > 1
        || bytes[6] & !0x3f != 0
        || bytes[7] != 0
        || bytes[40..].iter().any(|byte| *byte != 0)
    {
        return Err(ParseError::Invariant(
            "direct durable frame schema flags and reserved bytes are canonical",
        ));
    }
    let facts = [
        direct_durable_read_u64(&bytes, 8),
        direct_durable_read_u64(&bytes, 16),
        direct_durable_read_u64(&bytes, 24),
        direct_durable_read_u64(&bytes, 32),
    ];
    let [fact_0, fact_1, fact_2, fact_3] = facts;
    let kind = match bytes[4] {
        0 if facts == [0; 4] => DirectBlockKind::Document,
        1 if facts == [0; 4] => DirectBlockKind::BlockQuote,
        2 => DirectBlockKind::List(DirectListFacts {
            list_type: match fact_0 {
                0 => ListType::Bullet,
                1 => ListType::Ordered,
                _ => {
                    return Err(ParseError::Invariant(
                        "direct durable list type tag is known",
                    ));
                }
            },
            start: u32::try_from(fact_1)
                .map_err(|_| ParseError::Invariant("direct durable list start fits u32"))?,
            delimiter: match fact_2 {
                0 => ListDelimiter::Period,
                1 => ListDelimiter::Paren,
                _ => {
                    return Err(ParseError::Invariant(
                        "direct durable list delimiter tag is known",
                    ));
                }
            },
            bullet_char: u8::try_from(fact_3)
                .map_err(|_| ParseError::Invariant("direct durable bullet fits u8"))?,
        }),
        3 if fact_2 <= 2 && fact_3 == 0 => DirectBlockKind::Item(DirectItemFacts {
            marker_offset: u16::try_from(fact_0)
                .map_err(|_| ParseError::Invariant("direct durable item offset fits u16"))?,
            padding: u16::try_from(fact_1)
                .map_err(|_| ParseError::Invariant("direct durable item padding fits u16"))?,
            task_checked: match fact_2 {
                0 => None,
                1 => Some(false),
                2 => Some(true),
                _ => unreachable!(),
            },
        }),
        4 if facts == [0; 4] => DirectBlockKind::Paragraph,
        5 if fact_2 == 0 && fact_3 == 0 => DirectBlockKind::Heading(DirectHeadingFacts {
            level: u8::try_from(fact_0)
                .map_err(|_| ParseError::Invariant("direct durable heading level fits u8"))?,
            setext: match fact_1 {
                0 => false,
                1 => true,
                _ => {
                    return Err(ParseError::Invariant(
                        "direct durable heading setext flag is boolean",
                    ));
                }
            },
        }),
        6 if fact_3 == 0 => DirectBlockKind::FencedCode(DirectFencedCodeFacts {
            fence: match fact_0 {
                0 => DirectFenceCharacter::Backtick,
                1 => DirectFenceCharacter::Tilde,
                _ => {
                    return Err(ParseError::Invariant(
                        "direct durable fence marker tag is known",
                    ));
                }
            },
            minimum_closing_length: fact_1,
            fence_offset_columns: u8::try_from(fact_2)
                .map_err(|_| ParseError::Invariant("direct durable fence offset fits u8"))?,
        }),
        7 if facts == [0; 4] => DirectBlockKind::ThematicBreak,
        8 if facts == [0; 4] => DirectBlockKind::IndentedCode,
        9 if fact_1 == 0 && fact_2 == 0 && fact_3 == 0 => {
            DirectBlockKind::HtmlBlock(DirectHtmlBlockFacts {
                block_type: u8::try_from(fact_0)
                    .map_err(|_| ParseError::Invariant("direct durable HTML type fits u8"))?,
            })
        }
        _ => {
            return Err(ParseError::Invariant(
                "direct durable frame tag and facts are canonical",
            ));
        }
    };
    let fold_flags = bytes[6];
    Ok(DirectPauseFrame {
        kind,
        last_line_blank: bytes[5] != 0,
        closed_children: crate::tree::ChildSequenceFold {
            had_child: fold_flags & 1 != 0,
            any_nonlast_child_ends_blank: fold_flags & 2 != 0,
            last_child_ends_blank: fold_flags & 4 != 0,
            list_loose_before_last: fold_flags & 8 != 0,
            last_item_loose_if_nonlast: fold_flags & 16 != 0,
            last_item_loose_if_last: fold_flags & 32 != 0,
        },
    })
}

fn encode_direct_durable_grammar_frame(
    frame: DirectGrammarFrame,
    last_line_blank: bool,
) -> DirectDurableGrammarFrameRecord {
    let (kind_tag, has_any_child, facts) = match frame.kind {
        DirectGrammarKind::Document => (0, false, [0; 3]),
        DirectGrammarKind::BlockQuote => (1, false, [0; 3]),
        DirectGrammarKind::List { match_key } => match match_key {
            DirectListMatchKey::Bullet { marker } => (2, false, [0, u64::from(marker), 0]),
            DirectListMatchKey::Ordered { delimiter } => (
                2,
                false,
                [
                    1,
                    match delimiter {
                        ListDelimiter::Period => 0,
                        ListDelimiter::Paren => 1,
                    },
                    0,
                ],
            ),
        },
        DirectGrammarKind::Item {
            effective_content_indent,
            has_any_child,
        } => (
            3,
            has_any_child,
            [u64::from(effective_content_indent), 0, 0],
        ),
        DirectGrammarKind::Paragraph => (4, false, [0; 3]),
        DirectGrammarKind::Heading => (5, false, [0; 3]),
        DirectGrammarKind::FencedCode {
            fence,
            minimum_closing_length,
            fence_offset_columns,
        } => (
            6,
            false,
            [
                match fence {
                    DirectFenceCharacter::Backtick => 0,
                    DirectFenceCharacter::Tilde => 1,
                },
                minimum_closing_length,
                u64::from(fence_offset_columns),
            ],
        ),
        DirectGrammarKind::IndentedCode => (7, false, [0; 3]),
        DirectGrammarKind::HtmlBlock { block_type } => (8, false, [u64::from(block_type), 0, 0]),
    };
    let mut bytes = [0; DIRECT_DURABLE_GRAMMAR_FRAME_BYTES];
    direct_durable_write_u32(&mut bytes, 0, DIRECT_DURABLE_GRAMMAR_SCHEMA);
    bytes[4] = kind_tag;
    bytes[5] = u8::from(last_line_blank);
    bytes[6] = u8::from(has_any_child);
    for (index, fact) in facts.into_iter().enumerate() {
        direct_durable_write_u64(&mut bytes, 8 + index * 8, fact);
    }
    DirectDurableGrammarFrameRecord { bytes }
}

fn decode_direct_durable_grammar_frame(
    record: DirectDurableGrammarFrameRecord,
) -> Result<(DirectGrammarFrame, bool), ParseError> {
    let bytes = record.bytes;
    if direct_durable_read_u32(&bytes, 0) != DIRECT_DURABLE_GRAMMAR_SCHEMA
        || bytes[5] > 1
        || bytes[6] > 1
        || bytes[7] != 0
        || bytes[32..].iter().any(|byte| *byte != 0)
    {
        return Err(ParseError::Invariant(
            "direct durable grammar frame schema flags and reserved bytes are canonical",
        ));
    }
    let facts = [
        direct_durable_read_u64(&bytes, 8),
        direct_durable_read_u64(&bytes, 16),
        direct_durable_read_u64(&bytes, 24),
    ];
    let [fact_0, fact_1, fact_2] = facts;
    let has_any_child = bytes[6] != 0;
    let kind = match bytes[4] {
        0 if !has_any_child && facts == [0; 3] => DirectGrammarKind::Document,
        1 if !has_any_child && facts == [0; 3] => DirectGrammarKind::BlockQuote,
        2 if !has_any_child && fact_0 == 0 && fact_2 == 0 => {
            let marker = u8::try_from(fact_1)
                .map_err(|_| ParseError::Invariant("direct durable grammar bullet fits u8"))?;
            if !matches!(marker, b'-' | b'+' | b'*') {
                return Err(ParseError::Invariant(
                    "direct durable grammar bullet marker is known",
                ));
            }
            DirectGrammarKind::List {
                match_key: DirectListMatchKey::Bullet { marker },
            }
        }
        2 if !has_any_child && fact_0 == 1 && fact_2 == 0 => DirectGrammarKind::List {
            match_key: DirectListMatchKey::Ordered {
                delimiter: match fact_1 {
                    0 => ListDelimiter::Period,
                    1 => ListDelimiter::Paren,
                    _ => {
                        return Err(ParseError::Invariant(
                            "direct durable grammar list delimiter is known",
                        ));
                    }
                },
            },
        },
        3 if fact_1 == 0 && fact_2 == 0 => DirectGrammarKind::Item {
            effective_content_indent: u32::try_from(fact_0).map_err(|_| {
                ParseError::Invariant("direct durable grammar item indent fits u32")
            })?,
            has_any_child,
        },
        4 if !has_any_child && facts == [0; 3] => DirectGrammarKind::Paragraph,
        5 if !has_any_child && facts == [0; 3] => DirectGrammarKind::Heading,
        6 if !has_any_child => DirectGrammarKind::FencedCode {
            fence: match fact_0 {
                0 => DirectFenceCharacter::Backtick,
                1 => DirectFenceCharacter::Tilde,
                _ => {
                    return Err(ParseError::Invariant(
                        "direct durable grammar fence marker is known",
                    ));
                }
            },
            minimum_closing_length: fact_1,
            fence_offset_columns: u8::try_from(fact_2).map_err(|_| {
                ParseError::Invariant("direct durable grammar fence offset fits u8")
            })?,
        },
        7 if !has_any_child && facts == [0; 3] => DirectGrammarKind::IndentedCode,
        8 if !has_any_child && fact_1 == 0 && fact_2 == 0 => DirectGrammarKind::HtmlBlock {
            block_type: u8::try_from(fact_0)
                .map_err(|_| ParseError::Invariant("direct durable grammar HTML type fits u8"))?,
        },
        _ => {
            return Err(ParseError::Invariant(
                "direct durable grammar frame kind and facts are canonical",
            ));
        }
    };
    Ok((DirectGrammarFrame { kind }, bytes[5] != 0))
}

fn encode_direct_durable_header(
    pause: &DirectLineBoundaryPause,
    path_checksum: u64,
) -> Result<DirectDurableLineBoundaryHeader, ParseError> {
    if pause.schema != DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA {
        return Err(ParseError::Invariant(
            "direct durable pause uses the current donor schema",
        ));
    }
    let profile_tag = match pause.profile {
        SyntaxProfile::CommonMark => 0,
        SyntaxProfile::Gfm => 1,
    };
    let (deferred_tag, floor) = match pause.deferred {
        DirectDeferredState {
            terminator: false,
            blank_gap: false,
            blank_gap_floor: None,
        } => (0, u32::MAX),
        DirectDeferredState {
            terminator: true,
            blank_gap: false,
            blank_gap_floor: None,
        } => (1, u32::MAX),
        DirectDeferredState {
            terminator: false,
            blank_gap: true,
            blank_gap_floor,
        } => (
            2,
            blank_gap_floor
                .map(|depth| {
                    u32::try_from(depth)
                        .map_err(|_| ParseError::Invariant("direct blank floor fits u32"))
                })
                .transpose()?
                .unwrap_or(u32::MAX),
        ),
        _ => {
            return Err(ParseError::Invariant(
                "direct durable pause has one deferred source role",
            ));
        }
    };
    let (paragraph_flags, paragraph_depth) = match pause.paragraph {
        None => (0, u32::MAX),
        Some(paragraph) => (
            1 | (u8::from(paragraph.has_visible_content) << 1)
                | (u8::from(paragraph.may_have_reference_prefix) << 2),
            u32::try_from(paragraph.frame_depth)
                .map_err(|_| ParseError::Invariant("direct paragraph depth fits u32"))?,
        ),
    };
    let mut bytes = [0; DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES];
    bytes[..8].copy_from_slice(DIRECT_DURABLE_LINE_BOUNDARY_MAGIC);
    direct_durable_write_u32(&mut bytes, 8, DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA);
    direct_durable_write_u32(
        &mut bytes,
        12,
        u32::try_from(pause.frames.len())
            .map_err(|_| ParseError::Invariant("direct durable frame count fits u32"))?,
    );
    bytes[16] = profile_tag;
    bytes[17] = deferred_tag;
    bytes[18] = paragraph_flags;
    direct_durable_write_u32(
        &mut bytes,
        20,
        u32::try_from(pause.current_frame)
            .map_err(|_| ParseError::Invariant("direct current frame fits u32"))?,
    );
    direct_durable_write_u32(&mut bytes, 24, floor);
    direct_durable_write_u32(&mut bytes, 28, paragraph_depth);
    // Bytes 32..48 were the revision-local line ordinal and previous-line
    // length in schema 1. Schema 2 deliberately leaves them canonical zero:
    // the source-owning coordinator supplies those scalars when it rebinds
    // this coordinate-free continuation to a current source boundary.
    direct_durable_write_u64(&mut bytes, 48, path_checksum);
    let header_checksum = direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &bytes[..56]);
    direct_durable_write_u64(&mut bytes, 56, header_checksum);
    Ok(DirectDurableLineBoundaryHeader { bytes })
}

fn decode_direct_durable_header(
    header: DirectDurableLineBoundaryHeader,
) -> Result<DirectDecodedDurableHeader, ParseError> {
    let bytes = header.bytes;
    let stored_header_checksum = direct_durable_read_u64(&bytes, 56);
    if bytes[..8] != DIRECT_DURABLE_LINE_BOUNDARY_MAGIC[..]
        || direct_durable_read_u32(&bytes, 8) != DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA
        || bytes[19] != 0
        || bytes[32..48] != [0; 16]
        || direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &bytes[..56])
            != stored_header_checksum
    {
        return Err(ParseError::Invariant(
            "direct durable header schema reserved fields and checksum are valid",
        ));
    }
    let profile = match bytes[16] {
        0 => SyntaxProfile::CommonMark,
        1 => SyntaxProfile::Gfm,
        _ => {
            return Err(ParseError::Invariant(
                "direct durable header profile is supported by this schema",
            ));
        }
    };
    let frame_count = usize::try_from(direct_durable_read_u32(&bytes, 12))
        .map_err(|_| ParseError::Invariant("direct durable frame count fits usize"))?;
    let current_frame = usize::try_from(direct_durable_read_u32(&bytes, 20))
        .map_err(|_| ParseError::Invariant("direct current frame fits usize"))?;
    let floor = direct_durable_read_u32(&bytes, 24);
    let floor = (floor != u32::MAX)
        .then(|| {
            usize::try_from(floor)
                .map_err(|_| ParseError::Invariant("direct blank floor fits usize"))
        })
        .transpose()?;
    let deferred = match (bytes[17], floor) {
        (0, None) => DirectDeferredState::default(),
        (1, None) => DirectDeferredState {
            terminator: true,
            ..DirectDeferredState::default()
        },
        (2, floor) => DirectDeferredState {
            blank_gap: true,
            blank_gap_floor: floor,
            ..DirectDeferredState::default()
        },
        _ => {
            return Err(ParseError::Invariant(
                "direct durable header deferred state is canonical",
            ));
        }
    };
    let paragraph_depth = direct_durable_read_u32(&bytes, 28);
    let paragraph = match bytes[18] {
        0 if paragraph_depth == u32::MAX => None,
        flags @ (1 | 3 | 5 | 7) if paragraph_depth != u32::MAX => Some(DirectPauseParagraphState {
            frame_depth: usize::try_from(paragraph_depth)
                .map_err(|_| ParseError::Invariant("direct paragraph depth fits usize"))?,
            has_visible_content: flags & 2 != 0,
            may_have_reference_prefix: flags & 4 != 0,
        }),
        _ => {
            return Err(ParseError::Invariant(
                "direct durable header Paragraph state is canonical",
            ));
        }
    };
    if frame_count == 0
        || current_frame >= frame_count
        || floor.is_some_and(|depth| depth >= frame_count)
        || paragraph.is_some_and(|paragraph| paragraph.frame_depth >= frame_count)
    {
        return Err(ParseError::Invariant(
            "direct durable header scalar bounds are valid",
        ));
    }
    Ok(DirectDecodedDurableHeader {
        profile,
        current_frame,
        frame_count,
        deferred,
        paragraph,
        path_checksum: direct_durable_read_u64(&bytes, 48),
    })
}

fn encode_direct_durable_grammar_header(
    grammar: &DirectGrammarContinuation,
    path_checksum: u64,
) -> Result<DirectDurableGrammarHeader, ParseError> {
    validate_direct_grammar_shape(grammar)?;
    let profile_tag = match grammar.profile {
        SyntaxProfile::CommonMark => 0,
        SyntaxProfile::Gfm => 1,
    };
    let (deferred_tag, floor) = match grammar.deferred {
        DirectDeferredState {
            terminator: false,
            blank_gap: false,
            blank_gap_floor: None,
        } => (0, u32::MAX),
        DirectDeferredState {
            terminator: true,
            blank_gap: false,
            blank_gap_floor: None,
        } => (1, u32::MAX),
        DirectDeferredState {
            terminator: false,
            blank_gap: true,
            blank_gap_floor,
        } => (
            2,
            blank_gap_floor
                .map(|depth| {
                    u32::try_from(depth)
                        .map_err(|_| ParseError::Invariant("direct grammar blank floor fits u32"))
                })
                .transpose()?
                .unwrap_or(u32::MAX),
        ),
        _ => {
            return Err(ParseError::Invariant(
                "direct durable grammar has one deferred source role",
            ));
        }
    };
    let (paragraph_flags, paragraph_depth) = match grammar.paragraph {
        None => (0, u32::MAX),
        Some(paragraph) => (
            1 | (u8::from(paragraph.has_visible_content) << 1)
                | (u8::from(paragraph.may_have_reference_prefix) << 2),
            u32::try_from(paragraph.frame_depth)
                .map_err(|_| ParseError::Invariant("direct grammar paragraph depth fits u32"))?,
        ),
    };
    let mut bytes = [0; DIRECT_DURABLE_GRAMMAR_HEADER_BYTES];
    bytes[..8].copy_from_slice(DIRECT_DURABLE_GRAMMAR_MAGIC);
    direct_durable_write_u32(&mut bytes, 8, DIRECT_DURABLE_GRAMMAR_SCHEMA);
    direct_durable_write_u32(
        &mut bytes,
        12,
        u32::try_from(grammar.frames.len())
            .map_err(|_| ParseError::Invariant("direct grammar frame count fits u32"))?,
    );
    bytes[16] = profile_tag;
    bytes[17] = deferred_tag;
    bytes[18] = paragraph_flags;
    direct_durable_write_u32(
        &mut bytes,
        20,
        u32::try_from(grammar.current_frame)
            .map_err(|_| ParseError::Invariant("direct grammar current frame fits u32"))?,
    );
    direct_durable_write_u32(&mut bytes, 24, floor);
    direct_durable_write_u32(&mut bytes, 28, paragraph_depth);
    direct_durable_write_u64(&mut bytes, 48, path_checksum);
    let header_checksum = direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &bytes[..56]);
    direct_durable_write_u64(&mut bytes, 56, header_checksum);
    Ok(DirectDurableGrammarHeader { bytes })
}

fn decode_direct_durable_grammar_header(
    header: DirectDurableGrammarHeader,
) -> Result<DirectDecodedDurableHeader, ParseError> {
    let bytes = header.bytes;
    let stored_header_checksum = direct_durable_read_u64(&bytes, 56);
    if bytes[..8] != DIRECT_DURABLE_GRAMMAR_MAGIC[..]
        || direct_durable_read_u32(&bytes, 8) != DIRECT_DURABLE_GRAMMAR_SCHEMA
        || bytes[19] != 0
        || bytes[32..48] != [0; 16]
        || direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &bytes[..56])
            != stored_header_checksum
    {
        return Err(ParseError::Invariant(
            "direct durable grammar header schema reserved fields and checksum are valid",
        ));
    }
    let profile = match bytes[16] {
        0 => SyntaxProfile::CommonMark,
        1 => SyntaxProfile::Gfm,
        _ => {
            return Err(ParseError::Invariant(
                "direct durable grammar profile is supported",
            ));
        }
    };
    let frame_count = usize::try_from(direct_durable_read_u32(&bytes, 12))
        .map_err(|_| ParseError::Invariant("direct grammar frame count fits usize"))?;
    let current_frame = usize::try_from(direct_durable_read_u32(&bytes, 20))
        .map_err(|_| ParseError::Invariant("direct grammar current frame fits usize"))?;
    let floor = direct_durable_read_u32(&bytes, 24);
    let floor = (floor != u32::MAX)
        .then(|| {
            usize::try_from(floor)
                .map_err(|_| ParseError::Invariant("direct grammar blank floor fits usize"))
        })
        .transpose()?;
    let deferred = match (bytes[17], floor) {
        (0, None) => DirectDeferredState::default(),
        (1, None) => DirectDeferredState {
            terminator: true,
            ..DirectDeferredState::default()
        },
        (2, floor) => DirectDeferredState {
            blank_gap: true,
            blank_gap_floor: floor,
            ..DirectDeferredState::default()
        },
        _ => {
            return Err(ParseError::Invariant(
                "direct durable grammar deferred state is canonical",
            ));
        }
    };
    let paragraph_depth = direct_durable_read_u32(&bytes, 28);
    let paragraph = match bytes[18] {
        0 if paragraph_depth == u32::MAX => None,
        flags @ (1 | 3 | 5 | 7) if paragraph_depth != u32::MAX => Some(DirectPauseParagraphState {
            frame_depth: usize::try_from(paragraph_depth)
                .map_err(|_| ParseError::Invariant("direct grammar paragraph depth fits usize"))?,
            has_visible_content: flags & 2 != 0,
            may_have_reference_prefix: flags & 4 != 0,
        }),
        _ => {
            return Err(ParseError::Invariant(
                "direct durable grammar Paragraph state is canonical",
            ));
        }
    };
    if frame_count == 0
        || current_frame >= frame_count
        || floor.is_some_and(|depth| depth >= frame_count)
        || paragraph.is_some_and(|paragraph| paragraph.frame_depth >= frame_count)
    {
        return Err(ParseError::Invariant(
            "direct durable grammar header scalar bounds are valid",
        ));
    }
    Ok(DirectDecodedDurableHeader {
        profile,
        current_frame,
        frame_count,
        deferred,
        paragraph,
        path_checksum: direct_durable_read_u64(&bytes, 48),
    })
}

fn direct_pause_to_durable_capture(
    pause: &DirectLineBoundaryPause,
) -> Result<DirectDurableLineBoundaryCapture, ParseError> {
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(pause.frames.len())
        .map_err(|_| ParseError::Invariant("direct durable frame allocation failed"))?;
    let mut path_checksum = DIRECT_DURABLE_CHECKSUM_OFFSET;
    for frame in pause.frames.iter().copied() {
        let record = encode_direct_durable_frame(frame);
        path_checksum = direct_durable_checksum(path_checksum, record.as_bytes());
        frames.push(record);
    }
    let header = encode_direct_durable_header(pause, path_checksum)?;
    Ok(DirectDurableLineBoundaryCapture {
        header,
        frames: frames.into_boxed_slice(),
    })
}

fn direct_pause_to_durable_grammar_capture(
    pause: &DirectLineBoundaryPause,
) -> Result<DirectDurableGrammarCapture, ParseError> {
    validate_direct_pause_shape(
        pause.schema,
        pause.profile,
        pause.current_frame,
        &pause.frames,
        pause.deferred,
        pause.paragraph,
    )?;
    let grammar = project_direct_grammar_continuation(
        pause.schema,
        pause.profile,
        pause.current_frame,
        &pause.frames,
        pause.deferred,
        pause.paragraph,
    )?;
    validate_direct_grammar_shape(&grammar)?;

    let mut frames = Vec::new();
    frames
        .try_reserve_exact(grammar.frames.len())
        .map_err(|_| ParseError::Invariant("direct grammar path allocation failed"))?;
    let mut path_checksum = DIRECT_DURABLE_CHECKSUM_OFFSET;
    for (grammar_frame, output_frame) in grammar.frames.iter().zip(pause.frames.iter()) {
        let record =
            encode_direct_durable_grammar_frame(*grammar_frame, output_frame.last_line_blank);
        path_checksum = direct_durable_checksum(path_checksum, record.as_bytes());
        frames.push(record);
    }
    let header = encode_direct_durable_grammar_header(&grammar, path_checksum)?;
    Ok(DirectDurableGrammarCapture {
        header,
        frames: frames.into_boxed_slice(),
    })
}

fn decode_direct_durable_grammar_parts<I>(
    header: DirectDurableGrammarHeader,
    records: I,
) -> Result<
    (
        DirectGrammarContinuation,
        DirectRestartLineLocalContinuation,
    ),
    ParseError,
>
where
    I: IntoIterator<Item = DirectDurableGrammarFrameRecord>,
{
    let header = decode_direct_durable_grammar_header(header)?;
    let mut frames = Vec::new();
    let mut last_line_blank = Vec::new();
    frames
        .try_reserve_exact(header.frame_count)
        .map_err(|_| ParseError::Invariant("direct grammar path allocation failed"))?;
    last_line_blank
        .try_reserve_exact(header.frame_count)
        .map_err(|_| ParseError::Invariant("direct line-local path allocation failed"))?;
    let mut path_checksum = DIRECT_DURABLE_CHECKSUM_OFFSET;
    for record in records {
        if frames.len() == header.frame_count {
            return Err(ParseError::Invariant(
                "direct durable grammar path has exactly the stored frame count",
            ));
        }
        path_checksum = direct_durable_checksum(path_checksum, record.as_bytes());
        let (frame, blank) = decode_direct_durable_grammar_frame(record)?;
        frames.push(frame);
        last_line_blank.push(blank);
    }
    if frames.len() != header.frame_count || path_checksum != header.path_checksum {
        return Err(ParseError::Invariant(
            "direct durable grammar path count order and checksum match",
        ));
    }
    let grammar = DirectGrammarContinuation {
        schema: DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA,
        profile: header.profile,
        current_frame: header.current_frame,
        frames: frames.into_boxed_slice(),
        deferred: header.deferred,
        paragraph: header.paragraph,
    };
    let line_local = DirectRestartLineLocalContinuation {
        schema: DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA,
        profile: header.profile,
        current_frame: header.current_frame,
        last_line_blank: last_line_blank.into_boxed_slice(),
        deferred: header.deferred,
        paragraph: header.paragraph,
    };
    validate_direct_restart_line_local(&grammar, &line_local)?;
    Ok((grammar, line_local))
}

fn decode_direct_durable_restart_output<I>(
    header: DirectDurableLineBoundaryHeader,
    records: I,
) -> Result<DirectRestartOutput, ParseError>
where
    I: IntoIterator<Item = DirectDurableLineBoundaryFrameRecord>,
{
    let header = decode_direct_durable_header(header)?;
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(header.frame_count)
        .map_err(|_| ParseError::Invariant("direct durable frame allocation failed"))?;
    let mut path_checksum = DIRECT_DURABLE_CHECKSUM_OFFSET;
    for record in records {
        if frames.len() == header.frame_count {
            return Err(ParseError::Invariant(
                "direct durable path has exactly the stored frame count",
            ));
        }
        path_checksum = direct_durable_checksum(path_checksum, record.as_bytes());
        frames.push(decode_direct_durable_frame(record)?);
    }
    if frames.len() != header.frame_count || path_checksum != header.path_checksum {
        return Err(ParseError::Invariant(
            "direct durable path count order and checksum match",
        ));
    }
    Ok(DirectRestartOutput {
        schema: DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA,
        profile: header.profile,
        current_frame: header.current_frame,
        frames: frames.into_boxed_slice(),
        deferred: header.deferred,
        paragraph: header.paragraph,
    })
}

fn direct_durable_parts_into_pause<I>(
    header: DirectDurableLineBoundaryHeader,
    records: I,
    cursor: DirectLineBoundaryResumeCursor,
) -> Result<DirectLineBoundaryPause, ParseError>
where
    I: IntoIterator<Item = DirectDurableLineBoundaryFrameRecord>,
{
    let output = decode_direct_durable_restart_output(header, records)?;
    Ok(DirectLineBoundaryPause {
        schema: output.schema,
        profile: output.profile,
        cursor: DirectPauseCursor {
            line_number: cursor.line_number,
            last_line_length: cursor.last_line_length,
        },
        current_frame: output.current_frame,
        frames: output.frames,
        deferred: output.deferred,
        paragraph: output.paragraph,
    })
}

impl From<FacadeError> for ParseError {
    fn from(value: FacadeError) -> Self {
        Self::Facade(value)
    }
}

enum PrefixResult {
    Matched,
    Unmatched,
    Consumed,
}

pub struct ValueBlockParser {
    pub(crate) profile: SyntaxProfile,
    pub(crate) tree: BlockTree,
    pub(crate) references: Vec<ReferenceOccurrence>,
    pub(crate) current: NodeId,
    pub(crate) line_number: usize,
    pub(crate) line_leaf_id: u64,
    pub(crate) offset: usize,
    pub(crate) column: usize,
    pub(crate) thematic_break_kill_pos: usize,
    pub(crate) first_nonspace: usize,
    pub(crate) first_nonspace_column: usize,
    pub(crate) indent: usize,
    pub(crate) blank: bool,
    pub(crate) partially_consumed_tab: bool,
    pub(crate) curline_len: usize,
    pub(crate) curline_end_col: usize,
    pub(crate) last_line_length: usize,
    /// Continuation mode delegates historical-tree-only source-position
    /// repair to the write-only materializer, which owns the complete output.
    pub(crate) defer_output_repairs: bool,
    /// Ephemeral grammar fact for the physical line currently being parsed.
    /// Restored frames are necessarily older than the next pushed line.
    pub(crate) opened_this_line: HashSet<NodeId>,
    /// Present only for the direct stack-command driver. The ordinary parser
    /// keeps producing its exact legacy tree/events unchanged.
    direct: Option<DirectHooks>,
    /// Full physical metrics for the source-backed line currently being
    /// decided by `LineTransition`. The bounded text window remains owned by
    /// the direct wrapper; this value carries no grammar classification.
    direct_segmented_line: Option<DirectSegmentedLineFacts>,
    /// Test-only pre-refactor `OpenNew` scheduler. Grammar and scanner code is
    /// shared with the production coroutine; only the former atomic
    /// short-circuit call chain is retained as an equivalence oracle.
    #[cfg(test)]
    open_new_scheduler: OpenNewScheduler,
}

impl ValueBlockParser {
    fn new(profile: SyntaxProfile) -> Self {
        let tree = BlockTree::new();
        let current = tree.root;
        Self {
            profile,
            tree,
            references: Vec::new(),
            current,
            line_number: 0,
            line_leaf_id: 0,
            offset: 0,
            column: 0,
            thematic_break_kill_pos: 0,
            first_nonspace: 0,
            first_nonspace_column: 0,
            indent: 0,
            blank: false,
            partially_consumed_tab: false,
            curline_len: 0,
            curline_end_col: 0,
            last_line_length: 0,
            defer_output_repairs: false,
            opened_this_line: HashSet::new(),
            direct: None,
            direct_segmented_line: None,
            #[cfg(test)]
            open_new_scheduler: OpenNewScheduler::Resumable,
        }
    }

    fn begin_line_transition(&mut self, line: &str) -> LineTransition {
        self.opened_this_line.clear();
        let bytes = line.as_bytes();
        self.curline_len = line.len();
        self.curline_end_col = line.len();
        if self.curline_end_col > 0 && bytes[self.curline_end_col - 1] == b'\n' {
            self.curline_end_col -= 1;
        }
        if self.curline_end_col > 0 && bytes[self.curline_end_col - 1] == b'\r' {
            self.curline_end_col -= 1;
        }

        self.offset = 0;
        self.column = 0;
        self.first_nonspace = 0;
        self.first_nonspace_column = 0;
        self.indent = 0;
        self.thematic_break_kill_pos = 0;
        self.blank = false;
        self.partially_consumed_tab = false;
        if self.line_number == 0 && line.starts_with('\u{feff}') {
            self.offset += 3;
        }
        self.line_number += 1;

        LineTransition {
            phase: LinePhase::CheckOpen {
                container: self.tree.root,
            },
        }
    }

    fn direct_claim_initial_bom(&mut self) -> Result<(), ParseError> {
        if self.line_number != 1 || self.offset != '\u{feff}'.len_utf8() {
            return Ok(());
        }
        let root = self.tree.root;
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset != 0 {
            return Err(ParseError::Invariant(
                "initial BOM precedes all direct source claims",
            ));
        }
        direct.push_body(DirectIntent::Consume {
            owner: root,
            part: DirectCoveragePart::Gap,
            range: 0..u32::try_from(self.offset)
                .map_err(|_| ParseError::Invariant("BOM offset fits u32"))?,
            logical: DirectLogicalAction::None,
        })?;
        direct.claimed_offset = self.offset;
        Ok(())
    }

    fn step_line_transition(
        &mut self,
        transition: &mut LineTransition,
        line: &str,
        receipt: &mut WorkPollReceipt,
    ) -> Result<bool, ParseError> {
        receipt.transitions += 1;
        match transition.phase {
            LinePhase::CheckOpen { container } => {
                if !self.tree.last_child_is_open(container) {
                    self.enter_open_new(transition, container, true);
                    return Ok(false);
                }

                let child = self.tree.last_child(container).expect("open child exists");
                self.find_first_nonspace(line);
                let kind = self.tree.node(child).kind.clone();
                let result = match kind {
                    BlockKind::BlockQuote => {
                        if self.parse_block_quote_prefix(line, child)? {
                            PrefixResult::Matched
                        } else {
                            PrefixResult::Unmatched
                        }
                    }
                    BlockKind::Item(list) => self.parse_node_item_prefix(line, child, list)?,
                    BlockKind::CodeBlock { .. } => self.parse_code_block_prefix(line, child)?,
                    BlockKind::HtmlBlock { block_type, .. } => {
                        if self.parse_html_block_prefix(block_type) {
                            PrefixResult::Matched
                        } else {
                            PrefixResult::Unmatched
                        }
                    }
                    BlockKind::Paragraph => {
                        if self.blank {
                            PrefixResult::Unmatched
                        } else {
                            PrefixResult::Matched
                        }
                    }
                    BlockKind::Table(_) => {
                        if table::matches(&line[self.first_nonspace..])? {
                            PrefixResult::Matched
                        } else {
                            PrefixResult::Unmatched
                        }
                    }
                    BlockKind::Heading { .. }
                    | BlockKind::TableRow { .. }
                    | BlockKind::TableCell => PrefixResult::Unmatched,
                    _ => PrefixResult::Matched,
                };
                match result {
                    PrefixResult::Matched => {
                        transition.phase = LinePhase::CheckOpen { container: child };
                    }
                    PrefixResult::Unmatched => {
                        let parent = self
                            .tree
                            .parent(child)
                            .ok_or(ParseError::Invariant("unmatched container has parent"))?;
                        self.enter_open_new(transition, parent, false);
                    }
                    PrefixResult::Consumed => return Ok(self.complete_line_transition()),
                }
            }
            LinePhase::OpenNew(open) => {
                #[cfg(test)]
                if self.open_new_scheduler == OpenNewScheduler::LegacyAtomic {
                    return self.step_legacy_open_new_transition(transition, line, open);
                }
                return self.step_open_new_transition(transition, line, open);
            }
            LinePhase::PrepareText {
                container,
                last_matched_container,
            } => {
                self.find_first_nonspace(line);
                if self.blank && self.tree.has_any_child(container) {
                    self.tree.mark_last_child_line_blank(container);
                }
                let last_line_blank = self.blank
                    && match self.tree.node(container).kind {
                        BlockKind::BlockQuote
                        | BlockKind::Heading { .. }
                        | BlockKind::ThematicBreak => false,
                        BlockKind::CodeBlock { fenced, .. } => !fenced,
                        BlockKind::Item(_) => {
                            self.tree.has_any_child(container)
                                || !self.opened_this_line.contains(&container)
                        }
                        _ => true,
                    };
                self.tree.node_mut(container).last_line_blank = last_line_blank;
                transition.phase = LinePhase::ClearAncestors {
                    container,
                    last_matched_container,
                    ancestor: container,
                };
            }
            LinePhase::ClearAncestors {
                container,
                last_matched_container,
                ancestor,
            } => {
                receipt.index_operations += 1;
                if let Some(parent) = self.tree.parent(ancestor) {
                    self.tree.node_mut(parent).last_line_blank = false;
                    transition.phase = LinePhase::ClearAncestors {
                        container,
                        last_matched_container,
                        ancestor: parent,
                    };
                } else if self.current != last_matched_container
                    && container == last_matched_container
                    && !self.blank
                    && matches!(self.tree.node(self.current).kind, BlockKind::Paragraph)
                {
                    self.add_line(self.current, line)?;
                    return Ok(self.complete_line_transition());
                } else {
                    transition.phase = LinePhase::CloseUnmatched {
                        container,
                        last_matched_container,
                    };
                }
            }
            LinePhase::CloseUnmatched {
                container,
                last_matched_container,
            } => {
                if self.current != last_matched_container {
                    let discarded = self.current;
                    let current = self.finalize(discarded)?;
                    self.consume_reference_current_rebase(discarded, current)?;
                    self.current = current;
                } else {
                    transition.phase = LinePhase::DispatchText { container };
                }
            }
            LinePhase::DispatchText { mut container } => {
                match self.tree.node(container).kind.clone() {
                    BlockKind::CodeBlock { .. } => self.add_line(container, line)?,
                    BlockKind::HtmlBlock { block_type, .. } => {
                        self.add_line(container, line)?;
                        if html_block_end(block_type, &line[self.first_nonspace..])? {
                            container = self.finalize(container)?;
                        }
                    }
                    _ if self.blank => {}
                    _ if self.tree.node(container).kind.accepts_lines() => {
                        let mut effective_line = line;
                        if let BlockKind::Heading {
                            setext: false,
                            closed,
                            ..
                        } = &mut self.tree.node_mut(container).kind
                        {
                            if self.direct.is_none() {
                                let (chopped, was_closed) = chop_trailing_hashes(line)?;
                                effective_line = chopped;
                                *closed = was_closed;
                            }
                        }
                        let count = self.first_nonspace - self.offset;
                        if self.first_nonspace <= effective_line.len() {
                            self.advance_offset(effective_line, count, false);
                            self.add_line(container, effective_line)?;
                        }
                    }
                    _ => {
                        container = self.add_child(
                            container,
                            BlockKind::Paragraph,
                            self.first_nonspace + 1,
                        )?;
                        let count = self.first_nonspace - self.offset;
                        self.advance_offset(line, count, false);
                        self.add_line(container, line)?;
                    }
                }
                self.current = container;
                return Ok(self.complete_line_transition());
            }
        }
        Ok(false)
    }

    /// Advance one donor-owned block-opener subphase. `Start` performs only
    /// shared setup; every other stage calls one handler family at most once.
    fn step_open_new_transition(
        &mut self,
        transition: &mut LineTransition,
        line: &str,
        mut open: OpenNewTransition,
    ) -> Result<bool, ParseError> {
        if open.stage == OpenNewStage::Start {
            if matches!(
                self.tree.node(open.container).kind,
                BlockKind::CodeBlock { .. } | BlockKind::HtmlBlock { .. }
            ) {
                return Ok(self.settle_open_new_transition(transition, open));
            }

            open.depth += 1;
            self.find_first_nonspace(line);
            open.indented = self.indent >= CODE_INDENT;
            open.stage = if open.indented {
                OpenNewStage::List
            } else {
                OpenNewStage::BlockQuote
            };
            transition.phase = LinePhase::OpenNew(open);
            return Ok(false);
        }

        let (handled, next_stage) = match open.stage {
            OpenNewStage::Start => unreachable!("OpenNew Start returned after setup"),
            OpenNewStage::BlockQuote => (
                self.handle_blockquote(&mut open.container, line)?,
                Some(OpenNewStage::AtxHeading),
            ),
            OpenNewStage::AtxHeading => (
                self.handle_atx_heading(&mut open.container, line)?,
                Some(OpenNewStage::CodeFence),
            ),
            OpenNewStage::CodeFence => (
                self.handle_code_fence(&mut open.container, line)?,
                Some(OpenNewStage::HtmlBlock),
            ),
            OpenNewStage::HtmlBlock => (
                self.handle_html_block(&mut open.container, line)?,
                Some(OpenNewStage::SetextHeading),
            ),
            OpenNewStage::SetextHeading => (
                self.handle_setext_heading(&mut open.container, line)?,
                Some(OpenNewStage::ThematicBreak),
            ),
            OpenNewStage::ThematicBreak => (
                self.handle_thematic_break(&mut open.container, line, open.all_matched)?,
                Some(OpenNewStage::List),
            ),
            OpenNewStage::List => (
                self.handle_list(&mut open.container, line, open.indented, open.depth)?,
                Some(OpenNewStage::CodeBlock),
            ),
            OpenNewStage::CodeBlock => (
                self.handle_code_block(&mut open.container, line, open.indented, open.maybe_lazy)?,
                Some(OpenNewStage::Table),
            ),
            OpenNewStage::Table => (
                self.handle_table(&mut open.container, line, open.indented)?,
                None,
            ),
        };

        if let Some(rebase) = self
            .direct
            .as_mut()
            .and_then(|direct| direct.reference_current_rebase.take())
        {
            open.apply_reference_current_rebase(rebase)?;
        }

        if !handled {
            if let Some(next_stage) = next_stage {
                open.stage = next_stage;
                transition.phase = LinePhase::OpenNew(open);
                return Ok(false);
            }
            return Ok(self.settle_open_new_transition(transition, open));
        }

        if self.tree.node(open.container).kind.accepts_lines() {
            return Ok(self.settle_open_new_transition(transition, open));
        }

        open.maybe_lazy = false;
        open.indented = false;
        open.stage = OpenNewStage::Start;
        transition.phase = LinePhase::OpenNew(open);
        Ok(false)
    }

    fn settle_open_new_transition(
        &mut self,
        transition: &mut LineTransition,
        open: OpenNewTransition,
    ) -> bool {
        // `CheckOpen` owns the only line-consuming outcome (`PrefixResult::Consumed`).
        // Every completed opener scan therefore continues into text dispatch.
        // In particular, a reference-only Paragraph discard is an explicit
        // ownership rebase above, not an implicit `self.current` comparison.
        transition.phase = LinePhase::PrepareText {
            container: open.container,
            last_matched_container: open.last_matched_container,
        };
        false
    }

    /// Exact pre-refactor atomic short-circuit scheduler retained only as a
    /// test oracle. Handler and scanner implementations remain shared with the
    /// production coroutine above.
    #[cfg(test)]
    fn step_legacy_open_new_transition(
        &mut self,
        transition: &mut LineTransition,
        line: &str,
        mut open: OpenNewTransition,
    ) -> Result<bool, ParseError> {
        debug_assert_eq!(open.stage, OpenNewStage::Start);
        if matches!(
            self.tree.node(open.container).kind,
            BlockKind::CodeBlock { .. } | BlockKind::HtmlBlock { .. }
        ) {
            return Ok(self.settle_open_new_transition(transition, open));
        }

        open.depth += 1;
        self.find_first_nonspace(line);
        let indented = self.indent >= CODE_INDENT;
        let handled = (!indented
            && (self.handle_blockquote(&mut open.container, line)?
                || self.handle_atx_heading(&mut open.container, line)?
                || self.handle_code_fence(&mut open.container, line)?
                || self.handle_html_block(&mut open.container, line)?
                || self.handle_setext_heading(&mut open.container, line)?
                || self.handle_thematic_break(&mut open.container, line, open.all_matched)?))
            || self.handle_list(&mut open.container, line, indented, open.depth)?
            || self.handle_code_block(&mut open.container, line, indented, open.maybe_lazy)?
            || self.handle_table(&mut open.container, line, indented)?;
        if !handled || self.tree.node(open.container).kind.accepts_lines() {
            return Ok(self.settle_open_new_transition(transition, open));
        }

        open.maybe_lazy = false;
        open.indented = false;
        open.stage = OpenNewStage::Start;
        transition.phase = LinePhase::OpenNew(open);
        Ok(false)
    }

    fn enter_open_new(
        &self,
        transition: &mut LineTransition,
        container: NodeId,
        all_matched: bool,
    ) {
        transition.phase = LinePhase::OpenNew(OpenNewTransition {
            container,
            last_matched_container: container,
            all_matched,
            maybe_lazy: matches!(self.tree.node(self.current).kind, BlockKind::Paragraph),
            depth: 0,
            indented: false,
            stage: OpenNewStage::Start,
        });
    }

    fn complete_line_transition(&mut self) -> bool {
        self.last_line_length = self.curline_end_col;
        self.curline_len = 0;
        self.curline_end_col = 0;
        true
    }

    fn find_first_nonspace(&mut self, line: &str) {
        let mut chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
        let bytes = line.as_bytes();
        if self.first_nonspace <= self.offset {
            self.first_nonspace = self.offset;
            self.first_nonspace_column = self.column;
            loop {
                match bytes.get(self.first_nonspace) {
                    Some(b' ') => {
                        self.first_nonspace += 1;
                        self.first_nonspace_column += 1;
                        chars_to_tab -= 1;
                        if chars_to_tab == 0 {
                            chars_to_tab = TAB_STOP;
                        }
                    }
                    Some(b'\t') => {
                        self.first_nonspace += 1;
                        self.first_nonspace_column += chars_to_tab;
                        chars_to_tab = TAB_STOP;
                    }
                    _ => break,
                }
            }
        }
        self.indent = self.first_nonspace_column - self.column;
        self.blank = bytes
            .get(self.first_nonspace)
            .is_none_or(|byte| is_line_end_char(*byte));
    }

    fn parse_block_quote_prefix(
        &mut self,
        line: &str,
        container: NodeId,
    ) -> Result<bool, ParseError> {
        let bytes = line.as_bytes();
        if self.indent <= 3 && bytes.get(self.first_nonspace) == Some(&b'>') {
            let start = self.offset;
            self.advance_offset(line, self.indent + 1, true);
            if byte_matches(bytes, self.offset, is_space_or_tab) {
                self.advance_offset(line, 1, true);
            }
            let end = self.offset;
            if let Some(direct) = &mut self.direct {
                if direct.claimed_offset != start {
                    return Err(ParseError::Invariant(
                        "quote continuation follows the claimed prefix",
                    ));
                }
                direct.push_old_source(DirectIntent::Consume {
                    owner: container,
                    part: DirectCoveragePart::ContainerMarker,
                    range: u32::try_from(start)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                        ..u32::try_from(end)
                            .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                    logical: DirectLogicalAction::None,
                })?;
                direct.claimed_offset = end;
                direct.line_marker_floor = Some(container);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn is_greentext(&self, _line: &str) -> bool {
        false
    }

    fn parse_node_item_prefix(
        &mut self,
        line: &str,
        container: NodeId,
        list: ListData,
    ) -> Result<PrefixResult, ParseError> {
        if self.indent >= list.marker_offset + list.padding {
            let start = self.offset;
            self.advance_offset(line, list.marker_offset + list.padding, true);
            let end = self.offset;
            if let Some(direct) = &mut self.direct {
                if direct.claimed_offset != start {
                    return Err(ParseError::Invariant(
                        "item continuation follows the claimed prefix",
                    ));
                }
                direct.push_old_source(DirectIntent::Consume {
                    owner: container,
                    part: DirectCoveragePart::ContainerMarker,
                    range: u32::try_from(start)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                        ..u32::try_from(end)
                            .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                    logical: DirectLogicalAction::None,
                })?;
                direct.claimed_offset = end;
                direct.line_marker_floor = Some(container);
            }
            Ok(PrefixResult::Matched)
        } else if self.blank && self.tree.has_any_child(container) {
            let start = self.offset;
            let offset = self.first_nonspace - self.offset;
            self.advance_offset(line, offset, false);
            if let Some(direct) = &mut self.direct {
                if direct.claimed_offset != start {
                    return Err(ParseError::Invariant(
                        "blank item continuation follows the claimed prefix",
                    ));
                }
                if start < self.offset {
                    direct.push_old_source(DirectIntent::Consume {
                        owner: container,
                        part: DirectCoveragePart::ContainerMarker,
                        range: u32::try_from(start)
                            .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                            ..u32::try_from(self.offset)
                                .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                        logical: DirectLogicalAction::None,
                    })?;
                    direct.line_marker_floor = Some(container);
                }
                direct.claimed_offset = self.offset;
            }
            Ok(PrefixResult::Matched)
        } else {
            Ok(PrefixResult::Unmatched)
        }
    }

    fn parse_code_block_prefix(
        &mut self,
        line: &str,
        container: NodeId,
    ) -> Result<PrefixResult, ParseError> {
        let BlockKind::CodeBlock {
            fenced,
            fence_char,
            fence_length,
            fence_offset,
            ..
        } = self.tree.node(container).kind.clone()
        else {
            return Err(ParseError::Invariant("code prefix on code block"));
        };
        if !fenced {
            if self.indent >= CODE_INDENT {
                let start = self.offset;
                self.advance_offset(line, CODE_INDENT, true);
                if self.direct.is_some() {
                    self.direct_claim_indented_code_deindent(container, start)?;
                }
                return Ok(PrefixResult::Matched);
            }
            if self.blank {
                let start = self.offset;
                let offset = self.first_nonspace - self.offset;
                self.advance_offset(line, offset, false);
                if self.direct.is_some() {
                    self.direct_claim_indented_code_deindent(container, start)?;
                }
                return Ok(PrefixResult::Matched);
            }
            return Ok(PrefixResult::Unmatched);
        }

        let bytes = line.as_bytes();
        let matched = if self.indent <= 3 && bytes.get(self.first_nonspace) == Some(&fence_char) {
            close_code_fence(&line[self.first_nonspace..])?.unwrap_or(0)
        } else {
            0
        };
        if matched >= fence_length {
            if self.direct.is_some() {
                self.direct_claim_fenced_code_closer(container, line)?;
            }
            if let BlockKind::CodeBlock { closed, .. } = &mut self.tree.node_mut(container).kind {
                *closed = true;
            }
            self.advance_offset(line, matched, false);
            let _ = self.fix_zero_end_columns(container);
            self.current = self.finalize(container)?;
            return Ok(PrefixResult::Consumed);
        }
        let mut remaining = fence_offset;
        let marker_start = self.offset;
        while remaining > 0 && byte_matches(bytes, self.offset, is_space_or_tab) {
            self.advance_offset(line, 1, true);
            remaining -= 1;
        }
        if self.direct.is_some() {
            self.direct_claim_fenced_code_deindent(container, marker_start)?;
        }
        Ok(PrefixResult::Matched)
    }

    fn direct_claim_indented_code_deindent(
        &mut self,
        container: NodeId,
        start: usize,
    ) -> Result<(), ParseError> {
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset != start || start > self.offset {
            return Err(ParseError::Invariant(
                "indented-code deindent follows the claimed source prefix",
            ));
        }
        if start < self.offset {
            direct.push_old_source(DirectIntent::Consume {
                owner: container,
                part: DirectCoveragePart::BlockMarker,
                range: u32::try_from(start)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                // The parser consumed exactly the columns CommonMark removes
                // from this physical code line. Preserve that source as a
                // zero-width upstream projection so hosts never rediscover
                // tab stops or indentation grammar.
                logical: DirectLogicalAction::HiddenUpstream,
            })?;
        }
        direct.claimed_offset = self.offset;
        direct.line_marker_floor = Some(container);
        Ok(())
    }

    fn direct_claim_fenced_code_closer(
        &mut self,
        container: NodeId,
        line: &str,
    ) -> Result<(), ParseError> {
        let (content_end, ending) = direct_line_ending(line)?;
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset != self.offset || self.offset > content_end {
            return Err(ParseError::Invariant(
                "fence closer follows the claimed container prefix",
            ));
        }
        direct.push_old_source(DirectIntent::Consume {
            owner: container,
            part: DirectCoveragePart::BlockMarker,
            range: u32::try_from(self.offset)
                .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                ..u32::try_from(content_end)
                    .map_err(|_| ParseError::Invariant("direct line below u32"))?,
            logical: DirectLogicalAction::None,
        })?;
        if ending.is_some() {
            direct.push_old_source(DirectIntent::Consume {
                owner: container,
                part: DirectCoveragePart::Terminal,
                range: u32::try_from(content_end)
                    .map_err(|_| ParseError::Invariant("direct line below u32"))?
                    ..u32::try_from(line.len())
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::None,
            })?;
        }
        direct.claimed_offset = line.len();
        direct.line_marker_floor = Some(container);
        Ok(())
    }

    fn direct_claim_fenced_code_deindent(
        &mut self,
        container: NodeId,
        marker_start: usize,
    ) -> Result<(), ParseError> {
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset != marker_start {
            return Err(ParseError::Invariant(
                "fence body deindent follows the claimed container prefix",
            ));
        }
        if marker_start < self.offset {
            direct.push_old_source(DirectIntent::Consume {
                owner: container,
                part: DirectCoveragePart::ContainerMarker,
                range: u32::try_from(marker_start)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                logical: DirectLogicalAction::None,
            })?;
            direct.line_marker_floor = Some(container);
        }
        direct.claimed_offset = self.offset;
        Ok(())
    }

    fn parse_html_block_prefix(&self, block_type: u8) -> bool {
        match block_type {
            1..=5 => true,
            6 | 7 => !self.blank,
            _ => unreachable!("known HTML block type"),
        }
    }

    fn handle_blockquote(
        &mut self,
        container: &mut NodeId,
        line: &str,
    ) -> Result<bool, ParseError> {
        if !self.detect_blockquote(line) {
            return Ok(false);
        }
        let claim_start = self.offset;
        let start = self.first_nonspace;
        // `add_child` may pause to finalize a reference-bearing Paragraph.
        // Resolve that structural parent before advancing the line cursor so
        // replaying this coroutine stage after the rendezvous is idempotent.
        let child = self.add_child(*container, BlockKind::BlockQuote, start + 1)?;
        let offset = self.first_nonspace + 1 - self.offset;
        self.advance_offset(line, offset, false);
        if byte_matches(line.as_bytes(), self.offset, is_space_or_tab) {
            self.advance_offset(line, 1, true);
        }
        *container = child;
        if let Some(direct) = &mut self.direct {
            if direct.claimed_offset != claim_start {
                return Err(ParseError::Invariant(
                    "new quote follows the claimed prefix",
                ));
            }
            direct.push_body(DirectIntent::Consume {
                owner: *container,
                part: DirectCoveragePart::ContainerMarker,
                range: u32::try_from(claim_start)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                logical: DirectLogicalAction::None,
            })?;
            direct.claimed_offset = self.offset;
            direct.line_marker_floor = Some(*container);
        }
        Ok(true)
    }

    fn detect_blockquote(&self, line: &str) -> bool {
        line.as_bytes().get(self.first_nonspace) == Some(&b'>') && !self.is_greentext(line)
    }

    fn handle_atx_heading(
        &mut self,
        container: &mut NodeId,
        line: &str,
    ) -> Result<bool, ParseError> {
        let start = self.first_nonspace;
        let offset = self.offset;
        if self.direct.is_some() {
            let document_bom = self.line_number == 1
                && offset == '\u{feff}'.len_utf8()
                && line.starts_with('\u{feff}');
            let scan_start = if document_bom { 0 } else { offset };
            let Some(matched) = direct_atx_match_from_slice(
                &line[scan_start..],
                scan_start,
                self.column,
                document_bom,
            )?
            else {
                return Ok(false);
            };
            self.advance_offset(line, matched.opener_end - offset, false);
            *container = self.add_atx_heading(
                *container,
                matched.level,
                matched.closed,
                matched.opener_start + 1,
            )?;
            self.direct_claim_atx_heading_match(*container, matched.claim_start, matched)?;
            return Ok(true);
        }

        let Some(matched) = self.detect_atx_heading(line)? else {
            return Ok(false);
        };
        let bytes = line.as_bytes();
        let mut hash = start;
        while hash < bytes.len() && bytes[hash] == b'#' {
            hash += 1;
        }
        let level = u8::try_from(hash - start)
            .map_err(|_| ParseError::Invariant("ATX heading level fits u8"))?;
        if !(1..=6).contains(&level) {
            return Err(ParseError::Invariant(
                "ATX scanner returns a heading level from one through six",
            ));
        }
        self.advance_offset(line, start + matched - offset, false);
        *container = self.add_atx_heading(*container, level, false, start + 1)?;
        Ok(true)
    }

    fn add_atx_heading(
        &mut self,
        container: NodeId,
        level: u8,
        closed: bool,
        start_column: usize,
    ) -> Result<NodeId, ParseError> {
        if !(1..=6).contains(&level) {
            return Err(ParseError::Invariant(
                "ATX heading level is from one through six",
            ));
        }
        self.add_child(
            container,
            BlockKind::Heading {
                level,
                setext: false,
                closed,
            },
            start_column,
        )
    }

    fn direct_claim_atx_heading_match(
        &mut self,
        heading: NodeId,
        claim_start: usize,
        matched: DirectAtxMatch,
    ) -> Result<(), ParseError> {
        if matched.marker_end != matched.opener_end.min(matched.content_end)
            || matched.visible_end != matched.donor_chopped_end.max(matched.marker_end)
            || claim_start != matched.claim_start
            || claim_start > matched.marker_end
            || claim_start > matched.opener_start
            || matched.opener_start > matched.opener_end
            || matched.visible_end > matched.content_end
            || matched.content_end > matched.line_end
        {
            return Err(ParseError::Invariant(
                "direct ATX cuts form an ordered non-EOL partition",
            ));
        }
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset != claim_start {
            return Err(ParseError::Invariant(
                "direct ATX opener follows the claimed container prefix",
            ));
        }

        let mut push = |start: usize,
                        end: usize,
                        part: DirectCoveragePart,
                        logical: DirectLogicalAction|
         -> Result<(), ParseError> {
            if start == end {
                return Ok(());
            }
            direct.push_body(DirectIntent::Consume {
                owner: heading,
                part,
                range: u32::try_from(start)
                    .map_err(|_| ParseError::Invariant("direct ATX offset fits u32"))?
                    ..u32::try_from(end)
                        .map_err(|_| ParseError::Invariant("direct ATX offset fits u32"))?,
                logical,
            })?;
            direct.claimed_offset = end;
            Ok(())
        };

        push(
            claim_start,
            matched.marker_end,
            DirectCoveragePart::BlockMarker,
            DirectLogicalAction::None,
        )?;
        push(
            matched.marker_end,
            matched.visible_end,
            DirectCoveragePart::Content,
            DirectLogicalAction::CanonicalText,
        )?;
        push(
            matched.visible_end,
            matched.content_end,
            if matched.closed {
                DirectCoveragePart::BlockMarker
            } else {
                DirectCoveragePart::Content
            },
            if matched.closed {
                DirectLogicalAction::None
            } else {
                DirectLogicalAction::HiddenUpstream
            },
        )?;
        if matched.ending.is_some() {
            push(
                matched.content_end,
                matched.line_end,
                DirectCoveragePart::Terminal,
                DirectLogicalAction::None,
            )?;
        }
        Ok(())
    }

    fn detect_atx_heading(&self, line: &str) -> Result<Option<usize>, ParseError> {
        Ok(atx_heading_start(&line[self.first_nonspace..])?)
    }

    fn handle_code_fence(
        &mut self,
        container: &mut NodeId,
        line: &str,
    ) -> Result<bool, ParseError> {
        let Some(matched) = self.detect_code_fence(line)? else {
            return Ok(false);
        };
        let claim_start = self.offset;
        let first = self.first_nonspace;
        let offset = self.offset;
        *container = self.add_child(
            *container,
            BlockKind::CodeBlock {
                fenced: true,
                fence_char: line.as_bytes()[first],
                fence_length: matched,
                fence_offset: first - offset,
                info: LogicalProjection::default(),
                literal: LogicalProjection::default(),
                closed: false,
            },
            first + 1,
        )?;
        self.advance_offset(line, first + matched - offset, false);
        if let Some(direct) = &mut self.direct {
            if direct.claimed_offset != claim_start {
                return Err(ParseError::Invariant(
                    "new fence follows the claimed container prefix",
                ));
            }
            direct.push_body(DirectIntent::Consume {
                owner: *container,
                part: DirectCoveragePart::BlockMarker,
                range: u32::try_from(claim_start)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                logical: DirectLogicalAction::None,
            })?;
            direct.claimed_offset = self.offset;
            direct.line_marker_floor = Some(*container);
        }
        Ok(true)
    }

    fn detect_code_fence(&self, line: &str) -> Result<Option<usize>, ParseError> {
        Ok(open_code_fence(&line[self.first_nonspace..])?)
    }

    fn handle_html_block(
        &mut self,
        container: &mut NodeId,
        line: &str,
    ) -> Result<bool, ParseError> {
        let Some(block_type) = self.detect_html_block(*container, line)? else {
            return Ok(false);
        };
        *container = self.add_child(
            *container,
            BlockKind::HtmlBlock {
                block_type,
                literal: LogicalProjection::default(),
            },
            self.first_nonspace + 1,
        )?;
        Ok(true)
    }

    fn detect_html_block(&self, container: NodeId, line: &str) -> Result<Option<u8>, ParseError> {
        let allow_type_7 = !matches!(self.tree.node(container).kind, BlockKind::Paragraph);
        Ok(html_block_start(
            &line[self.first_nonspace..],
            allow_type_7,
        )?)
    }

    fn handle_setext_heading(
        &mut self,
        container: &mut NodeId,
        line: &str,
    ) -> Result<bool, ParseError> {
        let Some(kind) = self.detect_setext_heading(*container, line)? else {
            return Ok(false);
        };
        let level = match kind {
            FacadeSetextChar::Equals => 1,
            FacadeSetextChar::Hyphen => 2,
        };
        if self.direct.is_some() {
            // Stock donor resolves reference definitions against the already
            // completed Paragraph before deciding whether this underline can
            // promote it.  The underline itself is never finalizer lookahead.
            if self.direct_require_reference_finalizer(
                *container,
                DirectReferencePrefixContext::SetextCandidate,
            )? == DirectReferenceFinalizeResume::ReferenceOnly
            {
                self.direct_retain_reference_only_paragraph(*container)?;
                // A definitions-only prefix leaves an empty Paragraph shell.
                // CommonMark treats the candidate underline as literal text
                // in that same Paragraph, so claim the Setext opener as
                // handled and let PrepareText append the complete line.
                return Ok(true);
            }
            let (content_end, ending) = direct_line_ending(line)?;
            let direct = self
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            if !direct.paragraph_has_content {
                return Err(ParseError::Invariant(
                    "direct Setext requires visible Paragraph content",
                ));
            }
            if direct.claimed_offset != self.offset || self.offset > content_end {
                return Err(ParseError::Invariant(
                    "direct Setext begins at the exact unclaimed line suffix",
                ));
            }
            if direct.pending_terminator {
                direct.push_previous(DirectIntent::ResolveTerminator {
                    resolution: DirectTerminatorResolution::CloseNone,
                })?;
                direct.pending_terminator = false;
            }
            direct.push_body(DirectIntent::FinalizeParagraph {
                node: *container,
                outcome: DirectParagraphOutcome::SetextHeading { level },
            })?;
            if direct.claimed_offset < content_end {
                direct.push_body(DirectIntent::Consume {
                    owner: *container,
                    part: DirectCoveragePart::BlockMarker,
                    range: u32::try_from(direct.claimed_offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                        ..u32::try_from(content_end)
                            .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                    logical: DirectLogicalAction::None,
                })?;
                direct.claimed_offset = content_end;
            }
            if ending.is_some() {
                direct.push_body(DirectIntent::Consume {
                    owner: *container,
                    part: DirectCoveragePart::Terminal,
                    range: u32::try_from(content_end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?
                        ..u32::try_from(line.len())
                            .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                    logical: DirectLogicalAction::None,
                })?;
                direct.claimed_offset = line.len();
            }
            self.tree.node_mut(*container).kind = BlockKind::Heading {
                level,
                setext: true,
                closed: false,
            };
            let advance = content_end - self.offset;
            self.advance_offset(line, advance, false);
            return Ok(true);
        }
        let has_content = self.resolve_reference_link_definitions(*container)?;
        if has_content {
            self.tree.node_mut(*container).kind = BlockKind::Heading {
                level,
                setext: true,
                closed: false,
            };
            self.tree.events.push(BlockEvent::Promote {
                node: *container,
                from: "paragraph",
                to: "heading",
            });
            let advance = line.len() - newlines_of(line) - self.offset;
            self.advance_offset(line, advance, false);
        }
        Ok(true)
    }

    fn detect_setext_heading(
        &self,
        container: NodeId,
        line: &str,
    ) -> Result<Option<FacadeSetextChar>, ParseError> {
        if matches!(self.tree.node(container).kind, BlockKind::Paragraph) {
            Ok(setext_heading_line(&line[self.first_nonspace..])?)
        } else {
            Ok(None)
        }
    }

    fn handle_thematic_break(
        &mut self,
        container: &mut NodeId,
        line: &str,
        all_matched: bool,
    ) -> Result<bool, ParseError> {
        let Some(_) = self.detect_thematic_break(*container, line, all_matched) else {
            return Ok(false);
        };
        *container = self.add_child(
            *container,
            BlockKind::ThematicBreak,
            self.first_nonspace + 1,
        )?;
        let content_end = line.len() - newlines_of(line);
        let advance = content_end - self.offset;
        self.tree.node_mut(*container).source_end = Position::new(self.line_number, advance);
        if self.direct.is_some() {
            let leaf = *container;
            let direct = self
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            if direct.pending_terminator {
                // A thematic leaf cannot continue the previous paragraph's
                // logical line ending. Resolve that old deferred source before
                // emitting this line's instantaneous body recipe.
                direct.push_previous(DirectIntent::ResolveTerminator {
                    resolution: DirectTerminatorResolution::CloseNone,
                })?;
                direct.pending_terminator = false;
            }
            if direct.claimed_offset > content_end {
                return Err(ParseError::Invariant(
                    "direct thematic break starts after the line content cut",
                ));
            }
            if direct.claimed_offset < content_end {
                direct.push_body(DirectIntent::Consume {
                    owner: leaf,
                    part: DirectCoveragePart::BlockMarker,
                    range: u32::try_from(direct.claimed_offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                        ..u32::try_from(content_end)
                            .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                    logical: DirectLogicalAction::None,
                })?;
                direct.claimed_offset = content_end;
            }
            if content_end < line.len() {
                direct.push_body(DirectIntent::Consume {
                    owner: leaf,
                    part: DirectCoveragePart::Terminal,
                    range: u32::try_from(content_end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?
                        ..u32::try_from(line.len())
                            .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                    logical: DirectLogicalAction::None,
                })?;
                direct.claimed_offset = line.len();
            }
        }
        self.advance_offset(line, advance, false);
        if self.direct.is_some() {
            // A thematic break has no continuation state. Closing it in the
            // same recipe prevents a leaf from leaking into the durable open
            // path while retaining the donor's precedence decision.
            *container = self.finalize(*container)?;
        }
        Ok(true)
    }

    fn detect_thematic_break(
        &mut self,
        container: NodeId,
        line: &str,
        all_matched: bool,
    ) -> Option<usize> {
        if !(matches!(self.tree.node(container).kind, BlockKind::Paragraph) && !all_matched)
            && self.thematic_break_kill_pos <= self.first_nonspace
        {
            let (offset, found) = self.scan_thematic_break_inner(line);
            if found {
                Some(offset)
            } else {
                self.thematic_break_kill_pos = offset;
                None
            }
        } else {
            None
        }
    }

    fn scan_thematic_break_inner(&self, line: &str) -> (usize, bool) {
        let mut index = self.first_nonspace;
        if index >= line.len() {
            return (index, false);
        }
        let bytes = line.as_bytes();
        let marker = bytes[index];
        if !matches!(marker, b'*' | b'_' | b'-') {
            return (index, false);
        }
        let mut count = 1;
        let next;
        loop {
            index += 1;
            if index >= line.len() {
                next = 255;
                break;
            }
            let candidate = bytes[index];
            if candidate == marker {
                count += 1;
            } else if !matches!(candidate, b' ' | b'\t') {
                next = candidate;
                break;
            }
        }
        if count >= 3 && matches!(next, 255 | b'\r' | b'\n') {
            ((index - self.first_nonspace) + 1, true)
        } else {
            (index, false)
        }
    }

    fn handle_list(
        &mut self,
        container: &mut NodeId,
        line: &str,
        indented: bool,
        depth: usize,
    ) -> Result<bool, ParseError> {
        let Some((matched, mut list)) = self.detect_list(*container, line, indented, depth) else {
            return Ok(false);
        };
        let needs_list = match &self.tree.node(*container).kind {
            BlockKind::List(existing) => !lists_match(&list, &existing),
            _ => true,
        };
        // Closing an open Paragraph can pause for reference-prefix work. Do it
        // before advancing the line cursor so replaying this coroutine stage
        // remains idempotent after the rendezvous.
        if needs_list {
            while !self
                .tree
                .node(*container)
                .kind
                .can_contain(&BlockKind::List(list))
            {
                *container = self.finalize(*container)?;
            }
        }
        let claim_start = self.offset;
        let offset = self.first_nonspace + matched - self.offset;
        self.advance_offset(line, offset, false);
        let saved = (self.partially_consumed_tab, self.offset, self.column);
        let bytes = line.as_bytes();
        while self.column - saved.2 <= 5 && byte_matches(bytes, self.offset, is_space_or_tab) {
            self.advance_offset(line, 1, true);
        }
        let width = self.column - saved.2;
        if !(1..5).contains(&width) || byte_matches(bytes, self.offset, is_line_end_char) {
            list.padding = matched + 1;
            (self.partially_consumed_tab, self.offset, self.column) = saved;
            if width > 0 {
                self.advance_offset(line, 1, true);
            }
        } else {
            list.padding = matched + width;
        }
        list.marker_offset = self.indent;

        let task_checked = if self.profile == SyntaxProfile::Gfm {
            task_list_marker(&line[self.offset..])?.map(|marker| {
                self.advance_offset(line, marker.consumed_bytes, false);
                marker.checked
            })
        } else {
            None
        };

        if needs_list {
            let mut list_container = list;
            list_container.task_checked = None;
            *container = self.add_child(
                *container,
                BlockKind::List(list_container),
                self.first_nonspace + 1,
            )?;
        }
        list.task_checked = task_checked;
        *container = self.add_child(*container, BlockKind::Item(list), self.first_nonspace + 1)?;
        if let Some(direct) = &mut self.direct {
            if direct.claimed_offset != claim_start {
                return Err(ParseError::Invariant(
                    "new list item follows the claimed prefix",
                ));
            }
            direct.push_body(DirectIntent::Consume {
                owner: *container,
                part: DirectCoveragePart::ContainerMarker,
                range: u32::try_from(claim_start)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                logical: DirectLogicalAction::None,
            })?;
            direct.claimed_offset = self.offset;
            direct.line_marker_floor = Some(*container);
        }
        Ok(true)
    }

    fn detect_list(
        &self,
        container: NodeId,
        line: &str,
        indented: bool,
        depth: usize,
    ) -> Option<(usize, ListData)> {
        if (!indented || matches!(self.tree.node(container).kind, BlockKind::List(_)))
            && self.indent < 4
            && depth < MAX_LIST_DEPTH
        {
            parse_list_marker(
                line,
                self.first_nonspace,
                matches!(self.tree.node(container).kind, BlockKind::Paragraph),
            )
        } else {
            None
        }
    }

    fn handle_code_block(
        &mut self,
        container: &mut NodeId,
        line: &str,
        indented: bool,
        maybe_lazy: bool,
    ) -> Result<bool, ParseError> {
        if !self.detect_code_block(indented, maybe_lazy) {
            return Ok(false);
        }
        let claim_start = self.offset;
        self.advance_offset(line, CODE_INDENT, true);
        *container = self.add_child(
            *container,
            BlockKind::CodeBlock {
                fenced: false,
                fence_char: 0,
                fence_length: 0,
                fence_offset: 0,
                info: LogicalProjection::default(),
                literal: LogicalProjection::default(),
                closed: true,
            },
            self.offset + 1,
        )?;
        if let Some(direct) = &mut self.direct {
            if direct.claimed_offset != claim_start {
                return Err(ParseError::Invariant(
                    "new indented code follows the claimed source prefix",
                ));
            }
            direct.push_body(DirectIntent::Consume {
                owner: *container,
                part: DirectCoveragePart::BlockMarker,
                range: u32::try_from(claim_start)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                logical: DirectLogicalAction::None,
            })?;
            direct.claimed_offset = self.offset;
            direct.line_marker_floor = Some(*container);
        }
        Ok(true)
    }

    fn detect_code_block(&self, indented: bool, maybe_lazy: bool) -> bool {
        indented && !maybe_lazy && !self.blank
    }

    fn handle_table(
        &mut self,
        container: &mut NodeId,
        line: &str,
        indented: bool,
    ) -> Result<bool, ParseError> {
        let Some(opening) = self.detect_table(*container, line, indented)? else {
            return Ok(false);
        };
        if opening.replace {
            self.tree.insert_after(*container, opening.container);
            self.tree.detach(*container);
            *container = opening.container;
        } else {
            *container = opening.container;
        }
        if opening.mark_visited {
            self.tree.node_mut(*container).table_visited = true;
        }
        Ok(true)
    }

    fn detect_table(
        &mut self,
        container: NodeId,
        line: &str,
        indented: bool,
    ) -> Result<Option<table::TableOpening>, ParseError> {
        if !indented && self.profile == SyntaxProfile::Gfm {
            table::try_opening_block(self, container, line)
        } else {
            Ok(None)
        }
    }

    pub(crate) fn advance_offset(&mut self, line: &str, mut count: usize, columns: bool) {
        let bytes = line.as_bytes();
        while count > 0 {
            match bytes[self.offset] {
                b'\t' => {
                    let chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
                    if columns {
                        self.partially_consumed_tab = chars_to_tab > count;
                        let advance = min(count, chars_to_tab);
                        self.column += advance;
                        if !self.partially_consumed_tab {
                            self.offset += 1;
                        }
                        count -= advance;
                    } else {
                        self.partially_consumed_tab = false;
                        self.column += chars_to_tab;
                        self.offset += 1;
                        count -= 1;
                    }
                }
                _ => {
                    self.partially_consumed_tab = false;
                    self.offset += 1;
                    self.column += 1;
                    count -= 1;
                }
            }
        }
    }

    pub(crate) fn add_child(
        &mut self,
        mut parent: NodeId,
        kind: BlockKind,
        start_column: usize,
    ) -> Result<NodeId, ParseError> {
        while !self.tree.node(parent).kind.can_contain(&kind) {
            parent = self.finalize(parent)?;
        }
        if start_column == 0 {
            return Err(ParseError::Invariant("block start column is one-based"));
        }
        let direct_kind = self
            .direct
            .as_ref()
            .map(|_| direct_block_kind(&kind))
            .transpose()?;
        let start = Position::new(self.line_number, start_column);
        let child = if self.direct.is_some() {
            self.tree.append_scratch(parent, kind, start)
        } else {
            self.tree.append(parent, kind, start)
        };
        self.opened_this_line.insert(child);
        if let (Some(direct), Some(kind)) = (&mut self.direct, direct_kind) {
            direct.push_body(DirectIntent::Open { node: child, kind })?;
            if kind == DirectBlockKind::Paragraph {
                direct.paragraph_has_content = false;
            }
        }
        Ok(child)
    }

    pub(crate) fn add_line(&mut self, node: NodeId, line: &str) -> Result<(), ParseError> {
        if self.direct.is_some() {
            return self.direct_add_line(node, line);
        }
        let source_backed = matches!(
            self.tree.node(node).kind,
            BlockKind::CodeBlock { .. } | BlockKind::HtmlBlock { .. }
        );
        let logical_start = self.tree.node(node).content.logical_len();
        let origin_start = self.tree.node(node).content.origins.len();
        let line_offsets_start = self.tree.node(node).content.line_offsets.len();
        let original_offset = self.offset;
        if source_backed {
            self.tree.node_mut(node).content.ensure_source_backed();
            if self.partially_consumed_tab {
                self.offset += 1;
            }
            if self.partially_consumed_tab || self.offset < line.len() {
                let line_start = self
                    .tree
                    .node_mut(node)
                    .content
                    .start_source_backed_line(self.offset);
                if self.partially_consumed_tab {
                    let chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
                    self.tree.node_mut(node).content.push_source_backed(
                        self.line_leaf_id,
                        original_offset..self.offset,
                        chars_to_tab,
                        OriginTransform::TabExpansion,
                    );
                }
                if self.offset < line.len() {
                    self.tree.node_mut(node).content.push_source_backed(
                        self.line_leaf_id,
                        self.offset..line.len(),
                        line.len() - self.offset,
                        OriginTransform::Identity,
                    );
                }
                self.tree
                    .node_mut(node)
                    .content
                    .finish_source_backed_line(line_start, &line[self.offset..]);
            }
        } else {
            if self.partially_consumed_tab {
                self.offset += 1;
                let chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
                let spaces = " ".repeat(chars_to_tab);
                self.tree.node_mut(node).content.push_source(
                    self.line_leaf_id,
                    original_offset..self.offset,
                    &spaces,
                    OriginTransform::TabExpansion,
                );
            }
            if self.offset < line.len() {
                self.tree
                    .node_mut(node)
                    .content
                    .line_offsets
                    .push(self.offset);
                self.tree.node_mut(node).content.push_source(
                    self.line_leaf_id,
                    self.offset..line.len(),
                    &line[self.offset..],
                    OriginTransform::Identity,
                );
            }
        }
        let logical_end = self.tree.node(node).content.logical_len();
        if logical_end > logical_start {
            self.tree.events.push(BlockEvent::AppendContent {
                node,
                logical_start: u32::try_from(logical_start).expect("logical below u32"),
                logical_end: u32::try_from(logical_end).expect("logical below u32"),
                origin_start: u32::try_from(origin_start).expect("origin count below u32"),
                origin_end: u32::try_from(self.tree.node(node).content.origins.len())
                    .expect("origin count below u32"),
                line_offsets_start: u32::try_from(line_offsets_start)
                    .expect("line offset count below u32"),
                line_offsets_end: u32::try_from(self.tree.node(node).content.line_offsets.len())
                    .expect("line offset count below u32"),
                source_backed: self.tree.node(node).content.source_backed,
            });
        }
        Ok(())
    }

    /// Prove that a truncated source window is sufficient for the *existing*
    /// donor stage about to run. This is an input-availability gate, not a
    /// block classifier: the stage still executes its original handler and
    /// owns the result. Every opener stage reads only `open.container`, the
    /// line prefix up to `first_nonspace`, and that byte; none of them consult
    /// `self.current`. So a non-special first nonspace byte makes every
    /// `CommonMark` opener reject without consulting the omitted suffix, at any
    /// undecorated open-block boundary — not only at the document root.
    fn ensure_segmented_controller_stage_exact(
        &self,
        transition: &LineTransition,
        controller_window: &str,
    ) -> Result<(), ParseError> {
        let facts = self.direct_segmented_line.ok_or(ParseError::Invariant(
            "segmented transition owns physical facts",
        ))?;
        if facts.controller_window_complete {
            return Ok(());
        }
        let LinePhase::OpenNew(open) = transition.phase else {
            return Ok(());
        };
        if open.stage == OpenNewStage::Start {
            return Ok(());
        }
        // `Document` is the quiescent root boundary; `Paragraph` is the open
        // leaf a continuation line lands in. Any other container either owns a
        // matched prefix this pass has already re-entered (`Start` above) or
        // needs suffix-dependent continuation rules of its own.
        let container_is_open_leaf_or_root = matches!(
            self.tree.node(open.container).kind,
            BlockKind::Document | BlockKind::Paragraph
        );
        if open.container != open.last_matched_container
            || !container_is_open_leaf_or_root
            || self.indent >= CODE_INDENT
            || self.blank
        {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::SegmentedLine,
            ));
        }
        let Some(first) = controller_window
            .as_bytes()
            .get(self.first_nonspace)
            .copied()
        else {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::SegmentedLine,
            ));
        };
        // Each excluded byte can start a handler whose exact decision may
        // depend on an arbitrarily distant suffix (`---   x`, a fence info
        // string, HTML, or a list interruption). Returning Unknown is safer
        // than feeding a synthetic end-of-line to that handler.
        if matches!(
            first,
            b'>' | b'#' | b'`' | b'~' | b'<' | b'-' | b'_' | b'*' | b'+' | b'0'
                ..=b'9' | b'\r' | b'\n'
        ) {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::SegmentedLine,
            ));
        }
        // An open `Paragraph` container additionally arms the Setext underline
        // (`=`; `-` is already excluded above) and — once the staged GFM table
        // opener stops being a stub — the delimiter row (`|`, `:`; `-` again).
        // Both scan to end of line, which the truncated window cannot supply.
        if matches!(self.tree.node(open.container).kind, BlockKind::Paragraph)
            && matches!(first, b'=' | b'|' | b':')
        {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::SegmentedLine,
            ));
        }
        Ok(())
    }

    /// Admit only outcomes whose semantics are fully determined by the
    /// retained controller window. A complete window may use every existing
    /// donor handler. A truncated window remains safe only for Paragraph
    /// content: its marker prefix has already been consumed and the remaining
    /// physical content is represented by exact source metrics rather than a
    /// copied suffix.
    fn segmented_outcome_is_supported(&self) -> bool {
        if self
            .direct_segmented_line
            .is_some_and(|facts| facts.controller_window_complete)
        {
            return true;
        }
        let node = self.current;
        matches!(self.tree.node(node).kind, BlockKind::Paragraph)
            && self
                .direct
                .as_ref()
                .is_some_and(|direct| direct.commands.is_empty())
    }

    fn direct_add_line(&mut self, node: NodeId, line: &str) -> Result<(), ParseError> {
        if let Some(facts) = self.direct_segmented_line {
            if !facts.controller_window_complete
                || matches!(self.tree.node(node).kind, BlockKind::Paragraph)
            {
                return self.direct_add_segmented_line(node, line, facts);
            }
        }
        if matches!(
            self.tree.node(node).kind,
            BlockKind::Heading { setext: false, .. }
        ) {
            // The ATX handler owns the original, unchopped physical line and
            // emits its complete marker/content/hidden-tail/EOL partition.
            return Ok(());
        }
        if matches!(
            self.tree.node(node).kind,
            BlockKind::Heading { setext: true, .. }
        ) {
            let (content_end, _) = direct_line_ending(line)?;
            let direct = self
                .direct
                .as_ref()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            if self.offset == content_end && direct.claimed_offset == line.len() {
                return Ok(());
            }
            return Err(ParseError::Invariant(
                "direct Setext handler owns the complete underline line",
            ));
        }
        if matches!(
            self.tree.node(node).kind,
            BlockKind::CodeBlock { fenced: true, .. }
        ) {
            return self.direct_add_fenced_code_line(node, line);
        }
        if matches!(
            self.tree.node(node).kind,
            BlockKind::CodeBlock { fenced: false, .. }
        ) {
            return self.direct_add_indented_code_line(node, line);
        }
        if matches!(self.tree.node(node).kind, BlockKind::HtmlBlock { .. }) {
            return self.direct_add_html_block_line(node, line);
        }
        if !matches!(self.tree.node(node).kind, BlockKind::Paragraph) {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::AggregateContent,
            ));
        }
        let (content_end, ending) = direct_line_ending(line)?;
        if self.offset > content_end {
            return Err(ParseError::InvalidUtf8Boundary);
        }
        let parent = self
            .tree
            .parent(node)
            .ok_or(ParseError::Invariant("paragraph has a parent"))?;
        let opened_this_line = self.opened_this_line.contains(&node);
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if !direct.paragraph_has_content
            && line[self.offset..content_end].trim_start().starts_with('[')
        {
            direct.paragraph_may_have_reference_prefix = true;
        }

        if direct.pending_terminator {
            direct.push_previous(DirectIntent::ResolveTerminator {
                resolution: DirectTerminatorResolution::ContinueCanonicalNewline,
            })?;
            direct.pending_terminator = false;
        }
        if direct.claimed_offset > self.offset {
            return Err(ParseError::Invariant(
                "direct claimed prefix does not exceed parser offset",
            ));
        }
        if direct.claimed_offset < self.offset {
            let gap = DirectIntent::Consume {
                owner: parent,
                part: DirectCoveragePart::Gap,
                range: u32::try_from(direct.claimed_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("direct offset below u32"))?,
                logical: DirectLogicalAction::None,
            };
            if opened_this_line {
                direct.push_body_before_trailing_open(node, gap)?;
            } else {
                direct.push_body(gap)?;
            }
            direct.claimed_offset = self.offset;
        }
        let mut content_offset = self.offset;
        if self.partially_consumed_tab {
            if line.as_bytes().get(content_offset) != Some(&b'\t') {
                return Err(ParseError::Invariant(
                    "partial-tab donor state points at an exact tab byte",
                ));
            }
            let remaining_spaces = TAB_STOP - (self.column % TAB_STOP);
            let remaining_spaces = u8::try_from(remaining_spaces)
                .map_err(|_| ParseError::Invariant("partial-tab width fits u8"))?;
            let end = content_offset
                .checked_add(1)
                .ok_or(ParseError::Invariant("partial-tab range overflow"))?;
            if end > content_end || !(1..=3).contains(&remaining_spaces) {
                return Err(ParseError::Invariant(
                    "partial tab retains one through three content spaces",
                ));
            }
            let physical_owner = direct.line_marker_floor.unwrap_or(parent);
            let part = if direct.line_marker_floor.is_some() {
                DirectCoveragePart::ContainerMarker
            } else {
                DirectCoveragePart::Gap
            };
            direct.push_body(DirectIntent::ConsumePartialTab {
                owner: physical_owner,
                logical_target: node,
                part,
                range: u32::try_from(content_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                remaining_spaces,
            })?;
            direct.claimed_offset = end;
            content_offset = end;
        }
        if content_offset < content_end {
            direct.push_body(DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(content_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(content_end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::CanonicalText,
            })?;
            direct.claimed_offset = content_end;
        }
        if let Some(ending) = ending {
            direct.push_body(DirectIntent::StageTerminator {
                range: u32::try_from(content_end)
                    .map_err(|_| ParseError::Invariant("direct line below u32"))?
                    ..u32::try_from(line.len())
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                ending,
            })?;
            direct.pending_terminator = true;
            direct.claimed_offset = line.len();
        }
        direct.paragraph_has_content = true;
        self.offset = content_offset;
        self.partially_consumed_tab = false;
        Ok(())
    }

    fn direct_add_indented_code_line(
        &mut self,
        node: NodeId,
        line: &str,
    ) -> Result<(), ParseError> {
        let (content_end, ending) = direct_line_ending(line)?;
        if self.offset > content_end {
            return Err(ParseError::InvalidUtf8Boundary);
        }
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.pending_terminator {
            direct.push_previous(DirectIntent::ResolveTerminator {
                resolution: DirectTerminatorResolution::ContinueCanonicalNewline,
            })?;
            direct.pending_terminator = false;
        }
        if direct.claimed_offset != self.offset {
            return Err(ParseError::Invariant(
                "indented-code content follows its exact deindent",
            ));
        }
        let mut content_offset = self.offset;
        if self.partially_consumed_tab {
            if line.as_bytes().get(content_offset) != Some(&b'\t') {
                return Err(ParseError::Invariant(
                    "partial indented-code tab points at an exact tab byte",
                ));
            }
            let remaining_spaces = TAB_STOP - (self.column % TAB_STOP);
            let remaining_spaces = u8::try_from(remaining_spaces)
                .map_err(|_| ParseError::Invariant("partial-tab width fits u8"))?;
            let end = content_offset
                .checked_add(1)
                .ok_or(ParseError::Invariant("partial-tab range overflow"))?;
            if end > content_end || !(1..=3).contains(&remaining_spaces) {
                return Err(ParseError::Invariant(
                    "partial indented-code tab retains one through three spaces",
                ));
            }
            direct.push_body(DirectIntent::ConsumePartialTab {
                owner: direct.line_marker_floor.unwrap_or(node),
                logical_target: node,
                part: DirectCoveragePart::BlockMarker,
                range: u32::try_from(content_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                remaining_spaces,
            })?;
            direct.claimed_offset = end;
            content_offset = end;
        }
        if content_offset < content_end {
            direct.push_body(DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(content_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(content_end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::CanonicalText,
            })?;
            direct.claimed_offset = content_end;
        }
        if let Some(ending) = ending {
            let range = u32::try_from(content_end)
                .map_err(|_| ParseError::Invariant("direct line below u32"))?
                ..u32::try_from(line.len())
                    .map_err(|_| ParseError::Invariant("direct line below u32"))?;
            if self.blank {
                direct.push_body(DirectIntent::StageTerminator { range, ending })?;
                direct.pending_terminator = true;
            } else {
                direct.push_body(DirectIntent::Consume {
                    owner: node,
                    part: DirectCoveragePart::Content,
                    range,
                    logical: DirectLogicalAction::CanonicalNewline,
                })?;
            }
            direct.claimed_offset = line.len();
        }
        self.offset = content_offset;
        self.partially_consumed_tab = false;
        Ok(())
    }

    fn direct_add_html_block_line(&mut self, node: NodeId, line: &str) -> Result<(), ParseError> {
        let (content_end, ending) = direct_line_ending(line)?;
        let block_type = match self.tree.node(node).kind {
            BlockKind::HtmlBlock { block_type, .. } => block_type,
            _ => return Err(ParseError::Invariant("HTML line targets an HTML block")),
        };
        let retiring_old = !self.opened_this_line.contains(&node)
            && html_block_end(block_type, &line[self.first_nonspace..])?;
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.pending_terminator {
            direct.push_previous(DirectIntent::ResolveTerminator {
                resolution: DirectTerminatorResolution::CloseNone,
            })?;
            direct.pending_terminator = false;
        }
        if direct.claimed_offset != self.offset || self.offset > content_end {
            return Err(ParseError::Invariant(
                "HTML block content follows its exact container prefix",
            ));
        }
        if self.offset < content_end {
            let intent = DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(self.offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(content_end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::CanonicalText,
            };
            if retiring_old {
                direct.push_old_source(intent)?;
            } else {
                direct.push_body(intent)?;
            }
            direct.claimed_offset = content_end;
        }
        if ending.is_some() {
            let intent = DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(content_end)
                    .map_err(|_| ParseError::Invariant("direct line below u32"))?
                    ..u32::try_from(line.len())
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::CanonicalNewline,
            };
            if retiring_old {
                direct.push_old_source(intent)?;
            } else {
                direct.push_body(intent)?;
            }
            direct.claimed_offset = line.len();
        }
        self.offset = content_end;
        self.partially_consumed_tab = false;
        Ok(())
    }

    fn direct_add_segmented_line(
        &mut self,
        node: NodeId,
        controller_window: &str,
        facts: DirectSegmentedLineFacts,
    ) -> Result<(), ParseError> {
        if !matches!(self.tree.node(node).kind, BlockKind::Paragraph) {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::SegmentedLine,
            ));
        }
        let physical_bytes = usize::try_from(facts.physical_bytes)
            .map_err(|_| ParseError::Invariant("segmented bytes fit usize"))?;
        let ending_bytes = match facts.ending {
            None => 0,
            Some(DirectLineEnding::Lf | DirectLineEnding::Cr) => 1,
            Some(DirectLineEnding::CrLf) => 2,
        };
        if facts.content_end.checked_add(ending_bytes) != Some(physical_bytes)
            || self.offset > facts.content_end
            || self.partially_consumed_tab
            || self.offset > controller_window.len()
        {
            return Err(ParseError::Invariant(
                "segmented Paragraph has exact physical cuts",
            ));
        }
        let parent = self
            .tree
            .parent(node)
            .ok_or(ParseError::Invariant("segmented Paragraph has a parent"))?;
        let opened_this_line = self.opened_this_line.contains(&node);
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if !direct.paragraph_has_content
            && controller_window[self.offset..]
                .trim_start()
                .starts_with('[')
        {
            direct.paragraph_may_have_reference_prefix = true;
        }
        if direct.pending_terminator {
            direct.push_previous(DirectIntent::ResolveTerminator {
                resolution: DirectTerminatorResolution::ContinueCanonicalNewline,
            })?;
            direct.pending_terminator = false;
        }
        if direct.claimed_offset > self.offset {
            return Err(ParseError::Invariant(
                "segmented claimed prefix does not exceed parser offset",
            ));
        }
        if direct.claimed_offset < self.offset {
            let gap = DirectIntent::Consume {
                owner: parent,
                part: DirectCoveragePart::Gap,
                range: u32::try_from(direct.claimed_offset)
                    .map_err(|_| ParseError::Invariant("segmented offset fits u32"))?
                    ..u32::try_from(self.offset)
                        .map_err(|_| ParseError::Invariant("segmented offset fits u32"))?,
                logical: DirectLogicalAction::None,
            };
            if opened_this_line {
                direct.push_body_before_trailing_open(node, gap)?;
            } else {
                direct.push_body(gap)?;
            }
            direct.claimed_offset = self.offset;
        }
        if self.offset < facts.content_end {
            direct.push_body(DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(self.offset)
                    .map_err(|_| ParseError::Invariant("segmented offset fits u32"))?
                    ..u32::try_from(facts.content_end)
                        .map_err(|_| ParseError::Invariant("segmented content fits u32"))?,
                logical: DirectLogicalAction::CanonicalText,
            })?;
            direct.claimed_offset = facts.content_end;
        }
        if let Some(ending) = facts.ending {
            direct.push_body(DirectIntent::StageTerminator {
                range: u32::try_from(facts.content_end)
                    .map_err(|_| ParseError::Invariant("segmented content fits u32"))?
                    ..facts.physical_bytes,
                ending,
            })?;
            direct.pending_terminator = true;
            direct.claimed_offset = physical_bytes;
        }
        direct.paragraph_has_content = true;
        self.partially_consumed_tab = false;
        Ok(())
    }

    fn direct_add_fenced_code_line(&mut self, node: NodeId, line: &str) -> Result<(), ParseError> {
        let (content_end, ending) = direct_line_ending(line)?;
        if self.offset > content_end {
            return Err(ParseError::InvalidUtf8Boundary);
        }
        let opened_this_line = self.opened_this_line.contains(&node);
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset != self.offset {
            return Err(ParseError::Invariant(
                "fenced content follows its exact marker prefix",
            ));
        }
        let mut content_offset = self.offset;
        if self.partially_consumed_tab {
            if line.as_bytes().get(content_offset) != Some(&b'\t') {
                return Err(ParseError::Invariant(
                    "partial fenced-code tab points at an exact tab byte",
                ));
            }
            let remaining_spaces = TAB_STOP - (self.column % TAB_STOP);
            let remaining_spaces = u8::try_from(remaining_spaces)
                .map_err(|_| ParseError::Invariant("partial-tab width fits u8"))?;
            let end = content_offset
                .checked_add(1)
                .ok_or(ParseError::Invariant("partial-tab range overflow"))?;
            if end > content_end || !(1..=3).contains(&remaining_spaces) {
                return Err(ParseError::Invariant(
                    "partial fenced-code tab retains one through three spaces",
                ));
            }
            direct.push_body(DirectIntent::ConsumePartialTab {
                owner: direct.line_marker_floor.unwrap_or(node),
                logical_target: node,
                part: DirectCoveragePart::ContainerMarker,
                range: u32::try_from(content_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                remaining_spaces,
            })?;
            direct.claimed_offset = end;
            content_offset = end;
        }
        if content_offset < content_end {
            direct.push_body(DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(content_offset)
                    .map_err(|_| ParseError::Invariant("direct offset below u32"))?
                    ..u32::try_from(content_end)
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::CanonicalText,
            })?;
            direct.claimed_offset = content_end;
        }
        if opened_this_line {
            direct.push_body(DirectIntent::MarkFencedCodeBoundary {
                node,
                boundary: DirectFencedCodeBoundary::InfoEnd,
            })?;
        }
        if ending.is_some() {
            direct.push_body(DirectIntent::Consume {
                owner: node,
                part: DirectCoveragePart::Content,
                range: u32::try_from(content_end)
                    .map_err(|_| ParseError::Invariant("direct line below u32"))?
                    ..u32::try_from(line.len())
                        .map_err(|_| ParseError::Invariant("direct line below u32"))?,
                logical: DirectLogicalAction::CanonicalNewline,
            })?;
            direct.claimed_offset = line.len();
        }
        if opened_this_line {
            direct.push_body(DirectIntent::MarkFencedCodeBoundary {
                node,
                boundary: DirectFencedCodeBoundary::LiteralStart,
            })?;
        }
        self.offset = content_offset;
        self.partially_consumed_tab = false;
        Ok(())
    }

    fn begin_finish_transition(&self) -> FinishTransition {
        FinishTransition {
            phase: FinishPhase::CloseCurrent,
        }
    }

    fn step_finish_transition(
        &mut self,
        transition: &mut FinishTransition,
        receipt: &mut WorkPollReceipt,
    ) -> Result<bool, ParseError> {
        match &mut transition.phase {
            FinishPhase::CloseCurrent => {
                receipt.transitions += 1;
                if self.current != self.tree.root {
                    let discarded = self.current;
                    let current = self.finalize(discarded)?;
                    self.consume_reference_current_rebase(discarded, current)?;
                    self.current = current;
                } else {
                    transition.phase = FinishPhase::CloseRoot;
                }
            }
            FinishPhase::CloseRoot => {
                receipt.transitions += 1;
                let _ = self.finalize(self.tree.root)?;
                if self.defer_output_repairs {
                    return Ok(true);
                }
                transition.phase = FinishPhase::Propagate {
                    postorder: vec![TreeCursorFrame {
                        node: self.tree.root,
                        next_child: 0,
                    }],
                    active_list: None,
                };
            }
            FinishPhase::Propagate {
                postorder,
                active_list,
            } => {
                receipt.index_operations += 1;
                if let Some(scan) = active_list {
                    let Some(frame) = scan.stack.last_mut() else {
                        if scan.max_end.column != 0 {
                            self.tree.node_mut(scan.list).source_end = scan.max_end;
                        }
                        *active_list = None;
                        return Ok(false);
                    };
                    let children = &self.tree.node(frame.node).children;
                    if let Some(child) = children.get(frame.next_child).copied() {
                        frame.next_child += 1;
                        let candidate = self.tree.node(child).source_end;
                        if candidate.column != 0 && candidate > scan.max_end {
                            scan.max_end = candidate;
                        }
                        scan.stack.push(TreeCursorFrame {
                            node: child,
                            next_child: 0,
                        });
                    } else {
                        scan.stack.pop();
                    }
                    return Ok(false);
                }

                let Some(frame) = postorder.last_mut() else {
                    return Ok(true);
                };
                let children = &self.tree.node(frame.node).children;
                if let Some(child) = children.get(frame.next_child).copied() {
                    frame.next_child += 1;
                    postorder.push(TreeCursorFrame {
                        node: child,
                        next_child: 0,
                    });
                    return Ok(false);
                }

                let node = postorder.pop().expect("postorder frame exists").node;
                if matches!(self.tree.node(node).kind, BlockKind::List(_)) {
                    *active_list = Some(ListPositionScan {
                        list: node,
                        max_end: self.tree.node(node).source_end,
                        stack: vec![TreeCursorFrame {
                            node,
                            next_child: 0,
                        }],
                    });
                }
            }
        }
        Ok(false)
    }

    fn finalize(&mut self, node: NodeId) -> Result<NodeId, ParseError> {
        self.finalize_borrowed(node)
    }

    fn direct_require_reference_finalizer(
        &mut self,
        node: NodeId,
        context: DirectReferencePrefixContext,
    ) -> Result<DirectReferenceFinalizeResume, ParseError> {
        if self.direct.is_none() || !matches!(self.tree.node(node).kind, BlockKind::Paragraph) {
            return Ok(DirectReferenceFinalizeResume::Continue);
        }
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if !direct.paragraph_may_have_reference_prefix {
            return Ok(DirectReferenceFinalizeResume::Continue);
        }
        if let Some(disposition) = direct.reference_finalize_resume_once.take() {
            direct.paragraph_may_have_reference_prefix = false;
            return Ok(match disposition {
                DirectReferencePrefixDisposition::NoDefinitions
                | DirectReferencePrefixDisposition::VisibleRemainder => {
                    DirectReferenceFinalizeResume::Continue
                }
                DirectReferencePrefixDisposition::ReferenceOnly => {
                    DirectReferenceFinalizeResume::ReferenceOnly
                }
            });
        }
        let request = direct.request_reference_prefix(context)?;
        Err(ParseError::DirectExternalWork(request))
    }

    fn direct_discard_reference_only_paragraph(
        &mut self,
        node: NodeId,
    ) -> Result<NodeId, ParseError> {
        let parent = self.tree.parent(node).ok_or(ParseError::Invariant(
            "reference-only Paragraph has a parent",
        ))?;
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.emission_stack.last().copied() != Some(node) {
            return Err(ParseError::Invariant(
                "reference-only Paragraph is the emitted open leaf",
            ));
        }
        direct.emission_stack.pop();
        direct.pending_terminator = false;
        direct.paragraph_has_content = false;
        direct.paragraph_may_have_reference_prefix = false;
        if direct.reference_current_rebase.is_some() {
            return Err(ParseError::Invariant(
                "one reference-only current rebase is pending",
            ));
        }
        direct.reference_current_rebase = Some(DirectReferenceCurrentRebase {
            discarded: node,
            current: parent,
        });
        self.tree.close_scratch(node);
        self.tree.detach_scratch(node);
        self.current = parent;
        Ok(parent)
    }

    fn consume_reference_current_rebase(
        &mut self,
        discarded: NodeId,
        current: NodeId,
    ) -> Result<(), ParseError> {
        let Some(rebase) = self
            .direct
            .as_mut()
            .and_then(|direct| direct.reference_current_rebase.take())
        else {
            return Ok(());
        };
        if rebase.discarded != discarded || rebase.current != current {
            return Err(ParseError::Invariant(
                "reference-only current rebase matches its finalization caller",
            ));
        }
        Ok(())
    }

    fn direct_retain_reference_only_paragraph(&mut self, node: NodeId) -> Result<(), ParseError> {
        if !matches!(self.tree.node(node).kind, BlockKind::Paragraph) {
            return Err(ParseError::Invariant(
                "reference-only Setext disposition targets a Paragraph",
            ));
        }
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.emission_stack.last().copied() != Some(node)
            || direct.paragraph_may_have_reference_prefix
        {
            return Err(ParseError::Invariant(
                "reference-only Setext disposition retains the emitted open Paragraph",
            ));
        }
        // A reference-only terminal consumed the staged line ending as part
        // of the accepted definition prefix. The writer's terminal join hides
        // that physical source while retaining exhaustive coverage; replaying
        // it as the separator before the Setext-looking line would create a
        // spurious leading logical newline.
        direct.pending_terminator = false;
        direct.paragraph_has_content = false;
        Ok(())
    }

    fn finalize_borrowed(&mut self, node: NodeId) -> Result<NodeId, ParseError> {
        if self.direct_require_reference_finalizer(
            node,
            DirectReferencePrefixContext::ParagraphFinalization,
        )? == DirectReferenceFinalizeResume::ReferenceOnly
        {
            return self.direct_discard_reference_only_paragraph(node);
        }
        let parent = self.tree.parent(node).unwrap_or(self.tree.root);
        let direct_last_line_blank = self
            .direct
            .as_ref()
            .map(|_| self.tree.node(node).last_line_blank);
        if self.direct.is_some() {
            self.tree.close_scratch(node);
        } else {
            self.tree.close(node);
        }
        if self.curline_len == 0 {
            self.tree.node_mut(node).source_end =
                Position::new(self.line_number, self.last_line_length);
        } else if matches!(
            self.tree.node(node).kind,
            BlockKind::Document
                | BlockKind::CodeBlock {
                    fenced: true,
                    closed: true,
                    ..
                }
        ) {
            self.tree.node_mut(node).source_end =
                Position::new(self.line_number, self.curline_end_col);
        } else if !matches!(
            self.tree.node(node).kind,
            BlockKind::ThematicBreak | BlockKind::TableRow { .. } | BlockKind::Table(_)
        ) {
            self.tree.node_mut(node).source_end =
                Position::new(self.line_number.saturating_sub(1), self.last_line_length);
        }

        match self.tree.node(node).kind.clone() {
            BlockKind::Paragraph => {
                if let Some(direct) = &mut self.direct {
                    if direct.pending_terminator {
                        direct.push_previous(DirectIntent::ResolveTerminator {
                            resolution: DirectTerminatorResolution::CloseNone,
                        })?;
                        direct.pending_terminator = false;
                    }
                    direct.paragraph_has_content = false;
                    direct.paragraph_may_have_reference_prefix = false;
                } else if !self.resolve_reference_link_definitions(node)? {
                    self.tree.detach(node);
                }
            }
            BlockKind::CodeBlock { fenced, .. } => {
                if self.direct.is_some() {
                    if !fenced {
                        let direct = self
                            .direct
                            .as_mut()
                            .ok_or(ParseError::Invariant("direct hooks are present"))?;
                        if direct.pending_terminator {
                            direct.push_previous(DirectIntent::ResolveTerminator {
                                resolution: DirectTerminatorResolution::CloseNone,
                            })?;
                            direct.pending_terminator = false;
                        }
                    }
                    // The direct writer owns the source-backed projection and
                    // snapshots the parser-certified info/literal boundaries.
                    // Retaining donor `LeafContent` here would duplicate the
                    // aggregate raw stream and defeat direct scratch compaction.
                } else {
                    let metadata =
                        self.tree
                            .node(node)
                            .content
                            .source_backed
                            .ok_or(ParseError::Invariant(
                                "code block content is not source-backed",
                            ))?;
                    if fenced {
                        if let BlockKind::CodeBlock { info, literal, .. } =
                            &mut self.tree.node_mut(node).kind
                        {
                            *info = LogicalProjection::new(0, metadata.first_line_content_end);
                            *literal = LogicalProjection::new(
                                metadata.first_line_end,
                                metadata.logical_len,
                            );
                        }
                    } else if let BlockKind::CodeBlock { literal, .. } =
                        &mut self.tree.node_mut(node).kind
                    {
                        *literal = LogicalProjection::new(0, metadata.trimmed_end).with_newline();
                    }
                }
            }
            BlockKind::HtmlBlock { .. } => {
                if self.direct.is_none() {
                    let metadata =
                        self.tree
                            .node(node)
                            .content
                            .source_backed
                            .ok_or(ParseError::Invariant(
                                "HTML block content is not source-backed",
                            ))?;
                    let line_count = usize::try_from(metadata.trimmed_line_index)
                        .expect("HTML line count fits usize");
                    let last_line_length = usize::try_from(metadata.trimmed_last_line_len)
                        .expect("HTML line length fits usize");
                    let start_line = self.tree.node(node).source_start.line;
                    let line_offset = self
                        .tree
                        .node(node)
                        .content
                        .line_offsets
                        .get(line_count)
                        .copied()
                        .unwrap_or(0);
                    self.tree.node_mut(node).source_end =
                        Position::new(start_line + line_count, line_offset + last_line_length);
                    if let BlockKind::HtmlBlock { literal, .. } = &mut self.tree.node_mut(node).kind
                    {
                        *literal = LogicalProjection::new(0, metadata.logical_len);
                    }
                }
            }
            BlockKind::List(mut list) => {
                if self.direct.is_some() {
                    // Direct source coverage is the position authority. The
                    // donor tree's descendant position-repair chronology must
                    // never enter the normalized writer port.
                } else if self.defer_output_repairs {
                    let mut scratch_positions = Vec::new();
                    let mut stack = vec![node];
                    while let Some(scratch) = stack.pop() {
                        let value = self.tree.node(scratch);
                        scratch_positions.push((scratch, value.source_start, value.source_end));
                        stack.extend(value.children.iter().rev().copied());
                    }
                    self.tree
                        .events
                        .push(BlockEvent::RepairListSourcePositions {
                            node,
                            scratch_positions,
                        });
                } else if let Some(candidate_end) = self.fix_zero_end_columns(node) {
                    self.tree.node_mut(node).source_end = candidate_end;
                }
                list.tight = self.tree.list_is_tight(node);
                self.tree.node_mut(node).kind = BlockKind::List(list);
            }
            _ => {}
        }
        if self.direct.is_some() {
            let kind = direct_block_kind(&self.tree.node(node).kind)?;
            let final_facts = direct_final_facts(&self.tree.node(node).kind);
            let summary = self.tree.closed_child_summary(node);
            let intent = DirectIntent::Close {
                node,
                kind,
                final_facts,
                last_line_blank: direct_last_line_blank.ok_or(ParseError::Invariant(
                    "direct close captured intrinsic blank state",
                ))?,
                child: DirectClosedChild {
                    ends_blank: summary.ends_blank,
                    item_loose_if_nonlast: summary.item_loose_if_nonlast,
                    item_loose_if_last: summary.item_loose_if_last,
                },
            };
            let opened_this_line = self.opened_this_line.contains(&node);
            let direct = self
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            if opened_this_line {
                direct.push_body(intent)?;
            } else {
                direct.push_retired(intent)?;
            }
        }
        if self.direct.is_some() {
            self.tree.fold_finalized_direct_child(node);
        } else {
            self.tree.fold_finalized_child(node);
        }
        Ok(parent)
    }

    fn fix_zero_end_columns(&mut self, container: NodeId) -> Option<Position> {
        let mut stack = Vec::new();
        for child in self.tree.node(container).children.clone() {
            stack.push((child, false));
            while let Some((node, visited)) = stack.pop() {
                if !visited {
                    stack.push((node, true));
                    for descendant in self.tree.node(node).children.clone() {
                        stack.push((descendant, false));
                    }
                    continue;
                }
                if self.tree.node(node).source_end.column == 0 {
                    let mut last = self.tree.last_child(node);
                    while let Some(next) =
                        last.and_then(|candidate| self.tree.last_child(candidate))
                    {
                        last = Some(next);
                    }
                    if let Some(last) = last {
                        let position = self.tree.node(last).source_end;
                        if position.column != 0 {
                            self.tree.node_mut(node).source_end = position;
                            continue;
                        }
                    }
                    let start = self.tree.node(node).source_start;
                    self.tree.node_mut(node).source_end = start;
                }
            }
        }
        self.tree
            .last_child(container)
            .map(|last| self.tree.node(last).source_end)
            .filter(|end| end.column != 0)
    }

    fn resolve_reference_link_definitions(&mut self, node: NodeId) -> Result<bool, ParseError> {
        let _ = node;
        // The production promotion exposes only DirectValueBlockParser. Its
        // source-backed reference-prefix rendezvous resolves this case before
        // the legacy aggregate parser can reach this method. Keep the legacy
        // path fail-closed instead of recreating cooked URL/title Strings from
        // the production facade's source ranges merely to compile dead code.
        Err(ParseError::DirectUnsupported(
            DirectUnsupported::AggregateContent,
        ))
    }

    fn direct_prepare_pending_blank_gap(&mut self) -> Result<(), ParseError> {
        let (pending, floor, survivor) = {
            let direct = self
                .direct
                .as_ref()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            let survivor = direct
                .emission_stack
                .iter()
                .rev()
                .copied()
                .find(|node| self.tree.node(*node).open);
            (
                direct.pending_gap_at_line_start,
                direct.pending_gap_floor_at_line_start,
                survivor,
            )
        };
        if !pending {
            return Ok(());
        }
        let owner = floor
            .or(survivor)
            .ok_or(ParseError::Invariant("pending blank gap has an old owner"))?;
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if !direct.pending_blank_gap {
            return Err(ParseError::Invariant(
                "line-start blank gap remains pending",
            ));
        }
        direct.push_old_source_front(DirectIntent::ResolveBlankGap { owner })?;
        direct.pending_gap_at_line_start = false;
        direct.pending_gap_floor_at_line_start = None;
        direct.pending_blank_gap = false;
        direct.pending_blank_gap_floor = None;
        Ok(())
    }

    fn direct_stage_blank_line_bytes(&mut self, line_bytes: usize) -> Result<(), ParseError> {
        if !self.blank {
            return Ok(());
        }
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset > line_bytes {
            return Err(ParseError::Invariant(
                "blank-line claims remain within the line",
            ));
        }
        if direct.claimed_offset < line_bytes {
            let start = u32::try_from(direct.claimed_offset)
                .map_err(|_| ParseError::Invariant("direct offset below u32"))?;
            let end = u32::try_from(line_bytes)
                .map_err(|_| ParseError::Invariant("direct line below u32"))?;
            direct.push_body(DirectIntent::StageBlankGap { range: start..end })?;
            direct.pending_blank_gap = true;
            direct.pending_blank_gap_floor = direct.line_marker_floor;
            direct.claimed_offset = line_bytes;
        }
        Ok(())
    }

    fn direct_queue_finish_line(&mut self, line: &str) -> Result<(), ParseError> {
        let physical_bytes = u32::try_from(line.len())
            .map_err(|_| ParseError::Invariant("direct line below u32"))?;
        let physical_utf16 = u32::try_from(line.encode_utf16().count())
            .map_err(|_| ParseError::Invariant("direct UTF-16 length below u32"))?;
        self.direct_queue_finish_line_metrics(physical_bytes, physical_utf16)
    }

    fn direct_queue_finish_line_metrics(
        &mut self,
        physical_bytes: u32,
        physical_utf16: u32,
    ) -> Result<(), ParseError> {
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.claimed_offset
            != usize::try_from(physical_bytes)
                .map_err(|_| ParseError::Invariant("direct FinishLine physical bytes fit usize"))?
        {
            return Err(ParseError::Invariant(
                "direct line is exactly covered before FinishLine",
            ));
        }
        direct.push_command(
            DirectCommand::FinishLine {
                physical_bytes,
                physical_utf16,
            },
            None,
        )
    }

    /// Drop all closed parser nodes after the line command is acknowledged.
    /// The retained tree contains only the open path plus constant-size child
    /// folds, so direct scratch is O(open depth), not O(document length).
    fn compact_direct_scratch(&mut self) -> Result<(), ParseError> {
        if self.direct.is_none() {
            return Err(ParseError::Invariant(
                "direct compaction requires direct mode",
            ));
        }
        if self
            .tree
            .nodes
            .iter()
            .any(|node| !node.content.logical.is_empty())
        {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::AggregateContent,
            ));
        }

        let mut path = vec![self.tree.root];
        let mut cursor = self.tree.root;
        while let Some(child) = self
            .tree
            .last_child(cursor)
            .filter(|child| self.tree.node(*child).open)
        {
            path.push(child);
            cursor = child;
        }
        let current_depth = path
            .iter()
            .position(|candidate| *candidate == self.current)
            .ok_or(ParseError::Invariant("direct current is on the open path"))?;
        let pending_floor_depth = {
            let direct = self
                .direct
                .as_ref()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            if direct.emission_stack != path {
                return Err(ParseError::Invariant(
                    "direct emitted stack matches the semantic open path",
                ));
            }
            if direct.pending_gap_at_line_start || direct.pending_gap_floor_at_line_start.is_some()
            {
                return Err(ParseError::Invariant(
                    "line-start blank gap is resolved before compaction",
                ));
            }
            direct
                .pending_blank_gap_floor
                .map(|floor| {
                    path.iter().position(|candidate| *candidate == floor).ok_or(
                        ParseError::Invariant("pending blank-gap floor remains on the open path"),
                    )
                })
                .transpose()?
        };
        if self
            .direct
            .as_ref()
            .is_some_and(|direct| direct.pending_blank_gap != pending_floor_depth.is_some())
            && self
                .direct
                .as_ref()
                .is_some_and(|direct| direct.pending_blank_gap_floor.is_some())
        {
            return Err(ParseError::Invariant(
                "pending blank-gap floor accompanies pending gap state",
            ));
        }

        for (depth, node) in path.iter().copied().enumerate() {
            let retained = path.get(depth + 1).copied();
            self.tree.fold_children_before(node, retained);
        }

        let old_tree = std::mem::take(&mut self.tree);
        let mut compact = BlockTree::new();
        let mut compact_path = vec![compact.root];
        for (depth, old_id) in path.iter().copied().enumerate() {
            let new_id = if depth == 0 {
                compact.root
            } else {
                let parent = compact_path[depth - 1];
                let node = old_tree.node(old_id);
                let id = compact.append_scratch(parent, node.kind.clone(), node.source_start);
                compact_path.push(id);
                id
            };
            let old = old_tree.node(old_id);
            let new = compact.node_mut(new_id);
            new.kind = old.kind.clone();
            new.open = old.open;
            new.last_line_blank = old.last_line_blank;
            new.table_visited = old.table_visited;
            new.table_autocompleted_cells = old.table_autocompleted_cells;
            new.source_start = old.source_start;
            new.source_end = old.source_end;
            new.content = old.content.clone();
            new.historical_children = old.historical_children;
            new.folded_children = 0;
        }
        debug_assert!(compact.events.is_empty());
        self.current = compact_path[current_depth];
        self.tree = compact;
        let direct = self
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        direct.pending_blank_gap_floor = pending_floor_depth.map(|depth| compact_path[depth]);
        direct.line_marker_floor = None;
        direct.emission_stack = compact_path;
        self.opened_this_line.clear();
        Ok(())
    }
}

fn capture_direct_pause_kind(kind: &BlockKind) -> Result<Option<DirectBlockKind>, ParseError> {
    let supported = matches!(
        kind,
        BlockKind::Document
            | BlockKind::BlockQuote
            | BlockKind::List(_)
            | BlockKind::Item(_)
            | BlockKind::Paragraph
            | BlockKind::Heading {
                level: 1 | 2,
                setext: true,
                closed: false,
            }
            | BlockKind::Heading {
                level: 1..=6,
                setext: false,
                ..
            }
    ) || matches!(
        kind,
        BlockKind::CodeBlock {
            fenced: true,
            fence_char: b'`' | b'~',
            fence_length,
            fence_offset,
            info,
            literal,
            closed: false,
        } if *fence_length >= 3 && *fence_offset <= 3 && info.is_empty() && literal.is_empty()
    ) || matches!(
        kind,
        BlockKind::CodeBlock {
            fenced: false,
            fence_char: 0,
            fence_length: 0,
            fence_offset: 0,
            info,
            literal,
            closed: true,
        } if info.is_empty() && literal.is_empty()
    );
    if !supported {
        return Ok(None);
    }
    let direct = direct_block_kind(kind)?;
    // Keep the reverse projection exercised at capture time. This makes a new
    // direct kind or newly observable fact fail the pause seam rather than
    // silently falling out of its reconstruction contract.
    let _ = direct_pause_block_kind(direct)?;
    Ok(Some(direct))
}

fn direct_pause_line_local_output_is_available(
    frames: &[DirectPauseFrame],
    current_frame: usize,
    deferred: DirectDeferredState,
) -> bool {
    let mut blank_frame = None;
    for (depth, frame) in frames.iter().enumerate() {
        if !frame.last_line_blank {
            continue;
        }
        if blank_frame.replace(depth).is_some()
            || depth != current_frame
            || !deferred.blank_gap
            || matches!(
                frame.kind,
                DirectBlockKind::BlockQuote
                    | DirectBlockKind::Paragraph
                    | DirectBlockKind::Heading(_)
                    | DirectBlockKind::FencedCode(_)
            )
        {
            return false;
        }
    }
    true
}

fn direct_pause_block_kind(kind: DirectBlockKind) -> Result<BlockKind, ParseError> {
    match kind {
        DirectBlockKind::Document => Ok(BlockKind::Document),
        DirectBlockKind::BlockQuote => Ok(BlockKind::BlockQuote),
        DirectBlockKind::List(facts) => {
            match facts.list_type {
                ListType::Bullet
                    if facts.start == 1
                        && facts.delimiter == ListDelimiter::Period
                        && matches!(facts.bullet_char, b'-' | b'+' | b'*') => {}
                ListType::Ordered if facts.bullet_char == 0 && facts.start <= 999_999_999 => {}
                _ => {
                    return Err(ParseError::Invariant(
                        "direct pause list facts are donor-reachable",
                    ));
                }
            }
            Ok(BlockKind::List(ListData {
                list_type: facts.list_type,
                marker_offset: 0,
                padding: 0,
                start: usize::try_from(facts.start)
                    .map_err(|_| ParseError::Invariant("direct list start fits usize"))?,
                delimiter: facts.delimiter,
                bullet_char: facts.bullet_char,
                tight: false,
                task_checked: None,
            }))
        }
        DirectBlockKind::Item(facts) => {
            if facts.marker_offset > 3 || !(2..=14).contains(&facts.padding) {
                return Err(ParseError::Invariant(
                    "direct pause item facts are donor-reachable",
                ));
            }
            Ok(BlockKind::Item(ListData {
                list_type: ListType::Bullet,
                marker_offset: usize::from(facts.marker_offset),
                padding: usize::from(facts.padding),
                start: 1,
                delimiter: ListDelimiter::Period,
                bullet_char: b'-',
                tight: false,
                task_checked: facts.task_checked,
            }))
        }
        DirectBlockKind::Paragraph => Ok(BlockKind::Paragraph),
        DirectBlockKind::Heading(facts) if facts.setext && matches!(facts.level, 1 | 2) => {
            Ok(BlockKind::Heading {
                level: facts.level,
                setext: true,
                closed: false,
            })
        }
        DirectBlockKind::Heading(facts) if !facts.setext && (1..=6).contains(&facts.level) => {
            Ok(BlockKind::Heading {
                level: facts.level,
                setext: false,
                // Accepted closing-marker state is exhausted with the owned
                // physical line and has no suffix grammar effect.
                closed: false,
            })
        }
        DirectBlockKind::Heading(_) => Err(ParseError::Invariant(
            "direct pause heading facts are donor-reachable",
        )),
        DirectBlockKind::IndentedCode => Ok(BlockKind::CodeBlock {
            fenced: false,
            fence_char: 0,
            fence_length: 0,
            fence_offset: 0,
            info: LogicalProjection::default(),
            literal: LogicalProjection::default(),
            closed: true,
        }),
        DirectBlockKind::FencedCode(facts) => {
            if facts.minimum_closing_length < 3 || facts.fence_offset_columns > 3 {
                return Err(ParseError::Invariant(
                    "direct pause fence facts are donor-reachable",
                ));
            }
            Ok(BlockKind::CodeBlock {
                fenced: true,
                fence_char: facts.fence.marker(),
                fence_length: usize::try_from(facts.minimum_closing_length)
                    .map_err(|_| ParseError::Invariant("direct fence length fits usize"))?,
                fence_offset: usize::from(facts.fence_offset_columns),
                info: LogicalProjection::default(),
                literal: LogicalProjection::default(),
                closed: false,
            })
        }
        DirectBlockKind::HtmlBlock(facts) if (1..=7).contains(&facts.block_type) => {
            Ok(BlockKind::HtmlBlock {
                block_type: facts.block_type,
                literal: LogicalProjection::default(),
            })
        }
        DirectBlockKind::HtmlBlock(_) => Err(ParseError::Invariant(
            "direct pause HTML type is donor-reachable",
        )),
        DirectBlockKind::ThematicBreak => Err(ParseError::Invariant(
            "a thematic break never enters a direct pause",
        )),
    }
}

fn direct_block_kind(kind: &BlockKind) -> Result<DirectBlockKind, ParseError> {
    match kind {
        BlockKind::Document => Ok(DirectBlockKind::Document),
        BlockKind::BlockQuote => Ok(DirectBlockKind::BlockQuote),
        BlockKind::List(list) => Ok(DirectBlockKind::List(DirectListFacts {
            list_type: list.list_type,
            start: u32::try_from(list.start)
                .map_err(|_| ParseError::Invariant("direct list start below u32"))?,
            delimiter: list.delimiter,
            bullet_char: list.bullet_char,
        })),
        BlockKind::Item(item) => Ok(DirectBlockKind::Item(DirectItemFacts {
            marker_offset: u16::try_from(item.marker_offset)
                .map_err(|_| ParseError::Invariant("direct item marker offset below u16"))?,
            padding: u16::try_from(item.padding)
                .map_err(|_| ParseError::Invariant("direct item padding below u16"))?,
            task_checked: item.task_checked,
        })),
        BlockKind::Paragraph => Ok(DirectBlockKind::Paragraph),
        BlockKind::Heading { level, setext, .. }
            if (*setext && matches!(*level, 1 | 2)) || (!*setext && (1..=6).contains(level)) =>
        {
            Ok(DirectBlockKind::Heading(DirectHeadingFacts {
                level: *level,
                setext: *setext,
            }))
        }
        BlockKind::CodeBlock {
            fenced: true,
            fence_char,
            fence_length,
            fence_offset,
            ..
        } => {
            let fence = match fence_char {
                b'`' => DirectFenceCharacter::Backtick,
                b'~' => DirectFenceCharacter::Tilde,
                _ => {
                    return Err(ParseError::Invariant(
                        "direct fenced code has a valid marker character",
                    ));
                }
            };
            let minimum_closing_length = u64::try_from(*fence_length)
                .map_err(|_| ParseError::Invariant("direct fence length below u64"))?;
            if minimum_closing_length < 3 {
                return Err(ParseError::Invariant(
                    "direct fence minimum closing length is at least three",
                ));
            }
            let fence_offset_columns = u8::try_from(*fence_offset)
                .map_err(|_| ParseError::Invariant("direct fence offset below u8"))?;
            if fence_offset_columns > 3 {
                return Err(ParseError::Invariant(
                    "direct fence offset is at most three columns",
                ));
            }
            Ok(DirectBlockKind::FencedCode(DirectFencedCodeFacts {
                fence,
                minimum_closing_length,
                fence_offset_columns,
            }))
        }
        BlockKind::CodeBlock { fenced: false, .. } => Ok(DirectBlockKind::IndentedCode),
        BlockKind::HtmlBlock { block_type, .. } if (1..=7).contains(block_type) => {
            Ok(DirectBlockKind::HtmlBlock(DirectHtmlBlockFacts {
                block_type: *block_type,
            }))
        }
        BlockKind::ThematicBreak => Ok(DirectBlockKind::ThematicBreak),
        _ => Err(ParseError::DirectUnsupported(DirectUnsupported::BlockKind)),
    }
}

fn direct_final_facts(kind: &BlockKind) -> DirectFinalFacts {
    match kind {
        BlockKind::List(list) => DirectFinalFacts::List { tight: list.tight },
        BlockKind::CodeBlock {
            fenced: true,
            closed,
            ..
        } => DirectFinalFacts::FencedCode(DirectFencedCodeCloseFacts { closed: *closed }),
        _ => DirectFinalFacts::None,
    }
}

fn allocate_direct_parser_instance_id() -> Result<u64, ParseError> {
    DIRECT_PARSER_INSTANCE_IDS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| ParseError::Invariant("direct parser instance identity space exhausted"))
}

impl DirectValueBlockParser {
    /// Whole-`String` compatibility ceiling for `begin_line`.
    ///
    /// This is not a physical-line product limit: `begin_source_line_work`
    /// scans larger lines in bounded segments and installs their bounded
    /// controller window into the same [`LineTransition`].
    pub const MAX_LINE_BYTES: usize = DIRECT_MAX_LINE_BYTES;

    pub fn new(profile: SyntaxProfile) -> Result<Self, ParseError> {
        let mut parser = ValueBlockParser::new(profile);
        parser.defer_output_repairs = true;
        parser.direct = Some(DirectHooks::new());
        parser
            .direct
            .as_mut()
            .expect("direct hooks installed")
            .push_command(
                DirectCommand::Open {
                    kind: DirectBlockKind::Document,
                },
                Some(DirectStackEffect::Push(parser.tree.root)),
            )?;
        Ok(Self {
            parser,
            line_work: None,
            finish_work: None,
            line_complete: false,
            finished: false,
            source_line_instance_id: allocate_direct_parser_instance_id()?,
            next_source_line_admission: 1,
            active_source_line_admission: None,
        })
    }

    /// Reports whether the currently open Paragraph can still begin with one
    /// or more link-reference definitions.
    ///
    /// Callers use this only at an acknowledged physical-line boundary. Once
    /// false, the donor will not request reference-prefix replay for the
    /// current Paragraph, so a compact output may discard its provisional
    /// event window without guessing from source text.
    #[doc(hidden)]
    pub fn paragraph_may_have_reference_prefix(&self) -> Result<bool, ParseError> {
        if !self.line_complete
            || self.finished
            || self.line_work.is_some()
            || self.finish_work.is_some()
            || self.active_source_line_admission.is_some()
        {
            return Err(ParseError::Invariant(
                "reference-prefix possibility is observed at a quiescent line boundary",
            ));
        }
        let direct = self
            .parser
            .direct
            .as_ref()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        Ok(direct.paragraph_may_have_reference_prefix)
    }

    /// Capture the parser half of a direct restart immediately after an
    /// acknowledged [`DirectCommand::FinishLine`].
    ///
    /// The writer half is intentionally absent. This value can prove parser
    /// command equivalence, but cannot by itself authorize a production resume.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] unless the parser is in the exact
    /// quiescent state following acknowledgement of `FinishLine`, or if its
    /// compact scratch contains state outside the supported direct slice.
    #[doc(hidden)]
    #[allow(clippy::too_many_lines)]
    pub fn capture_line_boundary_pause(&self) -> Result<DirectLineBoundaryPause, ParseError> {
        match self.capture_pause(false)? {
            DirectLineBoundaryPauseCapture::Available(pause) => Ok(pause),
            DirectLineBoundaryPauseCapture::Unavailable => Err(ParseError::Invariant(
                "direct pause boundary has a resumable representation",
            )),
        }
    }

    /// Captures a restart sample when the acknowledged boundary is representable
    /// by the current codec. A valid but non-resumable boundary is reported as
    /// [`DirectLineBoundaryPauseCapture::Unavailable`]; parser and codec
    /// invariants continue to propagate as errors.
    #[doc(hidden)]
    pub fn capture_line_boundary_pause_if_available(
        &self,
    ) -> Result<DirectLineBoundaryPauseCapture, ParseError> {
        self.capture_pause(false)
    }

    /// Captures the canonical Document-only parser state before physical line
    /// one. This is the BOF counterpart to a line-boundary pause and exists so
    /// the first sparse checkpoint interval can be reparsed and converged
    /// without a whole-document fallback.
    #[doc(hidden)]
    pub fn capture_document_start_pause(&self) -> Result<DirectLineBoundaryPause, ParseError> {
        match self.capture_pause(true)? {
            DirectLineBoundaryPauseCapture::Available(pause) => Ok(pause),
            DirectLineBoundaryPauseCapture::Unavailable => Err(ParseError::Invariant(
                "direct document-start pause is resumable",
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn capture_pause(
        &self,
        allow_document_start: bool,
    ) -> Result<DirectLineBoundaryPauseCapture, ParseError> {
        let Self {
            parser,
            line_work,
            finish_work,
            line_complete,
            finished,
            source_line_instance_id: _,
            next_source_line_admission: _,
            active_source_line_admission,
        } = self;
        let is_document_start =
            allow_document_start && parser.line_number == 0 && parser.last_line_length == 0;
        if (!*line_complete && !is_document_start)
            || *finished
            || line_work.is_some()
            || finish_work.is_some()
            || active_source_line_admission.is_some()
        {
            return Err(ParseError::Invariant(
                "direct pause follows an acknowledged unfinished document line",
            ));
        }

        // Fields explicitly matched as `_` are line-local scratch recreated by
        // `ValueBlockParser::new`. The exhaustive pattern makes additions to
        // the donor state require a fresh pause audit at compile time.
        let ValueBlockParser {
            profile,
            tree,
            references,
            current,
            line_number,
            line_leaf_id: _,
            offset: _,
            column: _,
            thematic_break_kill_pos: _,
            first_nonspace: _,
            first_nonspace_column: _,
            indent: _,
            blank: _,
            partially_consumed_tab: _,
            curline_len: _,
            curline_end_col: _,
            last_line_length,
            defer_output_repairs,
            opened_this_line,
            direct,
            direct_segmented_line,
            #[cfg(test)]
                open_new_scheduler: _,
        } = parser;
        if !*defer_output_repairs
            || !references.is_empty()
            || !opened_this_line.is_empty()
            || !tree.events.is_empty()
            || direct_segmented_line.is_some()
            || (*line_number == 0 && !is_document_start)
        {
            return Err(ParseError::Invariant(
                "direct pause has canonical parser-owned boundary state",
            ));
        }
        let direct = direct
            .as_ref()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        // Recipe cursors and allocation scratch are likewise resettable. Keep
        // this pattern exhaustive so hook additions cannot bypass review.
        let DirectHooks {
            commands,
            pending_stack_effect,
            emission_stack,
            previous,
            retired,
            old_source,
            body,
            emission_phase,
            old_source_index: _,
            old_depth_by_node: _,
            old_last_use: _,
            recipe_sealed: _,
            recipe_line_bytes: _,
            intent_limit: _,
            pending_gap_at_line_start,
            pending_gap_floor_at_line_start,
            claimed_offset: _,
            line_marker_floor,
            pending_terminator,
            pending_blank_gap,
            pending_blank_gap_floor,
            paragraph_has_content,
            paragraph_may_have_reference_prefix,
            pending_external_work,
            reference_work_id,
            reference_finalize_resume_once,
            reference_current_rebase,
            next_reference_rendezvous: _,
            #[cfg(test)]
                retired_insertions: _,
            #[cfg(test)]
                retired_stack_probes: _,
        } = direct;
        if !commands.is_empty()
            || pending_stack_effect.is_some()
            || !previous.is_empty()
            || !retired.is_empty()
            || !old_source.is_empty()
            || !body.is_empty()
            || *emission_phase != DirectEmissionPhase::Complete
            || *pending_gap_at_line_start
            || pending_gap_floor_at_line_start.is_some()
            || pending_external_work.is_some()
            || reference_work_id.is_some()
            || reference_finalize_resume_once.is_some()
            || reference_current_rebase.is_some()
            || line_marker_floor.is_some()
        {
            return Err(ParseError::Invariant(
                "direct pause follows quiescent recipe emission",
            ));
        }
        // The command stack already is the claimed open path. Validate that
        // borrowed slice exhaustively against the compact tree below instead
        // of allocating a second copy just to rediscover it.
        let path = emission_stack.as_slice();
        if path.first().copied() != Some(tree.root) || tree.nodes.len() != path.len() {
            return Err(ParseError::Invariant(
                "direct pause tree is exactly its emitted open path",
            ));
        }
        let current_frame = path
            .iter()
            .position(|candidate| candidate == current)
            .ok_or(ParseError::Invariant(
                "direct pause current is on the open path",
            ))?;
        let floor_depth =
            pending_blank_gap_floor
                .map(|floor| {
                    path.iter().position(|candidate| *candidate == floor).ok_or(
                        ParseError::Invariant("direct pause blank floor is on the open path"),
                    )
                })
                .transpose()?;
        if floor_depth.is_some() && !*pending_blank_gap {
            return Err(ParseError::Invariant(
                "direct pause blank floor accompanies a pending gap",
            ));
        }
        if *pending_terminator && *pending_blank_gap {
            return Err(ParseError::Invariant(
                "direct pause has one deferred line-boundary source role",
            ));
        }

        let mut frames = Vec::new();
        frames
            .try_reserve_exact(path.len())
            .map_err(|_| ParseError::Invariant("direct pause frame allocation failed"))?;
        let mut restart_available = true;
        for (depth, id) in path.iter().copied().enumerate() {
            if id.index() != depth {
                return Err(ParseError::Invariant(
                    "direct pause frame is compact bounded scratch",
                ));
            }
            let node = tree.node(id);
            let expected_parent = depth.checked_sub(1).map(|parent| path[parent]);
            let expected_child = path.get(depth + 1).copied();
            let child_shape_matches = match expected_child {
                Some(child) => node.children.len() == 1 && node.children[0] == child,
                None => node.children.is_empty(),
            };
            if node.id != id
                || node.parent != expected_parent
                || !child_shape_matches
                || !node.open
                || node.folded_children != 0
                || node.table_visited
                || node.table_autocompleted_cells != 0
                || !node.content.logical.is_empty()
                || !node.content.origins.is_empty()
                || !node.content.line_offsets.is_empty()
                || node.content.source_backed.is_some()
            {
                return Err(ParseError::Invariant(
                    "direct pause frame is compact bounded scratch",
                ));
            }
            if let Some(kind) = capture_direct_pause_kind(&node.kind)? {
                if depth > 0 && kind == DirectBlockKind::Document {
                    return Err(ParseError::Invariant(
                        "direct pause document is the root frame",
                    ));
                }
                frames.push(DirectPauseFrame {
                    kind,
                    last_line_blank: node.last_line_blank,
                    closed_children: node.historical_children,
                });
            } else {
                restart_available = false;
            }
        }
        if !restart_available {
            return Ok(DirectLineBoundaryPauseCapture::Unavailable);
        }
        if frames.first().map(|frame| frame.kind) != Some(DirectBlockKind::Document) {
            return Err(ParseError::Invariant(
                "direct pause starts with the document frame",
            ));
        }
        let terminal_is_paragraph = frames
            .last()
            .is_some_and(|frame| frame.kind == DirectBlockKind::Paragraph);
        let terminal_is_indented_code = frames
            .last()
            .is_some_and(|frame| frame.kind == DirectBlockKind::IndentedCode);
        if terminal_is_paragraph && !*paragraph_has_content {
            return Err(ParseError::Invariant(
                "direct pause paragraph deferred state targets its terminal frame",
            ));
        }
        if *pending_terminator && !(terminal_is_paragraph || terminal_is_indented_code) {
            return Err(ParseError::Invariant(
                "direct pause terminator targets an open paragraph or indented code",
            ));
        }
        if let Some(depth) = floor_depth
            && !matches!(
                frames[depth].kind,
                DirectBlockKind::BlockQuote | DirectBlockKind::Item(_)
            )
        {
            return Err(ParseError::Invariant(
                "direct pause marked blank floor is a container marker owner",
            ));
        }
        let paragraph = terminal_is_paragraph
            .then(|| {
                Ok::<_, ParseError>(DirectPauseParagraphState {
                    frame_depth: frames
                        .len()
                        .checked_sub(1)
                        .ok_or(ParseError::Invariant("direct paragraph has one open frame"))?,
                    has_visible_content: *paragraph_has_content,
                    may_have_reference_prefix: *paragraph_may_have_reference_prefix,
                })
            })
            .transpose()?;

        if is_document_start
            && (frames.len() != 1
                || current_frame != 0
                || frames[0].last_line_blank
                || frames[0].closed_children != crate::tree::ChildSequenceFold::default()
                || *pending_terminator
                || *pending_blank_gap
                || floor_depth.is_some()
                || paragraph.is_some())
        {
            return Err(ParseError::Invariant(
                "direct document-start pause is canonical",
            ));
        }

        let deferred = DirectDeferredState {
            terminator: *pending_terminator,
            blank_gap: *pending_blank_gap,
            blank_gap_floor: floor_depth,
        };
        if !direct_pause_line_local_output_is_available(&frames, current_frame, deferred) {
            return Ok(DirectLineBoundaryPauseCapture::Unavailable);
        }

        Ok(DirectLineBoundaryPauseCapture::Available(
            DirectLineBoundaryPause {
                schema: DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA,
                profile: *profile,
                cursor: DirectPauseCursor {
                    line_number: *line_number,
                    last_line_length: *last_line_length,
                },
                current_frame,
                frames: frames.into_boxed_slice(),
                deferred,
                paragraph,
            },
        ))
    }

    /// Capture and split the in-memory direct pause into grammar equality and
    /// current-output halves. The caller must independently retain or remint
    /// the source cursor used for reconstruction.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] whenever line-boundary capture is not
    /// legal or the grammar projection cannot be allocated.
    #[doc(hidden)]
    pub fn capture_restart_parts(
        &self,
    ) -> Result<(DirectGrammarContinuation, DirectRestartOutput), ParseError> {
        self.capture_line_boundary_pause()?.into_restart_parts()
    }

    /// Encode only donor-owned semantic continuation state at the current
    /// acknowledged physical-line boundary. The fixed header is retained per
    /// sample; the opaque fixed-size frame records are transient input to the
    /// consumer's persistent shared open-path sequence. Neither contains a
    /// source payload, source identity, parser `NodeId`, writer binding, or
    /// consumer build identity.
    ///
    /// The caller must still join this parser recipe with its independently
    /// authenticated writer/source checkpoint before publication or resume.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if capture is not currently legal,
    /// semantic state is outside the direct slice, or the bounded split record
    /// materialization cannot be encoded.
    #[doc(hidden)]
    pub fn capture_durable_line_boundary_checkpoint(
        &self,
    ) -> Result<DirectDurableLineBoundaryCapture, ParseError> {
        let pause = self.capture_line_boundary_pause()?;
        direct_pause_to_durable_capture(&pause)
    }

    /// Capture only suffix-persistable grammar/control plus the predecessor
    /// line-local blankness that source lineage can authorize independently.
    /// Cumulative child folds and display facts are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if line-boundary capture is illegal,
    /// grammar projection is outside the direct slice, or bounded path
    /// materialization cannot be allocated.
    #[doc(hidden)]
    pub fn capture_durable_grammar_line_boundary_checkpoint(
        &self,
    ) -> Result<DirectDurableGrammarCapture, ParseError> {
        let pause = self.capture_line_boundary_pause()?;
        direct_pause_to_durable_grammar_capture(&pause)
    }

    /// Rebuild parser scratch from the parser-only line-boundary proof value.
    /// The caller remains responsible for restoring and validating the matching
    /// writer continuation before consuming any resulting command.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if the opaque pause has an incompatible
    /// schema/profile or fails any path, kind, cursor, or deferred-state check.
    #[doc(hidden)]
    #[allow(clippy::too_many_lines)]
    pub fn resume_line_boundary_pause(pause: DirectLineBoundaryPause) -> Result<Self, ParseError> {
        let DirectLineBoundaryPause {
            schema,
            profile,
            cursor,
            current_frame,
            frames,
            deferred,
            paragraph,
        } = pause;
        let is_document_start = cursor.line_number == 0;
        if schema != DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA
            || frames.is_empty()
            || u32::try_from(frames.len()).is_err()
            || current_frame.checked_add(1) != Some(frames.len())
            || frames[0].kind != DirectBlockKind::Document
            || cursor.line_number == usize::MAX
            || (is_document_start && cursor.last_line_length != 0)
        {
            return Err(ParseError::Invariant(
                "direct line-boundary pause header is valid",
            ));
        }
        let DirectDeferredState {
            terminator: pending_terminator,
            blank_gap: pending_blank_gap,
            blank_gap_floor: pending_blank_gap_floor,
        } = deferred;
        if pending_blank_gap_floor.is_some_and(|depth| depth >= frames.len())
            || (pending_blank_gap_floor.is_some() && !pending_blank_gap)
            || (pending_terminator && pending_blank_gap)
        {
            return Err(ParseError::Invariant(
                "direct line-boundary pause deferred state is valid",
            ));
        }

        let mut kinds = Vec::new();
        kinds
            .try_reserve_exact(frames.len())
            .map_err(|_| ParseError::Invariant("direct pause kind allocation failed"))?;
        for (depth, frame) in frames.iter().enumerate() {
            if depth > 0 && frame.kind == DirectBlockKind::Document {
                return Err(ParseError::Invariant(
                    "direct pause document is the root frame",
                ));
            }
            kinds.push(direct_pause_block_kind(frame.kind)?);
        }
        for pair in kinds.windows(2) {
            if !pair[0].can_contain(&pair[1]) {
                return Err(ParseError::Invariant(
                    "direct pause frames form a valid open block path",
                ));
            }
        }
        let terminal_is_paragraph = matches!(kinds.last(), Some(BlockKind::Paragraph));
        let terminal_is_indented_code = matches!(
            kinds.last(),
            Some(BlockKind::CodeBlock { fenced: false, .. })
        );
        let paragraph_has_content = match (terminal_is_paragraph, paragraph) {
            (true, Some(paragraph))
                if paragraph.frame_depth == frames.len() - 1 && paragraph.has_visible_content =>
            {
                true
            }
            (false, None) => false,
            _ => {
                return Err(ParseError::Invariant(
                    "direct pause provisional Paragraph state targets the terminal frame",
                ));
            }
        };
        if is_document_start
            && (frames.len() != 1
                || current_frame != 0
                || frames[0].last_line_blank
                || frames[0].closed_children != crate::tree::ChildSequenceFold::default()
                || pending_terminator
                || pending_blank_gap
                || pending_blank_gap_floor.is_some()
                || paragraph.is_some())
        {
            return Err(ParseError::Invariant(
                "direct document-start pause is canonical",
            ));
        }
        if pending_terminator && !(paragraph_has_content || terminal_is_indented_code) {
            return Err(ParseError::Invariant(
                "direct pause terminator targets an open paragraph or indented code",
            ));
        }
        if let Some(depth) = pending_blank_gap_floor
            && !matches!(
                frames[depth].kind,
                DirectBlockKind::BlockQuote | DirectBlockKind::Item(_)
            )
        {
            return Err(ParseError::Invariant(
                "direct pause marked blank floor is a container marker owner",
            ));
        }

        // `ValueBlockParser::new` retains its existing fixed one-root
        // allocation. Every pause-depth-proportional allocation below uses
        // `try_reserve_exact`; failure drops the private partial rebuild.
        let mut parser = ValueBlockParser::new(profile);
        parser.defer_output_repairs = true;
        parser
            .tree
            .nodes
            .try_reserve_exact(frames.len() - 1)
            .map_err(|_| ParseError::Invariant("direct pause tree allocation failed"))?;
        let root = parser.tree.root;
        let mut path = Vec::new();
        path.try_reserve_exact(frames.len())
            .map_err(|_| ParseError::Invariant("direct pause path allocation failed"))?;
        path.push(root);
        {
            let root_frame = frames[0];
            let root_node = parser.tree.node_mut(root);
            root_node.kind = kinds[0].clone();
            root_node.last_line_blank = root_frame.last_line_blank;
            root_node.historical_children = root_frame.closed_children;
        }
        for depth in 1..frames.len() {
            let parent = path[depth - 1];
            parser
                .tree
                .node_mut(parent)
                .children
                .try_reserve_exact(1)
                .map_err(|_| ParseError::Invariant("direct pause child allocation failed"))?;
            // Donor line/column positions are deliberately not control state.
            // The direct writer is the source-position authority.
            let node =
                parser
                    .tree
                    .append_scratch(parent, kinds[depth].clone(), Position::new(1, 1));
            let frame = frames[depth];
            let scratch = parser.tree.node_mut(node);
            scratch.last_line_blank = frame.last_line_blank;
            scratch.historical_children = frame.closed_children;
            path.push(node);
        }
        parser.current = path[current_frame];
        parser.line_number = cursor.line_number;
        parser.last_line_length = cursor.last_line_length;

        let pending_blank_gap_floor = pending_blank_gap_floor.map(|depth| path[depth]);
        let mut direct = DirectHooks::new();
        direct.emission_stack = path;
        direct.pending_terminator = pending_terminator;
        direct.pending_blank_gap = pending_blank_gap;
        direct.pending_blank_gap_floor = pending_blank_gap_floor;
        direct.paragraph_has_content = paragraph_has_content;
        direct.paragraph_may_have_reference_prefix =
            paragraph.is_some_and(|paragraph| paragraph.may_have_reference_prefix);
        parser.direct = Some(direct);

        Ok(Self {
            parser,
            line_work: None,
            finish_work: None,
            line_complete: true,
            finished: false,
            source_line_instance_id: allocate_direct_parser_instance_id()?,
            next_source_line_admission: 1,
            active_source_line_admission: None,
        })
    }

    /// Reconstruct and resume from grammar plus independently selected current
    /// output. The donor validates header and exact output-to-grammar
    /// projection, then rebuilds from every fact in `output`; grammar is never
    /// used to synthesize omitted output facts.
    ///
    /// Temporal currentness is outside donor semantics. The composite
    /// source/writer/green authority must select `output` before this call.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for a crossed or malformed join, an
    /// invalid cursor, or any pause state outside the supported direct slice.
    #[doc(hidden)]
    pub fn resume_restart_parts(
        grammar: &DirectGrammarContinuation,
        output: DirectRestartOutput,
        cursor: DirectLineBoundaryResumeCursor,
    ) -> Result<Self, ParseError> {
        let pause = direct_restart_parts_into_pause(grammar, output, cursor)?;
        Self::resume_line_boundary_pause(pause)
    }

    /// Decode one coordinate-free durable v2 sample into grammar equality and
    /// its opaque historical output half without rebinding a source cursor or
    /// constructing parser scratch.
    ///
    /// This is a lookup seam, not reuse authority. Consumers may compare the
    /// returned grammar and borrow the output's narrow line-local view, but a
    /// composite coordinator must still authenticate source lineage and build
    /// current output before resume.
    ///
    /// The same canonical header/frame decoders, exact count/order checksum,
    /// donor-reachability checks, and output-to-grammar projection used by the
    /// resume path are applied here.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for an unknown schema, corruption,
    /// missing/reordered records, malformed path or facts, or an internally
    /// inconsistent grammar/output split.
    #[doc(hidden)]
    pub fn decode_durable_restart_parts<I>(
        header: DirectDurableLineBoundaryHeader,
        records: I,
    ) -> Result<(DirectGrammarContinuation, DirectRestartOutput), ParseError>
    where
        I: IntoIterator<Item = DirectDurableLineBoundaryFrameRecord>,
    {
        let output = decode_direct_durable_restart_output(header, records)?;
        direct_restart_output_into_parts(output)
    }

    /// Decode the suffix-safe durable contract into grammar equality plus an
    /// opaque predecessor-line-local half. The latter must not be bound to
    /// current output until source lineage authorizes its sampled line.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for an unknown schema, corruption,
    /// malformed path/control facts, or noncanonical line-local blankness.
    #[doc(hidden)]
    pub fn decode_durable_grammar_restart_parts<I>(
        header: DirectDurableGrammarHeader,
        records: I,
    ) -> Result<
        (
            DirectGrammarContinuation,
            DirectRestartLineLocalContinuation,
        ),
        ParseError,
    >
    where
        I: IntoIterator<Item = DirectDurableGrammarFrameRecord>,
    {
        decode_direct_durable_grammar_parts(header, records)
    }

    /// Decode a donor-owned durable recipe and rebuild a fresh direct parser.
    /// The decoder owns the wire schema and fails closed on unknown versions,
    /// malformed facts, noncanonical states, missing/reordered records, or
    /// checksum mismatch;
    /// consumers never reconstruct donor frames themselves. `cursor` is an
    /// independently measured, validated current-source prefix receipt. Its
    /// positive line ordinal and previous-line length are rebound into the
    /// fresh parser; neither scalar is stored in or compared with the durable
    /// semantic recipe.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] for any incompatible or corrupted
    /// checkpoint.
    #[doc(hidden)]
    pub fn resume_durable_line_boundary_checkpoint<I>(
        header: DirectDurableLineBoundaryHeader,
        records: I,
        cursor: DirectLineBoundaryResumeCursor,
    ) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = DirectDurableLineBoundaryFrameRecord>,
    {
        let pause = direct_durable_parts_into_pause(header, records, cursor)?;
        Self::resume_line_boundary_pause(pause)
    }

    #[must_use]
    pub fn pending_external_work(&self) -> Option<&DirectExternalWork> {
        self.parser
            .direct
            .as_ref()
            .and_then(|direct| direct.pending_external_work.as_ref())
    }

    /// Consume the recognition half of the active parser rendezvous and mint
    /// one non-cloneable DFA bound to this parser, request, and logical-source
    /// identity.  A second mint is rejected until the terminal work is joined.
    pub fn begin_reference_prefix_work<I: Copy + Eq>(
        &mut self,
        request: DirectReferencePrefixRequest,
        source_identity: I,
    ) -> Result<DirectReferencePrefixWork<I>, ParseError> {
        if self.pending_command().is_some() {
            return Err(ParseError::Invariant(
                "reference work begins before command emission",
            ));
        }
        let direct = self
            .parser
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.reference_work_id.is_some()
            || !matches!(
                direct.pending_external_work,
                Some(DirectExternalWork::ReferencePrefixFinalizer {
                    request: pending
                }) if pending == request
            )
        {
            return Err(ParseError::Invariant(
                "reference work matches one unminted parser rendezvous",
            ));
        }
        let work = DirectReferencePrefixWork::new(
            self.source_line_instance_id,
            request.rendezvous_id,
            source_identity,
            request.logical_base,
        );
        direct.reference_work_id = Some(work.work_id());
        Ok(work)
    }

    /// Join a writer-authenticated terminal result back into the interrupted
    /// block transition. Ordered occurrences were already published one at a
    /// time while the actor retained the non-cloneable work.
    pub fn commit_reference_prefix_terminal<I: Copy + Eq>(
        &mut self,
        ack: DirectReferencePrefixTerminalAck<I>,
        expected_source_identity: I,
    ) -> Result<DirectReferencePrefixCommitStatus, ParseError> {
        let direct = self
            .parser
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        let request = match &direct.pending_external_work {
            Some(DirectExternalWork::ReferencePrefixFinalizer { request }) => *request,
            None => {
                return Err(ParseError::Invariant(
                    "reference finalizer joins its pending parser request",
                ));
            }
        };
        if direct.reference_work_id != Some(ack.work_id)
            || ack.parser_instance_id != self.source_line_instance_id
            || ack.rendezvous_id != request.rendezvous_id
            || ack.source_identity != expected_source_identity
        {
            return Err(ParseError::Invariant(
                "reference terminal belongs to this parser, work, request, and source",
            ));
        }
        let terminal = ack.terminal;
        let disposition_count_matches = match terminal.disposition {
            DirectReferencePrefixDisposition::NoDefinitions => terminal.definition_count == 0,
            DirectReferencePrefixDisposition::ReferenceOnly
            | DirectReferencePrefixDisposition::VisibleRemainder => terminal.definition_count > 0,
        };
        if !disposition_count_matches
            || terminal.logical_reference_prefix.bytes.start != request.logical_base.bytes
            || terminal.logical_reference_prefix.utf16.start != request.logical_base.utf16
            || terminal.logical_recognition.bytes.start != request.logical_base.bytes
            || terminal.logical_recognition.utf16.start != request.logical_base.utf16
        {
            return Err(ParseError::Invariant(
                "reference terminal disposition and logical cuts are canonical",
            ));
        }
        direct.reference_work_id = None;
        direct.pending_external_work = None;
        direct.reference_finalize_resume_once = Some(terminal.disposition);
        Ok(match terminal.disposition {
            DirectReferencePrefixDisposition::NoDefinitions => {
                DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
            }
            DirectReferencePrefixDisposition::VisibleRemainder => {
                DirectReferencePrefixCommitStatus::VisibleRemainderArmed
            }
            DirectReferencePrefixDisposition::ReferenceOnly => {
                DirectReferencePrefixCommitStatus::ReferenceOnlyArmed
            }
        })
    }

    /// Captures the donor-owned continuation at the internal cut between a
    /// leading reference-definition prefix and its visible Paragraph suffix.
    ///
    /// This is intentionally narrower than a general mid-block checkpoint:
    /// only the top-level `Document -> Paragraph` shape reached immediately
    /// after committing `VisibleRemainder` is accepted. The source-owning
    /// consumer must independently bind the physical cursor at the
    /// rendezvous-authenticated prefix end. A valid visible-remainder parse
    /// returns `None` when a later line is active or that narrow shape is not
    /// present; checkpoint ineligibility never invalidates the parse itself.
    #[doc(hidden)]
    pub fn capture_leading_reference_remainder_continuation(
        &self,
    ) -> Result<Option<DirectLeadingReferenceRemainderContinuation>, ParseError> {
        if self.pending_command().is_some()
            || self.line_work.is_some()
            || self.finished
            || self.active_source_line_admission.is_some()
        {
            return Ok(None);
        }
        let direct = self
            .parser
            .direct
            .as_ref()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if direct.reference_finalize_resume_once
            != Some(DirectReferencePrefixDisposition::VisibleRemainder)
            || direct.pending_external_work.is_some()
            || direct.reference_work_id.is_some()
        {
            return Err(ParseError::Invariant(
                "leading-reference remainder follows its committed parser terminal",
            ));
        }
        if direct.emission_stack.len() != 2
            || direct.emission_stack.first().copied() != Some(self.parser.tree.root)
            || direct.emission_stack.last().copied() != Some(self.parser.current)
            || self.parser.tree.nodes.len() != 2
        {
            return Ok(None);
        }
        let root = self.parser.tree.node(direct.emission_stack[0]);
        let paragraph = self.parser.tree.node(direct.emission_stack[1]);
        if !root.open
            || !paragraph.open
            || root.kind != BlockKind::Document
            || paragraph.kind != BlockKind::Paragraph
            || paragraph.parent != Some(root.id)
            || root.children.as_slice() != [paragraph.id]
            || !paragraph.children.is_empty()
        {
            return Ok(None);
        }
        let output = DirectRestartOutput {
            schema: DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA,
            profile: self.parser.profile,
            current_frame: 1,
            frames: vec![
                DirectPauseFrame {
                    kind: DirectBlockKind::Document,
                    last_line_blank: false,
                    closed_children: root.historical_children,
                },
                DirectPauseFrame {
                    kind: DirectBlockKind::Paragraph,
                    last_line_blank: false,
                    closed_children: paragraph.historical_children,
                },
            ]
            .into_boxed_slice(),
            // The removed definition terminator is already represented by
            // Green Gap coverage. Re-emitting it would create a logical
            // newline before the visible suffix.
            deferred: DirectDeferredState::default(),
            paragraph: Some(DirectPauseParagraphState {
                frame_depth: 1,
                has_visible_content: true,
                may_have_reference_prefix: false,
            }),
        };
        let (grammar, output) = direct_restart_output_into_parts(output)?;
        Ok(Some(DirectLeadingReferenceRemainderContinuation {
            grammar,
            output,
        }))
    }

    #[must_use]
    pub fn pending_command(&self) -> Option<&DirectCommand> {
        self.parser
            .direct
            .as_ref()
            .and_then(|direct| direct.commands.front())
    }

    /// Acknowledge exactly the currently visible command.
    ///
    /// Parser grammar never advances while an unacknowledged command exists.
    pub fn acknowledge_command(&mut self) -> Result<(), ParseError> {
        let command = {
            let direct = self
                .parser
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            let command = direct
                .commands
                .pop_front()
                .ok_or(ParseError::Invariant("no direct command is pending"))?;
            direct.acknowledge_stack_effect()?;
            command
        };
        match command {
            DirectCommand::FinishLine { .. } => {
                let work = self
                    .line_work
                    .take()
                    .ok_or(ParseError::Invariant("finish-line command has line work"))?;
                if !work.semantic_complete || !work.finish_queued {
                    return Err(ParseError::Invariant(
                        "finish-line command follows semantic completion",
                    ));
                }
                let direct = self
                    .parser
                    .direct
                    .as_ref()
                    .ok_or(ParseError::Invariant("direct hooks are present"))?;
                if direct.emission_phase != DirectEmissionPhase::Complete
                    || !direct.recipe_is_empty()
                {
                    return Err(ParseError::Invariant(
                        "FinishLine follows complete recipe emission",
                    ));
                }
                self.parser.compact_direct_scratch()?;
                self.parser.direct_segmented_line = None;
                self.line_complete = true;
            }
            DirectCommand::FinishDocument => {
                let work = self.finish_work.take().ok_or(ParseError::Invariant(
                    "finish-document command has EOF work",
                ))?;
                if !work.semantic_complete || !work.finish_queued {
                    return Err(ParseError::Invariant(
                        "finish-document command follows semantic completion",
                    ));
                }
                let direct = self
                    .parser
                    .direct
                    .as_ref()
                    .ok_or(ParseError::Invariant("direct hooks are present"))?;
                if direct.emission_phase != DirectEmissionPhase::Complete
                    || !direct.recipe_is_empty()
                    || !direct.emission_stack.is_empty()
                {
                    return Err(ParseError::Invariant(
                        "FinishDocument follows an empty emitted stack",
                    ));
                }
                self.finished = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_source_line_boundary(&self) -> Result<bool, ParseError> {
        if self.finished
            || self.finish_work.is_some()
            || self.line_work.is_some()
            || self.pending_command().is_some()
            || self.pending_external_work().is_some()
        {
            return Err(ParseError::Invariant(
                "source line begins at a quiescent direct boundary",
            ));
        }
        let root = self.parser.tree.root;
        let direct = self
            .parser
            .direct
            .as_ref()
            .ok_or(ParseError::Invariant("direct hooks are present"))?;
        if !direct.recipe_is_empty()
            || direct.pending_stack_effect.is_some()
            || !self.parser.opened_this_line.is_empty()
            || self.parser.direct_segmented_line.is_some()
            || direct.emission_phase != DirectEmissionPhase::Complete
            || direct.emission_stack.first().copied() != Some(root)
            || !direct.emission_stack.contains(&self.parser.current)
            || direct.pending_gap_at_line_start
            || direct.pending_gap_floor_at_line_start.is_some()
            || direct.line_marker_floor.is_some()
            || (direct.pending_blank_gap_floor.is_some() && !direct.pending_blank_gap)
            || (direct.pending_terminator && direct.pending_blank_gap)
        {
            return Err(ParseError::DirectUnsupported(DirectUnsupported::BlockKind));
        }
        Ok(self.parser.current == root
            && direct.emission_stack.as_slice() == [root]
            && !direct.pending_terminator
            && !direct.pending_blank_gap)
    }

    /// Mint one opaque source-backed line continuation at a quiescent direct
    /// line boundary. The root-only ATX fast path is used only when no
    /// container or deferred line role is open; every other boundary enters
    /// the ordinary donor transition through the segmented source adapter.
    ///
    /// The admission blocks all other parser transitions until its matched
    /// work is committed. A rejected or failed admission is terminal for this
    /// speculative parser candidate.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] unless the parser has no active work and the
    /// physical byte count fits the direct command protocol.
    pub fn begin_source_line_work<I: Copy + Eq>(
        &mut self,
        identity: I,
        physical_bytes: usize,
    ) -> Result<DirectSourceLineWork<I>, ParseError> {
        let use_root_atx_fast_path = self.validate_source_line_boundary()?;
        if self.active_source_line_admission.is_some() {
            return Err(ParseError::Invariant("one source line admission is active"));
        }
        let _ = u32::try_from(physical_bytes)
            .map_err(|_| ParseError::Invariant("direct source line below u32"))?;
        let admission_id = self.next_source_line_admission;
        self.next_source_line_admission = admission_id.checked_add(1).ok_or(
            ParseError::Invariant("source line admission identity exhausted"),
        )?;
        self.active_source_line_admission = Some(admission_id);
        if use_root_atx_fast_path {
            Ok(DirectSourceLineWork::new(
                self.source_line_instance_id,
                admission_id,
                self.parser.line_number,
                identity,
                physical_bytes,
            ))
        } else {
            DirectSourceLineWork::new_segmented(
                self.source_line_instance_id,
                admission_id,
                self.parser.line_number,
                identity,
                physical_bytes,
            )
        }
    }

    /// Consume one terminal source-line result into the parser. An ATX result
    /// enters its already-correspondent donor mutation seam; every other
    /// result installs the ordinary [`LineTransition`] with a bounded physical
    /// window and exact full-line metrics. No source adapter supplies a block
    /// kind.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] before mutation for incomplete, rejected,
    /// crossed-parser, crossed-admission, crossed-source, or byte-length-
    /// mismatched work. Segmented work independently derives UTF-16 while
    /// scanning and cross-checks the source authority's value at this join.
    /// Semantic construction failures poison the speculative candidate.
    #[allow(clippy::needless_pass_by_value)] // Consumption is the match authority.
    pub fn commit_source_line<I: Copy + Eq>(
        &mut self,
        work: DirectSourceLineWork<I>,
        expected_identity: I,
        physical_utf16: u32,
    ) -> Result<(), ParseError> {
        if work.parser_instance_id != self.source_line_instance_id
            || self.active_source_line_admission != Some(work.admission_id)
            || work.boundary_line_number != self.parser.line_number
            || work.source_identity != expected_identity
        {
            return Err(ParseError::Invariant(
                "source line work belongs to this parser admission and source",
            ));
        }
        self.validate_source_line_boundary()?;
        match work.stage {
            DirectSourceLineStage::MatchedAtx { matched, .. } => {
                if matched.line_end != work.physical_bytes {
                    return Err(ParseError::Invariant(
                        "source line match covers the admitted physical line",
                    ));
                }
                let physical_bytes = u32::try_from(work.physical_bytes)
                    .map_err(|_| ParseError::Invariant("direct source line below u32"))?;
                self.apply_source_atx_match(matched, physical_bytes, physical_utf16)?;
            }
            DirectSourceLineStage::MatchedSegmented { line } => {
                if usize::try_from(line.physical_bytes)
                    .map_err(|_| ParseError::Invariant("segmented line bytes fit usize"))?
                    != work.physical_bytes
                    || line.physical_utf16 != physical_utf16
                {
                    return Err(ParseError::Invariant(
                        "segmented source metrics match the admitted authority",
                    ));
                }
                self.apply_segmented_source_line(line)?;
            }
            DirectSourceLineStage::Atx { .. }
            | DirectSourceLineStage::Segmented { .. }
            | DirectSourceLineStage::Failed => {
                return Err(ParseError::Invariant(
                    "source line commit consumes one terminal donor result",
                ));
            }
        }
        self.active_source_line_admission = None;
        Ok(())
    }

    /// Explicitly abandon a suspended source admission without mutating the
    /// grammar or exposing commands. This is valid at every source-segment
    /// boundary and lets a superseded candidate release its bounded window.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Invariant`] if the continuation belongs to a
    /// different parser/admission or the parser is no longer at its original
    /// quiescent root boundary.
    #[allow(clippy::needless_pass_by_value)] // Consumption releases the opaque admission authority.
    pub fn cancel_source_line<I: Copy + Eq>(
        &mut self,
        work: DirectSourceLineWork<I>,
    ) -> Result<(), ParseError> {
        if work.parser_instance_id != self.source_line_instance_id
            || self.active_source_line_admission != Some(work.admission_id)
            || work.boundary_line_number != self.parser.line_number
        {
            return Err(ParseError::Invariant(
                "cancelled source line belongs to this parser admission",
            ));
        }
        self.validate_source_line_boundary()?;
        self.active_source_line_admission = None;
        Ok(())
    }

    fn apply_segmented_source_line(
        &mut self,
        line: DirectSegmentedPhysicalLine,
    ) -> Result<(), ParseError> {
        let line_bytes = usize::try_from(line.physical_bytes)
            .map_err(|_| ParseError::Invariant("segmented source bytes fit usize"))?;
        if line.content_end > line_bytes || line.controller_window.len() > line_bytes {
            return Err(ParseError::Invariant(
                "segmented controller window is inside its physical line",
            ));
        }
        self.parser.line_leaf_id = u64::try_from(
            self.parser
                .line_number
                .checked_add(1)
                .ok_or(ParseError::Invariant("direct line ordinal overflow"))?,
        )
        .map_err(|_| ParseError::Invariant("direct line id below u64"))?;
        self.parser
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?
            .begin_recipe(line_bytes)?;
        let transition = self.parser.begin_line_transition(&line.controller_window);
        self.parser.curline_len = line_bytes;
        self.parser.curline_end_col = line.content_end;
        self.parser.direct_segmented_line = Some(DirectSegmentedLineFacts {
            physical_bytes: line.physical_bytes,
            content_end: line.content_end,
            ending: line.ending,
            controller_window_complete: line.controller_window_complete,
        });
        self.parser.direct_claim_initial_bom()?;
        self.line_work = Some(DirectLineWork {
            input: DirectLineInput::Segmented {
                controller_window: line.controller_window,
                physical_bytes: line.physical_bytes,
                physical_utf16: line.physical_utf16,
            },
            transition: Some(transition),
            semantic_complete: false,
            output_prepared: false,
            finish_queued: false,
        });
        self.line_complete = false;
        Ok(())
    }

    fn apply_source_atx_match(
        &mut self,
        matched: DirectAtxMatch,
        physical_bytes: u32,
        physical_utf16: u32,
    ) -> Result<(), ParseError> {
        let line_bytes = usize::try_from(physical_bytes)
            .map_err(|_| ParseError::Invariant("direct source bytes fit usize"))?;
        if matched.line_end != line_bytes || matched.content_end > line_bytes {
            return Err(ParseError::Invariant(
                "direct source ATX match is inside the physical line",
            ));
        }
        self.parser.line_leaf_id = u64::try_from(
            self.parser
                .line_number
                .checked_add(1)
                .ok_or(ParseError::Invariant("direct line ordinal overflow"))?,
        )
        .map_err(|_| ParseError::Invariant("direct line id below u64"))?;
        self.parser
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?
            .begin_recipe(line_bytes)?;

        let root = self.parser.tree.root;
        if matched.claim_start > 0 {
            if matched.claim_start != '\u{feff}'.len_utf8()
                || matched.opener_start < matched.claim_start
            {
                return Err(ParseError::Invariant(
                    "direct source prefix claim is the initial UTF-8 BOM",
                ));
            }
            let direct = self
                .parser
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?;
            if direct.claimed_offset != 0 {
                return Err(ParseError::Invariant(
                    "source-backed BOM precedes all direct source claims",
                ));
            }
            direct.push_body(DirectIntent::Consume {
                owner: root,
                part: DirectCoveragePart::Gap,
                range: 0..u32::try_from(matched.claim_start)
                    .map_err(|_| ParseError::Invariant("BOM offset fits u32"))?,
                logical: DirectLogicalAction::None,
            })?;
            direct.claimed_offset = matched.claim_start;
        }

        self.parser.opened_this_line.clear();
        self.parser.curline_len = line_bytes;
        self.parser.curline_end_col = matched.content_end;
        self.parser.offset = matched.opener_end;
        self.parser.column = matched.opener_column;
        self.parser.first_nonspace = matched.opener_start;
        self.parser.first_nonspace_column = matched.opener_start_column;
        self.parser.indent = matched.indent_columns;
        self.parser.thematic_break_kill_pos = 0;
        self.parser.blank = false;
        self.parser.partially_consumed_tab = false;
        self.parser.line_number = self
            .parser
            .line_number
            .checked_add(1)
            .ok_or(ParseError::Invariant("direct line ordinal overflow"))?;

        let heading = self.parser.add_atx_heading(
            root,
            matched.level,
            matched.closed,
            matched.opener_start + 1,
        )?;
        self.parser
            .direct_claim_atx_heading_match(heading, matched.claim_start, matched)?;
        self.parser.tree.node_mut(heading).last_line_blank = false;
        self.parser.tree.node_mut(root).last_line_blank = false;
        self.parser.current = heading;
        let _ = self.parser.complete_line_transition();

        self.line_work = Some(DirectLineWork {
            input: DirectLineInput::SourceMetrics {
                physical_bytes,
                physical_utf16,
            },
            transition: None,
            semantic_complete: true,
            output_prepared: false,
            finish_queued: false,
        });
        self.line_complete = false;
        Ok(())
    }

    pub fn begin_line(&mut self, line: String) -> Result<(), ParseError> {
        if self.active_source_line_admission.is_some() {
            return Err(ParseError::Invariant(
                "source line admission blocks buffered line work",
            ));
        }
        if self.finished || self.finish_work.is_some() {
            return Err(ParseError::Invariant("direct parser is finishing"));
        }
        if self.line_work.is_some() {
            return Err(ParseError::Invariant("direct line work already active"));
        }
        if self.pending_external_work().is_some() {
            return Err(ParseError::Invariant(
                "resolve direct external work before beginning a line",
            ));
        }
        if self.pending_command().is_some() {
            return Err(ParseError::Invariant(
                "acknowledge direct command before beginning a line",
            ));
        }
        if line.len() > DIRECT_MAX_LINE_BYTES {
            return Err(ParseError::DirectUnsupported(
                DirectUnsupported::LineTooLarge,
            ));
        }
        let _ = direct_line_ending(&line)?;
        self.parser.line_leaf_id = u64::try_from(self.parser.line_number + 1)
            .map_err(|_| ParseError::Invariant("direct line id below u64"))?;
        self.parser
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?
            .begin_recipe(line.len())?;
        let transition = self.parser.begin_line_transition(&line);
        self.parser.direct_claim_initial_bom()?;
        self.line_work = Some(DirectLineWork {
            input: DirectLineInput::Buffered(line),
            transition: Some(transition),
            semantic_complete: false,
            output_prepared: false,
            finish_queued: false,
        });
        self.line_complete = false;
        Ok(())
    }

    pub fn poll_line(&mut self, fuel: usize) -> Result<DirectPollReceipt, ParseError> {
        if self.active_source_line_admission.is_some() {
            return Err(ParseError::Invariant(
                "source line admission is polled through its opaque work",
            ));
        }
        if self.pending_command().is_some() {
            return Ok(DirectPollReceipt {
                status: DirectPollStatus::CommandReady,
                ..DirectPollReceipt::default()
            });
        }
        if self.pending_external_work().is_some() {
            return Ok(DirectPollReceipt {
                status: DirectPollStatus::ExternalWorkReady,
                ..DirectPollReceipt::default()
            });
        }
        if self.line_complete {
            return Ok(DirectPollReceipt {
                status: DirectPollStatus::Complete,
                ..DirectPollReceipt::default()
            });
        }
        let mut work = self
            .line_work
            .take()
            .ok_or(ParseError::Invariant("no direct line is active"))?;
        let mut transitions = 0;
        while transitions < fuel && !work.semantic_complete {
            let mut donor_receipt = WorkPollReceipt::default();
            let line = match &work.input {
                DirectLineInput::Buffered(line) => line,
                DirectLineInput::Segmented {
                    controller_window, ..
                } => controller_window,
                DirectLineInput::SourceMetrics { .. } => {
                    return Err(ParseError::Invariant(
                        "source-backed line semantics commit atomically",
                    ));
                }
            };
            let transition = work.transition.as_mut().ok_or(ParseError::Invariant(
                "controller-driven direct line owns a donor transition",
            ))?;
            if matches!(&work.input, DirectLineInput::Segmented { .. }) {
                self.parser
                    .ensure_segmented_controller_stage_exact(transition, line)?;
            }
            match self
                .parser
                .step_line_transition(transition, line, &mut donor_receipt)
            {
                Ok(complete) => {
                    if complete
                        && matches!(&work.input, DirectLineInput::Segmented { .. })
                        && !self.parser.segmented_outcome_is_supported()
                    {
                        return Err(ParseError::DirectUnsupported(
                            DirectUnsupported::SegmentedLine,
                        ));
                    }
                    work.semantic_complete = complete;
                }
                Err(ParseError::DirectExternalWork(request)) => {
                    let pending = self.pending_external_work().ok_or(ParseError::Invariant(
                        "reference control error installs external work",
                    ))?;
                    if pending.request() != request
                        || pending.kind() != DirectExternalWorkKind::ReferencePrefixFinalizer
                    {
                        return Err(ParseError::Invariant(
                            "reference control error matches pending rendezvous",
                        ));
                    }
                }
                Err(error) => return Err(error),
            }
            transitions += donor_receipt.transitions;
            if self.pending_external_work().is_some() {
                break;
            }
        }
        if work.semantic_complete && !work.output_prepared {
            self.parser.direct_prepare_pending_blank_gap()?;
            match &work.input {
                DirectLineInput::Buffered(line) => {
                    self.parser.direct_stage_blank_line_bytes(line.len())?;
                }
                DirectLineInput::Segmented { physical_bytes, .. }
                | DirectLineInput::SourceMetrics { physical_bytes, .. } => {
                    self.parser.direct_stage_blank_line_bytes(
                        usize::try_from(*physical_bytes)
                            .map_err(|_| ParseError::Invariant("direct line bytes fit usize"))?,
                    )?;
                }
            }
            work.output_prepared = true;
        }
        if work.semantic_complete && self.pending_command().is_none() && !work.finish_queued {
            let queued = self
                .parser
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?
                .queue_next_intent()?;
            if !queued {
                match &work.input {
                    DirectLineInput::Buffered(line) => {
                        self.parser.direct_queue_finish_line(line)?;
                    }
                    DirectLineInput::Segmented {
                        physical_bytes,
                        physical_utf16,
                        ..
                    }
                    | DirectLineInput::SourceMetrics {
                        physical_bytes,
                        physical_utf16,
                    } => {
                        self.parser
                            .direct_queue_finish_line_metrics(*physical_bytes, *physical_utf16)?;
                    }
                }
                work.finish_queued = true;
            }
        }
        let status = if self.pending_command().is_some() {
            DirectPollStatus::CommandReady
        } else if self.pending_external_work().is_some() {
            DirectPollStatus::ExternalWorkReady
        } else {
            DirectPollStatus::Pending
        };
        self.line_work = Some(work);
        Ok(DirectPollReceipt {
            transitions,
            status,
        })
    }

    pub fn begin_finish(&mut self) -> Result<(), ParseError> {
        if self.active_source_line_admission.is_some() {
            return Err(ParseError::Invariant(
                "source line admission blocks document finish",
            ));
        }
        if self.finished {
            return Err(ParseError::Invariant("direct parser already finished"));
        }
        if self.line_work.is_some() || self.finish_work.is_some() {
            return Err(ParseError::Invariant("direct parser work already active"));
        }
        if self.pending_external_work().is_some() {
            return Err(ParseError::Invariant(
                "resolve direct external work before finishing",
            ));
        }
        if self.pending_command().is_some() {
            return Err(ParseError::Invariant(
                "acknowledge direct command before finishing",
            ));
        }
        self.parser
            .direct
            .as_mut()
            .ok_or(ParseError::Invariant("direct hooks are present"))?
            .begin_recipe(0)?;
        self.parser.direct_prepare_pending_blank_gap()?;
        self.finish_work = Some(DirectFinishWork {
            transition: self.parser.begin_finish_transition(),
            semantic_complete: false,
            finish_queued: false,
        });
        self.line_complete = false;
        Ok(())
    }

    pub fn poll_finish(&mut self, fuel: usize) -> Result<DirectPollReceipt, ParseError> {
        if self.pending_command().is_some() {
            return Ok(DirectPollReceipt {
                status: DirectPollStatus::CommandReady,
                ..DirectPollReceipt::default()
            });
        }
        if self.pending_external_work().is_some() {
            return Ok(DirectPollReceipt {
                status: DirectPollStatus::ExternalWorkReady,
                ..DirectPollReceipt::default()
            });
        }
        if self.finished {
            return Ok(DirectPollReceipt {
                status: DirectPollStatus::Complete,
                ..DirectPollReceipt::default()
            });
        }
        let mut work = self
            .finish_work
            .take()
            .ok_or(ParseError::Invariant("no direct EOF work is active"))?;
        let mut transitions = 0;
        while transitions < fuel && !work.semantic_complete {
            let mut donor_receipt = WorkPollReceipt::default();
            match self
                .parser
                .step_finish_transition(&mut work.transition, &mut donor_receipt)
            {
                Ok(complete) => work.semantic_complete = complete,
                Err(ParseError::DirectExternalWork(request)) => {
                    let pending = self.pending_external_work().ok_or(ParseError::Invariant(
                        "finish reference control error installs external work",
                    ))?;
                    if pending.request() != request
                        || pending.kind() != DirectExternalWorkKind::ReferencePrefixFinalizer
                    {
                        return Err(ParseError::Invariant(
                            "finish reference control matches pending rendezvous",
                        ));
                    }
                }
                Err(error) => return Err(error),
            }
            transitions += donor_receipt.transitions;
            if self.pending_external_work().is_some() {
                break;
            }
        }
        if work.semantic_complete && self.pending_command().is_none() && !work.finish_queued {
            let queued = self
                .parser
                .direct
                .as_mut()
                .ok_or(ParseError::Invariant("direct hooks are present"))?
                .queue_next_intent()?;
            if !queued {
                self.parser
                    .direct
                    .as_mut()
                    .ok_or(ParseError::Invariant("direct hooks are present"))?
                    .push_command(DirectCommand::FinishDocument, None)?;
                work.finish_queued = true;
            }
        }
        let status = if self.pending_command().is_some() {
            DirectPollStatus::CommandReady
        } else if self.pending_external_work().is_some() {
            DirectPollStatus::ExternalWorkReady
        } else {
            DirectPollStatus::Pending
        };
        self.finish_work = Some(work);
        Ok(DirectPollReceipt {
            transitions,
            status,
        })
    }

    #[must_use]
    pub fn scratch_node_count(&self) -> usize {
        self.parser.tree.nodes.len()
    }

    #[must_use]
    pub fn retained_logical_bytes(&self) -> usize {
        self.parser
            .tree
            .nodes
            .iter()
            .map(|node| node.content.logical.len())
            .sum()
    }

    #[must_use]
    pub fn retained_line_bytes(&self) -> usize {
        self.line_work.as_ref().map_or(0, |work| match &work.input {
            DirectLineInput::Buffered(line) => line.len(),
            DirectLineInput::Segmented {
                controller_window, ..
            } => controller_window.len(),
            DirectLineInput::SourceMetrics { .. } => 0,
        })
    }

    #[must_use]
    pub fn legacy_event_count(&self) -> usize {
        self.parser.tree.events.len()
    }
}

pub(crate) fn parse_list_marker(
    line: &str,
    mut position: usize,
    interrupts_paragraph: bool,
) -> Option<(usize, ListData)> {
    let bytes = line.as_bytes();
    if position >= line.len() {
        return None;
    }
    let mut marker = bytes[position];
    let start_position = position;
    if matches!(marker, b'*' | b'-' | b'+') {
        position += 1;
        if !bytes
            .get(position)
            .is_none_or(|byte| byte.is_ascii_whitespace())
        {
            return None;
        }
        if interrupts_paragraph {
            let mut index = position;
            if index == bytes.len() {
                return None;
            }
            while is_space_or_tab(bytes[index]) {
                index += 1;
                if index == bytes.len() {
                    return None;
                }
            }
            if is_line_end_char(bytes[index]) {
                return None;
            }
        }
        return Some((
            position - start_position,
            ListData {
                list_type: ListType::Bullet,
                marker_offset: 0,
                padding: 0,
                start: 1,
                delimiter: ListDelimiter::Period,
                bullet_char: marker,
                tight: false,
                task_checked: None,
            },
        ));
    }
    if marker.is_ascii_digit() {
        let mut start = 0;
        let mut digits = 0;
        loop {
            start = 10 * start + usize::from(bytes[position] - b'0');
            position += 1;
            digits += 1;
            if position == bytes.len() {
                return None;
            }
            if !(digits < 9 && bytes[position].is_ascii_digit()) {
                break;
            }
        }
        if interrupts_paragraph && start != 1 {
            return None;
        }
        marker = bytes[position];
        if !matches!(marker, b'.' | b')') {
            return None;
        }
        position += 1;
        if position == bytes.len() || !bytes[position].is_ascii_whitespace() {
            return None;
        }
        if interrupts_paragraph {
            let mut index = position;
            while is_space_or_tab(bytes[index]) {
                index += 1;
                if index == bytes.len() {
                    return None;
                }
            }
            if is_line_end_char(bytes[index]) {
                return None;
            }
        }
        return Some((
            position - start_position,
            ListData {
                list_type: ListType::Ordered,
                marker_offset: 0,
                padding: 0,
                start,
                delimiter: if marker == b'.' {
                    ListDelimiter::Period
                } else {
                    ListDelimiter::Paren
                },
                bullet_char: 0,
                tight: false,
                task_checked: None,
            },
        ));
    }
    None
}

pub(crate) fn lists_match(left: &ListData, right: &ListData) -> bool {
    left.list_type == right.list_type
        && left.delimiter == right.delimiter
        && left.bullet_char == right.bullet_char
}

fn byte_matches(bytes: &[u8], offset: usize, predicate: fn(u8) -> bool) -> bool {
    bytes.get(offset).is_some_and(|byte| predicate(*byte))
}

fn is_line_end_char(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n')
}

fn is_space_or_tab(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn direct_line_ending(line: &str) -> Result<(usize, Option<DirectLineEnding>), ParseError> {
    let bytes = line.as_bytes();
    let (content_end, ending) = if bytes.ends_with(b"\r\n") {
        (bytes.len() - 2, Some(DirectLineEnding::CrLf))
    } else if bytes.ends_with(b"\n") {
        (bytes.len() - 1, Some(DirectLineEnding::Lf))
    } else if bytes.ends_with(b"\r") {
        (bytes.len() - 1, Some(DirectLineEnding::Cr))
    } else {
        (bytes.len(), None)
    };
    if bytes[..content_end]
        .iter()
        .any(|byte| is_line_end_char(*byte))
    {
        return Err(ParseError::DirectUnsupported(
            DirectUnsupported::EmbeddedLineEnding,
        ));
    }
    Ok((content_end, ending))
}

pub(crate) fn newlines_of(line: &str) -> usize {
    line.bytes()
        .rev()
        .take_while(|byte| is_line_end_char(*byte))
        .count()
}

#[cfg(test)]
mod direct_pause_tests;
