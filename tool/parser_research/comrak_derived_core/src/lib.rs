//! Research-only Comrak-derived block-machine seam.
//!
//! This is deliberately not a Markdown parser. It ports a coherent subset of
//! Comrak v0.54.0's `process_line` -> `check_open_blocks` ->
//! `open_new_blocks` flow into a persistent value-state machine. The supported
//! subset is block quotes, bullet/ordered lists, paragraphs, and fenced code.
//! See `PROVENANCE.md` for the exact upstream functions and known omissions.
//!
//! The important experiment is representational: no arena node or owned leaf
//! text is retained. Open containers are an immutable `Arc` chain, leaf and
//! marker payloads are source ranges, and every state-machine tick performs at
//! most one source-byte inspection. A caller can therefore yield inside a
//! physical line or a deep-container walk.

use std::ops::Range;
use std::sync::Arc;

pub mod commitment_spine;
pub mod origin_runs;

const TAB_STOP: usize = 4;
const MAX_LIST_DEPTH: usize = 100;
const FINGERPRINT_SEED: u128 = 0x6c62_2f65_7669_6c2f_6b72_616d_6f63_2f2f;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListType {
    Bullet,
    Ordered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListDelimiter {
    Period,
    Paren,
}

/// Value equivalent of the Comrak `NodeList` fields needed by block parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListData {
    pub list_type: ListType,
    pub marker_offset: usize,
    pub padding: usize,
    pub start: usize,
    pub delimiter: ListDelimiter,
    pub bullet_char: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContainerKind {
    BlockQuote,
    List(ListData),
    Item(ListData),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FenceData {
    pub character: u8,
    pub length: usize,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LeafKind {
    Paragraph,
    Heading(u8),
    FencedCode(FenceData),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: u64,
    pub kind: ContainerKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub id: u64,
    pub kind: LeafKind,
}

#[derive(Clone, Debug)]
struct FrameNode {
    frame: Frame,
    parent: Option<Arc<FrameNode>>,
    depth: usize,
    list_depth: usize,
    semantic_fingerprint: u128,
}

/// Cheap restart/convergence state. Cloning this never walks container depth.
#[derive(Clone, Debug, Default)]
pub struct RestartState {
    top: Option<Arc<FrameNode>>,
    pub leaf: Option<Leaf>,
}

impl RestartState {
    pub fn depth(&self) -> usize {
        self.top.as_ref().map_or(0, |top| top.depth)
    }

    pub fn frames(&self) -> Vec<Frame> {
        let mut frames = Vec::with_capacity(self.depth());
        let mut cursor = self.top.clone();
        while let Some(node) = cursor {
            frames.push(node.frame.clone());
            cursor = node.parent.clone();
        }
        frames.reverse();
        frames
    }

    /// Exact shape comparison for clean/incremental convergence. Stable IDs are
    /// intentionally excluded; list/fence continuation data is not.
    pub fn semantic_eq(&self, other: &Self) -> bool {
        let mut comparison = self.begin_semantic_comparison(other);
        loop {
            match comparison.advance(usize::MAX) {
                ComparisonStatus::Pending => {}
                ComparisonStatus::Equal => return true,
                ComparisonStatus::NotEqual => return false,
            }
        }
    }

    pub fn semantic_fingerprint(&self) -> u128 {
        let frames = self
            .top
            .as_ref()
            .map_or(FINGERPRINT_SEED, |top| top.semantic_fingerprint);
        fingerprint_extend(frames, self.leaf.as_ref().map(|leaf| leaf.kind))
    }

    /// Collision-safe state equality that can be cooperatively budgeted. The
    /// stored fingerprint is an O(1) rejection filter, never the final proof.
    pub fn begin_semantic_comparison(&self, other: &Self) -> StateComparison {
        let status = if self.depth() != other.depth()
            || self.leaf.as_ref().map(|leaf| leaf.kind) != other.leaf.as_ref().map(|leaf| leaf.kind)
            || self.semantic_fingerprint() != other.semantic_fingerprint()
        {
            ComparisonStatus::NotEqual
        } else {
            ComparisonStatus::Pending
        };
        StateComparison {
            left: self.top.clone(),
            right: other.top.clone(),
            status,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonStatus {
    Pending,
    Equal,
    NotEqual,
}

pub struct StateComparison {
    left: Option<Arc<FrameNode>>,
    right: Option<Arc<FrameNode>>,
    status: ComparisonStatus,
}

impl StateComparison {
    /// Compare at most `fuel` container frames.
    pub fn advance(&mut self, fuel: usize) -> ComparisonStatus {
        if self.status != ComparisonStatus::Pending {
            return self.status;
        }
        for _ in 0..fuel {
            match (self.left.take(), self.right.take()) {
                (None, None) => {
                    self.status = ComparisonStatus::Equal;
                    return self.status;
                }
                (Some(left), Some(right)) if left.frame.kind == right.frame.kind => {
                    self.left = left.parent.clone();
                    self.right = right.parent.clone();
                }
                _ => {
                    self.status = ComparisonStatus::NotEqual;
                    return self.status;
                }
            }
        }
        if self.left.is_none() && self.right.is_none() {
            self.status = ComparisonStatus::Equal;
        }
        self.status
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Blank,
    Paragraph,
    SetextUnderline,
    FenceOpen,
    FenceBody,
    FenceClose,
}

#[derive(Clone, Debug)]
pub struct LineChunk {
    pub source: Range<usize>,
    pub content: Range<usize>,
    pub markers: Vec<Range<usize>>,
    pub kind: LineKind,
    pub leaf_id: Option<u64>,
    pub continues_leaf: bool,
    /// Spaces represented by the unconsumed part of a tab. This is the value
    /// counterpart of Comrak's `partially_consumed_tab` flag.
    pub virtual_prefix_spaces: usize,
    path: Option<Arc<FrameNode>>,
}

impl LineChunk {
    pub fn container_path(&self) -> Vec<Frame> {
        RestartState {
            top: self.path.clone(),
            leaf: None,
        }
        .frames()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockEvent {
    OpenContainer {
        id: u64,
        kind: ContainerKind,
        marker: Range<usize>,
    },
    CloseContainer {
        id: u64,
        kind: ContainerKind,
        at: usize,
    },
    StartLeaf {
        id: u64,
        kind: LeafKind,
        at: usize,
    },
    EndLeaf {
        id: u64,
        kind: LeafKind,
        at: usize,
    },
    PromoteLeaf {
        id: u64,
        from: LeafKind,
        to: LeafKind,
        marker: Range<usize>,
    },
}

#[derive(Clone, Debug)]
pub struct LineRecord {
    pub line_number: usize,
    pub state_after: RestartState,
    pub chunk: LineChunk,
    pub events: Vec<BlockEvent>,
    pub work_units: usize,
    pub bytes_inspected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceStatus {
    Yielded,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvanceReport {
    pub status: AdvanceStatus,
    pub work_units: usize,
    pub bytes_inspected: usize,
    pub completed_lines: usize,
}

#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub offset: usize,
    pub line_number: usize,
    pub state: RestartState,
    next_id: u64,
}

pub struct DerivedBlockMachine {
    source: Arc<str>,
    offset: usize,
    line_number: usize,
    state: RestartState,
    line: Option<LineWork>,
    records: Vec<LineRecord>,
    next_id: u64,
}

impl DerivedBlockMachine {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self {
            source: source.into(),
            offset: 0,
            line_number: 0,
            state: RestartState::default(),
            line: None,
            records: Vec::new(),
            next_id: 1,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn resume(source: impl Into<Arc<str>>, checkpoint: Checkpoint) -> Self {
        let source = source.into();
        assert!(checkpoint.offset <= source.len());
        assert!(source.is_char_boundary(checkpoint.offset));
        Self {
            source,
            offset: checkpoint.offset,
            line_number: checkpoint.line_number,
            state: checkpoint.state,
            line: None,
            records: Vec::new(),
            next_id: checkpoint.next_id,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn state(&self) -> &RestartState {
        &self.state
    }

    pub fn records(&self) -> &[LineRecord] {
        &self.records
    }

    /// Drain parser output into an external persistent/coalescing sink. Keeping
    /// every research `LineRecord` is intentionally optional; production would
    /// retain adaptive checkpoints and syntax fragments, not this trace format.
    pub fn take_records(&mut self) -> Vec<LineRecord> {
        std::mem::take(&mut self.records)
    }

    /// A checkpoint is available only at a published physical-line boundary.
    /// Cloning it is O(1) in source size and container depth.
    pub fn checkpoint(&self) -> Option<Checkpoint> {
        self.line.is_none().then(|| Checkpoint {
            offset: self.offset,
            line_number: self.line_number,
            state: self.state.clone(),
            next_id: self.next_id,
        })
    }

    pub fn is_complete(&self) -> bool {
        self.offset == self.source.len() && self.line.is_none()
    }

    /// Advance by parser work units. Each unit performs at most one source-byte
    /// inspection and one bounded state transition.
    pub fn advance(&mut self, fuel: usize) -> AdvanceReport {
        let mut report = AdvanceReport {
            status: AdvanceStatus::Yielded,
            work_units: 0,
            bytes_inspected: 0,
            completed_lines: 0,
        };

        while report.work_units < fuel && !self.is_complete() {
            if self.line.is_none() {
                self.line = Some(LineWork::new(
                    self.offset,
                    self.line_number + 1,
                    self.state.clone(),
                ));
            }

            let line = self.line.as_mut().expect("line initialized");
            let before_bytes = line.bytes_inspected;
            line.tick(self.source.as_bytes(), &mut self.next_id);
            report.work_units += 1;
            let inspected = line.bytes_inspected - before_bytes;
            debug_assert!(inspected <= 1, "one tick inspected {inspected} bytes");
            report.bytes_inspected += inspected;

            if line.done.is_some() {
                let finished = self.line.take().expect("finished line");
                let (record, next_offset) = finished.into_record();
                self.offset = next_offset;
                self.line_number += 1;
                self.state = record.state_after.clone();
                self.records.push(record);
                report.completed_lines += 1;
            }
        }

        if self.is_complete() {
            report.status = AdvanceStatus::Complete;
        }
        report
    }
}

#[derive(Clone, Debug)]
enum IndentNext {
    MatchQuote {
        reverse_index: usize,
        node: Arc<FrameNode>,
        frame_cursor: usize,
        frame_column: usize,
    },
    Open {
        paragraph_open: bool,
    },
    FenceClose {
        fence: FenceData,
        body_start: usize,
    },
    Lazy {
        target: Option<Arc<FrameNode>>,
        reopen_cursor: usize,
        reopen_column: usize,
    },
}

#[derive(Clone, Debug)]
struct IndentResult {
    first_nonspace: usize,
    start_column: usize,
    first_nonspace_column: usize,
}

impl IndentResult {
    fn indent(&self) -> usize {
        self.first_nonspace_column - self.start_column
    }
}

#[derive(Clone, Debug)]
enum Resume {
    Open { cursor: usize, column: usize },
    Blank { cursor: usize },
}

#[derive(Clone, Debug)]
enum TailKind {
    Paragraph {
        content_start: usize,
        continuation: bool,
    },
    FenceBody {
        content_start: usize,
        leaf_id: u64,
    },
}

#[derive(Clone, Debug)]
struct PendingFinish {
    content_end: usize,
    kind: LineKind,
    content_start: usize,
    leaf_id: Option<u64>,
    continuation: bool,
}

#[derive(Clone, Debug)]
enum Stage {
    BuildPath {
        next: Option<Arc<FrameNode>>,
    },
    MatchFrame {
        reverse_index: usize,
    },
    ScanIndent {
        cursor: usize,
        column: usize,
        start: usize,
        start_column: usize,
        next: IndentNext,
    },
    DispatchIndent {
        result: IndentResult,
        next: IndentNext,
    },
    MatchQuote {
        result: IndentResult,
        reverse_index: usize,
        node: Arc<FrameNode>,
        frame_cursor: usize,
        frame_column: usize,
    },
    MatchQuoteSpace {
        cursor: usize,
        column: usize,
        reverse_index: usize,
    },
    MatchItem {
        node: Arc<FrameNode>,
        reverse_index: usize,
        frame_cursor: usize,
        frame_column: usize,
        cursor: usize,
        column: usize,
        remaining_columns: usize,
    },
    AfterMatched,
    LazyDispatch {
        result: IndentResult,
        target: Option<Arc<FrameNode>>,
        reopen_cursor: usize,
        reopen_column: usize,
    },
    CloseTo {
        target: Option<Arc<FrameNode>>,
        at: usize,
        resume: Resume,
    },
    OpenDispatch {
        result: IndentResult,
        paragraph_open: bool,
    },
    OpenQuoteSpace {
        cursor: usize,
        column: usize,
        paragraph_open: bool,
    },
    FenceOpenRun {
        cursor: usize,
        marker_start: usize,
        character: u8,
        count: usize,
        indent: usize,
        paragraph_open: bool,
        fallback_content_start: usize,
    },
    FenceOpenInfo {
        cursor: usize,
        marker: Range<usize>,
        character: u8,
        count: usize,
        indent: usize,
        paragraph_open: bool,
        fallback_content_start: usize,
        invalid_backtick_info: bool,
    },
    SetextScan {
        cursor: usize,
        marker_start: usize,
        character: u8,
        marker_count: usize,
        in_trailing_whitespace: bool,
        indent: usize,
        fallback_content_start: usize,
    },
    FenceCloseRun {
        cursor: usize,
        marker_start: usize,
        count: usize,
        fence: FenceData,
        body_start: usize,
    },
    FenceCloseTail {
        cursor: usize,
        marker: Range<usize>,
        fence: FenceData,
        body_start: usize,
    },
    ListBulletAfter {
        marker_start: usize,
        marker_end: usize,
        marker_char: u8,
        cursor: usize,
        indent: usize,
        paragraph_open: bool,
        fallback_content_start: usize,
    },
    ListOrdered {
        marker_start: usize,
        cursor: usize,
        digits: usize,
        value: usize,
        indent: usize,
        paragraph_open: bool,
        fallback_content_start: usize,
    },
    ListAfterMarker {
        marker_start: usize,
        marker_end: usize,
        cursor: usize,
        data: ListData,
        paragraph_open: bool,
        fallback_content_start: usize,
    },
    ListPadding {
        marker_start: usize,
        marker_end: usize,
        cursor: usize,
        first_whitespace_end: Option<usize>,
        columns: usize,
        whitespace_bytes: usize,
        data: ListData,
        paragraph_open: bool,
        fallback_content_start: usize,
    },
    Tail {
        cursor: usize,
        kind: TailKind,
    },
    AwaitLf {
        cursor: usize,
        finish: PendingFinish,
    },
    Done,
    Poison,
}

struct CompletedLine {
    next_offset: usize,
    content_end: usize,
    kind: LineKind,
    content_start: usize,
    leaf_id: Option<u64>,
    continuation: bool,
}

struct LineWork {
    line_start: usize,
    line_number: usize,
    top: Option<Arc<FrameNode>>,
    leaf: Option<Leaf>,
    reverse_path: Vec<Arc<FrameNode>>,
    match_cursor: usize,
    match_column: usize,
    stage: Stage,
    events: Vec<BlockEvent>,
    markers: Vec<Range<usize>>,
    virtual_prefix_spaces: usize,
    work_units: usize,
    bytes_inspected: usize,
    done: Option<CompletedLine>,
}

impl LineWork {
    fn new(line_start: usize, line_number: usize, state: RestartState) -> Self {
        let next = state.top.clone();
        Self {
            line_start,
            line_number,
            top: state.top,
            leaf: state.leaf,
            reverse_path: Vec::new(),
            match_cursor: line_start,
            match_column: 0,
            stage: Stage::BuildPath { next },
            events: Vec::new(),
            markers: Vec::new(),
            virtual_prefix_spaces: 0,
            work_units: 0,
            bytes_inspected: 0,
            done: None,
        }
    }

    fn inspect(&mut self, source: &[u8], offset: usize) -> Option<u8> {
        let value = source.get(offset).copied();
        if value.is_some() {
            self.bytes_inspected += 1;
        }
        value
    }

    fn tick(&mut self, source: &[u8], next_id: &mut u64) {
        self.work_units += 1;
        let stage = std::mem::replace(&mut self.stage, Stage::Poison);
        match stage {
            Stage::BuildPath { next } => {
                if let Some(node) = next {
                    self.reverse_path.push(node.clone());
                    self.stage = Stage::BuildPath {
                        next: node.parent.clone(),
                    };
                } else {
                    self.stage = Stage::MatchFrame {
                        reverse_index: self.reverse_path.len(),
                    };
                }
            }
            Stage::MatchFrame { reverse_index } => {
                if reverse_index == 0 {
                    self.stage = Stage::AfterMatched;
                    return;
                }
                let node = self.reverse_path[reverse_index - 1].clone();
                match node.frame.kind {
                    ContainerKind::List(_) => {
                        self.stage = Stage::MatchFrame {
                            reverse_index: reverse_index - 1,
                        };
                    }
                    ContainerKind::BlockQuote => {
                        self.stage = Stage::ScanIndent {
                            cursor: self.match_cursor,
                            column: self.match_column,
                            start: self.match_cursor,
                            start_column: self.match_column,
                            next: IndentNext::MatchQuote {
                                reverse_index,
                                node,
                                frame_cursor: self.match_cursor,
                                frame_column: self.match_column,
                            },
                        };
                    }
                    ContainerKind::Item(data) => {
                        self.stage = Stage::MatchItem {
                            node,
                            reverse_index,
                            frame_cursor: self.match_cursor,
                            frame_column: self.match_column,
                            cursor: self.match_cursor,
                            column: self.match_column,
                            remaining_columns: data.marker_offset + data.padding,
                        };
                    }
                }
            }
            Stage::ScanIndent {
                cursor,
                column,
                start,
                start_column,
                next,
            } => match self.inspect(source, cursor) {
                Some(b' ') => {
                    self.stage = Stage::ScanIndent {
                        cursor: cursor + 1,
                        column: column + 1,
                        start,
                        start_column,
                        next,
                    };
                }
                Some(b'\t') => {
                    self.stage = Stage::ScanIndent {
                        cursor: cursor + 1,
                        column: column + tab_width(column),
                        start,
                        start_column,
                        next,
                    };
                }
                _ => {
                    self.stage = Stage::DispatchIndent {
                        result: IndentResult {
                            first_nonspace: cursor,
                            start_column,
                            first_nonspace_column: column,
                        },
                        next,
                    };
                }
            },
            Stage::DispatchIndent { result, next } => match next {
                IndentNext::MatchQuote {
                    reverse_index,
                    node,
                    frame_cursor,
                    frame_column,
                } => {
                    self.stage = Stage::MatchQuote {
                        result,
                        reverse_index,
                        node,
                        frame_cursor,
                        frame_column,
                    };
                }
                IndentNext::Open { paragraph_open } => {
                    self.stage = Stage::OpenDispatch {
                        result,
                        paragraph_open,
                    };
                }
                IndentNext::FenceClose { fence, body_start } => {
                    if result.indent() <= 3 {
                        self.stage = Stage::FenceCloseRun {
                            cursor: result.first_nonspace,
                            marker_start: result.first_nonspace,
                            count: 0,
                            fence,
                            body_start,
                        };
                    } else {
                        let leaf_id = self.leaf.as_ref().expect("open fence").id;
                        self.stage = Stage::Tail {
                            cursor: result.first_nonspace,
                            kind: TailKind::FenceBody {
                                content_start: body_start,
                                leaf_id,
                            },
                        };
                    }
                }
                IndentNext::Lazy {
                    target,
                    reopen_cursor,
                    reopen_column,
                } => {
                    self.stage = Stage::LazyDispatch {
                        result,
                        target,
                        reopen_cursor,
                        reopen_column,
                    };
                }
            },
            Stage::MatchQuote {
                result,
                reverse_index,
                node,
                frame_cursor,
                frame_column,
            } => {
                let byte = self.inspect(source, result.first_nonspace);
                if result.indent() <= 3 && byte == Some(b'>') {
                    self.stage = Stage::MatchQuoteSpace {
                        cursor: result.first_nonspace + 1,
                        column: result.first_nonspace_column + 1,
                        reverse_index,
                    };
                } else {
                    self.begin_mismatch(
                        node.parent.clone(),
                        frame_cursor,
                        frame_column,
                        result.first_nonspace,
                    );
                }
            }
            Stage::MatchQuoteSpace {
                cursor,
                column,
                reverse_index,
            } => {
                let byte = self.inspect(source, cursor);
                let (cursor, column) = match byte {
                    Some(b' ') => (cursor + 1, column + 1),
                    Some(b'\t') => (cursor + 1, column + tab_width(column)),
                    _ => (cursor, column),
                };
                self.match_cursor = cursor;
                self.match_column = column;
                self.stage = Stage::MatchFrame {
                    reverse_index: reverse_index - 1,
                };
            }
            Stage::MatchItem {
                node,
                reverse_index,
                frame_cursor,
                frame_column,
                cursor,
                column,
                remaining_columns,
            } => {
                if remaining_columns == 0 {
                    self.match_cursor = cursor;
                    self.match_column = column;
                    self.stage = Stage::MatchFrame {
                        reverse_index: reverse_index - 1,
                    };
                    return;
                }
                match self.inspect(source, cursor) {
                    Some(b' ') => {
                        self.stage = Stage::MatchItem {
                            node,
                            reverse_index,
                            frame_cursor,
                            frame_column,
                            cursor: cursor + 1,
                            column: column + 1,
                            remaining_columns: remaining_columns - 1,
                        }
                    }
                    Some(b'\t') => {
                        let width = tab_width(column);
                        if width > remaining_columns {
                            self.virtual_prefix_spaces += width - remaining_columns;
                            self.match_cursor = cursor + 1;
                            self.match_column = column + remaining_columns;
                            self.stage = Stage::MatchFrame {
                                reverse_index: reverse_index - 1,
                            };
                        } else {
                            self.stage = Stage::MatchItem {
                                node,
                                reverse_index,
                                frame_cursor,
                                frame_column,
                                cursor: cursor + 1,
                                column: column + width,
                                remaining_columns: remaining_columns - width,
                            };
                        }
                    }
                    Some(b'\r' | b'\n') | None => {
                        self.match_cursor = cursor;
                        self.match_column = column;
                        self.stage = Stage::MatchFrame {
                            reverse_index: reverse_index - 1,
                        };
                    }
                    _ => {
                        self.begin_mismatch(node.parent.clone(), frame_cursor, frame_column, cursor)
                    }
                }
            }
            Stage::AfterMatched => {
                let cursor = self.match_cursor;
                let column = self.match_column;
                if let Some(Leaf {
                    kind: LeafKind::FencedCode(fence),
                    ..
                }) = self.leaf
                {
                    self.stage = Stage::ScanIndent {
                        cursor,
                        column,
                        start: cursor,
                        start_column: column,
                        next: IndentNext::FenceClose {
                            fence,
                            body_start: cursor,
                        },
                    };
                } else {
                    self.stage = Stage::ScanIndent {
                        cursor,
                        column,
                        start: cursor,
                        start_column: column,
                        next: IndentNext::Open {
                            paragraph_open: matches!(
                                self.leaf,
                                Some(Leaf {
                                    kind: LeafKind::Paragraph,
                                    ..
                                })
                            ),
                        },
                    };
                }
            }
            Stage::LazyDispatch {
                result,
                target,
                reopen_cursor,
                reopen_column,
            } => match self.inspect(source, result.first_nonspace) {
                Some(b'\r' | b'\n') | None => {
                    self.stage = Stage::CloseTo {
                        target,
                        at: result.first_nonspace,
                        resume: Resume::Blank {
                            cursor: result.first_nonspace,
                        },
                    };
                }
                // Conservative supported-subset interruption test. Exact
                // Exact thematic-break and HTML handling remains outside this probe.
                Some(b'>' | b'`' | b'~' | b'*' | b'+' | b'-' | b'0'..=b'9') => {
                    self.stage = Stage::CloseTo {
                        target,
                        at: result.first_nonspace,
                        resume: Resume::Open {
                            cursor: reopen_cursor,
                            column: reopen_column,
                        },
                    };
                }
                _ => {
                    let leaf_id = self.leaf.as_ref().expect("lazy paragraph").id;
                    self.stage = Stage::Tail {
                        cursor: result.first_nonspace,
                        kind: TailKind::Paragraph {
                            content_start: result.first_nonspace,
                            continuation: true,
                        },
                    };
                    debug_assert_ne!(leaf_id, 0);
                }
            },
            Stage::CloseTo { target, at, resume } => {
                if let Some(leaf) = self.leaf.take() {
                    self.events.push(BlockEvent::EndLeaf {
                        id: leaf.id,
                        kind: leaf.kind,
                        at,
                    });
                    self.stage = Stage::CloseTo { target, at, resume };
                } else if !same_top(&self.top, &target) {
                    let node = self.top.take().expect("target is an ancestor");
                    self.events.push(BlockEvent::CloseContainer {
                        id: node.frame.id,
                        kind: node.frame.kind,
                        at,
                    });
                    self.top = node.parent.clone();
                    self.stage = Stage::CloseTo { target, at, resume };
                } else {
                    match resume {
                        Resume::Open { cursor, column } => {
                            self.stage = Stage::ScanIndent {
                                cursor,
                                column,
                                start: cursor,
                                start_column: column,
                                next: IndentNext::Open {
                                    paragraph_open: false,
                                },
                            };
                        }
                        Resume::Blank { cursor } => self.finish_blank(source, cursor),
                    }
                }
            }
            Stage::OpenDispatch {
                result,
                paragraph_open,
            } => match self.inspect(source, result.first_nonspace) {
                Some(b'\r' | b'\n') | None => {
                    if paragraph_open || self.leaf.is_some() {
                        let target = self.top.clone();
                        self.stage = Stage::CloseTo {
                            target,
                            at: result.first_nonspace,
                            resume: Resume::Blank {
                                cursor: result.first_nonspace,
                            },
                        };
                    } else {
                        self.finish_blank(source, result.first_nonspace);
                    }
                }
                Some(b'>') if result.indent() <= 3 => {
                    self.close_leaf(result.first_nonspace);
                    let marker = result.first_nonspace..result.first_nonspace + 1;
                    self.markers.push(marker.clone());
                    self.push_frame(ContainerKind::BlockQuote, marker, next_id);
                    self.stage = Stage::OpenQuoteSpace {
                        cursor: result.first_nonspace + 1,
                        column: result.first_nonspace_column + 1,
                        paragraph_open: false,
                    };
                }
                Some(character @ (b'`' | b'~')) if result.indent() <= 3 => {
                    self.stage = Stage::FenceOpenRun {
                        cursor: result.first_nonspace,
                        marker_start: result.first_nonspace,
                        character,
                        count: 0,
                        indent: result.indent(),
                        paragraph_open,
                        fallback_content_start: result.first_nonspace,
                    };
                }
                Some(character @ (b'=' | b'-')) if paragraph_open && result.indent() <= 3 => {
                    self.stage = Stage::SetextScan {
                        cursor: result.first_nonspace,
                        marker_start: result.first_nonspace,
                        character,
                        marker_count: 0,
                        in_trailing_whitespace: false,
                        indent: result.indent(),
                        fallback_content_start: result.first_nonspace,
                    };
                }
                Some(marker_char @ (b'*' | b'+' | b'-')) if result.indent() <= 3 => {
                    self.stage = Stage::ListBulletAfter {
                        marker_start: result.first_nonspace,
                        marker_end: result.first_nonspace + 1,
                        marker_char,
                        cursor: result.first_nonspace + 1,
                        indent: result.indent(),
                        paragraph_open,
                        fallback_content_start: result.first_nonspace,
                    };
                }
                Some(b'0'..=b'9') if result.indent() <= 3 => {
                    self.stage = Stage::ListOrdered {
                        marker_start: result.first_nonspace,
                        cursor: result.first_nonspace,
                        digits: 0,
                        value: 0,
                        indent: result.indent(),
                        paragraph_open,
                        fallback_content_start: result.first_nonspace,
                    };
                }
                _ => self.start_paragraph_tail(result.first_nonspace, paragraph_open, next_id),
            },
            Stage::OpenQuoteSpace {
                cursor,
                column,
                paragraph_open,
            } => {
                let byte = self.inspect(source, cursor);
                let (cursor, column) = match byte {
                    Some(b' ') => (cursor + 1, column + 1),
                    Some(b'\t') => (cursor + 1, column + tab_width(column)),
                    _ => (cursor, column),
                };
                self.stage = Stage::ScanIndent {
                    cursor,
                    column,
                    start: cursor,
                    start_column: column,
                    next: IndentNext::Open { paragraph_open },
                };
            }
            Stage::FenceOpenRun {
                cursor,
                marker_start,
                character,
                count,
                indent,
                paragraph_open,
                fallback_content_start,
            } => match self.inspect(source, cursor) {
                Some(byte) if byte == character => {
                    self.stage = Stage::FenceOpenRun {
                        cursor: cursor + 1,
                        marker_start,
                        character,
                        count: count + 1,
                        indent,
                        paragraph_open,
                        fallback_content_start,
                    };
                }
                _ if count >= 3 => {
                    self.stage = Stage::FenceOpenInfo {
                        cursor,
                        marker: marker_start..cursor,
                        character,
                        count,
                        indent,
                        paragraph_open,
                        fallback_content_start,
                        invalid_backtick_info: false,
                    };
                }
                _ => self.start_paragraph_tail_from(
                    cursor,
                    fallback_content_start,
                    paragraph_open,
                    next_id,
                ),
            },
            Stage::FenceOpenInfo {
                cursor,
                marker,
                character,
                count,
                indent,
                paragraph_open,
                fallback_content_start,
                invalid_backtick_info,
            } => match self.inspect(source, cursor) {
                Some(b'\n') => {
                    if character == b'`' && invalid_backtick_info {
                        self.finish_paragraph(
                            cursor,
                            cursor + 1,
                            fallback_content_start,
                            paragraph_open,
                            next_id,
                        );
                    } else {
                        self.open_fence_and_finish(
                            cursor,
                            cursor + 1,
                            marker,
                            FenceData {
                                character,
                                length: count,
                                offset: indent,
                            },
                            paragraph_open,
                            next_id,
                        );
                    }
                }
                Some(b'\r') => {
                    let valid = !(character == b'`' && invalid_backtick_info);
                    if valid {
                        self.open_fence(
                            marker.clone(),
                            FenceData {
                                character,
                                length: count,
                                offset: indent,
                            },
                            paragraph_open,
                            next_id,
                        );
                    } else {
                        self.ensure_paragraph(fallback_content_start, paragraph_open, next_id);
                    }
                    self.stage = Stage::AwaitLf {
                        cursor: cursor + 1,
                        finish: PendingFinish {
                            content_end: cursor,
                            kind: if valid {
                                LineKind::FenceOpen
                            } else {
                                LineKind::Paragraph
                            },
                            content_start: if valid {
                                marker.end
                            } else {
                                fallback_content_start
                            },
                            leaf_id: self.leaf.as_ref().map(|leaf| leaf.id),
                            continuation: !valid && paragraph_open,
                        },
                    };
                }
                None => {
                    if character == b'`' && invalid_backtick_info {
                        self.finish_paragraph(
                            cursor,
                            cursor,
                            fallback_content_start,
                            paragraph_open,
                            next_id,
                        );
                    } else {
                        self.open_fence_and_finish(
                            cursor,
                            cursor,
                            marker,
                            FenceData {
                                character,
                                length: count,
                                offset: indent,
                            },
                            paragraph_open,
                            next_id,
                        );
                    }
                }
                Some(byte) => {
                    self.stage = Stage::FenceOpenInfo {
                        cursor: cursor + 1,
                        marker,
                        character,
                        count,
                        indent,
                        paragraph_open,
                        fallback_content_start,
                        invalid_backtick_info: invalid_backtick_info
                            || (character == b'`' && byte == b'`'),
                    };
                }
            },
            Stage::SetextScan {
                cursor,
                marker_start,
                character,
                marker_count,
                in_trailing_whitespace,
                indent,
                fallback_content_start,
            } => match self.inspect(source, cursor) {
                Some(byte) if byte == character && !in_trailing_whitespace => {
                    self.stage = Stage::SetextScan {
                        cursor: cursor + 1,
                        marker_start,
                        character,
                        marker_count: marker_count + 1,
                        in_trailing_whitespace,
                        indent,
                        fallback_content_start,
                    };
                }
                Some(b' ' | b'\t') if marker_count > 0 => {
                    self.stage = Stage::SetextScan {
                        cursor: cursor + 1,
                        marker_start,
                        character,
                        marker_count,
                        in_trailing_whitespace: true,
                        indent,
                        fallback_content_start,
                    };
                }
                Some(b'\n') if marker_count > 0 => {
                    self.promote_setext_and_finish(
                        cursor,
                        cursor + 1,
                        marker_start..cursor,
                        character,
                    );
                }
                Some(b'\r') if marker_count > 0 => {
                    let marker = marker_start..cursor;
                    let leaf_id = self.promote_setext(marker.clone(), character);
                    self.stage = Stage::AwaitLf {
                        cursor: cursor + 1,
                        finish: PendingFinish {
                            content_end: cursor,
                            kind: LineKind::SetextUnderline,
                            content_start: cursor,
                            leaf_id: Some(leaf_id),
                            continuation: true,
                        },
                    };
                }
                None if marker_count > 0 => {
                    self.promote_setext_and_finish(cursor, cursor, marker_start..cursor, character);
                }
                _ if character == b'-' => {
                    self.stage = Stage::ListBulletAfter {
                        marker_start,
                        marker_end: marker_start + 1,
                        marker_char: b'-',
                        cursor: marker_start + 1,
                        indent,
                        paragraph_open: true,
                        fallback_content_start,
                    };
                }
                _ => self.start_paragraph_tail_from(cursor, fallback_content_start, true, next_id),
            },
            Stage::FenceCloseRun {
                cursor,
                marker_start,
                count,
                fence,
                body_start,
            } => match self.inspect(source, cursor) {
                Some(byte) if byte == fence.character => {
                    self.stage = Stage::FenceCloseRun {
                        cursor: cursor + 1,
                        marker_start,
                        count: count + 1,
                        fence,
                        body_start,
                    };
                }
                _ if count >= fence.length => {
                    self.stage = Stage::FenceCloseTail {
                        cursor,
                        marker: marker_start..cursor,
                        fence,
                        body_start,
                    };
                }
                _ => {
                    let leaf_id = self.leaf.as_ref().expect("open fence").id;
                    self.stage = Stage::Tail {
                        cursor,
                        kind: TailKind::FenceBody {
                            content_start: body_start,
                            leaf_id,
                        },
                    };
                }
            },
            Stage::FenceCloseTail {
                cursor,
                marker,
                fence,
                body_start,
            } => match self.inspect(source, cursor) {
                Some(b' ' | b'\t') => {
                    self.stage = Stage::FenceCloseTail {
                        cursor: cursor + 1,
                        marker,
                        fence,
                        body_start,
                    }
                }
                Some(b'\n') => self.close_fence_and_finish(cursor, cursor + 1, marker),
                Some(b'\r') => {
                    let leaf = self.leaf.take().expect("open fence");
                    self.events.push(BlockEvent::EndLeaf {
                        id: leaf.id,
                        kind: leaf.kind,
                        at: marker.start,
                    });
                    self.markers.push(marker);
                    self.stage = Stage::AwaitLf {
                        cursor: cursor + 1,
                        finish: PendingFinish {
                            content_end: cursor,
                            kind: LineKind::FenceClose,
                            content_start: cursor,
                            leaf_id: Some(leaf.id),
                            continuation: false,
                        },
                    };
                }
                None => self.close_fence_and_finish(cursor, cursor, marker),
                _ => {
                    let leaf_id = self.leaf.as_ref().expect("open fence").id;
                    self.stage = Stage::Tail {
                        cursor,
                        kind: TailKind::FenceBody {
                            content_start: body_start,
                            leaf_id,
                        },
                    };
                }
            },
            Stage::ListBulletAfter {
                marker_start,
                marker_end,
                marker_char,
                cursor,
                indent,
                paragraph_open,
                fallback_content_start,
            } => {
                let data = ListData {
                    list_type: ListType::Bullet,
                    marker_offset: indent,
                    padding: 0,
                    start: 1,
                    delimiter: ListDelimiter::Period,
                    bullet_char: marker_char,
                };
                self.stage = Stage::ListAfterMarker {
                    marker_start,
                    marker_end,
                    cursor,
                    data,
                    paragraph_open,
                    fallback_content_start,
                };
            }
            Stage::ListOrdered {
                marker_start,
                cursor,
                digits,
                value,
                indent,
                paragraph_open,
                fallback_content_start,
            } => match self.inspect(source, cursor) {
                Some(byte @ b'0'..=b'9') if digits < 9 => {
                    self.stage = Stage::ListOrdered {
                        marker_start,
                        cursor: cursor + 1,
                        digits: digits + 1,
                        value: value * 10 + usize::from(byte - b'0'),
                        indent,
                        paragraph_open,
                        fallback_content_start,
                    };
                }
                Some(delimiter @ (b'.' | b')')) if digits > 0 => {
                    if paragraph_open && value != 1 {
                        self.start_paragraph_tail_from(
                            cursor,
                            fallback_content_start,
                            true,
                            next_id,
                        );
                    } else {
                        self.stage = Stage::ListAfterMarker {
                            marker_start,
                            marker_end: cursor + 1,
                            cursor: cursor + 1,
                            data: ListData {
                                list_type: ListType::Ordered,
                                marker_offset: indent,
                                padding: 0,
                                start: value,
                                delimiter: if delimiter == b'.' {
                                    ListDelimiter::Period
                                } else {
                                    ListDelimiter::Paren
                                },
                                bullet_char: 0,
                            },
                            paragraph_open,
                            fallback_content_start,
                        };
                    }
                }
                _ => self.start_paragraph_tail_from(
                    cursor,
                    fallback_content_start,
                    paragraph_open,
                    next_id,
                ),
            },
            Stage::ListAfterMarker {
                marker_start,
                marker_end,
                cursor,
                data,
                paragraph_open,
                fallback_content_start,
            } => match self.inspect(source, cursor) {
                Some(b' ' | b'\t' | b'\r' | b'\n') => {
                    self.stage = Stage::ListPadding {
                        marker_start,
                        marker_end,
                        cursor,
                        first_whitespace_end: None,
                        columns: 0,
                        whitespace_bytes: 0,
                        data,
                        paragraph_open,
                        fallback_content_start,
                    };
                }
                None => {
                    self.stage = Stage::ListPadding {
                        marker_start,
                        marker_end,
                        cursor,
                        first_whitespace_end: None,
                        columns: 0,
                        whitespace_bytes: 0,
                        data,
                        paragraph_open,
                        fallback_content_start,
                    };
                }
                _ => self.start_paragraph_tail_from(
                    cursor,
                    fallback_content_start,
                    paragraph_open,
                    next_id,
                ),
            },
            Stage::ListPadding {
                marker_start,
                marker_end,
                cursor,
                first_whitespace_end,
                columns,
                whitespace_bytes,
                data,
                paragraph_open,
                fallback_content_start,
            } => match self.inspect(source, cursor) {
                Some(b' ') if columns <= 5 => {
                    self.stage = Stage::ListPadding {
                        marker_start,
                        marker_end,
                        cursor: cursor + 1,
                        first_whitespace_end: first_whitespace_end.or(Some(cursor + 1)),
                        columns: columns + 1,
                        whitespace_bytes: whitespace_bytes + 1,
                        data,
                        paragraph_open,
                        fallback_content_start,
                    }
                }
                Some(b'\t') if columns <= 5 => {
                    self.stage = Stage::ListPadding {
                        marker_start,
                        marker_end,
                        cursor: cursor + 1,
                        first_whitespace_end: first_whitespace_end.or(Some(cursor + 1)),
                        columns: columns
                            + tab_width(data.marker_offset + (marker_end - marker_start) + columns),
                        whitespace_bytes: whitespace_bytes + 1,
                        data,
                        paragraph_open,
                        fallback_content_start,
                    }
                }
                byte => {
                    let blank = matches!(byte, Some(b'\r' | b'\n') | None);
                    if paragraph_open && blank {
                        self.finish_paragraph(
                            cursor,
                            match byte {
                                Some(b'\n') => cursor + 1,
                                Some(b'\r') => cursor + 1,
                                _ => cursor,
                            },
                            fallback_content_start,
                            true,
                            next_id,
                        );
                        return;
                    }
                    let marker_len = marker_end - marker_start;
                    let (padding, content_cursor) = if !(1..5).contains(&columns) || blank {
                        (
                            marker_len + 1,
                            if whitespace_bytes > 0 {
                                first_whitespace_end.expect("whitespace byte")
                            } else {
                                marker_end
                            },
                        )
                    } else {
                        (marker_len + columns, cursor)
                    };
                    let mut data = data;
                    data.padding = padding;
                    self.open_list_item(data, marker_start..marker_end, content_cursor, next_id);
                }
            },
            Stage::Tail { cursor, kind } => match self.inspect(source, cursor) {
                Some(b'\n') => self.finish_tail(cursor, cursor + 1, kind),
                Some(b'\r') => {
                    let finish = self.pending_from_tail(cursor, kind);
                    self.stage = Stage::AwaitLf {
                        cursor: cursor + 1,
                        finish,
                    };
                }
                Some(_) => {
                    self.stage = Stage::Tail {
                        cursor: cursor + 1,
                        kind,
                    }
                }
                None => self.finish_tail(cursor, cursor, kind),
            },
            Stage::AwaitLf { cursor, finish } => {
                let next = if self.inspect(source, cursor) == Some(b'\n') {
                    cursor + 1
                } else {
                    cursor
                };
                self.complete(finish, next);
            }
            Stage::Done => self.stage = Stage::Done,
            Stage::Poison => unreachable!("stage was not replaced"),
        }
    }

    fn begin_mismatch(
        &mut self,
        target: Option<Arc<FrameNode>>,
        reopen_cursor: usize,
        reopen_column: usize,
        at: usize,
    ) {
        if matches!(
            self.leaf,
            Some(Leaf {
                kind: LeafKind::Paragraph,
                ..
            })
        ) {
            self.stage = Stage::ScanIndent {
                cursor: self.line_start,
                column: 0,
                start: self.line_start,
                start_column: 0,
                next: IndentNext::Lazy {
                    target,
                    reopen_cursor,
                    reopen_column,
                },
            };
        } else {
            self.stage = Stage::CloseTo {
                target,
                at,
                resume: Resume::Open {
                    cursor: reopen_cursor,
                    column: reopen_column,
                },
            };
        }
    }

    fn close_leaf(&mut self, at: usize) {
        if let Some(leaf) = self.leaf.take() {
            self.events.push(BlockEvent::EndLeaf {
                id: leaf.id,
                kind: leaf.kind,
                at,
            });
        }
    }

    fn push_frame(&mut self, kind: ContainerKind, marker: Range<usize>, next_id: &mut u64) {
        let id = take_id(next_id);
        let depth = self.top.as_ref().map_or(1, |top| top.depth + 1);
        let list_depth = self.top.as_ref().map_or(0, |top| top.list_depth)
            + usize::from(matches!(kind, ContainerKind::List(_)));
        let parent_fingerprint = self
            .top
            .as_ref()
            .map_or(FINGERPRINT_SEED, |top| top.semantic_fingerprint);
        self.top = Some(Arc::new(FrameNode {
            frame: Frame { id, kind },
            parent: self.top.clone(),
            depth,
            list_depth,
            semantic_fingerprint: fingerprint_container(parent_fingerprint, kind),
        }));
        self.events
            .push(BlockEvent::OpenContainer { id, kind, marker });
    }

    fn start_paragraph_tail(&mut self, cursor: usize, paragraph_open: bool, next_id: &mut u64) {
        self.start_paragraph_tail_from(cursor, cursor, paragraph_open, next_id);
    }

    fn start_paragraph_tail_from(
        &mut self,
        scan_cursor: usize,
        content_start: usize,
        paragraph_open: bool,
        next_id: &mut u64,
    ) {
        let continuation = self.ensure_paragraph(content_start, paragraph_open, next_id);
        self.stage = Stage::Tail {
            cursor: scan_cursor,
            kind: TailKind::Paragraph {
                content_start,
                continuation,
            },
        };
    }

    fn ensure_paragraph(&mut self, at: usize, paragraph_open: bool, next_id: &mut u64) -> bool {
        if paragraph_open
            && matches!(
                self.leaf,
                Some(Leaf {
                    kind: LeafKind::Paragraph,
                    ..
                })
            )
        {
            return true;
        }
        self.close_leaf(at);
        let id = take_id(next_id);
        self.leaf = Some(Leaf {
            id,
            kind: LeafKind::Paragraph,
        });
        self.events.push(BlockEvent::StartLeaf {
            id,
            kind: LeafKind::Paragraph,
            at,
        });
        false
    }

    fn open_fence(
        &mut self,
        marker: Range<usize>,
        fence: FenceData,
        _paragraph_open: bool,
        next_id: &mut u64,
    ) {
        self.close_leaf(marker.start);
        self.markers.push(marker.clone());
        let id = take_id(next_id);
        self.leaf = Some(Leaf {
            id,
            kind: LeafKind::FencedCode(fence),
        });
        self.events.push(BlockEvent::StartLeaf {
            id,
            kind: LeafKind::FencedCode(fence),
            at: marker.start,
        });
    }

    fn open_fence_and_finish(
        &mut self,
        content_end: usize,
        next_offset: usize,
        marker: Range<usize>,
        fence: FenceData,
        paragraph_open: bool,
        next_id: &mut u64,
    ) {
        let content_start = marker.end;
        self.open_fence(marker, fence, paragraph_open, next_id);
        let leaf_id = self.leaf.as_ref().map(|leaf| leaf.id);
        self.complete(
            PendingFinish {
                content_end,
                kind: LineKind::FenceOpen,
                content_start,
                leaf_id,
                continuation: false,
            },
            next_offset,
        );
    }

    fn open_list_item(
        &mut self,
        data: ListData,
        marker: Range<usize>,
        content_cursor: usize,
        next_id: &mut u64,
    ) {
        self.close_leaf(marker.start);
        let compatible = self.top.as_ref().is_some_and(|top| {
            matches!(top.frame.kind, ContainerKind::List(existing) if lists_match(&existing, &data))
        });
        if !compatible {
            // `Parser::add_child` finalizes an incompatible current List before
            // adding the new List; lists cannot directly contain sibling lists.
            // Preserve that containment rule without retaining an arena node.
            if matches!(
                self.top.as_ref().map(|top| top.frame.kind),
                Some(ContainerKind::List(_))
            ) {
                let old = self.top.take().expect("matched list top");
                self.events.push(BlockEvent::CloseContainer {
                    id: old.frame.id,
                    kind: old.frame.kind,
                    at: marker.start,
                });
                self.top = old.parent.clone();
            }
            let list_depth = self.top.as_ref().map_or(0, |top| top.list_depth);
            if list_depth >= MAX_LIST_DEPTH {
                self.start_paragraph_tail(content_cursor, false, next_id);
                return;
            }
            self.push_frame(ContainerKind::List(data), marker.clone(), next_id);
        }
        self.markers.push(marker.clone());
        self.push_frame(ContainerKind::Item(data), marker, next_id);
        self.stage = Stage::ScanIndent {
            cursor: content_cursor,
            column: data.marker_offset + data.padding,
            start: content_cursor,
            start_column: data.marker_offset + data.padding,
            next: IndentNext::Open {
                paragraph_open: false,
            },
        };
    }

    fn finish_paragraph(
        &mut self,
        content_end: usize,
        next_offset: usize,
        content_start: usize,
        paragraph_open: bool,
        next_id: &mut u64,
    ) {
        let continuation = self.ensure_paragraph(content_start, paragraph_open, next_id);
        self.complete(
            PendingFinish {
                content_end,
                kind: LineKind::Paragraph,
                content_start,
                leaf_id: self.leaf.as_ref().map(|leaf| leaf.id),
                continuation,
            },
            next_offset,
        );
    }

    fn finish_blank(&mut self, source: &[u8], cursor: usize) {
        match self.inspect(source, cursor) {
            Some(b'\n') => self.complete(
                PendingFinish {
                    content_end: cursor,
                    kind: LineKind::Blank,
                    content_start: cursor,
                    leaf_id: None,
                    continuation: false,
                },
                cursor + 1,
            ),
            Some(b'\r') => {
                self.stage = Stage::AwaitLf {
                    cursor: cursor + 1,
                    finish: PendingFinish {
                        content_end: cursor,
                        kind: LineKind::Blank,
                        content_start: cursor,
                        leaf_id: None,
                        continuation: false,
                    },
                };
            }
            None => self.complete(
                PendingFinish {
                    content_end: cursor,
                    kind: LineKind::Blank,
                    content_start: cursor,
                    leaf_id: None,
                    continuation: false,
                },
                cursor,
            ),
            Some(_) => unreachable!("blank cursor was not at line end"),
        }
    }

    fn finish_tail(&mut self, content_end: usize, next_offset: usize, kind: TailKind) {
        let finish = self.pending_from_tail(content_end, kind);
        self.complete(finish, next_offset);
    }

    fn pending_from_tail(&self, content_end: usize, kind: TailKind) -> PendingFinish {
        match kind {
            TailKind::Paragraph {
                content_start,
                continuation,
            } => PendingFinish {
                content_end,
                kind: LineKind::Paragraph,
                content_start,
                leaf_id: self.leaf.as_ref().map(|leaf| leaf.id),
                continuation,
            },
            TailKind::FenceBody {
                content_start,
                leaf_id,
            } => PendingFinish {
                content_end,
                kind: LineKind::FenceBody,
                content_start,
                leaf_id: Some(leaf_id),
                continuation: true,
            },
        }
    }

    fn close_fence_and_finish(
        &mut self,
        content_end: usize,
        next_offset: usize,
        marker: Range<usize>,
    ) {
        let leaf = self.leaf.take().expect("open fence");
        self.events.push(BlockEvent::EndLeaf {
            id: leaf.id,
            kind: leaf.kind,
            at: marker.start,
        });
        self.markers.push(marker);
        self.complete(
            PendingFinish {
                content_end,
                kind: LineKind::FenceClose,
                content_start: content_end,
                leaf_id: Some(leaf.id),
                continuation: false,
            },
            next_offset,
        );
    }

    fn promote_setext(&mut self, marker: Range<usize>, character: u8) -> u64 {
        let leaf = self
            .leaf
            .as_mut()
            .expect("setext requires an open paragraph");
        debug_assert_eq!(leaf.kind, LeafKind::Paragraph);
        let from = leaf.kind;
        let to = LeafKind::Heading(if character == b'=' { 1 } else { 2 });
        leaf.kind = to;
        let id = leaf.id;
        self.markers.push(marker.clone());
        self.events.push(BlockEvent::PromoteLeaf {
            id,
            from,
            to,
            marker,
        });
        id
    }

    fn promote_setext_and_finish(
        &mut self,
        content_end: usize,
        next_offset: usize,
        marker: Range<usize>,
        character: u8,
    ) {
        let leaf_id = self.promote_setext(marker, character);
        self.complete(
            PendingFinish {
                content_end,
                kind: LineKind::SetextUnderline,
                content_start: content_end,
                leaf_id: Some(leaf_id),
                continuation: true,
            },
            next_offset,
        );
    }

    fn complete(&mut self, finish: PendingFinish, next_offset: usize) {
        self.done = Some(CompletedLine {
            next_offset,
            content_end: finish.content_end,
            kind: finish.kind,
            content_start: finish.content_start.min(finish.content_end),
            leaf_id: finish.leaf_id,
            continuation: finish.continuation,
        });
        self.stage = Stage::Done;
    }

    fn into_record(self) -> (LineRecord, usize) {
        let done = self.done.expect("completed line");
        let state_after = RestartState {
            top: self.top.clone(),
            leaf: self.leaf.clone(),
        };
        let record = LineRecord {
            line_number: self.line_number,
            state_after,
            chunk: LineChunk {
                source: self.line_start..done.next_offset,
                content: done.content_start..done.content_end,
                markers: self.markers,
                kind: done.kind,
                leaf_id: done.leaf_id,
                continues_leaf: done.continuation,
                virtual_prefix_spaces: self.virtual_prefix_spaces,
                path: self.top,
            },
            events: self.events,
            work_units: self.work_units,
            bytes_inspected: self.bytes_inspected,
        };
        (record, done.next_offset)
    }
}

fn take_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id = next_id.checked_add(1).expect("probe id exhaustion");
    id
}

fn tab_width(column: usize) -> usize {
    TAB_STOP - (column % TAB_STOP)
}

fn fingerprint_container(parent: u128, kind: ContainerKind) -> u128 {
    let mut value = parent;
    match kind {
        ContainerKind::BlockQuote => value = fingerprint_word(value, 1),
        ContainerKind::List(data) => {
            value = fingerprint_word(value, 2);
            value = fingerprint_list(value, data);
        }
        ContainerKind::Item(data) => {
            value = fingerprint_word(value, 3);
            value = fingerprint_list(value, data);
        }
    }
    value
}

fn fingerprint_list(mut value: u128, data: ListData) -> u128 {
    value = fingerprint_word(value, data.list_type as u64);
    value = fingerprint_word(value, data.marker_offset as u64);
    value = fingerprint_word(value, data.padding as u64);
    value = fingerprint_word(value, data.start as u64);
    value = fingerprint_word(value, data.delimiter as u64);
    fingerprint_word(value, u64::from(data.bullet_char))
}

fn fingerprint_extend(value: u128, leaf: Option<LeafKind>) -> u128 {
    match leaf {
        None => fingerprint_word(value, 0),
        Some(LeafKind::Paragraph) => fingerprint_word(value, 11),
        Some(LeafKind::Heading(level)) => fingerprint_word(value, 12 + u64::from(level)),
        Some(LeafKind::FencedCode(fence)) => {
            let value = fingerprint_word(value, 20);
            let value = fingerprint_word(value, u64::from(fence.character));
            let value = fingerprint_word(value, fence.length as u64);
            fingerprint_word(value, fence.offset as u64)
        }
    }
}

fn fingerprint_word(value: u128, word: u64) -> u128 {
    let mixed = value ^ u128::from(word).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    mixed
        .rotate_left(29)
        .wrapping_mul(0x1000_0000_01b3_0000_0000_01b3_0000_01b3)
}

fn same_top(left: &Option<Arc<FrameNode>>, right: &Option<Arc<FrameNode>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        _ => false,
    }
}

/// Direct port of Comrak v0.54.0 `lists_match`.
fn lists_match(left: &ListData, right: &ListData) -> bool {
    left.list_type == right.list_type
        && left.delimiter == right.delimiter
        && left.bullet_char == right.bullet_char
}
