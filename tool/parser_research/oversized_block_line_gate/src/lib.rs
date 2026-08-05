//! Resumable exact-classifier feasibility gate for oversized physical lines.
//!
//! This crate is deliberately isolated from `comrak_value_block_core`. Its
//! state machines correspond to the regular scanner definitions in pinned
//! Comrak 0.54 and are differential witnesses, not a second shipping grammar.

#![allow(
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::option_option,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

mod html_type7;
mod reference;
mod table;

pub use html_type7::HtmlType7Job;
pub use reference::{ReferenceDefinitionShape, ReferencePrefixJob};
pub use table::{
    CellSummary, StreamingTableRowJob, TableBodyDisposition, TableBodyPassOneJob,
    TableBodyPassOnePoll, TableBodyRejectReason, TableBodyReplayCell, TableBodyReplayJob,
    TableBodyReplayPoll, TableBodyReplaySummary, TableHeaderDisposition, TableHeaderPassOneJob,
    TableHeaderPassOnePoll, TableHeaderRejectReason, TableHeaderReplayCell, TableHeaderReplayJob,
    TableHeaderReplayPoll, TableReplayError, TableRowJob, TableRowStreamCell, TableRowStreamPoll,
    TableRowStreamSummary, TableRowSummary, ValidatedTableBodyRow, ValidatedTableHeader,
};

pub const DEFAULT_POLL_BYTES: usize = 4 * 1024;
pub const MAX_TABLE_CELLS: usize = u16::MAX as usize;
pub const MAX_REFERENCE_LABEL_BYTES: usize = 1000;

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReceipt {
    pub polls: usize,
    pub bytes_inspected: usize,
    pub maximum_bytes_per_poll: usize,
    pub cancellation_checks: usize,
}

impl ScanReceipt {
    fn record_poll(&mut self, inspected: usize) {
        self.polls += 1;
        self.bytes_inspected += inspected;
        self.maximum_bytes_per_poll = self.maximum_bytes_per_poll.max(inspected);
        self.cancellation_checks += 1;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll<T> {
    Pending { inspected: usize },
    Ready { value: T, inspected: usize },
    Cancelled { inspected: usize },
}

impl<T> Poll<T> {
    pub const fn inspected(&self) -> usize {
        match self {
            Self::Pending { inspected }
            | Self::Ready { inspected, .. }
            | Self::Cancelled { inspected } => *inspected,
        }
    }
}

fn physical_content_end(input: &[u8]) -> usize {
    if input.ends_with(b"\r\n") {
        input.len() - 2
    } else if input
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        input.len() - 1
    } else {
        input.len()
    }
}

fn is_space_or_tab(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn is_cmark_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum FencePhase {
    Marker,
    BacktickRemainder,
    ClosingWhitespace,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FenceMode {
    Open,
    Close,
}

/// Resumable correspondent of Comrak's `open_code_fence` and
/// `close_code_fence` scanners. Input starts at first nonspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FenceJob {
    mode: FenceMode,
    cursor: usize,
    end: usize,
    marker: u8,
    run: usize,
    phase: FencePhase,
    result: Option<Option<usize>>,
    receipt: ScanReceipt,
}

impl FenceJob {
    pub fn new(input: &[u8], mode: FenceMode) -> Self {
        Self {
            mode,
            cursor: 0,
            end: physical_content_end(input),
            marker: 0,
            run: 0,
            phase: FencePhase::Marker,
            result: None,
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<Option<usize>> {
        assert!(fuel > 0);
        assert_eq!(self.end, physical_content_end(input));
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }
        if let Some(value) = self.result {
            self.receipt.record_poll(0);
            return Poll::Ready {
                value,
                inspected: 0,
            };
        }

        let mut inspected = 0;
        while inspected < fuel && self.result.is_none() {
            if self.cursor >= self.end {
                self.finish_at_end();
                break;
            }
            let byte = input[self.cursor];
            self.cursor += 1;
            inspected += 1;
            match self.phase {
                FencePhase::Marker => self.consume_marker(byte),
                FencePhase::BacktickRemainder => {
                    if byte == b'`' {
                        self.result = Some(None);
                        self.phase = FencePhase::Done;
                    }
                }
                FencePhase::ClosingWhitespace => {
                    if !is_space_or_tab(byte) {
                        self.result = Some(None);
                        self.phase = FencePhase::Done;
                    }
                }
                FencePhase::Done => unreachable!("completed job was returned above"),
            }
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            return Poll::Cancelled { inspected };
        }
        if let Some(value) = self.result {
            Poll::Ready { value, inspected }
        } else {
            Poll::Pending { inspected }
        }
    }

    fn consume_marker(&mut self, byte: u8) {
        if self.run == 0 {
            if !matches!(byte, b'`' | b'~') {
                self.result = Some(None);
                self.phase = FencePhase::Done;
                return;
            }
            self.marker = byte;
            self.run = 1;
            return;
        }
        if byte == self.marker {
            self.run += 1;
            return;
        }
        if self.run < 3 {
            self.result = Some(None);
            self.phase = FencePhase::Done;
            return;
        }
        match self.mode {
            FenceMode::Open if self.marker == b'~' => {
                self.result = Some(Some(self.run));
                self.phase = FencePhase::Done;
            }
            FenceMode::Open => {
                self.phase = FencePhase::BacktickRemainder;
                if byte == b'`' {
                    self.result = Some(None);
                    self.phase = FencePhase::Done;
                }
            }
            FenceMode::Close => {
                self.phase = FencePhase::ClosingWhitespace;
                if !is_space_or_tab(byte) {
                    self.result = Some(None);
                    self.phase = FencePhase::Done;
                }
            }
        }
    }

    fn finish_at_end(&mut self) {
        if self.phase == FencePhase::Marker {
            self.result = Some((self.run >= 3).then_some(self.run));
        } else if matches!(
            self.phase,
            FencePhase::BacktickRemainder | FencePhase::ClosingWhitespace
        ) {
            self.result = Some(Some(self.run));
        }
        self.phase = FencePhase::Done;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerLineResult {
    /// `Some(b'=')` or `Some(b'-')` for an exact setext underline.
    pub setext: Option<u8>,
    pub thematic_break: bool,
}

/// One-pass resumable correspondent of setext and thematic full-line checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerLineJob {
    cursor: usize,
    end: usize,
    first: Option<u8>,
    marker_count: usize,
    setext_valid: bool,
    setext_tail: bool,
    thematic_valid: bool,
    done: bool,
    result: MarkerLineResult,
    receipt: ScanReceipt,
}

impl MarkerLineJob {
    pub fn new(input: &[u8]) -> Self {
        Self {
            cursor: 0,
            end: physical_content_end(input),
            first: None,
            marker_count: 0,
            setext_valid: true,
            setext_tail: false,
            thematic_valid: true,
            done: false,
            result: MarkerLineResult::default(),
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<MarkerLineResult> {
        assert!(fuel > 0);
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }
        if self.done {
            self.receipt.record_poll(0);
            return Poll::Ready {
                value: self.result,
                inspected: 0,
            };
        }
        let mut inspected = 0;
        while inspected < fuel && self.cursor < self.end {
            let byte = input[self.cursor];
            self.cursor += 1;
            inspected += 1;
            let marker = *self.first.get_or_insert(byte);
            if byte == marker {
                self.marker_count += 1;
                if self.setext_tail {
                    self.setext_valid = false;
                }
            } else if is_space_or_tab(byte) {
                self.setext_tail = true;
            } else {
                self.setext_valid = false;
                self.thematic_valid = false;
            }
            if !matches!(marker, b'=' | b'-') {
                self.setext_valid = false;
            }
            if !matches!(marker, b'*' | b'_' | b'-') {
                self.thematic_valid = false;
            }
        }
        if self.cursor == self.end {
            self.result = MarkerLineResult {
                setext: (self.setext_valid && self.marker_count > 0).then_some(self.first.unwrap()),
                thematic_break: self.thematic_valid && self.marker_count >= 3,
            };
            self.done = true;
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            Poll::Cancelled { inspected }
        } else if self.done {
            Poll::Ready {
                value: self.result,
                inspected,
            }
        } else {
            Poll::Pending { inspected }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtxStartJob {
    cursor: usize,
    end: usize,
    hashes: usize,
    saw_separator: bool,
    done: bool,
    result: Option<usize>,
    receipt: ScanReceipt,
}

impl AtxStartJob {
    pub fn new(input: &[u8]) -> Self {
        Self {
            cursor: 0,
            // Unlike most full-line classifiers, the generated ATX scanner
            // includes one directly adjacent CR/LF in its returned match.
            end: input.len(),
            hashes: 0,
            saw_separator: false,
            done: false,
            result: None,
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<Option<usize>> {
        assert!(fuel > 0);
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }
        let mut inspected = 0;
        while !self.done && inspected < fuel {
            if self.cursor == self.end {
                self.result = (self.hashes > 0 && self.hashes <= 6).then_some(self.cursor);
                self.done = true;
                break;
            }
            let byte = input[self.cursor];
            self.cursor += 1;
            inspected += 1;
            if self.hashes == 0 || (!self.saw_separator && byte == b'#') {
                if byte == b'#' {
                    self.hashes += 1;
                    if self.hashes > 6 {
                        self.done = true;
                        self.result = None;
                    }
                } else {
                    self.done = true;
                    self.result = None;
                }
            } else if is_space_or_tab(byte) {
                self.saw_separator = true;
            } else if matches!(byte, b'\r' | b'\n') {
                self.result = Some(if self.saw_separator {
                    self.cursor - 1
                } else {
                    self.cursor
                });
                self.done = true;
            } else {
                self.result = self.saw_separator.then_some(self.cursor - 1);
                self.done = true;
            }
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            Poll::Cancelled { inspected }
        } else if self.done {
            Poll::Ready {
                value: self.result,
                inspected,
            }
        } else {
            Poll::Pending { inspected }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoppedAtx {
    pub end: usize,
    pub closed: bool,
}

/// Forward streaming equivalent of Comrak's backwards `chop_trailing_hashes`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtxTailJob {
    cursor: usize,
    end: usize,
    last_nonspace_end: usize,
    hash_run_start: Option<usize>,
    before_hash_nonspace_end: usize,
    previous_was_hash: bool,
    done: bool,
    result: ChoppedAtx,
    receipt: ScanReceipt,
}

impl AtxTailJob {
    pub fn new(input: &[u8]) -> Self {
        Self {
            cursor: 0,
            end: input.len(),
            last_nonspace_end: 0,
            hash_run_start: None,
            before_hash_nonspace_end: 0,
            previous_was_hash: false,
            done: false,
            result: ChoppedAtx {
                end: input.len(),
                closed: false,
            },
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<ChoppedAtx> {
        assert!(fuel > 0);
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }
        let mut inspected = 0;
        while !self.done && inspected < fuel && self.cursor < self.end {
            let index = self.cursor;
            let byte = input[index];
            self.cursor += 1;
            inspected += 1;
            if is_cmark_space(byte) {
                self.previous_was_hash = false;
                continue;
            }
            if byte == b'#' {
                if !self.previous_was_hash {
                    self.hash_run_start = Some(index);
                    self.before_hash_nonspace_end = self.last_nonspace_end;
                }
                self.previous_was_hash = true;
            } else {
                self.hash_run_start = None;
                self.previous_was_hash = false;
            }
            self.last_nonspace_end = index + 1;
        }
        if self.cursor == self.end {
            let trimmed_end = self.last_nonspace_end;
            let closed = self.hash_run_start.is_some_and(|start| {
                start > 0 && is_space_or_tab(input[start - 1]) && trimmed_end > start
            });
            self.result = ChoppedAtx {
                end: if closed {
                    self.before_hash_nonspace_end
                } else {
                    trimmed_end
                },
                closed,
            };
            self.done = true;
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            Poll::Cancelled { inspected }
        } else if self.done {
            Poll::Ready {
                value: self.result,
                inspected,
            }
        } else {
            Poll::Pending { inspected }
        }
    }
}

fn advance_pattern(pattern: &[u8], mut matched: usize, byte: u8) -> usize {
    let byte = byte.to_ascii_lowercase();
    while matched > 0 && pattern[matched] != byte {
        matched = prefix_fallback(pattern, matched);
    }
    if pattern[matched] == byte {
        matched += 1;
    }
    matched
}

fn prefix_fallback(pattern: &[u8], matched: usize) -> usize {
    for candidate in (1..matched).rev() {
        if pattern[..candidate] == pattern[matched - candidate..matched] {
            return candidate;
        }
    }
    0
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HtmlEndJob {
    block_type: u8,
    cursor: usize,
    end: usize,
    states: Vec<usize>,
    done: bool,
    result: bool,
    receipt: ScanReceipt,
}

impl HtmlEndJob {
    pub fn new(input: &[u8], block_type: u8) -> Self {
        let states = vec![0; html_needles(block_type).len()];
        Self {
            block_type,
            cursor: 0,
            end: physical_content_end(input),
            states,
            done: matches!(block_type, 6 | 7),
            result: false,
            receipt: ScanReceipt::default(),
        }
    }

    pub const fn receipt(&self) -> ScanReceipt {
        self.receipt
    }

    pub fn poll(
        &mut self,
        input: &[u8],
        fuel: usize,
        cancellation: &CancellationToken,
    ) -> Poll<bool> {
        assert!(fuel > 0);
        if cancellation.is_cancelled() {
            self.receipt.record_poll(0);
            return Poll::Cancelled { inspected: 0 };
        }
        let needles = html_needles(self.block_type);
        let mut inspected = 0;
        while !self.done && inspected < fuel && self.cursor < self.end {
            let byte = input[self.cursor];
            self.cursor += 1;
            inspected += 1;
            for (index, pattern) in needles.iter().enumerate() {
                let next = advance_pattern(pattern, self.states[index], byte);
                self.states[index] = next;
                if next == pattern.len() {
                    self.done = true;
                    self.result = true;
                    break;
                }
            }
        }
        if self.cursor == self.end {
            self.done = true;
        }
        self.receipt.record_poll(inspected);
        if cancellation.is_cancelled() {
            Poll::Cancelled { inspected }
        } else if self.done {
            Poll::Ready {
                value: self.result,
                inspected,
            }
        } else {
            Poll::Pending { inspected }
        }
    }
}

fn html_needles(block_type: u8) -> &'static [&'static [u8]] {
    match block_type {
        1 => &[b"</script>", b"</pre>", b"</textarea>", b"</style>"],
        2 => &[b"-->"],
        3 => &[b"?>"],
        4 => &[b">"],
        5 => &[b"]]>"],
        6 | 7 => &[],
        _ => panic!("unknown HTML block type {block_type}"),
    }
}

/// Run a job to completion with fixed byte fuel. Used by tests and receipts;
/// production scheduling calls `poll` directly.
pub fn run_to_ready<T: Clone>(
    mut poll: impl FnMut() -> Poll<T>,
) -> Result<(T, ScanReceipt), &'static str> {
    let mut receipt = ScanReceipt::default();
    loop {
        match poll() {
            Poll::Pending { inspected } => receipt.record_poll(inspected),
            Poll::Ready { value, inspected } => {
                receipt.record_poll(inspected);
                return Ok((value, receipt));
            }
            Poll::Cancelled { inspected } => {
                receipt.record_poll(inspected);
                return Err("cancelled");
            }
        }
    }
}
