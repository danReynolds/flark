//! A deliberately small experiment in resumable parser state and suffix reuse.
//!
//! This is not a Markdown parser. Its only purpose is to falsify or support the
//! mechanics behind page checkpoints, exact state convergence, and reusing
//! facts whose endpoints cross a checkpoint.

use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const SOURCE_PAGE_BYTES: usize = 4 * 1024;

static NEXT_PAGE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TAIL_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub page_id: u64,
    pub offset: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DelimiterKind {
    Square,
    Round,
    Curly,
    Star,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalKind {
    Text,
    Open(DelimiterKind),
    Close(DelimiterKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexicalFact {
    pub kind: LexicalKind,
    pub start: Anchor,
    pub end_offset: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticFact {
    Pair {
        kind: DelimiterKind,
        open: Anchor,
        close: Anchor,
    },
    AbandonedOpen {
        kind: DelimiterKind,
        open: Anchor,
        at_close: Anchor,
    },
    UnmatchedClose {
        kind: DelimiterKind,
        close: Anchor,
    },
    UnclosedAtEof {
        kind: DelimiterKind,
        open: Anchor,
    },
}

#[derive(Debug)]
struct SourcePage {
    id: u64,
    bytes: Arc<[u8]>,
}

impl SourcePage {
    fn new(bytes: &[u8]) -> Arc<Self> {
        assert!(!bytes.is_empty());
        assert!(bytes.len() <= SOURCE_PAGE_BYTES);
        Arc::new(Self {
            id: NEXT_PAGE_ID.fetch_add(1, Ordering::Relaxed),
            bytes: Arc::from(bytes),
        })
    }
}

#[derive(Debug)]
struct SourceTail {
    id: u64,
    page: Arc<SourcePage>,
    next: Option<Arc<SourceTail>>,
}

impl SourceTail {
    fn new(page: Arc<SourcePage>, next: Option<Arc<SourceTail>>) -> Arc<Self> {
        Arc::new(Self {
            id: NEXT_TAIL_ID.fetch_add(1, Ordering::Relaxed),
            page,
            next,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    tails: Arc<Vec<Arc<SourceTail>>>,
    len: usize,
}

#[derive(Clone, Debug)]
pub struct EditOutcome {
    pub document: Document,
    pub restart_page: usize,
}

impl Document {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let pages: Vec<_> = bytes
            .chunks(SOURCE_PAGE_BYTES)
            .filter(|chunk| !chunk.is_empty())
            .map(SourcePage::new)
            .collect();
        Self::from_pages_with_reused_suffix(&pages, None, bytes.len())
    }

    fn from_pages_with_reused_suffix(
        pages: &[Arc<SourcePage>],
        suffix: Option<Arc<SourceTail>>,
        len: usize,
    ) -> Self {
        let mut head = suffix;
        for page in pages.iter().rev() {
            head = Some(SourceTail::new(Arc::clone(page), head));
        }
        let mut tails = Vec::new();
        let mut cursor = head;
        while let Some(tail) = cursor {
            cursor = tail.next.clone();
            tails.push(tail);
        }
        Self {
            tails: Arc::new(tails),
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn page_count(&self) -> usize {
        self.tails.len()
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.len);
        for tail in self.tails.iter() {
            bytes.extend_from_slice(&tail.page.bytes);
        }
        bytes
    }

    fn locate_boundary(&self, position: usize) -> (usize, usize) {
        assert!(position <= self.len);
        let mut base = 0;
        for (index, tail) in self.tails.iter().enumerate() {
            if position == base {
                return (index, 0);
            }
            let end = base + tail.page.bytes.len();
            if position < end {
                return (index, position - base);
            }
            base = end;
        }
        (self.tails.len(), 0)
    }

    /// Applies an edit while retaining the untouched source-tail object.
    ///
    /// Page indexing is intentionally outside the parser job in this trial.
    /// The parser's measured work begins at `restart_page`.
    pub fn edit(&self, range: Range<usize>, replacement: &[u8]) -> EditOutcome {
        assert!(range.start <= range.end);
        assert!(range.end <= self.len);
        let (start_page, start_offset) = self.locate_boundary(range.start);
        let (end_page, end_offset) = self.locate_boundary(range.end);

        let mut fragment = Vec::new();
        let suffix_page;
        if start_page == end_page {
            if start_page == self.page_count() || (start_offset == 0 && end_offset == 0) {
                fragment.extend_from_slice(replacement);
                suffix_page = start_page;
            } else {
                let page = &self.tails[start_page].page.bytes;
                fragment.extend_from_slice(&page[..start_offset]);
                fragment.extend_from_slice(replacement);
                fragment.extend_from_slice(&page[end_offset..]);
                suffix_page = start_page + 1;
            }
        } else {
            if start_offset > 0 {
                fragment.extend_from_slice(&self.tails[start_page].page.bytes[..start_offset]);
            }
            fragment.extend_from_slice(replacement);
            if end_offset > 0 {
                fragment.extend_from_slice(&self.tails[end_page].page.bytes[end_offset..]);
                suffix_page = end_page + 1;
            } else {
                suffix_page = end_page;
            }
        }

        let new_fragment_pages: Vec<_> = fragment
            .chunks(SOURCE_PAGE_BYTES)
            .filter(|chunk| !chunk.is_empty())
            .map(SourcePage::new)
            .collect();
        let prefix_pages: Vec<_> = self.tails[..start_page]
            .iter()
            .map(|tail| Arc::clone(&tail.page))
            .collect();
        let reused_suffix = self.tails.get(suffix_page).cloned();

        let mut rebuilt = prefix_pages;
        rebuilt.extend(new_fragment_pages);
        let new_len = self.len - (range.end - range.start) + replacement.len();
        let document = Self::from_pages_with_reused_suffix(&rebuilt, reused_suffix, new_len);
        EditOutcome {
            document,
            restart_page: start_page,
        }
    }
}

#[derive(Clone, Debug)]
struct StackFrame {
    kind: DelimiterKind,
    open: Anchor,
    previous: Option<Arc<StackFrame>>,
    depth: u32,
    digest: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ParserState {
    root: Option<Arc<StackFrame>>,
    depth: u32,
    digest: u64,
}

impl ParserState {
    fn push(&self, kind: DelimiterKind, open: Anchor) -> Self {
        let digest = mix64(
            self.digest,
            kind_code(kind) ^ open.page_id ^ u64::from(open.offset),
        );
        let frame = Arc::new(StackFrame {
            kind,
            open,
            previous: self.root.clone(),
            depth: self.depth + 1,
            digest,
        });
        Self {
            root: Some(frame),
            depth: self.depth + 1,
            digest,
        }
    }

    fn pop(&self) -> Self {
        match &self.root {
            Some(frame) => match &frame.previous {
                Some(previous) => Self {
                    root: Some(Arc::clone(previous)),
                    depth: previous.depth,
                    digest: previous.digest,
                },
                None => Self::default(),
            },
            None => Self::default(),
        }
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }
}

struct ExactStateComparison {
    left: Option<Arc<StackFrame>>,
    right: Option<Arc<StackFrame>>,
    started: bool,
    left_depth: u32,
    right_depth: u32,
    left_digest: u64,
    right_digest: u64,
}

impl ExactStateComparison {
    fn new(left: &ParserState, right: &ParserState) -> Self {
        Self {
            left: left.root.clone(),
            right: right.root.clone(),
            started: false,
            left_depth: left.depth,
            right_depth: right.depth,
            left_digest: left.digest,
            right_digest: right.digest,
        }
    }

    /// Consumes at most one frame comparison. A digest is only a fast reject;
    /// equality always ends in pointer identity or exact frame comparisons.
    fn step(&mut self) -> Option<bool> {
        if !self.started {
            self.started = true;
            if self.left_depth != self.right_depth || self.left_digest != self.right_digest {
                return Some(false);
            }
            return None;
        }

        match (&self.left, &self.right) {
            (None, None) => Some(true),
            (Some(left), Some(right)) if Arc::ptr_eq(left, right) => Some(true),
            (Some(left), Some(right)) => {
                if left.kind != right.kind || left.open != right.open {
                    return Some(false);
                }
                self.left = left.previous.clone();
                self.right = right.previous.clone();
                None
            }
            _ => Some(false),
        }
    }
}

const FACT_CHUNK_CAPACITY: usize = 128;

#[derive(Clone, Debug)]
struct FactChunk<T> {
    values: Vec<T>,
    previous: Option<Arc<FactChunk<T>>>,
}

#[derive(Clone, Debug, Default)]
pub struct FactPages<T> {
    last: Option<Arc<FactChunk<T>>>,
    len: usize,
}

impl<T> FactPages<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn to_vec(&self) -> Vec<T>
    where
        T: Copy,
    {
        let mut chunks = Vec::new();
        let mut cursor = self.last.clone();
        while let Some(chunk) = cursor {
            cursor = chunk.previous.clone();
            chunks.push(chunk);
        }
        let mut values = Vec::with_capacity(self.len);
        for chunk in chunks.iter().rev() {
            values.extend_from_slice(&chunk.values);
        }
        values
    }

    fn retained_bytes(&self) -> usize {
        let mut bytes = 0;
        let mut cursor = self.last.clone();
        while let Some(chunk) = cursor {
            bytes += size_of::<FactChunk<T>>();
            bytes += chunk.values.capacity() * size_of::<T>();
            cursor = chunk.previous.clone();
        }
        bytes
    }
}

struct FactBuilder<T> {
    sealed: Option<Arc<FactChunk<T>>>,
    current: Vec<T>,
    len: usize,
}

impl<T> Default for FactBuilder<T> {
    fn default() -> Self {
        Self {
            sealed: None,
            current: Vec::new(),
            len: 0,
        }
    }
}

impl<T> FactBuilder<T> {
    fn push(&mut self, value: T) {
        if self.current.len() == FACT_CHUNK_CAPACITY {
            self.seal_chunk();
        }
        self.current.push(value);
        self.len += 1;
    }

    fn seal_chunk(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let values = std::mem::take(&mut self.current);
        self.sealed = Some(Arc::new(FactChunk {
            values,
            previous: self.sealed.take(),
        }));
    }

    fn finish(mut self) -> FactPages<T> {
        self.seal_chunk();
        FactPages {
            last: self.sealed,
            len: self.len,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PageOutput {
    pub source_page_id: u64,
    pub lexical: FactPages<LexicalFact>,
    pub semantic: FactPages<SemanticFact>,
    pub digest: u64,
}

#[derive(Clone, Debug)]
pub struct EofOutput {
    pub semantic: FactPages<SemanticFact>,
    pub digest: u64,
}

#[derive(Clone, Debug)]
struct PageRecord {
    tail: Arc<SourceTail>,
    state_before: ParserState,
    state_after: ParserState,
    output: Arc<PageOutput>,
}

#[derive(Clone, Debug, Default)]
pub struct ParseMetrics {
    pub work_units: u64,
    pub ticks: u64,
    pub source_bytes_scanned: u64,
    pub pages_scanned: u64,
    pub prefix_pages_copied: u64,
    pub suffix_pages_attached: u64,
    pub eof_frames_finalized: u64,
    pub convergence_page: Option<usize>,
    pub max_work_units_per_tick: u64,
    pub max_source_bytes_per_tick: u64,
    pub max_eof_frames_per_tick: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AdvanceReport {
    pub work_units: u64,
    pub source_bytes_scanned: u64,
    pub eof_frames_finalized: u64,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct ParseResult {
    document: Document,
    pages: Vec<PageRecord>,
    eof_checkpoint: ParserState,
    eof: Arc<EofOutput>,
    tail_index: HashMap<u64, usize>,
    metrics: ParseMetrics,
}

impl ParseResult {
    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn metrics(&self) -> &ParseMetrics {
        &self.metrics
    }

    pub fn page_outputs(&self) -> impl Iterator<Item = &Arc<PageOutput>> {
        self.pages.iter().map(|page| &page.output)
    }

    pub fn eof_output(&self) -> &Arc<EofOutput> {
        &self.eof
    }

    /// A deterministic byte representation of all observable lexical and
    /// semantic facts. Tests compare this byte-for-byte between clean and
    /// resumed parses.
    pub fn canonical_output_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_u64(&mut bytes, self.pages.len() as u64);
        for page in &self.pages {
            write_u64(&mut bytes, page.output.source_page_id);
            write_u64(&mut bytes, page.output.lexical.len() as u64);
            for fact in page.output.lexical.to_vec() {
                write_lexical(&mut bytes, &fact);
            }
            write_u64(&mut bytes, page.output.semantic.len() as u64);
            for fact in page.output.semantic.to_vec() {
                write_semantic(&mut bytes, &fact);
            }
        }
        write_u64(&mut bytes, self.eof.semantic.len() as u64);
        for fact in self.eof.semantic.to_vec() {
            write_semantic(&mut bytes, &fact);
        }
        bytes
    }

    pub fn checkpoints_exactly_equal(&self, other: &Self) -> bool {
        if self.pages.len() != other.pages.len() {
            return false;
        }
        for (left, right) in self.pages.iter().zip(&other.pages) {
            if !states_equal_now(&left.state_before, &right.state_before)
                || !states_equal_now(&left.state_after, &right.state_after)
            {
                return false;
            }
        }
        states_equal_now(&self.eof_checkpoint, &other.eof_checkpoint)
    }

    pub fn retained_estimate(&self) -> RetainedEstimate {
        let source_bytes = self
            .document
            .tails
            .iter()
            .map(|tail| tail.page.bytes.len())
            .sum::<usize>();
        let source_page_state = self.document.page_count()
            * (size_of::<SourcePage>() + size_of::<SourceTail>() + size_of::<Arc<SourceTail>>());
        let page_records = self.pages.capacity() * size_of::<PageRecord>();
        let mut output_payload = 0;
        let mut seen_outputs = HashSet::new();
        for page in &self.pages {
            let pointer = Arc::as_ptr(&page.output) as usize;
            if seen_outputs.insert(pointer) {
                output_payload += size_of::<PageOutput>();
                output_payload += page.output.lexical.retained_bytes();
                output_payload += page.output.semantic.retained_bytes();
            }
        }
        output_payload += size_of::<EofOutput>();
        output_payload += self.eof.semantic.retained_bytes();

        let mut seen_frames = HashSet::new();
        for state in self
            .pages
            .iter()
            .flat_map(|page| [&page.state_before, &page.state_after])
            .chain(std::iter::once(&self.eof_checkpoint))
        {
            let mut cursor = state.root.clone();
            while let Some(frame) = cursor {
                let pointer = Arc::as_ptr(&frame) as usize;
                if !seen_frames.insert(pointer) {
                    break;
                }
                cursor = frame.previous.clone();
            }
        }
        let persistent_stack = seen_frames.len() * size_of::<StackFrame>();
        let index = self.tail_index.capacity() * size_of::<(u64, usize)>();
        let total = source_bytes
            + source_page_state
            + page_records
            + output_payload
            + persistent_stack
            + index;
        RetainedEstimate {
            source_bytes,
            source_page_state,
            page_records,
            output_payload,
            persistent_stack,
            index,
            total,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RetainedEstimate {
    pub source_bytes: usize,
    pub source_page_state: usize,
    pub page_records: usize,
    pub output_payload: usize,
    pub persistent_stack: usize,
    pub index: usize,
    pub total: usize,
}

struct PageBuilder {
    tail: Arc<SourceTail>,
    state_before: ParserState,
    offset: usize,
    text_start: Option<u16>,
    lexical: FactBuilder<LexicalFact>,
    semantic: FactBuilder<SemanticFact>,
    digest: u64,
}

impl PageBuilder {
    fn new(tail: Arc<SourceTail>, state_before: ParserState) -> Self {
        Self {
            tail,
            state_before,
            offset: 0,
            text_start: None,
            lexical: FactBuilder::default(),
            semantic: FactBuilder::default(),
            digest: FNV_OFFSET,
        }
    }

    fn anchor(&self, offset: usize) -> Anchor {
        Anchor {
            page_id: self.tail.page.id,
            offset: offset as u16,
        }
    }

    fn flush_text(&mut self) {
        if let Some(start) = self.text_start.take() {
            let fact = LexicalFact {
                kind: LexicalKind::Text,
                start: self.anchor(start as usize),
                end_offset: self.offset as u16,
            };
            self.push_lexical(fact);
        }
    }

    fn push_lexical(&mut self, fact: LexicalFact) {
        hash_lexical(&mut self.digest, &fact);
        self.lexical.push(fact);
    }

    fn push_semantic(&mut self, fact: SemanticFact) {
        hash_semantic(&mut self.digest, &fact);
        self.semantic.push(fact);
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingClose {
    kind: DelimiterKind,
    close: Anchor,
}

enum Phase {
    CopyPrefix,
    Boundary,
    Compare,
    Scan,
    Resolve,
    SealFlush,
    SealPage,
    AttachSuffix,
    EofFinalize,
    EofSeal,
    Done,
}

pub struct ParseJob {
    document: Document,
    old: Option<Arc<ParseResult>>,
    restart_page: usize,
    phase: Phase,
    current_page: usize,
    prefix_cursor: usize,
    attach_old_page: usize,
    state: ParserState,
    comparison: Option<ExactStateComparison>,
    comparison_old_page: usize,
    page_builder: Option<PageBuilder>,
    pending_close: Option<PendingClose>,
    pages: Vec<PageRecord>,
    tail_index: HashMap<u64, usize>,
    eof_checkpoint: Option<ParserState>,
    eof_semantic: FactBuilder<SemanticFact>,
    eof_digest: u64,
    eof: Option<Arc<EofOutput>>,
    metrics: ParseMetrics,
}

impl ParseJob {
    pub fn clean(document: Document) -> Self {
        Self::new(document, None, 0)
    }

    pub fn incremental(old: Arc<ParseResult>, edit: EditOutcome) -> Self {
        assert!(edit.restart_page <= old.pages.len());
        Self::new(edit.document, Some(old), edit.restart_page)
    }

    fn new(document: Document, old: Option<Arc<ParseResult>>, restart_page: usize) -> Self {
        let page_count = document.page_count();
        Self {
            document,
            old,
            restart_page,
            phase: Phase::CopyPrefix,
            current_page: restart_page,
            prefix_cursor: 0,
            attach_old_page: 0,
            state: ParserState::default(),
            comparison: None,
            comparison_old_page: 0,
            page_builder: None,
            pending_close: None,
            pages: Vec::with_capacity(page_count),
            tail_index: HashMap::with_capacity(page_count),
            eof_checkpoint: None,
            eof_semantic: FactBuilder::default(),
            eof_digest: FNV_OFFSET,
            eof: None,
            metrics: ParseMetrics::default(),
        }
    }

    pub fn is_done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }

    pub fn advance(&mut self, fuel: u64) -> AdvanceReport {
        assert!(fuel > 0);
        let before_bytes = self.metrics.source_bytes_scanned;
        let before_eof = self.metrics.eof_frames_finalized;
        let mut used = 0;
        while used < fuel && !self.is_done() {
            self.step();
            used += 1;
            self.metrics.work_units += 1;
        }
        self.metrics.ticks += 1;
        let scanned = self.metrics.source_bytes_scanned - before_bytes;
        let finalized = self.metrics.eof_frames_finalized - before_eof;
        self.metrics.max_work_units_per_tick = self.metrics.max_work_units_per_tick.max(used);
        self.metrics.max_source_bytes_per_tick =
            self.metrics.max_source_bytes_per_tick.max(scanned);
        self.metrics.max_eof_frames_per_tick = self.metrics.max_eof_frames_per_tick.max(finalized);
        AdvanceReport {
            work_units: used,
            source_bytes_scanned: scanned,
            eof_frames_finalized: finalized,
            done: self.is_done(),
        }
    }

    fn step(&mut self) {
        match self.phase {
            Phase::CopyPrefix => self.step_copy_prefix(),
            Phase::Boundary => self.step_boundary(),
            Phase::Compare => self.step_compare(),
            Phase::Scan => self.step_scan(),
            Phase::Resolve => self.step_resolve(),
            Phase::SealFlush => self.step_seal_flush(),
            Phase::SealPage => self.step_seal_page(),
            Phase::AttachSuffix => self.step_attach_suffix(),
            Phase::EofFinalize => self.step_eof_finalize(),
            Phase::EofSeal => self.step_eof_seal(),
            Phase::Done => {}
        }
    }

    fn step_copy_prefix(&mut self) {
        if self.prefix_cursor < self.restart_page {
            let old = self
                .old
                .as_ref()
                .expect("incremental prefix needs old result");
            let old_record = &old.pages[self.prefix_cursor];
            let new_tail = Arc::clone(&self.document.tails[self.prefix_cursor]);
            assert!(Arc::ptr_eq(&new_tail.page, &old_record.tail.page));
            let record = PageRecord {
                tail: Arc::clone(&new_tail),
                state_before: old_record.state_before.clone(),
                state_after: old_record.state_after.clone(),
                output: Arc::clone(&old_record.output),
            };
            self.tail_index.insert(new_tail.id, self.pages.len());
            self.pages.push(record);
            self.prefix_cursor += 1;
            self.metrics.prefix_pages_copied += 1;
            return;
        }

        if let Some(old) = &self.old {
            self.state = if self.restart_page < old.pages.len() {
                old.pages[self.restart_page].state_before.clone()
            } else {
                old.eof_checkpoint.clone()
            };
        }
        self.phase = Phase::Boundary;
    }

    fn step_boundary(&mut self) {
        if self.current_page >= self.document.page_count() {
            self.eof_checkpoint = Some(self.state.clone());
            self.phase = Phase::EofFinalize;
            return;
        }

        let tail = &self.document.tails[self.current_page];
        let candidate = self.old.as_ref().and_then(|old| {
            old.tail_index.get(&tail.id).copied().filter(|old_page| {
                Arc::ptr_eq(tail, &old.pages[*old_page].tail)
                    && self.current_page >= self.restart_page
            })
        });
        if let Some(old_page) = candidate {
            let old = self.old.as_ref().expect("candidate requires old result");
            self.comparison = Some(ExactStateComparison::new(
                &self.state,
                &old.pages[old_page].state_before,
            ));
            self.comparison_old_page = old_page;
            self.phase = Phase::Compare;
        } else {
            self.start_page();
        }
    }

    fn step_compare(&mut self) {
        let outcome = self
            .comparison
            .as_mut()
            .expect("compare phase requires task")
            .step();
        match outcome {
            Some(true) => {
                self.attach_old_page = self.comparison_old_page;
                self.metrics.convergence_page = Some(self.current_page);
                self.phase = Phase::AttachSuffix;
                self.comparison = None;
            }
            Some(false) => {
                self.comparison = None;
                self.start_page();
            }
            None => {}
        }
    }

    fn start_page(&mut self) {
        let tail = Arc::clone(&self.document.tails[self.current_page]);
        self.page_builder = Some(PageBuilder::new(tail, self.state.clone()));
        self.phase = Phase::Scan;
    }

    fn step_scan(&mut self) {
        let builder = self.page_builder.as_mut().expect("scan requires page");
        if builder.offset >= builder.tail.page.bytes.len() {
            self.phase = Phase::SealFlush;
            return;
        }

        let byte = builder.tail.page.bytes[builder.offset];
        let offset = builder.offset;
        let anchor = builder.anchor(offset);
        builder.offset += 1;
        self.metrics.source_bytes_scanned += 1;

        if let Some(kind) = opener(byte) {
            builder.flush_text();
            builder.push_lexical(LexicalFact {
                kind: LexicalKind::Open(kind),
                start: anchor,
                end_offset: (offset + 1) as u16,
            });
            self.state = self.state.push(kind, anchor);
        } else if byte == b'*' {
            builder.flush_text();
            let is_close = self
                .state
                .root
                .as_ref()
                .is_some_and(|frame| frame.kind == DelimiterKind::Star);
            builder.push_lexical(LexicalFact {
                kind: if is_close {
                    LexicalKind::Close(DelimiterKind::Star)
                } else {
                    LexicalKind::Open(DelimiterKind::Star)
                },
                start: anchor,
                end_offset: (offset + 1) as u16,
            });
            if is_close {
                self.pending_close = Some(PendingClose {
                    kind: DelimiterKind::Star,
                    close: anchor,
                });
                self.phase = Phase::Resolve;
            } else {
                self.state = self.state.push(DelimiterKind::Star, anchor);
            }
        } else if let Some(kind) = closer(byte) {
            builder.flush_text();
            builder.push_lexical(LexicalFact {
                kind: LexicalKind::Close(kind),
                start: anchor,
                end_offset: (offset + 1) as u16,
            });
            self.pending_close = Some(PendingClose {
                kind,
                close: anchor,
            });
            self.phase = Phase::Resolve;
        } else if builder.text_start.is_none() {
            builder.text_start = Some(offset as u16);
        }
    }

    fn step_resolve(&mut self) {
        let pending = self.pending_close.expect("resolve requires closer");
        let builder = self.page_builder.as_mut().expect("resolve requires page");
        match self.state.root.as_ref() {
            Some(frame) if frame.kind == pending.kind => {
                builder.push_semantic(SemanticFact::Pair {
                    kind: pending.kind,
                    open: frame.open,
                    close: pending.close,
                });
                self.state = self.state.pop();
                self.pending_close = None;
                self.phase = Phase::Scan;
            }
            Some(frame) => {
                builder.push_semantic(SemanticFact::AbandonedOpen {
                    kind: frame.kind,
                    open: frame.open,
                    at_close: pending.close,
                });
                self.state = self.state.pop();
            }
            None => {
                builder.push_semantic(SemanticFact::UnmatchedClose {
                    kind: pending.kind,
                    close: pending.close,
                });
                self.pending_close = None;
                self.phase = Phase::Scan;
            }
        }
    }

    fn step_seal_flush(&mut self) {
        self.page_builder
            .as_mut()
            .expect("seal requires page")
            .flush_text();
        self.phase = Phase::SealPage;
    }

    fn step_seal_page(&mut self) {
        let builder = self.page_builder.take().expect("seal requires page");
        let output = Arc::new(PageOutput {
            source_page_id: builder.tail.page.id,
            lexical: builder.lexical.finish(),
            semantic: builder.semantic.finish(),
            digest: builder.digest,
        });
        let record = PageRecord {
            tail: Arc::clone(&builder.tail),
            state_before: builder.state_before,
            state_after: self.state.clone(),
            output,
        };
        self.tail_index.insert(builder.tail.id, self.pages.len());
        self.pages.push(record);
        self.current_page += 1;
        self.metrics.pages_scanned += 1;
        self.phase = Phase::Boundary;
    }

    fn step_attach_suffix(&mut self) {
        let old = self.old.as_ref().expect("suffix attachment requires old");
        if self.attach_old_page < old.pages.len() {
            let record = old.pages[self.attach_old_page].clone();
            self.tail_index.insert(record.tail.id, self.pages.len());
            self.pages.push(record);
            self.attach_old_page += 1;
            self.current_page += 1;
            self.metrics.suffix_pages_attached += 1;
            return;
        }
        self.eof_checkpoint = Some(old.eof_checkpoint.clone());
        self.eof = Some(Arc::clone(&old.eof));
        self.phase = Phase::Done;
    }

    fn step_eof_finalize(&mut self) {
        match self.state.root.as_ref() {
            Some(frame) => {
                let fact = SemanticFact::UnclosedAtEof {
                    kind: frame.kind,
                    open: frame.open,
                };
                hash_semantic(&mut self.eof_digest, &fact);
                self.eof_semantic.push(fact);
                self.state = self.state.pop();
                self.metrics.eof_frames_finalized += 1;
            }
            None => self.phase = Phase::EofSeal,
        }
    }

    fn step_eof_seal(&mut self) {
        self.eof = Some(Arc::new(EofOutput {
            semantic: std::mem::take(&mut self.eof_semantic).finish(),
            digest: self.eof_digest,
        }));
        self.phase = Phase::Done;
    }

    pub fn finish(self) -> Arc<ParseResult> {
        assert!(self.is_done());
        Arc::new(ParseResult {
            document: self.document,
            pages: self.pages,
            eof_checkpoint: self.eof_checkpoint.expect("done job has EOF checkpoint"),
            eof: self.eof.expect("done job has EOF output"),
            tail_index: self.tail_index,
            metrics: self.metrics,
        })
    }
}

pub fn run_to_completion(mut job: ParseJob, fuel_per_tick: u64) -> Arc<ParseResult> {
    while !job.is_done() {
        job.advance(fuel_per_tick);
    }
    job.finish()
}

fn states_equal_now(left: &ParserState, right: &ParserState) -> bool {
    let mut comparison = ExactStateComparison::new(left, right);
    loop {
        if let Some(equal) = comparison.step() {
            return equal;
        }
    }
}

fn opener(byte: u8) -> Option<DelimiterKind> {
    match byte {
        b'[' => Some(DelimiterKind::Square),
        b'(' => Some(DelimiterKind::Round),
        b'{' => Some(DelimiterKind::Curly),
        _ => None,
    }
}

fn closer(byte: u8) -> Option<DelimiterKind> {
    match byte {
        b']' => Some(DelimiterKind::Square),
        b')' => Some(DelimiterKind::Round),
        b'}' => Some(DelimiterKind::Curly),
        _ => None,
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn mix64(left: u64, right: u64) -> u64 {
    let mut value = left ^ right.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn kind_code(kind: DelimiterKind) -> u64 {
    match kind {
        DelimiterKind::Square => 1,
        DelimiterKind::Round => 2,
        DelimiterKind::Curly => 3,
        DelimiterKind::Star => 4,
    }
}

fn hash_word(hash: &mut u64, word: u64) {
    for byte in word.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn hash_anchor(hash: &mut u64, anchor: Anchor) {
    hash_word(hash, anchor.page_id);
    hash_word(hash, u64::from(anchor.offset));
}

fn hash_lexical(hash: &mut u64, fact: &LexicalFact) {
    match fact.kind {
        LexicalKind::Text => hash_word(hash, 0),
        LexicalKind::Open(kind) => hash_word(hash, 10 + kind_code(kind)),
        LexicalKind::Close(kind) => hash_word(hash, 20 + kind_code(kind)),
    }
    hash_anchor(hash, fact.start);
    hash_word(hash, u64::from(fact.end_offset));
}

fn hash_semantic(hash: &mut u64, fact: &SemanticFact) {
    match *fact {
        SemanticFact::Pair { kind, open, close } => {
            hash_word(hash, 1 + kind_code(kind));
            hash_anchor(hash, open);
            hash_anchor(hash, close);
        }
        SemanticFact::AbandonedOpen {
            kind,
            open,
            at_close,
        } => {
            hash_word(hash, 10 + kind_code(kind));
            hash_anchor(hash, open);
            hash_anchor(hash, at_close);
        }
        SemanticFact::UnmatchedClose { kind, close } => {
            hash_word(hash, 20 + kind_code(kind));
            hash_anchor(hash, close);
        }
        SemanticFact::UnclosedAtEof { kind, open } => {
            hash_word(hash, 30 + kind_code(kind));
            hash_anchor(hash, open);
        }
    }
}

fn write_u64(target: &mut Vec<u8>, value: u64) {
    target.extend_from_slice(&value.to_le_bytes());
}

fn write_anchor(target: &mut Vec<u8>, anchor: Anchor) {
    write_u64(target, anchor.page_id);
    target.extend_from_slice(&anchor.offset.to_le_bytes());
}

fn write_lexical(target: &mut Vec<u8>, fact: &LexicalFact) {
    let code = match fact.kind {
        LexicalKind::Text => 0,
        LexicalKind::Open(kind) => 10 + kind_code(kind),
        LexicalKind::Close(kind) => 20 + kind_code(kind),
    };
    write_u64(target, code);
    write_anchor(target, fact.start);
    target.extend_from_slice(&fact.end_offset.to_le_bytes());
}

fn write_semantic(target: &mut Vec<u8>, fact: &SemanticFact) {
    match *fact {
        SemanticFact::Pair { kind, open, close } => {
            write_u64(target, 1 + kind_code(kind));
            write_anchor(target, open);
            write_anchor(target, close);
        }
        SemanticFact::AbandonedOpen {
            kind,
            open,
            at_close,
        } => {
            write_u64(target, 10 + kind_code(kind));
            write_anchor(target, open);
            write_anchor(target, at_close);
        }
        SemanticFact::UnmatchedClose { kind, close } => {
            write_u64(target, 20 + kind_code(kind));
            write_anchor(target, close);
        }
        SemanticFact::UnclosedAtEof { kind, open } => {
            write_u64(target, 30 + kind_code(kind));
            write_anchor(target, open);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FUEL: u64 = 37;

    fn parse(bytes: &[u8]) -> Arc<ParseResult> {
        run_to_completion(ParseJob::clean(Document::from_bytes(bytes)), TEST_FUEL)
    }

    fn assert_same(resumed: &ParseResult, clean: &ParseResult) {
        assert_eq!(
            resumed.canonical_output_bytes(),
            clean.canonical_output_bytes()
        );
        assert!(resumed.checkpoints_exactly_equal(clean));
    }

    #[test]
    fn document_edits_preserve_bytes_across_page_boundaries() {
        let original = vec![b'a'; SOURCE_PAGE_BYTES * 3 + 17];
        let document = Document::from_bytes(&original);
        let cases = [
            (0..0, b"start".as_slice()),
            (SOURCE_PAGE_BYTES..SOURCE_PAGE_BYTES, b"boundary".as_slice()),
            (
                SOURCE_PAGE_BYTES - 2..SOURCE_PAGE_BYTES + 2,
                b"cross".as_slice(),
            ),
            (17..SOURCE_PAGE_BYTES * 2 + 9, b"wide".as_slice()),
            (original.len()..original.len(), b"end".as_slice()),
        ];
        for (range, replacement) in cases {
            let mut expected = original.clone();
            expected.splice(range.clone(), replacement.iter().copied());
            assert_eq!(document.edit(range, replacement).document.bytes(), expected);
        }
    }

    #[test]
    fn checkpoint_is_constant_size_and_shares_stack_root() {
        assert!(size_of::<ParserState>() <= 32);
        let anchor = Anchor {
            page_id: 7,
            offset: 9,
        };
        let state = ParserState::default().push(DelimiterKind::Square, anchor);
        let checkpoint = state.clone();
        assert!(Arc::ptr_eq(
            state.root.as_ref().unwrap(),
            checkpoint.root.as_ref().unwrap()
        ));
    }

    #[test]
    fn exact_comparison_rejects_a_digest_collision() {
        let mut left = ParserState::default().push(
            DelimiterKind::Square,
            Anchor {
                page_id: 1,
                offset: 0,
            },
        );
        let mut right = ParserState::default().push(
            DelimiterKind::Curly,
            Anchor {
                page_id: 2,
                offset: 0,
            },
        );
        left.digest = 42;
        right.digest = 42;
        let mut comparison = ExactStateComparison::new(&left, &right);
        assert_eq!(comparison.step(), None);
        assert_eq!(comparison.step(), Some(false));
    }

    #[test]
    fn deterministic_edits_equal_clean_parse() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"[before ");
        bytes.extend(std::iter::repeat_n(b'a', SOURCE_PAGE_BYTES * 3));
        bytes.extend_from_slice(b" after] {x} *y*");
        let mut result = parse(&bytes);

        let edits: Vec<(Range<usize>, &[u8])> = vec![
            (17..18, b"Z"),
            (SOURCE_PAGE_BYTES + 3..SOURCE_PAGE_BYTES + 3, b"(insert)"),
            (3..3, b"{"),
            (bytes.len() / 2..bytes.len() / 2 + 5, b""),
            (0..1, b"("),
        ];
        for (range, replacement) in edits {
            let outcome = result.document().edit(range, replacement);
            let clean_document = outcome.document.clone();
            let resumed = run_to_completion(
                ParseJob::incremental(Arc::clone(&result), outcome),
                TEST_FUEL,
            );
            let clean = run_to_completion(ParseJob::clean(clean_document), TEST_FUEL);
            assert_same(&resumed, &clean);
            result = resumed;
        }
    }

    #[test]
    fn spanning_fact_suffix_is_reused_when_entry_state_is_exact() {
        let mut bytes = vec![b'a'; SOURCE_PAGE_BYTES * 4];
        bytes[1] = b'[';
        bytes[SOURCE_PAGE_BYTES * 3 + 8] = b']';
        let old = parse(&bytes);
        let close_page_output = Arc::clone(&old.pages[3].output);

        let edit_at = SOURCE_PAGE_BYTES * 2 + 100;
        let outcome = old.document().edit(edit_at..edit_at + 1, b"z");
        let resumed = run_to_completion(
            ParseJob::incremental(Arc::clone(&old), outcome.clone()),
            TEST_FUEL,
        );
        let clean = run_to_completion(ParseJob::clean(outcome.document), TEST_FUEL);
        assert_same(&resumed, &clean);
        assert_eq!(resumed.metrics.pages_scanned, 1);
        assert!(Arc::ptr_eq(&resumed.pages[3].output, &close_page_output));
        assert!(matches!(
            resumed.pages[3].output.semantic.to_vec().as_slice(),
            [SemanticFact::Pair { .. }]
        ));
    }

    #[test]
    fn spanning_fact_is_recomputed_if_its_opener_anchor_changes() {
        let mut bytes = vec![b'a'; SOURCE_PAGE_BYTES * 4];
        bytes[1] = b'[';
        bytes[SOURCE_PAGE_BYTES * 3 + 8] = b']';
        let old = parse(&bytes);
        let old_close_page = Arc::clone(&old.pages[3].output);
        let outcome = old.document().edit(100..101, b"z");
        let resumed = run_to_completion(
            ParseJob::incremental(Arc::clone(&old), outcome.clone()),
            TEST_FUEL,
        );
        let clean = run_to_completion(ParseJob::clean(outcome.document), TEST_FUEL);
        assert_same(&resumed, &clean);
        assert!(resumed.metrics.pages_scanned >= 4);
        assert!(!Arc::ptr_eq(&resumed.pages[3].output, &old_close_page));
    }

    #[test]
    fn random_edit_histories_equal_clean_parse() {
        let mut random = XorShift64(0x59d2_38f1_7812_abcd);
        let mut bytes = Vec::with_capacity(SOURCE_PAGE_BYTES * 8);
        let alphabet = b"abc[]{}()* xyz";
        for _ in 0..SOURCE_PAGE_BYTES * 8 {
            bytes.push(alphabet[random.usize(alphabet.len())]);
        }
        let mut result = parse(&bytes);

        for _revision in 0..250 {
            let start = random.usize(bytes.len() + 1);
            let remove = random.usize(13).min(bytes.len() - start);
            let replacement_len = random.usize(17);
            let mut replacement = Vec::with_capacity(replacement_len);
            for _ in 0..replacement_len {
                replacement.push(alphabet[random.usize(alphabet.len())]);
            }
            let outcome = result.document().edit(start..start + remove, &replacement);
            bytes = outcome.document.bytes();
            let clean_document = outcome.document.clone();
            let resumed =
                run_to_completion(ParseJob::incremental(Arc::clone(&result), outcome), 11);
            let clean = run_to_completion(ParseJob::clean(clean_document), TEST_FUEL);
            assert_same(&resumed, &clean);
            result = resumed;
        }
    }

    #[test]
    fn changed_open_state_prevents_false_convergence() {
        let mut bytes = vec![b'a'; SOURCE_PAGE_BYTES * 32];
        for page in 0..32 {
            bytes[page * SOURCE_PAGE_BYTES] = b'[';
        }
        let old = parse(&bytes);
        let outcome = old.document().edit(0..1, b"a");
        let resumed =
            run_to_completion(ParseJob::incremental(Arc::clone(&old), outcome.clone()), 19);
        let clean = run_to_completion(ParseJob::clean(outcome.document), 19);
        assert_same(&resumed, &clean);
        assert_eq!(resumed.metrics.pages_scanned, 32);
        assert_eq!(resumed.metrics.convergence_page, None);
        assert_eq!(resumed.metrics.eof_frames_finalized, 31);
    }

    #[test]
    fn every_parser_phase_obeys_fuel() {
        let mut bytes = Vec::new();
        for _ in 0..80 {
            bytes.extend_from_slice(b"[{(");
        }
        bytes.push(b']');
        bytes.resize(SOURCE_PAGE_BYTES * 2, b'a');
        let mut job = ParseJob::clean(Document::from_bytes(&bytes));
        while !job.is_done() {
            let report = job.advance(3);
            assert!(report.work_units <= 3);
            assert!(report.source_bytes_scanned <= 3);
            assert!(report.eof_frames_finalized <= 3);
        }
        let result = job.finish();
        assert!(result.metrics.eof_frames_finalized > 0);
        assert_eq!(result.metrics.max_work_units_per_tick, 3);
        assert!(result.metrics.max_source_bytes_per_tick <= 3);
        assert!(result.metrics.max_eof_frames_per_tick <= 3);
    }

    struct XorShift64(u64);

    impl XorShift64 {
        fn next(&mut self) -> u64 {
            let mut value = self.0;
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            self.0 = value;
            value
        }

        fn usize(&mut self, upper: usize) -> usize {
            if upper == 0 {
                0
            } else {
                self.next() as usize % upper
            }
        }
    }
}
